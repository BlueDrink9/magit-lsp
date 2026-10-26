"""Deterministic Git repositories for the real-UI tests.

The environment mirrors `tests/common/mod.rs`: a throwaway `HOME` and
`XDG_CONFIG_HOME`, no system config, `LC_ALL=C`, a fixed identity and fixed
author/committer dates. Repositories built by the same script therefore have
the same commit hashes on every machine and every run.
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

FIXED_NAME = "Test User"
FIXED_EMAIL = "test@example.com"
FIXED_DATE = "2000-01-01T00:00:00+0000"

TEN_LINES = "".join(f"line{number}\n" for number in range(1, 11))

STANDARD_CHANGES = "".join(
    f"{line}\n"
    for line in (
        "line1",
        "line2",
        "line3 CHANGED",
        "line4",
        "line5",
        "NEW line",
        "line6",
        "line7",
        "line8",
        "line10",
    )
)

SECOND_FILE_CHANGES = "".join(
    f"{line}\n"
    for line in (
        "line1",
        "line2 CHANGED",
        "line3",
        "line4",
        "line5",
        "line6",
        "line7",
        "line8",
        "line9",
        "line10",
    )
)

FIXTURES = (
    "clean",
    "modified-two-files",
    "staged-one-file",
    "staged-and-unstaged",
    "staged-failing-hook",
    "staged-slow-hook",
    "untracked",
    "unborn",
    "in-progress-rebase",
    "in-progress-cherry-pick",
    "log-history",
)


def determinism_env(home: Path) -> dict[str, str]:
    """Return a git environment with fixed identity and isolated config."""
    env = {
        key: value
        for key, value in os.environ.items()
        if key
        not in (
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_PREFIX",
            "GIT_CONFIG_COUNT",
        )
    }
    (home / "config").mkdir(parents=True, exist_ok=True)
    env.update(
        {
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(home / "config"),
            "GIT_CONFIG_NOSYSTEM": "1",
            "LC_ALL": "C",
            "LANG": "C",
            "TZ": "UTC",
            "GIT_AUTHOR_NAME": FIXED_NAME,
            "GIT_AUTHOR_EMAIL": FIXED_EMAIL,
            "GIT_COMMITTER_NAME": FIXED_NAME,
            "GIT_COMMITTER_EMAIL": FIXED_EMAIL,
            "GIT_AUTHOR_DATE": FIXED_DATE,
            "GIT_COMMITTER_DATE": FIXED_DATE,
            "GIT_EDITOR": "true",
            "GIT_SEQUENCE_EDITOR": "true",
            "GIT_MERGE_AUTOEDIT": "no",
            "GIT_TERMINAL_PROMPT": "0",
        }
    )
    return env


def git(
    repo: Path,
    env: dict[str, str],
    *args: str,
    check: bool = True,
) -> subprocess.CompletedProcess:
    """Run git in `repo`; raise with stderr when `check` is true."""
    result = subprocess.run(
        ["git", *args],
        cwd=repo,
        env=env,
        capture_output=True,
        text=True,
    )
    if check and result.returncode != 0:
        raise RuntimeError(
            f"git {' '.join(args)} failed in {repo}: {result.stderr.strip()}"
        )
    return result


def write(path: Path, content: str) -> None:
    """Write `content`, creating parent directories."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def commit(repo: Path, env: dict[str, str], message: str, files: dict[str, str]) -> str:
    """Commit `files` and return the new HEAD hash."""
    for name, content in files.items():
        write(repo / name, content)
    git(repo, env, "add", "-A")
    git(repo, env, "commit", "-q", "-m", message)
    return git(repo, env, "rev-parse", "HEAD").stdout.strip()


def _init(repo: Path, env: dict[str, str]) -> None:
    git(repo, env, "init", "-q", "-b", "main")


def _clean(repo: Path, env: dict[str, str]) -> None:
    commit(repo, env, "initial", {"f.txt": TEN_LINES})


def _modified_two_files(repo: Path, env: dict[str, str]) -> None:
    commit(repo, env, "initial", {"f.txt": TEN_LINES, "g.txt": TEN_LINES})
    write(repo / "f.txt", STANDARD_CHANGES)
    write(repo / "g.txt", SECOND_FILE_CHANGES)


def _staged_one_file(repo: Path, env: dict[str, str]) -> None:
    commit(repo, env, "initial", {"f.txt": TEN_LINES})
    write(repo / "f.txt", STANDARD_CHANGES)
    git(repo, env, "add", "f.txt")


def _staged_and_unstaged(repo: Path, env: dict[str, str]) -> None:
    commit(repo, env, "initial", {"f.txt": TEN_LINES, "g.txt": TEN_LINES})
    write(repo / "f.txt", STANDARD_CHANGES)
    git(repo, env, "add", "f.txt")
    write(repo / "g.txt", SECOND_FILE_CHANGES)


def _staged_failing_hook(repo: Path, env: dict[str, str]) -> None:
    """A staged change whose `pre-commit` hook always rejects the commit.

    Git runs the hook before it opens the editor, so the commit fails without
    an `editDocument` effect: the client must still get an error message.
    """
    commit(repo, env, "initial", {"f.txt": TEN_LINES})
    write(repo / "f.txt", STANDARD_CHANGES)
    git(repo, env, "add", "f.txt")
    hook = repo / ".git" / "hooks" / "pre-commit"
    write(hook, "#!/bin/sh\necho 'pre-commit hook failed: commit blocked' >&2\nexit 1\n")
    hook.chmod(0o755)


def _staged_slow_hook(repo: Path, env: dict[str, str]) -> None:
    """A staged change whose `pre-commit` hook is slow, then rejects the commit.

    The delay keeps the commit outstanding long enough for `ui-stays-responsive`
    to press keys while it runs. The failure means Git never opens an editor,
    so the commit ends by itself and the client becomes idle again.
    """
    commit(repo, env, "initial", {"f.txt": TEN_LINES})
    write(repo / "f.txt", STANDARD_CHANGES)
    git(repo, env, "add", "f.txt")
    hook = repo / ".git" / "hooks" / "pre-commit"
    write(
        hook,
        "#!/bin/sh\nsleep 1\necho 'slow pre-commit hook: commit blocked' >&2\nexit 1\n",
    )
    hook.chmod(0o755)


def _untracked(repo: Path, env: dict[str, str]) -> None:
    commit(repo, env, "initial", {"f.txt": TEN_LINES})
    write(repo / "new.txt", "brand new\n")


def _unborn(repo: Path, env: dict[str, str]) -> None:
    write(repo / "f.txt", TEN_LINES)
    git(repo, env, "add", "f.txt")


def _in_progress_rebase(repo: Path, env: dict[str, str]) -> None:
    commit(repo, env, "initial", {"f.txt": "one\ntwo\nthree\n"})
    git(repo, env, "checkout", "-q", "-b", "topic")
    commit(repo, env, "topic change", {"f.txt": "one\ntwo topic\nthree\n"})
    git(repo, env, "checkout", "-q", "main")
    commit(repo, env, "main change", {"f.txt": "one\ntwo main\nthree\n"})
    git(repo, env, "checkout", "-q", "topic")
    result = git(repo, env, "rebase", "main", check=False)
    if result.returncode == 0:
        raise RuntimeError("rebase fixture did not conflict")
    if not (repo / ".git" / "rebase-merge").is_dir():
        raise RuntimeError("rebase fixture left no rebase state")


def _in_progress_cherry_pick(repo: Path, env: dict[str, str]) -> None:
    commit(repo, env, "initial", {"f.txt": "one\ntwo\nthree\n"})
    git(repo, env, "checkout", "-q", "-b", "side")
    picked = commit(repo, env, "side change", {"f.txt": "one\ntwo side\nthree\n"})
    git(repo, env, "checkout", "-q", "main")
    commit(repo, env, "main change", {"f.txt": "one\ntwo main\nthree\n"})
    result = git(repo, env, "cherry-pick", picked, check=False)
    if result.returncode == 0:
        raise RuntimeError("cherry-pick fixture did not conflict")
    if not (repo / ".git" / "CHERRY_PICK_HEAD").is_file():
        raise RuntimeError("cherry-pick fixture left no cherry-pick state")


def _log_history(repo: Path, env: dict[str, str]) -> None:
    """Create a decorated three-commit history for log UI scenarios."""
    commit(repo, env, "old searchable subject", {"f.txt": "old\n"})
    commit(repo, env, "middle graph subject", {"f.txt": "middle\n"})
    commit(repo, env, "latest decorated subject", {"f.txt": "latest\n"})
    git(repo, env, "branch", "topic")
    git(repo, env, "tag", "v1", "HEAD~1")


_BUILDERS = {
    "clean": _clean,
    "modified-two-files": _modified_two_files,
    "staged-one-file": _staged_one_file,
    "staged-and-unstaged": _staged_and_unstaged,
    "staged-failing-hook": _staged_failing_hook,
    "staged-slow-hook": _staged_slow_hook,
    "untracked": _untracked,
    "unborn": _unborn,
    "in-progress-rebase": _in_progress_rebase,
    "in-progress-cherry-pick": _in_progress_cherry_pick,
    "log-history": _log_history,
}


def build(name: str, dest: Path, home: Path | None = None) -> Path:
    """Create fixture `name` in the empty directory `dest`.

    `home` is the throwaway HOME used for git; it defaults to a sibling of
    `dest`. Returns `dest`.
    """
    builder = _BUILDERS.get(name)
    if builder is None:
        known = ", ".join(FIXTURES)
        raise ValueError(f"unknown fixture `{name}`; expected one of {known}")
    dest = Path(dest)
    if dest.exists() and any(dest.iterdir()):
        raise ValueError(f"fixture destination is not empty: {dest}")
    dest.mkdir(parents=True, exist_ok=True)
    home = Path(home) if home is not None else dest.parent / "home"
    env = determinism_env(home)
    _init(dest, env)
    builder(dest, env)
    return dest
