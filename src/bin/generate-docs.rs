//! Generate or check registry-derived documentation references.

#[path = "../../scripts/generate_plain_support.rs"]
mod plain_support;
#[path = "../../scripts/generate_client_behaviors.rs"]
mod client_behaviors;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Generate or check registry reference files under the selected root.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = args.iter().any(|arg| arg == "--check");
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--output-root" {
            let Some(value) = iter.next() else { eprintln!("--output-root needs a path"); return ExitCode::FAILURE; };
            root = PathBuf::from(value);
        }
    }
    let files = [
        ("docs/parity.md", magit_lsp::parity::markdown()),
        ("docs/plain-support.md", plain_support::render()),
        ("docs/known-client-behaviors.md", client_behaviors::render()),
    ];
    let mut failed = false;
    for (name, content) in files {
        let path = root.join(name);
        if check {
            if std::fs::read_to_string(&path).ok().as_deref() != Some(&content) {
                println!("stale: {}", path.display()); failed = true;
            }
        } else if let Err(error) = write(&root, &path, &content) {
            eprintln!("{}: {error}", path.display()); failed = true;
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

fn write(_root: &Path, path: &Path, content: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().expect("parent"))?;
    std::fs::write(path, content)
}
