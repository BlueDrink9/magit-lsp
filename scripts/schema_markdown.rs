//! Generated Markdown payload tables for the client specification.
//!
//! The tables are derived from the committed directional schemas and the
//! hand-maintained method catalog, so field names, JSON types and
//! required/optional status cannot drift from the wire contract.

use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use super::{Direction, MethodKind, Payload};

/// Repository-relative path of the client specification document.
pub const DOC_PATH: &str = "docs/CLIENT_SPEC.md";
/// Marker opening the generated payload-table block.
pub const BLOCK_BEGIN: &str = "<!-- BEGIN GENERATED PAYLOAD TABLES -->";
/// Marker closing the generated payload-table block.
pub const BLOCK_END: &str = "<!-- END GENERATED PAYLOAD TABLES -->";
/// Marker opening the generated worked-exchange block.
pub const EXCHANGES_BLOCK_BEGIN: &str = "<!-- BEGIN GENERATED WORKED EXCHANGES -->";
/// Marker closing the generated worked-exchange block.
pub const EXCHANGES_BLOCK_END: &str = "<!-- END GENERATED WORKED EXCHANGES -->";
/// Repository-relative directory holding the recorded exchange files.
pub const EXAMPLES_DIR: &str = "docs/examples";

/// Render the complete marker-delimited payload-table block.
///
/// Method rows come from the catalog; payload tables come from every
/// directional schema root. Catalog and shared-list data are not payloads, so
/// they are deliberately excluded.
pub fn block() -> Result<String, String> {
    let mut out = String::new();
    out.push_str(BLOCK_BEGIN);
    out.push('\n');
    out.push_str("#### Protocol methods\n\n");
    out.push_str("| Method | Kind | Direction | Params | Result |\n");
    out.push_str("|---|---|---|---|---|\n");
    for entry in super::methods() {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            entry.method,
            match entry.kind {
                MethodKind::Request => "request",
                MethodKind::Notification => "notification",
            },
            match entry.direction {
                Direction::Input => "clientToServer",
                Direction::Output => "serverToClient",
            },
            payload_label(&entry.params),
            entry
                .result
                .as_ref()
                .map(payload_label)
                .unwrap_or_else(|| "—".to_string()),
        ));
    }
    out.push_str("\n#### Payload schemas\n");
    let mut artifacts: Vec<_> = super::artifacts()
        .into_iter()
        .filter(|artifact| artifact.file != "methods.json" && artifact.file != "shared-lists.json")
        .collect();
    artifacts.sort_by_key(payload_title);
    for artifact in &artifacts {
        out.push('\n');
        out.push_str(&render_payload(
            &payload_title(artifact),
            artifact.file,
            &artifact.schema,
        )?);
    }
    out.push('\n');
    out.push_str(BLOCK_END);
    Ok(out)
}

/// Render the complete marker-delimited worked-exchange block.
///
/// One subsection per JSONL example file in sorted file order; each line's
/// recorded `message` is rendered as compact JSON without changing any value.
pub fn worked_exchanges_block() -> Result<String, String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(EXAMPLES_DIR);
    let mut files = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|error| format!("{}: {error}", dir.display()))? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("jsonl") {
            files.push(path);
        }
    }
    files.sort();
    if files.is_empty() {
        return Err(format!("{}: no JSONL examples found", dir.display()));
    }
    let mut out = String::new();
    out.push_str(EXCHANGES_BLOCK_BEGIN);
    out.push('\n');
    out.push_str("#### Worked exchanges\n\n");
    out.push_str("Recorded end-to-end exchanges from the example files, one subsection per file.\n");
    for path in &files {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("{}: invalid file name", path.display()))?;
        let text = fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let mut lines = Vec::new();
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            lines.push(
                serde_json::from_str::<Value>(line).map_err(|error| {
                    format!("{}:{}: {error}", path.display(), index + 1)
                })?,
            );
        }
        out.push('\n');
        out.push_str(&format!("##### `{name}`\n\n"));
        out.push_str(&provenance(&lines));
        out.push_str("\n\n```jsonl\n");
        for line in &lines {
            let message = line
                .get("message")
                .ok_or_else(|| format!("{}: frame lacks `message`", path.display()))?;
            out.push_str(
                &serde_json::to_string(message)
                    .map_err(|error| format!("{}: {error}", path.display()))?,
            );
            out.push('\n');
        }
        out.push_str("```\n");
    }
    out.push('\n');
    out.push_str(EXCHANGES_BLOCK_END);
    Ok(out)
}

/// One-sentence provenance derived from a file's recorded `source` labels.
fn provenance(lines: &[Value]) -> String {
    let recorded = lines.iter().all(|line| {
        line.get("source").and_then(Value::as_str) == Some("recorded")
    });
    if recorded {
        "Frames recorded from a real `TestServer` run against a temporary repository; temporary paths are normalized to `/repo`.".to_string()
    } else {
        "Frames are hand-written illustrative examples, not recorded server traffic.".to_string()
    }
}

/// Apply the freshly generated blocks to a committed document.
///
/// Each existing marker pair is replaced in place. Otherwise its block is
/// appended, which is how a new or block-less document becomes current.
pub fn apply(committed: &str) -> Result<String, String> {
    let tables = replace_block(committed, BLOCK_BEGIN, BLOCK_END, &block()?);
    Ok(replace_block(
        &tables,
        EXCHANGES_BLOCK_BEGIN,
        EXCHANGES_BLOCK_END,
        &worked_exchanges_block()?,
    ))
}

/// Replace one marker-delimited block, appending it when the markers are absent.
fn replace_block(committed: &str, begin: &str, end: &str, block: &str) -> String {
    if let (Some(start), Some(end_index)) = (committed.find(begin), committed.find(end)) {
        if start < end_index {
            let mut out = String::new();
            out.push_str(&committed[..start]);
            out.push_str(block);
            out.push_str(&committed[end_index + end.len()..]);
            return out;
        }
    }
    let mut out = committed.to_string();
    if out.is_empty() {
        out.push_str(block);
        out.push('\n');
    } else {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.ends_with("\n\n") {
            out.push('\n');
        }
        out.push_str(block);
        out.push('\n');
    }
    out
}

/// Write the client specification with a fresh generated block.
pub fn write(root: &Path) -> io::Result<()> {
    let path = root.join(DOC_PATH);
    let committed = fs::read_to_string(&path).unwrap_or_default();
    let fresh = apply(&committed).map_err(invalid_data)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, fresh)
}

/// Whether the committed document lacks or disagrees with the generated block.
pub fn stale(root: &Path) -> io::Result<bool> {
    let path = root.join(DOC_PATH);
    let committed = fs::read_to_string(&path).unwrap_or_default();
    let fresh = apply(&committed).map_err(invalid_data)?;
    Ok(fresh != committed)
}

/// Convert a rendering error into an I/O error for the shared drift API.
fn invalid_data(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Render one payload reference as it appears in the method table.
fn payload_label(payload: &Payload) -> String {
    match payload {
        Payload::Schema { rust_type, file } => format!("`{rust_type}` (`{file}`)"),
        Payload::UnconstrainedJson => "unconstrained JSON".to_string(),
    }
}

/// Root title used for a payload heading and for stable ordering.
fn payload_title(artifact: &super::Artifact) -> String {
    artifact
        .schema
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(artifact.file)
        .to_string()
}

/// Render one directional schema root as a field table.
fn render_payload(title: &str, file: &str, schema: &Value) -> Result<String, String> {
    let mut out = format!("##### `{title}` (`{file}`)\n\n");
    if let Some(description) = schema.get("description").and_then(Value::as_str) {
        out.push_str(&flatten(description));
        out.push_str("\n\n");
    }
    let defs = schema.get("$defs").and_then(Value::as_object);
    let mut variants = Vec::new();
    if let Some(values) = ["oneOf", "anyOf"]
        .into_iter()
        .find_map(|key| schema.get(key).and_then(Value::as_array))
    {
        for (index, variant) in values.iter().enumerate() {
            let map = variant
                .as_object()
                .ok_or_else(|| format!("{title}: union variants must be objects"))?;
            variants.push(format!(
                "{}\n\n{}",
                variant_label(map, index),
                field_table(map, defs)?
            ));
        }
    } else {
        let map = schema
            .as_object()
            .ok_or_else(|| format!("{title}: schema root must be an object"))?;
        out.push_str(&field_table(map, defs)?);
    }
    if !variants.is_empty() {
        out.push_str(&variants.join("\n"));
    }
    Ok(out)
}

/// Label a union variant by its literal `type` tag, falling back to its index.
fn variant_label(map: &Map<String, Value>, index: usize) -> String {
    let tag = map
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get("type"))
        .and_then(|schema| schema.get("const"))
        .and_then(Value::as_str);
    let label = match tag {
        Some(tag) => format!("`{tag}`"),
        None => format!("variant {}", index + 1),
    };
    match map.get("description").and_then(Value::as_str) {
        Some(description) => format!("**{label}** — {}", flatten(description)),
        None => format!("**{label}**"),
    }
}

/// Render one object schema's fields as a Markdown table.
fn field_table(map: &Map<String, Value>, defs: Option<&Map<String, Value>>) -> Result<String, String> {
    let required = required_set(map);
    let mut out = String::from("| Field | Type | Required | Notes |\n|---|---|---|---|\n");
    let properties = map.get("properties").and_then(Value::as_object);
    if let Some(properties) = properties.filter(|properties| !properties.is_empty()) {
        let sorted: BTreeMap<&String, &Value> = properties.iter().collect();
        for (name, schema) in sorted {
            let (kind, notes) = type_notes(schema, defs)?;
            out.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                cell(name),
                cell(&kind),
                if required.contains(name.as_str()) {
                    "required"
                } else {
                    "optional"
                },
                cell(&notes.join("; ")),
            ));
        }
    } else if map.get("additionalProperties").is_some() {
        out.push_str("| *(additional)* | any | optional | Unconstrained JSON values |\n");
    } else {
        return Ok("Unconstrained JSON.\n".to_string());
    }
    Ok(out)
}

/// Read one schema's `required` member names.
fn required_set(map: &Map<String, Value>) -> BTreeSet<&str> {
    map.get("required")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default()
}

/// Render one field schema as a JSON type label plus explanatory notes.
fn type_notes(
    schema: &Value,
    defs: Option<&Map<String, Value>>,
) -> Result<(String, Vec<String>), String> {
    match schema {
        Value::Bool(true) => Ok(("any".to_string(), vec!["Unconstrained JSON".to_string()])),
        Value::Bool(false) => Err("`false` schema is unsatisfiable".to_string()),
        Value::Object(map) => {
            let mut notes = Vec::new();
            if let Some(description) = map.get("description").and_then(Value::as_str) {
                notes.push(flatten(description));
            }
            if let Some(reference) = map.get("$ref").and_then(Value::as_str) {
                let name = reference.strip_prefix("#/$defs/").unwrap_or(reference);
                if let Some(definition) = defs.and_then(|defs| defs.get(name)) {
                    for note in ref_notes(definition) {
                        if !notes.contains(&note) {
                            notes.push(note);
                        }
                    }
                }
                return Ok((name.to_string(), notes));
            }
            if let Some(value) = map.get("const") {
                notes.push(format!("Literal `{}`", json_literal(value)?));
                let kind = map
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| literal_type(value));
                return Ok((kind.to_string(), notes));
            }
            if let Some(values) = map.get("enum").and_then(Value::as_array) {
                notes.push(format!("One of {}", list_literals(values)?));
                let kind = map.get("type").and_then(Value::as_str).unwrap_or("string");
                return Ok((kind.to_string(), notes));
            }
            if let Some(values) = ["anyOf", "oneOf"]
                .into_iter()
                .find_map(|key| map.get(key).and_then(Value::as_array))
            {
                let mut labels = Vec::new();
                for variant in values {
                    let (label, variant_notes) = type_notes(variant, defs)?;
                    labels.push(label);
                    for note in variant_notes {
                        if !notes.contains(&note) {
                            notes.push(note);
                        }
                    }
                }
                return Ok((labels.join(" | "), notes));
            }
            if map.contains_key("allOf") {
                return Err("`allOf` is unsupported in payload tables".to_string());
            }
            let mut kind = match map.get("type") {
                Some(Value::String(kind)) => kind.clone(),
                Some(Value::Array(kinds)) => {
                    let mut labels = Vec::new();
                    for kind in kinds {
                        labels.push(
                            kind.as_str()
                                .ok_or_else(|| "`type` array entries must be strings".to_string())?
                                .to_string(),
                        );
                    }
                    labels.join(" | ")
                }
                Some(other) => return Err(format!("unsupported `type` value {other}")),
                None => {
                    if map.contains_key("properties") || map.contains_key("additionalProperties") {
                        "object".to_string()
                    } else {
                        "any".to_string()
                    }
                }
            };
            if kind == "array" {
                match map.get("items") {
                    Some(items) => {
                        let (item, _) = type_notes(items, defs)?;
                        kind = if item.contains('|') {
                            format!("array<{item}>")
                        } else {
                            format!("{item}[]")
                        };
                    }
                    None => notes.push("Items are unconstrained JSON".to_string()),
                }
            }
            if kind == "object" && map.get("properties").is_none() {
                match map.get("additionalProperties") {
                    Some(Value::Bool(true)) => {
                        notes.push("Additional properties are unconstrained JSON".to_string())
                    }
                    Some(_) => notes.push("Additional property values follow the declared schema".to_string()),
                    None => {}
                }
            }
            if map.get("deprecated") == Some(&Value::Bool(true)) {
                notes.push("Deprecated".to_string());
            }
            Ok((kind, notes))
        }
        _ => Err("schema must be an object or a boolean".to_string()),
    }
}

/// Describe a referenced definition's literals without recursing through refs.
fn ref_notes(definition: &Value) -> Vec<String> {
    let Some(map) = definition.as_object() else {
        return Vec::new();
    };
    let mut notes = Vec::new();
    if let Some(description) = map.get("description").and_then(Value::as_str) {
        notes.push(flatten(description));
    }
    if let Some(value) = map.get("const") {
        if let Ok(literal) = json_literal(value) {
            notes.push(format!("Literal `{literal}`"));
        }
    } else if let Some(values) = map.get("enum").and_then(Value::as_array) {
        if let Ok(literals) = list_literals(values) {
            notes.push(format!("One of {literals}"));
        }
    } else if let Some(values) = ["oneOf", "anyOf"]
        .into_iter()
        .find_map(|key| map.get(key).and_then(Value::as_array))
    {
        let mut tags = Vec::new();
        for variant in values {
            if let Some(value) = variant.get("const") {
                if let Ok(literal) = json_literal(value) {
                    tags.push(format!("`{literal}`"));
                }
            }
        }
        if !tags.is_empty() {
            notes.push(format!("One of {}", tags.join(", ")));
        }
    }
    notes
}

/// Render a JSON literal for a table note.
fn json_literal(value: &Value) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| format!("literal must serialize: {error}"))
}

/// Render an enum's values as a comma-separated list.
fn list_literals(values: &[Value]) -> Result<String, String> {
    let mut rendered = Vec::new();
    for value in values {
        rendered.push(format!("`{}`", json_literal(value)?));
    }
    Ok(rendered.join(", "))
}

/// Infer the JSON type of a literal when the schema omits `type`.
fn literal_type(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::Null => "null",
        _ => "any",
    }
}

/// Collapse a schema description into one line.
fn flatten(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Escape a table cell so literal pipes cannot split the row.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}
