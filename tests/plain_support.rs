#[path = "../scripts/generate_plain_support.rs"]
mod plain_support;

#[test]
fn plain_support_registry_behavior() {
    let generated = plain_support::render();
    assert!(generated.starts_with("# Plain presentation command support\n"));
    assert!(generated.contains("[Plain-key guidance](./PLAIN_KEYS.md)"));
    assert!(generated.lines().count() > 10);
}
