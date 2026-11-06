//! Document prompts: tracked one-line fields, stable row ids and exact layout.

use magit_lsp::presentation::prompt::{PromptDocument, MAX_FIELD_BYTES};
use magit_lsp::protocol::ViewPromptParams;
use magit_lsp::tree::{NodeKind, Tree};
use serde_json::{json, Value};

fn params(value: Value) -> ViewPromptParams {
    serde_json::from_value(value).expect("valid prompt params")
}

fn document(value: Value) -> PromptDocument {
    match PromptDocument::new(params(value)) {
        Ok(document) => document,
        Err(error) => panic!("prompt was refused: {error}"),
    }
}

fn refusal(value: Value) -> String {
    match PromptDocument::new(params(value)) {
        Ok(_) => panic!("prompt was accepted"),
        Err(error) => error,
    }
}

fn assert_node(tree: &Tree, id: &str, kind: NodeKind, label: &str, line: usize) {
    let node = tree.node(id).unwrap_or_else(|| panic!("missing node {id}"));
    assert_eq!(node.kind, kind, "{id}");
    assert_eq!(node.label, label, "{id}");
    assert_eq!(node.start_line, line, "{id}");
    assert_eq!(node.end_line, line, "{id}");
    let entry = tree.line_nodes.get(line).unwrap_or_else(|| panic!("no line {line}"));
    assert_eq!(entry.node, id, "line {line}");
    assert_eq!(entry.kind, kind, "line {line}");
}

#[test]
fn confirm_renders_title_detail_blank_and_custom_yes_no_rows() {
    let document = document(json!({
        "promptId": "del",
        "kind": "confirm",
        "title": "Delete branch feature/x?",
        "detail": "This cannot be undone.\nIt is local only.",
        "yesLabel": "Delete",
        "noLabel": "Keep"
    }));
    let tree = document.render(80);

    assert_eq!(
        tree.lines,
        vec![
            "Delete branch feature/x?",
            "This cannot be undone.",
            "It is local only.",
            "",
            "Delete",
            "Keep",
        ]
    );
    assert_eq!(tree.nodes.len(), 6);
    assert_node(&tree, "prompt:del:title", NodeKind::Header, "Delete branch feature/x?", 0);
    assert_node(
        &tree,
        "prompt:del:detail:0",
        NodeKind::Message,
        "This cannot be undone.",
        1,
    );
    assert_node(
        &tree,
        "prompt:del:detail:1",
        NodeKind::Message,
        "It is local only.",
        2,
    );
    assert_node(&tree, "prompt:del:blank", NodeKind::Blank, "", 3);
    assert_node(&tree, "prompt:del:yes", NodeKind::Line, "Delete", 4);
    assert_node(&tree, "prompt:del:no", NodeKind::Line, "Keep", 5);
    assert_eq!(document.row_ids(), vec!["prompt:del:yes", "prompt:del:no"]);
    assert!(document.field_ids().is_empty());
}

#[test]
fn text_renders_one_prefilled_field_and_submit_cancel_rows() {
    let document = document(json!({
        "promptId": "rev",
        "kind": "text",
        "title": "Revision",
        "detail": "Commit to use.",
        "default": "HEAD~1"
    }));
    let tree = document.render(80);

    assert_eq!(
        tree.lines,
        vec!["Revision", "Commit to use.", "", "HEAD~1", "Submit", "Cancel"]
    );
    assert_node(&tree, "prompt:rev:title", NodeKind::Header, "Revision", 0);
    assert_node(&tree, "prompt:rev:detail:0", NodeKind::Message, "Commit to use.", 1);
    assert_node(&tree, "prompt:rev:blank", NodeKind::Blank, "", 2);
    assert_node(&tree, "prompt:rev:field", NodeKind::Field, "Value", 3);
    assert_node(&tree, "prompt:rev:submit", NodeKind::Line, "Submit", 4);
    assert_node(&tree, "prompt:rev:cancel", NodeKind::Line, "Cancel", 5);

    let field = tree.node("prompt:rev:field").expect("the field node");
    assert_eq!(field.start_line, field.end_line);
    assert_eq!(tree.lines[field.start_line], "HEAD~1");
    assert_eq!(document.field_ids(), vec!["prompt:rev:field"]);
    assert_eq!(document.field_label("prompt:rev:field"), Some("Value"));
    assert_eq!(document.field_text("prompt:rev:field"), Some("HEAD~1"));
    assert_eq!(document.row_ids(), vec!["prompt:rev:submit", "prompt:rev:cancel"]);
}

#[test]
fn text_without_default_renders_an_empty_field_line() {
    let document = document(json!({
        "promptId": "rev",
        "kind": "text",
        "title": "Revision"
    }));
    let tree = document.render(80);

    assert_eq!(tree.lines, vec!["Revision", "", "", "Submit", "Cancel"]);
    assert_eq!(document.field_text("prompt:rev:field"), Some(""));
    assert_eq!(tree.lines[2], "");
}

#[test]
fn choice_renders_item_rows_a_default_marker_and_a_custom_field_with_the_label() {
    let document = document(json!({
        "promptId": "pick",
        "kind": "choice",
        "title": "Pick a branch",
        "items": [
            {"id": "main", "label": "Main"},
            {"id": "feature/x", "label": "Feature x"}
        ],
        "default": "feature/x",
        "allowCustom": true
    }));
    let tree = document.render(80);

    assert_eq!(
        tree.lines,
        vec!["Pick a branch", "", "Main", "> Feature x", "Feature x"]
    );
    assert_node(&tree, "prompt:pick:title", NodeKind::Header, "Pick a branch", 0);
    assert_node(&tree, "prompt:pick:blank", NodeKind::Blank, "", 1);
    assert_node(&tree, "prompt:pick:item:main", NodeKind::Line, "Main", 2);
    assert_node(&tree, "prompt:pick:item:feature/x", NodeKind::Line, "Feature x", 3);
    assert_node(&tree, "prompt:pick:custom", NodeKind::Field, "Custom value", 4);

    assert_eq!(
        tree.node("prompt:pick:item:feature/x").unwrap().marker,
        Some('>')
    );
    assert_eq!(tree.node("prompt:pick:item:main").unwrap().marker, None);
    assert_eq!(
        document.row_ids(),
        vec!["prompt:pick:item:main", "prompt:pick:item:feature/x"]
    );
    assert_eq!(document.field_ids(), vec!["prompt:pick:custom"]);
    assert_eq!(
        document.field_text("prompt:pick:custom"),
        Some("Feature x"),
        "the custom field is prefilled with the label, never the id"
    );
    assert_ne!(document.field_text("prompt:pick:custom"), Some("feature/x"));
}

#[test]
fn multi_choice_renders_toggles_preselection_and_a_done_row() {
    let mut document = document(json!({
        "promptId": "tags",
        "kind": "multiChoice",
        "title": "Pick tags",
        "items": [
            {"id": "a", "label": "Alpha"},
            {"id": "b", "label": "Beta"}
        ],
        "preselected": ["a"]
    }));
    let tree = document.render(80);

    assert_eq!(tree.lines, vec!["Pick tags", "", "[x] Alpha", "[ ] Beta", "Done"]);
    assert_node(&tree, "prompt:tags:item:a", NodeKind::Line, "Alpha", 2);
    assert_node(&tree, "prompt:tags:item:b", NodeKind::Line, "Beta", 3);
    assert_node(&tree, "prompt:tags:done", NodeKind::Line, "Done", 4);
    assert_eq!(tree.node("prompt:tags:item:a").unwrap().marker, Some('x'));
    assert_eq!(tree.node("prompt:tags:item:b").unwrap().marker, Some(' '));
    assert_eq!(document.row_ids(), vec!["prompt:tags:item:a", "prompt:tags:item:b", "prompt:tags:done"]);
    assert_eq!(document.selected_ids(), vec!["a"]);

    assert!(document.toggle_item("b").unwrap());
    assert!(document.is_selected("b"));
    assert_eq!(tree.lines[3], "[ ] Beta", "toggling changes state without re-rendering");
    let rerendered = document.render(80);
    assert_eq!(rerendered.lines[3], "[x] Beta");

    assert!(!document.toggle_item("a").unwrap());
    assert_eq!(document.selected_ids(), vec!["b"]);
}

#[test]
fn rows_render_even_when_the_item_list_is_empty() {
    let multi = document(json!({
        "promptId": "tags",
        "kind": "multiChoice",
        "title": "Pick tags",
        "items": []
    }));
    assert_eq!(multi.render(80).lines, vec!["Pick tags", "", "Done"]);
    assert_eq!(multi.row_ids(), vec!["prompt:tags:done"]);

    let confirm = document(json!({
        "promptId": "ok",
        "kind": "confirm",
        "title": "Continue?"
    }));
    assert_eq!(confirm.render(80).lines, vec!["Continue?", "", "Yes", "No"]);

    let text = document(json!({
        "promptId": "rev",
        "kind": "text",
        "title": "Revision"
    }));
    assert_eq!(text.render(80).lines, vec!["Revision", "", "", "Submit", "Cancel"]);
}

#[test]
fn secret_prompts_are_refused() {
    let error = refusal(json!({
        "promptId": "pass",
        "kind": "text",
        "title": "Password",
        "secret": true
    }));
    assert!(error.contains("native-only"), "{error}");
}

#[test]
fn kind_specific_shapes_are_validated() {
    let confirm_with_items = refusal(json!({
        "promptId": "ok",
        "kind": "confirm",
        "title": "Continue?",
        "items": [{"id": "a", "label": "A"}]
    }));
    assert!(confirm_with_items.contains("cannot carry items"), "{confirm_with_items}");

    let text_with_items = refusal(json!({
        "promptId": "rev",
        "kind": "text",
        "title": "Revision",
        "items": [{"id": "a", "label": "A"}]
    }));
    assert!(text_with_items.contains("cannot carry items"), "{text_with_items}");

    let choice_without_items = refusal(json!({
        "promptId": "pick",
        "kind": "choice",
        "title": "Pick"
    }));
    assert!(choice_without_items.contains("require items"), "{choice_without_items}");

    let multi_without_items = refusal(json!({
        "promptId": "tags",
        "kind": "multiChoice",
        "title": "Pick"
    }));
    assert!(multi_without_items.contains("require items"), "{multi_without_items}");

    let unknown_preselection = refusal(json!({
        "promptId": "tags",
        "kind": "multiChoice",
        "title": "Pick",
        "items": [{"id": "a", "label": "Alpha"}],
        "preselected": ["missing"]
    }));
    assert!(
        unknown_preselection.contains("not in the prompt items"),
        "{unknown_preselection}"
    );
}

#[test]
fn field_text_is_exact_single_line_and_bounded() {
    let mut document = document(json!({
        "promptId": "rev",
        "kind": "text",
        "title": "Revision",
        "default": "a value"
    }));
    let id = "prompt:rev:field";
    let tree = document.render(80);
    let field = tree.node(id).expect("the field node");
    assert_eq!(field.start_line, field.end_line);
    assert_eq!(tree.lines[field.start_line], document.field_text(id).unwrap());

    let at_limit = "x".repeat(MAX_FIELD_BYTES);
    document.set_field_text(id, &at_limit).unwrap();
    assert_eq!(document.field_text(id), Some(at_limit.as_str()));
    let render = document.render(80);
    let line = render.node(id).unwrap().start_line;
    assert_eq!(render.lines[line], at_limit);

    let over_limit = "x".repeat(MAX_FIELD_BYTES + 1);
    let error = document.set_field_text(id, &over_limit).unwrap_err();
    assert!(error.contains("limit"), "{error}");
    assert_eq!(document.field_text(id), Some(at_limit.as_str()), "no truncation");

    let error = document.set_field_text(id, "two\nlines").unwrap_err();
    assert!(error.contains("single line"), "{error}");

    let error = refusal(json!({
        "promptId": "rev",
        "kind": "text",
        "title": "Revision",
        "default": over_limit
    }));
    assert!(error.contains("limit"), "{error}");

    let error = refusal(json!({
        "promptId": "rev",
        "kind": "text",
        "title": "Revision",
        "default": "two\nlines"
    }));
    assert!(error.contains("single line"), "{error}");
}

#[test]
fn long_detail_lines_are_clipped_without_splitting_rows() {
    let document = document(json!({
        "promptId": "ok",
        "kind": "confirm",
        "title": "Continue?",
        "detail": "abcdefghijklmnop"
    }));
    let tree = document.render(5);
    assert_eq!(tree.lines, vec!["Conti", "abcde", "", "Yes", "No"]);
    assert_node(&tree, "prompt:ok:yes", NodeKind::Line, "Yes", 3);
    assert_node(&tree, "prompt:ok:no", NodeKind::Line, "No", 4);
}

#[test]
fn field_kind_serializes_as_field() {
    assert_eq!(
        serde_json::to_value(NodeKind::Field).unwrap(),
        json!("field")
    );
}
