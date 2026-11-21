//! The real server over real pipes: framing, pipelining, concurrency, ordering.

mod common;
#[path = "server_async/repository.rs"]
mod repository;
#[path = "server_async/rich.rs"]
mod rich;

use repository::keymap_keys;
use rich::{invoke_rich, limited_rich};

#[cfg(unix)]
use common::fake_git::GatedGit;
use common::stdio::*;
use common::*;
use serde_json::{json, Value};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

fn repo_param(path: &std::path::Path) -> Value {
    json!(path.to_string_lossy())
}

#[cfg(unix)]
#[test]
fn status_open_held_git_unacknowledged_completion_sends_full_text() {
    status_open_held_git(false);
}

#[cfg(unix)]
#[test]
fn status_open_held_git_acknowledged_completion_uses_exact_base() {
    status_open_held_git(true);
}

#[cfg(unix)]
fn status_open_held_git(acknowledge: bool) {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    write(&tmp.path().join("status-open-fresh.txt"), "fresh status\n");
    let gate = GatedGit::arm("status");
    let env = gate.child_env();
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let mut server = TestServer::spawn(&env);
    server.initialize_rich(json!({}));
    server.recv_timeout = Duration::from_secs(3);
    server.send(open_view_message(2, &repo_param(tmp.path())));
    gate.wait_entered();

    // Use the raw transport: ready-state helpers would hide early-open failures.
    let opened = server.recv();
    assert_eq!(opened["id"], 2, "descriptor precedes notifications");
    assert!(opened["error"].is_null(), "{opened}");
    let view = opened["result"]["id"].as_str().expect("real view id");
    assert!(!view.is_empty());
    assert!(opened["result"]["uri"]
        .as_str()
        .is_some_and(|uri| !uri.is_empty()));
    let loading = server.recv();
    assert_eq!(loading["method"], "view/render");
    assert_eq!(loading["params"]["viewId"], view);
    assert_eq!(
        loading["params"]["text"],
        json!(["Loading repository status…"])
    );
    let version = loading["params"]["version"].as_u64().unwrap();
    assert_eq!(server.recv()["method"], "view/keymap");
    assert!(
        !gate.is_released(),
        "descriptor and honest loading arrive while Git is held"
    );

    if acknowledge {
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
            "params":{"viewId":view,"version":version}}));
    }
    // This request must answer while acquisition is still blocked; it also
    // orders the optional ack before the completion can be published.
    server.send(
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/foldingRange",
        "params":{"textDocument":{"uri":opened["result"]["uri"]}}}),
    );
    let responsive = server.response_with_id(3);
    assert_eq!(responsive["result"], json!([]), "loading has no file folds");
    assert!(
        !gate.is_released(),
        "another request answered before release"
    );

    gate.release();
    server.recv_timeout = Duration::from_secs(10);
    let ready = server.wait_for_view_render_after(view, version);
    assert_eq!(ready["params"]["viewId"], view);
    let text: Vec<String> = if acknowledge {
        assert_eq!(ready["params"]["baseVersion"], version);
        assert!(ready["params"].get("text").is_none(), "{ready}");
        let edits: Vec<magit_lsp::protocol::LineEdit> =
            serde_json::from_value(ready["params"]["edits"].clone()).unwrap();
        magit_lsp::diff_text::apply_edits(&["Loading repository status…".into()], &edits)
    } else {
        assert!(ready["params"].get("baseVersion").is_none(), "{ready}");
        assert!(ready["params"].get("edits").is_none(), "{ready}");
        serde_json::from_value(ready["params"]["text"].clone()).unwrap()
    };
    assert!(
        text.iter()
            .any(|line| line.contains("status-open-fresh.txt")),
        "{text:?}"
    );
    assert!(
        text.iter().any(|line| line.contains("+line3 CHANGED")),
        "{text:?}"
    );
    assert!(!text
        .iter()
        .any(|line| line.contains("Loading repository status")));
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/resync",
        "params":{"viewId":view}}));
    let resync = server.response_with_id(4);
    assert_eq!(resync["result"]["version"], ready["params"]["version"]);
    assert_eq!(resync["result"]["text"], json!(text));
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn opening_multirow_rebase_todo_batches_object_resolution_once() {
    for rows in [2, 5] {
        let mut fixture = RepoFixture::new().commit("root", [("f.txt", "0\n")]);
        for index in 1..=rows + 1 {
            fixture = fixture.commit(
                &format!("commit-{index}"),
                [("f.txt", format!("{index}\n"))],
            );
        }
        let (tmp, _) = fixture.build();
        let pause = Command::new("git")
            .args(["rebase", "-i", &format!("HEAD~{}", rows + 1)])
            .current_dir(tmp.path())
            .env("GIT_SEQUENCE_EDITOR", "sed -i '1s/^pick /edit /'")
            .output()
            .unwrap();
        assert!(
            pause.status.success(),
            "{}",
            String::from_utf8_lossy(&pause.stderr)
        );
        let trace = tmp.path().join("trace.jsonl");
        let trace_path = trace.to_str().unwrap();
        let mut server = TestServer::spawn(&[("GIT_TRACE2_EVENT", trace_path)]);
        assert!(server.initialize_rich(json!({}))["error"].is_null());
        server.send(json!({"jsonrpc":"2.0","id":2,"method":"view/open",
            "params":{"kind":"rebase-todo","args":{"repo_path":repo_param(tmp.path())},
            "viewport":{"cols":80,"rows":24}}}));
        let opened = server.response_with_id(2);
        assert!(opened["error"].is_null(), "{opened}");
        let render = server.recv();
        assert_eq!(render["method"], "view/render");
        server.recv();
        server.shutdown_and_exit();
        assert!(server.finish(Duration::from_secs(10)).success());
        let events = read(&trace);
        let starts = events.lines().filter_map(|line| {
            let event: Value = serde_json::from_str(line).unwrap();
            (event["event"] == "start").then_some(event)
        });
        let count = starts
            .filter(|event| {
                event["argv"].as_array().is_some_and(|argv| {
                    argv.iter().any(|arg| arg == "cat-file")
                        && argv.iter().any(|arg| {
                            arg.as_str()
                                .is_some_and(|arg| arg.starts_with("--batch-check"))
                        })
                })
            })
            .count();
        assert_eq!(count, 1, "{rows} rows: {events}");
    }
}

#[test]
fn prepared_rebase_submit_through_real_server_keys_restores_caller() {
    let (tmp, _) = RepoFixture::new()
        .commit("base", [("base", "base\n")])
        .commit("one", [("one", "1\n")])
        .commit("two", [("two", "2\n")])
        .commit("three", [("three", "3\n")])
        .build();
    let base = tmp.git(&["rev-parse", "HEAD~3"]).trim().to_owned();
    let mut server = TestServer::spawn(&[]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));

    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(
            json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{
            "viewId":view,"version":1,"key":key,"editorMode":"normal",
            "cursor":{"line":line,"column":0}}}),
        );
        server.response_with_id(id)
    };
    let pushed = |response: &Value| {
        let descriptor = response["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|effect| effect["type"] == "pushView")
            .expect("pushView effect")["descriptor"]
            .clone();
        (
            descriptor["id"].as_str().unwrap().to_owned(),
            descriptor["uri"].as_str().unwrap().to_owned(),
        )
    };
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false;
        let mut keymap = false;
        let mut version = None;
        while !render || !keymap {
            let message = server.recv();
            if message["params"]["viewId"] == view {
                if message["method"] == "view/render" {
                    render = true;
                    version = message["params"]["version"].as_u64();
                }
                keymap |= message["method"] == "view/keymap";
            }
        }
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
            "params":{"viewId":view,"version":version.unwrap()}}));
    };

    let (menu, _) = pushed(&press(&mut server, 3, &status, "r", 0));
    opened(&mut server, &menu);
    let (selector, _) = pushed(&press(&mut server, 4, &menu, "i", 0));
    opened(&mut server, &selector);
    let mut base_line = None;
    for line in 0..100 {
        let id = 50 + line as i64;
        server.send(
            json!({"jsonrpc":"2.0","id":id,"method":"view/invoke","params":{
            "viewId":selector,"command":"common.copy-value","version":1,
            "cursor":{"line":line,"column":0}}}),
        );
        let copied = server.response_with_id(id);
        if copied["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "clipboard" && effect["text"] == base)
        {
            base_line = Some(line);
            break;
        }
    }
    let base_line = base_line.expect("server-owned base identity");
    server.send(
        json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{
        "viewId":selector,"command":"common.confirm","version":1,
        "cursor":{"line":base_line,"column":0}}}),
    );
    let (todo, _) = pushed(&server.response_with_id(6));
    opened(&mut server, &todo);

    assert!(press(&mut server, 8, &todo, "J", 0)["result"]["effects"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(server.recv()["method"], "view/render");
    assert!(press(&mut server, 9, &todo, "d", 2)["result"]["effects"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(server.recv()["method"], "view/render");
    let submitted = press(&mut server, 10, &todo, "ctrl+s", 0);
    let submit_effects = submitted["result"]["effects"].as_array().unwrap();
    assert!(
        submit_effects
            .iter()
            .any(|effect| effect["type"] == "popView" && effect["viewId"] == todo),
        "{submitted}"
    );
    assert!(
        submit_effects
            .iter()
            .any(|effect| effect["type"] == "focusView" && effect["viewId"] == status),
        "{submitted}"
    );
    assert_eq!(
        tmp.git(&["log", "--format=%s", "--reverse", &format!("{base}..HEAD")]),
        "two\none\n"
    );
    assert_eq!(tmp.git(&["show", "HEAD:one"]), "1\n");
    assert_eq!(tmp.git(&["show", "HEAD~1:two"]), "2\n");
    assert!(!tmp.path().join("three").exists());

    server.send(json!({"jsonrpc":"2.0","id":11,"method":"view/invoke",
        "params":{"viewId":status,"command":"common.refresh"}}));
    let refreshed = server.response_with_id(11);
    assert!(
        refreshed["result"]["effects"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{refreshed}"
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn instant_fixup_selection_autosquashes_through_real_server_and_restores_caller() {
    let (tmp, _) = RepoFixture::new()
        .commit("root subject", [("root.txt", "root\n")])
        .commit("base subject", [("base.txt", "base\n")])
        .commit("selected target subject", [("target.txt", "base\n")])
        .commit("newer head subject", [("newer.txt", "head\n")])
        .staged_file("fix.txt", "fixed\n")
        .build();
    let target = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let base = tmp.git(&["rev-parse", "HEAD~2"]).trim().to_owned();
    let root = tmp.git(&["rev-parse", "HEAD~3"]).trim().to_owned();
    let prior_head = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let mut server = TestServer::spawn(&[]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(
            json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{
            "viewId":view,"version":1,"key":key,"editorMode":"normal",
            "cursor":{"line":line,"column":0}}}),
        );
        server.response_with_id(id)
    };
    let pushed = |response: &Value| {
        response["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|effect| effect["type"] == "pushView")
            .expect("pushView")["descriptor"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false;
        let mut keymap = false;
        let mut version = None;
        while !render || !keymap {
            let message = server.recv();
            if message["params"]["viewId"] == view {
                if message["method"] == "view/render" {
                    render = true;
                    version = message["params"]["version"].as_u64();
                }
                keymap |= message["method"] == "view/keymap";
            }
        }
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{
            "viewId":view,"version":version.unwrap()}}));
    };

    let menu = pushed(&press(&mut server, 3, &status, "c", 0));
    opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "F", 0));
    opened(&mut server, &selector);
    let mut target_line = None;
    for line in 0..100 {
        server.send(
            json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{
            "viewId":selector,"command":"common.copy-value","version":1,
            "cursor":{"line":line,"column":0}}}),
        );
        let copied = server.response_with_id(5);
        if copied["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "clipboard" && effect["text"] == target)
        {
            target_line = Some(line);
            break;
        }
    }
    let target_line = target_line.expect("structured full-OID target");
    server.send(
        json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{
        "viewId":selector,"command":"common.confirm","version":1,
        "cursor":{"line":target_line,"column":0}}}),
    );
    let selected = server.response_with_id(6);
    assert!(selected["error"].is_null(), "{selected}");
    let mut caller_rendered = false;
    while !caller_rendered {
        let message = server.recv();
        if message["method"] == "view/render" && message["params"]["viewId"] == status {
            server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{
                "viewId":status,"version":message["params"]["version"]}}));
            caller_rendered = true;
        }
    }
    let history = tmp.git(&["log", "--format=%s", "--reverse"]);
    assert_eq!(
        history,
        "root subject\nbase subject\nselected target subject\nnewer head subject\n"
    );
    assert_eq!(tmp.git(&["rev-parse", "HEAD~2"]).trim(), base);
    assert_eq!(tmp.git(&["rev-parse", "HEAD~3"]).trim(), root);
    assert_eq!(tmp.git(&["show", "HEAD:target.txt"]), "base\n");
    assert_eq!(tmp.git(&["show", "HEAD:newer.txt"]), "head\n");
    assert_eq!(tmp.git(&["show", "HEAD:fix.txt"]), "fixed\n");
    assert_eq!(
        tmp.git(&["write-tree"]),
        tmp.git(&["rev-parse", "HEAD^{tree}"])
    );
    assert!(!tmp.path().join(".git/rebase-merge").exists());
    assert!(!tmp.path().join(".git/rebase-apply").exists());
    assert_ne!(tmp.git(&["rev-parse", "HEAD"]), prior_head);
    server.send(
        json!({"jsonrpc":"2.0","id":7,"method":"view/invoke","params":{
        "viewId":status,"command":"common.refresh"}}),
    );
    assert!(server.response_with_id(7)["error"].is_null());
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn dispatched_instant_fixup_conflict_is_left_for_explicit_recovery() {
    let (tmp, _) = RepoFixture::new()
        .commit("root", [("file", "root\n")])
        .commit("selected target", [("file", "target\n")])
        .commit("newer head", [("file", "head\n")])
        .staged_file("file", "resolved\n")
        .build();
    let prior_head = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let marker_parent = prior_head.clone();
    let target = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(10);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":1,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| {
        response["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|effect| effect["type"] == "pushView")
            .expect("pushView")["descriptor"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false;
        let mut keymap = false;
        let mut version = None;
        while !render || !keymap {
            let message = server.recv();
            if message["params"]["viewId"] == view {
                if message["method"] == "view/render" {
                    render = true;
                    version = message["params"]["version"].as_u64();
                }
                keymap |= message["method"] == "view/keymap";
            }
        }
        let version = version.unwrap();
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version}}));
        version
    };
    let menu = pushed(&press(&mut server, 3, &status, "c", 0));
    opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "F", 0));
    let selector_version = opened(&mut server, &selector);
    let mut target_line = None;
    for line in 0..100 {
        server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{"viewId":selector,"command":"common.copy-value","version":1,"cursor":{"line":line,"column":0}}}));
        if server.response_with_id(5)["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "clipboard" && e["text"] == target)
        {
            target_line = Some(line);
            break;
        }
    }
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{"viewId":selector,"command":"common.confirm","version":selector_version,"cursor":{"line":target_line.expect("target oid"),"column":0}}}));
    let selected = server.response_with_id(6);
    assert!(selected["error"].is_null(), "{selected}");
    assert_eq!(
        selected["result"]["effects"],
        json!([
            {"type":"popView","viewId":selector},
            {"type":"focusView","viewId":status}
        ]),
        "{selected}"
    );
    let mut pending = std::mem::take(&mut server.notifications).into_iter();
    let mut caller = None;
    let mut stopped = None;
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut actual = Vec::new();
    let mut render_versions = Vec::new();
    let readiness_diagnostics = |actual: &[Value], render_versions: &[u64]| {
        format!(
        "confirm={selected}; selected_oid={target}; selector_ack_version={selector_version}; actual={actual:?}; render_versions={render_versions:?}; HEAD={}; status={}; unmerged={}; rebase_merge={}; rebase_apply={}",
        tmp.git(&["rev-parse", "HEAD"]).trim(),
        tmp.git(&["status", "--porcelain"]),
        tmp.git(&["diff", "--name-only", "--diff-filter=U"]),
        tmp.path().join(".git/rebase-merge").exists(),
        tmp.path().join(".git/rebase-apply").exists(),
    )
    };
    while caller.is_none() || stopped.is_none() {
        assert!(
            Instant::now() < deadline,
            "{}",
            readiness_diagnostics(&actual, &render_versions)
        );
        let message = if let Some(message) = pending.next() {
            message
        } else {
            server.recv_timeout = deadline.saturating_duration_since(Instant::now());
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| server.recv()))
                .unwrap_or_else(|_| panic!("{}", readiness_diagnostics(&actual, &render_versions)))
        };
        actual.push(message.clone());
        if message["method"] == "view/render" {
            if let Some(version) = message["params"]["version"].as_u64() {
                render_versions.push(version);
            }
        }
        if message["method"] == "view/render" && message["params"]["viewId"] == status {
            caller = Some(message.clone());
        }
        if message["method"] == "view/effects"
            && message["params"]["effects"]
                .as_array()
                .is_some_and(|effects| {
                    effects.iter().any(|effect| {
                        effect["type"] == "message"
                            && effect["text"].as_str().is_some_and(|text| {
                                text.starts_with(
                                    "Instant autosquash stopped during autosquash; marker ",
                                )
                            })
                    })
                })
        {
            stopped = Some(message);
        }
    }
    let caller = caller.unwrap();
    let stopped = stopped.unwrap();
    assert!(
        stopped["params"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "message" && effect["level"] == "error"),
        "{stopped}"
    );
    let marker = tmp.git(&["rev-parse", "refs/heads/main"]).trim().to_owned();
    let stopped_text = stopped["params"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == "message" && effect["level"] == "error")
        .unwrap()["text"]
        .as_str()
        .unwrap();
    assert!(
        stopped_text.starts_with(&format!(
            "Instant autosquash stopped during autosquash; marker {marker} retained: "
        )),
        "{stopped_text}"
    );
    let marker_tree = tmp
        .git(&["rev-parse", &format!("{marker}^{{tree}}")])
        .trim()
        .to_owned();
    assert_eq!(
        tmp.git(&["rev-parse", &format!("{marker}^")]).trim(),
        marker_parent
    );
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]).trim(), target);
    assert_eq!(tmp.git(&["status", "--porcelain"]), "UU file\n");
    assert!(std::fs::read_to_string(tmp.path().join("file"))
        .unwrap()
        .contains("<<<<<<<"));
    assert!(tmp
        .git(&["rebase", "--show-current-patch"])
        .contains(&format!("fixup! {target}")));
    assert_ne!(marker, prior_head);
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":status,"version":caller["params"]["version"]}}));
    server.send(json!({"jsonrpc":"2.0","id":7,"method":"view/invoke","params":{"viewId":status,"command":"rebase.abort","version":caller["params"]["version"]}}));
    let aborted = server.response_with_id(7);
    assert!(aborted["error"].is_null(), "{aborted}");
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]).trim(), marker);
    assert_eq!(tmp.git(&["rev-parse", "refs/heads/main"]).trim(), marker);
    assert_eq!(tmp.git(&["rev-parse", "HEAD^{tree}"]).trim(), marker_tree);
    assert_eq!(tmp.git(&["status", "--porcelain"]), "");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("file")).unwrap(),
        "resolved\n"
    );
    assert!(!tmp.path().join(".git/rebase-merge").exists());
    server.send(json!({"jsonrpc":"2.0","id":8,"method":"view/invoke","params":{"viewId":status,"command":"common.refresh"}}));
    assert!(server.response_with_id(8)["error"].is_null());
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn dispatched_instant_fixup_conflict_can_be_resolved_and_continued() {
    let (tmp, _) = RepoFixture::new()
        .commit("root", [("file", "root\n")])
        .commit("selected target", [("file", "target\n")])
        .commit("newer head", [("file", "head\n")])
        .staged_file("file", "resolved\n")
        .build();
    let target = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(10);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":1,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| {
        response["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|effect| effect["type"] == "pushView")
            .unwrap()["descriptor"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false;
        let mut keymap = false;
        let mut version = None;
        while !render || !keymap {
            let message = server.recv();
            if message["params"]["viewId"] == view {
                if message["method"] == "view/render" {
                    render = true;
                    version = message["params"]["version"].as_u64();
                }
                keymap |= message["method"] == "view/keymap";
            }
        }
        let version = version.unwrap();
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version}}));
        version
    };
    let menu = pushed(&press(&mut server, 3, &status, "c", 0));
    opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "F", 0));
    let selector_version = opened(&mut server, &selector);
    let mut target_line = None;
    for line in 0..100 {
        server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{"viewId":selector,"command":"common.copy-value","version":1,"cursor":{"line":line,"column":0}}}));
        if server.response_with_id(5)["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "clipboard" && e["text"] == target)
        {
            target_line = Some(line);
            break;
        }
    }
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{"viewId":selector,"command":"common.confirm","version":selector_version,"cursor":{"line":target_line.expect("target oid"),"column":0}}}));
    let selected = server.response_with_id(6);
    assert!(selected["error"].is_null(), "{selected}");
    let mut pending = std::mem::take(&mut server.notifications).into_iter();
    let mut caller = None;
    let mut stopped = None;
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut actual = Vec::new();
    let mut render_versions = Vec::new();
    let readiness_diagnostics = |actual: &[Value], render_versions: &[u64]| {
        format!(
        "confirm={selected}; selected_oid={target}; actual={actual:?}; render_versions={render_versions:?}; HEAD={}; status={}; unmerged={}; rebase_merge={}; rebase_apply={}",
        tmp.git(&["rev-parse", "HEAD"]).trim(),
        tmp.git(&["status", "--porcelain"]),
        tmp.git(&["diff", "--name-only", "--diff-filter=U"]),
        tmp.path().join(".git/rebase-merge").exists(),
        tmp.path().join(".git/rebase-apply").exists(),
    )
    };
    while caller.is_none() || stopped.is_none() {
        assert!(
            Instant::now() < deadline,
            "{}",
            readiness_diagnostics(&actual, &render_versions)
        );
        let message = if let Some(message) = pending.next() {
            message
        } else {
            server.recv_timeout = deadline.saturating_duration_since(Instant::now());
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| server.recv()))
                .unwrap_or_else(|_| panic!("{}", readiness_diagnostics(&actual, &render_versions)))
        };
        actual.push(message.clone());
        if message["method"] == "view/render" {
            if let Some(version) = message["params"]["version"].as_u64() {
                render_versions.push(version);
            }
        }
        if message["method"] == "view/render" && message["params"]["viewId"] == status {
            caller = Some(message.clone());
        }
        if message["method"] == "view/effects"
            && message["params"]["effects"]
                .as_array()
                .is_some_and(|effects| {
                    effects.iter().any(|effect| {
                        effect["type"] == "message"
                            && effect["text"].as_str().is_some_and(|text| {
                                text.starts_with(
                                    "Instant autosquash stopped during autosquash; marker ",
                                )
                            })
                    })
                })
        {
            stopped = Some(message);
        }
    }
    let caller = caller.unwrap();
    let stopped = stopped.unwrap();
    assert!(
        stopped["params"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "message" && effect["level"] == "error"),
        "{stopped}"
    );
    let marker = tmp.git(&["rev-parse", "refs/heads/main"]).trim().to_owned();
    let stopped_text = stopped["params"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == "message" && effect["level"] == "error")
        .unwrap()["text"]
        .as_str()
        .unwrap();
    assert!(
        stopped_text.starts_with(&format!(
            "Instant autosquash stopped during autosquash; marker {marker} retained: "
        )),
        "{stopped_text}"
    );
    assert_eq!(tmp.git(&["status", "--porcelain"]), "UU file\n");
    assert!(std::fs::read_to_string(tmp.path().join("file"))
        .unwrap()
        .contains("<<<<<<<"));
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":status,"version":caller["params"]["version"]}}));
    std::fs::write(tmp.path().join("file"), "resolved\n").unwrap();
    tmp.git(&["add", "file"]);
    server.send(json!({"jsonrpc":"2.0","id":20,"method":"view/invoke","params":{"viewId":status,"command":"common.refresh"}}));
    assert!(server.response_with_id(20)["error"].is_null());
    let caller = loop {
        let message = server.recv();
        if message["method"] == "view/render" && message["params"]["viewId"] == status {
            break message;
        }
    };
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":status,"version":caller["params"]["version"]}}));
    server.send(json!({"jsonrpc":"2.0","id":7,"method":"view/invoke","params":{"viewId":status,"command":"rebase.continue","version":caller["params"]["version"]}}));
    let continued = server.response_with_id(7);
    let effects = continued["result"]["effects"]
        .as_array()
        .expect("continue effects");
    assert_eq!(effects.len(), 1, "{continued}");
    assert_eq!(effects[0]["type"], "message");
    assert_eq!(effects[0]["level"], "warn");
    let message = effects[0]["text"].as_str().unwrap();
    assert!(
        message
            .starts_with("git rebase --continue failed: Rebasing (3/3)\rerror: could not apply "),
        "{message}"
    );
    assert!(
        message.contains("... newer head\nhint: Resolve all conflicts manually"),
        "{message}"
    );
    assert!(message.ends_with("# newer head"), "{message}");
    assert_eq!(tmp.git(&["status", "--porcelain"]), "UU file\n");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("file")).unwrap(),
        format!(
            "<<<<<<< HEAD\nresolved\n=======\nhead\n>>>>>>> {} (newer head)\n",
            &tmp.git(&["rev-parse", "REBASE_HEAD"])[..7]
        )
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".git/rebase-merge/git-rebase-todo")).unwrap(),
        ""
    );
    let done = std::fs::read_to_string(tmp.path().join(".git/rebase-merge/done")).unwrap();
    assert!(
        done.lines()
            .map(|line| line.split_once(' ').unwrap().1.split_once(' ').unwrap().1)
            .eq([
                "# selected target",
                &format!("# fixup! {target}"),
                "# newer head"
            ]),
        "{done}"
    );
    std::fs::write(tmp.path().join("file"), "resolved\n").unwrap();
    tmp.git(&["add", "file"]);
    server.send(json!({"jsonrpc":"2.0","id":8,"method":"view/invoke","params":{"viewId":status,"command":"rebase.continue","version":caller["params"]["version"]}}));
    let finished = server.response_with_id(8);
    assert!(finished["error"].is_null(), "{finished}");
    assert!(
        finished["result"]["effects"].as_array().unwrap().is_empty(),
        "{finished}"
    );
    assert!(!tmp.path().join(".git/rebase-merge").exists());
    assert!(!tmp.path().join(".git/rebase-apply").exists());
    assert_eq!(
        tmp.git(&["log", "-1", "--format=%B"]),
        "selected target\n\n"
    );
    assert_eq!(
        tmp.git(&["log", "--format=%s", "--reverse"]),
        "root\nselected target\n"
    );
    assert_eq!(tmp.git(&["show", "HEAD:file"]), "resolved\n");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("file")).unwrap(),
        "resolved\n"
    );
    assert_eq!(tmp.git(&["status", "--porcelain"]), "");
    assert_eq!(
        tmp.git(&["write-tree"]).trim(),
        tmp.git(&["rev-parse", "HEAD^{tree}"]).trim()
    );
    server.send(json!({"jsonrpc":"2.0","id":9,"method":"view/invoke","params":{"viewId":status,"command":"common.refresh"}}));
    assert!(server.response_with_id(9)["error"].is_null());
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn instant_squash_selection_edits_message_through_registered_server_callback() {
    let (tmp, _) = RepoFixture::new()
        .commit("root subject", [("root.txt", "root\n")])
        .commit("base subject", [("base.txt", "base\n")])
        .commit("selected target subject", [("target.txt", "base\n")])
        .commit("newer head subject", [("newer.txt", "head\n")])
        .staged_file("fix.txt", "fixed\n")
        .build();
    let target = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let mut server = TestServer::spawn(&[]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(
            json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{
            "viewId":view,"version":1,"key":key,"editorMode":"normal",
            "cursor":{"line":line,"column":0}}}),
        );
        server.response_with_id(id)
    };
    let pushed = |response: &Value| {
        response["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|effect| effect["type"] == "pushView")
            .expect("pushView")["descriptor"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false;
        let mut keymap = false;
        let mut version = None;
        while !render || !keymap {
            let message = server.recv();
            if message["params"]["viewId"] == view {
                if message["method"] == "view/render" {
                    render = true;
                    version = message["params"]["version"].as_u64();
                }
                keymap |= message["method"] == "view/keymap";
            }
        }
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{
            "viewId":view,"version":version.unwrap()}}));
    };
    let menu = pushed(&press(&mut server, 3, &status, "c", 0));
    opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "S", 0));
    opened(&mut server, &selector);
    let mut target_line = None;
    for line in 0..100 {
        server.send(
            json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{
            "viewId":selector,"command":"common.copy-value","version":1,
            "cursor":{"line":line,"column":0}}}),
        );
        if server.response_with_id(5)["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "clipboard" && effect["text"] == target)
        {
            target_line = Some(line);
            break;
        }
    }
    let target_line = target_line.expect("structured full-OID squash target");
    server.send(
        json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{
        "viewId":selector,"command":"common.confirm","version":1,
        "cursor":{"line":target_line,"column":0}}}),
    );
    assert!(server.response_with_id(6)["error"].is_null());
    let edit = loop {
        let message = server.recv();
        if message["method"] == "view/effects" {
            if let Some(edit) = message["params"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|effect| effect["type"] == "editDocument")
            {
                break edit.clone();
            }
        }
    };
    let paused_head = tmp.git(&["rev-parse", "HEAD"]);
    let paused_branch = tmp.git(&["rev-parse", "refs/heads/main"]);
    let paused_index = std::fs::read(tmp.path().join(".git/index")).unwrap();
    server.send(
        json!({"jsonrpc":"2.0","id":12,"method":"view/invoke","params":{
        "viewId":status,"command":"common.refresh"}}),
    );
    assert!(server.response_with_id(12)["error"].is_null());
    let (other_tmp, _) = init_repo();
    std::fs::write(other_tmp.path().join("f.txt"), "other\n").unwrap();
    let other = open_view(&mut server, 13, &repo_param(other_tmp.path()));
    server.send(
        json!({"jsonrpc":"2.0","id":14,"method":"view/invoke","params":{
        "viewId":other,"command":"status.stage-all"}}),
    );
    let other_staged = server.response_with_id(14);
    assert!(other_staged["error"].is_null(), "{other_staged}");
    assert_eq!(other_tmp.git(&["show", ":f.txt"]), "other\n");
    server.send(
        json!({"jsonrpc":"2.0","id":15,"method":"view/resync","params":{
        "viewId":status}}),
    );
    let status_version = server.response_with_id(15)["result"]["version"]
        .as_u64()
        .unwrap();
    server.send(
        json!({"jsonrpc":"2.0","id":16,"method":"view/key","params":{
        "viewId":status,"version":status_version,"key":"F5","editorMode":"normal",
        "cursor":{"line":0,"column":0}}}),
    );
    let read_key = server.response_with_id(16);
    assert!(
        read_key["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|effect| effect["text"] != "Wait for the current Git operation to finish"),
        "{read_key}"
    );
    server.send(
        json!({"jsonrpc":"2.0","id":17,"method":"view/key","params":{
        "viewId":status,"version":status_version,"key":"S","editorMode":"normal",
        "cursor":{"line":0,"column":0}}}),
    );
    let key_refused = server.response_with_id(17);
    assert!(
        key_refused["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["text"] == "Wait for the current Git operation to finish"),
        "{key_refused}"
    );
    server.send(
        json!({"jsonrpc":"2.0","id":9,"method":"view/invoke","params":{
        "viewId":status,"command":"rebase.abort"}}),
    );
    let refused = server.response_with_id(9);
    assert!(
        refused["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "message"
                && effect["text"] == "Wait for the current Git operation to finish"),
        "{refused}"
    );
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), paused_head);
    assert_eq!(tmp.git(&["rev-parse", "refs/heads/main"]), paused_branch);
    assert_eq!(
        std::fs::read(tmp.path().join(".git/index")).unwrap(),
        paused_index
    );
    let edited = "selected target subject\n\nnew squash paragraph\n";
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":edit["uri"],"languageId":"gitcommit","version":1,"text":edit["text"]}}}));
    server.send(
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":edit["uri"],"version":2},"contentChanges":[{"text":edited}]}}),
    );
    server.send(
        json!({"jsonrpc":"2.0","id":7,"method":"workspace/executeCommand","params":{
        "command":"magit.command","arguments":[{"command":"common.confirm","runId":edit["runId"],
            "uri":edit["uri"],"version":2,"source":"synchronized"}]}}),
    );
    let confirmed = server.response_with_id(7);
    assert!(confirmed["error"].is_null(), "{confirmed}");
    let mut caller_rendered = false;
    let mut completed = false;
    while !caller_rendered || !completed {
        let message = server.recv();
        if message["method"] == "view/render" && message["params"]["viewId"] == status {
            server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{
                "viewId":status,"version":message["params"]["version"]}}));
            caller_rendered = true;
        }
        if message["method"] == "view/effects" {
            completed |= message["params"]["effects"]
                .as_array()
                .is_some_and(|effects| {
                    effects.iter().any(|effect| {
                        effect["text"]
                            .as_str()
                            .is_some_and(|text| text.starts_with("Instant autosquash completed"))
                    })
                });
        }
    }
    assert_eq!(
        tmp.git(&["log", "--format=%s", "--reverse"]),
        "root subject\nbase subject\nselected target subject\nnewer head subject\n"
    );
    assert_eq!(
        tmp.git(&["show", "-s", "--format=%B", "HEAD~1"]),
        "selected target subject\n\nnew squash paragraph\n\n"
    );
    assert_eq!(tmp.git(&["show", "HEAD:target.txt"]), "base\n");
    assert_eq!(tmp.git(&["show", "HEAD:fix.txt"]), "fixed\n");
    assert_eq!(
        tmp.git(&["write-tree"]),
        tmp.git(&["rev-parse", "HEAD^{tree}"])
    );
    assert!(!tmp.path().join(".git/rebase-merge").exists());
    assert!(!tmp.path().join(".git/rebase-apply").exists());
    std::fs::write(tmp.path().join("fix.txt"), "after\n").unwrap();
    server.send(
        json!({"jsonrpc":"2.0","id":10,"method":"view/invoke","params":{
        "viewId":status,"command":"common.refresh"}}),
    );
    assert!(server.response_with_id(10)["error"].is_null());
    server.send(
        json!({"jsonrpc":"2.0","id":11,"method":"view/invoke","params":{
        "viewId":status,"command":"status.stage-all"}}),
    );
    let staged = server.response_with_id(11);
    assert!(staged["error"].is_null(), "{staged}");
    assert!(
        staged["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|effect| effect["text"] != "Wait for the current Git operation to finish"),
        "{staged}"
    );
    assert_eq!(tmp.git(&["show", ":fix.txt"]), "after\n");
    server.send(
        json!({"jsonrpc":"2.0","id":8,"method":"view/invoke","params":{
        "viewId":status,"command":"common.refresh"}}),
    );
    assert!(server.response_with_id(8)["error"].is_null());
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn instant_squash_registered_editor_abort_retains_marker_and_releases_worker() {
    let (tmp, _) = RepoFixture::new()
        .commit("root subject", [("root.txt", "root\n")])
        .commit("base subject", [("base.txt", "base\n")])
        .commit("selected target subject", [("target.txt", "base\n")])
        .commit("newer head subject", [("newer.txt", "head\n")])
        .staged_file("fix.txt", "fixed\n")
        .build();
    let target = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let original_head = tmp.git(&["rev-parse", "HEAD"]);
    let mut server = TestServer::spawn(&[]);
    server.auto_respond_to_progress_create();
    assert!(server.initialize_rich(json!({"window":{"workDoneProgress":true}}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":1,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| {
        response["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|effect| effect["type"] == "pushView")
            .unwrap()["descriptor"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let opened = |server: &mut TestServer, view: &str| {
        let mut rendered = false;
        let mut keymap = false;
        let mut version = None;
        while !rendered || !keymap {
            let message = server.recv();
            if message["params"]["viewId"] == view {
                if message["method"] == "view/render" {
                    rendered = true;
                    version = message["params"]["version"].as_u64();
                }
                keymap |= message["method"] == "view/keymap";
            }
        }
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version.unwrap()}}));
    };
    let menu = pushed(&press(&mut server, 3, &status, "c", 0));
    opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "S", 0));
    opened(&mut server, &selector);
    let mut line = None;
    for candidate in 0..100 {
        server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{"viewId":selector,"command":"common.copy-value","version":1,"cursor":{"line":candidate,"column":0}}}));
        if server.response_with_id(5)["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "clipboard" && e["text"] == target)
        {
            line = Some(candidate);
            break;
        }
    }
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{"viewId":selector,"command":"common.confirm","version":1,"cursor":{"line":line.expect("selected target"),"column":0}}}));
    assert!(server.response_with_id(6)["error"].is_null());
    let edit = loop {
        let message = server.recv();
        if message["method"] == "view/effects" {
            if let Some(edit) = message["params"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["type"] == "editDocument")
            {
                break edit.clone();
            }
        }
    };
    let marker = tmp.git(&["rev-parse", "refs/heads/main"]);
    let stopped_head = tmp.git(&["rev-parse", "HEAD"]);
    let stopped_index = std::fs::read(tmp.path().join(".git/index")).unwrap();
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":edit["uri"],"languageId":"gitcommit","version":1,"text":edit["text"]}}}));
    server.send(json!({"jsonrpc":"2.0","id":7,"method":"view/invoke","params":{"viewId":status,"command":"common.cancel","args":{"runId":edit["runId"]}}}));
    let mut saw_end = false;
    let mut cancel_response = None;
    let mut stopped_effect = None;
    while cancel_response.is_none() || stopped_effect.is_none() {
        let message = server.recv();
        if message["method"] == "$/progress" && message["params"]["value"]["kind"] == "end" {
            saw_end = true;
        }
        if message["id"] == 7 {
            cancel_response = Some(message.clone());
        }
        if message["method"] == "view/effects" {
            stopped_effect = Some(message);
        }
    }
    assert!(cancel_response.unwrap()["result"]["effects"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(stopped_effect.unwrap()["params"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["type"] == "message"));
    while !saw_end {
        let message = server.recv();
        if message["method"] == "$/progress" && message["params"]["value"]["kind"] == "end" {
            saw_end = true;
        }
    }
    assert!(saw_end, "progress ended after registered editor abort");
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), stopped_head);
    assert_eq!(
        tmp.git(&["rev-parse", "refs/heads/main"]).trim(),
        marker.trim()
    );
    assert_eq!(
        tmp.git(&["rev-parse", &format!("{}^", marker.trim())])
            .trim(),
        original_head.trim()
    );
    assert!(tmp.path().join(".git/rebase-merge").exists());
    assert_eq!(
        std::fs::read(tmp.path().join(".git/index")).unwrap(),
        stopped_index
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("fix.txt")).unwrap(),
        "fixed\n"
    );
    server.send(json!({"jsonrpc":"2.0","id":9,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[{"command":"common.confirm","runId":edit["runId"],"uri":edit["uri"],"version":1,"source":"synchronized"}]}}));
    assert!(
        server.response_with_id(9)["error"].is_object(),
        "aborted editor run cannot complete"
    );
    server.send(json!({"jsonrpc":"2.0","id":10,"method":"view/invoke","params":{"viewId":status,"command":"rebase.abort"}}));
    assert!(
        server.response_with_id(10)["error"].is_null(),
        "normal rebase abort works after editor cancellation"
    );
    assert!(!tmp.path().join(".git/rebase-merge").exists());
    server.send(json!({"jsonrpc":"2.0","id":8,"method":"view/invoke","params":{"viewId":status,"command":"common.refresh"}}));
    assert!(
        server.response_with_id(8)["error"].is_null(),
        "caller refresh after cancellation and abort"
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn prepared_rebase_message_actions_refuse_through_real_server_submit() {
    for (key, label) in [("r", "reword"), ("s", "squash")] {
        let (tmp, _) = RepoFixture::new()
            .commit("base", [("base", "base\n")])
            .commit("one", [("one", "1\n")])
            .commit("two", [("two", "2\n")])
            .build();
        let base = tmp.git(&["rev-parse", "HEAD~2"]).trim().to_owned();
        let mut server = TestServer::spawn(&[]);
        assert!(server.initialize_rich(json!({}))["error"].is_null());
        let status = open_view(&mut server, 2, &repo_param(tmp.path()));
        let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
            server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":1,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
            server.response_with_id(id)
        };
        let pushed = |response: &Value| {
            response["result"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|effect| effect["type"] == "pushView")
                .unwrap()["descriptor"]["id"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        let opened = |server: &mut TestServer, view: &str| {
            let mut render = false;
            let mut keymap = false;
            let mut version = None;
            while !render || !keymap {
                let message = server.recv();
                if message["params"]["viewId"] == view {
                    if message["method"] == "view/render" {
                        render = true;
                        version = message["params"]["version"].as_u64();
                    }
                    keymap |= message["method"] == "view/keymap";
                }
            }
            server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version.unwrap()}}));
        };
        let menu = pushed(&press(&mut server, 3, &status, "r", 0));
        opened(&mut server, &menu);
        let selector = pushed(&press(&mut server, 4, &menu, "i", 0));
        opened(&mut server, &selector);
        let mut base_line = None;
        for line in 0..100 {
            server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{"viewId":selector,"command":"common.copy-value","version":1,"cursor":{"line":line,"column":0}}}));
            if server.response_with_id(5)["result"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["type"] == "clipboard" && e["text"] == base)
            {
                base_line = Some(line);
                break;
            }
        }
        server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{"viewId":selector,"command":"common.confirm","version":1,"cursor":{"line":base_line.unwrap(),"column":0}}}));
        let todo = pushed(&server.response_with_id(6));
        opened(&mut server, &todo);
        let action_line = u64::from(label == "squash");
        let changed = press(&mut server, 7, &todo, key, action_line);
        assert!(
            changed["result"]["effects"].as_array().unwrap().is_empty(),
            "{changed}"
        );
        server.recv();
        let before = (
            tmp.git(&["rev-parse", "HEAD"]),
            tmp.git(&["for-each-ref", "--format=%(refname) %(objectname)"]),
            std::fs::read(tmp.path().join(".git/index")).unwrap(),
            std::fs::read(tmp.path().join("one")).ok(),
            std::fs::read(tmp.path().join("two")).ok(),
            tmp.path().join(".git/rebase-merge").exists(),
            tmp.path().join(".git/rebase-apply").exists(),
        );
        let submitted = press(&mut server, 8, &todo, "ctrl+s", 0);
        assert!(
            submitted["result"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["type"] == "message"
                    && e["text"]
                        .as_str()
                        .unwrap()
                        .contains("prepared rebase message editing is not supported yet")),
            "{submitted}"
        );
        assert_eq!(
            (
                tmp.git(&["rev-parse", "HEAD"]),
                tmp.git(&["for-each-ref", "--format=%(refname) %(objectname)"]),
                std::fs::read(tmp.path().join(".git/index")).unwrap(),
                std::fs::read(tmp.path().join("one")).ok(),
                std::fs::read(tmp.path().join("two")).ok(),
                tmp.path().join(".git/rebase-merge").exists(),
                tmp.path().join(".git/rebase-apply").exists()
            ),
            before
        );
        let retained = server.recv();
        assert_eq!(retained["method"], "view/render");
        assert_eq!(retained["params"]["viewId"], todo);
        assert!(
            retained["params"]["text"][action_line as usize]
                .as_str()
                .unwrap()
                .contains(label),
            "{retained}"
        );
        assert!(
            retained["params"]["text"][1 - action_line as usize]
                .as_str()
                .unwrap()
                .contains("pick"),
            "{retained}"
        );
        shutdown_and_exit(&mut server);
        assert!(server.finish(Duration::from_secs(10)).success());
    }
}

#[test]
fn prepared_rebase_edit_pause_is_abortable_from_usable_caller() {
    let (tmp, _) = RepoFixture::new()
        .commit("base", [("base", "base\n")])
        .commit("one", [("one", "1\n")])
        .commit("two", [("two", "2\n")])
        .build();
    let base = tmp.git(&["rev-parse", "HEAD~2"]).trim().to_owned();
    let original = (
        tmp.git(&["rev-parse", "HEAD"]),
        tmp.git(&["for-each-ref", "--format=%(refname) %(objectname)"]),
        tmp.git(&["write-tree"]),
        ["base", "one", "two"].map(|name| std::fs::read(tmp.path().join(name)).ok()),
    );
    let mut server = TestServer::spawn(&[("GIT_EDITOR", "true")]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":1,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| {
        response["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["type"] == "pushView")
            .unwrap()["descriptor"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false;
        let mut keymap = false;
        let mut version = None;
        while !render || !keymap {
            let message = server.recv();
            if message["params"]["viewId"] == view {
                if message["method"] == "view/render" {
                    render = true;
                    version = message["params"]["version"].as_u64();
                }
                keymap |= message["method"] == "view/keymap";
            }
        }
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version.unwrap()}}));
    };
    let menu = pushed(&press(&mut server, 3, &status, "r", 0));
    opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "i", 0));
    opened(&mut server, &selector);
    let mut base_line = None;
    for line in 0..100 {
        server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{"viewId":selector,"command":"common.copy-value","version":1,"cursor":{"line":line,"column":0}}}));
        if server.response_with_id(5)["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "clipboard" && e["text"] == base)
        {
            base_line = Some(line);
            break;
        }
    }
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{"viewId":selector,"command":"common.confirm","version":1,"cursor":{"line":base_line.unwrap(),"column":0}}}));
    let todo = pushed(&server.response_with_id(6));
    opened(&mut server, &todo);
    let edited = press(&mut server, 7, &todo, "e", 0);
    assert!(edited["result"]["effects"].as_array().unwrap().is_empty());
    server.recv();
    let submitted = press(&mut server, 8, &todo, "ctrl+s", 0);
    assert!(
        submitted["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "popView" && e["viewId"] == todo),
        "{submitted}"
    );
    assert!(tmp.path().join(".git/rebase-merge").exists());
    let caller_render = loop {
        let message = server.recv();
        if message["method"] == "view/render" && message["params"]["viewId"] == status {
            break message;
        }
    };
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":status,"version":caller_render["params"]["version"]}}));
    server.send(json!({"jsonrpc":"2.0","id":9,"method":"view/invoke","params":{"viewId":status,"command":"rebase.abort","version":caller_render["params"]["version"]}}));
    let aborted = server.response_with_id(9);
    assert!(aborted["error"].is_null(), "{aborted}");
    assert_eq!(
        (
            tmp.git(&["rev-parse", "HEAD"]),
            tmp.git(&["for-each-ref", "--format=%(refname) %(objectname)"]),
            tmp.git(&["write-tree"]),
            ["base", "one", "two"].map(|name| std::fs::read(tmp.path().join(name)).ok())
        ),
        original
    );
    assert!(!tmp.path().join(".git/rebase-merge").exists());
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn prepared_rebase_conflict_can_be_resolved_and_continued_from_caller() {
    let (tmp, _) = RepoFixture::new()
        .commit("base", [("file", "base\n")])
        .commit("first", [("file", "first\n")])
        .commit("second", [("file", "second\n")])
        .build();
    let base = tmp.git(&["rev-parse", "HEAD~2"]).trim().to_owned();
    let mut server = TestServer::spawn(&[("GIT_EDITOR", "true")]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":1,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| {
        response["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|effect| effect["type"] == "pushView")
            .unwrap()["descriptor"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false;
        let mut keymap = false;
        let mut version = None;
        while !render || !keymap {
            let message = server.recv();
            if message["params"]["viewId"] == view {
                if message["method"] == "view/render" {
                    render = true;
                    version = message["params"]["version"].as_u64();
                }
                keymap |= message["method"] == "view/keymap";
            }
        }
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version.unwrap()}}));
    };
    let menu = pushed(&press(&mut server, 3, &status, "r", 0));
    opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "i", 0));
    opened(&mut server, &selector);
    let mut base_line = None;
    for line in 0..100 {
        server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{"viewId":selector,"command":"common.copy-value","version":1,"cursor":{"line":line,"column":0}}}));
        if server.response_with_id(5)["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "clipboard" && e["text"] == base)
        {
            base_line = Some(line);
            break;
        }
    }
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{"viewId":selector,"command":"common.confirm","version":1,"cursor":{"line":base_line.unwrap(),"column":0}}}));
    let todo = pushed(&server.response_with_id(6));
    opened(&mut server, &todo);
    let moved = press(&mut server, 7, &todo, "J", 0);
    assert!(moved["error"].is_null(), "{moved}");
    server.recv();
    let submitted = press(&mut server, 8, &todo, "ctrl+s", 0);
    assert!(
        submitted["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "popView" && e["viewId"] == todo),
        "{submitted}"
    );
    assert!(
        submitted["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "message"
                && !e["text"].as_str().unwrap_or_default().trim().is_empty()),
        "{submitted}"
    );
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]).trim(), base);
    assert_eq!(tmp.git(&["status", "--porcelain"]), "UU file\n");
    let conflict = std::fs::read_to_string(tmp.path().join("file")).unwrap();
    assert!(
        conflict.starts_with("<<<<<<< HEAD\nbase\n=======\nsecond\n>>>>>>> "),
        "{conflict:?}"
    );
    assert!(tmp.path().join(".git/rebase-merge").exists());
    let caller = loop {
        let message = server.recv();
        if message["method"] == "view/render" && message["params"]["viewId"] == status {
            break message;
        }
    };
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":status,"version":caller["params"]["version"]}}));
    server.send(json!({"jsonrpc":"2.0","id":10,"method":"view/invoke","params":{"viewId":status,"command":"common.refresh"}}));
    let refreshed = server.response_with_id(10);
    assert!(refreshed["error"].is_null(), "{refreshed}");
    let caller = loop {
        let message = server.recv();
        if message["method"] == "view/render" && message["params"]["viewId"] == status {
            break message;
        }
    };
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":status,"version":caller["params"]["version"]}}));
    for attempt in 0..3 {
        if !tmp.path().join(".git/rebase-merge").exists() {
            break;
        }
        std::fs::write(tmp.path().join("file"), "resolved\n").unwrap();
        tmp.git(&["add", "file"]);
        let id = 11 + attempt;
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/invoke","params":{"viewId":status,"command":"rebase.continue","version":caller["params"]["version"]}}));
        let continued = server.response_with_id(id);
        assert!(continued["error"].is_null(), "{continued}");
    }
    assert_eq!(
        tmp.git(&["log", "--format=%s", "--reverse"]),
        "base\nsecond\n"
    );
    assert_eq!(tmp.git(&["show", "HEAD:file"]), "resolved\n");
    assert_eq!(tmp.git(&["status", "--porcelain"]), "");
    assert!(!tmp.path().join(".git/rebase-merge").exists());
    assert_eq!(
        tmp.git(&["write-tree"]).trim(),
        tmp.git(&["rev-parse", "HEAD^{tree}"]).trim()
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn stale_prepared_rebase_submit_retains_editor_and_edits() {
    let (tmp, _) = RepoFixture::new()
        .commit("base", [("base", "base\n")])
        .commit("one", [("one", "1\n")])
        .commit("two", [("two", "2\n")])
        .build();
    let base = tmp.git(&["rev-parse", "HEAD~2"]).trim().to_owned();
    let mut server = TestServer::spawn(&[]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":1,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| {
        response["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|effect| effect["type"] == "pushView")
            .unwrap()["descriptor"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false;
        let mut keymap = false;
        let mut version = None;
        while !render || !keymap {
            let message = server.recv();
            if message["params"]["viewId"] == view {
                if message["method"] == "view/render" {
                    render = true;
                    version = message["params"]["version"].as_u64();
                }
                keymap |= message["method"] == "view/keymap";
            }
        }
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version.unwrap()}}));
    };
    let menu = pushed(&press(&mut server, 3, &status, "r", 0));
    opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "i", 0));
    opened(&mut server, &selector);
    let mut base_line = None;
    for line in 0..100 {
        server.send(json!({"jsonrpc":"2.0","id":50,"method":"view/invoke","params":{"viewId":selector,"command":"common.copy-value","version":1,"cursor":{"line":line,"column":0}}}));
        if server.response_with_id(50)["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "clipboard" && e["text"] == base)
        {
            base_line = Some(line);
            break;
        }
    }
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{"viewId":selector,"command":"common.confirm","version":1,"cursor":{"line":base_line.unwrap(),"column":0}}}));
    let todo = pushed(&server.response_with_id(6));
    opened(&mut server, &todo);
    press(&mut server, 7, &todo, "e", 0);
    server.recv();
    press(&mut server, 8, &todo, "J", 0);
    server.recv();
    press(&mut server, 9, &todo, "d", 2);
    server.recv();
    tmp.git(&["commit", "--allow-empty", "-m", "external"]);
    let external = tmp.git(&["rev-parse", "HEAD"]);
    let pre_dispatch = (
        tmp.git(&["rev-parse", "HEAD"]),
        tmp.git(&["status", "--porcelain=v2"]),
        std::fs::read(tmp.path().join(".git/index")).unwrap(),
        tmp.git(&["for-each-ref", "--format=%(refname) %(objectname)"]),
        ["base", "one", "two"].map(|name| std::fs::read(tmp.path().join(name)).ok()),
        tmp.path().join(".git/rebase-merge").exists(),
        tmp.path().join(".git/rebase-apply").exists(),
    );
    let stale = press(&mut server, 10, &todo, "ctrl+s", 0);
    assert!(stale["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "message" && e["text"].as_str().unwrap().contains("HEAD changed")), "{stale}");
    assert_eq!(
        (
            tmp.git(&["rev-parse", "HEAD"]),
            tmp.git(&["status", "--porcelain=v2"]),
            std::fs::read(tmp.path().join(".git/index")).unwrap(),
            tmp.git(&["for-each-ref", "--format=%(refname) %(objectname)"]),
            ["base", "one", "two"].map(|name| std::fs::read(tmp.path().join(name)).ok()),
            tmp.path().join(".git/rebase-merge").exists(),
            tmp.path().join(".git/rebase-apply").exists()
        ),
        pre_dispatch
    );
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), external);
    let retained = server.recv();
    assert_eq!(retained["method"], "view/render");
    assert_eq!(retained["params"]["viewId"], todo);
    assert_eq!(
        retained["params"]["text"].as_array().unwrap().len(),
        2,
        "{retained}"
    );
    assert!(
        retained["params"]["text"][1]
            .as_str()
            .unwrap()
            .contains("edit"),
        "{retained}"
    );
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":todo,"version":retained["params"]["version"]}}));
    for (line, revision) in [(0, "HEAD~1"), (1, "HEAD~2")] {
        let id = 20 + line as i64;
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/invoke","params":{"viewId":todo,"command":"common.copy-value","version":5,"cursor":{"line":line,"column":0}}}));
        let copied = server.response_with_id(id);
        let oid = tmp.git(&["rev-parse", revision]);
        assert!(
            copied["result"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|effect| effect["type"] == "clipboard" && effect["text"] == oid.trim()),
            "{copied}"
        );
    }
    let cancel = press(&mut server, 11, &todo, "q", 0);
    assert!(cancel["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["type"] == "popView"));
    assert!(cancel["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["type"] == "focusView" && e["viewId"] == status));
    server.send(json!({"jsonrpc":"2.0","id":12,"method":"view/invoke","params":{"viewId":status,"command":"common.refresh"}}));
    let refreshed = server.response_with_id(12);
    assert!(
        refreshed["result"]["effects"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{refreshed}"
    );
    let caller_render = server.recv();
    assert_eq!(caller_render["method"], "view/render");
    assert_eq!(caller_render["params"]["viewId"], status);
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

/// Open a status view and consume its initial render and keymap pushes.
fn open_view(server: &mut TestServer, id: i64, repo: &Value) -> String {
    server.send(open_view_message(id, repo));
    let opened = server.response_with_id(id);
    let view_id = opened["result"]["id"]
        .as_str()
        .expect("view id")
        .to_string();
    let render = server.recv();
    assert_eq!(render["method"], json!("view/render"), "{render}");
    let keymap = server.recv();
    assert_eq!(keymap["method"], json!("view/keymap"), "{keymap}");
    server.wait_for_view_render_after(
        &view_id,
        render["params"]["version"]
            .as_u64()
            .expect("initial render version"),
    );
    view_id
}

/// Open a Plain logical view without waiting for Rich-only render/keymap pushes.
fn open_plain_view(server: &mut TestServer, id: i64, repo: &Value) -> String {
    server.send(open_view_message(id, repo));
    let opened = server.response_with_id(id);
    let view_id = opened["result"]["id"]
        .as_str()
        .expect("view id")
        .to_string();
    server.wait_for_view_resync_after(&view_id, id * 100_000 + 1, 1);
    view_id
}

/// A `view/open` request for a status view of `repo`.
fn open_view_message(id: i64, repo: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "view/open",
        "params": {
            "kind": "status",
            "args": {"repo_path": repo},
            "viewport": {"cols": 80, "rows": 24},
        },
    })
}

/// The 0-based cursor line of the first hunk, resolved from the server's own
/// outline. Tests never recover structure by matching rendered line shapes.
fn first_hunk_line(server: &mut TestServer, uri: &str) -> u64 {
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 90,
        "method": "textDocument/documentSymbol",
        "params": {"textDocument": {"uri": uri}},
    }));
    let response = server.response_with_id(90);
    let symbols: Vec<lsp_types::DocumentSymbol> =
        serde_json::from_value(response["result"].clone())
            .unwrap_or_else(|error| panic!("documentSymbol returns symbols: {error}: {response}"));
    hunk_symbol_line(&symbols).unwrap_or_else(|| panic!("no hunk symbol: {response}"))
}

/// The selection line of the first hunk in an outline, at any depth.
fn hunk_symbol_line(symbols: &[lsp_types::DocumentSymbol]) -> Option<u64> {
    symbols.iter().find_map(|symbol| {
        if symbol.kind == lsp_types::SymbolKind::METHOD {
            return Some(u64::from(symbol.selection_range.start.line));
        }
        symbol.children.as_deref().and_then(hunk_symbol_line)
    })
}

fn shutdown_and_exit(server: &mut TestServer) {
    server.send(json!({"jsonrpc": "2.0", "id": 99, "method": "shutdown", "params": null}));
    let response = server.response_with_id(99);
    assert_eq!(response["result"], Value::Null);
    server.send(json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
}

/// Full rich declaration including every native pick shape.
fn rich_pick_capabilities() -> Value {
    json!({"experimental":{"magit":{
        "v":1,"keys":"dynamic","modifiers":"full",
        "layout":["main"],"effects":[],"prompts":[],
        "pick":{"multi":false,"freeText":true,"description":true,"detail":true,
                "group":true,"preselection":true,"fallbackView":true,"maxItems":100}
    }}})
}

/// Mixed rung: native picks and document effects declared, native prompts not.
fn mixed_rung_pick_capabilities() -> Value {
    json!({"experimental":{"magit":{
        "v":1,"keys":"dynamic","modifiers":"full",
        "layout":["main"],"effects":["editDocument"],"prompts":[],
        "pick":{"multi":false,"freeText":true,"description":true,"detail":true,
                "group":true,"preselection":true,"fallbackView":true,"maxItems":100}
    }}})
}

#[test]
fn a_declared_native_pick_answers_the_staged_menu_prompt() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut server = TestServer::spawn(&[]);
    server.initialize(rich_pick_capabilities());
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let status = server.response_with_id(2)["result"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke",
        "params":{"viewId":status,"command":"open.branch"}}));
    let opened = server.response_with_id(3);
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let menu_render = server.recv();
    assert_eq!(menu_render["method"], json!("view/render"), "{menu_render}");
    assert_eq!(server.recv()["method"], json!("view/keymap"));
    let version = menu_render["params"]["version"].as_u64().unwrap();
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
        "params":{"viewId":menu,"version":version}}));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"=","editorMode":"normal","cursor":{"line":0,"column":0}}}));
    let prefix = server.response_with_id(4);
    assert_eq!(prefix["result"]["effects"], json!([]), "{prefix}");
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"u","editorMode":"normal","cursor":{"line":0,"column":0}}}));

    let pick = loop {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/pick") {
            break message;
        }
    };
    let params = &pick["params"];
    assert!(
        params["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == json!("main")),
        "{pick}"
    );
    assert_eq!(params["allowFreeText"], json!(true), "{pick}");
    assert_eq!(params["multi"], json!(false), "{pick}");
    let pick_id = params["pickId"].as_str().unwrap().to_owned();

    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/pickRespond",
        "params":{"pickId":pick_id,"cancelled":false,"selected":["main"]}}));

    let mut saw_key_response = false;
    let mut saw_update = false;
    for _ in 0..30 {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["id"] == json!(5) {
            saw_key_response = true;
        }
        if message["method"] == json!("view/render")
            && message["params"]
                .to_string()
                .contains("--set-upstream-to=main")
        {
            saw_update = true;
        }
        if saw_key_response && saw_update {
            break;
        }
    }
    assert!(
        saw_key_response,
        "the held view/key response arrives: {:?}",
        server.notifications
    );
    assert!(
        saw_update,
        "the menu re-renders the new option: {:?}",
        server.notifications
    );
    assert!(
        server
            .notifications
            .iter()
            .all(|message| message["method"] != json!("view/prompt")),
        "a declared pick must not emit a document prompt: {:?}",
        server.notifications
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn cancelled_native_pick_on_the_mixed_rung_leaves_the_menu_and_repo_unchanged() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["ls-files", "--stage", "-z"]);
    let worktree = read(&tmp.path().join("f.txt"));
    let mut server = TestServer::spawn(&[]);
    // Mixed rung: native pick declared, native prompts (confirm) undeclared.
    server.initialize(rich_pick_capabilities());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &status, "open.branch");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let menu_render = server.recv();
    assert_eq!(menu_render["method"], json!("view/render"), "{menu_render}");
    assert_eq!(server.recv()["method"], json!("view/keymap"));
    let version = menu_render["params"]["version"].as_u64().unwrap();
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
        "params":{"viewId":menu,"version":version}}));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"=","editorMode":"normal","cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"u","editorMode":"normal","cursor":{"line":0,"column":0}}}));

    let pick = loop {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/pick") {
            break message;
        }
    };
    let params = &pick["params"];
    assert_eq!(params["multi"], json!(false), "{pick}");
    assert_eq!(params["allowFreeText"], json!(true), "{pick}");
    let pick_id = params["pickId"].as_str().unwrap().to_owned();

    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/pickRespond",
        "params":{"pickId":pick_id,"cancelled":true,"selected":[]}}));

    let mut key_response = None;
    let mut respond_response = None;
    for _ in 0..30 {
        let message = server.recv();
        server.notifications.push(message.clone());
        match message["id"].as_i64() {
            Some(5) => key_response = Some(message),
            Some(6) => respond_response = Some(message),
            _ => {}
        }
        if key_response.is_some() && respond_response.is_some() {
            break;
        }
    }
    let key_response = key_response.expect("the held view/key response returns");
    let respond_response = respond_response.expect("view/pickRespond responds");
    assert!(
        respond_response.get("error").is_none(),
        "{respond_response}"
    );
    for response in [&key_response, &respond_response] {
        assert!(
            !response["result"]["effects"]
                .as_array()
                .is_some_and(|effects| {
                    effects
                        .iter()
                        .any(|effect| effect["type"] == json!("pushView"))
                }),
            "cancelling a pick must not push a view: {response}"
        );
    }
    assert!(
        server
            .notifications
            .iter()
            .all(|message| message["method"] != json!("view/prompt")),
        "a declared pick must not emit a document prompt: {:?}",
        server.notifications
    );
    assert!(
        server
            .notifications
            .iter()
            .all(|message| !message.to_string().contains("--set-upstream-to=main")),
        "a cancelled option pick leaves the option unset: {:?}",
        server.notifications
    );

    // The menu is still the top view and holds no pending prompt: a command
    // on it runs instead of being refused.
    let refreshed = invoke_rich(&mut server, 7, &menu, "common.refresh");
    assert!(refreshed.get("error").is_none(), "{refreshed}");
    let effects = refreshed["result"]["effects"].as_array().unwrap();
    assert!(
        effects
            .iter()
            .all(|effect| effect["type"] != json!("pushView")),
        "{refreshed}"
    );
    assert!(
        effects.iter().all(|effect| !effect["text"]
            .as_str()
            .is_some_and(|text| text.contains("pending prompt"))),
        "{refreshed}"
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index);
    assert_eq!(read(&tmp.path().join("f.txt")), worktree);
    assert!(!tmp.path().join(".git/index.lock").exists());
}

#[test]
fn cancelled_document_confirm_after_a_native_pick_on_the_mixed_rung_leaves_the_repo_unchanged() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", "initial\n")])
        .modify("f.txt", "changed\n")
        .build();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["ls-files", "--stage", "-z"]);
    let worktree = read(&tmp.path().join("f.txt"));
    let mut server = TestServer::spawn(&[]);
    // Mixed rung: native pick and editDocument declared, native confirm undeclared.
    server.initialize(mixed_rung_pick_capabilities());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &status, "open.branch");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let menu_render = server.recv();
    assert_eq!(menu_render["method"], json!("view/render"), "{menu_render}");
    assert_eq!(server.recv()["method"], json!("view/keymap"));
    let version = menu_render["params"]["version"].as_u64().unwrap();
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
        "params":{"viewId":menu,"version":version}}));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"=","editorMode":"normal","cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"u","editorMode":"normal","cursor":{"line":0,"column":0}}}));

    let pick = loop {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/pick") {
            break message;
        }
    };
    let params = &pick["params"];
    assert!(
        params["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == json!("main")),
        "{pick}"
    );
    let pick_id = params["pickId"].as_str().unwrap().to_owned();

    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/pickRespond",
        "params":{"pickId":pick_id,"cancelled":false,"selected":["main"]}}));

    let mut key_response = None;
    let mut respond_response = None;
    for _ in 0..30 {
        let message = server.recv();
        server.notifications.push(message.clone());
        match message["id"].as_i64() {
            Some(5) => key_response = Some(message),
            Some(6) => respond_response = Some(message),
            _ => {}
        }
        if key_response.is_some() && respond_response.is_some() {
            break;
        }
    }
    let key_response = key_response.expect("the held view/key response returns");
    let respond_response = respond_response.expect("view/pickRespond responds");
    assert!(
        respond_response.get("error").is_none(),
        "{respond_response}"
    );
    assert!(
        server
            .notifications
            .iter()
            .all(|message| message["method"] != json!("view/prompt")),
        "a declared pick must not emit a document prompt: {:?}",
        server.notifications
    );

    // The unstaged worktree change makes commit.create open the confirm prompt
    // as a document because the client does not declare native confirm.
    let opened = invoke_rich(&mut server, 7, &status, "commit.create");
    let prompt_effect = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| {
            effect["type"] == json!("pushView") && effect["descriptor"]["kind"] == json!("prompt")
        })
        .expect("commit.create opens the document confirm");
    let prompt = prompt_effect["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut prompt_render = server
        .notifications
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("view/render")
                && message["params"]["viewId"] == json!(prompt)
        })
        .cloned();
    while prompt_render.is_none() {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/render") && message["params"]["viewId"] == json!(prompt)
        {
            prompt_render = Some(message);
        }
    }
    let prompt_render = prompt_render.unwrap();
    let prompt_version = prompt_render["params"]["version"].as_u64().unwrap();
    let text: Vec<String> =
        serde_json::from_value(prompt_render["params"]["text"].clone()).unwrap();
    let no_row = text.iter().position(|line| line == "No").expect("No row");

    server.send(json!({"jsonrpc":"2.0","id":8,"method":"view/key",
        "params":{"viewId":prompt,"version":prompt_version,"key":"Enter","editorMode":"normal",
            "cursor":{"line":no_row,"column":0}}}));
    let cancelled = server.response_with_id(8);
    assert!(
        cancelled["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == json!("popView") && effect["viewId"] == json!(prompt)),
        "{cancelled}"
    );
    for response in [&key_response, &respond_response, &opened, &cancelled] {
        assert!(
            !response["result"]["effects"]
                .as_array()
                .is_some_and(|effects| {
                    effects
                        .iter()
                        .any(|effect| effect["type"] == json!("editDocument"))
                }),
            "cancelling the confirm must not edit the document: {response}"
        );
    }
    server.drain_notifications();
    assert!(
        server
            .notifications
            .iter()
            .all(|message| message["method"] != json!("view/prompt")),
        "an undeclared confirm uses the document prompt: {:?}",
        server.notifications
    );
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index);
    assert_eq!(read(&tmp.path().join("f.txt")), worktree);
    assert!(!tmp.path().join(".git/index.lock").exists());

    // The prompt popped, so the status view is usable instead of refusing
    // commands while a prompt is pending.
    let refreshed = invoke_rich(&mut server, 9, &status, "common.refresh");
    assert!(refreshed.get("error").is_none(), "{refreshed}");
    let effects = refreshed["result"]["effects"].as_array().unwrap();
    assert!(
        effects
            .iter()
            .all(|effect| effect["type"] != json!("pushView")),
        "{refreshed}"
    );
    assert!(
        effects.iter().all(|effect| !effect["text"]
            .as_str()
            .is_some_and(|text| text.contains("pending prompt"))),
        "{refreshed}"
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn a_rich_client_without_a_pick_declaration_gets_the_document_prompt() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut server = TestServer::spawn(&[]);
    server.initialize(limited_rich(json!([]), json!([])));
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let status = server.response_with_id(2)["result"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke",
        "params":{"viewId":status,"command":"open.branch"}}));
    let opened = server.response_with_id(3);
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(server.recv()["method"], json!("view/render"));
    assert_eq!(server.recv()["method"], json!("view/keymap"));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"=","editorMode":"normal","cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"u","editorMode":"normal","cursor":{"line":0,"column":0}}}));

    let answer = server.response_with_id(5);
    let descriptor = answer["result"]["effects"]
        .as_array()
        .expect("effects")
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap_or_else(|| panic!("the prompt view: {answer}"))["descriptor"]
        .clone();
    assert_eq!(descriptor["kind"], json!("prompt"), "{answer}");
    let prompt_view = descriptor["id"]
        .as_str()
        .expect("prompt view id")
        .to_owned();
    let mut rendered = server.notifications.iter().any(|message| {
        message["method"] == json!("view/render")
            && message["params"]["viewId"] == json!(prompt_view)
    });
    while !rendered {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/render")
            && message["params"]["viewId"] == json!(prompt_view)
        {
            rendered = true;
        }
    }
    assert!(
        server
            .notifications
            .iter()
            .all(|message| message["method"] != json!("view/pick")),
        "an undeclared pick must fall back to the document: {:?}",
        server.notifications
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn pick_preference_truncates_native_items() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    tmp.git(&["branch", "b1"]);
    tmp.git(&["branch", "b2"]);
    let config_dir = tempfile::TempDir::new().expect("config tempdir");
    let config_path = config_dir.path().join("config.toml");
    std::fs::write(&config_path, "[pick]\nmax_items = 2\n").expect("write config");
    let config = config_path.to_string_lossy().to_string();
    let mut server = TestServer::spawn(&[("MAGIT_LSP_CONFIG", config.as_str())]);
    server.initialize(rich_pick_capabilities());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &status, "open.branch");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let menu_render = server.recv();
    assert_eq!(menu_render["method"], json!("view/render"), "{menu_render}");
    assert_eq!(server.recv()["method"], json!("view/keymap"));
    let version = menu_render["params"]["version"].as_u64().unwrap();
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
        "params":{"viewId":menu,"version":version}}));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"=","editorMode":"normal","cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"u","editorMode":"normal","cursor":{"line":0,"column":0}}}));

    let pick = loop {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/pick") {
            break message;
        }
    };
    let params = &pick["params"];
    assert_eq!(
        params["items"].as_array().unwrap().len(),
        2,
        "the preference truncates the native pick: {pick}"
    );
    assert_eq!(params["allowFreeText"], json!(true), "{pick}");

    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/pickRespond",
        "params":{"pickId":params["pickId"],"cancelled":true,"selected":[]}}));
    let _ = server.response_with_id(5);
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn pick_preference_beyond_the_declared_limit_uses_the_document_picker() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    tmp.git(&["branch", "b1"]);
    tmp.git(&["branch", "b2"]);
    let config_dir = tempfile::TempDir::new().expect("config tempdir");
    let config_path = config_dir.path().join("config.toml");
    std::fs::write(&config_path, "[pick]\nmax_items = 10\n").expect("write config");
    let config = config_path.to_string_lossy().to_string();
    let mut server = TestServer::spawn(&[("MAGIT_LSP_CONFIG", config.as_str())]);
    let mut capabilities = rich_pick_capabilities();
    capabilities["experimental"]["magit"]["pick"]["maxItems"] = json!(2);
    server.initialize(capabilities);
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &status, "open.branch");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(server.recv()["method"], json!("view/render"));
    assert_eq!(server.recv()["method"], json!("view/keymap"));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"=","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"u","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));

    let answer = server.response_with_id(5);
    let effects = answer["result"]["effects"].as_array().expect("effects");
    let warn = effects
        .iter()
        .find(|effect| effect["type"] == json!("message") && effect["level"] == json!("warn"))
        .unwrap_or_else(|| panic!("the over-limit warning: {answer}"));
    assert!(
        warn["text"]
            .as_str()
            .is_some_and(|text| text.contains("document picker")),
        "{warn}"
    );
    let prompt_view = effects
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .and_then(|effect| effect["descriptor"]["id"].as_str())
        .unwrap_or_else(|| panic!("the document prompt: {answer}"))
        .to_owned();

    let mut prompt_text = None;
    while prompt_text.is_none() {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/render")
            && message["params"]["viewId"] == json!(prompt_view)
        {
            prompt_text = serde_json::from_value(message["params"]["text"].clone()).ok();
        }
    }
    let prompt_text: Vec<String> = prompt_text.expect("prompt text");
    assert!(
        prompt_text
            .iter()
            .any(|line| line.contains("Set the upstream branch (--set-upstream-to=)")),
        "{prompt_text:?}"
    );
    assert!(
        server
            .notifications
            .iter()
            .all(|message| message["method"] != json!("view/pick")),
        "the over-limit preference must not send a native pick: {:?}",
        server.notifications
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

/// Initialize with a named `clientInfo`, which `TestServer::initialize` omits.
fn initialize_named(server: &mut TestServer, capabilities: Value, name: &str) -> Value {
    let mut message = initialize_message();
    message["params"]["capabilities"] = capabilities;
    message["params"]["clientInfo"] = json!({"name": name, "version": "1"});
    server.send(message);
    server.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    server.response_with_id(1)
}

#[test]
fn pick_client_override_allows_native_beyond_the_declared_limit() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    for branch in ["b1", "b2", "b3", "b4"] {
        tmp.git(&["branch", branch]);
    }
    let config_dir = tempfile::TempDir::new().expect("config tempdir");
    let config_path = config_dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        "[pick]\nmax_items = 10\n[pick.clients]\nTest = 5\n",
    )
    .expect("write config");
    let config = config_path.to_string_lossy().to_string();
    let mut server = TestServer::spawn(&[("MAGIT_LSP_CONFIG", config.as_str())]);
    let mut capabilities = rich_pick_capabilities();
    capabilities["experimental"]["magit"]["pick"]["maxItems"] = json!(2);
    initialize_named(&mut server, capabilities, "Test");
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &status, "open.branch");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let menu_render = server.recv();
    assert_eq!(menu_render["method"], json!("view/render"), "{menu_render}");
    assert_eq!(server.recv()["method"], json!("view/keymap"));
    let version = menu_render["params"]["version"].as_u64().unwrap();
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
        "params":{"viewId":menu,"version":version}}));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"=","editorMode":"normal","cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"u","editorMode":"normal","cursor":{"line":0,"column":0}}}));

    let pick = loop {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/pick") {
            break message;
        }
    };
    let params = &pick["params"];
    assert_eq!(
        params["items"].as_array().unwrap().len(),
        5,
        "the per-client override allows the full native pick: {pick}"
    );

    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/pickRespond",
        "params":{"pickId":params["pickId"],"cancelled":true,"selected":[]}}));
    let _ = server.response_with_id(5);
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn pick_override_absent_names_the_client_in_the_fallback() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    tmp.git(&["branch", "b1"]);
    tmp.git(&["branch", "b2"]);
    let config_dir = tempfile::TempDir::new().expect("config tempdir");
    let config_path = config_dir.path().join("config.toml");
    std::fs::write(&config_path, "[pick]\nmax_items = 10\n").expect("write config");
    let config = config_path.to_string_lossy().to_string();
    let mut server = TestServer::spawn(&[("MAGIT_LSP_CONFIG", config.as_str())]);
    let mut capabilities = rich_pick_capabilities();
    capabilities["experimental"]["magit"]["pick"]["maxItems"] = json!(2);
    initialize_named(&mut server, capabilities, "Test");
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &status, "open.branch");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(server.recv()["method"], json!("view/render"));
    assert_eq!(server.recv()["method"], json!("view/keymap"));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"=","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"u","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));

    let answer = server.response_with_id(5);
    let effects = answer["result"]["effects"].as_array().expect("effects");
    let warn = effects
        .iter()
        .find(|effect| effect["type"] == json!("message") && effect["level"] == json!("warn"))
        .unwrap_or_else(|| panic!("the over-limit warning: {answer}"));
    assert!(
        warn["text"]
            .as_str()
            .is_some_and(|text| text.contains("pick.clients[\"Test\"]")),
        "{warn}"
    );
    assert!(
        server
            .notifications
            .iter()
            .all(|message| message["method"] != json!("view/pick")),
        "the absent override must not send a native pick: {:?}",
        server.notifications
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn rich_choice_item_row_enter_answers_the_pick() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut server = TestServer::spawn(&[]);
    server.initialize(limited_rich(json!(["editDocument"]), json!(["confirm"])));
    let view = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &view, "open.branch");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(server.recv()["method"], json!("view/render"));
    assert_eq!(server.recv()["method"], json!("view/keymap"));

    // `=` prefixes the option; `u` picks the upstream branch and opens a
    // document prompt because the client declares no pick.
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"=","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"u","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    let (prompt, version, text) = loop {
        let message = server.recv();
        if message["method"] == json!("view/render") && message["params"]["viewId"] != json!(menu) {
            let prompt = message["params"]["viewId"].as_str().unwrap().to_owned();
            let version = message["params"]["version"].as_u64().unwrap();
            let text: Vec<String> =
                serde_json::from_value(message["params"]["text"].clone()).unwrap();
            break (prompt, version, text);
        }
    };
    let item = text
        .iter()
        .position(|line| line.strip_prefix("> ").unwrap_or(line).trim_end() == "main")
        .unwrap_or_else(|| panic!("main item row: {text:?}"));

    // Enter on the prompt title is not a row: the prompt stays active.
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/key",
        "params":{"viewId":prompt,"version":version,"key":"Enter","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    let non_row = server.response_with_id(6);
    assert!(
        !non_row["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == json!("popView")),
        "{non_row}"
    );
    let refused = invoke_rich(&mut server, 7, &view, "status.stage-all");
    assert!(
        refused["result"]["effects"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("pending prompt")),
        "{refused}"
    );

    server.send(json!({"jsonrpc":"2.0","id":8,"method":"view/key",
        "params":{"viewId":prompt,"version":version,"key":"Enter","editorMode":"normal",
            "cursor":{"line":item,"column":0}}}));
    let answered = server.response_with_id(8);
    assert!(
        answered["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == json!("popView") && effect["viewId"] == json!(prompt)),
        "{answered}"
    );
    let after = server.drain_notifications();
    let menu_reflects = server
        .notifications
        .iter()
        .chain(after.iter())
        .any(|message| {
            message["method"] == json!("view/render")
                && message["params"]["viewId"] == json!(menu)
                && message["params"]["text"].as_array().is_some_and(|lines| {
                    lines.iter().any(|line| {
                        line.as_str()
                            .is_some_and(|line| line.contains("--set-upstream-to=main"))
                    })
                })
        });
    assert!(
        menu_reflects,
        "menu does not reflect the picked upstream: {:?}",
        server.notifications
    );
    assert!(
        !server
            .notifications
            .iter()
            .chain(after.iter())
            .any(|message| message["method"] == json!("view/pick")),
        "an undeclared pick must not emit view/pick: {:?}",
        server.notifications
    );
    assert!(
        !server
            .notifications
            .iter()
            .chain(after.iter())
            .any(|message| message["method"] == json!("view/prompt")),
        "an undeclared pick must not emit view/prompt: {:?}",
        server.notifications
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}
