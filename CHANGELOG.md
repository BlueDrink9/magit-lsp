# Changelog

Notable changes to magit-lsp. This file records unreleased work; a release date
is added only when the version is tagged. Nothing here is a publication claim.

## 0.2.0 (unreleased)

### Server acquisition in the VS Code client

- The extension resolves the server in one order: an explicit machine-scoped
  `magit-lsp.server.path`, then a version-matching `magit-lsp` on `PATH`, then a
  pinned managed download from GitHub Releases into extension storage.
  `magit-lsp.server.source` (`auto` by default, `system` or `managed`) selects
  the policy: `system` never downloads and `managed` ignores the path. An
  invalid explicit path is reported instead of silently falling back, and a
  `PATH` binary is accepted only when its `--version` matches.
- Managed downloads are SHA-256 verified, cached atomically and reused offline.
  `Magit LSP: Install/Reinstall Managed Server` and
  `Magit LSP: Show Server Information` expose installation and the active
  path/origin. `extensionKind: ["workspace"]` lets a remote, WSL or container
  window resolve its own platform. See
  [client installation](docs/CLIENT_INSTALL.md).

### Release automation (prepared, not run)

- `.github/workflows/release.yml` builds and packages a locked Linux x86_64 GNU
  release candidate on manual dispatch or a `v*` tag and holds `contents: read`.
- `.github/workflows/publish.yml` is dispatch-only, requires a completed build
  run id and defaults to a validation-only `dry-run`. A `publish` dispatch can
  create the GitHub Release from validated assets and push the VSIX and crate
  through registry lanes; each lane is gated on a human-provided `VSCE_PAT`,
  `OVSX_PAT` or `CARGO_REGISTRY_TOKEN` and skips readably when that credential
  is absent.
- **Nothing has been published.** Neither workflow has run, no tag has been
  pushed, no credential is configured, and there is no GitHub Release asset,
  VS Code Marketplace or Open VSX listing, crates.io crate or Mason registry
  entry. Publishing requires an explicit human dispatch with credentials. See
  the [release foundation and approval gates](docs/RELEASING.md).

### Packaging and adoption

- `make vscode-package` builds a local `build/vscode/magit-lsp-0.2.0.vsix`.
  This is a local artifact, not a published extension.
- The reviewed `scripts/package-release.sh` and `scripts/install.sh` are adopted
  and guarded by `make release-contract-check` (shellcheck plus the offline
  release suite).
- `clients/mason/package.yaml` is prepared for the Mason registry but **not
  submitted**; admission is external and subject to that registry's own review
  and thresholds.
- Client install and release documentation was updated for the acquisition
  order above: [client installation](docs/CLIENT_INSTALL.md),
  [release foundation](docs/RELEASING.md) and the README. README presentation
  work remains separate.

### Plain keys (experimental, opt-in)

- Plain keys stay off by default and are opt-in per document. A typed eligible
  key in a plain status document is recovered as an edit; the server runs the
  same command a code action would, changes exactly the intended file and
  repairs the typed text. Destructive, pasted or multi-key, planned, stale and
  keys-off input changes nothing, and the repair echo is not replayed. A chord
  such as `g r` and commit-menu switches such as `-a` keep server state.
- Limits: this is a tested subset, not Phase 9 completion. Hunk, visual,
  unstage and navigation key cases and the wider property/native matrix
  (T034–T039) remain unverified, and no broad plain-client support is claimed.
  The policy, per-document toggle and limits are in
  [Plain command support](docs/plain-support.md).

### Known limits and platform matrix

| Area | 0.2.0 status |
| --- | --- |
| Server binary | Linux x86_64 GNU with glibc >= 2.35 only (Ubuntu 22.04 baseline). No macOS, Windows, ARM or musl assets. |
| Managed download | Supported on Linux x86_64 GNU glibc >= 2.35. Escape hatches: an explicit `magit-lsp.server.path`, or `magit-lsp.server.source: "system"` with a version-matching binary on `PATH`. |
| Neovim client | Neovim 0.11+; the client never downloads a server. |
| VS Code client | VS Code 1.85+. |
| Windows / macOS | Managed download unsupported; other server sources untested. |
| Hosted and clean-machine evidence | Not yet collected: the tracked workflows have not run and there is no clean-machine install smoke. |
| Git | 2.30.8 is the tested recommendation, not a hard minimum; older versions are not blocked and some operations may fail. |

### Disclosures

**Same-session reads (BUG-READ-1).** The larger fix is deferred, not complete.
The project's own record states:

> BUG-READ-1: minimal intake/unblocked-request slice is complete under
> R-PLAIN-PREREQ; evidence is the checked record and probe noted in the Plain
> task file. The larger same-session read redesign remains deferred to T103. It
> is not closure of the larger design.

Source: the [task queue migration record](docs/evidence/task-queue-migration.md).
In 0.2.0 a view keeps one acknowledged tree. Successive renders may be
delivered before any acknowledgement, and standard-provider reads use the last
acknowledged tree, so a read in the same session can reflect the previously
acknowledged state while a newer render is still in flight; an acknowledgement
for an older version is silently ignored. The
[rich protocol audit](docs/design/rich-protocol-audit.md) records the current
contract, and the redesign is queued as T103 in [TASKS.md](TASKS.md).

**History rewrites.** Operations that change existing commits are destructive
and are not recoverable through magit-lsp: discarded content is not saved to a
private ref, and rewriting a commit that is reachable from a remote-tracking
branch prompts first because it may require a force push. Rewriting shared
history still needs an explicit policy decision about what counts as published
and which confirmation applies. Existing wording: the README says discarded
content is not saved to a private ref and suggests Git commits or stashes when
you need recoverable history ([README features](README.md)); the roadmap review
requires an explicit policy decision — "Rewriting shared history needs an
explicit policy decision: what counts as published, and what confirmation
behavior applies" — and says "Do not impose blanket confirmation silently"
([roadmap review](docs/ROADMAP_REVIEW.md)).
Several history-changing actions, including rebase, remain Planned; see the
generated [parity reference](docs/parity.md).
