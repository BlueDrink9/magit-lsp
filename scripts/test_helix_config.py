#!/usr/bin/env python3
"""Verify the documented Helix LSP configuration with real local `hx`.

The script extracts the fenced TOML block from `docs/client-helix-reference.md`,
runs an isolated real Helix session under tmux, and asserts server attachment,
server-rendered status, one native stage action and exact Git state. It keeps
text and structured evidence only and removes every owned process it started.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shlex
import shutil
import signal
import subprocess
import time
from pathlib import Path

from helix_smoke import documented_toml, executing
from ui_lib import fixtures
from ui_lib.process_identity import record_command, signal_owned

ROOT = Path(__file__).resolve().parents[1]
SESSION = "helix"
STAGE_TITLE = "Stage the change at point"
COMMIT_MENU_TITLE = "Open the commit menu"
COMMIT_TITLE = "Commit"
CONFIRM_TITLE = "Confirm commit"
ABORT_TITLE = "Abort commit"


def run(args: list[str], *, timeout: float = 15, **kwargs: object) -> subprocess.CompletedProcess[str]:
    """Run one bounded command and raise with captured output on failure."""
    try:
        result = subprocess.run(args, text=True, capture_output=True, timeout=timeout, **kwargs)
    except subprocess.TimeoutExpired as error:
        raise RuntimeError(f"timed out: {shlex.join(args)}") from error
    if result.returncode:
        raise RuntimeError(
            f"command failed ({result.returncode}): {shlex.join(args)}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def tmux(socket: str, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    """Run a tmux command against the owned server socket."""
    result = subprocess.run(["tmux", "-L", socket, *args], text=True, capture_output=True, timeout=5)
    if check and result.returncode:
        raise RuntimeError(f"tmux {' '.join(args)} failed: {result.stderr.strip()}")
    return result


def send_keys(socket: str, *keys: str) -> None:
    """Send named keys to the owned Helix pane."""
    tmux(socket, "send-keys", "-t", SESSION, *keys)


def send_text(socket: str, text: str) -> None:
    """Send literal text to the owned Helix pane."""
    tmux(socket, "send-keys", "-t", SESSION, "-l", text)


def capture(socket: str) -> str:
    """Read the current owned Helix pane as plain text."""
    return tmux(socket, "capture-pane", "-pt", SESSION).stdout


ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")


def selected_menu_title(socket: str) -> str | None:
    """Return the reverse-video menu row's text, or None when no menu is open."""
    screen = tmux(socket, "capture-pane", "-ep", "-t", SESSION).stdout
    for line in screen.splitlines():
        if "\x1b[7m" not in line:
            continue
        text = ANSI.sub("", line)
        text = re.sub(r"^\s*\d+\s{2,}", "", text).strip()
        text = text.rstrip("▐│|").rstrip()
        if text:
            return text
    return None


def matches_title(text: str | None, title: str) -> bool:
    """Match the exact offered title at the start of a reverse-video row.

    Helix can paint the popup over the buffer without clearing the rest of the
    row, so the captured row may continue with underlying buffer text. Accept
    the title only as a whole prefix followed by a boundary space, never as a
    loose substring or as a prefix of a longer title.
    """
    return text == title or (text is not None and text.startswith(title + " "))


def has_session(socket: str) -> bool:
    """Return whether the owned tmux session still exists."""
    return tmux(socket, "has-session", "-t", SESSION, check=False).returncode == 0


def choose_action(socket: str, title: str, label: str) -> str:
    """Wait for the native code-action picker and select the exact offered title."""
    selected = wait_for(
        lambda: selected_menu_title(socket),
        lambda value: value is not None,
        f"{label} code-action picker",
    )
    for _ in range(200):
        if matches_title(selected, title):
            break
        previous = selected
        send_keys(socket, "Down")
        selected = wait_for(
            lambda: selected_menu_title(socket),
            lambda value: value is not None and value != previous,
            "native menu selection movement",
            timeout=5,
        )
    else:
        raise RuntimeError(f"native code action menu never selected {title!r}: {selected!r}")
    screen = capture(socket)
    send_keys(socket, "Enter")
    return screen


def wait_for(read: object, predicate: object, label: str, timeout: float = 45) -> object:
    """Poll `read()` until `predicate` holds, with a bounded deadline."""
    deadline = time.monotonic() + timeout
    last: object = None
    while True:
        last = read()  # type: ignore[operator]
        if predicate(last):  # type: ignore[operator]
            return last
        if time.monotonic() >= deadline:
            raise RuntimeError(f"timed out waiting for {label}; last value: {last!r}")
        time.sleep(0.05)


def read_wire(path: Path) -> list[dict[str, object]]:
    """Read complete wirelog JSONL rows, ignoring a partial trailing line."""
    if not path.exists():
        return []
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(row, dict):
            rows.append(row)
    return rows


def raw(row: dict[str, object]) -> dict[str, object]:
    """Return the JSON-RPC message object recorded in one wirelog row."""
    value = row.get("raw")
    return value if isinstance(value, dict) else {}


def inbound(rows: list[dict[str, object]], method: str) -> list[dict[str, object]]:
    """Select client-to-server messages with `method`."""
    return [row for row in rows if row.get("direction") == "in" and raw(row).get("method") == method]


def outbound(rows: list[dict[str, object]], method: str) -> list[dict[str, object]]:
    """Select server-to-client messages with `method`."""
    return [row for row in rows if row.get("direction") == "out" and raw(row).get("method") == method]


def response_for(rows: list[dict[str, object]], request_id: object) -> dict[str, object] | None:
    """Return the server response row for a client request id."""
    for row in rows:
        value = raw(row)
        if row.get("direction") == "out" and "result" in value and value.get("id") == request_id:
            return value
        if row.get("direction") == "out" and "error" in value and value.get("id") == request_id:
            return value
    return None


def response_inbound(rows: list[dict[str, object]], request_id: object) -> dict[str, object] | None:
    """Return the client response row for a server request id."""
    for row in rows:
        value = raw(row)
        if row.get("direction") == "in" and "result" in value and value.get("id") == request_id:
            return value
    return None


def digest(path: Path) -> str:
    """Return the SHA-256 of a file's bytes."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git(repo: Path, env: dict[str, str], *args: str, input_text: str | None = None) -> str:
    """Run Git in the fixture and return stdout."""
    result = subprocess.run(
        ["git", *args], cwd=repo, env=env, capture_output=True, text=True,
        input=input_text, timeout=30,
    )
    if result.returncode:
        raise RuntimeError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout


def wire_summary(rows: list[dict[str, object]], magic_uri: str, target_line: int) -> dict[str, object]:
    """Assert and summarize attachment, offer, invocation and applied edit."""
    initializes = inbound(rows, "initialize")
    if not initializes:
        raise RuntimeError("no initialize request reached the server")
    initialize = raw(initializes[0]).get("params", {})
    client = initialize.get("clientInfo", {}) if isinstance(initialize, dict) else {}
    if client.get("name") != "helix":
        raise RuntimeError(f"initialize clientInfo is not helix: {client!r}")

    opened = [
        raw(row).get("params", {})
        for row in inbound(rows, "textDocument/didOpen")
    ]
    if not any(
        isinstance(params, dict)
        and str(params.get("textDocument", {}).get("uri", "")).endswith(".magit")
        for params in opened
    ):
        raise RuntimeError(f"no .magit didOpen reached the server: {opened!r}")

    requested = [
        row for row in inbound(rows, "textDocument/codeAction")
        if raw(row).get("params", {}).get("textDocument", {}).get("uri") == magic_uri
    ]
    if not requested:
        raise RuntimeError(f"no codeAction request for {magic_uri}")
    request = next(
        row for row in requested
        if raw(row).get("params", {}).get("range", {}).get("start", {}).get("line") == target_line
    )
    wanted = raw(request).get("id")
    offered = response_for(rows, wanted)
    if offered is None:
        raise RuntimeError(f"no codeAction response for request {wanted!r}")
    titles = [item.get("title") for item in offered.get("result", []) if isinstance(item, dict)]
    if STAGE_TITLE not in titles:
        raise RuntimeError(f"documented action {STAGE_TITLE!r} was not offered: {titles!r}")

    executed = [
        raw(row).get("params", {}) for row in inbound(rows, "workspace/executeCommand")
        if raw(row).get("params", {}).get("command") == "magit.command"
        and (raw(row).get("params", {}).get("arguments") or [{}])[0].get("command") == "status.stage"
    ]
    if not executed:
        raise RuntimeError("the stage action was offered but never invoked")

    edits = outbound(rows, "workspace/applyEdit")
    if not edits:
        raise RuntimeError("the staged document was never updated with workspace/applyEdit")
    edit_id = raw(edits[-1]).get("id")
    applied = response_inbound(rows, edit_id)
    if applied is None or applied.get("result", {}).get("applied") is not True:
        raise RuntimeError(f"applyEdit was not acknowledged as applied: {applied!r}")

    return {
        "initialize_client": client,
        "did_open_uri": magic_uri,
        "code_action_line": target_line,
        "offered_titles": titles,
        "executed_command": executed[0].get("arguments", [{}])[0].get("command"),
        "apply_edit_acknowledged": True,
        "rows": len(rows),
    }


def draft_wire_summary(rows: list[dict[str, object]], draft_uri: str) -> dict[str, object]:
    """Assert the owned draft attached as git-commit and offered its actions."""
    opened = [
        raw(row).get("params", {})
        for row in inbound(rows, "textDocument/didOpen")
        if raw(row).get("params", {}).get("textDocument", {}).get("uri") == draft_uri
    ]
    if not opened:
        raise RuntimeError(f"no didOpen for the owned draft {draft_uri}")
    languages = {item.get("textDocument", {}).get("languageId") for item in opened}
    if languages != {"git-commit"}:
        raise RuntimeError(f"draft didOpen languageId is not git-commit: {languages!r}")

    requested = [
        row for row in inbound(rows, "textDocument/codeAction")
        if raw(row).get("params", {}).get("textDocument", {}).get("uri") == draft_uri
    ]
    if not requested:
        raise RuntimeError(f"no codeAction request for the owned draft {draft_uri}")
    offered = response_for(rows, raw(requested[-1]).get("id"))
    if offered is None:
        raise RuntimeError("no codeAction response for the owned draft")
    titles = [item.get("title") for item in offered.get("result") or [] if isinstance(item, dict)]
    for title in (CONFIRM_TITLE, ABORT_TITLE):
        if title not in titles:
            raise RuntimeError(f"draft action {title!r} was not offered: {titles!r}")

    aborted = [
        (row, (raw(row).get("params", {}).get("arguments") or [{}])[0])
        for row in inbound(rows, "workspace/executeCommand")
        if raw(row).get("params", {}).get("command") == "magit.command"
        and (raw(row).get("params", {}).get("arguments") or [{}])[0].get("command") == "common.cancel"
        and (raw(row).get("params", {}).get("arguments") or [{}])[0].get("uri") == draft_uri
    ]
    if not aborted:
        raise RuntimeError("the draft Abort commit action was never invoked")
    response = response_for(rows, raw(aborted[-1][0]).get("id"))
    if response is None or "error" in response:
        raise RuntimeError(f"draft abort did not succeed: {response!r}")

    return {
        "did_open_uri": draft_uri,
        "language_ids": sorted(languages),
        "offered_titles": titles,
        "aborted": True,
    }


def cleanup(
    socket: str,
    socket_path: Path | None,
    pids: dict[str, int | None],
    records: dict[str, Path],
    directory: Path,
) -> dict[str, object]:
    """Stop owned Helix, its server and tmux, then report residual state."""
    if has_session(socket):
        try:
            send_keys(socket, "Escape")
            send_text(socket, ":lsp-stop")
            send_keys(socket, "Enter")
        except (RuntimeError, subprocess.TimeoutExpired):
            pass
    server = pids.get("server")
    if server is not None:
        deadline = time.monotonic() + 5
        while executing(server) and time.monotonic() < deadline:
            time.sleep(0.05)
    if has_session(socket):
        try:
            send_keys(socket, "Escape")
            send_text(socket, ":q!")
            send_keys(socket, "Enter")
        except (RuntimeError, subprocess.TimeoutExpired):
            pass
        deadline = time.monotonic() + 5
        while has_session(socket) and time.monotonic() < deadline:
            time.sleep(0.05)
    tmux(socket, "kill-server", check=False)
    for role in ("server", "editor"):
        pid = pids.get(role)
        if pid is None:
            continue
        for sig in (signal.SIGTERM, signal.SIGKILL):
            if not executing(pid):
                break
            try:
                if not signal_owned(pid, records[role], sig):
                    break
            except ProcessLookupError:
                break
            deadline = time.monotonic() + 2
            while executing(pid) and time.monotonic() < deadline:
                time.sleep(0.02)
    if socket_path is not None:
        socket_path.unlink(missing_ok=True)
    report = {
        "editor_pid": pids.get("editor"),
        "server_pid": pids.get("server"),
        "editor_executing": executing(pids["editor"]) if pids.get("editor") else False,
        "server_executing": executing(pids["server"]) if pids.get("server") else False,
        "editor_proc_entry": Path(f"/proc/{pids['editor']}").exists() if pids.get("editor") else False,
        "server_proc_entry": Path(f"/proc/{pids['server']}").exists() if pids.get("server") else False,
        "tmux_session": has_session(socket),
        "tmux_socket_exists": socket_path.exists() if socket_path is not None else False,
    }
    (directory / "cleanup.json").write_text(json.dumps(report, indent=2) + "\n")
    residuals = [
        name for name, value in (
            ("editor still executing", report["editor_executing"]),
            ("server still executing", report["server_executing"]),
            ("editor process entry remains", report["editor_proc_entry"]),
            ("server process entry remains", report["server_proc_entry"]),
            ("tmux session remains", report["tmux_session"]),
            ("tmux socket remains", report["tmux_socket_exists"]),
        ) if value
    ]
    if residuals:
        raise RuntimeError("owned cleanup left residuals: " + ", ".join(residuals))
    return report


def main() -> None:
    """Run the documented Helix configuration check end to end."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--helix", default="hx")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/magit-lsp")
    parser.add_argument("--evidence", type=Path, default=ROOT / "build/ui/helix-config-check")
    args = parser.parse_args()

    for tool in ("tmux", "git"):
        if not shutil.which(tool):
            raise SystemExit(f"helix-config-check: required executable not found: {tool}")
    helix = shutil.which(args.helix)
    if not helix:
        raise SystemExit(
            f"helix-config-check: required executable not found: {args.helix} "
            "(install Helix or pass HELIX_BIN)"
        )
    binary = args.binary.resolve()
    if not binary.is_file():
        raise SystemExit(f"helix-config-check: server binary missing: {binary}")

    directory = args.evidence.resolve()
    if directory.exists():
        directory.rename(directory.with_name(f"{directory.name}-{time.time_ns()}"))
    directory.mkdir(parents=True)

    documented = documented_toml()
    repo = directory / "repo"
    home = directory / "home"
    fixtures.build("modified-two-files", repo, home)
    (repo / ".git/info/exclude").write_text("/.magit\n")
    (repo / ".magit").write_text("")
    config = home / "config/helix"
    config.mkdir(parents=True)
    languages = config / "languages.toml"
    languages.write_text(documented)

    tools = directory / "bin"
    tools.mkdir()
    records = {"editor": directory / "editor.pid", "server": directory / "server.pid"}
    server = tools / "magit-lsp"
    server.write_text(
        "#!/bin/sh\nset -e\n"
        + record_command(records["server"])
        + f"exec {shlex.quote(str(binary))}\n"
    )
    server.chmod(0o700)
    editor = tools / "hx"
    editor.write_text(
        "#!/bin/sh\nset -e\n"
        + record_command(records["editor"])
        + f"exec {shlex.quote(helix)} -vv --log {shlex.quote(str(directory / 'helix.log'))} .magit\n"
    )
    editor.chmod(0o700)

    env = fixtures.determinism_env(home)
    env.update({
        "PATH": f"{tools}:{os.environ['PATH']}",
        "TERM": "xterm-256color",
        "XDG_CACHE_HOME": str(home / "cache"),
        "XDG_DATA_HOME": str(home / "data"),
        "XDG_STATE_HOME": str(home / "state"),
        "MAGIT_LSP_WIRELOG": str(directory / "wire.jsonl"),
    })
    launch = directory / "launch.sh"
    launch.write_text("#!/bin/sh\nexec " + shlex.join([
        "env", "-i", *(f"{key}={value}" for key, value in sorted(env.items())),
        str(editor),
    ]) + "\n")
    launch.chmod(0o700)

    socket = f"helix-config-check-{os.getpid()}"
    socket_path: Path | None = None
    pids: dict[str, int | None] = {"editor": None, "server": None}
    report: dict[str, object] = {
        "helix": run([helix, "--version"]).stdout.strip(),
        "documented_toml_sha256": hashlib.sha256(documented.encode()).hexdigest(),
        "documented_toml_lines": len(documented.splitlines()),
        "binary": str(binary),
    }
    failure: BaseException | None = None
    try:
        tmux(socket, "new-session", "-d", "-s", SESSION, "-x", "120", "-y", "40", "-c", str(repo), str(launch))
        socket_path = Path(tmux(socket, "display-message", "-p", "#{socket_path}").stdout.strip())
        tmux(socket, "set", "-g", "status", "off")
        tmux(socket, "set", "-g", "escape-time", "0")
        tmux(socket, "set", "-g", "default-terminal", "xterm-256color")

        def identities() -> dict[str, int | None]:
            for role, path in records.items():
                if pids[role] is None and path.exists():
                    pids[role] = int(path.read_text().strip())
            return pids

        wait_for(identities, lambda value: value.get("server") is not None, "owned server pid")
        wait_for(identities, lambda value: value.get("editor") is not None, "owned editor pid")

        before = {
            "head": git(repo, env, "rev-parse", "HEAD").strip(),
            "index_f": git(repo, env, "rev-parse", ":f.txt").strip(),
            "index_g": git(repo, env, "rev-parse", ":g.txt").strip(),
            "worktree_f": digest(repo / "f.txt"),
            "worktree_g": digest(repo / "g.txt"),
        }
        report["git_before"] = before

        def rendered(screen: str) -> bool:
            return (
                "Head: main" in screen
                and "Unstaged changes (2)" in screen
                and "f.txt" in screen
                and "g.txt" in screen
            )

        status = wait_for(lambda: capture(socket), rendered, "server-rendered status")
        (directory / "status.screen").write_text(status)
        lines = status.splitlines()
        target_line = next(
            index for index, line in enumerate(lines)
            if "M f.txt" in line and "f.txt" in line
        )

        send_keys(socket, "Escape")
        send_text(socket, "gg")
        send_keys(socket, *(["j"] * target_line))
        send_keys(socket, "Space", "a")
        picker = wait_for(lambda: capture(socket), lambda screen: STAGE_TITLE in screen, "stage code action offer")
        (directory / "code-action.screen").write_text(picker)
        selected = selected_menu_title(socket)
        for _ in range(len(picker.splitlines())):
            if matches_title(selected, STAGE_TITLE):
                break
            previous = selected
            send_keys(socket, "Down")
            selected = wait_for(
                lambda: selected_menu_title(socket),
                lambda title: title is not None and title != previous,
                "native menu selection movement",
                timeout=5,
            )
        else:
            raise RuntimeError(f"native code action menu never selected {STAGE_TITLE!r}: {selected!r}")
        (directory / "code-action-selected.screen").write_text(picker + f"\nselected: {selected}\n")
        send_keys(socket, "Enter")
        staged = wait_for(
            lambda: capture(socket),
            lambda screen: "Staged changes (1)" in screen and "Unstaged changes (1)" in screen,
            "server-rendered staged status",
        )
        (directory / "staged.screen").write_text(staged)

        rows = wait_for(
            lambda: read_wire(directory / "wire.jsonl"),
            lambda entries: any(raw(row).get("method") == "workspace/applyEdit" for row in outbound(entries, "workspace/applyEdit")),
            "applied workspace edit",
        )
        magic_uri = next(
            str(raw(row).get("params", {}).get("textDocument", {}).get("uri"))
            for row in inbound(rows, "textDocument/didOpen")
            if str(raw(row).get("params", {}).get("textDocument", {}).get("uri", "")).endswith(".magit")
        )
        report["wire"] = wire_summary(rows, magic_uri, target_line)

        head_after = git(repo, env, "rev-parse", "HEAD").strip()
        staged_paths = git(repo, env, "diff", "--cached", "--name-only").splitlines()
        expected_blob = git(repo, env, "hash-object", "--stdin", input_text=fixtures.STANDARD_CHANGES).strip()
        git_state = {
            "head_after": head_after,
            "staged_paths": staged_paths,
            "index_f_after": git(repo, env, "rev-parse", ":f.txt").strip(),
            "index_g_after": git(repo, env, "rev-parse", ":g.txt").strip(),
            "worktree_f_after": digest(repo / "f.txt"),
            "worktree_g_after": digest(repo / "g.txt"),
            "expected_staged_blob": expected_blob,
        }
        report["git_after"] = git_state
        assert head_after == before["head"], (head_after, before["head"])
        assert staged_paths == ["f.txt"], staged_paths
        assert git_state["index_f_after"] == expected_blob, (git_state["index_f_after"], expected_blob)
        assert git_state["index_g_after"] == before["index_g"], (git_state["index_g_after"], before["index_g"])
        assert git_state["worktree_f_after"] == before["worktree_f"], "f.txt worktree changed"
        assert git_state["worktree_g_after"] == before["worktree_g"], "g.txt worktree changed"

        status_row = next(index for index, line in enumerate(staged.splitlines()) if "f.txt" in line)
        send_keys(socket, "Escape")
        send_text(socket, "gg")
        send_keys(socket, *(["j"] * status_row))
        send_keys(socket, "Space", "a")
        (directory / "commit-menu-action.screen").write_text(
            choose_action(socket, COMMIT_MENU_TITLE, "open commit menu")
        )
        menu = wait_for(
            lambda: capture(socket),
            lambda screen: "git commit" in screen,
            "server-rendered commit menu",
        )
        (directory / "commit-menu.screen").write_text(menu)
        commit_row = next(
            index for index, line in enumerate(menu.splitlines())
            if re.search(r"(?:^|\s)Commit\s*$", line)
        )
        send_keys(socket, "Escape")
        send_text(socket, "gg")
        send_keys(socket, *(["j"] * commit_row))
        send_keys(socket, "Space", "a")
        (directory / "commit-action.screen").write_text(
            choose_action(socket, COMMIT_TITLE, "create commit")
        )
        issued = wait_for(
            lambda: capture(socket),
            lambda screen: ".git/COMMIT_EDITMSG" in screen,
            "issued commit draft",
        )
        (directory / "commit-issued.screen").write_text(issued)
        send_keys(socket, "Escape")
        send_text(socket, f":open {repo / '.git/COMMIT_EDITMSG'}")
        send_keys(socket, "Enter")
        draft_screen = wait_for(
            lambda: capture(socket),
            lambda screen: "COMMIT_EDITMSG" in screen and "Please enter the commit message" in screen,
            "opened commit draft",
        )
        (directory / "commit-draft.screen").write_text(draft_screen)
        draft_rows = wait_for(
            lambda: read_wire(directory / "wire.jsonl"),
            lambda entries: any(
                str(raw(row).get("params", {}).get("textDocument", {}).get("uri", "")).endswith(
                    "/.git/COMMIT_EDITMSG"
                )
                for row in inbound(entries, "textDocument/didOpen")
            ),
            "owned commit draft didOpen",
        )
        draft_uri = next(
            str(raw(row).get("params", {}).get("textDocument", {}).get("uri"))
            for row in inbound(draft_rows, "textDocument/didOpen")
            if str(raw(row).get("params", {}).get("textDocument", {}).get("uri", "")).endswith(
                "/.git/COMMIT_EDITMSG"
            )
        )
        send_keys(socket, "Space", "a")
        (directory / "draft-action.screen").write_text(
            choose_action(socket, ABORT_TITLE, "abort commit")
        )

        def abort_answered(entries: list[dict[str, object]]) -> bool:
            for row in inbound(entries, "workspace/executeCommand"):
                args = (raw(row).get("params", {}).get("arguments") or [{}])[0]
                if args.get("command") != "common.cancel" or args.get("uri") != draft_uri:
                    continue
                response = response_for(entries, raw(row).get("id"))
                if response is not None and "error" not in response:
                    return True
            return False

        rows = wait_for(
            lambda: read_wire(directory / "wire.jsonl"),
            abort_answered,
            "draft abort result",
        )
        report["draft_wire"] = draft_wire_summary(rows, draft_uri)

        send_keys(socket, "Escape")
        send_text(socket, f":open {repo / '.magit'}")
        send_keys(socket, "Enter")
        retired = wait_for(
            lambda: capture(socket),
            lambda screen: "Staged changes (1)" in screen and "COMMIT_EDITMSG" not in screen,
            "draft abort retirement",
        )
        (directory / "commit-aborted.screen").write_text(retired)

        after_abort = {
            "head": git(repo, env, "rev-parse", "HEAD").strip(),
            "staged_paths": git(repo, env, "diff", "--cached", "--name-only").splitlines(),
            "index_f": git(repo, env, "rev-parse", ":f.txt").strip(),
            "index_g": git(repo, env, "rev-parse", ":g.txt").strip(),
            "worktree_f": digest(repo / "f.txt"),
            "worktree_g": digest(repo / "g.txt"),
        }
        report["git_after_abort"] = after_abort
        assert after_abort["head"] == before["head"], after_abort
        assert after_abort["staged_paths"] == ["f.txt"], after_abort
        assert after_abort["index_f"] == expected_blob, after_abort
        assert after_abort["index_g"] == before["index_g"], after_abort
        assert after_abort["worktree_f"] == before["worktree_f"], "f.txt worktree changed"
        assert after_abort["worktree_g"] == before["worktree_g"], "g.txt worktree changed"
    except BaseException as error:
        failure = error
        try:
            (directory / "failure.screen").write_text(capture(socket))
        except (RuntimeError, subprocess.TimeoutExpired):
            pass
    finally:
        try:
            cleanup_report = cleanup(socket, socket_path, pids, records, directory)
            report["cleanup"] = cleanup_report
        except BaseException as error:
            report["cleanup_error"] = str(error)
            failure = failure or error
        (directory / "report.json").write_text(json.dumps(report, indent=2) + "\n")

    if failure is not None:
        raise failure
    print(
        f"{report['helix']}; documented TOML {report['documented_toml_lines']} lines; "
        f"staged f.txt as {report['git_after']['index_f_after']}; "
        f"draft didOpen {report['draft_wire']['language_ids']} with "
        f"{report['draft_wire']['offered_titles']}; "
        f"HEAD {report['git_after_abort']['head']} unchanged; cleanup clean; "
        f"evidence: {directory}"
    )


if __name__ == "__main__":
    main()
