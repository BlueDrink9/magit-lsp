'use strict';

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync, fork } = require('node:child_process');
const zlib = require('node:zlib');

const { LIMITS, resolveServer } = require('../lib/server-download');

const VERSION = require('../package.json').version;
const ASSET = 'x86_64-unknown-linux-gnu';
const BINARY = Buffer.from('#!/bin/sh\nexit 0\n');
const RUNTIME = { platform: 'linux', arch: 'x64', glibc: '2.36' };
const FIXTURE_PROGRAM = path.join(__dirname, '..', '..', '..', 'scripts', 'test_release_archive_fixture.py');
const PROCESS_FIXTURE = path.join(__dirname, 'server-download-process.js');
const RELEASE_BASE = `https://github.com/BlueDrink9/magit-lsp/releases/download/v${VERSION}`;

/** Create a temporary cache root; the caller removes it. */
function temporaryRoot() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'magit-server-download-'));
}

/**
 * Hex SHA-256 of bytes.
 *
 * @param {Buffer} bytes
 * @returns {string}
 */
function digest(bytes) {
  return crypto.createHash('sha256').update(bytes).digest('hex');
}

/** Versioned archive directory name published for one release. */
function stemOf(version = VERSION) {
  return `magit-lsp-v${version}-${ASSET}`;
}

/** Archive file name published for one release. */
function archiveNameOf(version = VERSION) {
  return `${stemOf(version)}.tar.gz`;
}

/**
 * Write one ustar header for a fixture archive member.
 *
 * @param {{name: string, size: number, type?: string}} member
 * @returns {Buffer}
 */
function tarHeader({ name, size, type = '0' }) {
  const header = Buffer.alloc(512);
  header.write(name, 0, 100, 'utf8');
  header.write(`${(0o755).toString(8).padStart(7, '0')}\0`, 100, 8, 'ascii');
  header.write(`${size.toString(8).padStart(11, '0')}\0`, 124, 12, 'ascii');
  header.write('00000000000\0', 136, 12, 'ascii');
  header.write('        ', 148, 8, 'ascii');
  header.write(type, 156, 1, 'ascii');
  header.write('ustar\0', 257, 6, 'ascii');
  header.write('00', 263, 2, 'ascii');
  let checksum = 0;
  for (const byte of header) checksum += byte;
  header.write(`${checksum.toString(8).padStart(6, '0')}\0 `, 148, 8, 'ascii');
  return header;
}

/**
 * Build an uncompressed ustar archive from fixture members.
 *
 * @param {{name: string, content?: Buffer|string, type?: string}[]} members
 * @returns {Buffer}
 */
function tarOf(members) {
  const parts = [];
  for (const member of members) {
    const content = Buffer.isBuffer(member.content)
      ? member.content
      : Buffer.from(member.content ?? '', 'utf8');
    parts.push(tarHeader({ name: member.name, size: content.length, type: member.type }));
    parts.push(content);
    parts.push(Buffer.alloc((512 - (content.length % 512)) % 512));
  }
  parts.push(Buffer.alloc(1024));
  return Buffer.concat(parts);
}

/**
 * Build the published file set for one release.
 *
 * @param {string} stem versioned archive directory
 * @param {Buffer} binary released executable bytes
 * @returns {{name: string, content: Buffer|string}[]}
 */
function releaseMembers(stem, binary) {
  return [
    { name: `${stem}/magit-lsp`, content: binary },
    { name: `${stem}/LICENSE`, content: 'license\n' },
    { name: `${stem}/LICENSE-APACHE`, content: 'apache\n' },
    { name: `${stem}/INSTALL.md`, content: 'install\n' },
  ];
}

/**
 * Build a release archive with the packager's Python tarfile settings.
 *
 * `scripts/package-release.sh` writes with Python's default tarfile format, so
 * a fractional mtime adds pax extended headers. Running the real producer-side
 * code keeps the fixture honest instead of hand-writing an equivalent archive.
 *
 * @param {Buffer} binary released executable bytes
 * @param {string} [version] release version named inside the archive
 * @returns {Buffer} gzip release archive bytes
 */
function packagedArchive(binary, version = VERSION) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'magit-release-fixture-'));
  try {
    const binaryPath = path.join(directory, 'magit-lsp');
    fs.writeFileSync(binaryPath, binary);
    const archivePath = path.join(directory, archiveNameOf(version));
    execFileSync('python3', [FIXTURE_PROGRAM, archivePath, stemOf(version), binaryPath], { stdio: 'pipe' });
    return fs.readFileSync(archivePath);
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

/**
 * Encode one pax extended-header record with its self-describing length.
 *
 * @param {string} key record key
 * @param {string} value record value
 * @returns {string}
 */
function paxRecord(key, value) {
  const body = ` ${key}=${value}\n`;
  let length = Buffer.byteLength(body) + 1;
  for (;;) {
    const record = `${length}${body}`;
    const actual = Buffer.byteLength(record);
    if (actual === length) return record;
    length = actual;
  }
}

/**
 * Build a pax extended header member carrying already-encoded records.
 *
 * @param {string} records concatenated pax records
 * @param {string} [name] member name replacing the producer's `././@PaxHeader`
 * @returns {{name: string, content: string, type: string}}
 */
function paxMember(records, name = '././@PaxHeader') {
  return { name, content: records, type: 'x' };
}

/**
 * Serve fixture release bytes over the injected transport boundary.
 *
 * @param {object} [options]
 * @param {string} [options.version] released version
 * @param {Buffer} [options.binary] released executable bytes
 * @param {Buffer} [options.archive] archive bytes replacing the built one
 * @param {object[]} [options.members] archive members replacing the published set
 * @param {string} [options.sidecar] sidecar text replacing the matching one
 * @param {string} [options.failure] message every transfer fails with
 * @param {Promise<void>} [options.gate] promise every transfer waits for
 * @returns {{transport: Function, calls: string[], base: string, archive: Buffer}}
 */
function releaseFixture(options = {}) {
  const version = options.version ?? VERSION;
  const stem = stemOf(version);
  const archiveName = archiveNameOf(version);
  const base = `https://github.com/BlueDrink9/magit-lsp/releases/download/v${version}`;
  const archive = options.archive ??
    zlib.gzipSync(tarOf(options.members ?? releaseMembers(stem, options.binary ?? BINARY)));
  const sidecar = options.sidecar ?? `${digest(archive)}  ${archiveName}\n`;
  /** @type {string[]} */
  const calls = [];
  const transport = async (url) => {
    calls.push(url);
    if (options.gate) await options.gate;
    if (options.failure) throw new Error(options.failure);
    if (url === `${base}/${archiveName}.sha256`) return Buffer.from(sidecar, 'utf8');
    if (url === `${base}/${archiveName}`) return archive;
    throw new Error(`unexpected download ${url}`);
  };
  return { transport, calls, base, archive };
}

/** Cache directory the helper owns for one version. */
function entryOf(root, version = VERSION) {
  return path.join(root, 'server', `v${version}`, ASSET);
}

/**
 * Write one release fixture's archive and sidecar for an independent process.
 *
 * @param {string} directory destination directory
 * @param {string} name fixture file stem
 * @param {Buffer} binary released executable bytes
 * @returns {{archivePath: string, sidecarPath: string}}
 */
function writeReleaseFixture(directory, name, binary) {
  const fixture = releaseFixture({ binary });
  const archivePath = path.join(directory, `${name}.tar.gz`);
  const sidecarPath = path.join(directory, `${name}.sha256`);
  fs.writeFileSync(archivePath, fixture.archive);
  fs.writeFileSync(sidecarPath, `${digest(fixture.archive)}  ${archiveNameOf()}\n`);
  return { archivePath, sidecarPath };
}

/**
 * Start one independent resolver process sharing this cache root.
 *
 * The fixture gates every transport call, so the parent deterministically
 * controls which extension host reaches publication first.
 *
 * @param {object} plan serializable plan handed to the fixture
 * @param {import('node:child_process').ChildProcess[]} children every spawned process, for cleanup
 * @returns {{child: import('node:child_process').ChildProcess, next: (type: string) => Promise<any>, send: (message: object) => void}}
 */
function startResolver(plan, children) {
  const child = fork(PROCESS_FIXTURE, [], { silent: true });
  children.push(child);
  let stderr = '';
  child.stderr.on('data', (chunk) => {
    stderr += chunk;
  });
  /** @type {Map<string, any[]>} */
  const queued = new Map();
  /** @type {Map<string, ((message: any) => void)[]>} */
  const waiting = new Map();
  child.on('message', (message) => {
    const waiters = waiting.get(message.type) ?? [];
    const waiter = waiters.shift();
    if (waiter) {
      waiter(message);
      return;
    }
    queued.set(message.type, [...(queued.get(message.type) ?? []), message]);
  });
  const next = (type) =>
    new Promise((resolve, reject) => {
      const available = (queued.get(type) ?? []).shift();
      if (available) {
        resolve(available);
        return;
      }
      const timer = setTimeout(() => reject(new Error(`resolver did not report ${type}: ${stderr}`)), 30000);
      waiting.set(type, [
        ...(waiting.get(type) ?? []),
        (message) => {
          clearTimeout(timer);
          resolve(message);
        },
      ]);
    });
  child.send({ type: 'resolve', ...plan });
  return { child, next, send: (message) => child.send(message) };
}

/**
 * List staging directories beside one entry directory.
 *
 * @param {string} entry cache directory for one version
 * @returns {string[]} sorted staging sibling names
 */
function stagingSiblings(entry) {
  const parent = path.dirname(entry);
  return fs.existsSync(parent)
    ? fs
        .readdirSync(parent)
        .filter((name) => name.startsWith('.staging-'))
        .sort()
    : [];
}

/**
 * Assert one entry is exactly a complete, self-consistent install.
 *
 * @param {string} entry cache directory for one version
 * @param {Buffer} binary expected executable bytes
 * @param {string} version expected release version
 */
function assertCompleteInstall(entry, binary, version) {
  assert.deepEqual(fs.readdirSync(entry).sort(), ['magit-lsp', 'manifest.json'], 'entry holds only its files');
  const manifest = JSON.parse(fs.readFileSync(path.join(entry, 'manifest.json'), 'utf8'));
  assert.equal(manifest.version, version);
  assert.equal(manifest.asset, ASSET);
  assert.equal(manifest.size, binary.length);
  assert.equal(manifest.digest, digest(binary));
  assert.deepEqual(fs.readFileSync(path.join(entry, 'magit-lsp')), binary);
}

/**
 * Assert a rejected resolution names its reason readably.
 *
 * @param {Promise<unknown>} promise
 * @param {RegExp[]} patterns fragments the message must contain
 */
async function rejects(promise, ...patterns) {
  await assert.rejects(promise, (error) => {
    assert.equal(error.name, 'ServerDownloadError', `readable failure: ${error.message}`);
    for (const pattern of patterns) assert.match(error.message, pattern);
    return true;
  });
}

/**
 * Fail the next publication rename deterministically at the production boundary.
 *
 * The helper under test is unchanged: this replaces the filesystem call, not a
 * code path, so `install` still stages, locks and cleans up exactly as it does
 * for a real rename error. Only destinations inside the entry directory fail,
 * so nothing outside publication is affected.
 *
 * @param {string} entry cache directory for one version
 * @returns {{calls: () => number, restore: () => void}}
 */
function failPublicationRename(entry) {
  const original = fs.renameSync;
  let calls = 0;
  fs.renameSync = (source, destination) => {
    if (destination === entry || destination.startsWith(`${entry}${path.sep}`)) {
      calls += 1;
      const error = new Error(`EACCES: permission denied, rename '${source}' -> '${destination}'`);
      error.code = 'EACCES';
      throw error;
    }
    return original(source, destination);
  };
  return {
    calls: () => calls,
    restore: () => {
      fs.renameSync = original;
    },
  };
}

/**
 * Assert a failed resolution published no files and left no staging debris.
 *
 * @param {string} entry cache directory for one version
 */
function assertNoInstall(entry) {
  assert.equal(fs.existsSync(path.join(entry, 'magit-lsp')), false, 'no binary published');
  assert.equal(fs.existsSync(path.join(entry, 'manifest.json')), false, 'no manifest published');
  const leftovers = fs.existsSync(entry) ? fs.readdirSync(entry) : [];
  assert.deepEqual(leftovers, [], 'failed download leaves no partial files');
}

/**
 * Build a response body that records whether the downloader released it.
 *
 * @param {object} [options]
 * @param {Buffer[]} [options.chunks] fragments yielded to a reader
 * @param {boolean} [options.failCancel] make cancellation itself throw
 * @param {boolean} [options.stall] never yield a fragment until aborted
 * @param {AbortSignal} [options.signal] clock signal that errors a stalled body
 * @returns {{stream: ReadableStream, cancelled: () => boolean, cancelFailure: () => unknown, delivered: () => number}}
 */
function trackedBody(options = {}) {
  const chunks = options.chunks ?? [];
  let index = 0;
  let cancelled = false;
  /** @type {unknown} */
  let cancelFailure;
  const stream = new ReadableStream({
    start(controller) {
      if (!options.signal) return;
      const stop = () => {
        try {
          controller.error(options.signal?.reason ?? new Error('aborted'));
        } catch {}
      };
      if (options.signal.aborted) stop();
      else options.signal.addEventListener('abort', stop, { once: true });
    },
    pull(controller) {
      if (options.stall) return;
      if (index >= chunks.length) {
        controller.close();
        return;
      }
      controller.enqueue(new Uint8Array(chunks[index]));
      index += 1;
    },
    cancel() {
      cancelled = true;
      if (options.failCancel) {
        cancelFailure = new Error('body cancellation failed');
        throw cancelFailure;
      }
    },
  });
  return {
    stream,
    cancelled: () => cancelled,
    cancelFailure: () => cancelFailure,
    delivered: () => index,
  };
}

/**
 * Run one scenario against the real production download transport.
 *
 * Only the fetch boundary and the production request clock are replaced, so the
 * real redirect, size and abort logic still runs. Both globals are restored even
 * when the scenario throws, and an unhandled rejection fails it.
 *
 * @param {(url: URL, signal: AbortSignal) => Promise<Response>} respond
 * @param {(control: {calls: string[], controllers: AbortController[]}) => Promise<void>} body
 * @returns {Promise<void>}
 */
async function withProductionFetch(respond, body) {
  const originalFetch = globalThis.fetch;
  const originalTimeout = AbortSignal.timeout;
  /** @type {string[]} */
  const calls = [];
  /** @type {AbortController[]} */
  const controllers = [];
  /** @type {unknown[]} */
  const rejections = [];
  const onRejection = (reason) => rejections.push(reason);
  globalThis.fetch = (url, init = {}) => {
    calls.push(String(url));
    return respond(url, init.signal);
  };
  AbortSignal.timeout = () => {
    const controller = new AbortController();
    controllers.push(controller);
    return controller.signal;
  };
  process.on('unhandledRejection', onRejection);
  try {
    await body({ calls, controllers });
    await new Promise((resolve) => setImmediate(resolve));
    assert.deepEqual(rejections.map(String), [], 'no unhandled rejection escapes the downloader');
  } finally {
    globalThis.fetch = originalFetch;
    AbortSignal.timeout = originalTimeout;
    process.off('unhandledRejection', onRejection);
  }
}

/**
 * Resolve one version through the production fetch transport.
 *
 * @param {string} root cache root
 * @returns {Promise<{executable: string, version: string}>}
 */
function resolveProduction(root) {
  return resolveServer({ version: VERSION, cacheRoot: root, runtime: RUNTIME });
}

/**
 * Build a temporary PATH containing one fake `magit-lsp` program.
 *
 * The program records that it was started, so a test can prove the PATH probe
 * did or did not run, and prints the version its fixture names.
 *
 * @param {{version?: string, script?: string}} [options]
 * @returns {{root: string, bin: string, marker: string, env: NodeJS.ProcessEnv, remove: () => void}}
 */
function fakePathServer(options = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'magit-server-path-'));
  const bin = path.join(root, 'bin');
  fs.mkdirSync(bin);
  const marker = path.join(root, 'probed');
  const script = options.script
    ?? `#!/bin/sh\n: > '${marker}'\necho 'magit-lsp ${options.version ?? VERSION}'\n`;
  const program = path.join(bin, 'magit-lsp');
  fs.writeFileSync(program, script);
  fs.chmodSync(program, 0o755);
  return {
    root,
    bin,
    marker,
    env: { ...process.env, PATH: bin },
    remove: () => fs.rmSync(root, { recursive: true, force: true }),
  };
}

/**
 * Build a temporary executable file for an explicit server-path setting.
 *
 * @returns {{path: string, remove: () => void}}
 */
function fakeExplicitServer() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'magit-server-explicit-'));
  const executable = path.join(root, 'magit-lsp');
  fs.writeFileSync(executable, '#!/bin/sh\nexit 0\n');
  fs.chmodSync(executable, 0o755);
  return { path: executable, remove: () => fs.rmSync(root, { recursive: true, force: true }) };
}

/**
 * Record managed resolutions without reaching the network.
 *
 * @returns {{calls: object[], resolve: (options: object) => Promise<{executable: string, version: string}>}}
 */
function managedFixture() {
  /** @type {object[]} */
  const calls = [];
  return {
    calls,
    resolve: async (options) => {
      calls.push(options);
      return { executable: '/managed/cache/magit-lsp', version: options.version };
    },
  };
}

/**
 * Assert one readable server-source rejection.
 *
 * @param {Promise<unknown>} promise
 * @param {...RegExp} patterns
 * @returns {Promise<void>}
 */
async function rejectsSource(promise, ...patterns) {
  await assert.rejects(promise, (error) => {
    assert.equal(error.name, 'ServerSourceError', `readable failure: ${error.message}`);
    for (const pattern of patterns) assert.match(error.message, pattern);
    return true;
  });
}

/**
 * Load the server-source resolver fresh so an earlier harness's injected
 * downloader cannot stay bound to the cached module.
 *
 * @returns {typeof import('../lib/server-source')}
 */
function freshServerSource() {
  for (const name of ['server-source', 'server-download']) {
    delete require.cache[require.resolve(`../lib/${name}`)];
  }
  return require('../lib/server-source');
}

/**
 * Run every verified-download check, returning how many passed.
 *
 * @returns {Promise<number>} number of passed checks
 */
async function run() {
  const { resolveServerSource } = freshServerSource();
  const failures = [];
  let passed = 0;
  /**
   * Run one named check and record its failure instead of stopping the suite.
   *
   * @param {string} name
   * @param {() => Promise<void>} body
   */
  async function check(name, body) {
    try {
      await body();
      passed += 1;
    } catch (error) {
      failures.push({ name, error });
    }
  }

  await check('downloads, verifies and caches the exact released binary', async () => {
    const root = temporaryRoot();
    try {
      const fixture = releaseFixture();
      const resolve = (version = VERSION) =>
        resolveServer({ version, cacheRoot: root, transport: fixture.transport, runtime: RUNTIME });
      const first = await resolve();
      const executable = path.join(entryOf(root), 'magit-lsp');
      assert.equal(first.executable, executable);
      assert.equal(first.version, VERSION);
      assert.equal(fs.lstatSync(executable).isFile(), true);
      assert.equal(fs.statSync(executable).mode & 0o111, 0o111, 'cached binary is executable');
      assert.deepEqual(fs.readFileSync(executable), BINARY, 'cached bytes are the release bytes');
      assert.deepEqual(fixture.calls, [
        `${fixture.base}/${archiveNameOf()}.sha256`,
        `${fixture.base}/${archiveNameOf()}`,
      ]);
      const second = await resolve();
      assert.equal(second.executable, executable);
      assert.equal(fixture.calls.length, 2, 'second resolution reuses the cached binary');
      const explicit = await resolve(`v${VERSION}`);
      assert.equal(explicit.executable, executable, 'a leading v names the same release');
      assert.equal(fixture.calls.length, 2, 'normalized version reuses the cache');
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('installs the packager Python tarfile archive with pax mtime headers', async () => {
    const root = temporaryRoot();
    try {
      const archive = packagedArchive(BINARY);
      const tar = zlib.gunzipSync(archive);
      assert.equal(tar.includes(Buffer.from('././@PaxHeader')), true, 'fixture exercises pax headers');
      assert.equal(tar.includes(Buffer.from('mtime=1700000000.125')), true, 'fixture has a fractional mtime');
      const fixture = releaseFixture({ archive });
      const installed = await resolveServer({
        version: VERSION,
        cacheRoot: root,
        transport: fixture.transport,
        runtime: RUNTIME,
      });
      assert.deepEqual(
        fs.readFileSync(installed.executable),
        BINARY,
        'the exact binary bytes from the Python archive are installed',
      );
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('concurrent resolutions share one download', async () => {
    const root = temporaryRoot();
    /** @type {(value?: unknown) => void} */
    let release;
    const gate = new Promise((resolve) => {
      release = resolve;
    });
    try {
      const fixture = releaseFixture({ gate });
      const resolve = () =>
        resolveServer({ version: VERSION, cacheRoot: root, transport: fixture.transport, runtime: RUNTIME });
      const resolving = [resolve(), resolve()];
      await new Promise((resolve) => setImmediate(resolve));
      release();
      const [first, second] = await Promise.all(resolving);
      assert.equal(first.executable, second.executable);
      assert.equal(fixture.calls.length, 2, 'one sidecar and one archive transfer for both callers');
    } finally {
      release();
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check("independent resolvers never delete another host's staging or winner", async () => {
    const root = temporaryRoot();
    /** @type {import('node:child_process').ChildProcess[]} */
    const children = [];
    try {
      const entry = entryOf(root);
      const parent = path.dirname(entry);
      fs.mkdirSync(parent, { recursive: true });
      // Another extension host is already staging a verified install here.
      const foreignStaging = fs.mkdtempSync(path.join(parent, '.staging-'));
      fs.writeFileSync(path.join(foreignStaging, 'magit-lsp'), BINARY);
      const fixtures = path.join(root, 'fixtures');
      fs.mkdirSync(fixtures);
      const winner = Buffer.from('#!/bin/sh\n# winner\nexit 0\n');
      const contender = Buffer.from('#!/bin/sh\n# contender\nexit 0\n');

      const first = startResolver({
        cacheRoot: root,
        version: VERSION,
        runtime: RUNTIME,
        ...writeReleaseFixture(fixtures, 'winner', winner),
      }, children);
      const second = startResolver({
        cacheRoot: root,
        version: VERSION,
        runtime: RUNTIME,
        ...writeReleaseFixture(fixtures, 'contender', contender),
      }, children);

      // Both hosts verified the empty cache before either transfer, then wait.
      await Promise.all([first.next('transport'), second.next('transport')]);
      first.send({ type: 'continue' });
      const published = await first.next('resolved');
      assert.equal(published.executable, path.join(entry, 'magit-lsp'));
      assertCompleteInstall(entry, winner, VERSION);

      // The second host loses publication and must reuse the winner unchanged.
      second.send({ type: 'continue' });
      const reused = await second.next('resolved');
      assert.equal(reused.executable, published.executable);
      assertCompleteInstall(entry, winner, VERSION);
      assert.deepEqual(
        fs.readFileSync(path.join(foreignStaging, 'magit-lsp')),
        BINARY,
        "the other host's active staging survives",
      );
      assert.deepEqual(stagingSiblings(entry), [path.basename(foreignStaging)], 'only the foreign staging remains');

      // A failed resolution must not touch another host's staging either.
      const failing = startResolver({
        cacheRoot: root,
        version: '0.0.9',
        runtime: RUNTIME,
        ...writeReleaseFixture(fixtures, 'failing', BINARY),
        failure: 'connection reset by peer',
      }, children);
      const failure = await failing.next('failed');
      assert.equal(failure.name, 'ServerDownloadError');
      assert.match(failure.message, /connection reset by peer/);
      assert.deepEqual(
        fs.readFileSync(path.join(foreignStaging, 'magit-lsp')),
        BINARY,
        'a failed resolution leaves staging alone',
      );
      assert.deepEqual(stagingSiblings(entry), [path.basename(foreignStaging)]);
      assert.deepEqual(stagingSiblings(entryOf(root, '0.0.9')), [], 'the failed attempt removed its own staging');
    } finally {
      for (const child of children) child.kill('SIGKILL');
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('failed publication leaves no partial entry and preserves prior versions', async () => {
    const root = temporaryRoot();
    /** @type {import('node:child_process').ChildProcess[]} */
    const children = [];
    try {
      const priorVersion = '0.1.1';
      const priorFixture = releaseFixture({ version: priorVersion });
      await resolveServer({
        version: priorVersion,
        cacheRoot: root,
        transport: priorFixture.transport,
        runtime: RUNTIME,
      });
      const priorEntry = entryOf(root, priorVersion);
      assertCompleteInstall(priorEntry, BINARY, priorVersion);

      const entry = entryOf(root);
      const parent = path.dirname(entry);
      fs.mkdirSync(parent, { recursive: true });
      const fixtures = path.join(root, 'fixtures');
      fs.mkdirSync(fixtures);
      const release = writeReleaseFixture(fixtures, 'release', BINARY);
      const start = () =>
        startResolver({ cacheRoot: root, version: VERSION, runtime: RUNTIME, ...release }, children);

      // A live owner holds the publication lock: the attempt fails readably.
      const lock = `${entry}.lock`;
      fs.mkdirSync(lock);
      fs.writeFileSync(path.join(lock, 'owner'), `${os.hostname()} ${process.pid}\n`);
      const blocked = start();
      blocked.send({ type: 'continue' });
      const locked = await blocked.next('failed');
      assert.equal(locked.name, 'ServerDownloadError');
      assert.match(locked.message, /timed out waiting for another process/);
      assert.equal(fs.existsSync(path.join(entry, 'magit-lsp')), false, 'no binary published');
      assert.equal(fs.existsSync(path.join(entry, 'manifest.json')), false, 'no manifest published');
      assert.deepEqual(stagingSiblings(entry), [], 'the failed attempt removed its own staging');
      assert.equal(fs.existsSync(lock), true, 'a live owner keeps its lock');
      fs.rmSync(lock, { recursive: true, force: true });

      // An invalid entry holding foreign files cannot be replaced atomically.
      fs.mkdirSync(entry);
      fs.writeFileSync(path.join(entry, 'keep.txt'), 'foreign\n');
      fs.writeFileSync(path.join(entry, 'magit-lsp'), 'tampered binary\n');
      const obstructed = start();
      obstructed.send({ type: 'continue' });
      const unexpected = await obstructed.next('failed');
      assert.equal(unexpected.name, 'ServerDownloadError');
      assert.match(unexpected.message, /contains unexpected files/);
      assert.deepEqual(fs.readdirSync(entry), ['keep.txt'], 'no binary-only entry and foreign files untouched');
      assert.deepEqual(stagingSiblings(entry), [], 'the failed attempt removed its own staging');
      assertCompleteInstall(priorEntry, BINARY, priorVersion);

      // The publication rename itself fails: no entry or staging survives.
      fs.rmSync(entry, { recursive: true, force: true });
      const fault = failPublicationRename(entry);
      try {
        const fixture = releaseFixture();
        await rejects(
          resolveServer({ version: VERSION, cacheRoot: root, transport: fixture.transport, runtime: RUNTIME }),
          /cannot install magit-lsp/,
          /permission denied/,
        );
        assert.equal(fault.calls(), 1, 'failure happened at the publication rename');
      } finally {
        fault.restore();
      }
      assert.equal(fs.existsSync(entry), false, 'no binary-only entry after a failed rename');
      assert.equal(fs.existsSync(`${entry}.lock`), false, 'the publication lock is released');
      assert.deepEqual(stagingSiblings(entry), [], 'the failed attempt removed its own staging');
      assertCompleteInstall(priorEntry, BINARY, priorVersion);
    } finally {
      for (const child of children) child.kill('SIGKILL');
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('stale dead-owner lock is reclaimed and a foreign-host lock is not stolen', async () => {
    const root = temporaryRoot();
    try {
      const entry = entryOf(root);
      fs.mkdirSync(path.dirname(entry), { recursive: true });
      // The pid comes from a process that has already exited, so reclamation
      // is the only way this lock can ever be resolved.
      const deadPid = Number(
        execFileSync(process.execPath, ['-e', 'process.stdout.write(String(process.pid))'], { encoding: 'utf8' }),
      );
      assert.ok(Number.isSafeInteger(deadPid) && deadPid > 0, 'spawned a process whose pid is known');
      const stale = `${entry}.lock`;
      fs.mkdirSync(stale);
      fs.writeFileSync(path.join(stale, 'owner'), `${os.hostname()} ${deadPid}\n`);
      const fixture = releaseFixture();
      const resolved = await resolveServer({
        version: VERSION,
        cacheRoot: root,
        transport: fixture.transport,
        runtime: RUNTIME,
      });
      assert.equal(resolved.executable, path.join(entry, 'magit-lsp'));
      assertCompleteInstall(entry, BINARY, VERSION);
      assert.equal(fs.existsSync(stale), false, 'the reclaimed lock left nothing behind');

      // Ownership is hostname-scoped: another host's lock is never stolen and
      // the failure names the lock the user has to remove.
      const foreignRoot = temporaryRoot();
      try {
        const foreignEntry = entryOf(foreignRoot);
        fs.mkdirSync(path.dirname(foreignEntry), { recursive: true });
        const foreign = `${foreignEntry}.lock`;
        fs.mkdirSync(foreign);
        fs.writeFileSync(path.join(foreign, 'owner'), `another-host.invalid ${process.pid}\n`);
        const other = releaseFixture();
        await assert.rejects(
          resolveServer({ version: VERSION, cacheRoot: foreignRoot, transport: other.transport, runtime: RUNTIME }),
          (error) => {
            assert.equal(error.name, 'ServerDownloadError', `readable failure: ${error.message}`);
            assert.match(error.message, /timed out waiting for another process/);
            assert.ok(error.message.includes(foreign), `failure names the lock: ${error.message}`);
            return true;
          },
        );
        assert.equal(fs.existsSync(foreign), true, "another host's lock is kept");
      } finally {
        fs.rmSync(foreignRoot, { recursive: true, force: true });
      }
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('tampered cache is replaced rather than launched', async () => {
    const root = temporaryRoot();
    try {
      const fixture = releaseFixture();
      const resolve = () =>
        resolveServer({ version: VERSION, cacheRoot: root, transport: fixture.transport, runtime: RUNTIME });
      const first = await resolve();
      fs.writeFileSync(first.executable, 'tampered binary\n');
      fs.chmodSync(first.executable, 0o755);
      const again = await resolve();
      assert.deepEqual(fs.readFileSync(again.executable), BINARY, 're-verified bytes replace the tamper');
      assert.equal(fixture.calls.length, 4, 'tampered cache triggers one verified re-download');
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('malformed versions fail before any URL is built', async () => {
    const root = temporaryRoot();
    try {
      const fixture = releaseFixture();
      for (const version of [
        'latest',
        '',
        'v1.2',
        '1.2.3.4',
        '01.2.3',
        '1.2.3-rc.1',
        '1.2.3/../../evil',
        ' 1.2.3',
        123,
        null,
        undefined,
      ]) {
        await rejects(
          resolveServer({ version, cacheRoot: root, transport: fixture.transport, runtime: RUNTIME }),
          /version/,
        );
      }
      assert.equal(fixture.calls.length, 0, 'invalid versions never reach the network');
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('unsupported hosts get manual server-path guidance', async () => {
    const root = temporaryRoot();
    try {
      const fixture = releaseFixture();
      const hosts = [
        [{ platform: 'darwin', arch: 'arm64', glibc: null }, /darwin-arm64/],
        [{ platform: 'linux', arch: 'arm64', glibc: '2.36' }, /linux-arm64/],
        [{ platform: 'win32', arch: 'x64', glibc: null }, /win32-x64/],
        [{ platform: 'linux', arch: 'x64', glibc: '2.31' }, /glibc 2\.35/],
        [{ platform: 'linux', arch: 'x64', glibc: null }, /glibc 2\.35/],
      ];
      for (const [runtime, pattern] of hosts) {
        await rejects(
          resolveServer({ version: VERSION, cacheRoot: root, transport: fixture.transport, runtime }),
          pattern,
          /magit-lsp\.server\.path/,
        );
      }
      assert.equal(fixture.calls.length, 0, 'unsupported hosts never download a binary');
      const supported = await resolveServer({
        version: VERSION,
        cacheRoot: root,
        transport: fixture.transport,
        runtime: { platform: 'linux', arch: 'x64', glibc: '2.35' },
      });
      assert.equal(supported.version, VERSION, 'glibc 2.35 exactly is supported');
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('bad checksum sidecars and digests publish nothing', async () => {
    const archiveName = archiveNameOf();
    const cases = [
      ['a sidecar naming another archive', `${'a'.repeat(64)}  other.tar.gz\n`, /checksum sidecar/],
      ['an uppercase digest', `${'A'.repeat(64)}  ${archiveName}\n`, /checksum sidecar/],
      ['a sidecar without a newline', `${'a'.repeat(64)}  ${archiveName}`, /checksum sidecar/],
      ['a digest that does not match', `${'0'.repeat(64)}  ${archiveName}\n`, /checksum does not match/],
      ['an oversized sidecar', 'a'.repeat(LIMITS.sidecarBytes + 1), /exceeds the \d+-byte limit/],
    ];
    for (const [label, sidecar, pattern] of cases) {
      const root = temporaryRoot();
      try {
        const fixture = releaseFixture({ sidecar });
        await rejects(
          resolveServer({ version: VERSION, cacheRoot: root, transport: fixture.transport, runtime: RUNTIME }),
          pattern,
        );
        assertNoInstall(entryOf(root));
      } catch (error) {
        error.message = `${label}: ${error.message}`;
        throw error;
      } finally {
        fs.rmSync(root, { recursive: true, force: true });
      }
    }
  });

  await check('unsafe archive members and types publish nothing', async () => {
    const stem = stemOf();
    const binaryName = `${stem}/magit-lsp`;
    const published = releaseMembers(stem, BINARY);
    const without = (name) => published.filter((member) => member.name !== `${stem}/${name}`);
    const cases = [
      ['path traversal', [...published, { name: '../magit-lsp', content: 'x' }], /unexpected archive member/],
      ['an absolute path', [{ name: '/magit-lsp', content: 'x' }, ...published], /unexpected archive member/],
      ['a symlink binary', [{ name: binaryName, content: '', type: '2' }, ...without('magit-lsp')], /not a regular file/],
      ['a hardlink binary', [{ name: binaryName, content: '', type: '1' }, ...without('magit-lsp')], /not a regular file/],
      ['a directory binary', [{ name: binaryName, content: '', type: '5' }, ...without('magit-lsp')], /not a regular file/],
      ['a duplicate binary', [{ name: binaryName, content: BINARY }, ...published], /duplicate archive member/],
      ['a missing binary', without('magit-lsp'), /archive is missing/],
      ['a missing license', without('LICENSE'), /archive is missing/],
      ['an empty binary', [{ name: binaryName, content: '' }, ...without('magit-lsp')], /invalid size/],
      [
        'an oversized license',
        [{ name: `${stem}/LICENSE`, content: 'x'.repeat(LIMITS.memberBytes + 1) }, ...without('LICENSE')],
        /invalid size/,
      ],
    ];
    for (const [label, members, pattern] of cases) {
      const root = temporaryRoot();
      try {
        const fixture = releaseFixture({ members });
        await rejects(
          resolveServer({ version: VERSION, cacheRoot: root, transport: fixture.transport, runtime: RUNTIME }),
          pattern,
        );
        assertNoInstall(entryOf(root));
      } catch (error) {
        error.message = `${label}: ${error.message}`;
        throw error;
      } finally {
        fs.rmSync(root, { recursive: true, force: true });
      }
    }
  });

  await check('malicious pax metadata cannot bypass archive checks', async () => {
    const stem = stemOf();
    const binaryName = `${stem}/magit-lsp`;
    const published = releaseMembers(stem, BINARY);
    const without = (name) => published.filter((member) => member.name !== `${stem}/${name}`);
    const cases = [
      [
        'a pax path override',
        [paxMember(paxRecord('path', '../magit-lsp')), ...published],
        /unsupported archive pax metadata "path"/,
      ],
      [
        'a pax linkpath override',
        [paxMember(paxRecord('linkpath', '/etc/passwd')), ...published],
        /unsupported archive pax metadata "linkpath"/,
      ],
      ['pax size metadata', [paxMember(paxRecord('size', '1')), ...published], /unsupported archive pax metadata "size"/],
      ['a pax record longer than its member', [paxMember('99 mtime=1\n'), ...published], /corrupt release archive pax header/],
      ['a pax record without a newline', [paxMember('12 mtime=1.5'), ...published], /corrupt release archive pax header/],
      ['an invalid pax mtime value', [paxMember(paxRecord('mtime', 'nan')), ...published], /corrupt release archive pax header/],
      [
        'a pax header naming another member',
        [paxMember(paxRecord('mtime', '1.5'), 'PaxHeaders/evil'), ...published],
        /unsupported archive pax member/,
      ],
      [
        'a pax header at end of archive',
        [...published, paxMember(paxRecord('mtime', '1.5'))],
        /corrupt release archive pax header/,
      ],
      [
        'a pax-wrapped symlink binary',
        [paxMember(paxRecord('mtime', '1.5')), { name: binaryName, content: '', type: '2' }, ...without('magit-lsp')],
        /not a regular file/,
      ],
      [
        'a pax-wrapped duplicate binary',
        [...published, paxMember(paxRecord('mtime', '1.5')), { name: binaryName, content: BINARY }],
        /duplicate archive member/,
      ],
    ];
    for (const [label, members, pattern] of cases) {
      const root = temporaryRoot();
      try {
        const fixture = releaseFixture({ members });
        await rejects(
          resolveServer({ version: VERSION, cacheRoot: root, transport: fixture.transport, runtime: RUNTIME }),
          pattern,
        );
        assertNoInstall(entryOf(root));
      } catch (error) {
        error.message = `${label}: ${error.message}`;
        throw error;
      } finally {
        fs.rmSync(root, { recursive: true, force: true });
      }
    }
  });

  await check('corrupt archives publish nothing', async () => {
    const tar = tarOf(releaseMembers(stemOf(), BINARY));
    const corruptedHeader = Buffer.from(tar);
    corruptedHeader[0] = 0x58;
    const cases = [
      ['not gzip', Buffer.from('not a gzip archive')],
      ['a truncated member', zlib.gzipSync(tar.subarray(0, 512 + 16))],
      ['a bad header checksum', zlib.gzipSync(corruptedHeader)],
    ];
    for (const [label, archive] of cases) {
      const root = temporaryRoot();
      try {
        const fixture = releaseFixture({ archive });
        await rejects(
          resolveServer({ version: VERSION, cacheRoot: root, transport: fixture.transport, runtime: RUNTIME }),
          /release archive/,
        );
        assertNoInstall(entryOf(root));
      } catch (error) {
        error.message = `${label}: ${error.message}`;
        throw error;
      } finally {
        fs.rmSync(root, { recursive: true, force: true });
      }
    }
  });

  await check('transport failures preserve good installs and remove tampered ones', async () => {
    const root = temporaryRoot();
    const failure = 'connection reset by peer';
    try {
      const good = releaseFixture();
      const first = await resolveServer({
        version: VERSION,
        cacheRoot: root,
        transport: good.transport,
        runtime: RUNTIME,
      });
      const failing = async () => {
        throw new Error(failure);
      };
      await rejects(
        resolveServer({ version: '0.1.1', cacheRoot: root, transport: failing, runtime: RUNTIME }),
        /connection reset by peer/,
      );
      assertNoInstall(entryOf(root, '0.1.1'));
      assert.deepEqual(fs.readFileSync(first.executable), BINARY, 'prior good version is preserved');
      fs.writeFileSync(first.executable, 'tampered binary\n');
      fs.chmodSync(first.executable, 0o755);
      await rejects(
        resolveServer({ version: VERSION, cacheRoot: root, transport: failing, runtime: RUNTIME }),
        /connection reset by peer/,
      );
      assert.equal(fs.existsSync(first.executable), false, 'tampered cache is never left launchable');
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('oversized downloads fail readably', async () => {
    const root = temporaryRoot();
    try {
      const repository = releaseFixture({ archive: Buffer.alloc(LIMITS.archiveBytes + 1) });
      await rejects(
        resolveServer({ version: VERSION, cacheRoot: root, transport: repository.transport, runtime: RUNTIME }),
        /exceeds the \d+-byte limit/,
      );
      assertNoInstall(entryOf(root));
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('real transport follows relative HTTPS redirects and cancels the redirect body', async () => {
    const root = temporaryRoot();
    try {
      /** @type {ReturnType<typeof trackedBody>[]} */
      const bodies = [];
      await withProductionFetch(
        async () => {
          const body = trackedBody({ chunks: [Buffer.from('stale redirect payload')] });
          bodies.push(body);
          if (bodies.length === 1) {
            return new Response(body.stream, { status: 302, headers: { location: '/moved' } });
          }
          return new Response(body.stream, { status: 200 });
        },
        async ({ calls }) => {
          await rejects(resolveProduction(root), /invalid checksum sidecar/);
          assert.deepEqual(calls, [`${RELEASE_BASE}/${archiveNameOf()}.sha256`, 'https://github.com/moved']);
          assert.equal(bodies[0].cancelled(), true, 'redirect body is released');
          assert.equal(bodies[1].cancelled(), false, 'the body that was read is not cancelled');
          assertNoInstall(entryOf(root));
        },
      );
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('real transport never fetches a downgrade target and cancels the redirect body', async () => {
    const root = temporaryRoot();
    try {
      /** @type {ReturnType<typeof trackedBody>[]} */
      const bodies = [];
      await withProductionFetch(
        async () => {
          const body = trackedBody({ chunks: [Buffer.from('redirect body')] });
          bodies.push(body);
          return new Response(body.stream, { status: 302, headers: { location: 'http://example.test/archive' } });
        },
        async ({ calls }) => {
          await rejects(resolveProduction(root), /refusing non-HTTPS download/);
          assert.equal(calls.length, 1, 'the downgraded target is never fetched');
          assert.equal(bodies[0].cancelled(), true, 'refused redirect body is released');
          assertNoInstall(entryOf(root));
        },
      );
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('real transport cancels every redirect body and stops at the redirect bound', async () => {
    const root = temporaryRoot();
    try {
      /** @type {ReturnType<typeof trackedBody>[]} */
      const bodies = [];
      await withProductionFetch(
        async () => {
          const body = trackedBody({ chunks: [Buffer.from('hop')] });
          bodies.push(body);
          return new Response(body.stream, { status: 302, headers: { location: '/loop' } });
        },
        async ({ calls }) => {
          await rejects(resolveProduction(root), /too many redirects/);
          assert.equal(calls.length, LIMITS.redirects + 1, 'stops at the redirect bound without another fetch');
          assert.equal(bodies.length, calls.length, 'one response per hop');
          assert.deepEqual(bodies.map((body) => body.cancelled()), bodies.map(() => true), 'every hop body released');
          assertNoInstall(entryOf(root));
        },
      );
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('real transport cancels a redirect body that has no location', async () => {
    const root = temporaryRoot();
    try {
      /** @type {ReturnType<typeof trackedBody>[]} */
      const bodies = [];
      await withProductionFetch(
        async () => {
          const body = trackedBody({ chunks: [Buffer.from('no location')] });
          bodies.push(body);
          return new Response(body.stream, { status: 302 });
        },
        async ({ calls }) => {
          await rejects(resolveProduction(root), /redirect without a location/);
          assert.equal(calls.length, 1);
          assert.equal(bodies[0].cancelled(), true, 'locationless redirect body is released');
          assertNoInstall(entryOf(root));
        },
      );
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('real transport cancels non-OK bodies and keeps the HTTP diagnostic when cancel fails', async () => {
    const root = temporaryRoot();
    try {
      /** @type {ReturnType<typeof trackedBody>[]} */
      const bodies = [];
      await withProductionFetch(
        async () => {
          const body = trackedBody({ chunks: [Buffer.from('error page')], failCancel: true });
          bodies.push(body);
          return new Response(body.stream, { status: 503 });
        },
        async () => {
          await assert.rejects(resolveProduction(root), (error) => {
            assert.equal(error.name, 'ServerDownloadError', `readable failure: ${error.message}`);
            assert.match(error.message, /HTTP 503/);
            assert.equal(error.message.includes('body cancellation failed'), false, 'primary diagnostic survives');
            return true;
          });
          assert.notEqual(bodies[0].cancelFailure(), undefined, 'the scenario exercised a failing cancellation');
          assert.equal(bodies[0].cancelled(), true, 'the refusal still attempts cancellation');
          assertNoInstall(entryOf(root));
        },
      );
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  await check('real transport cancels advertised and streamed oversized bodies', async () => {
    const advertised = temporaryRoot();
    try {
      /** @type {ReturnType<typeof trackedBody>[]} */
      const bodies = [];
      await withProductionFetch(
        async () => {
          const body = trackedBody({ chunks: [Buffer.from('x')] });
          bodies.push(body);
          return new Response(body.stream, {
            status: 200,
            headers: { 'content-length': String(LIMITS.sidecarBytes + 1) },
          });
        },
        async () => {
          await rejects(resolveProduction(advertised), /exceeds the \d+-byte limit/);
          assert.equal(bodies[0].cancelled(), true, 'advertised oversize body is released unread');
          assertNoInstall(entryOf(advertised));
        },
      );
    } finally {
      fs.rmSync(advertised, { recursive: true, force: true });
    }

    const streamed = temporaryRoot();
    const chunk = Buffer.alloc(1024, 0x61);
    try {
      /** @type {ReturnType<typeof trackedBody>[]} */
      const bodies = [];
      await withProductionFetch(
        async () => {
          const body = trackedBody({ chunks: Array.from({ length: 50 }, () => chunk) });
          bodies.push(body);
          return new Response(body.stream, { status: 200 });
        },
        async () => {
          await rejects(resolveProduction(streamed), /exceeds the \d+-byte limit/);
          assert.equal(bodies[0].cancelled(), true, 'streamed oversize body is released');
          assert.equal(bodies[0].delivered() < 50, true, 'the stream is not drained past the limit');
          assertNoInstall(entryOf(streamed));
        },
      );
    } finally {
      fs.rmSync(streamed, { recursive: true, force: true });
    }
  });

  await check('real transport honours the request clock while awaiting headers and body', async () => {
    const headersRoot = temporaryRoot();
    try {
      /** @type {() => void} */
      let startedHeaders;
      const headersStarted = new Promise((resolve) => {
        startedHeaders = () => resolve(undefined);
      });
      await withProductionFetch(
        (url, signal) =>
          new Promise((resolve, reject) => {
            startedHeaders();
            signal.addEventListener('abort', () => reject(signal.reason), { once: true });
          }),
        async ({ controllers }) => {
          const rejection = rejects(resolveProduction(headersRoot), /download failed/, /request clock expired/);
          await headersStarted;
          controllers.forEach((controller) => controller.abort(new Error('request clock expired')));
          await rejection;
          assertNoInstall(entryOf(headersRoot));
        },
      );
    } finally {
      fs.rmSync(headersRoot, { recursive: true, force: true });
    }

    const bodyRoot = temporaryRoot();
    try {
      /** @type {ReturnType<typeof trackedBody>[]} */
      const bodies = [];
      /** @type {() => void} */
      let startedBody;
      const bodyStarted = new Promise((resolve) => {
        startedBody = () => resolve(undefined);
      });
      await withProductionFetch(
        async (url, signal) => {
          const body = trackedBody({ stall: true, signal });
          bodies.push(body);
          startedBody();
          return new Response(body.stream, { status: 200 });
        },
        async ({ controllers }) => {
          const rejection = rejects(resolveProduction(bodyRoot), /cannot install magit-lsp/, /request clock expired/);
          await bodyStarted;
          controllers.forEach((controller) => controller.abort(new Error('request clock expired')));
          await rejection;
          assert.equal(bodies[0].cancelled(), false, 'the aborted body is errored, not drained');
          assertNoInstall(entryOf(bodyRoot));
        },
      );
    } finally {
      fs.rmSync(bodyRoot, { recursive: true, force: true });
    }
  });

  await check('automatic source prefers explicit path, matching PATH, then managed', async () => {
    const explicit = fakeExplicitServer();
    const pathFixture = fakePathServer();
    const managed = managedFixture();
    try {
      const selected = await resolveServerSource({
        source: 'auto',
        path: explicit.path,
        version: VERSION,
        cacheRoot: '/cache',
        env: pathFixture.env,
        managed: managed.resolve,
      });
      assert.deepEqual(selected,
        { command: explicit.path, origin: 'explicit', version: null },
        'a valid explicit path is used verbatim');
      assert.equal(fs.existsSync(pathFixture.marker), false, 'explicit path skips the PATH probe');
      assert.deepEqual(managed.calls, [], 'explicit path never resolves a managed release');

      const fromPath = await resolveServerSource({
        source: 'auto',
        path: '',
        version: VERSION,
        cacheRoot: '/cache',
        env: pathFixture.env,
        managed: managed.resolve,
      });
      assert.deepEqual(fromPath,
        { command: 'magit-lsp', origin: 'system', version: VERSION },
        'a PATH binary reporting the pinned version is used');
      assert.equal(fs.existsSync(pathFixture.marker), true, 'PATH discovery probes --version');
      assert.deepEqual(managed.calls, [], 'matching PATH binary never resolves a managed release');

      const empty = fs.mkdtempSync(path.join(os.tmpdir(), 'magit-server-no-path-'));
      try {
        const fallback = await resolveServerSource({
          source: 'auto',
          path: '',
          version: VERSION,
          cacheRoot: '/cache',
          env: { ...process.env, PATH: empty },
          managed: managed.resolve,
        });
        assert.deepEqual(fallback,
          { command: '/managed/cache/magit-lsp', origin: 'managed', version: VERSION },
          'no explicit path and no PATH binary falls back to managed');
      } finally {
        fs.rmSync(empty, { recursive: true, force: true });
      }
    } finally {
      explicit.remove();
      pathFixture.remove();
    }
  });

  await check('automatic source reports an invalid explicit path without fallback', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'magit-server-invalid-'));
    const pathFixture = fakePathServer();
    const managed = managedFixture();
    try {
      for (const configured of [path.join(root, 'missing'), root]) {
        await rejectsSource(
          resolveServerSource({
            source: 'auto',
            path: configured,
            version: VERSION,
            cacheRoot: '/cache',
            env: pathFixture.env,
            managed: managed.resolve,
          }),
          /configured server path/,
          new RegExp(configured.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')),
        );
      }
      assert.equal(fs.existsSync(pathFixture.marker), false, 'invalid path never probes PATH');
      assert.deepEqual(managed.calls, [], 'invalid path never starts a download');
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
      pathFixture.remove();
    }
  });

  await check('automatic source skips a mismatched PATH binary to managed', async () => {
    const path = fakePathServer({ version: '9.9.9' });
    const managed = managedFixture();
    try {
      const selected = await resolveServerSource({
        source: 'auto',
        path: '',
        version: VERSION,
        cacheRoot: '/cache',
        env: path.env,
        managed: managed.resolve,
      });
      assert.deepEqual(selected,
        { command: '/managed/cache/magit-lsp', origin: 'managed', version: VERSION },
        'a mismatched PATH binary is skipped');
      assert.deepEqual(managed.calls,
        [{ version: VERSION, cacheRoot: '/cache' }],
        'managed resolution names the pinned version');
    } finally {
      path.remove();
    }
  });

  await check('automatic source ignores unusable PATH binaries', async () => {
    for (const script of ['#!/bin/sh\nexit 3\n', '#!/bin/sh\necho not-a-server\n']) {
      const path = fakePathServer({ script });
      const managed = managedFixture();
      try {
        const selected = await resolveServerSource({
          source: 'auto',
          path: '',
          version: VERSION,
          cacheRoot: '/cache',
          env: path.env,
          managed: managed.resolve,
        });
        assert.equal(selected.origin, 'managed', 'an unverifiable PATH binary is not selected');
        assert.deepEqual(managed.calls, [{ version: VERSION, cacheRoot: '/cache' }]);
      } finally {
        path.remove();
      }
    }
  });

  await check('system source never downloads and reports PATH failures readably', async () => {
    const managed = managedFixture();
    const empty = fs.mkdtempSync(path.join(os.tmpdir(), 'magit-server-empty-path-'));
    const matched = fakePathServer();
    const mismatched = fakePathServer({ version: '9.9.9' });
    const failing = fakePathServer({ script: '#!/bin/sh\nexit 2\n' });
    const garbled = fakePathServer({ script: '#!/bin/sh\necho magit-lsp-not-a-version\n' });
    try {
      const selected = await resolveServerSource({
        source: 'system',
        path: '',
        version: VERSION,
        cacheRoot: '/cache',
        env: matched.env,
        managed: managed.resolve,
      });
      assert.deepEqual(selected,
        { command: 'magit-lsp', origin: 'system', version: VERSION },
        'a matching PATH binary is used in system mode');

      await rejectsSource(
        resolveServerSource({
          source: 'system',
          path: '',
          version: VERSION,
          cacheRoot: '/cache',
          env: { ...process.env, PATH: empty },
          managed: managed.resolve,
        }),
        /no magit-lsp was found on PATH/,
        /magit-lsp\.server\.path/,
        /magit-lsp\.server\.source/,
      );
      await rejectsSource(
        resolveServerSource({
          source: 'system',
          path: '',
          version: VERSION,
          cacheRoot: '/cache',
          env: mismatched.env,
          managed: managed.resolve,
        }),
        /reports version 9\.9\.9/,
        /requires 0\.1\.0/,
      );
      await rejectsSource(
        resolveServerSource({
          source: 'system',
          path: '',
          version: VERSION,
          cacheRoot: '/cache',
          env: failing.env,
          managed: managed.resolve,
        }),
        /cannot report its version/,
        /--version exited 2/,
      );
      await rejectsSource(
        resolveServerSource({
          source: 'system',
          path: '',
          version: VERSION,
          cacheRoot: '/cache',
          env: garbled.env,
          managed: managed.resolve,
        }),
        /cannot report its version/,
        /did not print "magit-lsp <X\.Y\.Z>"/,
      );
      assert.deepEqual(managed.calls, [], 'system source never resolves a managed release');
    } finally {
      fs.rmSync(empty, { recursive: true, force: true });
      matched.remove();
      mismatched.remove();
      failing.remove();
      garbled.remove();
    }
  });

  await check('managed source ignores the explicit path and resolves the pinned release', async () => {
    const explicit = fakeExplicitServer();
    const managed = managedFixture();
    try {
      const selected = await resolveServerSource({
        source: 'managed',
        path: explicit.path,
        version: VERSION,
        cacheRoot: '/cache',
        managed: managed.resolve,
      });
      assert.deepEqual(selected,
        { command: '/managed/cache/magit-lsp', origin: 'managed', version: VERSION },
        'managed mode ignores the explicit path');
      assert.deepEqual(managed.calls, [{ version: VERSION, cacheRoot: '/cache' }]);
    } finally {
      explicit.remove();
    }
  });

  await check('offline managed reuse never reaches the network', async () => {
    const root = temporaryRoot();
    const fixture = releaseFixture();
    try {
      const installed = await resolveServer({
        version: VERSION, cacheRoot: root, transport: fixture.transport, runtime: RUNTIME,
      });
      const originalFetch = globalThis.fetch;
      let fetches = 0;
      globalThis.fetch = () => {
        fetches += 1;
        throw new Error('offline resolution must not fetch');
      };
      try {
        const reused = await resolveServerSource({
          source: 'managed',
          path: '',
          version: VERSION,
          cacheRoot: root,
          runtime: RUNTIME,
        });
        assert.deepEqual(reused,
          { command: installed.executable, origin: 'managed', version: VERSION },
          'the previously installed pinned binary is reused');
        assert.equal(fetches, 0, 'resolution never calls the fetch boundary');
      } finally {
        globalThis.fetch = originalFetch;
      }
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  if (failures.length > 0) {
    const detail = failures
      .map((failure) => `FAIL: ${failure.name}\n${failure.error.stack ?? failure.error}`)
      .join('\n');
    throw new Error(`${failures.length} verified download checks failed\n${detail}`);
  }
  return passed;
}

module.exports = run;
