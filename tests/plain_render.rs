//! Deterministic tests for the reusable Plain document synchronization controller.
#[path = "common/fake_editor.rs"]
mod fake_editor;

use fake_editor::{FakeEditor, Profile};
use magit_lsp::docstore::Encoding;
use magit_lsp::presentation::plain::{hash_text, text_edits, DocumentState, PlainDocument};
use serde_json::{json, Value};

fn controller(encoding: Encoding, versioned: bool) -> PlainDocument {
    PlainDocument::new("file:///status", encoding, versioned)
}

#[test]
fn minimal_edits_use_utf8_utf16_and_utf32_positions_with_crlf() {
    let old = "head\r\n😀old\r\ntail";
    let new = "head\r\n😀new\r\ntail";
    for (encoding, column) in [(Encoding::Utf8, 4), (Encoding::Utf16, 2), (Encoding::Utf32, 1)] {
        let edits = text_edits(old, new, encoding);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0]["range"]["start"], json!({"line":1,"character":column}));
        assert_eq!(edits[0]["newText"], "new");
    }
    assert!(text_edits(new, new, Encoding::Utf16).is_empty());
    let newline_edit = text_edits("a\r\nb", "a\nb", Encoding::Utf16);
    assert_eq!(newline_edit[0]["range"]["start"], json!({"line":0,"character":1}));
    assert_eq!(newline_edit[0]["range"]["end"], json!({"line":1,"character":0}));
    assert_eq!(newline_edit[0]["newText"], "\n");
}

#[test]
fn render_coalesces_and_echo_before_or_after_response_reconciles_without_version_assumptions() {
    for echo_first in [true, false] {
        let uri = "file:///status";
        let mut c = controller(Encoding::Utf16, true);
        let mut editor = FakeEditor::new(Profile::Standard);
        editor.open(uri, "stale");
        c.render("authoritative\n");
        let first = c.open(1, "stale").unwrap();
        assert_eq!(c.state(), DocumentState::Applying);
        assert_eq!(first.edit["documentChanges"][0]["textDocument"]["version"], 1);
        assert_eq!(c.render("intermediate"), None);
        let second_render = c.render("latest\n");
        assert_eq!(second_render, None);
        let applied = editor.apply_edit(&first.edit);
        assert_eq!(applied["applied"], true);
        let echo_version = editor.documents[uri].version;
        if echo_first {
            assert!(c.change(echo_version as i32, editor.text(uri)).is_none());
            let next = c.response(first.id, true).unwrap();
            assert_eq!(next.edit["documentChanges"][0]["edits"][0]["newText"], "latest");
        } else {
            assert!(c.response(first.id, true).is_none());
            let next = c.change(echo_version as i32, editor.text(uri)).unwrap();
            assert_eq!(next.edit["documentChanges"][0]["edits"][0]["newText"], "latest");
        }
    }
}

#[test]
fn unversioned_changes_profile_and_stale_rejection_recompute_from_latest_snapshot() {
    let uri = "file:///status";
    let mut c = controller(Encoding::Utf8, false);
    let mut editor = FakeEditor::new(Profile::NoDocumentChanges);
    editor.open(uri, "old");
    c.render("new");
    let request = c.open(7, "old").unwrap();
    assert!(request.edit.get("changes").is_some());
    assert!(request.edit.get("documentChanges").is_none());
    editor.edit(uri, "user text");
    assert!(c.change(19, "user text").is_none());
    let retry = c.response(request.id, false).unwrap();
    assert_eq!(retry.edit["changes"][uri][0]["newText"], "new");
    assert_eq!(retry.edit["changes"][uri][0]["range"]["start"]["character"], 0);
}

#[test]
fn divergence_is_repaired_and_close_discards_late_responses_without_resurrection() {
    let mut c = controller(Encoding::Utf16, true);
    c.render("disk");
    let first = c.open(1, "old").unwrap();
    assert!(c.change(4, "formatted").is_none());
    let repair = c.response(first.id, true).unwrap();
    assert_eq!(repair.edit["documentChanges"][0]["textDocument"]["version"], 4);
    c.close();
    assert_eq!(c.state(), DocumentState::Detached);
    assert!(c.response(repair.id, true).is_none());
    assert!(c.open(22, "disk").is_none());
    assert_eq!(c.state(), DocumentState::Ready);
    assert_ne!(hash_text("disk"), hash_text("formatted"));
}

#[test]
fn bounded_snapshots_are_keyed_by_exact_text_hash() {
    let mut c = controller(Encoding::Utf16, true);
    c.open(1, "a");
    let hash = hash_text("a");
    for i in 0..10 { c.render(format!("render {i}")); }
    assert!(c.snapshot(&hash).is_none());
    let retained = hash_text("render 9");
    assert_eq!(c.snapshot(&retained).unwrap().0, "render 9");
}

#[test]
fn every_fake_editor_profile_can_be_driven_without_assuming_next_version() {
    let profiles = [Profile::Standard, Profile::DecomposedChanges, Profile::DebouncedDidChange,
        Profile::ReorderEcho, Profile::NoDocumentChanges, Profile::NoShowDocument,
        Profile::CrlfNormalise, Profile::Utf16];
    for profile in profiles {
        let mut editor = FakeEditor::new(profile);
        editor.open("file:///status", "a\r\nb");
        let mut c = controller(Encoding::Utf16, profile != Profile::NoDocumentChanges);
        c.render("a\r\nB");
        let request = c.open(1, "a\r\nb").unwrap();
        let outcome: Value = editor.apply_edit(&request.edit);
        assert_eq!(outcome["applied"], true, "profile: {profile:?}");
        let actual_version = editor.documents["file:///status"].version;
        assert!(actual_version > 1);
        assert!(c.change(actual_version as i32, editor.text("file:///status")).is_none());
        let followup = c.response(request.id, true);
        if profile == Profile::CrlfNormalise {
            assert!(followup.is_some());
        } else {
            assert!(followup.is_none());
        }
        assert_eq!(c.state(), if profile == Profile::CrlfNormalise { DocumentState::Applying } else { DocumentState::Ready });
    }
}
