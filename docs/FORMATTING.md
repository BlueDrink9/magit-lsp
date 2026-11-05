# Formatting

`make format-check` checks owned working-tree changes without requiring staging.
`make format` formats those files in place. Both cover changed staged, unstaged
and untracked nonignored sources; ignored files are excluded. Legacy formatting
is checked only when a maintained source file changes, so these commands do not
normalize the whole repository.

Install the locked Lua and JavaScript/TypeScript formatter tools:

```sh
npm ci --prefix clients/vscode
```

Opt in to the repository hook, or remove it later:

```sh
make hooks-install
make hooks-remove
```

Check or format working-tree sources:

```sh
make format-check
make format
```

After `make format`, review and stage the intended changes again. Formatting
never changes the index, so partial staging remains intact; formatted worktree
content may differ from staged blobs until re-staged.

The pre-commit hook formats staged content through the same dispatcher. It
updates only eligible staged blobs. It mirrors a fix to a working file only
when that file still matches the original staged content; partial unstaged
work remains untouched. Run `make hooks-run` to invoke the actual hook.
CI checks committed changed paths/check-only through the same runner and cannot write. Rust
formatting is workspace-aware: if any owned Rust source changes, the runner
uses `cargo fmt --all` (or `cargo fmt --all -- --check` in check modes).
Manual writes affect the workspace's Rust files, so inspect Rust changes before
staging. `--write` is accepted only with `--working-tree`.
