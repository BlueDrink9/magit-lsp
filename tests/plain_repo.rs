use magit_lsp::presentation::resolve::{resolve, GitRepository, Resolution};
use std::path::Path;

fn git(path: &Path, args: &[&str]) {
    std::fs::create_dir_all(path).unwrap();
    let status = std::process::Command::new("git").args(args).current_dir(path).status().unwrap();
    assert!(status.success(), "git {args:?}");
}

fn repo(path: &Path) {
    git(path, &["init", "-q"]);
    git(path, &["-c", "user.name=test", "-c", "user.email=test@invalid", "commit", "--allow-empty", "-qm", "initial"]);
}

#[test]
fn resolves_none_workspace_document_nested_alias_and_linked_worktree_without_guessing() {
    let temp = tempfile::tempdir().unwrap();
    let plain = temp.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert_eq!(resolve(None, &[plain.clone()], None), Resolution::None);

    let a = temp.path().join("a");
    let b = temp.path().join("b");
    repo(&a);
    repo(&b);
    assert!(matches!(resolve(None, &[a.clone(), b.clone()], None), Resolution::Choose(xs) if xs.len() == 2));
    assert!(matches!(resolve(None, &[], Some(&a.join("document.txt"))), Resolution::Resolved(_)));

    let nested = a.join("nested");
    repo(&nested);
    assert!(matches!(resolve(None, &[a.clone(), nested.clone()], None), Resolution::Choose(xs) if xs.len() == 2));
    let alias = temp.path().join("alias");
    std::os::unix::fs::symlink(&a, &alias).unwrap();
    let linked = temp.path().join("linked");
    git(&a, &["worktree", "add", "-q", "-b", "linked", linked.to_str().unwrap()]);
    let discovered = GitRepository::discover(&linked).unwrap();
    let original = GitRepository::discover(&a).unwrap();
    assert_ne!(discovered.root, original.root);
    assert_ne!(discovered.git_dir, original.git_dir);
    assert_eq!(discovered.common_dir, original.common_dir);
    assert!(matches!(resolve(None, &[a.clone(), alias], None), Resolution::Resolved(_)));
}

#[test]
fn explicit_path_takes_precedence_and_ambiguous_candidates_are_not_resolved() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    repo(&a);
    repo(&b);
    assert!(matches!(resolve(Some(&a), &[b], None), Resolution::Resolved(repo) if repo.root == std::fs::canonicalize(a).unwrap()));
}
