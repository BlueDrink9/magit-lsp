//! Reconstructed Plain-key edit-shape profiles; this module does not implement policy.

use serde_json::Value;

/// A named ordered sequence of recorded or reconstructed didChange notifications.
pub struct Profile {
    /// Stable fixture name.
    pub name: &'static str,
    /// Expected test-only shape classification.
    pub shape: Shape,
    /// Newline-delimited exact LSP notification records.
    pub trace: &'static str,
}

/// The three candidate shapes, plus all other edit shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// One insertion with an empty ranged edit.
    SingleInsert,
    /// One ranged replacement inserting one character.
    RangedOneCharacterReplace,
    /// Delete then insert one character at the same start in consecutive notifications.
    DeleteThenInsertPair,
    /// Any shape not matching the three descriptions above.
    Other,
}

/// Profiles used by the plain harness, in stable order.
pub const PROFILES: &[Profile] = &[
    Profile {
        name: "selection-replace-pair",
        shape: Shape::DeleteThenInsertPair,
        trace: include_str!(
            "../../../../tests/fixtures/lsp_traces/plain_keys/selection-replace-pair.jsonl"
        ),
    },
    Profile {
        name: "selection-replace-single",
        shape: Shape::RangedOneCharacterReplace,
        trace: include_str!(
            "../../../../tests/fixtures/lsp_traces/plain_keys/selection-replace-single.jsonl"
        ),
    },
    Profile {
        name: "rangeless-full-text",
        shape: Shape::Other,
        trace: include_str!(
            "../../../../tests/fixtures/lsp_traces/plain_keys/rangeless-full-text.jsonl"
        ),
    },
    Profile {
        name: "batched-changes",
        shape: Shape::Other,
        trace: include_str!(
            "../../../../tests/fixtures/lsp_traces/plain_keys/batched-changes.jsonl"
        ),
    },
    Profile {
        name: "paste-multichar",
        shape: Shape::Other,
        trace: include_str!(
            "../../../../tests/fixtures/lsp_traces/plain_keys/paste-multichar.jsonl"
        ),
    },
    Profile {
        name: "undo-after-repair",
        shape: Shape::Other,
        trace: include_str!(
            "../../../../tests/fixtures/lsp_traces/plain_keys/undo-after-repair.jsonl"
        ),
    },
];

/// Parse a fixture's exact ordered didChange notifications.
pub fn notifications(trace: &str) -> Vec<Value> {
    trace
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid JSONL notification"))
        .collect()
}

/// Classify only the structural shape of a recorded notification sequence.
pub fn classify(messages: &[Value]) -> Shape {
    if messages.len() == 1 {
        let changes = messages[0]["params"]["contentChanges"]
            .as_array()
            .expect("contentChanges array");
        if changes.len() == 1 {
            let change = &changes[0];
            if change.get("range").is_none() {
                return Shape::Other;
            }
            let start = &change["range"]["start"];
            let end = &change["range"]["end"];
            let length = change["text"].as_str().unwrap_or("").chars().count();
            if start == end && length == 1 {
                return Shape::SingleInsert;
            }
            if start != end && length == 1 {
                return Shape::RangedOneCharacterReplace;
            }
        }
        return Shape::Other;
    }
    if messages.len() == 2 {
        let a = &messages[0]["params"]["contentChanges"][0];
        let b = &messages[1]["params"]["contentChanges"][0];
        let start = &a["range"]["start"];
        if a["range"].is_object()
            && b["range"].is_object()
            && a["text"] == ""
            && a["range"]["start"] != a["range"]["end"]
            && b["range"]["start"] == *start
            && b["range"]["end"] == *start
            && b["text"]
                .as_str()
                .is_some_and(|text| text.chars().count() == 1)
            && messages[1]["params"]["textDocument"]["version"].as_i64()
                == messages[0]["params"]["textDocument"]["version"]
                    .as_i64()
                    .map(|v| v + 1)
        {
            return Shape::DeleteThenInsertPair;
        }
    }
    Shape::Other
}
