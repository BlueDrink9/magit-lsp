# Trace and capture cleanup strategy

## Findings

| Asset | Actual consumer / value | Decision |
|---|---|---|
| `tests/wirelog.rs:11+` and `:99–587` | Parses retained JSONL and validates selected sidecar rows, source indices, hashes and action claims. Provenance/fixture integrity only; it does not exercise server behavior. | Keep provenance checks that enforce explicit claims; remove redundant row-shape assertions only after mapping each claim to a retained consumer. |
| `tests/wirelog.rs:589+` | Spawns the real server. Checks opt-in recording, disabled behavior, concurrent writers/restarts, unavailable destination, redaction, ordering and real framing. | Keep: this is actual recorder/runtime coverage, distinct from fixture audit. Deduplicate helpers/lifecycle only where semantics remain asserted. |
| `tests/quirks.rs` | Loads plain and bundled Neovim 0.12.6 plus Helix 25.07.1 initialize presets; tests identity/version matching, capability rules and overrides. `src/quirks.rs` currently encodes those client/version boundaries. | Keep the three presets while production registry depends on them. Do not infer behavior coverage from captured provenance. |
| `tests/plain_harness.rs:242` | Parses each line of `reconstructed-plain.jsonl` as JSON. No replay, protocol conformance or editor behavior assertion. | Delete this test and reconstructed fixture unless a named consumer/claim is established; it adds no behavior coverage. |
| `tests/fixtures/current-client-capabilities.json` | `tests/plain_core.rs` strict-deserializes/negotiates its two current-client declarations; producer provenance is documented in `docs/evidence/typed-capability-declarations.md`. | Keep schema/negotiation test and evidence. This fixture is not interchangeable with historical initialize traces. Add producer drift enforcement only if independently feasible and scoped. |
| `tests/fixtures/lsp_traces/README.md` | Human reproduction/provenance, source hashes, retained row maps and limitations; includes embedded capture recipes and `/tmp/opencode` paths. | Preserve historical evidence. Separate reproducible recipes from machine-specific old paths; do not rewrite history as current behavior. |
| `scripts/test_plain_neovim.py` | Runs Plain Neovim synchronization acceptance and retains bounded `lsp.log` excerpts in run evidence. | Keep as behavior test. Its runtime trace is diagnostic evidence, not fixture source or a substitute for assertions. |
| `scripts/ui_lib/driver.py` | Shared isolated native Neovim/UI lifecycle, exposes `extra_env` for recorder injection; used by README recipe. | Prefer this maintained driver over bespoke Neovim setup. Keep lifecycle ownership and cleanup centralized. |
| `clients/vscode/test/e2e/run.mjs` | Native VSCode host launcher; trace README invokes it with `MAGIT_LSP_WIRELOG`. | Keep native path for VSCode-specific evidence; do not replace with Neovim evidence or claim a current capture from historical logs. |

## Duplication, portability, evidence limits

The recorder's Rust integration tests and trace-fixture tests both inspect JSONL, but test different contracts: current-server recorder mechanics versus historical source/sidecar integrity. Avoid collapsing them into one generic parser test. Plain harness's reconstructed-JSONL parse is uniquely redundant; it neither drives fake editor nor server. Native recipes in the trace README contain lifecycle/setup logic distinct from fake-editor and UI tests. Reuse `ui_lib.driver` where it supports the required native action; retain VSCode's own launcher and Helix as manual smoke only, not a supported harness (AGENTS.md).

The trace README records unavailable `/tmp/opencode` source paths and missing exact launch/cleanup commands for some captures. These paths are historical provenance, not runtime dependencies. Embedded recipes using `/tmp/opencode` paths, installed editor binaries, shell process tools or editor-version assumptions are machine-specific; make future capture destinations temporary/repository-independent and report prerequisites explicitly. Current Linux-only evidence does not establish Windows/macOS execution; retain CI follow-ups and make no cross-platform claim. No builds, tests, native captures, or recapture were run for this audit.

## Ordered cleanup slices

1. **Consumer inventory and assertion mapping:** classify every wirelog test and fixture assertion as runtime recorder, historical provenance, current declaration contract, or dead parse-only check. Confirmed candidate: remove `plain_harness::reconstructed_trace_is_valid_jsonl` and its fixture only after checking no other consumer.
2. **Deduplicate without losing claims:** consolidate duplicated JSONL/sidecar helpers and owned process/temp-path cleanup only when source-row, ordering, redaction, shutdown, and failure semantics remain explicitly tested. Preserve `wirelog.rs:589+` runtime coverage and three quirk presets.
3. **Recipe portability:** move only still-needed capture recipes to standalone tooling after documenting stable prerequisites, temporary output ownership, bounded waits, cleanup-on-failure, and source/hash verification. Prefer existing UI driver; no extraction in this audit.
4. **Evidence maintenance:** retain immutable historical capture claims and current declaration fixture coverage separately. Run focused Linux checks after each implementation slice; schedule Windows/macOS CI where applicable. Native behavior must be revalidated only for changed user-visible claims, using text/structured evidence.

Removal is not authorized by historical age or path inconvenience alone. Each slice must recheck consumers and state exact deleted claims; this report does not implement cleanup or revalidate native behavior.
