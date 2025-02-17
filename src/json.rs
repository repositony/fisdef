// internal
use crate::create_file_with_fallback;
use crate::source::Source;

// standard lib
use std::path::Path;

// external
use anyhow::{Context, Result};

/// Writes the nuclide data to a JSON file at the specified path.
pub fn write(sources: &[Source], path: &Path, index: usize) -> Result<()> {
    let f = create_file_with_fallback(path, "json", &format!("step_{index}.json"))?;
    serde_json::to_writer_pretty(f, &sources).context("Unable to serialise to JSON")?;
    Ok(())
}
