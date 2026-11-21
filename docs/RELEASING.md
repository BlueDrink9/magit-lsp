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
close R-REL-2/3, hosted/clean-machine or published-VSIX installation gates. The
actual lazy.nvim-manager run (`make lazy-install-check`) is local Linux evidence
only. `nix develop -c make mason-install-check` additionally exercises pinned
mason.nvim `v2.3.1` (`2a6940af80375532e5e9e7c1f2fc6319a1b7a69d`) through
its actual registry/compiler/installer with the unchanged production package
spec mechanically translated from YAML. Registry selection and the GitHub
download URL template alone redirect registry/assets to a private loopback
server. No extra registry/compiler dependency is fetched: Mason's compiler and
vendored dependencies belong to that exact checkout; registry schema commit
`215cb7beb49e781a126bd5ba3397106367ed5316` is research provenance only, not a
runtime dependency or a claim of schema validation.

The Mason check packages the current **debug** 0.2.0 binary with the production
packaging script. It rejects corrupt bytes through actual tar unpack, retries
the valid archive in the same install root, then verifies receipt/raw source,
binlink, executable version and installed-server initialize/status render.
`build/ui/mason-install/mason-install.json` records the exact dependency pin,
production-manifest hash/parity, loopback requests, corrupt/valid results and
cleanup (isolated HOME/XDG, removed staging/locks/temp root, reaped editors,
no executing owned server and stopped HTTP thread). See the
[Mason installation scope](CLIENT_INSTALL.md#mason-server-package-prepared-not-submitted).
This does not submit the package, download a live release, test release-mode
or baseline-glibc artifacts, verify Windows/macOS, or close hosted/clean-machine
release gates. The production manifest/server and user configuration are not
changed.

The release workflow does not install Neovim, so its setup
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
not uploaded or published. Hosted Ubuntu baseline, clean-machine/VSIX
installation and all publication gates remain open; the actual lazy.nvim-manager
run is local Linux evidence only.

## Contracts

Run from the repository root in an environment that provides Rust 1.94.1:

```sh
make release-locked
make release-artifact-check
```

`release-locked` runs the workflow's exact locked build
(`cargo build --release --locked --target x86_64-unknown-linux-gnu`).
`release-artifact-check` depends on it, then validates the run-bound identity
(target, original ref, version, source SHA, event, run id/attempt), writes the
explicit provenance record and packages
`target/x86_64-unknown-linux-gnu/release/magit-lsp` with
`scripts/package-release.sh` into `build/release/artifacts`, validates the
archive/sidecar set offline (`--validate-artifacts`) and writes the run manifest
with its external digest under `build/release/`. Packaging never executes the
binary; native runtime validation consumes the archive separately. The
protected `dist/` directory is never a pipeline output.

The strict Rust 1.94.1 pin is the only release build: there is no
installed-compiler or `--local` alias. A missing pin fails readably, and no
target downloads or installs Rust.

Those pinned targets run `scripts/check_release_pipeline.py`, the script
`.github/workflows/release.yml` invokes, so there is no CI-only build or
packaging path. The pinned build requires exactly Rust 1.94.1: another
toolchain fails with the found version and a request to provide the pin.
Use the matching Cargo/extension version (0.2.0); the packager checks both and
the supplied binary's `--version`. `RELEASE_VERSION` reaches packaging from the
workflow's `--validate-release-ref` step, which writes it to `$GITHUB_ENV`;
locally the pipeline derives the same value from `Cargo.toml` and
`clients/vscode/package.json` and fails on a mismatch. `nix develop -c make
release-contract-check` covers shellcheck plus the offline suite; run
`git diff --check` before committing.
The build workflow also checks any tag equals `v<Cargo version>`; manual runs use
the checked-out version. Inputs enter shell through quoted environment variables,
never expression interpolation. The build workflow grants only `contents: read`;
checkout and artifact actions are revision-pinned and Rust is fixed at 1.94.1.
Update pins through review. Regular CI runs offline contract tests, not expensive
release builds.

## Portable production toolchain decision (NREL-01)

This is a researched contract, not an executed build. No Nix production
derivation, archive, Windows executable or macOS output exists yet, and nothing
below is a runtime result. It binds NREL-02 and NREL-04..NREL-12 to one
builder/runtime/identity contract and names the bounded experiment where a
mechanism is still unproven. The current rustup `release-locked` path remains
the only executed release path until NREL-06 replaces it.

> **Scope change (2026-11-20):** the user has deferred all Windows work — the
> Windows GNU cross-build, the Windows editor-helper transport and Windows-native
> runtime acceptance — to a dedicated deferred phase executed immediately before
> the optional extras. The Windows rows, native-runner contract and risk items
> below are retained research/evidence and are **informational only**: they are
> not current release gates, and no release or publish decision depends on
> Windows. Linux and both Darwin architectures remain the current targets. This
> supersedes NREL-03's place in the active NREL sequence and the Windows portions
> of NREL-07/NREL-09/NREL-12; the queue records the split and defers the work.

### Decision table

| Area | Decision | Status and limits |
| --- | --- | --- |
| One compiler source | `rustc`/`cargo` come only from the flake's locked `nixpkgs` input (`flake.lock:4-10`, rev `04906337b6d53e8bb0eaf0470ae9f070a4d4eb55`, `nixos-unstable`). At that rev `rust_1_99` sets `rustcVersion = "1.99.1"`. Every target uses this compiler. The release workflow's separate rustup 1.94.1 install is a legacy release path owned by NREL-06, never the production contract; no rust-overlay or second compiler pin is added. | VERIFIED at the locked rev (`.../pkgs/development/compilers/rust/1_99.nix`). The existing `docs/RELEASING.md` note about local Rust 1.99.0 is a dated local observation, not the locked version. |
| Linux target and runtime | `x86_64-unknown-linux-gnu`; minimum Ubuntu 22.04, glibc 2.35. **Candidate** mechanism: add one sysroot-only flake input pinned to nixpkgs `nixos-22.11` head `ea4c80b39be4c09702b0cb3b42eab59e2ba4f24b` (its `glibc` is `2.35-224`), then build with the locked compiler via `makeRustPlatform { rustc = locked.rustc; cargo = locked.cargo; stdenv = sysroot.stdenv; }`; set the interpreter to `/lib64/ld-linux-x86-64.so.2`, remove Nix RPATH, and require `DT_NEEDED` to contain only system libraries with every `GLIBC_*` symbol version <= 2.35. | `makeRustPlatform` accepting an explicit `stdenv` is VERIFIED at the locked rev. The final link/ABI is **UNRESOLVED**: the locked compiler wrapper and prebuilt `libstd` may still inject or require newer glibc paths. NREL-02's bounded `make release-nix-linux` experiment decides success and must fail readably on any newer symbol, Nix store reference or unexpected `DT_NEEDED`. |
| Old-sysroot cost | The extra input supplies only the C runtime/toolchain (`glibc`, GCC, binutils, stdenv) for the Linux link; it is **not** a whole-nixpkgs rollback and does not change the Rust compiler. It is accepted only if the experiment passes; otherwise the decision returns to a human. | Explicit cost: a second, EOL nixpkgs source of security/maintenance updates and a larger closure. It must not become a general downgrade of the project toolchain. |
| Rejected Linux alternatives | musl/static was not chosen: it changes libc, allocator and NSS behavior and the user rejected an assumed migration. Static glibc has NSS/dlopen limits. rustup on `ubuntu-22.04` is the second compiler policy being retired. Shipping the Nix store closure is not portable off Nix because interpreter/RPATH point into `/nix/store`. | Decision, not a measured comparison. |
| Windows target | `x86_64-pc-windows-gnu`, cross-built with the locked compiler through `pkgsCross.mingwW64`. The locked rev has mingw-w64 14.0.0 headers and GCC 15. Rust's target spec links executables with `-lgcc_eh`/`-l:libpthread.a` when all crates are statically linked, so no MinGW runtime DLL is expected; expected imports are OS DLLs only (`msvcrt`, `kernel32`, `advapi32`, `bcrypt`, `ntdll`, `userenv`, `ws2_32`, `user32`). | Upstream minima (see evidence): GNU Binutils 2.44, GCC 14.2, mingw-w64 12.0.0. The exact import table and any bundled DLL allowlist are **UNRESOLVED** until NREL-03 inspects the produced `.exe`; if a DLL is needed, bundle an explicit allowlist or declare a supported-system requirement, never ship a silently broken executable. **Deferred 2026-11-20:** this row is retained research; informational only and not a release gate. |
| Windows capability boundary | A Windows GNU server provides LSP, view rendering and Git status/actions only. Managed commit drafts, the editor bridge and askpass return "requires Unix"/"needs a Unix socket" on non-Unix builds, and the non-Unix owned-run fallback makes no cancellation guarantee. | VERIFIED source boundaries: `src/editor.rs:697-700,730-739,790-793,871,1260-1270,1276-1288`; `src/runner/owned.rs:45-53,67-94`. `tests/editor_bridge.rs:4` is `#![cfg(unix)]`, so that suite never counts as Windows coverage. A small explicit follow-up item is required; Windows helper support is not claimed. Git for Windows' MSYS2/BASH layer does not provide the existing bridge's Unix socket. **Deferred 2026-11-20:** informational only; Windows helper support is not a current gate. |
| Darwin outputs | Native `x86_64-darwin` and `aarch64-darwin` builders in the same flake, using the locked compiler; distinct archive and executable identities per architecture. Contract: record `LC_BUILD_VERSION`/minos and `otool -L`; runtime dependencies must be system libraries only (no `/nix/store` install names or RPATH, rewritten with `install_name_tool` if needed). | Rust documents macOS 10.12+ for x86_64 and 11.0+ for arm64. NREL-04 must run on real matching builders; the Nix builder's own minimum macOS is not the artifact deployment baseline. Linux-to-Darwin cross output is not planned: nix.dev documents cross-Darwin only between the two Darwin hosts and barely tested. |
| Artifact identity | One `.tar.gz` plus `.sha256` per target (existing asset shape), with a `provenance.json` in the CI artifact. Required fields: `source_sha` (`git rev-parse HEAD`, equal to the workflow SHA), `target`, `archive_sha256`, `executable_sha256` (extracted `magit-lsp`/`magit-lsp.exe` before packaging), `cargo_lock_sha256`, `rustc -Vv`, nixpkgs rev, Linux sysroot rev/glibc, Windows mingw-w64/GCC/binutils, Darwin SDK/minos. Consumers verify both SHAs and execute the bytes unchanged; they never rebuild the server. | GitHub `upload-artifact` artifacts are immutable from v4 and expose `artifact-id`/`artifact-digest`, but zipped uploads do not preserve file modes and the artifact bytes are not the released archive bytes; the durable identity is the sidecar SHA over the uploaded `.tar.gz`. VERIFIED against the pinned action README. |

### Native assertion contract

Assertions run on the target OS without Nix installed or its store mounted. The
same archive and extracted executable bytes are used; only the test harness is
compiled natively.

* **Linux x86_64 GNU (clean Ubuntu 22.04):** `--version`, initialize, Git status
  plus one mutation, `editor_bridge` against the selected bytes, and the
  existing Neovim status/Git/editor-helper scenarios.
* **Windows x86_64 GNU (native Windows runner; deferred 2026-11-20, informational only and not a current gate):** `--version`, initialize, Git
  status plus one mutation. Editor helper, askpass, managed drafts and
  cancellation are not supported and are reported as skipped; skipped Unix
  suites never count as Windows coverage.
* **macOS x86_64 and arm64:** the full Unix Rust suite, `--version`, Git status
  plus one mutation, and the Neovim scenarios on available hosts.

### Upstream evidence (reviewed 2026-11-20)

* Locked input: `flake.lock`; locked `rust_1_99` (1.99.1):
  <https://github.com/NixOS/nixpkgs/blob/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55/pkgs/development/compilers/rust/1_99.nix>
* `makeRustPlatform` `stdenv` parameter:
  <https://github.com/NixOS/nixpkgs/blob/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55/pkgs/development/compilers/rust/make-rust-platform.nix>
* glibc 2.35-224 at nixos-22.11 head `ea4c80b39be4c09702b0cb3b42eab59e2ba4f24b`:
  <https://github.com/NixOS/nixpkgs/blob/ea4c80b39be4c09702b0cb3b42eab59e2ba4f24b/pkgs/development/libraries/glibc/common.nix>
* Rust platform support and minimum OS versions:
  <https://doc.rust-lang.org/nightly/rustc/platform-support.html>
* Rust `windows-gnu` requirements and maintenance warning:
  <https://doc.rust-lang.org/rustc/platform-support/windows-gnu.html>
* Rust `windows_gnu.rs` static/dynamic unwind link arguments:
  <https://github.com/rust-lang/rust/blob/master/compiler/rustc_target/src/spec/base/windows_gnu.rs>
* mingw-w64 14.0.0 at the locked rev:
  <https://github.com/NixOS/nixpkgs/blob/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55/pkgs/os-specific/windows/mingw-w64/headers.nix>
* Nix cross-compilation and `pkgsCross.mingwW64`:
  <https://nix.dev/tutorials/cross-compilation.html>
* Git for Windows (MSYS2/BASH distribution):
  <https://gitforwindows.org/> and Git editor configuration:
  <https://git-scm.com/docs/git-config#Documentation/git-config.txt-coreeditor>
* GitHub artifacts (workflow outputs, immutable action artifacts):
  <https://docs.github.com/en/actions/concepts/workflows-and-actions/workflow-artifacts>
  and the pinned action README:
  <https://github.com/actions/upload-artifact/blob/cf430e030ddbb5b0abf93d22962f4752f3646cd9/README.md>

### Unresolved risks and follow-ups

1. Linux mixed-sysroot link/ABI: the locked compiler wrapper or `libstd` may
   leak newer glibc requirements. NREL-02 must prove interpreter, dependency and
   symbol versions on the produced ELF and fail readably.
2. Windows import table and DLL set (deferred 2026-11-20 with the deferred Windows phase): NREL-03 must record the actual imports and
   decide bundling versus a supported-system requirement.
3. Windows editor helper/askpass/managed drafts/cancellation (deferred 2026-11-20 with the deferred Windows phase): unsupported until
   a dedicated follow-up designs a Windows transport; do not describe the
   Windows artifact as fully feature-equivalent.
4. Rust `windows-gnu` has no upstream target maintainers and may be demoted;
   record this risk rather than substituting MSVC for the production artifact.
5. Darwin deployment baseline and non-system dylib removal: NREL-04 must verify
   on both native architectures; unavailable hosts stay explicit CI follow-ups.
6. Artifact-versus-asset byte identity: the zip upload layer changes bytes and
   modes, so NREL-07/08 must bind the released archive by `archive_sha256` and
   provenance, not by artifact digest alone.
7. CI reuse: build each target once in the Nix derivation and let native lanes
   consume that archive; no second build recipe and no server rebuild in
   consumers.

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
`make release-automation-check` (workflow structure, lane gating, the shared
pinned build/artifact handoff and Cargo packaging) and
`make release-contract-check` (shellcheck plus the offline release suite). Two
more reproduce the build side locally without publishing: `release-locked` and
`release-artifact-check` above. The no-build `release-automation-unit` runs the
static workflow/pipeline guards and their fixture self-tests without Cargo
metadata or package.

In `.github/workflows/ci.yml`, the `native-test` matrix keeps the
`windows-2025` (`x86_64-pc-windows-msvc`) lane explicitly informational: only
that lane sets `informational: true`, and the job's
`continue-on-error: ${{ matrix.informational }}` prevents a Windows failure from
failing the workflow. Both macOS lanes set `informational: false` and stay
required, and every Linux job remains required. No job declares
`needs: native-test`, and `release.yml`/`publish.yml` do not consume CI, so the
Windows lane gates neither releases nor other jobs. Branch protection and
required-check lists are external repository settings: this repository cannot
assert their contents, so an external required-check list must itself omit the
Windows lane.

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
