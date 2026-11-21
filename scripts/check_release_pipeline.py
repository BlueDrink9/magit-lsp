#!/usr/bin/env python3
"""Run the release workflow's locked build and separate package/consume steps.

``build`` refuses any compiler that is not exactly the pinned Rust toolchain and
then runs the workflow's ``--release --locked --target`` build. There is no local
compiler alias: the strict pin is the only release build. ``build-native`` is the
experimental hosted native-linker lane: it accepts only the matching Linux
x86_64, Darwin arm64 or Darwin x86_64 host and target. Linux points Cargo's
target linker explicitly at ``/usr/bin/cc``; Darwin resolves Apple Clang, ld and
the Xcode macOS SDK through the system ``/usr/bin/xcrun`` and sets an explicit
deployment target. Both record the exact locked argv plus compiler, linker,
SDK/libc and source/lock provenance under ``build/release/``.

``artifacts`` is the packaging interface, separate from building and from native
runtime validation. It resolves a run-bound identity (target, original ref,
version, source SHA, event, run ID/attempt), refuses absent or contradictory
values before invoking the packager, writes the explicit provenance record the
deterministic packager needs, then packages and validates the archive/sidecar
offline. It never executes the packaged binary and never writes to the protected
``dist/`` directory. The run manifest binds the identity to the Cargo, flake and
pnpm lock hashes, toolchain, target and archive/executable digests; its digest is
written to a separate file so the manifest is not self-referential. Native
runtime checks consume the produced archive separately.

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
sys.path.insert(0, str(ROOT / 'scripts'))
import package_release  # noqa: E402  (the single intended-target contract)

TOOLCHAIN = '1.94.1'
TARGET = 'x86_64-unknown-linux-gnu'
RELEASE_DIR = Path('build/release')
ARTIFACT_OUTPUT = RELEASE_DIR / 'artifacts'
PROVENANCE_FILE = 'provenance.json'
MANIFEST_FILE = 'release-manifest.json'
# The archive path the packaging interface must never write to: it is the
# operator's protected local directory, not a pipeline output.
PROTECTED_OUTPUT = Path('dist')
LOCK_FILES = ('Cargo.lock', 'flake.lock', 'clients/vscode/pnpm-lock.yaml')
PACKAGE_TARGETS = tuple(sorted(package_release.TARGETS))
REQUIRED_IDENTITY = ('target', 'original_ref', 'version', 'source_sha', 'event',
                     'run_id', 'run_attempt')
VALIDATOR = Path('scripts/check_release_automation.py')
VERSION_PATTERN = re.compile(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)')
HASH_PATTERN = re.compile(r'[0-9a-f]{64}')
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


def target_binary(target=TARGET):
    """Return the built executable path for ``target``."""
    return Path('target') / target / 'release' / 'magit-lsp'


def artifact_commands(plan):
    """Return the package/validate step argv list and their environment.

    ``plan`` is a validated package plan from :func:`package_plan`. The steps are
    only the deterministic packager and the offline archive checker: this
    interface never compiles, runs Cargo or executes the packaged binary, and it
    does not hand the binary to a test process.
    """
    identity = plan['identity']
    steps = [
        ['sh', 'scripts/package-release.sh', identity['version'],
         plan['binary'], plan['output'], identity['target'],
         plan['provenance_path']],
        [sys.executable, str(VALIDATOR), '--validate-artifacts', plan['output']],
    ]
    return steps, {'PYTHONDONTWRITEBYTECODE': '1'}


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


def identity_value(identity, key):
    """Return one required non-empty release-identity string or refuse it."""
    value = identity.get(key) if isinstance(identity, dict) else None
    if not isinstance(value, str) or not value.strip():
        raise PipelineError(f'release identity is missing {key}')
    return value


def validate_identity(identity):
    """Validate and normalize the run-bound packaging identity.

    Target, original ref, version, source SHA, event and run ID/attempt must all
    be present and mutually consistent before any packaging step runs. A tag ref
    must name ``v<version>`` and the version must equal the manifests, so a
    split or foreign identity is refused rather than packed.
    """
    target = identity_value(identity, 'target')
    if target not in PACKAGE_TARGETS:
        raise PipelineError(
            f'release identity target {target!r} is unsupported; expected one of '
            + ', '.join(PACKAGE_TARGETS))
    version = identity_value(identity, 'version')
    if not VERSION_PATTERN.fullmatch(version):
        raise PipelineError(
            f'release identity version {version!r} is not a stable X.Y.Z version')
    expected_version = manifest_version()
    if version != expected_version:
        raise PipelineError(
            f'release identity version {version} does not match the manifest '
            f'version {expected_version}')
    ref = identity_value(identity, 'original_ref')
    if not ref.startswith(('refs/tags/', 'refs/heads/')):
        raise PipelineError(f'release identity original_ref {ref!r} is not a Git ref')
    if ref.startswith('refs/tags/') and ref != f'refs/tags/v{version}':
        raise PipelineError(
            f'release identity tag ref {ref} does not name v{version}')
    source_sha = identity_value(identity, 'source_sha')
    if not re.fullmatch(r'[0-9a-f]{40}', source_sha):
        raise PipelineError(
            f'release identity source_sha {source_sha!r} is not a full commit SHA')
    event = identity_value(identity, 'event')
    run_id = identity_value(identity, 'run_id')
    if not re.fullmatch(r'[0-9]+', run_id):
        raise PipelineError(f'release identity run_id {run_id!r} is not numeric')
    run_attempt = identity_value(identity, 'run_attempt')
    if not re.fullmatch(r'[0-9]+', run_attempt):
        raise PipelineError(
            f'release identity run_attempt {run_attempt!r} is not numeric')
    return {
        'target': target,
        'original_ref': ref,
        'version': version,
        'source_sha': source_sha,
        'event': event,
        'run_id': run_id,
        'run_attempt': run_attempt,
    }


def resolve_identity(environment):
    """Return the packaging identity the workflow environment supplies.

    The build lane's fixed target is the default; everything else must come from
    the run environment and is validated by :func:`validate_identity`.
    """
    return validate_identity({
        'target': environment.get('RELEASE_BUILD_TARGET') or TARGET,
        'original_ref': environment.get('RELEASE_REF') or environment.get('GITHUB_REF', ''),
        'version': resolve_version(environment),
        'source_sha': environment.get('RELEASE_SOURCE_SHA') or environment.get('GITHUB_SHA', ''),
        'event': environment.get('RELEASE_EVENT') or environment.get('GITHUB_EVENT_NAME', ''),
        'run_id': environment.get('RELEASE_RUN_ID') or environment.get('GITHUB_RUN_ID', ''),
        'run_attempt': (environment.get('RELEASE_RUN_ATTEMPT')
                        or environment.get('GITHUB_RUN_ATTEMPT', '')),
    })


def verify_source(identity, environment, probe=None):
    """Refuse a source SHA that is not the checked-out commit."""
    probe = probe or run_probe
    code, stdout, stderr = probe(['git', 'rev-parse', 'HEAD'], environment)
    head = stdout.strip()
    if code != 0 or not re.fullmatch(r'[0-9a-f]{40}', head):
        raise PipelineError(
            f'git rev-parse HEAD failed ({stderr.strip() or "no output"}); the '
            'release identity needs the checked-out source SHA')
    if head != identity['source_sha']:
        raise PipelineError(
            f'release identity source {identity["source_sha"]} does not match the '
            f'checked-out source {head}')
    return identity


def lock_hashes():
    """Return the SHA-256 of every lock file the run manifest binds."""
    hashes = {}
    for lock in LOCK_FILES:
        path = ROOT / lock
        if not path.is_file():
            raise PipelineError(f'{lock} is missing; the run manifest needs its hash')
        hashes[lock] = hashlib.sha256(path.read_bytes()).hexdigest()
    return hashes


def package_plan(identity, binary=None, output=None):
    """Return the isolated package plan for one validated identity.

    The plan binds the explicit provenance record the packager consumes and the
    run-manifest fields available before packaging. It refuses a missing binary,
    an absent or contradictory identity and a protected ``dist/`` output, so
    none of those can reach the packager.
    """
    validated = validate_identity(identity)
    binary_path = Path(binary) if binary is not None else target_binary(validated['target'])
    if not binary_path.is_file():
        raise PipelineError(
            f'{binary_path} is missing; build the {validated["target"]} release first')
    output_path = Path(output).resolve() if output is not None else \
        (ROOT / ARTIFACT_OUTPUT).resolve()
    protected = (ROOT / PROTECTED_OUTPUT).resolve()
    if output_path == protected or protected in output_path.parents:
        raise PipelineError(
            f'refusing protected output {output_path}; package into a build directory')
    try:
        content = binary_path.read_bytes()
    except OSError as error:
        raise PipelineError(
            f'cannot read binary {binary_path}: {error.strerror or error}') from error
    executable_sha256 = hashlib.sha256(content).hexdigest()
    return {
        'identity': validated,
        'binary': str(binary_path),
        'output': str(output_path),
        'provenance_path': str(ROOT / RELEASE_DIR / PROVENANCE_FILE),
        'provenance': {
            'version': validated['version'],
            'target': validated['target'],
            'executable_sha256': executable_sha256,
        },
        'executable_sha256': executable_sha256,
        'lock_hashes': lock_hashes(),
        'toolchain': {'rust': TOOLCHAIN},
    }


def release_manifest(plan, archive_sha256):
    """Return the run-bound manifest for one packaged archive.

    The manifest carries no digest of itself: :func:`write_manifest` emits the
    manifest digest as a separate file, so it cannot be self-referential.
    """
    if not isinstance(archive_sha256, str) or not HASH_PATTERN.fullmatch(archive_sha256):
        raise PipelineError('archive_sha256 must be 64 lowercase hex digits')
    identity = plan['identity']
    return {
        'source_sha': identity['source_sha'],
        'original_ref': identity['original_ref'],
        'event': identity['event'],
        'run_id': identity['run_id'],
        'run_attempt': identity['run_attempt'],
        'version': identity['version'],
        'lock_hashes': dict(plan['lock_hashes']),
        'target': identity['target'],
        'toolchain': dict(plan['toolchain']),
        'archive_sha256': archive_sha256,
        'executable_sha256': plan['executable_sha256'],
    }


def manifest_bytes(manifest):
    """Return the canonical UTF-8 manifest payload."""
    return (json.dumps(manifest, indent=2, sort_keys=True) + '\n').encode('utf-8')


def manifest_digest(manifest):
    """Return the SHA-256 of the canonical manifest payload."""
    return hashlib.sha256(manifest_bytes(manifest)).hexdigest()


def write_provenance(plan, directory=None):
    """Write the packager's explicit provenance record and return its path."""
    path = (Path(directory) / PROVENANCE_FILE if directory is not None
            else Path(plan['provenance_path']))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(plan['provenance'], sort_keys=True) + '\n',
                    encoding='utf-8')
    return path


def write_manifest(plan, archive_sha256, directory=None):
    """Write the run manifest plus its external SHA-256 sidecar.

    Returns ``(manifest_path, digest)``. The digest lives only in the sidecar so
    the manifest payload stays non-self-referential.
    """
    root = Path(directory) if directory is not None else ROOT / RELEASE_DIR
    root.mkdir(parents=True, exist_ok=True)
    manifest = release_manifest(plan, archive_sha256)
    path = root / MANIFEST_FILE
    path.write_bytes(manifest_bytes(manifest))
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    (root / (MANIFEST_FILE + '.sha256')).write_text(
        f'{digest}  {MANIFEST_FILE}\n', encoding='utf-8')
    return path, digest


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


def build(environment):
    """Run the workflow's locked, target-specific release build with the strict pin.

    The pinned toolchain is the only release build; there is no installed-compiler
    alias, and the produced binary is what ``artifacts`` later packages.
    """
    run(build_command(compiler(environment)), environment)


def artifacts(environment):
    """Package and validate the target binary for one validated run identity.

    Identity and source are checked before the packager runs. Packaging and
    offline archive validation are the only steps; the packaged binary is never
    executed, and the manifest digest is written outside the manifest payload.
    """
    identity = verify_source(resolve_identity(environment), environment)
    plan = package_plan(identity)
    provenance = write_provenance(plan)
    print(f'release provenance: {provenance}', file=sys.stderr)
    steps, step_environment = artifact_commands(plan)
    for argv in steps:
        run(argv, step_environment)
    archive = (Path(plan['output'])
               / f'magit-lsp-v{identity["version"]}-{identity["target"]}.tar.gz')
    if not archive.is_file():
        raise PipelineError(f'{archive} is missing after packaging')
    manifest, digest = write_manifest(plan, hashlib.sha256(archive.read_bytes()).hexdigest())
    print(f'release manifest: {manifest} (sha256 {digest})', file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('build', 'artifacts', 'build-native'),
                        help='build the pinned binary, package/validate it, or run '
                             'the experimental native-linker build')
    parser.add_argument('--target', default='',
                        help='RELEASE_TARGET for build-native; must match the host')
    args = parser.parse_args()
    try:
        if args.mode == 'build':
            build(os.environ)
        elif args.mode == 'build-native':
            native_build(args.target, os.environ)
        else:
            artifacts(os.environ)
    except PipelineError as error:
        print(f'release-pipeline: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
