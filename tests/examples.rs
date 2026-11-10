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
//! `magit-lsp/outbound/N` ids are substituted in the client's replies, and
//! opaque commit-run ids become `run-1`. Versions, order and payloads are not
//! relaxed: an unexpected frame or a divergent expectation fails.

#[path = "common/mod.rs"]
mod fixtures;

use fixtures::schema::{read_schema, schema_errors};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
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

/// Parse one committed example file into its validated line shapes.
fn example_lines(name: &str) -> Vec<Line> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join(EXAMPLES_DIR).join(name);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
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
        "plain-open" => RepoFixture::new()
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

/// The recorded comparison form of a frame.
///
/// The current server omits `jsonrpc` on outbound frames, recorded runs used a
/// temporary repository and a checkout-local binary path, and commit run ids are
/// opaque hashes. The recordings normalized those by adding the member and by
/// the `/repo`, binary-path and `run-1` placeholders. Versions and every other
/// value stay exact.
fn canonical_frame(message: &Value, root: &str, binary: &str) -> Value {
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
    let text = serde_json::to_string(&value).expect("serialise observed frame");
    let text = text
        .replace(&format!("file://{root}"), &format!("file://{RECORDED_ROOT}"))
        .replace(root, RECORDED_ROOT)
        .replace(binary, RECORDED_BINARY);
    serde_json::from_str(&text).expect("parse canonical frame")
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
fn replay_example(name: &str, lines: &[Line]) -> Result<(), String> {
    let fixture = replay_fixture(name.strip_suffix(".jsonl").unwrap_or(name));
    let (repo, _git) = fixture.build();
    let root = repo.path().to_string_lossy().to_string();
    let binary = env!("CARGO_BIN_EXE_magit-lsp");

    let mut server = fixtures::stdio::TestServer::spawn(&[]);
    // Outbound requests observed so far, indexed by their recorded number.
    let mut outbound: Vec<Value> = Vec::new();
    // The commit run id this server generated, if the exchange starts one.
    let mut run_id: Option<String> = None;
    // The code-action and commit recordings deliberately exclude the server's
    // dynamic `view/keymap` pushes (client display metadata, not part of the
    // recorded exchange). Only those files may skip them; a file that does
    // record one keeps it in strict order.
    let ignore_keymaps = !lines.iter().any(|line| {
        line.dir == Dir::ServerToClient
            && line.message.get("method").and_then(Value::as_str) == Some("view/keymap")
    });

    for (index, line) in lines.iter().enumerate() {
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
                server.send(expand_client_frame(&message, &root, run_id.as_deref()));
            }
            Dir::ServerToClient => {
                let mut actual = server.recv();
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
                let observed = canonical_frame(&actual, &root, binary);
                let expected = Value::Object(line.message.clone());
                if observed != expected {
                    return Err(format!(
                        "{where_}: recorded s2c frame mismatch\n  expected: {expected}\n  observed: {observed}"
                    ));
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
    Ok(())
}

/// Replay every Plain example file against a real server and fixture.
#[test]
fn plain_examples_replay_against_the_real_binary() {
    for name in [
        "plain-open.jsonl",
        "plain-code-action.jsonl",
        "plain-document-prompt.jsonl",
        "commit-edit-abort.jsonl",
        "commit-edit-confirm.jsonl",
    ] {
        let lines = example_lines(name);
        replay_example(name, &lines).unwrap_or_else(|error| panic!("{error}"));
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
