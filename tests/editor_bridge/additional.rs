use super::*;

#[test]
fn scoped_real_helpers_confirm_sequential_files_and_reject_unsafe_callbacks() {
    use magit_lsp::editor::{registry, EDIT_SCOPE_ENV, SOCKET_ENV};
    use magit_lsp::protocol::{DraftFinishReason, Effect};
    use std::sync::{mpsc, Arc};

    let (tmp, _) = staged_repo();
    let owner = "scoped-sequential-test";
    let registry = registry();
    let scope = registry.scope(tmp.path().to_str().unwrap(), owner).unwrap();
    let (effects_tx, effects_rx) = mpsc::channel();
    let socket = magit_lsp::editor::start_listener(
        Arc::new(move |effects| effects_tx.send(effects).unwrap()),
        Arc::new(|_| None),
    )
    .expect("editor bridge listener");
    let message_path = tmp.path().join(".git/COMMIT_EDITMSG");
    let mut runs = Vec::new();

    for message in ["First scoped message\n", "Second scoped message\n"] {
        write(&message_path, "template\n");
        let mut helper = Command::new(env!("CARGO_BIN_EXE_magit-lsp"))
            .arg("--editor-helper")
            .arg(&message_path)
            .env(SOCKET_ENV, socket)
            .env(EDIT_SCOPE_ENV, scope.credential())
            .env_remove(magit_lsp::editor::EDIT_ID_ENV)
            .env_remove(magit_lsp::editor::HELPER_TOKEN_ENV)
            .spawn()
            .unwrap();
        let effects = effects_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let (run_id, uri, text) = effects
            .into_iter()
            .find_map(|effect| match effect {
                Effect::EditDocument {
                    run_id, uri, text, ..
                } => Some((run_id, uri, text)),
                _ => None,
            })
            .expect("ordinary real-file edit announcement");
        registry
            .apply(owner, &run_id, &uri, |draft, context| {
                draft.did_open(context, 1, text.as_bytes())?;
                draft.did_change(context, 2, message.as_bytes())?;
                assert_eq!(
                    draft.finish(context, 2, DraftFinishReason::Confirm)?,
                    magit_lsp::editor::Reply::Ok
                );
                Ok(())
            })
            .unwrap();
        assert!(helper.wait().unwrap().success());
        assert_eq!(read(&message_path), message);
        runs.push(run_id);
    }

    assert_ne!(runs[0], runs[1], "each admitted helper gets a fresh run");
    drop(scope);
    assert_scoped_helper_denials_and_cleanup(socket, &effects_rx);
    assert_real_scoped_squash_edits_and_cancel(socket, &effects_rx);
}

#[cfg(unix)]
fn squash_repo() -> (TempDir, String, String, String) {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    write(&dir.path().join("base"), "base\n");
    git(dir.path(), &["add", "base"]);
    git(dir.path(), &["commit", "-m", "base"]);
    let base = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_string();
    write(&dir.path().join("alpha"), "alpha\n");
    git(dir.path(), &["add", "alpha"]);
    git(dir.path(), &["commit", "-m", "alpha"]);
    let alpha = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_string();
    write(&dir.path().join("beta"), "beta\n");
    git(dir.path(), &["add", "beta"]);
    git(dir.path(), &["commit", "-m", "beta"]);
    let beta = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_string();
    write(&dir.path().join("beta-extra"), "beta-extra\n");
    git(dir.path(), &["add", "beta-extra"]);
    git(dir.path(), &["commit", "--squash", &beta, "--no-edit"]);
    write(&dir.path().join("tail"), "tail\n");
    git(dir.path(), &["add", "tail"]);
    git(dir.path(), &["commit", "-m", "tail"]);
    (dir, base, alpha, beta)
}

#[cfg(unix)]
fn assert_real_scoped_squash_edits_and_cancel(
    socket: &'static Path,
    effects_rx: &std::sync::mpsc::Receiver<Vec<magit_lsp::protocol::Effect>>,
) {
    use magit_lsp::editor::registry;
    use magit_lsp::instant_rebase::{
        InstantAutosquashKind, InstantRebaseAuthority, InstantRebaseOutcome,
    };
    use magit_lsp::protocol::{DraftFinishReason, Effect};
    use std::sync::Arc;

    let owner = "real-scoped-squash";
    let (dir, base, alpha, _) = squash_repo();
    write(&dir.path().join("instant"), "instant\n");
    git(dir.path(), &["add", "instant"]);
    let repo = magit_lsp::git::GitRepo::open(dir.path()).unwrap();
    let authority =
        InstantRebaseAuthority::prepare(&repo, &alpha, InstantAutosquashKind::Squash).unwrap();
    let operation = magit_lsp::git::RepositoryOperation::InstantRebase(authority);
    let scope = Arc::new(
        registry()
            .scope(dir.path().to_str().unwrap(), owner)
            .unwrap(),
    );
    let worker_scope = scope.clone();
    let repo_path = dir.path().to_string_lossy().into_owned();
    let worker = thread::spawn(move || {
        magit_lsp::editor::execute_operation_scoped(
            &repo_path,
            "",
            socket,
            &operation,
            Path::new(env!("CARGO_BIN_EXE_magit-lsp")),
            Some(&worker_scope),
        )
        .unwrap()
    });
    let mut runs = Vec::new();
    for _ in 0..2 {
        let effects = effects_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let (run_id, uri, text) = effects
            .into_iter()
            .find_map(|effect| match effect {
                Effect::EditDocument {
                    run_id, uri, text, ..
                } => Some((run_id, uri, text)),
                _ => None,
            })
            .expect("real squash message edit");
        let message = if text.contains("alpha") {
            "Final alpha message\n"
        } else if text.contains("beta") {
            "Final beta message\n"
        } else {
            panic!("unexpected squash message template: {text}")
        };
        registry()
            .apply(owner, &run_id, &uri, |draft, context| {
                draft.did_open(context, 1, text.as_bytes())?;
                draft.did_change(context, 2, message.as_bytes())?;
                draft.finish(context, 2, DraftFinishReason::Confirm)?;
                Ok(())
            })
            .unwrap();
        runs.push(run_id);
    }
    let marker = match worker.join().unwrap() {
        magit_lsp::git::RepositoryOperationOutcome::InstantRebase(
            InstantRebaseOutcome::Completed {
                marker,
                warning: None,
            },
        ) => marker,
        outcome => panic!("unexpected squash outcome: {outcome:?}"),
    };
    assert_ne!(
        runs[0], runs[1],
        "Git's two message editors receive fresh runs"
    );
    assert_eq!(
        git(
            dir.path(),
            &["log", "--format=%s", &format!("{base}..HEAD")]
        ),
        "tail\nFinal beta message\nFinal alpha message\n"
    );
    let alpha_commit = git(dir.path(), &["cat-file", "commit", "HEAD~2"]);
    let beta_commit = git(dir.path(), &["cat-file", "commit", "HEAD~1"]);
    assert_eq!(
        alpha_commit.split_once("\n\n").unwrap().1,
        "Final alpha message\n"
    );
    assert_eq!(
        beta_commit.split_once("\n\n").unwrap().1,
        "Final beta message\n"
    );
    for path in ["alpha", "beta", "beta-extra", "tail", "instant"] {
        assert_eq!(read(&dir.path().join(path)), format!("{path}\n"));
    }
    assert!(!git(dir.path(), &["rev-list", "--all"])
        .lines()
        .any(|oid| oid == marker));
    assert!(git(dir.path(), &["status", "--porcelain"]).is_empty());
    assert!(!dir.path().join(".git/rebase-merge").exists());
    drop(scope);

    let (cancel_dir, _, cancel_target, _) = squash_repo();
    write(&cancel_dir.path().join("instant"), "instant\n");
    git(cancel_dir.path(), &["add", "instant"]);
    let repo = magit_lsp::git::GitRepo::open(cancel_dir.path()).unwrap();
    let authority =
        InstantRebaseAuthority::prepare(&repo, &cancel_target, InstantAutosquashKind::Squash)
            .unwrap();
    let original_head = git(cancel_dir.path(), &["rev-parse", "HEAD"])
        .trim()
        .to_string();
    let operation = magit_lsp::git::RepositoryOperation::InstantRebase(authority);
    let cancel_scope = Arc::new(
        registry()
            .scope(cancel_dir.path().to_str().unwrap(), owner)
            .unwrap(),
    );
    let worker_scope = cancel_scope.clone();
    let repo_path = cancel_dir.path().to_string_lossy().into_owned();
    let worker = thread::spawn(move || {
        magit_lsp::editor::execute_operation_scoped(
            &repo_path,
            "",
            socket,
            &operation,
            Path::new(env!("CARGO_BIN_EXE_magit-lsp")),
            Some(&worker_scope),
        )
        .unwrap()
    });
    let effects = effects_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let run = effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::EditDocument { run_id, .. } => Some(run_id),
            _ => None,
        })
        .expect("post-marker squash edit");
    cancel_scope.revoke();
    let marker = match worker.join().unwrap() {
        magit_lsp::git::RepositoryOperationOutcome::InstantRebase(
            InstantRebaseOutcome::RebaseStopped { marker, phase, .. },
        ) => {
            assert_eq!(phase, "autosquash");
            marker
        }
        outcome => panic!("unexpected cancelled squash outcome: {outcome:?}"),
    };
    assert!(!registry().resolve(&run, magit_lsp::editor::Reply::Abort));
    assert_eq!(
        git(cancel_dir.path(), &["rev-parse", "refs/heads/main"]).trim(),
        marker
    );
    assert_eq!(
        git(cancel_dir.path(), &["rev-parse", &format!("{marker}^")]).trim(),
        original_head
    );
    assert_ne!(
        git(cancel_dir.path(), &["rev-parse", "HEAD"]).trim(),
        marker
    );
    assert!(cancel_dir.path().join(".git/rebase-merge").exists());
    assert_eq!(read(&cancel_dir.path().join("instant")), "instant\n");
    assert!(!git(cancel_dir.path(), &["diff", "--cached", "--name-only"]).is_empty());
}

#[cfg(unix)]
fn assert_scoped_helper_denials_and_cleanup(
    socket: &Path,
    effects_rx: &std::sync::mpsc::Receiver<Vec<magit_lsp::protocol::Effect>>,
) {
    use magit_lsp::editor::{registry, EDIT_SCOPE_ENV, SOCKET_ENV};
    use magit_lsp::protocol::{DraftFinishReason, Effect};

    let (tmp, _) = staged_repo();
    let owner = "scoped-denial-test";
    let registry = registry();
    let launch = |path: &Path, credential: &str| {
        Command::new(env!("CARGO_BIN_EXE_magit-lsp"))
            .arg("--editor-helper")
            .arg(path)
            .env(SOCKET_ENV, socket)
            .env(EDIT_SCOPE_ENV, credential)
            .env_remove(magit_lsp::editor::EDIT_ID_ENV)
            .env_remove(magit_lsp::editor::HELPER_TOKEN_ENV)
            .spawn()
            .unwrap()
    };
    let wait = |child: &mut std::process::Child| {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let _ = child.wait();
                panic!("editor helper did not exit within 10 seconds");
            }
            thread::sleep(Duration::from_millis(10));
        }
    };
    let message_path = tmp.path().join(".git/COMMIT_EDITMSG");
    write(&message_path, "template\n");
    let scope = registry.scope(tmp.path().to_str().unwrap(), owner).unwrap();
    let mut first = launch(&message_path, scope.credential());
    let edit = effects_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let (run, uri, text) = edit
        .into_iter()
        .find_map(|effect| match effect {
            Effect::EditDocument {
                run_id, uri, text, ..
            } => Some((run_id, uri, text)),
            _ => None,
        })
        .unwrap();
    let mut denied = launch(&message_path, scope.credential());
    let denied = wait(&mut denied);
    assert!(!denied.success(), "concurrent callback denied");
    assert!(effects_rx.try_recv().is_err(), "denial emits no delivery");
    assert_eq!(
        registry
            .pending_for(owner, tmp.path().to_str().unwrap())
            .len(),
        1
    );
    registry
        .apply(owner, &run, &uri, |draft, context| {
            draft.did_open(context, 1, text.as_bytes())?;
            draft.did_change(context, 2, b"confirmed original\n")?;
            draft.finish(context, 2, DraftFinishReason::Confirm)?;
            Ok(())
        })
        .unwrap();
    assert!(
        wait(&mut first).success(),
        "original callback remains confirmable"
    );

    write(&message_path, "invalid-path\n");
    let mut invalid = launch(&tmp.path().join("missing-editor-file"), scope.credential());
    assert!(!wait(&mut invalid).success());
    assert!(
        effects_rx.try_recv().is_err(),
        "invalid path emits no delivery"
    );

    write(&message_path, "template\n");
    let mut active = launch(&message_path, scope.credential());
    let edit = effects_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let active_run = edit
        .into_iter()
        .find_map(|effect| match effect {
            Effect::EditDocument { run_id, .. } => Some(run_id),
            _ => None,
        })
        .unwrap();
    assert!(!registry.resolve(&run, magit_lsp::editor::Reply::Abort));
    assert!(registry
        .apply(owner, &run, &uri, |draft, context| {
            draft.finish(context, 1, DraftFinishReason::Confirm)
        })
        .is_err());
    assert_eq!(
        registry
            .pending_for(owner, tmp.path().to_str().unwrap())
            .len(),
        1
    );
    let revoked_credential = scope.credential().to_string();
    drop(scope);
    assert!(
        !wait(&mut active).success(),
        "scope revocation aborts helper"
    );
    assert_eq!(
        registry
            .pending_for(owner, tmp.path().to_str().unwrap())
            .len(),
        0
    );
    assert!(!registry.resolve(&active_run, magit_lsp::editor::Reply::Abort));

    let mut late = launch(&message_path, &revoked_credential);
    assert!(
        !wait(&mut late).success(),
        "late callback after revoke is denied"
    );
    assert!(
        effects_rx.try_recv().is_err(),
        "late denial emits no delivery"
    );

    let next = registry.scope(tmp.path().to_str().unwrap(), owner).unwrap();
    let mut disconnected = launch(&message_path, next.credential());
    let edit = effects_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let disconnected_run = edit
        .into_iter()
        .find_map(|effect| match effect {
            Effect::EditDocument { run_id, .. } => Some(run_id),
            _ => None,
        })
        .unwrap();
    disconnected.kill().unwrap();
    let _ = wait(&mut disconnected);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !registry
        .pending_for(owner, tmp.path().to_str().unwrap())
        .is_empty()
    {
        assert!(
            Instant::now() < deadline,
            "disconnected callback cleanup timed out"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!registry.resolve(&disconnected_run, magit_lsp::editor::Reply::Abort));

    let mut recovered = launch(&message_path, next.credential());
    let edit = effects_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let recovered_run = edit
        .into_iter()
        .find_map(|effect| match effect {
            Effect::EditDocument { run_id, .. } => Some(run_id),
            _ => None,
        })
        .unwrap();
    assert!(registry.resolve(&recovered_run, magit_lsp::editor::Reply::Abort));
    assert!(!wait(&mut recovered).success());
    assert!(registry
        .pending_for(owner, tmp.path().to_str().unwrap())
        .is_empty());
    assert!(effects_rx.try_recv().is_err(), "cleanup leaves no delivery");
    drop(next);
}

#[test]
fn a_cancel_creates_no_commit() {
    let (tmp, _repo) = staged_repo();
    let before = tmp.git(&["status", "--porcelain"]);
    let repo = json!(tmp.path().to_string_lossy());
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    let response = server.initialize(rich_capabilities());
    assert!(
        response["result"]["capabilities"].is_object(),
        "initialize response: {response}"
    );

    let status = open_status(&mut server, 2, &repo);
    let _menu = invoke(&mut server, 3, &status, "open.commit", json!(null));

    let edit = start_commit(&mut server, &status);
    let edit_id = edit["runId"].as_str().expect("run id").to_string();
    sync_draft(&mut server, &edit, "Should not land\n");

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "view/invoke",
        "params": {
            "viewId": status,
            "command": "common.cancel",
            "args": {"runId": edit_id},
        },
    }));

    // The worker reports the aborted Git command asynchronously; it may be
    // queued before the cancel response, so collect both.
    let mut cancelled = None;
    let mut aborted = None;
    while cancelled.is_none() || aborted.is_none() {
        let message = server.recv();
        if message["id"] == json!(5) {
            cancelled = Some(message);
        } else if message["method"] == json!("view/effects") {
            aborted = Some(message);
        }
    }
    let cancelled = cancelled.expect("cancel response");
    assert!(
        cancelled["result"]["effects"]
            .as_array()
            .is_some_and(|effects| effects.is_empty()),
        "cancel reports nothing: {cancelled}"
    );
    let aborted = aborted.expect("the aborted commit effect");
    let effects = aborted["params"]["effects"].as_array().expect("effects");
    assert!(
        effects.iter().any(|effect| {
            effect["type"] == json!("message")
                && effect["level"] == json!("error")
                && effect["text"].as_str().is_some_and(|text| !text.is_empty())
        }),
        "an aborted commit reports an error message: {aborted}"
    );

    assert_eq!(
        tmp.git(&["log", "-1", "--format=%B"]).trim(),
        "init",
        "no new commit was created"
    );
    assert_eq!(
        tmp.git(&["status", "--porcelain"]),
        before,
        "the staged change survives a cancel"
    );

    server.shutdown_and_exit();
    let exit = server.finish(Duration::from_secs(20));
    assert!(exit.success(), "server exited: {exit:?}");
}

#[cfg(target_os = "linux")]
#[test]
fn terminal_during_editor_cancels_with_absent_false_or_unanswered_progress() {
    for work_done_progress in [None, Some(false), Some(true)] {
        let mut capabilities = rich_capabilities();
        match work_done_progress {
            Some(flag) => capabilities["window"]["workDoneProgress"] = json!(flag),
            None => {
                capabilities["window"]
                    .as_object_mut()
                    .expect("window capabilities")
                    .remove("workDoneProgress");
            }
        }
        for disconnect in [false, true] {
            let (tmp, _) = staged_repo();
            let head = tmp.git(&["rev-parse", "HEAD"]);
            let staged = tmp.git(&["ls-files", "--stage", "-z"]);
            let tree = tmp.git(&["write-tree"]);
            let worktree = read(&tmp.path().join("f.txt"));
            let mut server = TestServer::spawn(&[]);
            server.initialize(capabilities.clone());
            let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
            server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
            let enabled = capabilities["window"]["workDoneProgress"] == true;
            let mut create = None;
            let mut edit = None;
            while edit.is_none() || (enabled && create.is_none()) {
                let message = server.recv_raw();
                if message["method"] == "window/workDoneProgress/create" {
                    create = Some(message.clone());
                }
                if let Some(found) = edit_document(&message) {
                    edit = Some(found);
                }
                server.notifications.push(message);
            }
            // The real Git/editor worker is waiting, but no visible progress is
            // needed for ownership, intake, or terminal cancellation.
            server.send(
                json!({"jsonrpc":"2.0","id":5,"method":"view/resync","params":{"viewId":status}}),
            );
            assert_eq!(server.response_with_id(5)["result"]["viewId"], status);
            assert!(!server
                .notifications
                .iter()
                .any(|m| m["method"] == "$/progress"));
            assert_eq!(
                server
                    .notifications
                    .iter()
                    .filter(|m| m["method"] == "window/workDoneProgress/create")
                    .count(),
                usize::from(enabled)
            );
            if disconnect {
                server.close_stdin();
            } else {
                server.send(json!({"jsonrpc":"2.0","id":90,"method":"shutdown","params":null}));
                assert_eq!(server.response_with_id(90)["result"], Value::Null);
                if let Some(create) = create {
                    server.send(json!({"jsonrpc":"2.0","id":create["id"],"result":null}));
                    server.send(json!({"jsonrpc":"2.0","id":91,"method":"view/resync","params":{"viewId":status}}));
                    assert!(server.response_with_id(91)["error"].is_object());
                }
                assert!(
                    !server
                        .notifications
                        .iter()
                        .any(|m| m["method"] == "$/progress"),
                    "terminal emitted orphan progress"
                );
                server.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));
            }
            let (exit, remaining) = server.finish_with_messages(Duration::from_secs(10));
            assert!(exit.success());
            assert!(
                !remaining.iter().any(|m| m["method"] == "$/progress"),
                "orphan progress through EOF: {remaining:?}"
            );
            assert!(
                !remaining
                    .iter()
                    .any(|m| m["method"] == "window/workDoneProgress/create"),
                "late create through EOF: {remaining:?}"
            );
            assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
            assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), staged);
            assert_eq!(tmp.git(&["write-tree"]), tree);
            assert_eq!(read(&tmp.path().join("f.txt")), worktree);
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn r_edit_2_progress_cancel_during_editor_releases_once() {
    let (tmp, _repo) = staged_repo();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let staged = tmp.git(&["ls-files", "--stage", "-z"]);
    let tree = tmp.git(&["write-tree"]);
    let worktree = read(&tmp.path().join("f.txt"));
    let repo = json!(tmp.path().to_string_lossy());
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    server.auto_respond_to_progress_create();
    server.initialize(rich_capabilities());
    let status = open_status(&mut server, 2, &repo);

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
    let mut token = None;
    let mut edit = None;
    while token.is_none() || edit.is_none() {
        let message = server.recv();
        if message["method"] == json!("$/progress")
            && message["params"]["value"]["kind"] == json!("begin")
        {
            token = Some(message["params"]["token"].clone());
        }
        if let Some(found) = edit_document(&message) {
            edit = Some(found);
        }
    }
    let edit = edit.unwrap();
    let edit_id = edit["runId"].as_str().unwrap().to_string();
    sync_draft(&mut server, &edit, "Must not commit\n");
    let socket = server.socket_path();
    let mut owned = wait_for_edit_processes(&edit_id, &socket);
    server.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token.unwrap()}}));

    let mut ends = 0;
    let mut failed = false;
    while !failed || ends == 0 {
        let message = server.recv();
        if message["method"] == json!("$/progress")
            && message["params"]["value"]["kind"] == json!("end")
        {
            ends += 1;
        }
        if message["method"] == json!("view/effects") {
            failed |= message["params"]["effects"]
                .as_array()
                .is_some_and(|effects| {
                    effects.iter().any(|e| {
                        e["type"] == json!("message")
                            && e["level"] == json!("error")
                            && e["text"].as_str().is_some_and(|s| !s.is_empty())
                    })
                });
        }
    }
    owned.finished();
    assert_eq!(ends, 1, "progress must end exactly once");
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{"viewId":status,"command":"common.confirm","args":{"runId":edit_id}}}));
    let late = server.response_with_id(5);
    assert!(
        late["error"].is_object()
            || late["result"]["effects"]
                .as_array()
                .is_some_and(|e| !e.is_empty()),
        "late confirm must be rejected: {late}"
    );
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), staged);
    assert_eq!(tmp.git(&["write-tree"]), tree);
    assert_eq!(read(&tmp.path().join("f.txt")), worktree);

    let edit = start_commit(&mut server, &status);
    sync_draft(&mut server, &edit, "Follow-up commit\n");
    let confirmed = finish_draft(&mut server, 6, &edit);
    assert_eq!(confirmed["result"]["type"], "confirmed", "{confirmed}");
    wait_for_commit(&tmp, "Follow-up commit");
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(target_os = "linux")]
#[test]
fn r_edit_2_progress_cancel_during_commit_msg_hook() {
    let (tmp, _repo) = staged_repo();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let staged = tmp.git(&["ls-files", "--stage", "-z"]);
    let tree = tmp.git(&["write-tree"]);
    let worktree = read(&tmp.path().join("f.txt"));
    let ready = tmp.path().join("hook.ready");
    let child_pid = tmp.path().join("hook-child.pid");
    let hook = tmp.path().join(".git/hooks/commit-msg");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\ntrap '' TERM\n(sleep 1000) &\necho $! > '{}'\necho ready > '{}'\nwait\n",
            child_pid.display(),
            ready.display()
        ),
    )
    .expect("write commit-msg hook");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let repo = json!(tmp.path().to_string_lossy());
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    server.auto_respond_to_progress_create();
    server.initialize(rich_capabilities());
    let status = open_status(&mut server, 2, &repo);
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
    let mut token = None;
    let mut edit = None;
    while token.is_none() || edit.is_none() {
        let message = server.recv();
        if message["method"] == json!("$/progress")
            && message["params"]["value"]["kind"] == json!("begin")
        {
            token = Some(message["params"]["token"].clone());
        }
        if let Some(found) = edit_document(&message) {
            edit = Some(found);
        }
    }
    let edit = edit.unwrap();
    let edit_id = edit["runId"].as_str().unwrap().to_string();
    sync_draft(&mut server, &edit, "Must not commit\n");
    let confirmed = finish_draft(&mut server, 5, &edit);
    assert_eq!(confirmed["result"]["type"], "confirmed", "{confirmed}");
    let socket = server.socket_path();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        assert!(
            Instant::now() < deadline,
            "commit-msg hook did not become ready"
        );
        thread::sleep(Duration::from_millis(20));
    }
    let processes = edit_processes(&edit_id, &socket).expect("/proc inspection");
    assert!(
        !processes.is_empty(),
        "commit processes carry edit identity"
    );
    let mut owned = OwnedEdit {
        edit_id: edit_id.clone(),
        socket: socket.clone(),
        pids: processes.into_iter().map(|p| p.pid).collect(),
    };
    let child: u32 = read(&child_pid).trim().parse().expect("hook child pid");
    let token = token.unwrap();
    let cancelled_at = Instant::now();
    server.send(
        json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}),
    );
    server.recv_timeout = Duration::from_millis(500);
    loop {
        let message = server.recv();
        assert_ne!(
            message["params"]["value"]["kind"], "end",
            "acknowledgement must precede terminal cleanup"
        );
        if message["method"] == "$/progress"
            && message["params"]["token"] == token
            && message["params"]["value"]["kind"] == "report"
        {
            assert_eq!(message["params"]["value"]["message"], "Cancelling…");
            assert_eq!(message["params"]["value"]["cancellable"], false);
            break;
        }
    }
    assert!(
        cancelled_at.elapsed() < Duration::from_millis(500),
        "cancellation intent was not promptly acknowledged"
    );
    // Duplicate/foreign requests must not restart cleanup or release the repo.
    server.send(
        json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}),
    );
    server.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":"not-a-run"}}));
    server.send(json!({"jsonrpc":"2.0","id":55,"method":"view/invoke","params":{"viewId":status,"command":"status.stage-all"}}));
    let refused = server.response_with_id(55);
    assert!(
        refused["result"]["effects"][0]["text"]
            .as_str()
            .is_some_and(|s| s.contains("Cancelling")),
        "mutation overlapped cleanup: {refused}"
    );
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(1),
        "refusal waited for cleanup"
    );
    server.recv_timeout = Duration::from_secs(20);
    let mut ends = 0;
    let mut failed = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !failed || ends == 0 {
        assert!(
            Instant::now() < deadline,
            "cancellation did not finish promptly"
        );
        let message = server.recv();
        if message["method"] == "$/progress" && message["params"]["token"] == token {
            assert_ne!(
                message["params"]["value"]["kind"], "report",
                "duplicate cancellation was acknowledged twice"
            );
            if message["params"]["value"]["kind"] == "end" {
                ends += 1;
            }
        }
        if message["method"] == json!("view/effects") {
            failed |= message["params"]["effects"]
                .as_array()
                .is_some_and(|effects| {
                    effects.iter().any(|e| {
                        e["type"] == json!("message")
                            && e["level"] == json!("error")
                            && e["text"].as_str().is_some_and(|s| !s.is_empty())
                    })
                });
        }
    }
    owned.finished();
    assert_eq!(ends, 1, "progress ends exactly once");
    assert!(
        cancelled_at.elapsed() >= Duration::from_secs(2),
        "resistant hook lost its full TERM grace"
    );
    server.notifications.clear();
    server.send(
        json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}),
    );
    server.send(json!({"jsonrpc":"2.0","id":56,"method":"view/resync","params":{"viewId":status}}));
    assert!(server.response_with_id(56).get("error").is_none());
    server.drain_notifications();
    assert!(
        !server
            .notifications
            .iter()
            .any(|m| m["method"] == "$/progress" && m["params"]["token"] == token),
        "late cancellation resurrected an ended token"
    );
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), staged);
    assert_eq!(tmp.git(&["write-tree"]), tree);
    assert_eq!(read(&tmp.path().join("f.txt")), worktree);
    let deadline = Instant::now() + Duration::from_secs(5);
    while std::fs::read_to_string(format!("/proc/{child}/stat"))
        .ok()
        .and_then(|stat| {
            stat.rsplit_once(')')
                .map(|(_, tail)| !matches!(tail.trim_start().chars().next(), Some('Z' | 'X')))
        })
        .unwrap_or(false)
    {
        assert!(
            Instant::now() < deadline,
            "hook descendant survived cancellation"
        );
        thread::sleep(Duration::from_millis(20));
    }
    std::fs::remove_file(&hook).expect("remove blocking commit-msg hook");
    let edit = start_commit(&mut server, &status);
    sync_draft(&mut server, &edit, "Follow-up commit\n");
    let confirmed = finish_draft(&mut server, 6, &edit);
    assert_eq!(confirmed["result"]["type"], "confirmed", "{confirmed}");
    wait_for_commit(&tmp, "Follow-up commit");
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

/// An early cancellation survives repository discovery and targets the actual
/// mutation, even when it arrives before the worker has spawned any child.
#[cfg(unix)]
#[test]
fn early_commit_cancellation_does_not_kill_repository_discovery() {
    let (tmp, _repo) = staged_repo();
    let before = tmp.git(&["rev-parse", "HEAD"]);
    let cancel = magit_lsp::runner::CancelHandle::new();
    let run = magit_lsp::editor::registry()
        .create(
            tmp.path().to_str().unwrap(),
            "early-test",
            magit_lsp::editor::draft::DraftKind::CommitMessage,
        )
        .unwrap();
    let registry = magit_lsp::editor::registry();
    let pending = registry.pending_for("early-test", tmp.path().to_str().unwrap());
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].run_id, run);
    assert_eq!(
        registry.run_for_uri("early-test", &pending[0].uri).unwrap(),
        run
    );
    assert!(registry
        .pending_for("other-owner", tmp.path().to_str().unwrap())
        .is_empty());
    assert!(registry
        .pending_for("early-test", "/different/repository")
        .is_empty());
    cancel.cancel();
    let error = magit_lsp::runner::with_cancel(Some(cancel), || {
        magit_lsp::editor::commit(
            tmp.path().to_str().unwrap(),
            &run,
            &tmp.path().join("unused.sock"),
            &magit_lsp::git::CommitOperation::Create(Default::default()),
        )
    })
    .expect_err("the pre-cancelled commit must fail");
    registry.remove(&run);
    assert!(registry
        .pending_for("early-test", tmp.path().to_str().unwrap())
        .is_empty());
    assert_eq!(
        error.git.expect("a structured Git failure").subcommand,
        "commit"
    );
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), before);
}

#[cfg(target_os = "linux")]
#[test]
fn r_edit_2_recorded_commit_wins_finalizing_and_completed_cancellation() {
    use std::os::unix::fs::PermissionsExt;

    for finalizing in [true, false] {
        let (tmp, _) = staged_repo();
        let head = tmp.git(&["rev-parse", "HEAD"]);
        let tree = tmp.git(&["write-tree"]);
        let worktree = read(&tmp.path().join("f.txt"));
        let ready = tmp.path().join(".git/post-ready");
        let hook = tmp.path().join(".git/hooks/post-commit");
        if finalizing {
            write(
                &hook,
                &format!(
                    "#!/bin/sh\ntouch '{}'\nwhile :; do sleep 0.02; done\n",
                    ready.display()
                ),
            );
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut server = TestServer::spawn(&[]);
        server.auto_respond_to_progress_create();
        server.initialize(rich_capabilities());
        let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
        server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
        let mut token = None;
        let mut edit = None;
        while token.is_none() || edit.is_none() {
            let message = server.recv();
            if message["method"] == "$/progress" && message["params"]["value"]["kind"] == "begin" {
                token = Some(message["params"]["token"].clone());
            }
            if let Some(found) = edit_document(&message) {
                edit = Some(found);
            }
        }
        let token = token.unwrap();
        let edit = edit.unwrap();
        sync_draft(&mut server, &edit, "Recorded before cancel\n");
        let confirmed = finish_draft(&mut server, 5, &edit);
        assert!(confirmed.get("error").is_none(), "{confirmed}");
        let mut observed = std::mem::take(&mut server.notifications);
        if finalizing {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !ready.exists() {
                assert!(Instant::now() < deadline, "post-commit did not start");
                thread::sleep(Duration::from_millis(10));
            }
            assert_ne!(tmp.git(&["rev-parse", "HEAD"]), head);
            server.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}));
        }
        while !observed.iter().any(|m| {
            m["method"] == "$/progress"
                && m["params"]["token"] == token
                && m["params"]["value"]["kind"] == "end"
        }) {
            observed.push(server.recv());
        }
        // End is terminal: repeated confirms and cancellation cannot resurrect
        // the edit or its progress, nor overwrite the successful outcome.
        server.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}));
        let late = command_confirm(&mut server, 6, &edit, 2, "synchronized");
        assert!(
            late["error"].is_object(),
            "ended edit accepted a duplicate finish: {late}"
        );
        server.send(
            json!({"jsonrpc":"2.0","id":7,"method":"view/resync","params":{"viewId":status}}),
        );
        assert!(server.response_with_id(7).get("error").is_none());
        observed.extend(std::mem::take(&mut server.notifications));
        observed.extend(server.drain_notifications());
        let progress: Vec<_> = observed
            .iter()
            .filter(|m| m["method"] == "$/progress" && m["params"]["token"] == token)
            .collect();
        assert_eq!(
            progress
                .iter()
                .filter(|m| m["params"]["value"]["kind"] == "end")
                .count(),
            1
        );
        assert_eq!(
            progress
                .iter()
                .filter(|m| m["params"]["value"]["kind"] == "report")
                .count(),
            usize::from(finalizing)
        );
        let effects: Vec<_> = observed
            .iter()
            .filter(|m| m["method"] == "view/effects")
            .flat_map(|m| m["params"]["effects"].as_array().into_iter().flatten())
            .collect();
        assert_eq!(
            effects
                .iter()
                .filter(|e| e["text"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("Committed:")))
                .count(),
            1,
            "exactly one truthful completion: {effects:?}"
        );
        assert!(
            !effects.iter().any(|e| e["level"] == "error"),
            "recorded commit reported failure: {effects:?}"
        );
        assert_eq!(tmp.git(&["rev-parse", "HEAD^"]), head);
        assert_eq!(tmp.git(&["rev-parse", "HEAD^{tree}"]), tree);
        assert_eq!(tmp.git(&["write-tree"]), tree);
        assert!(tmp.git(&["diff", "--cached", "--binary"]).is_empty());
        assert_eq!(read(&tmp.path().join("f.txt")), worktree);
        if finalizing {
            std::fs::remove_file(hook).unwrap();
        }
        write(&tmp.path().join("f.txt"), "Follow-up\n");
        tmp.git(&["add", "f.txt"]);
        let next = start_commit(&mut server, &status);
        sync_draft(&mut server, &next, "Follow-up commit\n");
        assert!(finish_draft(&mut server, 8, &next).get("error").is_none());
        wait_for_commit(&tmp, "Follow-up commit");
        server.shutdown_and_exit();
        assert!(server.finish(Duration::from_secs(10)).success());
    }
}

/// The processes holding a pending edit: Git's commit and its editor helper.
///
/// Both carry the edit id the server injected into their environment, so they
/// are exactly the processes this test owns through its own server. Git may
/// also run the helper through `sh -c`, which execs and keeps the same pid.
struct OwnedEdit {
    edit_id: String,
    socket: PathBuf,
    pids: Vec<u32>,
}

impl OwnedEdit {
    /// Release the ownership once every process ended by itself.
    fn finished(&mut self) {
        self.pids.clear();
    }
}

impl Drop for OwnedEdit {
    fn drop(&mut self) {
        if self.pids.is_empty() {
            // Every process ended by itself before the assertions ran.
            return;
        }
        for pid in std::mem::take(&mut self.pids) {
            // Re-verify the identity before killing: a pid may have been
            // reused by an unrelated process since it was recorded.
            let environ = std::fs::read(format!("/proc/{pid}/environ"));
            if matches!(&environ, Ok(bytes) if owned_environ(bytes, &self.edit_id, &self.socket)) {
                kill_pid(pid);
            }
        }
        // A failed run must never leave a Git commit or a helper waiting on a
        // dead socket behind, so wait until every owned process is really gone.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match edit_processes(&self.edit_id, &self.socket) {
                Ok(processes) if processes.is_empty() => return,
                Ok(_) => {}
                Err(warning) => {
                    eprintln!("warning: {warning}");
                    return;
                }
            }
            if Instant::now() >= deadline {
                eprintln!(
                    "warning: owned processes for edit {:?} survived the cleanup guard",
                    self.edit_id
                );
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Kill one owned process; only this test's own Git and helper need it.
fn kill_pid(pid: u32) {
    let _ = Command::new("kill").arg(pid.to_string()).status();
}

/// The environments of the processes carrying the edit id and socket path.
///
/// The socket path is part of the match: edit ids restart at `e1` in every
/// server process, so an id alone cannot distinguish this test's Git and
/// helper processes from another server's. A missing or unreadable `/proc`
/// is a hard error, never a silent pass.
fn owned_environ(environ: &[u8], edit_id: &str, socket: &Path) -> bool {
    let id_marker = format!("MAGIT_LSP_EDIT_ID={edit_id}\0");
    let socket_marker = format!("MAGIT_LSP_SOCKET={}\0", socket.to_string_lossy());
    let (id_marker, socket_marker) = (id_marker.as_bytes(), socket_marker.as_bytes());
    let carries = |marker: &[u8]| environ.windows(marker.len()).any(|w| w == marker);
    carries(id_marker) && carries(socket_marker)
}

/// A live process carrying the markers: its pid and its command line.
struct OwnedProcess {
    pid: u32,
    command: String,
}

/// The processes whose environment carries the edit id and socket path.
fn edit_processes(edit_id: &str, socket: &Path) -> Result<Vec<OwnedProcess>, String> {
    let mut processes = Vec::new();
    let entries =
        std::fs::read_dir("/proc").map_err(|error| format!("cannot inspect /proc: {error}"))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(environ) = std::fs::read(entry.path().join("environ")) else {
            continue;
        };
        if owned_environ(&environ, edit_id, socket) {
            let command = std::fs::read_to_string(entry.path().join("cmdline"))
                .map(|command| command.replace('\0', " "))
                .unwrap_or_default();
            processes.push(OwnedProcess {
                pid: name.parse().expect("a numeric pid"),
                command,
            });
        }
    }
    Ok(processes)
}

/// Wait until both the Git commit and its editor helper are running.
fn wait_for_edit_processes(edit_id: &str, socket: &Path) -> OwnedEdit {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let processes = edit_processes(edit_id, socket).expect("/proc inspection");
        let has_git = processes.iter().any(|process| {
            process.command.contains(" commit ") || process.command.ends_with(" commit")
        });
        let has_helper = processes
            .iter()
            .any(|process| process.command.contains("--editor-helper"));
        if has_git && has_helper {
            return OwnedEdit {
                edit_id: edit_id.to_string(),
                socket: socket.to_path_buf(),
                pids: processes.into_iter().map(|p| p.pid).collect(),
            };
        }
        assert!(
            Instant::now() < deadline,
            "no Git commit was running for the edit {edit_id:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

/// The client leaving (stdin closed) must abort the pending edit so Git never
/// hangs: the server ends on its own, the Git commit and its editor helper end
/// by themselves, no commit was created, and the staged change survives (§3.10
/// step 7).
///
/// `close_stdin` is the same EOF every real client produces, so this observes
/// the natural shutdown rather than a killed process. The owned Git commit and
/// its helper are watched by pid, so a server exit alone cannot count as the
/// abort, and the assertion happens before any cleanup could fake a pass.
#[test]
fn a_client_disconnect_aborts_the_edit_so_git_never_hangs() {
    let (tmp, _repo) = staged_repo();
    let before = tmp.git(&["status", "--porcelain"]);
    let before_head = tmp.git(&["rev-parse", "HEAD"]);
    let repo = json!(tmp.path().to_string_lossy());
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    let response = server.initialize(rich_capabilities());
    let socket = server.socket_path();
    assert!(
        response["result"]["capabilities"].is_object(),
        "initialize response: {response}"
    );

    let status = open_status(&mut server, 2, &repo);
    let _menu = invoke(&mut server, 3, &status, "open.commit", json!(null));

    let edit = start_commit(&mut server, &status);
    let edit_id = edit["runId"].as_str().expect("run id").to_string();
    sync_draft(&mut server, &edit, "Never confirmed\n");
    write(&tmp.path().join(".git/COMMIT_EDITMSG"), "Never confirmed\n");

    let mut owned = wait_for_edit_processes(&edit_id, &socket);
    assert!(
        !owned.pids.is_empty(),
        "no Git commit was running for the edit {edit_id:?}"
    );

    // The client is gone: close its input and let the server end by itself.
    server.close_stdin();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let server_gone = server.try_wait().is_some();
        let git_gone = edit_processes(&edit_id, &socket)
            .expect("/proc inspection")
            .is_empty();
        if server_gone && git_gone {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the Git commit or its editor helper outlived the disconnect"
        );
        thread::sleep(Duration::from_millis(20));
    }
    // Every process ended by itself; the guard must not kill anything now.
    owned.finished();

    assert_eq!(
        tmp.git(&["log", "-1", "--format=%B"]).trim(),
        "init",
        "no new commit was created"
    );
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]),
        before_head,
        "HEAD did not move"
    );
    assert_eq!(
        tmp.git(&["status", "--porcelain"]),
        before,
        "the staged change survives the disconnect"
    );

    let exit = server.finish(Duration::from_secs(20));
    assert!(exit.success(), "server exited: {exit:?}");
}

/// The bridge socket path, from a server log file at `path`.
fn socket_path_in_log(path: &Path) -> PathBuf {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(log) = std::fs::read_to_string(path) {
            if let Some(socket) = log.lines().find_map(|line| {
                line.split_once("editor bridge listening socket=")
                    .map(|(_, socket)| socket.trim())
            }) {
                return PathBuf::from(socket);
            }
        }
        assert!(
            Instant::now() < deadline,
            "the server did not report a bridge socket in its log"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

/// Ask Git's question through the bridge and return the helper's output.
pub(super) fn answer_askpass(
    server: &mut TestServer,
    socket: &Path,
    question: &str,
    answer: &str,
) -> Output {
    let helper = Command::new(env!("CARGO_BIN_EXE_magit-lsp"))
        .arg("--askpass")
        .arg(question)
        .env("MAGIT_LSP_SOCKET", socket)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the askpass helper");

    let prompt = server.prompt();
    let params = &prompt["params"];
    assert_eq!(params["kind"], json!("text"), "the prompt kind");
    assert_eq!(params["secret"], json!(true), "the prompt is secret");
    assert!(
        params["title"].as_str() == Some(question),
        "the prompt asks Git's question"
    );
    let prompt_id = params["promptId"].as_str().expect("prompt id").to_string();
    // A text answer is the bare string on the wire.
    server.respond_prompt(90, &prompt_id, false, json!(answer));

    helper.wait_with_output().expect("the helper finished")
}

/// Assert the helper printed exactly the answer and nothing to stderr.
///
/// The diagnostics never include the captured streams: after a successful
/// round trip the answer is in stdout by design.
pub(super) fn assert_helper_kept_the_answer(output: &Output, answer: &str) {
    assert!(output.status.success(), "the askpass helper failed");
    assert!(
        String::from_utf8_lossy(&output.stdout).trim() == answer,
        "the askpass helper did not print the answered secret"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains(answer),
        "the askpass helper wrote the answer to stderr"
    );
}

/// Assert `log` proves the traced askpass path ran and omits the answer.
pub(super) fn assert_log_kept_the_answer(sink: &str, log: &str, answer: &str) {
    assert!(
        log.contains("editor bridge: askpass"),
        "the {sink} log is missing the askpass trace; logging was not active"
    );
    assert!(
        !log.contains(answer),
        "the {sink} log contains the askpass answer"
    );
}

/// Run the askpass helper against the running server's bridge.
///
/// The helper is what Git runs when it needs a password: it asks the server
/// and prints the secret on stdout. The socket comes from the server's
/// environment, so the helper finds the bridge the same way Git would.
#[test]
fn the_askpass_helper_returns_the_secret_to_whoever_asked_for_it() {
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    let response = server.initialize_rich(json!({}));
    assert!(
        response["result"]["capabilities"].is_object(),
        "the server did not initialize"
    );
    let socket = server.socket_path();

    let output = answer_askpass(&mut server, &socket, ASKPASS_QUESTION, ASKPASS_ANSWER);
    assert_helper_kept_the_answer(&output, ASKPASS_ANSWER);

    server.shutdown_and_exit();
    let exit = server.finish(Duration::from_secs(20));
    assert!(exit.success(), "server exited: {exit:?}");
}

/// The server's trace log on stderr must never receive the askpass answer.
#[test]
fn the_askpass_answer_is_never_logged_to_stderr() {
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    let response = server.initialize_rich(json!({}));
    assert!(
        response["result"]["capabilities"].is_object(),
        "the server did not initialize"
    );
    let socket = server.socket_path();

    let output = answer_askpass(&mut server, &socket, ASKPASS_QUESTION, ASKPASS_ANSWER);
    assert_helper_kept_the_answer(&output, ASKPASS_ANSWER);

    // Drain stderr fully: the trace that proves the round trip was logged may
    // still be in flight when the helper exits.
    server.shutdown_and_exit();
    let (exit, log) = server.finish_with_stderr(Duration::from_secs(20));
    assert!(exit.success(), "the server exited unsuccessfully");
    assert_log_kept_the_answer("stderr", &log, ASKPASS_ANSWER);
}

/// The same guard for trace logging redirected to `MAGIT_LSP_LOG_FILE`.
///
/// The socket comes from that file as well, so this exercises the file sink
/// end to end instead of relying on stderr.
#[test]
fn the_askpass_answer_is_never_logged_to_a_log_file() {
    let dir = TempDir::new().expect("temp dir for the log");
    let log_path = dir.path().join("magit-lsp.log");
    let log_path_arg = log_path.to_string_lossy().into_owned();
    let mut server = TestServer::spawn(&[(magit_lsp::logging::FILE_ENV, &log_path_arg)]);
    server.recv_timeout = Duration::from_secs(20);
    let response = server.initialize_rich(json!({}));
    assert!(
        response["result"]["capabilities"].is_object(),
        "the server did not initialize"
    );
    let socket = socket_path_in_log(&log_path);

    let output = answer_askpass(&mut server, &socket, ASKPASS_QUESTION, ASKPASS_ANSWER);
    assert_helper_kept_the_answer(&output, ASKPASS_ANSWER);

    server.shutdown_and_exit();
    let (exit, stderr) = server.finish_with_stderr(Duration::from_secs(20));
    assert!(exit.success(), "the server exited unsuccessfully");

    let log = std::fs::read_to_string(&log_path).expect("read the server log");
    assert_log_kept_the_answer("file", &log, ASKPASS_ANSWER);
    assert!(
        !stderr.contains(ASKPASS_ANSWER),
        "the server wrote the askpass answer to stderr"
    );
}

#[test]
fn a_cancelled_askpass_prompt_makes_the_helper_fail() {
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    let response = server.initialize_rich(json!({}));
    assert!(
        response["result"]["capabilities"].is_object(),
        "initialize response: {response}"
    );
    let socket = server.socket_path();

    let helper = Command::new(env!("CARGO_BIN_EXE_magit-lsp"))
        .arg("--askpass")
        .arg("Password:")
        .env("MAGIT_LSP_SOCKET", &socket)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the askpass helper");

    let prompt = server.prompt();
    let prompt_id = prompt["params"]["promptId"]
        .as_str()
        .expect("prompt id")
        .to_string();
    server.respond_prompt(91, &prompt_id, true, Value::Null);

    let output = helper.wait_with_output().expect("the helper finished");
    assert!(
        !output.status.success(),
        "a cancelled prompt must fail the helper: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "",
        "a cancelled prompt prints no secret"
    );

    server.shutdown_and_exit();
    let exit = server.finish(Duration::from_secs(20));
    assert!(exit.success(), "server exited: {exit:?}");
}
