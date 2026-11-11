# Known client behaviors

| ID | Automatic match | Capability override | Server workaround | Fake-editor model |
|---|---|---|---|---|
| `assume-document-changes` | Exact name `Neovim`; inclusive version `0.12.6`–`0.12.6` | add `workspace.workspaceEdit.documentChanges` | — | `neovim-accepts-undeclared-documentChanges` |
| `apply-result-unreliable` | Exact name `Neovim`; inclusive version `0.12.6`–`0.12.6` | — | `reconcile-apply-result` | `silent-stale-apply` |
| `decomposed-changes` | Exact name `helix`; inclusive version `25.07.1`–`25.07.1` | — | — | `decomposed-changes` |
| `assume-show-document` | None; explicit user override only | add `window.showDocument.support` | — | — |

`assume-show-document` has no automatic match. Helix initialize omission is not evidence; enable only by explicit user override after evidence that the build handles `window/showDocument`.

Quirk resolution applies automatic matches first, then user overrides; explicit user settings win. Echo/hash reconciliation and one-document-at-a-time `workspace/applyEdit` safety are universal and independent of quirks. These entries record bounded evidence and tests, not fresh native execution or general client support claims.
