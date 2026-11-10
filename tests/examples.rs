//! Validation of the JSONL protocol examples under `docs/examples/`.
//!
//! Each line is `{dir, message, note?, source}`: `dir` is `c2s` or `s2c`,
//! `source` is `recorded` or `written`, and `message` is one complete JSON-RPC
//! frame (request with `id`, response with the matching `id`, or notification).
//!
//! Custom `magit` methods are validated against the directional params/result
//! schema named by `docs/schema/methods.json`. Standard LSP frames
//! (`initialize`, `initialized`, `textDocument/didOpen`, ... and their
//! responses) get envelope checks only: `jsonrpc`, method presence, the
//! request/response frame shape and request id correlation. The narrow
//! validator does not model the official LSP schemas, so a malformed standard
//! payload would pass here; checking it belongs to the recorded live replay
//! against the real binary in a later slice.

#[path = "common/mod.rs"]
mod fixtures;

use fixtures::schema::{read_schema, schema_errors};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Directory holding the committed example exchanges.
const EXAMPLES_DIR: &str = "docs/examples";

/// Which peer sent a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Dir {
    ClientToServer,
    ServerToClient,
}

impl Dir {
    /// Parse the wire name of a direction.
    fn parse(text: &str) -> Option<Self> {
        match text {
            "c2s" => Some(Self::ClientToServer),
            "s2c" => Some(Self::ServerToClient),
            _ => None,
        }
    }

    /// The wire name of this direction.
    fn as_str(self) -> &'static str {
        match self {
            Self::ClientToServer => "c2s",
            Self::ServerToClient => "s2c",
        }
    }

    /// The peer that answers a request sent in this direction.
    fn peer(self) -> Self {
        match self {
            Self::ClientToServer => Self::ServerToClient,
            Self::ServerToClient => Self::ClientToServer,
        }
    }
}

/// Whether a custom method is a request or a notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Request,
    Notification,
}

/// One entry of the hand-maintained method catalog.
struct MethodSpec {
    method: String,
    direction: Dir,
    kind: Kind,
    params_schema: Option<String>,
    result_schema: Option<String>,
}

/// The method catalog read from `docs/schema/methods.json`.
struct Catalog {
    methods: Vec<MethodSpec>,
}

impl Catalog {
    /// Read the committed method catalog.
    fn load(root: &Path) -> Result<Self, String> {
        let path = root.join("docs/schema/methods.json");
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let value: Value = serde_json::from_str(&text)
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        let entries = value["methods"]
            .as_array()
            .ok_or("methods.json has no methods array")?;
        let mut methods = Vec::new();
        for entry in entries {
            let method = entry["method"]
                .as_str()
                .ok_or("method entry has no name")?
                .to_owned();
            let direction = match entry["direction"].as_str() {
                Some("clientToServer") => Dir::ClientToServer,
                Some("serverToClient") => Dir::ServerToClient,
                other => return Err(format!("{method}: unknown direction {other:?}")),
            };
            let kind = match entry["kind"].as_str() {
                Some("request") => Kind::Request,
                Some("notification") => Kind::Notification,
                other => return Err(format!("{method}: unknown kind {other:?}")),
            };
            methods.push(MethodSpec {
                method,
                direction,
                kind,
                params_schema: schema_name(&entry["params"]),
                result_schema: schema_name(&entry["result"]),
            });
        }
        Ok(Self { methods })
    }

    /// Find the catalog entry for a custom method.
    fn find(&self, method: &str) -> Option<&MethodSpec> {
        self.methods.iter().find(|spec| spec.method == method)
    }
}

/// The schema artifact named by a catalog params/result block, if constrained.
fn schema_name(block: &Value) -> Option<String> {
    block
        .get("schema")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// One parsed example line.
struct Line {
    dir: Dir,
    message: Map<String, Value>,
    #[allow(dead_code)]
    note: Option<String>,
    source: String,
}

/// Parse and shape-check one JSONL line.
fn parse_line(text: &str) -> Result<Line, String> {
    let value: Value = serde_json::from_str(text).map_err(|error| format!("not JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or("line is not a JSON object")?;
    for key in object.keys() {
        if !matches!(key.as_str(), "dir" | "message" | "note" | "source") {
            return Err(format!("unknown key {key:?}"));
        }
    }
    let dir = Dir::parse(object.get("dir").and_then(Value::as_str).unwrap_or(""))
        .ok_or("dir must be \"c2s\" or \"s2c\"")?;
    let source = object
        .get("source")
        .and_then(Value::as_str)
        .ok_or("missing string source")?;
    if !matches!(source, "recorded" | "written") {
        return Err(format!(
            "source must be \"recorded\" or \"written\", found {source:?}"
        ));
    }
    let note = match object.get("note") {
        None => None,
        Some(Value::String(note)) => Some(note.clone()),
        Some(other) => return Err(format!("note must be a string, found {other}")),
    };
    let message = object
        .get("message")
        .and_then(Value::as_object)
        .ok_or("missing object message")?
        .clone();
    Ok(Line {
        dir,
        message,
        note,
        source: source.to_owned(),
    })
}

/// A request whose response has not appeared yet.
struct Pending {
    method: String,
}

/// The JSON-RPC id as a correlation key, accepting only number or string ids.
fn id_key(id: &Value) -> Result<String, String> {
    match id {
        Value::Number(_) | Value::String(_) => Ok(id.to_string()),
        other => Err(format!("id must be a number or string, found {other}")),
    }
}

/// Reject JSON-RPC members that do not belong on this frame kind.
fn check_members(message: &Map<String, Value>, allowed: &[&str]) -> Result<(), String> {
    for key in message.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unexpected member {key:?}"));
        }
    }
    Ok(())
}

/// Validate one frame and update the outstanding-request table.
fn validate_message(
    line: &Line,
    catalog: &Catalog,
    pending: &mut HashMap<(Dir, String), Pending>,
) -> Result<(), String> {
    let message = &line.message;
    if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err("message.jsonrpc must be \"2.0\"".into());
    }

    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // A response: correlate its id with a request sent by the other peer.
        check_members(message, &["jsonrpc", "id", "result", "error"])?;
        let id = message.get("id").ok_or("response frame needs an id")?;
        let key = id_key(id)?;
        let request_dir = line.dir.peer();
        let request = pending
            .remove(&(request_dir, key.clone()))
            .ok_or_else(|| format!("response id {key} has no preceding {} request", request_dir.as_str()))?;
        let has_result = message.contains_key("result");
        let has_error = message.contains_key("error");
        if has_result == has_error {
            return Err("response must carry exactly one of result and error".into());
        }
        if has_result {
            if let Some(spec) = catalog.find(&request.method) {
                if let Some(schema_name) = &spec.result_schema {
                    let schema = read_schema(schema_name);
                    let result = &message["result"];
                    let errors = schema_errors(&schema, &schema, result, "$.result");
                    if !errors.is_empty() {
                        return Err(format!(
                            "custom method {} result: {}",
                            request.method,
                            errors.join("; ")
                        ));
                    }
                }
            }
        }
        return Ok(());
    };

    // A request or notification.
    check_members(message, &["jsonrpc", "id", "method", "params"])?;
    let is_request = message.contains_key("id");
    if is_request {
        id_key(&message["id"])?;
    }

    if let Some(spec) = catalog.find(method) {
        if spec.direction != line.dir {
            return Err(format!(
                "custom method {method} must travel {} but the line is {}",
                spec.direction.as_str(),
                line.dir.as_str()
            ));
        }
        match spec.kind {
            Kind::Request if !is_request => {
                return Err(format!("custom request {method} needs an id"));
            }
            Kind::Notification if is_request => {
                return Err(format!("custom notification {method} must not carry an id"));
            }
            _ => {}
        }
        let params = message
            .get("params")
            .ok_or_else(|| format!("custom method {method} needs params"))?;
        if let Some(schema_name) = &spec.params_schema {
            let schema = read_schema(schema_name);
            let errors = schema_errors(&schema, &schema, params, "$.params");
            if !errors.is_empty() {
                return Err(format!(
                    "custom method {method} params: {}",
                    errors.join("; ")
                ));
            }
        }
    }

    if is_request {
        let key = id_key(&message["id"])?;
        pending.insert(
            (line.dir, key),
            Pending {
                method: method.to_owned(),
            },
        );
    }
    Ok(())
}

/// Validate every line of one example file, including id correlation.
fn validate_exchange(
    path: &Path,
    text: &str,
    catalog: &Catalog,
) -> Result<usize, String> {
    let mut pending: HashMap<(Dir, String), Pending> = HashMap::new();
    let mut count = 0;
    for (index, raw) in text.lines().enumerate() {
        if raw.trim().is_empty() {
            return Err(format!("{}:{}: blank line", path.display(), index + 1));
        }
        let line = parse_line(raw)
            .map_err(|error| format!("{}:{}: {error}", path.display(), index + 1))?;
        validate_message(&line, catalog, &mut pending)
            .map_err(|error| format!("{}:{}: {error}", path.display(), index + 1))?;
        count += 1;
    }
    if let Some(((direction, id), request)) = pending.into_iter().next() {
        return Err(format!(
            "{}: unanswered {} request {id} ({})",
            path.display(),
            direction.as_str(),
            request.method
        ));
    }
    Ok(count)
}

/// Every committed example file, sorted by path.
fn example_files(root: &Path) -> Vec<PathBuf> {
    let dir = root.join(EXAMPLES_DIR);
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .map(|entry| entry.expect("example entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
        .collect();
    files.sort();
    files
}

#[test]
fn committed_example_files_parse_and_validate_against_the_method_catalog() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let catalog = Catalog::load(root).expect("method catalog");
    assert!(
        catalog.methods.len() >= 10,
        "the method catalog is unexpectedly small"
    );
    let files = example_files(root);
    assert!(
        !files.is_empty(),
        "expected at least one example under {EXAMPLES_DIR}"
    );
    for path in files {
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let count = validate_exchange(&path, &text, &catalog)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(count > 0, "{} has no lines", path.display());
    }
}

#[test]
fn plain_open_example_records_initialize_and_a_plain_document_open() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join(EXAMPLES_DIR).join("plain-open.jsonl");
    let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let lines: Vec<Line> = text
        .lines()
        .map(|raw| parse_line(raw).unwrap_or_else(|error| panic!("{error}")))
        .collect();
    assert_eq!(lines[0].dir, Dir::ClientToServer);
    assert_eq!(
        lines[0].message.get("method").and_then(Value::as_str),
        Some("initialize")
    );
    assert_eq!(lines[0].source, "recorded");
    assert!(lines.iter().any(|line| {
        line.dir == Dir::ClientToServer
            && line.message.get("method").and_then(Value::as_str)
                == Some("textDocument/didOpen")
    }));
    assert!(lines.iter().any(|line| {
        line.dir == Dir::ServerToClient
            && line.message.get("method").and_then(Value::as_str) == Some("workspace/applyEdit")
    }));
    assert!(lines.iter().any(|line| {
        line.dir == Dir::ClientToServer
            && line.message.get("method").and_then(Value::as_str)
                == Some("textDocument/didChange")
    }));
    assert!(lines.iter().all(|line| line.source == "recorded"));
}

#[test]
fn corrupted_example_lines_fail_validation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let catalog = Catalog::load(root).expect("method catalog");
    let path = Path::new("in-memory.jsonl");

    let notification = |params: Value| {
        serde_json::json!({
            "dir": "c2s",
            "source": "written",
            "message": {"jsonrpc": "2.0", "method": "view/resize", "params": params},
        })
    };
    let valid = notification(serde_json::json!({"viewId": "view", "cols": 80, "rows": 24}));
    let text = serde_json::to_string(&valid).unwrap();
    validate_exchange(path, &text, &catalog)
        .expect("the uncorrupted custom notification must validate");

    let mut bad_dir = valid.clone();
    bad_dir["dir"] = serde_json::json!("sideways");
    let mut missing_source = valid.clone();
    missing_source.as_object_mut().unwrap().remove("source");
    let mut bad_params = valid.clone();
    bad_params["message"]["params"]["cols"] = serde_json::json!("eighty");
    let mut wrong_direction = valid.clone();
    wrong_direction["dir"] = serde_json::json!("s2c");

    for (case, corrupted) in [
        ("an illegal dir", bad_dir),
        ("a missing source", missing_source),
        ("custom params violating the schema", bad_params),
        ("a custom method in the wrong direction", wrong_direction),
    ] {
        let text = serde_json::to_string(&corrupted).unwrap();
        assert!(
            validate_exchange(path, &text, &catalog).is_err(),
            "a line with {case} must fail validation"
        );
    }

    // Custom request results are validated against the result schema, not just
    // the params. Start from a valid `view/open` exchange and corrupt its result.
    let request = serde_json::json!({
        "dir": "c2s",
        "source": "written",
        "message": {
            "jsonrpc": "2.0", "id": 7, "method": "view/open",
            "params": {
                "kind": "status",
                "args": {"repo_path": "/repo"},
                "viewport": {"cols": 80, "rows": 24},
            },
        },
    });
    let descriptor = serde_json::json!({
        "id": "view-1", "kind": "status", "uri": "file:///repo/.magit",
        "title": "status", "placement": "main", "mode": "interactive",
        "scope": "common", "capture": false, "viewport": {"cols": 80, "rows": 24},
    });
    let response = |result: Value| {
        serde_json::json!({
            "dir": "s2c",
            "source": "written",
            "message": {"jsonrpc": "2.0", "id": 7, "result": result},
        })
    };
    let valid_pair = format!(
        "{}\n{}\n",
        serde_json::to_string(&request).unwrap(),
        serde_json::to_string(&response(descriptor.clone())).unwrap()
    );
    validate_exchange(path, &valid_pair, &catalog)
        .expect("the uncorrupted custom request/response pair must validate");

    let mut bad_result = descriptor;
    bad_result.as_object_mut().unwrap().remove("id");
    let corrupt_pair = format!(
        "{}\n{}\n",
        serde_json::to_string(&request).unwrap(),
        serde_json::to_string(&response(bad_result)).unwrap()
    );
    let error = validate_exchange(path, &corrupt_pair, &catalog)
        .expect_err("a custom result missing a required field must fail validation");
    assert!(
        error.contains("$.result") && error.contains("missing required property id"),
        "the result schema error must be reported: {error}"
    );
}
