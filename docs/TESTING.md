# Testing strategy

## 8. Testing strategy

**Target distribution:** ~80% Rust in-process, ~15% protocol-level (spawned
binary over stdio), ~5% Neovim. The Neovim tests that exist stay as
integration tests; their *intent* for logic moves to Rust.

### 8.1 In-process `TestSession` harness (main tool)

The harness is implemented in `tests/common/session.rs` (IMM-7; completed
history in the archive); extend it there rather than adding another session
type, with visibility/keymap/effects inspection as needed. Drive the real key/invoke path,
locate cursor targets through tree metadata, snapshot buffers (`insta` or
`expect-test`) and assert repo state with Git. `ScriptedUi` queues text,
confirm, choice and `edit_document(|text| …)` replies.

### 8.2 Determinism

`make vscode-e2e` runs the focused A06 quit/reopen scenario in an isolated real
VSCode host. It requires a supported `code`/`codium` binary, `DISPLAY`, the
built server, and `xdotool`; missing prerequisites fail rather than skip.
Structured text evidence is written to the runner's isolated evidence folder.

Fixtures set `GIT_AUTHOR_DATE`, `GIT_COMMITTER_DATE`, `GIT_CONFIG_NOSYSTEM=1`,
a temp `HOME`, `XDG_CONFIG_HOME` to a temp dir, `LC_ALL=C`, fixed author and
committer, so hashes are stable and appear verbatim in snapshots. Normalise
only temp paths and timings.

### Protocol example replay and capture

`make test-rust FILTER="--test examples"` replays every maintained recording
against the real binary. Server frames and action arrays remain strictly
ordered; only the path, binary, outbound-id, run-id and commit-hash
canonicalization documented in `tests/examples.rs` is allowed. Live client
frames substitute the placeholder root with the shared native-path encoder's
`file://` URI in URI positions and with the native path where the protocol
expects a filesystem path; canonicalization reduces both forms, including the
UTF-16 range widths they address, back to the recorded placeholders. Strict
replay fails on a divergent payload, a reordered action array or changed frame
ordering; only files that do not record a dynamic `view/keymap` push may skip
one, while a file that records one keeps it in strict order
([tests/examples.rs:1197–1204](../tests/examples.rs#L1197),
[tests/examples.rs:1151–1288](../tests/examples.rs#L1151)).

`make update-examples` regenerates all six maintained logical recordings:
`plain-open`, `rich-lifecycle`, `plain-code-action`, `plain-document-prompt`,
`commit-edit-abort` and `commit-edit-confirm`
([tests/examples.rs:1290–1298](../tests/examples.rs#L1290)). Regeneration
sends the recorded requests in order and enforces the same frame ordering and
post-exit checks as replay. It still normalizes roots, binary paths, run ids,
outbound ids and hashes; only a divergent `s2c` payload is captured. A capture
keeps the recorded line's envelope and refuses a changed exchange identity:
the method (request or notification) and the request/response id must match
the recorded frame ([tests/examples.rs:1114–1149](../tests/examples.rs#L1114)).
New entries come from validated real-server responses; retained entries keep
their recorded values, not temporary fixture paths.

Regeneration keeps each recording's artifact mapping: the two commit-edit
exchanges stay split across their existing `-01`/`-02` fragments, every other
recording is monolithic, and an unrelated recording name is refused
([tests/examples.rs:1300–1315](../tests/examples.rs#L1300)). Unchanged lines
stay byte-identical, existing fragment boundaries are preserved, and every
generated fragment must stay below 100000 bytes (a bounded capture/storage
constraint, not a human-readability or authored-transcript quota)
([tests/examples.rs:1317–1336](../tests/examples.rs#L1317)). Intact authored
transcripts need not be split solely for size. Inspect the diff, rerun normal
replay, and commit recordings separately; then run `make update-schema`,
`make schema-check` and `make docs-check`, committing generated worked
exchanges separately.

### Fixtures and oracles

#### Commit identity

- **VERIFIED:** Amend and Extend use `--amend` (Extend adds `--no-edit`); Reword adds `--only --allow-empty`, excluding staged content ([src/git.rs:239–249](../src/git.rs#L239)).
- **VERIFIED:** Changed-message Reword changes HEAD's OID but preserves its tree and index; staged amendments change the OID. Clean Extend with preserved metadata can retain it: “amend” does **not** imply an unconditional inequality oracle ([tests/editor_bridge.rs:452–486](../tests/editor_bridge.rs#L452)).
- **VERIFIED:** Ordinary `commit.fixup` creates an additional commit whose parent is the previous HEAD; it does not yet fold the target ([src/session.rs:3341–3355](../src/session.rs#L3341), [tests/log_commands.rs:55–59](../tests/log_commands.rs#L55)).
- **VERIFIED:** Instant fixup/squash creates a marker then launches autosquash; tests preserve the exclusive base OID and check that the marker disappears from reachable history ([src/instant_rebase.rs:61–84](../src/instant_rebase.rs#L61), [tests/instant_rebase.rs:224–238](../tests/instant_rebase.rs#L224)).
- **INFERRED:** Rebase rewrites commits whose parent/tree/message/metadata changes, and consequently affected descendants; unchanged picks may retain OIDs, so do not assert that every replayed OID differs merely because rebase ran ([src/rebase_plan.rs:100–108](../src/rebase_plan.rs#L100); external semantics: [Git rebase manual](https://git-scm.com/docs/git-rebase)).
- **VERIFIED:** Preparing/editing/discarding an unlaunched plan preserves repository state; prepared Reword/Squash currently refuse launch without mutation. Cancelled amendment also preserves HEAD/index ([tests/rebase_interactive.rs:89–118](../tests/rebase_interactive.rs#L89), [tests/editor_bridge.rs:443–446](../tests/editor_bridge.rs#L443)).

#### Abbreviations and full identities

- **VERIFIED:** Log rows use a hard-coded **8** characters, while node identity, selection and copy retain the full OID ([src/log_view.rs:312–346](../src/log_view.rs#L312)).
- **VERIFIED:** Rebase-todo rows use a hard-coded **12** characters; their node IDs/commit metadata retain full OIDs, and the submitted prepared Git todo writes full OIDs ([src/rebase_todo_view.rs:42–48](../src/rebase_todo_view.rs#L42), [src/rebase_plan.rs:103–106](../src/rebase_plan.rs#L103)).
- **VERIFIED:** These display lengths are renderer literals, not Git abbreviation configuration; log acquisition requests full `%H`/`%P`. Full-OID validation accepts **40 or 64** hexadecimal characters, not universally 40 ([src/log_view.rs:333](../src/log_view.rs#L333), [src/rebase_todo_view.rs:46](../src/rebase_todo_view.rs#L46), [src/git.rs:726](../src/git.rs#L726), [src/git.rs:1855](../src/git.rs#L1855)).

#### Capability declarations, not inferred editor features

- **VERIFIED:** Absence of `experimental.magit` selects Plain; a present declaration selects validated Rich v1. Wire semantics belong in [PROTOCOL.md](PROTOCOL.md), not this fixture inventory ([src/presentation/mod.rs:35–51](../src/presentation/mod.rs#L35)).

| Declaration/profile | Declared capability matrix | Evidence |
| --- | --- | --- |
| Plain minimum | `workspace.applyEdit=true`, `textDocument.codeAction={}`; no Rich selector | **VERIFIED** [tests/common/editor_model/mod.rs:82–86](../tests/common/editor_model/mod.rs#L82) |
| Rich minimum | v1, dynamic keys, basic modifiers, main layout; empty optional effects/prompts, no picker | **VERIFIED** [tests/common/editor_model/mod.rs:82–86](../tests/common/editor_model/mod.rs#L82) |
| Current Neovim | v1, dynamic keys; basic/full modifiers depending on terminal/GUI; main/side/bottom/float | **VERIFIED** [clients/neovim/lua/magit-lsp/config.lua:84–106](../clients/neovim/lua/magit-lsp/config.lua#L84) |
| Current VSCode | v1, static keys, full modifiers, main only | **VERIFIED** [clients/vscode/lib/session.js:50–58](../clients/vscode/lib/session.js#L50) |
| Both current clients | Effects: visitFile, clipboard, editDocument; prompts: confirm, text, choice, multiChoice, secret | **VERIFIED** [clients/neovim/lua/magit-lsp/config.lua:107–108](../clients/neovim/lua/magit-lsp/config.lua#L107), [clients/neovim/lua/magit-lsp/shared.lua:11–15](../clients/neovim/lua/magit-lsp/shared.lua#L11), [clients/vscode/lib/session.js:57–58](../clients/vscode/lib/session.js#L57), [clients/vscode/types/shared-lists.js:6–20](../clients/vscode/types/shared-lists.js#L6) |
| Current Neovim picker | Only fallbackView=true; multi/freeText/description/detail/group/preselection=false; maxItems=100 | **VERIFIED** [clients/neovim/lua/magit-lsp/config.lua:109–117](../clients/neovim/lua/magit-lsp/config.lua#L109) |
| Current VSCode picker | multi/description/detail/preselection/fallbackView=true; freeText/group=false; no declared item limit | **VERIFIED** [clients/vscode/lib/session.js:59–67](../clients/vscode/lib/session.js#L59), [src/presentation/mod.rs:104–105](../src/presentation/mod.rs#L104) |

- **VERIFIED:** Named `Plain` uses Plain minimum; `RichNative`/`RichDoc` use retained Neovim/VSCode rows, respectively. Those rows omit `pick`, unlike current producers; the labels are harness profiles, not alternate protocol modes ([tests/common/editor_model/presets.rs:7–9,38–55](../tests/common/editor_model/presets.rs#L38), [tests/fixtures/current-client-capabilities.json:7–28,46–64](../tests/fixtures/current-client-capabilities.json#L7)).

#### Fixture inventory

- **VERIFIED:** Rust `RepoFixture` generates temporary repositories from ordered commits/files, staged renames/files and unstaged edits, with unborn/detached options; `init_repo` has committed ten-line `f.txt`, and `init_unborn_repo` only staged `f.txt` containing `a/b/c` lines ([tests/common/mod.rs:146–185,234–292](../tests/common/mod.rs#L146)).
- **VERIFIED:** Rust helpers isolate identity/config/dates; UI builders additionally fix UTC and disable interactive Git editors/prompts. UI repositories are generated into empty destinations, not committed repository directories ([tests/common/mod.rs:18–43](../tests/common/mod.rs#L18), [scripts/ui_lib/fixtures.py:78–113,315–333](../scripts/ui_lib/fixtures.py#L78)).

| Generated UI fixture | Contents | Evidence (all **VERIFIED**) |
| --- | --- | --- |
| `clean` | Initial commit, ten-line `f.txt` | [scripts/ui_lib/fixtures.py:156–157](../scripts/ui_lib/fixtures.py#L156) |
| `modified-two-files` | Committed ten-line f/g; unstaged standard f edits and g line-2 edit | [scripts/ui_lib/fixtures.py:21–51,160–163](../scripts/ui_lib/fixtures.py#L160) |
| `two-hunk-siblings` | 40-line f with edits at 5/35; independently modified g | [scripts/ui_lib/fixtures.py:53–58,166–175](../scripts/ui_lib/fixtures.py#L166) |
| `staged-one-file` | Committed f, standard edits staged | [scripts/ui_lib/fixtures.py:178–181](../scripts/ui_lib/fixtures.py#L178) |
| `staged-and-unstaged` | Standard f edits staged, g edits unstaged | [scripts/ui_lib/fixtures.py:184–188](../scripts/ui_lib/fixtures.py#L184) |
| `staged-failing-hook` / `staged-slow-hook` | Staged f edits; rejecting pre-commit hook, immediate / one-second delay | [scripts/ui_lib/fixtures.py:191–220](../scripts/ui_lib/fixtures.py#L191) |
| `untracked` / `unborn` | Initial f plus untracked new.txt / staged ten-line f without commits | [scripts/ui_lib/fixtures.py:223–230](../scripts/ui_lib/fixtures.py#L223) |
| `in-progress-rebase` | Divergent topic/main edits to f line 2; conflicting rebase | [scripts/ui_lib/fixtures.py:233–244](../scripts/ui_lib/fixtures.py#L233) |
| `paused-linear-rebase-todo` | Initial plus four topic commits; stopped at first edit, three pending entries | [scripts/ui_lib/fixtures.py:247–259](../scripts/ui_lib/fixtures.py#L247) |
| `linear-rebase-history` | Base plus one/two/three commits adding corresponding files | [scripts/ui_lib/fixtures.py:262–265](../scripts/ui_lib/fixtures.py#L262) |
| `instant-rebase-history` | Linear history plus staged sentinel in two; registered builder, absent from public `FIXTURES` tuple | [scripts/ui_lib/fixtures.py:60–75,268–271,309](../scripts/ui_lib/fixtures.py#L268) |
| `in-progress-cherry-pick` | Divergent side/main line-2 edits; conflicting cherry-pick | [scripts/ui_lib/fixtures.py:274–284](../scripts/ui_lib/fixtures.py#L274) |
| `log-history` | Three f revisions/subjects, topic at HEAD, v1 at HEAD~1 | [scripts/ui_lib/fixtures.py:287–293](../scripts/ui_lib/fixtures.py#L287) |

- **VERIFIED:** Committed `tests/fixtures/lsp_traces/initialize-*.json` are retained initialize declarations/client identities for plain/bundled Neovim 0.12.6 and Helix 25.07.1, consumed rather than regenerated by tests ([tests/quirks.rs:7–19,24–30](../tests/quirks.rs#L7), [tests/common/editor_model/presets.rs:18–26](../tests/common/editor_model/presets.rs#L18)).
- **VERIFIED:** Committed `current-client-capabilities.json` contains Neovim/VSCodium identity, Rich declarations and capture/producer provenance; presets load it, not a live client capture ([tests/fixtures/current-client-capabilities.json:3–38,42–74](../tests/fixtures/current-client-capabilities.json#L3), [tests/common/editor_model/presets.rs:47–55](../tests/common/editor_model/presets.rs#L47)).
- **VERIFIED:** Committed text goldens cover standard/vim manifests and generated VSCode bindings, plus commit-menu layouts at widths 60/140; tests render fresh output for comparison ([tests/manifest.rs:41–68](../tests/manifest.rs#L41), [tests/menu.rs:194–200](../tests/menu.rs#L194)).

#### Choosing assertion values

- **VERIFIED — contract-derived:** Capability expectations come from minimum/client declarations; display lengths from renderer literals; Reword tree/index preservation and read-only plan preparation have direct state assertions ([tests/common/editor_model/mod.rs:82–86](../tests/common/editor_model/mod.rs#L82), [src/log_view.rs:333](../src/log_view.rs#L333), [src/rebase_todo_view.rs:46](../src/rebase_todo_view.rs#L46), [tests/editor_bridge.rs:458–461](../tests/editor_bridge.rs#L458), [tests/rebase_interactive.rs:90–101](../tests/rebase_interactive.rs#L90)).
- **VERIFIED — observed outputs:** Captured declaration values, exact rendered golden text and Git-produced hashes/state are observations, not universal constants; tests compare generated manifests to goldens, capture baseline HEAD/tree/index, and compare independently built UI fixtures for determinism ([tests/common/editor_model/presets.rs:18–35](../tests/common/editor_model/presets.rs#L18), [tests/manifest.rs:43–45](../tests/manifest.rs#L43), [tests/ui/scenarios/commit-head-operations.scn:7,23](../tests/ui/scenarios/commit-head-operations.scn#L7), [tests/ui/test_fixtures.py:73–86](../tests/ui/test_fixtures.py#L73)).
- **VERIFIED:** A `40`-character assertion is fixture-specific, whereas production accepts 40/64; exact hunk-header strings are Git-output expectations for the chosen fixture, not wire contracts ([tests/instant_rebase.rs:243–252](../tests/instant_rebase.rs#L243), [src/git.rs:1855](../src/git.rs#L1855), [tests/ui/test_fixtures.py:106–117](../tests/ui/test_fixtures.py#L106)).

#### Windows-portable test premises

- **Never string-compare our canonical output against `std::fs::canonicalize` cross-API:** Windows may retain an 8.3 short component where `canonicalize` returns the long name, so gate the assertion `#[cfg(unix)]` or compare our form to our form ([src/path.rs:9–16](../src/path.rs#L9)).
- **Never feed `std::fs::canonicalize` output to Git or session state:** use the shared `git_compatible_absolute` / `canonical_or_raw` helpers instead, so production and fixtures agree on the non-verbatim form Git for Windows accepts ([tests/common/mod.rs:34](../tests/common/mod.rs#L34), [src/path.rs:25](../src/path.rs#L25)).
- **Fixture names must pass `windows_path_problem`:** the runtime guard rejects reserved characters, device names and trailing dots or spaces in every segment at the point a name is recorded; `make check-test-premises` additionally scans `tests/**/*.rs` and `src/**/*.rs` for ungated `canonicalize` calls and reserved literals in `join`/`with_file_name`/`rename`/`prefix` positions, using the same tables parsed from the guard ([tests/common/mod.rs:65](../tests/common/mod.rs#L65), [scripts/check_test_premises.py](../scripts/check_test_premises.py)).
- **Test Windows-only behavior on Linux with synthetic Windows-shaped inputs** (`src/path.rs:67`, `src/file_uri.rs`, [tests/git_backend.rs:1830](../tests/git_backend.rs#L1830)); real Windows behavior is verified only by the CI native-test lanes ([§8.7](#87-ci)).

### 8.3 Other Rust layers

- **Patch engine:** `proptest` — random file pairs and random selections;
  `git apply --cached --check` passes; staging everything selected gives the
  expected index; stage-then-unstage is identity; selections containing context
  or header lines never produce a corrupt patch (IMM-2).
- **Fixture matrix:** renames, copies, mode changes, symlinks, binary, CRLF,
  no trailing newline, spaces/unicode/quote paths, unborn HEAD, detached HEAD,
  merge conflicts, in-progress rebase/merge/cherry-pick, worktrees, submodules,
  empty repo, large diffs, changes 4 lines apart and 20 lines apart (hunk
  merging, IMM-5).
- **Keymap:** no duplicate keys within a scope chain; every command has a doc
  and a binding in each profile (R5); manifest snapshot per profile; text-mode
  keymap has only the toggle; workbench deny-list; key-notation round trips
  for every translator.
- **Config:** include order, relative/absolute/`~`/env expansion, optional
  includes, cycle detection, array-replace/table-merge, error messages with
  file and line.
- **Document model:** stable Node IDs across refresh; cursor successor;
  stale-action rejection; lazy bodies not computed while collapsed.
- **Menus:** render snapshots at several widths; switch/option/action flows
  through `ScriptedUi`; `Planned` entries show the message and change nothing.
- **Editor bridge/askpass:** fake `GIT_EDITOR` flow end to end through the
  socket helper; client disconnect aborts Git.
- **Error mapping:** one test per row of the IMM-3 table.

### 8.4 Colour and presentation tests (T028)

Test token names and role coverage. Add a Rust token-response test for a fixture,
client data tests that map each legend type to a Neovim link or VS Code scope,
and one ANSI smoke test per client using one colour scheme. The smoke test checks
that added and removed cells have different foreground colours. Do not test
contrast, multiple schemes, theme changes, or HTML rendering. `:TOhtml` cannot
see semantic-token namespaces; the historical IMM-8 spike is not acceptance.

### 8.5 Protocol-level tests

A few tests spawn the real binary over stdio: `initialize` capabilities,
`view/open` → `view/render`, `view/key` round trip, ack/resync, prompt request
and response, standard LSP requests (`documentSymbol`, `semanticTokens`).

### 8.6 Neovim integration (kept, few)


Attach and no orphan processes; atomic keymap replace (old keys gone, new
present, nothing between); visual-selection forwarding; fold-key mirroring
(including a user `nnoremap <Tab> za`); `passThrough` of an unbound key;
read-only enforcement (IMM-4); text-mode toggle; error notifications (IMM-3);
prompt effects via stubbed `vim.ui.input`/`vim.ui.select`; the editor bridge
opens the commit-message buffer in the same instance; bottom menu placement.

### 8.7 CI

The Linux jobs in `.github/workflows/ci.yml` run their command bodies inside the
repository's locked Nix development shell (`nix develop --no-update-lock-file -c
...`), installed from `flake.lock`. Rustc, rustfmt, StyLua, cargo-llvm-cov,
Node.js, pnpm, Neovim, tmux and shellcheck therefore come from one pinned
nixpkgs revision instead of separate toolchain/action installs. The jobs cover
formatting, the release contract, Rust/Lua/VSCode tests, Rust coverage,
documentation and UI scenarios; `make vscode-test` still installs the locked
VSCode dependencies from `pnpm-lock.yaml`. The Windows/macOS `native-test` lanes
deliberately keep each runner's native toolchain: Nix has no native Windows
support and those lanes exist to cover native OS behavior, so they make no Nix
coverage claim. `release.yml` also stays outside the shell, because it builds
on Ubuntu 22.04 with its fixed Rust 1.94.1 toolchain to preserve the glibc
baseline; `publish.yml` only validates artifacts that workflow already built.

### Rust line coverage

`make coverage-rust` measures Rust-only line coverage through
`cargo-llvm-cov`: it runs the ordinary Rust test suite with LLVM
instrumentation, including the stdio tests that spawn the real server, and
writes an LCOV report to `build/coverage/lcov.info` (ignored with `build/`).
The invocation is `cargo llvm-cov --lcov --output-path build/coverage/lcov.info
--ignore-filename-regex=/tests/`. Workspace dependencies are excluded by
default and the Rust test harnesses are filtered out, so the report describes
server source rather than tests or the editor clients. There is no threshold:
a coverage number never gates a change.

Prerequisites are `cargo-llvm-cov` plus `llvm-cov`/`llvm-profdata` from the same
LLVM release as the active `rustc`; `rustc -vV` prints that release, and the
Nix dev shell exports the matching pair as `LLVM_COV`/`LLVM_PROFDATA` and also
provides `cargo-llvm-cov`. Outside the shell, set both variables to a matching
pair (for example the rustup `llvm-tools-preview` component). No compiler
version is hardcoded. Missing tools fail before tests run, naming the
prerequisite; the target never installs anything.

The full `cargo-llvm-cov` output is retained at
`build/coverage/cargo-llvm-cov.log`. On success, when it contains a recognized
warning diagnostic (the same structural `WARN_RE` heuristic the Make wrapper
uses), the script forwards the matching lines (up to 20) so the wrapper keeps
its log and reports `warnings detected`. A failing suite propagates: the script
exits nonzero, prints the failure tail and the retained command log, and the
target presents no coverage summary. It does not misreport a test failure as a missing tool or LLVM
mismatch. `make test-rust` remains the ordinary test gate, and ordinary
`make test` stays uninstrumented.

CI publishes this Rust-only measurement from a dedicated Linux `coverage` job
in `.github/workflows/ci.yml`. The job runs inside the same locked Nix dev shell
as local use, whose `cargo-llvm-cov` and exported `LLVM_COV`/`LLVM_PROFDATA`
pair come from the nixpkgs revision in `flake.lock`, so they always match the
shell's rustc; the job then runs the same `make coverage-rust` target. It never
calls `cargo llvm-cov` with different arguments or adds `--ignore-run-fail`.
`build/coverage/lcov.info`, `build/coverage/cargo-llvm-cov.log` and
`build/coverage/summary.txt` are uploaded as the `rust-coverage` artifact even
when the job fails, so a failing suite still leaves the LCOV and command log for
inspection while the job itself stays red.

The upload to Codecov is scoped to Rust with `files:
build/coverage/lcov.info`, `flags: rust` and `name: rust-linux`. It requires the
`CODECOV_TOKEN` repository secret; Codecov upload is therefore inactive until a
maintainer enables the repository there and stores that secret. Without the
secret (and always for fork pull requests, where secrets are not exposed) the
upload step is skipped and the artifact remains the published result. The job
requests only `contents: read` and the action fails the job on a real upload
error (`fail_ci_if_error: true`). There is no README badge; it stays deferred
until the upload is verified green upstream.

The Rust suite currently has known-baseline replay failures (see the queue's
COV-RUST entries), so this job reports them honestly instead of suppressing
them; `make test-rust` and this job share the same pass/fail gate.

### Logged Make targets and retained warning logs

Most Make targets wrap their command in the `run` macro (`Makefile`). By
default its stdout/stderr is captured to `build/test-logs/<target>.XXXXXX`. A
failing command prints the last `LOG_TAIL` lines plus the full-log path and
keeps the log; `V=1` bypasses capture and streams the command directly. A
successful command normally prints `ok: <target>` and deletes its log. If the
captured output contains a recognized warning diagnostic, the command still
exits 0 but the full log is **kept** and the macro prints
`ok: <target> (warnings detected; check log: <path>)` so the warnings can be
inspected; the printed path is the retained log.

Detection is a deliberately structural heuristic (`WARN_RE` in the Makefile),
not semantic analysis. It matches a line beginning with `warning:`/`Warning:`,
a compiler-style `file:line[:column]: warning:` line, or a Python
`CategoryWarning:` line with or without a `file:line:` prefix. Incidental prose
that merely contains the word "warning" (for example `0 warnings` or
`warning count zero`) does not match. Limits: a sentence that *looks* like a
diagnostic (for example one starting `Warning:`) is a possible false positive,
and a warning in an unrecognized format can be missed (false negative). The
behavior and these boundaries are exercised by `make wrapper-check`, which
drives the real `run` macro through temporary included Makefiles.

### Documentation maintenance

Behavior tests cover runtime behavior and generator/checker logic using
temporary fixtures. Checkout freshness for generated references and authored
Markdown links are separate maintenance checks: `make docs-artifacts-check`
checks committed outputs, and `make docs-check` runs checker unit tests plus
link validation. Ordinary `make test` does not regenerate or check artifacts.

Generated references live in standalone files linked from authored docs. Produce
them with `make docs-generate` (schema and registry references) or the other
generation targets, never by hand. Commit them separately from implementation,
authored documentation and queue bookkeeping, with a message naming the changed
source or contract and the generation command. Inspect the generated diff before
committing, and never regenerate merely to silence a failing behavior test.
The Nix development shell supplies Node.js and pnpm. VSCode TypeScript and
declarations remain owned by `clients/vscode/pnpm-lock.yaml`; VSCode Make targets
install locked dependencies when needed, using the shared pnpm store.

`make vscode-version-startup` and `make vscode-version-warning` both run the
shared real VSCode startup-and-visible-warning check against an isolated
extension fixture with version `9999.0.0`. The check asserts visible native
notification text against the fixture version, initialized server version and
selected executable, then records usable status text in
`version-warning-evidence.json`. Evidence records rendered notification DOM text
through text-only CDP `Runtime.evaluate`; artifact from a successful focused run
is retained under its printed `/tmp/opencode/vscode-review-*` directory.

### 8.8 Real-UI tests (L3)

L3 drives a real Neovim inside tmux and asserts on the captured screen. It
catches what the other layers cannot: timing, chord timeouts, key encoding,
the command line and real colours.

| Layer | Tool | Use it for |
| --- | --- | --- |
| L1 | Rust `TestSession` | Menu content, pending-chord state, view-stack depth, messages, token attributes |
| L2 | mini.test child Neovim | Window counts, floats, keymaps applied, buffer options, `:messages` |
| L3 | Real Neovim in tmux | What the user sees: timing, chord timeouts, key encoding, the command line, real colours |

- `scripts/ui start <fixture>` builds a deterministic repo (§8.2 environment)
  and opens the client in a dedicated tmux server; `keys`, `wait`, `screen`,
  `state`, `trace` and `stop` form the "inspect, act, inspect" loop described
  in `AGENTS.md`.
- `clients/neovim/lua/magit-lsp/debug.lua` exposes `snapshot()` (windows, view stack, pending
  chord, render versions, keymap, messages) and `idle()`; `MAGIT_LSP_TRACE=<path>`
  records per-hop timings for `ui trace`.
- Scenarios are `.scn` files in `tests/ui/` using the DSL in
  `ui-test-plan.md` §A.5. Artefacts land in `build/ui/<scenario>/` as
  `.screen` and `.ansi`.
- **Never sleep:** poll `idle()` with `ui wait`.
- A known bug is an `xfail: <bug id>` scenario. The runner reports XFAIL, and a
  passing `xfail` fails the run until the tag is removed.
- `make ui-test` runs every scenario; `make ui-tour` writes
  `build/ui/tour.md`; `make ui-unit` runs the harness's own Python tests.
- Each scenario runs once by default at `80x24` with the `standard` profile.
  Add `matrix: true` to opt into all four combinations of `80x24`/`100x30` and
  `standard`/`vim`. An explicit `size` or `profile` selects that value first
  (and, with `matrix: true`, its other standard matrix value is also run).
  Existing `size-only: true` and `profile-only: true` headers restrict matrix
  expansion to the explicitly selected size or profile, respectively.
