mod common;

use common::*;
use magit_lsp::git::GitRepo;
use magit_lsp::git::{IgnoreRule, IgnoreScope};
use magit_lsp::patch::{self, FileLayout};
use magit_lsp::ErrorKind;
use proptest::prelude::*;
use std::path::Path;

#[test]
fn index_flags_are_independent_literal_and_atomic() {
    let (tmp, repo) = RepoFixture::new()
        .commit(
            "initial",
            [("-odd [x]*.txt", "literal"), ("other.txt", "other")],
        )
        .build();
    let flags = || git(tmp.path(), &["ls-files", "-v"]);
    let head = git(tmp.path(), &["rev-parse", "HEAD"]);
    let index = git(tmp.path(), &["ls-files", "--stage"]);
    repo.set_skip_worktree(&["-odd [x]*.txt"], true).unwrap();
    assert!(flags()
        .lines()
        .any(|line| line.starts_with('S') && line.ends_with("-odd [x]*.txt")));
    repo.set_assume_unchanged(&["-odd [x]*.txt"], true).unwrap();
    assert!(flags()
        .lines()
        .any(|line| line.starts_with('s') && line.ends_with("-odd [x]*.txt")));
    repo.set_skip_worktree(&["-odd [x]*.txt"], false).unwrap();
    assert!(flags()
        .lines()
        .any(|line| line.starts_with('h') && line.ends_with("-odd [x]*.txt")));
    repo.set_assume_unchanged(&["-odd [x]*.txt"], false)
        .unwrap();
    assert!(flags()
        .lines()
        .any(|line| line.starts_with('H') && line.ends_with("-odd [x]*.txt")));
    let before = flags();
    assert!(repo
        .set_skip_worktree(&["other.txt", "missing"], true)
        .is_err());
    assert_eq!(flags(), before);
    assert!(repo.set_assume_unchanged(&["../other.txt"], true).is_err());
    assert_eq!(flags(), before);
    assert_eq!(git(tmp.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(tmp.path(), &["ls-files", "--stage"]), index);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("-odd [x]*.txt")).unwrap(),
        "literal"
    );
    std::fs::write(repo.git_dir().join("index.lock"), "locked").unwrap();
    assert!(repo.set_skip_worktree(&["other.txt"], true).is_err());
    std::fs::remove_file(repo.git_dir().join("index.lock")).unwrap();
    assert_eq!(flags(), before);
}

#[test]
fn ignore_scopes_preserve_content_and_literal_paths_are_exact() {
    let (tmp, repo) = RepoFixture::new()
        .commit("initial", [("tracked.txt", TEN_LINES)])
        .build();
    std::fs::create_dir_all(tmp.path().join("dir")).unwrap();
    std::fs::write(tmp.path().join("dir/a [x].txt"), "untracked").unwrap();
    std::fs::write(tmp.path().join("dir/otherx.txt"), "untracked").unwrap();
    std::fs::write(tmp.path().join(".gitignore"), "# retained").unwrap();
    repo.add_ignore_rule(
        IgnoreScope::Root,
        IgnoreRule::LiteralPath(Path::new("dir/a [x].txt")),
    )
    .unwrap();
    assert!(git(tmp.path(), &["check-ignore", "dir/a [x].txt"]).contains("dir/a [x].txt"));
    assert!(!std::process::Command::new("git")
        .args(["check-ignore", "dir/otherx.txt"])
        .current_dir(tmp.path())
        .status()
        .unwrap()
        .success());
    let root_contents = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
    assert!(root_contents.starts_with("# retained\n"));
    assert!(root_contents.ends_with("/dir/a\\ \\[x\\].txt\n"));
    repo.add_ignore_rule(
        IgnoreScope::Directory,
        IgnoreRule::LiteralPath(Path::new("dir/otherx.txt")),
    )
    .unwrap();
    assert!(tmp.path().join("dir/.gitignore").exists());
    assert!(git(tmp.path(), &["check-ignore", "dir/otherx.txt"]).contains("dir/otherx.txt"));
    repo.add_ignore_rule(IgnoreScope::Repository, IgnoreRule::Pattern("*.cache"))
        .unwrap();
    let repository_excludes =
        std::fs::read_to_string(repo.common_dir().join("info/exclude")).unwrap();
    assert!(repository_excludes.starts_with("# git ls-files"));
    assert!(repository_excludes.ends_with("*.cache\n"));
    std::fs::write(tmp.path().join("intended.cache"), "untracked").unwrap();
    std::fs::write(tmp.path().join("unrelated.txt"), "untracked").unwrap();
    assert!(git(tmp.path(), &["check-ignore", "intended.cache"]).contains("intended.cache"));
    assert!(!std::process::Command::new("git")
        .args(["check-ignore", "unrelated.txt"])
        .current_dir(tmp.path())
        .status()
        .unwrap()
        .success());
    repo.add_ignore_rule(IgnoreScope::User, IgnoreRule::Pattern("*.private"))
        .unwrap();
    let default_excludes =
        std::path::PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").unwrap()).join("git/ignore");
    assert_eq!(
        std::fs::read_to_string(default_excludes).unwrap(),
        "*.private\n"
    );
    assert_eq!(git(tmp.path(), &["show", "HEAD:tracked.txt"]), TEN_LINES);
    assert_eq!(
        git(tmp.path(), &["write-tree"]),
        git(tmp.path(), &["rev-parse", "HEAD^{tree}"])
    );
}

#[test]
fn configured_user_excludes_and_linked_worktree_repository_scope_resolve_correctly() {
    let (tmp, repo) = RepoFixture::new()
        .commit("initial", [("tracked.txt", TEN_LINES)])
        .build();
    let configured = tmp.path().join("private/excludes");
    git(
        tmp.path(),
        &[
            "config",
            "--local",
            "core.excludesFile",
            configured.to_str().unwrap(),
        ],
    );
    repo.add_ignore_rule(
        IgnoreScope::User,
        IgnoreRule::Pattern("*.configured-private"),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&configured).unwrap(),
        "*.configured-private\n"
    );

    let linked_path = tmp.path().join("linked-worktree");
    git(
        tmp.path(),
        &[
            "worktree",
            "add",
            "-b",
            "ignore-linked",
            linked_path.to_str().unwrap(),
        ],
    );
    let linked = GitRepo::discover(&linked_path).unwrap().unwrap();
    assert_ne!(linked.git_dir(), linked.common_dir());
    assert_eq!(linked.common_dir(), repo.common_dir());
    linked
        .add_ignore_rule(
            IgnoreScope::Repository,
            IgnoreRule::Pattern("*.linked-cache"),
        )
        .unwrap();
    assert!(
        std::fs::read_to_string(repo.common_dir().join("info/exclude"))
            .unwrap()
            .ends_with("*.linked-cache\n")
    );
}

#[test]
fn ignore_rejects_traversal_symlinks_and_write_failures() {
    let (tmp, repo) = RepoFixture::new()
        .commit("initial", [("tracked.txt", TEN_LINES)])
        .build();
    let head = git(tmp.path(), &["rev-parse", "HEAD"]);
    let index = git(tmp.path(), &["write-tree"]);
    assert!(repo
        .add_ignore_rule(
            IgnoreScope::Root,
            IgnoreRule::LiteralPath(Path::new("../outside"))
        )
        .is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(tmp.path().join("elsewhere"), tmp.path().join("linked"))
            .unwrap();
        assert!(repo
            .add_ignore_rule(
                IgnoreScope::Directory,
                IgnoreRule::LiteralPath(Path::new("linked/file"))
            )
            .is_err());
        assert!(!tmp.path().join("elsewhere/.gitignore").exists());
    }
    std::fs::create_dir(tmp.path().join(".gitignore")).unwrap();
    assert!(repo
        .add_ignore_rule(IgnoreScope::Root, IgnoreRule::Pattern("*.tmp"))
        .is_err());
    assert_eq!(git(tmp.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(tmp.path(), &["write-tree"]), index);
}

#[test]
fn notes_round_trip_on_explicit_commits_and_honor_configured_ref() {
    let (tmp, repo) = RepoFixture::new()
        .commit("first", [("f.txt", TEN_LINES)])
        .commit("second", [("g.txt", TEN_LINES)])
        .build();
    let first = git(tmp.path(), &["rev-parse", "HEAD~"]);
    let second = git(tmp.path(), &["rev-parse", "HEAD"]);
    git(
        tmp.path(),
        &["config", "core.notesRef", "refs/notes/custom"],
    );
    let head = git(tmp.path(), &["rev-parse", "HEAD"]);
    let index = git(tmp.path(), &["write-tree"]);
    let worktree = git(tmp.path(), &["status", "--porcelain"]);
    repo.add_note(&first, "first note").expect("add note");
    assert!(repo.add_note(&first, "must not overwrite").is_err());
    assert_eq!(
        repo.note(&first).expect("read note").as_deref(),
        Some("first note\n")
    );
    repo.add_note(&second, "second note")
        .expect("add other note");
    repo.replace_note(&first, "replaced").expect("replace note");
    assert_eq!(
        repo.note(&first).expect("read replaced note").as_deref(),
        Some("replaced\n")
    );
    assert_eq!(
        repo.note(&second).expect("read other note").as_deref(),
        Some("second note\n")
    );
    assert_eq!(
        git(tmp.path(), &["notes", "--ref=refs/notes/commits", "list"]).trim(),
        ""
    );
    repo.remove_note(&first).expect("remove note");
    assert_eq!(repo.note(&first).expect("missing note"), None);
    assert_eq!(git(tmp.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(tmp.path(), &["write-tree"]), index);
    assert_eq!(git(tmp.path(), &["status", "--porcelain"]), worktree);
    assert_eq!(git(tmp.path(), &["rev-parse", "HEAD~"]), first);
    assert!(repo.note("not-a-commit").is_err());
    assert_eq!(
        repo.note("HEAD").expect("read HEAD note").as_deref(),
        Some("second note\n")
    );
    let lock = repo.git_dir().join("refs/notes/custom.lock");
    std::fs::create_dir_all(lock.parent().unwrap()).expect("notes directory");
    write(&lock, "locked\n");
    assert!(repo.replace_note(&second, "failed replacement").is_err());
    std::fs::remove_file(lock).expect("remove notes lock");
    assert_eq!(
        repo.note(&second)
            .expect("note retained after failure")
            .as_deref(),
        Some("second note\n")
    );
    git(
        tmp.path(),
        &["config", "core.notesRef", "refs/notes/broken"],
    );
    write(
        &repo.git_dir().join("refs/notes/broken"),
        "not-an-object-id\n",
    );
    assert!(repo.note(&second).is_err());
    git(
        tmp.path(),
        &["config", "core.notesRef", "refs/notes/custom"],
    );
    assert_eq!(
        repo.note(&second)
            .expect("original note retained")
            .as_deref(),
        Some("second note\n")
    );
}

#[test]
fn notes_reject_unborn_targets_and_propagate_read_failures() {
    let (_tmp, repo) = RepoFixture::new().unborn().build();
    assert!(repo.note("HEAD").is_err());
    assert!(repo.add_note("HEAD", "unborn").is_err());
}

#[test]
fn configuration_read_failures_are_not_reported_as_unset() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    git(tmp.path(), &["branch", "feature"]);
    write(&repo.git_dir().join("config"), "[broken\n");
    assert!(repo.branch_rebase("feature").is_err());
    assert!(repo.pull_rebase().is_err());
}

#[test]
fn failed_configuration_clears_preserve_values_and_absent_clears_succeed() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    git(tmp.path(), &["branch", "feature"]);
    repo.set_branch_rebase("feature", Some(true))
        .expect("set branch value");
    repo.set_pull_rebase(Some(true)).expect("set repo value");
    repo.set_branch_push_remote("main", None)
        .expect("clear absent branch value");
    repo.set_push_default(None)
        .expect("clear absent repo value");
    let lock = repo.git_dir().join("config.lock");
    write(&lock, "locked\n");
    assert!(repo.set_branch_rebase("feature", None).is_err());
    assert!(repo.set_pull_rebase(None).is_err());
    std::fs::remove_file(lock).expect("remove config lock");
    assert_eq!(
        repo.branch_rebase("feature")
            .expect("branch value retained"),
        Some(true)
    );
    assert_eq!(repo.pull_rebase().expect("repo value retained"), Some(true));
}

#[test]
fn branch_and_repository_configuration_round_trips_locally() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    git(tmp.path(), &["branch", "feature"]);
    git(
        tmp.path(),
        &["remote", "add", "origin", "https://example.invalid/repo"],
    );
    git(
        tmp.path(),
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    write(
        &tmp.path().join("untracked.txt"),
        "keep this worktree content\n",
    );
    let head = git(tmp.path(), &["rev-parse", "HEAD"]);
    repo.set_branch_upstream("feature", Some("origin/main"))
        .expect("set upstream");
    assert_eq!(
        repo.branch_upstream("feature")
            .expect("read upstream")
            .as_deref(),
        Some("origin/main")
    );
    repo.set_branch_rebase("feature", Some(true))
        .expect("set rebase");
    assert_eq!(
        repo.branch_rebase("feature").expect("read rebase"),
        Some(true)
    );
    repo.set_branch_push_remote("feature", Some("origin"))
        .expect("set push remote");
    assert_eq!(
        repo.branch_push_remote("feature")
            .expect("read push remote")
            .as_deref(),
        Some("origin")
    );
    repo.set_branch_upstream("feature", None)
        .expect("clear upstream");
    repo.set_branch_rebase("feature", None)
        .expect("clear rebase");
    repo.set_branch_push_remote("feature", None)
        .expect("clear push remote");
    assert_eq!(
        repo.branch_upstream("feature")
            .expect("read cleared upstream"),
        None
    );
    assert_eq!(
        repo.branch_rebase("feature").expect("read cleared rebase"),
        None
    );
    assert_eq!(
        repo.branch_push_remote("feature")
            .expect("read cleared remote"),
        None
    );
    repo.set_pull_rebase(Some(true)).expect("set pull.rebase");
    assert_eq!(repo.pull_rebase().expect("read pull.rebase"), Some(true));
    repo.set_push_default(Some(magit_lsp::git::PushDefault::Current))
        .expect("set push.default");
    assert_eq!(
        repo.push_default().expect("read push.default"),
        Some(magit_lsp::git::PushDefault::Current)
    );
    assert!(git(
        tmp.path(),
        &[
            "config",
            "--show-origin",
            "--local",
            "--get",
            "push.default"
        ]
    )
    .contains("file:"));
    repo.set_pull_rebase(None).expect("clear pull.rebase");
    repo.set_push_default(None).expect("clear push.default");
    assert_eq!(repo.pull_rebase().expect("read cleared pull.rebase"), None);
    assert_eq!(
        repo.push_default().expect("read cleared push.default"),
        None
    );
    assert_eq!(
        git(tmp.path(), &["branch", "--show-current"]).trim(),
        "main"
    );
    assert_eq!(git(tmp.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(
        repo.branch_rebase("main").expect("other branch untouched"),
        None
    );
    assert_eq!(
        read(&tmp.path().join("untracked.txt")),
        "keep this worktree content\n"
    );
}

#[test]
fn branch_configuration_rejects_invalid_targets_and_handles_head_states() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    git(tmp.path(), &["branch", "feature"]);
    assert!(repo
        .set_branch_rebase("missing", Some(true))
        .unwrap_err()
        .to_string()
        .contains("missing"));
    assert!(repo
        .set_branch_upstream("feature", Some("origin/missing"))
        .is_err());
    assert!(repo
        .set_branch_push_remote("feature", Some("missing-remote"))
        .is_err());
    assert_eq!(
        repo.branch_rebase("feature")
            .expect("failed input did not write"),
        None
    );
    git(
        tmp.path(),
        &[
            "config",
            "--local",
            "branch.feature.rebase",
            "not-a-boolean",
        ],
    );
    assert!(repo
        .branch_rebase("feature")
        .unwrap_err()
        .to_string()
        .contains("Invalid"));
    git(
        tmp.path(),
        &["config", "--local", "push.default", "not-a-mode"],
    );
    assert!(repo
        .push_default()
        .unwrap_err()
        .to_string()
        .contains("Invalid push.default"));

    let (_detached_tmp, detached) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .detached()
        .build();
    detached
        .set_branch_rebase("main", Some(false))
        .expect("explicit branch works with detached HEAD");
    assert_eq!(detached.current_branch(), None);
    assert_eq!(
        detached
            .branch_rebase("main")
            .expect("read detached branch config"),
        Some(false)
    );

    let (_unborn_tmp, unborn) = RepoFixture::new()
        .unborn()
        .staged_file("f.txt", TEN_LINES)
        .build();
    assert!(unborn.set_branch_rebase("main", Some(true)).is_err());
    assert_eq!(unborn.current_branch(), Some("main".to_owned()));
}

/// `git diff [-U3]` for [`standard_changes`]: the whole fixture staged.
const STANDARD_DIFF: &str = concat!(
    "diff --git a/f.txt b/f.txt\n",
    "index 4083766..31cf455 100644\n",
    "--- a/f.txt\n",
    "+++ b/f.txt\n",
    "@@ -1,10 +1,10 @@\n",
    " line1\n",
    " line2\n",
    "-line3\n",
    "+line3 CHANGED\n",
    " line4\n",
    " line5\n",
    "+NEW line\n",
    " line6\n",
    " line7\n",
    "-line8\n",
    " line9\n",
    " line10\n",
);

/// Only the `line3` replacement staged.
const MODIFICATION_DIFF: &str = concat!(
    "diff --git a/f.txt b/f.txt\n",
    "index 4083766..cf23a08 100644\n",
    "--- a/f.txt\n",
    "+++ b/f.txt\n",
    "@@ -1,6 +1,6 @@\n",
    " line1\n",
    " line2\n",
    "-line3\n",
    "+line3 CHANGED\n",
    " line4\n",
    " line5\n",
    " line6\n",
);

/// Only the `NEW line` insertion staged.
const ADDITION_DIFF: &str = concat!(
    "diff --git a/f.txt b/f.txt\n",
    "index 4083766..411441e 100644\n",
    "--- a/f.txt\n",
    "+++ b/f.txt\n",
    "@@ -3,6 +3,7 @@ line2\n",
    " line3\n",
    " line4\n",
    " line5\n",
    "+NEW line\n",
    " line6\n",
    " line7\n",
    " line8\n",
);

/// Only the `line8` deletion staged.
const DELETION_DIFF: &str = concat!(
    "diff --git a/f.txt b/f.txt\n",
    "index 4083766..fddd710 100644\n",
    "--- a/f.txt\n",
    "+++ b/f.txt\n",
    "@@ -5,6 +5,5 @@ line4\n",
    " line5\n",
    " line6\n",
    " line7\n",
    "-line8\n",
    " line9\n",
    " line10\n",
);

/// The insertion and the deletion staged, with `line3` left alone.
const INSERTION_AND_DELETION_DIFF: &str = concat!(
    "diff --git a/f.txt b/f.txt\n",
    "index 4083766..54d9ea4 100644\n",
    "--- a/f.txt\n",
    "+++ b/f.txt\n",
    "@@ -3,8 +3,8 @@ line2\n",
    " line3\n",
    " line4\n",
    " line5\n",
    "+NEW line\n",
    " line6\n",
    " line7\n",
    "-line8\n",
    " line9\n",
    " line10\n",
);

/// Both replacements of the 30-line fixture staged.
const TWO_HUNKS_DIFF: &str = concat!(
    "diff --git a/f.txt b/f.txt\n",
    "index 19339a3..b5dd80e 100644\n",
    "--- a/f.txt\n",
    "+++ b/f.txt\n",
    "@@ -1,5 +1,5 @@\n",
    " line1\n",
    "-line2\n",
    "+line2 CHANGED\n",
    " line3\n",
    " line4\n",
    " line5\n",
    "@@ -25,6 +25,6 @@ line24\n",
    " line25\n",
    " line26\n",
    " line27\n",
    "-line28\n",
    "+line28 CHANGED\n",
    " line29\n",
    " line30\n",
);

/// `f.txt`'s `line3` replacement in the two-file fixture.
const TWO_FILES_F_DIFF: &str = concat!(
    "diff --git a/f.txt b/f.txt\n",
    "index 4083766..a716c2d 100644\n",
    "--- a/f.txt\n",
    "+++ b/f.txt\n",
    "@@ -1,6 +1,6 @@\n",
    " line1\n",
    " line2\n",
    "-line3\n",
    "+f3 CHANGED\n",
    " line4\n",
    " line5\n",
    " line6\n",
);

/// `g.txt`'s `line3` replacement in the two-file fixture.
const TWO_FILES_G_DIFF: &str = concat!(
    "diff --git a/g.txt b/g.txt\n",
    "index 4083766..4ad0b2b 100644\n",
    "--- a/g.txt\n",
    "+++ b/g.txt\n",
    "@@ -1,6 +1,6 @@\n",
    " line1\n",
    " line2\n",
    "-line3\n",
    "+g3 CHANGED\n",
    " line4\n",
    " line5\n",
    " line6\n",
);

#[test]
fn remote_configuration_preserves_exact_urls_and_rejects_invalid_input_without_changes() {
    let (fixture, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    repo.add_remote("origin", "original").expect("add remote");
    repo.add_remote("other", "other-original")
        .expect("second remote");
    fixture.git(&["config", "--add", "remote.origin.url", "extra-fetch"]);
    fixture.git(&["config", "--add", "remote.origin.pushurl", "extra-push"]);
    fixture.git(&["config", "remote.origin.prune", "true"]);
    let fetch = "https://example.invalid/repo with spaces?x=1&y=2";
    let push = "git@example.invalid:team/push repo.git";
    repo.remote_configure("origin", fetch, push)
        .expect("configure URLs");
    assert_eq!(
        fixture.git(&["config", "--get-all", "remote.origin.url"]),
        format!("{fetch}\n")
    );
    assert_eq!(
        fixture.git(&["config", "--get-all", "remote.origin.pushurl"]),
        format!("{push}\n")
    );
    assert_eq!(
        fixture.git(&["config", "--get", "remote.origin.prune"]),
        "true\n"
    );
    assert_eq!(
        fixture.git(&["config", "--get", "remote.other.url"]),
        "other-original\n"
    );
    let before = fixture.git(&["config", "--local", "--list"]);
    for (name, fetch, push) in [
        ("origin", "", "valid"),
        ("origin", "valid", ""),
        ("origin", "bad\nurl", "valid"),
        ("origin", "valid", "bad\rurl"),
        ("origin", "bad\0url", "valid"),
        ("origin", "valid", "bad\0url"),
        ("origin", "-fetch", "valid"),
        ("origin", "valid", "-push"),
        ("-origin", "valid", "valid"),
        ("missing", "valid", "valid"),
        ("", "valid", "valid"),
    ] {
        assert!(
            repo.remote_configure(name, fetch, push).is_err(),
            "accepted {name:?}, {fetch:?}, {push:?}"
        );
        assert_eq!(fixture.git(&["config", "--local", "--list"]), before);
    }
    assert!(repo.add_remote("origin", "replacement").is_err());
    assert!(repo.rename_remote("origin", "other").is_err());
    assert!(repo.add_remote("new", "-url").is_err());
    assert_eq!(fixture.git(&["config", "--local", "--list"]), before);
}

#[test]
fn lightweight_tag_selects_commit_and_rejects_invalid_or_conflicting_input_without_changes() {
    let (fixture, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .commit("second", [("f.txt", "second\n")])
        .build();
    let selected = fixture.git(&["rev-parse", "HEAD^"]);
    fixture.git(&["tag", "-a", "annotated", "-m", "selected commit", "HEAD^"]);
    fixture.git(&["config", "tag.gpgSign", "true"]);
    fixture.git(&["config", "gpg.program", "/nonexistent/tag-signing-program"]);
    repo.create_lightweight_tag("selected", "annotated")
        .expect("tag selected commit");
    assert_eq!(
        fixture.git(&["cat-file", "-t", "refs/tags/selected"]),
        "commit\n"
    );
    assert_eq!(fixture.git(&["rev-parse", "refs/tags/selected"]), selected);
    let refs = fixture.git(&["show-ref"]);
    let status = fixture.git(&["status", "--porcelain=v1"]);
    for (name, revision) in [
        ("selected", "HEAD"),
        ("invalid name", "HEAD"),
        ("-tag", "HEAD"),
        ("", "HEAD"),
        ("new", "missing"),
        ("new", "HEAD:f.txt"),
        ("new", "--all"),
        ("new", ""),
    ] {
        assert!(
            repo.create_lightweight_tag(name, revision).is_err(),
            "accepted {name:?}, {revision:?}"
        );
        assert_eq!(fixture.git(&["show-ref"]), refs);
        assert_eq!(fixture.git(&["status", "--porcelain=v1"]), status);
    }
    assert!(repo.create_tag("selected").is_err());
    assert!(repo.create_tag("-tag").is_err());
    assert_eq!(fixture.git(&["show-ref"]), refs);
    repo.delete_tag("selected")
        .expect("existing tag deletion still works");
}

#[test]
fn create_tag_is_lightweight_even_when_signing_is_enabled() {
    let (fixture, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    fixture.git(&["config", "tag.gpgSign", "true"]);
    fixture.git(&["config", "gpg.program", "/nonexistent/tag-signing-program"]);
    repo.create_tag("unsigned")
        .expect("lightweight tag ignores signing config");
    assert_eq!(
        fixture
            .git(&["cat-file", "-t", "refs/tags/unsigned"])
            .trim(),
        "commit"
    );
    assert_eq!(
        fixture.git(&["rev-parse", "refs/tags/unsigned"]),
        fixture.git(&["rev-parse", "HEAD"])
    );
}

#[test]
fn identical_fixtures_produce_identical_commit_hashes() {
    let (first, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (second, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();

    assert_eq!(
        first.git(&["rev-parse", "HEAD"]),
        second.git(&["rev-parse", "HEAD"])
    );
}

#[test]
fn status_reports_modified_file_with_hunk() {
    let (_tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();

    let status = repo.status().expect("status");
    assert_eq!(status.branch, "main");
    assert!(status.staged.is_empty());
    assert_eq!(status.unstaged.len(), 1);
    let entry = &status.unstaged[0];
    assert_eq!(entry.path, "f.txt");
    assert_eq!(entry.hunks.len(), 1);
    assert_eq!(entry.hunks[0].lines.len(), 12);
}

#[test]
fn diff_unstaged_returns_full_diff() {
    let (_tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let diff = repo.diff_unstaged("f.txt").expect("diff");
    assert_eq!(diff, STANDARD_DIFF);
}

#[test]
fn stage_hunk_stages_the_hunk() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();

    repo.stage_hunk("f.txt", &hunk_id).expect("stage hunk");

    assert_eq!(cached_diff(tmp.path()), STANDARD_DIFF);

    let status = repo.status().expect("status");
    assert!(status.unstaged.is_empty());
    assert_eq!(status.staged.len(), 1);
}

#[test]
fn discard_hunk_preserves_unrelated_worktree_content() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.discard_hunk("f.txt", &id).expect("discard hunk");
    assert!(repo.status().expect("status").unstaged.is_empty());
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).expect("read"),
        TEN_LINES
    );
}

#[test]
fn discard_untracked_file_removes_it() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    write(&tmp.path().join("untracked.bin"), "binary\0contents");
    repo.discard_file("untracked.bin")
        .expect("discard untracked");
    assert!(!tmp.path().join("untracked.bin").exists());
    assert!(repo.status().expect("status").untracked.is_empty());
}

#[test]
fn discard_selection_removes_selected_line_only() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let layout =
        FileLayout::from_diff(&repo.diff_unstaged("f.txt").expect("diff"), 0).expect("layout");
    let first = layout.hunks[0].first_line;
    assert!(repo
        .discard_selection(&[layout], first + 3, first + 3)
        .expect("discard selection"));
    let content = std::fs::read_to_string(tmp.path().join("f.txt")).expect("read");
    assert!(!content.contains("line3 CHANGED"));
    assert!(content.contains("NEW line"));
}

#[test]
fn stage_hunk_by_id_survives_line_shifts() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", lines(40).to_string())])
        .modify("f.txt", lines(40).with(35, "line35 CHANGED").to_string())
        .build();
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();

    // Insert lines above the hunk: its line numbers move, its content does not.
    write(
        &tmp.path().join("f.txt"),
        &format!(
            "inserted1\ninserted2\n{}",
            lines(40).with(35, "line35 CHANGED")
        ),
    );

    repo.stage_hunk("f.txt", &hunk_id)
        .expect("stage the shifted hunk by id");

    let cached = cached_diff(tmp.path());
    assert!(cached.contains("+line35 CHANGED"), "got: {cached}");
    assert!(!cached.contains("inserted1"), "got: {cached}");
    assert!(!cached.contains("inserted2"), "got: {cached}");
}

#[test]
fn stage_hunk_rejects_hunk_changed_since_render() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", lines(40).to_string())])
        .modify("f.txt", lines(40).with(35, "line35 CHANGED").to_string())
        .build();
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();

    write(
        &tmp.path().join("f.txt"),
        &lines(40).with(35, "line35 DIFFERENT").to_string(),
    );

    let err = repo
        .stage_hunk("f.txt", &hunk_id)
        .expect_err("the old hunk id must not resolve");
    assert_eq!(err.kind, ErrorKind::HunkNotFound);
    assert!(
        cached_diff(tmp.path()).is_empty(),
        "the index must stay untouched"
    );
}

#[test]
fn duplicate_content_hunks_get_distinct_ids_and_stage_independently() {
    let block = ["A", "B", "C", "D", "target", "E", "F", "G", "H", "I"];
    let filler: Vec<String> = (1..=10).map(|i| format!("J{i}")).collect();
    let mut original: Vec<String> = block.iter().map(|s| s.to_string()).collect();
    original.extend(filler.iter().cloned());
    original.extend(block.iter().map(|s| s.to_string()));
    let original = original.join("\n") + "\n";
    let modified = original.replace("target\n", "changed\n");

    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", original)])
        .build();
    write(&tmp.path().join("f.txt"), &modified);

    let status = repo.status().expect("status");
    let hunks = &status.unstaged[0].hunks;
    assert_eq!(hunks.len(), 2, "the edits are far enough apart");
    assert_ne!(hunks[0].id, hunks[1].id);
    assert_eq!(hunks[1].id, format!("{}-1", hunks[0].id));

    // Stage only the second occurrence.
    repo.stage_hunk("f.txt", &hunks[1].id)
        .expect("stage the second duplicate");
    let cached = cached_diff(tmp.path());
    assert_eq!(cached.matches("+changed").count(), 1, "got: {cached}");
    assert_eq!(
        repo.status().expect("status").unstaged[0].hunks.len(),
        1,
        "the first duplicate stays unstaged"
    );

    // The remaining occurrence still has the base id and can be staged.
    repo.stage_hunk("f.txt", &hunks[0].id)
        .expect("stage the first duplicate");
    let cached = cached_diff(tmp.path());
    assert_eq!(cached.matches("+changed").count(), 2, "got: {cached}");
}

#[test]
fn stage_lines_stages_only_modification() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();

    // line3 modification is hunk lines 2..=3
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_lines("f.txt", &hunk_id, 2, 3)
        .expect("stage lines");

    assert_eq!(cached_diff(tmp.path()), MODIFICATION_DIFF);
}

#[test]
fn stage_lines_stages_only_addition() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();

    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_lines("f.txt", &hunk_id, 6, 6)
        .expect("stage lines");

    assert_eq!(cached_diff(tmp.path()), ADDITION_DIFF);
}

#[test]
fn stage_lines_stages_only_deletion() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();

    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_lines("f.txt", &hunk_id, 9, 9)
        .expect("stage lines");

    assert_eq!(cached_diff(tmp.path()), DELETION_DIFF);
}

#[test]
fn unstage_hunk_reverts_staged_hunk() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_hunk("f.txt", &hunk_id).expect("stage");
    assert!(!cached_diff(tmp.path()).is_empty());

    let staged_id = repo.status().expect("status").staged[0].hunks[0].id.clone();
    repo.unstage_hunk("f.txt", &staged_id).expect("unstage");

    let cached = cached_diff(tmp.path());
    assert!(!cached.contains("line3 CHANGED"));
    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert_eq!(status.unstaged.len(), 1);
}

#[test]
fn stage_file_stages_untracked_file() {
    let (tmp, repo) = init_repo();
    write(&tmp.path().join("new.txt"), "hello\n");

    let status = repo.status().expect("status");
    assert!(status.unstaged.is_empty());
    assert_eq!(status.untracked.len(), 1);
    assert_eq!(status.untracked[0].status, "?");

    repo.stage_file("new.txt").expect("stage file");

    let status = repo.status().expect("status");
    assert!(status.unstaged.is_empty());
    assert!(status.untracked.is_empty());
    assert_eq!(status.staged.len(), 1);
    assert_eq!(status.staged[0].path, "new.txt");
}

#[test]
fn unstage_file_unstages_staged_file() {
    let (_tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    repo.stage_file("f.txt").expect("stage file");
    assert_eq!(repo.status().expect("status").staged.len(), 1);

    repo.unstage_file("f.txt").expect("unstage file");

    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert_eq!(status.unstaged.len(), 1);
}

#[test]
fn status_reports_partially_staged_as_both() {
    let (_tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_lines("f.txt", &hunk_id, 2, 3)
        .expect("stage lines");

    let status = repo.status().expect("status");
    assert_eq!(status.staged.len(), 1);
    assert_eq!(status.unstaged.len(), 1);
    let staged = &status.staged[0];
    let unstaged = &status.unstaged[0];
    assert_eq!(staged.path, "f.txt");
    assert_eq!(unstaged.path, "f.txt");
    // staged side only has the line3 change; unstaged side has the rest
    assert!(staged
        .hunks
        .iter()
        .any(|h| { h.lines.iter().any(|l| l.text == "line3 CHANGED") }));
    assert!(unstaged
        .hunks
        .iter()
        .any(|h| { h.lines.iter().any(|l| l.text == "NEW line") }));
}

#[test]
fn stage_all_stages_everything() {
    let (_tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();

    repo.stage_all().expect("stage all");

    let status = repo.status().expect("status");
    assert_eq!(status.staged.len(), 1);
    assert!(status.unstaged.is_empty());
}

#[test]
fn unstage_all_resets_the_index() {
    let (_tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    repo.stage_all().expect("stage all");

    repo.unstage_all().expect("unstage all");

    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert_eq!(status.unstaged.len(), 1);
}

#[test]
fn unstage_all_works_without_commits() {
    let (_tmp, repo) = RepoFixture::new()
        .unborn()
        .staged_file("f.txt", "hello\n")
        .build();
    assert_eq!(repo.status().expect("status").staged.len(), 1);

    repo.unstage_all().expect("unstage all without HEAD");

    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert!(status.unstaged.is_empty());
    assert_eq!(status.untracked.len(), 1);
    assert_eq!(status.untracked[0].status, "?");
}

#[test]
fn unstage_file_works_without_commits() {
    let (_tmp, repo) = init_unborn_repo();
    assert_eq!(repo.status().expect("status").staged.len(), 1);

    repo.unstage_file("f.txt")
        .expect("unstage file without HEAD");

    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert!(status.unstaged.is_empty());
    assert_eq!(status.untracked.len(), 1);
    assert_eq!(status.untracked[0].status, "?");
}

#[test]
fn unstage_file_keeps_worktree_changes_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    // The index still holds a/b/c while the working tree holds a/B/c.
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    repo.unstage_file("f.txt")
        .expect("unstage file without HEAD");

    assert_eq!(read(&tmp.path().join("f.txt")), "a\nB\nc\n");
    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert!(status.unstaged.is_empty());
    assert_eq!(status.untracked.len(), 1);
    assert_eq!(status.untracked[0].status, "?");
}

#[test]
fn unstage_hunk_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    let hunk_id = repo.status().expect("status").staged[0].hunks[0].id.clone();
    repo.unstage_hunk("f.txt", &hunk_id)
        .expect("unstage hunk without HEAD");

    assert_eq!(read(&tmp.path().join("f.txt")), "a\nB\nc\n");
    assert!(repo.status().expect("status").staged.is_empty());
}

#[test]
fn unstage_lines_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    // Hunk lines are +a, +b, +c; unstage only +b.
    let hunk_id = repo.status().expect("status").staged[0].hunks[0].id.clone();
    repo.unstage_lines("f.txt", &hunk_id, 1, 1)
        .expect("unstage lines without HEAD");

    assert_eq!(read(&tmp.path().join("f.txt")), "a\nB\nc\n");
    let status = repo.status().expect("status");
    assert_eq!(status.staged.len(), 1);
    let staged = &status.staged[0];
    assert_eq!(staged.hunks.len(), 1);
    let text: Vec<&str> = staged.hunks[0]
        .lines
        .iter()
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(text, vec!["a", "c"]);
}

#[test]
fn stage_hunk_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_hunk("f.txt", &hunk_id)
        .expect("stage hunk without HEAD");

    assert_eq!(git(tmp.path(), &["show", ":f.txt"]), "a\nB\nc\n");
    assert!(repo.status().expect("status").unstaged.is_empty());
}

#[test]
fn stage_lines_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    // Hunk lines are context a, -b, +B, context c.
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_lines("f.txt", &hunk_id, 1, 2)
        .expect("stage lines without HEAD");

    assert_eq!(git(tmp.path(), &["show", ":f.txt"]), "a\nB\nc\n");
    assert!(repo.status().expect("status").unstaged.is_empty());
}

#[test]
fn stage_file_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("new.txt"), "hello\n");

    repo.stage_file("new.txt").expect("stage file without HEAD");

    let status = repo.status().expect("status");
    assert_eq!(status.staged.len(), 2);
    assert!(status.staged.iter().any(|f| f.path == "new.txt"));
}

#[test]
fn stage_all_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    repo.stage_all().expect("stage all without HEAD");

    assert_eq!(git(tmp.path(), &["show", ":f.txt"]), "a\nB\nc\n");
    assert!(repo.status().expect("status").unstaged.is_empty());
}

fn layout_for(repo: &GitRepo, path: &str, staged: bool) -> FileLayout {
    let diff = if staged {
        repo.diff_staged(path).expect("staged diff")
    } else {
        repo.diff_unstaged(path).expect("unstaged diff")
    };
    FileLayout::from_diff(&diff, 0).expect("file layout")
}

/// Stage the replacement of `old` by `new`, recomputing the unstaged layout
/// first so earlier stagings are reflected.
fn stage_replacement(repo: &GitRepo, path: &str, old: &str, new: &str) {
    let layout = layout_for(repo, path, false);
    let (hunk_id, lines) = layout
        .hunks
        .iter()
        .find_map(|hunk| {
            let lines: Vec<usize> = hunk
                .hunk
                .lines
                .iter()
                .enumerate()
                .filter(|(_, line)| {
                    (line.marker == '-' && line.text == old)
                        || (line.marker == '+' && line.text == new)
                })
                .map(|(index, _)| index)
                .collect();
            (!lines.is_empty()).then_some((hunk.hunk.id.clone(), lines))
        })
        .unwrap_or_else(|| panic!("no change {old} -> {new}"));
    repo.stage_lines(path, &hunk_id, lines[0], lines[lines.len() - 1])
        .expect("stage change");
}

/// Assert the status view's staged hunks equal what `git diff --cached -U3`
/// printed for `f.txt`.
fn assert_staged_hunks_match_git(repo: &GitRepo, tmp: &Path) {
    let cached = cached_diff(tmp);
    let expected = patch::parse_file_diff(&cached)
        .expect("cached diff contains one file")
        .hunks;
    let status = repo.status().expect("status");
    let staged = status
        .staged
        .iter()
        .find(|file| file.path == "f.txt")
        .expect("f.txt is staged");
    assert_eq!(
        staged.hunks, expected,
        "status hunks must match git diff --cached -U3\n{cached}"
    );
}

#[test]
fn two_distant_line_stagings_show_two_staged_hunks() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", lines(40).to_string())])
        .build();
    write(
        &tmp.path().join("f.txt"),
        &lines(40)
            .with(5, "line5 CHANGED")
            .with(35, "line35 CHANGED")
            .to_string(),
    );

    stage_replacement(&repo, "f.txt", "line5", "line5 CHANGED");
    stage_replacement(&repo, "f.txt", "line35", "line35 CHANGED");

    let cached = cached_diff(tmp.path());
    assert!(cached.contains("-line5\n"), "got: {cached}");
    assert!(cached.contains("+line5 CHANGED\n"), "got: {cached}");
    assert!(cached.contains("-line35\n"), "got: {cached}");
    assert!(cached.contains("+line35 CHANGED\n"), "got: {cached}");
    assert_eq!(hunk_count(&cached), 2, "got: {cached}");
    assert_staged_hunks_match_git(&repo, tmp.path());
}

#[test]
fn nearby_line_stagings_show_one_staged_hunk() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", lines(40).to_string())])
        .build();
    write(
        &tmp.path().join("f.txt"),
        &lines(40)
            .with(5, "line5 CHANGED")
            .with(9, "line9 CHANGED")
            .to_string(),
    );

    stage_replacement(&repo, "f.txt", "line5", "line5 CHANGED");
    stage_replacement(&repo, "f.txt", "line9", "line9 CHANGED");

    let cached = cached_diff(tmp.path());
    assert!(cached.contains("-line9\n"), "got: {cached}");
    assert!(cached.contains("+line9 CHANGED\n"), "got: {cached}");
    assert_eq!(hunk_count(&cached), 1, "got: {cached}");
    assert_staged_hunks_match_git(&repo, tmp.path());
}

#[test]
fn status_hunks_match_git_when_staged_together() {
    for (first, second) in [(4usize, 34usize), (4, 8)] {
        let (tmp, repo) = RepoFixture::new()
            .commit("init", [("f.txt", lines(40).to_string())])
            .build();
        write(
            &tmp.path().join("f.txt"),
            &lines(40)
                .with(first + 1, format!("line{} CHANGED", first + 1))
                .with(second + 1, format!("line{} CHANGED", second + 1))
                .to_string(),
        );

        let layout = layout_for(&repo, "f.txt", false);
        let start = layout.hunks.first().expect("hunk").first_line;
        let end = layout
            .hunks
            .last()
            .map(|hunk| hunk.first_line + hunk.hunk.lines.len() as u32 - 1)
            .expect("hunk");
        let applied = repo
            .apply_selection(&[layout], start, end, false)
            .expect("apply selection");
        assert!(applied);

        let cached = cached_diff(tmp.path());
        let expected_hunks = if second - first - 1 > 6 { 2 } else { 1 };
        assert_eq!(hunk_count(&cached), expected_hunks, "got: {cached}");
        assert_staged_hunks_match_git(&repo, tmp.path());
    }
}

#[test]
fn selection_context_only_is_noop() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let layout = layout_for(&repo, "f.txt", false);
    let context_line = layout.hunks[0].first_line;

    let applied = repo
        .apply_selection(&[layout], context_line, context_line, false)
        .expect("apply selection");

    assert!(!applied, "context-only selection must not apply anything");
    assert!(cached_diff(tmp.path()).is_empty());
}

#[test]
fn selection_context_plus_change_stages_only_change() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let layout = layout_for(&repo, "f.txt", false);
    let first = layout.hunks[0].first_line;

    // Context line1 through the line3 replacement.
    let applied = repo
        .apply_selection(&[layout], first, first + 3, false)
        .expect("apply selection");

    assert!(applied);
    assert_eq!(cached_diff(tmp.path()), MODIFICATION_DIFF);
}

#[test]
fn selection_header_plus_change_stages_whole_file() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let layout = layout_for(&repo, "f.txt", false);
    let end = layout.hunks[0].first_line + 3;

    let applied = repo
        .apply_selection(&[layout], 0, end, false)
        .expect("apply selection");

    assert!(applied);
    assert_eq!(cached_diff(tmp.path()), STANDARD_DIFF);
}

#[test]
fn selection_crossing_two_hunks_stages_both() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", lines(30).to_string())])
        .build();
    write(
        &tmp.path().join("f.txt"),
        &lines(30)
            .with(2, "line2 CHANGED")
            .with(28, "line28 CHANGED")
            .to_string(),
    );

    let layout = layout_for(&repo, "f.txt", false);
    assert_eq!(layout.hunks.len(), 2);
    let start = layout.hunks[0].first_line + 1;
    let end = layout.hunks[1].first_line + 1;

    let applied = repo
        .apply_selection(&[layout], start, end, false)
        .expect("apply selection");

    assert!(applied);
    assert_eq!(cached_diff(tmp.path()), TWO_HUNKS_DIFF);
}

#[test]
fn selection_crossing_two_files_stages_both() {
    let (tmp, repo) = RepoFixture::new()
        .commit(
            "init",
            [
                ("f.txt", lines(10).to_string()),
                ("g.txt", lines(10).to_string()),
            ],
        )
        .modify("f.txt", lines(10).with(3, "f3 CHANGED").to_string())
        .modify("g.txt", lines(10).with(3, "g3 CHANGED").to_string())
        .build();

    let first = layout_for(&repo, "f.txt", false);
    let second =
        FileLayout::from_diff(&repo.diff_unstaged("g.txt").expect("diff"), 100).expect("layout");
    let start = first.hunks[0].first_line + 1;
    let end = second.hunks[0].first_line + 1;

    let applied = repo
        .apply_selection(&[first, second], start, end, false)
        .expect("apply selection");

    assert!(applied);
    assert_eq!(
        git(tmp.path(), &["diff", "--cached", "--", "f.txt"]),
        TWO_FILES_F_DIFF
    );
    assert_eq!(
        git(tmp.path(), &["diff", "--cached", "--", "g.txt"]),
        TWO_FILES_G_DIFF
    );
}

#[test]
fn selection_inside_staged_hunk_unstages_only_that_change() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    repo.stage_file("f.txt").expect("stage file");
    let layout = layout_for(&repo, "f.txt", true);
    let first = layout.hunks[0].first_line;

    // The staged hunk has the same shape as the unstaged sample.
    let applied = repo
        .apply_selection(&[layout], first + 2, first + 3, true)
        .expect("apply selection");

    assert!(applied);
    assert_eq!(cached_diff(tmp.path()), INSERTION_AND_DELETION_DIFF);
}

#[test]
fn stage_lines_context_only_returns_false() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();

    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    let applied = repo
        .stage_lines("f.txt", &hunk_id, 0, 0)
        .expect("stage lines");

    assert!(!applied);
    assert!(cached_diff(tmp.path()).is_empty());
}

fn random_content(edits: &[u8]) -> (String, String) {
    let mut original = String::new();
    let mut modified = String::new();
    for (i, edit) in edits.iter().enumerate() {
        original.push_str(&format!("L{i}\n"));
        match edit % 4 {
            0 => modified.push_str(&format!("L{i}\n")),
            1 => modified.push_str(&format!("M{i}\n")),
            2 => {}
            _ => {
                modified.push_str(&format!("L{i}\n"));
                modified.push_str(&format!("I{i}\n"));
            }
        }
    }
    (original, modified)
}

fn model_selection(
    current: &str,
    layout: &FileLayout,
    start: u32,
    end: u32,
    reverse: bool,
) -> String {
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let mut source: Vec<String> = current.lines().map(str::to_owned).collect();
    let mut result = Vec::new();
    let mut cursor = 0usize;

    for hunk in &layout.hunks {
        let whole_file = start <= layout.header_line && layout.header_line <= end;
        let whole_hunk = start <= hunk.header_line && hunk.header_line <= end;
        let source_start = if reverse {
            hunk.hunk.new_start
        } else {
            hunk.hunk.old_start
        } as usize;
        let before = source_start.saturating_sub(1);
        result.extend(source[cursor..before].iter().cloned());
        cursor = before;

        for (index, line) in hunk.hunk.lines.iter().enumerate() {
            let document_line = hunk.first_line + index as u32;
            let selected =
                whole_file || whole_hunk || (document_line >= start && document_line <= end);
            let source_line = if reverse { line.new_no } else { line.old_no };
            let is_source = if reverse {
                line.marker == '+'
            } else {
                line.marker == '-'
            };
            let is_target = if reverse {
                line.marker == '-'
            } else {
                line.marker == '+'
            };

            if line.marker == ' ' {
                if let Some(number) = source_line {
                    let position = number as usize - 1;
                    result.extend(source[cursor..position].iter().cloned());
                    result.push(source[position].clone());
                    cursor = position + 1;
                }
            } else if is_source {
                if let Some(number) = source_line {
                    let position = number as usize - 1;
                    result.extend(source[cursor..position].iter().cloned());
                    if !selected {
                        result.push(source[position].clone());
                    }
                    cursor = position + 1;
                }
            } else if is_target && selected {
                result.push(line.text.clone());
            }
        }
    }
    result.extend(source.drain(cursor..));
    if !result.is_empty() {
        result.join("\n") + "\n"
    } else {
        result.join("\n")
    }
}

fn index_content(repo: &RepoFixture) -> String {
    repo.git(&["show", ":f.txt"])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_selections_apply_expected_content_and_reverse_back(
        edits in prop::collection::vec(0u8..4, 1..25),
        a in 0usize..60,
        b in 0usize..60,
        reverse in any::<bool>(),
    ) {
        let (original, modified) = random_content(&edits);
        prop_assume!(original != modified);

        let (tmp, repo) = RepoFixture::new()
            .commit("init", [("f.txt", original.clone())])
            .modify("f.txt", modified.clone())
            .build();
        if reverse {
            tmp.git(&["add", "f.txt"]);
        }
        let initial = if reverse { &modified } else { &original };
        let diff = if reverse { repo.diff_staged("f.txt") } else { repo.diff_unstaged("f.txt") }.expect("diff");
        prop_assume!(!diff.is_empty());
        let layout = FileLayout::from_diff(&diff, 0).expect("layout");
        let max_line = layout
            .hunks
            .last()
            .map(|h| h.first_line + h.hunk.lines.len() as u32 - 1)
            .unwrap_or(0);
        let a = (a as u32).min(max_line);
        let b = (b as u32).min(max_line);
        let (start, end) = if a <= b { (a, b) } else { (b, a) };

        let expected = model_selection(initial, &layout, start, end, reverse);
        let has_changes = layout.header_line >= start && layout.header_line <= end || layout.hunks.iter().any(|hunk| {
            let whole_hunk = hunk.header_line >= start && hunk.header_line <= end;
            hunk.hunk.lines.iter().enumerate().any(|(i, line)| {
                (line.marker == '+' || line.marker == '-') && (whole_hunk || (hunk.first_line + i as u32 >= start && hunk.first_line + i as u32 <= end))
            })
        });
        let applied = repo.apply_selection(&[layout], start, end, reverse).expect("apply selection");
        prop_assert_eq!(applied, has_changes, "selection {}..={}", start, end);
        prop_assert_eq!(index_content(&tmp), expected);

        let staged = if reverse { repo.diff_unstaged("f.txt") } else { repo.diff_staged("f.txt") }.expect("inverse diff");
        if staged.is_empty() {
            prop_assert_eq!(index_content(&tmp), initial.as_str());
        } else {
            let staged_layout = FileLayout::from_diff(&staged, 0).expect("staged layout");
            let changed: Vec<u32> = staged_layout.hunks.iter().flat_map(|hunk| {
                hunk.hunk.lines.iter().enumerate().filter_map(move |(i, line)| {
                    (line.marker == '+' || line.marker == '-').then_some(hunk.first_line + i as u32)
                })
            }).collect();
            let (reverse_start, reverse_end) = (*changed.first().unwrap(), *changed.last().unwrap());
            let applied = repo.apply_selection(&[staged_layout], reverse_start, reverse_end, !reverse).expect("inverse selection");
            prop_assert!(applied);
            prop_assert_eq!(index_content(&tmp), initial.as_str());
        }
    }
}

#[test]
fn stage_lines_partial_deletion_uses_normal_header() {
    let (tmp, repo) = init_repo();
    std::fs::remove_file(tmp.path().join("f.txt")).expect("remove file");

    // The deletion hunk is -line1 .. -line10; stage only -line3.
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    let applied = repo
        .stage_lines("f.txt", &hunk_id, 2, 2)
        .expect("stage lines");

    assert!(applied);
    let cached = cached_diff(tmp.path());
    assert!(cached.contains("-line3"), "got: {cached}");
    assert!(!cached.contains("-line1"), "got: {cached}");
    assert!(!cached.contains("-line10"), "got: {cached}");
}

#[test]
fn unstage_selection_context_only_is_noop() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    repo.stage_file("f.txt").expect("stage file");
    let layout = layout_for(&repo, "f.txt", true);
    let context_line = layout.hunks[0].first_line;

    let applied = repo
        .apply_selection(&[layout], context_line, context_line, true)
        .expect("apply selection");

    assert!(!applied, "context-only selection must not apply anything");
    assert!(cached_diff(tmp.path()).contains("line3 CHANGED"));
}
