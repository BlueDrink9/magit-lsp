#!/bin/sh
# UI-harness server wrapper: widen the deferred status open promise so the
# background diff fetch always installs the expanded render before the promise
# expires. Pass this file as MAGIT_LSP_TEST_BINARY (`make ui-file` uses it as
# MAGIT_LSP_BIN); the harness overwrites MAGIT_LSP_BIN, so it must be this
# variable, not that one.
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
exec "${MAGIT_LSP_REAL_BIN:-$root/target/debug/magit-lsp}" --open-promise-ms 30000 "$@"
