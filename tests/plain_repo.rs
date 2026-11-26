use magit_lsp_test_support as common;

use common::git;
use common::stdio::TestServer;
use magit_lsp::file_uri::uri_from_path;
use magit_lsp::presentation::resolve::{resolve, Resolution};
use std::path::Path;

fn repo(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
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
    assert_eq!(
        resolve(None, std::slice::from_ref(&plain), None),
        Resolution::None
    );

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
    #[cfg(unix)]
    let alias = temp.path().join("alias");
    #[cfg(unix)]
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
    #[cfg(unix)]
    assert!(matches!(
        resolve(None, &[a.clone(), alias], None),
        Resolution::Resolved(_)
    ));
}

#[test]
fn real_binary_accepts_workspace_file_uri_and_preserves_explicit_rich_open() {
    magit_lsp_test_support::cargo_server_binary!();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo é #%");
    repo(&root);
    let uri = uri_from_path(&root).expect("native path has a file URI");
    let mut server = TestServer::spawn(&[]);
    let mut init = common::stdio::initialize_message();
    init["params"]["rootUri"] = serde_json::Value::String(uri.as_str().to_owned());
    init["params"]["capabilities"] = serde_json::json!({"experimental":{"magit":{
        "v":1,"keys":"dynamic","modifiers":"full",
        "layout":["main","side","bottom","float"],
        "effects":["visitFile","editDocument","clipboard"],
        "prompts":["confirm","text","choice","multiChoice","secret"]
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
    assert!(matches!(
        resolve(Some(&a), &[b], None),
        Resolution::Resolved(repo) if magit_lsp::path::same_object(&repo.root, &a).unwrap()
    ));
}
