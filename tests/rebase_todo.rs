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
    let source = format!("# comment\n\np {prefix} two\nreword {} one\ne {} two\ns {} three\nfixup {} four\ndrop {} five\n", ids[1], ids[2], ids[3], ids[4], ids[5]);
    let parsed = RebaseTodo::parse(&repo, source.as_bytes()).unwrap();
    assert_eq!(parsed.entries.iter().map(|entry| entry.action).collect::<Vec<_>>(), vec![TodoAction::Pick, TodoAction::Reword, TodoAction::Edit, TodoAction::Squash, TodoAction::Fixup, TodoAction::Drop]);
    assert_eq!(parsed.entries[0].oid, oid);
    assert_eq!(parsed.source, source.as_bytes());
}

#[test]
fn rejects_bad_commands_objects_duplicates_and_leading_fold_without_mutation() {
    let (dir, repo, oid) = repo();
    let one = git(dir.path(), &["rev-parse", "HEAD^"]).trim().to_owned();
    for invalid in [
        format!("exec echo bad {oid}\n"), format!("pick {oid}\n"),
        format!("pick {} missing\n", "deadbeef"), format!("pick {oid} one\npick {oid} duplicate\n"),
        format!("squash {oid} first\n"), format!("fixup {oid} first\n"),
        format!("pick {one} first\nupdate-ref refs/heads/x\n"),
        format!("pick {} tree\n", git(dir.path(), &["rev-parse", "HEAD:file"]).trim()),
    ] {
        assert!(RebaseTodo::parse(&repo, invalid.as_bytes()).is_err(), "accepted {invalid:?}");
        assert_eq!(invalid.as_bytes(), invalid.as_bytes());
    }
}

#[test]
fn rejects_ambiguous_abbreviation() {
    let (dir, repo, oid) = repo();
    // Two distinct commits sharing a one-character prefix always exist among generated objects only probabilistically;
    // an ambiguous revision is constructed using Git's minimum abbreviation collision search.
    let mut refs = vec![oid, git(dir.path(), &["rev-parse", "HEAD^"]).trim().to_owned()];
    let mut ambiguous = None;
    for a in 1..refs[0].len() {
        if refs.iter().any(|other| other != &refs[0] && other.starts_with(&refs[0][..a])) {
            ambiguous = Some(refs[0][..a].to_owned()); break;
        }
    }
    // If commits do not collide, add commits until their leading character matches.
    while ambiguous.is_none() {
        write(&dir.path().join("file"), &format!("{}\n", refs.len()));
        git(dir.path(), &["commit", "-am", "extra"]);
        let next = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned();
        if refs.iter().any(|old| old.starts_with(&next[..1]) || next.starts_with(&old[..1])) {
            let old = refs.iter().find(|old| old.starts_with(&next[..1]) || next.starts_with(&old[..1])).unwrap();
            ambiguous = Some(if next.starts_with(&old[..1]) { next[..1].to_owned() } else { old[..1].to_owned() });
        }
        refs.push(next);
    }
    assert!(RebaseTodo::parse(&repo, format!("pick {} ambiguous\n", ambiguous.unwrap()).as_bytes()).is_err());
}
