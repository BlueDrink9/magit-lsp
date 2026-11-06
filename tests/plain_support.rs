#[path = "../scripts/generate_plain_support.rs"]
mod plain_support;

#[test]
fn plain_support_document_matches_the_registry() {
    let path = "docs/plain-support.md";
    let generated = plain_support::render();
    if std::env::var_os("UPDATE_PLAIN_SUPPORT").is_some() {
        std::fs::write(path, &generated).unwrap();
    } else {
        let text = std::fs::read_to_string(path).unwrap();
        assert_eq!(text, generated);
    }
    assert!(generated.contains("| `status.discard` |"), "{generated}");
    assert!(generated.contains("Unavailable — "), "{generated}");
    assert!(generated.contains("| Planned ("), "{generated}");
}
