//! Write or check the committed directional JSON Schemas.

#[path = "../../scripts/generate_schema.rs"]
mod generate_schema;

use std::path::Path;
use std::process::ExitCode;

/// Run the generator from the repository root.
fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    if std::env::args().nth(1).as_deref() == Some("--check") {
        match generate_schema::stale_files(root) {
            Ok(stale) if stale.is_empty() => {
                println!("ok: schemas current");
                ExitCode::SUCCESS
            }
            Ok(stale) => {
                for path in stale {
                    println!("stale: {}", path.display());
                }
                ExitCode::FAILURE
            }
            Err(error) => {
                eprintln!("schema check failed: {error}");
                ExitCode::FAILURE
            }
        }
    } else {
        match generate_schema::write_all(root) {
            Ok(()) => {
                println!("ok: schemas written");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("schema update failed: {error}");
                ExitCode::FAILURE
            }
        }
    }
}
