//! Generated Markdown payload tables for the client specification.
//!
//! The tables are derived from the committed directional schemas and the
//! hand-maintained method catalog, so field names, JSON types and
//! required/optional status cannot drift from the wire contract.

use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{Direction, MethodKind, Payload};

/// Repository-relative path of the generated payload tables.
pub const DOC_PATH: &str = "docs/schema/payload-tables.md";
/// Repository-relative path of the generated worked exchanges.
pub const EXCHANGES_DOC_PATH: &str = "docs/schema/worked-exchanges.md";
const EXCHANGES_LIMIT: usize = 100_000;
/// Repository-relative path of the generated capability levels.
pub const CAPABILITY_DOC_PATH: &str = "docs/schema/capability-levels.md";
/// Repository-relative directory holding the recorded exchange files.
pub const EXAMPLES_DIR: &str = "docs/examples";

/// One standard capability row in the generated Plain capability table.
pub struct CapabilityLevel {
    /// The `PlainCapabilities` field the row documents.
    pub field: &'static str,
    /// JSON pointer the initialize capability is read from.
    pub pointer: &'static str,
    /// Rendered initialize capability name.
    pub capability: &'static str,
    /// Plain level the capability belongs to.
    pub level: &'static str,
    /// Behavior the declaration enables, or its absence.
    pub effect: &'static str,
}

/// Plain capability classification in `PlainCapabilities` field order.
///
/// The field list is pinned against the implemented struct by
/// `tests/schema.rs`; a new capability field fails the pin until this table
/// documents it.
pub const CAPABILITY_LEVELS: &[CapabilityLevel] = &[
    CapabilityLevel {
        field: "apply_edit",
        pointer: "/workspace/applyEdit",
        capability: "`workspace.applyEdit`",
        level: "L1",
        effect: "Status document delivered as a standard `WorkspaceEdit`; status actions and commit drafts need it.",
    },
    CapabilityLevel {
        field: "document_changes",
        pointer: "/workspace/workspaceEdit/documentChanges",
        capability: "`workspace.workspaceEdit.documentChanges`",
        level: "L1 (versioned sub-capability)",
        effect: "Versioned `documentChanges` with the client's version; absent uses the legacy unversioned `changes` map.",
    },
    CapabilityLevel {
        field: "code_actions",
        pointer: "/textDocument/codeAction",
        capability: "`textDocument/codeAction`",
        level: "L2",
        effect: "Code actions are returned; `magit.command` runs them without the client declaring `workspace/executeCommand`.",
    },
    CapabilityLevel {
        field: "disabled_actions",
        pointer: "/textDocument/codeAction/disabledSupport",
        capability: "`textDocument/codeAction.disabledSupport`",
        level: "L2 (sub-capability)",
        effect: "Unavailable and planned actions carry a disabled reason; absent they are omitted.",
    },
    CapabilityLevel {
        field: "show_document",
        pointer: "/window/showDocument/support",
        capability: "`window/showDocument.support`",
        level: "L3",
        effect: "Commit drafts open through `window/showDocument`; absent sends manual instructions instead.",
    },
    CapabilityLevel {
        field: "work_done_progress",
        pointer: "/window/workDoneProgress",
        capability: "`window/workDoneProgress`",
        level: "L3",
        effect: "Mutations run `window/workDoneProgress/create` and `$/progress` begin/report/end; absent still runs with messages.",
    },
    CapabilityLevel {
        field: "completion",
        pointer: "/textDocument/completion",
        capability: "`textDocument/completion`",
        level: "none (not implemented)",
        effect: "Extracted into the effective-capability echo only; the server advertises no provider and registers no handler.",
    },
    CapabilityLevel {
        field: "folding_range",
        pointer: "/textDocument/foldingRange",
        capability: "`textDocument/foldingRange`",
        level: "L3 (request-only)",
        effect: "The client may request server-owned folds; declaring it gates no lower level.",
    },
    CapabilityLevel {
        field: "position_encodings",
        pointer: "/general/positionEncodings",
        capability: "`general.positionEncodings`",
        level: "L0",
        effect: "Negotiates `utf-8`, `utf-16` or `utf-32`; absent defaults to `utf-16`.",
    },
];

/// Render the complete capability-to-level reference.
pub fn capability_levels_block() -> String {
    let mut out = String::new();
    out.push_str("| `PlainCapabilities` field | Initialize capability | Plain level | Effect |\n");
    out.push_str("|---|---|---|---|\n");
    for row in CAPABILITY_LEVELS {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} |\n",
            row.field, row.capability, row.level, row.effect
        ));
    }
    out
}

/// Render the complete payload-table reference.
///
/// Method rows come from the catalog; payload tables come from every
/// directional schema root. Catalog and shared-list data are not payloads, so
/// they are deliberately excluded.
pub fn block() -> Result<String, String> {
    let mut out = String::new();
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
    Ok(out)
}

/// Render the complete worked-exchange reference.
///
/// One subsection per JSONL example file in sorted file order; each line's
/// recorded `message` is rendered as compact JSON without changing any value.
pub fn worked_exchanges_block() -> Result<String, String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(EXAMPLES_DIR);
    worked_exchanges_block_from(&dir)
}

/// Render worked exchanges loaded from an examples directory.
pub fn worked_exchanges_block_from(dir: &Path) -> Result<String, String> {
    let files = super::example_recordings::recordings(&dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?;
    if files.is_empty() {
        return Err(format!("{}: no JSONL examples found", dir.display()));
    }
    let mut out = String::new();
    out.push_str("#### Worked exchanges\n\n");
    out.push_str(
        "Recorded end-to-end exchanges from the example files, one subsection per file.\n",
    );
    for (name, bytes) in &files {
        let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
        let mut lines = Vec::new();
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            lines.push(
                serde_json::from_str::<Value>(line)
                    .map_err(|error| format!("{}:{}: {error}", name, index + 1))?,
            );
        }
        out.push('\n');
        out.push_str(&format!("##### `{name}`\n\n"));
        out.push_str(&provenance(&lines));
        out.push_str("\n\n```jsonl\n");
        for line in &lines {
            let message = line
                .get("message")
                .ok_or_else(|| format!("{}: frame lacks `message`", name))?;
            out.push_str(
                &serde_json::to_string(message).map_err(|error| format!("{}: {error}", name))?,
            );
            out.push('\n');
        }
        out.push_str("```\n");
    }
    out.push('\n');
    Ok(out)
}

/// Render the generated worked-exchange index and its numbered transcript parts.
pub fn worked_exchanges_artifacts() -> Result<Vec<(String, String)>, String> {
    let full = worked_exchanges_block()?;
    worked_exchanges_artifacts_from(&full, EXCHANGES_LIMIT)
}

pub(crate) fn worked_exchanges_artifacts_from(
    full: &str,
    limit: usize,
) -> Result<Vec<(String, String)>, String> {
    let sections: Vec<_> = full.split("\n##### ").skip(1).collect();
    let mut parts = Vec::new();
    let mut current = String::from("# Worked exchanges\n\n");
    for section in sections {
        let section = format!("##### {section}");
        let Some((prefix, body)) = section.split_once("```jsonl\n") else {
            return Err("worked exchange lacks jsonl fence".into());
        };
        let Some((messages, _)) = body.split_once("```\n") else {
            return Err("worked exchange has unterminated jsonl fence".into());
        };
        let mut exchange_part = format!("{prefix}```jsonl\n");
        for message in messages.lines() {
            let addition = format!("{message}\n");
            if current.len() + exchange_part.len() + addition.len() + 5 > limit {
                if current.len() > "# Worked exchanges\n\n".len()
                    && current.len() + exchange_part.len() + 5 > limit
                {
                    parts.push(current);
                    current = String::from("# Worked exchanges (continued)\n\n");
                }
                if current.len() + exchange_part.len() + addition.len() + 5 > limit {
                    if exchange_part.ends_with("```jsonl\n") {
                        return Err(
                            "one worked message exceeds the generated part size limit".into()
                        );
                    }
                    exchange_part.push_str("```\n");
                    current.push_str(&exchange_part);
                    parts.push(current);
                    current = String::from("# Worked exchanges (continued)\n\n");
                    exchange_part = format!(
                        "##### {} (continued)\n\n{}```jsonl\n",
                        prefix.trim_start_matches("##### ").trim(),
                        "Frames continue from the preceding part.\n\n"
                    );
                }
            }
            exchange_part.push_str(&addition);
        }
        exchange_part.push_str("```\n");
        current.push_str(&exchange_part);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    let mut index =
        String::from("# Worked exchanges\n\nGenerated transcript parts, in source order:\n\n");
    let paths: Vec<_> = (1..=parts.len())
        .map(|number| format!("docs/schema/worked-exchanges-{number:02}.md"))
        .collect();
    for (number, _) in paths.iter().enumerate() {
        index.push_str(&format!(
            "- [Part {:02}](worked-exchanges-{:02}.md)\n",
            number + 1,
            number + 1
        ));
        parts[number] = format!("{}\n", parts[number]);
        if parts[number].len() + 1 >= limit {
            return Err("worked exchange part exceeds the generated part size limit".into());
        }
    }
    let mut artifacts = vec![(EXCHANGES_DOC_PATH.to_string(), index)];
    artifacts.extend(paths.into_iter().zip(parts));
    Ok(artifacts)
}

/// One-sentence provenance derived from a file's recorded `source` labels.
fn provenance(lines: &[Value]) -> String {
    let recorded = lines
        .iter()
        .all(|line| line.get("source").and_then(Value::as_str) == Some("recorded"));
    if recorded {
        "Frames recorded from a real `TestServer` run against a temporary repository; temporary paths are normalized to `/repo`.".to_string()
    } else {
        "Frames are hand-written illustrative examples, not recorded server traffic.".to_string()
    }
}

/// Write all standalone generated references beneath `root`.
pub fn write(root: &Path) -> io::Result<()> {
    let mut files = vec![
        (DOC_PATH.to_string(), block().map_err(invalid_data)?),
        (CAPABILITY_DOC_PATH.to_string(), capability_levels_block()),
    ];
    files.extend(worked_exchanges_artifacts().map_err(invalid_data)?);
    let expected: BTreeSet<_> = files
        .iter()
        .filter_map(|(relative, _)| numbered_exchange(relative).map(str::to_owned))
        .collect();
    for path in numbered_exchange_paths(root)? {
        if !expected.contains(&path.file_name().unwrap().to_string_lossy().into_owned()) {
            fs::remove_file(path)?;
        }
    }
    for (relative, text) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, format!("{text}\n"))?;
    }
    Ok(())
}

/// Return standalone references that differ from generated content.
pub fn stale_paths(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut generated = vec![
        (DOC_PATH.to_string(), block().map_err(invalid_data)?),
        (CAPABILITY_DOC_PATH.to_string(), capability_levels_block()),
    ];
    generated.extend(worked_exchanges_artifacts().map_err(invalid_data)?);
    let mut stale = Vec::new();
    for (relative, text) in generated {
        let path = root.join(&relative);
        if fs::read_to_string(path).unwrap_or_default() != format!("{text}\n") {
            stale.push(PathBuf::from(relative));
        }
    }
    let expected: BTreeSet<_> = worked_exchanges_artifacts()
        .map_err(invalid_data)?
        .iter()
        .filter_map(|(relative, _)| numbered_exchange(relative).map(str::to_owned))
        .collect();
    stale.extend(
        numbered_exchange_paths(root)?
            .into_iter()
            .filter(|path| {
                !expected.contains(&path.file_name().unwrap().to_string_lossy().into_owned())
            })
            .map(|path| path.strip_prefix(root).unwrap().to_path_buf()),
    );
    stale.sort();
    Ok(stale)
}

fn numbered_exchange(path: &str) -> Option<&str> {
    let name = Path::new(path).file_name()?.to_str()?;
    let number = name
        .strip_prefix("worked-exchanges-")?
        .strip_suffix(".md")?;
    (!number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())).then_some(name)
}

fn numbered_exchange_paths(root: &Path) -> io::Result<Vec<PathBuf>> {
    let dir = root.join("docs/schema");
    let mut paths = Vec::new();
    if !dir.exists() {
        return Ok(paths);
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(numbered_exchange)
                .is_some()
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

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
fn field_table(
    map: &Map<String, Value>,
    defs: Option<&Map<String, Value>>,
) -> Result<String, String> {
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
                    Some(_) => notes
                        .push("Additional property values follow the declared schema".to_string()),
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
