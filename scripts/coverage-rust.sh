#!/bin/sh
# Rust-only line coverage through cargo-llvm-cov; see docs/TESTING.md.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

out=build/coverage/lcov.info
log=build/coverage/cargo-llvm-cov.log
# Same structural warning heuristic as WARN_RE in the Makefile, so the Make
# wrapper keeps its captured log when the tool output has warning diagnostics.
warn_re='^[[:space:]]*[Ww]arning:|^[[:space:]]*[[:upper:]][[:alnum:]]*Warning:|^.+:[0-9]+(:[0-9]+)?:[[:space:]]*[Ww]arning:|^.+:[0-9]+:[[:space:]]*[[:upper:]][[:alnum:]]*Warning:'

missing=
if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
    missing=cargo-llvm-cov
elif [ -n "${LLVM_COV:-}" ] && [ -x "${LLVM_COV}" ] &&
    [ -n "${LLVM_PROFDATA:-}" ] && [ -x "${LLVM_PROFDATA}" ]; then
    :
elif command -v llvm-cov >/dev/null 2>&1 &&
    command -v llvm-profdata >/dev/null 2>&1; then
    :
elif command -v rustup >/dev/null 2>&1 &&
    rustup component list --installed 2>/dev/null | grep -q '^llvm-tools'; then
    :
else
    missing="llvm-cov/llvm-profdata"
fi

case $missing in
cargo-llvm-cov)
    cat >&2 <<'EOF'
coverage-rust: missing prerequisite: cargo-llvm-cov

Enter the Nix dev shell (`nix develop`), which provides cargo-llvm-cov, or
install it with `cargo install cargo-llvm-cov`. Nothing was built or changed.
EOF
    exit 1
    ;;
llvm-cov/llvm-profdata)
    cat >&2 <<'EOF'
coverage-rust: missing prerequisite: llvm-cov/llvm-profdata

cargo-llvm-cov needs llvm-cov and llvm-profdata from the LLVM release that
built the active rustc (`rustc -vV` prints that release). Enter the Nix dev
shell (`nix develop`), which exports LLVM_COV and LLVM_PROFDATA, or set both to
a matching executable pair (for example the rustup llvm-tools-preview
component). Nothing was built or changed.
EOF
    exit 1
    ;;
esac

mkdir -p "$(dirname "$out")"
rm -f "$out" "$log" build/coverage/summary.txt
# Stale or truncated profiles from an interrupted earlier run make llvm-profdata
# abort the merge; the run regenerates coverage data from scratch anyway.
rm -f target/llvm-cov-target/*.profraw target/llvm-cov-target/*.profdata

# The ordinary Rust suite, instrumented by cargo-llvm-cov. Workspace
# dependencies are excluded by default; test harnesses are excluded so the
# report describes server source rather than the tests that exercise it.
# Test failures propagate: `make test-rust` owns the pass/fail gate, and this
# target must not report coverage as if the suite were green.
status=0
# cargo-llvm-cov's default profile name ends in `-%m`, the online-merge pool:
# every instrumented process appends to one lock-protected file. The suite
# re-executes the instrumented binary as children and SIGKILLs some of them, and
# a child killed while holding the merge lock leaves that shared file corrupt,
# which makes `llvm-profdata merge` abort with "file header is corrupt". The
# supported override writes one profile per process instead (`%p` keeps test
# binaries and re-executed children apart), so no shared pool is left for a
# killed child to corrupt. LLVM_PROFILE_FILE itself cannot be used:
# cargo-llvm-cov overwrites it unconditionally.
export LLVM_PROFILE_FILE_NAME='magit-lsp-%p.profraw'
cargo llvm-cov --no-report >"$log" 2>&1 || status=$?

# A child killed mid-dump also leaves its own truncated profile, and
# llvm-profdata aborts the whole merge on a single unreadable file. Trial-merge
# each profile and drop the unreadable ones; the summary reports how many were
# dropped so the lost counters stay visible instead of failing the whole run.
kept=0
dropped=0
if [ "$status" -eq 0 ]; then
    profdata=${LLVM_PROFDATA:-}
    if [ -z "$profdata" ] || [ ! -x "$profdata" ]; then
        if command -v llvm-profdata >/dev/null 2>&1; then
            profdata=llvm-profdata
        else
            profdata="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/llvm-profdata"
        fi
    fi
    if [ -x "$profdata" ] || command -v "$profdata" >/dev/null 2>&1; then
        for profile in target/llvm-cov-target/*.profraw; do
            [ -e "$profile" ] || continue
            if "$profdata" merge -sparse -o /dev/null "$profile" >/dev/null 2>&1; then
                kept=$((kept + 1))
            else
                printf 'coverage-rust: dropping unreadable profile %s\n' "$profile" >>"$log"
                rm -f "$profile"
                dropped=$((dropped + 1))
            fi
        done
    fi
    cargo llvm-cov report --lcov --output-path "$out" \
        --ignore-filename-regex='/tests/' >>"$log" 2>&1 || status=$?
fi

if [ "$status" -ne 0 ]; then
    tail -n 40 "$log" >&2 || true
    printf '\ncoverage-rust: cargo-llvm-cov failed (exit %s). Full command output: %s\n' \
        "$status" "$log" >&2
    if grep -Eq '^test result: .*FAILED|^error: [0-9]+ targets? failed|^error: test failed' "$log"; then
        cat >&2 <<'EOF'
The Rust suite failed; this target does not suppress test failures. Run
`make test-rust` for the ordinary test gate and inspect the failing test names
in the full log above.
EOF
    else
        cat >&2 <<'EOF'
The log shows a build or instrumentation failure, not a test result. Check the
full log above first. If it points at an LLVM tool mismatch, the Nix dev shell
(`nix develop`) provides cargo-llvm-cov with matching llvm-cov/llvm-profdata
(`rustc -vV` prints the active LLVM release; LLVM_COV and LLVM_PROFDATA select
the pair). This target never installs anything.
EOF
    fi
    exit 1
fi

if [ ! -s "$out" ] || ! grep -q 'src/server\.rs' "$out"; then
    echo "coverage-rust: LCOV report is empty or lacks server source" >&2
    exit 1
fi

if grep -q 'SF:.*/tests/' "$out"; then
    echo "coverage-rust: LCOV report unexpectedly includes test sources" >&2
    exit 1
fi

# Retain the full cargo-llvm-cov output above and forward its warning
# diagnostics so the Make wrapper's WARN_RE keeps its captured log.
if grep -Eq "$warn_re" "$log"; then
    printf 'warning: coverage-rust: cargo-llvm-cov emitted warnings; full output: %s\n' \
        "$log" >&2
    grep -E "$warn_re" "$log" | head -n 20 >&2 || true
fi

# Report line coverage from the LCOV records (LF = lines found, LH = hit).
# `make` captures the run's output, so the summary also lands in a file the
# Makefile echoes back on success.
{
    awk -v out="$out" -F: '
        /^SF:/ { files++; current = $0; sub(/^SF:/, "", current) }
        /^LF:/ { lf += $2 + 0; if (current ~ /src\/server\.rs$/) server_lf += $2 + 0 }
        /^LH:/ { lh += $2 + 0; if (current ~ /src\/server\.rs$/) server_lh += $2 + 0 }
        END {
            if (lf == 0) {
                printf "coverage-rust: %d source files reported in %s (no line data)\n", files, out
                exit
            }
            printf "coverage-rust: %d source files, %.1f%% of %d lines covered in %s\n",
                files, 100 * lh / lf, lf, out
            if (server_lf > 0)
                printf "coverage-rust: src/server.rs %.1f%% of %d lines covered\n",
                    100 * server_lh / server_lf, server_lf
        }' "$out"
    printf 'coverage-rust: %d of %d profraw files kept, %d unreadable dropped\n' \
        "$kept" "$((kept + dropped))" "$dropped"
} >build/coverage/summary.txt
cat build/coverage/summary.txt
