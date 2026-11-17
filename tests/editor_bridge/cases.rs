use super::*;
use super::additional::{answer_askpass, assert_helper_kept_the_answer, assert_log_kept_the_answer};

#[test]
fn head_operations_real_git_tree_message_date_and_saved_close() {
    for command in ["commit.amend", "commit.extend", "commit.reword"] {
        for state in [
            "staged",
            "unstaged",
            "all",
            "clean",
            "cancel",
            "reset-staged",
            "reset-clean",
            "reset-unstaged",
        ] {
            let reset_author = state.starts_with("reset-");
            let state = state.strip_prefix("reset-").unwrap_or(state);
            if command == "commit.reword" && state == "all" {
                continue;
            }
            if command == "commit.extend" && state == "cancel" {
                continue;
            }
            let fixture = if reset_author {
                RepoFixture::new().commit("parent", [("parent.txt", "parent\n")])
            } else {
                RepoFixture::new()
            };
            let (tmp, repo) = fixture
                .commit("original message", [("f.txt", TEN_LINES)])
                .build();
            if reset_author {
                tmp.git(&[
                    "commit",
                    "--amend",
                    "--no-edit",
                    "--author=Original Author <original@example.com>",
                    "--date=1999-01-02T03:04:05+00:00",
                ]);
            }
            // Make date preservation observable independently of wall-clock speed.
            let output = Command::new("git")
                .args(["commit", "--amend", "--no-edit"])
                .current_dir(tmp.path())
                .env("GIT_COMMITTER_DATE", "2001-02-03T04:05:06+00:00")
                .output()
                .unwrap();
            assert!(output.status.success());
            let head = tmp.git(&["rev-parse", "HEAD"]);
            let head_tree = tmp.git(&["rev-parse", "HEAD^{tree}"]);
            let date = repo.head_committer_date().unwrap();
            let message = tmp.git(&["log", "-1", "--format=%B"]);
            let parents = tmp.git(&["show", "-s", "--format=%P", "HEAD"]);
            let author_date = tmp.git(&["show", "-s", "--format=%aI", "HEAD"]);
            if state != "clean" {
                write(&tmp.path().join("f.txt"), "staged content\n");
                if state == "staged" || state == "cancel" {
                    tmp.git(&["add", "f.txt"]);
                }
                write(&tmp.path().join("f.txt"), "unstaged content\n");
            }
            if state != "clean" {
                write(
                    &tmp.path().join("untracked.txt"),
                    "never implicitly added\n",
                );
            }
            let index = tmp.git(&["ls-files", "--stage", "-z"]);
            let index_tree = tmp.git(&["write-tree"]);
            let worktree = read(&tmp.path().join("f.txt"));
            let mut server = TestServer::spawn(if reset_author {
                &[
                    ("GIT_AUTHOR_NAME", "New Committer"),
                    ("GIT_AUTHOR_EMAIL", "new@example.com"),
                    ("GIT_AUTHOR_DATE", "2026-10-29T12:00:00+00:00"),
                ]
            } else {
                &[]
            });
            server.initialize(rich_capabilities());
            let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
            let args = json!({"switches": if reset_author { vec!["-R"] } else if state == "all" { vec!["-a"] }
                else if command == "commit.extend" && state == "clean" { vec!["-e"] } else { vec![] }, "options": {}});
            if command == "commit.extend" {
                server.send(
                    json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{
                        "viewId":status,"command":command,"args":args
                    }}),
                );
                if state == "unstaged" && !reset_author {
                    let prompt = server.prompt();
                    server.respond_prompt(
                        5,
                        prompt["params"]["promptId"].as_str().unwrap(),
                        false,
                        json!(true),
                    );
                }
            } else {
                let edit = start_head_operation(&mut server, &status, command, args);
                assert!(edit["text"]
                    .as_str()
                    .unwrap()
                    .starts_with("original message\n"));
                assert_eq!(
                    tmp.git(&["rev-parse", "HEAD"]),
                    head,
                    "editor alone must not commit"
                );
                assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index);
                if state == "cancel" {
                    let response = invoke(
                        &mut server,
                        5,
                        &status,
                        "common.cancel",
                        json!({"runId":edit["runId"]}),
                    );
                    assert!(response.get("error").is_none(), "{response}");
                } else {
                    // Amend deliberately closes the unchanged, saved Git message.
                    // Reword saves different bytes, but never commits the index tree.
                    if command == "commit.reword" {
                        write(
                            &tmp.path().join(".git/COMMIT_EDITMSG"),
                            "reworded message\n",
                        );
                    }
                    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                        "textDocument":{"uri":edit["uri"],"languageId":"gitcommit","version":1,"text":edit["text"]}
                    }}));
                    let response = command_confirm(&mut server, 5, &edit, 1, "saved-file");
                    assert_eq!(response["result"]["type"], "confirmed", "{response}");
                }
            }
            let outcome = loop {
                let frame = server.recv();
                assert!(
                    edit_document(&frame).is_none(),
                    "Extend must not open an editor"
                );
                if frame["method"] == "view/effects" {
                    if let Some(effect) = frame["params"]["effects"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|effect| {
                            effect["type"] == "message" && effect["text"] != "Committing…"
                        })
                    {
                        break effect.clone();
                    }
                }
            };
            if state == "cancel" {
                assert_eq!(outcome["level"], "error", "{outcome}");
                assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
                assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index);
            } else {
                assert!(
                    outcome["text"].as_str().unwrap().starts_with("Committed:"),
                    "{command}/{state}: {outcome}"
                );
                let expected_message = if command == "commit.reword" {
                    "reworded message\n\n"
                } else {
                    &message
                };
                assert_eq!(tmp.git(&["log", "-1", "--format=%B"]), expected_message);
                if command == "commit.reword" {
                    assert_ne!(tmp.git(&["rev-parse", "HEAD"]), head);
                    assert_eq!(tmp.git(&["rev-parse", "HEAD^{tree}"]), head_tree);
                    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index);
                } else if state == "all"
                    || (command == "commit.extend" && state == "unstaged" && !reset_author)
                {
                    assert_eq!(tmp.git(&["show", "HEAD:f.txt"]), worktree);
                    assert_ne!(tmp.git(&["rev-parse", "HEAD"]), head);
                    assert!(tmp.git(&["diff", "--cached"]).is_empty());
                    assert!(tmp.git(&["diff"]).is_empty());
                } else {
                    assert_eq!(tmp.git(&["rev-parse", "HEAD^{tree}"]), index_tree);
                    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index);
                    if state == "staged" {
                        assert_ne!(tmp.git(&["rev-parse", "HEAD"]), head);
                    }
                }
                if command == "commit.extend" && state == "clean" && !reset_author {
                    assert_eq!(
                        tmp.git(&["rev-parse", "HEAD"]),
                        head,
                        "identical preserved metadata may retain the OID"
                    );
                }
                if command != "commit.amend" {
                    assert_eq!(repo.head_committer_date().unwrap(), date);
                } else {
                    assert_ne!(repo.head_committer_date().unwrap(), date);
                }
                if reset_author {
                    assert_ne!(tmp.git(&["rev-parse", "HEAD"]), head);
                    assert_eq!(
                        tmp.git(&["show", "-s", "--format=%an <%ae>", "HEAD"]),
                        "New Committer <new@example.com>\n"
                    );
                    assert_ne!(
                        tmp.git(&["show", "-s", "--format=%aI", "HEAD"]),
                        author_date
                    );
                    assert!(tmp
                        .git(&["show", "-s", "--format=%aI", "HEAD"])
                        .starts_with("2026-10-29T12:00:00"));
                    assert_eq!(tmp.git(&["show", "-s", "--format=%P", "HEAD"]), parents);
                }
            }
            assert_eq!(
                tmp.git(&["rev-list", "--count", "HEAD"]),
                if reset_author { "2\n" } else { "1\n" }
            );
            assert_eq!(read(&tmp.path().join("f.txt")), worktree);
            if state != "clean" {
                assert_eq!(
                    read(&tmp.path().join("untracked.txt")),
                    "never implicitly added\n"
                );
            }
            assert!(!tmp.git(&["ls-files"]).contains("untracked.txt"));
            server.shutdown_and_exit();
            assert!(server.finish(Duration::from_secs(10)).success());
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn r_edit_2_eof_during_precommit_cancels_owned_hook() {
    use std::os::unix::fs::PermissionsExt;

    fn start_time(pid: i32) -> Option<u64> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        stat.rsplit_once(')')?
            .1
            .split_whitespace()
            .nth(19)?
            .parse()
            .ok()
    }
    fn live(pid: i32) -> bool {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|stat| {
                stat.rsplit_once(')')
                    .map(|(_, tail)| !matches!(tail.trim_start().chars().next(), Some('Z' | 'X')))
            })
            .unwrap_or(false)
    }

    let (tmp, _repo) = staged_repo();
    let marker = tmp.path().join(".git/hook-ready");
    let pidfile = tmp.path().join(".git/hook-pid");
    let hook = tmp.path().join(".git/hooks/pre-commit");
    write(&hook, &format!("#!/bin/sh\nsh -c 'trap \"\" TERM; echo $$ > \"$1\"; touch \"$2\"; exec sleep 60' sh '{}' '{}' </dev/null >/dev/null 2>&1 &\nwait\n", pidfile.display(), marker.display()));
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let staged = tmp.git(&["ls-files", "--stage", "-z"]);
    let tree = tmp.git(&["write-tree"]);
    let patch = tmp.git(&["diff", "--cached", "--binary"]);
    let worktree = read(&tmp.path().join("f.txt"));

    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    server.initialize(rich_capabilities());
    let repo = json!(tmp.path().to_string_lossy());
    let status = open_status(&mut server, 2, &repo);
    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !marker.exists() {
        assert!(
            Instant::now() < deadline,
            "pre-commit readiness marker missing"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let pid: i32 = read(&pidfile).trim().parse().unwrap();
    let identity = start_time(pid).expect("hook descendant identity");
    struct Cleanup(i32, u64);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if start_time_for_cleanup(self.0) == Some(self.1) && process_live_for_cleanup(self.0) {
                unsafe {
                    libc::kill(self.0, libc::SIGKILL);
                }
            }
        }
    }
    fn start_time_for_cleanup(pid: i32) -> Option<u64> {
        let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        s.rsplit_once(')')?
            .1
            .split_whitespace()
            .nth(19)?
            .parse()
            .ok()
    }
    fn process_live_for_cleanup(pid: i32) -> bool {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|s| {
                s.rsplit_once(')')
                    .map(|(_, t)| !matches!(t.trim_start().chars().next(), Some('Z' | 'X')))
            })
            .unwrap_or(false)
    }
    let _owned = Cleanup(pid, identity);
    server.close_stdin();
    let exit = server.finish(Duration::from_secs(8));
    assert!(exit.success(), "server exited: {exit:?}");
    let deadline = Instant::now() + Duration::from_secs(3);
    while live(pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!live(pid), "owned hook descendant survived EOF");
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), staged);
    assert_eq!(tmp.git(&["write-tree"]), tree);
    assert_eq!(tmp.git(&["diff", "--cached", "--binary"]), patch);
    assert_eq!(read(&tmp.path().join("f.txt")), worktree);
    std::fs::remove_file(hook).unwrap();
    tmp.git(&["commit", "-m", "follow-up"]);
}

#[test]
fn bridge_directory_is_private_with_permissive_umask() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().expect("private temp root");
    let root = tmp.path().to_string_lossy().into_owned();
    let mut server = TestServer::spawn_with_umask(&[("TMPDIR", &root)], "000");
    server.recv_timeout = Duration::from_secs(20);
    let initialized = server.initialize_rich(json!({}));
    assert!(initialized["result"]["capabilities"].is_object());
    let socket = server.socket_path();
    let dir = PathBuf::from(&root).join(format!("magit-lsp-{}", server.pid()));
    assert_eq!(socket, dir.join("editor.sock"));
    assert_eq!(
        std::fs::metadata(&dir)
            .expect("bridge dir")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let output = answer_askpass(&mut server, &socket, ASKPASS_QUESTION, ASKPASS_ANSWER);
    assert_helper_kept_the_answer(&output, ASKPASS_ANSWER);
    server.shutdown_and_exit();
    let (exit, log) = server.finish_with_stderr(Duration::from_secs(20));
    assert!(exit.success());
    assert_log_kept_the_answer("stderr", &log, ASKPASS_ANSWER);
}
#[test]
fn bridge_refuses_preexisting_directory_and_symlink() {
    use std::os::unix::fs::{symlink, FileTypeExt, MetadataExt};
    use std::os::unix::net::UnixListener;

    for attack in ["directory", "socket", "symlink"] {
        let tmp = TempDir::new().expect("private temp root");
        let root = tmp.path().to_string_lossy().into_owned();
        // The shell blocks before exec. Prepare a real foreign socket in Rust,
        // then release the same PID into the normal server startup path.
        let mut server = TestServer::spawn_prepared(&[("TMPDIR", &root)], "read -r ready");
        let path = tmp.path().join(format!("magit-lsp-{}", server.pid()));
        let target = if attack == "symlink" {
            tmp.path().join("foreign")
        } else {
            path.clone()
        };
        std::fs::create_dir(&target).expect("foreign directory");
        std::fs::write(target.join("keep"), b"sentinel").expect("foreign sentinel");
        let foreign_socket = if attack != "directory" {
            Some(UnixListener::bind(target.join("editor.sock")).expect("foreign socket"))
        } else {
            None
        };
        if attack == "symlink" {
            symlink(&target, &path).expect("foreign symlink");
        }
        let original_dir = std::fs::symlink_metadata(&path).expect("original directory entry");
        let original_socket = foreign_socket
            .as_ref()
            .map(|_| std::fs::metadata(target.join("editor.sock")).expect("original socket"));

        server.send_raw(b"ready\n");
        let initialized = server.initialize(json!({}));
        assert!(initialized["result"]["capabilities"].is_object());
        server.shutdown_and_exit();
        let (exit, log) = server.finish_with_stderr(Duration::from_secs(20));
        // Bridge refusal is nonfatal: the LSP server remains usable.
        assert!(exit.success());
        assert!(log.contains("editor bridge unavailable"), "{attack}: {log}");
        assert!(log.contains("cannot exclusively create the editor bridge directory"));
        assert!(!log.contains("editor bridge listening"));
        let preserved_dir = std::fs::symlink_metadata(&path).expect("foreign entry remains");
        assert_eq!(preserved_dir.ino(), original_dir.ino());
        assert_eq!(preserved_dir.file_type(), original_dir.file_type());
        if attack == "symlink" {
            assert_eq!(std::fs::read_link(&path).expect("symlink remains"), target);
        }
        assert_eq!(
            std::fs::read(target.join("keep")).expect("sentinel remains"),
            b"sentinel"
        );
        if let Some(original_socket) = original_socket {
            let preserved = std::fs::metadata(target.join("editor.sock")).expect("socket remains");
            assert!(preserved.file_type().is_socket());
            assert_eq!(preserved.ino(), original_socket.ino());
        } else {
            assert!(!target.join("editor.sock").exists());
        }
        drop(foreign_socket);
    }
}

#[test]
fn real_git_saved_file_empty_and_template_semantics() {
    for (name, saved, template, expected) in [
        ("empty", Some(""), false, None),
        ("comments", Some("# Only a comment\n# Another comment\n"), false, None),
        ("whitespace", Some(" \t\n\n \n"), false, None),
        ("untouched template", None, true, None),
        (
            "edited template",
            Some("  Edited template subject  \n\n# Git removes this comment\nBody with trailing space  \n\n\n"),
            true,
            Some("  Edited template subject\n\nBody with trailing space\n\n"),
        ),
    ] {
        let (tmp, _) = staged_repo();
        if template {
            write(&tmp.path().join(".git/test-template"), "Configured subject\n\n# Template comment\n");
            tmp.git(&["config", "commit.template", ".git/test-template"]);
        }
        let head = tmp.git(&["rev-parse", "HEAD"]);
        let index = tmp.git(&["ls-files", "--stage", "-z"]);
        let tree = tmp.git(&["write-tree"]);
        let patch = tmp.git(&["diff", "--cached", "--binary"]);
        let worktree = read(&tmp.path().join("f.txt"));
        let mut server = TestServer::spawn(&[]);
        server.initialize(rich_capabilities());
        let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
        let edit = start_commit(&mut server, &status);
        let initial = edit["text"].as_str().unwrap();
        assert!(initial.contains("# Please enter the commit message"), "{name}: {initial}");
        assert!(initial.contains("# Changes to be committed:"), "{name}: {initial}");
        assert!(initial.contains("f.txt"), "{name}: {initial}");
        if template {
            assert!(initial.starts_with("Configured subject\n\n# Template comment\n"), "{initial}");
        }
        let target = tmp.path().join(".git/COMMIT_EDITMSG");
        assert_eq!(read(&target), initial);
        // Native save is disk-only and cannot release the waiting Git editor.
        if let Some(text) = saved { write(&target, text); }
        assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head, "save alone: {name}");
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":edit["uri"],"languageId":"gitcommit","version":1,"text":initial}
        }}));
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
            "textDocument":{"uri":edit["uri"]}
        }}));
        let outcome = loop {
            let message = server.recv();
            if message["method"] == "view/effects" {
                if let Some(effect) = message["params"]["effects"].as_array().unwrap().iter().find(|effect|
                    effect["type"] == "message" && effect["text"] != "Committing…") {
                    break effect.clone();
                }
            }
        };
        assert_eq!(outcome["level"], "info", "{name}: {outcome}");
        if let Some(message) = expected {
            assert!(outcome["text"].as_str().unwrap().starts_with("Committed:"), "{outcome}");
            assert_eq!(tmp.git(&["log", "-1", "--format=%B"]), message, "{name}");
            assert_eq!(tmp.git(&["rev-parse", "HEAD^"]), head);
            assert_eq!(tmp.git(&["rev-parse", "HEAD^{tree}"]), tree);
            assert_eq!(tmp.git(&["status", "--porcelain"]), "");
        } else {
            assert_eq!(outcome["text"], "Commit aborted: empty message", "{name}: {outcome}");
            assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head, "{name}");
            assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index, "{name}");
            assert_eq!(tmp.git(&["write-tree"]), tree, "{name}");
            assert_eq!(tmp.git(&["diff", "--cached", "--binary"]), patch, "{name}");
        }
        assert_eq!(read(&tmp.path().join("f.txt")), worktree, "{name}");
        server.shutdown_and_exit();
        assert!(server.finish(Duration::from_secs(10)).success());
    }
}

#[test]
fn a_commit_is_created_from_the_typed_message() {
    let (tmp, _repo) = staged_repo();
    assert!(
        hunk_count(&cached_diff(tmp.path())) >= 1,
        "something staged"
    );
    let repo = json!(tmp.path().to_string_lossy());
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    let response = server.initialize_rich(json!({}));
    assert!(
        response["result"]["capabilities"].is_object(),
        "initialize response: {response}"
    );

    let status = open_status(&mut server, 2, &repo);
    // The commit menu is where a user starts, even though this test invokes
    // the action directly.
    let _menu = invoke(&mut server, 3, &status, "open.commit", json!(null));

    let edit = start_commit(&mut server, &status);
    assert!(edit["runId"].is_string());
    assert!(edit.get("editId").is_none());
    assert!(edit.get("path").is_none());
    assert!(edit["uri"].as_str().unwrap().starts_with("file:///"));
    assert_eq!(edit["kind"], json!("commit-message"), "{edit}");
    assert_eq!(edit["title"], json!("Commit message"), "{edit}");
    assert_eq!(edit["mode"], json!("realFile"), "{edit}");
    assert_eq!(edit["scope"], json!("commit-message"), "{edit}");
    sync_draft(&mut server, &edit, "Bridge commit message\n");
    let confirmed = finish_draft(&mut server, 5, &edit);
    assert_eq!(confirmed["result"]["type"], "confirmed", "{confirmed}");

    wait_for_commit(&tmp, "Bridge commit message");
    assert_eq!(
        tmp.git(&["status", "--porcelain"]),
        "",
        "the commit leaves the worktree and index clean"
    );

    server.shutdown_and_exit();
    let exit = server.finish(Duration::from_secs(20));
    assert!(exit.success(), "server exited: {exit:?}");
}

#[cfg(unix)]
#[test]
fn typed_commit_options_reach_real_git_through_the_editor_worker() {
    use std::os::unix::fs::PermissionsExt;
    let (tmp, _) = staged_repo();
    write(&tmp.path().join("f.txt"), "typed all content\n");
    let hook = tmp.path().join(".git/hooks/pre-commit");
    write(&hook, "#!/bin/sh\ntouch .git/unexpected-hook\nexit 1\n");
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    server.initialize(rich_capabilities());
    let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
    let author = "Typed ; $(not-a-shell) <typed@example.com>";
    let edit = start_commit_with_args(
        &mut server,
        &status,
        json!({
            "switches": ["-a", "-v", "-n", "-s"],
            "options": {"=A": author, "=D": "2001-02-03T04:05:06+00:00"}
        }),
    );
    let path = magit_lsp::git::GitRepo::open(tmp.path())
        .unwrap()
        .commit_message_path()
        .unwrap();
    let initial = read(&path);
    assert!(initial.contains("Signed-off-by:"), "{initial}");
    assert!(
        initial.contains("diff --git"),
        "verbose diff reaches the real editor file: {initial}"
    );
    // Preserve Git's generated signoff and verbose cleanup/scissors content.
    sync_draft(
        &mut server,
        &edit,
        &format!("Typed options message\n{initial}"),
    );
    let confirmed = finish_draft(&mut server, 5, &edit);
    assert_eq!(confirmed["result"]["type"], "confirmed", "{confirmed}");
    wait_for_commit(
        &tmp,
        "Typed options message\n\nSigned-off-by: Test User <test@example.com>",
    );
    assert_eq!(tmp.git(&["log", "-1", "--format=%an <%ae>"]).trim(), author);
    assert_eq!(tmp.git(&["log", "-1", "--format=%at"]).trim(), "981173106");
    let message = tmp.git(&["log", "-1", "--format=%B"]);
    assert!(message.contains("Signed-off-by:"));
    assert!(!message.contains("diff --git"), "Git owns verbose cleanup");
    assert_eq!(tmp.git(&["show", "HEAD:f.txt"]), "typed all content\n");
    assert!(!tmp.path().join(".git/unexpected-hook").exists());
    assert_eq!(tmp.git(&["status", "--porcelain"]), "");
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(20)).success());
}

#[test]
fn real_file_stdio_sync_and_checkpoint_barriers_are_exact_and_recoverable() {
    let (tmp, _) = staged_repo();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let target = tmp.path().join(".git/COMMIT_EDITMSG");
    let mut server = TestServer::spawn(&[]);
    let initialized = server.initialize(rich_capabilities());
    assert_eq!(
        initialized["result"]["capabilities"]["textDocumentSync"],
        json!({"openClose":true,"change":2,"save":{"includeText":true}})
    );
    let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
    let edit = start_commit(&mut server, &status);
    assert_eq!(edit["version"], 1);
    assert_eq!(read(&target), edit["text"].as_str().unwrap());
    sync_draft(&mut server, &edit, "Earlier live\n");
    change_draft(
        &mut server,
        &edit,
        9,
        "Chosen checkpoint\n\n# preserved comment\n",
    );
    let unapplied = command_confirm(&mut server, 10, &edit, 8, "synchronized");
    assert!(unapplied["error"].is_object());
    write(&target, "Chosen checkpoint\n\n# preserved comment\n");
    // Save and the old common.confirm route cannot release Git or write its file.
    let bypass = invoke(
        &mut server,
        12,
        &status,
        "common.confirm",
        json!({"runId":edit["runId"]}),
    );
    assert!(!bypass["result"]["effects"].as_array().unwrap().is_empty());
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(read(&target), "Chosen checkpoint\n\n# preserved comment\n");

    for version in [8, 9] {
        change_draft(&mut server, &edit, version, "Invalid stale text\n");
    }
    change_draft(
        &mut server,
        &edit,
        10,
        &"x".repeat(magit_lsp::editor::draft::MAX_DRAFT_BYTES + 1),
    );
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":edit["uri"],"version":10},"contentChanges":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"text":"Not full sync"}]
    }}));
    let unchanged = (tmp.git(&["ls-files", "--stage", "-z"]), read(&target));
    assert!(command_confirm(&mut server, 13, &edit, 10, "synchronized")["error"].is_object());
    let mut wrong = edit.clone();
    wrong["runId"] = json!("wrong-run");
    assert!(command_confirm(&mut server, 14, &wrong, 9, "synchronized")["error"].is_object());
    wrong = edit.clone();
    wrong["uri"] = json!("magit-commit://wrong-run/message");
    assert!(command_confirm(&mut server, 15, &wrong, 9, "synchronized")["error"].is_object());
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), unchanged.0);
    assert_eq!(read(&target), unchanged.1);
    // Confirmation accepts only its schema; extra text cannot replace the synchronized message.
    let args = json!({"command":"common.confirm","runId":edit["runId"],"uri":edit["uri"],"version":9,"source":"synchronized","text":"Forbidden"});
    server.send(json!({"jsonrpc":"2.0","id":16,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[args]}}));
    let extra_text = server.response_with_id(16);
    assert!(
        extra_text["error"].is_object() || extra_text["result"]["type"] == "confirmed",
        "{extra_text}"
    );
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), unchanged.0);
    assert_eq!(read(&target), unchanged.1);

    change_draft(&mut server, &edit, 11, "Unsaved newer text must not land\n");
    assert!(command_confirm(&mut server, 17, &edit, 9, "synchronized")["error"].is_object());
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), unchanged.0);
    assert_eq!(read(&target), unchanged.1);
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]),
        head,
        "detach is not close intent"
    );
    let closed = command_confirm(&mut server, 19, &edit, 9, "saved-file");
    assert_eq!(
        closed["result"],
        json!({"type":"confirmed","runId":edit["runId"],"uri":edit["uri"],"version":9})
    );
    wait_for_commit(&tmp, "Chosen checkpoint");
    assert_eq!(read(&target), "Chosen checkpoint\n\n# preserved comment\n");
    assert!(command_confirm(&mut server, 20, &edit, 9, "saved-file")["error"].is_object());
    change_draft(&mut server, &edit, 12, "Late terminal text\n");
    assert!(command_confirm(&mut server, 21, &edit, 12, "saved-file")["error"].is_object());
    let errors: Vec<_> = server
        .notifications
        .iter()
        .filter(|n| n["method"] == "view/effects")
        .flat_map(|n| n["params"]["effects"].as_array().into_iter().flatten())
        .filter(|effect| {
            effect["type"] == "message"
                && effect["text"]
                    .as_str()
                    .unwrap_or("")
                    .contains("commit draft:")
        })
        .collect();
    assert!(
        errors.len() >= 4,
        "stale, duplicate, oversized and range errors: {errors:?}"
    );
    assert!(errors.iter().all(|n| n["text"]
        .as_str()
        .unwrap()
        .contains("Your text is retained")));
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn incremental_commit_sync_negotiates_encodings_and_repairs_unknown_client_state() {
    for (encoding, lambda_end, astral_end) in [("utf-8", 3, 5), ("utf-16", 2, 3), ("utf-32", 2, 2)]
    {
        let (tmp, _) = staged_repo();
        let (other_tmp, _) = staged_repo();
        let head = tmp.git(&["rev-parse", "HEAD"]);
        let target = tmp.path().join(".git/COMMIT_EDITMSG");
        let other_target = other_tmp.path().join(".git/COMMIT_EDITMSG");
        let mut server = TestServer::spawn(&[]);
        let mut capabilities = rich_capabilities();
        capabilities["general"]["positionEncodings"] = json!([encoding]);
        let initialized = server.initialize(capabilities);
        assert_eq!(
            initialized["result"]["capabilities"]["positionEncoding"],
            encoding
        );
        let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
        let other_status = open_status(&mut server, 3, &json!(other_tmp.path().to_string_lossy()));
        let edit = start_commit(&mut server, &status);
        let other_edit = start_commit(&mut server, &other_status);
        sync_draft(&mut server, &other_edit, "other\n");
        sync_draft(&mut server, &edit, "aλ\r\nz😀\n");
        // Skipped versions and multiple ranges, including a dependent range.
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":edit["uri"],"version":5},"contentChanges":[
                {"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":lambda_end}},"text":"λ!"},
                {"range":{"start":{"line":0,"character":lambda_end+1},"end":{"line":0,"character":lambda_end+1}},"text":"?"},
                {"range":{"start":{"line":1,"character":astral_end},"end":{"line":1,"character":astral_end}},"text":"!"}
            ]
        }}));
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":other_edit["uri"],"version":3},"contentChanges":[
                {"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":0}},"text":"success\n"}
            ]
        }}));
        // Failed native close re-presents precisely the accepted incremental
        // snapshot, not a reconstruction from the original template or disk.
        let held = tmp.path().join(".git/held-sync-message");
        std::fs::rename(&target, &held).unwrap();
        server.send(
            json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
                "textDocument":{"uri":edit["uri"]}
            }}),
        );
        let recovery = loop {
            if let Some(effect) = edit_document(&server.recv()) {
                break effect;
            }
        };
        assert_eq!(recovery["runId"], edit["runId"]);
        assert_eq!(recovery["version"], 5);
        assert_eq!(recovery["text"], "aλ!?\r\nz😀!\n");
        std::fs::rename(&held, &target).unwrap();
        // A rejected reopen must not install a different store snapshot or
        // prevent the same pending run from accepting its trustworthy reopen.
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":edit["uri"],"languageId":"text","version":99,"text":"wrong reopen"}
        }}));
        assert!(command_confirm(&mut server, 20, &edit, 99, "synchronized")["error"].is_object());
        assert!(command_confirm(&mut server, 21, &edit, 5, "synchronized")["error"].is_object());
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":edit["uri"],"languageId":"text","version":5,"text":recovery["text"]}
        }}));
        change_draft(&mut server, &edit, 5, "stale must not land\n");
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":other_edit["uri"],"version":4},"contentChanges":[
                {"range":{"start":{"line":2,"character":0},"end":{"line":2,"character":0}},"text":"stale\n"}
            ]
        }}));
        let (line, column) = match encoding {
            "utf-8" => (0, 2),  // inside λ
            "utf-16" => (1, 2), // inside 😀
            _ => (0, 5),        // past the CRLF line boundary
        };
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":edit["uri"],"version":6},"contentChanges":[
                {"range":{"start":{"line":line,"character":column},"end":{"line":line,"character":column}},"text":"invalid boundary"}
            ]
        }}));
        // Invalid notification must not partially apply its first valid edit.
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":edit["uri"],"version":7},"contentChanges":[
                {"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"text":"BAD"},
                {"range":{"start":{"line":99,"character":0},"end":{"line":99,"character":0}},"text":"invalid"}
            ]
        }}));
        assert!(command_confirm(&mut server, 10, &edit, 7, "synchronized")["error"].is_object());
        assert!(command_confirm(&mut server, 11, &edit, 5, "synchronized")["error"].is_object());
        // didSave text is not a second unversioned synchronization channel.
        server.send(
            json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{
                "textDocument":{"uri":edit["uri"]},"text":"aλ!?\r\nz😀!\n"
            }}),
        );
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":edit["uri"],"version":8},"contentChanges":[
                {"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"text":"unknown"}
            ]
        }}));
        assert!(command_confirm(&mut server, 12, &edit, 8, "synchronized")["error"].is_object());
        // The registry's byte limit rejects an otherwise valid store update.
        // Neither that rejection nor malformed range JSON may advance its base.
        change_draft(
            &mut server,
            &edit,
            9,
            &"x".repeat(magit_lsp::editor::draft::MAX_DRAFT_BYTES + 1),
        );
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":edit["uri"],"version":10},"contentChanges":[
                {"range":{"start":{"line":0,"character":-1},"end":{"line":0,"character":0}},"text":"malformed"}
            ]
        }}));
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":other_edit["uri"],"version":5},"contentChanges":[
                {"range":{"start":{"line":3,"character":0},"end":{"line":3,"character":0}},"text":"rejected\n"}
            ]
        }}));
        assert!(command_confirm(&mut server, 14, &edit, 9, "synchronized")["error"].is_object());
        let other_held = other_tmp.path().join(".git/held-sync-message");
        std::fs::rename(&other_target, &other_held).unwrap();
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
            "textDocument":{"uri":other_edit["uri"]}
        }}));
        let other_recovery = loop {
            if let Some(effect) = edit_document(&server.recv()) {
                if effect["runId"] == other_edit["runId"] {
                    break effect;
                }
            }
        };
        assert_eq!(other_recovery["version"], 5);
        assert_eq!(other_recovery["text"], "other\nsuccess\nstale\nrejected\n");
        std::fs::rename(&other_held, &other_target).unwrap();
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":other_edit["uri"],"languageId":"text","version":5,"text":other_recovery["text"]}
        }}));
        std::fs::rename(&target, &held).unwrap();
        server.send(
            json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
                "textDocument":{"uri":edit["uri"]}
            }}),
        );
        let retained = loop {
            if let Some(effect) = edit_document(&server.recv()) {
                break effect;
            }
        };
        assert_eq!(retained["runId"], edit["runId"]);
        assert_eq!(
            retained["version"], 5,
            "rejected changes never advance registry/store snapshots"
        );
        assert_eq!(retained["text"], recovery["text"]);
        std::fs::rename(&held, &target).unwrap();
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":edit["uri"],"languageId":"text","version":5,"text":retained["text"]}
        }}));
        assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
        assert_eq!(read(&target), edit["text"].as_str().unwrap());
        // Full replacement reestablishes a trustworthy base for relative edits.
        change_draft(&mut server, &edit, 12, "aλ!?\r\nz😀!\n");
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":edit["uri"],"version":15},"contentChanges":[
                {"range":{"start":{"line":1,"character":astral_end+1},"end":{"line":1,"character":astral_end+1}},"text":"?"}
            ]
        }}));
        server.send(
            json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{
                "textDocument":{"uri":edit["uri"]},"text":"must not overwrite live text"
            }}),
        );
        // The rejected save did not corrupt the live snapshot or its version.
        assert!(command_confirm(&mut server, 15, &edit, 12, "synchronized")["error"].is_object());
        let confirmed = command_confirm(&mut server, 13, &edit, 15, "synchronized");
        assert_eq!(confirmed["result"]["type"], "confirmed", "{confirmed}");
        wait_for_commit(&tmp, "aλ!?\nz😀!?");
        assert_eq!(read(&target), "aλ!?\r\nz😀!?\n");
        let other_confirmed = command_confirm(&mut server, 30, &other_edit, 5, "synchronized");
        assert_eq!(other_confirmed["result"]["type"], "confirmed", "{other_confirmed}");
        wait_for_commit(&other_tmp, "other\nsuccess\nstale\nrejected");
        server.shutdown_and_exit();
        assert!(server.finish(Duration::from_secs(10)).success());
    }
}

#[cfg(unix)]
#[test]
fn real_file_stdio_failed_write_retains_helper_and_selected_checkpoint() {
    use std::os::unix::fs::symlink;
    let (tmp, _) = staged_repo();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let target = tmp.path().join(".git/COMMIT_EDITMSG");
    let held = tmp.path().join(".git/held-message");
    let neighbor = tmp.path().join("neighbor.txt");
    write(&neighbor, "Neighbor must survive\n");
    let mut server = TestServer::spawn(&[]);
    server.initialize(rich_capabilities());
    let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
    let edit = start_commit(&mut server, &status);
    sync_draft(&mut server, &edit, "Recovered checkpoint\n");
    std::fs::rename(&target, &held).unwrap();
    symlink(&neighbor, &target).unwrap();
    let failed = command_confirm(&mut server, 11, &edit, 2, "synchronized");
    assert_eq!(failed["error"]["data"]["recoverable"], true);
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(read(&neighbor), "Neighbor must survive\n");
    assert_eq!(read(&held), edit["text"].as_str().unwrap());
    std::fs::remove_file(&target).unwrap();
    std::fs::rename(&held, &target).unwrap();
    let retried = command_confirm(&mut server, 12, &edit, 2, "synchronized");
    assert_eq!(retried["result"]["type"], "confirmed", "{retried}");
    wait_for_commit(&tmp, "Recovered checkpoint");
    assert_eq!(read(&neighbor), "Neighbor must survive\n");
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn native_failed_close_re_presents_same_pending_run_and_retries_saved_disk() {
    let (tmp, _) = staged_repo();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let target = tmp.path().join(".git/COMMIT_EDITMSG");
    let held = tmp.path().join(".git/held-message");
    let mut server = TestServer::spawn(&[]);
    server.initialize(rich_capabilities());
    let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
    let edit = start_commit(&mut server, &status);
    sync_draft(&mut server, &edit, "Unsaved retained local text\n");
    std::fs::rename(&target, &held).unwrap();
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose",
        "params":{"textDocument":{"uri":edit["uri"]}}}));
    let recovery =
        loop {
            let message = server.recv();
            if let Some(recovery) = edit_document(&message) {
                assert!(message["params"]["effects"].as_array().unwrap().iter().any(
                    |effect| effect["type"] == "message"
                        && effect["text"]
                            .as_str()
                            .unwrap()
                            .contains("Your text is retained")
                ));
                break recovery;
            }
        };
    assert_eq!(recovery["runId"], edit["runId"]);
    assert_eq!(recovery["uri"], edit["uri"]);
    assert_eq!(recovery["version"], 2);
    assert_eq!(recovery["text"], "Unsaved retained local text\n");
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]),
        head,
        "unreadable saved file cannot confirm"
    );
    std::fs::rename(&held, &target).unwrap();
    write(&target, "Restored saved disk message\n");
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":recovery["uri"],"languageId":"gitcommit","version":recovery["version"],"text":recovery["text"]}
    }}));
    // The same pending helper accepts subsequent synchronization, but native
    // retry still commits disk rather than those newer unsaved bytes.
    change_draft(&mut server, &edit, 3, "Newer unsaved recovery edits\n");
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose",
        "params":{"textDocument":{"uri":edit["uri"]}}}));
    wait_for_commit(&tmp, "Restored saved disk message");
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn real_file_stdio_normal_close_selects_initial_template_without_save() {
    use std::os::unix::fs::PermissionsExt;
    let (tmp, _) = staged_repo();
    let hook = tmp.path().join(".git/hooks/prepare-commit-msg");
    write(
        &hook,
        "#!/bin/sh\nprintf 'Prepared initial message\\n# comment\\n' > \"$1\"\n",
    );
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut server = TestServer::spawn(&[]);
    server.initialize(rich_capabilities());
    let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
    let edit = start_commit(&mut server, &status);
    assert!(edit["text"]
        .as_str()
        .unwrap()
        .starts_with("Prepared initial message\n"));
    sync_draft(&mut server, &edit, "Discard this unsaved message\n");
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":edit["uri"]}}}));
    let closed = command_confirm(&mut server, 10, &edit, 1, "saved-file");
    assert!(
        closed["error"].is_object(),
        "didClose already won terminal arbitration: {closed}"
    );
    wait_for_commit(&tmp, "Prepared initial message");
    assert_eq!(
        read(&tmp.path().join(".git/COMMIT_EDITMSG")),
        edit["text"].as_str().unwrap()
    );
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn real_file_stdio_multiroot_and_other_connection_cannot_cross_runs() {
    let (first, _) = staged_repo();
    let (second, _) = staged_repo();
    let first_head = first.git(&["rev-parse", "HEAD"]);
    let second_head = second.git(&["rev-parse", "HEAD"]);
    let mut server = TestServer::spawn(&[]);
    server.initialize(rich_capabilities());
    let status_a = open_status(&mut server, 2, &json!(first.path().to_string_lossy()));
    let status_b = open_status(&mut server, 3, &json!(second.path().to_string_lossy()));
    let a = start_commit(&mut server, &status_a);
    let b = start_commit(&mut server, &status_b);
    assert_ne!(a["runId"], b["runId"]);
    assert_ne!(a["uri"], b["uri"]);
    sync_draft(&mut server, &a, "First repository\n");
    sync_draft(&mut server, &b, "Second repository\n");
    let mut crossed = a.clone();
    crossed["uri"] = b["uri"].clone();
    assert!(command_confirm(&mut server, 10, &crossed, 2, "synchronized")["error"].is_object());
    assert!(command_confirm(&mut server, 11, &crossed, 1, "saved-file")["error"].is_object());
    let mut foreign = TestServer::spawn(&[]);
    foreign.initialize(json!({}));
    change_draft(&mut foreign, &a, 99, "Foreign connection\n");
    assert!(command_confirm(&mut foreign, 10, &a, 2, "synchronized")["error"].is_object());
    assert!(command_confirm(&mut foreign, 11, &a, 1, "saved-file")["error"].is_object());
    assert!(!foreign
        .notifications
        .iter()
        .any(|n| n["method"] == "view/effects"));
    foreign.shutdown_and_exit();
    assert!(foreign.finish(Duration::from_secs(10)).success());
    assert_eq!(first.git(&["rev-parse", "HEAD"]), first_head);
    assert_eq!(second.git(&["rev-parse", "HEAD"]), second_head);
    assert_eq!(
        read(&first.path().join(".git/COMMIT_EDITMSG")),
        a["text"].as_str().unwrap()
    );
    assert_eq!(
        read(&second.path().join(".git/COMMIT_EDITMSG")),
        b["text"].as_str().unwrap()
    );
    assert!(finish_draft(&mut server, 12, &b)["error"].is_null());
    wait_for_commit(&second, "Second repository");
    assert_eq!(first.git(&["rev-parse", "HEAD"]), first_head);
    assert!(finish_draft(&mut server, 13, &a)["error"].is_null());
    wait_for_commit(&first, "First repository");
    assert_eq!(
        read(&first.path().join(".git/COMMIT_EDITMSG")),
        "First repository\n"
    );
    assert_eq!(
        read(&second.path().join(".git/COMMIT_EDITMSG")),
        "Second repository\n"
    );
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn reused_commit_uri_rejects_old_run_signals_before_new_run_commits_once() {
    let (tmp, _) = staged_repo();
    let mut server = TestServer::spawn(&[]);
    server.initialize(rich_capabilities());
    let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
    let old = start_commit(&mut server, &status);
    let old_uri = old["uri"].clone();
    sync_draft(&mut server, &old, "First run\n");
    assert_eq!(
        command_confirm(&mut server, 5, &old, 2, "synchronized")["result"]["type"],
        "confirmed"
    );
    wait_for_commit(&tmp, "First run");
    loop {
        let message = server.recv();
        if message["method"] == "view/effects"
            && message["params"]["effects"].as_array().is_some_and(|effects| {
                effects.iter().any(|effect| {
                    effect["text"]
                        .as_str()
                        .is_some_and(|text| text.starts_with("Committed:"))
                })
            })
        {
            break;
        }
    }
    write(&tmp.path().join("f.txt"), "second staged content\n");
    git(tmp.path(), &["add", "f.txt"]);
    let fresh = start_commit(&mut server, &status);
    assert_eq!(fresh["uri"], old_uri);
    assert_ne!(fresh["runId"], old["runId"]);
    sync_draft(&mut server, &fresh, "Fresh run message\n");
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["ls-files", "--stage", "-z"]);
    let file = read(&tmp.path().join(".git/COMMIT_EDITMSG"));
    assert!(command_confirm(&mut server, 6, &old, 2, "synchronized")["error"].is_object());
    server.send(json!({"jsonrpc":"2.0","id":7,"method":"view/invoke","params":{"viewId":status,"command":"common.cancel","args":{"runId":old["runId"]}}}));
    let cancel = server.response_with_id(7);
    assert!(
        cancel.get("error").is_none(),
        "stale cancel request itself is valid: {cancel}"
    );
    assert!(
        cancel["result"]["effects"].as_array().unwrap().is_empty(),
        "stale cancel is ignored: {cancel}"
    );
    let mut old_duplicate = fresh.clone();
    old_duplicate["runId"] = old["runId"].clone();
    let second_terminal = command_confirm(&mut server, 8, &old_duplicate, 2, "synchronized");
    assert!(
        second_terminal["error"].is_object(),
        "old terminal on reused URI: {second_terminal}"
    );
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index);
    assert_eq!(read(&tmp.path().join(".git/COMMIT_EDITMSG")), file);
    assert_eq!(
        command_confirm(&mut server, 9, &fresh, 2, "synchronized")["result"]["type"],
        "confirmed"
    );
    wait_for_commit(&tmp, "Fresh run message");
    assert_eq!(tmp.git(&["rev-list", "--count", "HEAD"]), "3\n");
    assert_eq!(tmp.git(&["rev-parse", "HEAD^"]), head);
    assert_eq!(tmp.git(&["status", "--porcelain"]), "");
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn real_file_stdio_linked_worktree_writes_only_actual_gitdir() {
    let (main, _) = staged_repo();
    let main_head = main.git(&["rev-parse", "HEAD"]);
    let main_message = read(&main.path().join(".git/COMMIT_EDITMSG"));
    let linked_parent = TempDir::new().unwrap();
    let linked = linked_parent.path().join("linked");
    main.git(&[
        "worktree",
        "add",
        "-b",
        "draft-linked",
        linked.to_str().unwrap(),
    ]);
    write(&linked.join("f.txt"), "Linked staged content\n");
    git(&linked, &["add", "f.txt"]);
    let mut server = TestServer::spawn(&[]);
    server.initialize(rich_capabilities());
    let status = open_status(&mut server, 2, &json!(linked.to_string_lossy()));
    let edit = start_commit(&mut server, &status);
    sync_draft(&mut server, &edit, "Linked worktree message\n");
    assert_eq!(
        finish_draft(&mut server, 10, &edit)["result"]["type"],
        "confirmed"
    );
    let actual = magit_lsp::git::GitRepo::open(&linked)
        .unwrap()
        .commit_message_path()
        .unwrap();
    assert!(linked.join(".git").is_file());
    assert_ne!(actual, main.path().join(".git/COMMIT_EDITMSG"));
    assert_eq!(read(&actual), "Linked worktree message\n");
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
    assert_eq!(
        git(&linked, &["log", "-1", "--format=%B"]).trim(),
        "Linked worktree message"
    );
    assert_eq!(main.git(&["rev-parse", "HEAD"]), main_head);
    assert_eq!(read(&main.path().join(".git/COMMIT_EDITMSG")), main_message);
}

#[cfg(unix)]
#[test]
fn real_file_production_helper_rejects_spoofs_and_duplicate_join() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::{fs::PermissionsExt, net::UnixStream};
    let (tmp, _) = staged_repo();
    let (other, _) = staged_repo();
    let neighbor = tmp.path().join(".git/neighbor");
    write(&neighbor, "Protected neighbor\n");
    let hook = tmp.path().join(".git/hooks/pre-commit");
    write(&hook, "#!/bin/sh\nprintf '%s' \"$MAGIT_LSP_EDIT_ID\" > .git/test-run\nprintf '%s' \"$MAGIT_LSP_HELPER_TOKEN\" > .git/test-token\ntouch .git/test-ready\nwhile ! test -e .git/test-release; do sleep 0.01; done\n");
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut server = TestServer::spawn(&[]);
    server.initialize(rich_capabilities());
    let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
    let socket = server.socket_path();
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !tmp.path().join(".git/test-ready").exists() {
        assert!(
            Instant::now() < deadline,
            "pre-commit helper test did not start"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let run = read(&tmp.path().join(".git/test-run"));
    let token = read(&tmp.path().join(".git/test-token"));
    let hello = json!({"editId":run,"token":token,"kind":"commit-message","path":tmp.path().join(".git/COMMIT_EDITMSG")});
    let rejected = |value: &Value| {
        let mut stream = UnixStream::connect(&socket).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        writeln!(stream, "{value}").unwrap();
        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&reply).unwrap()["ok"], false);
    };
    for (key, value) in [
        ("editId", json!("wrong-run")),
        ("token", json!("wrong-token")),
        ("kind", json!("merge-message")),
        ("path", json!(neighbor)),
        ("path", json!(other.path().join(".git/COMMIT_EDITMSG"))),
    ] {
        let mut invalid = hello.clone();
        invalid[key] = value;
        rejected(&invalid);
    }
    write(&tmp.path().join(".git/test-release"), "release\n");
    let mut response = false;
    let mut edit = None;
    while !response || edit.is_none() {
        let message = server.recv();
        if message["id"] == 4 {
            response = true;
        }
        if let Some(found) = edit_document(&message) {
            edit = Some(found);
        }
    }
    let edit = edit.unwrap();
    assert_eq!(edit["runId"], run);
    assert!(
        !edit.to_string().contains(&token),
        "private helper token leaked in announcement"
    );
    rejected(&hello); // one-shot credential cannot join a second socket
    sync_draft(&mut server, &edit, "Validated real helper\n");
    assert_eq!(
        finish_draft(&mut server, 10, &edit)["result"]["type"],
        "confirmed"
    );
    wait_for_commit(&tmp, "Validated real helper");
    assert_eq!(read(&neighbor), "Protected neighbor\n");
    server.shutdown_and_exit();
    let (exit, log) = server.finish_with_stderr(Duration::from_secs(10));
    assert!(exit.success());
    assert!(!log.contains(&token), "private helper token leaked in logs");
}
