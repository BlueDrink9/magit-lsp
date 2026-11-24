#!/usr/bin/env python3
"""Run the documented lazy.nvim spec against the real manager in isolation."""

from __future__ import annotations

import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import subprocess
import tempfile
import time
import traceback

from ui_lib.process_identity import record_command, signal_owned

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BINARY = ROOT / "target/debug/magit-lsp"
DEFAULT_EVIDENCE = ROOT / "build/ui/lazy-install"
DOCUMENT = ROOT / "docs/CLIENT_INSTALL.md"
LUA_DRIVER = ROOT / "tests/release/lazy_install.lua"
LAZY_REPO = "https://github.com/folke/lazy.nvim.git"
LAZY_TAG = "v11.17.5"
LAZY_COMMIT = "85c7ff3711b730b4030d03144f6db6375044ae82"
SPEC_MARKER = "-- lazy.nvim plugin spec"
SPEC_SOURCE = "  'BlueDrink9/magit-lsp',"
FIXTURE = "lazy-install.txt"
RUNTIME_SUBDIR = "clients/neovim"
PLUGIN_SOURCE = ROOT / RUNTIME_SUBDIR / "lua/magit-lsp/init.lua"
RUNTIME_HOOK = "vim.opt.rtp:append(plugin.dir .. '/clients/neovim')"
PLUGIN_HOOK = "vim.cmd.source(plugin.dir .. '/clients/neovim/plugin/magit-lsp.lua')"


def run(args: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
    """Run a bounded subprocess and raise with captured output on failure."""
    kwargs.setdefault("timeout", 60)
    result = subprocess.run(args, text=True, capture_output=True, check=False, **kwargs)
    if result.returncode:
        raise RuntimeError(
            f"command failed ({result.returncode}): {args!r}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def spec_violations(snippet: str) -> list[str]:
    """Return documented lazy-spec contract violations for one snippet."""
    violations = []
    for key in ("opts", "build"):
        if re.search(rf"\b{key}\s*=", snippet):
            violations.append(f"documented lazy spec must not set {key}")
    for needle, label in (
        ("lazy = true", "lazy = true"),
        ("<leader>gs", "the <leader>gs mapping"),
        ("require('magit-lsp').setup", "the magit-lsp setup callback"),
        ("vim.fn.expand('~/.local/bin/magit-lsp')", "the installed binary command"),
    ):
        if needle not in snippet:
            violations.append(f"documented lazy spec missing {label}")
    if snippet.count(SPEC_SOURCE) != 1:
        violations.append(f"documented lazy spec must contain exactly one {SPEC_SOURCE!r}")
    hook = re.search(
        r"\bconfig\s*=\s*function\(plugin\)\n(.*?)\n  end,", snippet, re.S)
    if not hook:
        violations.append("documented lazy spec must add the client runtime in config(plugin)")
    else:
        statements = [
            line.strip() for line in hook.group(1).splitlines()
            if line.strip() and not line.strip().startswith("--")]
        if statements != [RUNTIME_HOOK, PLUGIN_HOOK]:
            violations.append(
                "documented lazy config(plugin) must append clients/neovim to "
                f"runtimepath and source its plugin entry point, found {statements!r}")
    return violations


def documented_spec() -> tuple[str, list[str]]:
    """Extract and validate the exact lazy spec block from the install guide."""
    document = DOCUMENT.read_text(encoding="utf-8")
    match = re.search(
        rf"```lua\n({re.escape(SPEC_MARKER)}\n.*?)\n```", document, re.S)
    if not match:
        raise RuntimeError(f"{DOCUMENT} has no lazy.nvim plugin spec block")
    snippet = match.group(1)
    violations = spec_violations(snippet)
    if violations:
        raise RuntimeError("; ".join(violations))
    return snippet, violations


def adapt_spec(snippet: str, checkout: Path) -> tuple[str, list[str], list[str]]:
    """Replace only the documented source with this checkout and prove it.

    The documented URL names the plugin `magit-lsp`; a local `dir` source keeps
    that name explicitly so the manager registers the same plugin identity.
    """
    adapted = snippet.replace(
        SPEC_SOURCE,
        f"  dir = {json.dumps(str(checkout))}, name = 'magit-lsp',", 1)
    diff = list(difflib.unified_diff(
        snippet.splitlines(), adapted.splitlines(),
        fromfile="documented", tofile="adapted", lineterm=""))
    changed = [line for line in diff if line[:1] in "+-" and not line.startswith(("+++", "---"))]
    if (len(changed) != 2 or "BlueDrink9/magit-lsp" not in changed[0]
            or "dir =" not in changed[1]):
        raise RuntimeError(f"adapted spec changed more than the source directory: {changed!r}")
    return adapted, changed, diff


def fetch_lazy(manager: Path, env: dict[str, str]) -> str:
    """Fetch the pinned lazy.nvim commit into a disposable cache and verify it."""
    manager.mkdir(parents=True)
    run(["git", "init", "-q", str(manager)], env=env)
    run(["git", "-C", str(manager), "remote", "add", "origin", LAZY_REPO], env=env)
    run(["git", "-C", str(manager), "fetch", "-q", "--depth", "1", "origin", LAZY_COMMIT],
        env=env, timeout=180)
    run(["git", "-C", str(manager), "-c", "advice.detachedHead=false",
         "checkout", "-q", "--detach", "FETCH_HEAD"], env=env)
    head = run(["git", "-C", str(manager), "rev-parse", "HEAD"], env=env).stdout.strip()
    if head != LAZY_COMMIT:
        raise RuntimeError(f"lazy.nvim checkout mismatch: {head!r} != {LAZY_COMMIT!r}")
    return run(["git", "-C", str(manager), "config", "--get", "remote.origin.url"], env=env).stdout.strip()


def child_env(
    base: Path, home: Path, config: Path, data: Path, state: Path, cache: Path,
    spec: Path, manager: Path, evidence: Path,
) -> dict[str, str]:
    """Build a Neovim/Git environment isolated from the user's configuration."""
    env = {key: value for key, value in os.environ.items() if not key.startswith("XDG_")}
    for key in ("MAGIT_LSP_BIN", "VIMINIT", "MYVIMRC", "LUA_INIT"):
        env.pop(key, None)
    env.update({
        "HOME": str(home),
        "XDG_CONFIG_HOME": str(config),
        "XDG_DATA_HOME": str(data),
        "XDG_STATE_HOME": str(state),
        "XDG_CACHE_HOME": str(cache),
        "NVIM_APPNAME": "nvim",
        "GIT_CONFIG_GLOBAL": str(base / "gitconfig"),
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_TERMINAL_PROMPT": "0",
        "MAGIT_LAZY_CHECKOUT": str(ROOT),
        "MAGIT_LAZY_SPEC": str(spec),
        "MAGIT_LAZY_MANAGER": str(manager),
        "MAGIT_LAZY_EVIDENCE": str(evidence),
        "MAGIT_LAZY_FIXTURE": FIXTURE,
    })
    return env


def pid_executing(pid: int | None) -> bool:
    """Distinguish an executing owned process from a Linux zombie entry."""
    if not isinstance(pid, int):
        return False
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    status = Path(f"/proc/{pid}/status")
    if status.is_file():
        text = status.read_text(encoding="utf-8", errors="replace")
        if "\nState:\tZ" in text or "\nState:\tX" in text:
            return False
    return True


def wait_pid_gone(pid: int | None, timeout: float) -> bool:
    """Wait briefly for one owned process to stop executing."""
    if not isinstance(pid, int):
        return True
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if not pid_executing(pid):
            return True
        time.sleep(0.05)
    return not pid_executing(pid)


def recorded_pid(record: Path) -> int | None:
    """Read the PID recorded by the exec wrapper, if the server started."""
    try:
        return int(record.read_text(encoding="utf-8").strip())
    except (FileNotFoundError, ValueError):
        return None


def stop_owned_pid(pid: int | None, record: Path) -> dict[str, object]:
    """Terminate one wrapper-recorded process after validating its birth identity."""
    result: dict[str, object] = {
        "pid": pid,
        "was_executing": False,
        "terminated": False,
        "identity_checked": False,
        "identity_matched": None,
    }
    if not isinstance(pid, int):
        return result
    if wait_pid_gone(pid, 5):
        return result
    result["was_executing"] = True
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            if not signal_owned(pid, record, sig):
                result["identity_checked"] = True
                result["identity_matched"] = False
                return result
            result["identity_checked"] = True
            result["identity_matched"] = True
            if wait_pid_gone(pid, 5):
                result["terminated"] = True
                result["signal"] = sig.name
                return result
        except ProcessLookupError:
            result["terminated"] = True
            result["signal"] = sig.name
            return result
    return result


def read_json(path: Path) -> dict[str, object] | None:
    """Read a JSON evidence file when a run produced one."""
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return None


def validate_positive(positive: dict[str, object]) -> None:
    """Cross-check the Lua oracles from Python using stable expected values."""
    pre = positive["pre_key"]
    post = positive["post_key"]
    client = post["client"]
    checks = {
        "pre-key did not preload the plugin": pre["plugin_loaded"] is False,
        "pre-key started no LSP client": pre["lsp_clients"] == 0,
        "pre-key lazy state is unloaded": pre["lazy_loaded"] is False,
        "pre-key client runtime is absent from runtimepath": pre["runtime_on_runtimepath"] is False,
        "checkout root ships no Neovim runtime": pre["root_runtime_present"] is False,
        "pre-key :Magit is unavailable": pre["magit_command_available"] is False,
        "lazy.nvim owns the documented key": pre["lazy_key_owner"] == "magit-lsp",
        "post-key lazy state is loaded": post["plugin_loaded"] is True,
        "plugin dir is this checkout": post["plugin_dir"] == str(ROOT),
        "module loaded from this checkout": post["plugin_source"] == str(PLUGIN_SOURCE),
        "client runtime is on runtimepath after loading": post["runtime_on_runtimepath"] is True,
        "post-key :Magit is registered": post["magit_command_available"] is True,
        "status buffer has the magit filetype": post["filetype"] == "magit-lsp",
        "status rendered the fixture": FIXTURE in str(post["text"]),
        "status rendered the head": "Head:" in str(post["text"]),
        "status rendered the untracked section": "Untracked files" in str(post["text"]),
        "handshake completed": client["initialized"] is True and client["server_capabilities"] is True,
        "client name is magit-lsp": client["name"] == "magit-lsp",
        "owned client stopped": positive["clients_stopped"] is True,
    }
    failed = [message for message, passed in checks.items() if not passed]
    if failed:
        raise RuntimeError("; ".join(failed))


def prepare_evidence(path: Path) -> Path:
    """Keep prior evidence and create a fresh directory for this run."""
    path = path.resolve()
    if path.exists():
        path.rename(path.with_name(f"{path.name}-{time.time_ns()}"))
    path.mkdir(parents=True)
    return path


def write_json(path: Path, value: object) -> None:
    """Write one structured evidence file."""
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def run_nvim_owned(
    nvim: str, cwd: Path, env: dict[str, str], timeout: float = 120,
) -> dict[str, object]:
    """Run one owned headless Neovim and always reap it on success or timeout."""
    process = subprocess.Popen(
        [nvim, "--headless", "-i", "NONE", "-u", str(LUA_DRIVER)],
        cwd=cwd, env=env, text=True, stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    timed_out = False
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        process.kill()
        stdout, stderr = process.communicate()
    return {
        "returncode": process.returncode,
        "stdout": stdout,
        "stderr": stderr,
        "editor_pid": process.pid,
        "timed_out": timed_out,
        "reaped": process.poll() is not None,
    }


def execute(args: argparse.Namespace) -> None:
    """Run the positive and negative real-manager checks and record evidence."""
    binary = args.binary.resolve()
    if not binary.is_file():
        raise RuntimeError(f"server binary missing: {binary}")
    if not LUA_DRIVER.is_file():
        raise RuntimeError(f"Lua driver missing: {LUA_DRIVER}")
    nvim = shutil.which("nvim")
    if not nvim:
        raise RuntimeError("required executable not found: nvim")
    if not shutil.which("git"):
        raise RuntimeError("required executable not found: git")

    evidence = prepare_evidence(args.evidence)
    report: dict[str, object] = {"pinned_dependency": {
        "repo": LAZY_REPO, "tag": LAZY_TAG, "commit": LAZY_COMMIT,
    }}
    state: dict[str, int | None] = {"server_pid": None, "editor_pid": None}
    failure: BaseException | None = None
    base = Path(tempfile.mkdtemp(prefix="magit-lazy-install-"))
    pid_file = base / "server.pid"

    try:
        home = base / "home"
        config = base / "config"
        data = base / "data"
        state_home = base / "state"
        cache = base / "cache"
        repo = base / "repo"
        for directory in (home, config, data, state_home, cache, repo):
            directory.mkdir(parents=True)
        (base / "gitconfig").write_text("", encoding="utf-8")
        server = home / ".local/bin/magit-lsp"
        server.parent.mkdir(parents=True)
        server.write_text(
            "#!/bin/sh\nset -e\n"
            + record_command(pid_file)
            + f"exec {shlex.quote(str(binary))} \"$@\"\n",
            encoding="utf-8")
        server.chmod(0o700)
        manager = data / "nvim/lazy/lazy.nvim"
        real_home = Path.home()
        isolated = (home, config, data, state_home, cache, manager, repo)
        if not all(str(path).startswith(str(base) + os.sep) for path in isolated):
            raise RuntimeError(f"isolation paths escaped the temporary root: {isolated!r}")
        report["isolation"] = {
            "temporary_root": str(base),
            "home": str(home),
            "xdg_config": str(config),
            "xdg_data": str(data),
            "xdg_state": str(state_home),
            "xdg_cache": str(cache),
            "nvim_appname": "nvim",
            "user_home": str(real_home),
            "user_home_isolated": str(real_home) != str(home),
            "lazy_manager": str(manager),
        }

        git_env = child_env(base, home, config, data, state_home, cache,
                            base / "spec.adapted.lua", manager, evidence)
        run(["git", "init", "-q"], cwd=repo, env=git_env)
        run(["git", "config", "user.name", "Lazy Install Check"], cwd=repo, env=git_env)
        run(["git", "config", "user.email", "lazy-install@example.invalid"], cwd=repo, env=git_env)
        (repo / "tracked.txt").write_text("base\n", encoding="utf-8")
        run(["git", "add", "tracked.txt"], cwd=repo, env=git_env)
        run(["git", "commit", "-qm", "initial"], cwd=repo, env=git_env)
        (repo / FIXTURE).write_text("lazy install\n", encoding="utf-8")

        remote = fetch_lazy(manager, git_env)
        report["pinned_dependency"].update({
            "remote": remote,
            "verified_head": run(["git", "-C", str(manager), "rev-parse", "HEAD"],
                                 env=git_env).stdout.strip(),
        })

        snippet, _ = documented_spec()
        adapted, changed, diff = adapt_spec(snippet, ROOT)
        (evidence / "spec.documented.lua").write_text(snippet + "\n", encoding="utf-8")
        (evidence / "spec.adapted.lua").write_text(adapted + "\n", encoding="utf-8")
        (evidence / "spec.diff").write_text("\n".join(diff) + "\n", encoding="utf-8")
        report["documented_spec"] = {
            "path": str(DOCUMENT),
            "sha256": hashlib.sha256(snippet.encode()).hexdigest(),
            "adapted_sha256": hashlib.sha256(adapted.encode()).hexdigest(),
            "changed_lines": changed,
            "contract": "ok",
        }

        # Negative control for the spec oracle: the documented snippet plus an
        # eager opts hook must be rejected, so a broken doc cannot pass silently.
        broken = snippet.replace("  lazy = true,", "  lazy = true,\n  opts = {},", 1)
        broken_violations = spec_violations(broken)
        if not any("opts" in violation for violation in broken_violations):
            raise RuntimeError("spec oracle accepted a broken documented spec")
        report["negative_broken_spec"] = {
            "injected": "opts = {}",
            "violations": broken_violations,
        }

        spec = base / "spec.adapted.lua"
        spec.write_text(adapted + "\n", encoding="utf-8")
        env = child_env(base, home, config, data, state_home, cache, spec,
                        manager, evidence / "positive.json")
        started = time.monotonic()
        positive_run = run_nvim_owned(nvim, repo, env)
        state["editor_pid"] = positive_run["editor_pid"]
        state["server_pid"] = recorded_pid(pid_file)
        report["positive"] = {
            "returncode": positive_run["returncode"],
            "seconds": round(time.monotonic() - started, 3),
            "stdout": positive_run["stdout"],
            "stderr": positive_run["stderr"],
            "editor_pid": positive_run["editor_pid"],
            "editor_reaped": positive_run["reaped"],
            "timed_out": positive_run["timed_out"],
            "server_pid": state["server_pid"],
        }
        if positive_run["returncode"] != 0:
            raise RuntimeError(
                "real lazy.nvim run failed:\n"
                + str(positive_run["stdout"]) + str(positive_run["stderr"]))
        positive = read_json(evidence / "positive.json")
        if not positive:
            raise RuntimeError("real lazy.nvim run produced no positive evidence")
        validate_positive(positive)
        report["positive"]["evidence"] = positive

        # The client stop and editor exit must end the owned server before the
        # negative control reuses the same documented command path.
        positive_cleanup = stop_owned_pid(state["server_pid"], pid_file)
        report["positive_server_cleanup"] = positive_cleanup
        if pid_executing(state["server_pid"]):
            raise RuntimeError(f"owned server survived the positive run: {positive_cleanup!r}")
        for owned_path in (pid_file, pid_file.with_suffix(".identity")):
            owned_path.unlink(missing_ok=True)

        negative_env = dict(env)
        negative_env.update({
            "MAGIT_LAZY_FORCE_PRELOAD": "1",
            "MAGIT_LAZY_EVIDENCE": str(evidence / "negative.json"),
        })
        negative_run = run_nvim_owned(nvim, repo, negative_env)
        negative_output = str(negative_run["stdout"]) + str(negative_run["stderr"])
        negative_server_pid = recorded_pid(pid_file)
        if negative_server_pid is not None:
            state["server_pid"] = negative_server_pid
            raise RuntimeError(
                f"preload negative control started an owned server: {negative_server_pid}")
        expected = "pre-key oracle: magit-lsp was preloaded or bypassed the lazy manager"
        if negative_run["returncode"] == 0:
            raise RuntimeError("preload negative control unexpectedly passed")
        if expected not in negative_output:
            raise RuntimeError(
                "preload negative control failed for the wrong reason:\n" + negative_output)
        if (evidence / "negative.json").exists():
            raise RuntimeError("preload negative control loaded the plugin through the manager")
        matched_line = next(
            (line.strip() for line in negative_output.splitlines() if expected in line), "")
        report["negative_preload"] = {
            "returncode": negative_run["returncode"],
            "expected_message": expected,
            "matched_line": matched_line,
            "editor_pid": negative_run["editor_pid"],
            "editor_reaped": negative_run["reaped"],
            "timed_out": negative_run["timed_out"],
            "server_started": negative_server_pid is not None,
        }
    except BaseException as error:
        failure = error
        report["traceback"] = traceback.format_exc()
    finally:
        if state["server_pid"] is None:
            state["server_pid"] = recorded_pid(pid_file)
        cleanup_pid = stop_owned_pid(state["server_pid"], pid_file)
        server_pid = state["server_pid"]
        proc_entry = Path(f"/proc/{server_pid}").exists() if isinstance(server_pid, int) else False
        shutil.rmtree(base, ignore_errors=True)
        report["cleanup"] = {
            "server_pid": cleanup_pid,
            "server_pid_recorded": server_pid,
            "server_proc_entry_remains": proc_entry,
            "server_executing_after": pid_executing(server_pid),
            "editor_pid": state["editor_pid"],
            "temporary_root": str(base),
            "temporary_removed": not base.exists(),
            "remaining_owned_processes": [pid for pid in (server_pid,) if pid_executing(pid)],
        }
        write_json(evidence / ("failure.json" if failure else "lazy-install.json"), report)

    if failure:
        raise failure
    print(
        f"lazy-install-check: {LAZY_TAG}@{LAZY_COMMIT[:7]} loaded this checkout "
        f"through <leader>gs; status rendered {FIXTURE}; cleanup ok; evidence {evidence}")


def main() -> None:
    """Parse command-line arguments and run the isolated installation check."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--evidence", type=Path, default=DEFAULT_EVIDENCE)
    execute(parser.parse_args())


if __name__ == "__main__":
    main()
