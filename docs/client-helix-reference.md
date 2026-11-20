# Client Helix reference

### Generic Helix reference configuration

Illustrative generic Helix TOML (no editor-specific expansions):

<!-- helix-reference-config -->
```toml
[language-server.magit-lsp]
command = "magit-lsp"

[[language]]
name = "magit"
scope = "source.magit"
file-types = [{ glob = "*.magit" }]
roots = [".git"]
language-servers = ["magit-lsp"]

[[language]]
name = "git-commit"
language-servers = ["magit-lsp"]
```

`make helix-config-check` extracts this exact block and runs the installed
`hx` with an isolated `HOME` and XDG configuration, cache, data and state. It
fails unless the server attaches, renders the status, stages one file and then
opens the real commit draft it announces, keeping HEAD and the worktree
unchanged. It needs local `hx`, `tmux` and `git`, installs nothing and makes no
wider Helix or operating-system promise.

The second entry is a partial override: Helix merges language entries by name,
so `git-commit` keeps its built-in `gitcommit` grammar, `COMMIT_EDITMSG` and
`MERGE_MSG` file types, `#` comment token, indentation, 51/73-column rulers and
72-column text width. Only `language-servers` changes, adding the server.

The editor helper is required; repository resolution has no launcher. Helix does
not declare `window.showDocument`, so the server cannot open the draft itself.
When a commit starts, follow the server's message and open the owned
`.git/COMMIT_EDITMSG` with an ordinary `:open .git/COMMIT_EDITMSG` (the unopened
draft deadline is 30 seconds). Opening that file attaches `magit-lsp`
automatically through the `git-commit` entry above; no `:set-language` or manual
language change is needed. The draft buffer then offers the native Confirm
commit or Abort commit code action. An already-attached draft buffer can be
reused for the next run; its new template is synchronized through a versioned
edit.

The [manual Helix 25.07.1 checklist](evidence/plain-helix.md) records Linux
open/status/stage/menu/log, Confirm, Abort and connection reuse. Helix did not
declare `window.showDocument`; no automatic attachment or general Helix support
promise follows. For a minimal Plain Neovim client, use the [plugin-free Neovim
configuration](CLIENT_INSTALL.md#neovim-plugin-free-plain-lsp); the test init
`tests/ui/plain_init.lua` boots that exact block with test instrumentation.
