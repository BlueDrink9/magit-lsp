#!/usr/bin/env python3
"""Assert a packaged VSIX carries the unbundled client's entries and runtime deps.

Used by `make vscode-package`: a VSIX that omits the extension entry point, its
metadata or either unbundled runtime dependency (`jsonc-parser`,
`vscode-languageclient`) must fail packaging instead of shipping broken.
"""

import sys
import zipfile

# Archive paths the installed extension needs at activation time.
REQUIRED = [
    "extension/extension.js",
    "extension/package.json",
    # vsce lowercases the shipped README to readme.md.
    "extension/readme.md",
    "extension/node_modules/jsonc-parser/package.json",
    "extension/node_modules/jsonc-parser/lib/umd/main.js",
    "extension/node_modules/vscode-languageclient/package.json",
    "extension/node_modules/vscode-languageclient/lib/node/main.js",
]


def main(argv):
    if len(argv) != 2:
        print("usage: check_vsix.py <vsix>", file=sys.stderr)
        return 2
    vsix = argv[1]
    try:
        archive = zipfile.ZipFile(vsix)
    except (OSError, zipfile.BadZipFile) as error:
        print(f"FAILED: cannot read VSIX {vsix}: {error}", file=sys.stderr)
        return 1

    with archive:
        entries = {info.filename: info for info in archive.infolist()}
        missing = [name for name in REQUIRED if name not in entries]
        if missing:
            print(f"FAILED: {vsix} is missing required content:", file=sys.stderr)
            for name in missing:
                print(f"  {name}", file=sys.stderr)
            return 1
        listing = f"{vsix}.contents.txt"
        with open(listing, "w", encoding="utf-8") as output:
            for name in sorted(entries):
                output.write(f"{entries[name].file_size:>10}  {name}\n")
        print(f"vsix: {vsix} ({len(entries)} entries, listing: {listing})")
        for name in REQUIRED:
            print(f"  {entries[name].file_size:>8}  {name}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
