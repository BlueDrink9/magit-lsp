.PHONY: build test test-rust test-lua test-file ui-unit ui-test ui-tour color-spike vscode-test

.SILENT:
MAKEFLAGS += --no-print-directory
export CARGO_TERM_COLOR=never
export CARGO_TERM_PROGRESS_WHEN=never
LOG_TAIL ?= 150

define run
@if [ -n "$(V)" ]; then $(2); else mkdir -p build/test-logs || exit $$?; log=$$(mktemp build/test-logs/$(1).XXXXXX) || exit $$?; if $(2) >"$$log" 2>&1; then status=0; printf 'ok: $(1)\n'; rm -f "$$log"; else status=$$?; tail -n $(LOG_TAIL) "$$log"; printf 'FAILED: $(1) (full log: %s)\n' "$$log"; fi; exit $$status; fi
endef

build:
	$(call run,build,cargo build -q)

# `make test` covers the Rust, Neovim and VSCode suites. The real-UI Python
# suites are separate: `make ui-unit` (harness) and `make ui-test`/`make ui-tour`
# (scripted Neovim screens) must be run when touching the UI harness or
# user-visible rendering.
test: build test-rust test-lua vscode-test

vscode-test: build
	$(call run,vscode,npm --silent --prefix clients/vscode test)
	$(call run,keybindings,node clients/vscode/scripts/gen-keybindings.mjs --check)

test-rust:
	$(call run,rust,cargo test -q $(FILTER))

test-lua: build
	$(call run,lua,nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run()")

test-file: build
	$(call run,lua,nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run_file('$(FILE)')")

ui-unit:
	$(call run,ui-unit,PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/ui -p 'test_*.py')

ui-test:
	$(call run,ui-test,PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run $(if $(V),-v) tests/ui/scenarios/*.scn tests/ui/smoke.scn)

ui-tour:
	$(call run,ui-tour,PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run $(if $(V),-v) tests/ui/tour.scn tests/ui/tour-clean.scn tests/ui/tour-error.scn --markdown build/ui/tour.md)

color-spike: build
	$(call run,color-spike,nvim --headless --noplugin -u ./scripts/minimal_init.lua -l ./scripts/color_spike.lua)
