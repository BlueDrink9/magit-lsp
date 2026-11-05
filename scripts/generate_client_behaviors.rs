use magit_lsp::quirks::{Capability, CapabilityTest, Match, REGISTRY};

/// Render the registry-derived known-client behavior appendix.
pub fn render() -> String {
    let mut out = String::from("<!-- BEGIN GENERATED KNOWN CLIENT BEHAVIORS -->\n## Known client behaviors\n\n| ID | Automatic match | Capability override | Server workaround | Fake-editor model |\n|---|---|---|---|---|\n");
    for q in REGISTRY {
        let matching = match q.matches {
            Some(Match::ClientVersion { name, min, max }) => format!("Exact name `{name}`; inclusive version `{min}`–`{max}`"),
            Some(Match::Capability(CapabilityTest::DeclaresMagit)) => "Declares `experimental.magit`".into(),
            None => "None; explicit user override only".into(),
        };
        let capability = q.capability.map(|c| format!("{} `{}`", if c.enabled { "add" } else { "remove" }, match c.capability { Capability::DocumentChanges => "workspace.workspaceEdit.documentChanges", Capability::ShowDocument => "window.showDocument.support" })).unwrap_or_else(|| "—".into());
        out.push_str(&format!("| `{}` | {} | {} | {} | {} |\n", q.id, matching, capability, q.workaround.map(|x| format!("`{x}`")).unwrap_or_else(|| "—".into()), q.model.map(|x| format!("`{x}`")).unwrap_or_else(|| "—".into())));
    }
    out.push_str("\n`assume-show-document` has no automatic match. Helix initialize omission is not evidence; enable only by explicit user override after evidence that the build handles `window/showDocument`.\n\nQuirk resolution applies automatic matches first, then user overrides; explicit user settings win. Echo/hash reconciliation and one-document-at-a-time `workspace/applyEdit` safety are universal and independent of quirks. These entries record bounded evidence and tests, not fresh native execution or general client support claims.\n<!-- END GENERATED KNOWN CLIENT BEHAVIORS -->");
    out
}
