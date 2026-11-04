# Human-only README image guidance

This document is manual guidance for human maintainers publishing README images.
Agents must not capture, open, inspect or render images. Follow the no-vision
policy in [AGENTS.md](../AGENTS.md): automated native evidence is text and
structured state (`.screen`, accessibility/DOM text, JSON, wire/API and Git
state), never PNG or CDP image capture. `make readme-shots` prints this path;
it does not capture anything.

README images are optional human-owned assets. Use a real native editor window,
review it for secrets and legibility, and retain relevant commands, settings,
versions and commit alongside any published asset. Never present composed text,
JSON, a headless buffer or mock editor as a native screenshot. Do not add image
capture code or CI jobs.

Historical 2026-10-28 notes document bounded observations only; they are not
current agent instructions or proof of release readiness. Neovim `.screen`
artifacts were text evidence. Historical VSCode PNG observations and manual
desktop fallback limitations are recorded in
[RELEASING](RELEASING.md#final-bounded-local-verification-2026-10-28).

Client setup smoke in [CLIENT_INSTALL](CLIENT_INSTALL.md#evidence-scope-and-external-clients)
is headless evidence, not native screenshot evidence or a published install.
Manual image publication does not satisfy hosted, clean-machine or release
approval gates.
