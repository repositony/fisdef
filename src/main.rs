//! Command line tool to do FISPACT decay data stuff
#![doc(hidden)]

// crate modules
mod cli;
mod json;
mod mcnp;
mod source;
mod table;
mod wrappers;

// re-exports for convenience
use cli::{Cli, MultiRange};
use source::Source;

// neutronics toolbox
use ntools::fispact::{self, Inventory};
use ntools::iaea::{self};
use ntools::utils::{f, ValueExt};

// standard lib
use std::fs::{self, File};
use std::path::{Path, PathBuf};

// other
use anyhow::{bail, Context, Result};
use clap::Parser;
use log::{debug, info, trace, warn};

fn main() -> Result<()> {
    // set up the command line interface and logging
    let cli = Cli::parse();
    cli::init_logging(&cli)?;

    info!("Reading fispact JSON data");
    let path: &Path = Path::new(&cli.path);
    debug!("{:?}", path.display());
    let inventory = fispact::read_json(path)?;

    info!("Table of FISPACT intervals");
    fispact_summary(&inventory);

    // short-circuit if no outputs given
    if !cli.mcnp && !cli.json && !cli.text {
        debug!("No outputs requested");
        return Ok(());
    }

    info!("Parsing user input to explicit interval indices");
    let index_list = index_list(&cli.index, &inventory)?;

    for index in index_list {
        process_interval(&inventory, index, &cli)?;
    }

    Ok(())
}

fn process_interval(inventory: &Inventory, index: usize, cli: &Cli) -> Result<()> {
    info!("Generating sources from interval {index}");
    if let Some(sources) = get_sources(inventory, index, cli) {
        let path = output_path(cli, index);

        if cli.json {
            info!("Writing to JSON");
            json::write(&sources, path.as_path(), index)?;
        }

        if cli.mcnp {
            info!("Writing to MCNP");
            mcnp::write(&sources, path.as_path(), index, cli.id)?;
        }

        if cli.text {
            info!("Writing to text file");
            let table = table::Table::new(&sources);
            table.write(path.as_path())?;
        }
    } else {
        info!("No relevant decay data found");
    }

    Ok(())
}

/// Summarise intervals in the file
fn fispact_summary(inventory: &Inventory) {
    struct Record {
        irrad_time: f64,
        cool_time: f64,
        total_time: f64,
        mass: f64,
        dose: f64,
        activity: f64,
    }

    let records = inventory
        .intervals
        .iter()
        .map(|i| Record {
            irrad_time: i.irradiation_time,
            cool_time: i.cooling_time,
            total_time: i.irradiation_time + i.cooling_time,
            mass: i.mass,
            dose: i.dose.rate,
            activity: i.activity,
        })
        .collect::<Vec<Record>>();

    println!("\n{:-<1$}", "", 71);
    println!("           Interval Time [s]                  Interval totals");
    println!("Index   Irrad     Cool    Total      Mass [g]  Dose [uSv/hr]   Act [Bq]");
    println!("{:-<1$}", "", 71);

    for (i, r) in records.iter().enumerate() {
        println!(
            " {i:<3}   {} {} {}    {}     {}     {}",
            r.irrad_time.sci(2, 2),
            r.cool_time.sci(2, 2),
            r.total_time.sci(2, 2),
            r.mass.sci(2, 2),
            (r.dose * 1e6).sci(2, 2),
            r.activity.sci(2, 2),
        )
    }
    println!();
}

/// List of explicit index for each valid interval
fn index_list(user_idx: &MultiRange, inventory: &Inventory) -> Result<Vec<usize>> {
    let n = inventory.intervals.len();
    trace!("{n} intervals found in file");

    let mut index_list = match user_idx {
        MultiRange::Single(idx) => vec![*idx],
        MultiRange::List(values) => values.clone(),
        MultiRange::Range(start, end) => (*start..=*end).collect(),
        MultiRange::All => (0..n).collect(),
    };

    index_list.sort();
    index_list.dedup();

    // validate whatever is left
    index_list.retain(|i| *i < n);
    if index_list.is_empty() {
        bail!("\"{user_idx}\" not valid for expected 0-{} range", n - 1)
    }

    debug!("Valid intervals: {:?}", index_list);
    Ok(index_list)
}

/// Turn FISPACT nuclide names into IAEA nuclide structs
fn parse_nuclides(inventory: &Inventory, index: usize) -> Option<Vec<Source>> {
    // collect all unstable nuclides that also exist in the IAEA data
    let mut sources = inventory.intervals[index]
        .unstable_nuclides()
        .into_iter()
        .filter_map(|n| {
            if let Ok(nuclide) = iaea::Nuclide::try_from(n.name()) {
                Some(Source {
                    fispact_name: n.name(),
                    fispact_activity: n.activity,
                    iaea_nuclide: nuclide,
                    iaea_records: Vec::new(),
                })
            } else {
                debug!("Could not convert {n:?} to nuclide, skipping...");
                None
            }
        })
        .collect::<Vec<Source>>();

    trace!("Nuclides sorted by name");
    sources.sort_by_key(|n| n.fispact_name.clone());

    trace!("Removing duplicates");
    sources.dedup();
    if sources.is_empty() {
        return None;
    }

    debug!("Fispact to IAEA nuclide map:");
    for s in &sources {
        debug!(
            "   {:<6} -> {}",
            &s.fispact_name,
            s.iaea_nuclide.name_with_state()
        )
    }

    Some(sources)
}

fn get_sources(inventory: &Inventory, index: usize, cli: &Cli) -> Option<Vec<Source>> {
    // start mapping fispact to iaea nuclides
    let mut sources = parse_nuclides(inventory, index)?;

    // fill with records for the relevant decay type
    for s in sources.iter_mut() {
        s.find_records(cli.rad.into(), cli.fetch);
        s.remove_unobserved_records();
        s.sort_records(&cli.sort);
    }

    // filter out anything with no remaining records
    sources.retain(|s| !s.iaea_records.is_empty());

    // if none of them had decay data, then sources will be empty
    if sources.is_empty() {
        return None;
    }

    // sort the sources by name because why not
    sources.sort_by_key(|s| s.fispact_name.clone());

    Some(sources)
}

/// Sanitise the output given and append interval index
pub fn output_path(cli: &Cli, index: usize) -> PathBuf {
    let mut path = PathBuf::from(&cli.output);

    // take the file name provided
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("step");
    trace!("Found the name \"{name}\"");

    // append the mesh tally number to the name
    path.set_file_name(f!("{name}_{index}"));

    trace!("Output prefix: {:?}", path.file_name().unwrap());
    path
}

/// Try to create a file, including all dirs, with a default to fallback on
fn create_file_with_fallback(path: &Path, extension: &str, default: &str) -> Result<File> {
    let mut p = path.to_path_buf();

    // Ensure all parent directories exist
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            warn!("{e}. Falling back to working directory.");
            p = p.file_name().expect("No file name provided").into();
        }
    }

    // Create the file, fall back to a default if not
    let f = File::create(p.with_extension(extension)).or_else(|e| {
        warn!("{e}. Falling back to \"{default}\".",);
        File::create(default).context("Unable to create fallback file")
    })?;

    Ok(f)
}
