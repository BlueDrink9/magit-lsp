#!/usr/bin/env python3
"""Check or format changed source files, including staged index content and
explicit whole-repository tracked sweeps."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib

ROOT = Path.cwd()
LUA = {".lua"}
WEB = {".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx", ".mts", ".cts"}
RUST = {".rs"}
FORMATTERS = ("rustfmt", "stylua", "prettier")
SKIP_PARTS = {"node_modules", "target", "vendor", "fixtures", "generated"}


def run_git(*args: str, check: bool = True) -> bytes:
    """Run Git in the repository and return its byte output."""
    result = subprocess.run(["git", *args], cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if check and result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace").strip() or f"git {' '.join(args)} failed")
    return result.stdout


def clean_git_env() -> dict[str, str]:
    """Return an environment without inherited Git repository/index overrides."""
    return {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}


def changed_paths(base: str | None) -> list[str]:
    """Return changed paths using NUL-delimited Git output, including rename destinations."""
    if base is None:
        if run_git("ls-files", "-u", "-z"):
            raise RuntimeError("unmerged index entries found; resolve conflicts before formatting")
        raw = run_git("diff", "--cached", "--name-status", "-z", "--find-renames")
    else:
        if base == "EMPTY_TREE":
            raw = run_git("diff", "--name-status", "-z", "--find-renames", base_tree(), "HEAD")
        else:
            raw = run_git("diff", "--name-status", "-z", "--find-renames", f"{base}...HEAD")
    fields = raw.decode("utf-8", errors="surrogateescape").split("\0")
    paths: list[str] = []
    i = 0
    while i < len(fields) and fields[i]:
        status = fields[i]
        i += 1
        count = 2 if status.startswith("R") or status.startswith("C") else 1
        values = fields[i : i + count]
        i += count
        path = values[-1]
        if status[0] != "D":
            paths.append(path)
    return paths


def working_paths() -> list[str]:
    """Return staged, unstaged and untracked nonignored working-tree paths."""
    raw = run_git("status", "--porcelain=v1", "-z", "--untracked-files=all")
    fields = raw.decode("utf-8", errors="surrogateescape").split("\0")
    paths: list[str] = []
    i = 0
    while i < len(fields) and fields[i]:
        entry = fields[i]
        i += 1
        path = entry[3:]
        if "R" in entry[:2] or "C" in entry[:2]:
            i += 1
        if "D" not in entry[:2]:
            paths.append(path)
    return paths


def base_tree() -> str:
    """Return Git's canonical empty-tree object id for new-branch comparisons."""
    return run_git("hash-object", "-t", "tree", "/dev/null").decode().strip()


def eligible(path: str) -> bool:
    """Return whether path is an owned formatter input rather than generated or fixture data."""
    parts = Path(path).parts
    if any(part.lower() in SKIP_PARTS for part in parts):
        return False
    suffix = Path(path).suffix.lower()
    if suffix in LUA:
        return path.startswith(("lua/", "scripts/", "tests/"))
    if suffix in WEB:
        return path.startswith(("lua/", "scripts/", "clients/", "tests/"))
    return suffix in RUST


def formatter_for(path: str) -> str | None:
    """Return the formatter that owns an eligible path."""
    suffix = Path(path).suffix.lower()
    if suffix in LUA:
        return "stylua"
    if suffix in WEB:
        return "prettier"
    if suffix in RUST:
        return "rustfmt"
    return None


def invoke(command: list[str], cwd: Path, label: str) -> bool:
    """Run one formatter and print readable missing-tool or failure guidance."""
    try:
        result = subprocess.run(command, cwd=cwd, env=clean_git_env(), text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    except OSError as error:
        print(f"{label}: cannot run {command[0]}: {error}", file=sys.stderr)
        return False
    if result.returncode:
        print(f"{label}: formatting command failed", file=sys.stderr)
        if result.stdout:
            print(result.stdout.rstrip(), file=sys.stderr)
        return False
    return True


def format_paths(paths: list[str], tree: Path, write: bool, selected: list[str] | None = None) -> bool:
    """Run the selected shared language formatters for paths in a workspace snapshot."""
    wanted = None if selected is None else set(selected)

    def picked(name: str) -> bool:
        return wanted is None or name in wanted

    lua = [path for path in paths if formatter_for(path) == "stylua" and picked("stylua")]
    web = [path for path in paths if formatter_for(path) == "prettier" and picked("prettier")]
    rust = [path for path in paths if formatter_for(path) == "rustfmt" and picked("rustfmt")]
    okay = True
    stylua = shutil.which("stylua")
    prettier_path = ROOT / "clients/vscode/node_modules/.bin/prettier"
    prettier = shutil.which(str(prettier_path))
    for name, tool, files, missing in (
        ("StyLua 2.6.0", stylua, lua, "install StyLua 2.6.0 on PATH, for example by entering the Nix dev shell with nix develop"),
        ("Prettier 3.9.10", prettier, web, f"install locked VSCode dependencies with npm ci in clients/vscode (expected at {prettier_path})"),
    ):
        if files:
            if tool is None:
                print(f"{name} missing; {missing}", file=sys.stderr)
                okay = False
            else:
                command = [tool]
                if name.startswith("Prettier") and write:
                    command.append("--write")
                elif not write:
                    command.append("--check")
                okay &= invoke([*command, *files], tree, name)
    if rust:
        manifest = tomllib.loads((tree / "Cargo.toml").read_text())
        edition = str(manifest["package"]["edition"])
        # rustfmt normally follows mod declarations into unselected files.
        # Format only the selected inputs, retaining the snapshot mutation guard.
        command = ["rustfmt", "--edition", edition, "--config", "skip_children=true"]
        if not write:
            command.append("--check")
        okay &= invoke([*command, *rust], tree, "rustfmt selected inputs")
    return okay


def git_bytes(args: list[str], *, env: dict[str, str], input: bytes | None = None) -> bytes:
    """Run Git with byte-safe arguments and return stdout or raise a readable error."""
    result = subprocess.run(["git", *args], cwd=ROOT, env=env, input=input, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace").strip() or f"git {' '.join(args)} failed")
    return result.stdout


def tree_state(root: Path) -> dict[str, tuple[int, bytes, bool]]:
    """Capture regular files and modes below a formatter snapshot."""
    state: dict[str, tuple[int, bytes, bool]] = {}
    for path in root.rglob("*"):
        if path.is_symlink():
            state[path.relative_to(root).as_posix()] = (path.lstat().st_mode & 0o777, os.fsencode(os.readlink(path)), True)
        elif path.is_file():
            relative = path.relative_to(root).as_posix()
            state[relative] = (path.stat().st_mode & 0o777, path.read_bytes(), False)
    return state


def staged_records(index_env_vars: dict[str, str]) -> dict[str, tuple[int, str]]:
    """Read stage-zero index paths, modes and object ids without pathname parsing."""
    raw = git_bytes(["ls-files", "--stage", "-z"], env=index_env_vars)
    records: dict[str, tuple[int, str]] = {}
    for field in raw.split(b"\0"):
        if not field:
            continue
        meta, raw_path = field.split(b"\t", 1)
        mode, oid, stage = meta.split(b" ")
        if stage != b"0":
            raise RuntimeError("unmerged index entries found; resolve conflicts before formatting")
        path = os.fsdecode(raw_path)
        if path in records or mode not in (b"100644", b"100755"):
            continue
        records[path] = (int(mode, 8), oid.decode())
    return records


def tracked_eligible(formatters: list[str] | None = None) -> list[str]:
    """Return tracked regular maintained sources, optionally restricted to formatters."""
    records = staged_records(os.environ.copy())
    paths = [path for path in records if eligible(path)]
    if formatters:
        paths = [path for path in paths if formatter_for(path) in formatters]
    return sorted(paths)


def ensure_clean_worktree(paths: list[str]) -> None:
    """Refuse to overwrite tracked inputs that differ from the index."""
    if not paths:
        return
    raw = run_git("diff", "--name-only", "-z", "--", *paths)
    dirty = [path for path in raw.decode("utf-8", errors="surrogateescape").split("\0") if path]
    if dirty:
        raise RuntimeError(
            "refusing to overwrite unstaged changes in: "
            + ", ".join(dirty)
            + "; stage or revert them before the full sweep"
        )
    for path in paths:
        candidate = ROOT / path
        if candidate.is_symlink() or not candidate.is_file():
            raise RuntimeError(f"tracked formatter input is not a regular file: {path}")


def staged_fix(paths: list[str], snapshot_root: Path, index_path: Path, index_env_vars: dict[str, str], gitdir: Path) -> int:
    """Format staged blobs in isolation, then atomically publish eligible replacements."""
    effective_index = Path(os.environ.get("GIT_INDEX_FILE", str(index_path))).resolve()
    observed_index = effective_index
    before_bytes = observed_index.read_bytes()
    original = staged_records(index_env_vars)
    allowed = {path for path in paths if path in original and eligible(path)}
    expected_indices = {index_path.resolve(), (gitdir / "index.lock").resolve()}
    unusual = effective_index not in expected_indices
    if not allowed:
        return 0
    # A temporary commit index may contain only explicitly selected paths. Clean
    # checks are safe; changing it could alter Git's path-only commit semantics.
    for path in allowed:
        current = ROOT
        for part in Path(path).parts:
            current = current / part
            if current.is_symlink():
                raise RuntimeError(f"unsafe staged formatter path through symlink: {path}")
    git_bytes(["checkout-index", "--all", "--force", f"--prefix={snapshot_root}/"], env=index_env_vars)
    if unusual and not format_paths(sorted(allowed), snapshot_root, False):
        raise RuntimeError("staged formatter autofix needs the normal index; stage changes and commit without paths")
    if unusual:
        return 0
    before = tree_state(snapshot_root)
    if not format_paths(sorted(allowed), snapshot_root, True):
        return 1
    after = tree_state(snapshot_root)
    unexpected = {path for path in before.keys() | after.keys() if before.get(path) != after.get(path)} - allowed
    if unexpected:
        raise RuntimeError("formatter changed files outside staged formatter inputs: " + ", ".join(sorted(unexpected)))
    if not format_paths(sorted(allowed), snapshot_root, False):
        return 1
    changed: dict[str, str] = {}
    for path in allowed:
        candidate = snapshot_root / path
        if not candidate.is_file() or candidate.is_symlink():
            raise RuntimeError(f"formatter removed or replaced staged path: {path}")
        blob = git_bytes(["hash-object", "-w", "--path", path, "--", str(candidate)], env=index_env_vars).decode().strip()
        if blob != original[path][1]:
            changed[path] = blob
    if not changed:
        return 0
    if observed_index.read_bytes() != before_bytes:
        raise RuntimeError("Git index changed concurrently; retry the commit")
    updates = b"".join(f"{original[path][0]:o} {oid}\t".encode() + os.fsencode(path) + b"\0" for path, oid in changed.items())
    git_bytes(["update-index", "-z", "--index-info"], env=index_env_vars, input=updates)
    return 0


def snapshot(base: str | None, destination: Path) -> None:
    """Materialize the index or committed tree into an isolated repository-context snapshot."""
    if base is None:
        result = subprocess.run(["git", "checkout-index", "--all", "--force", f"--prefix={destination}/"], cwd=ROOT)
        if result.returncode:
            raise RuntimeError("could not materialize staged index snapshot")
    else:
        archive = subprocess.Popen(["git", "archive", "HEAD"], cwd=ROOT, stdout=subprocess.PIPE)
        extract = subprocess.run(["tar", "-x", "-C", str(destination)], stdin=archive.stdout)
        archive.stdout.close()
        if archive.wait() or extract.returncode:
            raise RuntimeError("could not materialize HEAD snapshot")


def sweep_check(formatter: str) -> int:
    """Fail unless every tracked file owned by formatter is formatted in the index."""
    paths = tracked_eligible([formatter])
    if not paths:
        print(f"format: no tracked {formatter} sources")
        return 0
    with tempfile.TemporaryDirectory(prefix="magit-format-") as temporary:
        tree = Path(temporary)
        snapshot(None, tree)
        return 0 if format_paths(paths, tree, False, [formatter]) else 1


def main(argv: list[str] | None = None) -> int:
    """Format or check changed, staged or all tracked maintained source paths."""
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--working-tree", action="store_true", help="check working-tree changes")
    mode.add_argument("--staged", action="store_true", help="check staged index content")
    mode.add_argument("--base", metavar="COMMIT", help="check HEAD changes against commit, or EMPTY_TREE for a new branch")
    mode.add_argument("--all-tracked", action="store_true", help="check or write every tracked maintained source")
    mode.add_argument("--list", action="store_true", help="list tracked maintained sources by formatter")
    parser.add_argument("--formatter", action="append", choices=FORMATTERS, help="restrict --all-tracked or --list to a formatter (repeatable)")
    parser.add_argument("--write", action="store_true", help="write formatted working-tree files (working-tree or all-tracked mode)")
    parser.add_argument("--fix", action="store_true", help="safely fix staged content (hook mode only)")
    parser.add_argument("--sweep", choices=FORMATTERS, help="after --staged --fix, verify all tracked files for one formatter")
    args = parser.parse_args(argv)
    try:
        if args.write and not (args.working_tree or args.all_tracked):
            raise RuntimeError("--write is only allowed with --working-tree or --all-tracked")
        if args.fix and (not args.staged or args.write):
            raise RuntimeError("--fix is only allowed with --staged")
        if args.formatter and not (args.all_tracked or args.list):
            raise RuntimeError("--formatter is only allowed with --all-tracked or --list")
        if args.all_tracked and not args.formatter:
            raise RuntimeError("--all-tracked requires at least one explicit --formatter")
        if args.sweep and (not args.staged or not args.fix):
            raise RuntimeError("--sweep is only allowed with --staged --fix")
        global ROOT
        ROOT = Path(run_git("rev-parse", "--show-toplevel").decode().strip())
        if args.list:
            for name in args.formatter or FORMATTERS:
                paths = tracked_eligible([name])
                print(f"{name}: {len(paths)}")
                for path in paths:
                    print(f"  {path}")
            return 0
        if args.all_tracked:
            paths = tracked_eligible(args.formatter)
            if not paths:
                print(f"format: no tracked {', '.join(args.formatter)} sources")
                return 0
            if args.write:
                ensure_clean_worktree(paths)
                return 0 if format_paths(paths, ROOT, True, args.formatter) else 1
            with tempfile.TemporaryDirectory(prefix="magit-format-") as temporary:
                tree = Path(temporary)
                snapshot(None, tree)
                return 0 if format_paths(paths, tree, False, args.formatter) else 1
        paths = [path for path in (working_paths() if args.working_tree else changed_paths(args.base)) if eligible(path)]
        if args.staged and args.fix:
            if run_git("ls-files", "-u", "-z"):
                raise RuntimeError("unmerged index entries found; resolve conflicts before formatting")
            with tempfile.TemporaryDirectory(prefix="magit-format-") as temporary:
                gitdir = Path(run_git("rev-parse", "--absolute-git-dir").decode().strip())
                index_path = gitdir / "index"
                snapshot_root = Path(temporary) / "tree"
                snapshot_root.mkdir()
                status = staged_fix(paths, snapshot_root, index_path, os.environ.copy(), gitdir)
                if status:
                    return status
                return sweep_check(args.sweep) if args.sweep else 0
        if not paths:
            print("format: no owned source changes")
            return 0
        with tempfile.TemporaryDirectory(prefix="magit-format-") as temporary:
            tree = Path(temporary)
            if not args.working_tree:
                snapshot(args.base, tree)
            else:
                tree = ROOT
            return 0 if format_paths(paths, tree, args.write) else 1
    except (RuntimeError, OSError) as error:
        print(f"format: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
