# Neovim opening investigation

Source inspection: 2026-11-21, base `5d1da91e6527adb1d060171d75b61cae62a59369`.
Implementation and measurements remain pending: worker execution was blocked by
the account usage limit. This is a hypothesis packet, not a performance result.
Canonical task: [PERF-MAGIT-OPEN](../TASKS.md).

## Verified path

- `lua/magit-lsp/init.lua:427-435`: initial status opening waits for LSP
  initialization, version checking and highlight legend application.
- `lua/magit-lsp/init.lua:134-154`: `view/open` must return its descriptor
  before the client calls `view.open`. This request has no `traceId`.
- `src/server/view_execution.rs:24-28,32-55`: opening discovers/canonicalizes
  the Git root and creates a repository session if needed.
- `src/server/view_execution.rs:58-82,90-92`: the server acquires the session
  lock, opens the view and obtains its render, then ensures the watcher and
  prepares render delivery. These steps precede completion of this handler.
- `src/session.rs:2507` constructs a status view through `StatusView::open`.
- `src/status_view.rs:40-51`: status construction synchronously reads
  `status_listing` and `rebase_state`, with maximum detail as its initial level.
- `lua/magit-lsp/init.lua:166-170`: opening an already registered status view
  simply focuses it. This is not equivalent to reopening a closed status view.

## Ranked working hypotheses (not measured)

1. **Repository work before first display.** Status collection and view/render
   construction are on the open handler's critical path. Large dirty repositories
   could cost more than clean fixtures; default maximum detail is another reason
   to inspect subsequent hunk loading/rendering, not proof that it dominates.
   Discriminate with Git spans versus total handler time, clean/dirty fixtures,
   and matched repository size. Inspect actual hunk loading before suggesting
   lazy loading or caching.
2. **Cold server initialization.** First open cannot request status until
   `on_init`; process creation, handshake and client setup may dominate first
   use but not a genuine warm reopen. Time command, process launch, initialize
   reply and `view/open` separately.
3. **Serialization or watcher setup.** Opening takes the repository session
   lock and ensures a watcher before delivery. Concurrent work or watcher setup
   might delay opening. Separate lock wait, render work and watcher creation;
   compare idle and busy sessions. No contention has been demonstrated.
4. **Client work after server completion.** Descriptor processing, document
   application and editor redraw may account for a gap after server work.
   Compare response/render reception, document application and an attached
   UI's redraw notification. Buffer contents alone do not prove displayed pixels.

## Required timing check

Add `make nvim-startup-check` with maintained Python/Lua helpers. Run real
Neovim through public `:Magit`, using text/structured state only. Record:

- Process-cold first open (not a claim of cold filesystem caches), reopened
  status using an initialized server, and already-open focus as distinct cases.
- Repeated fixture and repository samples, environment, server path/build
  profile, editor version and median/range with raw JSON evidence.
- Command, process/initialize, open request/response, server handler, Git,
  render reception/application and UI redraw boundaries. Report overlap and
  clock precision rather than adding overlapping spans.
- Opt-in trace correlation on initial `view/open` so existing server/Git
  traces can explain startup. Confirm correlation survives through dispatch.
- A failure exit for missing events, invalid status document/window or process
  cleanup failure; no arbitrary speed threshold.

No dominant delay or improvement is established yet. A clean-profile run also
cannot explain a user's configuration without a matching comparison.
