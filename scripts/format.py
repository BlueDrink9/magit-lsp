#!/usr/bin/env python3
"""Check changed source formatting without modifying the checkout or Git index."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

TOOL_ROOT = Path(__file__).resolve().parents[1]
ROOT = Path.cwd()
LUA = {".lua"}
WEB = {".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx", ".mts", ".cts"}
RUST = {".rs"}
SKIP_PARTS = {"node_modules", "target", "vendor", "fixtures", "generated"}


def run_git(*args: str, check: bool = True) -> bytes:
    """Run Git in the repository and return its byte output."""
    result = subprocess.run(["git", *args], cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if check and result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace").strip() or f"git {' '.join(args)} failed")
    return result.stdout


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


def invoke(command: list[str], cwd: Path, label: str) -> bool:
    """Run one checker and print readable missing-tool or formatter guidance."""
    try:
        result = subprocess.run(command, cwd=cwd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    except OSError as error:
        print(f"{label}: cannot run {command[0]}: {error}", file=sys.stderr)
        return False
    if result.returncode:
        print(f"{label}: formatting check failed; run {label} on the affected source to fix it", file=sys.stderr)
        if result.stdout:
            print(result.stdout.rstrip(), file=sys.stderr)
        return False
    return True


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


def main(argv: list[str] | None = None) -> int:
    """Check staged content or committed changed paths without writing to the checkout."""
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--staged", action="store_true", help="check staged index content")
    mode.add_argument("--base", metavar="COMMIT", help="check HEAD changes against commit, or EMPTY_TREE for a new branch")
    args = parser.parse_args(argv)
    try:
        global ROOT
        ROOT = Path(run_git("rev-parse", "--show-toplevel").decode().strip())
        paths = [path for path in changed_paths(args.base) if eligible(path)]
        if not paths:
            print("format: no owned source changes")
            return 0
        with tempfile.TemporaryDirectory(prefix="magit-format-") as temporary:
            tree = Path(temporary)
            snapshot(args.base, tree)
            lua = [path for path in paths if Path(path).suffix.lower() in LUA]
            web = [path for path in paths if Path(path).suffix.lower() in WEB]
            rust = [path for path in paths if Path(path).suffix.lower() in RUST]
            okay = True
            stylua = TOOL_ROOT / "clients/vscode/node_modules/.bin/stylua"
            prettier = TOOL_ROOT / "clients/vscode/node_modules/.bin/prettier"
            for name, tool, files in (("StyLua 2.6.0", stylua, lua), ("Prettier 3.9.10", prettier, web)):
                if files:
                    if not tool.is_file():
                        print(f"{name} missing at {tool}; install locked VSCode dependencies with npm ci in clients/vscode", file=sys.stderr)
                        okay = False
                    else:
                        okay &= invoke([str(tool), "--check", *files], tree, name)
            if rust:
                okay &= invoke(["cargo", "fmt", "--all", "--", "--check"], tree, "cargo fmt --all -- --check")
            return 0 if okay else 1
    except (RuntimeError, OSError) as error:
        print(f"format: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
