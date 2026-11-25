#!/bin/sh
# UI-harness server wrapper: widen the deferred status open promise so the
# background diff fetch always installs the expanded render before the promise
# expires. Pass this file as MAGIT_LSP_BIN; the harness owns that variable (a
# scenario `env:` block cannot redirect it), so select the wrapper from the
# caller environment instead.
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
exec "${MAGIT_LSP_REAL_BIN:-$root/target/debug/magit-lsp}" --open-promise-ms 30000 "$@"
