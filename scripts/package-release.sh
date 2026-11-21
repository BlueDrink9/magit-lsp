#!/bin/sh
# Package an already-built target binary; never builds, executes or publishes.
set -eu
if [ "$#" -ne 5 ]; then
    echo 'usage: sh scripts/package-release.sh VERSION BINARY OUTPUT_DIR TARGET PROVENANCE' >&2
    exit 2
fi
directory=$(dirname -- "$0")
exec python3 "$directory/package_release.py" "$@"
