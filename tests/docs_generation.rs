use std::path::{Path, PathBuf};
use std::process::Command;

const OUTPUTS: [&str; 3] = [
    "docs/parity.md",
    "docs/plain-support.md",
    "docs/known-client-behaviors.md",
];

fn root() -> PathBuf {
    std::env::temp_dir().join(format!("magit-lsp-docs-{}", std::process::id()))
}

fn run(root: &Path, check: bool) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_generate-docs"));
    if check {
        command.arg("--check");
    }
    command.arg("--output-root").arg(root);
    command.output().expect("run docs generator")
}

#[test]
fn generator_writes_and_checks_each_output() {
    let root = root();
    let _ = std::fs::remove_dir_all(&root);
    assert!(run(&root, false).status.success());
    for relative in OUTPUTS {
        assert!(root.join(relative).is_file(), "missing {relative}");
    }
    assert!(run(&root, true).status.success());

    for relative in OUTPUTS {
        let path = root.join(relative);
        let content = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("{content}\ntampered\n")).unwrap();
        assert!(
            !run(&root, true).status.success(),
            "accepted tampered {relative}"
        );
        std::fs::write(&path, content).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(
            !run(&root, true).status.success(),
            "accepted missing {relative}"
        );
        assert!(run(&root, false).status.success());
    }
    let _ = std::fs::remove_dir_all(root);
}
