.PHONY: build test test-rust test-lua test-file ui-unit ui-test ui-tour plain-neovim color-spike vscode-test readme-shots hygiene format-check hooks-install hooks-remove hooks-run format update-client-behaviors docs-check docs-generate docs-artifacts-check

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

hygiene:
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/hygiene.py

DOC_FILES ?= TASKS.md AGENTS.md README.md ui-test-plan.md hygiene-allowlist.md LICENSING.md docs/ARCHITECTURE.md docs/PROTOCOL.md docs/KEYMAPS.md docs/VIEWS.md docs/TESTING.md docs/OPEN_QUESTIONS.md docs/CLIENT_SPEC.md docs/parity.md docs/plain-support.md docs/known-client-behaviors.md docs/schema/payload-tables.md docs/schema/worked-exchanges.md docs/schema/capability-levels.md

docs-check:
	$(call run,docs-check,PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/docs -p 'test_*.py' && PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_docs.py $(DOC_FILES))

format:
	python3 scripts/format.py --working-tree --write

format-check:
	python3 scripts/format.py --working-tree

hooks-install:
	scripts/install-hooks.sh install

hooks-remove:
	scripts/install-hooks.sh remove

hooks-run:
	.githooks/pre-commit

# `make test` covers the Rust, Neovim and VSCode suites. The real-UI Python
# suites are separate: `make ui-unit` (harness) and `make ui-test`/`make ui-tour`
# (scripted Neovim screens) must be run when touching the UI harness or
# user-visible rendering.
test: build test-rust test-lua vscode-test

vscode-test: build
	$(call run,vscode-typecheck,npm --silent --prefix clients/vscode run typecheck)
	$(call run,vscode,npm --silent --prefix clients/vscode test)
	$(call run,keybindings,node clients/vscode/scripts/gen-keybindings.mjs --check)

vscode-keybindings-generate:
	$(call run,vscode-keybindings-generate,node clients/vscode/scripts/gen-keybindings.mjs)

vscode-typecheck:
	$(call run,vscode-typecheck,npm --silent --prefix clients/vscode run typecheck)

vscode-unit:
	$(call run,vscode-typecheck,npm --silent --prefix clients/vscode run typecheck)
	$(call run,vscode-unit,node clients/vscode/test/test.js)

test-rust:
	$(call run,rust,cargo test -q $(FILTER))

test-lua: build
	$(call run,lua,MAGIT_LSP_BIN="$(CURDIR)/target/debug/magit-lsp" nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run()")

test-file: build
	$(call run,lua,MAGIT_LSP_BIN="$(CURDIR)/target/debug/magit-lsp" nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run_file('$(FILE)')")

ui-unit:
	$(call run,ui-unit,PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/ui -p 'test_*.py')

PLAIN_ARGS ?=
plain-neovim: build
	$(call run,plain-neovim,MAGIT_LSP_BIN="$(CURDIR)/target/debug/magit-lsp" python3 scripts/test_plain_neovim.py --binary "$(CURDIR)/target/debug/magit-lsp" --evidence "$(CURDIR)/build/ui/plain-render" --size 120x100 $(PLAIN_ARGS))

ui-test: build
	$(call run,ui-test,PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run $(if $(V),-v) tests/ui/scenarios/*.scn tests/ui/smoke.scn)

ui-tour: build
	$(call run,ui-tour,PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run $(if $(V),-v) tests/ui/tour.scn tests/ui/tour-clean.scn tests/ui/tour-error.scn --markdown build/ui/tour.md)

color-spike: build
	$(call run,color-spike,nvim --headless --noplugin -u ./scripts/minimal_init.lua -l ./scripts/color_spike.lua)

readme-shots:
	@printf '%s\n' 'Human-only README image guidance: docs/SCREENSHOTS.md'

update-client-behaviors:
	$(call run,update-client-behaviors,UPDATE_CLIENT_BEHAVIORS=1 cargo test -q --test quirks known_client_appendix_matches_registry)

.PHONY: update-schema schema-check

update-schema:
	$(call run,update-schema,cargo run -q --bin generate-schema)

schema-check:
	$(call run,schema-check,cargo run -q --bin generate-schema -- --check)

registry-docs-generate:
	$(call run,registry-docs-generate,cargo run -q --bin generate-docs -- $(if $(ROOT),--output-root $(ROOT)))

registry-docs-check:
	$(call run,registry-docs-check,cargo run -q --bin generate-docs -- --check $(if $(ROOT),--output-root $(ROOT)))

.PHONY: docs-generate docs-artifacts-check

docs-generate: update-schema registry-docs-generate

docs-artifacts-check: schema-check registry-docs-check
