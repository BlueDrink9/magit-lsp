"""Unit tests for the phase-aware trace wait in the startup measurement."""

from __future__ import annotations

import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import measure_nvim_startup  # noqa: E402

STATUS_PHASES = ("phase_status_listing", "phase_rebase_state")


def write_jsonl(path: Path, records: list[dict], newline: bool = True) -> None:
    text = "".join(json_line(record) for record in records)
    path.write_text(text if newline else text.rstrip("\n"), encoding="utf-8")


def json_line(record: dict) -> str:
    import json

    return json.dumps(record) + "\n"


class DeferredStatusWaitTest(unittest.TestCase):
    def traces(self, directory: Path) -> tuple[Path, Path]:
        client = directory / "sample.client.jsonl"
        server = directory / "sample.client.jsonl.server"
        write_jsonl(client, [{"event": "view_open_requested", "id": 1}])
        early = [event for event in measure_nvim_startup.REQUIRED_PHASES if event not in STATUS_PHASES]
        write_jsonl(server, [{"event": event, "id": 1} for event in early])
        # The status phase record is in flight: written, not yet terminated.
        in_flight = json_line({"event": STATUS_PHASES[0], "id": 1}).rstrip("\n")
        server.write_text(server.read_text() + in_flight, encoding="utf-8")
        return client, server

    def finish_status(self, path: Path) -> None:
        with path.open("a", encoding="utf-8") as handle:
            handle.write("\n" + json_line({"event": STATUS_PHASES[1], "id": 1}))

    def test_readiness_waits_for_deferred_status_phases_and_final_newline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            client, server = self.traces(Path(directory))
            issues, missing = measure_nvim_startup.trace_readiness(client, server)
            self.assertIn("server trace lacks a complete final line", issues)
            self.assertEqual(missing, {"1": sorted(STATUS_PHASES)})

            self.finish_status(server)
            issues, missing = measure_nvim_startup.trace_readiness(client, server)
            self.assertEqual((issues, missing), ([], {}))

    def test_wait_keeps_polling_until_deferred_status_arrives(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            client, server = self.traces(Path(directory))
            arrived = threading.Event()

            def finish_status() -> None:
                time.sleep(0.2)
                self.finish_status(server)
                arrived.set()

            writer = threading.Thread(target=finish_status)
            writer.start()
            started = time.monotonic()
            try:
                measure_nvim_startup.wait_for_traces_ready(client, server, poll_s=0.01, timeout_s=10.0)
            finally:
                writer.join()
            self.assertGreaterEqual(time.monotonic() - started, 0.15, "wait returned before status arrived")
            self.assertTrue(arrived.is_set(), "wait returned before the deferred status was written")

    def test_timeout_names_trace_path_and_missing_phases(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            client, server = self.traces(Path(directory))
            with self.assertRaises(RuntimeError) as raised:
                measure_nvim_startup.wait_for_traces_ready(client, server, poll_s=0.01, timeout_s=0.05)
            message = str(raised.exception)
            self.assertIn(str(server), message)
            self.assertIn("phase_status_listing", message)
            self.assertIn("phase_rebase_state", message)


if __name__ == "__main__":
    unittest.main()
