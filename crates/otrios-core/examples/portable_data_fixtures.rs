// SPDX-License-Identifier: LGPL-3.0-only
//! Explicit golden-vector maintenance. Review byte/hash diffs before updating.
#[path = "../tests/fixtures/portable_data_case.rs"]
mod portable_data_case;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/portable-data-fixtures.json");
    let result = format!(
        "{}\n",
        serde_json::to_string_pretty(&portable_data_case::build())?
    );
    match std::env::args().nth(1).as_deref() {
        Some("--write") => std::fs::write(path, result)?,
        Some("--check") => {
            if std::fs::read_to_string(path)? != result {
                return Err(
                    "Portable data fixtures changed; review byte and semantic differences".into(),
                );
            }
        }
        _ => return Err("Use --check or --write".into()),
    }
    println!("Portable data fixtures match deterministic producer");
    Ok(())
}
