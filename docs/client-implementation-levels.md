# Client implementation levels

## Client implementation levels

The server selects a presentation at `initialize`: a client that omits
`capabilities.experimental.magit` gets the Plain presentation, and one that
sends the strict rich declaration gets the rich protocol. Both presentations are
implemented and tested. Levels 0–3 are cumulative Plain capabilities; Level 4 is
the rich protocol used by the bundled clients. Each level lists what the client
sends and receives, the standard capabilities it declares and the behavior when
one is absent, its recorded exchange, and the repository identifiers that prove
it. A client that stops at Level 1 keeps a working read-only status document;
features above its level are omitted or disabled by the server rather than
advertised speculatively.

The table below classifies each standard capability the server extracts into
`PlainCapabilities` (`src/presentation/mod.rs`) by the Plain level it belongs to.
The rows are generated from the capability field set and are checked against
the implemented struct by `tests/schema.rs`.

See generated [capability levels](schema/capability-levels.md).

`textDocument.foldingRange` is request-only: the server answers folds once the
client declares it, so the capability gates no lower level even though it is
grouped with the Level 3 extras.

Classification rule: `src/presentation/mod.rs::negotiate` selects
`Presentation::Plain` exactly when `capabilities.experimental.magit` is absent; a
present block is strictly validated as rich v1 and selects Level 4. The Plain
level is derived from the standard capabilities the client actually declares,
not from its name or version. A client quirk from
[Known client behaviors](known-client-behaviors.md#known-client-behaviors) can correct one recorded
behavior beyond the declaration: the quirk's capability override is applied to
the negotiated `PlainCapabilities` before the level is used, so
`assume-document-changes` adds `workspace.workspaceEdit.documentChanges` for the
matched Neovim build even though its initialize trace omits it. Actions are
offered and run through `magit.command` without the client declaring
`workspace/executeCommand`
(`tests/plain_actions.rs::code_actions_work_without_execute_command_declaration`).
`completion` is not implemented: the server advertises no
`textDocument/completion` provider and registers no handler, so the field appears
only in the effective-capability echo and unlocks no level.

Recorded Neovim fact:
`tests/fixtures/lsp_traces/initialize-plain-neovim-0.12.6.json` (Neovim 0.12.6)
declares `workspace.applyEdit` and omits
`workspace.workspaceEdit.documentChanges`, but Neovim still applies
`workspace/applyEdit` edits that use `documentChanges`. The generated known-client
appendix records this as `assume-document-changes` with the fake-editor model
`neovim-accepts-undeclared-documentChanges`
(`tests/common/editor_model/mod.rs`), so the server corrects the declaration. Do
not generalize from this trace; it is bounded to the matched client version. The
same trace also matches `apply-result-unreliable` (`silent-stale-apply`), which
reconciles a silent stale apply result without changing the derived level.

### Level 0 — Connection, initialization and document synchronization

Sends: standard `initialize` with the client's supported capabilities, then
`initialized` and only standard document notifications. Include
`general.positionEncodings` for the encodings you can apply; the server answers
with one negotiated `positionEncoding` and every range in both directions uses
that encoding. Omit it and the server selects `utf-16`. The server advertises
incremental synchronization (`textDocumentSync.change: 2`, `openClose` and
`save.includeText`) and accepts ranged or rangeless `didChange` sequences,
applying each change in order against the preceding result. Versions are opaque;
do not assume the server's version is your version plus one.

Receives: the standard server capabilities (`documentSymbolProvider`,
`definitionProvider`, `documentLinkProvider` without resolve,
`foldingRangeProvider`, full `semanticTokensProvider`, `codeActionProvider`,
`executeCommandProvider` for `magit.command`, incremental `textDocumentSync` and
the negotiated `positionEncoding`). No custom method is required at Level 0.

Capabilities and degradation: `general.positionEncodings` absent → `utf-16`; an
invalid or reversed range is rejected and the server waits for an authoritative
repair rather than guessing. `textDocument.foldingRange` and
`textDocument.semanticTokens` only let the client request those providers; they
gate nothing on the server. Keeping `experimental.magit` absent is what selects
Plain; `initializationOptions.profile` is not a selector.

Worked exchange: [plain-open.jsonl](client-exchanges.md#plain-openjsonl) records the initialize
handshake and the first document edit. Conformance checklist:
`tests/plain_core.rs::absent_selector_selects_plain_and_extracts_standard_capabilities`
(selector and extracted capabilities), `tests/plain_sync.rs`
(`stdio_advertises_incremental_sync_and_accepts_document_lifecycle`,
`stdio_defaults_to_utf16_when_no_supported_encoding_is_offered`,
`sequential_changes_are_atomic_and_encoding_aware`,
`invalid_unicode_crlf_and_reversed_ranges_require_authoritative_repair`), and
`tests/ui/plain_init.lua` (a minimal Plain client using stock Neovim
capabilities with `window.showDocument.support` disabled).

### Level 1 — Plain read-only

Sends: `textDocument/didOpen` for a presentation file (default name `.magit`,
configurable through `presentation_files`), an echoing `textDocument/didChange`
after applying each server edit, and the matching `workspace/applyEdit` response.

Receives: `workspace/applyEdit` carrying the complete status document (head,
staged/unstaged/untracked sections and hunks) as a standard `WorkspaceEdit`.
With `workspace.workspaceEdit.documentChanges` the edit uses `documentChanges`
and your current document version; without it the edit uses the legacy
unversioned `changes` map. Exactly one apply is in flight per document, and a
matching didChange echo is recognized by text hash whether it arrives before or
after the apply response, so it is never mistaken for user input. The server
also answers the advertised providers for the open document.

Bootstrap: the repository is resolved from the workspace folder or the document's
parent. Ambiguous candidates produce `window/showMessage` and no render; the
server never guesses for a mutation. Presentation files are excluded from
untracked status. Apply each edit and echo it; a rejected or divergent edit is
retried from authoritative text, and persistent format drift suspends further
edits with one warning.

| Capability | Present | Missing |
|---|---|---|
| `workspace.applyEdit` | The server sends the status document as a workspace edit. | `window/showMessage` explains that the document cannot be rendered; the session is inert for it. |
| `workspace.workspaceEdit.documentChanges` | Versioned `documentChanges` edits, refused when your version moved. | Legacy `changes` edits without versions. |
| `general.positionEncodings` | Ranges use the negotiated `utf-8`, `utf-16` or `utf-32`. | `utf-16` is used; encode non-BMP ranges accordingly. |
| `textDocument.foldingRange` | You may request the advertised folds; server folding stays authoritative. | No folding requests; the text is unaffected. |

Worked exchange: [plain-open.jsonl](client-exchanges.md#plain-openjsonl). Conformance checklist:
`tests/plain_harness.rs::plain_did_open_apply_edit_is_pumped_and_echoed`,
`tests/plain_render.rs`
(`controller_serializes_unicode_last_line_growth_and_shrink_without_eof_retry`,
`named_workaround_reconciles_silent_stale_success_and_preserves_universal_safety`,
`binary_profiles_watch_and_repair_divergence_in_both_response_orders`),
`tests/plain_sync.rs::observing_confirms_echoes_and_responses_in_either_order`,
`tests/plain_repo.rs::resolves_none_workspace_document_nested_alias_and_linked_worktree_without_guessing`,
and `tests/plain_vis.rs`
(`bootstrap_applies_default_and_repository_section_limits_to_plain_snapshots`,
`plain_provider_ranges_honor_negotiated_encoding_for_unicode_and_reject_invalid_columns`).

### Level 2 — Plain actions

Sends: `textDocument/codeAction` with a half-open range, then
`workspace/executeCommand` with command `magit.command` and exactly one opaque
argument `{command,documentUri,target?,viewGeneration}`.

Receives: an array of standard `CodeAction`s. Implemented actions carry a
`command` whose arguments are opaque; `target` is a tagged union with camelCase
`kind` (`content`, `view`, `node`, `menuItem`), and a stale `viewGeneration` is
refused without mutation after a refresh. Unavailable actions carry a disabled
reason only when `textDocument.codeAction.disabledSupport` is declared;
otherwise they are omitted. Planned commands stay disabled or omitted with their
roadmap reason. The server declares `workspace/executeCommand` (it advertises
`magit.command`); the client does not need to declare the execute-command
capability for action invocation to work.

Capabilities and degradation: `textDocument/codeAction` absent → no actions are
returned and Plain status stays read-only. `disabledSupport` present →
unavailable and planned actions are included with reasons; absent → those entries
are omitted. `workspace.applyEdit` plus `textDocument/codeAction` are both
required for `status.stage`, `status.unstage` and `status.discard`; without them
those actions are unavailable with a readable reason. Selections are LSP
half-open ranges in the negotiated encoding; the server resolves them against the
exact snapshot and converts endpoints to its inclusive selection, so never parse
rendered text for structure.

Worked exchange: [plain-code-action.jsonl](client-exchanges.md#plain-code-actionjsonl). Conformance
checklist: `tests/plain_actions.rs`
(`action_target_uses_strict_camel_case_wire_fields`,
`plain_discard_opens_a_document_confirm_and_cancel_changes_nothing`,
`unavailable_actions_are_omitted_without_disabled_support`,
`code_actions_work_without_execute_command_declaration`,
`code_actions_are_unsupported_without_declaration`,
`scoped_planned_and_unavailable_commands_are_disabled_and_forged_requests_are_refused`),
`tests/plain_select.rs` (`plain_log_select_offers_commit_code_actions`,
`plain_log_select_confirm_returns_the_caller_menu_with_the_chosen_commit`), and
`tests/plain_stack.rs`
(`commit_menu_and_back_stay_in_plain_document_and_refuse_stale_targets`,
`plain_menu_switches_use_retained_targets_and_restore_caller`).

### Level 3 — Plain extras

Sends: `textDocument/didChange` for field and prompt edits, and code actions for
the rendered Submit/Cancel rows. Document prompts (`confirm`, `text`, `choice`,
`multiChoice`) are rendered as rows and fields in the same Plain document; type
into a field, then invoke the offered action. The server reconciles the field
text against its own document and repairs edits outside fields. Secret prompts
are unsupported: they cancel cleanly with a readable message
(`tests/plain_core.rs::plain_secret_prompt_refusal_uses_the_readable_constant`),
and credential-dependent work fails readably.

Capabilities and degradation: prompts need no capability, only the Level 0
document sync. `window.showDocument.support` present → a commit draft is opened
through `window/showDocument` after any pre-opened buffer is replaced by a
versioned apply edit. Absent → the server sends manual instructions naming the
`COMMIT_EDITMSG` path together with the Confirm and Abort actions; you can still
open that file yourself, send `didOpen`/`didChange`, and invoke the offered
Confirm/Abort code actions (the manual path sends neither `applyEdit` nor
`showDocument`). Saved-file close confirms disk bytes; explicit Abort uses the
run-aware command above and restores the status document only after its exact acknowledgement;
failed Abort retains the editable draft. `window.workDoneProgress` present → the
server runs `window/workDoneProgress/create` and `$/progress` begin/report/end
for mutations; absent → no progress notifications, the operation still runs and
outcomes arrive as messages. Completion is not implemented: the server
advertises no `textDocument/completion` provider and registers no handler;
`completion` appears only in the `experimental.magit.effective` echo
(`tests/plain_core.rs::absent_selector_selects_plain_and_extracts_standard_capabilities`),
and unregistered methods fail with `-32601`
(`tests/plain_harness.rs::real_binary_lifecycle_and_current_unsupported_methods_are_honest`),
so do not depend on completion.

Worked exchanges: [plain-document-prompt.jsonl](client-exchanges.md#plain-document-promptjsonl),
[commit-edit-confirm.jsonl](client-exchanges.md#commit-edit-confirmjsonl) and
[commit-edit-abort.jsonl](client-exchanges.md#commit-edit-abortjsonl). Conformance checklist:
`tests/plain_prompt.rs`
(`confirm_renders_title_detail_blank_and_custom_yes_no_rows`,
`text_renders_one_prefilled_field_and_submit_cancel_rows`,
`choice_answers_by_item_default_custom_or_cancel`,
`multi_choice_items_toggle_and_done_answers_the_selection`),
`tests/plain_draft.rs`
(`open_plain_commit_buffer_is_replaced_before_show_document`,
`manual_plain_commit_instructions_have_no_apply_edit`,
`saved_file_close_commits_the_disk_text`,
`stale_explicit_confirm_is_refused_and_keeps_the_draft_pending`,
`explicit_abort_creates_no_commit_and_restores_the_status_view`),
`tests/plain_harness.rs`
(`plain_commit_editor_manual_instructions_cancel_pending_git_run`,
`live_binary_progress_requests_are_serviced_while_command_runs`), and
[`docs/plain-support.md`](./plain-support.md) with
`tests/plain_support.rs::plain_support_document_matches_the_registry` for the
generated per-command availability. Regenerate the support table with
`UPDATE_PLAIN_SUPPORT=1 make test-rust FILTER="--test plain_support"`; the
staleness test fails when the checked-in file drifts. Do not hand-edit it.

### Level 4 — Rich protocol

The bundled Neovim and VS Code clients send the strict rich declaration:
`capabilities.experimental.magit` with `v: 1`, `keys` (`dynamic` or `static`),
`modifiers` (`full` or `basic`), `layout`, `effects`, `prompts` and the optional
`pick` block. That presence selects rich instead of Plain; unknown fields and
unsupported values are rejected. `modifiers:"basic"` filtering is implemented: it
retains a chord only when every step lacks explicit `shift` or `meta`, except the
portable lone `shift+Tab` backtab, and removes an unsupported whole chord rather
than the command. It affects keymap delivery and captured keys only, never
command availability through menus or direct invocation.

Sends/receives: the rich lifecycle (`view/open`, `view/render`, `view/rendered`,
`view/resync`, `view/close`, `view/invoke`, `view/key`, `view/setMode`,
`view/resize`, `view/promptRespond` and `view/pickRespond`, plus the
`view/keymap`, `view/prompt`, `view/pick` and `view/effects` notifications). It
adds dynamic or static key delivery, captured input, menus, native prompts and
picks, text mode, and the real-file commit draft. All payloads, ordering rules
and failure handling are specified earlier in this document; do not duplicate
them here:

- [Capability map (`customRender`)](CLIENT_SPEC.md#capability-map-customrender),
  [Methods and lifecycle](CLIENT_SPEC.md#methods-and-lifecycle) and
  [Descriptors, URIs and view lifetime](CLIENT_SPEC.md#descriptors-uris-and-view-lifetime) for
  the declaration, full message set and view identity.
- [Input, navigation and presentation](CLIENT_SPEC.md#input-navigation-and-presentation) for
  keys, modes and folding; [Prompts, effects and drafts](CLIENT_SPEC.md#prompts-effects-and-drafts)
  for prompt/effect handling and work-done progress.
- [Current real-file commit editing](CLIENT_SPEC.md#current-real-file-commit-editing) for the
  draft, confirmation and abort contract.
- [Feature-to-capability mapping and clients (`customRender`)](CLIENT_SPEC.md#feature-to-capability-mapping-and-clients-customrender)
  for per-workflow client behavior.

Capabilities and degradation: absent `effects`, `prompts` or `layout` entries
never receive undeclared optional payloads — the server turns an undeclared
optional effect or prompt into a readable warning message and an undeclared
placement into `main`, while `cfg(test)` builds reject the emission outright.
`keys` controls dynamic versus static delivery; static does not replace key
capture. A client that omits `editDocument` cannot receive commit drafts and must
not enable commit commands; one that omits `visitFile` must not route visit
actions. The core `view/open`, render/ack, and key/invoke traffic has no
capability fallback: without it the rich presentation is unusable.

Worked exchange: [rich-lifecycle.jsonl](client-exchanges.md#rich-lifecyclejsonl). Conformance
checklist: `tests/ui/scenarios/status-basics.scn`,
`tests/ui/scenarios/common-navigation.scn`,
`tests/ui/scenarios/hunk-body-fold.scn`,
`tests/ui/scenarios/process-log.scn`, `tests/ui/scenarios/log-select.scn`,
`tests/ui/scenarios/log-view.scn`, `tests/ui/scenarios/commit-message-editor.scn`,
`tests/ui/scenarios/commit-confirm.scn`, `tests/ui/scenarios/commit-real-file.scn`,
`tests/ui/scenarios/option-prompt.scn`, `tests/ui/scenarios/multiple-views.scn`,
`tests/ui/scenarios/text-mode-roundtrip.scn`,
`tests/keymap.rs::basic_modifiers_keep_portable_backtab_and_drop_other_shift_or_meta_chords`
(basic modifier filtering), `tests/plain_harness.rs`
(`recorded_presets_and_named_profiles_bind_declarations_and_registry_behaviors`,
`rich_server_render_ack_and_stale_resync_preserve_caller_reply`),
`tests/plain_core.rs` (`runtime_unsupported_effect_is_a_readable_message`,
`unsupported_view_placement_falls_back_to_main_without_changing_descriptor`), and
`tests/examples.rs::plain_examples_replay_against_the_real_binary` (replays every
recorded exchange, including `rich-lifecycle.jsonl`).
