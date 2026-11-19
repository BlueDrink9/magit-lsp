# Release foundation (R-REL-1, not release approval)

This is a prepared release foundation, not an approved release. Two tracked
workflows cover the automation boundary: `.github/workflows/release.yml` builds
and packages the release candidate, and `.github/workflows/publish.yml` is a
manual, dispatch-only publication workflow that defaults to a validation-only
dry run and gates its registry lanes on human-provided credentials. Neither
has been run, no tag has been pushed, no credential is configured and nothing
has been published. GitHub Actions artifacts are short-lived CI outputs, **not**
GitHub Release assets. R-REL-1 and the release go/no-go remain open.

Client setup/discovery and on-demand Neovim examples are in
[CLIENT_INSTALL](CLIENT_INSTALL.md); human-only README image guidance is in
[SCREENSHOTS](SCREENSHOTS.md). External adapters use
[CLIENT_SPEC](CLIENT_SPEC.md). These docs and isolated local setup tests do not
close R-REL-2/3, hosted/clean-machine, actual lazy.nvim-manager or published-VSIX
installation gates. The release workflow does not install Neovim, so its setup
smoke may skip; inspect verbose unittest output rather than counting a skip as
client evidence. README images are optional human-owned assets; harnesses emit
text/structured evidence and do not capture screenshots. See the
[no-vision policy](../AGENTS.md).

- [x] **Bounded R-REL-1 foundation:** offline packaging/download contracts,
  `contents: read` build workflow, safe installer and local actual-binary launch smoke.
  The verification command below passed locally on 2026-10-28, including the
  actual-binary test and shellcheck. Hosted CI and publication are not checked.
- [ ] Parent R-REL-1 and release gates: remaining evidence/approvals listed below.

### Final bounded local verification (2026-10-28)

At checkout `909e40a7665752e207486c4f06f379670fada903` with working-tree
release/docs changes, the exact unittest/shellcheck/diff gate below passed:
**13 tests, 0 skips**, including headless client setup and actual-binary
package/install/`--version`. Generated release-test bytecode was removed;
the bytecode-disabled rerun left none. No owned `magit-lsp` server remained.

Historical dated native observations used the existing debug server (0.1.0), Neovim 0.12.6
and VSCodium 1.140.06880; it is not installation evidence:

* `PYTHONDONTWRITEBYTECODE=1 scripts/ui run tests/ui/scenarios/commit-menu-shows-actions.scn`
  exited 0: `Summary: 1 PASS, 0 XFAIL, 0 FAIL`. Inspected the four plain
  `build/ui/commit-menu-shows-actions/{standard,vim}-{100x30,80x24}/commit-menu.screen`
  captures: commit actions visible in every variant. These are text terminal
  captures, not PNGs. This dated observation is not current agent guidance.
* `MAGIT_VSCODE_BIN="$(command -v codium)" MAGIT_VSCODE_A06_ONLY=1 npm --prefix clients/vscode run test:e2e`
  exited 0 without a skip. Log: `/tmp/opencode/magit-vscode-capture.log`;
  evidence: `/tmp/opencode/vscode-review-yukphd/`, including
  `a06-evidence.json`. Opened `r-vs-2-first-open.png`,
  `a06-menu-before-quit.png` and `a06-reopened.png` (1192x756, dark theme).
  Status/reopen text is legible; the focused Commit QuickPick shows its title
  and input **but no action rows**. This historical observation validates
  capture/root lifecycle, not complete menu presentation or release readiness.

Both harnesses cleaned up their owned sessions/processes. Evidence is local,
not uploaded or published. Hosted Ubuntu baseline, actual lazy.nvim-manager,
clean-machine/VSIX installation and all publication gates remain open.

## Contracts

Run from the repository root:

```sh
cargo build --release --locked
sh scripts/package-release.sh 0.2.0 target/release/magit-lsp dist
PYTHONDONTWRITEBYTECODE=1 MAGIT_LSP_TEST_BINARY=target/release/magit-lsp python3 -m unittest discover -s tests/release -p 'test_*.py' -v
shellcheck scripts/install.sh scripts/package-release.sh
git diff --check
```

Use the matching Cargo/extension version (0.2.0).
The packager checks both versions and the supplied binary's `--version`.
The build workflow also checks any tag equals `v<Cargo version>`; manual runs use
the checked-out version. Inputs enter shell through quoted environment variables,
never expression interpolation. The build workflow grants only `contents: read`;
checkout and artifact actions are revision-pinned and Rust is fixed at 1.94.1.
Update pins through review. Regular CI runs offline contract tests, not expensive
release builds.

## Tracked workflows and local checks

`.github/workflows/release.yml` runs on manual dispatch and on `v*` tags. It
installs Rust 1.94.1, builds the locked `x86_64-unknown-linux-gnu` target on the
Ubuntu 22.04 baseline, packages and smoke-tests the binary, and uploads the
`linux-x86_64-gnu-release-candidate` artifact (14-day retention). It publishes
nothing and holds no credentials.

`.github/workflows/publish.yml` is `workflow_dispatch`-only, so no push or tag
can publish on its own. Its inputs are a required `build_run_id` naming a
completed build-workflow run and a `mode` choice defaulting to `dry-run`. The
`validate` job re-checks that run's archive, sidecar and version triple and
rebuilds the offline VSIX; only a `publish`-mode dispatch continues, creating the
GitHub Release `v<version>` from the validated assets (refusing an existing
release) and publishing the VSIX to the VS Code Marketplace (`VSCE_PAT`), Open
VSX (`OVSX_PAT`) and the crate to crates.io (`CARGO_REGISTRY_TOKEN`). Each of
those three registry lanes is gated on its own human-provided secret and skips
with a notice when that secret is absent.

Three local targets guard this automation: `make workflows-check` (actionlint
syntax, expression and `run:` shell lint over every workflow),
`make release-automation-check` (workflow structure, lane gating and Cargo
packaging) and `make release-contract-check` (shellcheck plus the offline
release suite).

For version X.Y.Z the two exact release assets are:

* `magit-lsp-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
* `magit-lsp-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz.sha256`

The checksum is one lowercase SHA-256 digest, two spaces, the archive filename
and a newline. The archive contains exactly four regular files under
`magit-lsp-vX.Y.Z-x86_64-unknown-linux-gnu/`: `magit-lsp`, `LICENSE`, `LICENSE-APACHE`, `INSTALL.md`.
Both canonical license texts from `LICENCES/` are copied verbatim so the Apache-2.0 protocol exception remains
available alongside the GPL server. No extension is included. Archive metadata
is partially normalized; byte-for-byte reproducibility is **not** claimed.

## Installer interface and trust

`sh scripts/install.sh [--version latest|X.Y.Z|vX.Y.Z] [--install-dir DIRECTORY]`
requires Python >=3.9, curl, uname and GNU getconf; defaults to `~/.local/bin`.
It downloads only HTTPS from `BlueDrink9/magit-lsp` GitHub Releases. Latest uses
the GitHub API's latest release tag, then the same versioned asset URLs as pinned
installs. Stable numeric tags only; prereleases are rejected. GitHub rate limits,
missing assets and curl failures stop installation. There is no sudo, Rust build
fallback, alternative mirror or production test URL override.

The installer rejects unsupported OS/architecture/libc, invalid checksums,
malformed archives, unexpected members, links, duplicate files and traversal.
It does not extract archive paths: only binary bytes are staged beside the
destination and atomically renamed. Existing binaries survive failures before
replacement. Temporary files are cleaned on normal failures (unavoidable abrupt
termination, such as SIGKILL, may leave temporary files). HTTPS checksums provide
integrity, **not independent authenticity**: compromise of the release account
can replace both files. Review the installer before executing it. It neither
launches the downloaded binary nor modifies PATH/editor configuration.

Tests mock curl, uname and getconf via PATH; all downloads stay offline. The
optional `MAGIT_LSP_TEST_BINARY` test packages, installs and launches a real local
binary with `--version`; regular CI otherwise uses a synthetic executable.

## Platform evidence and limits

Only Linux x86_64 GNU is in scope. Hosted build baseline is Ubuntu 22.04,
glibc 2.35, Rust 1.94.1, target `x86_64-unknown-linux-gnu`. This is a proposed
build/runtime baseline, **pending a successful hosted workflow and clean-machine
runtime evidence**. The installer enforces glibc >=2.35 but that alone is not a
claim that every distribution is tested. Git must be installed separately;
see README for the supported Git policy. No macOS, Windows, ARM or musl assets.

Local verification uses Fedora 44 with Nix-provided glibc 2.44 and Rust 1.99.0
(the local executable dynamically references Nix store libraries). A local binary smoke
does not prove the hosted artifact runs on glibc 2.35; do not publish local Fedora
builds under the Ubuntu-baseline asset name. No clean-machine/editor-native
installation evidence is implied by the command-line smoke.

## Publication blockers and manual protected procedure

* Complete prerequisite feature gates R-4.1a/R-7.3a and R-CLIENT-SPEC; assess
  their canonical evidence in TASKS.md rather than assuming readiness here.
* Component licensing is resolved: server GPL-3.0-only, separately enumerated
  client/protocol/specification/documentation Apache-2.0 exceptions. Cargo description
  and repository metadata are present. Dependency/upstream-license and
  corresponding-source review and publication approval remain open.
* Crates.io, Marketplace, Open VSX dry runs, actual crate/VSIX install smoke,
  client discovery, release notes and both-client native evidence remain gates.
  The current extension uses an external server; no bundled server, automatic
  installer or published extension is promised. Binary users need no Rust;
  source installation does.
* Preserve the disclosed BUG-READ-1 limitation; do not turn this foundation into
  a responsiveness fix. Full regression, phase evidence and explicit **pre-tag
  human go/no-go** for v0.2.0 remain required. No tag/publish approval is implied.

Focused licensing/release verification on 2026-10-28 passed: 16 unittest tests,
0 skips, including the real-binary package/install/launch smoke; shellcheck and
`git diff --check` passed. This local evidence does not close hosted or publication
gates.

After those gates and explicit human approval, an authorized maintainer should
review the exact commit and aligned versions, protect the release tag and require
reviewers on the environment used for any publication dispatch. Each registry
credential (`VSCE_PAT`, `OVSX_PAT`, `CARGO_REGISTRY_TOKEN`) is human-provided,
minimally scoped and kept out of build jobs; none is configured today and no
publication has occurred.

Run `release.yml` at the approved commit (dispatch, or an approved `v*` tag),
inspect logs and checksums, download the CI artifact, verify its actual
Ubuntu-baseline binary on a clean machine, and retain install/launch evidence.
Only then may an approved maintainer dispatch `publish.yml` with that run's
`build_run_id`: a `dry-run` dispatch validates the artifact and packaged VSIX
without touching any channel, while a `publish` dispatch creates the GitHub
Release with the validated archive and sidecar and publishes each registry
deliverable only where that registry's credential is present. Never mark a
release successful merely
because the build job passed. Re-download published assets and smoke both latest
and pinned installation; verify all registry and client deliverables separately.

For recovery, inventory which channels and asset hashes actually exist first.
Rerunning this build workflow is safe (CI artifacts only); it does not repair
published channels. Never silently replace a published binary/checksum pair,
move a public tag, or overwrite an immutable registry version. Stop promotion
on partial failure, preserve logs, communicate affected channels, and obtain a
human decision on completing unchanged missing assets versus a new patch version.
Treat a mismatched/checksum-compromised release as an incident, not an automatic
retry. Rollback locally by explicitly installing a known-good version after
reviewing its provenance; failed installs leave the prior executable intact.
