#!/usr/bin/env python3
"""Run report-only repository hygiene checks."""

import os
import subprocess
import shutil
from pathlib import Path


ROOT = Path.cwd()
LIMIT = int(os.environ.get("HYGIENE_SOFT_LIMIT", "100000"))


def tracked(root):
    """Return sorted Git-tracked paths, excluding Linguist-generated files."""
    result = subprocess.run(["git", "ls-files", "-z"], cwd=root, check=True, capture_output=True)
    paths = sorted(p.decode() for p in result.stdout.split(b"\0") if p)
    attrs = subprocess.run(
        ["git", "check-attr", "linguist-generated", "--", *paths],
        cwd=root, capture_output=True, text=True, check=True,
    )
    generated = {
        line.rsplit(": ", 1)[0].split(": linguist-generated: ", 1)[0]
        for line in attrs.stdout.splitlines() if line.endswith(": set")
    }
    return [p for p in paths if p not in generated]


def run_check(label, command, cwd, paths=()):
    """Run one optional checker and report availability, status, and output."""
    exe = command[0]
    if not Path(exe).is_file() and not shutil.which(exe):
        print(f"{label}: MISSING executable {exe}")
        return
    try:
        result = subprocess.run(command + list(paths), cwd=cwd, text=True, capture_output=True)
    except OSError as error:
        print(f"{label}: INVOCATION FAILURE: {error}")
        return
    failed_to_start = "no such command" in result.stderr.lower() or "not found" in result.stderr.lower()
    kind = "INVOCATION FAILURE" if result.returncode and failed_to_start else ("FINDINGS" if result.returncode else "CLEAN")
    print(f"{label}: {kind} (exit {result.returncode})")
    output = (result.stdout + result.stderr).strip()
    if output:
        print(output)


def report(root=ROOT):
    """Print tool status, tracked-file sizes, and tracked source markers."""
    paths = tracked(root)
    run_check("cargo machete", ["cargo", "machete"], root)
    local_knip = root / "clients/vscode/node_modules/.bin/knip"
    knip = str(local_knip) if local_knip.exists() else "knip"
    run_check("knip", [knip], root / "clients/vscode")
    lua_paths = [p for p in paths if p.endswith(".lua")]
    run_check("luacheck", ["luacheck", "--config", str(root / ".luacheckrc")], root, lua_paths)

    sizes = []
    markers = []
    for name in paths:
        path = root / name
        try:
            data = path.read_bytes()
            text = data.decode("utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        sizes.append((len(data), name, text.count("\n") + (bool(text) and not text.endswith("\n"))))
        for number, line in enumerate(text.splitlines(), 1):
            if "allow(dead_code)" in line or "keep-for:" in line:
                markers.append(f"{name}:{number}:{line.strip()}")
    print(f"Largest tracked text files (top 20; soft limit {LIMIT} bytes):")
    for size, name, lines in sorted(sizes, key=lambda item: (-item[0], item[1]))[:20]:
        print(f"{size:>9} bytes {lines:>7} lines {name}" + (" [over soft limit]" if size > LIMIT else ""))
    print("Tracked markers (allow(dead_code), keep-for:):")
    print("\n".join(markers) if markers else "(none)")


if __name__ == "__main__":
    report()
