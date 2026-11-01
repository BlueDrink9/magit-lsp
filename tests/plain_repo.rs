mod common;

use common::stdio::TestServer;
use magit_lsp::presentation::resolve::{resolve, Resolution};
use std::path::Path;

fn git(path: &Path, args: &[&str]) {
    std::fs::create_dir_all(path).unwrap();
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(path)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn repo(path: &Path) {
    git(path, &["init", "-q"]);
    git(
        path,
        &[
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@invalid",
            "commit",
            "--allow-empty",
            "-qm",
            "initial",
        ],
    );
}

#[test]
fn resolves_none_workspace_document_nested_alias_and_linked_worktree_without_guessing() {
    let temp = tempfile::tempdir().unwrap();
    let plain = temp.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert_eq!(resolve(None, &[plain.clone()], None), Resolution::None);

    let a = temp.path().join("a");
    let b = temp.path().join("b");
    repo(&a);
    repo(&b);
    assert!(
        matches!(resolve(None, &[a.clone(), b.clone()], None), Resolution::Choose(xs) if xs.len() == 2)
    );
    assert!(matches!(
        resolve(None, &[], Some(&a.join("document.txt"))),
        Resolution::Resolved(_)
    ));

    let nested = a.join("nested");
    repo(&nested);
    assert!(
        matches!(resolve(None, &[a.clone(), nested.clone()], None), Resolution::Choose(xs) if xs.len() == 2)
    );
    let alias = temp.path().join("alias");
    std::os::unix::fs::symlink(&a, &alias).unwrap();
    let linked = temp.path().join("linked");
    git(
        &a,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            linked.to_str().unwrap(),
        ],
    );
    let discovered = magit_lsp::git::GitRepo::discover(&linked).unwrap().unwrap();
    let original = magit_lsp::git::GitRepo::discover(&a).unwrap().unwrap();
    assert_ne!(discovered.root(), original.root());
    assert_ne!(discovered.git_dir(), original.git_dir());
    assert_eq!(discovered.common_dir(), original.common_dir());
    assert!(matches!(
        resolve(None, &[a.clone(), alias], None),
        Resolution::Resolved(_)
    ));
}

#[test]
fn real_binary_accepts_workspace_file_uri_and_preserves_explicit_rich_open() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo é #%");
    repo(&root);
    let uri = format!(
        "file://{}",
        root.to_string_lossy()
            .replace('%', "%25")
            .replace(' ', "%20")
            .replace('#', "%23")
            .replace('é', "%C3%A9")
    );
    let mut server = TestServer::spawn(&[]);
    let mut init = common::stdio::initialize_message();
    init["params"]["rootUri"] = serde_json::Value::String(uri);
    init["params"]["capabilities"] = serde_json::json!({"experimental":{"magit":{
        "v":1,"keys":"dynamic","modifiers":"full","render":"edits",
        "layout":["main","side","bottom","float"],
        "effects":["visitFile","editDocument","clipboard"],
        "prompts":["confirm","text","choice","multiChoice","secret"],"folds":"server"
    }}});
    server.send(init);
    server.send(serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    assert!(server.response_with_id(1)["result"]["capabilities"].is_object());
    server.send(serde_json::json!({"jsonrpc":"2.0","id":2,"method":"view/open","params":{"kind":"status","args":{"repo_path":root.to_string_lossy()},"viewport":{"cols":80,"rows":24}}}));
    let response = server.response_with_id(2);
    assert!(response["result"]["id"].is_string(), "{response}");
    server.shutdown_and_exit();
}

#[test]
fn configurable_presentation_policy_filters_git_status_and_watcher() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    repo(&root);
    std::fs::write(root.join(".magit"), "presentation").unwrap();
    std::fs::write(root.join("status.magit"), "presentation").unwrap();
    std::fs::write(root.join("ordinary"), "tracked candidate").unwrap();
    let listing = magit_lsp::git::GitRepo::open(&root)
        .unwrap()
        .status_listing_excluding(&[".magit".into(), "status.magit".into()])
        .unwrap();
    assert_eq!(
        listing
            .untracked
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        ["ordinary"]
    );
    assert!(!magit_lsp::watcher::relevant_path_excluding(
        &root.join("status.magit"),
        &root,
        &["status.magit".into()]
    ));
    assert!(magit_lsp::watcher::relevant_path_excluding(
        &root.join("ordinary"),
        &root,
        &["status.magit".into()]
    ));
}

#[test]
fn presentation_file_configuration_is_validated_and_defaults_to_magit() {
    let temp = tempfile::tempdir().unwrap();
    let config_file = temp.path().join("config.toml");
    std::fs::write(
        &config_file,
        "[presentation]\nfiles = [\"status.magit\", \".editor-view\"]\n",
    )
    .unwrap();
    let config = magit_lsp::config::load_config(&magit_lsp::config::LoadOptions {
        config_path: Some(config_file),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(config.presentation_files, ["status.magit", ".editor-view"]);
    assert!(magit_lsp::config::is_presentation_file(
        Path::new("sub/status.magit"),
        &config.presentation_files
    ));
    assert_eq!(
        magit_lsp::config::Config::default().presentation_files,
        [".magit"]
    );
}

#[test]
fn explicit_path_takes_precedence_and_ambiguous_candidates_are_not_resolved() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    repo(&a);
    repo(&b);
    assert!(
        matches!(resolve(Some(&a), &[b], None), Resolution::Resolved(repo) if repo.root == std::fs::canonicalize(a).unwrap())
    );
}
