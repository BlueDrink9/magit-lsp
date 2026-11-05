# Open questions

## 9. Open questions

1. **Config editing:** The client should provide a root command for editing its
   config file. The macOS config directory choice is resolved in
   [Architecture §3.18](ARCHITECTURE.md#318-configuration): `dirs` /
   `~/Library/Application Support`. The earlier `~/.config` via `etcetera`
   proposal is superseded; runtime behavior remains unverified.
2. **Escape behavior:** Escape cancels most views. Commit-message and rebase
   buffers may need client-specific exit behavior; retain this caveat.
3. **VSCode menu default:** QuickPick (proposed) vs. text document in a lower
   editor group. Revisit after the Phase 2 spike.
4. **Per-repo config trust:** is `.magit-lsp.toml` worth the risk surface?
   Currently limited to keys and display, behind `trust_repo_config`.
5. **Minimum supported Git version** (decide by running IMM-1 tests on old
   versions).
6. **Full-line background tinting:** wanted; T094 owns the feature.
7. **LSP 3.18 `workspace/textDocumentContent`** adoption when clients support it.
8. **Fold keys in VSCode** for users with customised `editor.toggleFold`:
   the "mirror my shortcuts" command (proposed) vs. any better mechanism found
   during the spike.
