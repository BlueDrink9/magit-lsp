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
```

`make helix-config-check` extracts this exact block and runs the installed
`hx` with an isolated `HOME` and XDG configuration, cache, data and state. It
fails unless the server attaches, renders the status and stages one file while
HEAD and the worktree stay unchanged. It needs local `hx`, `tmux` and `git`,
installs nothing and makes no wider Helix or operating-system promise.

The editor helper is required; repository resolution has no launcher. With this
generic configuration, `COMMIT_EDITMSG` does not attach automatically. Follow the
server's manual open instruction, then run `:set-language magit` promptly on the
first draft (the unopened-draft deadline is 30 seconds). Use the native Confirm
commit or Abort commit code action. An already-attached draft buffer can be reused
for the next run; its new template is synchronized through a versioned edit.

The [manual Helix 25.07.1 checklist](evidence/plain-helix.md) records Linux
open/status/stage/menu/log, Confirm, Abort and connection reuse. Helix did not
declare `window.showDocument`; no automatic attachment or general Helix support
promise follows. For a minimal Plain Neovim client, use the [plugin-free Neovim
configuration](CLIENT_INSTALL.md#neovim-plugin-free-plain-lsp); the test init
`tests/ui/plain_init.lua` boots that exact block with test instrumentation.
