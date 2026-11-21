//! The tree's semantic tokens in LSP wire form.
//!
//! The tree stores byte offsets into rendered lines, while LSP counts columns
//! in UTF-16 code units. This module owns that conversion plus the delta
//! encoding, so the rest of the server never touches either.

use crate::docstore::Encoding;
use crate::tree::{Token, Tree};
use lsp_types::{SemanticToken, SemanticTokens, SemanticTokensLegend};

/// Authored role metadata consumed by the server and downstream clients.
#[derive(serde::Deserialize)]
pub struct SemanticRole {
    /// LSP semantic token type.
    pub name: &'static str,
    /// Suggested Neovim highlight group.
    pub group: &'static str,
    /// VS Code TextMate scope.
    pub scope: &'static str,
}

/// Shared role definitions; malformed project data is an invariant failure.
pub fn roles() -> &'static [SemanticRole] {
    static ROLES: std::sync::OnceLock<Vec<SemanticRole>> = std::sync::OnceLock::new();
    ROLES.get_or_init(|| {
        serde_json::from_str(include_str!("semantic_roles.json"))
            .expect("valid semantic role table")
    })
}

/// Token modifiers, in legend bit order.
/// keep-for: TASKS §3.15; selected is produced by log selection, while planned/disabled and bold retain recorded presentation uses (R-5.3).
pub const TOKEN_MODIFIERS: &[&str] = &["staged", "selected", "planned", "disabled", "bold"];

/// The legend advertised in `semanticTokensProvider`.
pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: roles().iter().map(|role| role.name.into()).collect(),
        token_modifiers: TOKEN_MODIFIERS.iter().copied().map(Into::into).collect(),
    }
}

/// The server-suggested theme group for each token type, keyed by type.
pub fn token_groups() -> serde_json::Value {
    let mut groups = serde_json::Map::new();
    for role in roles() {
        groups.insert(role.name.to_string(), role.group.into());
    }
    serde_json::Value::Object(groups)
}

/// Encode every token of `tree` as a full-document semantic-token response.
pub fn encode(tree: &Tree) -> SemanticTokens {
    encode_with_encoding(tree, Encoding::Utf16)
}

/// Encode all tokens using the negotiated position unit.
pub fn encode_with_encoding(tree: &Tree, encoding: Encoding) -> SemanticTokens {
    SemanticTokens {
        result_id: None,
        data: encode_lines_with_encoding(&tree.lines, &tree.tokens, encoding),
    }
}

/// Delta-encode `tokens` against the rendered `lines`.
///
/// Tokens are sorted by position and any token overlapping the previous one on
/// the same line is dropped, because LSP requires sorted, non-overlapping
/// ranges. Types outside the legend cannot be encoded and are skipped.
pub fn encode_lines(lines: &[String], tokens: &[Token]) -> Vec<SemanticToken> {
    encode_lines_with_encoding(lines, tokens, Encoding::Utf16)
}

/// Delta-encode tokens using the negotiated position unit.
pub fn encode_lines_with_encoding(
    lines: &[String],
    tokens: &[Token],
    encoding: Encoding,
) -> Vec<SemanticToken> {
    let mut ordered: Vec<&Token> = tokens.iter().collect();
    ordered.sort_by_key(|token| (token.line, token.start));

    let mut data = Vec::new();
    let mut previous: Option<(u32, u32)> = None;
    let mut previous_end: Option<(usize, usize)> = None;

    for token in ordered {
        let Some(text) = lines.get(token.line) else {
            continue;
        };
        let Some(token_type) = type_index(&token.token_type) else {
            continue;
        };

        let start_byte = char_boundary(text, token.start);
        let end_byte = char_boundary(text, token.start.saturating_add(token.length));
        if end_byte <= start_byte {
            continue;
        }
        if let Some((line, end)) = previous_end {
            if line == token.line && start_byte < end {
                continue;
            }
        }

        let start = column_len(&text[..start_byte], encoding);
        let length = column_len(&text[start_byte..end_byte], encoding);
        if length == 0 {
            continue;
        }

        let (delta_line, delta_start) = match previous {
            Some((line, previous_start)) if line as usize == token.line => {
                (0, start - previous_start)
            }
            Some((line, _)) => (token.line as u32 - line, start),
            None => (token.line as u32, start),
        };
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: modifier_bitset(&token.modifiers),
        });

        previous = Some((token.line as u32, start));
        previous_end = Some((token.line, end_byte));
    }

    data
}

/// Index of `token_type` in the legend.
fn type_index(token_type: &str) -> Option<u32> {
    roles()
        .iter()
        .position(|candidate| candidate.name == token_type)
        .map(|index| index as u32)
}

/// Bitset of the known `modifiers`, in legend order.
fn modifier_bitset(modifiers: &[String]) -> u32 {
    modifiers
        .iter()
        .filter_map(|modifier| {
            TOKEN_MODIFIERS
                .iter()
                .position(|candidate| *candidate == modifier)
        })
        .fold(0, |bits, index| bits | (1 << index))
}

/// Length of `text` in UTF-16 code units, the unit LSP columns use.
fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

fn column_len(text: &str, encoding: Encoding) -> u32 {
    match encoding {
        Encoding::Utf8 => text.len() as u32,
        Encoding::Utf16 => utf16_len(text),
        Encoding::Utf32 => text.chars().count() as u32,
    }
}

/// `index` moved back to the nearest UTF-8 character boundary within `text`.
fn char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(line: usize, start: usize, length: usize, token_type: &str) -> Token {
        Token {
            line,
            start,
            length,
            token_type: token_type.to_string(),
            modifiers: Vec::new(),
        }
    }

    #[test]
    fn columns_and_lengths_are_utf16_code_units() {
        // "é" is two UTF-8 bytes but one UTF-16 code unit; "😀" is four bytes
        // and two code units.
        let lines = vec!["é😀 x".to_string()];
        let tokens = vec![token(0, 0, "é😀".len(), "magit.file")];
        let data = encode_lines(&lines, &tokens);
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].length, 3);
    }

    #[test]
    fn overlapping_and_unknown_tokens_are_dropped() {
        let lines = vec!["abcdef".to_string()];
        let tokens = vec![
            token(0, 2, 3, "magit.file"),
            token(0, 3, 2, "magit.file.added"),
            token(0, 0, 1, "magit.unknown"),
        ];
        let data = encode_lines(&lines, &tokens);
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].delta_start, 2);
        assert_eq!(data[0].length, 3);
    }

    #[test]
    fn tokens_are_delta_encoded_in_document_order() {
        let lines = vec!["aaaa".to_string(), "bbbb".to_string()];
        let tokens = vec![
            token(1, 2, 2, "magit.count"),
            token(0, 0, 2, "magit.section"),
        ];
        let data = encode_lines(&lines, &tokens);
        assert_eq!(
            data.iter()
                .map(|token| (token.delta_line, token.delta_start))
                .collect::<Vec<_>>(),
            vec![(0, 0), (1, 2)]
        );
    }

    #[test]
    fn modifiers_become_legend_bits() {
        let lines = vec!["abc".to_string()];
        let mut staged = token(0, 0, 3, "magit.file");
        staged.modifiers = vec!["staged".to_string(), "unknown".to_string()];
        let data = encode_lines(&lines, &[staged]);
        assert_eq!(data[0].token_modifiers_bitset, 1);
    }

    #[test]
    fn every_token_type_has_a_suggested_group() {
        let groups = token_groups();
        for role in roles() {
            assert!(
                groups
                    .get(role.name)
                    .and_then(serde_json::Value::as_str)
                    .is_some(),
                "{} has no suggested group",
                role.name
            );
        }
    }
}
