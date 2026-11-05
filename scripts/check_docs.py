#!/usr/bin/env python3
"""Check local Markdown links in selected documentation files."""

import re
import sys
from pathlib import Path


LINK = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)|(?<!!)\[[^\]]*\]\[([^]]*)\]")
HEADING = re.compile(r"^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$", re.M)


def check_document(path):
    """Return local Markdown link errors found in one document."""
    text = path.read_text(encoding="utf-8")
    heading_text = re.sub(r"```.*?```|~~~.*?~~~", "", text, flags=re.S)
    headings = {slug(match.group(1)) for match in HEADING.finditer(heading_text)}
    text = re.sub(r"```.*?```|~~~.*?~~~|`+[^`]*`+", "", text, flags=re.S)
    errors = []
    for match in LINK.finditer(text):
        target = (match.group(1) or match.group(2)).strip().split(maxsplit=1)[0].strip("<>")
        if re.match(r"[a-zA-Z][a-zA-Z0-9+.-]*:", target) or target.startswith("//"):
            continue
        file_part, _, fragment = target.partition("#")
        destination = path.parent / file_part if file_part else path
        if not destination.exists():
            errors.append(f"{path}: missing path {file_part}")
        elif fragment and destination.suffix.lower() == ".md":
            dest_text = re.sub(r"```.*?```|~~~.*?~~~", "", destination.read_text(encoding="utf-8"), flags=re.S)
            dest_headings = {slug(item.group(1)) for item in HEADING.finditer(dest_text)}
            if fragment.lower() not in dest_headings:
                errors.append(f"{path}: missing heading #{fragment} in {file_part or path.name}")
    return errors


def slug(heading):
    """Convert a Markdown heading to its GitHub-style fragment."""
    heading = re.sub(r"`([^`]*)`", r"\1", heading).lower()
    return re.sub(r"[^\w-]", "", re.sub(r"\s+", "-", heading))


def main(args):
    """Check requested Markdown files and return a process status."""
    paths = [Path(value) for value in args] or [Path("TASKS.md")]
    errors = [error for path in paths for error in check_document(path)]
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
