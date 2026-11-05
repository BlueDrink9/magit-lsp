# Typed VSCode capability declaration evidence

T004.4a native run, 2026-11-05. Command:

```sh
mkdir -p /tmp/opencode/t004-vscode && cargo build
MAGIT_VSCODE_BIN=$(command -v codium) MAGIT_VSCODE_EDIT_ONLY=1 MAGIT_LSP_WIRELOG=/tmp/opencode/t004-vscode/raw.jsonl timeout 240s npm --prefix clients/vscode run test:e2e
```

Host: VSCodium 1.140.0 (Electron build `6880`); extension client identity in the actual initialize request: `VSCodium` 1.140.0. Language client: installed `vscode-languageclient` 9.0.1. Production session source SHA-256: `03570767518c8bb8573a40281a70cb0baf70cfba0e03e0f2bf479e154bd48055` (`clients/vscode/lib/session.js`).

Wire source: `/tmp/opencode/t004-vscode/raw.jsonl`. Actual production server PID 525743 received `initialize`; its `capabilities.experimental.magit` was exactly `{v:1,keys:"static",modifiers:"full",layout:["main"],effects:["visitFile","editDocument","clipboard"],prompts:["confirm","text","choice","multiChoice","secret"]}`. The request retained native `clientInfo`; no compatibility alias or extra layout was sent. Server returned real rich `view/render` notifications (wire rows 6, 40, 59, 66, 88, 98, 123); rows 40 onward include incremental edits. E2E text evidence: `/tmp/opencode/vscode-review-xUoN0f`; the scenario edited the tracked file through the rich service and checked rendered status text and edit effects.

The focused e2e process exited 0. The runner completed its cleanup; no extension-host or owned server process remains. Verification: `npm --prefix clients/vscode run test:unit && npm --prefix clients/vscode run test:integration && git diff --check` passed (`ok 22 tests`, integration passed).
