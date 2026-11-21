#!/usr/bin/env python3
"""Measure real public :Magit opens; persist JSONL and merged raw traces."""
from __future__ import annotations

import json
import os
import shutil
import statistics
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path


def run_sample(repo: Path, root: Path, nvim: str, server: str, artifact: Path, label: str) -> list[dict]:
    stem = f"{label}-{time.time_ns()}"
    client_trace = artifact / f"{stem}.client.jsonl"
    env = os.environ.copy()
    env.update(MAGIT_STARTUP_REPO=str(repo), MAGIT_STARTUP_ROOT=str(root),
               MAGIT_STARTUP_OUTPUT=str(artifact / f"{stem}.events.jsonl"),
               MAGIT_LSP_BIN=server, MAGIT_LSP_TRACE=str(client_trace))
    command = [nvim, "--headless", "-u", "NONE", "-l", str(repo / "scripts/measure_nvim_startup.lua")]
    started = time.time()
    result = subprocess.run(command, cwd=root, env=env, text=True, capture_output=True, timeout=60)
    if result.returncode:
        raise RuntimeError(f"{label} failed ({result.returncode}): {result.stderr[-2000:]}")
    event_path = Path(env["MAGIT_STARTUP_OUTPUT"])
    events = [json.loads(line) for line in event_path.read_text().splitlines()]
    wanted = {"process_cold_first_open", "initialized_server_reopen", "already_open_focus"}
    if {event["case"] for event in events} != wanted or not all(e["valid_status"] for e in events):
        raise RuntimeError(f"missing event or invalid status: {events}")
    server_trace = Path(str(client_trace) + ".server")
    raw = []
    for path in (client_trace, server_trace):
        if path.exists():
            raw.extend(json.loads(line) for line in path.read_text().splitlines())
    raw.sort(key=lambda e: e.get("t_unix_ms", 0))
    (artifact / f"{stem}.trace.jsonl").write_text("".join(json.dumps(e) + "\n" for e in raw))
    client_trace.unlink(missing_ok=True)
    server_trace.unlink(missing_ok=True)
    record = {"label": label, "started_unix": started, "events": events,
              "trace": f"{stem}.trace.jsonl", "trace_events": raw}
    (artifact / f"{stem}.json").write_text(json.dumps(record, indent=2) + "\n")
    return [dict(label=label, **event) for event in events]


def main() -> int:
    repo = Path(__file__).resolve().parents[1]
    nvim = shutil.which("nvim")
    server = os.environ.get("MAGIT_LSP_BIN", str(repo / "target/debug/magit-lsp"))
    if not nvim or not Path(server).is_file():
        raise SystemExit(f"required executable missing: nvim={nvim!r}, server={server}")
    artifact = repo / "build/startup-diagnosis" / datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    artifact.mkdir(parents=True, exist_ok=False)
    fixture = artifact / "fixture"
    fixture.mkdir()
    subprocess.run(["git", "init", "-q", str(fixture)], check=True)
    (fixture / "fixture.txt").write_text("diagnostic fixture\n")
    subprocess.run(["git", "-C", str(fixture), "add", "."], check=True)
    rows = []
    for root, label in ((fixture, "fixture"), (repo, "worktree")):
        for _ in range(3):
            rows.extend(run_sample(repo, root, nvim, server, artifact, label))
    distributions = {}
    for case in ("process_cold_first_open", "initialized_server_reopen", "already_open_focus"):
        for label in ("fixture", "worktree"):
            values = [r["elapsed_ms"] for r in rows if r["case"] == case and r["label"] == label]
            distributions[f"{label}.{case}"] = {"samples_ms": values, "median_ms": statistics.median(values),
                                                   "range_ms": [min(values), max(values)]}
    metadata = {"nvim": nvim, "nvim_version": subprocess.check_output([nvim, "--version"], text=True).splitlines()[0],
                "server": str(Path(server).resolve()), "profile": "debug (Makefile MAGIT_LSP_BIN)",
                "host": subprocess.check_output(["uname", "-a"], text=True).strip(),
                "python": os.sys.version, "fixture": str(fixture), "worktree": str(repo),
                "samples_per_repository": 3,
                "definitions": {"process_cold_first_open": "fresh Neovim and server process to visible Head status; filesystem cache not cold",
                                "initialized_server_reopen": "same Neovim/server after status buffer wipe; view reopened",
                                "already_open_focus": "same status view, public command focus only"},
                "distributions": distributions, "raw_json": "*.json", "merged_trace": "*.trace.jsonl"}
    (artifact / "summary.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(json.dumps(metadata, indent=2))
    print(f"artifacts: {artifact}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
