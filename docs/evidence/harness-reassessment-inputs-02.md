#### `Makefile` — complete (45 lines)

```text
   1: .PHONY: build test test-rust test-lua test-file ui-unit ui-test ui-tour color-spike vscode-test
   2:
   3: .SILENT:
   4: MAKEFLAGS += --no-print-directory
   5: export CARGO_TERM_COLOR=never
   6: export CARGO_TERM_PROGRESS_WHEN=never
   7: LOG_TAIL ?= 150
   8:
   9: define run
  10: @if [ -n "$(V)" ]; then $(2); else mkdir -p build/test-logs || exit $$?; log=$$(mktemp build/test-logs/$(1).XXXXXX) || exit $$?; if $(2) >"$$log" 2>&1; then status=0; printf 'ok: $(1)\n'; rm -f "$$log"; else status=$$?; tail -n $(LOG_TAIL) "$$log"; printf 'FAILED: $(1) (full log: %s)\n' "$$log"; fi; exit $$status; fi
  11: endef
  12:
  13: build:
  14: 	$(call run,build,cargo build -q)
  15:
  16: # `make test` covers the Rust, Neovim and VSCode suites. The real-UI Python
  17: # suites are separate: `make ui-unit` (harness) and `make ui-test`/`make ui-tour`
  18: # (scripted Neovim screens) must be run when touching the UI harness or
  19: # user-visible rendering.
  20: test: build test-rust test-lua vscode-test
  21:
  22: vscode-test: build
  23: 	$(call run,vscode,npm --silent --prefix clients/vscode test)
  24: 	$(call run,keybindings,node clients/vscode/scripts/gen-keybindings.mjs --check)
  25:
  26: test-rust:
  27: 	$(call run,rust,cargo test -q $(FILTER))
  28:
  29: test-lua: build
  30: 	$(call run,lua,nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run()")
  31:
  32: test-file: build
  33: 	$(call run,lua,nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run_file('$(FILE)')")
  34:
  35: ui-unit:
  36: 	$(call run,ui-unit,PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/ui -p 'test_*.py')
  37:
  38: ui-test:
  39: 	$(call run,ui-test,PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run $(if $(V),-v) tests/ui/scenarios/*.scn tests/ui/smoke.scn)
  40:
  41: ui-tour:
  42: 	$(call run,ui-tour,PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run $(if $(V),-v) tests/ui/tour.scn tests/ui/tour-clean.scn tests/ui/tour-error.scn --markdown build/ui/tour.md)
  43:
  44: color-spike: build
  45: 	$(call run,color-spike,nvim --headless --noplugin -u ./scripts/minimal_init.lua -l ./scripts/color_spike.lua)
```

#### `tests/ui/plain_init.lua` — complete (32 lines)

```text
   1: -- Standalone standard-LSP init for the Plain native smoke. No project Lua modules.
   2: vim.opt.swapfile = false
   3: vim.opt.undofile = false
   4: vim.opt.shortmess:append('I')
   5: vim.g.plain_smoke_rich_notifications = 0
   6: vim.lsp.log.set_level('trace')
   7: vim.lsp.handlers['view/render'] = function()
   8:   vim.g.plain_smoke_rich_notifications = vim.g.plain_smoke_rich_notifications + 1
   9: end
  10:
  11: local repo = vim.env.PLAIN_REPO
  12: local server = vim.env.PLAIN_SERVER
  13: assert(repo and server)
  14:
  15: vim.api.nvim_create_autocmd('VimEnter', {
  16:   once = true,
  17:   callback = function()
  18:     local capabilities = vim.lsp.protocol.make_client_capabilities()
  19:     -- Stock Neovim executes server commands but omits this optional capability
  20:     -- object by default. Declare static support, not dynamic registration.
  21:     capabilities.workspace.executeCommand = { dynamicRegistration = false }
  22:     local client_id = vim.lsp.start({
  23:       name = 'magit-lsp-plain-smoke',
  24:       cmd = { server },
  25:       root_dir = repo,
  26:       capabilities = capabilities,
  27:     })
  28:     assert(client_id, 'vim.lsp.start did not create a client')
  29:     vim.g.plain_smoke_client_id = client_id
  30:     vim.lsp.buf_attach_client(0, client_id)
  31:   end,
  32: })
```

#### `ui-test-plan.md` A.5 — verbatim complete section (source lines 320-353; next same-level heading at 357)

```text
 320: ### A.5 Scenario DSL
 321:
 322: Plain text, one statement per line, `#` comments. Header before `---`:
 323:
 324: ```
 325: fixture: modified-two-files
 326: size: 100x30            # runner also runs 80x24 unless "size-only: true"
 327: profile: vim            # runner also runs "standard" unless "profile-only: true"
 328: xfail: bug-2            # optional; see Rules
 329: env: MAGIT_LSP_PLACEMENT=vsplit   # optional extra Neovim environment
 330: ---
 331: keys: m ?               # space-separated keys, each sent separately
 332: goto "+line3 CHANGED"   # move the cursor to the first line containing the text
 333: wait                    # until idle
 334: expect screen contains "Transient and dwim commands"
 335: expect screen not-contains "Committing"
 336: expect screen matches /Unstaged changes \(\d+\)/
 337: expect windows.floating == 1
 338: expect view_stack.menus == 1
 339: expect pending == "-"
 340: expect no_message /W10|readonly/
 341: expect style "(--all)" dim              # dim | bold | coloured | plain
 342: expect cursor on "Unstaged changes"
 343: expect latency("?") <= 100ms            # key-to-idle for the last key named
 344: snapshot: name                          # writes build/ui/<scenario>/<name>.screen and .ansi
 345: ```
 346:
 347: `style` predicates use the ANSI decoder on the run containing the quoted text. `latency` uses the trace. A failed `expect` prints the expectation, the actual value, and the full current screen, then continues to the next scenario.
 348:
 349: `goto "<text>"` moves the cursor to the first character of the first buffer line containing the quoted text (a literal match, no regex). It fails when the text is absent. It is an RPC call rather than a key press, so it neither appears in the trace nor contributes to a `latency` measurement; scenarios use it to name a target instead of counting motion keys.
 350:
 351: `sh: <command>` runs a POSIX shell command in the fixture repo with the deterministic Git environment and `/dev/null` stdin, between key batches. Scenarios use it to change the repository outside Neovim (the `auto-refresh` watcher test modifies a tracked file this way); any direct Neovim RPC command must include `--headless`. A failed, timed-out or unlaunchable shell step is reported with its statement number, command and bounded detail as a failed combination; remaining size/profile combinations still run, and that run is always stopped.
 352:
 353: `latency` always reports the measured key-to-idle time. Budgets are multiplied by `UI_BUDGET_SCALE`, and an over-budget measurement only fails the run when `UI_STRICT_LATENCY=1`; otherwise it is informational.
```

#### `tests/common/session.rs` pump/dispatch signatures and implementations

Signatures and function index are source-verbatim; implementation excerpts
show the actual production entry points and render/focus bookkeeping.

```text
--- source lines 1-9 ---
   1: //! Shared in-process [`TestSession`] harness.
   2: //!
   3: //! The harness owns one real [`Session`], the focused view, the cursor and the
   4: //! scripted prompt UI. It always drives the production `view/key` and
   5: //! `view/invoke` paths, remembers the last render version it applied and sends
   6: //! that version with every key request, so tests never hard-code a version.
   7: //!
   8: //! Cursor targets are resolved through the server tree (`line_nodes`, `path`,
   9: //! `marker`, `text`, `copy`); rendered text is never parsed to find structure.
--- source lines 56-58 ---
  56: /// One in-process session driven like a client: key routing, invokes, acks and
  57: /// prompt answers all go through the real [`Session`] entry points.
  58: pub struct TestSession {
--- source lines 76-103 ---
  76: impl TestSession {
  77:     /// A session for `repo` with the profile's keymaps and no open view.
  78:     pub fn new(repo: impl Into<String>, profile: Profile) -> Self {
  79:         Self::with_config(
  80:             repo,
  81:             Config {
  82:                 profile,
  83:                 ..Default::default()
  84:             },
  85:         )
  86:     }
  87:
  88:     /// A session for `repo` with an explicit configuration.
  89:     pub fn with_config(repo: impl Into<String>, config: Config) -> Self {
  90:         let mut session = Session::new(repo);
  91:         session.set_keymaps(Arc::new(
  92:             keymap::validate(&config).expect("resolve keymaps"),
  93:         ));
  94:         Self {
  95:             session,
  96:             config,
  97:             view: "v1".to_string(),
  98:             cursor: Cursor { line: 0, column: 0 },
  99:             versions: BTreeMap::new(),
 100:             ui: ScriptedUi::new(),
 101:             key_version: 0,
 102:         }
 103:     }
--- source lines 229-247 ---
 229:     pub fn ack(&mut self) {
 230:         self.ack_view(&self.view.clone());
 231:     }
 232:
 233:     /// Acknowledge the last applied render of `view_id`.
 234:     pub fn ack_view(&mut self, view_id: &str) {
 235:         let version = self.version_of(view_id);
 236:         self.session
 237:             .ack(view_id, version)
 238:             .unwrap_or_else(|error| panic!("ack {view_id} at version {version}: {error}"));
 239:         assert_eq!(
 240:             self.acked_version_of(view_id),
 241:             Some(version),
 242:             "ack of {view_id} at version {version} did not take"
 243:         );
 244:     }
 245:
 246:     /// The cursor used by [`Self::press`] and [`Self::invoke_here`].
 247:     pub fn cursor(&self) -> Cursor {
--- source lines 448-483 ---
 448:     pub fn press(&mut self, keys: &str) -> InvokeResult {
 449:         let steps: Vec<&str> = keys.split_whitespace().collect();
 450:         assert!(!steps.is_empty(), "press needs at least one key step");
 451:         let mut last = None;
 452:         for step in steps {
 453:             last = Some(self.press_full(step, EditorMode::Normal, self.cursor, None));
 454:         }
 455:         last.expect("at least one step")
 456:     }
 457:
 458:     /// Press `key` at `cursor` with the given editor mode and selection.
 459:     pub fn press_full(
 460:         &mut self,
 461:         key: &str,
 462:         mode: EditorMode,
 463:         cursor: Cursor,
 464:         selection: Option<Selection>,
 465:     ) -> InvokeResult {
 466:         let view = self.view.clone();
 467:         let version = self.version_of(&view);
 468:         self.key_version = version;
 469:         let result = self
 470:             .session
 471:             .key(&ViewKeyParams {
 472:                 view_id: view,
 473:                 version,
 474:                 key: key.to_string(),
 475:                 editor_mode: mode,
 476:                 cursor,
 477:                 selection,
 478:             })
 479:             .expect("view/key");
 480:         self.track_result(&result);
 481:         result
 482:     }
 483:
--- source lines 543-582 ---
 543:     pub fn invoke(&mut self, command: &str) -> InvokeResult {
 544:         self.invoke_full(command, None, None, None)
 545:     }
 546:
 547:     /// Invoke `command` with JSON arguments and no cursor.
 548:     pub fn invoke_args(&mut self, command: &str, args: Value) -> InvokeResult {
 549:         self.invoke_full(command, None, None, Some(args))
 550:     }
 551:
 552:     /// Invoke `command` at `cursor`.
 553:     pub fn invoke_at(&mut self, command: &str, cursor: Cursor) -> InvokeResult {
 554:         self.invoke_full(command, Some(cursor), None, None)
 555:     }
 556:
 557:     /// Invoke `command` at `cursor` with JSON arguments.
 558:     pub fn invoke_at_args(
 559:         &mut self,
 560:         command: &str,
 561:         cursor: Option<Cursor>,
 562:         args: Value,
 563:     ) -> InvokeResult {
 564:         self.invoke_full(command, cursor, None, Some(args))
 565:     }
 566:
 567:     /// Invoke `command` at the stored cursor.
 568:     pub fn invoke_here(&mut self, command: &str) -> InvokeResult {
 569:         self.invoke_full(command, Some(self.cursor), None, None)
 570:     }
 571:
 572:     /// Invoke `command` with a cursor and a visual selection.
 573:     pub fn invoke_selection(
 574:         &mut self,
 575:         command: &str,
 576:         cursor: Option<Cursor>,
 577:         selection: Option<Selection>,
 578:     ) -> InvokeResult {
 579:         self.invoke_full(command, cursor, selection, None)
 580:     }
 581:
 582:     /// Invoke `command`, answering any prompt from the harness's [`ScriptedUi`].
--- source lines 619-650 ---
 619:     pub fn invoke_full(
 620:         &mut self,
 621:         command: &str,
 622:         cursor: Option<Cursor>,
 623:         selection: Option<Selection>,
 624:         args: Option<Value>,
 625:     ) -> InvokeResult {
 626:         let version = self.version();
 627:         self.invoke_at_version(command, version, cursor, selection, args)
 628:     }
 629:
 630:     /// Invoke using coordinates from an explicitly observed render version.
 631:     pub fn invoke_at_version(
 632:         &mut self,
 633:         command: &str,
 634:         version: u64,
 635:         cursor: Option<Cursor>,
 636:         selection: Option<Selection>,
 637:         args: Option<Value>,
 638:     ) -> InvokeResult {
 639:         let view = self.view.clone();
 640:         let result = self
 641:             .session
 642:             .invoke(&ViewInvokeParams {
 643:                 view_id: view,
 644:                 command: command.to_string(),
 645:                 version: Some(version),
 646:                 args,
 647:                 cursor,
 648:                 selection,
 649:             })
 650:             .expect("invoke");
--- source lines 708-735 ---
 708:     /// Record a render as applied and focus a newly pushed view.
 709:     fn track_result(&mut self, result: &InvokeResult) {
 710:         if let Some(render) = &result.render {
 711:             self.apply(render);
 712:         }
 713:         for (render, _keymap) in &result.opened {
 714:             self.apply(render);
 715:         }
 716:         let focused = result
 717:             .opened
 718:             .last()
 719:             .map(|(render, _)| render.view_id.clone())
 720:             .or_else(|| {
 721:                 result.effects.iter().rev().find_map(|effect| match effect {
 722:                     Effect::FocusView { view_id } => Some(view_id.clone()),
 723:                     _ => None,
 724:                 })
 725:             });
 726:         if let Some(view_id) = focused {
 727:             self.view = view_id;
 728:         }
 729:     }
 730:
 731:     /// Record the render version applied for its view.
 732:     fn apply(&mut self, render: &ViewRender) {
 733:         self.versions.insert(render.view_id.clone(), render.version);
 734:     }
 735: }
```

Function/signature index from `grep -nE '^    pub fn (press|invoke|ack|open_view|open_status|set_mode)|^    fn (press|invoke|ack|open_view|render)' tests/common/session.rs`: constructors `new` 78, `with_config` 89; open status/view 110, 115, 126, 132, 150; ack 229, 234; `press`, `press_full`, `press_at_version`, `press_visual`, `press_ui` 448, 459, 485, 504, 520; invoke family 543-631; set_mode 656; `track_result` 709; `apply` 732; second impl `new_at` 740. The full `ClientPump` implementation is already included above in the complete `tests/plain_harness.rs` block.

#### Captured command outputs

Truecolor experiment output (actual stdout from owned-session capture; repr and
hex include the terminal line padding):

```text
exit 0 stderr
repr b'\\x1b[38;2;18;52;86mCOLOR\\x1b[39m\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n\\n'
hex 1b5b33383b323b31383b35323b38366d434f4c4f521b5b33396d0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a
has_rgb_sgr True
```

Focused repository text inventory (only names/terms; no image bytes read or opened). Excludes `.git`, generated `build`, `node_modules`, `target`, and this report itself to avoid generated artifact and recursive self-matches:

```text
$ grep -RInE 'png|screenshot|jpg|video|trace\.zip' --exclude-dir=.git --exclude-dir=build --exclude-dir=node_modules --exclude-dir=target --exclude='harness-reassessment-inputs.md' . 2>/dev/null
./REVIEW_REPORT.md:283:  - **Final native/full gate (2026-10-27):** The single final `npm --prefix clients/vscode run test:e2e && make test && git diff --check` exited 0. The default full native suite reached A06 without bypassing Enter/folding assertions. Inspected `/tmp/opencode/vscode-review-P45Bx8/a06-menu-before-quit.png`, `a06-quit.png`, and `a06-reopened.png`: Commit QuickPick painted before quit, dismissed over the real tracked file after quit with the unrelated Status tab retained, and reopened Status visibly rendered alongside that tab. `a06-evidence.json` confirms removed views `v1` and `v1:menu.commit`, preserved root `v2`, reopened root `v3`, no recursive `view/close`, no real-file Magit key dispatch, and physical `g` dispatching `common.refresh` on `v3`. `make test` reported `ok: build`, `rust`, `lua`, `vscode`, and `keybindings`. Inspected invocation producers: the suite already supplies the fourth `info` argument; both production callers (`commands.js`, `edits.js`) supply `info.cursor`, so no fallback or producer change was needed. No editor/runner for this isolated directory remained; other-session test processes were observed and left untouched.
./REVIEW_REPORT.md:284:  - **Exact ticket gate (2026-10-27):** `npm --prefix clients/vscode run test:e2e && make vscode-test && npm --prefix clients/vscode run test:integration && make test && git diff --check` exited 0. Inspected the final default-suite A06 screenshots and `a06-evidence.json` at `/tmp/opencode/vscode-review-C36z2h`; VSCode test and Node integration both passed, and `make test` reported build/Rust/Lua/VSCode/keybindings green. No owned editor/server processes remained.
./REVIEW_REPORT.md:305:  - **Completion evidence (2026-10-27):** Removed the raw-key confirm/cancel policy; edit context now uses `effect.scope`, and `currentEdit()` only resolves the focused editor document. The real-server Node integration drives commit creation, generated `common.confirm`/`common.cancel` dispatcher bindings, and verifies confirm creates the requested commit while cancel does not. Native VSCode A07 uses a dirty real bridge document to prove save-before-confirm; it separately exercises generated confirm/cancel IDs, native close cancellation, and `common.refresh` reaching the server while another edit tab remains open. Inspected `/tmp/opencode/vscode-review-3IYDyR/a07-edit-{before-confirm,confirmed,cancelled,closed}.png` and `a07-evidence.json`; the before screenshot shows the dirty Git Commit Message editor, confirm returns to status with the committed message, and cancel/close return to status without a new commit. Exact final gate `make vscode-test && npm --prefix clients/vscode run test:integration && npm --prefix clients/vscode run test:e2e && make test && git diff --check` exited 0; `make test` reported build/Rust/Lua/VSCode/keybindings green. Five earlier gate attempts failed during iteration (three staged-fixture checks and two fake-editor focus timeouts); the final run passed. Post-run process inspection found no owned VSCode/e2e/server process remaining; no process was killed.
./REVIEW_REPORT.md:614:  - **Acceptance evidence (2026-10-27):** Exact final gate `make test && make vscode-test && scripts/ui run tests/ui/scenarios/text-mode-roundtrip.scn && git diff --check` passed. `/tmp/opencode/vscode-review-04pmBr/a29-evidence.json` records metadata-only same-version entry, invoke/key/menu/setMode coverage, native selection/copy/navigation, queued refresh selection stability and applied replacement on exit, and capture=true with zero suppressed requests. Inspected real UI screenshots `a29-before.png`, `a29-during.png`, `a29-queued.png`, `a29-after.png`, and `a29-capture.png`; full gate retains Neovim semantics. A11 remains deferred as previously recorded.
./REVIEW_REPORT.md:659:  - **Native acceptance (2026-10-27):** Exact gate `cargo build && cargo test --test menu --test menus && make vscode-test && MAGIT_VSCODE_A33_ONLY=1 npm --prefix clients/vscode run test:e2e && git diff --check` exited 0 (30 Rust menu tests plus VSCode/unit/keybinding gates and native e2e). The isolated fixture configures `magit-lsp.profile = "vim"` before opening. Real `open.commit` / dispatch rows are selected with native arrow keys and Return, never typed filters or synthetic metadata. Exact view/key identities and sequences prove `-a` → `['-', 'a']` toggles the server switch and rendered `--all · off` → `on` row; `ctrl+t` → one request changes server-rendered mode to `text`; `g l` → `['g', 'l']` opens the native Log menu. Inspected all six readable `a33-{flag,modifier,spaced}-{before,after}.png` screenshots: Commit off/on, Dispatch modifier selection/list reset (text mode recorded in JSON), and Dispatch Log selection → Log menu. Retained screenshots, server metadata, exact requests, rendered lines and modes in `/tmp/opencode/vscode-review-kiTL7h/a33-evidence.json`. Earlier wrong-menu and initial-render/selection races are resolved. Only owned host/server processes were stopped; final process check found none remaining.
./REVIEW_REPORT.md:674:  - **Native completion (2026-10-27):** The preceding progress paragraph is retained verbatim as historical evidence, relocated from A02. A34 is now complete. `cargo build && MAGIT_VSCODE_A34_ONLY=1 npm --prefix clients/vscode run test:e2e && make test && git diff --check` passed. The isolated native branch runs before unrelated widgets and fails on missing editor/display/server or `a34-evidence.json`; A29 textmode and A33 activation branches are unchanged. Sixteen real VSCode `Selection` cases use registered `magit.key` and `magit.invoke`, the rebuilt server and real Git without mutation stubs: addition/deletion × stage/unstage × colzero/nonzero ends × dispatcher, with forward/reverse ranges, context plus adjacent changed rows and independently asserted active cursors. Server `documentSymbol` hunk metadata locates fixture rows; exact fresh requests (including version, inclusive selection and cursor) and base64 HEAD/index/worktree/expected-index bytes are retained in `/tmp/opencode/vscode-review-M0B3HF/a34-evidence.json`. FIRST alone or FIRST+SECOND changes, while SECOND/GUARD or GUARD respectively remains untouched; every worktree stays byte-identical. Thirty-two readable before/after screenshots are retained there; inspected `a34-add-stage-colzero-key-{before,after}.png` shows FIRST alone staged with SECOND/GUARD still unstaged, and `a34-delete-unstage-nonzero-invoke-{before,after}.png` shows FIRST/SECOND deletions unstaged with GUARD deletion still staged. Native document and painted Monaco rows are polled before screenshots, including both resulting changed sections, preventing stale pre-mutation screenshots. Full regression output: build/rust/lua/vscode/keybindings all green. Owned host cleanup and extension deactivation completed; process inspection found no remaining magit-lsp or owned isolated host processes. Native cases complement, rather than repeat, the 72 shape/240 real-Git automated cases above.
./magit-lsp-plain-client-tasks.md:6:never use vision, inspect/render images, invoke image/screenshot tools, or capture
./magit-lsp-plain-client-tasks.md:7:screenshots. Native UI verification remains real, using text/structured state
./magit-lsp-plain-client-tasks.md:133:pre-action screenshots retained and inspected. Core fallback and dirty choices
./magit-lsp-plain-client-tasks.md:252:`real-file-native-4-wrong-version-retained.png`, `real-file-native-5-unsafe-{rejected,recovered}.png`,
./magit-lsp-plain-client-tasks.md:253:`real-file-native-6-stale-probes-retained.png` and commit-result screens.
./README.md:8:[native screenshot capture/manual fallback](docs/SCREENSHOTS.md), and the
./TASKS.md:612:    trace}.json` and `real-file-native-3-discard-dialog.png` retain production
./TASKS.md:615:    never unsaved text. Additional inspected screenshots in the passing
./TASKS.md:687:    and `real-file-native-trace.json`, plus `real-file-native-2-cancel-dialog.png`
./TASKS.md:688:    and `real-file-native-3-committed.png`, verify actual native Save/Don't Save/
./TASKS.md:1403:circumstances use vision, inspect/render images, invoke image/screenshot tools,
./TASKS.md:1404:or capture screenshots. Native UI verification remains real, but uses text and
./TASKS.md:1426:#### NO-VISION-TESTS — Remove executable screenshot paths
./TASKS.md:1428:- [x] **[P0] Remove image/screenshot test paths and replace checkpoints with substantive text evidence.** Preserve real native interactions and all nonvisual assertions, and retain text snapshots.
./TASKS.md:1431:  - **Done when:** all screenshot capture and image-snapshot executable paths are removed; meaningful native checkpoints use text/structured state (accessibility/DOM text, JSON, wire/API and Git state) while preserving interaction and nonvisual assertions. No image-based acceptance or future screenshot inspection remains.
./TASKS.md:1472:  - [x] **Native reset-author acceptance (2026-10-29):** Reused Rust acceptance from `9002cbd`; typed `-R` in Neovim Reword (both profiles, both sizes) and VSCode Amend/clean Extend/staged Reword. Exact before/after author/date, message/tree/index/worktree, parents/count and applicable committer-date checks pass; real saved native closure is inspected. `cargo build --release && scripts/ui run tests/ui/scenarios/commit-head-operations.scn && MAGIT_VSCODE_BIN=/home/<user>/.nix-profile/bin/codium MAGIT_VSCODE_HEAD_ONLY=1 npm --prefix clients/vscode run test:e2e && git diff --check` exited 0. Retained Neovim `reset-author-{option,saved,result}.screen` and `reset-author-evidence.json` under all four `build/ui/commit-head-operations/{standard,vim}-{100x30,80x24}/` variants; VSCode `/tmp/opencode/vscode-review-nmM2OM/` records eight original plus three reset-author cases and 39 named screenshots. Inspected screens/JSON and verified owned cleanup. Failed fixture/assertion/schema attempts are diagnosed in `docs/evidence/commit-head-operations.md`. R-4.1-head-edit's stated typed-operation, guard, Rust state, and both-client native criteria are complete; broad gates remain failed as documented.
./TASKS.md:1533:  - **Constraints:** Do not weaken `parse_file_diff()`'s refusal to treat a multi-section type change as one text patch. Do not reconstruct the original identity from fresh Git state at invocation or prompt creation, hash the whole repository per row, parse rendered text, change bulk-command semantics, or narrow the documented whole-file contract to expanded text files. Preserve inherited release edits; use no images or screenshots.
./TASKS.md:1541:  - **Constraints:** Do not bypass stale-state validation for all untracked files, substitute the untracked section's bulk command for a file action, change unrelated dispatch semantics, add a client-side workaround, or modify inherited release edits. No images or screenshots.
./TASKS.md:1572:  - **Constraints:** Do not remove ordinal disambiguation, introduce hunk-index addressing, broaden bulk restrictions, change whole-file behavior, parse rendered text, or modify inherited release edits. No images or screenshots.
./TASKS.md:1579:  - **Constraints:** Keep this a focused backend/tree/dispatcher correction, not a generic VCS abstraction or token registry. Do not strip meaningful headers from execution, change deliberate bulk semantics, weaken whole-file validation, or introduce full-file identity checks that invalidate harmless unique-hunk shifts. Preserve inherited edits; no images or screenshots.
./TASKS.md:1751:  - **Inspected native evidence:** `/tmp/opencode/vscode-review-uDwXKm/real-file-native-evidence.json` and `real-file-native-trace.json`; screenshots `real-file-native-{save-alone,two-groups,nonlast-close,cancel-retained}.png`, `real-file-native-2-{cancel,save}-dialog.png`, `real-file-native-3-discard-dialog.png`, and `real-file-native-3-committed.png`. Save alone, switching/focus, language change and nonlast two-group close send no confirmation/change no HEAD. Cancel retains the dirty tab/run. Saved last-close commits exact subject/body `native saved message\n\nexact body` (HEAD `14b8cf0d72162ded52943209cc9e841138dc7b3a`); native Save commits `native Save new bytes\n\nnew body` (`01f0e865e4080264e53f64b04177d8167a929e07`); Don't Save commits `native PREVIOUS saved bytes\n\nold body` (`48eb44fbaf3427d68ab696a7c53b5382dec120c4`) despite verified unsaved full-sync version 3. Each run issues exactly one saved-file confirmation/commit, and subsequent runs have fresh Git text and distinct run IDs. Owned native host/server processes were absent after cleanup. No synthetic didClose, observer-spike substitution or shutdown confirmation; history removal is a separate pending slice and parent R-EDIT-REAL remains unchecked.
./TASKS.md:1764:  - [x] **VSCode native HEAD-only slice (2026-10-29):** `MAGIT_VSCODE_BIN=/home/<user>/.nix-profile/bin/codium MAGIT_VSCODE_HEAD_ONLY=1 npm --prefix clients/vscode run test:e2e && git diff --check` exited 0. Normal production host, real typed commit-menu options/actions and saved native last-tab closures cover staged/clean unchanged Amend, exact edited Reword with nonempty index/tree/date retained, no-editor Extend with exact message/date, tracked-only `--all`, clean guard/allow-empty and exact-state explicit abort. All replacements retain original parents/count, not a new descendant. Machine-readable state/requests and 26 screenshots are retained in `/tmp/opencode/vscode-review-tAX9dE/`; inspected native evidence and harness-only failure diagnoses are detailed in `docs/evidence/commit-head-operations.md`. No production changes, synthetic didClose/dialogs, added sleeps or deadline inflation. Owned host/server processes stopped; R-4.1-head-edit stated client criteria complete; broad gates remain failed as documented.
./TASKS.md:1938:  - [x] **R-EDIT-REAL multi-root native isolation slice (2026-10-29).** EDIT_ONLY's normal live Codium host owns two canonical repository sessions (`v1`/`v2`) and simultaneous distinct runIds/URIs/messages. Both-direction run/URI swaps and independent-connection owner confirmations are labelled semantic protocol probes (synchronized and saved-file), rejected with exact per-root HEAD/tree/index/worktree/count/message/status and pending disk/live text unchanged. Real saved last-tab close commits A exactly once (count 7→8), while B stays open/pending with count 1 and unchanged exact state. Production `magit.cancelEdit` aborts B, preserves both roots and B's file, then actual reopen/late tab close sends no new confirmation or commit. Inspected all four `real-file-native-two-repository-*.png` and before/after/request evidence in `/tmp/opencode/vscode-review-S81D41/real-file-native-{trace,evidence}.json`; B's staged delta remains visible after Abort (the existing Git-abort failure toast is not a B commit). **Setup root cause:** explicit second-root `view/open`/production invoke already routed correctly; clean Create was refused before any helper/draft. Stage independent deltas in both roots, not longer deadlines. A second workbench LanguageClient attempt collided with registered `magit.command`; independent-owner probes instead use a labelled stdio connection, shut down and awaited in cleanup. No production/protocol change, reload/deactivation, fabricated native event, sleep or increased deadline. No magit-lsp helpers remained. Same-URI/late/duplicate semantic probes and native nonlast/last-tab and unsafe-read recovery count for their finite criteria; duplicate native events and universal platform guarantees are not claimed. This slice supersedes the parent's old multi-root evidence gap only; parent and broad gates remain unchecked; retained ring policy is unchanged. Verification exited 0: `MAGIT_VSCODE_BIN=/home/<user>/.nix-profile/bin/codium MAGIT_VSCODE_EDIT_ONLY=1 npm --prefix clients/vscode run test:e2e && git diff --check`.
./TASKS.md:1982:  - **Evidence:** `npm --prefix clients/vscode run test:integration` and `npm --prefix clients/vscode run test:e2e` pass against the real server; real native `r-edit-5-*.png` screenshots and `r-edit-5-evidence.json` / `r-edit-5-disconnect.json` inspected; save-stays-editing, confirm, abort, dirty Save/Discard/Cancel, ambiguous tab close, progress cancel, version-error recovery, history recall and return-to-current-draft covered; owned UI stopped; `git diff --check` clean (see §3.10).
./TASKS.md:2203:- [ ] **R-REL-2 — Client install and screenshots**
./TASKS.md:2206:  discovery. Add cheap deterministic CI screenshot capture for both real
./TASKS.md:2215:  screenshot assets or manual fallback are usable; client spec is linked and
./TASKS.md:2218:  screenshot job checked for both harnesses; spec link/consumption review.
./TASKS.md:2222:    discovery for both clients, and usable screenshot capture/manual-fallback
./TASKS.md:2225:    clean-machine and screenshot evidence. Link `docs/CLIENT_SPEC.md` and welcome
./TASKS.md:2229:    server setup and existing native screenshot/manual-capture paths. All 13
./TASKS.md:2236:    release-ready menu screenshot. The lazy.nvim manager, desktop-terminal PNG
./TASKS.md:2244:  quickstart, screenshots, links and concise feature-focused roadmap. Publish
./clients/vscode/NOTES.md:9:The screenshot descriptions in the dated sections below are historical records
./clients/vscode/NOTES.md:34:`r-vs-4-theme-dark.png`, `r-vs-4-theme-light.png` and
./clients/vscode/NOTES.md:35:`r-vs-4-theme-dark-restored.png` are actual renderer captures through CDP
./clients/vscode/NOTES.md:46:| Screenshots | `r-vs-4-theme-dark.png`, `r-vs-4-theme-light.png`, `r-vs-4-theme-dark-restored.png` show the same `Status` document with painted headings, additions and deletions; dark and light screenshots visibly differ in chrome and token colours, and the restored screenshot matches the first. |
./clients/vscode/NOTES.md:94:| Arbitrary ID and opaque arguments forwarded unchanged | `magit.invoke { command: "common.hide-section", args: { recursive: true } }` produced `view/invoke` `{ viewId: "v1", command: "common.hide-section", cursor: { line: 4, column: 0 }, args: { recursive: true } }`. The server-rendered text visibly collapsed the targeted section (`r-vs-3-generic-dispatch.png`). Dispatching `common.show-section` with the same `{ recursive: true }` restored the exact previous server text (`r-vs-3-generic-restored.png`). The arguments were opaque: nothing was added, removed or rewritten. |
./clients/vscode/NOTES.md:95:| Another action through the dispatcher visibly takes effect | `magit.invoke { command: "status.stage-all" }` sent that canonical ID and moved the file: `git diff --cached --name-only` was empty before and `tracked.txt` after, and the status render visibly changed to `Staged changes (1)` / `Unstaged changes (0)` (`r-vs-3-generic-action.png`). `status.unstage-all` through the same dispatcher restored an empty index and `Staged changes (0)` (`repositoryRestored: true`). |
./clients/vscode/NOTES.md:96:| Default fold shortcut in active status | Physical `Tab` (the server-owned `common.toggle-section` default) sent exactly two `view/invoke` requests `{ viewId: "v1", command: "common.toggle-section", cursor: { line: 4, column: 0 } }`. The screen visibly folded the `Unstaged changes (1)` section (`r-vs-3-fold.png`) and restored it on the second press. Two additional requests came from the mirrored `ctrl+alt+]` binding, giving four toggles in total. |
./clients/vscode/NOTES.md:97:| Default refresh shortcut in active status | Physical `g` sent exactly one `view/invoke` `{ viewId: "v1", command: "common.refresh", cursor: { line: 4, column: 0 } }`; the server reread the worktree and rendered the new `+after-return` line in the same visible, server-owned view (`06-status-refresh.png`). The document text equalled the received server render and the URI/tab were unchanged. |
./clients/vscode/NOTES.md:99:All of these screenshots were opened and inspected: `00-status-expanded.png`,
./clients/vscode/NOTES.md:100:`r-vs-3-fold.png`, `r-vs-3-generic-dispatch.png`,
./clients/vscode/NOTES.md:101:`r-vs-3-generic-restored.png`, `r-vs-3-generic-action.png`,
./clients/vscode/NOTES.md:102:`06-status-refresh.png`, `07-magit-quickpick.png`,
./clients/vscode/NOTES.md:103:`08-f5-before-dismissal.png`, `08-f5-probe.png`, `08-f5-control-g.png`, and the
./clients/vscode/NOTES.md:104:run's `r-vs-2-*.png` files. Text and tree ownership stayed with the server:
./clients/vscode/NOTES.md:117:step, and the F5 diagnostic screenshots show no `+f5-*` content.
./clients/vscode/NOTES.md:144:traces/screenshots, a bounded retry for focusing the existing find-widget setup,
./clients/vscode/NOTES.md:168:- `r-vs-2-first-open.png` — screen captured before any reopen/expand/action.
./clients/vscode/NOTES.md:169:- `r-vs-2-reopen.png` — screen after one sequential and two concurrent opens.
./clients/vscode/NOTES.md:170:- `r-vs-2-refresh.png` — screen after the `g` refresh.
./clients/vscode/NOTES.md:179:| First open from a fresh window | `view/open` returned descriptor `id=v1`, `kind=status`, `title=Status`, server URI `magit://status/tmp/opencode/vscode-review-K6aS1m/repo`. The VSCode document is `magit://v1/Status`; its text exactly equalled the received full `view/render` text (version 1, 12 lines), including `Head: main`, `Unstaged changes (1)`, `M tracked.txt`, `-original`, `+replacement` and `Untracked files (1)`. The tab label is `Status`; exactly one status tab existed; the editor was visible and its visible ranges covered line 0. `r-vs-2-first-open.png`. |
./clients/vscode/NOTES.md:180:| Repeat/concurrent opens | One sequential `magit.open` plus two concurrent calls produced **no** additional `view/open` request (count stayed 1), one `Status` tab, the same `magit://v1/Status` URI, unchanged server text and a visible editor. `r-vs-2-reopen.png`. |
./clients/vscode/NOTES.md:181:| Refresh | `g` sent `view/invoke common.refresh`; the server returned version 2 edits with `baseVersion: 1`; the document text exactly matched the applied render chain, `+after-return` was in a visible range, and the URI/tab stayed `magit://v1/Status` / `Status`. `r-vs-2-refresh.png`. |
./clients/vscode/NOTES.md:236:| Actual client case | Physical input/result | Magit requests | Inspected screenshot |
./clients/vscode/NOTES.md:238:| Foreign QuickPick | mirrored Ctrl+Alt+], `beta g`, Enter accepts `beta g` | 0 | `01-foreign-quickpick.png` |
./clients/vscode/NOTES.md:239:| Foreign InputBox | mirrored Ctrl+Alt+], `hello foreign`, Enter accepts text | 0 | `02-foreign-inputbox.png` |
./clients/vscode/NOTES.md:240:| Find | mirrored Ctrl+Alt+], `g`, Ctrl+A, `tracked`, Enter selects match; Ctrl+A/C copies `tracked`; Escape returns | 0 | `03-find-widget.png` |
./clients/vscode/NOTES.md:241:| Restored status | Enter reaches `common.visit` (planned-action message, not file visiting) | 1 | `04-status-enter.png` |
./clients/vscode/NOTES.md:242:| Restored status | Tab twice folds/unfolds the server section | 2 toggles | `05-status-fold.png` |
./clients/vscode/NOTES.md:243:| Restored status | mirrored Ctrl+Alt+] twice folds/unfolds the server section | 2 toggles | `05b-mirrored-fold.png` |
./clients/vscode/NOTES.md:244:| Restored status | `g` rereads Git and renders `+after-return` | 1 refresh | `06-status-refresh.png` |
./clients/vscode/NOTES.md:245:| Magit commit QuickPick | `c` opens; `-`, `a` turn `--all` on; `q` pops to status | `open.commit` plus 3 `view/key` | `07-magit-quickpick.png` |
./clients/vscode/NOTES.md:246:| F5 diagnostic | no refresh request, no `+f5-probe` render | 0 | `08-f5-before-dismissal.png`, `08-f5-probe.png` |
./clients/vscode/NOTES.md:248:All listed screenshots except the redundant post-dismissal F5 image were opened
./clients/vscode/NOTES.md:249:and visually inspected, along with `00-status-expanded.png` and `screen.png`:
./clients/vscode/NOTES.md:263:in the pre-dismissal screenshot. The interception/resolver cause is unproven;
./clients/vscode/NOTES.md:500:  from the pass/fail refresh path. It also captures per-step screenshots and
./clients/vscode/NOTES.md:503:  DISPLAY/binary precondition checks, screenshot listing, and targeted cleanup
./clients/vscode/NOTES.md:526:so status-text screenshots from `import` are not valid evidence. `F5` was not
./clients/vscode/NOTES.md:530:client scenario, verify the find-widget focus path, capture the screenshots and
./clients/vscode/test/e2e/run.mjs:35:  if (/image|screenshot|snapshot/i.test(method)) {
./clients/vscode/test/no-vision.js:21:    assert(!/\b(?:Page\.)?captureScreenshot\b|\b(?:writeFileSync|writeFile|readFileSync|readFile)\s*\([^\n]*\.(?:png|jpe?g|webp)\b/i.test(source), `${file} contains image capture/artifact I/O`);
./ui-test-plan.md:7:**No-vision policy:** Never use vision, inspect/render images, invoke image or screenshot tools, or capture screenshots. This applies to every agent and delegated worker. Native UI interaction remains required where specified, but checkpoints and acceptance use text/structured evidence only: tmux `.screen`, accessibility/DOM text, JSON, wire/API and Git state. Text snapshots remain allowed. Do not add image-snapshot tests or screenshot capture paths, and do not require future image inspection. Historical evidence descriptions remain historical; they are not claims that evidence was text-based.
./ui-test-plan.md:25:| L2 | mini.test child Neovim | Window counts, floats, keymaps applied, buffer options, `:messages`. Its screenshot cannot capture the command-line area reliably (neovim#21886), so read `:messages` instead |
./ui-test-plan.md:96:  history, version-error, dirty-close, abort and disconnect screenshots plus
./docs/evidence/commit-head-operations.md:193:`head-operations-evidence.json` (pass, eight isolated cases, 26 named screenshots)
./docs/evidence/commit-head-operations.md:229:Inspected current screenshots `head-amend-clean-saved.png`,
./docs/evidence/commit-head-operations.md:230:`head-amend-staged-status.png`, `head-reword-{saved,status}.png`,
./docs/evidence/commit-head-operations.md:231:`head-extend-tracked-all-status.png`, `head-extend-clean-guard-status.png`,
./docs/evidence/commit-head-operations.md:232:`head-extend-clean-allowed-menu.png`, and `head-amend-abort-edited.png`.
./docs/evidence/commit-head-operations.md:238:status screenshots.
./docs/evidence/commit-head-operations.md:245:screenshots exposed immediate subprocess exit truncating CDP JSON; awaiting
./docs/evidence/commit-head-operations.md:322:three `resetAuthorCases` and 39 named screenshots; the trace retains native
./docs/evidence/commit-head-operations.md:324:Inspected current `head-reset-author-amend-saved.png`,
./docs/evidence/commit-head-operations.md:325:`head-reset-author-extend-menu.png`, and
./docs/evidence/commit-head-operations.md:326:`head-reset-author-reword-{saved,status}.png`: the real saved subject/body,
./docs/evidence/real-file-client-spike.md:46:Inspected all three `pre-action.png` images: native Explorer, permanent draft
./docs/evidence/real-file-client-spike.md:130:Inspected both verified hosts' API/wire traces and the normal host screenshots:
./docs/evidence/real-file-client-spike.md:131:`03c-dirty-save-dialog.png` (Save focused; two dirty native text lines),
./docs/evidence/real-file-client-spike.md:132:`03e-dirty-discard-dialog.png` (actual Save/Don't Save/Cancel prompt over
./docs/evidence/real-file-client-spike.md:133:`discarded text`), `03b-dirty-cancel.png` (dirty tab retained),
./docs/evidence/real-file-client-spike.md:134:`03d-dirty-save-closed.png` and `03f-dirty-discard-closed.png` (no draft tab),
./docs/evidence/real-file-client-spike.md:135:and `04-refresh-language.png` (reopened, refreshed dirty plaintext editor).
./docs/evidence/real-file-client-spike.md:215:Inspected the normal host's `01-open.png`, `02-native-close.png`,
./docs/evidence/real-file-client-spike.md:216:`03-dirty-dialog.png` and `04-refresh-language.png`: real file/text visible;
./docs/RELEASING.md:10:screenshot fallback are in [SCREENSHOTS](SCREENSHOTS.md). External adapters use
./docs/RELEASING.md:15:client evidence. Native screenshots are captured on demand, not by a new CI job.
./docs/RELEASING.md:42:  `a06-evidence.json`. Opened `r-vs-2-first-open.png`,
./docs/RELEASING.md:43:  `a06-menu-before-quit.png` and `a06-reopened.png` (1192x756, dark theme).
./docs/RELEASING.md:46:  not complete menu presentation or release-ready product screenshots.
./docs/CLIENT_INSTALL.md:132:published-VSIX evidence; those release gates remain open. Native screenshots
./docs/SCREENSHOTS.md:1:# Native screenshot capture and manual fallback
./docs/SCREENSHOTS.md:9:or a mock editor into a purported native screenshot. Keep original artifacts
./docs/SCREENSHOTS.md:15:is headless evidence, not native screenshot evidence or a published install.
./docs/SCREENSHOTS.md:22:desktop screenshot tool if a PNG is needed. The harness's `.screen` and `.ansi`
./docs/SCREENSHOTS.md:63:not publish a synthesized image of `.screen` as a native terminal screenshot.
./docs/SCREENSHOTS.md:91:`r-vs-2-first-open.png`, `a06-menu-before-quit.png` and `a06-reopened.png` there;
./docs/SCREENSHOTS.md:93:focus, readable layout and absence of errors. `failure.txt`/`failure-screen.png`
./docs/SCREENSHOTS.md:94:are diagnostic evidence, not successful product screenshots.
./docs/SCREENSHOTS.md:105:window with a desktop screenshot tool. Retain commands, settings, versions and
./docs/SCREENSHOTS.md:106:screenshots; report manual evidence separately from automated e2e success.
./docs/SCREENSHOTS.md:107:Do not attempt commit editing merely to capture status/menu screenshots.
./AGENTS.md:65:- **No-vision policy (mandatory for every agent, including delegated workers and subagents):** never use vision, inspect or render images, or invoke image/screenshot tools, under any circumstances. Do not capture screenshots or add image-snapshot tests/capture paths. Native UI verification remains real and interactive, but evidence and acceptance must be text/structured state only (for example `.screen`, accessibility/DOM text, JSON, wire/API and Git state). Text snapshots are allowed. Do not impose image-based acceptance or future screenshot inspection requirements.
./AGENTS.md:116:  scenario's `failure.screen` only if its inline text output is insufficient. Never use screenshot/image output.
```
(exit 0)



Color-spike path/output classification, checked against source: its header says
`tohtml` skips the semantic-token namespace and HTML is “only a reference”; the
token table, not HTML, is the result (`scripts/color_spike.lua:9-11`). The six
configured HTML cases are `default/dark`, `default/light`, `vim/dark`,
`habamax/dark`, `delek/dark`, and `lunaperche/dark` (`:256-262`); each is written
as `build/test-output/status-<scheme>-<background>.html` (`:267-288`). The
seventh intended output is `build/test-output/semantic-tokens.md` (`:388-400`),
a Markdown legend. Because setup failed at line 193 before the case loop, no
HTML or Markdown output was emitted in this attempt. The setup failure is the
only observed failure; no fallback output is claimed.

Full `scripts/color_spike.lua` function/export index from
`grep -nE '^(local function|function |local [[:alnum:]_]+ = function)'`: `sys`
24, `make_fixture` 32, `hex` 72, `resolved_name` 79, `probe_tohtml` 91,
`collect_tokens` 164, local `pad` 309. Loops/report blocks: tokens 228,
token types 237, modifiers 244, cases 267, columns/table report 305-413;
no exported module functions. Full callable index for `scripts/ui_lib/` is
reproducible with `grep -nE '^    def |^def |^class ' scripts/ui_lib/*.py`;
`cli.py` 26-199, `decoder.py` 36-212, `driver.py` 29-895, `fixtures.py` 68-262,
`paths.py` 13, `scenario.py` 35-1095, `startup.py` 26-201. Existing detailed
indexes above are excerpts; these ranges denote all indexed declarations, not
full implementation transcriptions of the long Python modules.

### Reproducible full declaration indexes

Complete callable declaration index, requested long Python helpers:

```text
$ grep -nE '^    def |^def |^class ' scripts/ui_lib/*.py
scripts/ui_lib/cli.py:26:def _in_ci() -> bool:
scripts/ui_lib/cli.py:31:def _take_run(args: list[str]) -> tuple[str | None, list[str]]:
scripts/ui_lib/cli.py:53:def _take_value(args: list[str], flag: str, default: str | None = None) -> tuple[str | None, list[str]]:
scripts/ui_lib/cli.py:75:def _start(args: list[str]) -> int:
scripts/ui_lib/cli.py:96:def _keys(args: list[str]) -> int:
scripts/ui_lib/cli.py:105:def _wait(args: list[str]) -> int:
scripts/ui_lib/cli.py:125:def _screen(args: list[str]) -> int:
scripts/ui_lib/cli.py:138:def _state(args: list[str]) -> int:
scripts/ui_lib/cli.py:147:def _trace(args: list[str]) -> int:
scripts/ui_lib/cli.py:165:def _stop(args: list[str]) -> int:
scripts/ui_lib/cli.py:173:def _run(args: list[str]) -> int:
scripts/ui_lib/cli.py:199:def main(argv: list[str] | None = None) -> int:
scripts/ui_lib/decoder.py:36:def _style(fg, bg, bold, dim, reverse) -> tuple:
scripts/ui_lib/decoder.py:40:def _apply_sgr(
scripts/ui_lib/decoder.py:97:def _extended_colour(values: list[int], index: int) -> tuple[str | None, int]:
scripts/ui_lib/decoder.py:112:def decode(text: str) -> list[dict]:
scripts/ui_lib/decoder.py:121:    def flush() -> None:
scripts/ui_lib/decoder.py:127:    def emit(chunk: str) -> None:
scripts/ui_lib/decoder.py:212:def runs_containing(runs: Iterable[dict], needle: str) -> list[dict]:
scripts/ui_lib/driver.py:29:class DriverError(RuntimeError):
scripts/ui_lib/driver.py:33:class StartupError(DriverError):
scripts/ui_lib/driver.py:41:class SkipError(DriverError):
scripts/ui_lib/driver.py:46:class Run:
scripts/ui_lib/driver.py:63:    def dir(self) -> Path:
scripts/ui_lib/driver.py:68:    def repo_path(self) -> Path:
scripts/ui_lib/driver.py:73:    def home_path(self) -> Path:
scripts/ui_lib/driver.py:78:    def socket_path(self) -> Path:
scripts/ui_lib/driver.py:83:    def trace_path(self) -> Path:
scripts/ui_lib/driver.py:87:    def save(self) -> None:
scripts/ui_lib/driver.py:93:    def load(cls, run_id: str) -> "Run":
scripts/ui_lib/driver.py:105:def check_tools() -> None:
scripts/ui_lib/driver.py:112:def parse_size(value: str) -> tuple[int, int]:
scripts/ui_lib/driver.py:123:def new_run_id() -> str:
scripts/ui_lib/driver.py:129:def _update_last_run(run_id: str) -> None:
scripts/ui_lib/driver.py:134:def latest_run_id() -> str:
scripts/ui_lib/driver.py:147:def load_run(run_id: str | None) -> Run:
scripts/ui_lib/driver.py:152:def _forget_last_run(run_id: str) -> None:
scripts/ui_lib/driver.py:166:def _tmux(run: Run, *args: str, check: bool = True) -> subprocess.CompletedProcess:
scripts/ui_lib/driver.py:179:def _nvim(run: Run, *args: str, timeout: float = 5.0) -> subprocess.CompletedProcess:
scripts/ui_lib/driver.py:189:def _expr(run: Run, expression: str, timeout: float = 5.0) -> str | None:
scripts/ui_lib/driver.py:237:def snapshot(run: Run) -> dict:
scripts/ui_lib/driver.py:248:def state_json(run: Run, indent: int | None = 2) -> str:
scripts/ui_lib/driver.py:256:def idle(run: Run) -> bool:
scripts/ui_lib/driver.py:261:def _idle_probe(run: Run) -> startup.IdleProbe:
scripts/ui_lib/driver.py:285:def _decode_output(output: str | bytes | None) -> str:
scripts/ui_lib/driver.py:292:def _client_startup_state(run: Run) -> list[str]:
scripts/ui_lib/driver.py:344:def _safe_text(value: object, limit: int = 400) -> str:
scripts/ui_lib/driver.py:351:def document(run: Run) -> str:
scripts/ui_lib/driver.py:369:def goto_text(run: Run, text: str) -> None:
scripts/ui_lib/driver.py:384:def clear_message_line(run: Run) -> None:
scripts/ui_lib/driver.py:393:def prompt_pending(run: Run) -> bool:
scripts/ui_lib/driver.py:398:def wait_prompt(run: Run, timeout_ms: int = 2000, poll_ms: int = 20) -> bool:
scripts/ui_lib/driver.py:412:def wait_idle(run: Run, timeout_ms: int = 2000, settle_ms: int = 50) -> bool:
scripts/ui_lib/driver.py:434:def capture(run: Run, ansi: bool = False) -> str:
scripts/ui_lib/driver.py:445:def wait_screen(run: Run, text: str, timeout_ms: int = SCREEN_TIMEOUT_MS) -> bool:
scripts/ui_lib/driver.py:470:def is_key_name(token: str) -> bool:
scripts/ui_lib/driver.py:514:def _csi_u_bytes(token: str) -> list[str] | None:
scripts/ui_lib/driver.py:541:def send_keys(run: Run, tokens: list[str]) -> None:
scripts/ui_lib/driver.py:557:def _pane_dead(run: Run) -> bool:
scripts/ui_lib/driver.py:564:def _wait_for_socket(run: Run, timeout_ms: int = SOCKET_TIMEOUT_MS) -> None:
scripts/ui_lib/driver.py:591:def _wait_for_view(run: Run, timeout_ms: int = START_TIMEOUT_MS) -> None:
scripts/ui_lib/driver.py:614:def _cleanup(run: Run) -> None:
scripts/ui_lib/driver.py:624:def _launcher_path(run: Run) -> Path:
scripts/ui_lib/driver.py:629:def _stderr_path(run: Run) -> Path:
scripts/ui_lib/driver.py:634:def _exit_path(run: Run) -> Path:
scripts/ui_lib/driver.py:639:def _shell() -> str:
scripts/ui_lib/driver.py:644:def _nvim_argv(run: Run) -> list[str]:
scripts/ui_lib/driver.py:661:def _launcher_script(run: Run, env: dict[str, str]) -> Path:
scripts/ui_lib/driver.py:692:def _launcher_command(run: Run) -> str:
scripts/ui_lib/driver.py:697:def startup_report(
scripts/ui_lib/driver.py:732:def start(
scripts/ui_lib/driver.py:844:def stop(run: Run) -> None:
scripts/ui_lib/driver.py:867:def _read_events(path: Path) -> list[dict]:
scripts/ui_lib/driver.py:880:def read_trace(run: Run) -> list[dict]:
scripts/ui_lib/driver.py:895:def format_trace(events: list[dict], count: int | None = None) -> str:
scripts/ui_lib/fixtures.py:68:def determinism_env(home: Path) -> dict[str, str]:
scripts/ui_lib/fixtures.py:106:def git(
scripts/ui_lib/fixtures.py:127:def write(path: Path, content: str) -> None:
scripts/ui_lib/fixtures.py:133:def commit(repo: Path, env: dict[str, str], message: str, files: dict[str, str]) -> str:
scripts/ui_lib/fixtures.py:142:def _init(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:146:def _clean(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:150:def _modified_two_files(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:156:def _staged_one_file(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:162:def _staged_and_unstaged(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:169:def _staged_failing_hook(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:183:def _staged_slow_hook(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:201:def _untracked(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:206:def _unborn(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:211:def _in_progress_rebase(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:225:def _in_progress_cherry_pick(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:238:def _log_history(repo: Path, env: dict[str, str]) -> None:
scripts/ui_lib/fixtures.py:262:def build(name: str, dest: Path, home: Path | None = None) -> Path:
scripts/ui_lib/paths.py:13:def run_dir(run_id: str) -> Path:
scripts/ui_lib/scenario.py:35:class ScenarioError(RuntimeError):
scripts/ui_lib/scenario.py:39:class ShellStepError(ScenarioError):
scripts/ui_lib/scenario.py:44:class Expectation:
scripts/ui_lib/scenario.py:60:class Statement:
scripts/ui_lib/scenario.py:74:class Scenario:
scripts/ui_lib/scenario.py:91:class ComboResult:
scripts/ui_lib/scenario.py:103:class Row:
scripts/ui_lib/scenario.py:117:def strip_comment(line: str) -> str:
scripts/ui_lib/scenario.py:146:def parse_bool(value: str) -> bool:
scripts/ui_lib/scenario.py:156:def unquote(text: str) -> str:
scripts/ui_lib/scenario.py:164:def parse_value(text: str) -> Any:
scripts/ui_lib/scenario.py:184:def parse_keys(text: str) -> list[str]:
scripts/ui_lib/scenario.py:195:def parse_env(text: str) -> dict[str, str]:
scripts/ui_lib/scenario.py:212:def parse_expectation(line: str) -> Expectation:
scripts/ui_lib/scenario.py:276:def parse_statement_name(line: str, keyword: str, where: str) -> str:
scripts/ui_lib/scenario.py:286:def parse_statement(line: str, where: str) -> Statement:
scripts/ui_lib/scenario.py:337:def parse_scenario(path: Path) -> Scenario:
scripts/ui_lib/scenario.py:416:def _format_value(value: Any) -> str:
scripts/ui_lib/scenario.py:420:def resolve_path(path: str, snapshot: dict) -> Any:
scripts/ui_lib/scenario.py:454:def check_screen_contains(screen: str, text: str) -> tuple[bool, str]:
scripts/ui_lib/scenario.py:461:def check_screen_not_contains(screen: str, text: str) -> tuple[bool, str]:
scripts/ui_lib/scenario.py:469:def check_screen_matches(screen: str, pattern: str) -> tuple[bool, str]:
scripts/ui_lib/scenario.py:480:def _row_view(runs: Iterable[dict], row: int) -> tuple[str, list[dict]]:
scripts/ui_lib/scenario.py:494:def _overlapping_runs(
scripts/ui_lib/scenario.py:519:def check_style(ansi: str, text: str, mode: str) -> tuple[bool, str]:
scripts/ui_lib/scenario.py:545:def check_no_message(snapshot: dict, pattern: str) -> tuple[bool, str]:
scripts/ui_lib/scenario.py:560:def check_cursor(snapshot: dict, screen: str, text: str) -> tuple[bool, str]:
scripts/ui_lib/scenario.py:576:def measure_latency(key: str, events: list[dict]) -> float | None:
scripts/ui_lib/scenario.py:604:def budget_scale() -> float:
scripts/ui_lib/scenario.py:615:def strict_latency() -> bool:
scripts/ui_lib/scenario.py:620:def check_latency(
scripts/ui_lib/scenario.py:644:def format_expectation(expectation: Expectation) -> str:
scripts/ui_lib/scenario.py:668:def check_document_unchanged(
scripts/ui_lib/scenario.py:682:def evaluate(
scripts/ui_lib/scenario.py:732:def combinations(scenario: Scenario) -> list[tuple[str, str]]:
scripts/ui_lib/scenario.py:756:def classify(xfail: str | None, results: list[ComboResult]) -> str:
scripts/ui_lib/scenario.py:767:def _snapshot_paths(scenario: Scenario, profile: str, size: str, name: str) -> tuple[Path, Path]:
scripts/ui_lib/scenario.py:775:def _write_snapshot(
scripts/ui_lib/scenario.py:792:def _clear_command_line(run: driver.Run) -> None:
scripts/ui_lib/scenario.py:804:def _run_shell(run: driver.Run, command: str, where: int) -> None:
scripts/ui_lib/scenario.py:843:def _shell_output(output: str | bytes | None) -> str:
scripts/ui_lib/scenario.py:851:def _run_combo(
scripts/ui_lib/scenario.py:996:def _trim_screen(screen: str) -> str:
scripts/ui_lib/scenario.py:1004:def _save_failure_screen(scenario: Scenario, result: ComboResult) -> Path:
scripts/ui_lib/scenario.py:1012:def _report_failure(
scripts/ui_lib/scenario.py:1037:def _print_table(rows: list[Row], verbose: bool = False) -> None:
scripts/ui_lib/scenario.py:1065:def _write_markdown(path: Path, snapshots: list[tuple[str, str, str, str, str]]) -> None:
scripts/ui_lib/scenario.py:1083:def expand_paths(paths: Iterable[str]) -> list[Path]:
scripts/ui_lib/scenario.py:1095:def run(paths: Iterable[str], markdown: str | None = None, verbose: bool = False) -> int:
scripts/ui_lib/startup.py:26:class SocketState:
scripts/ui_lib/startup.py:37:    def too_long(self) -> bool:
scripts/ui_lib/startup.py:41:    def summary(self) -> str:
scripts/ui_lib/startup.py:61:class ConnectState:
scripts/ui_lib/startup.py:68:    def summary(self) -> str:
scripts/ui_lib/startup.py:76:class IdleProbe:
scripts/ui_lib/startup.py:85:    def summary(self) -> str:
scripts/ui_lib/startup.py:109:def _bounded(text: str) -> str:
scripts/ui_lib/startup.py:117:def inspect_socket(path: Path) -> SocketState:
scripts/ui_lib/startup.py:140:def read_tail(path: Path, lines: int = STDERR_TAIL_LINES) -> list[str]:
scripts/ui_lib/startup.py:150:def read_exit_status(path: Path) -> int | None:
scripts/ui_lib/startup.py:162:def connect_probe(
scripts/ui_lib/startup.py:201:def format_report(
```
(exit 0)

Complete `debug.lua` exported/local function line index:

```text
$ grep -nE '^(local function|function M\.|M\.[[:alnum:]_]+ =|local M)' lua/magit-lsp/debug.lua
9:local M = {}
14:M.state = {
25:M.views = {}
28:M.order = {}
31:M.notifications = {}
34:M.progress_events = {}
60:function M.trace(event, detail)
81:function M.record_notification(msg)
90:function M.configure(opts)
125:function M.view_open(id, kind, buf)
142:function M.view_close(id)
158:function M.render_received(id, version)
165:function M.render_applied(id, version)
176:function M.render_acked(id, version)
186:function M.keymap_received(id, notice)
220:function M.set_pending(id, pending)
229:function M.key_pressed(steps)
236:function M.trace_id()
243:function M.progress_started(token, value)
251:function M.record_progress(token, value)
263:function M.progress_reported(token, value)
273:function M.progress_finished(token)
281:function M.key_sent(step, id)
286:function M.request_started()
291:function M.request_finished()
296:function M.queued_started()
301:function M.queued_finished()
306:function M.prompt_started()
311:function M.prompt_finished()
317:function M.schedule(fn)
330:function M.idle()
358:local function active_view()
371:local function window_state(win)
400:local function recent_messages()
418:function M.snapshot()
```
(exit 0)

`TestSession` relevant public signatures and internal dispatch/result functions:

```text
$ grep -nE '^    pub fn (press|invoke|ack|open_view|open_status|set_mode)|^    fn (press|invoke|ack|open_view|render)' tests/common/session.rs
110:    pub fn open_status(fixture: &RepoFixture, profile: Profile) -> (Self, ViewRender) {
115:    pub fn open_status_at(
126:    pub fn open_status_view(&mut self, viewport: Viewport) -> ViewRender {
132:    pub fn open_view(
150:    pub fn open_view_with_args(
216:    pub fn acked_version(&self) -> Option<u64> {
221:    pub fn acked_version_of(&self, view_id: &str) -> Option<u64> {
229:    pub fn ack(&mut self) {
234:    pub fn ack_view(&mut self, view_id: &str) {
369:    pub fn acked_tree(&self) -> &Tree {
448:    pub fn press(&mut self, keys: &str) -> InvokeResult {
459:    pub fn press_full(
485:    pub fn press_at_version(&mut self, key: &str, version: u64) -> InvokeResult {
504:    pub fn press_visual(&mut self, start: usize, end: usize, key: &str) -> InvokeResult {
520:    pub fn press_ui(&mut self, key: &str) -> InvokeResult {
543:    pub fn invoke(&mut self, command: &str) -> InvokeResult {
548:    pub fn invoke_args(&mut self, command: &str, args: Value) -> InvokeResult {
553:    pub fn invoke_at(&mut self, command: &str, cursor: Cursor) -> InvokeResult {
558:    pub fn invoke_at_args(
568:    pub fn invoke_here(&mut self, command: &str) -> InvokeResult {
573:    pub fn invoke_selection(
583:    pub fn invoke_ui(&mut self, command: &str) -> InvokeResult {
619:    pub fn invoke_full(
631:    pub fn invoke_at_version(
656:    pub fn set_mode(&mut self, mode: ViewMode) -> InvokeResult {
```
(exit 0)

Complete `color_spike.lua` function index:

```text
$ grep -nE '^(local function|function |local [[:alnum:]_]+ = function)' scripts/color_spike.lua
24:local function sys(cmd)
32:local function make_fixture()
72:local function hex(n)
79:local function resolved_name(group)
91:local function probe_tohtml()
164:local function collect_tokens(buf, lines)
309:local function pad(s, n)
```
(exit 0)

