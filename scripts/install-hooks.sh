#!/bin/sh
# Opt in to the repository-owned formatter hook, or remove that setting.
set -eu

usage() {
    echo "Usage: $0 [install|remove]" >&2
    exit 2
}

action=${1:-install}
[ "$#" -le 1 ] || usage
case "$action" in install|remove) ;; *) usage ;; esac
root=$(git rev-parse --show-toplevel 2>/dev/null) || {
    echo "install-hooks: run this command inside a Git repository" >&2
    exit 1
}
cd "$root"

owned_path=.githooks
effective=$(git config --path --get core.hooksPath 2>/dev/null || true)
local_path=$(git config --local --path --get core.hooksPath 2>/dev/null || true)

if [ "$action" = remove ]; then
    if [ "$local_path" = "$owned_path" ]; then
        git config --local --unset core.hooksPath
        echo "Removed repository formatter hook setting."
    else
        echo "No repository formatter hook setting to remove."
    fi
    exit 0
fi

if [ "$effective" = "$owned_path" ] && [ "$local_path" = "$owned_path" ]; then
    echo "Repository formatter hook already installed."
    exit 0
fi
if [ -n "$effective" ]; then
    echo "install-hooks: refusing existing core.hooksPath: $effective" >&2
    exit 1
fi
default_hooks=$(git rev-parse --git-path hooks)
case "$default_hooks" in /*) ;; *) default_hooks=$root/$default_hooks ;; esac
for hook in "$default_hooks"/*; do
    [ -e "$hook" ] || continue
    case "$hook" in *.sample) continue ;; esac
    echo "install-hooks: refusing existing default hook: $hook" >&2
    exit 1
done
git config --local core.hooksPath "$owned_path"
echo "Installed repository formatter hook. Remove with: scripts/install-hooks.sh remove"
