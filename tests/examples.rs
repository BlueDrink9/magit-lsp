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

#[path = "common/editor_model/mod.rs"]
mod editor_model;
#[path = "../scripts/example_recordings.rs"]
mod example_recordings;
#[path = "common/fake_editor.rs"]
mod fake_editor;
#[path = "common/mod.rs"]
mod fixtures;

use fake_editor::{FakeEditor, Profile};
use fixtures::schema::{read_schema, schema_errors};
use magit_lsp::file_uri::{path_from_uri, uri_from_path};
use serde_json::{json, Map, Value};
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
    let object = value.as_object().ok_or("line is not a JSON object")?;
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
        let request = pending.remove(&(request_dir, key.clone())).ok_or_else(|| {
            format!(
                "response id {key} has no preceding {} request",
                request_dir.as_str()
            )
        })?;
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
fn validate_exchange(path: &Path, text: &str, catalog: &Catalog) -> Result<usize, String> {
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
        let text =
            String::from_utf8(bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let count =
            validate_exchange(&path, &text, &catalog).unwrap_or_else(|error| panic!("{error}"));
        assert!(count > 0, "{} has no lines", path.display());
    }
}

#[test]
fn numbered_large_recording_fragments_load_as_the_original_logical_recording() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let examples = example_recordings::recordings(&root.join(EXAMPLES_DIR)).expect("examples");
    let dir = root.join(EXAMPLES_DIR);
    for (logical, fragments) in [
        (
            "commit-edit-confirm.jsonl",
            [
                "commit-edit-confirm-01.jsonl",
                "commit-edit-confirm-02.jsonl",
            ],
        ),
        (
            "commit-edit-abort.jsonl",
            ["commit-edit-abort-01.jsonl", "commit-edit-abort-02.jsonl"],
        ),
    ] {
        let matches: Vec<_> = examples
            .iter()
            .filter(|(name, _)| name == logical)
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "fragments expose one original logical name"
        );
        let expected = fragments
            .iter()
            .map(|name| fs::read(dir.join(name)).unwrap())
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(
            matches[0].1.as_slice(),
            expected.as_slice(),
            "ordered fragments form the logical bytes"
        );
        assert!(
            matches[0].1.len() > 100_000,
            "validation sees the whole recording"
        );
    }
}

#[test]
fn plain_open_example_records_initialize_and_a_plain_document_open() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join(EXAMPLES_DIR).join("plain-open.jsonl");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
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
            && line.message.get("method").and_then(Value::as_str) == Some("textDocument/didOpen")
    }));
    assert!(lines.iter().any(|line| {
        line.dir == Dir::ServerToClient
            && line.message.get("method").and_then(Value::as_str) == Some("workspace/applyEdit")
    }));
    assert!(lines.iter().any(|line| {
        line.dir == Dir::ClientToServer
            && line.message.get("method").and_then(Value::as_str) == Some("textDocument/didChange")
    }));
    assert!(lines.iter().all(|line| line.source == "recorded"));
}

/// Parse one committed example file into its validated line shapes.
fn example_lines(name: &str) -> Vec<Line> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join(EXAMPLES_DIR);
    let text = example_recordings::recordings(&path)
        .unwrap()
        .into_iter()
        .find(|(logical, _)| logical == name)
        .unwrap_or_else(|| panic!("missing example {name}"));
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
        effects
            .iter()
            .any(|effect| effect["descriptor"]["kind"] == "prompt"),
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
                && line.message.get("method").and_then(Value::as_str) == Some("workspace/applyEdit")
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
                && line.message.get("method").and_then(Value::as_str) == Some("window/showDocument")
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
            line.message
                .get("result")?
                .as_array()?
                .iter()
                .find_map(|action| {
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

/// An explicit status refresh while a commit draft is pending renders its row.
///
/// Template delivery no longer pushes a detached status render, so this guards
/// the behavior that deletion must preserve: pending-draft rows come from the
/// session's registry lookup on every explicit render.
///
/// A scheduling red against the removed detached refresh is not reproducible
/// deterministically: the extra frame came from an OS thread racing the client
/// handshake, and forcing that order would need sleeps or production hooks.
#[cfg(unix)]
#[test]
fn explicit_status_refresh_while_a_commit_draft_is_pending_renders_its_row() {
    let (repo, _git) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let root = magit_lsp::git::GitRepo::open(repo.path())
        .expect("open repository")
        .root()
        .to_string_lossy()
        .to_string();
    let owner = "pending-row-refresh";
    let mut session =
        magit_lsp::session::Session::new_with_draft_owner(root.clone(), Some(owner.to_owned()));
    session
        .open(
            "v1",
            magit_lsp::view::ViewKind::Status,
            magit_lsp::view::Placement::Main,
            fixtures::STATUS_VIEWPORT,
        )
        .expect("open the status view");
    assert!(
        !session
            .rendered_text("v1")
            .expect("status render")
            .contains(".git/COMMIT_EDITMSG"),
        "no pending row exists before the draft"
    );

    let run_id = magit_lsp::editor::registry()
        .create(&root, owner, magit_lsp::editor::DraftKind::CommitMessage)
        .expect("create a pending commit draft");
    let pending = magit_lsp::editor::registry().pending_for(owner, &root);
    assert_eq!(
        pending.len(),
        1,
        "one pending draft for this owner and repo"
    );
    let pending_uri = pending[0].uri.clone();

    let render = session
        .refresh("v1")
        .expect("refresh the status view")
        .expect("the pending row changes the status render");
    let text = render
        .text
        .as_deref()
        .map(|lines| lines.join("\n"))
        .or_else(|| session.rendered_text("v1"))
        .expect("status render text");
    assert!(
        text.lines().any(|line| line.trim() == pending_uri),
        "the explicit refresh renders the pending draft row {pending_uri}: {text}"
    );

    assert!(
        magit_lsp::editor::registry().resolve(&run_id, magit_lsp::editor::Reply::Abort),
        "the pending draft is cleaned up"
    );
    session.refresh("v1").expect("refresh after the abort");
    assert!(
        !session
            .rendered_text("v1")
            .expect("status render")
            .contains(&pending_uri),
        "the row disappears once the draft is no longer pending"
    );
}

#[test]
fn rich_lifecycle_example_pins_the_progress_and_render_order() {
    let lines = example_lines("rich-lifecycle.jsonl");
    assert!(lines.iter().all(|line| line.source == "recorded"));

    let method =
        |line: &Line, name: &str| line.message.get("method").and_then(Value::as_str) == Some(name);
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
    let loading_render = index(&|line| {
        line.dir == Dir::ServerToClient
            && method(line, "view/render")
            && line.message["params"]["version"] == 1
    });
    assert_eq!(
        lines[loading_render].message["params"]["text"],
        json!(["Head: main", "Loading repository status…"]),
        "the initial render carries the discovered branch above the loading placeholder"
    );
    let fresh_render = index(&|line| {
        line.dir == Dir::ServerToClient
            && method(line, "view/render")
            && line.message["params"]["version"] == 2
    });
    assert!(
        loading_render < fresh_render,
        "the fresh status render follows the loading placeholder"
    );
    assert!(
        lines[fresh_render].message["params"]
            .get("baseVersion")
            .is_none()
            && lines[fresh_render].message["params"].get("edits").is_none(),
        "the fresh render is a full replace: no render was acknowledged before it"
    );
    assert_eq!(
        lines[fresh_render].message["params"]["text"][0], "Head: main",
        "the fresh render carries the acquired status"
    );
    let loading_ack = index(&|line| {
        line.dir == Dir::ClientToServer
            && method(line, "view/rendered")
            && line.message["params"]["version"] == 1
    });
    let fresh_ack = index(&|line| {
        line.dir == Dir::ClientToServer
            && method(line, "view/rendered")
            && line.message["params"]["version"] == 2
    });
    let key = index(&|line| line.dir == Dir::ClientToServer && method(line, "view/key"));
    assert!(
        fresh_render < loading_ack && loading_ack < fresh_ack && fresh_ack < key,
        "the client receives the fresh render before acknowledging either version, \
         then presses the key against the acknowledged fresh version"
    );
    assert_eq!(
        lines[key].message["params"]["key"], "S",
        "the recorded key is status.stage-all, the mutation whose progress the file pins"
    );
    assert_eq!(
        lines[key].message["params"]["version"], lines[fresh_render].message["params"]["version"],
        "the key is pressed against the acknowledged fresh render"
    );

    // The status-open handshake also emits progress frames now; pin the
    // mutation handshake by its begin message and matching token.
    let begin = index(&|line| {
        line.dir == Dir::ServerToClient
            && phase(line, "begin")
            && line.message["params"]["value"]["message"] == "view/key"
    });
    let mutation_token = lines[begin].message["params"]["token"]
        .as_str()
        .expect("progress token")
        .to_owned();
    let create = index(&|line| {
        line.dir == Dir::ServerToClient
            && method(line, "window/workDoneProgress/create")
            && line.message["params"]["token"].as_str() == Some(mutation_token.as_str())
    });
    let end = index(&|line| {
        line.dir == Dir::ServerToClient
            && phase(line, "end")
            && line.message["params"]["token"].as_str() == Some(mutation_token.as_str())
    });
    let key_id = lines[key].message["id"].clone();
    let key_response =
        index(&|line| line.dir == Dir::ServerToClient && line.message.get("id") == Some(&key_id));
    let next_render = index(&|line| {
        line.dir == Dir::ServerToClient
            && method(line, "view/render")
            && line.message["params"]["version"] == 3
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
    assert_eq!(
        lines[begin].message["params"]["value"]["message"],
        "view/key"
    );
    assert_eq!(lines[begin].message["params"]["value"]["cancellable"], true);

    assert_eq!(
        lines[next_render].message["params"]["viewId"], view_id,
        "the refreshed render targets the acknowledged view"
    );
    assert_eq!(
        lines[next_render].message["params"]["baseVersion"], 2,
        "the mutation render edits from the acknowledged fresh version"
    );
    let next_ack = index(&|line| {
        line.dir == Dir::ClientToServer
            && method(line, "view/rendered")
            && line.message["params"]["version"] == 3
    });
    assert!(
        next_render < next_ack,
        "the client acknowledges the mutation render only after receiving it"
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

/// The recorded form of a status view's URI: one separator after the scope.
const RECORDED_STATUS_URI: &str = "magit://status/repo";

/// The placeholder the recordings use for the running server binary.
const RECORDED_BINARY: &str = "/magit-lsp/target/debug/magit-lsp";

/// Build the repository state the named example was recorded against.
///
/// The fixture names mirror the recorded status renders: `plain-open` starts
/// from the ten-line file and applies `standard_changes`; the code-action and
/// prompt examples modify a one-line `f.txt`; the commit examples stage the
/// standard change before running the Commit menu item.
fn replay_fixture(name: &str) -> fixtures::RepoFixture {
    use fixtures::{RepoFixture, TEN_LINES};
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

/// The live repository root's `file://` URI, encoded by the shared
/// native-path encoder so a native root never enters a URI by hand.
fn live_root_uri(root: &str) -> String {
    let uri = uri_from_path(Path::new(root)).expect("the replay fixture root is an absolute path");
    uri.as_str().to_owned()
}

/// Expand the recording's root placeholders within one parsed string value.
///
/// A `file:///repo` occurrence becomes the live root's encoded URI and a bare
/// `/repo` becomes the native root. The scan is single-pass so inserted text
/// is never rescanned: a live root can itself contain the placeholder text.
fn expand_root(text: &str, root_uri: &str, root: &str) -> String {
    let mut expanded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find(RECORDED_ROOT) {
        let prefix_is_uri = rest[..index].ends_with("file://");
        let keep = if prefix_is_uri {
            index - "file://".len()
        } else {
            index
        };
        expanded.push_str(&rest[..keep]);
        expanded.push_str(if prefix_is_uri { root_uri } else { root });
        rest = &rest[index + RECORDED_ROOT.len()..];
    }
    expanded.push_str(rest);
    expanded
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
    // The root is replaced within the parsed string values: a live root can
    // contain characters a URI must percent-encode and Windows roots contain
    // backslashes, while splicing either into the serialized form would
    // corrupt its escapes before the frame is sent. URI positions take the
    // shared encoder's output; filesystem-path positions keep the native root.
    let root_uri = live_root_uri(root);
    for_each_string(&mut value, &mut |text| {
        *text = expand_root(text, &root_uri, root);
    });
    value
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
    let [change] = message
        .get("params")?
        .get("edit")?
        .get("documentChanges")?
        .as_array()?
        .as_slice()
    else {
        return None;
    };
    let [edit] = change.get("edits")?.as_array()?.as_slice() else {
        return None;
    };
    let version = change.get("textDocument")?.get("version")?.as_u64()?;
    Some((
        version,
        edit.get("range")?.clone(),
        edit.get("newText")?.as_str()?.to_owned(),
    ))
}

/// The UTF-16 position of an LSP `range` endpoint as a byte offset.
fn position_offset(text: &str, position: &Value) -> Result<usize, String> {
    let line = position
        .get("line")
        .and_then(Value::as_u64)
        .ok_or("edit position has no line")? as usize;
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

/// Content-target hashes the live server offers in band, keyed
/// `(command, nodeId)`.
///
/// One `textDocument/codeAction` response offers a different `mutationHash`
/// per command for the same node, and the hash binds the command itself, so
/// the key must carry both. Regeneration uses these offers to refresh the
/// recorded client's stale content hashes before sending its request.
#[derive(Default)]
struct ContentOffers(HashMap<(String, String), String>);

impl ContentOffers {
    /// Record every content target `frame` offers as a code-action argument.
    fn note(&mut self, frame: &Value) {
        let Some(actions) = frame.get("result").and_then(Value::as_array) else {
            return;
        };
        for action in actions {
            let arguments = action
                .get("command")
                .and_then(|command| command.get("arguments"))
                .and_then(Value::as_array);
            for argument in arguments.into_iter().flatten() {
                let Some(target) = argument.get("target") else {
                    continue;
                };
                if target.get("kind").and_then(Value::as_str) != Some("content") {
                    continue;
                }
                let (Some(command), Some(node), Some(hash)) = (
                    argument.get("command").and_then(Value::as_str),
                    target.get("nodeId").and_then(Value::as_str),
                    target.get("mutationHash").and_then(Value::as_str),
                ) else {
                    continue;
                };
                self.0
                    .insert((command.to_owned(), node.to_owned()), hash.to_owned());
            }
        }
    }

    /// Substitute the live offer into every content target of a client frame.
    ///
    /// Returns whether a target changed. A content target no captured frame
    /// offered is a hard regeneration error: keeping the recorded stale hash
    /// would silently replay a request the server now rejects.
    fn refresh(&self, message: &mut Value, where_: &str) -> Result<bool, String> {
        let arguments = message
            .get_mut("params")
            .and_then(|params| params.get_mut("arguments"))
            .and_then(Value::as_array_mut);
        let mut changed = false;
        for argument in arguments.into_iter().flatten() {
            if argument
                .get("target")
                .and_then(|target| target.get("kind"))
                .and_then(Value::as_str)
                != Some("content")
            {
                continue;
            }
            let (Some(command), Some(node)) = (
                argument.get("command").and_then(Value::as_str),
                argument["target"]["nodeId"].as_str(),
            ) else {
                continue;
            };
            let Some(hash) = self.0.get(&(command.to_owned(), node.to_owned())) else {
                return Err(format!(
                    "{where_}: no in-band offer for content target ({command}, {node})"
                ));
            };
            if argument["target"]["mutationHash"].as_str() != Some(hash.as_str()) {
                argument["target"]["mutationHash"] = Value::String(hash.clone());
                changed = true;
            }
        }
        Ok(changed)
    }
}

/// Visit every string leaf in a JSON value.
///
/// Machine-dependent values (repository roots, binary paths) are replaced
/// through this walk on the parsed string values, never by splicing into
/// serialized JSON text: a run root can contain backslashes (Windows), and
/// splicing those into the serialized form would corrupt its escapes before
/// a re-parse.
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

/// Visit every LSP `range` in a frame, carrying the document URI in scope.
///
/// A `documentUri` or `textDocument.uri` on any enclosing object scopes the
/// ranges below it, so an edit's range is matched with the document it edits.
fn for_each_range(
    value: &mut Value,
    uri: Option<&str>,
    apply: &mut dyn FnMut(&mut Value, Option<&str>),
) {
    match value {
        Value::Object(map) => {
            let scoped: Option<String> = map
                .get("documentUri")
                .and_then(Value::as_str)
                .or_else(|| {
                    map.get("textDocument")
                        .and_then(|document| document.get("uri"))
                        .and_then(Value::as_str)
                })
                .or(uri)
                .map(str::to_owned);
            if let Some(range) = map.get_mut("range") {
                apply(range, scoped.as_deref());
            }
            for (key, item) in map.iter_mut() {
                if key != "range" {
                    for_each_range(item, scoped.as_deref(), apply);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                for_each_range(item, uri, apply);
            }
        }
        _ => {}
    }
}

/// Per-side normalization of the machine-dependent repository root.
///
/// The recorded side uses [`RECORDED_ROOT`] and the observed side the live
/// fixture root. Both are reduced to the same canonical form: the root's
/// native and encoded URI forms are replaced in text, and a range endpoint is
/// shifted by the UTF-16 width its canonicalized prefix loses. Text and
/// numbers therefore move together, so the strict frame comparison stays
/// exact on any machine.
struct RootNormalizer {
    /// This side's root, as it appears in frames before canonicalization.
    root: String,
    /// Canonical-URI-keyed client documents whose ranges may cross the root.
    documents: HashMap<String, String>,
}

impl RootNormalizer {
    /// A normalizer for one side of the replay comparison.
    fn new(root: &str) -> Self {
        Self {
            root: root.to_owned(),
            documents: HashMap::new(),
        }
    }

    /// Remember the full text a client document holds on this side.
    fn note_document(&mut self, uri: &str, text: &str) {
        self.documents
            .insert(canonical_root(uri, &self.root), text.to_owned());
    }

    /// Shift range endpoints whose addressed text canonicalizes shorter.
    ///
    /// Endpoints are UTF-16 positions in the client document, while
    /// canonicalization rewrites both the encoded URI root and the raw native
    /// root. The shift is the UTF-16 width the canonicalized prefix loses,
    /// computed per endpoint so several roots on one line and their different
    /// encoded widths move together.
    fn normalize_ranges(&self, value: &mut Value) {
        for_each_range(value, None, &mut |range, uri| {
            let Some(uri) = uri else { return };
            let Some(text) = self.documents.get(&canonical_root(uri, &self.root)) else {
                return;
            };
            let lines: Vec<&str> = text.split('\n').collect();
            for endpoint in ["start", "end"] {
                let Some(position) = range.get_mut(endpoint) else {
                    continue;
                };
                let (Some(line), Some(character)) = (
                    position.get("line").and_then(Value::as_u64),
                    position.get("character").and_then(Value::as_u64),
                ) else {
                    continue;
                };
                let Some(line_text) = lines.get(line as usize) else {
                    continue;
                };
                let prefix = utf16_prefix(line_text, character as usize);
                let delta = utf16_len(prefix) as i64
                    - utf16_len(&canonical_root(prefix, &self.root)) as i64;
                if delta == 0 {
                    continue;
                }
                position["character"] = Value::from(character as i64 - delta);
            }
        });
    }
}

/// The UTF-16 code-unit length of `text`, the unit of LSP character offsets.
fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The prefix of `text` spanning at most `units` UTF-16 code units.
fn utf16_prefix(text: &str, units: usize) -> &str {
    let mut used = 0;
    for (index, character) in text.char_indices() {
        if used + character.len_utf16() > units {
            return &text[..index];
        }
        used += character.len_utf16();
    }
    text
}

/// Every spelling this machine knows for one repository root.
///
/// The harness and the server reach the root by different routes, so their
/// spellings can differ while naming the same directory: the fixture
/// normalizes its temporary directory with the shared resolver, which preserves
/// a Windows 8.3 short name or a symlinked component, while the server adopts
/// the spelling Git discovery reports — the long, resolved form on Windows —
/// and Git spells Windows paths with forward separators. Reducing a root
/// therefore means reducing each of its spellings: as given, its
/// [`std::fs::canonicalize`] form (without the verbatim prefix Windows adds and
/// neither Git nor the server uses) and its separator variants.
fn root_spellings(root: &str) -> Vec<String> {
    let mut spellings = vec![root.to_owned()];
    if let Ok(resolved) = fs::canonicalize(root) {
        let resolved = resolved.to_string_lossy();
        spellings.push(
            resolved
                .strip_prefix(r"\\?\")
                .unwrap_or(&resolved)
                .to_owned(),
        );
    }
    for spelling in spellings.clone() {
        if spelling.contains('\\') {
            spellings.push(spelling.replace('\\', "/"));
        }
        // Only a Windows-shaped spelling has a backslash form; a POSIX path
        // with a literal backslash in a name is not a separator variant.
        if spelling.contains('/') && windows_shaped(&spelling) {
            spellings.push(spelling.replace('/', "\\"));
        }
    }
    let mut unique: Vec<String> = Vec::new();
    for spelling in spellings {
        if !unique.contains(&spelling) {
            unique.push(spelling);
        }
    }
    unique
}

/// Whether `path` is spelled as a Windows absolute path (a drive or UNC), so a
/// backslash variant names one location rather than unrelated text.
fn windows_shaped(path: &str) -> bool {
    path.starts_with("//")
        || (path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            && path.as_bytes().get(1) == Some(&b':'))
}

/// Replace one side's repository root with the recording's placeholder.
///
/// Operates on plain string values (URIs, document text), never on serialized
/// JSON text: a Windows root's backslashes are escaped there and would never
/// match. Each [`root_spellings`] form is replaced — its native text and its
/// URI form, encoded by the shared native-path encoder — so a live encoded URI
/// root reduces to the recorded `file:///repo` placeholder while a
/// filesystem-path occurrence keeps the native form. A status view's URI embeds
/// the native root after exactly one separator, and the server trims the root's
/// leading separators: a root without one (Windows drive or verbatim-prefixed)
/// leaves the recorded form's separator doubled, so that artifact is repaired
/// to [`RECORDED_STATUS_URI`].
fn canonical_root(text: &str, root: &str) -> String {
    let mut canonical = text.to_owned();
    for spelling in root_spellings(root) {
        if let Some(uri) = uri_from_path(Path::new(&spelling)) {
            canonical = canonical.replace(uri.as_str(), &format!("file://{RECORDED_ROOT}"));
        }
        canonical = canonical.replace(&spelling, RECORDED_ROOT);
    }
    canonical.replace(
        &format!("magit://status/{RECORDED_ROOT}"),
        RECORDED_STATUS_URI,
    )
}

/// Replace this run's repository root and binary path with the recording's
/// placeholders within one plain string value.
///
/// This is the repository-root/binary half of [`canonical_frame`]'s contract,
/// shared so a persisted client echo uses the same placeholders as a captured
/// server frame. Callers hand it parsed string values — [`canonical_frame`]
/// walks the frame with [`for_each_string`] — never serialized JSON text: a
/// live root can contain backslashes (Windows), which never match the
/// serialized form's escapes. Replaying a recorded frame expands them back to
/// the live values.
fn canonical_paths(text: &str, root: &str, binary: &str) -> String {
    canonical_root(text, root).replace(binary, RECORDED_BINARY)
}

/// The recorded comparison form of a frame.
///
/// The current server omits `jsonrpc` on outbound frames, recorded runs used a
/// temporary repository and a checkout-local binary path, commit run ids are
/// opaque hashes, and commit/object hashes differ between runs. The recordings
/// normalized those by adding the member and by the `/repo`, binary-path and
/// `run-1` placeholders; [`HashNormalizer`] maps 40-hex hashes to `hash-N`.
/// [`RootNormalizer`] reduces each side's root string and root-dependent range
/// endpoints to the same form. Versions and every other value stay exact.
fn canonical_frame(
    message: &Value,
    roots: &RootNormalizer,
    binary: &str,
    hashes: &mut HashNormalizer,
) -> Value {
    let mut value = message.clone();
    roots.normalize_ranges(&mut value);
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
    // Roots and binary paths are replaced within the parsed string values: a
    // live root can contain backslashes (Windows), which never match — or
    // corrupt — the serialized JSON form.
    for_each_string(&mut value, &mut |text| {
        *text = canonical_paths(text, &roots.root, binary);
    });
    value
}

/// Return first differing JSON path and its values.
fn first_difference<'a>(
    expected: &'a Value,
    observed: &'a Value,
) -> Option<(String, Option<&'a Value>, Option<&'a Value>)> {
    fn walk<'a>(
        expected: Option<&'a Value>,
        observed: Option<&'a Value>,
        path: &str,
    ) -> Option<(String, Option<&'a Value>, Option<&'a Value>)> {
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
            _ => Some((
                if path.is_empty() {
                    "$".to_owned()
                } else {
                    path.to_owned()
                },
                expected,
                observed,
            )),
        }
    }
    walk(Some(expected), Some(observed), "$")
}

const JSON_EXCERPT_LIMIT: usize = 240;

/// Serialize a differing JSON value as a bounded excerpt.
fn json_excerpt(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return "<missing>".to_owned();
    };
    let text = serde_json::to_string(value).expect("serialise JSON value");
    let total = value
        .as_str()
        .map_or_else(|| text.chars().count(), |value| value.chars().count());
    if text.chars().count() <= JSON_EXCERPT_LIMIT {
        return format!("{text:?} ({total} chars total)");
    }
    let excerpt: String = text.chars().take(JSON_EXCERPT_LIMIT).collect();
    format!("{:?}... ({total} chars total)", excerpt)
}

/// Maximum bytes of the server's stderr attached to a replay mismatch.
const STDERR_TAIL_LIMIT: usize = 2048;

/// A bounded, newline-normalized tail of the server's stderr, or a marker when
/// the server logged nothing.
fn stderr_tail(stderr: &str) -> String {
    let normalized = stderr.replace("\r\n", "\n");
    let normalized = normalized.trim_end();
    if normalized.is_empty() {
        return "<empty>".to_owned();
    }
    let start = normalized.len().saturating_sub(STDERR_TAIL_LIMIT);
    let start = (start..normalized.len())
        .find(|index| normalized.is_char_boundary(*index))
        .unwrap_or(normalized.len());
    let tail = &normalized[start..];
    if start == 0 {
        tail.to_owned()
    } else {
        format!("...{tail}")
    }
}

/// Format a recorded s2c frame mismatch, including a bounded server stderr tail
/// so a failure whose frame text omits the cause stays diagnosable.
fn s2c_mismatch_message(where_: &str, expected: &Value, observed: &Value, stderr: &str) -> String {
    let (path, expected_value, observed_value) = first_difference(expected, observed)
        .unwrap_or_else(|| ("<unknown>".to_owned(), None, None));
    format!(
        "{where_}: recorded s2c frame mismatch at {path}\n  expected: {}\n  observed: {}\n  observed frame: {}\n  server stderr tail: {}",
        json_excerpt(expected_value),
        json_excerpt(observed_value),
        json_excerpt(Some(observed)),
        stderr_tail(stderr)
    )
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
    let observed_object = observed
        .as_object()
        .ok_or("captured frame is not an object")?;
    let recorded_identity = frame_identity(&line.message);
    let observed_identity = frame_identity(observed_object);
    if recorded_identity != observed_identity {
        // The deferred status acquisition now reports progress while Git runs:
        // its `window/workDoneProgress/create` request takes the slot that used
        // to carry the fresh status render. Accept only that intended wire
        // change; every other identity change stays a replay failure.
        let status_progress_handshake = recorded_identity == "notification view/render"
            && observed_identity
                == "request window/workDoneProgress/create id \"magit-lsp/outbound/1\"";
        // The status handshake consumes the first outbound id, so the mutation's
        // progress-create request shifts from outbound/1 to outbound/2. Accept
        // only that renumbering; any other method or id change still fails.
        let shifted_progress_id = recorded_identity
            == "request window/workDoneProgress/create id \"magit-lsp/outbound/1\""
            && observed_identity
                == "request window/workDoneProgress/create id \"magit-lsp/outbound/2\"";
        if !status_progress_handshake && !shifted_progress_id {
            return Err(format!(
                "captured frame identity changed: recorded {recorded_identity}, observed {observed_identity}"
            ));
        }
    }
    let mut value: Value = serde_json::from_str(&line.raw).map_err(|error| error.to_string())?;
    value["message"] = observed.clone();
    let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
    parse_line(&text)
        .map_err(|error| format!("captured line is not a recorded exchange: {error}"))?;
    Ok(text)
}

/// Build an extra recorded line for a server frame added by regeneration.
///
/// The status-acquisition progress handshake is new wire traffic that the old
/// recording predates; it is inserted alongside the frame it precedes rather
/// than overwriting that frame's identity.
fn inserted_line(template: &Line, message: &Value, note: &str) -> Result<String, String> {
    let mut value: Value =
        serde_json::from_str(&template.raw).map_err(|error| error.to_string())?;
    value["message"] = message.clone();
    value["note"] = Value::String(note.to_owned());
    value["source"] = Value::String("recorded".to_owned());
    let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
    parse_line(&text)
        .map_err(|error| format!("inserted line is not a recorded exchange: {error}"))?;
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
    replay_example_mode(
        name,
        lines,
        std::env::var_os("MAGIT_LSP_REGENERATE_EXAMPLES").is_some(),
    )
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
///
/// Regeneration likewise refreshes a recorded client content target's
/// `mutationHash` from the live server's in-band `textDocument/codeAction`
/// offers, keyed by `(command, nodeId)`; the substituted hash is persisted in
/// the artifact, and a content target with no live offer is an error.
fn replay_example_mode(
    name: &str,
    lines: &[Line],
    regenerate: bool,
) -> Result<Vec<String>, String> {
    replay_example_env(name, lines, regenerate, &[])
}

/// [`replay_example_mode`] with extra environment pairs passed to the spawned
/// server binary, so a replay can tighten watcher or timing behavior.
fn replay_example_env(
    name: &str,
    lines: &[Line],
    regenerate: bool,
    extra_env: &[(&str, &str)],
) -> Result<Vec<String>, String> {
    let mut regenerated = Vec::with_capacity(lines.len());
    let fixture = replay_fixture(name.strip_suffix(".jsonl").unwrap_or(name));
    let (repo, _git) = fixture.build();
    let root = repo.path().to_string_lossy().to_string();
    let binary = env!("CARGO_BIN_EXE_magit-lsp");

    let mut server = fixtures::stdio::TestServer::spawn(extra_env);
    // Outbound requests observed so far, indexed by their recorded number.
    let mut outbound: Vec<Value> = Vec::new();
    // The deferred status acquisition's progress token, once regeneration has
    // inserted its create request; its begin/end frames are inserted too.
    let mut status_token: Option<String> = None;
    // Whether the status progress create has already been inserted.
    let mut create_seen = false;
    // Extra lines inserted into the regenerated recording before this index.
    let mut inserted: usize = 0;
    // Recorded status-handshake lines already inserted earlier and skipped.
    let mut skipped: usize = 0;
    // The commit run id this server generated, if the exchange starts one.
    let mut run_id: Option<String> = None;
    // Recorded and observed frames name commit hashes independently, each by
    // first-seen position on its own side.
    let mut recorded_hashes = HashNormalizer::new();
    let mut observed_hashes = HashNormalizer::new();
    // Each side normalizes its own root: the recording already carries the
    // placeholder, the live frames carry this run's temporary fixture root.
    let mut recorded_roots = RootNormalizer::new(RECORDED_ROOT);
    let mut observed_roots = RootNormalizer::new(&root);
    // Regeneration only: the live server's in-band content-target offers.
    let mut content_offers = ContentOffers::default();
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
        // The status handshake precedes the open response and its end lands at
        // the next client request, so a recording that already carries those
        // lines must not replay them: the insertion loop below re-inserts
        // every one at the position this run actually observes, in the
        // server's own publication order.
        if regenerate
            && [
                "server asks the client to create the deferred status progress token",
                "server reports the deferred status progress phase",
                "the client answers the deferred status progress-create request",
            ]
            .iter()
            .any(|note| line.raw.contains(&format!("\"note\":\"{note}\"")))
        {
            skipped += 1;
            continue;
        }
        regenerated.push(line.raw.clone());
        let where_ = format!("{name}:{}", index + 1);
        match line.dir {
            Dir::ClientToServer => {
                let mut message = Value::Object(line.message.clone());
                // The inserted status handshake took outbound id 1, so the
                // mutation's recorded progress-create reply becomes id 2 and
                // answers the request observed after the status create.
                let mut shifted_by_status_handshake = false;
                if regenerate && status_token.is_some() {
                    if let Some(id) = line.message.get("id").and_then(Value::as_str) {
                        if id == "magit-lsp/outbound/1" {
                            shifted_by_status_handshake = true;
                            let mut value: Value = serde_json::from_str(&line.raw)
                                .map_err(|error| error.to_string())?;
                            value["message"]["id"] =
                                Value::String("magit-lsp/outbound/2".to_owned());
                            let text =
                                serde_json::to_string(&value).map_err(|error| error.to_string())?;
                            parse_line(&text).map_err(|error| {
                                format!("{where_}: renumbered reply is not a recorded exchange: {error}")
                            })?;
                            regenerated[index + inserted - skipped] = text;
                        }
                    }
                }
                if message.get("method").is_none() {
                    if let Some(recorded) = message.get("id").and_then(Value::as_str) {
                        if let Some(number) = recorded.strip_prefix("magit-lsp/outbound/") {
                            let number: usize = number
                                .parse()
                                .map_err(|error| format!("{where_}: bad outbound id: {error}"))?;
                            let observed_index = if shifted_by_status_handshake {
                                number
                            } else {
                                number.wrapping_sub(1)
                            };
                            let actual = outbound.get(observed_index).ok_or_else(|| {
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
                        let opened = line.message.get("method").and_then(Value::as_str)
                            == Some("textDocument/didOpen");
                        let probe = expand_client_frame(&message, &root, run_id.as_deref());
                        let current_uri = probe["params"]["textDocument"]["uri"]
                            .as_str()
                            .ok_or_else(|| format!("{where_}: client document update has no uri"))?
                            .to_owned();
                        if opened {
                            current_documents.insert(current_uri, update.text.clone());
                            pending_edits.remove(&update.uri);
                        } else {
                            let current_text =
                                current_documents.get(&current_uri).map(String::as_str);
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
                                message["params"]["contentChanges"][0]["text"] =
                                    Value::String(refreshed.clone());
                                // The live server receives the refreshed text with
                                // this run's real paths, but the artifact must keep
                                // the recording's placeholders like every captured
                                // server frame; strict replay expands them again.
                                let mut persisted = message.clone();
                                persisted["params"]["contentChanges"][0]["text"] =
                                    Value::String(canonical_paths(refreshed, &root, binary));
                                regenerated[index + inserted - skipped] =
                                    captured_line(line, &persisted)
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
                if regenerate && content_offers.refresh(&mut message, &where_)? {
                    // Persist the substituted hash in the artifact with the
                    // recorded envelope, like the renumbered replies above.
                    regenerated[index + inserted - skipped] = captured_line(line, &message)
                        .map_err(|error| format!("{where_}: {error}"))?;
                }
                let expanded = expand_client_frame(&message, &root, run_id.as_deref());
                // Track each side's client documents so root-dependent range
                // endpoints can be shifted together with the text they address.
                if let Some(update) = recorded_document_update(&line.message) {
                    recorded_roots.note_document(&update.uri, &update.text);
                    if let Some(observed) = expanded.as_object().and_then(recorded_document_update)
                    {
                        observed_roots.note_document(&observed.uri, &observed.text);
                    }
                }
                server.send(expanded);
            }
            Dir::ServerToClient => {
                let mut actual =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| server.recv()))
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
                let mut observed =
                    canonical_frame(&actual, &observed_roots, binary, &mut observed_hashes);
                // The deferred status acquisition announces progress before the
                // fresh render. Regeneration keeps those extra frames in the
                // recording rather than letting them overwrite the render.
                loop {
                    let method = actual.get("method").and_then(Value::as_str);
                    let token = actual["params"]["token"].as_str().unwrap_or("");
                    let is_create = regenerate
                        && method == Some("window/workDoneProgress/create")
                        && !create_seen;
                    let is_early_begin = regenerate
                        && method == Some("$/progress")
                        && actual["params"]["value"]["kind"] == "begin"
                        && actual["params"]["value"]["message"] == "status open";
                    let is_phase = regenerate
                        && method == Some("$/progress")
                        && (status_token.as_deref().is_some_and(|live| live == token)
                            || is_early_begin);
                    if !is_create && !is_phase {
                        break;
                    }
                    let note = if is_create {
                        "server asks the client to create the deferred status progress token"
                    } else {
                        "server reports the deferred status progress phase"
                    };
                    let text = inserted_line(line, &observed, note)
                        .map_err(|error| format!("{where_}: {error}"))?;
                    // The handshake precedes this recorded frame on the wire,
                    // so each inserted line takes the slot before it and the
                    // captured frame's write at `index + inserted - skipped` still lands
                    // on this line's own slot.
                    regenerated.insert(index + inserted - skipped, text);
                    inserted += 1;
                    if is_early_begin {
                        status_token = Some(token.to_string());
                    }
                    if is_create {
                        create_seen = true;
                        status_token = observed["params"]["token"].as_str().map(str::to_owned);
                        let reply =
                            json!({"jsonrpc":"2.0","id":actual["id"].clone(),"result":null});
                        let mut recorded_reply = reply.clone();
                        recorded_reply["id"] =
                            Value::String(format!("magit-lsp/outbound/{}", outbound.len()));
                        let mut reply_value: Value = serde_json::from_str(&inserted_line(
                            line,
                            &recorded_reply,
                            "the client answers the deferred status progress-create request",
                        )?)
                        .map_err(|error| error.to_string())?;
                        reply_value["dir"] = Value::String("c2s".to_owned());
                        let reply_text = serde_json::to_string(&reply_value)
                            .map_err(|error| error.to_string())?;
                        parse_line(&reply_text).map_err(|error| {
                            format!("{where_}: inserted reply is not a recorded exchange: {error}")
                        })?;
                        regenerated.insert(index + inserted - skipped, reply_text);
                        inserted += 1;
                        server.send(reply);
                    }
                    actual = server.recv();
                    // A request the insertion loop stepped past (for example
                    // the mutation's progress create) still needs its observed
                    // id registered, so the recorded reply answers it.
                    if actual.get("method").is_some() && actual.get("id").is_some() {
                        outbound.push(actual["id"].clone());
                    }
                    observed =
                        canonical_frame(&actual, &observed_roots, binary, &mut observed_hashes);
                }
                if regenerate {
                    content_offers.note(&observed);
                }
                let expected = canonical_frame(
                    &Value::Object(line.message.clone()),
                    &recorded_roots,
                    RECORDED_BINARY,
                    &mut recorded_hashes,
                );
                if observed != expected {
                    if !regenerate {
                        return Err(s2c_mismatch_message(
                            &where_,
                            &expected,
                            &observed,
                            &server.stderr_text(),
                        ));
                    }
                    // Regeneration captures the server's current answer instead
                    // of blessing the stale recording, but only for the same
                    // exchange: a different method or request/response id is a
                    // replay failure, not a capture.
                    regenerated[index + inserted - skipped] = captured_line(line, &observed)
                        .map_err(|error| format!("{where_}: {error}"))?;
                }
                if regenerate
                    && line.message.get("method").and_then(Value::as_str)
                        == Some("workspace/applyEdit")
                {
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
        "commit-edit-confirm.jsonl" => &[
            "commit-edit-confirm-01.jsonl",
            "commit-edit-confirm-02.jsonl",
        ],
        "plain-open.jsonl" => &["plain-open.jsonl"],
        "rich-lifecycle.jsonl" => &["rich-lifecycle.jsonl"],
        "plain-code-action.jsonl" => &["plain-code-action.jsonl"],
        "plain-document-prompt.jsonl" => &["plain-document-prompt.jsonl"],
        _ => return Err(format!("refusing to regenerate unrelated recording {name}")),
    };
    Ok(artifacts.to_vec())
}

/// Split regenerated lines across the exact artifacts that recorded them.
fn regenerated_fragments(
    name: &str,
    lines: &[String],
) -> Result<Vec<(std::path::PathBuf, String)>, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(EXAMPLES_DIR);
    let artifacts = regeneration_artifacts(name)?;
    let mut fragments = Vec::new();
    let mut offset = 0;
    for artifact in &artifacts {
        let path = root.join(artifact);
        let original = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        // Fragmented recordings keep their recorded line boundaries; a
        // monolithic recording owns every remaining line.
        let end = if artifacts.len() == 1 {
            lines.len()
        } else {
            offset + original.lines().count()
        };
        let content = lines
            .get(offset..end)
            .ok_or("missing fragment lines")?
            .join("\n")
            + "\n";
        if content.len() >= 100_000 {
            return Err(format!("fragment too large: {} bytes", content.len()));
        }
        fragments.push((path, content));
        offset = end;
    }
    if offset != lines.len() {
        return Err("unrecorded fragment lines".into());
    }
    Ok(fragments)
}

/// Capture preserves source bytes and fragment boundaries, and rejects overflow.
#[test]
fn capture_preserves_fragments_and_rejects_overflow() {
    for name in ["commit-edit-abort.jsonl", "commit-edit-confirm.jsonl"] {
        let mut lines: Vec<String> = example_lines(name)
            .into_iter()
            .map(|line| line.raw)
            .collect();
        for (path, content) in regenerated_fragments(name, &lines).unwrap() {
            assert_eq!(
                content,
                fs::read_to_string(path).unwrap(),
                "unchanged source bytes and fragment boundaries"
            );
        }
        lines[0] = "x".repeat(100_000);
        assert!(regenerated_fragments(name, &lines)
            .unwrap_err()
            .contains("fragment too large"));
    }
    assert!(regenerated_fragments("unrelated.jsonl", &[]).is_err());
}

/// Recording artifacts contain no CR bytes, so replay byte comparisons hold.
///
/// `docs/examples/*.jsonl` is pinned to `eol=lf` in `.gitattributes`; a checkout
/// that converts line endings fails here with the artifact name instead of the
/// opaque byte-diff mismatch the replay tests report.
#[test]
fn recording_artifacts_use_lf_line_endings() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = example_recordings::recordings(&root.join(EXAMPLES_DIR))
        .unwrap_or_else(|error| panic!("load examples: {error}"));
    assert!(
        !files.is_empty(),
        "expected at least one example under {EXAMPLES_DIR}"
    );
    for (name, bytes) in files {
        if let Some(offset) = bytes.iter().position(|byte| *byte == b'\r') {
            panic!(
                "{}: carriage return at byte {offset}; recordings must be checked out with LF \
                 endings (docs/examples/*.jsonl is pinned to eol=lf in .gitattributes, \
                 re-checkout the file)",
                root.join(EXAMPLES_DIR).join(&name).display()
            );
        }
    }
}

/// Regeneration refreshes a recorded c2s content hash from the live offer.
#[test]
fn regeneration_refreshes_recorded_content_hashes() {
    let name = "plain-code-action.jsonl";
    let stale = "0".repeat(64);
    let content_request = |line: &Line| {
        line.dir == Dir::ClientToServer
            && line.message.get("method").and_then(Value::as_str)
                == Some("workspace/executeCommand")
            && line.message["params"]["arguments"][0]["target"]["kind"] == "content"
    };
    let mut lines = example_lines(name);
    let request = lines
        .iter_mut()
        .find(|line| content_request(line))
        .expect("a recorded content command");
    request.message["params"]["arguments"][0]["target"]["mutationHash"] =
        Value::String(stale.clone());
    let command = request.message["params"]["arguments"][0]["command"]
        .as_str()
        .expect("content command name")
        .to_owned();
    let node = request.message["params"]["arguments"][0]["target"]["nodeId"]
        .as_str()
        .expect("content node id")
        .to_owned();

    let regenerated = replay_example_mode(name, &lines, true).expect("regeneration");
    assert!(
        !regenerated.iter().any(|raw| raw.contains(&stale)),
        "the stale recorded hash must not survive regeneration"
    );
    let parsed = regenerated
        .iter()
        .map(|raw| parse_line(raw).unwrap_or_else(|error| panic!("{error}")))
        .collect::<Vec<_>>();
    // The live offer, read straight from the captured codeAction response.
    let mut offer = None;
    for line in parsed.iter().filter(|line| line.dir == Dir::ServerToClient) {
        let Some(actions) = line.message.get("result").and_then(Value::as_array) else {
            continue;
        };
        for action in actions {
            let argument = action
                .get("command")
                .and_then(|command| command.get("arguments"))
                .and_then(|arguments| arguments.get(0));
            let Some(argument) = argument else {
                continue;
            };
            let target = &argument["target"];
            if argument.get("command").and_then(Value::as_str) != Some(command.as_str())
                || target.get("kind").and_then(Value::as_str) != Some("content")
                || target.get("nodeId").and_then(Value::as_str) != Some(node.as_str())
            {
                continue;
            }
            offer = target
                .get("mutationHash")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
    }
    let offer = offer.expect("the captured codeAction response offers the command");
    assert_ne!(
        offer, stale,
        "the live offer is not the stale recorded hash"
    );
    let sent = parsed
        .iter()
        .find(|line| content_request(line))
        .expect("the regenerated content command");
    assert_eq!(
        sent.message["params"]["arguments"][0]["target"]["mutationHash"],
        Value::String(offer),
        "the regenerated client frame persists the live offer"
    );

    let mut missing = example_lines(name);
    let request = missing
        .iter_mut()
        .find(|line| content_request(line))
        .expect("a recorded content command");
    request.message["params"]["arguments"][0]["target"]["nodeId"] =
        Value::String("unstaged/not-offered.txt".to_owned());
    let error =
        replay_example_mode(name, &missing, true).expect_err("an unoffered content hash must fail");
    assert!(
        error.contains("no in-band offer") && error.contains("plain-code-action.jsonl:"),
        "the missing offer must be named: {error}"
    );
}

/// Regeneration maps every maintained recording to its exact artifact files.
#[test]
fn regeneration_keeps_fragment_mapping_and_rejects_unrelated_names() {
    for name in MAINTAINED_RECORDINGS {
        let lines: Vec<String> = example_lines(name)
            .into_iter()
            .map(|line| line.raw)
            .collect();
        let fragments =
            regenerated_fragments(name, &lines).unwrap_or_else(|error| panic!("{name}: {error}"));
        let artifacts = fragments
            .iter()
            .map(|(path, _)| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            artifacts,
            regeneration_artifacts(name).unwrap(),
            "{name} maps to its own artifacts"
        );
        assert_eq!(
            fragments
                .iter()
                .map(|(_, content)| content.lines().count())
                .sum::<usize>(),
            lines.len(),
            "{name} covers every recorded line exactly once"
        );
        for (path, content) in fragments {
            assert_eq!(
                content,
                fs::read_to_string(&path).unwrap(),
                "{name} keeps {} byte-identical",
                path.display()
            );
        }
    }
    assert_eq!(
        regeneration_artifacts("commit-edit-abort.jsonl").unwrap(),
        ["commit-edit-abort-01.jsonl", "commit-edit-abort-02.jsonl"]
    );
    assert_eq!(
        regeneration_artifacts("plain-open.jsonl").unwrap(),
        ["plain-open.jsonl"]
    );
    for unrelated in [
        "example.jsonl",
        "plain-open-03.jsonl",
        "commit-edit-abort-01.jsonl",
    ] {
        let error =
            regenerated_fragments(unrelated, &[]).expect_err("unrelated recording must be refused");
        assert!(
            error.contains("refusing to regenerate unrelated recording"),
            "{unrelated}: {error}"
        );
    }
}

/// Replay every example file, Plain and rich, against a real server and fixture.
#[test]
fn plain_examples_replay_against_the_real_binary() {
    let mut fragments = Vec::new();
    for name in MAINTAINED_RECORDINGS {
        // Managed commit drafts need Unix-only primitives (`openat`,
        // `O_NOFOLLOW`, `nlink` and effective-uid checks), so the server
        // refuses the draft elsewhere and the two commit-edit recordings
        // cannot replay there.
        if !cfg!(unix) && name.starts_with("commit-edit-") {
            continue;
        }
        let lines = example_lines(name);
        let captured = replay_example(name, &lines).unwrap_or_else(|error| panic!("{error}"));
        if std::env::var_os("MAGIT_LSP_REGENERATE_EXAMPLES").is_some() {
            fragments.extend(
                regenerated_fragments(name, &captured).unwrap_or_else(|error| panic!("{error}")),
            );
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
        let response = lines
            .iter_mut()
            .find(|line| {
                line.dir == Dir::ServerToClient
                    && line.message.get("result").is_some_and(Value::is_array)
            })
            .unwrap();
        response.message["result"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        let error =
            replay_example(name, &lines).expect_err("reordered actions must fail normal replay");
        assert!(
            error.contains("recorded s2c frame mismatch at $.result[0]"),
            "{error}"
        );
    }
}

/// The recorded final commit-edit range end matches the canonical root.
///
/// The final applyEdit of `commit-edit-confirm` replaces `Staged changes (`
/// through the commit-message line, so its end is the length of the
/// `file:///repo/.git/COMMIT_EDITMSG` line in the recorded client text. The
/// recording stores that text with the placeholder root, so the stored range
/// must be computed against the same placeholder — never against a real
/// fixture root, whose length varies with `TMPDIR`.
#[test]
fn commit_edit_confirm_final_range_end_matches_canonical_root() {
    let lines = example_lines("commit-edit-confirm.jsonl");
    let mut final_edit = None;
    for (index, line) in lines.iter().enumerate() {
        let is_status_edit = line.dir == Dir::ServerToClient
            && line.message.get("method").and_then(Value::as_str) == Some("workspace/applyEdit")
            && line.message["params"]["edit"]["documentChanges"][0]["textDocument"]["uri"]
                == "file:///repo/.magit";
        if is_status_edit {
            final_edit = Some((
                index,
                line.message["params"]["edit"]["documentChanges"][0]["edits"][0].clone(),
            ));
        }
    }
    let (final_index, final_edit) =
        final_edit.expect("commit-edit-confirm records a status applyEdit");
    let client_text = lines[..final_index]
        .iter()
        .rev()
        .find(|line| {
            line.dir == Dir::ClientToServer
                && line.message.get("method").and_then(Value::as_str)
                    == Some("textDocument/didChange")
                && line.message["params"]["textDocument"]["uri"] == "file:///repo/.magit"
        })
        .and_then(|line| line.message["params"]["contentChanges"][0]["text"].as_str())
        .expect("a recorded client status text precedes the final edit")
        .to_owned();
    let canonical_lines: Vec<&str> = client_text.split('\n').collect();
    assert!(
        canonical_lines.len() > 20,
        "the status text must include the commit line"
    );
    assert_eq!(
        canonical_lines[20], "file:///repo/.git/COMMIT_EDITMSG",
        "the recorded commit line must use the canonical root"
    );
    let expected_end = json!({"line": 20, "character": canonical_lines[20].len()});
    assert_eq!(
        final_edit["range"]["end"], expected_end,
        "the recorded range end must match the canonical commit-message line"
    );
}

/// Two roots of different length and URI encoding produce identical canonical
/// frames.
///
/// Both sides carry the same logical document, target URI and edit, but each
/// side names its own root: the recording's ASCII placeholder and a long
/// native root whose URI form is percent-encoded. The document text carries
/// both the encoded URI root and the raw native root, so normalizing each side
/// must canonicalize every root occurrence and shift the addressed UTF-16
/// endpoint by the width it loses. A deliberately divergent range or text is
/// still rejected by strict comparison.
#[test]
fn root_normalization_is_invariant_to_root_encoding_and_length() {
    // Spaces, non-ASCII, `#` and `%` all change width under URI encoding.
    let long_root = std::env::temp_dir()
        .join("a deliberately long fixture root/é space#percent%")
        .to_string_lossy()
        .into_owned();
    let recorded_uri = format!("file://{RECORDED_ROOT}");
    let commit_uri = |uri_root: &str| format!("{uri_root}/.git/COMMIT_EDITMSG");
    let commit_line =
        |uri_root: &str, native: &str| format!("{} root {native}", commit_uri(uri_root));
    let document = |uri_root: &str, native: &str| {
        let mut text: String = (0..20).map(|index| format!("line{index}\n")).collect();
        text.push_str(&commit_line(uri_root, native));
        text.push('\n');
        text
    };
    let frame = |uri_root: &str, native: &str| {
        let commit_line = commit_line(uri_root, native);
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "workspace/applyEdit",
            "params": {"edit": {"documentChanges": [{
                "textDocument": {"uri": format!("{uri_root}/.magit"), "version": 5},
                "edits": [{
                    "newText": commit_uri(uri_root),
                    "range": {
                        "start": {"line": 2, "character": 16},
                        "end": {"line": 20, "character": commit_line.encode_utf16().count()},
                    },
                }],
            }]}},
        })
    };

    let canonical = |native: &str, uri_root: &str| {
        let mut normalizer = RootNormalizer::new(native);
        normalizer.note_document(&format!("{uri_root}/.magit"), &document(uri_root, native));
        canonical_frame(
            &frame(uri_root, native),
            &normalizer,
            RECORDED_BINARY,
            &mut HashNormalizer::new(),
        )
    };
    let short = canonical(RECORDED_ROOT, &recorded_uri);
    let long_uri = uri_from_path(Path::new(&long_root))
        .expect("absolute root has a file URI")
        .as_str()
        .to_owned();
    let long = canonical(&long_root, &long_uri);
    assert_eq!(
        short, long,
        "root encoding and length must not affect the canonical frame"
    );
    let placeholder_line = commit_line(&recorded_uri, RECORDED_ROOT);
    let edit = &short["params"]["edit"]["documentChanges"][0]["edits"][0];
    assert_eq!(edit["newText"], commit_uri(&recorded_uri));
    assert_eq!(
        edit["range"]["end"]["character"],
        placeholder_line.encode_utf16().count()
    );
    // The unaffected endpoint stays exact.
    assert_eq!(edit["range"]["start"]["character"], 16);

    // Strict comparison is retained: a wrong range or text still diverges
    // after both sides have been normalized.
    let mut divergent = long.clone();
    divergent["params"]["edit"]["documentChanges"][0]["edits"][0]["range"]["end"]["character"] =
        Value::from(0);
    assert_ne!(short, divergent);
    let mut divergent = long;
    divergent["params"]["edit"]["documentChanges"][0]["edits"][0]["newText"] =
        Value::String("file:///repo/.git/OTHER".to_owned());
    assert_ne!(short, divergent);
}

/// A status URI reduces to the recorded single-separator form for any root.
///
/// `status_uri` writes one separator after the scope and then the root with
/// its leading separators trimmed, which a POSIX root supplies and a Windows
/// drive or verbatim-prefixed root does not. Replacing the live root with the
/// recording's placeholder must restore the form the recorded exchange pins
/// (`rich-lifecycle.jsonl:8`) without moving a POSIX reduction. The harness and
/// the server reach one directory through different spellings — a Windows 8.3
/// short name vs its long form, a symlinked component vs its target, or Git's
/// forward separators vs the native form — and every such pair must reduce.
#[test]
fn status_uri_reduces_to_the_recorded_form_for_any_platform_root() {
    let recorded_uri = "magit://status/repo";
    let recorded_frame = canonical_frame(
        &json!({"id": 2, "result": {"id": "v1", "kind": "status", "uri": recorded_uri}}),
        &RootNormalizer::new(RECORDED_ROOT),
        RECORDED_BINARY,
        &mut HashNormalizer::new(),
    );
    // Synthetic roots only: on Linux a Windows root has no file URI, which
    // `canonical_root` already tolerates, and the status URI embeds the native
    // root rather than its encoded form on either platform. Each pair is the
    // harness's spelling then the server's: Git reports Windows roots with
    // forward separators while the native form keeps backslashes.
    let mut cases: Vec<(String, String)> = vec![
        ("/tmp/fixture-root".into(), "/tmp/fixture-root".into()),
        (r"C:\Users\me\repo".into(), r"C:\Users\me\repo".into()),
        (
            r"\\?\C:\Users\me\repo".into(),
            r"\\?\C:\Users\me\repo".into(),
        ),
        (r"C:\Users\me\repo".into(), r"C:/Users/me/repo".into()),
        (r"C:/Users/me/repo".into(), r"C:\Users\me\repo".into()),
    ];
    // One real directory, two spellings: the fixture's form and the resolved
    // form the server's Git discovery can report instead. On Windows that is
    // the long form of an 8.3 name, which canonicalization returns; on Unix a
    // symlinked component is the same divergence.
    let dir = tempfile::tempdir().expect("tempdir");
    let raw = dir.path().to_string_lossy().into_owned();
    let resolved = fs::canonicalize(dir.path())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| raw.clone());
    cases.push((raw.clone(), resolved.clone()));
    #[cfg(unix)]
    {
        let link = dir.path().join("spelling");
        std::os::unix::fs::symlink(dir.path(), &link).expect("symlink");
        cases.push((link.to_string_lossy().into_owned(), resolved));
    }

    for (root, server) in cases {
        let live = magit_lsp::session::status_uri(&server);
        assert!(
            live.starts_with("magit://status/"),
            "{server}: the construction keeps one separator after the scope"
        );
        assert_ne!(
            live, recorded_uri,
            "{server}: the live URI names this run's root"
        );
        let canonical = canonical_frame(
            &json!({"id": 2, "result": {"id": "v1", "kind": "status", "uri": live}}),
            &RootNormalizer::new(&root),
            RECORDED_BINARY,
            &mut HashNormalizer::new(),
        );
        assert_eq!(
            canonical, recorded_frame,
            "{root} vs {server}: the status URI must reduce to the recorded form"
        );
    }
}

/// Expanding a recorded client frame encodes live file URIs with the shared
/// native-path encoder and keeps filesystem-path substitutions native.
///
/// Windows temporary roots contain backslashes; splicing those into a URI
/// carved out of the recording produced an invalid `file://` URI (the CI
/// failures: `invalid escape` while expanding a client frame, then the server
/// rejecting the document). Expansion operates on parsed string values, so a
/// URI position takes the encoder's percent-encoded output while a position
/// the protocol defines as a filesystem path keeps the raw native root.
#[test]
fn expanding_a_recorded_client_frame_encodes_live_file_uris() {
    // Roots whose native text differs from their URI form on each platform.
    #[cfg(unix)]
    let root = "/tmp/magit lsp répo#percent%/checkout";
    #[cfg(windows)]
    let root = r"C:\Users\runner\magit lsp répo#percent%\checkout";
    let recorded = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": {"uri": "file:///repo/.magit", "version": 5},
            "contentChanges": [
                {"text": "file:///repo/.git/COMMIT_EDITMSG\nroot /repo\n"}
            ],
        },
    });
    let expanded = expand_client_frame(&recorded, root, None);
    let root_uri = uri_from_path(Path::new(root)).expect("absolute root has a file URI");
    let status_uri = format!("{}/.magit", root_uri.as_str());
    assert_eq!(
        expanded["params"]["textDocument"]["uri"], status_uri,
        "the URI is encoded, not a native root spliced into file://"
    );
    let expected_path = Path::new(root).join(".magit");
    assert_eq!(
        path_from_uri(&status_uri).as_deref(),
        Some(expected_path.as_path()),
        "the encoded URI round-trips to the live path"
    );
    assert_eq!(
        expanded["params"]["contentChanges"][0]["text"],
        format!("{}/.git/COMMIT_EDITMSG\nroot {root}\n", root_uri.as_str()),
        "document URI text is encoded while a filesystem path stays native"
    );
    // The expanded frame stays valid JSON, so the client driver can send it.
    serde_json::to_string(&expanded).expect("the expanded frame serialises");

    // A verbatim Windows root is canonicalized by the same encoder.
    #[cfg(windows)]
    {
        let verbatim = r"\\?\C:\Users\runner\magit lsp";
        let expanded = expand_client_frame(&recorded, verbatim, None);
        assert_eq!(
            expanded["params"]["textDocument"]["uri"],
            "file:///C:/Users/runner/magit%20lsp/.magit"
        );
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
    assert_eq!(
        captured.len(),
        stale.len(),
        "regeneration keeps every recorded line"
    );
    let mut reparsed: Vec<Line> = captured
        .iter()
        .map(|raw| parse_line(raw).expect("captured lines parse"))
        .collect();
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
///
/// Unix only: the recorded commit draft runs through the managed-draft
/// boundary, which needs Unix-only primitives (`openat`, `O_NOFOLLOW`, `nlink`
/// and effective-uid checks), so the server refuses the draft elsewhere.
#[cfg(unix)]
#[test]
fn regeneration_captures_registry_changes_and_strict_replay_rejects_them() {
    // Same rule as the render case: build the stale registry here so the test
    // does not require the committed recording to lag the server.
    let current = example_lines("commit-edit-abort.jsonl");
    let index = current
        .iter()
        .position(|line| {
            line.dir == Dir::ServerToClient
                && line.message.get("result").is_some_and(Value::is_array)
        })
        .expect("the recorded code-action response");
    replay_example_mode("commit-edit-abort.jsonl", &current, false)
        .expect("the committed recording replays strictly before it is perturbed");
    let mut stale: Vec<Line> = current
        .iter()
        .map(|line| parse_line(&line.raw).expect("recorded lines reparse"))
        .collect();
    stale[index].message["result"][0]["title"] =
        Value::String("Stale: not the current registry".to_owned());
    let error = replay_example_mode("commit-edit-abort.jsonl", &stale, false)
        .expect_err("strict replay must reject the stale recording");
    assert!(error.contains("recorded s2c frame mismatch"), "{error}");
    let captured = replay_example_mode("commit-edit-abort.jsonl", &stale, true)
        .expect("regeneration captures the current code actions");
    let mut reparsed: Vec<Line> = captured
        .iter()
        .map(|raw| parse_line(raw).expect("captured lines parse"))
        .collect();
    replay_example_mode("commit-edit-abort.jsonl", &reparsed, false)
        .expect("captured frames replay strictly against the same server");
    assert_ne!(
        reparsed[index].message["result"][0]["title"], stale[index].message["result"][0]["title"],
        "regeneration replaced the stale action with the current registry entry"
    );
    reparsed[index].message["result"][0]["title"] =
        Value::String("Drifted: not the current registry".to_owned());
    let error = replay_example_mode("commit-edit-abort.jsonl", &reparsed, false)
        .expect_err("strict replay must reject the changed action");
    assert!(
        error.contains("recorded s2c frame mismatch at $.result[0].title"),
        "{error}"
    );
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
    assert_eq!(
        parse_line(&captured).expect("the captured line parses").dir,
        Dir::ServerToClient
    );
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
                && line.message.get("method").and_then(Value::as_str)
                    == Some("textDocument/didOpen")
        })
        .expect("the recorded document open");
    let echo = current
        .iter()
        .position(|line| {
            line.dir == Dir::ClientToServer
                && line.message.get("method").and_then(Value::as_str)
                    == Some("textDocument/didChange")
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
                if let Some(changes) =
                    line.message["params"]["edit"]["documentChanges"].as_array_mut()
                {
                    for change in changes {
                        if let Some(edits) = change["edits"].as_array_mut() {
                            for edit in edits {
                                if let Some(text) = edit["newText"].as_str() {
                                    if text.contains("Head: main") {
                                        edit["newText"] = Value::String(
                                            text.replace("Head: main", "Head: stale"),
                                        );
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
                    if let Some(text) = line.message["params"]["contentChanges"][0]["text"].as_str()
                    {
                        line.message["params"]["contentChanges"][0]["text"] =
                            Value::String(text.replace("Head: main", "Head: stale"));
                        replaced += 1;
                    }
                }
            }
            _ => {}
        }
    }
    assert!(
        replaced >= 3,
        "the stale recording keeps its renders and echoes coherent"
    );
    let opened = stale[open].message["params"]["textDocument"]["text"]
        .as_str()
        .expect("the recorded opened text")
        .to_owned();
    let stale_text = stale[render].message["params"]["edit"]["documentChanges"][0]["edits"][0]
        ["newText"]
        .as_str()
        .expect("the recorded render text")
        .to_owned();
    let stale_range =
        stale[render].message["params"]["edit"]["documentChanges"][0]["edits"][0]["range"].clone();
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
    assert!(
        error.contains("recorded s2c frame mismatch") && error.contains("Head: stale"),
        "{error}"
    );

    let captured = replay_example_mode(name, &stale, true)
        .expect("regeneration refreshes the bound echo and captures the rest");
    let reparsed: Vec<Line> = captured
        .iter()
        .map(|raw| parse_line(raw).expect("captured lines parse"))
        .collect();
    replay_example_mode(name, &reparsed, false)
        .expect("the refreshed echo and the captured frames replay strictly");

    let observed_text = reparsed[render].message["params"]["edit"]["documentChanges"][0]["edits"]
        [0]["newText"]
        .as_str()
        .expect("the captured render text");
    let observed_range = reparsed[render].message["params"]["edit"]["documentChanges"][0]["edits"]
        [0]["range"]
        .clone();
    let expected_echo = apply_text_edit(&opened, &observed_range, observed_text)
        .expect("the observed edit applies");
    let observed_echo = reparsed[echo].message["params"]["contentChanges"][0]["text"]
        .as_str()
        .expect("the refreshed echo text");
    assert_ne!(
        observed_echo, stale_echo,
        "regeneration replaced the stale echo"
    );
    assert_eq!(
        observed_echo, expected_echo,
        "the echo is the observed server edit applied to the opened text"
    );
    assert!(
        observed_echo.contains("Keys: off (built-in)"),
        "the echo carries the current render"
    );
}

/// Regeneration never rewrites authored client edits: a commit message the
/// client typed itself stays exactly as recorded.
///
/// Unix only: the recorded commit draft runs through the managed-draft
/// boundary, which needs Unix-only primitives (`openat`, `O_NOFOLLOW`, `nlink`
/// and effective-uid checks), so the server refuses the draft elsewhere.
#[cfg(unix)]
#[test]
fn regeneration_echo_leaves_authored_client_edits_unchanged() {
    let name = "commit-edit-confirm.jsonl";
    let captured = replay_example_mode(name, &example_lines(name), true)
        .expect("regeneration captures the commit example");
    let reparsed: Vec<Line> = captured
        .iter()
        .map(|raw| parse_line(raw).expect("captured lines parse"))
        .collect();
    replay_example_mode(name, &reparsed, false)
        .expect("the captured commit example replays strictly");
    // The captured echo of the pending-draft row must persist the recording's
    // root placeholder, never this run's temporary repository root; a stale
    // run root makes the strict replay above fail on the next run.
    let pending_echo = reparsed
        .iter()
        .find(|line| {
            line.dir == Dir::ClientToServer
                && line.message.get("method").and_then(Value::as_str)
                    == Some("textDocument/didChange")
                && line.message["params"]["textDocument"]["uri"]
                    .as_str()
                    .is_some_and(|uri| uri.ends_with(".magit"))
                && line.message["params"]["contentChanges"][0]["text"]
                    .as_str()
                    .is_some_and(|text| text.contains("/.git/COMMIT_EDITMSG"))
        })
        .map(|line| {
            line.message["params"]["contentChanges"][0]["text"]
                .as_str()
                .unwrap()
        })
        .expect("the captured pending-draft echo");
    let pending_uri = format!("file://{RECORDED_ROOT}/.git/COMMIT_EDITMSG");
    assert!(
        pending_echo.contains(&pending_uri),
        "the captured echo keeps the recording's repository placeholder: {pending_echo}"
    );
    let placeholder_prefix = format!("file://{RECORDED_ROOT}/");
    for (start, _) in pending_echo.match_indices("file://") {
        assert!(
            pending_echo[start..].starts_with(&placeholder_prefix),
            "a captured echo must not retain a run-specific root: {pending_echo}"
        );
    }
    let authored = reparsed
        .iter()
        .find_map(|line| {
            let is_authored = line.dir == Dir::ClientToServer
                && line.message.get("method").and_then(Value::as_str)
                    == Some("textDocument/didChange")
                && line.message["params"]["textDocument"]["uri"]
                    .as_str()
                    .is_some_and(|uri| uri.ends_with(".git/COMMIT_EDITMSG"));
            is_authored.then(|| {
                line.message["params"]["contentChanges"][0]["text"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
        })
        .expect("the authored commit-message change");
    assert_eq!(
        authored, "Recorded commit body\n",
        "an authored change is never rewritten"
    );
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
                && line.message.get("method").and_then(Value::as_str)
                    == Some("textDocument/didChange")
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
    assert!(
        error.contains("identity changed"),
        "the server's correction is refused: {error}"
    );

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
                && line.message["params"]["version"] == 2
        })
        .expect("the recorded fresh rich render");
    render.message["params"]["viewId"] =
        Value::String("v-not-the-recorded-fresh-render".to_owned());
    let error = replay_example("rich-lifecycle.jsonl", &lines)
        .expect_err("a divergent fresh rich frame must fail the replay");
    assert!(
        error.contains("recorded s2c frame mismatch")
            && error.contains("v-not-the-recorded-fresh-render"),
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
    let roots = RootNormalizer::new(RECORDED_ROOT);
    let expected = canonical_frame(&frame, &roots, RECORDED_BINARY, &mut recorded);
    let actual = canonical_frame(&frame, &roots, RECORDED_BINARY, &mut observed);
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
    let renumbered = canonical_frame(&only_second, &roots, RECORDED_BINARY, &mut other);
    assert_eq!(renumbered["result"]["text"][0], "parent hash-1");
}

#[test]
fn mismatch_message_reports_differing_values_and_path() {
    let expected = serde_json::json!({"result":[{"command":"a"},{"command":"b"}]});
    let observed = serde_json::json!({"result":[{"command":"a"},{"command":"c"}]});
    let (path, expected_value, observed_value) = first_difference(&expected, &observed).unwrap();
    let message = format!(
        "at {path} expected: {} observed: {}",
        json_excerpt(expected_value),
        json_excerpt(observed_value)
    );
    assert!(message.contains("$.result[1].command"));
    assert!(message.contains(r#"expected: "\"b\"""#));
    assert!(message.contains(r#"observed: "\"c\"""#));
}

#[test]
fn json_excerpt_bounds_long_values() {
    let value = Value::String("x".repeat(500));
    let excerpt = json_excerpt(Some(&value));
    assert_eq!(
        excerpt.split("...").next().unwrap().matches('x').count(),
        239
    );
    assert!(excerpt.contains("500 chars total"));
    assert!(!excerpt.contains(&"x".repeat(500)));
}

#[test]
fn mismatch_message_includes_bounded_server_stderr_tail() {
    let expected = serde_json::json!({"result": {"text": ["expected"]}});
    let observed = serde_json::json!({"result": {"text": ["observed"]}});

    let message = s2c_mismatch_message(
        "plain-status.jsonl:42",
        &expected,
        &observed,
        "log line\r\nwarning: boom\n",
    );
    assert!(message.contains("plain-status.jsonl:42: recorded s2c frame mismatch"));
    assert!(message.contains("server stderr tail: log line\nwarning: boom"));
    assert!(!message.contains('\r'));

    let empty = s2c_mismatch_message("plain-status.jsonl:42", &expected, &observed, "");
    assert!(empty.contains("server stderr tail: <empty>"));

    let noisy = "x".repeat(STDERR_TAIL_LIMIT + 100);
    let bounded = s2c_mismatch_message("plain-status.jsonl:42", &expected, &observed, &noisy);
    assert!(bounded.contains(&format!("...{}", "x".repeat(STDERR_TAIL_LIMIT))));
    assert!(!bounded.contains(&noisy));
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

/// Capabilities for a Plain client that accepts edits and shows documents.
fn plain_capabilities() -> Value {
    json!({
        "general": {"positionEncodings": ["utf-8", "utf-16"]},
        "textDocument": {"codeAction": {"disabledSupport": true}},
        "window": {"showDocument": {"support": true}},
        "workspace": {
            "applyEdit": true,
            "executeCommand": {"dynamicRegistration": true},
            "workspaceEdit": {"documentChanges": true}
        }
    })
}

/// Open the repository's `.magit` status document as a Plain client.
fn open_plain_status(
    repo: &fixtures::RepoFixture,
) -> (fixtures::stdio::TestServer, FakeEditor, String) {
    let status_uri = uri_from_path(&repo.path().join(".magit"))
        .expect("native status path has a file URI")
        .as_str()
        .to_owned();
    let root_uri = uri_from_path(repo.path())
        .expect("native root has a file URI")
        .as_str()
        .to_owned();
    let mut server = fixtures::stdio::TestServer::spawn(&[]);
    let mut init = fixtures::stdio::initialize_message();
    init["params"]["capabilities"] = plain_capabilities();
    init["params"]["workspaceFolders"] = json!([{"uri": root_uri, "name": "repo"}]);
    server.send(init);
    let initialized = server.response_with_id(1);
    assert!(
        initialized["result"]["capabilities"]["codeActionProvider"] == true,
        "Plain initialize failed: {initialized}"
    );
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor = FakeEditor::new(Profile::Standard);
    server.send(editor.open(&status_uri, "stale\n"));
    loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
            break;
        }
    }
    (server, editor, status_uri)
}

/// Send one request, answering server requests, and keep every other frame.
///
/// Returns the response carrying the request's id; `frames` collects the
/// notifications and requests observed first, so callers can assert on the
/// edits the server published before answering.
fn plain_request(
    server: &mut fixtures::stdio::TestServer,
    editor: &mut FakeEditor,
    frames: &mut Vec<Value>,
    message: Value,
) -> Value {
    let id = message["id"].clone();
    server.send(message);
    loop {
        let received = server.recv();
        if received["method"].is_string() {
            frames.push(received.clone());
            if !received["id"].is_null() {
                for reply in editor.dispatch(&received) {
                    server.send(reply);
                }
            }
            continue;
        }
        if received["id"] == id {
            return received;
        }
        frames.push(received);
    }
}

/// Request code actions for one status or draft line.
fn plain_code_actions(
    server: &mut fixtures::stdio::TestServer,
    editor: &mut FakeEditor,
    frames: &mut Vec<Value>,
    uri: &str,
    line: u32,
) -> Vec<Value> {
    let id = fake_editor::allocate_request_id() + 1000;
    let response = plain_request(
        server,
        editor,
        frames,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/codeAction",
            "params": {
                "textDocument": {"uri": uri},
                "range": {
                    "start": {"line": line, "character": 0},
                    "end": {"line": line, "character": 0}
                },
                "context": {"diagnostics": []}
            }
        }),
    );
    response["result"].as_array().cloned().unwrap_or_default()
}

/// The semantic arguments of the offered action whose command is `command`.
fn action_args<'a>(actions: &'a [Value], command: &str) -> Option<&'a Value> {
    actions.iter().find_map(|action| {
        let args = &action["command"]["arguments"][0];
        (args["command"] == command).then_some(args)
    })
}

/// Execute one offered Plain action and return the command response.
fn plain_execute(
    server: &mut fixtures::stdio::TestServer,
    editor: &mut FakeEditor,
    frames: &mut Vec<Value>,
    args: &Value,
) -> Value {
    let id = fake_editor::allocate_request_id() + 1000;
    plain_request(
        server,
        editor,
        frames,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "workspace/executeCommand",
            "params": {"command": "magit.command", "arguments": [args]}
        }),
    )
}

/// Ask the session for the same refresh entry the file watcher uses.
fn plain_refresh(
    server: &mut fixtures::stdio::TestServer,
    editor: &mut FakeEditor,
    frames: &mut Vec<Value>,
    view_id: &str,
) -> Value {
    let id = fake_editor::allocate_request_id() + 1000;
    plain_request(
        server,
        editor,
        frames,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "view/invoke",
            "params": {"viewId": view_id, "command": "common.refresh"}
        }),
    )
}

/// Whether a server request is an edit of `uri`.
fn edits_uri(frame: &Value, uri: &str) -> bool {
    frame["method"] == "workspace/applyEdit"
        && frame["params"]["edit"]["documentChanges"]
            .as_array()
            .is_some_and(|changes| {
                changes
                    .iter()
                    .any(|change| change["textDocument"]["uri"] == uri)
            })
}

/// Direct `commit.create` delivers the pending row before an unchanged refresh.
#[cfg(unix)]
#[test]
fn direct_commit_create_publishes_pending_draft_row_before_refresh() {
    let (repo, _git) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .staged_file("f.txt", "after\n")
        .build();
    let draft_uri = uri_from_path(&repo.path().join(".git/COMMIT_EDITMSG"))
        .expect("native draft path has a file URI")
        .as_str()
        .to_owned();
    let (mut server, mut editor, status_uri) = open_plain_status(&repo);
    let mut frames = Vec::new();
    let line = editor
        .text(&status_uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .unwrap() as u32;
    let actions = plain_code_actions(&mut server, &mut editor, &mut frames, &status_uri, line);
    let open = action_args(&actions, "open.commit").expect("status offers commit");
    let view_id = open["target"]["viewId"]
        .as_str()
        .expect("view id")
        .to_owned();
    // The adapter opens the editor document before a commit operation can
    // wait for the user to edit and confirm it.
    server.send(editor.open(&draft_uri, "stale\n"));
    frames.clear();
    let response = plain_request(
        &mut server,
        &mut editor,
        &mut frames,
        json!({
            "jsonrpc":"2.0", "id":fake_editor::allocate_request_id()+1000, "method":"view/invoke",
            "params":{"viewId":view_id,"command":"commit.create"}
        }),
    );
    assert!(
        response.get("error").is_none(),
        "direct commit.create failed: {response}"
    );
    let published = editor.text(&status_uri).to_owned();
    assert!(
        published.contains(".git/COMMIT_EDITMSG"),
        "initial direct invocation delivers row: {published}"
    );
    assert_eq!(
        frames
            .iter()
            .filter(|frame| edits_uri(frame, &status_uri))
            .count(),
        1,
        "one initial status edit: {frames:#?}"
    );
    server.send(editor.open(&draft_uri, "stale\n"));
    frames.clear();
    let refreshed = plain_refresh(&mut server, &mut editor, &mut frames, &view_id);
    assert!(
        refreshed.get("error").is_none(),
        "refresh failed: {refreshed}"
    );
    assert!(
        !frames.iter().any(|frame| edits_uri(frame, &status_uri)),
        "unchanged refresh emits no status edit"
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while editor.text(&draft_uri) == "stale\n" {
        assert!(
            std::time::Instant::now() < deadline,
            "commit template never attached"
        );
        let received = server.recv();
        if received["method"].is_string() && !received["id"].is_null() {
            for frame in editor.dispatch(&received) {
                server.send(frame);
            }
        }
    }
    let actions = plain_code_actions(&mut server, &mut editor, &mut frames, &draft_uri, 0);
    let abort = action_args(&actions, "common.cancel").expect("draft offers abort");
    let aborted = plain_execute(&mut server, &mut editor, &mut frames, abort);
    assert_eq!(aborted["result"]["type"], "aborted");
    frames.clear();
    let _ = plain_refresh(&mut server, &mut editor, &mut frames, &view_id);
    assert!(
        !editor.text(&status_uri).contains(".git/COMMIT_EDITMSG"),
        "abort clears pending row"
    );
    server.shutdown_and_exit();
    let (exit, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(exit.success());
}

/// A commit run's published status already contains the pending draft row.
///
/// The draft is admitted at the operation boundary, after the command's status
/// render is published, so the server must re-publish the focused status once
/// the draft exists. Otherwise the operation publishes a status document
/// without the row and the next watcher refresh appends it as a second edit.
/// This drives the real Plain server through the offered commit action and then
/// invokes the watcher's refresh entry: an unchanged repository must emit no
/// further status edit while the draft is pending.
#[cfg(unix)]
#[test]
fn commit_start_publishes_pending_draft_row_before_any_refresh() {
    let (repo, _git) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .staged_file("f.txt", "after\n")
        .build();
    let draft_uri = uri_from_path(&repo.path().join(".git/COMMIT_EDITMSG"))
        .expect("native draft path has a file URI")
        .as_str()
        .to_owned();
    let (mut server, mut editor, status_uri) = open_plain_status(&repo);
    let mut frames = Vec::new();

    // Open the commit menu from the staged row.
    let staged_line = editor
        .text(&status_uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .expect("status shows the staged f.txt row") as u32;
    let actions = plain_code_actions(
        &mut server,
        &mut editor,
        &mut frames,
        &status_uri,
        staged_line,
    );
    let open_commit = action_args(&actions, "open.commit")
        .expect("open.commit is offered for the staged row")
        .clone();
    let view_id = open_commit["target"]["viewId"]
        .as_str()
        .expect("the offered action names the status view")
        .to_owned();
    let opened = plain_execute(&mut server, &mut editor, &mut frames, &open_commit);
    assert!(
        opened.get("error").is_none(),
        "open.commit failed: {opened}"
    );

    // Pre-open the draft buffer so the adapter attaches the commit template.
    server.send(editor.open(&draft_uri, "stale\n"));

    // Find the menu's commit-creation item.
    let mut create = None;
    let menu_lines = editor.text(&status_uri).lines().count() as u32;
    'menu: for line in 0..menu_lines {
        let actions = plain_code_actions(&mut server, &mut editor, &mut frames, &status_uri, line);
        for action in actions {
            let args = &action["command"]["arguments"][0];
            if args["command"] == "menu.run-action" && args["target"]["itemKey"].is_string() {
                create = Some(args.clone());
                break 'menu;
            }
        }
    }
    let create = create.unwrap_or_else(|| {
        panic!(
            "the commit menu offers its creation item; document:\n{}\nframes:\n{:#?}",
            editor.text(&status_uri),
            frames
        )
    });
    let before_commit = editor.text(&status_uri).to_owned();

    // The operation boundary: the draft is admitted and the status republished.
    frames.clear();
    let committed = plain_execute(&mut server, &mut editor, &mut frames, &create);
    assert!(
        committed.get("error").is_none(),
        "the commit command failed: {committed}"
    );
    let published = editor.text(&status_uri).to_owned();
    assert_ne!(
        published, before_commit,
        "the commit command republishes the status document"
    );
    assert!(
        published.contains(".git/COMMIT_EDITMSG"),
        "the operation's published status already includes the pending draft row: {published}"
    );

    // The watcher's refresh entry must see an unchanged tree for the pending run.
    frames.clear();
    let refreshed = plain_refresh(&mut server, &mut editor, &mut frames, &view_id);
    assert!(
        refreshed.get("error").is_none(),
        "refresh failed: {refreshed}"
    );
    assert!(
        !frames.iter().any(|frame| edits_uri(frame, &status_uri)),
        "an unchanged repository emits no extra status applyEdit while the draft is pending: {frames:#?}"
    );
    assert_eq!(
        editor.text(&status_uri),
        published,
        "the pending status text is unchanged by the refresh"
    );

    // The template edit attaches the pre-opened draft; abort to end the run.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while editor.text(&draft_uri) == "stale\n" {
        assert!(
            std::time::Instant::now() < deadline,
            "the commit template never attached to the pre-opened draft"
        );
        let received = server.recv();
        if received["method"].is_string() && !received["id"].is_null() {
            for frame in editor.dispatch(&received) {
                server.send(frame);
            }
        }
    }
    let actions = plain_code_actions(&mut server, &mut editor, &mut frames, &draft_uri, 0);
    let abort =
        action_args(&actions, "common.cancel").expect("the attached draft offers Abort commit");
    let aborted = plain_execute(&mut server, &mut editor, &mut frames, abort);
    assert!(aborted.get("error").is_none(), "Abort failed: {aborted}");
    assert_eq!(aborted["result"]["type"], "aborted");

    frames.clear();
    let restored = plain_refresh(&mut server, &mut editor, &mut frames, &view_id);
    assert!(
        restored.get("error").is_none(),
        "refresh failed: {restored}"
    );
    let status = editor.text(&status_uri).to_owned();
    assert!(
        !status.contains(".git/COMMIT_EDITMSG"),
        "the aborted draft's row is gone: {status}"
    );
    assert!(
        status.contains("f.txt"),
        "the status view is restored after the abort: {status}"
    );

    server.shutdown_and_exit();
    let (exit, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(exit.success());
}

/// A green canary for the commit-edit replay under an eager watcher.
///
/// Three focused runs at a zero debounce were green before the
/// publication-window fix — the recorded interleave is macOS-timing, not
/// Linux-forcing — so this test is a canary, not the RED regression (the
/// deterministic proof lives in the server_async suite). It stays green
/// through the fix: with git's own writes scheduling watcher bursts against
/// the commit's publication window, strict replay requires each burst to
/// coalesce into the identical-tree no-op refresh.
///
/// Unix only: the recorded commit draft runs through the managed-draft
/// boundary, which needs Unix-only primitives (`openat`, `O_NOFOLLOW`, `nlink`
/// and effective-uid checks), so the server refuses the draft elsewhere.
#[cfg(unix)]
#[test]
fn commit_edit_confirm_replays_with_an_eager_watcher() {
    let name = "commit-edit-confirm.jsonl";
    replay_example_env(
        name,
        &example_lines(name),
        false,
        &[("MAGIT_LSP_WATCH_DEBOUNCE_MS", "0")],
    )
    .expect("the commit example replays strictly with an eager watcher");
}
