mod common;

use common::RepoFixture;
use magit_lsp::git::{GitLogOptions, GitLogRow};

#[test]
fn log_pages_and_resolves_full_commit_ids() {
    let (_fixture, repo) = RepoFixture::new()
        .commit("first", vec![("f.txt", "one\n")])
        .commit("second — Unicode", vec![("f.txt", "two\n")])
        .build();
    let page = repo
        .log(&GitLogOptions {
            page_size: 1,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.commits.len(), 1);
    assert!(page.has_more);
    assert_eq!(page.commits[0].subject, "second — Unicode");
    assert_eq!(repo.resolve_commit("HEAD").unwrap(), page.commits[0].oid);
    let last = repo
        .log(&GitLogOptions {
            page_size: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(last.commits.len(), 2);
    assert!(!last.has_more);
}

#[test]
fn log_filters_are_passed_as_values_and_paths_are_literal() {
    let (_fixture, repo) = RepoFixture::new()
        .commit("add needle", vec![("f.txt", "needle\n")])
        .commit("replace needle", vec![("f.txt", "other\n")])
        .build();
    let matching = repo
        .log(&GitLogOptions {
            page_size: 10,
            pickaxe: Some("needle".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(matching.commits.len(), 2);
    let not_matching = repo
        .log(&GitLogOptions {
            page_size: 10,
            pickaxe_regex: Some("absent-pattern".into()),
            ..Default::default()
        })
        .unwrap();
    assert!(not_matching.commits.is_empty());
    assert!(repo
        .log(&GitLogOptions {
            revision: Some("--all".into()),
            ..Default::default()
        })
        .is_err());
}

#[test]
fn default_log_is_empty_for_unborn_repository() {
    let (_fixture, repo) = RepoFixture::new().unborn().build();
    let page = repo.log(&GitLogOptions::default()).unwrap();
    assert!(page.commits.is_empty());
    assert!(!page.has_more);
    assert!(repo.resolve_commit("HEAD").is_err());
}

#[test]
fn graph_rows_are_ordered_and_exclude_page_lookahead() {
    let (fixture, _) = RepoFixture::new()
        .commit("base", vec![("f", "base\n")])
        .commit("left", vec![("f", "left\n")])
        .build();
    // Add a side branch and merge it, producing connector-only graph rows.
    common::git(fixture.path(), &["checkout", "-b", "side", "HEAD~1"]);
    common::write(&fixture.path().join("side"), "side\n");
    common::git(fixture.path(), &["add", "side"]);
    common::git(fixture.path(), &["commit", "-m", "side"]);
    common::git(fixture.path(), &["checkout", "-"]);
    common::git(fixture.path(), &["merge", "--no-ff", "side", "-m", "merge"]);
    let repo = magit_lsp::git::GitRepo::open(fixture.path()).unwrap();
    let page = repo
        .log(&GitLogOptions {
            graph: true,
            page_size: 2,
            ..Default::default()
        })
        .unwrap();
    assert!(page.has_more);
    assert_eq!(
        page.rows
            .iter()
            .filter(|r| matches!(r, GitLogRow::Commit(_)))
            .count(),
        2
    );
    assert!(page.rows.iter().all(|r| match r {
        GitLogRow::Commit(oid) => page.commits.iter().any(|c| &c.oid == oid),
        _ => true,
    }));
}

#[test]
fn log_default_options_return_commits_and_overflow_is_rejected() {
    let (_fixture, repo) = RepoFixture::new()
        .commit("subject — λ", vec![("f", "x\n")])
        .build();
    assert_eq!(
        repo.log(&GitLogOptions::default()).unwrap().commits.len(),
        1
    );
    assert!(repo
        .log(&GitLogOptions {
            page_size: usize::MAX,
            ..Default::default()
        })
        .is_err());
    let page = repo
        .log(&GitLogOptions {
            graph: true,
            decorate: false,
            ..Default::default()
        })
        .unwrap();
    assert!(page.commits[0].decorations.is_empty());
    assert_eq!(page.commits[0].subject, "subject — λ");
}
