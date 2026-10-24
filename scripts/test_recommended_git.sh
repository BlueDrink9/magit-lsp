#!/bin/sh
# Build integrity-pinned recommended Git and run the backend compatibility gate.
set -eu

version=$(cat "$(dirname "$0")/../RECOMMENDED_GIT_VERSION")
test "$version" = 2.30.8
sha=62e95f694f28ba4f35bae1d0d19328093ac4783dc5b9c84e2e3d5eca0ab064e9
work=${RECOMMENDED_GIT_WORK_DIR:-/tmp/opencode/git230}
mkdir -p "$work"
work=$(cd "$work" && pwd)
archive="$work/git-$version.tar.xz"
if [ ! -f "$archive" ]; then
    curl --fail --location "https://www.kernel.org/pub/software/scm/git/git-$version.tar.xz" -o "$archive"
fi
printf '%s  %s\n' "$sha" "$archive" | sha256sum -c -
source="$work/source/git-$version"
prefix="$work/install"
if [ ! -d "$source" ]; then
    mkdir -p "$work/source"
    tar -xf "$archive" -C "$work/source"
fi
# GNU99 avoids compiler-default C23 incompatibilities; explicit zlib flags can
# be supplied when development headers and runtime libraries are split.
make -C "$source" -j2 prefix="$prefix" \
    CFLAGS="-O2 -std=gnu99 ${RECOMMENDED_GIT_CFLAGS:-}" \
    LDFLAGS="${RECOMMENDED_GIT_LDFLAGS:-}" \
    NO_GETTEXT=YesPlease NO_TCLTK=YesPlease NO_CURL=YesPlease \
    NO_EXPAT=YesPlease NO_OPENSSL=YesPlease NO_PERL=YesPlease \
    NO_PYTHON=YesPlease install

# Clear inherited Git overrides before selecting both executable and helpers.
env -i HOME="$HOME" PATH="$prefix/bin:$PATH" \
    GIT_EXEC_PATH="$prefix/libexec/git-core" \
    GIT_TEMPLATE_DIR="$prefix/share/git-core/templates" \
    RECOMMENDED_GIT_PREFIX="$prefix" sh -eu -c '
    test "$(command -v git)" = "$RECOMMENDED_GIT_PREFIX/bin/git"
    test "$(git --version)" = "git version 2.30.8"
    test "$(git --exec-path)" = "$RECOMMENDED_GIT_PREFIX/libexec/git-core"
    test "$("$(git --exec-path)/git" --version)" = "git version 2.30.8"
    printf "Selected %s; helpers %s\n" "$(command -v git)" "$(git --exec-path)"
    cargo test --test git_backend --test status --test patch_cases
'
