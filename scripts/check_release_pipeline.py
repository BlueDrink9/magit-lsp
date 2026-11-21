#!/usr/bin/env python3
"""Run the release workflow's locked build and actual-artifact checks locally.

``build`` refuses any compiler that is not exactly the pinned Rust toolchain and
then runs the workflow's ``--release --locked --target`` build; ``build
--local`` instead uses the installed compiler without downloading one and
records its real versions, while keeping the same locked command and artifact
path. ``build-native`` is the experimental hosted native-linker lane: it accepts
only the matching Linux x86_64, Darwin arm64 or Darwin x86_64 host and target.
Linux points Cargo's target linker explicitly at ``/usr/bin/cc``; Darwin resolves
Apple Clang, ld and the Xcode macOS SDK through the system ``/usr/bin/xcrun`` and
sets an explicit deployment target. Both record the exact locked argv plus
compiler, linker, SDK/libc and source/lock provenance under ``build/release/``. It
never changes or replaces the strict pinned/``--local`` commands. ``artifacts``
packages that exact binary with ``scripts/package-release.sh``, validates the
archive/sidecar set offline with the release checker and runs the release suite
with ``MAGIT_LSP_TEST_BINARY`` set, so the real-binary case runs instead of
skipping. ``.github/workflows/release.yml`` invokes this same script, so the
local targets are the workflow's steps rather than a parallel implementation.

A missing pinned compiler fails with a readable message; this script never
downloads or installs a toolchain and never publishes anything.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parent.parent
TOOLCHAIN = '1.94.1'
TARGET = 'x86_64-unknown-linux-gnu'
BINARY = Path('target') / TARGET / 'release' / 'magit-lsp'
DIST = Path('dist')
VALIDATOR = Path('scripts/check_release_automation.py')
VERSION_PATTERN = re.compile(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)')
# The experimental native lane builds only on the matching native host, never a
# cross or Nix-wrapped toolchain. Linux uses the explicit host compiler; Darwin
# resolves Apple Clang, ld and the Xcode SDK through the system xcrun and pins
# the deployment target per architecture.
NATIVE_HOST = ('linux', 'x86_64')
NATIVE_CC = '/usr/bin/cc'
NATIVE_LINKER_VARIABLE = 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER'
NATIVE_XCRUN = '/usr/bin/xcrun'
# Rust's documented macOS minimums are only proposed build targets. The hosted
# build (NREL-17) still has to inspect the produced load commands, so these are
# not proof that an older runtime is supported or was tested.
DARWIN_DEPLOYMENT_TARGETS = {
    'aarch64-apple-darwin': '11.0',
    'x86_64-apple-darwin': '10.12',
}
DEPLOYMENT_TARGET_SOURCE = (
    'Proposed build targets from Rust documented minima: macOS 10.12+ for '
    'x86_64 and 11.0+ for arm64 (docs/RELEASING.md). These are not proof of the '
    'final supported macOS versions; hosted inspection is pending NREL-17.')
DEPLOYMENT_TARGET_STATUS = (
    'proposed build target, not a tested or final macOS support baseline; '
    'hosted build evidence is pending NREL-17 and load-command/deployment '
    'verification belongs to NREL-12')
NATIVE_TARGETS = {
    TARGET: {
        'host': NATIVE_HOST,
        'host_label': 'Linux x86_64',
        'linker_variable': NATIVE_LINKER_VARIABLE,
        'linker': NATIVE_CC,
    },
    'aarch64-apple-darwin': {
        'host': ('darwin', 'arm64'),
        'host_label': 'Darwin arm64',
        'linker_variable': 'CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER',
        'deployment_target': DARWIN_DEPLOYMENT_TARGETS['aarch64-apple-darwin'],
    },
    'x86_64-apple-darwin': {
        'host': ('darwin', 'x86_64'),
        'host_label': 'Darwin x86_64',
        'linker_variable': 'CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER',
        'deployment_target': DARWIN_DEPLOYMENT_TARGETS['x86_64-apple-darwin'],
    },
}
NATIVE_EVIDENCE_DIR = Path('build/release')
NATIVE_EVIDENCE_FILE = 'native-build.json'
# Inherited settings that would inject a C compiler, linker, sysroot or
# deployment target from a Nix dev shell or a caller wrapper. The native lane
# refuses them instead of trusting flags the release shell happened to export.
NATIVE_INJECTION_VARIABLES = (
    'CC', 'CXX', 'AR', 'CFLAGS', 'CPPFLAGS', 'LDFLAGS',
    'NIX_CC', 'NIX_CFLAGS_COMPILE', 'NIX_LDFLAGS', 'SDKROOT',
    'MACOSX_DEPLOYMENT_TARGET', 'DYLD_LIBRARY_PATH', 'DYLD_FALLBACK_LIBRARY_PATH',
)


class PipelineError(Exception):
    """One failed release pipeline step, reported without a traceback."""


def build_command(cargo, target=TARGET):
    """Return the lockstep build argv for a ``cargo`` prefix and ``target``."""
    return list(cargo) + ['build', '--release', '--locked', '--target', target]


def artifact_commands(version, binary, dist):
    """Return the ordered package/validate/test step argv list and their environment.

    ``binary`` is the actual release binary and ``dist`` the output directory.
    The environment carries the ``MAGIT_LSP_TEST_BINARY`` handoff the release
    suite needs to exercise packaging, installation and launch instead of
    skipping, plus bytecode suppression.
    """
    steps = [
        ['sh', 'scripts/package-release.sh', version, str(binary), str(dist)],
        [sys.executable, str(VALIDATOR), '--validate-artifacts', str(dist)],
        [sys.executable, '-m', 'unittest', 'discover', '-s', 'tests/release',
         '-p', 'test_*.py', '-v'],
    ]
    environment = {
        'MAGIT_LSP_TEST_BINARY': str(binary),
        'PYTHONDONTWRITEBYTECODE': '1',
    }
    return steps, environment


def manifest_version():
    """Return the shared Cargo/extension version, refusing a malformed or split pair."""
    package = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf-8'))['package']
    version = package.get('version', '')
    if not VERSION_PATTERN.fullmatch(version):
        raise PipelineError(f'Cargo.toml version {version!r} is not a stable X.Y.Z version')
    extension = json.loads(
        (ROOT / 'clients/vscode/package.json').read_text(encoding='utf-8'))['version']
    if extension != version:
        raise PipelineError(
            f'Cargo.toml {version} and extension package.json {extension} versions differ')
    return version


def resolve_version(environment):
    """Return the packaging version, refusing a broken CI handoff.

    The build workflow's earlier ``--validate-release-ref`` step writes
    ``RELEASE_VERSION`` to ``$GITHUB_ENV``, so in CI the value must be present
    and match the manifests; locally it is absent and the manifests decide.
    """
    version = manifest_version()
    supplied = environment.get('RELEASE_VERSION')
    if environment.get('GITHUB_ACTIONS') and not supplied:
        raise PipelineError(
            'RELEASE_VERSION is missing; the --validate-release-ref step must publish it')
    if supplied and supplied.removeprefix('v') != version:
        raise PipelineError(
            f'RELEASE_VERSION {supplied} does not match the manifest version {version}')
    return version


def process_environment(environment):
    """Return ``environment`` layered over this process's environment."""
    return dict(os.environ, **(environment or {}))


def compiler(environment):
    """Return the cargo prefix for exactly the pinned toolchain, or fail readably."""
    found = 'no cargo/rustc on PATH'
    merged = process_environment(environment)
    if shutil.which('cargo', path=merged.get('PATH')) and \
            shutil.which('rustc', path=merged.get('PATH')):
        probe = subprocess.run(
            ['cargo', f'+{TOOLCHAIN}', '--version'], cwd=ROOT, env=merged,
            capture_output=True, text=True)
        if probe.returncode == 0:
            return ['cargo', f'+{TOOLCHAIN}']
        version = subprocess.run(
            ['rustc', '--version'], cwd=ROOT, env=merged,
            capture_output=True, text=True)
        found = version.stdout.strip() or version.stderr.strip() or found
        if version.returncode == 0 and version.stdout.split()[1:2] == [TOOLCHAIN]:
            return ['cargo']
    raise PipelineError(
        f'the pinned release build needs Rust {TOOLCHAIN}; found {found}. '
        f'Provide that exact toolchain and target {TARGET} (for example with '
        f'`rustup toolchain install {TOOLCHAIN}`); this target never downloads '
        'or installs a toolchain itself.')


def installed_prefix(environment):
    """Return the installed ``cargo`` prefix without downloading or pinning one."""
    merged = process_environment(environment)
    missing = [tool for tool in ('cargo', 'rustc')
               if not shutil.which(tool, path=merged['PATH'])]
    if missing:
        raise PipelineError(
            f'the local release build needs installed {" and ".join(missing)} on PATH; '
            'this target never downloads or installs a toolchain')
    return ['cargo']


def installed_compiler(environment):
    """Return the installed ``cargo`` prefix after recording the real toolchain."""
    prefix = installed_prefix(environment)
    versions = []
    for tool in ('rustc', 'cargo'):
        result = subprocess.run(
            [tool, '--version'], cwd=ROOT, env=process_environment(environment),
            capture_output=True, text=True)
        line = result.stdout.strip() or result.stderr.strip()
        if result.returncode != 0 or not line:
            raise PipelineError(f'{tool} --version failed; the local release build '
                                'needs a working installed toolchain')
        versions.append(line)
    print(f'local release toolchain: {versions[0]}; {versions[1]}', file=sys.stderr)
    print('note: local mode proves the installed compiler; the hosted workflow '
          f'keeps Rust {TOOLCHAIN} on Ubuntu 22.04 (glibc 2.35) for the exact '
          'binary/glibc baseline', file=sys.stderr)
    return prefix


def compiler_for(local, environment):
    """Return the cargo prefix: the installed compiler for ``local``, else the pin."""
    return installed_compiler(environment) if local else compiler(environment)


def run(argv, environment=None):
    """Run one step from the repository root, failing readably on a nonzero exit."""
    merged = dict(os.environ, **(environment or {}))
    print(f'+ {" ".join(argv)}', file=sys.stderr)
    result = subprocess.run(argv, cwd=ROOT, env=merged)
    if result.returncode != 0:
        raise PipelineError(f'{" ".join(argv)} exited {result.returncode}')


def host_platform():
    """Return the running host as a lower-cased ``(system, machine)`` pair."""
    return (platform.system().lower(), platform.machine().lower())


def run_probe(argv, environment=None):
    """Run one host probe from the repository root and return (code, stdout, stderr)."""
    result = subprocess.run(
        argv, cwd=ROOT, env=process_environment(environment),
        capture_output=True, text=True)
    return result.returncode, result.stdout, result.stderr


def inherited_injection(environment, linker_variable=NATIVE_LINKER_VARIABLE,
                        linker=NATIVE_CC):
    """Return inherited compiler/linker/sysroot settings the native lane refuses.

    A Nix dev shell or caller wrapper can export ``CC``, target linker, sysroot
    flags or a deployment target; the native lane sets its own explicit values
    and refuses the rest rather than trusting them. Only the exact selected
    target linker value is allowed through the target linker variable.
    """
    names = [name for name in NATIVE_INJECTION_VARIABLES if environment.get(name)]
    for name, value in environment.items():
        if not value:
            continue
        if name.startswith('CARGO_TARGET_') and name.endswith(('_LINKER', '_RUSTFLAGS')):
            if name == linker_variable and value == linker:
                continue
            names.append(name)
        elif name in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS') and \
                re.search(r'sysroot|linker|link-arg', value):
            names.append(name)
    return sorted(set(names))


def native_cargo(environment, probe):
    """Return the installed ``cargo`` prefix and versions, or fail readably."""
    versions = {}
    for tool in ('rustc', 'cargo'):
        code, stdout, stderr = probe([tool, '--version'], environment)
        line = stdout.strip() or stderr.strip()
        if code != 0 or not line:
            raise PipelineError(
                f'{tool} --version failed; run the native lane inside '
                '`nix develop --no-update-lock-file .#release-native -c`')
        versions[tool] = line
    return ['cargo'], versions


def native_target(target):
    """Return the configured native target entry, refusing an unsupported triple."""
    if not target:
        raise PipelineError(
            'build-native needs RELEASE_TARGET (for example '
            f'RELEASE_TARGET={TARGET})')
    config = NATIVE_TARGETS.get(target)
    if config is None:
        raise PipelineError(
            'native link mode only supports '
            + ', '.join(sorted(NATIVE_TARGETS)) + f'; got {target!r}')
    return config


def probe_text(argv, environment, probe, message):
    """Run one probe and return its first non-empty output line, or fail readably."""
    code, stdout, stderr = probe(argv, environment)
    text = stdout.strip() or stderr.strip()
    if code != 0 or not text:
        raise PipelineError(message)
    return text.splitlines()[0].strip()


def linux_native_toolchain(config, environment, compiler_path, probe):
    """Return the explicit Linux host compiler/linker and libc evidence."""
    compiler = Path(compiler_path)
    if not compiler.is_file() or not os.access(compiler, os.X_OK):
        raise PipelineError(
            f'{compiler_path} is missing or not executable; the native lane needs '
            'the host C compiler and linker')
    resolved = compiler.resolve()
    if str(resolved).startswith('/nix/store/'):
        raise PipelineError(
            f'{compiler_path} resolves into the Nix store ({resolved}); point it at '
            'the host C compiler')
    compiler_version = probe_text(
        [compiler_path, '--version'], environment, probe,
        f'{compiler_path} --version failed; the native lane needs a working '
        'host C compiler')
    libc = probe_text(
        ['ldd', '--version'], environment, probe,
        'ldd --version failed; the native lane needs the host libc')
    return {
        'compiler': compiler_version,
        'linker': NATIVE_CC,
        'libc': libc,
        'linker_environment': {config['linker_variable']: NATIVE_CC},
    }


def darwin_native_toolchain(config, environment, xcrun_path, probe):
    """Return Apple Clang, ld and Xcode SDK identities resolved through xcrun.

    The system ``/usr/bin/xcrun`` selects the host Xcode toolchain; any
    Nix-store resolution is refused, and the deployment target is set
    explicitly from the configured table instead of inherited flags.
    """
    xcrun = Path(xcrun_path)
    if not xcrun.is_file() or not os.access(xcrun, os.X_OK):
        raise PipelineError(
            f'{xcrun_path} is missing or not executable; the Darwin native lane '
            'needs the system xcrun')
    resolved = xcrun.resolve()
    if str(resolved).startswith('/nix/store/'):
        raise PipelineError(
            f'{xcrun_path} resolves into the Nix store ({resolved}); use the '
            'system xcrun')
    def query(arguments, message):
        return probe_text([xcrun_path, '--sdk', 'macosx'] + arguments,
                          environment, probe, message)

    clang_path = query(['--find', 'clang'],
                       'xcrun --find clang failed; the Darwin native lane needs '
                       'Apple Clang')
    linker_path = query(['--find', 'ld'],
                        'xcrun --find ld failed; the Darwin native lane needs the '
                        'Apple linker')
    sdk_path = query(['--show-sdk-path'],
                     'xcrun --show-sdk-path failed; the Darwin native lane needs '
                     'the Xcode macOS SDK')
    sdk_version = query(['--show-sdk-version'],
                        'xcrun --show-sdk-version failed; the Darwin native lane '
                        'needs the Xcode macOS SDK')
    for label, value in (('Apple Clang', clang_path), ('linker', linker_path),
                         ('SDK', sdk_path)):
        if value.startswith('/nix/store/'):
            raise PipelineError(
                f'the resolved {label} is in the Nix store ({value}); use the '
                'system Apple toolchain')
    compiler_version = probe_text(
        [clang_path, '--version'], environment, probe,
        f'{clang_path} --version failed; the Darwin native lane needs a working '
        'Apple Clang')
    linker_version = probe_text(
        [linker_path, '-v'], environment, probe,
        f'{linker_path} -v failed; the Darwin native lane needs a working '
        'Apple linker')
    deployment_target = config['deployment_target']
    return {
        'compiler': compiler_version,
        'compiler_path': clang_path,
        'linker': linker_path,
        'linker_version': linker_version,
        'sdk': {'path': sdk_path, 'version': sdk_version},
        'deployment_target': deployment_target,
        'deployment_target_source': DEPLOYMENT_TARGET_SOURCE,
        'deployment_target_status': DEPLOYMENT_TARGET_STATUS,
        'linker_environment': {
            config['linker_variable']: clang_path,
            'MACOSX_DEPLOYMENT_TARGET': deployment_target,
        },
    }


def native_plan(target, environment, host=None, compiler_path=NATIVE_CC,
                xcrun_path=NATIVE_XCRUN, probe=run_probe):
    """Return the validated native build plan and its provenance evidence.

    The plan carries the strict ``--release --locked --target`` argv, the
    environment with Cargo's target linker set explicitly, and the matching
    compiler/linker/SDK/libc/deployment-target plus source-SHA/lock-hash record
    written under ``build/release/``. ``host``, ``xcrun_path`` and ``probe`` are
    injectable so the self-tests exercise the refusals and recorded values with
    fake processes, never a real compiler, Cargo or build.
    """
    host = host or host_platform()
    config = native_target(target)
    if host != config['host']:
        raise PipelineError(
            f'native link mode for {target} needs a {config["host_label"]} host; '
            f'found {host[0]} {host[1]}')
    injected = inherited_injection(environment)
    if injected:
        raise PipelineError(
            'refusing inherited compiler/linker/sysroot settings: ' + ', '.join(injected))
    if config['host'] == NATIVE_HOST:
        toolchain = linux_native_toolchain(config, environment, compiler_path, probe)
    else:
        toolchain = darwin_native_toolchain(config, environment, xcrun_path, probe)
    code, stdout, stderr = probe(['git', 'rev-parse', 'HEAD'], environment)
    source_sha = stdout.strip()
    if code != 0 or not re.fullmatch(r'[0-9a-f]{40}', source_sha):
        raise PipelineError(
            'git rev-parse HEAD failed; the native evidence needs the source SHA')
    lock_hashes = {}
    for lock in ('Cargo.lock', 'flake.lock'):
        path = ROOT / lock
        if not path.is_file():
            raise PipelineError(f'{lock} is missing; the native evidence needs its hash')
        lock_hashes[lock] = hashlib.sha256(path.read_bytes()).hexdigest()
    cargo_prefix, versions = native_cargo(environment, probe)
    merged = dict(environment)
    merged.update(toolchain['linker_environment'])
    argv = build_command(cargo_prefix, target)
    evidence = {
        'target': target,
        'host': {'system': host[0], 'machine': host[1]},
        'argv': argv,
        'source_sha': source_sha,
        'lock_hashes': lock_hashes,
        'rustc': versions['rustc'],
        'cargo': versions['cargo'],
    }
    evidence.update(toolchain)
    return {'argv': argv, 'environment': merged, 'evidence': evidence}


def write_evidence(evidence, directory=None):
    """Write the native build provenance JSON under ``build/release/``."""
    root = Path(directory) if directory is not None else ROOT / NATIVE_EVIDENCE_DIR
    root.mkdir(parents=True, exist_ok=True)
    path = root / NATIVE_EVIDENCE_FILE
    path.write_text(json.dumps(evidence, indent=2, sort_keys=True) + '\n',
                    encoding='utf-8')
    return path


def native_build(target, environment):
    """Run the experimental native-linker release build and record its provenance."""
    plan = native_plan(target, environment)
    path = write_evidence(plan['evidence'])
    print(f'native release evidence: {path}', file=sys.stderr)
    run(plan['argv'], plan['environment'])


def build(local, environment):
    """Run the workflow's locked, target-specific release build.

    ``local`` selects the installed compiler (never downloading one) and records
    its actual versions; both modes issue the same ``--release --locked
    --target`` command and produce the binary ``artifacts`` later packages.
    """
    run(build_command(compiler_for(local, environment)), environment)


def artifacts(environment):
    """Package, validate and smoke-test the actual release binary the build produced."""
    version = resolve_version(environment)
    if not (ROOT / BINARY).is_file():
        raise PipelineError(f'{BINARY} is missing; run `make release-locked` first')
    steps, step_environment = artifact_commands(version, BINARY, DIST)
    for argv in steps:
        run(argv, step_environment)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('build', 'artifacts', 'build-native'),
                        help='build the pinned binary, package and smoke-test it, '
                             'or run the experimental native-linker build')
    parser.add_argument('--local', action='store_true',
                        help='build with the installed compiler instead of the '
                             'pinned toolchain (never downloads one)')
    parser.add_argument('--target', default='',
                        help='RELEASE_TARGET for build-native; must match the host')
    args = parser.parse_args()
    try:
        if args.mode == 'build':
            build(args.local, os.environ)
        elif args.mode == 'build-native':
            if args.local:
                raise PipelineError(
                    'build-native is a separate experimental lane; it never combines '
                    'with the pinned/--local modes')
            native_build(args.target, os.environ)
        else:
            artifacts(os.environ)
    except PipelineError as error:
        print(f'release-pipeline: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
