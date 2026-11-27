//! The real server over real pipes: framing, pipelining, concurrency, ordering.

use magit_lsp_test_support as common;
#[path = "server_async/repository.rs"]
mod repository;
#[path = "server_async/rich.rs"]
mod rich;

use repository::keymap_keys;

#[cfg(unix)]
use common::fake_git::GatedGit;
use common::stdio::*;
use common::*;
use magit_lsp::status_view::StatusView;
use magit_lsp::view::{Ctx, View, Viewport};
use serde_json::{json, Value};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

fn repo_param(path: &std::path::Path) -> Value {
    json!(path.to_string_lossy())
}

/// Assert a polled status render against the synchronous oracle, appending the
/// server's captured stderr so a Git child killed or reaped mid-fetch is
/// decidable from the panic text (`child spawn` / `child term` warn lines).
fn assert_expanded_render(
    server: &TestServer,
    lines: &[String],
    expected: &[String],
    context: &str,
) {
    assert_eq!(
        lines,
        expected,
        "{context}: {lines:?}\nserver stderr:\n{}",
        server.stderr_text()
    );
}

/// The server's trace file for a client trace path (see `runner::TRACE_ENV`).
#[cfg(unix)]
fn completion_trace_file(trace: &std::path::Path) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{}.server", trace.display()))
}

/// Wait until the server processed `expected` status completions for `view`.
///
/// The completion task traces only after its render is enqueued, so observing
/// the event proves the completion's ordering turn has delivered (or dropped)
/// its output. This is the barrier the close regressions assert against; no
/// unrelated request or quiet window stands in for it.
#[cfg(unix)]
fn wait_for_completion_barrier(trace: &std::path::Path, view: &str, expected: usize) {
    let path = completion_trace_file(trace);
    let needle = format!("\"view\":\"{view}\"");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let events = std::fs::read_to_string(&path).unwrap_or_default();
        let seen = events
            .lines()
            .filter(|line| {
                line.contains("\"event\":\"status_completion_processed\"") && line.contains(&needle)
            })
            .count();
        if seen >= expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "status completion barrier {expected} not observed for {view}: {events}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

/// How long a gated acquisition wait tolerates a healthy but loaded CI runner.
#[cfg(unix)]
const GATE_ENTER_TIMEOUT: Duration = Duration::from_secs(60);

/// What a gated acquisition wait observed on the wire.
#[cfg(unix)]
#[derive(Debug)]
enum GateWait {
    /// The gated Git invocation is verifiably blocked inside the fake gate.
    Entered,
    /// The server published an acquisition failure for the view instead.
    AcquisitionFailed {
        /// The rendered view text naming the failure.
        text: Vec<String>,
        /// The server's captured stderr, carrying any kill/reap trace lines.
        stderr: String,
    },
}

/// Wait for a gated acquisition, resolving at once when it failed instead.
///
/// A plain `gate.wait_entered()` waits out its whole bound and hides the cause:
/// a Git child killed before its invocation-log echo never enters the gate, so
/// the test timed out silently while the server had already published
/// "Unable to load repository status" (docs/evidence/
/// native-functional-status-gate-macos-kill.md). This polls the gate marker and
/// the client stream together, so the wait ends at the first of: gate entered,
/// a render of `view` reporting the acquisition failure (returned with its
/// text), or the [`GATE_ENTER_TIMEOUT`] bound, whose panic carries the gate's
/// invocation log. Frames for other views are preserved in
/// [`TestServer::notifications`], so a caller's later wire assertions still see
/// them.
#[cfg(unix)]
fn wait_for_gate_or_failure(gate: &GatedGit, server: &mut TestServer, view: &str) -> GateWait {
    let deadline = Instant::now() + GATE_ENTER_TIMEOUT;
    loop {
        if gate.has_entered() {
            return GateWait::Entered;
        }
        if let Some(message) = server.try_recv_within(Duration::from_millis(20)) {
            if message["method"] == "view/render" && message["params"]["viewId"] == view {
                let text: Vec<String> =
                    serde_json::from_value(message["params"]["text"].clone()).unwrap_or_default();
                if text
                    .iter()
                    .any(|line| line.contains("Unable to load repository status"))
                {
                    return GateWait::AcquisitionFailed {
                        text,
                        stderr: server.stderr_text_settled(),
                    };
                }
            }
            server.notifications.push(message);
            continue;
        }
        assert!(
            Instant::now() < deadline,
            "the gated acquisition neither entered the gate nor reported a failure within \
             {GATE_ENTER_TIMEOUT:?}; the fake Git's invocation log:\n{}\nserver stderr:\n{}",
            gate.invocation_log(),
            server.stderr_text_settled()
        );
    }
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
#[test]
fn status_open_progress_spans_deferred_acquisition() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let gate = GatedGit::arm("status");
    let env = gate.child_env();
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let mut server = TestServer::spawn(&env);
    server.auto_respond_to_progress_create();
    server.initialize_rich(json!({"window": {"workDoneProgress": true}}));
    server.recv_timeout = Duration::from_secs(3);
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    // The begin is published before the open response and its loading render,
    // so it (and the create that follows it) arrive ahead of that render.
    let status_begin = server
        .notifications
        .iter()
        .find(|message| {
            message["method"] == "$/progress"
                && message["params"]["value"]["kind"] == json!("begin")
                && message["params"]["value"]["message"] == "status open"
        })
        .expect("the status-open begin precedes the loading render");
    let token = status_begin["params"]["token"].clone();
    assert!(
        server.notifications.iter().any(|message| message["method"]
            == "window/workDoneProgress/create"
            && message["params"]["token"] == token),
        "the create handshake accompanies the early begin"
    );
    let loading = server.recv();
    assert_eq!(loading["method"], "view/render");
    let version = loading["params"]["version"].as_u64().unwrap();
    assert_eq!(server.recv()["method"], "view/keymap");
    assert!(!gate.is_released(), "the token began while Git is held");

    gate.release();
    server.recv_timeout = Duration::from_secs(10);
    let ready = server.wait_for_view_render_after(&view, version);
    assert_eq!(ready["params"]["viewId"], view, "{ready}");
    // The token must survive past the fresh render: its end is enqueued only
    // after the completion render, so the client cannot be observed idle
    // between the acknowledged loading render and the delivered status.
    let deadline = Instant::now() + server.recv_timeout;
    loop {
        assert!(
            Instant::now() < deadline,
            "the acquisition token did not end after the completion: {:?}",
            server.notifications
        );
        let message = server.recv();
        if message["method"] == "$/progress"
            && message["params"]["token"] == token
            && message["params"]["value"]["kind"] == json!("end")
        {
            break;
        }
    }
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn status_open_fast_background_fetch_expands_like_the_sync_render() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    write(&tmp.path().join("untracked.txt"), "untracked\n");
    let mut server = TestServer::spawn(&[]);
    server.initialize_rich(json!({}));
    server.recv_timeout = Duration::from_secs(3);
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    let loading = server.recv();
    let loading_version = loading["params"]["version"].as_u64().unwrap();
    assert_eq!(server.recv()["method"], "view/keymap");

    // The completion publish is the closed render; the background fetch then
    // opens the default sections before the promise expires.
    server.recv_timeout = Duration::from_secs(10);
    let closed = server.wait_for_view_render_after(&view, loading_version);
    let closed_lines: Vec<String> =
        serde_json::from_value(closed["params"]["text"].clone()).unwrap();
    assert!(
        !closed_lines.iter().any(|line| line.contains("f.txt")),
        "{closed_lines:?}"
    );
    let closed_version = closed["params"]["version"].as_u64().unwrap();
    let expanded = server.wait_for_view_render_after(&view, closed_version);
    let lines: Vec<String> = serde_json::from_value(expanded["params"]["text"].clone()).unwrap();
    assert!(
        lines.iter().any(|line| line.contains("M f.txt")),
        "{lines:?}"
    );
    assert!(lines.iter().any(|line| line.contains("@@")), "{lines:?}");
    assert!(
        lines.iter().any(|line| line.contains("+line3 CHANGED")),
        "{lines:?}"
    );

    // The fast path must reproduce the synchronous fully-expanded render.
    let mut authoritative = StatusView::open(tmp.path().to_str().unwrap()).unwrap();
    let expected = authoritative.render(&Ctx {
        viewport: Viewport::default(),
        pending: &[],
    });
    assert_expanded_render(
        &server,
        &lines,
        &expected.lines,
        "the fast background fetch must reproduce the synchronous fully-expanded render",
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

/// A4: `sections.default_visibility = "open"` over the real server publishes
/// the expanded seed as rows first, and its batch streams the hunks into the
/// synchronous fully-expanded buffer; the same fixture under the default
/// (`auto`) still publishes the closed render first.
#[test]
fn status_open_config_open_streams_to_the_complete_buffer() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    write(&tmp.path().join("untracked.txt"), "untracked\n");
    let config = tempfile::tempdir().unwrap();
    let config_path = config.path().join("open.toml");
    write(&config_path, "[sections]\ndefault_visibility = \"open\"\n");
    let config_arg = config_path.to_string_lossy().into_owned();

    let mut server = TestServer::spawn_with_args(&["--config", &config_arg], &[]);
    server.initialize_rich(json!({}));
    server.recv_timeout = Duration::from_secs(3);
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    let loading = server.recv();
    let loading_version = loading["params"]["version"].as_u64().unwrap();
    assert_eq!(server.recv()["method"], "view/keymap");
    server.recv_timeout = Duration::from_secs(10);
    let ready = server.wait_for_view_render_after(&view, loading_version);
    let mut lines: Vec<String> = serde_json::from_value(ready["params"]["text"].clone()).unwrap();
    assert!(
        lines.iter().any(|line| line == "  M f.txt"),
        "the open seed publishes the expanded file rows first: {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("@@")),
        "the hunks arrive by batch, not on the seed render: {lines:?}"
    );

    // The batch streams the hunks into the complete synchronous render.
    let mut authoritative = StatusView::open(tmp.path().to_str().unwrap()).unwrap();
    let expected = authoritative.render(&Ctx {
        viewport: Viewport::default(),
        pending: &[],
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut id = 3;
    while lines != expected.lines && Instant::now() < deadline {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/resync",
            "params":{"viewId":view}}));
        let resync = server.response_with_id(id);
        id += 1;
        lines = serde_json::from_value(resync["result"]["text"].clone()).unwrap();
        if lines == expected.lines {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert_expanded_render(
        &server,
        &lines,
        &expected.lines,
        "the open batch must stream into the synchronous fully-expanded buffer",
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());

    let mut server = TestServer::spawn(&[]);
    server.initialize_rich(json!({}));
    server.recv_timeout = Duration::from_secs(3);
    server.send(open_view_message(3, &repo_param(tmp.path())));
    let opened = server.response_with_id(3);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    let loading = server.recv();
    let loading_version = loading["params"]["version"].as_u64().unwrap();
    assert_eq!(server.recv()["method"], "view/keymap");
    server.recv_timeout = Duration::from_secs(10);
    let closed = server.wait_for_view_render_after(&view, loading_version);
    let closed_lines: Vec<String> =
        serde_json::from_value(closed["params"]["text"].clone()).unwrap();
    assert!(
        !closed_lines.iter().any(|line| line.contains("f.txt")),
        "the default auto seed must publish the closed render first: {closed_lines:?}"
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

/// A3: a large `--open-promise-ms` keeps the auto fast path inside the promise,
/// so the expanded render installs. The zero case is covered by the text-mode
/// test, whose `@@` assertion can then never observe hunks.
#[test]
fn status_open_large_promise_observes_expanded_install() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    write(&tmp.path().join("untracked.txt"), "untracked\n");
    let mut server = TestServer::spawn_with_args(&["--open-promise-ms", "30000"], &[]);
    server.initialize_rich(json!({}));
    server.recv_timeout = Duration::from_secs(3);
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    let _ = server.recv();
    assert_eq!(server.recv()["method"], "view/keymap");
    server.recv_timeout = Duration::from_secs(10);

    let mut authoritative = StatusView::open(tmp.path().to_str().unwrap()).unwrap();
    let expected = authoritative.render(&Ctx {
        viewport: Viewport::default(),
        pending: &[],
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut lines: Vec<String> = Vec::new();
    let mut id = 3;
    while Instant::now() < deadline {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/resync",
            "params":{"viewId":view}}));
        let resync = server.response_with_id(id);
        id += 1;
        lines = serde_json::from_value(resync["result"]["text"].clone()).unwrap();
        if lines == expected.lines {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert_expanded_render(
        &server,
        &lines,
        &expected.lines,
        "the long promise must install the fully-expanded render",
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

/// A refresh inside the open promise re-arms the deferred fetch with the
/// refresh render's own deadline, so the sections still expand like the
/// synchronous render even though the original fetch was superseded.
#[cfg(unix)]
#[test]
fn status_open_refresh_inside_promise_rearms_background_fetch() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    // Park the opening fetch so only a re-armed fetch can expand the sections.
    let gate = GatedGit::arm("diff");
    let gate_env = gate.child_env();
    let env: Vec<_> = gate_env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let mut server = TestServer::spawn(&env);
    server.initialize_rich(json!({}));
    server.recv_timeout = Duration::from_secs(3);
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    let loading = server.recv();
    let loading_version = loading["params"]["version"].as_u64().unwrap();
    assert_eq!(server.recv()["method"], "view/keymap");
    server.recv_timeout = Duration::from_secs(10);
    let closed = server.wait_for_view_render_after(&view, loading_version);
    let closed_version = closed["params"]["version"].as_u64().unwrap();
    gate.wait_entered();

    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke",
        "params":{"viewId":view,"command":"common.refresh"}}));
    let refreshed = server.response_with_id(3);
    assert!(refreshed["error"].is_null(), "{refreshed}");
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/resync",
        "params":{"viewId":view}}));
    let current = server.response_with_id(4);
    let refreshed_version = current["result"]["version"].as_u64().unwrap();
    assert!(refreshed_version > closed_version, "{current}");

    // Poll the server-owned render: a re-armed fetch installs the same text as
    // the synchronous fully-expanded status opening.
    let mut authoritative = StatusView::open(tmp.path().to_str().unwrap()).unwrap();
    let expected = authoritative.render(&Ctx {
        viewport: Viewport::default(),
        pending: &[],
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut lines: Vec<String> = Vec::new();
    let mut id = 5;
    while Instant::now() < deadline {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/resync",
            "params":{"viewId":view}}));
        let resync = server.response_with_id(id);
        id += 1;
        lines = serde_json::from_value(resync["result"]["text"].clone()).unwrap();
        if lines == expected.lines {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert_expanded_render(
        &server,
        &lines,
        &expected.lines,
        "a refresh inside the promise must re-arm the fully-expanded render",
    );
    gate.release();
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn status_open_slow_background_fetch_leaves_sections_closed_until_opened() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let gate = GatedGit::arm("diff");
    let env = gate.child_env();
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let mut server = TestServer::spawn(&env);
    server.initialize_rich(json!({}));
    server.recv_timeout = Duration::from_secs(3);
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    let loading = server.recv();
    let loading_version = loading["params"]["version"].as_u64().unwrap();
    assert_eq!(server.recv()["method"], "view/keymap");

    server.recv_timeout = Duration::from_secs(10);
    let closed = server.wait_for_view_render_after(&view, loading_version);
    let closed_lines: Vec<String> =
        serde_json::from_value(closed["params"]["text"].clone()).unwrap();
    assert!(
        !closed_lines.iter().any(|line| line.contains("f.txt")),
        "{closed_lines:?}"
    );
    let closed_version = closed["params"]["version"].as_u64().unwrap();
    // The background fetch is held inside Git, so it cannot beat the promise.
    // Poll the gate and the client stream together so a killed or failed
    // background child surfaces with its invocation log and server stderr.
    match wait_for_gate_or_failure(&gate, &mut server, &view) {
        GateWait::Entered => {}
        GateWait::AcquisitionFailed { text, stderr } => panic!(
            "the background fetch must not fail the view; text: {text:?}\nserver stderr:\n{stderr}"
        ),
    }
    thread::sleep(Duration::from_millis(700));
    gate.release();

    // No expansion render followed: the closed view is still current.
    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/resync",
        "params":{"viewId":view}}));
    let resync = server.response_with_id(3);
    assert_eq!(resync["result"]["version"], closed_version, "{resync}");
    let still_closed: Vec<String> =
        serde_json::from_value(resync["result"]["text"].clone()).unwrap();
    assert!(
        !still_closed.iter().any(|line| line.contains("f.txt")),
        "{still_closed:?}"
    );

    // A later visibility open still fetches the hunks on demand.
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke",
        "params":{"viewId":view,"command":"common.show-all"}}));
    assert!(server.response_with_id(4)["error"].is_null());
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/resync",
        "params":{"viewId":view}}));
    let opened_text: Vec<String> =
        serde_json::from_value(server.response_with_id(5)["result"]["text"].clone()).unwrap();
    assert!(
        opened_text.iter().any(|line| line.contains("M f.txt")),
        "{opened_text:?}"
    );
    assert!(
        opened_text
            .iter()
            .any(|line| line.contains("+line3 CHANGED")),
        "{opened_text:?}"
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

/// The promise deadline abandons the background hunk fetch, so the owner must
/// terminate that fetch's Git child before its gate can hold it, and the closed
/// render must still stand.
#[cfg(unix)]
#[test]
fn status_open_expired_promise_cancels_background_hunk_fetch() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let gate = GatedGit::arm("diff");
    let env = gate.child_env();
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let mut server = TestServer::spawn(&env);
    server.initialize_rich(json!({}));
    server.recv_timeout = Duration::from_secs(3);
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    let loading = server.recv();
    let loading_version = loading["params"]["version"].as_u64().unwrap();
    assert_eq!(server.recv()["method"], "view/keymap");

    server.recv_timeout = Duration::from_secs(10);
    let closed = server.wait_for_view_render_after(&view, loading_version);
    let closed_version = closed["params"]["version"].as_u64().unwrap();
    // The background fetch is held inside Git past the promise deadline, so the
    // server's timeout abandons it.
    match wait_for_gate_or_failure(&gate, &mut server, &view) {
        GateWait::Entered => {}
        GateWait::AcquisitionFailed { text, stderr } => panic!(
            "the background fetch must not fail the view; text: {text:?}\nserver stderr:\n{stderr}"
        ),
    }
    // A child terminated inside the gate never runs the gate's `rm`, so the
    // marker cannot show termination: the recorded pid is the evidence. The
    // bounded poll also covers the wait past the promise deadline.
    let pid = gate
        .child_pid()
        .expect("the gated diff child recorded its pid");
    assert!(
        gate.wait_child_exited_within(Duration::from_secs(10)),
        "the abandoned fetch's git child {pid} still runs past the promise deadline; \
         invocation log:\n{}\nserver stderr:\n{}",
        gate.invocation_log(),
        server.stderr_text_settled()
    );

    // No expansion render followed: the closed view is still current.
    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/resync",
        "params":{"viewId":view}}));
    let resync = server.response_with_id(3);
    assert_eq!(resync["result"]["version"], closed_version, "{resync}");
    let still_closed: Vec<String> =
        serde_json::from_value(resync["result"]["text"].clone()).unwrap();
    assert!(
        !still_closed.iter().any(|line| line.contains("f.txt")),
        "{still_closed:?}"
    );
    gate.release();
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

/// A temporary config file seeding the interactive status opening as expanded.
#[cfg(unix)]
fn open_seed_config() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("open.toml");
    write(&path, "[sections]\ndefault_visibility = \"open\"\n");
    (dir, path.to_string_lossy().into_owned())
}

/// Receive frames until `id` answers, returning the first render of `view` that
/// follows `version`, plus that response.
///
/// The render and the response can arrive in either order, so both are
/// collected without assuming which comes first.
#[cfg(unix)]
fn render_and_response(
    server: &mut TestServer,
    view: &str,
    version: u64,
    id: i64,
) -> (Value, Value) {
    let mut render = None;
    let mut response = None;
    while response.is_none() || render.is_none() {
        let message = server.recv();
        if message["id"] == json!(id) {
            response = Some(message);
        } else if render.is_none()
            && message["method"] == "view/render"
            && message["params"]["viewId"] == json!(view)
            && message["params"]["version"]
                .as_u64()
                .is_some_and(|next| next > version)
        {
            render = Some(message);
        }
    }
    (
        render.expect("render after the base version"),
        response.expect("answering response"),
    )
}

/// A1: the seeded-open slow path publishes the expanded file rows while the
/// batch is still held in Git, then streams its hunks in one incremental render
/// that reproduces the synchronous fully-expanded oracle.
#[cfg(unix)]
#[test]
fn status_open_slow_batch_streams_hunks_by_incremental_edit() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (_config, config_arg) = open_seed_config();
    let gate = GatedGit::arm("diff");
    let gate_env = gate.child_env();
    let env: Vec<(&str, &str)> = gate_env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let mut server = TestServer::spawn_with_args(&["--config", &config_arg], &env);
    server.initialize_rich(json!({}));
    server.recv_timeout = Duration::from_secs(3);
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    let loading = server.recv();
    let loading_version = loading["params"]["version"].as_u64().unwrap();
    assert_eq!(server.recv()["method"], "view/keymap");
    server.recv_timeout = Duration::from_secs(10);

    // The first ready render is the expanded seed with file rows and no hunks.
    // It cannot have fetched inline: the gate claimant below is the batch.
    let ready = server.wait_for_view_render_after(&view, loading_version);
    let ready_version = ready["params"]["version"].as_u64().unwrap();
    let ready_lines: Vec<String> = serde_json::from_value(ready["params"]["text"].clone()).unwrap();
    assert!(
        ready_lines.iter().any(|line| line == "  M f.txt"),
        "the seed render shows the expanded file row: {ready_lines:?}"
    );
    assert!(
        !ready_lines.iter().any(|line| line.contains("@@")),
        "the hunks belong to the batch, not the seed render: {ready_lines:?}"
    );
    gate.wait_entered();
    assert!(!gate.is_released(), "the batch is still held inside Git");

    // Acknowledging the rows makes the streamed install one incremental edit.
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
        "params":{"viewId":view,"version":ready_version}}));
    // A neutral request orders the ack before the install can be published.
    server.send(
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/foldingRange",
        "params":{"textDocument":{"uri":opened["result"]["uri"]}}}),
    );
    server.response_with_id(3);

    gate.release();
    let installed = server.wait_for_view_render_after(&view, ready_version);
    assert_eq!(
        installed["params"]["baseVersion"],
        json!(ready_version),
        "the install must diff against the acknowledged rows: {installed}"
    );
    let edits: Vec<magit_lsp::protocol::LineEdit> =
        serde_json::from_value(installed["params"]["edits"].clone()).unwrap();
    let streamed = magit_lsp::diff_text::apply_edits(&ready_lines, &edits);

    let mut authoritative = StatusView::open(tmp.path().to_str().unwrap()).unwrap();
    let expected = authoritative.render(&Ctx {
        viewport: Viewport::default(),
        pending: &[],
    });
    assert!(
        expected
            .lines
            .iter()
            .any(|line| line.contains("+line3 CHANGED")),
        "the oracle itself carries the hunk body"
    );
    assert_expanded_render(
        &server,
        &streamed,
        &expected.lines,
        "the streamed hunks reproduce the synchronous fully-expanded render",
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

/// A2: a `common.refresh` during the in-flight batch supersedes it: the
/// refreshed render stays rows-only (no inline refetch under the lock) and its
/// renewed no-cutoff batch restores the hunks.
#[cfg(unix)]
#[test]
fn status_open_refresh_during_batch_rearms_rows_only_then_hunks() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (_config, config_arg) = open_seed_config();
    let gate = GatedGit::arm("diff");
    let gate_env = gate.child_env();
    let env: Vec<(&str, &str)> = gate_env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let mut server = TestServer::spawn_with_args(&["--config", &config_arg], &env);
    server.initialize_rich(json!({}));
    server.recv_timeout = Duration::from_secs(3);
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    let loading = server.recv();
    let loading_version = loading["params"]["version"].as_u64().unwrap();
    assert_eq!(server.recv()["method"], "view/keymap");
    server.recv_timeout = Duration::from_secs(10);
    let ready = server.wait_for_view_render_after(&view, loading_version);
    let ready_version = ready["params"]["version"].as_u64().unwrap();
    gate.wait_entered();

    // The refresh supersedes the held generation; it must answer with a
    // rows-only render rather than fetching the diffs inline.
    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke",
        "params":{"viewId":view,"command":"common.refresh"}}));
    let (refreshed, response) = render_and_response(&mut server, &view, ready_version, 3);
    assert!(response["error"].is_null(), "{response}");
    assert!(
        !gate.is_released(),
        "the refresh answered without waiting on the held diff"
    );
    assert!(
        refreshed["params"]["version"].as_u64().unwrap() > ready_version,
        "{refreshed}"
    );
    let refreshed_lines: Vec<String> =
        serde_json::from_value(refreshed["params"]["text"].clone()).unwrap();
    assert!(
        refreshed_lines.iter().any(|line| line.contains("M f.txt")),
        "the refreshed render keeps the expanded rows: {refreshed_lines:?}"
    );
    assert!(
        !refreshed_lines.iter().any(|line| line.contains("@@")),
        "the refreshed render must stay rows-only: {refreshed_lines:?}"
    );

    gate.release();
    // The renewed batch has no cutoff and restores the hunks.
    let mut authoritative = StatusView::open(tmp.path().to_str().unwrap()).unwrap();
    let expected = authoritative.render(&Ctx {
        viewport: Viewport::default(),
        pending: &[],
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut lines = refreshed_lines;
    let mut id = 4;
    while lines != expected.lines && Instant::now() < deadline {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/resync",
            "params":{"viewId":view}}));
        let resync = server.response_with_id(id);
        id += 1;
        lines = serde_json::from_value(resync["result"]["text"].clone()).unwrap();
        if lines == expected.lines {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert_expanded_render(
        &server,
        &lines,
        &expected.lines,
        "the re-armed batch must restore the fully-expanded hunks",
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

/// A3: a user collapse of one section while the batch is in flight wins — the
/// post-install render respects it, proving the install never rewrote
/// visibility.
#[cfg(unix)]
#[test]
fn status_open_user_collapse_during_batch_wins_over_the_install() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .staged_file("s.txt", "staged\n")
        .modify_standard()
        .build();
    let (_config, config_arg) = open_seed_config();
    let gate = GatedGit::arm("diff");
    let gate_env = gate.child_env();
    let env: Vec<(&str, &str)> = gate_env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let mut server = TestServer::spawn_with_args(&["--config", &config_arg], &env);
    server.initialize_rich(json!({}));
    server.recv_timeout = Duration::from_secs(3);
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    let loading = server.recv();
    let loading_version = loading["params"]["version"].as_u64().unwrap();
    assert_eq!(server.recv()["method"], "view/keymap");
    server.recv_timeout = Duration::from_secs(10);
    let ready = server.wait_for_view_render_after(&view, loading_version);
    let ready_version = ready["params"]["version"].as_u64().unwrap();
    let ready_lines: Vec<String> = serde_json::from_value(ready["params"]["text"].clone()).unwrap();
    assert!(
        ready_lines.iter().any(|line| line.contains("s.txt")),
        "the seed render shows the staged file row: {ready_lines:?}\nserver stderr:\n{}",
        server.stderr_text()
    );
    let staged_line = ready_lines
        .iter()
        .position(|line| line == "Staged changes (1)")
        .expect("the staged section header");
    gate.wait_entered();

    // Collapse the staged section while the batch is held: the user decision
    // must survive the install.
    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke",
        "params":{"viewId":view,"command":"common.toggle-section","version":ready_version,
        "cursor":{"line":staged_line,"column":0}}}));
    let (collapsed, response) = render_and_response(&mut server, &view, ready_version, 3);
    assert!(response["error"].is_null(), "{response}");
    assert!(
        !gate.is_released(),
        "the collapse must not fetch the other section inline"
    );
    let collapsed_lines: Vec<String> =
        serde_json::from_value(collapsed["params"]["text"].clone()).unwrap();
    assert!(
        !collapsed_lines.iter().any(|line| line.contains("s.txt")),
        "the collapsed section hides its file row: {collapsed_lines:?}"
    );
    assert!(
        !collapsed_lines.iter().any(|line| line.contains("@@")),
        "no hunk arrives inline while the batch is held: {collapsed_lines:?}"
    );

    gate.release();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut lines = collapsed_lines;
    let mut id = 4;
    while !lines.iter().any(|line| line.contains("@@")) && Instant::now() < deadline {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/resync",
            "params":{"viewId":view}}));
        let resync = server.response_with_id(id);
        id += 1;
        lines = serde_json::from_value(resync["result"]["text"].clone()).unwrap();
        if lines.iter().any(|line| line.contains("@@")) {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        lines.iter().any(|line| line.contains("+line3 CHANGED")),
        "the batch must install the unstaged hunks: {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("s.txt")),
        "the install must respect the user's collapse: {lines:?}"
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn status_open_progress_ends_when_view_closes_during_acquisition() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let gate = GatedGit::arm("status");
    let env = gate.child_env();
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let mut server = TestServer::spawn(&env);
    server.auto_respond_to_progress_create();
    server.initialize_rich(json!({"window": {"workDoneProgress": true}}));
    server.recv_timeout = Duration::from_secs(3);
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    let token = server
        .notifications
        .iter()
        .find(|message| {
            message["method"] == "$/progress"
                && message["params"]["value"]["kind"] == json!("begin")
                && message["params"]["value"]["message"] == "status open"
        })
        .expect("the status-open begin precedes the loading render")["params"]["token"]
        .clone();
    let loading = server.recv();
    assert_eq!(loading["method"], "view/render");
    assert_eq!(server.recv()["method"], "view/keymap");

    // Closing the view mid-acquisition leaves the completion with nothing to
    // install; its token must still end, so no busy state can get stuck.
    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/close",
        "params":{"viewId":view}}));
    let closed = server.response_with_id(3);
    assert!(closed["error"].is_null(), "{closed}");
    assert!(!gate.is_released(), "the close answered while Git is held");
    gate.release();
    server.recv_timeout = Duration::from_secs(10);
    let deadline = Instant::now() + server.recv_timeout;
    loop {
        assert!(
            Instant::now() < deadline,
            "the closed view's token did not end: {:?}",
            server.notifications
        );
        let message = server.recv();
        if message["method"] == "$/progress"
            && message["params"]["token"] == token
            && message["params"]["value"]["kind"] == json!("end")
        {
            break;
        }
    }
    let drained = server.drain_notifications();
    assert!(
        drained
            .iter()
            .chain(server.notifications.iter())
            .all(
                |message| message["method"] != "view/render" || message["params"]["viewId"] != view
            ),
        "the closed view received a completion render: {:?}",
        drained
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn status_open_progress_end_does_not_wait_for_late_create_reply() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let gate = GatedGit::arm("status");
    let env = gate.child_env();
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let mut server = TestServer::spawn(&env);
    // No automatic create reply: this client answers the progress create only
    // after the deferred acquisition has already completed.
    server.initialize_rich(json!({"window": {"workDoneProgress": true}}));
    server.recv_timeout = Duration::from_secs(3);
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    let create = server
        .notifications
        .iter()
        .find(|message| message["method"] == "window/workDoneProgress/create")
        .expect("the create accompanies the early begin")
        .clone();
    let begin = server
        .notifications
        .iter()
        .find(|message| {
            message["method"] == "$/progress"
                && message["params"]["value"]["kind"] == json!("begin")
                && message["params"]["value"]["message"] == "status open"
        })
        .expect("the status begin precedes the loading render");
    let token = begin["params"]["token"].clone();
    assert_eq!(create["params"]["token"], token, "{create}");
    let loading = server.recv();
    assert_eq!(loading["method"], "view/render");
    let version = loading["params"]["version"].as_u64().unwrap();
    assert_eq!(server.recv()["method"], "view/keymap");
    assert!(!gate.is_released(), "the create arrived while Git is held");

    // The begin was published with the open response, so the end follows the
    // completion render without waiting for the client to confirm the token.
    gate.release();
    server.recv_timeout = Duration::from_secs(10);
    let ready = server.wait_for_view_render_after(&view, version);
    assert_eq!(ready["params"]["viewId"], view, "{ready}");
    let deadline = Instant::now() + server.recv_timeout;
    let ended = loop {
        assert!(
            Instant::now() < deadline,
            "the acquisition token did not end after the completion: {:?}",
            server.notifications
        );
        let message = server.recv();
        if message["method"] == "$/progress" && message["params"]["token"] == token {
            assert_eq!(
                message["params"]["value"]["kind"],
                json!("end"),
                "the first observed phase after begin must be end: {message}"
            );
            break true;
        }
    };
    assert!(ended, "the token ended after the completion render");

    // An unanswered create is harmless: its late reply must not publish a
    // second begin/end pair for an already-finished token.
    server.drain_notifications();
    server.send(json!({"jsonrpc": "2.0", "id": create["id"], "result": null}));
    let drained = server.drain_notifications();
    assert!(
        drained
            .iter()
            .all(|message| message["method"] != "$/progress"
                || message["params"]["token"] != token),
        "a late create reply republished the finished token: {drained:?}"
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

/// A watcher burst that fires while a commit-confirm's detached worker still
/// owns the repository's publication order must queue behind it instead of
/// publishing an interleaved status edit. macOS CI observed exactly that
/// interleave: an extra `workspace/applyEdit` landed between the confirm's
/// own edit and the `Committed:` message. The commit-msg hook below parks Git
/// mid-tail after the confirm replied, so a deliberate worktree touch arms an
/// eager burst against the held publication turn — deterministically, with no
/// timing window on the asserting side.
#[cfg(unix)]
#[test]
fn commit_confirm_holds_the_publication_order_against_a_watcher_refresh() {
    magit_lsp_test_support::cargo_server_binary!();
    use std::os::unix::fs::PermissionsExt;

    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .staged_file("f.txt", standard_changes())
        .build();
    // Gate markers live inside .git, where only state files the views render
    // count as watcher-relevant, so nothing but the deliberate touch below
    // can arm a refresh burst while Git is parked.
    let ready = tmp.path().join(".git").join("hook.ready");
    let release = tmp.path().join(".git").join("hook.release");
    let hook = tmp.path().join(".git").join("hooks").join("commit-msg");
    write(
        &hook,
        &format!(
            "#!/bin/sh\nprintf ok > '{ready}'\ni=0\nwhile [ ! -f '{release}' ] && [ \"$i\" -lt 300 ]; do i=$((i+1)); sleep 0.05; done\nexit 0\n",
            ready = ready.display(),
            release = release.display(),
        ),
    );
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    // Release the gate even when an assertion fails, so Git can never outlive
    // the test on the panic path.
    struct ReleaseGate(std::path::PathBuf);
    impl Drop for ReleaseGate {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.0, "release");
        }
    }
    let _gate = ReleaseGate(release.clone());

    let mut server = TestServer::spawn(&[("MAGIT_LSP_WATCH_DEBOUNCE_MS", "25")]);
    server.auto_respond_to_progress_create();
    server.initialize_rich(json!({"window": {"workDoneProgress": true}}));
    let view = open_view(&mut server, 2, &repo_param(tmp.path()));
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{"viewId":view,"command":"commit.create"}}));
    let mut edit = None;
    while edit.is_none() {
        let message = server.recv();
        if message["method"] == json!("view/effects") {
            edit = message["params"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["type"] == json!("editDocument"))
                .cloned()
                .or(edit);
        }
    }
    let edit = edit.unwrap();
    // The one explicit full-sync stream, then a confirm barrier on the exact
    // synchronized checkpoint, exactly as a rich client drives it.
    server.send_raw(&frame(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":edit["uri"],"languageId":"gitcommit","version":1,"text":edit["text"]}}})));
    server.send_raw(&frame(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":edit["uri"],"version":2},"contentChanges":[{"text":"Gated publication\n"}]}})));
    server.send_raw(&frame(json!({"jsonrpc":"2.0","id":501,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[{"command":"common.confirm","runId":edit["runId"],"uri":edit["uri"],"version":2,"source":"synchronized"}]}})));
    assert!(server.response_with_id(501).get("error").is_none());
    // The hook's ready marker proves Git is parked mid-tail: the confirm has
    // already replied, so the commit's publication turn is held.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        assert!(
            Instant::now() < deadline,
            "commit-msg hook did not become ready"
        );
        thread::sleep(Duration::from_millis(20));
    }

    // A worktree touch arms an eager watcher burst against the held
    // publication order: the refresh must queue behind the in-flight
    // mutation, not publish an interleaved status edit of its own.
    write(&tmp.path().join("touched.txt"), "worktree noise\n");
    let drained = server.drain_notifications();
    assert!(
        drained.iter().all(|message| {
            message["method"] != "view/render" && message["method"] != "workspace/applyEdit"
        }),
        "a watcher refresh published while the commit held the publication order: {drained:?}"
    );

    // Completing the commit releases the window: the worker's own render
    // sees the touched file, the Committed outcome follows it, and only then
    // may the queued burst refresh.
    std::fs::write(&release, "release").unwrap();
    // Full-text renders carry `text`; incremental renders carry per-edit
    // `lines` against the acknowledged base version.
    fn render_lines(render: &Value) -> Vec<String> {
        let mut lines: Vec<String> =
            serde_json::from_value(render["params"]["text"].clone()).unwrap_or_default();
        if let Some(edits) = render["params"]["edits"].as_array() {
            for edit in edits {
                lines.extend(
                    serde_json::from_value::<Vec<String>>(edit["lines"].clone())
                        .unwrap_or_default(),
                );
            }
        }
        lines
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut committed = false;
    let mut render_saw_touch = false;
    while !(committed && render_saw_touch) {
        for message in server.drain_notifications() {
            if message["method"] == "view/effects"
                && message["params"]["effects"]
                    .as_array()
                    .is_some_and(|effects| {
                        effects.iter().any(|effect| {
                            effect["type"] == "message"
                                && effect["text"]
                                    .as_str()
                                    .is_some_and(|text| text.contains("Committed:"))
                        })
                    })
            {
                committed = true;
            }
            // The published render keeps sections closed, so the touched file
            // is visible through its section count rather than a file row.
            if message["method"] == "view/render"
                && render_lines(&message)
                    .iter()
                    .any(|line| line == "Untracked files (1)")
            {
                render_saw_touch = true;
            }
        }
        assert!(
            Instant::now() < deadline,
            "expected the committed outcome and a render mentioning the touched file after the release"
        );
    }
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn status_open_arms_watcher_after_open_response() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let gate = GatedGit::arm("status");
    let trace_dir = tempfile::tempdir().unwrap();
    let trace = trace_dir.path().join("status-watcher-deferred");
    let trace_env = trace.to_string_lossy().into_owned();
    let gate_env = gate.child_env();
    let mut env: Vec<(&str, &str)> = gate_env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    env.push(("MAGIT_LSP_TRACE", trace_env.as_str()));
    env.push(("MAGIT_LSP_WATCH_DEBOUNCE_MS", "50"));
    let mut server = TestServer::spawn(&env);
    server.initialize_rich(json!({}));
    server.recv_timeout = Duration::from_secs(3);
    let mut open = open_view_message(2, &repo_param(tmp.path()));
    open["params"]["traceId"] = json!(7);
    server.send(open);
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"].as_str().expect("status view id");
    let loading = server.recv();
    assert_eq!(loading["method"], "view/render");
    assert_eq!(
        loading["params"]["text"],
        json!(["Head: main", "Loading repository status…"])
    );
    let version = loading["params"]["version"].as_u64().unwrap();
    assert_eq!(server.recv()["method"], "view/keymap");
    assert!(!gate.is_released(), "the token began while Git is held");

    // The deferred task installs trace id 7 on its blocking thread, so the
    // watcher phases belong to this open's trace. Wait for the one terminal
    // "armed" `phase_watcher_setup` event (the full-duration registration)
    // plus the `phase_watcher_setup_start` marker emitted before the walk.
    let path = completion_trace_file(&trace);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let events = std::fs::read_to_string(&path).unwrap_or_default();
        let records: Vec<Value> = events
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|record| record["id"] == json!(7))
            .collect();
        let handler_end = records
            .iter()
            .position(|record| record["event"] == json!("handler_end"));
        let armed: Vec<usize> = records
            .iter()
            .enumerate()
            .filter(|(_, record)| {
                record["event"] == json!("phase_watcher_setup")
                    && record["detail"]["outcome"] == json!("armed")
            })
            .map(|(index, _)| index)
            .collect();
        let started: Vec<usize> = records
            .iter()
            .enumerate()
            .filter(|(_, record)| record["event"] == json!("phase_watcher_setup_start"))
            .map(|(index, _)| index)
            .collect();
        if let (Some(handler_end), Some(&armed_at), Some(&started_at)) =
            (handler_end, armed.first(), started.first())
        {
            assert!(
                armed_at > handler_end && started_at > handler_end,
                "watcher phases were traced before the open response's handler_end: {records:?}"
            );
            assert_eq!(
                armed.len(),
                1,
                "expected exactly one terminal armed watcher setup: {records:?}"
            );
            assert_eq!(
                started.len(),
                1,
                "expected exactly one watcher setup start marker: {records:?}"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "deferred watcher setup not observed for trace 7: {events}"
        );
        thread::sleep(Duration::from_millis(10));
    }

    gate.release();
    server.recv_timeout = Duration::from_secs(10);
    let ready = server.wait_for_view_render_after(view, version);
    assert_eq!(ready["params"]["viewId"], view, "{ready}");

    // Only now, with the watcher demonstrably armed, does a real worktree
    // event have to refresh the view.
    write(
        &tmp.path().join("watcher-deferred-proof.txt"),
        "watcher is armed\n",
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "the deferred watcher did not refresh for the marker"
        );
        let message = server.recv();
        // Sections stay closed on the published render: the new worktree file
        // surfaces as the untracked count, not as a file row.
        if message["method"] == "view/render"
            && message["params"]["text"]
                .to_string()
                .contains("Untracked files (1)")
        {
            break;
        }
    }
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn status_open_text_mode_defers_held_completion_until_exit() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    write(
        &tmp.path().join("status-open-text.txt"),
        "fresh while editing\n",
    );
    let gate = GatedGit::arm("status");
    let trace_dir = tempfile::tempdir().unwrap();
    let trace = trace_dir.path().join("status-text-completion");
    let trace_env = trace.to_string_lossy().into_owned();
    let gate_env = gate.child_env();
    let mut env: Vec<(&str, &str)> = gate_env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    env.push(("MAGIT_LSP_TRACE", trace_env.as_str()));
    // Zero discards the deferred open's background fetch deterministically, so
    // the exit render below can never observe an installed hunk body.
    let mut server = TestServer::spawn_with_args(&["--open-promise-ms", "0"], &env);
    server.initialize_rich(json!({}));
    let mut open = open_view_message(2, &repo_param(tmp.path()));
    open["params"]["traceId"] = json!(7);
    server.send(open);
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"].as_str().expect("status view id");
    let loading = server.recv();
    assert_eq!(loading["method"], "view/render");
    assert_eq!(
        loading["params"]["text"],
        json!(["Head: main", "Loading repository status…"])
    );
    let version = loading["params"]["version"].as_u64().unwrap();
    assert_eq!(server.recv()["method"], "view/keymap");
    gate.wait_entered();

    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke",
        "params":{"viewId":view,"command":"common.toggle-text-mode"}}));
    let toggled = server.response_with_id(3);
    assert!(toggled["error"].is_null(), "{toggled}");
    let mode_render = server.recv();
    assert_eq!(mode_render["method"], "view/render", "{mode_render}");
    assert_eq!(mode_render["params"]["mode"], "text", "{mode_render}");
    assert_eq!(mode_render["params"]["version"], json!(version));
    assert!(!gate.is_released(), "text mode entered while Git is held");

    gate.release();
    // The completion must actually be processed before absence of output is
    // asserted; the trace event follows its enqueued render.
    wait_for_completion_barrier(&trace, view, 1);
    // The resync round-trip also drains any completion notification, so the
    // assertion below sees every message the completion could have sent.
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/resync",
        "params":{"viewId":view}}));
    let held = server.response_with_id(4);
    assert_eq!(held["result"]["version"], json!(version));
    assert_eq!(
        held["result"]["text"],
        json!(["Head: main", "Loading repository status…"])
    );
    // The acquired success must stay unpublished and unversioned in text mode.
    assert!(
        !server.notifications.iter().any(
            |message| message["method"] == "view/render" && message["params"]["viewId"] == view
        ),
        "completion rendered into text mode: {:?}",
        server.notifications
    );

    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke",
        "params":{"viewId":view,"command":"common.toggle-text-mode"}}));
    let exited = server.response_with_id(5);
    assert!(exited["error"].is_null(), "{exited}");
    let ready = server.recv();
    assert_eq!(ready["method"], "view/render", "{ready}");
    assert_eq!(ready["params"]["mode"], "interactive", "{ready}");
    assert_eq!(
        ready["params"]["version"],
        json!(version + 1),
        "the stored completion advances the version exactly once, on exit"
    );
    assert!(ready["params"].get("baseVersion").is_none(), "{ready}");
    assert!(ready["params"].get("edits").is_none(), "{ready}");
    let text: Vec<String> = serde_json::from_value(ready["params"]["text"].clone()).unwrap();
    // The stored completion publishes the closed render: the fresh untracked
    // file appears in its section count, with no file or hunk row.
    assert!(
        text.iter().any(|line| line == "Untracked files (1)"),
        "{text:?}"
    );
    assert!(
        text.iter().any(|line| line == "Unstaged changes (1)"),
        "{text:?}"
    );
    assert!(text.iter().all(|line| !line.contains("@@")), "{text:?}");
    assert!(!text
        .iter()
        .any(|line| line.contains("Loading repository status")));
    assert_eq!(
        gate.invocation_count(),
        1,
        "exiting text mode must expose the acquired result without another Git read"
    );
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/resync",
        "params":{"viewId":view}}));
    let resync = server.response_with_id(6);
    assert_eq!(resync["result"]["version"], ready["params"]["version"]);
    assert_eq!(resync["result"]["text"], ready["params"]["text"]);
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn status_open_text_mode_exit_honors_refresh_after_held_completion() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    write(
        &tmp.path().join("status-open-text.txt"),
        "fresh while editing\n",
    );
    let gate = GatedGit::arm("status");
    let trace_dir = tempfile::tempdir().unwrap();
    let trace = trace_dir.path().join("status-later-refresh");
    let trace_env = trace.to_string_lossy().into_owned();
    let gate_env = gate.child_env();
    let mut env: Vec<(&str, &str)> = gate_env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    env.push(("MAGIT_LSP_TRACE", trace_env.as_str()));
    // Zero discards the background fetch deterministically, so the exit render
    // below can never observe an installed hunk body.
    let mut server = TestServer::spawn_with_args(&["--open-promise-ms", "0"], &env);
    server.initialize_rich(json!({}));
    let mut open = open_view_message(2, &repo_param(tmp.path()));
    open["params"]["traceId"] = json!(7);
    server.send(open);
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"].as_str().expect("status view id");
    let loading = server.recv();
    assert_eq!(loading["method"], "view/render");
    assert_eq!(server.recv()["method"], "view/keymap");
    let version = loading["params"]["version"].as_u64().unwrap();
    gate.wait_entered();

    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke",
        "params":{"viewId":view,"command":"common.toggle-text-mode"}}));
    let toggled = server.response_with_id(3);
    assert!(toggled["error"].is_null(), "{toggled}");
    let mode_render = server.recv();
    assert_eq!(mode_render["method"], "view/render", "{mode_render}");
    assert_eq!(mode_render["params"]["mode"], "text", "{mode_render}");

    gate.release();
    wait_for_completion_barrier(&trace, view, 1);
    // The resync round-trip drains any completion notification, so the
    // assertions below see every message the completion could have sent.
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/resync",
        "params":{"viewId":view}}));
    let held = server.response_with_id(4);
    assert_eq!(held["result"]["version"], json!(version));
    assert_eq!(
        held["result"]["text"],
        json!(["Head: main", "Loading repository status…"])
    );
    assert!(
        !server.notifications.iter().any(
            |message| message["method"] == "view/render" && message["params"]["viewId"] == view
        ),
        "completion rendered into text mode: {:?}",
        server.notifications
    );

    // A repository change after the acquired completion must survive into the
    // queued refresh instead of being published as the older acquired state.
    write(&tmp.path().join("later-refresh.txt"), "later refresh\n");
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke",
        "params":{"viewId":view,"command":"common.refresh"}}));
    let queued = server.response_with_id(5);
    assert!(queued["error"].is_null(), "{queued}");
    assert!(
        !server.notifications.iter().any(
            |message| message["method"] == "view/render" && message["params"]["viewId"] == view
        ),
        "a text-mode refresh must not render: {:?}",
        server.notifications
    );

    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke",
        "params":{"viewId":view,"command":"common.toggle-text-mode"}}));
    let exited = server.response_with_id(6);
    assert!(exited["error"].is_null(), "{exited}");
    let ready = server.recv();
    assert_eq!(ready["method"], "view/render", "{ready}");
    assert_eq!(ready["params"]["mode"], "interactive", "{ready}");
    assert_eq!(
        ready["params"]["version"],
        json!(version + 1),
        "the exit replacement advances the version exactly once"
    );
    assert!(ready["params"].get("baseVersion").is_none(), "{ready}");
    assert!(ready["params"].get("edits").is_none(), "{ready}");
    let text: Vec<String> = serde_json::from_value(ready["params"]["text"].clone()).unwrap();
    // Both untracked files — the one present at acquisition and the one added
    // before the queued refresh — are counted in the closed render. The count
    // of two is only reachable from the refreshed status, not the older one.
    assert!(
        text.iter().any(|line| line == "Untracked files (2)"),
        "the exit refresh must include the file created after the completion: {text:?}"
    );
    assert!(
        text.iter().any(|line| line == "Unstaged changes (1)"),
        "{text:?}"
    );
    assert!(text.iter().all(|line| !line.contains("@@")), "{text:?}");
    assert!(!text
        .iter()
        .any(|line| line.contains("Loading repository status")));
    assert_eq!(
        gate.invocation_count(),
        2,
        "the queued refresh re-reads status exactly once on exit"
    );

    server.send(json!({"jsonrpc":"2.0","id":7,"method":"view/resync",
        "params":{"viewId":view}}));
    let resync = server.response_with_id(7);
    assert_eq!(resync["result"]["version"], ready["params"]["version"]);
    assert_eq!(resync["result"]["text"], ready["params"]["text"]);
    assert!(
        !server.notifications.iter().any(
            |message| message["method"] == "view/render" && message["params"]["viewId"] == view
        ),
        "the exit must emit exactly one full replacement: {:?}",
        server.notifications
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn status_open_close_drops_held_success_without_resurrection() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let gate = GatedGit::arm("status");
    let trace_dir = tempfile::tempdir().unwrap();
    let trace = trace_dir.path().join("status-completion");
    let trace_env = trace.to_string_lossy().into_owned();
    let gate_env = gate.child_env();
    let mut env: Vec<(&str, &str)> = gate_env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    env.push(("MAGIT_LSP_TRACE", trace_env.as_str()));
    let mut server = TestServer::spawn(&env);
    server.initialize_rich(json!({}));
    let mut open = open_view_message(2, &repo_param(tmp.path()));
    open["params"]["traceId"] = json!(7);
    server.send(open);
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"].as_str().expect("status view id");
    let loading = server.recv();
    assert_eq!(loading["method"], "view/render");
    assert_eq!(server.recv()["method"], "view/keymap");
    gate.wait_entered();

    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/close",
        "params":{"viewId":view}}));
    let closed = server.response_with_id(3);
    assert!(closed["error"].is_null(), "{closed}");
    gate.release();
    // The completion must actually execute before absence of output is
    // asserted; the trace event follows its enqueued render.
    wait_for_completion_barrier(&trace, view, 1);

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/resync",
        "params":{"viewId":view}}));
    let resync = server.response_with_id(4);
    assert!(
        !resync["error"].is_null(),
        "the closed view must stay unroutable: {resync}"
    );
    assert!(
        !server.notifications.iter().any(
            |message| message["method"] == "view/render" && message["params"]["viewId"] == view
        ),
        "closed status view was resurrected: {:?}",
        server.notifications
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn status_open_close_drops_held_error_without_resurrection() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let gate = GatedGit::arm("status");
    let trace_dir = tempfile::tempdir().unwrap();
    let trace = trace_dir.path().join("status-error-completion");
    let trace_env = trace.to_string_lossy().into_owned();
    let gate_env = gate.child_env();
    let mut env: Vec<(&str, &str)> = gate_env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    env.push(("MAGIT_LSP_TRACE", trace_env.as_str()));
    let mut server = TestServer::spawn(&env);
    server.initialize_rich(json!({}));
    let mut open = open_view_message(2, &repo_param(tmp.path()));
    open["params"]["traceId"] = json!(8);
    server.send(open);
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"].as_str().expect("status view id");
    assert_eq!(server.recv()["method"], "view/render");
    assert_eq!(server.recv()["method"], "view/keymap");
    gate.wait_entered();

    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/close",
        "params":{"viewId":view}}));
    assert!(server.response_with_id(3)["error"].is_null());
    let git_dir = tmp.path().join(".git");
    let saved_git = tmp.path().join(".git-held");
    std::fs::rename(&git_dir, &saved_git).unwrap();
    gate.release();
    // The held error must be processed before asserting it was not rendered.
    wait_for_completion_barrier(&trace, view, 1);

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/resync",
        "params":{"viewId":view}}));
    let resync = server.response_with_id(4);
    assert!(
        !resync["error"].is_null(),
        "the closed view must stay unroutable: {resync}"
    );
    assert!(
        !server.notifications.iter().any(
            |message| message["method"] == "view/render" && message["params"]["viewId"] == view
        ),
        "closed status view was resurrected by late Git error: {:?}",
        server.notifications
    );
    std::fs::rename(saved_git, git_dir).unwrap();
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn status_open_refreshes_while_loading_are_coalesced() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let gate = GatedGit::arm("status");
    let trace_dir = tempfile::tempdir().unwrap();
    let trace = trace_dir.path().join("status-coalesced-completion");
    let trace_env = trace.to_string_lossy().into_owned();
    let gate_env = gate.child_env();
    let mut env: Vec<(&str, &str)> = gate_env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    env.push(("MAGIT_LSP_TRACE", trace_env.as_str()));
    let mut server = TestServer::spawn(&env);
    server.initialize_rich(json!({}));
    let mut open = open_view_message(2, &repo_param(tmp.path()));
    open["params"]["traceId"] = json!(9);
    server.send(open);
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    let loading = server.recv();
    assert_eq!(
        loading["params"]["text"],
        json!(["Head: main", "Loading repository status…"])
    );
    assert_eq!(server.recv()["method"], "view/keymap");
    let entered = wait_for_gate_or_failure(&gate, &mut server, &view);
    assert!(
        matches!(entered, GateWait::Entered),
        "a healthy gated acquisition must enter the gate, not report a failure: {entered:?}"
    );
    assert_eq!(gate.invocation_count(), 1);

    write(
        &tmp.path().join("overlap-fresh.txt"),
        "fresh after refresh\n",
    );
    for id in 3..=6 {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/invoke",
            "params":{"viewId":view,"command":"common.refresh"}}));
        let refreshed = server.response_with_id(id);
        assert!(refreshed["error"].is_null(), "{refreshed}");
        assert_eq!(
            gate.invocation_count(),
            1,
            "refresh during held acquisition must not start Git"
        );
    }
    assert!(
        !gate.is_released(),
        "all refresh requests answered while Git stayed held"
    );

    gate.release();
    // Both the superseded acquisition and its single follow-up must complete
    // before the newer render is treated as final.
    wait_for_completion_barrier(&trace, &view, 2);
    let loading_version = loading["params"]["version"].as_u64().unwrap();
    let ready = server.wait_for_view_render_after(&view, loading_version);
    let lines: Vec<String> = serde_json::from_value(ready["params"]["text"].clone()).unwrap();
    // The follow-up's status is published as a closed render: the fresh file
    // is visible only through the untracked section's count.
    assert!(
        lines.iter().any(|line| line == "Untracked files (1)"),
        "{lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("Loading repository status")),
        "{lines:?}"
    );
    assert_eq!(
        gate.invocation_count(),
        2,
        "overlapping refreshes schedule one follow-up acquisition"
    );
    // Exactly one render follows loading: the follow-up's fresh content at the
    // next version. A superseded completion publishing late would add a second
    // render (older content at a newer version), and a duplicate would repeat
    // the version; the wire order exposes both.
    let received_before_late = server.notifications.len();
    server.drain_notifications();
    let published: Vec<(u64, String)> = server.notifications[..received_before_late]
        .iter()
        .chain(std::iter::once(&ready))
        .chain(server.notifications[received_before_late..].iter())
        .filter(|message| {
            message["method"] == "view/render"
                && message["params"]["viewId"] == view
                && message["params"]["version"]
                    .as_u64()
                    .is_some_and(|version| version > loading_version)
        })
        .map(|message| {
            (
                message["params"]["version"].as_u64().unwrap(),
                message["params"]["text"].to_string(),
            )
        })
        .collect();
    assert!(
        published.len() <= 2,
        "completion published a stale or duplicate render after loading: {published:?}"
    );
    assert_eq!(
        published[0].0,
        loading_version + 1,
        "the fresh render must directly follow the loading version: {published:?}"
    );
    assert!(
        !published[0].1.contains("overlap-fresh.txt"),
        "the fresh render must start closed: {published:?}"
    );
    // The fast-path fetch may then expand the same generation: at most one
    // further render, directly after the closed one, now showing the file row.
    if let Some((version, text)) = published.get(1) {
        assert_eq!(
            *version,
            loading_version + 2,
            "a fast-path expansion must directly follow the closed render: {published:?}"
        );
        assert!(
            text.contains("overlap-fresh.txt"),
            "a render after the closed one must be the expansion: {published:?}"
        );
    }
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn status_open_refresh_wait_reports_killed_git_child_as_acquisition_failure() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    // The fixture kills the first `status` invocation by signal before it can
    // log itself: the CI signature of run 45759764509 (docs/evidence/
    // native-functional-status-gate-macos-kill.md).
    let gate = GatedGit::arm_killing("status");
    let gate_env = gate.child_env();
    let env: Vec<(&str, &str)> = gate_env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let mut server = TestServer::spawn(&env);
    server.initialize_rich(json!({}));
    let mut open = open_view_message(2, &repo_param(tmp.path()));
    open["params"]["traceId"] = json!(10);
    server.send(open);
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"]
        .as_str()
        .expect("status view id")
        .to_string();
    let loading = server.recv();
    assert_eq!(
        loading["params"]["text"],
        json!(["Head: main", "Loading repository status…"])
    );
    assert_eq!(server.recv()["method"], "view/keymap");

    let started = Instant::now();
    let (text, mut stderr) = match wait_for_gate_or_failure(&gate, &mut server, &view) {
        GateWait::Entered => panic!(
            "a killed Git child must surface as an acquisition failure; server stderr:\n{}",
            server.stderr_text()
        ),
        GateWait::AcquisitionFailed { text, stderr } => (text, stderr),
    };
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(30),
        "the failure must resolve promptly, not at the {GATE_ENTER_TIMEOUT:?} bound: {elapsed:?}"
    );
    assert!(
        text.iter()
            .any(|line| line.contains("Unable to load repository status")),
        "{text:?}"
    );
    assert!(
        !text
            .iter()
            .any(|line| line.contains("Loading repository status")),
        "{text:?}"
    );

    // The reproduced signature: the killed child never entered the gate, never
    // reached its invocation-log echo, and only the below-recommendation probe
    // that follows a reaped failed child ran.
    assert!(
        !gate.has_entered(),
        "the killed child can never enter the gate"
    );
    assert_eq!(gate.invocation_count(), 0);
    let log = gate.invocation_log();
    assert!(
        log.lines().any(|line| line.trim() == "--version"),
        "the failure probe must run after the reaped child: {log:?}"
    );
    assert!(
        !log.lines().any(|line| line.trim() == "status"),
        "the killed child died before its log echo: {log:?}"
    );
    // Three-way evidence: the spawn line proves the gated script ran, so this
    // is not an exec-time kill, while its subcommand echo stays absent. Read
    // the file-backed witness before touching stderr: `drain_stderr` can drop
    // the front of the captured stream, but never this log.
    let spawn = log
        .lines()
        .find(|line| line.trim_start().starts_with("spawned pid="))
        .unwrap_or_else(|| panic!("the killed child must record its spawn line: {log:?}"));
    let spawn_fields: Vec<&str> = spawn.split_whitespace().collect();
    assert_eq!(
        spawn_fields.len(),
        3,
        "one complete spawn line, no torn suffix: {spawn:?} in {log:?}"
    );
    assert_eq!(spawn_fields[0], "spawned", "{spawn:?}");
    for field in &spawn_fields[1..] {
        assert!(
            field
                .split_once('=')
                .is_some_and(|(_, value)| value.parse::<u64>().is_ok_and(|n| n > 0)),
            "spawn line field is not a positive number: {field:?} in {spawn:?}"
        );
    }
    // The parent reaped a signal death and logged it at warn, so the trace
    // survives the default `warn` filter into this test's captured stderr.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !stderr.contains("child reap") && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
        stderr = server.stderr_text();
    }
    assert!(
        stderr.contains("child reap") && stderr.contains("signal=9"),
        "the signal-death reap trace must be carried in the evidence:\n{stderr}"
    );
    // The parent-side spawn warn is the only witness when a kill lands at or
    // before exec, so the evidence must carry it, pid-bearing and ordered
    // before the reap that proves the same child died by signal.
    let lines: Vec<&str> = stderr.lines().collect();
    let spawn = lines
        .iter()
        .position(|line| line.contains("child spawn") && line.contains("status"))
        .unwrap_or_else(|| {
            panic!("the gated status child's parent-side spawn trace must be carried:\n{stderr}")
        });
    let spawn_line = lines[spawn];
    let spawn_pid = spawn_line
        .split_once("pid=")
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .and_then(|value| value.parse::<u32>().ok());
    assert!(
        spawn_pid.is_some(),
        "the spawn trace must carry the child pid: {spawn_line}"
    );
    let spawn_pid = spawn_pid.expect("the spawn trace asserted above");
    let reap = lines
        .iter()
        .position(|line| {
            line.contains("child reap")
                && line.contains(&format!("pid={spawn_pid}"))
                && line.contains("signal=9")
        })
        .unwrap_or_else(|| {
            panic!("the spawned child's signal-death reap trace must be carried:\n{stderr}")
        });
    assert!(
        spawn < reap,
        "the spawn witness must precede the reap witness:\n{stderr}"
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn status_open_failure_is_terminal_and_loading_actions_are_safe() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let gate = GatedGit::arm("status");
    let trace_dir = tempfile::tempdir().unwrap();
    let trace = trace_dir.path().join("status-text-error-completion");
    let trace_env = trace.to_string_lossy().into_owned();
    let gate_env = gate.child_env();
    let mut env: Vec<(&str, &str)> = gate_env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    env.push(("MAGIT_LSP_TRACE", trace_env.as_str()));
    let mut server = TestServer::spawn(&env);
    server.initialize_rich(json!({}));
    let mut open = open_view_message(2, &repo_param(tmp.path()));
    open["params"]["traceId"] = json!(8);
    server.send(open);
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"].as_str().expect("status view id");
    let loading = server.recv();
    assert_eq!(
        loading["params"]["text"],
        json!(["Head: main", "Loading repository status…"])
    );
    assert_eq!(server.recv()["method"], "view/keymap");
    gate.wait_entered();

    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke",
        "params":{"viewId":view,"command":"status.stage"}}));
    let refused = server.response_with_id(3);
    assert!(
        refused["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "message"),
        "{refused}"
    );
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke",
        "params":{"viewId":view,"command":"common.toggle-text-mode"}}));
    let menu = server.response_with_id(4);
    assert!(menu["error"].is_null(), "{menu}");
    assert_eq!(menu["result"]["effects"], json!([]), "{menu}");
    let mode_render = server.recv();
    assert_eq!(mode_render["method"], "view/render", "{mode_render}");
    assert_eq!(mode_render["params"]["mode"], "text", "{mode_render}");
    assert!(
        !gate.is_released(),
        "safe menu affordance responds while status Git is held"
    );

    let git_dir = tmp.path().join(".git");
    let saved_git = tmp.path().join(".git-held");
    std::fs::rename(&git_dir, &saved_git).unwrap();
    gate.release();
    // The failure is generation-validated and stored, but text mode owns the
    // buffer: it must not render or advance the displayed version yet. Wait for
    // the actual completion before asserting the absence of output.
    wait_for_completion_barrier(&trace, view, 1);
    // The resync round-trip also drains any completion notification, so the
    // assertion below sees every message the completion could have sent.
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/resync",
        "params":{"viewId":view}}));
    let held = server.response_with_id(5);
    assert_eq!(held["result"]["version"], loading["params"]["version"]);
    assert_eq!(held["result"]["text"], loading["params"]["text"]);
    assert!(
        !server.notifications.iter().any(
            |message| message["method"] == "view/render" && message["params"]["viewId"] == view
        ),
        "held error rendered into text mode: {:?}",
        server.notifications
    );

    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke",
        "params":{"viewId":view,"command":"common.toggle-text-mode"}}));
    let exited = server.response_with_id(6);
    assert!(exited["error"].is_null(), "{exited}");
    let failed = server.recv();
    assert_eq!(failed["method"], "view/render", "{failed}");
    assert_eq!(failed["params"]["mode"], "interactive", "{failed}");
    assert_eq!(
        failed["params"]["version"],
        loading["params"]["version"].as_u64().unwrap() + 1
    );
    assert!(failed["params"].get("baseVersion").is_none(), "{failed}");
    assert!(failed["params"].get("edits").is_none(), "{failed}");
    std::fs::rename(&saved_git, &git_dir).unwrap();
    let lines: Vec<String> = serde_json::from_value(failed["params"]["text"].clone()).unwrap();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Unable to load repository status")),
        "{lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("Loading repository status")),
        "{lines:?}"
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
fn status_open_held_git(acknowledge: bool) {
    magit_lsp_test_support::cargo_server_binary!();
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
        json!(["Head: main", "Loading repository status…"])
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
        magit_lsp::diff_text::apply_edits(
            &["Head: main".into(), "Loading repository status…".into()],
            &edits,
        )
    } else {
        assert!(ready["params"].get("baseVersion").is_none(), "{ready}");
        assert!(ready["params"].get("edits").is_none(), "{ready}");
        serde_json::from_value(ready["params"]["text"].clone()).unwrap()
    };
    assert!(
        text.iter().any(|line| line == "Unstaged changes (1)"),
        "{text:?}"
    );
    assert!(
        text.iter().any(|line| line == "Untracked files (1)"),
        "{text:?}"
    );
    // The published completion is the closed render: headers and counts, no
    // file row and no hunk line.
    assert!(
        !text
            .iter()
            .any(|line| line.contains("status-open-fresh.txt")),
        "{text:?}"
    );
    assert!(!text.iter().any(|line| line.contains("f.txt")), "{text:?}");
    assert!(!text.iter().any(|line| line.contains("@@")), "{text:?}");
    assert!(
        !text.iter().any(|line| line.contains("+line3 CHANGED")),
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
    // A later visibility open still serves the on-demand hunk path.
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke",
        "params":{"viewId":view,"command":"common.show-all"}}));
    assert!(server.response_with_id(5)["error"].is_null());
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/resync",
        "params":{"viewId":view}}));
    let opened: Vec<String> =
        serde_json::from_value(server.response_with_id(6)["result"]["text"].clone()).unwrap();
    assert!(
        opened
            .iter()
            .any(|line| line.contains("status-open-fresh.txt")),
        "{opened:?}"
    );
    assert!(
        opened.iter().any(|line| line.contains("+line3 CHANGED")),
        "{opened:?}"
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn opening_multirow_rebase_todo_batches_object_resolution_once() {
    magit_lsp_test_support::cargo_server_binary!();
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
            .env("GIT_SEQUENCE_EDITOR", "sed -i.bak '1s/^pick /edit /'")
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
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("base", [("base", "base\n")])
        .commit("one", [("one", "1\n")])
        .commit("two", [("two", "2\n")])
        .commit("three", [("three", "3\n")])
        .build();
    let base = tmp.git(&["rev-parse", "HEAD~3"]).trim().to_owned();
    let mut server = TestServer::spawn(&[]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let (status, status_version) = open_view_versioned(&mut server, 2, &repo_param(tmp.path()));

    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        let version = if view == status { status_version } else { 1 };
        server.send(
            json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{
            "viewId":view,"version":version,"key":key,"editorMode":"normal",
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

// Managed commit drafts need the Unix helper/validated-fd boundary. On Windows
// `EditRegistry::create` is the platform stub at src/editor.rs:791-794
// ("managed commit drafts require Unix"), so the instant-autosquash draft scope
// fails to open and the server answers with one "Cannot open instant autosquash
// draft scope: {error}" showMessage while sending none of the frames the awaits
// below need; the test then stalls in the bounded receive instead of failing
// clearly. Real Windows draft support belongs to the deferred Windows phase;
// tests/plain_draft_windows.rs pins the refusal.
#[cfg(unix)]
#[test]
fn instant_fixup_selection_autosquashes_through_real_server_and_restores_caller() {
    magit_lsp_test_support::cargo_server_binary!();
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
    let (status, status_version) = open_view_versioned(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        let version = if view == status { status_version } else { 1 };
        server.send(
            json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{
            "viewId":view,"version":version,"key":key,"editorMode":"normal",
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

// Windows draft gate: `EditRegistry::create` is the stub at src/editor.rs:791-794
// ("managed commit drafts require Unix"), so the instant-autosquash draft scope
// cannot open and the awaited draft frames never arrive — the test stalls in the
// bounded receive instead of failing clearly. tests/plain_draft_windows.rs pins
// the refusal; real Windows draft support is the deferred Windows phase.
#[cfg(unix)]
#[test]
fn dispatched_instant_fixup_conflict_is_left_for_explicit_recovery() {
    magit_lsp_test_support::cargo_server_binary!();
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
        let version = if view == status { 2 } else { 1 };
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":version,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
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

// Windows draft gate: `EditRegistry::create` is the stub at src/editor.rs:791-794
// ("managed commit drafts require Unix"), so the instant-autosquash draft scope
// cannot open and the awaited draft frames never arrive — the test stalls in the
// bounded receive instead of failing clearly. tests/plain_draft_windows.rs pins
// the refusal; real Windows draft support is the deferred Windows phase.
#[cfg(unix)]
#[test]
fn dispatched_instant_fixup_conflict_can_be_resolved_and_continued() {
    magit_lsp_test_support::cargo_server_binary!();
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
    let (status, status_version) = open_view_versioned(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        let version = if view == status { status_version } else { 1 };
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":version,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
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

// Windows draft gate: `EditRegistry::create` is the stub at src/editor.rs:791-794
// ("managed commit drafts require Unix"), so the instant-autosquash draft scope
// cannot open and the awaited draft frames never arrive — the test stalls in the
// bounded receive instead of failing clearly. tests/plain_draft_windows.rs pins
// the refusal; real Windows draft support is the deferred Windows phase.
#[cfg(unix)]
#[test]
fn instant_squash_selection_edits_message_through_registered_server_callback() {
    magit_lsp_test_support::cargo_server_binary!();
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
    let (status, status_version) = open_view_versioned(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        let version = if view == status { status_version } else { 1 };
        server.send(
            json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{
            "viewId":view,"version":version,"key":key,"editorMode":"normal",
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

// Windows draft gate: `EditRegistry::create` is the stub at src/editor.rs:791-794
// ("managed commit drafts require Unix"), so the instant-autosquash draft scope
// cannot open and the awaited draft frames never arrive — the test stalls in the
// bounded receive instead of failing clearly. tests/plain_draft_windows.rs pins
// the refusal; real Windows draft support is the deferred Windows phase.
#[cfg(unix)]
#[test]
fn instant_squash_registered_editor_abort_retains_marker_and_releases_worker() {
    magit_lsp_test_support::cargo_server_binary!();
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
    let (status, status_version) = open_view_versioned(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        let version = if view == status { status_version } else { 1 };
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":version,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
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
    magit_lsp_test_support::cargo_server_binary!();
    for (key, label) in [("r", "reword"), ("s", "squash")] {
        let (tmp, _) = RepoFixture::new()
            .commit("base", [("base", "base\n")])
            .commit("one", [("one", "1\n")])
            .commit("two", [("two", "2\n")])
            .build();
        let base = tmp.git(&["rev-parse", "HEAD~2"]).trim().to_owned();
        let mut server = TestServer::spawn(&[]);
        assert!(server.initialize_rich(json!({}))["error"].is_null());
        let (status, status_version) = open_view_versioned(&mut server, 2, &repo_param(tmp.path()));
        let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
            let version = if view == status { status_version } else { 1 };
            server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":version,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
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
    magit_lsp_test_support::cargo_server_binary!();
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
    // The caller's status render after the pause is watcher-driven, so keep the
    // debounce short instead of the harness's default parked value.
    let mut server = TestServer::spawn(&[
        ("GIT_EDITOR", "true"),
        ("MAGIT_LSP_WATCH_DEBOUNCE_MS", "25"),
    ]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let (status, status_version) = open_view_versioned(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        let version = if view == status { status_version } else { 1 };
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":version,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
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
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("base", [("file", "base\n")])
        .commit("first", [("file", "first\n")])
        .commit("second", [("file", "second\n")])
        .build();
    let base = tmp.git(&["rev-parse", "HEAD~2"]).trim().to_owned();
    // The caller's status render after resolving is watcher-driven, so keep the
    // debounce short instead of the harness's default parked value.
    let mut server = TestServer::spawn(&[
        ("GIT_EDITOR", "true"),
        ("MAGIT_LSP_WATCH_DEBOUNCE_MS", "25"),
    ]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let (status, status_version) = open_view_versioned(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        let version = if view == status { status_version } else { 1 };
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":version,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
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
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("base", [("base", "base\n")])
        .commit("one", [("one", "1\n")])
        .commit("two", [("two", "2\n")])
        .build();
    let base = tmp.git(&["rev-parse", "HEAD~2"]).trim().to_owned();
    let mut server = TestServer::spawn(&[]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let (status, status_version) = open_view_versioned(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        let version = if view == status { status_version } else { 1 };
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":version,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
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
    open_view_versioned(server, id, repo).0
}

/// Open a status view and acknowledge its deferred status completion.
///
/// Returns the view id and the acknowledged render version. Opening is
/// asynchronous: the loading render is followed by the fresh status render, so
/// positional keys must send the version the client acknowledged, not the
/// loading version.
fn open_view_versioned(server: &mut TestServer, id: i64, repo: &Value) -> (String, u64) {
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
    let ready = server.wait_for_view_render_after(
        &view_id,
        render["params"]["version"]
            .as_u64()
            .expect("initial render version"),
    );
    let version = ready["params"]["version"]
        .as_u64()
        .expect("ready render version");
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view_id,"version":version}}));
    (view_id, version)
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

/// Expand every section of a freshly acquired status render and acknowledge
/// the result, returning the version tests must cite for positional reads.
/// The acquired render starts with its sections closed, so its outline has no
/// hunk to address until the client opens them.
fn show_all_and_ack(server: &mut TestServer, id: i64, view: &str, version: u64) -> u64 {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/invoke",
        "params":{"viewId":view,"command":"common.show-all","version":version}}));
    let opened = server.response_with_id(id);
    assert!(opened["error"].is_null(), "{opened}");
    let expanded = server.wait_for_view_render_after(view, version);
    let expanded_version = expanded["params"]["version"]
        .as_u64()
        .expect("expanded render version");
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
        "params":{"viewId":view,"version":expanded_version}}));
    expanded_version
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

#[test]
fn the_shared_selector_uses_a_supplied_server_verbatim_and_cargo_otherwise() {
    magit_lsp_test_support::cargo_server_binary!();
    match std::env::var_os(SERVER_BINARY_ENV) {
        Some(value) => assert_eq!(
            server_binary(),
            std::path::PathBuf::from(&value),
            "an explicit selection must be used verbatim, never resolved or rebuilt"
        ),
        None => assert_eq!(
            server_binary(),
            std::path::PathBuf::from(env!("CARGO_BIN_EXE_magit-lsp")),
            "without a selection the transport keeps using the Cargo-built server"
        ),
    }
}

/// Absolute path that does not exist on this platform, so the validator's
/// readable-file refusal is exercised rather than the absolute-path one.
#[cfg(windows)]
const MISSING_ABSOLUTE_SERVER: &str = r"C:\nonexistent\magit-lsp";
/// Absolute path that does not exist on this platform, so the validator's
/// readable-file refusal is exercised rather than the absolute-path one.
#[cfg(not(windows))]
const MISSING_ABSOLUTE_SERVER: &str = "/nonexistent/magit-lsp";

#[test]
fn an_invalid_explicit_server_selection_is_refused_without_a_cargo_fallback() {
    for (value, expected) in [
        ("", "is set but empty"),
        ("relative/magit-lsp", "must be an absolute path"),
        (MISSING_ABSOLUTE_SERVER, "does not name a readable file"),
    ] {
        let error = explicit_server_binary(std::ffi::OsStr::new(value))
            .expect_err("an invalid explicit server path must be refused");
        assert!(
            error.contains(expected),
            "refusal {error:?} does not mention {expected:?}"
        );
    }

    let temp = tempfile::TempDir::new().expect("selection tempdir");
    let directory = explicit_server_binary(temp.path().as_os_str())
        .expect_err("a directory is not a server executable");
    assert!(
        directory.contains("does not name a regular file"),
        "refusal {directory:?} does not mention a regular file"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let path = temp.path().join("magit-lsp");
        std::fs::write(&path, b"#!/bin/sh\n").expect("write placeholder server");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("make the placeholder non-executable");
        let error = explicit_server_binary(path.as_os_str())
            .expect_err("a non-executable file must be refused");
        assert!(
            error.contains("is not executable"),
            "refusal {error:?} does not mention executability"
        );
    }
}

/// Selects the re-executed child of [`every_spawn_variant_runs_the_supplied_server`].
#[cfg(unix)]
const SPAWN_CHILD_ENV: &str = "MAGIT_LSP_TEST_SPAWN_CHILD";
/// Marker the wrapper writes for each spawn variant that runs it.
#[cfg(unix)]
const SPAWN_MARKER_ENV: &str = "MAGIT_LSP_TEST_SPAWN_MARKER";

#[cfg(unix)]
#[test]
fn every_spawn_variant_runs_the_supplied_server() {
    magit_lsp_test_support::cargo_server_binary!();
    use std::os::unix::fs::PermissionsExt as _;

    if std::env::var_os(SPAWN_CHILD_ENV).is_none() {
        // The selector lives on the child's `Command`, so this parallel test
        // binary never rewrites its own environment.
        let temp = tempfile::TempDir::new().expect("wrapper tempdir");
        let wrapper = temp.path().join("magit-lsp");
        let marker = temp.path().join("spawned");
        let served = server_binary().display().to_string().replace('\'', r"'\''");
        std::fs::write(
            &wrapper,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$MAGIT_LSP_TEST_VARIANT\" >> \"${SPAWN_MARKER_ENV}\"\nexec '{served}' \"$@\"\n"
            ),
        )
        .expect("write server wrapper");
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755))
            .expect("make the wrapper executable");

        let output = Command::new(std::env::current_exe().expect("current test binary"))
            .args([
                "--exact",
                "every_spawn_variant_runs_the_supplied_server",
                "--test-threads=1",
            ])
            .env(SPAWN_CHILD_ENV, "1")
            .env(SERVER_BINARY_ENV, &wrapper)
            .env(SPAWN_MARKER_ENV, &marker)
            .output()
            .expect("re-exec the spawn child");
        assert!(
            output.status.success(),
            "spawn child failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let spawned = std::fs::read_to_string(&marker).expect("wrapper recorded its spawns");
        assert_eq!(
            spawned.lines().collect::<Vec<_>>(),
            ["spawn", "prepared", "umask"],
            "every spawn variant must execute the selected server"
        );
        return;
    }

    let selection = std::env::var_os(SERVER_BINARY_ENV).expect("the child is given a selection");
    assert_eq!(
        server_binary(),
        std::path::PathBuf::from(selection),
        "the child must spawn exactly the wrapper it was given"
    );
    let marker = std::env::var(SPAWN_MARKER_ENV).expect("the child is given a marker");
    let capabilities = || json!({"window": {"workDoneProgress": true}});

    let mut server = TestServer::spawn(&[
        ("MAGIT_LSP_TEST_VARIANT", "spawn"),
        (SPAWN_MARKER_ENV, marker.as_str()),
    ]);
    server.initialize(capabilities());

    let mut server = TestServer::spawn_prepared(
        &[
            ("MAGIT_LSP_TEST_VARIANT", "prepared"),
            (SPAWN_MARKER_ENV, marker.as_str()),
        ],
        "true",
    );
    server.initialize(capabilities());

    let mut server = TestServer::spawn_with_umask(
        &[
            ("MAGIT_LSP_TEST_VARIANT", "umask"),
            (SPAWN_MARKER_ENV, marker.as_str()),
        ],
        "077",
    );
    server.initialize(capabilities());
}
