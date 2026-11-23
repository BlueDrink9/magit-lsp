.PHONY: build test-parallel vscode-host-spike release release-locked release-artifact-check release-native-build release-pipeline-check release-automation-unit release-publish release-contract-unit native-artifact-unit native-artifact-inspect native-artifact-check native-vscode-artifact-check vsix-candidate-unit test test-rust test-rust-artifact test-lua test-file ui-unit ui-test ui-file ui-tour native-neovim-artifact-check plain-neovim lazy-install-check coverage-rust update-snapshots update-examples registry-docs-generate registry-docs-check vscode-test readme-shots hygiene format-check hooks-install hooks-remove hooks-run format format-all format-all-check format-all-check-worktree format-all-list format-test docs-check vscode-deps-refresh vscode-download-startup release-automation-check release-contract-check workflows-check mason-package-check nvim-startup-check nvim-startup-strace nvim-startup-perf

.SILENT:
MAKEFLAGS += --no-print-directory
export CARGO_TERM_COLOR=never
export CARGO_TERM_PROGRESS_WHEN=never
LOG_TAIL ?= 60
# Warning-log heuristic: on a successful logged command, keep the full log and
# print its path when a line looks like a structural warning diagnostic (a
# Rust/compiler/general `warning:` line, a compiler `file:line[:col]: warning:`
# line, or a Python `CategoryWarning:` line with or without a location). This
# deliberately does not match incidental prose that merely contains the word
# "warning" (e.g. "0 warnings", "warning count zero"). See docs/TESTING.md.
WARN_RE := ^[[:space:]]*[Ww]arning:|^[[:space:]]*[[:upper:]][[:alnum:]]*Warning:|^.+:[0-9]+(:[0-9]+)?:[[:space:]]*[Ww]arning:|^.+:[0-9]+:[[:space:]]*[[:upper:]][[:alnum:]]*Warning:
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
# hooks-*, *-refresh, format or format-all (help-tags is a fixed exception),
# because .opencode/agents/scout.md denies those patterns. Name any new
# mutating target to match, or add a deny rule there.

define run
@if [ -n "$(V)" ]; then $(2); else mkdir -p build/test-logs || exit $$?; log=$$(mktemp build/test-logs/$(1).XXXXXX) || exit $$?; if $(2) >"$$log" 2>&1; then status=0; printf 'ok: $(1)'; if grep -Eq '$(WARN_RE)' "$$log"; then printf ' (warnings detected; check log: %s)\n' "$$log"; else printf '\n'; rm -f "$$log"; fi; else status=$$?; tail -n $(LOG_TAIL) "$$log"; printf 'FAILED: $(1) (full log: %s)\n' "$$log"; fi; exit $$status; fi
endef

build:
	$(call run,build,cargo build -q)

release:
	$(call run,release,cargo build --release -q)

# The release workflow's pinned, locked build and actual-artifact check, for
# local reproduction without publication. Both targets run the same
# scripts/check_release_pipeline.py the workflow runs, so a change here cannot
# drift into a second CI-only build path. A machine without exactly the pinned
# Rust toolchain fails readably; the target never downloads or installs one.
release-locked:
	$(call run,release-locked,PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_release_pipeline.py build)

release-artifact-check: release-locked
	$(call run,release-artifact-check,PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_release_pipeline.py artifacts)

# Experimental hosted native-linker lane, separate from the strict pinned
# fallback above. Run only inside
# `nix develop --no-update-lock-file .#release-native -c make release-native-build
# RELEASE_TARGET=x86_64-unknown-linux-gnu`. It refuses a non-Linux/x86_64 host,
# a different target, inherited Nix CC/linker/sysroot flags or a missing host
# compiler, sets Cargo's target linker explicitly to /usr/bin/cc, and records
# the locked argv plus compiler/linker/libc/source/lock evidence under
# build/release/native-build.json before building.
release-native-build:
	$(call run,release-native-build,PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_release_pipeline.py build-native --target "$(RELEASE_TARGET)")

hygiene:
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/hygiene.py

DOC_FILES ?= TASKS.md AGENTS.md README.md ui-test-plan.md LICENSING.md docs/ARCHITECTURE.md docs/PROTOCOL.md docs/KEYMAPS.md docs/VIEWS.md docs/TESTING.md docs/OPEN_QUESTIONS.md docs/CLIENT_SPEC.md docs/parity.md docs/plain-support.md docs/PLAIN_KEYS.md docs/known-client-behaviors.md docs/schema/payload-tables.md docs/schema/worked-exchanges.md docs/schema/worked-exchanges-01.md docs/schema/worked-exchanges-02.md docs/schema/worked-exchanges-03.md docs/schema/capability-levels.md

docs-check:
	$(call run,docs-check,PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/docs -p 'test_*.py' && PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_docs.py $(DOC_FILES))

format:
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/format.py --working-tree --write

format-check:
	$(call run,format-check,PYTHONDONTWRITEBYTECODE=1 python3 scripts/format.py --working-tree)

# Explicit opt-in whole-repository sweep for one formatter. `format-all-list`
# prints the exact tracked eligible files and counts; use `format-all` only on a
# clean working copy of the selected files (it refuses to overwrite unstaged
# changes) and stage exactly those paths afterwards.
FORMATTER ?= rustfmt
FORMAT_CHOICES := rustfmt stylua prettier

format-all-list:
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/format.py --list

format-all-check:
	@case " $(FORMAT_CHOICES) " in *" $(FORMATTER) "*) ;; *) echo 'format-all-check: FORMATTER must be rustfmt, stylua or prettier' >&2; exit 2;; esac
	$(call run,format-all-check,PYTHONDONTWRITEBYTECODE=1 python3 scripts/format.py --all-tracked --formatter $(FORMATTER))

format-all-check-worktree:
	@case " $(FORMAT_CHOICES) " in *" $(FORMATTER) "*) ;; *) echo 'format-all-check-worktree: FORMATTER must be rustfmt, stylua or prettier' >&2; exit 2;; esac
	$(call run,format-all-check-worktree,PYTHONDONTWRITEBYTECODE=1 python3 scripts/format.py --all-tracked --source working-tree --formatter $(FORMATTER))

format-all:
	@case " $(FORMAT_CHOICES) " in *" $(FORMATTER) "*) ;; *) echo 'format-all: FORMATTER must be rustfmt, stylua or prettier' >&2; exit 2;; esac
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/format.py --all-tracked --formatter $(FORMATTER) --write

format-test:
	$(call run,format-test,PYTHONDONTWRITEBYTECODE=1 python3 scripts/test_format.py)

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

# Same three suite groups as `make test`, run concurrently, for a shorter
# local wall time; `test:` above stays the serial default and CI gate. One
# `build` must finish first (a prerequisite here), and `-o build` keeps the
# lua and vscode sub-makes from rebuilding it, so no two Cargo runs overlap.
# Each group is its own sub-make, so every ok:/fail digest still prints (lines
# may interleave). Every child pid is collected and waited on, so the recipe
# exits nonzero if any group fails.
test-parallel: build
	@pids=""; \
	for suite in test-rust test-lua vscode-test; do \
	  $(MAKE) -o build $$suite & pids="$$pids $$!"; \
	done; \
	status=0; \
	for pid in $$pids; do wait $$pid || status=1; done; \
	exit $$status

vscode-keybindings-generate:
	$(call run,vscode-keybindings-generate,node clients/vscode/scripts/gen-keybindings.mjs)

.PHONY: vscode-semantic-generate vscode-semantic-check vscode-semantic-test
vscode-semantic-generate:
	$(call run,vscode-semantic-generate,node clients/vscode/scripts/gen-semantic-roles.mjs)

vscode-semantic-check:
	$(call run,vscode-semantic-check,node clients/vscode/scripts/gen-semantic-roles.mjs --check)

vscode-semantic-test:
	$(call run,vscode-semantic-test,node --test clients/vscode/test/semantic-roles.test.mjs)

test-rust:
	$(call run,rust,cargo test -q $(FILTER))

# Real-pipe Rust suites against one explicitly supplied server executable. The
# native harness may be compiled here, but the server under test must be the
# supplied artifact, never the binary Cargo builds: `MAGIT_LSP_TEST_BINARY` is
# required, absolute and executable, and its SHA-256 is recorded before Cargo
# starts. The Rust harness is compiled separately from the selected server
# identity; there is no `build` (server) prerequisite. FILTER may select a
# focused hosted suite set without changing ordinary test-rust defaults.
MAGIT_LSP_TEST_BINARY ?=
RUST_ARTIFACT_FILTER ?= --test server_async
test-rust-artifact:
	@case "$(MAGIT_LSP_TEST_BINARY)" in \
	'') echo "test-rust-artifact: MAGIT_LSP_TEST_BINARY is required: absolute path to the verified server executable" >&2; exit 2;; \
	/*) if [ ! -f "$(MAGIT_LSP_TEST_BINARY)" ] || [ ! -x "$(MAGIT_LSP_TEST_BINARY)" ]; then \
	      echo "test-rust-artifact: MAGIT_LSP_TEST_BINARY is not an executable file: $(MAGIT_LSP_TEST_BINARY)" >&2; exit 2; \
	    fi;; \
	*) echo "test-rust-artifact: MAGIT_LSP_TEST_BINARY must be an absolute path, got: $(MAGIT_LSP_TEST_BINARY)" >&2; exit 2;; \
	esac
	@python3 -c 'import hashlib,sys; h=hashlib.sha256(); f=open(sys.argv[1], "rb"); [h.update(b) for b in iter(lambda: f.read(1048576), b"")]; print("test-rust-artifact: server sha256 %s %s" % (h.hexdigest(), sys.argv[1]))' "$(MAGIT_LSP_TEST_BINARY)"
	$(call run,rust-artifact,MAGIT_LSP_TEST_BINARY="$(MAGIT_LSP_TEST_BINARY)" cargo test -q $(if $(strip $(FILTER)),$(FILTER),$(RUST_ARTIFACT_FILTER)))

# Exercises the real `run` macro above through temporary included Makefiles.
.PHONY: wrapper-check
wrapper-check:
	$(call run,wrapper-check,PYTHONDONTWRITEBYTECODE=1 python3 scripts/test_make_wrapper.py)

# Rust-only line coverage via cargo-llvm-cov: LCOV lands under the gitignored
# build/ tree. Prerequisites are checked before any build.
coverage-rust:
	$(call run,coverage-rust,scripts/coverage-rust.sh)
	@cat build/coverage/summary.txt 2>/dev/null || true
	@printf 'coverage-rust: LCOV at %s\n' "$(CURDIR)/build/coverage/lcov.info"

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
	$(call run,plain-neovim,MAGIT_LSP_BIN="$(CURDIR)/target/debug/magit-lsp" PYTHONDONTWRITEBYTECODE=1 python3 scripts/test_plain_neovim.py --binary "$(CURDIR)/target/debug/magit-lsp" --evidence "$(CURDIR)/build/ui/plain-render" --size 120x100 $(PLAIN_ARGS))

.PHONY: plain-presets update-capability-presets
plain-presets: build
	$(call run,plain-presets,MAGIT_LSP_BIN="$(CURDIR)/target/debug/magit-lsp" PYTHONDONTWRITEBYTECODE=1 python3 scripts/test_plain_presets.py --binary "$(CURDIR)/target/debug/magit-lsp")

update-capability-presets:
	$(call run,update-capability-presets,PYTHONDONTWRITEBYTECODE=1 python3 scripts/test_plain_presets.py --generate)

.PHONY: helix-smoke helix-smoke-stop helix-config-check
HELIX_BIN ?= hx
HELIX_ARGS ?=
helix-smoke: build
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/helix_smoke.py --helix "$(HELIX_BIN)" $(HELIX_ARGS)

helix-smoke-stop:
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/helix_smoke.py --stop $(HELIX_ARGS)

helix-config-check: build
	$(call run,helix-config-check,PYTHONDONTWRITEBYTECODE=1 python3 scripts/test_helix_config.py --helix "$(HELIX_BIN)" --binary "$(CURDIR)/target/debug/magit-lsp" --evidence "$(CURDIR)/build/ui/helix-config-check")

lazy-install-check: build
	$(call run,lazy-install-check,PYTHONDONTWRITEBYTECODE=1 python3 scripts/test_lazy_install.py --binary "$(CURDIR)/target/debug/magit-lsp" --evidence "$(CURDIR)/build/ui/lazy-install")

.PHONY: mason-install-check
mason-install-check: build
	$(call run,mason-install-check,PYTHONDONTWRITEBYTECODE=1 python3 scripts/test_mason_install.py)

ui-test: build
	$(call run,ui-test,PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run $(if $(V),-v) tests/ui/scenarios/*.scn tests/ui/smoke.scn)

ui-file: build
	$(call run,ui-file,PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run $(if $(V),-v) $(FILE))

# Consume the exact externally supplied server; intentionally has no build prerequisite.
native-neovim-artifact-check:
	@case "$(MAGIT_LSP_TEST_BINARY)" in \
	'') echo "native-neovim-artifact-check: MAGIT_LSP_TEST_BINARY is required: absolute path to the verified server executable" >&2; exit 2;; \
	/*) if [ ! -f "$(MAGIT_LSP_TEST_BINARY)" ] || [ ! -x "$(MAGIT_LSP_TEST_BINARY)" ]; then \
	      echo "native-neovim-artifact-check: MAGIT_LSP_TEST_BINARY is not an executable file: $(MAGIT_LSP_TEST_BINARY)" >&2; exit 2; \
	    fi;; \
	*) echo "native-neovim-artifact-check: MAGIT_LSP_TEST_BINARY must be an absolute path, got: $(MAGIT_LSP_TEST_BINARY)" >&2; exit 2;; \
	esac
	@python3 -c 'import hashlib,sys; h=hashlib.sha256(); f=open(sys.argv[1], "rb"); [h.update(b) for b in iter(lambda: f.read(1048576), b"")]; print("native-neovim-artifact-check: server sha256 %s %s" % (h.hexdigest(), sys.argv[1]))' "$(MAGIT_LSP_TEST_BINARY)"
	$(call run,native-neovim-artifact-check,MAGIT_LSP_TEST_BINARY="$(MAGIT_LSP_TEST_BINARY)" MAGIT_LSP_BIN="$(MAGIT_LSP_TEST_BINARY)" PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run tests/ui/scenarios/status-basics.scn tests/ui/scenarios/commit-message-editor.scn)

nvim-startup-check: build
	$(call run,nvim-startup-check,PYTHONDONTWRITEBYTECODE=1 python3 scripts/measure_nvim_startup.py)

# Same public-command harness under strace; per-launch traces land in build/.
nvim-startup-strace: build
	$(call run,nvim-startup-strace,PYTHONDONTWRITEBYTECODE=1 python3 scripts/profile_nvim_startup.py)

# Same public-command harness under perf; sampled CPU stacks land in build/.
nvim-startup-perf: build
	$(call run,nvim-startup-perf,PYTHONDONTWRITEBYTECODE=1 python3 scripts/profile_nvim_startup.py --mode perf)


update-snapshots:
	$(call run,update-snapshots,UPDATE_EXPECT=1 cargo test -q --test manifest --test parity --test menu --test menus)

.PHONY: update-schema schema-check

vscode-deps:
	$(call run,vscode-deps,pnpm --dir clients/vscode install --frozen-lockfile --store-dir "$${PNPM_STORE_PATH:-$${XDG_CACHE_HOME:-$$HOME/.cache}/magit-lsp/pnpm-store}")

# Regenerate clients/vscode/pnpm-lock.yaml after a dependency change.
vscode-deps-refresh:
	$(call run,vscode-deps-refresh,pnpm --dir clients/vscode install --no-frozen-lockfile --store-dir "$${PNPM_STORE_PATH:-$${XDG_CACHE_HOME:-$$HOME/.cache}/magit-lsp/pnpm-store}")

# Fast check before the native VSCode targets: names what is missing instead of
# failing deep inside a run. The launcher needs DISPLAY and an editor binary:
# MAGIT_VSCODE_BIN when set in local.mk, otherwise codium or code on PATH.
.PHONY: vscode-prereqs
vscode-prereqs:
	missing=""; \
	[ -n "$$DISPLAY" ] || missing="$$missing DISPLAY(set it or run under Xvfb)"; \
	if [ -n "$(MAGIT_VSCODE_BIN)" ]; then \
	  [ -x "$(MAGIT_VSCODE_BIN)" ] || missing="$$missing MAGIT_VSCODE_BIN=$(MAGIT_VSCODE_BIN)(not executable; fix local.mk)"; \
	elif ! command -v codium >/dev/null 2>&1 && ! command -v code >/dev/null 2>&1; then \
	  missing="$$missing codium-or-code(set MAGIT_VSCODE_BIN in local.mk)"; \
	fi; \
	[ -z "$$missing" ] || { printf 'MISSING:%s\n' "$$missing"; exit 1; }

vscode-test: build vscode-deps
	$(MAKE) vscode-semantic-test
	$(MAKE) vscode-semantic-check
	$(call run,vscode-typecheck,pnpm --dir clients/vscode run typecheck)
	$(call run,vscode,pnpm --dir clients/vscode test)
	$(call run,keybindings,node clients/vscode/scripts/gen-keybindings.mjs --check)

vscode-typecheck: vscode-deps
	$(call run,vscode-typecheck,pnpm --dir clients/vscode run typecheck)

vscode-unit: vscode-deps
	$(call run,vscode-typecheck,pnpm --dir clients/vscode run typecheck)
	$(call run,vscode-unit,node clients/vscode/test/test.js)

.PHONY: vscode-download-unit
vscode-download-unit:
	$(call run,vscode-download-unit,node clients/vscode/test/server-download.test.js)

# Offline distributable VSIX (no bundler, no publish): package the unbundled
# client with its runtime deps into build/ and fail if the archive lacks the
# entry files or either runtime dependency. vsce discovers dependencies through
# `npm list`, which cannot read pnpm's isolated symlinks, so install the same
# frozen lockfile with pnpm's hoisted linker first (build tree only).
.PHONY: vscode-package
vscode-package:
	$(call run,vscode-package,pnpm --dir clients/vscode install --frozen-lockfile --node-linker=hoisted --store-dir "$${PNPM_STORE_PATH:-$${XDG_CACHE_HOME:-$$HOME/.cache}/magit-lsp/pnpm-store}" && version=$$(node -p "require('./clients/vscode/package.json').version") && mkdir -p build/vscode && pnpm --dir clients/vscode exec vsce package --out "../../build/vscode/magit-lsp-$$version.vsix" && PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_vsix.py "build/vscode/magit-lsp-$$version.vsix")

# Static guard for the manual publication workflow and its crate metadata.
# PyYAML is unavailable, so the workflow is checked as text; the Cargo half
# runs `cargo package --list` offline to catch an incomplete crate.
release-automation-check:
	$(call run,release-automation-check,PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_release_automation.py)

# Pipeline-adapter self-tests only: fake process boundaries for the native
# linker lane, strict pinned-fallback assertions and Makefile wiring. No Cargo
# invocation, no build, no toolchain download and no Cargo package metadata.
release-pipeline-check:
	$(call run,release-pipeline-check,PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_release_automation.py --pipeline-self-tests)

# Static workflow/pipeline guards plus their fixture self-tests, excluding the
# Cargo metadata/package half. This is the honest no-build boundary for a
# tools-only shell; the broad release-automation-check remains Cargo-aware.
release-automation-unit:
	$(call run,release-automation-unit,PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_release_automation.py --automation-unit)

# Publication consumes only the validated handoff downloaded by the protected
# publisher job. It never builds, repackages, installs a PAT or picks a
# cheaper credential path; the script verifies the handoff and OIDC/GitHub
# context before the first mutation and reports partial outcomes honestly.
release-publish:
	@test -n "$(HANDOFF)" || { echo 'release-publish: HANDOFF is required' >&2; exit 2; }
	$(call run,release-publish,PYTHONDONTWRITEBYTECODE=1 python3 scripts/publish_release.py --handoff "$(HANDOFF)")

# Explicit offline release contract cases plus shellcheck. Selecting the
# methods avoids optional real-binary cases and needs no Cargo build; the
# real-binary install/launch smoke is only for an actual artifact.
release-contract-unit:
	$(call run,release-contract-unit,command -v shellcheck >/dev/null 2>&1 || { echo 'release-contract-unit: shellcheck is required (install it or add it to the flake dev shell)' >&2; exit 1; }; shellcheck scripts/install.sh scripts/package-release.sh && PYTHONDONTWRITEBYTECODE=1 python3 -m unittest tests.release.test_release.ReleaseTests.test_latest_and_pinned tests.release.test_release.ReleaseTests.test_default_directory tests.release.test_release.ReleaseTests.test_download_failures tests.release.test_release.ReleaseTests.test_missing_asset tests.release.test_release.ReleaseTests.test_bad_metadata_and_versions tests.release.test_release.ReleaseTests.test_destination_failure_preserves_binary tests.release.test_release.ReleaseTests.test_checksum_failures tests.release.test_release.ReleaseTests.test_platform_failures tests.release.test_release.ReleaseTests.test_archive_structure tests.release.test_release.ReleaseTests.test_package_install_contract tests.release.test_release.ReleaseTests.test_package_rejects_version_mismatch)

# Synthetic archive/tool fixtures only; never executes or builds a candidate.
native-artifact-unit:
	$(call run,native-artifact-unit,PYTHONDONTWRITEBYTECODE=1 python3 -m unittest tests.release.test_native_artifact)

# Mocked API and synthetic archive fixtures only; never executes a candidate.
native-candidate-unit:
	$(call run,native-candidate-unit,PYTHONDONTWRITEBYTECODE=1 python3 -m unittest tests.release.test_native_candidate_download)

vsix-candidate-unit:
	$(call run,vsix-candidate-unit,PYTHONDONTWRITEBYTECODE=1 python3 -m unittest tests.release.test_vsix_candidate_download)

native-artifact-inspect:
	$(call run,native-artifact-inspect,test -n "$(RELEASE_ARTIFACT)" && python3 scripts/check_native_artifact.py "$(RELEASE_ARTIFACT)")

native-artifact-check:
	$(call run,native-artifact-check,test -n "$(RELEASE_ARTIFACT)" && python3 scripts/check_native_artifact.py --functional "$(RELEASE_ARTIFACT)")

# Shellcheck plus the offline release contract suite for the download/packaging
# scripts the release workflow and installer path depend on. The flake dev shell
# provides shellcheck; a missing tool fails with a readable message instead of
# silently skipping the lint half.
release-contract-check:
	$(call run,release-contract-check,command -v shellcheck >/dev/null 2>&1 || { echo 'release-contract-check: shellcheck is required (install it or add it to the flake dev shell)' >&2; exit 1; }; shellcheck scripts/install.sh scripts/package-release.sh && PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/release -p 'test_*.py')

# Syntax, expression and `run:` shell lint for every tracked workflow.
# release-automation-check asserts structure as text; actionlint is the parse
# layer (malformed `${{ }}`, unknown job/output references, shell defects).
# It is in the flake dev shell, so a missing tool fails readably, never skips;
# when shellcheck is also on PATH actionlint lints the shell snippets too.
# WORKFLOW_FILES selects specific workflows for a scoped check; unset lints all.
WORKFLOW_FILES ?=
workflows-check:
	$(call run,workflows-check,command -v actionlint >/dev/null 2>&1 || { echo 'workflows-check: actionlint is required (install it or add it to the flake dev shell)' >&2; exit 1; }; actionlint $(if $(strip $(WORKFLOW_FILES)),$(WORKFLOW_FILES),.github/workflows/*.yml))

# Static guard for the prepared (not submitted) Mason registry package. PyYAML
# is absent, so clients/mason/package.yaml is checked line by line against
# Cargo.toml and the release archive contract.
mason-package-check:
	$(call run,mason-package-check,PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_mason_package.py)

vscode-version-startup: vscode-prereqs build vscode-deps
	$(call run,vscode-version-startup,MAGIT_VSCODE_VERSION_ONLY=1 node clients/vscode/test/e2e/run.mjs)

vscode-version-warning: vscode-prereqs build vscode-deps
	$(call run,vscode-version-warning,MAGIT_VSCODE_VERSION_WARNING_ONLY=1 node clients/vscode/test/e2e/run.mjs)

# Focused native automatic startup: an isolated profile with no serverPath must
# find, verify and launch its pre-seeded cached release, then serve status.
.PHONY: vscode-download-startup
vscode-download-startup: vscode-prereqs build vscode-deps
	$(call run,vscode-download-startup,MAGIT_VSCODE_DOWNLOAD_ONLY=1 node clients/vscode/test/e2e/run.mjs)

# Native VSIX install smoke: install the packaged archive into an isolated
# profile, launch with no development path, and prove the installed extension
# activates and starts its managed seeded-cache server offline.
.PHONY: vscode-vsix-install
vscode-vsix-install: vscode-prereqs vscode-package build vscode-deps
	$(call run,vscode-vsix-install,MAGIT_VSCODE_VSIX_INSTALL_ONLY=1 node clients/vscode/test/e2e/run.mjs)

# Hosted artifact consumer: validate explicit candidate inputs before launching
# the installed-VSIX smoke; deliberately has no build, package or deps target.
.PHONY: native-vscode-artifact-check
native-vscode-artifact-check:
	$(call run,native-vscode-artifact-check,test -n "$(MAGIT_LSP_SERVER_PATH)" || { echo 'MAGIT_LSP_SERVER_PATH is required' >&2; exit 1; }; test -n "$(RELEASE_ARTIFACT)" || { echo 'RELEASE_ARTIFACT is required' >&2; exit 1; }; test -n "$(RELEASE_MANIFEST)" || { echo 'RELEASE_MANIFEST is required' >&2; exit 1; }; test -n "$(RELEASE_VSIX)" || { echo 'RELEASE_VSIX is required' >&2; exit 1; }; for value in "$(MAGIT_LSP_SERVER_PATH)" "$(RELEASE_ARTIFACT)" "$(RELEASE_MANIFEST)" "$(RELEASE_VSIX)"; do test "$${value#/}" != "$$value" && test -f "$$value" || { echo 'release inputs must be absolute files' >&2; exit 1; }; done; test -x "$(MAGIT_LSP_SERVER_PATH)" || { echo 'MAGIT_LSP_SERVER_PATH is not executable' >&2; exit 1; }; if test ! -e /proc/sys/kernel/ostype || test -n "$$DISPLAY"; then :; else echo 'DISPLAY is required on Linux' >&2; exit 1; fi; editor="$(MAGIT_VSCODE_BIN)"; if [ -n "$$editor" ]; then test -x "$$editor" || { echo 'MAGIT_VSCODE_BIN is not executable' >&2; exit 1; }; else editor=$$(command -v codium || command -v code || true); test -n "$$editor" || { echo 'VSCode-compatible editor is required' >&2; exit 1; }; fi; MAGIT_VSCODE_ARTIFACT_ONLY=1 node clients/vscode/test/e2e/run.mjs)

# Focused native server management: against the pre-seeded managed release, the
# server-info command must report it and install/reinstall must force a fresh
# verified download and atomically replace the cached binary.
.PHONY: vscode-server-commands
vscode-server-commands: vscode-prereqs build vscode-deps
	$(call run,vscode-server-commands,MAGIT_VSCODE_SERVER_COMMANDS_ONLY=1 node clients/vscode/test/e2e/run.mjs)

vscode-host-spike: build vscode-deps
	MAGIT_VSCODE_BIN="$${MAGIT_VSCODE_BIN:-code}" node clients/vscode/test/e2e/real-file-spike.mjs

vscode-e2e: vscode-prereqs build vscode-deps
	$(call run,vscode-e2e,MAGIT_VSCODE_A06_ONLY=1 node clients/vscode/test/e2e/run.mjs)

# Focused native semantic-role smoke: one real status diff must render the
# server's canonical added and removed content cells with explicitly unequal
# computed foregrounds under the default dark scheme, without touching the
# isolated fixture or running the theme-transition cases.
.PHONY: vscode-semantic-e2e
vscode-semantic-e2e: vscode-prereqs build vscode-deps
	$(call run,vscode-semantic-e2e,MAGIT_VSCODE_SEMANTIC_ROLES_ONLY=1 node clients/vscode/test/e2e/run.mjs)

# Focused native F5 check: physical F5 on the focused status view must dispatch
# and render common.refresh, recording key/modifier events and the request or
# its absence. Fails honestly when the contributed binding produces no refresh.
.PHONY: vscode-f5
vscode-f5: vscode-prereqs build vscode-deps
	$(call run,vscode-f5,MAGIT_VSCODE_SUITE_MODE=f5 node clients/vscode/test/e2e/run.mjs)

update-schema:
	$(call run,update-schema,cargo run -q --bin generate-schema)

schema-check:
	$(call run,schema-check,cargo run -q --bin generate-schema -- --check)

registry-docs-generate:
	$(call run,registry-docs-generate,cargo run -q --bin generate-docs -- $(if $(ROOT),--output-root $(ROOT)))

registry-docs-check:
	$(call run,registry-docs-check,cargo run -q --bin generate-docs -- --check $(if $(ROOT),--output-root $(ROOT)))

.PHONY: docs-generate docs-artifacts-check help-tags

docs-generate: update-schema registry-docs-generate

docs-artifacts-check: schema-check registry-docs-check

# Generate doc/tags so an installed plugin resolves `:h magit`. Runs a
# plugin-free Neovim so the index depends only on the shipped help file, and
# fails when no index is produced. The help-tags name is fixed by the
# NVIM-HELP-DISCOVERY ticket rather than the usual update-*/*-generate pattern.
help-tags:
	$(call run,help-tags,rm -f doc/tags && nvim --headless -u NONE --noplugin -c "lua local gen = function() return vim.cmd('helptags ' .. vim.fn.fnameescape('doc')) end; if not pcall(gen) or vim.fn.filereadable('doc/tags') == 0 then vim.api.nvim_err_writeln('help-tags: helptags failed or doc/tags was not generated'); vim.cmd('cquit 1') end" -c "qa!")

ui-tour: build
	$(call run,ui-tour,PYTHONDONTWRITEBYTECODE=1 python3 scripts/ui run $(if $(V),-v) tests/ui/tour.scn tests/ui/tour-clean.scn tests/ui/tour-error.scn --markdown build/ui/tour.md)

.PHONY: color-spike-retirement-check
# One-off retirement guard: the obsolete colour-spike experiment must stay gone.
# Every assertion below is a fail-capable absence check (a `test`/`grep` that
# succeeds only when the retired artifact is missing); no artifact is produced.
color-spike-retirement-check:
	$(call run,color-spike-retirement-check,test ! -e scripts/color_spike.lua && ! grep -qE '^color-spike:' Makefile && ! grep -qE '(^|[[:space:]])color-spike([[:space:]]|:)' Makefile && ! git ls-files '*status-*.html' | grep -q .)

readme-shots:
	@printf '%s\n' 'Human-only README image guidance: docs/SCREENSHOTS.md'
