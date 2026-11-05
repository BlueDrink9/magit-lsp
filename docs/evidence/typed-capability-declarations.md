# Typed VSCode capability declaration evidence

## Current bundled Neovim declaration

Native Linux run, 2026-11-05. Reproduce from the repository root:

```sh
cargo build
mkdir -p /tmp/opencode/t004-neovim
PYTHONPATH=scripts python3 - <<'PY'
from pathlib import Path
from ui_lib import driver

wire = '/tmp/opencode/t004-neovim/raw.jsonl'
Path(wire).parent.mkdir(parents=True, exist_ok=True)
run = None
try:
    run = driver.start('staged-one-file', extra_env={'MAGIT_LSP_WIRELOG': wire})
    print(driver.snapshot(run))
    print(driver.capture(run))
finally:
    if run is not None:
        driver.stop(run)
PY
```

The driver pins `target/debug/magit-lsp`, starts isolated Neovim, waits for the first render, and clears its run directory on `stop`; the opt-in wirelog stays outside that directory. Snapshot after render: idle `true`, view stack `status`, render version/ack `1/1`. Captured screen showed the staged `f.txt` hunk, including `line3 CHANGED` and `NEW line`, plus empty unstaged/untracked sections.

Recorder: `/tmp/opencode/t004-neovim/raw.jsonl`, drained after driver shutdown; 17 rows, SHA-256 `1a497a77373707ab68583a37ed431f4b9d7ae35d94159dd1b695207aae60251e`. Initialize is source row 1, server PID `833505`; row SHA-256 `24fc99d497d70056aaff9bd571fa8cfbcb8be178d7bd9e3261d1bc8e224e6c06`. Actual `clientInfo` is `{"name":"Neovim","version":"0.12.6"}`. Emitted `capabilities.experimental.magit` was exactly:

```json
{"effects":["visitFile","editDocument","clipboard"],"keys":"dynamic","layout":["main","side","bottom","float"],"modifiers":"basic","prompts":["confirm","text","choice","multiChoice","secret"],"v":1}
```

Server emitted real `view/render` at source row 9, version 1, mode `interactive`, view `v1`. Its text was `Head: main`, `Staged changes (1)`, the full `f.txt` patch shown in the native screen, then `Unstaged changes (0)` and `Untracked files (0)`; client acknowledged version 1 at row 13. PID 833505 was the owned server; driver `finally` called `stop(run)`, which quits Neovim, kills its private tmux server, and removes its run files. No other resources were stopped. The wirelog remains available for T004.4b2.

Current VSCode source check: `clients/vscode/lib/session.js` still hashes to `03570767518c8bb8573a40281a70cb0baf70cfba0e03e0f2bf479e154bd48055`, matching its retained evidence below. Neovim production Lua source `lua/magit-lsp/config.lua` SHA-256: `e2cdd00a28865ac8589a5fd8aa827685d1e0b57e50c6074b02d13381f022259f`.

T004.4a native run, 2026-11-05. Command:

```sh
mkdir -p /tmp/opencode/t004-vscode && cargo build
MAGIT_VSCODE_BIN=$(command -v codium) MAGIT_VSCODE_EDIT_ONLY=1 MAGIT_LSP_WIRELOG=/tmp/opencode/t004-vscode/raw.jsonl timeout 240s npm --prefix clients/vscode run test:e2e
```

Host: VSCodium 1.140.0 (Electron build `6880`); extension client identity in the actual initialize request: `VSCodium` 1.140.0. Language client: installed `vscode-languageclient` 9.0.1. Production session source SHA-256: `03570767518c8bb8573a40281a70cb0baf70cfba0e03e0f2bf479e154bd48055` (`clients/vscode/lib/session.js`).

Wire source: `/tmp/opencode/t004-vscode/raw.jsonl`. Actual production server PID 525743 received `initialize`; its `capabilities.experimental.magit` was exactly `{v:1,keys:"static",modifiers:"full",layout:["main"],effects:["visitFile","editDocument","clipboard"],prompts:["confirm","text","choice","multiChoice","secret"]}`. The request retained native `clientInfo`; no compatibility alias or extra layout was sent. Server returned real rich `view/render` notifications (wire rows 6, 40, 59, 66, 88, 98, 123); rows 40 onward include incremental edits. E2E text evidence: `/tmp/opencode/vscode-review-xUoN0f`; the scenario edited the tracked file through the rich service and checked rendered status text and edit effects.

The focused e2e process exited 0. The runner completed its cleanup; no extension-host or owned server process remains. Verification: `npm --prefix clients/vscode run test:unit && npm --prefix clients/vscode run test:integration && git diff --check` passed (`ok 22 tests`, integration passed).

`tests/fixtures/current-client-capabilities.json` retains the exact `experimental.magit` blocks and clientInfo from initialize row 1 for both recorders, with source PID, row, full recorder SHA-256, and producer path/SHA-256. `current_native_client_capabilities_are_strictly_negotiated` loads both records, checks production `MagitCapabilities` deserialization and `negotiate`, verifies their rich modes/layouts, and rejects an unknown-field mutation. Recorder/source equality and native causality were manually checked; CI enforces the stored records' schema and negotiation only. It does not reread ephemeral recorder files, detect later producer drift, or independently prove native provenance.

## T004 acceptance audit

Original acceptance is immutable at `b89bf60:TASKS_PART_SEVEN.md` (T004, clauses 1–7). Current implementation and enforcing evidence:

1. `src/presentation/mod.rs` deserializes typed `MagitCapabilities` with strict serde fields and typed key/modifier modes; `tests/plain_core.rs::current_native_client_capabilities_are_strictly_negotiated` checks the retained actual Neovim and VSCode declaration blocks, negotiation, and unknown-field rejection. The unsupported `forwardAll`, `full` render and native-fold variants, plus singleton declaration fields, are absent from live declarations and schema.
2. `lua/magit-lsp/config.lua` contains the current Neovim declaration without obsolete `prompts`, `placements` or `editorBridge` fields; native emitted block and source hash are recorded above.
3. `PromptUi::multi_choice` remains consumed by real client/shared prompt vectors (`tests/plain_core.rs` declaration coverage); this is a consumer rationale, not a claim that a command produces it. `process_log_id`, obsolete `OpenView` production path and parsing helpers were audited against current usages; `tests/common/recording_ui.rs` supplies shared recording behavior.
4. `src/dispatch.rs` uses camelCase serde names; Plain tests exercise `nodeId`/`mutationHash` payloads and reject obsolete snake_case names.
5. `src/commands.rs` owns optional-effect requirements in registry metadata; `RichCapabilities::command_requirement` consumes that metadata. `tests/plain_core.rs::rich_command_requirements_come_from_registry` checks all seven initiators and no clipboard requirement for generic confirmation. Real visit/copy, selection-copy completion and commit-editor initiation are exercised in `tests/commands.rs`, `tests/log_selection.rs` and `tests/editor_bridge.rs`; missing-capability tests retain preflight/no-side-effect checks. This covers exercised producers, not every possible future producer.
6. VSCode declares only `main`, the placement honored by `clients/vscode/lib/views.js` and `effects.js`; retained native wire evidence above confirms the actual initialize declaration. `tests/plain_core.rs` exercises supported placement and production fallback.
7. `src/presentation/custom.rs` checks undeclared effects, prompt kinds/secret text and placements only under library `cfg(test)`, before fallback. Unit tests cover strict rejection and declared emissions; `tests/plain_core.rs` covers production warning and Main-placement fallbacks with descriptor identity. No production test-only mode is exposed.

Full source/provenance and native causality checks were manual; retained declaration inputs are immutable historical captures. Automated tests validate checked-in records and exercised code paths, not live recapture, future producer discovery or native provenance.
