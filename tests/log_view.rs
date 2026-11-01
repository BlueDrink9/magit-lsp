mod common;

use magit_lsp::git::{GitLogOptions, GitRepo};
use magit_lsp::log_view::{CommitSelection, LogView};
use magit_lsp::view::View;
use magit_lsp::{semantic_tokens, tree::Tree};
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

#[test]
fn log_token_ranges_are_byte_correct_and_encode_unicode_metadata() {
    let (fixture, _) = common::RepoFixture::new()
        .commit("subject 😀 repeated Élodie", [("file", "content")])
        .build();
    fixture.git(&[
        "commit",
        "--amend",
        "-q",
        "--no-edit",
        "--author",
        "Élodie <elodie@example.com>",
    ]);
    fixture.git(&["tag", "référence"]);

    for multi in [false, true] {
        let mut view = LogView::new(
            fixture.path().to_path_buf(),
            GitLogOptions {
                page_size: 1,
                graph: true,
                decorate: true,
                ..Default::default()
            },
            multi.then(|| CommitSelection {
                purpose: "choose".into(),
                multi: true,
                prompt: "Select".into(),
                allow_free_text: false,
            }),
        )
        .unwrap();
        if multi {
            let oid = view.commits().next().unwrap().to_owned();
            assert!(view.toggle_mark(&oid));
        }
        let tree = view.render(&magit_lsp::view::Ctx {
            viewport: Default::default(),
            pending: &[],
        });
        assert_log_tokens(&tree);
    }
}

fn assert_log_tokens(tree: &Tree) {
    let line = tree
        .lines
        .iter()
        .position(|line| line.contains("subject 😀 repeated Élodie"))
        .unwrap();
    let text = &tree.lines[line];
    let ref_token = tree
        .tokens
        .iter()
        .find(|token| token.line == line && token.token_type == "magit.ref")
        .unwrap();
    let ref_start = text.find("référence").unwrap();
    assert!(text[..ref_start].ends_with(" "));
    let expected = [
        (
            "magit.hash",
            tree.tokens
                .iter()
                .find(|token| token.line == line && token.token_type == "magit.hash")
                .unwrap()
                .start,
            8,
        ),
        ("magit.ref", ref_token.start, ref_token.length),
        (
            "magit.author",
            text.find("Élodie <elodie@example.com>").unwrap(),
            "Élodie <elodie@example.com>".len(),
        ),
    ];
    for (kind, start, length) in expected {
        assert!(
            tree.tokens.iter().any(|token| token.line == line
                && token.token_type == kind
                && token.start == start
                && token.length == length),
            "missing exact {kind} range: {:?}",
            tree.tokens
        );
        let selected = text.get(start..start + length).unwrap();
        match kind {
            "magit.hash" => assert_eq!(selected.len(), 8),
            "magit.ref" => assert!(selected.starts_with(" (") && selected.contains("référence")),
            _ => assert_eq!(selected, "Élodie <elodie@example.com>"),
        }
    }
    let date = tree
        .tokens
        .iter()
        .find(|token| token.line == line && token.token_type == "magit.date")
        .unwrap();
    assert_eq!(
        text.get(date.start..date.start + date.length).unwrap(),
        text.rsplit("  ").next().unwrap()
    );

    // The encoder's shared legend does not yet advertise log-specific types;
    // map copies to an existing legend type while exercising its real offset conversion.
    let encoder_tokens: Vec<_> = tree
        .tokens
        .iter()
        .filter(|token| token.line == line)
        .cloned()
        .map(|mut token| {
            token.token_type = "magit.file".into();
            token
        })
        .collect();
    let encoded = semantic_tokens::encode_lines(&tree.lines, &encoder_tokens);
    let mut start16 = 0;
    let mut decoded = Vec::new();
    for token in encoded {
        start16 = if token.delta_line == 0 {
            start16 + token.delta_start
        } else {
            token.delta_start
        };
        decoded.push((token.token_type as usize, start16, token.length));
    }
    for token in &encoder_tokens {
        let expected_start = text[..token.start].encode_utf16().count() as u32;
        let expected_len = text[token.start..token.start + token.length]
            .encode_utf16()
            .count() as u32;
        let index = semantic_tokens::TOKEN_TYPES
            .iter()
            .position(|kind| *kind == token.token_type)
            .unwrap();
        assert!(
            decoded.contains(&(index, expected_start, expected_len)),
            "encoded token differs for {}",
            token.token_type
        );
    }
}
