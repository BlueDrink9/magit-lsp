# UI test plan (for the builder)

Linked from `TASKS.md`. Agent-facing. Written so that each item can be implemented without guessing.

**Why this exists.** Tasks have been accepted against server state and unit-level Neovim cases, so bugs that only show on screen (wrong keys, stacked menus, missing actions, "frozen" messages, lag) went unnoticed. From now on, **a task that changes what the user sees is not done until a scripted test has driven the real UI and its screen output has been checked.**

**Status of the causes below.** The "suspected cause" lines are hypotheses written without reading the current code. Confirm each with a failing test before changing anything.

## 0. Rules

- [ ] Write the failing scenario first, commit nothing that "fixes" a bug without one.
- [ ] Never use `sleep` in a test. Wait for the client's `idle` state (§A.3).
- [ ] Every test runs against deterministic fixtures (the §8.2 git environment) and fixed terminal sizes.
- [ ] Every scenario that touches menus runs at **100x30 and 80x24**, under both profiles (`standard`, `vim`).
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

**Target files:** `scripts/ui`, `lua/magit-lsp/debug.lua`, `tests/ui/` (runner, decoder, fixtures, scenarios), `Makefile`, `AGENTS.md`.
**Reference files:** `scripts/minimal_init.lua`, `tests/common/mod.rs` (determinism env), existing mini.test cases.
**Constraints:** no new runtime dependency for the product; the driver may use `tmux`, `nvim`, and either POSIX shell or Python 3. If `tmux` or `nvim` is missing, `make ui-test` prints "SKIP: tmux not found" and exits 0 locally (non-zero in CI).

- [x] **U0.1 Debug API** in `lua/magit-lsp/debug.lua`:
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
- [x] **U0.6 Make target** `make ui-test` (runs all scenarios under both profiles and both sizes) and `make ui-tour` (§U2.2).
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
- [ ] **U2.4 R-3.4 clients (next ticket):** add scenario `commit-message-editor`: with a staged change, `c c`; expect a bottom split with a `gitcommit` buffer; type a message; confirm; expect a clean tree and the commit in the log. Add the same style of scenario for every later task that opens a new view (log, revision, refs).

---

## Appendix A — Specifications

### A.1 `scripts/ui` commands

| Command | Behaviour |
| --- | --- |
| `ui start <fixture> [--size WxH] [--profile standard\|vim] [--ansi\|--truecolor]` | Build the fixture repo, start Neovim in a detached tmux session with `--listen <sock>`, `TERM=xterm-256color`, `NVIM_APPNAME` isolated, `-u scripts/minimal_init.lua`. Prints the run id |
| `ui keys <key>…` | Send keys. Literal text uses `tmux send-keys -l`; names (`Escape`, `BSpace`, `Enter`, `C-c`, `Up`) are sent as tmux key names |
| `ui wait [--ms N]` | Poll `idle()` (via `nvim --server <sock> --remote-expr 'luaeval(...)'`) until true or timeout (default 2000 ms). Non-zero exit on timeout and print `state` |
| `ui screen [--ansi]` | `tmux capture-pane -p` (plain) or `-p -e` (escape codes). Plain output is exactly `rows` lines |
| `ui state` | Print `snapshot()` JSON |
| `ui trace [N]` | Print the last N key traces with per-hop durations |
| `ui stop` | Kill session, socket and temp repo |
| `ui run <scenario>…` | Execute scenarios (§A.5), write artefacts, print a table of PASS / FAIL / XFAIL |

tmux settings for the dedicated server: `set -g escape-time 0`, `set -g status off`, `set -g default-terminal xterm-256color`. Create the session with `-x <cols> -y <rows>` so the screen size is fixed.

### A.2 `snapshot()` JSON

```json
{
  "mode": "n",
  "cursor": {"row": 5, "col": 2},
  "idle": true,
  "windows": [
    {"id": 1000, "float": false, "row": 0, "col": 0, "width": 100, "height": 28, "buffer": 3, "filetype": "magit-lsp"},
    {"id": 1004, "float": true,  "row": 20, "col": 0, "width": 100, "height": 9,  "buffer": 5, "filetype": "magit-lsp"}
  ],
  "view_stack": [{"id": "v1", "kind": "status"}, {"id": "v2", "kind": "menu.commit"}],
  "pending": "-",
  "render": {"version": 14, "acked": 14},
  "keymap": {"scope": "menu.commit", "capture": true, "keys": ["c", "e", "a", "-a", "q"]},
  "messages": ["…last 20 lines of :messages…"],
  "notifications": ["…last 20 vim.notify texts…"]
}
```

`pending` and `keymap.keys` come from the server's `view/keymap` and `view/render` so clients stay thin; the debug module only reports what the client already holds.

### A.3 `idle()`

True only when all hold: no outstanding request to the server, `render.version == render.acked`, no prompt waiting, no `vim.schedule` callback pending that the client created. The client counts its own in-flight work (increment on send, decrement on response or applied render).

### A.4 Trace events

One JSON line per event: `{t_ms, id, event, detail}` with a monotonic clock and an id that follows one key press through the system. Events in order: `key_pressed`, `key_sent`, `server_received`, `handler_start`, `git_call` (with `cmd`, `duration_ms`; reuse `ProcessLog`), `handler_end`, `render_sent`, `render_received`, `render_applied`, `window_opened`, `idle`. `ui trace` prints the delta between consecutive events.

### A.5 Scenario DSL

Plain text, one statement per line, `#` comments. Header before `---`:

```
fixture: modified-two-files
size: 100x30            # runner also runs 80x24 unless "size-only: true"
profile: vim            # runner also runs "standard" unless "profile-only: true"
xfail: bug-2            # optional; see Rules
---
keys: m ?               # space-separated keys, each sent separately
wait                    # until idle
expect screen contains "Transient and dwim commands"
expect screen not-contains "Committing"
expect screen matches /Unstaged changes \(\d+\)/
expect windows.floating == 1
expect view_stack.menus == 1
expect pending == "-"
expect no_message /W10|readonly/
expect style "(--all)" dim              # dim | bold | coloured | plain
expect cursor on "Unstaged changes"
expect latency("?") <= 100ms            # key-to-idle for the last key named
snapshot: name                          # writes build/ui/<scenario>/<name>.screen and .ansi
```

`style` predicates use the ANSI decoder on the run containing the quoted text. `latency` uses the trace. A failed `expect` prints the expectation, the actual value, and the full current screen, then continues to the next scenario.

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
