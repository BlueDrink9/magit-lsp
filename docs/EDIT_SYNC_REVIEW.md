# Rich/Plain editing and synchronization review

Date: 2026-11-13. Review base: `1ccb038adc2b9e9633f394ca1fea789aed2eabf8`, plus the dirty working checkout. Concurrent edits and commits occurred during review; this is not an immutable-snapshot or clean-tree verification. Findings below are statically VERIFIED at the cited paths/lines; runtime behavior was not exercised. Queue completion remains authoritative in `TASKS.md`.

## Conclusion

Most semantic ownership is already shared. Tighten transport and transaction edges, not everything into a universal editor abstraction. Structural sharing is more meaningful than a line-count percentage:

- One `DocStore` owns accepted editor text and LSP versions.
- One `EditRegistry`/`Draft` lifecycle handles commit-message editing, including instant-rebase message callbacks.
- One session/tree model owns rendered views; Rich and Plain have different delivery protocols.
- Two bundled native commit adapters implement editor-specific lifetime rules.
- Render generations, synchronized editor versions, and Plain delivery/incarnation identities are distinct contracts, not duplicate versions to merge.

## Path ownership

| Concern | Shared path | Presentation-specific boundary |
| --- | --- | --- |
| Commit editing | `src/editor.rs:540-610`, `src/editor/draft.rs:145-277`, `src/server.rs:5089-5140`: registry, draft, repository worker | Rich `editDocument` reaches Neovim `lua/magit-lsp/edit.lua:236-285` and VSCode `clients/vscode/lib/drafts.js:561-640`; Plain uses standard document opening/template delivery or instructions (`src/server.rs:1013-1180`). |
| Rebase message editing | Scoped instant-rebase editor admission uses commit-message drafts (`src/editor.rs:943-977,1271-1333`) | Availability/presentation gates remain separate. |
| Rebase todo editing | Structured paused/prepared model (`src/rebase_todo_view.rs:15-35,80-94`), row commands and submit (`src/commands.rs:1193-1247`) | Not another native draft editor. Plain restricts supported action dispatch (`src/server.rs:2470-2503,2823-2855`). |
| Rendered documents | Authoritative session tree and generation (`src/session.rs:389-438`) | Rich sends renders and receives acknowledgements (`lua/magit-lsp/view.lua:368-401`, `clients/vscode/lib/requests.js:98-152`); Plain serializes delivery through `workspace/applyEdit` (`src/presentation/plain.rs:148-169,410-450`). |
| `didOpen` / `didChange` | Shared ordered handler, store, encoding/range/version validation, transactional draft acceptance (`src/server.rs:3153-3223`) | Plain reconciles render echoes and editable fields (`src/server.rs:1432-1563`); Rich prompt reconciliation uses the session model (`src/server.rs:1571-1622`). |
| `didSave` / `didClose` | Save is nonterminal; close can confirm independently read saved bytes (`src/server.rs:3225-3289`, `src/editor/draft.rs:251-277`) | Neovim buffer teardown and VSCode last-owned-tab closure differ intentionally (`clients/vscode/lib/drafts.js:693-743`). |
| Version validation | Draft confirmation requires synchronized identity/version (`src/protocol.rs:77-156`, `src/editor/draft.rs:206-222,264-277`) | Render acknowledgement identifies a view/generation (`src/session.rs:956-966`); Plain also correlates applyEdit response and observed echo (`src/presentation/plain.rs:282-356`). |
| Confirm / Abort | Ordered semantic barriers (`src/server.rs:2219-2277,3011-3094`); Plain carries run/URI/version (`src/server.rs:3674-3702`) | VSCode Abort diverges from the acknowledged semantic path; see finding below. |
| Messages and errors | Readable domain effects (`src/server.rs:5297-5332`) | Rich custom effects versus standard Plain notifications (`src/server.rs:1008-1077`); preserve native recovery state when delivery fails. |

Text mode is disposable local editing, not Plain presentation or a managed Git draft (`src/session.rs:306-323`, `docs/CLIENT_SPEC.md:86`). The native helper announces `commit-message` despite the existence of `DraftKind::RebaseTodo` (`src/editor.rs:1086-1102`). Prepared rebase rejects `Reword`/`Squash` (`src/rebase_plan.rs:72-75`), while instant autosquash supports message callbacks. Shared names do not establish capability parity.

## Prioritized tightening

### P0: retain VSCode draft text until Abort is positively acknowledged

`cancel()` catches request failure and still calls `forget()` (`clients/vscode/lib/drafts.js:529-552`); `forget()` reverts the document and closes tabs (`:420-438`). This is a statically verified potential data-loss path, not a runtime reproduction: failed cancellation can discard unsaved text without cancelling the server helper.

Reuse the existing run/URI/version-correlated semantic Abort used by Neovim and Plain. Keep text and registration unless the exact aborted acknowledgement succeeds. Extend existing managed-draft integration coverage for rejection, transport failure, mismatched acknowledgement, and successful retry. Verification planned: `make vscode-test`; mocked coverage must not be described as native evidence. Worker-ready acceptance and constraints are recorded under this finding's unique title in `TASKS.md`.

### P2: stage synchronized changes for only the addressed document

`draft_notification()` clones and replaces the whole `DocStore` for each change (`src/server.rs:3196-3222`). The transaction concerns one URI; unrelated documents need not be copied. No performance measurement was made.

Use one document-level validation/application implementation, stage one candidate, obtain draft acceptance, then commit only that URI. Preserve registry/store atomicity, error wording, version rejection, and repair behavior. Extend the existing incremental commit-sync regression with another open URI and rejected-change isolation checks. Verification planned: `make test-rust FILTER="--lib --test plain_sync --test editor_bridge --test plain_draft"`. Detailed acceptance lives in `TASKS.md`.

## Boundaries to retain

Do not merge render acknowledgements with editor-text versions, synchronized confirmation with saved-file confirmation, structured todo submission with helper release, or Plain response/echo synchronization with Rich rendering. Consolidate common semantic barriers and single-document validation instead. No implementation changes or verification runs were performed by this review.
