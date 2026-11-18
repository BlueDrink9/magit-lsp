.PHONY: build release test test-rust test-lua test-file ui-unit ui-test ui-file ui-tour plain-neovim update-snapshots update-examples registry-docs-generate registry-docs-check color-spike vscode-test readme-shots hygiene format-check hooks-install hooks-remove hooks-run format docs-check

.SILENT:
MAKEFLAGS += --no-print-directory
export CARGO_TERM_COLOR=never
export CARGO_TERM_PROGRESS_WHEN=never
LOG_TAIL ?= 60
# Always test the freshly built debug server, never a stale release binary.
# (Confirm scripts/ui and clients/vscode read MAGIT_LSP_BIN.)
export MAGIT_LSP_BIN := $(CURDIR)/target/debug/magit-lsp
# Optional: isolate tests from your Git config. Fixtures may then need
# explicit GIT_AUTHOR_*/GIT_COMMITTER_* env, and private git deps may break.
# export GIT_CONFIG_GLOBAL := /dev/null
# export GIT_CONFIG_NOSYSTEM := 1

# Machine-local overrides (gitignored), e.g.
#   export MAGIT_VSCODE_BIN := /opt/env/vscode/bin/codium
-include local.mk

# Convention: targets that rewrite files must be named update-*, *-generate,
# hooks-* or format, because .opencode/agents/scout.md denies those patterns.
# Name any new mutating target to match, or add a deny rule there.

define run
@if [ -n "$(V)" ]; then $(2); else mkdir -p build/test-logs || exit $$?; log=$$(mktemp build/test-logs/$(1).XXXXXX) || exit $$?; if $(2) >"$$log" 2>&1; then status=0; printf 'ok: $(1)\n'; rm -f "$$log"; else status=$$?; tail -n $(LOG_TAIL) "$$log"; printf 'FAILED: $(1) (full log: %s)\n' "$$log"; fi; exit $$status; fi
endef

build:
	$(call run,build,cargo build -q)

release:
	$(call run,release,cargo build --release -q)

hygiene:
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/hygiene.py

DOC_FILES ?= TASKS.md AGENTS.md README.md ui-test-plan.md hygiene-allowlist.md LICENSING.md docs/ARCHITECTURE.md docs/PROTOCOL.md docs/KEYMAPS.md docs/VIEWS.md docs/TESTING.md docs/OPEN_QUESTIONS.md docs/CLIENT_SPEC.md docs/parity.md docs/plain-support.md docs/known-client-behaviors.md docs/schema/payload-tables.md docs/schema/worked-exchanges.md docs/schema/worked-exchanges-01.md docs/schema/worked-exchanges-02.md docs/schema/worked-exchanges-03.md docs/schema/capability-levels.md

docs-check:
	$(call run,docs-check,PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/docs -p 'test_*.py' && PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_docs.py $(DOC_FILES))

format:
	python3 scripts/format.py --working-tree --write

format-check:
	$(call run,format-check,python3 scripts/format.py --working-tree)

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

vscode-keybindings-generate:
	$(call run,vscode-keybindings-generate,node clients/vscode/scripts/gen-keybindings.mjs)

test-rust:
	$(call run,rust,cargo test -q $(FILTER))

# Regenerate only the two commit-edit exchanges from the real-server replay.
update-examples:
	$(call run,update-examples,MAGIT_LSP_REGENERATE_EXAMPLES=1 cargo test -q --test examples plain_examples_replay_against_the_real_binary)

test-lua: build
	$(call run,lua,MAGIT_LSP_BIN="$(CURDIR)/target/debug/magit-lsp" nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run()")

test-file: build
	$(call run,lua,MAGIT_LSP_BIN="$(CURDIR)/target/debug/magit-lsp" nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run_file('$(FILE)')")

ui-unit:
	$(call run,ui-unit,PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/ui -p 'test_*.py')

PLAIN_ARGS ?=
plain-neovim: build
	$(call run,plain-neovim,MAGIT_LSP_BIN="$(CURDIR)/target/debug/magit-lsp" python3 scripts/test_plain_neovim.py --binary "$(CURDIR)/target/debug/magit-lsp" --evidence "$(CURDIR)/build/ui/plain-render" --size 120x100 $(PLAIN_ARGS))

.PHONY: plain-presets update-capability-presets
plain-presets: build
	$(call run,plain-presets,MAGIT_LSP_BIN="$(CURDIR)/target/debug/magit-lsp" python3 scripts/test_plain_presets.py --binary "$(CURDIR)/target/debug/magit-lsp")

update-capability-presets:
	$(call run,update-capability-presets,PYTHONDONTWRITEBYTECODE=1 python3 scripts/test_plain_presets.py --generate)

.PHONY: helix-smoke helix-smoke-stop
HELIX_BIN ?= hx
HELIX_ARGS ?=
helix-smoke: build
	python3 scripts/helix_smoke.py --helix "$(HELIX_BIN)" $(HELIX_ARGS)

helix-smoke-stop:
	python3 scripts/helix_smoke.py --stop $(HELIX_ARGS)

ui-test: build
	$(call run,ui-test,PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run $(if $(V),-v) tests/ui/scenarios/*.scn tests/ui/smoke.scn)

ui-file: build
	$(call run,ui-file,PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run $(if $(V),-v) $(FILE))

update-snapshots:
	$(call run,update-snapshots,UPDATE_EXPECT=1 cargo test -q --test manifest --test parity --test menu --test menus)

.PHONY: update-schema schema-check

vscode-deps:
	$(call run,vscode-deps,pnpm --dir clients/vscode install --frozen-lockfile --store-dir "$${PNPM_STORE_PATH:-$${XDG_CACHE_HOME:-$$HOME/.cache}/magit-lsp/pnpm-store}")

# Fast check before the native VSCode targets: names what is missing instead of
# failing deep inside a run. Set MAGIT_VSCODE_BIN in local.mk. Xvfb is not
# needed when DISPLAY is already set.
.PHONY: vscode-prereqs
vscode-prereqs:
	missing=""; \
	command -v Xvfb >/dev/null 2>&1 || missing="$$missing Xvfb"; \
	{ [ -x "$(MAGIT_VSCODE_BIN)" ] || command -v code >/dev/null 2>&1; } || missing="$$missing code(set MAGIT_VSCODE_BIN in local.mk)"; \
	[ -z "$$missing" ] || { printf 'MISSING:%s\n' "$$missing"; exit 1; }

vscode-test: build vscode-deps
	$(call run,vscode-typecheck,pnpm --dir clients/vscode run typecheck)
	$(call run,vscode,pnpm --dir clients/vscode test)
	$(call run,keybindings,node clients/vscode/scripts/gen-keybindings.mjs --check)

vscode-typecheck: vscode-deps
	$(call run,vscode-typecheck,pnpm --dir clients/vscode run typecheck)

vscode-unit: vscode-deps
	$(call run,vscode-typecheck,pnpm --dir clients/vscode run typecheck)
	$(call run,vscode-unit,node clients/vscode/test/test.js)

vscode-version-startup: vscode-prereqs build vscode-deps
	$(call run,vscode-version-startup,MAGIT_VSCODE_VERSION_ONLY=1 node clients/vscode/test/e2e/run.mjs)

vscode-version-warning: build vscode-deps
	$(call run,vscode-version-warning,MAGIT_VSCODE_VERSION_WARNING_ONLY=1 node clients/vscode/test/e2e/run.mjs)

vscode-e2e: build vscode-deps
	$(call run,vscode-e2e,MAGIT_VSCODE_A06_ONLY=1 node clients/vscode/test/e2e/run.mjs)

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

ui-tour: build
	$(call run,ui-tour,PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run $(if $(V),-v) tests/ui/tour.scn tests/ui/tour-clean.scn tests/ui/tour-error.scn --markdown build/ui/tour.md)

color-spike: build
	$(call run,color-spike,nvim --headless --noplugin -u ./scripts/minimal_init.lua -l ./scripts/color_spike.lua)

readme-shots:
	@printf '%s\n' 'Human-only README image guidance: docs/SCREENSHOTS.md'
