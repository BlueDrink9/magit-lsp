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
//! payload would pass here; that is covered by the live replay below.
//!
//! The replay tests drive the real server with the recorded `c2s` frames and
//! compare every `s2c` frame with its recorded counterpart. Comparison is
//! exact after the recording's own normalization, documented at
//! [`replay_example`]: the temporary repository root becomes `/repo`, the
//! server binary path becomes `/magit-lsp/target/debug/magit-lsp`, outbound
//! frames gain the `jsonrpc` member the current server omits, outbound
//! `magit-lsp/outbound/N` ids are substituted in the client's replies, opaque
//! commit-run ids become `run-1`, and standalone 40-hex commit/object hashes
//! become `hash-N` by first-seen position. Versions, order and payloads are not
//! relaxed: an unexpected frame or a divergent expectation fails.

#[path = "common/mod.rs"]
mod fixtures;
#[path = "../scripts/example_recordings.rs"]
mod example_recordings;

use fixtures::schema::{read_schema, schema_errors};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

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
    source: String,
    raw: String,
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
    if let Some(note) = object.get("note") {
        if !note.is_string() {
            return Err(format!("note must be a string, found {note}"));
        }
    }
    let message = object
        .get("message")
        .and_then(Value::as_object)
        .ok_or("missing object message")?
        .clone();
    Ok(Line {
        dir,
        message,
        source: source.to_owned(),
        raw: text.to_owned(),
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

#[test]
fn committed_example_files_parse_and_validate_against_the_method_catalog() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let catalog = Catalog::load(root).expect("method catalog");
    assert!(
        catalog.methods.len() >= 10,
        "the method catalog is unexpectedly small"
    );
    let files = example_recordings::recordings(&root.join(EXAMPLES_DIR))
        .unwrap_or_else(|error| panic!("load examples: {error}"));
    assert!(
        !files.is_empty(),
        "expected at least one example under {EXAMPLES_DIR}"
    );
    for (name, bytes) in files {
        let path = root.join(EXAMPLES_DIR).join(&name);
        let text = String::from_utf8(bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let count = validate_exchange(&path, &text, &catalog)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(count > 0, "{} has no lines", path.display());
    }
}

#[test]
fn numbered_large_recording_fragments_load_as_the_original_logical_recording() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let examples = example_recordings::recordings(&root.join(EXAMPLES_DIR)).expect("examples");
    let dir = root.join(EXAMPLES_DIR);
    for (logical, fragments) in [
        ("commit-edit-confirm.jsonl", ["commit-edit-confirm-01.jsonl", "commit-edit-confirm-02.jsonl"]),
        ("commit-edit-abort.jsonl", ["commit-edit-abort-01.jsonl", "commit-edit-abort-02.jsonl"]),
    ] {
        let matches: Vec<_> = examples.iter().filter(|(name, _)| name == logical).collect();
        assert_eq!(matches.len(), 1, "fragments expose one original logical name");
        let expected = fragments.iter().map(|name| fs::read(dir.join(name)).unwrap()).flatten().collect::<Vec<_>>();
        assert_eq!(matches[0].1.as_slice(), expected.as_slice(), "ordered fragments form the logical bytes");
        assert!(matches[0].1.len() > 100_000, "validation sees the whole recording");
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

/// Parse one committed example file into its validated line shapes.
fn example_lines(name: &str) -> Vec<Line> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join(EXAMPLES_DIR);
    let text = example_recordings::recordings(&path).unwrap().into_iter()
        .find(|(logical, _)| logical == name).unwrap_or_else(|| panic!("missing example {name}"));
    let text = String::from_utf8(text.1).expect("example is UTF-8");
    text.lines()
        .map(|raw| parse_line(raw).unwrap_or_else(|error| panic!("{error}")))
        .collect()
}

/// The `s2c` response answering the first `c2s` request with `method`.
fn response_to<'a>(lines: &'a [Line], method: &str) -> &'a Line {
    let request = lines
        .iter()
        .find(|line| {
            line.dir == Dir::ClientToServer
                && line.message.get("method").and_then(Value::as_str) == Some(method)
        })
        .unwrap_or_else(|| panic!("no {method} request"));
    let id = &request.message["id"];
    lines
        .iter()
        .find(|line| line.dir == Dir::ServerToClient && line.message.get("id") == Some(id))
        .unwrap_or_else(|| panic!("no response to {method}"))
}

#[test]
fn plain_code_action_example_records_tagged_targets_and_execution() {
    let lines = example_lines("plain-code-action.jsonl");
    assert!(lines.iter().all(|line| line.source == "recorded"));

    let response = response_to(&lines, "textDocument/codeAction");
    let actions = response.message["result"]
        .as_array()
        .unwrap_or_else(|| panic!("code action result is not an array: {:?}", response.message));
    let kinds: Vec<&str> = actions
        .iter()
        .filter_map(|action| action["command"]["arguments"][0]["target"]["kind"].as_str())
        .collect();
    assert!(
        kinds.contains(&"content"),
        "a content target must be offered: {kinds:?}"
    );
    assert!(
        kinds.contains(&"node"),
        "a semantic node target must be offered: {kinds:?}"
    );

    let executed = lines
        .iter()
        .find(|line| {
            line.dir == Dir::ClientToServer
                && line.message.get("method").and_then(Value::as_str)
                    == Some("workspace/executeCommand")
        })
        .expect("executeCommand request");
    let arguments = &executed.message["params"]["arguments"][0];
    assert_eq!(arguments["command"], "status.stage");
    assert_eq!(arguments["target"]["kind"], "content");
    assert!(arguments["target"]["mutationHash"].is_string());
    assert!(arguments["target"]["nodeId"].is_string());
}

#[test]
fn plain_document_prompt_example_records_the_prompt_document_exchange() {
    let lines = example_lines("plain-document-prompt.jsonl");
    assert!(lines.iter().all(|line| line.source == "recorded"));
    assert!(
        !lines.iter().any(|line| {
            line.message.get("method").and_then(Value::as_str) == Some("view/prompt")
        }),
        "Plain prompts must not use the native view/prompt notification"
    );

    let opened = response_to(&lines, "workspace/executeCommand");
    let effects = opened.message["result"]["effects"]
        .as_array()
        .unwrap_or_else(|| panic!("executeCommand result has no effects: {:?}", opened.message));
    assert!(
        effects.iter().any(|effect| effect["descriptor"]["kind"] == "prompt"),
        "the command must report the pushed prompt view: {effects:?}"
    );

    let prompt_text = lines
        .iter()
        .filter_map(|line| {
            line.message
                .get("params")?
                .get("edit")?
                .get("documentChanges")?
                .get(0)?
                .get("edits")?
                .get(0)?
                .get("newText")?
                .as_str()
        })
        .find(|text| text.contains("Discard changes to f.txt?"))
        .expect("the prompt document is applied as rendered text");
    assert!(prompt_text.lines().any(|line| line.trim() == "Yes"));
    assert!(prompt_text.lines().any(|line| line.trim() == "No"));

    let answer = lines
        .iter()
        .find(|line| {
            line.dir == Dir::ClientToServer
                && line.message.get("method").and_then(Value::as_str)
                    == Some("workspace/executeCommand")
                && line.message["params"]["arguments"][0]["command"] == "prompt.answer"
        })
        .expect("prompt.answer request");
    let target = &answer.message["params"]["arguments"][0]["target"];
    assert_eq!(target["kind"], "node");
    assert!(target["nodeId"]
        .as_str()
        .is_some_and(|node| node.starts_with("prompt:") && node.ends_with(":no")));
}

/// Assert the example delivers the Git commit template into the draft buffer.
fn assert_commit_template_delivered(lines: &[Line]) {
    let draft_uri = "file:///repo/.git/COMMIT_EDITMSG";
    let template = lines
        .iter()
        .find(|line| {
            line.dir == Dir::ServerToClient
                && line.message.get("method").and_then(Value::as_str)
                    == Some("workspace/applyEdit")
                && line.message["params"]["edit"]["documentChanges"][0]["textDocument"]["uri"]
                    == draft_uri
        })
        .expect("the commit template is delivered as a workspace edit");
    let text = template.message["params"]["edit"]["documentChanges"][0]["edits"][0]["newText"]
        .as_str()
        .expect("template text");
    assert!(
        text.starts_with("\n# Please enter the commit message"),
        "the delivered draft text is the Git template: {text:?}"
    );
    assert!(
        lines.iter().any(|line| {
            line.dir == Dir::ServerToClient
                && line.message.get("method").and_then(Value::as_str)
                    == Some("window/showDocument")
                && line.message["params"]["uri"] == draft_uri
        }),
        "the draft buffer is shown"
    );
}

/// The offered run-bound draft action carrying `command`.
fn offered_draft_action(lines: &[Line], command: &str) -> Value {
    lines
        .iter()
        .filter(|line| line.dir == Dir::ServerToClient)
        .find_map(|line| {
            line.message.get("result")?.as_array()?.iter().find_map(|action| {
                let arguments = action.get("command")?.get("arguments")?.get(0)?;
                (arguments.get("command")? == command && arguments.get("runId")?.is_string())
                    .then(|| arguments.clone())
            })
        })
        .unwrap_or_else(|| panic!("{command} is offered for the draft"))
}

/// The `c2s` executeCommand request whose semantic command is `command`.
fn command_request<'a>(lines: &'a [Line], command: &str) -> &'a Line {
    lines
        .iter()
        .find(|line| {
            line.dir == Dir::ClientToServer
                && line.message.get("method").and_then(Value::as_str)
                    == Some("workspace/executeCommand")
                && line.message["params"]["arguments"][0]["command"] == command
        })
        .unwrap_or_else(|| panic!("no executeCommand request for {command}"))
}

/// The `s2c` response answering `request`.
fn response_for<'a>(lines: &'a [Line], request: &Line) -> &'a Line {
    let id = &request.message["id"];
    lines
        .iter()
        .find(|line| line.dir == Dir::ServerToClient && line.message.get("id") == Some(id))
        .unwrap_or_else(|| panic!("no response to the request with id {id}"))
}

#[test]
fn commit_edit_confirm_example_records_the_draft_and_its_confirmation() {
    let lines = example_lines("commit-edit-confirm.jsonl");
    assert!(lines.iter().all(|line| line.source == "recorded"));
    assert_commit_template_delivered(&lines);

    let confirm = offered_draft_action(&lines, "common.confirm");
    assert_eq!(confirm["source"], "synchronized");
    assert!(
        confirm["version"].is_i64(),
        "the confirm is bound to a synchronized version: {confirm}"
    );
    let request = command_request(&lines, "common.confirm");
    assert_eq!(
        request.message["params"]["arguments"][0], confirm,
        "the executed confirm carries exactly the offered arguments"
    );
    let response = response_for(&lines, request);
    assert_eq!(response.message["result"]["type"], "confirmed");
    assert_eq!(response.message["result"]["version"], confirm["version"]);
    assert!(
        lines.iter().any(|line| {
            line.message.get("method").and_then(Value::as_str) == Some("window/showMessage")
                && line.message["params"]["message"] == "Committed: Recorded commit body"
        }),
        "the completed commit is reported"
    );
    assert!(
        !lines.iter().any(|line| {
            line.message
                .get("result")
                .and_then(|result| result.get("type"))
                .and_then(Value::as_str)
                == Some("aborted")
        }),
        "the confirm path never reports a cancellation"
    );
}

#[test]
fn commit_edit_abort_example_records_the_cancellation() {
    let lines = example_lines("commit-edit-abort.jsonl");
    assert!(lines.iter().all(|line| line.source == "recorded"));
    assert_commit_template_delivered(&lines);

    let abort = offered_draft_action(&lines, "common.cancel");
    assert!(
        abort["version"].is_i64(),
        "the abort is bound to an open document version: {abort}"
    );
    let request = command_request(&lines, "common.cancel");
    assert_eq!(
        request.message["params"]["arguments"][0], abort,
        "the executed abort carries exactly the offered arguments"
    );
    let response = response_for(&lines, request);
    assert_eq!(response.message["result"]["type"], "aborted");
    assert!(
        !lines.iter().any(|line| {
            line.message
                .get("result")
                .and_then(|result| result.get("type"))
                .and_then(Value::as_str)
                == Some("confirmed")
        }),
        "the abort path never reports a confirmation"
    );
    assert!(
        !lines.iter().any(|line| {
            line.message.get("method").and_then(Value::as_str) == Some("window/showMessage")
                && line.message["params"]["message"]
                    .as_str()
                    .is_some_and(|text| text.starts_with("Committed:"))
        }),
        "the abort path never reports a commit"
    );
}

#[test]
fn rich_lifecycle_example_pins_the_progress_and_render_order() {
    let lines = example_lines("rich-lifecycle.jsonl");
    assert!(lines.iter().all(|line| line.source == "recorded"));

    let method = |line: &Line, name: &str| {
        line.message.get("method").and_then(Value::as_str) == Some(name)
    };
    let phase = |line: &Line, kind: &str| {
        method(line, "$/progress") && line.message["params"]["value"]["kind"] == kind
    };
    let index = |predicate: &dyn Fn(&Line) -> bool| {
        lines
            .iter()
            .position(|line| predicate(line))
            .unwrap_or_else(|| panic!("no matching line in rich-lifecycle.jsonl"))
    };

    let initialize = &lines[0];
    assert_eq!(initialize.dir, Dir::ClientToServer);
    assert!(method(initialize, "initialize"));
    let declaration = &initialize.message["params"]["capabilities"]["experimental"]["magit"];
    assert_eq!(declaration["v"], 1);
    assert_eq!(declaration["keys"], "dynamic");
    assert_eq!(declaration["modifiers"], "full");

    let opened = response_to(&lines, "view/open");
    let view_id = opened.message["result"]["id"]
        .as_str()
        .expect("view descriptor id");
    let initial_render = index(&|line| {
        line.dir == Dir::ServerToClient
            && method(line, "view/render")
            && line.message["params"]["version"] == 1
    });
    let initial_ack = index(&|line| {
        line.dir == Dir::ClientToServer
            && method(line, "view/rendered")
            && line.message["params"]["version"] == 1
    });
    assert!(
        initial_render < initial_ack,
        "the client acknowledges the initial render only after receiving it"
    );
    let key = index(&|line| line.dir == Dir::ClientToServer && method(line, "view/key"));
    assert!(
        initial_ack < key,
        "the key is pressed against the acknowledged version"
    );
    assert_eq!(
        lines[key].message["params"]["key"], "S",
        "the recorded key is status.stage-all, the mutation whose progress the file pins"
    );

    let create = index(&|line| {
        line.dir == Dir::ServerToClient && method(line, "window/workDoneProgress/create")
    });
    let begin = index(&|line| line.dir == Dir::ServerToClient && phase(line, "begin"));
    let end = index(&|line| line.dir == Dir::ServerToClient && phase(line, "end"));
    let key_id = lines[key].message["id"].clone();
    let key_response = index(&|line| {
        line.dir == Dir::ServerToClient && line.message.get("id") == Some(&key_id)
    });
    let next_render = index(&|line| {
        line.dir == Dir::ServerToClient && method(line, "view/render") && line.message["params"]["version"] == 2
    });

    // The recorded ordering, as observed from the real server: create, begin and
    // end all precede the key response, and the refreshed render follows it.
    assert!(
        create < begin && begin < end && end < key_response,
        "progress create/begin/end must precede the key response: \
         create {create}, begin {begin}, end {end}, response {key_response}"
    );
    assert!(
        key_response < next_render,
        "the post-refresh render must follow the key response: \
         response {key_response}, render {next_render}"
    );

    let token = |index: usize| {
        lines[index].message["params"]["token"]
            .as_str()
            .expect("progress token")
            .to_owned()
    };
    assert_eq!(token(create), token(begin));
    assert_eq!(token(begin), token(end));
    assert_eq!(lines[begin].message["params"]["value"]["title"], "Git");
    assert_eq!(lines[begin].message["params"]["value"]["message"], "view/key");
    assert_eq!(
        lines[begin].message["params"]["value"]["cancellable"],
        true
    );

    assert_eq!(
        lines[next_render].message["params"]["viewId"], view_id,
        "the refreshed render targets the acknowledged view"
    );
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

/// The placeholder the recordings use for the temporary repository root.
const RECORDED_ROOT: &str = "/repo";

/// The placeholder the recordings use for the running server binary.
const RECORDED_BINARY: &str = "/magit-lsp/target/debug/magit-lsp";

/// Build the repository state the named example was recorded against.
///
/// The fixture names mirror the recorded status renders: `plain-open` starts
/// from the ten-line file and applies `standard_changes`; the code-action and
/// prompt examples modify a one-line `f.txt`; the commit examples stage the
/// standard change before running the Commit menu item.
fn replay_fixture(name: &str) -> fixtures::RepoFixture {
    use fixtures::{TEN_LINES, RepoFixture};
    match name {
        "plain-open" | "rich-lifecycle" => RepoFixture::new()
            .commit("init", [("f.txt", TEN_LINES)])
            .modify_standard(),
        "plain-code-action" | "plain-document-prompt" => RepoFixture::new()
            .commit("init", [("f.txt", "one\n")])
            .modify("f.txt", "changed\n"),
        "commit-edit-abort" | "commit-edit-confirm" => RepoFixture::new()
            .commit("initial", [("f.txt", "before\n")])
            .modify_standard(),
        other => panic!("no replay fixture for {other}"),
    }
}

/// The placeholder the recordings use for a commit run id.
const RECORDED_RUN_ID: &str = "run-1";

/// Visit every `runId` member in a frame, including nested command arguments.
fn for_each_run_id(value: &mut Value, apply: &mut dyn FnMut(&mut Value)) {
    match value {
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if key == "runId" {
                    apply(item);
                } else {
                    for_each_run_id(item, apply);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                for_each_run_id(item, apply);
            }
        }
        _ => {}
    }
}

/// The first commit run id in a frame, if it carries one.
fn observed_run_id(message: &Value) -> Option<String> {
    let mut found = None;
    let mut value = message.clone();
    for_each_run_id(&mut value, &mut |id| {
        if found.is_none() {
            if let Some(text) = id.as_str() {
                found = Some(text.to_owned());
            }
        }
    });
    found
}

/// Replace the recorded placeholders with this run's real values in a `c2s`
/// frame, so the client drives the real temporary repository and answers the
/// run the server actually started.
fn expand_client_frame(message: &Value, root: &str, run_id: Option<&str>) -> Value {
    let mut value = message.clone();
    if let Some(run_id) = run_id {
        for_each_run_id(&mut value, &mut |id| {
            if id.as_str() == Some(RECORDED_RUN_ID) {
                *id = Value::String(run_id.to_owned());
            }
        });
    }
    let text = serde_json::to_string(&value).expect("serialise client frame");
    let text = text
        .replace(&format!("file://{RECORDED_ROOT}"), &format!("file://{root}"))
        .replace(RECORDED_ROOT, root);
    serde_json::from_str(&text).expect("parse expanded client frame")
}

/// One recorded client document as the recording's frames assert it.
struct RecordedDocument {
    /// The document version the recording last assigned to the client copy.
    version: u64,
    /// The complete client text the recording last assigned.
    text: String,
}

/// A server edit that a following client `didChange` may echo.
///
/// The recorded edit proves that the client frame is an echo; the observed
/// edit is what the current server actually asked the client to apply, so the
/// refreshed echo is derived from the edit this replay really captured.
#[derive(Clone)]
struct ServerEdit {
    /// The document version the recorded edit applies to.
    version: u64,
    recorded_range: Value,
    recorded_text: String,
    observed_range: Value,
    observed_text: String,
}

/// One full-document client update asserted by a `didOpen` or `didChange`.
struct DocumentUpdate {
    uri: String,
    version: u64,
    text: String,
}

/// The full-document update a client frame asserts, if it is one.
///
/// Only a single full-text `contentChanges` entry counts: an incremental
/// change is client-authored input, not an echo of a server edit.
fn recorded_document_update(message: &Map<String, Value>) -> Option<DocumentUpdate> {
    let method = message.get("method").and_then(Value::as_str)?;
    let params = message.get("params")?;
    let document = params.get("textDocument")?;
    let uri = document.get("uri")?.as_str()?.to_owned();
    let version = document.get("version")?.as_u64()?;
    let text = match method {
        "textDocument/didOpen" => document.get("text")?.as_str()?.to_owned(),
        "textDocument/didChange" => {
            let [change] = params.get("contentChanges")?.as_array()?.as_slice() else {
                return None;
            };
            if change.get("range").is_some() {
                return None;
            }
            change.get("text")?.as_str()?.to_owned()
        }
        _ => return None,
    };
    Some(DocumentUpdate { uri, version, text })
}

/// The target URIs of a recorded `workspace/applyEdit` frame.
fn edited_document_uris(message: &Map<String, Value>) -> Vec<String> {
    message
        .get("params")
        .and_then(|params| params.get("edit"))
        .and_then(|edit| edit.get("documentChanges"))
        .and_then(Value::as_array)
        .map(|changes| {
            changes
                .iter()
                .filter_map(|change| {
                    change
                        .get("textDocument")
                        .and_then(|document| document.get("uri"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The version, range and new text of the sole edit in an `workspace/applyEdit`
/// frame, when it has exactly one document change with exactly one edit.
fn sole_text_edit(message: &Value) -> Option<(u64, Value, String)> {
    let [change] = message.get("params")?.get("edit")?.get("documentChanges")?.as_array()?.as_slice() else {
        return None;
    };
    let [edit] = change.get("edits")?.as_array()?.as_slice() else {
        return None;
    };
    let version = change.get("textDocument")?.get("version")?.as_u64()?;
    Some((version, edit.get("range")?.clone(), edit.get("newText")?.as_str()?.to_owned()))
}

/// The UTF-16 position of an LSP `range` endpoint as a byte offset.
fn position_offset(text: &str, position: &Value) -> Result<usize, String> {
    let line = position.get("line").and_then(Value::as_u64).ok_or("edit position has no line")? as usize;
    let character = position
        .get("character")
        .and_then(Value::as_u64)
        .ok_or("edit position has no character")? as usize;
    let line_start = if line == 0 {
        0
    } else {
        let mut start = None;
        for (seen, (index, _)) in text.match_indices('\n').enumerate() {
            if seen + 1 == line {
                start = Some(index + 1);
                break;
            }
        }
        start.ok_or_else(|| format!("edit line {line} is past the document end"))?
    };
    let line_text = text[line_start..].split('\n').next().unwrap_or("");
    let mut units = 0;
    for (index, character_text) in line_text.char_indices() {
        if units == character {
            return Ok(line_start + index);
        }
        units += character_text.len_utf16();
        if units > character {
            return Err(format!("edit character {character} splits a code point"));
        }
    }
    if units == character {
        Ok(line_start + line_text.len())
    } else {
        Err(format!("edit character {character} is past the line end"))
    }
}

/// Apply one LSP text edit to a complete document text.
fn apply_text_edit(text: &str, range: &Value, new_text: &str) -> Result<String, String> {
    let start = position_offset(text, range.get("start").ok_or("edit range has no start")?)?;
    let end = position_offset(text, range.get("end").ok_or("edit range has no end")?)?;
    if start > end {
        return Err("edit range ends before it starts".to_owned());
    }
    Ok(format!("{}{}{}", &text[..start], new_text, &text[end..]))
}

/// The refreshed text of a client `didChange` that is provably an echo.
///
/// Binding is mechanical: the recorded edit targets the same document version
/// as the recording's previous client text, the `didChange` is exactly one
/// version later, and replaying the recorded edit on that previous text yields
/// exactly the recorded echo. Otherwise the frame is authored input and is
/// left untouched. A proven echo is replaced by the observed server edit
/// applied to the current client text.
fn refreshed_echo(
    recorded_uri: &str,
    recorded_version: u64,
    recorded_text: &str,
    current_text: Option<&str>,
    documents: &HashMap<String, RecordedDocument>,
    edits: &HashMap<String, Option<ServerEdit>>,
) -> Result<Option<String>, String> {
    let Some(Some(edit)) = edits.get(recorded_uri) else {
        return Ok(None);
    };
    let Some(previous) = documents.get(recorded_uri) else {
        return Ok(None);
    };
    if edit.version != previous.version || recorded_version != edit.version + 1 {
        return Ok(None);
    }
    match apply_text_edit(&previous.text, &edit.recorded_range, &edit.recorded_text) {
        Ok(applied) if applied == recorded_text => {}
        _ => return Ok(None),
    }
    let current_text =
        current_text.ok_or_else(|| format!("no client text is recorded for {recorded_uri}"))?;
    apply_text_edit(current_text, &edit.observed_range, &edit.observed_text).map(Some)
}

/// Visit every string leaf in a JSON value.
fn for_each_string(value: &mut Value, apply: &mut dyn FnMut(&mut String)) {
    match value {
        Value::Object(map) => {
            for item in map.values_mut() {
                for_each_string(item, apply);
            }
        }
        Value::Array(items) => {
            for item in items {
                for_each_string(item, apply);
            }
        }
        Value::String(text) => apply(text),
        _ => {}
    }
}

/// Whether a byte is a lowercase hexadecimal digit.
fn is_lower_hex(byte: u8) -> bool {
    matches!(byte, b'0'..=b'9' | b'a'..=b'f')
}

/// Replace each standalone 40-hex hash in `text` with its `hash-N` name.
///
/// A hash is a maximal run of lowercase hex digits that is exactly 40 bytes
/// long; other runs (for example the 64-hex patch content hashes) stay exact.
fn replace_hashes(text: &str, names: &mut HashMap<String, String>) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if !is_lower_hex(bytes[index]) {
            index += 1;
            continue;
        }
        let run_start = index;
        while index < bytes.len() && is_lower_hex(bytes[index]) {
            index += 1;
        }
        out.push_str(&text[start..run_start]);
        let run = &text[run_start..index];
        if run.len() == 40 {
            let next = names.len() + 1;
            let name = names
                .entry(run.to_owned())
                .or_insert_with(|| format!("hash-{next}"));
            out.push_str(name);
        } else {
            out.push_str(run);
        }
        start = index;
    }
    out.push_str(&text[start..]);
    out
}

/// First-seen hash names for one side of the replay comparison.
///
/// The recorded and observed sides each use their own instance, so a hash is
/// named `hash-N` by its position on that side rather than by any shared map.
struct HashNormalizer {
    names: HashMap<String, String>,
}

impl HashNormalizer {
    /// An empty map.
    fn new() -> Self {
        Self {
            names: HashMap::new(),
        }
    }

    /// Rename every standalone 40-hex hash in a frame.
    fn normalize_frame(&mut self, value: &mut Value) {
        for_each_string(value, &mut |text| {
            *text = replace_hashes(text, &mut self.names);
        });
    }
}

/// The recorded comparison form of a frame.
///
/// The current server omits `jsonrpc` on outbound frames, recorded runs used a
/// temporary repository and a checkout-local binary path, commit run ids are
/// opaque hashes, and commit/object hashes differ between runs. The recordings
/// normalized those by adding the member and by the `/repo`, binary-path and
/// `run-1` placeholders; [`HashNormalizer`] maps 40-hex hashes to `hash-N`.
/// Versions and every other value stay exact.
fn canonical_frame(
    message: &Value,
    root: &str,
    binary: &str,
    hashes: &mut HashNormalizer,
) -> Value {
    let mut value = message.clone();
    if value.get("jsonrpc").is_none() {
        value
            .as_object_mut()
            .expect("frame is an object")
            .insert("jsonrpc".to_owned(), Value::String("2.0".to_owned()));
    }
    for_each_run_id(&mut value, &mut |id| {
        if id.is_string() {
            *id = Value::String(RECORDED_RUN_ID.to_owned());
        }
    });
    hashes.normalize_frame(&mut value);
    let text = serde_json::to_string(&value).expect("serialise observed frame");
    let text = text
        .replace(&format!("file://{root}"), &format!("file://{RECORDED_ROOT}"))
        .replace(root, RECORDED_ROOT)
        .replace(binary, RECORDED_BINARY);
    serde_json::from_str(&text).expect("parse canonical frame")
}

/// Return first differing JSON path and its values.
fn first_difference<'a>(expected: &'a Value, observed: &'a Value) -> Option<(String, Option<&'a Value>, Option<&'a Value>)> {
    fn walk<'a>(expected: Option<&'a Value>, observed: Option<&'a Value>, path: &str) -> Option<(String, Option<&'a Value>, Option<&'a Value>)> {
        match (expected, observed) {
            (Some(Value::Object(a)), Some(Value::Object(b))) => {
                for key in a.keys().chain(b.keys()) {
                    if a.get(key) != b.get(key) {
                        let child = format!("{path}.{}", key);
                        return walk(a.get(key), b.get(key), &child);
                    }
                }
                None
            }
            (Some(Value::Array(a)), Some(Value::Array(b))) => {
                for index in 0..a.len().max(b.len()) {
                    if a.get(index) != b.get(index) {
                        let child = format!("{path}[{index}]");
                        return walk(a.get(index), b.get(index), &child);
                    }
                }
                None
            }
            _ => Some((if path.is_empty() { "$".to_owned() } else { path.to_owned() }, expected, observed)),
        }
    }
    walk(Some(expected), Some(observed), "$")
}

const JSON_EXCERPT_LIMIT: usize = 240;

/// Serialize a differing JSON value as a bounded excerpt.
fn json_excerpt(value: Option<&Value>) -> String {
    let Some(value) = value else { return "<missing>".to_owned() };
    let text = serde_json::to_string(value).expect("serialise JSON value");
    let total = value.as_str().map_or_else(|| text.chars().count(), |value| value.chars().count());
    if text.chars().count() <= JSON_EXCERPT_LIMIT {
        return format!("{text:?} ({total} chars total)");
    }
    let excerpt: String = text.chars().take(JSON_EXCERPT_LIMIT).collect();
    format!("{:?}... ({total} chars total)", excerpt)
}

/// The exchange identity of a recorded or captured frame.
///
/// Regeneration may replace a response payload but must not silently rename an
/// exchange: the method (request or notification) and the request/response id
/// still have to match the recorded frame.
fn frame_identity(message: &Map<String, Value>) -> String {
    let method = message.get("method").and_then(Value::as_str);
    match (method, message.get("id")) {
        (Some(method), Some(id)) => format!("request {method} id {id}"),
        (Some(method), None) => format!("notification {method}"),
        (None, Some(id)) => format!("response id {id}"),
        (None, None) => "payload".to_owned(),
    }
}

/// Build a recorded artifact line from a captured server frame.
///
/// The recorded `dir`, `note` and `source` envelope is preserved and only
/// `message` is replaced with the normalized captured frame. The result is
/// re-parsed as a recorded line, so an unexpected key or source is refused
/// rather than written, and a changed exchange identity is an error.
fn captured_line(line: &Line, observed: &Value) -> Result<String, String> {
    let observed_object = observed.as_object().ok_or("captured frame is not an object")?;
    let recorded_identity = frame_identity(&line.message);
    let observed_identity = frame_identity(observed_object);
    if recorded_identity != observed_identity {
        return Err(format!(
            "captured frame identity changed: recorded {recorded_identity}, observed {observed_identity}"
        ));
    }
    let mut value: Value = serde_json::from_str(&line.raw).map_err(|error| error.to_string())?;
    value["message"] = observed.clone();
    let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
    parse_line(&text).map_err(|error| format!("captured line is not a recorded exchange: {error}"))?;
    Ok(text)
}

/// Replay one recorded example against the real server.
///
/// Every recorded `c2s` line is sent in order after the fixture substitution
/// above; every recorded `s2c` line is compared with the next frame the server
/// emits, normalized by [`canonical_frame`]. Replies to the server's own
/// outbound requests keep the recorded `magit-lsp/outbound/N` identity by
/// mapping it to the actual request id observed in the same position, so the
/// reply always answers the real request. No sleeps are used: a phase is over
/// when its recorded frames have been received.
///
/// Frames are compared in strict order. The one pair that could look
/// independent, `view/render` and `view/keymap` for a newly opened view, is
/// ordered by the server: `send_outcome` emits every render before its keymap,
/// so the rich recording keeps that order and needs no phase grouping.
///
/// Set `MAGIT_LSP_REGENERATE_EXAMPLES` (via `make update-examples`) to capture
/// the server's current answers instead: see [`replay_example_mode`].
fn replay_example(name: &str, lines: &[Line]) -> Result<Vec<String>, String> {
    replay_example_mode(name, lines, std::env::var_os("MAGIT_LSP_REGENERATE_EXAMPLES").is_some())
}

/// Replay one recorded example, optionally capturing instead of comparing.
///
/// Strict replay (`regenerate == false`) is the default: every captured frame
/// must equal its recorded counterpart after normalization. Regeneration
/// (`regenerate == true`) still sends the recorded requests in order, enforces
/// the same frame ordering and the same post-exit checks, and still normalizes
/// roots, binary paths, run ids, outbound ids and hashes; only a divergent `s2c`
/// payload is captured through [`captured_line`], which keeps the recorded
/// envelope and refuses a changed exchange identity.
///
/// Regeneration also refreshes the client's `didChange` echo of a server edit:
/// when the recording mechanically proves the frame is that reply (same
/// document and version, and the recorded edit applied to the recorded client
/// text yields exactly the recorded echo), the echo is re-derived from the edit
/// the current server actually sent. Authored or divergent client changes are
/// left untouched, and a proven echo that cannot be re-derived is an error.
fn replay_example_mode(name: &str, lines: &[Line], regenerate: bool) -> Result<Vec<String>, String> {
    let mut regenerated = Vec::with_capacity(lines.len());
    let fixture = replay_fixture(name.strip_suffix(".jsonl").unwrap_or(name));
    let (repo, _git) = fixture.build();
    let root = repo.path().to_string_lossy().to_string();
    let binary = env!("CARGO_BIN_EXE_magit-lsp");

    let mut server = fixtures::stdio::TestServer::spawn(&[]);
    // Outbound requests observed so far, indexed by their recorded number.
    let mut outbound: Vec<Value> = Vec::new();
    // The commit run id this server generated, if the exchange starts one.
    let mut run_id: Option<String> = None;
    // Recorded and observed frames name commit hashes independently, each by
    // first-seen position on its own side.
    let mut recorded_hashes = HashNormalizer::new();
    let mut observed_hashes = HashNormalizer::new();
    // The code-action and commit recordings deliberately exclude the server's
    // dynamic `view/keymap` pushes (client display metadata, not part of the
    // recorded exchange). Only those files may skip them; a file that does
    // record one keeps it in strict order.
    let ignore_keymaps = !lines.iter().any(|line| {
        line.dir == Dir::ServerToClient
            && line.message.get("method").and_then(Value::as_str) == Some("view/keymap")
    });
    // Regeneration only: the client document each recorded frame asserts, the
    // client document this replay currently maintains, and the last server
    // edit each document received. A `None` edit means the recorded edit was
    // not a single unambiguous text edit, so no echo may bind to it.
    let mut recorded_documents: HashMap<String, RecordedDocument> = HashMap::new();
    let mut current_documents: HashMap<String, String> = HashMap::new();
    let mut pending_edits: HashMap<String, Option<ServerEdit>> = HashMap::new();

    for (index, line) in lines.iter().enumerate() {
        regenerated.push(line.raw.clone());
        let where_ = format!("{name}:{}", index + 1);
        match line.dir {
            Dir::ClientToServer => {
                let mut message = Value::Object(line.message.clone());
                if message.get("method").is_none() {
                    if let Some(recorded) = message.get("id").and_then(Value::as_str) {
                        if let Some(number) = recorded.strip_prefix("magit-lsp/outbound/") {
                            let number: usize = number
                                .parse()
                                .map_err(|error| format!("{where_}: bad outbound id: {error}"))?;
                            let actual = outbound.get(number.wrapping_sub(1)).ok_or_else(|| {
                                format!(
                                    "{where_}: reply to outbound request {number} has no observed request"
                                )
                            })?;
                            message["id"] = actual.clone();
                        }
                    }
                }
                if regenerate {
                    if let Some(update) = recorded_document_update(&line.message) {
                        let opened =
                            line.message.get("method").and_then(Value::as_str) == Some("textDocument/didOpen");
                        let probe = expand_client_frame(&message, &root, run_id.as_deref());
                        let current_uri = probe["params"]["textDocument"]["uri"]
                            .as_str()
                            .ok_or_else(|| format!("{where_}: client document update has no uri"))?
                            .to_owned();
                        if opened {
                            current_documents.insert(current_uri, update.text.clone());
                            pending_edits.remove(&update.uri);
                        } else {
                            let current_text = current_documents.get(&current_uri).map(String::as_str);
                            let refreshed = refreshed_echo(
                                &update.uri,
                                update.version,
                                &update.text,
                                current_text,
                                &recorded_documents,
                                &pending_edits,
                            )
                            .map_err(|error| format!("{where_}: {error}"))?;
                            if let Some(refreshed) = &refreshed {
                                message["params"]["contentChanges"][0]["text"] = Value::String(refreshed.clone());
                                regenerated[index] = captured_line(line, &message)
                                    .map_err(|error| format!("{where_}: {error}"))?;
                            }
                            let sent_text = refreshed.unwrap_or_else(|| update.text.clone());
                            current_documents.insert(current_uri, sent_text);
                            pending_edits.remove(&update.uri);
                        }
                        recorded_documents.insert(
                            update.uri,
                            RecordedDocument {
                                version: update.version,
                                text: update.text,
                            },
                        );
                    }
                }
                server.send(expand_client_frame(&message, &root, run_id.as_deref()));
            }
            Dir::ServerToClient => {
                let mut actual = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| server.recv()))
                    .map_err(|panic| {
                        let detail = panic
                            .downcast_ref::<String>()
                            .map(String::as_str)
                            .or_else(|| panic.downcast_ref::<&str>().copied())
                            .unwrap_or("server receive panicked");
                        format!("{where_}: {detail}")
                    })?;
                while ignore_keymaps
                    && actual.get("method").and_then(Value::as_str) == Some("view/keymap")
                {
                    actual = server.recv();
                }
                if run_id.is_none() {
                    run_id = observed_run_id(&actual);
                }
                if actual.get("method").is_some() && actual.get("id").is_some() {
                    outbound.push(actual["id"].clone());
                }
                let observed = canonical_frame(&actual, &root, binary, &mut observed_hashes);
                let expected = canonical_frame(
                    &Value::Object(line.message.clone()),
                    RECORDED_ROOT,
                    RECORDED_BINARY,
                    &mut recorded_hashes,
                );
                if observed != expected {
                    if !regenerate {
                        let (path, expected_value, observed_value) = first_difference(&expected, &observed)
                            .unwrap_or_else(|| ("<unknown>".to_owned(), None, None));
                        return Err(format!(
                            "{where_}: recorded s2c frame mismatch at {path}\n  expected: {}\n  observed: {}\n  observed frame: {}",
                            json_excerpt(expected_value), json_excerpt(observed_value), json_excerpt(Some(&observed))
                        ));
                    }
                    // Regeneration captures the server's current answer instead
                    // of blessing the stale recording, but only for the same
                    // exchange: a different method or request/response id is a
                    // replay failure, not a capture.
                    regenerated[index] =
                        captured_line(line, &observed).map_err(|error| format!("{where_}: {error}"))?;
                }
                if regenerate && line.message.get("method").and_then(Value::as_str) == Some("workspace/applyEdit") {
                    let recorded = Value::Object(line.message.clone());
                    let pending = match (sole_text_edit(&recorded), sole_text_edit(&actual)) {
                        (
                            Some((version, recorded_range, recorded_text)),
                            Some((_, observed_range, observed_text)),
                        ) => Some(ServerEdit {
                            version,
                            recorded_range,
                            recorded_text,
                            observed_range,
                            observed_text,
                        }),
                        _ => None,
                    };
                    for uri in edited_document_uris(&line.message) {
                        pending_edits.insert(uri, pending.clone());
                    }
                }
            }
        }
    }

    let (status, extra) = server.finish_with_messages(Duration::from_secs(10));
    if !status.success() {
        return Err(format!("{name}: server exited with {status:?}"));
    }
    if !extra.is_empty() {
        return Err(format!(
            "{name}: {} unrecorded server frame(s) after exit: {extra:?}",
            extra.len()
        ));
    }
    Ok(regenerated)
}

/// Every maintained logical recording, in replay/regeneration order.
const MAINTAINED_RECORDINGS: [&str; 6] = [
    "plain-open.jsonl",
    "rich-lifecycle.jsonl",
    "plain-code-action.jsonl",
    "plain-document-prompt.jsonl",
    "commit-edit-abort.jsonl",
    "commit-edit-confirm.jsonl",
];

/// Artifact files that make up one maintained logical recording, in order.
///
/// The commit-edit exchanges stay split across their two existing fragments;
/// every other recording is one monolithic file named after itself.
fn regeneration_artifacts(name: &str) -> Result<Vec<&'static str>, String> {
    let artifacts: &[&str] = match name {
        "commit-edit-abort.jsonl" => &["commit-edit-abort-01.jsonl", "commit-edit-abort-02.jsonl"],
        "commit-edit-confirm.jsonl" => &["commit-edit-confirm-01.jsonl", "commit-edit-confirm-02.jsonl"],
        "plain-open.jsonl" => &["plain-open.jsonl"],
        "rich-lifecycle.jsonl" => &["rich-lifecycle.jsonl"],
        "plain-code-action.jsonl" => &["plain-code-action.jsonl"],
        "plain-document-prompt.jsonl" => &["plain-document-prompt.jsonl"],
        _ => return Err(format!("refusing to regenerate unrelated recording {name}")),
    };
    Ok(artifacts.to_vec())
}

/// Split regenerated lines across the exact artifacts that recorded them.
fn regenerated_fragments(name: &str, lines: &[String]) -> Result<Vec<(std::path::PathBuf, String)>, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(EXAMPLES_DIR);
    let artifacts = regeneration_artifacts(name)?;
    let mut fragments = Vec::new();
    let mut offset = 0;
    for artifact in &artifacts {
        let path = root.join(artifact);
        let original = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        // Fragmented recordings keep their recorded line boundaries; a
        // monolithic recording owns every remaining line.
        let end = if artifacts.len() == 1 { lines.len() } else { offset + original.lines().count() };
        let content = lines.get(offset..end).ok_or("missing fragment lines")?.join("\n") + "\n";
        if content.len() >= 100_000 { return Err(format!("fragment too large: {} bytes", content.len())); }
        fragments.push((path, content));
        offset = end;
    }
    if offset != lines.len() { return Err("unrecorded fragment lines".into()); }
    Ok(fragments)
}

/// Capture preserves source bytes and fragment boundaries, and rejects overflow.
#[test]
fn capture_preserves_fragments_and_rejects_overflow() {
    for name in ["commit-edit-abort.jsonl", "commit-edit-confirm.jsonl"] {
        let mut lines: Vec<String> = example_lines(name).into_iter().map(|line| line.raw).collect();
        for (path, content) in regenerated_fragments(name, &lines).unwrap() {
            assert_eq!(content, fs::read_to_string(path).unwrap(), "unchanged source bytes and fragment boundaries");
        }
        lines[0] = "x".repeat(100_000);
        assert!(regenerated_fragments(name, &lines).unwrap_err().contains("fragment too large"));
    }
    assert!(regenerated_fragments("unrelated.jsonl", &[]).is_err());
}

/// Regeneration maps every maintained recording to its exact artifact files.
#[test]
fn regeneration_keeps_fragment_mapping_and_rejects_unrelated_names() {
    for name in MAINTAINED_RECORDINGS {
        let lines: Vec<String> = example_lines(name).into_iter().map(|line| line.raw).collect();
        let fragments = regenerated_fragments(name, &lines).unwrap_or_else(|error| panic!("{name}: {error}"));
        let artifacts = fragments.iter().map(|(path, _)| path.file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert_eq!(artifacts, regeneration_artifacts(name).unwrap(), "{name} maps to its own artifacts");
        assert_eq!(fragments.iter().map(|(_, content)| content.lines().count()).sum::<usize>(), lines.len(), "{name} covers every recorded line exactly once");
        for (path, content) in fragments {
            assert_eq!(content, fs::read_to_string(&path).unwrap(), "{name} keeps {} byte-identical", path.display());
        }
    }
    assert_eq!(regeneration_artifacts("commit-edit-abort.jsonl").unwrap(), ["commit-edit-abort-01.jsonl", "commit-edit-abort-02.jsonl"]);
    assert_eq!(regeneration_artifacts("plain-open.jsonl").unwrap(), ["plain-open.jsonl"]);
    for unrelated in ["example.jsonl", "plain-open-03.jsonl", "commit-edit-abort-01.jsonl"] {
        let error = regenerated_fragments(unrelated, &[]).expect_err("unrelated recording must be refused");
        assert!(error.contains("refusing to regenerate unrelated recording"), "{unrelated}: {error}");
    }
}

/// Replay every example file, Plain and rich, against a real server and fixture.
#[test]
fn plain_examples_replay_against_the_real_binary() {
    let mut fragments = Vec::new();
    for name in MAINTAINED_RECORDINGS {
        let lines = example_lines(name);
        let captured = replay_example(name, &lines).unwrap_or_else(|error| panic!("{error}"));
        if std::env::var_os("MAGIT_LSP_REGENERATE_EXAMPLES").is_some() {
            fragments.extend(regenerated_fragments(name, &captured).unwrap_or_else(|error| panic!("{error}")));
        }
    }
    // Validate every exchange and fragment before writing any recording.
    for (path, content) in fragments {
        fs::write(path, content).expect("write validated capture");
    }
}

/// Neither commit recording may sort away a changed code-action order.
#[test]
fn commit_edit_action_order_is_strict() {
    for name in ["commit-edit-abort.jsonl", "commit-edit-confirm.jsonl"] {
        let mut lines = example_lines(name);
        let response = lines.iter_mut().find(|line| line.dir == Dir::ServerToClient && line.message.get("result").is_some_and(Value::is_array)).unwrap();
        response.message["result"].as_array_mut().unwrap().swap(0, 1);
        let error = replay_example(name, &lines).expect_err("reordered actions must fail normal replay");
        assert!(error.contains("recorded s2c frame mismatch at $.result[0]"), "{error}");
    }
}

/// A deliberately divergent expectation must make the replay fail.
#[test]
fn a_divergent_example_expectation_fails_the_replay() {
    let mut lines = example_lines("plain-code-action.jsonl");
    let render = lines
        .iter_mut()
        .find(|line| {
            line.dir == Dir::ServerToClient
                && line.message.get("method").and_then(Value::as_str) == Some("workspace/applyEdit")
        })
        .expect("the recorded status render");
    render.message["params"]["edit"]["documentChanges"][0]["edits"][0]["newText"] =
        Value::String("Head: not the recorded render".to_owned());
    let error = replay_example("plain-code-action.jsonl", &lines)
        .expect_err("a divergent recorded frame must fail the replay");
    assert!(
        error.contains("recorded s2c frame mismatch") && error.contains("not the recorded render"),
        "the mismatch must be reported: {error}"
    );
}

/// Regeneration captures the server's current answers and they replay strictly;
/// normal replay still rejects a divergent render.
#[test]
fn regeneration_captures_a_changed_render_and_strict_replay_rejects_it() {
    // The committed recording is current, so a stale payload is constructed
    // here instead of borrowed from the artifact: the test state it needs must
    // not depend on whether a recording happens to lag the server.
    let current = example_lines("plain-code-action.jsonl");
    let index = current
        .iter()
        .position(|line| {
            line.dir == Dir::ServerToClient
                && line.message.get("method").and_then(Value::as_str) == Some("workspace/applyEdit")
        })
        .expect("the recorded status render");
    replay_example_mode("plain-code-action.jsonl", &current, false)
        .expect("the committed recording replays strictly before it is perturbed");
    let mut stale: Vec<Line> = current
        .iter()
        .map(|line| parse_line(&line.raw).expect("recorded lines reparse"))
        .collect();
    stale[index].message["params"]["edit"]["documentChanges"][0]["edits"][0]["newText"] =
        Value::String("Head: not the recorded render".to_owned());
    let error = replay_example_mode("plain-code-action.jsonl", &stale, false)
        .expect_err("strict replay must reject a stale recording");
    assert!(error.contains("recorded s2c frame mismatch"), "{error}");
    let captured = replay_example_mode("plain-code-action.jsonl", &stale, true)
        .expect("regeneration captures the current frames");
    assert_eq!(captured.len(), stale.len(), "regeneration keeps every recorded line");
    let mut reparsed: Vec<Line> = captured.iter().map(|raw| parse_line(raw).expect("captured lines parse")).collect();
    replay_example_mode("plain-code-action.jsonl", &reparsed, false)
        .expect("captured frames replay strictly against the same server");
    assert_ne!(
        reparsed[index].message["params"]["edit"]["documentChanges"][0]["edits"][0]["newText"],
        stale[index].message["params"]["edit"]["documentChanges"][0]["edits"][0]["newText"],
        "regeneration replaced the stale render with the server's answer"
    );
    reparsed[index].message["params"]["edit"]["documentChanges"][0]["edits"][0]["newText"] =
        Value::String("Head: not the recorded render".to_owned());
    let error = replay_example_mode("plain-code-action.jsonl", &reparsed, false)
        .expect_err("strict replay must reject the divergent render");
    assert!(
        error.contains("recorded s2c frame mismatch") && error.contains("not the recorded render"),
        "the mismatch must be reported: {error}"
    );
}

/// Regeneration captures the current code-action registry and the capture
/// replays strictly; normal replay still rejects a changed action.
#[test]
fn regeneration_captures_registry_changes_and_strict_replay_rejects_them() {
    // Same rule as the render case: build the stale registry here so the test
    // does not require the committed recording to lag the server.
    let current = example_lines("commit-edit-abort.jsonl");
    let index = current
        .iter()
        .position(|line| line.dir == Dir::ServerToClient && line.message.get("result").is_some_and(Value::is_array))
        .expect("the recorded code-action response");
    replay_example_mode("commit-edit-abort.jsonl", &current, false)
        .expect("the committed recording replays strictly before it is perturbed");
    let mut stale: Vec<Line> = current
        .iter()
        .map(|line| parse_line(&line.raw).expect("recorded lines reparse"))
        .collect();
    stale[index].message["result"][0]["title"] = Value::String("Stale: not the current registry".to_owned());
    let error = replay_example_mode("commit-edit-abort.jsonl", &stale, false)
        .expect_err("strict replay must reject the stale recording");
    assert!(error.contains("recorded s2c frame mismatch"), "{error}");
    let captured = replay_example_mode("commit-edit-abort.jsonl", &stale, true)
        .expect("regeneration captures the current code actions");
    let mut reparsed: Vec<Line> = captured.iter().map(|raw| parse_line(raw).expect("captured lines parse")).collect();
    replay_example_mode("commit-edit-abort.jsonl", &reparsed, false)
        .expect("captured frames replay strictly against the same server");
    assert_ne!(
        reparsed[index].message["result"][0]["title"],
        stale[index].message["result"][0]["title"],
        "regeneration replaced the stale action with the current registry entry"
    );
    reparsed[index].message["result"][0]["title"] = Value::String("Drifted: not the current registry".to_owned());
    let error = replay_example_mode("commit-edit-abort.jsonl", &reparsed, false)
        .expect_err("strict replay must reject the changed action");
    assert!(error.contains("recorded s2c frame mismatch at $.result[0].title"), "{error}");
}

/// Regeneration refuses a captured frame that changes the recorded exchange.
#[test]
fn regeneration_refuses_a_changed_exchange_identity() {
    let lines = example_lines("plain-open.jsonl");
    let request = lines
        .iter()
        .find(|line| {
            line.dir == Dir::ServerToClient
                && line.message.get("method").is_some()
                && line.message.get("id").is_some()
        })
        .expect("the recorded outbound request");
    let mut observed = Value::Object(request.message.clone());
    observed["params"] = serde_json::json!({"drifted": true});
    let captured = captured_line(request, &observed).expect("a payload change is captured");
    assert_eq!(parse_line(&captured).expect("the captured line parses").dir, Dir::ServerToClient);
    let method = observed["method"].clone();
    observed["method"] = Value::String("workspace/notApplyEdit".to_owned());
    let error = captured_line(request, &observed).expect_err("a renamed method is refused");
    assert!(error.contains("identity changed"), "{error}");
    observed["method"] = method;
    observed["id"] = Value::String("magit-lsp/outbound/99".to_owned());
    let error = captured_line(request, &observed).expect_err("a changed id is refused");
    assert!(error.contains("identity changed"), "{error}");
}

/// Regeneration refreshes a client echo only when the recording mechanically
/// proves it is the result of the preceding server edit, and derives the new
/// text from the edit the current server actually sent.
#[test]
fn regeneration_echo_refreshes_a_bound_client_echo_from_the_observed_edit() {
    let name = "plain-code-action.jsonl";
    let current = example_lines(name);
    let render = current
        .iter()
        .position(|line| {
            line.dir == Dir::ServerToClient
                && line.message.get("method").and_then(Value::as_str) == Some("workspace/applyEdit")
        })
        .expect("the recorded status render");
    let open = current
        .iter()
        .position(|line| {
            line.dir == Dir::ClientToServer
                && line.message.get("method").and_then(Value::as_str) == Some("textDocument/didOpen")
        })
        .expect("the recorded document open");
    let echo = current
        .iter()
        .position(|line| {
            line.dir == Dir::ClientToServer
                && line.message.get("method").and_then(Value::as_str) == Some("textDocument/didChange")
        })
        .expect("the recorded client echo");
    // Build a coherent stale recording: every server render and every client
    // echo names the old head, so each echo is still provably the recorded
    // edit applied to the recorded client text, but none matches the current
    // server. Edits start at line 2 or later, so the line 0 change is safe.
    let mut stale: Vec<Line> = current
        .iter()
        .map(|line| parse_line(&line.raw).expect("recorded lines reparse"))
        .collect();
    let mut replaced = 0;
    for line in stale.iter_mut() {
        match line.message.get("method").and_then(Value::as_str) {
            Some("workspace/applyEdit") => {
                if let Some(changes) = line.message["params"]["edit"]["documentChanges"].as_array_mut() {
                    for change in changes {
                        if let Some(edits) = change["edits"].as_array_mut() {
                            for edit in edits {
                                if let Some(text) = edit["newText"].as_str() {
                                    if text.contains("Head: main") {
                                        edit["newText"] = Value::String(text.replace("Head: main", "Head: stale"));
                                        replaced += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Some("textDocument/didChange") => {
                let is_status = line.message["params"]["textDocument"]["uri"]
                    .as_str()
                    .is_some_and(|uri| uri.ends_with("/.magit"));
                if is_status {
                    if let Some(text) = line.message["params"]["contentChanges"][0]["text"].as_str() {
                        line.message["params"]["contentChanges"][0]["text"] =
                            Value::String(text.replace("Head: main", "Head: stale"));
                        replaced += 1;
                    }
                }
            }
            _ => {}
        }
    }
    assert!(replaced >= 3, "the stale recording keeps its renders and echoes coherent");
    let opened = stale[open].message["params"]["textDocument"]["text"]
        .as_str()
        .expect("the recorded opened text")
        .to_owned();
    let stale_text = stale[render].message["params"]["edit"]["documentChanges"][0]["edits"][0]["newText"]
        .as_str()
        .expect("the recorded render text")
        .to_owned();
    let stale_range = stale[render].message["params"]["edit"]["documentChanges"][0]["edits"][0]["range"].clone();
    let stale_echo = stale[echo].message["params"]["contentChanges"][0]["text"]
        .as_str()
        .expect("the recorded echo text")
        .to_owned();
    assert_eq!(
        apply_text_edit(&opened, &stale_range, &stale_text).expect("the stale edit applies"),
        stale_echo,
        "the synthetic recording stays a mechanically coherent echo"
    );

    let error = replay_example_mode(name, &stale, false)
        .expect_err("strict replay must reject the coherent stale echo");
    assert!(error.contains("recorded s2c frame mismatch") && error.contains("Head: stale"), "{error}");

    let captured = replay_example_mode(name, &stale, true)
        .expect("regeneration refreshes the bound echo and captures the rest");
    let reparsed: Vec<Line> = captured.iter().map(|raw| parse_line(raw).expect("captured lines parse")).collect();
    replay_example_mode(name, &reparsed, false)
        .expect("the refreshed echo and the captured frames replay strictly");

    let observed_text = reparsed[render].message["params"]["edit"]["documentChanges"][0]["edits"][0]["newText"]
        .as_str()
        .expect("the captured render text");
    let observed_range = reparsed[render].message["params"]["edit"]["documentChanges"][0]["edits"][0]["range"].clone();
    let expected_echo = apply_text_edit(&opened, &observed_range, observed_text).expect("the observed edit applies");
    let observed_echo = reparsed[echo].message["params"]["contentChanges"][0]["text"]
        .as_str()
        .expect("the refreshed echo text");
    assert_ne!(observed_echo, stale_echo, "regeneration replaced the stale echo");
    assert_eq!(observed_echo, expected_echo, "the echo is the observed server edit applied to the opened text");
    assert!(observed_echo.contains("Keys: off (built-in)"), "the echo carries the current render");
}

/// Regeneration never rewrites authored client edits: a commit message the
/// client typed itself stays exactly as recorded.
#[test]
fn regeneration_echo_leaves_authored_client_edits_unchanged() {
    let name = "commit-edit-confirm.jsonl";
    let captured = replay_example_mode(name, &example_lines(name), true)
        .expect("regeneration captures the commit example");
    let reparsed: Vec<Line> = captured.iter().map(|raw| parse_line(raw).expect("captured lines parse")).collect();
    replay_example_mode(name, &reparsed, false).expect("the captured commit example replays strictly");
    let authored = reparsed
        .iter()
        .find_map(|line| {
            let is_authored = line.dir == Dir::ClientToServer
                && line.message.get("method").and_then(Value::as_str) == Some("textDocument/didChange")
                && line.message["params"]["textDocument"]["uri"]
                    .as_str()
                    .is_some_and(|uri| uri.ends_with(".git/COMMIT_EDITMSG"));
            is_authored.then(|| line.message["params"]["contentChanges"][0]["text"].as_str().unwrap().to_owned())
        })
        .expect("the authored commit-message change");
    assert_eq!(authored, "Recorded commit body\n", "an authored change is never rewritten");
}

/// Regeneration refuses to rewrite a divergent echo, and refuses a captured
/// frame whose exchange identity changed instead of accepting it.
#[test]
fn regeneration_echo_refuses_divergent_edits_and_identity_changes() {
    let name = "plain-code-action.jsonl";
    let current = example_lines(name);
    let echo = current
        .iter()
        .position(|line| {
            line.dir == Dir::ClientToServer
                && line.message.get("method").and_then(Value::as_str) == Some("textDocument/didChange")
        })
        .expect("the recorded client echo");
    // A didChange that does not equal the recorded edit applied to the recorded
    // client text is authored input, so regeneration must not rewrite it.
    let mut divergent: Vec<Line> = current
        .iter()
        .map(|line| parse_line(&line.raw).expect("recorded lines reparse"))
        .collect();
    let divergent_text = divergent[echo].message["params"]["contentChanges"][0]["text"]
        .as_str()
        .expect("the recorded echo text")
        .replace("Head: main", "Head: authored");
    divergent[echo].message["params"]["contentChanges"][0]["text"] = Value::String(divergent_text);
    let error = replay_example_mode(name, &divergent, true)
        .expect_err("a divergent echo must not be rewritten or accepted");
    assert!(error.contains("identity changed"), "the server's correction is refused: {error}");

    // Regeneration must still refuse a recorded server exchange whose observed
    // identity differs, even when the payload could be captured.
    let render = current
        .iter()
        .position(|line| {
            line.dir == Dir::ServerToClient
                && line.message.get("method").and_then(Value::as_str) == Some("workspace/applyEdit")
        })
        .expect("the recorded status render");
    let mut mismatched: Vec<Line> = current
        .iter()
        .map(|line| parse_line(&line.raw).expect("recorded lines reparse"))
        .collect();
    mismatched[render].message.remove("method");
    let error = replay_example_mode(name, &mismatched, true)
        .expect_err("a changed exchange identity must be refused");
    assert!(error.contains("identity changed"), "{error}");
}

/// A divergent rich expectation must make the rich replay fail.
#[test]
fn a_divergent_rich_expectation_fails_the_replay() {
    let mut lines = example_lines("rich-lifecycle.jsonl");
    let render = lines
        .iter_mut()
        .find(|line| {
            line.dir == Dir::ServerToClient
                && line.message.get("method").and_then(Value::as_str) == Some("view/render")
                && line.message["params"]["version"] == 1
        })
        .expect("the recorded initial rich render");
    render.message["params"]["text"][0] = Value::String("Head: not the recorded rich render".to_owned());
    let error = replay_example("rich-lifecycle.jsonl", &lines)
        .expect_err("a divergent rich frame must fail the replay");
    assert!(
        error.contains("recorded s2c frame mismatch")
            && error.contains("not the recorded rich render"),
        "the rich mismatch must be reported: {error}"
    );
}

/// Commit/object hashes become `hash-N` by first-seen position on each side;
/// 64-hex content hashes stay exact.
#[test]
fn commit_hashes_normalize_by_first_seen_position() {
    let first = "1111111111111111111111111111111111111111";
    let second = "2222222222222222222222222222222222222222";
    let content = "a".repeat(64);
    let frame = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 9,
        "result": {"text": [
            format!("commit {first}"),
            format!("parent {second}"),
            format!("patch {content}"),
        ]},
    });
    let mut recorded = HashNormalizer::new();
    let mut observed = HashNormalizer::new();
    let expected = canonical_frame(&frame, RECORDED_ROOT, RECORDED_BINARY, &mut recorded);
    let actual = canonical_frame(&frame, RECORDED_ROOT, RECORDED_BINARY, &mut observed);
    assert_eq!(actual, expected);
    assert_eq!(actual["result"]["text"][0], "commit hash-1");
    assert_eq!(actual["result"]["text"][1], "parent hash-2");
    assert_eq!(actual["result"]["text"][2], format!("patch {content}"));

    // The maps are independent: the observed side numbers from `hash-1` even
    // though the recorded side already saw this hash as `hash-2`.
    let only_second = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 9,
        "result": {"text": [format!("parent {second}")]},
    });
    let mut other = HashNormalizer::new();
    let renumbered = canonical_frame(&only_second, RECORDED_ROOT, RECORDED_BINARY, &mut other);
    assert_eq!(renumbered["result"]["text"][0], "parent hash-1");
}

#[test]
fn mismatch_message_reports_differing_values_and_path() {
    let expected = serde_json::json!({"result":[{"command":"a"},{"command":"b"}]});
    let observed = serde_json::json!({"result":[{"command":"a"},{"command":"c"}]});
    let (path, expected_value, observed_value) = first_difference(&expected, &observed).unwrap();
    let message = format!("at {path} expected: {} observed: {}", json_excerpt(expected_value), json_excerpt(observed_value));
    assert!(message.contains("$.result[1].command"));
    assert!(message.contains(r#"expected: "\"b\"""#));
    assert!(message.contains(r#"observed: "\"c\"""#));
}

#[test]
fn json_excerpt_bounds_long_values() {
    let value = Value::String("x".repeat(500));
    let excerpt = json_excerpt(Some(&value));
    assert_eq!(excerpt.split("...").next().unwrap().matches('x').count(), 239);
    assert!(excerpt.contains("500 chars total"));
    assert!(!excerpt.contains(&"x".repeat(500)));
}

#[test]
fn first_difference_reports_missing_key() {
    let expected = serde_json::json!({"result":{"command":"a"}});
    let observed = serde_json::json!({"result":{}});
    let (path, expected_value, observed_value) = first_difference(&expected, &observed).unwrap();
    assert_eq!(path, "$.result.command");
    assert!(expected_value.is_some());
    assert_eq!(json_excerpt(observed_value), "<missing>");
}
