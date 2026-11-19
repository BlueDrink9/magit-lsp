'use strict';

const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const zlib = require('node:zlib');

const { message } = require('./util');

/** GitHub repository publishing magit-lsp release archives. */
const REPOSITORY = 'BlueDrink9/magit-lsp';

/** The only published release asset: Linux x86_64 GNU with glibc 2.35+. */
const ASSET = 'x86_64-unknown-linux-gnu';

/** GNU libc version the released Linux binary requires. */
const MINIMUM_GLIBC = [2, 35];

/** Files every release archive contains below its versioned stem directory. */
const ARCHIVE_FILES = ['magit-lsp', 'LICENSE', 'LICENSE-APACHE', 'INSTALL.md'];

/** Member name Python's tarfile uses for the pax extended header it emits. */
const PAX_HEADER_NAME = '././@PaxHeader';

/**
 * Resource bounds for one resolution.
 *
 * The archive bound matches `scripts/install.sh`; the unpacked bound refuses a
 * gzip bomb because this client decompresses in memory instead of on disk.
 */
const LIMITS = Object.freeze({
  archiveBytes: 128 * 1024 * 1024,
  unpackedBytes: 320 * 1024 * 1024,
  binaryBytes: 256 * 1024 * 1024,
  memberBytes: 1024 * 1024,
  sidecarBytes: 4096,
  members: 8,
  redirects: 5,
  requestMilliseconds: 180000,
  lockAttempts: 150,
  lockWaitMilliseconds: 10,
});

/**
 * @typedef {(url: string, limit: number) => Promise<Buffer>} Transport
 * Download one release URL, returning its bytes and failing beyond `limit`.
 * Tests inject fixture bytes here; validation stays in this module.
 */

/** A readable, actionable failure while resolving or installing the server. */
class ServerDownloadError extends Error {
  /** @param {string} reason user-facing reason without the product prefix */
  constructor(reason) {
    super(`magit-lsp: ${reason}`);
    this.name = 'ServerDownloadError';
  }
}

/**
 * Throw a readable download failure.
 *
 * @param {string} reason
 * @returns {never}
 */
function fail(reason) {
  throw new ServerDownloadError(reason);
}

/**
 * Validate and normalize a released extension version.
 *
 * Accepts a stable `X.Y.Z` with an optional leading `v` and rejects everything
 * else, so no unvalidated text can name a cache path or release URL.
 *
 * @param {unknown} value version reported by the extension or its package
 * @returns {string} normalized `X.Y.Z`
 */
function releaseVersion(value) {
  if (typeof value !== 'string') fail('extension version must be a string');
  const version = value.startsWith('v') ? value.slice(1) : value;
  if (!/^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/.test(version)) {
    fail(`unsupported release version ${JSON.stringify(value)}; expected a stable X.Y.Z version`);
  }
  return version;
}

/**
 * Report the running GNU libc version.
 *
 * @returns {string|null} version such as `2.35`, or null under musl and non-Linux hosts
 */
function hostGlibc() {
  try {
    const report = /** @type {any} */ (process.report?.getReport?.());
    const version = report?.header?.glibcVersionRuntime;
    return typeof version === 'string' ? version : null;
  } catch {
    return null;
  }
}

/**
 * Decide whether a reported libc version satisfies the released binary.
 *
 * @param {unknown} value reported GNU libc version
 * @returns {boolean}
 */
function supportedGlibc(value) {
  const match = typeof value === 'string' ? /^(\d+)\.(\d+)/.exec(value) : null;
  if (!match) return false;
  const major = Number(match[1]);
  const minor = Number(match[2]);
  return major > MINIMUM_GLIBC[0] || (major === MINIMUM_GLIBC[0] && minor >= MINIMUM_GLIBC[1]);
}

/**
 * Pick the release asset for the reported host.
 *
 * Only the published Linux x86_64 GNU asset is ever selected. Any other host is
 * told how to point the client at a server it installed itself rather than
 * being handed a binary it cannot run.
 *
 * @param {{platform?: string, arch?: string, glibc?: string|null}} runtime
 * @returns {string} release asset target
 */
function releaseAsset(runtime) {
  const platform = runtime.platform ?? process.platform;
  const arch = runtime.arch ?? process.arch;
  const glibc = runtime.glibc === undefined ? hostGlibc() : runtime.glibc;
  const shape = `${platform}-${arch}`;
  const guidance = 'set "magit-lsp.server.path" to a magit-lsp executable instead';
  if (platform !== 'linux' || arch !== 'x64') {
    fail(`no release asset for ${shape}: only ${ASSET} is published; ${guidance}`);
  }
  if (!supportedGlibc(glibc)) {
    fail(
      `no release asset for ${shape}: GNU glibc ${MINIMUM_GLIBC.join('.')} or newer is ` +
        `required (found ${glibc || 'none'}); ${guidance}`,
    );
  }
  return ASSET;
}

/**
 * Hash bytes with SHA-256.
 *
 * @param {Buffer} bytes
 * @returns {string} lowercase hex digest
 */
function sha256(bytes) {
  return crypto.createHash('sha256').update(bytes).digest('hex');
}

/**
 * Cache directory owning one version and asset pair.
 *
 * @param {string} cacheRoot directory the extension owns
 * @param {string} version normalized release version
 * @param {string} asset release asset target
 * @returns {string}
 */
function cacheEntry(cacheRoot, version, asset) {
  return path.join(cacheRoot, 'server', `v${version}`, asset);
}

/**
 * Return a cached executable only when a manifest and its bytes still agree.
 *
 * The cached binary is re-verified on every resolution, so a tampered binary or
 * manifest is never launched.
 *
 * @param {string} entry cache directory for this version and asset
 * @param {string} version normalized release version
 * @returns {string|undefined} verified executable path
 */
function cachedExecutable(entry, version) {
  const executable = path.join(entry, 'magit-lsp');
  try {
    const stat = fs.lstatSync(executable);
    if (!stat.isFile()) return undefined;
    const manifest = JSON.parse(fs.readFileSync(path.join(entry, 'manifest.json'), 'utf8'));
    if (!manifest || manifest.version !== version || manifest.asset !== ASSET) return undefined;
    if (!/^[0-9a-f]{64}$/.test(manifest.digest) || manifest.size !== stat.size) return undefined;
    if (sha256(fs.readFileSync(executable)) !== manifest.digest) return undefined;
    return executable;
  } catch {
    return undefined;
  }
}

/**
 * Synchronously pause between bounded lock attempts.
 *
 * Publication is synchronous once the download completes, so a promise-based
 * delay could not let other work in this process make progress anyway.
 *
 * @param {number} milliseconds
 * @returns {void}
 */
function pause(milliseconds) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);
}

/**
 * Path of the per-entry publication lock shared by every extension host.
 *
 * The lock is a sibling of the entry, so it never becomes part of a published
 * install and no staging directory is mistaken for it.
 *
 * @param {string} entry cache directory for this version and asset
 * @returns {string}
 */
function entryLock(entry) {
  return `${entry}.lock`;
}

/**
 * Remove a lock whose recorded owner is a dead process on this host.
 *
 * A lock is never taken from a live owner, and elapsed time alone never expires
 * one: a crashed publisher is recognised only from its recorded hostname and
 * process id, so a slow or suspended publisher keeps ownership. A lock whose
 * owner cannot be identified (another host, a corrupt record) is left alone.
 *
 * @param {string} lock lock directory
 * @returns {boolean} whether a stale lock was removed
 */
function removeStaleLock(lock) {
  let owner;
  try {
    const [host, pid] = fs.readFileSync(path.join(lock, 'owner'), 'utf8').trim().split(/\s+/);
    owner = { host, pid: Number(pid) };
  } catch {
    return false;
  }
  if (owner.host !== os.hostname() || !Number.isSafeInteger(owner.pid) || owner.pid <= 0) return false;
  try {
    process.kill(owner.pid, 0);
    return false;
  } catch (error) {
    if (error.code !== 'ESRCH') return false;
  }
  try {
    fs.rmSync(lock, { recursive: true, force: true });
    return true;
  } catch {
    return false;
  }
}

/**
 * Acquire the bounded per-entry publication lock.
 *
 * Publication mutates only one entry directory, so a short lock is enough to
 * serialize invalid-entry removal and the publishing rename between extension
 * hosts. Waiting is bounded; a publisher that never gets the lock fails
 * readably instead of racing a live owner.
 *
 * @param {string} entry cache directory for this version and asset
 * @returns {string} lock directory now owned by this attempt
 */
function acquireEntryLock(entry) {
  const lock = entryLock(entry);
  for (let attempt = 0; attempt < LIMITS.lockAttempts; attempt += 1) {
    try {
      fs.mkdirSync(lock);
      try {
        fs.writeFileSync(path.join(lock, 'owner'), `${os.hostname()} ${process.pid}\n`);
      } catch {}
      return lock;
    } catch (error) {
      if (error.code !== 'EEXIST') throw error;
    }
    if (removeStaleLock(lock)) continue;
    pause(LIMITS.lockWaitMilliseconds);
  }
  fail(`timed out waiting for another process to finish installing magit-lsp; remove ${lock} if no other VS Code window is installing`);
}

/**
 * Run one action while owning the entry publication lock.
 *
 * @template T
 * @param {string} entry cache directory for this version and asset
 * @param {() => T} action
 * @returns {T}
 */
function withEntryLock(entry, action) {
  const lock = acquireEntryLock(entry);
  try {
    return action();
  } finally {
    try {
      fs.rmSync(lock, { recursive: true, force: true });
    } catch {}
  }
}

/**
 * Delete an unverified cached install so it can never be launched.
 *
 * Only this entry's known files are removed, and only under the entry lock, so
 * a competing publisher that just completed a good install is re-read and
 * reused instead of deleted. An entry directory holding anything else is left
 * intact and reported, never swept. Staging directories live beside the entry
 * now, so no other process's staging is ever touched here.
 *
 * @param {string} entry cache directory for this version and asset
 * @param {string} version normalized release version
 * @returns {void}
 */
function removeInvalidEntry(entry, version) {
  if (cachedExecutable(entry, version)) return;
  for (const name of ['magit-lsp', 'manifest.json']) {
    try {
      fs.rmSync(path.join(entry, name), { force: true });
    } catch {}
  }
  try {
    fs.rmdirSync(entry);
  } catch (error) {
    if (error.code === 'ENOENT') return;
    if (error.code === 'ENOTEMPTY' || error.code === 'EEXIST') {
      fail(`cannot replace the cached magit-lsp ${version}: ${entry} contains unexpected files`);
    }
    throw error;
  }
}

/**
 * Escape text for use in a regular expression.
 *
 * @param {string} text
 * @returns {string}
 */
function escapePattern(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Read the strict SHA-256 sidecar published next to a release archive.
 *
 * @param {Buffer} sidecar fetched sidecar bytes
 * @param {string} archiveName archive file name the sidecar must name
 * @returns {string} expected lowercase hex digest
 */
function parseSidecar(sidecar, archiveName) {
  if (sidecar.length === 0) fail(`empty checksum sidecar for ${archiveName}`);
  const pattern = new RegExp(`^([0-9a-f]{64}) {2}${escapePattern(archiveName)}\\n$`);
  const match = pattern.exec(sidecar.toString('utf8'));
  if (!match) fail(`invalid checksum sidecar for ${archiveName}`);
  return match[1];
}

/**
 * Accept only bounded byte payloads from the external transport.
 *
 * @param {unknown} value transport result
 * @param {number} limit maximum accepted length
 * @param {string} label payload named in the failure message
 * @returns {Buffer}
 */
function checkedBytes(value, limit, label) {
  if (!Buffer.isBuffer(value)) fail(`${label} is not bytes`);
  if (value.length > limit) fail(`${label} exceeds the ${limit}-byte limit`);
  return value;
}

/**
 * Read one NUL-terminated ASCII field from a tar header.
 *
 * @param {Buffer} header 512-byte header
 * @param {number} offset field offset
 * @param {number} length field length
 * @returns {string}
 */
function tarField(header, offset, length) {
  const field = header.subarray(offset, offset + length);
  const end = field.indexOf(0);
  return (end === -1 ? field : field.subarray(0, end)).toString('utf8');
}

/**
 * Read one octal tar header field.
 *
 * @param {Buffer} header 512-byte header
 * @param {number} offset field offset
 * @param {number} length field length
 * @returns {number}
 */
function tarNumber(header, offset, length) {
  const text = tarField(header, offset, length).trim();
  if (!/^[0-7]+$/.test(text)) fail('corrupt release archive header');
  const value = Number.parseInt(text, 8);
  if (!Number.isSafeInteger(value)) fail('corrupt release archive header');
  return value;
}

/**
 * Validate the narrow pax metadata the release packager emits.
 *
 * `scripts/package-release.sh` writes archives with Python's default tarfile
 * format, which emits a `././@PaxHeader` record whenever a member has a
 * non-integer mtime. Only the `mtime` key is accepted, and its value is never
 * applied to the member: names, types and sizes always come from the tar header
 * itself, so pax metadata cannot redirect or resize a member.
 *
 * @param {Buffer} content pax extended header payload
 * @returns {void}
 */
function parsePaxMetadata(content) {
  let offset = 0;
  while (offset < content.length) {
    const space = content.indexOf(0x20, offset);
    if (space === -1) fail('corrupt release archive pax header');
    const lengthText = content.toString('ascii', offset, space);
    if (!/^[1-9][0-9]*$/.test(lengthText)) fail('corrupt release archive pax header');
    const length = Number.parseInt(lengthText, 10);
    if (!Number.isSafeInteger(length) || length < space - offset + 4) {
      fail('corrupt release archive pax header');
    }
    if (offset + length > content.length) fail('corrupt release archive pax header');
    const record = content.subarray(space + 1, offset + length);
    if (record[record.length - 1] !== 0x0a) fail('corrupt release archive pax header');
    const body = record.subarray(0, record.length - 1);
    const equals = body.indexOf(0x3d);
    if (equals <= 0) fail('corrupt release archive pax header');
    const key = body.toString('ascii', 0, equals);
    const value = body.toString('utf8', equals + 1);
    if (key !== 'mtime') fail(`unsupported archive pax metadata ${JSON.stringify(key)}`);
    if (!/^-?[0-9]+(?:\.[0-9]+)?$/.test(value)) fail('corrupt release archive pax header');
    offset += length;
  }
}

/**
 * @typedef {object} TarMember
 * @property {string} name member path
 * @property {string} type member type flag
 * @property {number} size declared content length
 * @property {Buffer} content member bytes
 */

/**
 * Split an uncompressed tar archive into members, validating every header.
 *
 * This deliberately implements only the format `scripts/package-release.sh`
 * writes: POSIX ustar regular files, plus the `mtime`-only pax extended header
 * Python's default tarfile format adds for non-integer mtimes. No member path is
 * ever used as a filesystem path, so traversal, absolute paths and link members
 * are refused.
 *
 * @param {Buffer} buffer uncompressed archive bytes
 * @returns {TarMember[]}
 */
function tarMembers(buffer) {
  /** @type {TarMember[]} */
  const members = [];
  let offset = 0;
  let paxPending = false;
  while (offset + 512 <= buffer.length) {
    const header = buffer.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) break;
    const expected = tarNumber(header, 148, 8);
    let checksum = 0;
    for (let index = 0; index < 512; index += 1) {
      checksum += index >= 148 && index < 156 ? 0x20 : header[index];
    }
    if (checksum !== expected) fail('corrupt release archive header');
    if (tarField(header, 257, 6) !== 'ustar') fail('unsupported release archive format');
    if (members.length >= LIMITS.members) fail('release archive has too many members');
    const size = tarNumber(header, 124, 12);
    if (size < 0 || offset + 512 + size > buffer.length) fail('release archive is truncated');
    const prefix = tarField(header, 345, 155);
    const name = prefix ? `${prefix}/${tarField(header, 0, 100)}` : tarField(header, 0, 100);
    const type = String.fromCharCode(header[156] === 0 ? 0x30 : header[156]);
    if (type === 'x') {
      if (paxPending) fail('corrupt release archive pax header');
      if (name !== PAX_HEADER_NAME) fail(`unsupported archive pax member ${JSON.stringify(name)}`);
      if (size <= 0 || size > LIMITS.memberBytes) fail('invalid size for archive pax header');
      parsePaxMetadata(buffer.subarray(offset + 512, offset + 512 + size));
      paxPending = true;
      offset += 512 + Math.ceil(size / 512) * 512;
      continue;
    }
    paxPending = false;
    members.push({ name, type, size, content: buffer.subarray(offset + 512, offset + 512 + size) });
    offset += 512 + Math.ceil(size / 512) * 512;
  }
  if (paxPending) fail('corrupt release archive pax header');
  if (members.length === 0) fail('release archive is empty');
  return members;
}

/**
 * Extract the single verified magit-lsp binary from a release archive.
 *
 * The archive must contain exactly the published file set as plain regular
 * files; duplicates, links, directories, unexpected paths and bad sizes fail.
 *
 * @param {Buffer} archive compressed release archive
 * @param {string} stem versioned archive directory
 * @returns {Buffer} binary bytes
 */
function extractBinary(archive, stem) {
  let tar;
  try {
    tar = zlib.gunzipSync(archive, { maxOutputLength: LIMITS.unpackedBytes });
  } catch (error) {
    fail(`corrupt release archive: ${message(error)}`);
  }
  const binaryName = `${stem}/magit-lsp`;
  const expected = new Set(ARCHIVE_FILES.map((name) => `${stem}/${name}`));
  const seen = new Set();
  /** @type {Buffer|undefined} */
  let binary;
  for (const member of tarMembers(tar)) {
    if (!expected.has(member.name)) fail(`unexpected archive member ${JSON.stringify(member.name)}`);
    if (member.type !== '0') fail(`archive member ${member.name} is not a regular file`);
    if (seen.has(member.name)) fail(`duplicate archive member ${member.name}`);
    seen.add(member.name);
    const limit = member.name === binaryName ? LIMITS.binaryBytes : LIMITS.memberBytes;
    if (member.size <= 0 || member.size > limit) fail(`invalid size for archive member ${member.name}`);
    if (member.name === binaryName) binary = member.content;
  }
  const missing = [...expected].filter((name) => !seen.has(name));
  if (missing.length > 0) fail(`archive is missing ${missing.join(', ')}`);
  if (!binary) fail('archive is missing the magit-lsp binary');
  return binary;
}

/**
 * Release a response body this download will not read.
 *
 * Cancellation is best effort: a transport that fails to cancel must never
 * replace the readable reason the response is being rejected for.
 *
 * @param {Response} response unused response
 * @returns {Promise<void>}
 */
async function discardResponse(response) {
  const body = response.body;
  if (!body) return;
  try {
    await body.cancel();
  } catch {}
}

/**
 * Download one URL over HTTPS with redirect, size and time bounds.
 *
 * This is the production transport. Redirects are followed manually so every
 * hop is required to stay on HTTPS, bounded in number and bounded in time.
 * Every response whose body is not returned is cancelled before the redirect is
 * followed or the refusal is reported, so no socket or background transfer
 * outlives the decision.
 *
 * @param {string} url release URL from this repository
 * @param {number} limit maximum accepted bytes
 * @returns {Promise<Buffer>}
 */
async function fetchBytes(url, limit) {
  let current = new URL(url);
  for (let hop = 0; hop <= LIMITS.redirects; hop += 1) {
    if (current.protocol !== 'https:') fail(`refusing non-HTTPS download of ${current.href}`);
    /** @type {Response} */
    let response;
    try {
      response = await fetch(current, {
        redirect: 'manual',
        signal: AbortSignal.timeout(LIMITS.requestMilliseconds),
      });
    } catch (error) {
      fail(`download failed for ${current.href}: ${message(error)}`);
    }
    if (response.status >= 300 && response.status < 400) {
      const location = response.headers.get('location');
      await discardResponse(response);
      if (!location) fail(`redirect without a location for ${current.href}`);
      try {
        current = new URL(location, current);
      } catch {
        fail(`invalid redirect target from ${current.href}`);
      }
      continue;
    }
    if (!response.ok) {
      await discardResponse(response);
      fail(`download failed: HTTP ${response.status} for ${current.href}`);
    }
    const advertised = Number(response.headers.get('content-length'));
    if (Number.isFinite(advertised) && advertised > limit) {
      await discardResponse(response);
      fail(`download exceeds the ${limit}-byte limit for ${current.href}`);
    }
    /** @type {Buffer[]} */
    const chunks = [];
    let total = 0;
    for await (const chunk of /** @type {any} */ (response.body)) {
      total += chunk.length;
      // Stopping early releases the stream through the iterator's cancellation.
      if (total > limit) break;
      chunks.push(Buffer.from(chunk));
    }
    if (total > limit) fail(`download exceeds the ${limit}-byte limit for ${current.href}`);
    return Buffer.concat(chunks, total);
  }
  fail(`too many redirects for ${url}`);
}

/**
 * Download, verify and atomically publish one release executable.
 *
 * Every check happens before anything is written. The verified binary and its
 * manifest are staged together in a uniquely owned sibling directory and the
 * complete entry is published by renaming that one directory into the entry
 * path, under a bounded per-entry lock. A competing extension host that
 * published first is re-verified and reused, never replaced or deleted, and a
 * failure leaves no partial entry and removes only this attempt's staging.
 *
 * Extraction uses Node's built-in `zlib` and a minimal ustar reader (plus the
 * mtime-only pax extended header Python's default tarfile format emits) rather
 * than a tar dependency: the published archive is produced by
 * `scripts/package-release.sh`, and reusing that exact file set as the only
 * accepted structure keeps the format surface small without new dependencies.
 *
 * @param {string} entry cache directory for this version and asset
 * @param {string} version normalized release version
 * @param {string} asset validated release asset target
 * @param {Transport} transport external download boundary
 * @returns {Promise<string>} published executable path
 */
async function install(entry, version, asset, transport) {
  const stem = `magit-lsp-v${version}-${asset}`;
  const archiveName = `${stem}.tar.gz`;
  const base = `https://github.com/${REPOSITORY}/releases/download/v${version}`;
  const parent = path.dirname(entry);
  /** @type {string|undefined} */
  let staging;
  try {
    const sidecar = checkedBytes(
      await transport(`${base}/${archiveName}.sha256`, LIMITS.sidecarBytes),
      LIMITS.sidecarBytes,
      'checksum sidecar',
    );
    const digest = parseSidecar(sidecar, archiveName);
    const archive = checkedBytes(
      await transport(`${base}/${archiveName}`, LIMITS.archiveBytes),
      LIMITS.archiveBytes,
      'release archive',
    );
    if (sha256(archive) !== digest) fail(`archive checksum does not match ${archiveName}.sha256`);
    const binary = extractBinary(archive, stem);
    fs.mkdirSync(parent, { recursive: true });
    staging = fs.mkdtempSync(path.join(parent, '.staging-'));
    const stagedBinary = path.join(staging, 'magit-lsp');
    fs.writeFileSync(stagedBinary, binary);
    fs.chmodSync(stagedBinary, 0o755);
    const handle = fs.openSync(stagedBinary, 'r');
    try {
      fs.fsyncSync(handle);
    } finally {
      fs.closeSync(handle);
    }
    const manifest = { version, asset, size: binary.length, digest: sha256(binary) };
    fs.writeFileSync(path.join(staging, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
    return withEntryLock(entry, () => {
      const winner = cachedExecutable(entry, version);
      if (winner) return winner;
      removeInvalidEntry(entry, version);
      fs.renameSync(staging, entry);
      staging = undefined;
      return path.join(entry, 'magit-lsp');
    });
  } catch (error) {
    throw error instanceof ServerDownloadError
      ? error
      : new ServerDownloadError(`cannot install magit-lsp ${version}: ${message(error)}`);
  } finally {
    if (staging !== undefined) {
      try {
        fs.rmSync(staging, { recursive: true, force: true });
      } catch {}
    }
  }
}

/** @type {Map<string, Promise<string>>} */
const inFlight = new Map();

/**
 * @typedef {object} ResolveOptions
 * @property {string} version extension version released for this client
 * @property {string} cacheRoot directory the extension owns for cached servers
 * @property {Transport} [transport] external download boundary; tests inject fixtures here
 * @property {{platform?: string, arch?: string, glibc?: string|null}} [runtime] reported host facts
 */

/**
 * Resolve a verified magit-lsp executable for one extension version.
 *
 * The release tag is exactly the requested version, never `latest`. Only the
 * published Linux x86_64 GNU asset is installed; any other host fails with
 * guidance to set `magit-lsp.server.path`. A cached binary is re-verified before
 * it is returned, concurrent calls in this process share one download, and
 * extension hosts sharing the cache publish complete entries by directory
 * rename under a bounded per-entry lock, so a failed download leaves no partial
 * files and no unverified binary behind.
 *
 * @param {ResolveOptions} options
 * @returns {Promise<{executable: string, version: string}>}
 */
async function resolveServer(options) {
  const version = releaseVersion(options?.version);
  const cacheRoot = options?.cacheRoot;
  if (typeof cacheRoot !== 'string' || cacheRoot.trim() === '') {
    fail('a cache directory is required');
  }
  const asset = releaseAsset(options?.runtime ?? {});
  const entry = cacheEntry(cacheRoot, version, asset);
  const cached = cachedExecutable(entry, version);
  if (cached) return { executable: cached, version };
  const transport = options?.transport ?? fetchBytes;
  let pending = inFlight.get(entry);
  if (!pending) {
    if (fs.existsSync(entry)) {
      // Best effort before downloading: an invalid entry must never stay
      // launchable if this attempt fails. The lock re-check keeps a competing
      // host's freshly published winner from being deleted; publication
      // repeats the check authoritatively, so a failure here is not fatal.
      try {
        withEntryLock(entry, () => removeInvalidEntry(entry, version));
      } catch {}
    }
    // Re-check after clearing so an install that finished while we waited for
    // the lock is reused instead of downloaded again.
    const settled = cachedExecutable(entry, version);
    if (settled) return { executable: settled, version };
    pending = install(entry, version, asset, transport);
    inFlight.set(entry, pending);
    pending.then(
      () => inFlight.delete(entry),
      () => inFlight.delete(entry),
    );
  }
  const executable = await pending;
  return { executable, version };
}

exports.LIMITS = LIMITS;
exports.ServerDownloadError = ServerDownloadError;
exports.releaseVersion = releaseVersion;
exports.resolveServer = resolveServer;
