# Hygiene baseline allowlist

Baseline captured 2026-11-05 with `cargo-machete 0.9.2`, `knip 5.88.1` and
`luacheck 1.2.0`; reproduction:

```sh
nix shell nixpkgs#cargo-machete nixpkgs#luajitPackages.luacheck --command npm exec --yes --package=knip@5.88.1 -- make hygiene
```

Full raw report: `/tmp/opencode/t003-hygiene/baseline.log`. cargo-machete
reported clean. Knip and luacheck findings below are recorded, not suppressed.
No dependency is allowlisted; isolated probe below confirms unused dependency
detection.

## Knip

- Unlisted dependency: `vscode-jsonrpc/node` at
  `clients/vscode/test/e2e/real-file-native.js:391:14`. Test imports the
  package but its package declaration is absent; determine the owning package
  declaration and add it (or remove the import) in follow-up T003 policy work.
- Unused exports: these module exports are not imported by Knip's configured
  entry graph. This is evidence of graph reachability, not proof they are safe
  to delete; validate public/test runtime consumers before disposition.
  - `clients/vscode/lib/commands.js`: `openStatus`, `viewport`,
    `pressCommand`, `invokeCommand`, `mirrorFoldShortcuts`, `keybindingsPath`.
  - `clients/vscode/lib/context.js`: `deriveContext`, `inactiveContext`,
    `staticScope`.
  - `clients/vscode/lib/drafts.js`: `cancel`, `close`, `confirm`,
    `disconnect`, `handleInvoke`, `onChanged`, `onClosed`, `save`, `setContext`,
    `setup`.
  - `clients/vscode/lib/edit.js`: `KIND`, `SCHEME`, `draftAction`,
    `isEditEffect`, `languageForKind`.
  - `clients/vscode/lib/edits.js`: `onDocumentClosed`,
    `onActiveEditorChanged`, `confirmEditCommand`, `cancelEditCommand`,
    `currentEdit`.
  - `clients/vscode/lib/effects.js`: `showMessage`, `replay`.
  - `clients/vscode/lib/foldkeys.js`: `parseJsonc`, `mapFoldCommand`,
    `mapFoldKeys`.
  - `clients/vscode/lib/menu.js`: `quickPickItems`, `quickPickItem`,
    `describeItem`, `routeTyped`, `charToKey`.
  - `clients/vscode/lib/menus.js`: `showMenu`, `menuItems`, `hideMenu`,
    `queueSteps`.
  - `clients/vscode/lib/prompt.js`: `CUSTOM_ID`, `promptItems`,
    `answerFor`, `customChoice`.
  - `clients/vscode/lib/render.js`: `applyRender`, `applyEdit`.
  - `clients/vscode/lib/requests.js`: `press`, `invoke`, `applyRender`,
    `handlePrompt`, `resync`.
  - `clients/vscode/lib/semantic.js`: `register`.
  - `clients/vscode/lib/session.js`: `activate`, `deactivate`,
    `createClient`.
  - `clients/vscode/lib/views.js`: `stateForUri`, `activeState`,
    `isMenuKind`, `ensureView`, `showView`, `showDocument`, `applyCursor`,
    `activateView`, `closeView`, `closeRoot`, `focusView`.
- Configuration hints: `clients/vscode/knip.json` entries `node_modules/**` and
  `out/**` are redundant ignores; `extension.js` is a redundant entry pattern.
  Remove/review these entries in follow-up rather than treating them as
  intentional exclusions.

## Luacheck

All 34 warnings / 0 errors from the baseline are itemized. They are pre-existing
code issues, not allowlisted source suppressions; fix or disposition individually.

- `lua/magit-lsp/debug.lua:91:3`: unused assignment to `opts`.
- Over-120-character lines: `lua/magit-lsp/edit.lua:98` (125 chars),
  `lua/magit-lsp/highlights.lua:19` (146), `lua/magit-lsp/keymap.lua:83` (140),
  `lua/magit-lsp/requests.lua:179` (127), `lua/magit-lsp/view.lua:626` (211),
  `lua/magit-lsp/view.lua:728` (190), `tests/spikes/real_file_nvim.lua:14`
  (127), `tests/test_magit_lsp.lua:514` (125), `:894` (126), `:1143` (126),
  `:1526` (170), `:1550` (133), `:1553` (171), `:1555` (369), `:1556` (323),
  `:1557` (292).
- `lua/magit-lsp/hooks.lua:42:3`, `:49:3`, `:56:3`, `:63:3`, `:70:3`, `:78:3`,
  `:103:3`: assignment has more right-side values than left-side variables.
- `lua/magit-lsp/init.lua:122:10`: writes undeclared global `open_status`;
  `:167:3` and `:373:3`: reads that undefined global. Likely spelling mismatch
  with declared `open_status`/configured global; confirm intended name.
- Empty branches: `lua/magit-lsp/keys.lua:256:106`,
  `scripts/color_spike.lua:57:19`, `tests/test_magit_lsp.lua:45:21`.
- `tests/spikes/real_file_nvim.lua:27:10`, `:40:10`, `:47:10`: writes
  undeclared globals `SpikeState`, `SpikeShow`, `SpikeRefresh`; declare local or
  test globals explicitly after confirming test contract.
- `tests/test_magit_lsp.lua:1743:16`: undefined `table.unpack` field under
  configured Lua version; reconcile runtime compatibility and checker config.

## Size and marker report

The runner soft limit is 100000 bytes. Over-limit tracked text files:
`docs/evidence/harness-reassessment-inputs.md` (508906 bytes / 9530 lines),
`src/server.rs` (162205 / 4242), `tests/server_async.rs` (107846 / 2717),
`tests/test_magit_lsp.lua` (107501 / 2730), `TASKS.md` (103993 / 1321), and
`tests/editor_bridge.rs` (103096 / 2442). These are concrete size findings;
the evidence archive and canonical roadmap may need separate policy treatment,
but are not blanket exemptions.

Tracked marker matches:
- `TASKS.md:15` and `TASKS_PART_SEVEN.md:98` mention runner requirements; these
  are documentation, not code-retention claims.
- `docs/evidence/harness-reassessment-inputs.md:6019:27`, `:6107:29`,
  `:6115:37`, `:7598:27`, `:8158:44`, `:8162:48`, `:8230:19`, and
  `:8889:19` are historical evidence text; not live source annotations.
- `scripts/hygiene.py:72`, `:77` and `tests/hygiene/test_hygiene.py:21`, `:72`,
  `:73`, `:77`, `:79` are the scanner and its fixtures/assertions.
- `src/commands.rs:643`, `:658`: retained bindings for planned navigation,
  TASKS §5.1.
- `src/config.rs:136`, `:143`, `:145`, `:152`, `:159`, `:161`: inactive or
  partially applied configuration schema, with roadmap references in source;
  review against TASKS §3.18 / R-5.5 / R-3.6 / R-5.1.
- `src/errors.rs:33`, `src/protocol.rs:29`: optional process-log link retained
  by IMM-3/R-3.7 although no current producer sets it.
- `src/protocol.rs:37`: effect contract supports `PushView` through shared
  server handling.
- `src/keymap.rs:63`: keymap payload metadata retained per TASKS §3.9 despite
  no functional client reader.
- `src/menu.rs:19`: reader schema retained for TASKS §3.11 Phase-2/R-2.7.
- `src/semantic_tokens.rs:44`, `:48` and `lua/magit-lsp/highlights.lua:19`:
  selected token use plus planned/disabled/bold modifier and type-to-theme
  presentation contracts in TASKS §3.15/R-5.3.
- `src/ui.rs:27`: generic prompt kind retained for TASKS §3.6/R-3.1; no current
  command emits it.
- `src/view.rs:158`: descriptor viewport retained per TASKS §3.2 for clients.
  `src/view.rs:263`: view contract retained for LogView page extension,
  TASKS §7/R-5.1.
- `tests/common/mod.rs:6`: test-only `allow(dead_code)` for shared helpers.

## Unused-dependency probe

Copied source was not needed: created and removed isolated
`/tmp/opencode/t003-hygiene/probe` crate with `itoa = "1"` and empty `main`.
Actual `cargo-machete` reported `machete-probe -- .../Cargo.toml: itoa`
(exit 1). The repository `Cargo.toml` SHA-256 before and after probe is
`7d9bbbb6843dbd7da8656b3808b89662b7b8a9af7c51463245ad7b304d4da019`; it was
not modified by the probe.
