// internal
use crate::wrappers::CliRadType;

// command line modules
use clap::builder::styling::{AnsiColor, Effects};
use clap::builder::Styles;
use clap::{arg, Parser};

// standard lib
use std::str::FromStr;

// xternal
use anyhow::Result;

/// Convert FISPACT-II steps to decay sources
///
/// Examples
/// --------
///
///  Typical use:
///     $ fisdef file.json --rad gamma --mcnp
///
///  Summary of time steps:
///     $ fisdef file.json
///
///  Choose specific steps (e.g. 5 total steps):
///     $ fisdef file.json    1    => 1
///     $ fisdef file.json   0-2   => [0, 1, 2]
///     $ fisdef file.json "1 3 4" => [1, 3, 4]
///     $ fisdef file.json   all   => [0, 1, 2, 3, 4]
///     $ fisdef file.json         => [0, 1, 2, 3, 4]
///
///  Choose radiation type:
///     $ fisdef file.json --rad gamma      => Gamma + X-ray
///     $ fisdef file.json --rad xray       => X-ray only
///     $ fisdef file.json --rad alpha      => alpha only
///     $ fisdef file.json --rad beta-plus  => b+ decay
///     $ fisdef file.json --rad beta-minus => b- decay
///     $ fisdef file.json --rad electron   => Auger/conversion electron
///
///  Choose output formats:
///     $ fisdef file.json 2 --mcnp --text --json
///       |_ creates 'step_2.i', 'step_2.txt', 'step_2.json'
///
///     $ fisdef file.json 2 --text --output myname
///       |_ creates 'myname_2.txt'
///
/// Notes
/// -----
///
/// ! WARNING ! FISPACT-II state notation *should* align with IAEA data
/// (i.e. m,n,o... => m1,m2,m3...), but this is not completely guaranteed.
/// ENSDF can contain much more information than ENDF on structure.
///
/// Pre-fetched data are from the IAEA chart of nuclides and will generally be
/// up to date and extremely fast. However, '--fetch' can retrieve decay data
/// directly from the IAEA API.
///
/// IAEA records with missing or unobserved intensities are omitted.
///
/// Decay data are sorted in ascending energy but this may be swapped to
/// descending intensity with '--sort intensity'.
///
#[derive(Parser)]
#[command(
    verbatim_doc_comment,
    arg_required_else_help(true),
    after_help("Note: --help shows more information and examples"),
    term_width(76),
    hide_possible_values(true),
    override_usage("fisjson <path> [options]"),
    styles=custom_style(),
)]
pub struct Cli {
    // * Positional
    /// Path to fispact JSON file
    #[arg(name = "path")]
    pub path: String,

    /// Indices of time steps (optional)
    ///
    /// A fispact calculation will save results of every time step. The
    /// times of interest are selected by index of the "inventory_data"
    /// list.
    ///
    /// For convenience, several input formats are accepted:
    ///     Single number       : e.g. 1
    ///     Multiple number     : e.g. "1 5 12" (quoted)
    ///     Range (inclusive)   : e.g. 1-3
    ///     All steps           : 'all' or leave blank
    ///
    /// Defaults to 'all'.
    #[arg(name = "idx")]
    #[arg(verbatim_doc_comment)]
    #[arg(value_parser = MultiRange::from_str)]
    #[arg(hide_default_value(true))]
    #[arg(default_value_t = MultiRange::All)]
    pub index: MultiRange,

    /// Type of decay radiation
    ///
    /// The IAEA chart of nuclides contains the following:
    ///   > Alpha ("a")
    ///   > Beta+ or electron capture ("bp")
    ///   > Beta- ("bm")
    ///   > Gamma decay ("g") [Default]
    ///   > Auger and conversion electron ("e")
    ///   > X-ray ("x")
    #[arg(help_heading("Data options"))]
    #[arg(short, long, value_enum)]
    #[arg(hide_default_value(true))]
    #[arg(default_value_t = CliRadType::Gamma)]
    #[arg(verbatim_doc_comment)]
    #[arg(value_name = "type")]
    pub rad: CliRadType,

    /// Sort records by property ['energy', 'intensity']
    ///
    /// Defaults to sorting decay data by ascending energy ('e' or 'energy').
    /// Alternatively, data may be sorted in descending order of relative
    /// intensity with 'i' or 'intensity'.
    #[arg(help_heading("Data options"))]
    #[arg(short, long)]
    #[arg(hide_default_value(true))]
    #[arg(default_value = "energy")]
    #[arg(value_name = "property")]
    pub sort: SortProperty,

    /// Query IAEA directly rather than pre-fetched data
    ///
    /// Note that this requires and internet connection and will be much slower
    /// than using pre-processed data.
    #[arg(help_heading("Data options"))]
    #[arg(long)]
    pub fetch: bool,

    /// Prefix for output files
    ///
    /// Defaults to `step`.
    ///
    /// Files are named `<name>_<n>.<ext>` where <n> is the index of the
    /// fispact time interval, and <ext> the appropriate extension.
    #[arg(help_heading("Output files"))]
    #[arg(short, long)]
    #[arg(value_name = "name")]
    #[arg(hide_default_value(true))]
    #[arg(default_value = "step")]
    pub output: String,

    /// Text based table
    ///
    /// Write a table of all nuclides and expected lines, excluding any that
    /// are unobserverd or have invalid data from the IAEA.
    #[arg(help_heading("Output files"))]
    #[arg(short, long)]
    pub text: bool,

    /// JSON output format
    ///
    /// Provides a list of every relevant nuclide with activity and
    /// energy/intensity decay data.
    #[arg(help_heading("Output files"))]
    #[arg(short, long)]
    pub json: bool,

    /// MCNP SDEF card
    ///
    /// Writes a source distribution of decay data for each nuclide, and an
    /// overall activity-based distribution to sample from.
    #[arg(help_heading("Output files"))]
    #[arg(short, long)]
    pub mcnp: bool,

    /// Starting MCNP distribution number
    ///
    /// Defaults to 100.
    #[arg(help_heading("Output files"))]
    #[arg(short, long)]
    #[arg(value_name = "num")]
    #[arg(hide_default_value(true))]
    #[arg(default_value = "100")]
    pub id: usize,

    // * Flags
    /// Verbose logging (-v, -vv)
    ///
    /// If specified, the default log level of INFO is increased to DEBUG (-v)
    /// or TRACE (-vv). Errors and Warnings are always logged unless in quiet
    /// (-q) mode.
    #[arg(short, long)]
    #[arg(action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Supress all logging
    ///
    /// Note that this overrules the --verbose flag.
    #[arg(short, long)]
    pub quiet: bool,
}

/// Customise the colour styles for clap v4
fn custom_style() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Cyan.on_default() | Effects::BOLD | Effects::UNDERLINE)
        .literal(AnsiColor::Blue.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Magenta.on_default())
}

/// Sets up logging at runtime to allow for multiple verbosity levels
pub fn init_logging(cli: &Cli) -> Result<()> {
    let show_level = cli.verbose > 0;

    Ok(stderrlog::new()
        .module("fisdef")
        .quiet(cli.quiet)
        .verbosity(cli.verbose as usize + 2)
        .show_level(show_level)
        .color(stderrlog::ColorChoice::Auto)
        .timestamp(stderrlog::Timestamp::Off)
        .init()?)
}

/// User input that can handle multiple ways of defining interval index
///
/// e.g. single number : 1
///      range         : 1-3
///      list          :"1 2 3"
///      "all"/blank   : all intervals
#[derive(Debug, Clone)]
pub enum MultiRange {
    Single(usize),
    List(Vec<usize>),
    Range(usize, usize),
    All,
}

impl std::fmt::Display for MultiRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultiRange::Single(value) => write!(f, "{}", value),
            MultiRange::List(values) => {
                let list_str = values
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                write!(f, "{}", list_str)
            }
            MultiRange::Range(start, end) => write!(f, "{}-{}", start, end),
            MultiRange::All => write!(f, "All"),
        }
    }
}

impl FromStr for MultiRange {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().to_lowercase() == "all" {
            return Ok(MultiRange::All);
        }

        if let Some(range_sep) = s.find('-') {
            let start = &s[..range_sep];
            let end = &s[range_sep + 1..];
            return match (start.parse::<usize>(), end.parse::<usize>()) {
                (Ok(start), Ok(end)) if start <= end => Ok(MultiRange::Range(start, end)),
                _ => Err(format!("Invalid range format: '{}'", s)),
            };
        }

        if let Ok(value) = s.parse::<usize>() {
            return Ok(MultiRange::Single(value));
        }

        if let Ok(list) = s
            .split_whitespace()
            .map(|num| num.parse::<usize>())
            .collect::<Result<Vec<_>, _>>()
        {
            return Ok(MultiRange::List(list));
        }
        Err("integers ('0 1 2'), range (0-2), or 'all'".to_string())
    }
}

/// User input for various sorting methods
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum SortProperty {
    Intensity,
    #[default]
    Energy,
}

impl SortProperty {
    pub fn name(&self) -> &str {
        match self {
            SortProperty::Intensity => "intensity",
            SortProperty::Energy => "energy",
        }
    }
}

impl From<String> for SortProperty {
    fn from(property: String) -> Self {
        match property.to_lowercase().as_str() {
            "i" | "intensity" => SortProperty::Intensity,
            "e" | "energy" => SortProperty::Energy,
            _ => SortProperty::default(),
        }
    }
}

impl std::fmt::Display for SortProperty {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}
