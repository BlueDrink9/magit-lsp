#!/usr/bin/env python3
"""Statically check Rust test premises for Windows portability.

Two rules apply to `tests/**/*.rs` and `src/**/*.rs`:

1. Every `canonicalize` call is Unix-only (its file or enclosing item carries
   `#[cfg(unix)]` or `#![cfg(unix)]`) or a nearby `windows-ok: <reason>`
   comment explains why comparing against `std::fs::canonicalize` is safe.
2. String literals passed to `.join(…)`, `.with_file_name(…)`, `.rename(…)` and
   the tempdir `.prefix(…)` builder are valid Windows path segments. The
   device-name, reserved-character and trailing-dot/space tables are parsed
   from `windows_path_problem` in `tests/common/mod.rs`, so the runtime guard
   and this checker share one definition rather than drifting apart.

An item annotated `#[should_panic(expected = "windows-reserved…")]` is testing
the guard itself, so rule 2 suppresses it (as does a `windows-ok:` comment).

Run by `make check-test-premises`, which first runs `--self-test` to plant one
violation per rule in a temporary tree. Standard library only.
"""

import argparse
import re
import sys
import tempfile
from pathlib import Path

NAME_TABLE_SOURCE = Path('tests/common/mod.rs')
SCAN_ROOTS = (Path('tests'), Path('src'))
NAME_METHODS = ('join', 'with_file_name', 'rename', 'prefix')
WINDOWS_OK = re.compile(r'windows-ok\s*:\s*\S')
SHOULD_PANIC = 'should_panic(expected = "windows-reserved'
INTERNAL_ERROR = 2


class CheckError(Exception):
    """One failed check, reported without a traceback."""


class NameRules:
    """The Windows filename rules parsed from `windows_path_problem`."""

    def __init__(self, devices, reserved, trailing_dot, trailing_space):
        self.devices = tuple(devices)
        self.reserved = frozenset(reserved)
        self.trailing_dot = trailing_dot
        self.trailing_space = trailing_space


def parse_name_rules(source):
    """Read the name tables from `windows_path_problem` in `source`."""
    try:
        text = source.read_text(encoding='utf-8')
    except OSError as error:
        raise CheckError(f'cannot read {source}: {error}') from error
    device_block = re.search(
        r'WINDOWS_DEVICE_NAMES[^=]*=\s*&\[(.*?)\];', text, re.S
    )
    if device_block is None:
        raise CheckError(f'{source}: cannot find WINDOWS_DEVICE_NAMES')
    devices = re.findall(r'"([^"\\]+)"', device_block.group(1))
    match = re.search(r'matches!\(c,\s*([^)]*)\)', text)
    if match is None:
        raise CheckError(f'{source}: cannot find the reserved-character match')
    reserved = re.findall(r"'(\\.|[^'\\])'", match.group(1))
    trailing = re.findall(r"ends_with\('(.)'\)", text)
    if not devices or not reserved or 'starts_with(r"\\\\?\\")' not in text:
        raise CheckError(f'{source}: Windows name tables have an unexpected shape')
    return NameRules(
        devices,
        reserved,
        '.' in trailing,
        ' ' in trailing,
    )


def mask_comments(text):
    """Blank Rust comments, preserving newlines and string contents."""
    out = list(text)
    index = 0
    length = len(text)

    def blank(start, end):
        for position in range(start, end):
            if out[position] != '\n':
                out[position] = ' '

    while index < length:
        if text.startswith('//', index):
            end = text.find('\n', index)
            end = length if end == -1 else end
            blank(index, end)
            index = end
        elif text.startswith('/*', index):
            depth = 1
            end = index + 2
            while end < length and depth:
                if text.startswith('/*', end):
                    depth += 1
                    end += 2
                elif text.startswith('*/', end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            blank(index, end)
            index = end
        else:
            index += 1
    return ''.join(out)


def mask_strings(text):
    """Blank the contents of string and character literals."""
    out = list(text)
    index = 0
    length = len(text)

    def blank(start, end):
        for position in range(start, end):
            if out[position] != '\n':
                out[position] = ' '

    while index < length:
        if text[index] == 'r':
            raw = re.match(r'r(#*)"', text[index:])
            if raw is None:
                index += 1
                continue
            hashes = raw.group(1)
            start = index + 2 + len(hashes)
            end = text.find('"' + hashes, start)
            end = length if end == -1 else end
            blank(start, end)
            index = end + 1 + len(hashes)
        elif text[index] == '"':
            start = index + 1
            end = start
            while end < length:
                if text[end] == '\\':
                    end += 2
                elif text[end] == '"':
                    break
                else:
                    end += 1
            blank(start, end)
            index = end + 1
        elif text[index] == "'" and not re.match(r"'(?:\\.|[^'\\])'", text[index:]):
            # A lifetime, not a character literal.
            index += 1
        elif text[index] == "'":
            start = index + 1
            end = start
            while end < length:
                if text[end] == '\\':
                    end += 2
                elif text[end] == "'":
                    break
                else:
                    end += 1
            blank(start, end)
            index = end + 1
        else:
            index += 1
    return ''.join(out)


NEGATION = re.compile(r'(?<![\w])not\s*\(')


def strip_negations(expression):
    """Blank every `not(…)` group so only affirmative terms remain."""
    out = list(expression)
    index = 0
    while True:
        match = NEGATION.search(expression, index)
        if match is None:
            return ''.join(out)
        depth = 1
        end = match.end()
        while end < len(expression) and depth:
            if expression[end] == '(':
                depth += 1
            elif expression[end] == ')':
                depth -= 1
            end += 1
        for position in range(match.start(), end):
            out[position] = ' '
        index = end


def cfg_is_unix_only(expression):
    """Whether a `cfg(...)` expression only compiles off Windows."""
    if re.search(r'\bnot\s*\(\s*windows\s*\)', expression):
        return True
    affirmative = strip_negations(expression)
    return bool(re.search(r'\bunix\b', affirmative)) and 'windows' not in affirmative


def item_attributes(line):
    """Return the cfg/unix gate and should-panic flag declared on `line`."""
    gated = False
    suppressed = False
    match = re.search(r'#\s*\[\s*cfg\s*\((.*?)\)\s*\]', line)
    if match:
        gated = cfg_is_unix_only(match.group(1))
    if SHOULD_PANIC in line:
        suppressed = True
    return gated, suppressed


def scan_scopes(lines):
    """Map each line to (gated, suppressed) for its enclosing items."""
    code = mask_strings('\n'.join(lines))
    raw = mask_comments('\n'.join(lines)).split('\n')
    code_lines = code.split('\n')
    scopes = []
    gated_lines = {}
    suppressed_lines = {}
    depth = 0
    pending_gate = False
    pending_suppress = False
    pending_active = False
    item_gate = False
    item_suppress = False
    for number, code_line in enumerate(code_lines):
        raw_line = raw[number] if number < len(raw) else ''
        stripped = code_line.strip()
        if stripped.startswith('#'):
            gate, suppress = item_attributes(raw_line)
            pending_gate = pending_gate or gate
            pending_suppress = pending_suppress or suppress
            pending_active = True
            continue
        if pending_active and stripped:
            item_gate = pending_gate
            item_suppress = pending_suppress
            pending_gate = pending_suppress = pending_active = False
        active = any(gate for _, gate, _ in scopes) or item_gate
        hidden = any(suppress for _, _, suppress in scopes) or item_suppress
        gated_lines[number + 1] = active
        suppressed_lines[number + 1] = hidden
        for character in code_line:
            if character == '{':
                depth += 1
                if item_gate or item_suppress:
                    scopes.append((depth, item_gate, item_suppress))
                    item_gate = item_suppress = False
            elif character == '}':
                if scopes and scopes[-1][0] == depth:
                    scopes.pop()
                depth -= 1
        if item_gate or item_suppress:
            if ';' in code_line:
                item_gate = item_suppress = False
    return gated_lines, suppressed_lines


def module_declarations(path, lines):
    """Yield (module name, optional `#[path]`, attribute block) declarations."""
    attribute_lines = []
    for line in mask_comments('\n'.join(lines)).split('\n'):
        stripped = line.strip()
        if stripped.startswith('#['):
            attribute_lines.append(stripped)
            continue
        match = re.match(r'(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;', stripped)
        if match:
            yield match.group(1), attribute_lines
            attribute_lines = []
        elif stripped and not stripped.startswith('//'):
            attribute_lines = []


def resolve_module(path, name, attributes, crate_roots):
    """Return the file a `mod name;` declaration loads, if it exists."""
    explicit = None
    for attribute in attributes:
        match = re.search(r'#\[path\s*=\s*"([^"]+)"\]', attribute)
        if match:
            explicit = match.group(1)
            break
    if explicit:
        return (path.parent / explicit).resolve()
    if path.resolve() in crate_roots or path.name in ('lib.rs', 'main.rs'):
        base = path.parent
    else:
        base = path.with_suffix('')
    for candidate in (base / f'{name}.rs', base / name / 'mod.rs'):
        if candidate.is_file():
            return candidate.resolve()
    return None


def crates_roots(files):
    """Files that Rust treats as crate roots among the scanned files."""
    return {
        path.resolve()
        for path in files
        if path.parent.name == 'tests'
        or (path.parent.name == 'src' and path.name in ('lib.rs', 'main.rs'))
    }


def module_gates(files):
    """Resolve which scanned files are Unix-only through module inclusion."""
    roots = crates_roots(files)
    parents = {}
    declared_gate = {}
    for path in files:
        try:
            lines = path.read_text(encoding='utf-8').split('\n')
        except OSError:
            continue
        header = re.search(r'#!\[cfg\((.*?)\)\]', '\n'.join(lines))
        declared_gate[path.resolve()] = header is not None and cfg_is_unix_only(
            header.group(1)
        )
        for name, attributes in module_declarations(path, lines):
            target = resolve_module(path, name, attributes, roots)
            if target is None:
                continue
            gate = False
            for attribute in attributes:
                match = re.search(r'#\[cfg\((.*?)\)\]', attribute)
                if match and cfg_is_unix_only(match.group(1)):
                    gate = True
            parents.setdefault(target, []).append((path.resolve(), gate))
    gates = dict(declared_gate)
    changed = True
    while changed:
        changed = False
        for path, entries in parents.items():
            if gates.get(path):
                continue
            if any(gate or gates.get(parent) for parent, gate in entries):
                gates[path] = True
                changed = True
    return gates


def windows_ok(lines, number):
    """Whether a `windows-ok:` comment sits on or just above the line."""
    for offset in range(0, 6):
        index = number - 1 - offset
        if index < 0:
            break
        if WINDOWS_OK.search(lines[index]):
            return True
    return False


def name_problem(text, rules):
    """Return why the literal `text` is not a portable Windows name."""
    if text.startswith('\\\\?\\'):
        return 'verbatim `\\\\?\\` paths are rejected by Git for Windows'
    if not any(character.isalnum() for character in text):
        # A punctuation-only literal is a display/join separator, not a name.
        return None
    for index, segment in enumerate(re.split(r'[/\\]', text)):
        if index == 0:
            drive = re.fullmatch(r'[A-Za-z]:', segment)
            if not segment or drive:
                continue
        if any(character in rules.reserved for character in segment):
            return 'windows-reserved filename character in path segment'
        stem = segment.split('.')[0]
        if any(device.lower() == stem.lower() for device in rules.devices):
            return 'windows-reserved device name in path segment'
        if (
            (rules.trailing_dot and segment.endswith('.'))
            or (rules.trailing_space and segment.endswith(' '))
        ):
            return 'windows-reserved trailing dot or space in path segment'
    return None


def literal_arguments(text, start):
    """Split the arguments of a call at `start` into literal string values.

    Returns an empty list when any argument is not a plain string literal;
    format strings and computed names are left to the runtime guard.
    """
    depth = 1
    index = start
    arguments = []
    current = []
    while index < len(text) and depth:
        character = text[index]
        if character == '"':
            end = index + 1
            while end < len(text):
                if text[end] == '\\':
                    end += 2
                elif text[end] == '"':
                    end += 1
                    break
                else:
                    end += 1
            current.append(text[index:end])
            index = end
            continue
        if character == '(':
            depth += 1
        elif character == ')':
            depth -= 1
            if depth == 0:
                arguments.append(''.join(current))
                break
        if character == ',' and depth == 1:
            arguments.append(''.join(current))
            current = []
        else:
            current.append(character)
        index += 1
    else:
        return []
    values = []
    for argument in arguments:
        argument = argument.strip()
        match = re.fullmatch(r'"((?:[^"\\]|\\.)*)"', argument)
        if match is None or '\\' in match.group(1):
            return []
        values.append(match.group(1))
    return values


class Violation:
    """One reported premise violation."""

    def __init__(self, path, line, rule, message):
        self.path = path
        self.line = line
        self.rule = rule
        self.message = message

    def __str__(self):
        return f'{self.path}:{self.line}: rule {self.rule}: {self.message}'


def scan_file(path, rules, gated_files):
    """Return violations in one Rust source file."""
    try:
        text = path.read_text(encoding='utf-8')
    except OSError as error:
        raise CheckError(f'cannot read {path}: {error}') from error
    lines = text.split('\n')
    code = mask_strings(mask_comments(text))
    gated_lines, suppressed_lines = scan_scopes(lines)
    file_gated = gated_files.get(path.resolve(), False)
    violations = []
    for match in re.finditer(r'(?<![\w])canonicalize\b', code):
        number = code.count('\n', 0, match.start()) + 1
        if file_gated or gated_lines.get(number) or windows_ok(lines, number):
            continue
        violations.append(
            Violation(
                path,
                number,
                1,
                'canonicalize is only portable when Unix-gated or annotated '
                '`windows-ok: <reason>`',
            )
        )
    calls = re.compile(
        r'\.(' + '|'.join(NAME_METHODS) + r')\s*\('
    )
    masked = mask_comments(text)
    for match in calls.finditer(masked):
        number = masked.count('\n', 0, match.start()) + 1
        if suppressed_lines.get(number) or windows_ok(lines, number):
            continue
        values = literal_arguments(masked, match.end())
        for value in values:
            reason = name_problem(value, rules)
            if reason:
                violations.append(
                    Violation(path, number, 2, f'{match.group(1)}({value!r}): {reason}')
                )
    return violations


def rust_files(root):
    """All scanned Rust sources below `root`, in stable order."""
    files = []
    for base in SCAN_ROOTS:
        directory = root / base
        if directory.is_dir():
            files.extend(sorted(directory.rglob('*.rs')))
    return files


def audit(root, rules_source=NAME_TABLE_SOURCE):
    """Return violations for the tree at `root`."""
    source = root / rules_source
    if not source.is_file():
        # The self-test tree carries a minimal table source of its own.
        source = rules_source
    rules = parse_name_rules(source)
    files = rust_files(root)
    gates = module_gates(files)
    return [violation for path in files for violation in scan_file(path, rules, gates)]


SELF_TEST_SOURCE = '''
const WINDOWS_DEVICE_NAMES: &[&str] = &["CON", "NUL", "COM1"];

pub fn windows_path_problem(text: &str) -> Option<&'static str> {
    if text.starts_with(r"\\\\?\\") {
        return Some("verbatim");
    }
    for segment in text.split('/') {
        if segment
            .chars()
            .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        {
            return Some("reserved");
        }
        let stem = segment.split('.').next().unwrap_or(segment);
        if WINDOWS_DEVICE_NAMES.iter().any(|device| device.eq_ignore_ascii_case(stem)) {
            return Some("device");
        }
        if segment.ends_with('.') || segment.ends_with(' ') {
            return Some("trailing");
        }
    }
    None
}
'''

SELF_TEST_PROBE = '''
fn ungated() {
    let _ = std::fs::canonicalize("probe");
}

#[cfg(unix)]
fn unix_only() {
    let _ = std::fs::canonicalize("probe");
}

// windows-ok: the comparison below is our-form to our-form on every platform.
fn annotated() {
    let _ = std::fs::canonicalize("probe");
}

fn reserved_join(dir: std::path::PathBuf) {
    let _ = dir.join("bad*name.txt");
}

fn reserved_rename(fixture: Fixture) {
    let _ = fixture.rename("from.txt", "bad:name.txt");
}

#[should_panic(expected = "windows-reserved filename character")]
fn guard_self_test(fixture: Fixture) {
    let _ = fixture.rename("from.txt", "bad*name.txt");
}

#[cfg(not(unix))]
fn negated_gate() {
    let _ = std::fs::canonicalize("probe");
}

fn trailing_segment(dir: std::path::PathBuf) {
    let _ = dir.join("trailing ");
}
'''

SELF_TEST_MODULE_SOURCE = '''
#[cfg(unix)]
mod menu;
'''

SELF_TEST_DECOY = '''
pub fn decoy() {
    let _ = std::fs::canonicalize("decoy");
}
'''

SELF_TEST_REAL_CHILD = '''
pub fn real() {
    let _ = std::fs::canonicalize("real");
}
'''


def self_test():
    """Plant one violation per rule and confirm the checker reports it."""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        common = root / 'tests' / 'common'
        common.mkdir(parents=True)
        (common / 'mod.rs').write_text(SELF_TEST_SOURCE, encoding='utf-8')
        (root / 'tests' / 'probe.rs').write_text(SELF_TEST_PROBE, encoding='utf-8')
        source = root / 'src'
        source.mkdir()
        (source / 'session.rs').write_text(SELF_TEST_MODULE_SOURCE, encoding='utf-8')
        (source / 'menu.rs').write_text(SELF_TEST_DECOY, encoding='utf-8')
        (source / 'session').mkdir()
        (source / 'session' / 'menu.rs').write_text(SELF_TEST_REAL_CHILD, encoding='utf-8')
        violations = audit(root)
        found = {
            (violation.path.relative_to(root).as_posix(), violation.rule, violation.line)
            for violation in violations
        }
        expected = {
            ('tests/probe.rs', 1, 3),
            ('tests/probe.rs', 2, 17),
            ('tests/probe.rs', 2, 21),
            ('tests/probe.rs', 1, 31),
            ('tests/probe.rs', 2, 35),
            ('src/menu.rs', 1, 3),
        }
        if found != expected:
            for violation in violations:
                print(violation, file=sys.stderr)
            print(
                f'self-test: expected violations at {sorted(expected)}, '
                f'found {sorted(found)}',
                file=sys.stderr,
            )
            return 1
        suppressed = [
            violation
            for violation in violations
            if (violation.path.relative_to(root).as_posix(), violation.line)
            in {
                ('tests/probe.rs', 8),
                ('tests/probe.rs', 13),
                ('tests/probe.rs', 26),
                ('src/session/menu.rs', 3),
            }
        ]
        if suppressed:
            for violation in suppressed:
                print(violation, file=sys.stderr)
            print('self-test: cfg gate, windows-ok or should_panic did not suppress', file=sys.stderr)
            return 1
        print('ok: self-test planted violations in both rules and suppressed the annotations')
        return 0


def main(argv):
    """Run the self-test or audit the repository."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true', help='plant and detect fixture violations')
    arguments = parser.parse_args(argv)
    try:
        if arguments.self_test:
            return self_test()
        violations = audit(Path('.'))
    except CheckError as error:
        print(f'check-test-premises: {error}', file=sys.stderr)
        return INTERNAL_ERROR
    if violations:
        for violation in sorted(violations, key=lambda item: (str(item.path), item.line)):
            print(violation, file=sys.stderr)
        print(f'check-test-premises: {len(violations)} violation(s)', file=sys.stderr)
        return 1
    print('ok: Rust test premises are Windows-portable')
    return 0


if __name__ == '__main__':
    raise SystemExit(main(sys.argv[1:]))
