//! The real server over real pipes: framing, pipelining, concurrency, ordering.

mod common;
#[path = "server_async/repository.rs"]
mod repository;
#[path = "server_async/rich.rs"]
mod rich;

use repository::keymap_keys;

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
    view_id
}

/// Open a Plain logical view without waiting for Rich-only render/keymap pushes.
fn open_plain_view(server: &mut TestServer, id: i64, repo: &Value) -> String {
    server.send(open_view_message(id, repo));
    let opened = server.response_with_id(id);
    opened["result"]["id"]
        .as_str()
        .expect("view id")
        .to_string()
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
