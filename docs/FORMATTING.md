# Formatting

`make format-check` checks owned working-tree changes without requiring staging.
`make format` formats those files in place. Both cover changed staged, unstaged
and untracked nonignored sources; ignored files are excluded. Legacy formatting
is checked only when a maintained source file changes, so these commands do not
normalize the whole repository.

Install the formatter tools. Rustfmt and StyLua come from the locked development
environment and are resolved from `PATH`; CI runs the format job inside that same
flake shell (`nix develop --no-update-lock-file -c ...`), so the formatter
versions come from `flake.lock` rather than an ad hoc install. The repository
hook is optional:

```sh
nix develop
```

Install the locked JavaScript/TypeScript formatter:

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

## Full tracked sweep

Whole-repository formatting is explicit opt-in and runs one formatter at a time.
The sweep selects every tracked regular file the dispatcher owns and skips
`vendor/`, `generated/`, `fixtures/`, `node_modules/` and `target/` trees, text
fixtures and untracked files.

```sh
make format-all-list                         # exact tracked files and counts
make format-all-check FORMATTER=rustfmt      # fail-capable full check
make format-all FORMATTER=rustfmt            # rewrite clean working copies
```

`FORMATTER` is `rustfmt`, `stylua` or `prettier`. At this commit the sweep
covers 171 Rust, 32 Lua and 49 JavaScript/TypeScript files; run
`make format-all-list` for the exact current list. `format-all` refuses to run
when any selected file differs from the index, so it never overwrites unrelated
work; revert only its own output with `git checkout -- <paths>` if needed, then
stage the exact selected paths with `git add -- <paths>`.

Rust formatting is not `cargo fmt --all`: the dispatcher runs `rustfmt` on the
selected inputs with `skip_children=true`, so a sweep stays inside the eligible
set. Prettier is the locked `clients/vscode/node_modules/.bin/prettier` tool.

The pre-commit hook formats staged content through the same dispatcher. It
updates only eligible staged blobs and never changes working files. For a
format-only sweep commit, opt in to a completeness guard:

```sh
MAGIT_FORMAT_SWEEP=rustfmt git commit ...
```

That fixes staged content first, then fails the commit unless every tracked file
owned by that formatter is formatted in the index. Without the variable the hook
behaves exactly as before. Run `make hooks-run` to invoke the actual hook; CI
checks committed changed paths/check-only through the same runner and cannot
write.
