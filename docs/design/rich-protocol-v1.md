# Rich protocol v1 decisions (T113)

This is the decision record for the rich protocol. It decides the smallest set
of rules that works for the clients that exist now; it does not implement them.
The facts are in the [T112 audit](rich-protocol-audit.md), whose evidence
baseline is `f15c026`. Implementation belongs to T114 unless a disposition says
this ticket already corrected authored documentation.

**Status labels.** *Current* means implemented and relied on by shipped clients.
*Planned* means decided here and implemented later (T114 or a later client
ticket); the maintained reference marks planned behavior as such. *Documented*
means the behavior exists and only its description changes.

**Baseline.** Before project 1.0, breaking protocol, API and config changes are
permitted without compatibility shims or an additive-only guarantee. The wire's
`v=1` label alone grants no stability.

## 1. Core and optional parts

**Core** is the minimum every rich client implements for the protocol version
it declares. It is frozen at protocol 1.0.

| Part | Current status |
| --- | --- |
| The capability block (declaration, effective capabilities, quirks) | Current |
| `view/open` and `view/close` | Current |
| The three `view/render` forms: full `text`; one or more `edits`; state/mode-only (neither `text` nor `edits`) | Current |
| Acknowledgement (`view/rendered`) and `view/resync` | Current |
| `view/key` and `view/invoke` | Current |
| `view/keymap` (dynamic delivery; static clients generate the same bindings) | Current |
| Basic effects: `message`, `pushView`, `popView`, `focusView`, `passThrough`, `quit` | Current |
| Prompts (notification plus `view/promptRespond`) | Current |
| Progress (`$/progress`) | Current |

**Optional** surfaces are declared by a client and are never required for a
core flow.

| Part | Current status |
| --- | --- |
| `editDocument`, `visitFile`, `clipboard` (declared in `effects`) | Current |
| Picks (`view/pick` / `view/pickRespond`, declared in `pick`) | Current |
| The structured menu model (`menu` on a menu view's render) | Current |
| Text mode | Current |
| Layouts other than `main` (declared in `layout`) | Current |

Rules:

- A client declares the optional parts it supports. The declaration remains
  strictly validated, so a new optional field is a protocol-version change, not
  an unannounced extension.
- The server must not require an optional part to complete a core flow. When an
  optional part is undeclared, the server uses the core path or refuses the
  specific optional request with a readable user-facing message; it never fails
  silently.
- The menu model and text mode are optional to implement even where the current
  capability block has no separate field for them; a client that omits them
  still receives core renders and commands.

**Reason:** the editors differ in native UI, and no client needs every surface.
A small frozen core lets a new client attach safely, while declared optional
parts keep the server from assuming capabilities a client does not have.

## 2. Evolution and stability policy

**Before 1.0 (current).** Protocol, API and config changes may break clients and
users without shims. There is no additive-only promise for optional parts, and
`v=1` is a label, not a compatibility guarantee. Boundary validation and
negotiation continue to work throughout this period: the capability block stays
strictly validated and an unsupported declaration version is rejected with a
readable error. Until the decision 3 handshake is implemented, the strict `v`
check is the only version validation in force.

**After 1.0 (explicit decision).** Within protocol major version 1, core is
stable: additions must be optional and declared, and a change to core semantics
or wire shape requires a new major protocol version (`v=2`) with the decision 3
handshake and a migration note in the protocol reference. Public server config
keys and documented client APIs follow the same major-version rule: additive
within a major; removal or rename at a major with documented migration
guidance. Only this paragraph creates post-1.0 compatibility guarantees; nothing
before 1.0 does.

**Reason:** client authors need one stable minimum to target, but the whole
surface need not freeze. Tying breaks to a major version keeps optional growth
possible without making old clients guess.

## 3. Version negotiation and readable mismatch

**Current (implemented).** A rich client declares
`capabilities.experimental.magit.v`; only `1` is accepted, and any other value
fails initialization with the readable
`capabilities.experimental.magit unsupported rich protocol version` error.
Unknown or missing declaration fields fail the same strict parse. The
initialize result advertises no rich protocol version. The VS Code package
version warning is release alignment, not protocol negotiation.

**Planned (decided here, implemented by T114).**

- The initialize result reports
  `capabilities.experimental.magit.protocolVersion` (integer, currently `1`).
  It is the version of every accepted rich session, not a maximum and not the
  start of a supported range. The server supports exactly one version and
  reports that value.
- A rich client declaration carries `v` (the single session version it selects)
  and `minimumVersion` (the oldest server session version it can use). Both are
  integers. `v` is accepted only when it is `1` (the existing strict check).
  An absent `minimumVersion` means `v`; a present `minimumVersion` is a
  positive integer and must not exceed `v`. A declaration that violates either
  rule is a readable initialize error. The server never clamps, downgrades,
  retries or substitutes a version.
- The server accepts a session only when `minimumVersion <= v ==
  protocolVersion`. Since `v` is the session version, the accepted session can
  never be below the client's minimum. The server rejects `v != 1` with the
  existing `capabilities.experimental.magit unsupported rich protocol version`
  error and `minimumVersion > v` with a readable initialize error naming both
  numbers; neither opens a session.
- After initialize the client checks the advertisement itself: the reported
  `protocolVersion` must be a positive integer equal to the `v` it declared. A
  missing, malformed or different advertisement is a client-side compatibility
  failure — a readable message naming the required and advertised versions, no
  rich view opened and no rich traffic sent. The client never treats a missing
  advertisement as `1`, falls back to Plain, retries with another version or
  installs a compatibility shim.
- An initialize rejection is an RPC error and reaches the user before any
  session exists: VS Code shows it through the rejected `client.start()`
  (`clients/vscode/lib/session.js:154-157`: output channel plus
  `showErrorMessage`); Neovim observes a failed/stopped LSP client rather than
  a response callback (`lua/magit-lsp/init.lua:369-375`), so T114 must report
  the readable error there. A post-initialize failure is handled where each
  client already holds the initialize result — VS Code in the `client.start()`
  success path (`clients/vscode/lib/session.js:145-153`), Neovim in `on_init`
  (`lua/magit-lsp/init.lua:360-367`) — using that client's existing
  notification channel; neither opens a rich view.
- A server that predates this handshake fails the strict parse of
  `minimumVersion`; that is a readable initialize error. No compatibility with
  pre-T114 peers is promised before 1.0, and the existing strict `v` check is
  kept rather than duplicated or replaced.

**Outcome table.**

| Declaration and advertisement | Check | Result |
| --- | --- | --- |
| `v = 1`, `minimumVersion` absent or `1`, server advertises `1` | matching | accept; the session speaks `1`; rich traffic allowed |
| `v` present but not `1` (including `0`) | server strict `v` parse | initialize error `capabilities.experimental.magit unsupported rich protocol version`; no session, no rich traffic |
| `minimumVersion` above the selected version (`v = 1`, `minimumVersion = 2`) | server compares with `v` and `protocolVersion` | readable initialize error naming both numbers; no session, no rich traffic |
| `minimumVersion` not an integer or not positive (for example `0`, `1.5`, `"2"`) | server strict parse | readable initialize error; no session, no rich traffic |
| server result omits `protocolVersion`, carries a non-integer, or advertises a value other than the declared `v` | client post-initialize check | readable client error naming required and advertised versions; no rich view opened, no rich traffic, no fallback to `1` |

**Reason:** today a client cannot check compatibility before declaring, and a
server cannot state the minimum it requires. Making `protocolVersion` the
accepted session version and constraining both numbers to the one selected
version closes the gap where `v = 1, minimumVersion = 2` passed both written
checks, without adding negotiation, downgrade or legacy fallback. Reporting one
version and one minimum covers both directions with two integers, preserves the
existing readable rejection, and gives a client with newer optional
requirements a way to stop with an explanation instead of failing later.

## 4. Audit wart dispositions

Every real finding is fixed, documented or rejected. A *not real* finding gets
no code or wire change.

| Finding | Status | Disposition | Decision and reason | Wire change |
| --- | --- | --- | --- | --- |
| 1. Keymap `version` reuses the render version; draft keymaps use `0` | Real (ambiguity) | Document | Keep the field. `view/keymap.version` is the advisory render generation the bindings belong to and is never acknowledged; `0` identifies a draft/edit keymap keyed by `runId`. Clients already ignore it for acknowledgement, so renaming or removing it would add work without changing behavior. Documented in PROTOCOL §4.2 and ARCHITECTURE §3.9. | No |
| 2. `view/render` sends zero or one edit; multi-edit rules unwritten | Real (documentation) | Document | Keep the type. The server currently emits at most one edit. If a future server emits several, they do not overlap, `endLine` is exclusive, and clients apply them bottom-up (descending `startLine`). Both shipped clients already implement that order; the gap was only the written contract. | No |
| 3. Draft `runId` keymaps may be dead after the real-file cutover | Not real | Reject removal | No dead field or path is removed. `EditDocumentMode` has only `RealFile`, and the edit keymap is delivered to dynamic clients on the same path as the render. A future static client wanting draft keys is new work, not a cleanup. | No |
| 4. One acknowledgement snapshot vs. several outstanding renders is unwritten | Real (documentation) | Document; reject flow control | Document current behavior: a view retains one acknowledged tree/version, successive renders may be delivered before any ack, and a render whose base is not the retained acknowledged version carries full text. Stale acks are no-ops; a lost update needs `view/resync`. Reject "at most one outstanding delivered version": it is not baseline behavior and would add server-side deferral or rejection with new concurrency risk that neither client needs. | No |
| 5. No bidirectional rich protocol version handshake | Real (missing negotiation) | Fix (planned) | Add the decision 3 handshake; keep the existing strict `v` rejection and readable initialize error. The audit's residual gap was that a client cannot check compatibility before declaring and the server cannot state a minimum. | Yes (initialize result and declaration) |
| 6. `keyModifiers` is read and logged but inert | Not real | Reject; document actual behavior | Keep `keyModifiers` and the basic-modifier filtering, including the lone `shift+Tab` exception. It already changes both delivered bindings and server-side key resolution. No second mechanism is added. | No |
| 7. VS Code cannot replay a non-printable, non-clipboard `passThrough` | Real | Document the limit; reject modifier downgrading | Keep the printable/clipboard replay fallback and record that a static client may be unable to replay other canonical keys, logging the limit. A bounded replay table would be behavior work with a risk of mapping keys to the wrong editor action, and the narrowest rule that works now is the documented limit. Declaring `modifiers: 'basic'` does not fix replay: modifier fidelity describes reporting, not replay, and would only drop bindings or create more pass-throughs. Recorded in PROTOCOL §4.2 and ARCHITECTURE §3.9. | No |
| 8. Static clients learn about a mode change only through render-derived context | Real (narrow: menu entry) | Fix | Route document-only commands invoked from a menu (including `common.toggle-text-mode`) to the nearest ancestor with a document, using the existing `matching_ancestor` idea; `view/invoke`, `view/key` and `view/setMode` already reach the document. Removing the dispatch entry would lose discoverability and fixing only that one entry would leave the class broken. T114 owns the server change; the wire does not change. | No |
| 9. Standard LSP reads and rich command versioning are different policies | Real (documentation) | Document both | Standard positional reads use the last acknowledged tree, the current tree before the first acknowledgement, and `null` in text mode. Rich `view/invoke`/`view/key` commands use the exact acknowledged version or, under the acknowledgement and validated-offer rules, the exact current version. Keep both policies and write them separately; collapsing them would be a behavior change no client needs. Documented in PROTOCOL §4.1. | No |
| 10. Historical prompt wording vs. maintained docs and transport | Real (documentation drift) | Document current transport | Keep the notification plus `view/promptRespond` transport (decision 5) and correct ARCHITECTURE §3.6 and PROTOCOL §4.2. The implementation and `CLIENT_SPEC.md` already agree. | No |
| 11. VS Code declares layouts it does not honor | Not real (fixed by T004) | Reject change | Keep `layout: ["main"]`. T004 made the declaration strict and checked; a future client that honors more placements declares them as new work. | No |
| W1. `PROTOCOL` omits `view/invoke.version` | Real (documentation) | Fix documentation | Add `version?` to the §4.1 row and cross-reference the read policy in finding 9. Clients already send it and rich commands use it for positional resolution. | No |
| W2. `PROTOCOL` lists nonexistent keymap binding fields | Real (documentation) | Fix documentation | The wire binding is `{command, keys, modes, label, status}`. Remove `doc` and `group` from the documentation only; they never existed on the wire, so no wire field is deleted. Corrected in PROTOCOL §4.2 and ARCHITECTURE §3.9. | No |
| W3. A mode-only render reuses the current version | Real (minor ambiguity) | Document | A mode-only render is the third render form: it carries no `text` or `edits` and does not advance `version`. Clients must apply `mode` regardless of render status. Advancing the version would make entering text mode look like a document update and neither client needs it. Documented in PROTOCOL §4.2. | No |

**No false dead-field removals.** Findings 3, 6 and 11 are not real, so no field
or code path is removed for them: the draft `runId` keymap path and
`keyModifiers` stay, and the `main`-only layout declaration stays. The only
documentation fields removed are W2's `doc` and `group`, which were never sent.

## 5. Prompt transport

A prompt is a server notification plus a client request, not a server request:

1. The server sends `view/prompt` as a notification carrying `promptId` and the
   prompt fields.
2. The client shows the prompt and answers with the `view/promptRespond`
   request carrying `{promptId, cancelled, value?}`.
3. `cancelled: true` stops the command quietly.

This is the current transport and stays. It is written in ARCHITECTURE §3.6 and
PROTOCOL §4.2. **Reason:** the server blocks on a channel for the answer, so a
notification plus a separate client request gives the same result without
pretending the server request carries the answer; both shipped clients already
implement it.

## 6. VS Code presentation defaults

**Menus use QuickPick by default.** A VS Code menu renders as a QuickPick, not
as a text document. **Reason:** the static client owns no server text buffer for
menus, QuickPick is native and keyboard-driven, and a document menu would add a
second surface and mode plumbing without helping the current client. A future
client that cannot present a quick picker may use the core render forms.

**Fold shortcuts are mirrored explicitly, never intercepted.** The extension
cannot outrank a user's `editor.toggleFold` binding, so it ships default
bindings on VS Code's native fold keys for `magit.active` views and offers an
explicit, consent-based "Magit: Mirror my fold shortcuts" command that reads
`keybindings.json` and appends equivalent `magit.invoke` entries. It never
watches or rewrites the user's file automatically. When the user changes
`editor.toggleFold`, only the next explicit mirror run reflects the change.
**Reason:** silently editing user configuration is unacceptable, the native
command acts on VS Code's own folding model rather than the server tree, and an
on-demand mirror keeps the user in control.

## 7. Documentation changed by this ticket

- ARCHITECTURE §3.6: prompt request → notification plus `view/promptRespond`
  (decision 5, finding 10).
- ARCHITECTURE §3.9: keymap binding fields, advisory `version` (finding 1, W2);
  static-client replay limit (finding 7); explicit fold mirroring (decision 6).
- PROTOCOL §4.1: `view/invoke.version?`, the standard-read vs. rich-command
  policy (finding 9, W1), and the planned version negotiation marked as not
  implemented (decision 3).
- PROTOCOL §4.2: the three render forms and multi-edit order (finding 2, W3),
  one retained acknowledgement and `view/resync` (finding 4), keymap fields and
  `version` (finding 1, W2), prompt notification and `view/promptRespond`
  (finding 10), and the static-client `passThrough` limit (finding 7).

The new decision record is not in the Makefile `DOC_FILES` list; `make
docs-check` checks the two edited reference files and the link to this record,
not this file's own content.
