#!/usr/bin/env python3
"""Focused plugin-free native Neovim smoke for standard Plain rendering."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "build/ui/plain-render"


def run(args: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(args, text=True, capture_output=True, check=False, **kwargs)
    if result.returncode:
        raise RuntimeError(
            f"command failed ({result.returncode}): {args!r}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def main() -> None:
    for executable in ("nvim", "tmux", "git"):
        if not shutil.which(executable):
            raise RuntimeError(f"required executable not found: {executable}")
    binary = ROOT / "target/release/magit-lsp"
    if not binary.exists():
        raise RuntimeError(f"release binary missing: {binary}")
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    name = "plain-" + uuid.uuid4().hex[:10]
    socket = f"/tmp/{name}.sock"
    server_id = name
    report: dict[str, object] = {"tmux_server": server_id, "nvim_socket": socket}
    failure: BaseException | None = None

    def rpc(expr: str) -> str:
        return run(["nvim", "--server", socket, "--remote-expr", expr], timeout=8).stdout.strip()

    def state() -> dict[str, object]:
        lua = (
            "(function() "
            "local b=vim.api.nvim_get_current_buf(); "
            "local c=vim.lsp.get_clients(); local names={}; "
            "for _,x in ipairs(c) do names[#names+1]=x.name end; "
            "return vim.json.encode({text=table.concat(vim.api.nvim_buf_get_lines(b,0,-1,false),'\\n'), "
            "name=vim.api.nvim_buf_get_name(b), modified=vim.bo[b].modified, clients=names, "
            "rtp=vim.o.runtimepath, version=vim.lsp.util.buf_versions[b], "
            "project_modules=(function() local m={}; for k in pairs(package.loaded) do "
            "if tostring(k):match('magit%-lsp') then m[#m+1]=k end end; return m end)(), "
            "rich_notifications=vim.g.plain_smoke_rich_notifications}) end)()"
        )
        expr = "luaeval(" + json.dumps(lua) + ")"
        return json.loads(rpc(expr))

    try:
        with tempfile.TemporaryDirectory(prefix="plain-render-") as temporary:
            temp = Path(temporary)
            repo = temp / "repo"
            repo.mkdir()
            home = temp / "home"
            home.mkdir()
            env = os.environ.copy()
            env.update({"HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config"), "GIT_CONFIG_NOSYSTEM": "1", "GIT_TERMINAL_PROMPT": "0", "PLAIN_REPO": str(repo), "PLAIN_SERVER": str(binary)})
            run(["git", "init", "-q"], cwd=repo, env=env)
            run(["git", "config", "user.name", "Plain Smoke"], cwd=repo, env=env)
            run(["git", "config", "user.email", "plain@example.invalid"], cwd=repo, env=env)
            (repo / "tracked.txt").write_text("base\n")
            run(["git", "add", "tracked.txt"], cwd=repo, env=env)
            run(["git", "commit", "-qm", "initial"], cwd=repo, env=env)
            presentation = repo / ".magit"
            disk_before = b"stale presentation sentinel\n"
            presentation.write_bytes(disk_before)
            config = ROOT / "tests/ui/plain_init.lua"
            run(["tmux", "-L", server_id, "new-session", "-d", "-s", "plain", "nvim", "--listen", socket, "-u", str(config), str(presentation)], cwd=repo, env=env)
            deadline = time.monotonic() + 30
            rpc_error = ""
            while True:
                try:
                    initial = state()
                    if "magit-lsp-plain-smoke" in initial["clients"] and "stale presentation sentinel" not in initial["text"] and "Head:" in initial["text"]:
                        break
                except (RuntimeError, json.JSONDecodeError) as error:
                    rpc_error = str(error)
                if time.monotonic() >= deadline:
                    diagnostics: dict[str, object] = {"last_state": locals().get("initial", {}), "rpc_error": rpc_error}
                    diagnostics["tmux"] = subprocess.run(["tmux", "-L", server_id, "capture-pane", "-pt", "plain"], capture_output=True, text=True).stdout
                    (EVIDENCE / "failure.json").write_text(json.dumps(diagnostics, indent=2) + "\n")
                    raise RuntimeError(f"timed out waiting for Plain didOpen rendering; diagnostics: {diagnostics}")
            report["bootstrap"] = initial
            report["plugin_free"] = not any("magit-lsp" in entry for entry in initial["rtp"].split(","))
            report["project_lua_modules"] = initial["project_modules"]
            if not report["plugin_free"]:
                raise AssertionError("magit-lsp runtimepath unexpectedly loaded")
            if initial["project_modules"]:
                raise AssertionError(f"magit-lsp Lua modules loaded: {initial['project_modules']!r}")

            (repo / "watcher-only.txt").write_text("watcher bytes\n")
            deadline = time.monotonic() + 30
            while "watcher-only.txt" not in state()["text"]:
                if time.monotonic() >= deadline:
                    raise RuntimeError("timed out waiting for watcher-only render")
            watched = state()
            report["watcher"] = watched

            # Native insert followed by native undo creates an actual divergent
            # didChange sequence while leaving the buffer at the user snapshot.
            before_edit = state()
            run(["nvim", "--server", socket, "--remote-send", "gg0iNATIVE-DIVERGENCE<Esc>"], timeout=8)
            deadline = time.monotonic() + 30
            while True:
                typed = state()
                if typed["version"] is not None and typed["version"] > before_edit["version"] and "NATIVE-DIVERGENCE" in typed["text"]:
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"timed out waiting for native typing/didChange: {typed!r}")
            run(["nvim", "--server", socket, "--remote-send", "u"], timeout=8)
            deadline = time.monotonic() + 30
            while True:
                edited = state()
                if edited["version"] is not None and edited["version"] > typed["version"] and edited["text"] == watched["text"]:
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"timed out waiting for authoritative repair after native undo: {edited!r}")
            report["native_edit_undo_repair"] = {"before": before_edit, "typed": typed, "after_undo": edited}
            report["presentation_disk_unchanged"] = presentation.read_bytes() == disk_before
            report["presentation_disk_bytes"] = presentation.read_bytes().decode()
            report["custom_notifications"] = edited["rich_notifications"]
            if not report["presentation_disk_unchanged"]:
                raise AssertionError("server changed presentation file on disk")
            if edited["rich_notifications"] != 0:
                raise AssertionError("custom view/render notification received")
    except BaseException as error:
        failure = error
    finally:
        killed = subprocess.run(["tmux", "-L", server_id, "kill-server"], capture_output=True, text=True)
        # Neovim owns its stdio LSP child; killing the owned tmux server stops
        # the editor and that child together.
        remaining_server = subprocess.run(["pgrep", "-fx", str(binary)], capture_output=True, text=True)
        report["cleanup"] = {
            "tmux_kill_returncode": killed.returncode,
            "tmux_sessions_remain": subprocess.run(["tmux", "-L", server_id, "list-sessions"], capture_output=True, text=True).returncode == 0,
            "nvim_socket_remains": os.path.exists(socket),
            "server_remaining": remaining_server.stdout.strip(),
        }
        if os.path.exists(socket):
            os.unlink(socket)
        report["cleanup"]["cleanup_verified"] = not report["cleanup"]["tmux_sessions_remain"] and not os.path.exists(socket) and not report["cleanup"]["server_remaining"]
        if "native_edit_undo_repair" in report:
            final = report["native_edit_undo_repair"]["after_undo"]
            (EVIDENCE / "native.screen").write_text(str(final["text"]) + "\n")
        (EVIDENCE / ("failure.json" if failure else "native.json")).write_text(json.dumps({**report, "error": str(failure) if failure else None}, indent=2, sort_keys=True) + "\n")
    if failure:
        raise failure
    if not report["cleanup"]["cleanup_verified"]:
        raise RuntimeError(f"owned process cleanup failed: {report['cleanup']!r}")


if __name__ == "__main__":
    main()
