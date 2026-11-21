#!/usr/bin/env python3
"""Exercise pinned Mason's real registry/compiler/installer over loopback HTTP."""

from __future__ import annotations

import functools
import hashlib
import http.server
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import threading
import traceback
import zipfile

import check_mason_package as manifest
from test_lazy_install import (
    pid_executing, prepare_evidence, recorded_pid, run, stop_owned_pid, write_json,
)
from ui_lib.process_identity import record_command

ROOT = Path(__file__).resolve().parents[1]
MASON_REPO = "https://github.com/mason-org/mason.nvim.git"
MASON_TAG = "v2.3.1"
MASON_COMMIT = "2a6940af80375532e5e9e7c1f2fc6319a1b7a69d"
REGISTRY = "github:local/magit-registry@fixture"


def package_spec() -> dict:
    """Derive JSON from the production YAML, rejecting any unhandled content."""
    manifest.check()
    content = manifest.content_lines(manifest.read_lines(manifest.PACKAGE))
    top = manifest.parse_top_level(content[1:])
    if set(top) != set(manifest.REQUIRED_KEYS) | {"bin"}:
        raise RuntimeError("manifest has unhandled top-level keys")
    spec = {}
    for key in ("name", "description", "homepage"):
        if top[key]["children"]:
            raise RuntimeError(f"unhandled children in {key}")
        spec[key] = manifest.unquote(top[key]["value"])
    for key in ("licenses", "languages", "categories"):
        spec[key] = manifest.list_values(top[key], key)
    fields, assets = manifest.parse_asset(top["source"]["children"])
    if set(fields) != {"id"} or len(assets) != 1 or set(assets[0]) != {"target", "file", "bin"}:
        raise RuntimeError("manifest has unhandled source fields")
    # Round-trip the complete supported subset, including indentation and every
    # non-comment line: a parser silently dropping syntax cannot pass parity.
    spec["source"] = {"id": fields["id"], "asset": assets}
    spec["bin"] = manifest.mapping(top["bin"])
    expected = [(0, "---")]
    for key, value in spec.items():
        if isinstance(value, str):
            expected.append((0, f"{key}: {value}"))
        elif isinstance(value, list):
            expected.append((0, f"{key}:" if value else f"{key}: []"))
            expected.extend((2, f"- {item}") for item in value)
        elif key == "source":
            expected.extend([(0, "source:"), (2, f'id: {fields["id"]}'), (2, "asset:")])
            for index, (field, item) in enumerate(assets[0].items()):
                expected.append((4 if index == 0 else 6, f"{'- ' if index == 0 else ''}{field}: {item}"))
        else:
            expected.append((0, "bin:"))
            expected.extend((2, f'{name}: "{item}"') for name, item in value.items())
    if [(indent, text) for indent, text, _ in content] != expected:
        raise RuntimeError("production YAML does not round-trip through the supported subset")
    return spec


def execute() -> None:
    """Run corrupt-archive rejection then a real install and installed LSP smoke."""
    evidence = prepare_evidence(ROOT / "build/ui/mason-install")
    base = Path(tempfile.mkdtemp(prefix="magit-mason-install-"))
    report = {"dependency": {"repo": MASON_REPO, "tag": MASON_TAG, "commit": MASON_COMMIT}}
    pid_file = base / "server.pid"
    httpd = None
    thread = None
    failure = None
    try:
        env = {key: value for key, value in os.environ.items() if not key.startswith("XDG_")}
        for key in ("VIMINIT", "MYVIMRC", "LUA_INIT", "MAGIT_LSP_BIN"):
            env.pop(key, None)
        env["HOME"] = str(base / "home")
        for name in ("CONFIG", "DATA", "STATE", "CACHE", "RUNTIME"):
            directory = base / name.lower()
            directory.mkdir(mode=0o700)
            env[f"XDG_{name}_HOME" if name != "RUNTIME" else "XDG_RUNTIME_DIR"] = str(directory)
        Path(env["HOME"]).mkdir()
        env.update({"NVIM_APPNAME": "nvim", "GIT_CONFIG_GLOBAL": str(base / "gitconfig"),
                    "GIT_CONFIG_NOSYSTEM": "1", "GIT_TERMINAL_PROMPT": "0"})
        (base / "gitconfig").write_text("")
        manager = base / "mason.nvim"
        run(["git", "init", "-q", str(manager)], env=env)
        run(["git", "-C", str(manager), "remote", "add", "origin", MASON_REPO], env=env)
        run(["git", "-C", str(manager), "fetch", "-q", "--depth", "1", "origin", MASON_COMMIT], env=env, timeout=180)
        run(["git", "-C", str(manager), "checkout", "-q", "--detach", "FETCH_HEAD"], env=env)
        head = run(["git", "-C", str(manager), "rev-parse", "HEAD"], env=env).stdout.strip()
        if head != MASON_COMMIT:
            raise RuntimeError(f"Mason pin mismatch: {head}")
        report["dependency"]["verified_head"] = head
        spec = package_spec()
        write_json(evidence / "production-spec.json", spec)
        report["manifest"] = {"path": str(manifest.PACKAGE), "parity": "complete subset round-trip",
                              "sha256": hashlib.sha256(manifest.PACKAGE.read_bytes()).hexdigest(),
                              "package_overrides": [], "transport_registry": REGISTRY}
        web = base / "web"
        registry_dir = web / "local/magit-registry/fixture"
        registry_dir.mkdir(parents=True)
        registry_json = json.dumps([spec]).encode()
        with zipfile.ZipFile(registry_dir / "registry.json.zip", "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("registry.json", registry_json)
        (registry_dir / "checksums.txt").write_text(
            hashlib.sha256(registry_json).hexdigest() + "  registry.json\n")
        dist = base / "dist"
        binary = ROOT / "target/debug/magit-lsp"
        provenance = base / "provenance.json"
        provenance.write_text(json.dumps({
            "version": "0.2.0",
            "target": "x86_64-unknown-linux-gnu",
            "executable_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        }), encoding="utf-8")
        run(["sh", "scripts/package-release.sh", "0.2.0", str(binary), str(dist),
             "x86_64-unknown-linux-gnu", str(provenance)], cwd=ROOT, env=env)
        asset = spec["source"]["asset"][0]["file"]
        served = web / "bluedrink9/magit-lsp/v0.2.0" / asset
        served.parent.mkdir(parents=True)
        served.write_bytes(b"intentionally corrupt release archive\n")
        requests = []

        class Handler(http.server.SimpleHTTPRequestHandler):
            """Record exactly which registry and release routes Mason fetched."""

            def log_message(self, format, *args):
                """Retain HTTP result text as structured transport evidence."""
                requests.append({"path": self.path, "message": format % args})

        httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Handler, directory=str(web)))
        thread = threading.Thread(target=httpd.serve_forever, daemon=True)
        thread.start()
        url = f"http://127.0.0.1:{httpd.server_port}/%s/%s/%s"
        install = base / "install"
        launcher = base / "launch-installed"
        launcher.write_text("#!/bin/sh\nset -e\n" + record_command(pid_file)
                            + f"exec {shlex.quote(str(install / 'bin/magit-lsp'))} \"$@\"\n")
        launcher.chmod(0o700)
        repo = base / "repo"
        repo.mkdir()
        run(["git", "init", "-q"], cwd=repo, env=env)
        run(["git", "-c", "user.name=Mason Test", "-c", "user.email=mason@example.invalid", "commit", "--allow-empty", "-qm", "fixture"], cwd=repo, env=env)
        (repo / "mason-install.txt").write_text("untracked fixture\n")
        env.update({"MAGIT_MASON_MANAGER": str(manager), "MAGIT_MASON_ROOT": str(install),
                    "MAGIT_MASON_URL": url, "MAGIT_MASON_REGISTRY": REGISTRY,
                    "MAGIT_MASON_CHECKOUT": str(ROOT), "MAGIT_MASON_LAUNCHER": str(launcher)})
        report["isolation"] = {key: value for key, value in env.items() if key == "HOME" or key.startswith("XDG_")}
        for mode in ("negative", "positive"):
            if mode == "positive":
                shutil.copyfile(dist / asset, served)
            env["MAGIT_MASON_MODE"] = mode
            env["MAGIT_MASON_EVIDENCE"] = str(evidence / f"{mode}.json")
            process = subprocess.Popen(["nvim", "--headless", "-i", "NONE", "-u", str(ROOT / "tests/release/mason_install.lua")], cwd=repo, env=env,
                                       stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            try:
                stdout, stderr = process.communicate(timeout=120)
            except subprocess.TimeoutExpired:
                process.kill()
                process.communicate()
                raise RuntimeError(f"{mode} Neovim timed out")
            report[mode] = {"editor_pid": process.pid, "reaped": process.poll() is not None,
                            "returncode": process.returncode, "stdout": stdout, "stderr": stderr}
            if process.returncode:
                raise RuntimeError(f"{mode} Mason run failed:\n{stdout}{stderr}")
            report[mode]["evidence"] = json.loads((evidence / f"{mode}.json").read_text())
            assert not (install / "staging/magit-lsp.lock").exists(), "installer left its lock"
            assert not (install / "staging/magit-lsp").exists(), "installer left staging files"
            report[mode]["staging_and_lock_removed"] = True
        receipt = json.loads((install / "packages/magit-lsp/mason-receipt.json").read_text())
        link = install / "bin/magit-lsp"
        assert receipt["name"] == "magit-lsp" and receipt["schema_version"] == "2.0", receipt
        assert receipt["source"]["raw"] == spec["source"], receipt
        assert receipt["source"]["type"] == "registry+v1", receipt
        # GitHub purl namespace/name are case-insensitive and Mason normalizes
        # them to lowercase; the raw source above must retain production bytes.
        assert receipt["source"]["id"] == "pkg:github/bluedrink9/magit-lsp@v0.2.0", receipt
        assert receipt["links"]["bin"]["magit-lsp"] == spec["source"]["asset"][0]["bin"], receipt
        assert {key: receipt["registry"][key] for key in ("proto", "namespace", "name", "version")} == {
            "proto": "github", "namespace": "local", "name": "magit-registry", "version": "fixture"}, receipt
        assert link.is_symlink() and link.resolve() == install / "packages/magit-lsp" / spec["source"]["asset"][0]["bin"]
        version = run([str(link), "--version"], env=env).stdout.strip()
        assert version == "magit-lsp 0.2.0", version
        report["installed"] = {"receipt": receipt, "binlink": os.readlink(link), "version": version}
        assert [item["path"] for item in requests] == [
            "/local/magit-registry/fixture/registry.json.zip",
            "/local/magit-registry/fixture/checksums.txt",
            f"/bluedrink9/magit-lsp/v0.2.0/{asset}",
            f"/bluedrink9/magit-lsp/v0.2.0/{asset}",
        ], requests
        assert all('"GET ' in item["message"] and ' 200 ' in item["message"] for item in requests), requests
        assert recorded_pid(pid_file) is not None, "installed server ownership was not recorded"
        report["http_requests"] = requests
    except BaseException as error:
        failure = error
        report["traceback"] = traceback.format_exc()
    finally:
        pid = recorded_pid(pid_file)
        cleanup = stop_owned_pid(pid, pid_file)
        if httpd:
            httpd.shutdown()
            httpd.server_close()
        if thread:
            thread.join(timeout=5)
        shutil.rmtree(base)
        report["cleanup"] = {"server": cleanup, "remaining_owned_processes": [pid] if pid_executing(pid) else [],
                             "http_thread_stopped": thread is None or not thread.is_alive(),
                             "temporary_root": str(base), "temporary_removed": not base.exists()}
        if (pid_executing(pid) or (thread and thread.is_alive())) and failure is None:
            failure = RuntimeError("owned processes or HTTP thread survived cleanup")
        write_json(evidence / ("failure.json" if failure else "mason-install.json"), report)
    if failure:
        raise failure
    print(f"mason-install-check: {MASON_TAG}@{MASON_COMMIT}; corrupt unpack rejected, retry installed 0.2.0; LSP status and cleanup ok; {evidence}")


if __name__ == "__main__":
    execute()
