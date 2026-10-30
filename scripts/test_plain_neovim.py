#!/usr/bin/env python3
"""Plugin-free native Neovim smoke for standard Plain rendering and actions."""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import tempfile
import time
import traceback
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


def run_variant(final_newline: bool) -> None:
    for executable in ("nvim", "tmux", "git"):
        if not shutil.which(executable):
            raise RuntimeError(f"required executable not found: {executable}")
    binary = ROOT / "target/release/magit-lsp"
    if not binary.exists():
        raise RuntimeError(f"release binary missing: {binary}")
    name = "plain-" + uuid.uuid4().hex[:10]
    run_evidence = EVIDENCE / ("with-final-newline" if final_newline else "without-final-newline")
    run_evidence.mkdir(parents=True, exist_ok=True)
    # A previous run must not leave success/failure artifacts behind.
    for artifact in ("native.json", "failure.json"):
        (run_evidence / artifact).unlink(missing_ok=True)
    trace_path = run_evidence / "state/nvim/lsp.log"
    # Neovim appends to its shared state log; isolate this run's trace before launch.
    trace_path.parent.mkdir(parents=True, exist_ok=True)
    trace_path.unlink(missing_ok=True)
    socket = f"/tmp/{name}.sock"
    server_id = name
    report: dict[str, object] = {"tmux_server": server_id, "nvim_socket": socket, "final_newline": final_newline}
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
            "endofline=vim.bo[b].endofline, fixeol=vim.bo[b].fixeol, fileformat=vim.bo[b].fileformat, "
            "rtp=vim.o.runtimepath, version=vim.lsp.util.buf_versions[b], pid=vim.fn.getpid(), "
            "server_pid=c[1] and c[1].rpc.pid, "
            "capabilities=c[1] and c[1].config.capabilities, server_capabilities=c[1] and c[1].server_capabilities, "
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
            env.update({"HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config"), "XDG_STATE_HOME": str(run_evidence / "state"), "GIT_CONFIG_NOSYSTEM": "1", "GIT_TERMINAL_PROMPT": "0", "PLAIN_REPO": str(repo), "PLAIN_SERVER": str(binary)})
            run(["git", "init", "-q"], cwd=repo, env=env)
            run(["git", "config", "user.name", "Plain Smoke"], cwd=repo, env=env)
            run(["git", "config", "user.email", "plain@example.invalid"], cwd=repo, env=env)
            (repo / "tracked.txt").write_text("base\n")
            run(["git", "add", "tracked.txt"], cwd=repo, env=env)
            run(["git", "commit", "-qm", "initial"], cwd=repo, env=env)
            presentation = repo / ".magit"
            disk_before = b"stale presentation sentinel\n" if final_newline else b"stale presentation sentinel"
            presentation.write_bytes(disk_before)
            config = ROOT / "tests/ui/plain_init.lua"
            run(["tmux", "-L", server_id, "new-session", "-d", "-x", "120", "-y", "100", "-s", "plain", "nvim", "--listen", socket, "-u", str(config), str(presentation)], cwd=repo, env=env)
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
                    (run_evidence / "failure.json").write_text(json.dumps(diagnostics, indent=2) + "\n")
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

            def git(*args: str) -> str:
                return run(["git", *args], cwd=repo, env=env).stdout

            base = "one\ntwo\nthree\n"
            changed = "ONE\ntwo\nTHREE\n"
            # Keep both edits in one hunk: staging just the first replacement
            # must not accidentally stage the owning hunk or file.
            (repo / "tracked.txt").write_text(base)
            git("add", "tracked.txt")
            git("commit", "-qm", "action baseline")
            (repo / "tracked.txt").write_text(changed)
            head = git("rev-parse", "HEAD")

            def snapshot() -> dict[str, str]:
                return {"head": git("rev-parse", "HEAD"), "head_text": git("show", "HEAD:tracked.txt"),
                        "index": git("show", ":tracked.txt"), "index_tree": git("write-tree"),
                        "worktree": (repo / "tracked.txt").read_text(),
                        "watcher": (repo / "watcher-only.txt").read_text()}

            def wait_text(needle: str) -> dict[str, object]:
                deadline = time.monotonic() + 30
                while True:
                    current = state()
                    if needle in current["text"]:
                        return current
                    if time.monotonic() >= deadline:
                        raise RuntimeError(f"render missing {needle!r}; actual state: {current!r}")

            def wait_state(label: str, predicate: object) -> dict[str, object]:
                deadline = time.monotonic() + 30
                current: dict[str, object] = {}
                while True:
                    current = state()
                    if predicate(current):
                        return current
                    if time.monotonic() >= deadline:
                        raise RuntimeError(f"timed out waiting for {label}; actual state: {current!r}")

            actions: list[dict[str, object]] = []
            report["actions"] = actions

            def picker(label: str, keys: str, title: str | None) -> str:
                # Invoke the unmodified runtime picker. In visual mode <Cmd>
                # preserves the actual native selection for code_action().
                invocation = "<Cmd>lua vim.lsp.buf.code_action()<CR>" if "V" in keys else ":lua vim.lsp.buf.code_action()<CR>"
                run(["tmux", "-L", server_id, "clear-history", "-t", "plain"])
                run(["nvim", "--server", socket, "--remote-send", "<Esc>" + keys + invocation], timeout=8)
                deadline = time.monotonic() + 30
                offered_by_number: dict[str, str] = {}
                last_page_max = 0
                page = 0
                while True:
                    screen = run(["tmux", "-L", server_id, "capture-pane", "-pt", "plain"]).stdout
                    (run_evidence / f"{label}.screen").write_text(screen)
                    visible = re.findall(r"^\s*(\d+): (.+)$", screen, re.MULTILINE)
                    offered_by_number.update(visible)
                    maximum = max((int(number) for number, _ in visible), default=0)
                    if "Type number" in screen:
                        (run_evidence / f"{label}-page-{page}.screen").write_text(screen)
                        break
                    if screen.rstrip().endswith("-- More --") and maximum > last_page_max:
                        (run_evidence / f"{label}-page-{page}.screen").write_text(screen)
                        last_page_max = maximum
                        page += 1
                        run(["tmux", "-L", server_id, "send-keys", "-t", "plain", "Space"])
                    if time.monotonic() >= deadline:
                        raise RuntimeError(f"native picker timeout ({label}):\n{screen}")
                offered = sorted(offered_by_number.items(), key=lambda item: int(item[0]))
                assert offered, screen
                discard = [text for _, text in offered if "discard" in text.lower()]
                assert all("(disabled)" in text for text in discard), screen
                matches = [(number, text) for number, text in offered if title and title in text and "(disabled)" not in text]
                if title:
                    assert len(matches) == 1, (title, offered)
                choice = matches[0][0] if title else "0"
                actions.append({"label": label, "offered": offered, "chosen": choice, "discard": discard})
                # Actual inputlist number, never an invented command payload.
                run(["tmux", "-L", server_id, "send-keys", "-t", "plain", "-l", choice])
                run(["tmux", "-L", server_id, "send-keys", "-t", "plain", "Enter"])
                acknowledged = False
                while True:
                    closed = run(["tmux", "-L", server_id, "capture-pane", "-pt", "plain"]).stdout
                    if "Press ENTER or type command to continue" in closed:
                        if not acknowledged:
                            run(["tmux", "-L", server_id, "send-keys", "-t", "plain", "Enter"])
                            acknowledged = True
                    elif "Type number" not in closed and not closed.rstrip().endswith("-- More --"):
                        (run_evidence / f"{label}-closed.screen").write_text(closed)
                        break
                    if time.monotonic() >= deadline:
                        raise RuntimeError(f"native picker did not close ({label}):\n{closed}")
                return screen

            rendered = wait_text("+THREE")
            # The test locates display rows only to drive native cursor motion;
            # all action identity/range interpretation remains server-owned.
            def file_keys(current: dict[str, object]) -> str:
                rows = [i + 1 for i, line in enumerate(current["text"].splitlines()) if "tracked.txt" in line]
                assert len(rows) == 1, current
                return f"{rows[0]}G0"

            before = snapshot()
            assert before["index"] == base and before["head_text"] == base and before["worktree"] == changed
            picker("file-discard-unavailable", file_keys(rendered), None)
            assert snapshot() == before

            # Exercise the server-owned menu through stock Neovim code actions.
            # Use different menu rows for the on/off offers and Back.
            status_row = next(i + 1 for i, line in enumerate(rendered["text"].splitlines()) if "tracked.txt" in line)
            picker("menu-open", f"{status_row}G0", "Open the commit menu")
            menu = wait_text("git commit")
            assert snapshot() == before
            menu_rows = menu["text"].splitlines()
            assert len(menu_rows) > 1, menu
            first_menu_row, last_menu_row = 1, len(menu_rows)
            picker("menu-switch-on", f"{first_menu_row}G0", "Toggle Stage all modified and deleted files")
            menu_on = wait_text("git commit --all")
            (run_evidence / "menu-on.screen").write_text(menu_on["text"] + "\n")
            assert "-a  Stage all modified and deleted files (--all)  [x]" in menu_on["text"]
            assert snapshot() == before
            picker("menu-switch-off", f"{last_menu_row}G0", "Toggle Stage all modified and deleted files")
            menu_off = wait_text("git commit")
            (run_evidence / "menu-off.screen").write_text(menu_off["text"] + "\n")
            assert "-a  Stage all modified and deleted files (--all)  [ ]" in menu_off["text"]
            assert "git commit --all" not in menu_off["text"]
            assert snapshot() == before
            # Undo only after the server has transitioned to this menu. The
            # intermediate native undo must be observed before server repair,
            # so a pre-existing already-correct buffer cannot satisfy this.
            menu_before_undo = state()
            run(["nvim", "--server", socket, "--remote-send", "gg0iNATIVE-MENU-UNDO<Esc>"], timeout=8)
            menu_typed = wait_state("native edit in active menu", lambda current:
                                    current["version"] > menu_before_undo["version"]
                                    and "NATIVE-MENU-UNDO" in current["text"])
            run(["nvim", "--server", socket, "--remote-send", "u"], timeout=8)
            native_undo = wait_state("actual native undo in active menu", lambda current:
                                     current["version"] > menu_typed["version"]
                                     and "NATIVE-MENU-UNDO" not in current["text"])
            repaired_menu = wait_state("authoritative active-menu repair", lambda current:
                                       current["version"] >= native_undo["version"]
                                       and current["text"] == menu_off["text"])
            assert repaired_menu["text"] == menu_off["text"]
            report["transition_undo_repair"] = {"menu_before": menu_before_undo,
                                                  "typed": menu_typed,
                                                  "native_undo": native_undo,
                                                  "repaired": repaired_menu}
            picker("menu-back", f"{last_menu_row}G0", "Cancel the current operation")
            restored = wait_text("+THREE")
            assert restored["text"] == rendered["text"], (rendered, restored)
            assert snapshot() == before
            # Follow the retained native log-menu action into the concrete log,
            # then exercise a nested commit menu and restore each exact caller.
            picker("log-menu-open", f"{status_row}G0", "Open the log menu")
            log_menu = wait_text("Show graph")
            assert "git log" in log_menu["text"]
            if "-g  Show graph (--graph)  [x]" not in log_menu["text"]:
                picker("log-graph-on", "1G0", "Toggle Show graph")
            graph_on = wait_state("log graph switch and argument preview", lambda current:
                                  "-g  Show graph (--graph)  [x]" in current["text"]
                                  and "--graph" in current["text"])
            picker("log-current", "1G0", "Open current")
            concrete_log = wait_state("concrete current log", lambda current:
                                      "initial" in current["text"] and "*" in current["text"])
            assert any(line.lstrip().startswith("*") for line in concrete_log["text"].splitlines()), concrete_log
            assert "git log --graph" in graph_on["text"]
            picker("log-commit-menu", "1G0", "Open the commit menu")
            commit_menu = wait_text("git commit")
            assert "git commit --all" not in commit_menu["text"]
            picker("nested-commit-switch", "1G0", "Toggle Stage all modified and deleted files")
            nested_on = wait_state("nested commit switch", lambda current:
                                   "-a  Stage all modified and deleted files (--all)  [x]" in current["text"]
                                   and "git commit --all" in current["text"])
            assert "-a  Stage all modified and deleted files (--all)  [x]" in nested_on["text"]
            assert snapshot() == before
            picker("nested-commit-back", "1G0", "Cancel the current operation")
            log_restored = wait_state("restored concrete log", lambda current:
                                      current["text"] == concrete_log["text"])
            assert log_restored["text"] == concrete_log["text"], (concrete_log, log_restored)
            picker("log-menu-back", "1G0", "Cancel the current operation")
            status_restored = wait_state("restored original status", lambda current:
                                         current["text"] == rendered["text"])
            assert status_restored["text"] == rendered["text"], (rendered, status_restored)
            assert snapshot() == before
            report["menu"] = {"caller_text": rendered["text"], "on": menu_on, "off": menu_off,
                                "restored": restored, "git_before_after": [before, snapshot()],
                                "distinct_rows": [first_menu_row, last_menu_row]}
            report["nested_log"] = {"log_menu": log_menu, "graph_on": graph_on,
                                    "concrete_log": concrete_log, "commit_menu": commit_menu,
                                    "nested_on": nested_on, "log_restored": log_restored,
                                    "status_restored": status_restored,
                                    "git_before_after": [before, snapshot()]}
            picker("file-stage", file_keys(state()), "Stage the change at point")
            staged = wait_text("Staged changes (1)")
            after_stage = snapshot()
            assert after_stage["index"] == changed, after_stage
            assert after_stage["head"] == head and after_stage["head_text"] == base and after_stage["worktree"] == changed
            assert after_stage["watcher"] == before["watcher"]
            picker("file-unstage", file_keys(staged), "Unstage the change at point")
            unstaged = wait_text("Unstaged changes (1)")
            assert snapshot() == before
            rows = [i + 1 for i, line in enumerate(unstaged["text"].splitlines()) if line.endswith("-one") or line.endswith("+ONE")]
            assert len(rows) == 2 and rows[1] == rows[0] + 1, unstaged
            picker("selected-lines-stage", f"{rows[0]}G0Vj", "Stage")
            partial_render = wait_text("Staged changes (1)")
            partial = snapshot()
            assert partial["index"] == "ONE\ntwo\nthree\n", partial
            assert partial["head"] == head and partial["head_text"] == base and partial["worktree"] == changed
            assert partial["watcher"] == before["watcher"]
            # A subsequent native request demonstrates the server remains
            # responsive after unavailable, file and selection interactions.
            staged_row = next(i + 1 for i, line in enumerate(partial_render["text"].splitlines()) if "tracked.txt" in line)
            picker("subsequent-request", f"{staged_row}G0", None)
            assert snapshot() == partial

            # Build an actual oversized unstaged section only after the
            # established menu/stage assertions have completed.
            large_base = "".join(f"base line {i:04d}\n" for i in range(430))
            large_changed = "".join(f"changed line {i:04d}\n" for i in range(430))
            (repo / "oversized.txt").write_text(large_base)
            git("add", "oversized.txt")
            git("commit", "-qm", "oversized baseline")
            (repo / "oversized.txt").write_text(large_changed)
            def full_git_state() -> dict[str, str]:
                return {"head": git("rev-parse", "HEAD"), "index": git("write-tree"),
                        "cached": git("diff", "--cached", "--binary", "HEAD"),
                        "worktree": git("diff", "--binary"),
                        "status": git("status", "--porcelain=v1", "-z")}

            oversized = wait_text("more lines: expand")
            summary_row = next(i + 1 for i, line in enumerate(oversized["text"].splitlines())
                               if "more lines: expand" in line)
            git_before_visibility = full_git_state()
            report["visibility"] = {"summary": oversized, "git_before": git_before_visibility}

            def provider(method: str, params: dict[str, object]) -> object:
                # request() flushes pending native changes before resolving 0
                # to a buffer number: pass the actual attached buffer explicitly.
                lua = ("(function() local c=vim.lsp.get_clients()[1]; local p=vim.json.decode(" +
                       json.dumps(json.dumps(params)) + "); "
                       "p.textDocument.uri=vim.uri_from_bufnr(0); "
                       "local r,e=c:request_sync(" + json.dumps(method) + ", p" +
                       ", 5000, vim.api.nvim_get_current_buf()); assert(r, tostring(e)); return vim.json.encode(r.result) end)()")
                return json.loads(rpc("luaeval(" + json.dumps(lua) + ")"))

            def inspect_providers(current: dict[str, object], expanded: bool) -> dict[str, object]:
                text = current["text"]
                lines = text.splitlines()
                params = {"textDocument": {"uri": current["name"]}}
                deadline = time.monotonic() + 30
                while True:
                    symbols = provider("textDocument/documentSymbol", params)
                    links = provider("textDocument/documentLink", params)
                    folds = provider("textDocument/foldingRange", params)
                    tokens = provider("textDocument/semanticTokens/full", params)
                    if all(value is not None for value in (symbols, links, folds, tokens)):
                        break
                    latest = state()
                    if latest["text"] != text or latest["version"] != current["version"]:
                        raise AssertionError(f"document changed during provider synchronization: {current!r} -> {latest!r}")
                    if time.monotonic() >= deadline:
                        trace = trace_path.read_text(errors="replace") if trace_path.exists() else "<missing trace>"
                        raise AssertionError(f"provider remained null for synchronized snapshot; trace tail:\n{trace[-5000:]}")
                files = [i for i, line in enumerate(lines) if "oversized.txt" in line]
                if expanded:
                    assert len(files) == 1, (files, text)
                else:
                    files = [i for i, line in enumerate(lines) if "more lines: expand" in line]
                    assert len(files) == 1, (files, text)
                definition = provider("textDocument/definition", {"textDocument": {"uri": current["name"]},
                                                                     "position": {"line": files[0], "character": 0}})
                report["visibility"].setdefault("provider_observations", []).append({
                    "snapshot": current, "fresh_snapshot": state(),
                    "position": {"line": files[0], "character": 0}, "line_text": lines[files[0]],
                    "symbols": symbols, "links": links, "folds": folds,
                    "tokens": tokens, "definition": definition})
                assert isinstance(symbols, list) and isinstance(links, list) and isinstance(folds, list), (symbols, links, folds)
                assert isinstance(tokens, dict) and isinstance(tokens.get("data"), list), tokens
                def walk_symbols(items: list[dict[str, object]]) -> list[dict[str, object]]:
                    flattened = []
                    for item in items:
                        flattened.append(item)
                        flattened.extend(walk_symbols(item.get("children", [])))
                    return flattened
                flat_symbols = walk_symbols(symbols)
                def bounded_range(value: dict[str, object]) -> bool:
                    start, end = value["start"], value["end"]
                    return (0 <= start["line"] <= end["line"] < len(lines)
                            and 0 <= start["character"] <= len(lines[start["line"]].encode("utf-8"))
                            and 0 <= end["character"] <= len(lines[end["line"]].encode("utf-8"))
                            and (start["line"], start["character"]) <= (end["line"], end["character"]))
                assert all(bounded_range(item["range"]) for item in links)
                assert all(bounded_range(item[key]) for item in flat_symbols for key in ("range", "selectionRange"))
                assert all(0 <= item["range"]["start"]["line"] < len(lines) for item in links)
                assert all(0 <= item["startLine"] <= item["endLine"] < len(lines) for item in folds)
                assert all("range" in item and "selectionRange" in item for item in flat_symbols)
                if expanded:
                    locations = definition if isinstance(definition, list) else [definition] if definition else []
                    assert locations and locations[0]["uri"] == (repo / "oversized.txt").as_uri()
                    assert locations[0]["range"] == {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}
                    assert any(item["name"] == "oversized.txt" for item in flat_symbols)
                    assert any(item["target"] == (repo / "oversized.txt").as_uri() for item in links)
                else:
                    assert definition is None
                assert ("more lines: expand" in text) != expanded
                assert len(tokens["data"]) % 5 == 0, tokens
                token_line = token_column = 0
                for offset in range(0, len(tokens["data"]), 5):
                    delta, column, length, kind, modifiers = tokens["data"][offset:offset + 5]
                    token_line += delta
                    token_column = token_column + column if delta == 0 else column
                    assert 0 <= token_line < len(lines) and length > 0
                    assert 0 <= token_column < token_column + length <= len(lines[token_line].encode("utf-8"))
                    assert 0 <= kind < len(current["server_capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"])
                fresh = state()
                assert fresh["text"] == text and fresh["version"] == current["version"]
                assert fresh["endofline"] == final_newline
                assert (flat_symbols or links or folds or tokens["data"]), "no standard provider exposed structure"
                if expanded:
                    assert flat_symbols or links, "expanded file is absent from symbol/link snapshot"
                return {"symbols": symbols, "links": links, "folds": folds,
                        "tokens": tokens, "definition": definition}

            report["visibility"]["summary_providers"] = inspect_providers(oversized, False)
            picker("visibility-expand", f"{summary_row}G0", "Unfold the section at point")
            expanded = wait_state("expanded oversized section", lambda current:
                                  "more lines: expand" not in current["text"] and "changed line 0429" in current["text"])
            report["visibility"]["expanded"] = expanded
            report["visibility"]["expanded_providers"] = inspect_providers(expanded, True)
            assert full_git_state() == git_before_visibility
            (repo / "watcher-visibility.txt").write_text("visibility watcher\n")
            watcher_expanded = wait_state("expanded visibility after watcher refresh", lambda current:
                                          "watcher-visibility.txt" in current["text"] and "changed line 0429" in current["text"])
            git_after_watcher_setup = full_git_state()
            assert git_after_watcher_setup != git_before_visibility
            assert (repo / "watcher-visibility.txt").read_text() == "visibility watcher\n"
            assert b"watcher-visibility.txt" in git_after_watcher_setup["status"].encode()
            report["visibility"]["watcher_expanded"] = watcher_expanded
            report["visibility"]["git_after_watcher_setup"] = git_after_watcher_setup
            # Explicitly hide then show using the same native stock picker.
            expanded_rows = expanded["text"].splitlines()
            section_row = next(i + 1 for i, line in enumerate(expanded_rows) if "Unstaged changes" in line)
            picker("visibility-hide", f"{section_row}G0", "Fold the section at point")
            hidden = wait_state("explicitly hidden section", lambda current:
                                "Unstaged changes" in current["text"] and "oversized.txt" not in current["text"]
                                and "changed line 0429" not in current["text"])
            hidden_header = next(i + 1 for i, line in enumerate(hidden["text"].splitlines())
                                 if "Unstaged changes" in line)
            picker("visibility-show", f"{hidden_header}G0", "Unfold the section at point")
            shown = wait_state("explicitly shown section", lambda current:
                               "changed line 0429" in current["text"] and "oversized.txt" in current["text"])
            shown_providers = inspect_providers(shown, True)
            assert full_git_state() == git_after_watcher_setup
            assert presentation.read_bytes() == disk_before
            report["visibility"].update({"hidden": hidden, "shown": shown,
                                         "shown_providers": shown_providers,
                                         "git_after": full_git_state(),
                                         "presentation_unchanged": presentation.read_bytes() == disk_before})
            picker("visibility-subsequent-request", f"{hidden_header}G0", None)
            after_visibility_request = state()
            assert after_visibility_request["text"] == shown["text"]
            assert full_git_state() == git_after_watcher_setup
            report["visibility"]["after_subsequent_request"] = after_visibility_request
            (run_evidence / "visibility.screen").write_text(shown["text"] + "\n")
            final = state()
            assert not final["project_modules"] and final["rich_notifications"] == 0
            assert final["rtp"] == initial["rtp"]
            report["action_git_states"] = {"before": before, "file_staged": after_stage, "partial": partial}
            report["actions_final"] = final
            (run_evidence / "actions-final.screen").write_text(final["text"] + "\n")
            report["presentation_disk_unchanged"] = presentation.read_bytes() == disk_before
            report["presentation_disk_bytes"] = presentation.read_bytes().decode()
            report["custom_notifications"] = edited["rich_notifications"]
            if not report["presentation_disk_unchanged"]:
                raise AssertionError("server changed presentation file on disk")
            if edited["rich_notifications"] != 0:
                raise AssertionError("custom view/render notification received")
    except BaseException as error:
        failure = error
        report["traceback"] = traceback.format_exc()
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
            (run_evidence / "native.screen").write_text(str(final["text"]) + "\n")
        if trace_path.exists():
            trace_lines = trace_path.read_text(errors="replace").splitlines()
            changes = [index for index, line in enumerate(trace_lines) if "textDocument/didChange" in line]
            edits = [index for index, line in enumerate(trace_lines) if '"server_request"\t"workspace/applyEdit"' in line]
            noop_eof_deletes = [line for line in trace_lines if "textDocument/didChange" in line and "rangeLength = 0, text = \"\"" in line]
            report["trace_counts"] = {"lines": len(trace_lines), "did_change": len(changes), "apply_edit": len(edits), "empty_insert_changes": len(noop_eof_deletes)}
            request_ids = {value for line in trace_lines for value in re.findall(r"request_id = (\d+)", line)}
            report["trace_counts"]["distinct_request_ids"] = len(request_ids)
            # Count outbound applyEdit requests once (not their log send/handler
            # duplicates). 30 allows the established content/log/menu sequence,
            # oversized fixture refresh, expansion, watcher, Hide and Show (five
            # added renders), with bounded refresh coalescing; each transition
            # has its own bounded no-progress deadline.
            if len(request_ids) > 100 or len(edits) > 30 or len(noop_eof_deletes) > 2:
                failure = RuntimeError(f"unbounded Plain synchronization trace: {report['trace_counts']!r}")
            keep = {index for index, line in enumerate(trace_lines) if "textDocument/didOpen" in line or '"magit-lsp/outbound/1"' in line}
            keep.update(changes[:20])
            keep.update(changes[-10:])
            keep.update(edits[:8])
            keep.update(edits[-3:])
            selected = [line for index, line in enumerate(trace_lines) if index in keep]
            (run_evidence / "trace-excerpts.log").write_text("\n".join(selected) + "\n")
        (run_evidence / ("failure.json" if failure else "native.json")).write_text(json.dumps({**report, "error": str(failure) if failure else None}, indent=2, sort_keys=True) + "\n")
    if failure:
        raise failure
    if not report["cleanup"]["cleanup_verified"]:
        raise RuntimeError(f"owned process cleanup failed: {report['cleanup']!r}")
    if not report.get("trace_counts"):
        raise RuntimeError("Neovim LSP trace missing; synchronization bounds were not checked")


if __name__ == "__main__":
    errors: list[str] = []
    for with_final_newline in (False, True):
        try:
            run_variant(with_final_newline)
        except BaseException as error:
            errors.append(f"{with_final_newline=}: {error}")
    if errors:
        raise RuntimeError("; ".join(errors))
