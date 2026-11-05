## Retained client declarations

The three `initialize-*.json` files are inputs to `tests/quirks.rs`. They retain
captured initialize parameters for plain Neovim 0.12.6, bundled Neovim 0.12.6,
and Helix 25.07.1. `tests/fixtures/current-client-capabilities.json` separately
holds current Neovim and VS Code declarations used by capability negotiation.
Tests validate stored declarations; they do not recapture clients or prove
native causality.

The retained inputs are immutable historical captures from before trace cleanup
2. See Git `83f79f5cbf2e272a8be92673ef3c8bd41118c23e`:
`tests/fixtures/lsp_traces/initialize-plain-neovim-0.12.6.json`,
`tests/fixtures/lsp_traces/initialize-bundled-neovim-0.12.6.json`,
`tests/fixtures/lsp_traces/initialize-helix-25.07.1.json`, and
`tests/fixtures/current-client-capabilities.json`.

No supported Helix harness exists. Historical native evidence has bounded
coverage and cannot establish general client conformance. Retired capture
recipes and sidecars are not supported workflows. Retained runtime workflows
are maintained through `make test-rust FILTER="--test quirks"` and
`make test-rust FILTER="--test plain_core"`. Historical README and captures
remain as provenance at immutable Git revision
`83f79f5cbf2e272a8be92673ef3c8bd41118c23e`.
For maintained native Plain Neovim acceptance, use [`make plain-neovim`](../../../ui-test-plan.md#retained-native-plain-neovim-workflow).
