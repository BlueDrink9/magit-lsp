# Cargo server registration sweep: evidence

> Historical evidence only. Linux-only verification; macOS and Windows remain
> pending CI. Current evidence policy: [AGENTS.md no-vision policy](../../AGENTS.md#working-conventions).

Base commit: `3e99a93` (`TASKS.md:349`, TESTSUPPORT-SPAWN-PORT ticked).

## Problem

`crates/test-support/src/stdio.rs` resolves the server executable through
`MAGIT_LSP_BIN`, then the path registered by `cargo_server_binary!()`, then a
two-parent walk fallback (`stdio.rs:59-82`). The walk is only correct on the
machine that built the harness in place: it is wrong wherever Cargo places the
server elsewhere. CI run 45726381910 lost 25 `editor_bridge` tests on the
required macOS lane (`ci.yml:189-191`) and
`errors::not_a_repository_returns_a_message_effect` on the informational Windows
lane (`ci.yml:186-188`); both panicked `spawn magit-lsp: NotFound` at
`stdio.rs:193`.

Commit `71365db` added `cargo_server_binary!()`, which expands
`env!("CARGO_BIN_EXE_magit-lsp")` inside the invoking test crate and registers it
first-write-wins (`stdio.rs:21-50`). `tests/capability_lattice.rs` was the only
suite registering it.

## Sweep

Inserted `magit_lsp_test_support::cargo_server_binary!();` as the first statement
of every test or helper whose body reaches a server resolution
(`TestServer::spawn*`, `Server::spawn`, or bare `server_binary()`). Repeats are
harmless because registration is first-write-wins. `crates/test-support/src/stdio.rs`
and the walk fallback are unchanged.

| File | Registrations |
| --- | ---: |
| tests/errors.rs | 1 |
| tests/examples.rs | 2 |
| tests/navigation.rs | 1 |
| tests/quirks.rs | 3 |
| tests/text_mode.rs | 2 |
| tests/wirelog.rs | 4 |
| tests/stdout_hygiene.rs | 2 |
| tests/semantic_tokens.rs | 3 |
| tests/rebase_interactive.rs | 9 |
| tests/plain_harness.rs | 13 |
| tests/plain_repo.rs | 1 |
| tests/plain_draft.rs | 1 |
| tests/plain_select.rs | 1 |
| tests/plain_stack.rs | 2 |
| tests/plain_sync.rs | 2 |
| tests/plain_actions.rs | 5 |
| tests/plain_render.rs | 5 |
| tests/plain_vis.rs | 2 |
| tests/plain_keys_exec.rs | 1 |
| tests/plain_keys_arm.rs | 1 |
| tests/plain_draft_windows.rs | 1 |
| tests/server_async.rs | 32 |
| tests/server_async/repository.rs | 22 |
| tests/server_async/rich.rs | 11 |
| tests/editor_bridge/cases.rs | 17 |
| tests/editor_bridge/additional.rs | 14 |
| **Total** | **158** |

Not edited: `tests/capability_lattice.rs` (already registered at the three
funnels: `:47`, `:357`, `:643`/`:655`/`:674`).

## A2 grep audit

Scope: `tests/**/*.rs` excluding `tests/release/`. Every file matching
`TestServer::spawn|Server::spawn|server_binary\(\)` must also match
`cargo_server_binary!()` or `env!("CARGO_BIN_EXE_magit-lsp")`.

```sh
for f in $(rg -l 'TestServer::spawn|Server::spawn|server_binary\(\)' tests \
    --glob '*.rs' -g '!tests/release/**'); do
  rg -q 'cargo_server_binary!\(\)|env!\("CARGO_BIN_EXE_magit-lsp"\)' "$f" \
    || echo "MISSING $f"
done
```

Result: no `MISSING` lines. 27 spawning files matched; the 26 edited above plus
`tests/capability_lattice.rs`. No spawning file remains walk-only.

Exempt (direct `env!` only, unchanged): `tests/instant_rebase.rs:71`,
`tests/manifest.rs:24`, `tests/parity.rs:147`. `tests/runner.rs` is not a server
spawner (fake-Git child, `runner.rs:32`).

## Verification (Linux)

- `make test-rust FILTER="--locked --no-run"` — all touched suites compile with
  their registrations.
- `make test-rust FILTER="--locked --test editor_bridge"` — green.
- `make test-rust FILTER="--locked --test errors"` — green.

These are necessary but not sufficient: the suites also pass locally through the
walk fallback, so green here proves the registration compiles and precedes every
spawn, not that Cargo placed the server elsewhere.

## Pending oracle

The real oracle is the next macOS and Windows `cargo test --workspace --locked`
run (`ci.yml:186-204`). Until that CI evidence exists, macOS and Windows
behavior is unverified and is not claimed here.
