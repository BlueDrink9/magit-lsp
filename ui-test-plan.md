# UI test plan (for the builder)

Linked from `TASKS.md`. Agent-facing. Written so that each item can be implemented without guessing.

**Why this exists.** Tasks have been accepted against server state and unit-level Neovim cases, so bugs that only show on screen (wrong keys, stacked menus, missing actions, "frozen" messages, lag) went unnoticed. From now on, **a task that changes what the user sees is not done until a scripted test has driven the real UI and its screen output has been checked.**

Follow the authoritative [no-vision policy in AGENTS.md](./AGENTS.md#working-conventions).

### Retained native Plain Neovim workflow

Run `make plain-neovim` to build a fresh server binary and exercise both final-newline variants. Set `PLAIN_ARGS` to pass runner arguments; it overrides Make's `--binary`, `--evidence` and `--size` defaults. Use `TMPDIR` to select a portable temporary root. The maintained driver uses finite waits and shared role-aware teardown. Evidence is JSON, `.screen` and logs only; never use images. Current acceptance and follow-ups are tracked under T020 in [the queue](TASKS.md); Windows/macOS remain CI follow-ups. No Helix harness exists.

To rerun only one newline variant, use `make plain-neovim PLAIN_ARGS="--final-newline without"` or `--final-newline with`; the default `both` retains the full acceptance run. Discard coverage uses the native code-action picker and document confirmation: opening and cancelling preserve Git state, while confirming restores the exact worktree file without changing HEAD or the index.

**Status of the causes below.** The "suspected cause" lines are hypotheses written without reading the current code. Confirm each with a failing test before changing anything.

## 0. Rules

- [ ] Write the failing scenario first, commit nothing that "fixes" a bug without one.
- [ ] Never use `sleep` in a test. Wait for the client's `idle` state (§A.3).
- [ ] Every test runs against deterministic fixtures ([§8.2 Git environment](docs/TESTING.md#82-determinism)) and fixed terminal sizes.
- [ ] Scenarios run once by default at `80x24` with the `standard` profile. Set `matrix: true` to opt into all four combinations of **100x30 and 80x24** × (`standard`, `vim`). An explicit `size` or `profile` selects that value first and remains compatible with matrix expansion, which adds the other standard matrix value. In matrix mode, `size-only: true` or `profile-only: true` restricts expansion of that dimension to its explicit value.
- [ ] Scenario runs write `.screen` and `.ansi` only for an explicit `snapshot:` statement or a failed combination. Failure artifacts retain quiet-`xfail` diagnostics; successful scenarios without snapshots create neither file.
- [ ] The final report for a task lists the scenario files, their results, and the paths of the saved `.screen` files.
- [ ] Known bugs are tracked as scenarios marked `xfail: <bug id>`. The runner reports them as XFAIL. If an `xfail` scenario passes, the runner **fails** until the tag is removed (this forces the fix to remove it).

## Test layers

| Layer | Tool | Use it for |
| --- | --- | --- |
| L1 | Rust `TestSession` | Menu content, pending-chord state, view-stack depth, messages, token attributes |
| L2 | mini.test child Neovim | Window counts, floats, keymaps applied, buffer options, `:messages`. Its screenshot cannot capture the command-line area reliably (neovim#21886), so read `:messages` instead |
| L3 | Real Neovim in tmux (new, §A) | What the user sees: timing, chord timeouts, key encoding, the command line, real colours |

---

## Phase U0 — Harness

### R-UI-1 — tmux driver, debug API, scenario runner

**Target files:** `scripts/ui`, `clients/neovim/lua/magit-lsp/debug.lua`, `tests/ui/` (runner, decoder, fixtures, scenarios), `Makefile`, `AGENTS.md`.
**Reference files:** `scripts/minimal_init.lua`, `tests/common/mod.rs` (determinism env), existing mini.test cases.
**Constraints:** no new runtime dependency for the product; the driver may use `tmux`, `nvim`, and either POSIX shell or Python 3. If `tmux` or `nvim` is missing, `make ui-test` prints "SKIP: tmux not found" and exits 0 locally (non-zero in CI).

- [x] **U0.1 Debug API** in `clients/neovim/lua/magit-lsp/debug.lua`:
  - [x] `snapshot()` returns the JSON shape in §A.2.
  - [x] `idle()` is true only when: no request in flight, last rendered version equals acked version, no prompt pending, no scheduled callback outstanding.
  - [x] `trace` support behind `MAGIT_LSP_TRACE=<path>` (§A.4).
  - [x] L2 test asserts `snapshot()` fields on a freshly opened status view.
- [x] **U0.2 Driver** `scripts/ui` with the subcommands in §A.1. Uses a dedicated tmux server (`-L magit-ui`), `escape-time 0`, `status off`.
  - [x] `start`, `keys`, `wait`, `screen`, `state`, `trace`, `stop`.
  - [x] `stop` always removes the session, socket and temp repo, even after a failure (use a trap).
  - [x] Two starts in parallel do not collide (unique socket per run id).
- [x] **U0.3 Fixtures** `clean`, `modified-two-files`, `staged-one-file`, `staged-and-unstaged`, `untracked`, `unborn`, `in-progress-rebase`, `in-progress-cherry-pick`. Each built by a script using the deterministic environment.
- [x] **U0.4 ANSI decoder:** converts `tmux capture-pane -p -e` output into runs `{row, col, text, fg, bg, bold, dim, reverse}`. Unit tests with hand-written escape strings covering 16-colour, 256-colour and truecolor sequences.
- [x] **U0.5 Scenario runner** `scripts/ui run <file>…` implementing the DSL in §A.5, writing artefacts to `build/ui/<scenario>/`. Unit tests for the predicate evaluator.
- [x] **U0.6 Make target** `make ui-test` and `make ui-tour` (§U2.2). Historical completion note: the original U0.6 requirement described running all scenarios under both profiles and both sizes; current scenarios opt into that matrix with `matrix: true`.
- [x] **U0.7 Fix the render-before-open race** the R-3.2 worker reported: the Neovim client buffers `view/render` by `viewId` until the view exists. *Done when:* an L2 test that delays the `view/open` response still ends with the correct buffer text, and 50 consecutive `ui start` runs show no missing first render.
  - [x] `view/render` and `view/keymap` are buffered per `viewId` and applied in order once the view/buffer exists (L2 test `buffers renders that beat the view/open response`).
  - [x] 50 consecutive `ui start` runs show no missing first render (driver U0.2).
- [x] **U0.8 Docs:** add to `AGENTS.md` the workflow "inspect, act, inspect": `ui start`, `ui keys`, `ui wait`, `ui screen`, `ui state`, `ui stop`.
- [x] **U0.9 Bug scenarios:** one scenario per bug in Phase U1, each marked `xfail`.

- [x] **Done when:** `make ui-test` runs and reports the ten bug scenarios as
  3 PASS / 7 XFAIL (bug 3 passes and is a plain regression); the smoke scenario
  (`open status, expect "Unstaged changes", stop`) passes under both profiles
  and both sizes. Bug 5 did not reproduce (11–16 ms end to end), so
  `latency-menu-open` is a plain passing regression guard;
  `commit-failure-clears-progress` was added in R-UI-5.

---

## Phase 3E — Managed commit draft native acceptance (design mapping)

Canonical protocol and bounds are in
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md#310-editor-bridge-open-gits-editor-inside-the-users-editor); this section does not
claim the planned draft protocol or adapters are implemented. Keep the existing
`commit-cancel-progress.scn` realFile evidence historical until coordinated
cutover. At cutover, extend that scenario and add focused native cases, using
inspect → act → inspect and always stopping owned UI:

- Neovim real UI (R-EDIT-4): Save/checkpoint leaves editor/run active; explicit
  confirm flushes exact version; normal close confirms saved checkpoint; dirty
  Save/Discard/Cancel distinguishes latest unsaved text; `didClose` without
  native intent does not confirm; explicit abort and disconnect cancel; stale
  version/owner and terminal late response do not write/commit; failed close
  recovers text. Verify Git message, HEAD/index/worktree, and progress terminal
  state. R-EDIT-8 history-prev/next recalls and restores the current unsent draft.
- VSCode native e2e (R-EDIT-5, `clients/vscode/test/e2e/suite.js`): same barriers,
  close-intent distinctions, recovery, history, status read-only and no implicit
  commit on save; scripted widget integration is not native evidence. Historical screenshot references are not current agent evidence requirements; follow the [no-vision policy](AGENTS.md#working-conventions).
  The writable `magit-commit` provider sends one explicit full-sync stream,
  independent of VSCode's native document-version counter. Use native Ctrl+S to
  checkpoint; use **Magit: Confirm Commit Draft**, **Magit: Abort Commit Draft**,
  **Magit: Close Commit Draft (Saved Checkpoint)** and **Magit: Previous/Next
  Commit Message** in the command palette or editor-title actions. The explicit
  close command classifies intent before teardown and offers a native
  Save/Discard/Cancel picker when dirty. VSCode exposes no reliable pre-close
  intent for its ordinary tab X/Ctrl+W, shutdown, or API disposal: those ambiguous
  closes abort, never confirm. Focus loss does neither. Printable text and native
  editing keys are not captured by the status-view keymap. History temporarily
  gates native mutation, checks document identity/version and reconciles any API
  edit race without overwriting local text. The native suite retains save,
  history, version-error, dirty-close, abort and disconnect evidence artifacts (historical screenshots are not agent requirements) plus
  `r-edit-5-evidence.json` / `r-edit-5-disconnect.json`; inspect fresh artifacts,
  not historical real-file A07 evidence. Process/progress-end ordering remains
  server-owned; tests wait independently for actual Git state and progress end.
- Adversarial process cases belong to Rust real-Git R-EDIT-2/A11 tests, not a
  UI-only claim: cancel in pre-hook/editor/commit-msg/finalizing, finish-v-cancel,
  inherited pipes after normal exit, bounded drain and TERM-resistant child,
  subsequent commit without lock cleanup, and no foreign process kill. Linux is
  required for R-EDIT-2 and R-EDIT-6 acceptance/cutover; unavailable macOS
  execution does not block Linux acceptance. Focus native Neovim evidence on new
  visible behavior; require VSCode evidence only for changed declared
  capabilities or files under `clients/vscode/`.
  macOS remains unverified, with its full-grace fallback retained. Separate later
  macOS CI/platform acceptance (R-EDIT-9) requires identified runner/OS/toolchain,
  real-Git group/session, non-reaping WNOWAIT and sole-reap traces (full grace,
  final group KILL before reap, no signals after reap), adversarial process cases
  and focused client evidence where the queue requires it. Linux `/proc` latency evidence is not
  macOS validation. Windows remains separate feasibility work (R-EDIT-7).

R-EDIT-2 cooperative-hook refinement uses `commit-owned-cancel.scn` (current
real-file editor, not managed drafts). Inspect the committing snapshot, select
the unique live `Git` / `commit` progress token from client-held metadata, cancel
that token, then retain the fresh plain `cancellation-feedback.screen` showing
the final Git outcome. Before any idle/state RPC, use terminal capture to dismiss
only an observed hit-enter prompt; do not send Enter or q blindly into status
mappings or open `:messages`. After idle, assert bounded notification history
contains `Cancelling…` and the final readable Git failure, plus one begin/report/end
for the actual token; retain those notifications in `cancel.trace.jsonl`.
Inspect the fresh `cancelled.screen` for staged changes and `next-commit.screen`
for the subsequent successful commit, with unchanged-HEAD and hook-TERM checks.
Client trace records cancellation intent with the actual token. The default
native idle deadline is unchanged; resistant/stopped hooks,
separate prompt-intent timing and escalation safety belong to Rust/stdio tests.
Never reuse older cancelled/next-commit artifacts after a failed run; the scenario
runner stops each owned native session even on failure. Verification attempts
and remaining R-EDIT-2 obligations are recorded in TASKS.md.

The Linux R-EDIT-2 acceptance ledger in TASKS.md maps each phase/race and process
adversary to its Rust/stdio test. Finalizing cancellation includes a real
post-commit hook: cancellation cannot undo an already recorded commit, so the
server reports success with an interrupted-hook advisory. Ordinary/linked-worktree
lock-warning probes leave supplied gitdir/common-dir/ref locks byte-for-byte
untouched; stalled probes have an independent readonly deadline. Production
`owned_process` trace events and owned unit assertions establish non-reaping
observation, TERM/CONT, grace, KILL-before-reap and no signals after reap. These
are current real-file runner evidence, not R-EDIT-6 managed-draft/native cutover
or macOS/Windows acceptance.

The final Linux acceptance gate passed on 2026-10-28: focused Rust, this native
scenario (all four variants), full `make test` and `git diff --check`. TASKS.md
records the terminal log path, counts and criterion-to-test mapping, and checks
R-EDIT-2 for Linux only. The earlier shell-budget timeout was not accepted as a
passing gate; no native deadline was increased. Post-gate inspection found no
owned server/editor/hook/UI survivors.

The supervised 2026-10-28 run passed standard/vim at 100x30 and 80x24; its fresh
plain screens and `cancel.trace.jsonl` in each variant directory show final
failure feedback, retained staged changes, and the next successful commit. The
single token has one begin/report/end and both feedback notifications. Observed
intent-to-report / intent-to-end times were respectively 0.677 / 40.852 ms,
0.891 / 36.696 ms, 0.644 / 28.587 ms, and 0.711 / 29.166 ms in that variant order;
these are progress timings, not key-to-idle bounds. Historical failed native runs
remain in the attempt ledger, not evidence against these fresh completed runs.
The following full regression failed the visual-line staging test's pre-existing
unstaged-line completion predicate (`build/test-logs/lua.VDSuAF`); the final
supervised correction waits for the server-symbol-scoped staged line and settled
idle, preserving native input and all exact Git assertions. Its one authorized
`make test-file FILE=tests/test_magit_lsp.lua && make test && git diff --check`
gate passed without retry, and the process audit found no owned survivors.
No unrelated UI was rerun; all canonical process/race/lock-warning and platform
gaps listed in TASKS.md remain open, so this is not phase completion.

## Phase U1 — Bug fixes (each against its scenario)

For every bug: **(a)** confirm the failing scenario fails for the stated reason, **(b)** fix, **(c)** remove the `xfail` tag, **(d)** run `make test` and `make ui-test`.

### R-UI-2 — Help and menu stack (bugs 1, 2, 3)

**Suspected cause:** `open.dispatch` is bound only in menu scopes, or bound as `g ?` in `vim` with no `h`; menu-from-menu is a push, and the client leaves the old float open.

- [x] **U1.1 (bug 1)** Keymap test: `?` and `h` resolve to `open.dispatch` in the `status` scope under both profiles. Fix the profiles; update the keymap manifest snapshots.
  - [x] Scenario `help-from-status`: fresh status, `?`, screen contains `Transient and dwim commands`; repeat with `h` (`help-from-status-h`, standard-only because `h` is a motion in vim).
- [x] **U1.2 (bug 2)** Define behaviour: inside a menu, `?` toggles that menu's help (or, if the menu has none, closes it and opens the dispatch). Opening a top-level menu from another menu **replaces** it unless the action is a deliberate sub-menu (for example "Configure…").
  - [x] L1 test: after `m` then `?`, the view stack contains one menu above status.
  - [x] Client closes the previous menu window on replace and pop (no leaked floats).
  - [x] Scenario `menu-then-help`: `m ?`, expect `windows.floating == 1`, `q`, expect `windows.floating == 0` and status visible.
- [x] **U1.3 (bug 3)** Regression scenarios: `q` and `Escape` each close an open menu (floating count 0, status focused).

### R-UI-3 — Read-only buffer warning (bug 4)

**Suspected cause:** `readonly=true` plus a write on every render triggers Vim's W10 ("Changing a readonly file").

- [x] **U1.4** Scenario `no-readonly-warning`: start, press `j j k`, `?`, `q`; expect no message matching `/W10|readonly/`.
- [x] **U1.5** Fix: drop `readonly` (keep `modifiable=false`, `buftype=nofile`, `undolevels=-1`). Re-run the IMM-4 L2 tests (`dd`, `x`, `ciw`, `u`, `:s/a/b/` leave the lines unchanged).
- [x] **U1.6** Text mode (R-2.6) still toggles modifiability correctly; add a scenario that enters text mode, types a character, leaves, and sees the buffer restored.

### R-UI-4 — Menu rendering: actions, flag colours, pending chords (bugs 6, 7, 8, 10)

**Suspected cause:** action groups missing or clipped; flag text unstyled; no pending-chord state.

- [x] **U1.7 (bug 10)** For every menu in the registry: render at width 80 and 120 and assert that every action key in the menu model appears in the text. *Done when:* a generated test covers all menus; the failing ones are listed in the ticket report.
  - [x] Client float height is at least the number of rendered lines, or the float scrolls. L2 test at 80x24 with the longest menu.
  - [x] Scenario `commit-menu-shows-actions`: `c`, expect lines containing `Commit`, `Extend`, `Amend`, `Reword`, `Fixup`, `Squash`.
- [x] **U1.8 (bug 6)** Tokens `magitMenuFlagOn` and `magitMenuFlagOff` for the `(--flag)` text, plus the existing on/off switch markers.
  - [x] L1 token test: toggling a switch changes the token on the flag text.
  - [x] Scenario: `style "(--all)" dim` before toggling; `style "(--all)" coloured` after (coloured = foreground differs from the default and not dim).
- [x] **U1.9 (bugs 7, 8)** Pending-chord state on the menu view (generic `pending: Vec<Key>`):
  - [x] `-` or `=` pushes onto `pending`. Items whose key does not start with the prefix render dim (`magitMenuDisabled`); the pending prefix renders highlighted; only matching items accept the next key.
  - [x] A key that completes an item toggles or prompts and clears `pending`. A key that matches nothing is swallowed with a one-line hint and `pending` is kept.
  - [x] `Backspace` pops one key. With empty `pending` it does nothing (it must not close the menu).
  - [x] L1 tests for each rule; `view/keymap` exposes the currently live keys so clients need no logic.
  - [x] Scenario `chord-prefix`: `c`, `-`, expect `pending == "-"` and non-switch items dim; `BSpace`, expect `pending == ""` and items normal; `-a`, expect the `--all` flag coloured.

### R-UI-5 — Commit feedback and latency (bugs 5, 9)

**Suspected cause (9):** nothing ends the "Committing…" progress when git exits without opening the editor. **(5):** unknown; measure first.

- [x] **U1.10 (bug 9)** Pre-flight in `commit.create`:
  - [x] Clean tree: info message `Nothing to commit (working tree clean)` with a hint about `--allow-empty`; no progress shown.
  - [x] Unstaged changes only: `confirm` prompt "Nothing staged. Stage all changes and commit?".
  - [x] Progress is always ended in a `finally`, including when git exits without editing.
  - [x] Scenario `commit-clean-tree`: `c c` on `clean`; expect screen contains `Nothing to commit`, expect screen does not contain `Committing`.
  - [x] Scenario `commit-failure-clears-progress`: force git to fail (for example a failing hook); expect an error message and no `Committing`.
- [x] **U1.11 (bug 5)** Measure first. Run `ui trace` for `?` from status on `modified-two-files` and paste the per-hop timings into the ticket report.
  - [x] Fix the slowest hop. Typical suspects: a git call inside `open.dispatch`; waiting on a per-repo lock; the client acking before sending the next key; a chord `timeoutlen` wait.
  - [x] Budgets (initial; tune once measured, scale with `UI_BUDGET_SCALE` on slow CI):

    | Action | Server time | End to end (key to idle) |
    | --- | --- | --- |
    | Open a menu or the dispatch (no git) | ≤ 50 ms | ≤ 100 ms |
    | Toggle a switch | ≤ 30 ms | ≤ 80 ms |
    | Stage a hunk on the fixture | ≤ 300 ms | ≤ 400 ms |
  - [x] Scenario `latency-menu-open` asserts the first row.

---

## Phase U2 — Keeping it from regressing

- [x] **U2.1 Ticket template** in `TASKS.md`: add to the TASK TICKET format: *Acceptance Criteria must include a visible outcome ("after pressing X the screen shows …") for any task that changes what the user sees; Verification Command must include `make ui-test`; the report lists the saved screens.* (Done: §7 "Task ticket template and UI rule".)
- [x] **U2.2 Tour:** `scripts/ui run tests/ui/tour.scn` opens status, `?`, each menu, a toggled switch, a pending chord, a clean-tree commit, an error. It writes `build/ui/tour.md` with every screen. Run it at the end of each phase and read it. (Done: `tests/ui/tour.scn` plus `tour-clean.scn`/`tour-error.scn`, merged by `make ui-tour`.)
- [x] **U2.3 CI:** a separate job `make ui-test` with tmux and Neovim installed; upload `build/ui/` as an artefact on failure. (Done: the `ui-test` job in `.github/workflows/ci.yml`.)
- [x] **U2.4 R-3.4 clients (next ticket):** add scenario `commit-message-editor`: with a staged change, `c c`; expect a bottom split with a `gitcommit` buffer; type a message; confirm; expect a clean tree and the commit in the log. Add the same style of scenario for every later task that opens a new view (log, revision, refs). (Done: `tests/ui/scenarios/commit-message-editor.scn` passes under both profiles at 100x30 and 80x24; it also drives the process log and its `git commit` entry.)

---

## Appendix A — Specifications

### A.1 `scripts/ui` commands

| Command | Behaviour |
| --- | --- |
| `ui start <fixture> [--size WxH] [--profile standard\|vim] [--ansi\|--truecolor]` | Build the fixture repo, start Neovim in a detached tmux session with `--listen <sock>`, `TERM=xterm-256color`, `NVIM_APPNAME` isolated, `-u scripts/minimal_init.lua`. Prints the run id |
| `ui keys <key>…` | Send keys. Literal text uses `tmux send-keys -l`; names (`Escape`, `BSpace`, `Enter`, `C-c`, `Up`) are sent as tmux key names. `--list` prints the supported names and both client spellings; `--check <key>…` validates without sending |
| `ui wait [--ms N] [--text S] [--cursor R[:C]] [--view ID\|TITLE]` | Poll the selected barrier(s) until all hold or timeout (default 2000 ms). With no barrier, wait for `idle()`. Non-zero exit on timeout and print state, a screen excerpt and the log path |
| `ui screen [--ansi]` | `tmux capture-pane -p` (plain) or `-p -e` (escape codes). Plain output is exactly `rows` lines |
| `ui state` | Print `snapshot()` JSON |
| `ui trace [N]` | Print the last N key traces with per-hop durations |
| `ui stop` | Kill session, socket and temp repo |
| `ui run <scenario>…` | Execute scenarios (§A.5), write artefacts, print a table of PASS / FAIL / XFAIL |

tmux settings for the dedicated server: `set -g escape-time 0`, `set -g status off`, `set -g default-terminal xterm-256color`. Create the session with `-x <cols> -y <rows>` so the screen size is fixed.

### A.1.1 Startup and socket diagnosis

`start` writes `launch-nvim.sh` into the run directory and asks tmux to run
only that short script. The script starts Neovim with the cleared environment,
redirects Neovim's stderr to `nvim.stderr` and records its exit status in
`nvim.exit`. This matters in Nix development shells: putting the multi-kilobyte
environment directly on the `tmux respawn-pane` command line fails with
"command too long" before Neovim starts.

A failure to start Neovim or reach its RPC socket raises a startup error whose
report names the socket path and its byte length (Unix limit 107 bytes), whether
the path exists and is a socket, its permission bits, whether the pane or
Neovim exited, the recorded exit status, a bounded connect probe and the tail of
Neovim's stderr. Scenario runs print these as `STARTUP FAILED`, separately from
scenario `expect:` failures: a startup problem is never reported as a behaviour
regression, and a failed scenario assertion is never called a socket failure.

When the socket exists but the startup idle gate does not pass, the error says
that RPC answered but idle readiness was not reached. It includes the last idle
expression outcome (false, remote-expression error with exit code, timeout, or
unexpected output) and bounded stdout/stderr. Failure-only diagnostics also
report readable client work counts, view render/ack versions and recent Neovim
messages. These extra probes run only after startup fails; successful startup
polling remains unchanged. A successful trivial RPC connect probe is not treated
as proof that the client's idle expression succeeded.

RPC helper processes are invoked with `--headless` and `/dev/null` stdin. This
is separate from the actual Neovim UI, which remains interactive in tmux. With a
PTY attached, a plain `nvim --server … --remote-expr …` helper can start a TUI,
emit terminal-control queries and send the expression result to stderr; captured
output then misclassifies a valid `true` as unexpected and terminal responses
may be consumed by the shell. The headless invocation keeps expression output on
stdout and prevents helper-side terminal reads/probes.

### A.1.2 Key tokens and spellings

`scripts/ui keys --list` prints one table from the encoding table in
`scripts/ui_lib/keys.py`, which is also what `send_keys` uses; there is no
second hand-maintained key list. Columns are the canonical key, the accepted
Neovim harness spellings, the canonical §3.8 VSCode `keybindings.json`
spelling, and how the unmodified key is sent. The VSCode column is a
cross-reference, not a spelling the Neovim harness accepts.

Accepted harness tokens are:

- W3C names (`Enter`, `Escape`, `Tab`, `Backspace`, `Delete`, `Insert`, `Home`,
  `End`, `PageUp`, `PageDown`, `ArrowUp`, `ArrowDown`, `ArrowLeft`,
  `ArrowRight`, `F1`–`F12`) and the older tmux aliases (`CR`, `Esc`, `BSpace`,
  `DC`, `IC`, `PPage`, `NPage`, `Up`, `Down`, `Left`, `Right`).
- Modifier chords use the tmux short forms for one character (`C-c`, `S-a`,
  `M-3`). Ctrl or Shift plus a special key also accepts the long forms
  (`ctrl+Enter`, `control+Enter`, `shift+Tab`) and is sent as raw CSI-u bytes
  because tmux drops modifiers it has no terminfo sequence for. `shift+Tab`,
  `S-Tab` and `BTab` all use terminal backtab. Meta has no harness encoding.
  A canonical long form that the tmux path cannot express (`ctrl+c`) is not
  silently equivalent to `C-c`; `keys --check` rejects it with the short
  suggestion.
- Anything else is literal text, sent verbatim with `send-keys -l`.

`scripts/ui keys --check <key>…` validates tokens without loading a run or
sending anything. Check mode accepts named keys, supported chords and single
characters, not arbitrary multi-character literal strings (use `keys <text>`
to send those). Unknown names therefore cannot pass as literal text. It prints
`ok` with the resolved transport for supported tokens, and `invalid` with the
nearest valid spelling for an unknown name
(`Escpae` → `Escape`, `ctrl+c` → `C-c`, `C-Escpae` → `C-Escape`). It exits 1
when any token is invalid. A canonical chord is rejected rather than silently
typed when the harness cannot encode it, even though the send path would treat
that token as literal text.

### A.1.3 Wait barriers

`scripts/ui wait` accepts these barriers, and waits until every supplied
barrier holds in the same polling iteration (screen and state observations
are sequential, not an atomic snapshot):

| Barrier | Observation |
| --- | --- |
| `--text <substring>` | Literal substring in `tmux capture-pane` output. This also works while a prompt blocks Neovim's RPC loop, where `idle()` stays false |
| `--cursor <row[:col]>` | Snapshot `cursor.row` and, when given, `cursor.col` (zero-based) |
| `--view <id or title>` | A `view_stack` entry whose full id, server-id suffix after the client-id separator, `title` or `kind` equals the value. Rich snapshots only; the Plain client has no server view stack |

With no barrier it polls `idle()` exactly as before. `--ms N` is the shared
deadline (default 2000 ms). On timeout `wait` exits 2 and prints the unmet
barriers, compact `snapshot()` state, a trimmed screen excerpt and the trace log
path plus the run directory, so a failure is diagnosable from the terminal
without re-running the scenario.

### A.2 `snapshot()` JSON

```json
{
  "mode": "n",
  "cursor": {"row": 5, "col": 2},
  "idle": true,
  "windows": [
    {"id": 1000, "float": false, "row": 0, "col": 0, "width": 100, "height": 28, "full_width": true, "buffer": 3, "filetype": "magit-lsp"},
    {"id": 1004, "float": true,  "row": 20, "col": 0, "width": 100, "height": 9,  "full_width": true, "buffer": 5, "filetype": "magit-lsp"}
  ],
  "view_stack": [{"id": "v1", "kind": "status", "title": "Status"},
                 {"id": "v2", "kind": "menu.commit", "title": "Commit"}],
  "pending": "-",
  "render": {"version": 14, "acked": 14},
  "keymap": {"scope": "menu.commit", "capture": true, "keys": ["c", "e", "a", "-a", "q"]},
  "messages": ["…last 20 lines of :messages…"],
  "notifications": ["…last 20 vim.notify texts…"]
}
```

`pending` and `keymap.keys` come from the server's `view/keymap` and `view/render` so clients stay thin; the debug module only reports what the client already holds. `view_stack` entries also carry the server's `title`, which `wait --view` matches alongside the id and kind.

### A.3 `idle()`

True only when all hold: no outstanding request to the server, `render.version == render.acked`, no prompt waiting, no `vim.schedule` callback pending that the client created. The client counts its own in-flight work (increment on send, decrement on response or applied render).

### A.4 Trace events

One JSON line per event: `{t_ms, id, event, detail}` with a monotonic clock and an id that follows one key press through the system. Events in order: `key_pressed`, `key_sent`, `server_received`, `handler_start`, `git_call` (with `cmd`, `duration_ms`; reuse `ProcessLog`), `handler_end`, `render_sent`, `render_received`, `render_applied`, `window_opened`, `idle`. `ui trace` prints the delta between consecutive events.

### A.5 Scenario DSL

Plain text, one statement per line, `#` comments. Header before `---`:

```
fixture: modified-two-files
matrix: true            # opt into all four standard size/profile combinations
size: 100x30            # with matrix:true, this size is tried first
profile: vim            # with matrix:true, this profile is tried first
client: rich            # optional rich | plain; independent of the keymap profile
xfail: bug-2            # optional; see Rules
env: MAGIT_LSP_PLACEMENT=vsplit   # optional extra Neovim environment
---
keys: m ?               # space-separated keys, each sent separately
goto "+line3 CHANGED"   # move the cursor to the first line containing the text
wait                    # until idle
expect screen contains "Transient and dwim commands"
expect screen not-contains "Committing"
expect screen matches /Unstaged changes \(\d+\)/
expect windows.floating == 1
expect view_stack.menus == 1
expect pending == "-"
expect no_message /W10|readonly/
expect style "(--all)" dim              # dim | bold | coloured | plain
expect style "added" different-foreground-from "removed"  # distinct fg evidence
expect cursor on "Unstaged changes"
expect latency("?") <= 100ms            # key-to-idle for the last key named
snapshot: name                          # writes build/ui/<scenario>/<name>.screen and .ansi
```

`style` predicates use the ANSI decoder on the run containing the quoted text. `expect style "<left>" different-foreground-from "<right>"` requires both texts on screen, each with one non-default foreground across its nonempty cells (a text split across runs must share one foreground), and those foregrounds must differ. It fails for missing text, default foregrounds, mixed foregrounds or equal foregrounds, and does not pin a colour value or scheme. `latency` uses the trace. A failed `expect` prints the expectation, the actual value, and the full current screen, then continues to the next scenario.

`goto "<text>"` moves the cursor to the first character of the first buffer line containing the quoted text (a literal match, no regex). It fails when the text is absent. It is an RPC call rather than a key press, so it neither appears in the trace nor contributes to a `latency` measurement; scenarios use it to name a target instead of counting motion keys.

`wait state <path> == <value>` polls structured native state with the same bounded timeout as `wait`; it does not infer readiness from document text. For example, `wait state draft == true` waits until Plain Neovim has actually opened `COMMIT_EDITMSG`, not merely displayed its pending URI.

`startup: held` opts into early startup for loading-phase tests. The driver returns once the RPC socket answers instead of waiting for the first rendered view, and a Git wrapper holds the first `status` subprocess until released; the default `startup: ready` keeps the existing ready wait. In the body, `wait status entered` polls the explicit entered marker and `release status` releases that first subprocess once; later Git calls pass straight through. The statements are rejected at parse time unless `startup: held`, an unknown mode is rejected at parse time, and normal teardown releases a still-held gate even when an expectation fails. Example:

```
fixture: modified-two-files
startup: held
---
wait status entered
expect screen contains "Loading"
release status
wait
```


`client: plain` uses `tests/ui/plain_init.lua` and stock `vim.lsp.start`, without loading `clients/neovim/lua/magit-lsp/`. The test-only `PlainUI` observations expose native buffer/client state, not fabricated Rich view metadata; `PlainUI.action` selects a server-returned command through Neovim's built-in code-action API. Plain snapshots also write `.json` state, and each combination retains `lsp.log` plus `cleanup.json` with owned editor/server PIDs and verified teardown. Run `make ui-file FILE=tests/ui/scenarios/plain-status-stage.scn` and `make ui-file FILE=tests/ui/scenarios/plain-commit.scn` for the focused 80x24 scenarios. These do not replace the retained `make plain-neovim` workflow or its distinct 120x100 coverage.

For opt-in commit-input diagnosis, run the focused scenario with `MAGIT_LSP_UI_PROBE=1`; the harness records Enter token tmux arguments/timestamps in run-specific files under `build/ui/probes/`, and Plain Neovim records Enter and commit-buffer text, typed key/key translation, detailed mode/blocking state, cursor, command-line type, messages, changedtick/document-version, mappings, options and relevant autocmd observations there. It also records scoped `.magit`/`COMMIT_EDITMSG` workspace/applyEdit versions and results, warning print/notify before/after call state and traceback, and `ModeChanged` transitions using a shared monotonically increasing event sequence. Probing is restricted to those fixture buffers; files are diagnostic, not pass/fail oracles.

Original diagnostic artifacts from the earlier failing run remain in `build/ui/plain-commit/original-artifacts/` (`edited-draft.json`, `edited-draft.screen`, `lsp.log`); the prior failing probe artifacts were preserved as `.preserved.jsonl` before the focused rerun.

VERIFIED: the 2026-11-17 passing instrumented run (`build/ui/probes/20261117-071900-071699-native-input-probe.jsonl`) records both message Enter callbacks in mode `i`, `blocking=false`; each is followed by a separate `TextChangedI`, and changedtick/document version advance 4→5→6. In contrast, the focused diagnostic rerun `20261117-072939-a98327` failed the unchanged Git message oracle. Its preserved probe (`build/ui/probes/20261117-072939-a98327-native-input-probe.jsonl`) records the first message Enter in `i`, followed by a newline and tick/version 4→5. The second Enter is observed in mode `r`, `blocking=false`, with text and tick/version unchanged; Neovim's subsequent `TextChangedI` advances only after the next literal message text, leaving the second line without a blank separator. Per the installed Neovim mode documentation, `r` is the hit-enter/return prompt mode (replace mode is uppercase `R`). This is direct evidence that the second Enter was consumed by that prompt in this failed run, correcting the earlier categorical conclusion: the passing run does not disprove prompt consumption in the failing run.

The instrumented run `20261117-073832-04b2ec` preserves the unchanged exact Git oracle failure (`build/test-logs/ui-file.hNa4kT`, exit 2; expected/actual differ at char 21, line 2). Its shared-sequence trace records `.magit` applyEdit incoming version 7/current version 8 at sequence 15; sequence 16 is the exact warning print from installed Neovim `vim/lsp/util.lua:545` in mode `i`; sequence 17 observes the second CR in mode `r`; sequence 18 is after print returns. The stock handler stack is recorded; applyEdit returns `applied=true` at sequence 19. This directly establishes warning→hit-enter mode transition→consumed CR ordering for this failing run. A1 and A2 are VERIFIED. The follow-up production correction path is the versioned `.magit` workspace/applyEdit delivery that sends version 7 after the current version is 8; this is diagnostic evidence, not a fix. No server source changed.

The diagnostic run's per-run cleanup succeeded; `pgrep -af '[m]agit-lsp'` returned no matches. The run-specific input/native probes and `build/test-logs/ui-file.hNa4kT` preserve the evidence. Probe changes pass `make ui-unit` (`ok: ui-unit`).

VERIFIED correction (2026-11-17): the failing run's `lsp.log:78–85`, now preserved under `build/ui/plain-commit/pre-newline-fix/standard-80x24/`, shows the source ordering: the server inserts the draft URI at virtual EOF; Neovim echoes an intermediate extra final line at version 7, returns `applied=true`, then echoes its own final-line cleanup at version 8. The resulting server repair uses version 7 and arrives after that cleanup. This is not an arbitrary draft/user edit race. `text_edits` now retains the shared terminal newline in its suffix, anchoring the append before that newline without changing server text or weakening document versions. The permanent `plain_eof_append_does_not_repair_a_native_intermediate_echo_with_a_stale_version` regression models this interleaving with observed versions and checks safe versioned delivery and exact target text hash. Before the fix it failed with `an intermediate EOF echo must not produce a stale versioned repair` (`build/test-logs/rust.BYjG3l`); after the fix `make test-rust FILTER="--test plain_harness"` passed all 30 tests. `make test-rust FILTER="--test plain_draft"` also passed.

VERIFIED native correction: `MAGIT_LSP_UI_PROBE=1 make ui-file FILE=tests/ui/scenarios/plain-commit.scn` exited 0, with scenario input and exact message oracle unchanged. Run `20261117-075317-344cce` passed the byte-exact commit message (including the blank separator), parent/content comparisons, clean Git state/no index lock, zero project Lua modules and zero Rich notifications. Its persistent `build/ui/probes/20261117-075317-344cce-native-input-probe.jsonl` records matching incoming/current versions on every `.magit` applyEdit entry; sequence 10–11 shows the draft-URI append advancing only 6→7, with no intermediate cleanup repair. Sequences 15–18 show both Enter keys in insert mode, unblocked, each followed by a newline and version advancement (4→5→6). There are no warning-print/notify events or hit-enter mode transitions in the probe. The scenario's `cleanup.json` reports `cleanup_verified=true`, no remaining owned PIDs, socket or tmux session; `pgrep -ax magit-lsp` returned no matches. Older run-specific probes and pre-fix text/JSON/log artifacts remain preserved.

`sh: <command>` runs a POSIX shell command in the fixture repo with the deterministic Git environment and `/dev/null` stdin, between key batches. Scenarios use it to change the repository outside Neovim (the `auto-refresh` watcher test modifies a tracked file this way); any direct Neovim RPC command must include `--headless`. A failed, timed-out or unlaunchable shell step is reported with its statement number, command and bounded detail as a failed combination; remaining size/profile combinations still run, and that run is always stopped.

`latency` always reports the measured key-to-idle time. Budgets are multiplied by `UI_BUDGET_SCALE`, and an over-budget measurement only fails the run when `UI_STRICT_LATENCY=1`; otherwise it is informational.

---

## Appendix B — Bug reference

| # | Bug | Scenario | Ticket |
| --- | --- | --- | --- |
| 1 | `?` and `h` open the help only after a menu was opened | `help-from-status` | R-UI-2 |
| 2 | `m` then `?` stacks a second menu | `menu-then-help` | R-UI-2 |
| 3 | `q` / `Escape` close a menu (regression) | `menu-close-keys` | R-UI-2 |
| 4 | "Changing a readonly file" on entry and on every key | `no-readonly-warning` | R-UI-3 |
| 5 | Long delay before the dispatch menu appears | `latency-menu-open` | R-UI-5 |
| 6 | Flag text should be dim when off, coloured when on | `flag-colours` | R-UI-4 |
| 7 | `-` should dim the prefix and restrict keys to switch characters | `chord-prefix` | R-UI-4 |
| 8 | Backspace should undo a chord step | `chord-prefix` | R-UI-4 |
| 9 | `c c` on a clean tree leaves "Committing…" on screen | `commit-clean-tree` | R-UI-5 |
| 10 | Menus show only arguments, not actions | `commit-menu-shows-actions` plus the generated all-menus test | R-UI-4 |
