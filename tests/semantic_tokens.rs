//! `textDocument/semanticTokens/full` over real pipes: the encoded data must
//! decode back to the tree's tokens, with UTF-16 columns and modifiers.

mod common;

use common::stdio::*;
use common::*;
use magit_lsp::status_view;
use magit_lsp::tree::Tree;
use serde_json::{json, Value};
use std::time::Duration;

/// One decoded semantic token, in absolute positions.
#[derive(Debug, PartialEq, Eq)]
struct DecodedToken {
    line: u32,
    start: u32,
    length: u32,
    token_type: String,
    modifiers: Vec<String>,
}

/// Expand the flat `[deltaLine, deltaStart, length, type, modifiers]` data.
fn decode(data: &[Value], token_types: &[String], token_modifiers: &[String]) -> Vec<DecodedToken> {
    let numbers: Vec<u32> = data
        .iter()
        .map(|value| value.as_u64().expect("integer token field") as u32)
        .collect();
    assert_eq!(numbers.len() % 5, 0, "data is not groups of five");

    let mut line = 0;
    let mut start = 0;
    let mut tokens = Vec::new();
    for chunk in numbers.chunks_exact(5) {
        line += chunk[0];
        start = if chunk[0] == 0 {
            start + chunk[1]
        } else {
            chunk[1]
        };
        let modifiers = chunk[4];
        tokens.push(DecodedToken {
            line,
            start,
            length: chunk[2],
            token_type: token_types[chunk[3] as usize].clone(),
            modifiers: token_modifiers
                .iter()
                .enumerate()
                .filter(|(index, _)| modifiers & (1 << index) != 0)
                .map(|(_, modifier)| modifier.clone())
                .collect(),
        });
    }
    tokens
}

fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

/// The tree's tokens as the client should see them, computed independently.
fn expected_tokens(
    tree: &Tree,
    token_types: &[String],
    token_modifiers: &[String],
) -> Vec<DecodedToken> {
    let mut tokens: Vec<_> = tree.tokens.iter().collect();
    tokens.sort_by_key(|token| (token.line, token.start));
    tokens
        .into_iter()
        .filter_map(|token| {
            let text = tree.lines.get(token.line)?;
            let start = utf16_len(&text[..token.start]);
            let length = utf16_len(&text[token.start..token.start + token.length]);
            let index = token_types
                .iter()
                .position(|candidate| *candidate == token.token_type)?;
            let modifiers = token_modifiers
                .iter()
                .enumerate()
                .filter(|(_, modifier)| token.modifiers.contains(modifier))
                .map(|(_, modifier)| modifier.clone())
                .collect();
            Some(DecodedToken {
                line: token.line as u32,
                start,
                length,
                token_type: token_types[index].clone(),
                modifiers,
            })
        })
        .collect()
}

fn strings(value: &Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is not an array: {value}"))
        .iter()
        .map(|entry| entry.as_str().expect("legend entry").to_string())
        .collect()
}

/// A repo with a unicode tracked file (unstaged change) and a unicode staged
/// file, so tokens cover both sections and both modifier states.
fn unicode_fixture() -> (RepoFixture, magit_lsp::git::GitRepo) {
    RepoFixture::new()
        .commit("init", [("üñï.txt", TEN_LINES)])
        .staged_file("staged ünï.txt", "a\nb\n")
        .modify("üñï.txt", standard_changes())
        .build()
}

#[test]
fn capabilities_advertise_the_legend_and_full_tokens() {
    let mut server = TestServer::spawn(&[(magit_lsp::logging::LEVEL_ENV, "warn")]);
    let initialize = server.initialize(json!({}));
    let provider = &initialize["result"]["capabilities"]["semanticTokensProvider"];
    assert_eq!(provider["full"], json!(true), "{provider}");
    let token_types = strings(&provider["legend"], "tokenTypes");
    for expected in [
        "magit.section",
        "magit.file",
        "magit.hunk.header",
        "magit.diff.added",
        "magit.diff.removed",
    ] {
        assert!(
            token_types.iter().any(|candidate| candidate == expected),
            "legend is missing {expected}: {token_types:?}"
        );
    }
    assert_eq!(
        strings(&provider["legend"], "tokenModifiers"),
        vec![
            "staged".to_string(),
            "selected".to_string(),
            "planned".to_string(),
            "disabled".to_string(),
            "bold".to_string()
        ]
    );
    server.send(json!({"jsonrpc":"2.0","id":99,"method":"shutdown","params":null}));
    server.response_with_id(99);
    server.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn semantic_tokens_decode_back_to_the_tree_tokens() {
    let (tmp, repo) = unicode_fixture();
    let mut server = TestServer::spawn(&[(magit_lsp::logging::LEVEL_ENV, "warn")]);
    let initialize = server.initialize(json!({}));
    let legend = &initialize["result"]["capabilities"]["semanticTokensProvider"]["legend"];
    let token_types = strings(legend, "tokenTypes");
    let token_modifiers = strings(legend, "tokenModifiers");

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "view/open",
        "params": {
            "kind": "status",
            "args": {"repo_path": tmp.path().to_string_lossy()},
            "viewport": {"cols": 80, "rows": 24},
        },
    }));
    let opened = server.response_with_id(2);
    let uri = opened["result"]["uri"]
        .as_str()
        .expect("view uri")
        .to_string();

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/semanticTokens/full",
        "params": {"textDocument": {"uri": uri}},
    }));
    let response = server.response_with_id(3);
    let data = response["result"]["data"]
        .as_array()
        .unwrap_or_else(|| panic!("no token data: {response}"));
    let decoded = decode(data, &token_types, &token_modifiers);

    let tree = status_view::build(&repo.status().expect("status"));
    let expected = expected_tokens(&tree, &token_types, &token_modifiers);
    assert_eq!(decoded, expected);

    // A unicode path is longer in bytes than in UTF-16 code units, so the
    // decoded length proves the conversion happened.
    let path_line = tree
        .lines
        .iter()
        .position(|line| line == "  M üñï.txt")
        .expect("unstaged unicode path line");
    let path = decoded
        .iter()
        .find(|token| token.line == path_line as u32 && token.token_type == "magit.file")
        .expect("file path token");
    assert_eq!(path.start, 4);
    assert_eq!(path.length, utf16_len("üñï.txt"));
    assert!(
        path.length < "üñï.txt".len() as u32,
        "the path must be measured in UTF-16 code units"
    );

    let staged = decoded
        .iter()
        .find(|token| token.token_type == "magit.file.added")
        .expect("staged status token");
    assert_eq!(staged.modifiers, vec!["staged".to_string()]);

    // LSP requires sorted, non-overlapping tokens.
    for pair in decoded.windows(2) {
        let (previous, next) = (&pair[0], &pair[1]);
        assert!(
            previous.line < next.line
                || (previous.line == next.line && previous.start + previous.length <= next.start),
            "tokens overlap or are out of order: {previous:?} then {next:?}"
        );
    }

    server.send(json!({"jsonrpc":"2.0","id":99,"method":"shutdown","params":null}));
    server.response_with_id(99);
    server.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn an_unknown_uri_returns_no_tokens() {
    let mut server = TestServer::spawn(&[(magit_lsp::logging::LEVEL_ENV, "warn")]);
    server.initialize(json!({}));
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/semanticTokens/full",
        "params": {"textDocument": {"uri": "file:///nope/status"}},
    }));
    let response = server.response_with_id(2);
    assert_eq!(response["result"]["data"], json!([]), "{response}");
    server.send(json!({"jsonrpc":"2.0","id":99,"method":"shutdown","params":null}));
    server.response_with_id(99);
    server.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));
    assert!(server.finish(Duration::from_secs(10)).success());
}
