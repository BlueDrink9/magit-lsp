"""Command line for `scripts/ui` (spec §A.1)."""

from __future__ import annotations

import os
import sys

from . import driver, scenario

USAGE = """Usage:
  scripts/ui start <fixture> [--size WxH] [--profile standard|vim] [--ansi|--truecolor]
  scripts/ui keys <key>... [--run <id>]
  scripts/ui wait [--ms N] [--run <id>]
  scripts/ui screen [--ansi] [--run <id>]
  scripts/ui state [--run <id>]
  scripts/ui trace [N] [--run <id>]
  scripts/ui stop [--run <id>]
  scripts/ui run <scenario>... [--markdown <path>]

Fixtures: clean, modified-two-files, staged-one-file, staged-and-unstaged,
          untracked, unborn, in-progress-rebase, in-progress-cherry-pick.
Without --run the most recent run is used."""


def _in_ci() -> bool:
    value = os.environ.get("CI", "").strip().lower()
    return value not in ("", "0", "false", "no")


def _take_run(args: list[str]) -> tuple[str | None, list[str]]:
    """Remove `--run <id>` / `--run=<id>` and return its value."""
    run_id = None
    rest: list[str] = []
    index = 0
    while index < len(args):
        arg = args[index]
        if arg == "--run":
            if index + 1 >= len(args):
                raise driver.DriverError("`--run` needs an id")
            run_id = args[index + 1]
            index += 2
            continue
        if arg.startswith("--run="):
            run_id = arg.split("=", 1)[1]
            index += 1
            continue
        rest.append(arg)
        index += 1
    return run_id, rest


def _take_value(args: list[str], flag: str, default: str | None = None) -> tuple[str | None, list[str]]:
    """Remove `--flag value` or `--flag=value`."""
    value = default
    rest: list[str] = []
    index = 0
    while index < len(args):
        arg = args[index]
        if arg == flag:
            if index + 1 >= len(args):
                raise driver.DriverError(f"`{flag}` needs a value")
            value = args[index + 1]
            index += 2
            continue
        if arg.startswith(flag + "="):
            value = arg.split("=", 1)[1]
            index += 1
            continue
        rest.append(arg)
        index += 1
    return value, rest


def _start(args: list[str]) -> int:
    size, args = _take_value(args, "--size", "100x30")
    profile, args = _take_value(args, "--profile", driver.DEFAULT_PROFILE)
    color = "ansi"
    positional: list[str] = []
    for arg in args:
        if arg == "--ansi":
            color = "ansi"
        elif arg == "--truecolor":
            color = "truecolor"
        elif arg.startswith("-"):
            raise driver.DriverError(f"unknown option `{arg}`")
        else:
            positional.append(arg)
    if len(positional) != 1:
        raise driver.DriverError("`start` needs exactly one fixture")
    run = driver.start(positional[0], size=size, profile=profile, color=color)
    print(run.id)
    return 0


def _keys(args: list[str]) -> int:
    run_id, tokens = _take_run(args)
    if not tokens:
        raise driver.DriverError("`keys` needs at least one key")
    run = driver.load_run(run_id)
    driver.send_keys(run, tokens)
    return 0


def _wait(args: list[str]) -> int:
    run_id, args = _take_run(args)
    value, args = _take_value(args, "--ms", "2000")
    if args:
        raise driver.DriverError(f"unexpected argument `{args[0]}`")
    try:
        timeout_ms = int(value or "2000")
    except ValueError as error:
        raise driver.DriverError(f"invalid --ms `{value}`") from error
    run = driver.load_run(run_id)
    if driver.wait_idle(run, timeout_ms=timeout_ms):
        return 0
    print(f"timed out after {timeout_ms} ms", file=sys.stderr)
    try:
        print(driver.state_json(run), file=sys.stderr)
    except driver.DriverError as error:
        print(str(error), file=sys.stderr)
    return 2


def _screen(args: list[str]) -> int:
    run_id, args = _take_run(args)
    ansi = False
    for arg in args:
        if arg == "--ansi":
            ansi = True
        else:
            raise driver.DriverError(f"unexpected argument `{arg}`")
    run = driver.load_run(run_id)
    print(driver.capture(run, ansi=ansi))
    return 0


def _state(args: list[str]) -> int:
    run_id, args = _take_run(args)
    if args:
        raise driver.DriverError(f"unexpected argument `{args[0]}`")
    run = driver.load_run(run_id)
    print(driver.state_json(run))
    return 0


def _trace(args: list[str]) -> int:
    run_id, args = _take_run(args)
    count: int | None = None
    if len(args) > 1:
        raise driver.DriverError("`trace` takes at most one count")
    if args:
        try:
            count = int(args[0])
        except ValueError as error:
            raise driver.DriverError(f"invalid count `{args[0]}`") from error
    else:
        count = 10
    run = driver.load_run(run_id)
    text = driver.format_trace(driver.read_trace(run), count)
    print(text if text else "(no trace events)")
    return 0


def _stop(args: list[str]) -> int:
    run_id, args = _take_run(args)
    if args:
        raise driver.DriverError(f"unexpected argument `{args[0]}`")
    driver.stop(driver.load_run(run_id))
    return 0


def _run(args: list[str]) -> int:
    markdown, args = _take_value(args, "--markdown")
    if not args:
        raise driver.DriverError("`run` needs at least one scenario file")
    for arg in args:
        if arg.startswith("-"):
            raise driver.DriverError(f"unknown option `{arg}`")
    return scenario.run(args, markdown=markdown)


_COMMANDS = {
    "start": _start,
    "keys": _keys,
    "wait": _wait,
    "screen": _screen,
    "state": _state,
    "trace": _trace,
    "stop": _stop,
    "run": _run,
}


def main(argv: list[str] | None = None) -> int:
    """Run one `scripts/ui` command."""
    args = list(sys.argv[1:] if argv is None else argv)
    if not args or args[0] in ("-h", "--help", "help"):
        print(USAGE)
        return 0 if args else 2
    command = _COMMANDS.get(args[0])
    if command is None:
        print(f"unknown command `{args[0]}`\n\n{USAGE}", file=sys.stderr)
        return 2
    try:
        return command(args[1:])
    except driver.SkipError as error:
        print(f"SKIP: {error}")
        return 1 if _in_ci() else 0
    except driver.DriverError as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
