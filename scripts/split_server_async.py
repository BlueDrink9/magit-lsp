#!/usr/bin/env python3
"""Prepare or explicitly apply the reviewed server_async test split."""

from __future__ import annotations

import hashlib
import difflib
import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "tests/server_async.rs"
DESTINATION = ROOT / "tests/server_async"
EXPECTED_SHA256 = "387585f500b1bb8a97fbedcff3ea29a33efaf6c26e5cefbaf86fa33af57fb87b"
RICH_START = b"fn limited_rich(effects: Value, prompts: Value) -> Value {\n"
RICH_END = b"/// Open a status view and consume its initial render and keymap pushes.\n"
REPOSITORY_START = b"#[test]\nfn repository_configuration_is_trusted_isolated_canonical_and_reloaded() {\n"
REPOSITORY_END = b"/// Full rich declaration including every native pick shape.\n"
MODULES = b'#[path = "server_async/rich.rs"]\nmod rich;\n#[path = "server_async/repository.rs"]\nmod repository;\n'
MAX_BYTES = 100_000


def digest(data: bytes) -> str:
    """Return the SHA-256 digest used for source and output reporting."""
    return hashlib.sha256(data).hexdigest()


def outputs(source: bytes) -> dict[Path, bytes]:
    """Validate the exact baseline and construct all split files in memory."""
    if digest(source) != EXPECTED_SHA256:
        raise ValueError("server_async.rs differs from the reviewed baseline")
    rich_start = source.find(RICH_START)
    rich_end = source.find(RICH_END, rich_start)
    repo_start = source.find(REPOSITORY_START)
    if min(rich_start, rich_end, repo_start) < 0 or not rich_start < rich_end < repo_start:
        raise ValueError("unexpected server_async.rs section layout")
    rich = source[rich_start:rich_end]
    middle = source[rich_end:repo_start]
    repo_end = source.find(REPOSITORY_END, repo_start)
    if repo_end < 0:
        raise ValueError("repository section end marker is missing")
    repository = source[repo_start:repo_end]
    suffix = source[repo_end:]
    prefix = source[:rich_start]
    if prefix + rich + middle + repository + suffix != source:
        raise ValueError("source sections do not reconstruct the baseline")
    marker = b"mod common;\n"
    if prefix.count(marker) != 1:
        raise ValueError("expected exactly one common module declaration")
    wiring = MODULES + b"\nuse rich::{invoke_rich, limited_rich};\nuse repository::keymap_keys;\n"
    parent = prefix.replace(marker, marker + wiring, 1) + middle + suffix
    rich_file = b"use super::*;\n\n" + rich.removesuffix(b"\n")
    rich_file = rich_file.replace(b"fn limited_rich", b"pub(super) fn limited_rich", 1)
    rich_file = rich_file.replace(b"fn invoke_rich", b"pub(super) fn invoke_rich", 1)
    rich_file = rich_file.replace(b'include_str!("snapshots/', b'include_str!("../snapshots/')
    repository_file = b"use super::*;\nuse super::rich::{invoke_rich, limited_rich};\n\n" + repository.removesuffix(b"\n")
    repository_file = repository_file.replace(b"fn keymap_keys", b"pub(super) fn keymap_keys", 1)
    repository_file = repository_file.replace(b'include_str!("snapshots/', b'include_str!("../snapshots/')
    result = {
        SOURCE: parent,
        DESTINATION / "rich.rs": rich_file,
        DESTINATION / "repository.rs": repository_file,
    }
    for path, data in result.items():
        if len(data) >= MAX_BYTES:
            raise ValueError(f"planned output exceeds {MAX_BYTES} bytes: {path.relative_to(ROOT)}")
    if ((rich_file[len(b"use super::*;\n\n") :] + b"\n")
            .replace(b"pub(super) fn limited_rich", b"fn limited_rich", 1)
            .replace(b"pub(super) fn invoke_rich", b"fn invoke_rich", 1)
            .replace(b'include_str!("../snapshots/', b'include_str!("snapshots/') != rich
            or (repository_file[len(b"use super::*;\nuse super::rich::{invoke_rich, limited_rich};\n\n") :] + b"\n")
            .replace(b"pub(super) fn keymap_keys", b"fn keymap_keys")
            .replace(b'include_str!("../snapshots/', b'include_str!("snapshots/') != repository):
        raise ValueError("moved test bodies were not preserved")
    if parent != prefix.replace(marker, marker + wiring, 1) + middle + suffix:
        raise ValueError("parent contains unexpected edits")
    return result


def main() -> int:
    """Dry-run or audit without writes; apply only for explicit APPLY=1."""
    apply = os.environ.get("APPLY", "")
    check = sys.argv[1:] == ["--check-applied"]
    if apply not in ("", "1") or (check and apply) or (sys.argv[1:] and not check):
        print("use no arguments or --check-applied; APPLY=1 is apply-only", file=sys.stderr)
        return 2
    try:
        source = (subprocess.check_output(
            ["git", "show", "89dc88c:tests/server_async.rs"], cwd=ROOT
        ) if check else SOURCE.read_bytes())
        planned = outputs(source)
        if check:
            actual = {path: path.read_bytes() for path in planned}
            for path, data in planned.items():
                if actual[path] != data:
                    print("".join(difflib.unified_diff(
                        data.decode().splitlines(keepends=True),
                        actual[path].decode().splitlines(keepends=True),
                        fromfile="planned", tofile=str(path.relative_to(ROOT)),
                    )), file=sys.stderr)
                    raise ValueError(f"applied output differs from plan: {path.relative_to(ROOT)}")
            tests = rb"(?m)^#\[test\]\nfn ([a-zA-Z0-9_]+)\("
            inventory = sorted(re.findall(tests, source))
            if inventory != sorted(name for data in actual.values() for name in re.findall(tests, data)):
                raise ValueError("test inventory differs from baseline")
            print(f"inventory: {len(inventory)} tests match 89dc88c exactly")
        if apply:
            if DESTINATION.exists():
                raise ValueError(f"destination already exists: {DESTINATION.relative_to(ROOT)}")
            for path in planned:
                if path != SOURCE and path.exists():
                    raise ValueError(f"destination already exists: {path.relative_to(ROOT)}")
        print("mode: check-applied (no files written)" if check else "mode: apply" if apply else "mode: dry-run (no files written)")
        for path, data in planned.items():
            print(f"{path.relative_to(ROOT)}: {len(data)} bytes sha256={digest(data)}")
        print("preservation: source baseline, test/helper bodies, comments, ordering, and allowed module wiring validated")
        if apply:
            DESTINATION.mkdir()
            for path, data in planned.items():
                if path != SOURCE:
                    path.write_bytes(data)
            SOURCE.write_bytes(planned[SOURCE])
            print("applied split")
        elif not check:
            print("apply only after review: make split-server-async APPLY=1")
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"refusing split: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
