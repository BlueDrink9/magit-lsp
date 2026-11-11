mod common;

use common::{git, write};
use magit_lsp::git::GitRepo;
use magit_lsp::rebase_todo::{RebaseTodo, TodoAction};
use tempfile::TempDir;

fn repo() -> (TempDir, GitRepo, String) {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    write(&dir.path().join("file"), "one\n");
    git(dir.path(), &["add", "file"]);
    git(dir.path(), &["commit", "-m", "one"]);
    write(&dir.path().join("file"), "two\n");
    git(dir.path(), &["commit", "-am", "two"]);
    let oid = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = GitRepo::open(dir.path()).unwrap();
    (dir, repo, oid)
}

#[test]
fn parses_supported_actions_and_abbreviated_identity() {
    let (dir, repo, oid) = repo();
    let prefix = &oid[..8];
    let mut ids = vec![oid.clone(), git(dir.path(), &["rev-parse", "HEAD^"]).trim().to_owned()];
    for index in 0..4 {
        write(&dir.path().join("file"), &format!("{index}\n"));
        git(dir.path(), &["commit", "-am", &format!("extra-{index}")]);
        ids.push(git(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned());
    }
    let source = format!("# comment\n\np   {prefix}\ttwo\nreword {} one\ne {} two\ns {} three\nfixup {} four\ndrop {} five\n", ids[1], ids[2], ids[3], ids[4], ids[5]);
    let parsed = RebaseTodo::parse(&repo, source.as_bytes()).unwrap();
    assert_eq!(parsed.entries.iter().map(|entry| entry.action).collect::<Vec<_>>(), vec![TodoAction::Pick, TodoAction::Reword, TodoAction::Edit, TodoAction::Squash, TodoAction::Fixup, TodoAction::Drop]);
    assert_eq!(parsed.entries[0].oid, oid);
    assert_eq!(parsed.source, source.as_bytes());
}

#[test]
fn rejects_bad_commands_objects_duplicates_and_leading_fold_without_mutation() {
    let (dir, repo, oid) = repo();
    let file_before = std::fs::read(dir.path().join("file")).unwrap();
    let head_before = git(dir.path(), &["rev-parse", "HEAD"]);
    let index_before = git(dir.path(), &["write-tree"]);
    let one = git(dir.path(), &["rev-parse", "HEAD^"]).trim().to_owned();
    for invalid in [
        format!("exec echo bad {oid}\n"), format!("pick {oid}\n"),
        format!("pick {} missing\n", "deadbeef"), format!("pick {oid} one\npick {oid} duplicate\n"),
        format!("squash {oid} first\n"), format!("fixup {oid} first\n"),
        format!("pick {one} first\nupdate-ref refs/heads/x\n"),
        format!("pick {} tree\n", git(dir.path(), &["rev-parse", "HEAD:file"]).trim()),
    ] {
        assert!(RebaseTodo::parse(&repo, invalid.as_bytes()).is_err(), "accepted {invalid:?}");
        assert_eq!(std::fs::read(dir.path().join("file")).unwrap(), file_before);
        assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head_before);
        assert_eq!(git(dir.path(), &["write-tree"]), index_before);
    }
}

#[test]
fn rejects_ambiguous_abbreviation() {
    let (dir, repo, _) = repo();
    let mut prefixes = std::collections::HashMap::<String, usize>::new();
    let mut ambiguous = None;
    for index in 0..5000 {
        let data = format!("fixture-{index}");
        let path = dir.path().join(format!("fixture-{index}"));
        write(&path, &data);
        let oid = git(dir.path(), &["hash-object", "-w", path.to_str().unwrap()]).trim().to_owned();
        let prefix = oid[..4].to_owned();
        let count = prefixes.entry(prefix.clone()).or_default();
        *count += 1;
        if *count > 1 { ambiguous = Some(prefix); break; }
    }
    let prefix = ambiguous.expect("bounded fixture must produce 4-hex collision");
    let matching = git(dir.path(), &["rev-parse", &format!("--disambiguate={prefix}")]);
    assert!(matching.lines().count() > 1, "Git must prove ambiguity");
    assert!(RebaseTodo::parse(&repo, format!("pick {prefix} ambiguous\n").as_bytes()).is_err());
}
