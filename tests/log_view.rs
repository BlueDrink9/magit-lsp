use magit_lsp::git::{GitLogOptions, GitRepo};
use magit_lsp::log_view::{CommitSelection, LogView};
use magit_lsp::view::View;
use std::path::PathBuf;

#[test]
fn log_backend_returns_structured_commit_rows() {
    let repo = GitRepo::open(env!("CARGO_MANIFEST_DIR")).expect("repository");
    let page = repo
        .log(&GitLogOptions {
            page_size: 2,
            graph: true,
            decorate: true,
            ..Default::default()
        })
        .expect("log");
    assert!(!page.commits.is_empty());
    assert!(page.commits.iter().all(|commit| commit.oid.len() >= 40));
    assert!(page
        .rows
        .iter()
        .any(|row| matches!(row, magit_lsp::git::GitLogRow::Commit(_))));
}

#[test]
fn log_scope_selection_marks_and_nodes_are_consistent() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut ordinary = LogView::new(
        path.clone(),
        GitLogOptions {
            page_size: 3,
            graph: true,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    assert_eq!(ordinary.scope().as_str(), "log");
    let ids: Vec<_> = ordinary.commits().map(str::to_owned).collect();
    assert!(!ordinary.toggle_mark(&ids[0]));

    let mut selected = LogView::new(
        path,
        GitLogOptions {
            page_size: 3,
            graph: true,
            ..Default::default()
        },
        Some(CommitSelection {
            purpose: "choose".into(),
            multi: true,
            prompt: "Select".into(),
            allow_free_text: false,
        }),
    )
    .unwrap();
    assert_eq!(selected.scope().as_str(), "log.select");
    assert!(selected.toggle_mark(&ids[0]));
    assert!(selected.toggle_mark(&ids[1]));
    let tree = selected.render(&magit_lsp::view::Ctx {
        viewport: Default::default(),
        pending: &[],
    });
    assert!(tree
        .line_nodes
        .iter()
        .all(|line| tree.nodes.iter().any(|node| node.id == line.node)));
    assert_eq!(
        selected.selected_commits(None),
        vec![ids[0].clone(), ids[1].clone()]
    );
}
