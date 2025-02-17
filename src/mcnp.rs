// internal
use crate::create_file_with_fallback;
use crate::source::Source;

// neutronics toolbox
use ntools::utils::{f, ValueExt};

// standard lib
use std::io::Write;
use std::path::Path;

// external
use anyhow::Result;

const KEV_TO_MEV: f32 = 1.0e-03;

/// Writes the mcnp cards to a file at the specified path.
pub fn write(sources: &[Source], path: &Path, index: usize, id: usize) -> Result<()> {
    let mut f = create_file_with_fallback(path, "i", &f!("step_{index}.i"))?;
    let cards = generate_mcnp_cards(sources, id);
    f.write_all(cards.as_bytes())?;
    Ok(())
}

/// Make source distribution cards for every nuclide
fn generate_mcnp_cards(sources: &[Source], id: usize) -> String {
    let mut card = activity_distribution(sources, id);
    for (i, s) in sources.iter().enumerate() {
        card += &nuclide_distribution(s, id + i + 1);
    }
    card
}

/// Generates a formatted comment string for the main source distribution.
///
/// The comment includes the source ID and the total normalized source count
/// per particle.
fn activity_distribution(sources: &[Source], id: usize) -> String {
    let comment = f!(
        "sc{id:<5} Main source distribution ({} counts/src particle)",
        total_norm(sources).sci(5, 2)
    );

    let mut si_card = f!("si{:<6}", f!("{id} S "));
    let mut sp_card = f!("sp{id:<6}");

    for (i, s) in sources.iter().enumerate() {
        let dist_id = id + i + 1;
        si_card += &f!("{dist_id} ");
        sp_card += &f!(
            "{}    $ {:<6} = {} Bq * {} particles/decay ",
            (s.fispact_activity * s.norm()).sci(5, 2),
            s.fispact_name,
            s.fispact_activity.sci(5, 2),
            s.norm().sci(5, 2),
        );
    }

    f!(
        "{}\n{}\n{}\nc",
        comment,
        &wrap_text(si_card, 80, "        "),
        &wrap_text(sp_card, 80, "        ")
    )
}

// Find the total normalisation factor
fn total_norm(sources: &[Source]) -> f64 {
    let total_activity = sources.iter().map(|s| s.fispact_activity).sum::<f64>();

    sources
        .iter()
        .map(|s| (s.fispact_activity / total_activity) * s.norm())
        .sum::<f64>()
}

/// Make a single source distribution for a nuclide
fn nuclide_distribution(source: &Source, id: usize) -> String {
    // Create a comment line with nuclide name and normalization factor
    let comment = f!(
        "sc{id:<5} {} decay data, norm = {} particles/decay",
        source.fispact_name,
        source.norm().sci(5, 2) // this is already ignoring None intensities
    );

    // Create the SI card with energy values
    let si_card = f!(
        "si{id} L {}",
        source
            .iaea_records
            .iter()
            .map(|record| (record.energy.unwrap() * KEV_TO_MEV).sci(5, 2))
            .collect::<Vec<String>>()
            .join(" ")
    );

    // Create the SP card with intensity values
    let sp_card = f!(
        "sp{id:<6}{}",
        source
            .iaea_records
            .iter()
            .map(|record| (record.intensity.unwrap() * 1e-2).sci(5, 2))
            .collect::<Vec<String>>()
            .join(" ")
    );

    // Combine the comment, SI card, and SP card with proper formatting
    f!(
        "\n{}\n{}\n{}\nc",
        comment,
        &wrap_text(si_card, 80, "        "),
        &wrap_text(sp_card, 80, "        ")
    )
}

// wrap everything to a fixed number of characters for mcnp
fn wrap_text(text: String, width: usize, subsequent_indent: &str) -> String {
    let options = textwrap::Options::new(width)
        .initial_indent("")
        .subsequent_indent(subsequent_indent)
        .word_splitter(textwrap::WordSplitter::NoHyphenation)
        .break_words(false);
    textwrap::fill(&text, options)
}
