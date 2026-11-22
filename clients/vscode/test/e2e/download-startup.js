"use strict";

/**
 * Pre-seed the isolated profile's server cache for the automatic startup test.
 *
 * The extension resolves its release through the production helper below its
 * own global storage. This module installs exactly that version and asset into
 * a fixture archive containing the real built binary, using the same producer
 * code and injected transport as `test/integration.js`, so the native run never
 * needs a published or network-reachable release.
 */

const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

const root = path.resolve(__dirname, "../../../..");
const FIXTURE_PROGRAM = path.join(root, "scripts", "test_release_archive_fixture.py");
const ASSET = "x86_64-unknown-linux-gnu";
const EXTENSION_ID = "magit-lsp.magit-lsp";

/**
 * Hex SHA-256 of bytes.
 *
 * @param {Buffer} bytes
 * @returns {string}
 */
function digest(bytes) {
  return crypto.createHash("sha256").update(bytes).digest("hex");
}

/**
 * Build a release archive around the real server binary with the packager's
 * Python tarfile settings.
 *
 * @param {string} binary path to `magit-lsp`
 * @param {string} stem versioned archive directory
 * @param {string} archiveName published archive file name
 * @returns {Buffer} gzip release archive bytes
 */
function packagedArchive(binary, stem, archiveName) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "magit-lsp-download-startup-archive-"));
  try {
    const binaryPath = path.join(directory, "magit-lsp");
    fs.copyFileSync(binary, binaryPath);
    const archivePath = path.join(directory, archiveName);
    execFileSync("python3", [FIXTURE_PROGRAM, archivePath, stem, binaryPath], { stdio: "pipe" });
    return fs.readFileSync(archivePath);
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

/**
 * Build the one release payload an isolated run may download.
 *
 * The archive is produced from the supplied binary with the packager's Python
 * tarfile settings, so a forced reinstall can be served entirely offline while
 * still exercising the production checksum and extraction path.
 *
 * @param {string} binary path to `magit-lsp`
 * @returns {{version: string, stem: string, archiveName: string, base: string,
 *   archive: Buffer, sidecar: string}}
 */
function releasePayload(binary) {
  const version = JSON.parse(
    fs.readFileSync(path.join(root, "clients/vscode/package.json"), "utf8"),
  ).version;
  const stem = `magit-lsp-v${version}-${ASSET}`;
  const archiveName = `${stem}.tar.gz`;
  const archive = packagedArchive(binary, stem, archiveName);
  return {
    version,
    stem,
    archiveName,
    base: `https://github.com/BlueDrink9/magit-lsp/releases/download/v${version}`,
    archive,
    sidecar: `${digest(archive)}  ${archiveName}\n`,
  };
}

/**
 * Install the extension's pinned release into one isolated profile's cache.
 *
 * The cache root is the exact directory the extension passes as its global
 * storage, so the returning executable path doubles as the expected launch
 * command. Any transport request other than the fixture archive and sidecar
 * fails, because an automatic startup must reuse the seeded entry instead of
 * downloading.
 *
 * @param {{directory: string, binary: string}} options isolated profile root and built server
 * @returns {Promise<{version: string, asset: string, cacheRoot: string, executable: string}>}
 */
async function prepareDownloadCache({
  directory,
  binary,
  artifact,
  manifest: manifestPath,
  sidecar: sidecarPath,
}) {
  let payload;
  let runtime;
  if (artifact || manifestPath || sidecarPath) {
    assert.ok(
      artifact && manifestPath,
      "artifact mode requires RELEASE_ARTIFACT and RELEASE_MANIFEST",
    );
    const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
    const archive = fs.readFileSync(artifact);
    const archiveName = path.basename(artifact);
    const host = { platform: process.platform, arch: process.arch };
    const assetByHost = {
      "linux-x64": "x86_64-unknown-linux-gnu",
      "darwin-arm64": "aarch64-apple-darwin",
      "darwin-x64": "x86_64-apple-darwin",
    };
    const asset = assetByHost[`${host.platform}-${host.arch}`];
    // Strip the runtime's exact asset suffix instead of splitting on the last
    // dash: the asset itself contains dashes, and the version may use any form
    // the resolver accepts (including a prerelease).
    const prefix = "magit-lsp-v";
    const suffix = asset ? `-${asset}.tar.gz` : "";
    assert.ok(
      suffix && archiveName.startsWith(prefix) && archiveName.endsWith(suffix),
      "artifact filename and extension-host runtime must be supported",
    );
    const version = archiveName.slice(prefix.length, -suffix.length);
    assert.ok(version.length > 0, "artifact filename must name a version");
    assert.equal(manifest.target, asset, "manifest target must match the extension-host runtime");
    assert.equal(manifest.version, version, "manifest version must match the archive filename");
    assert.equal(
      digest(archive),
      manifest.archive_sha256,
      "archive bytes must match the manifest hash",
    );
    const sidecar = fs.readFileSync(sidecarPath || `${artifact}.sha256`, "utf8");
    payload = {
      version: manifest.version,
      stem: `magit-lsp-v${manifest.version}-${asset}`,
      archiveName,
      base: `https://github.com/BlueDrink9/magit-lsp/releases/download/v${manifest.version}`,
      archive,
      sidecar,
      asset,
      executableHash: manifest.executable_sha256,
    };
    runtime = { ...host };
    if (host.platform === "linux")
      runtime.glibc = process.report?.getReport?.().header?.glibcVersionRuntime ?? null;
  } else {
    payload = releasePayload(binary);
    runtime = { platform: "linux", arch: "x64", glibc: "2.36" };
  }
  const cacheRoot = path.join(directory, "user", "User", "globalStorage", EXTENSION_ID);
  const { resolveServer } = require("../../lib/server-download.js");
  const resolved = await resolveServer({
    version: payload.version,
    cacheRoot,
    runtime,
    transport: async (url) => {
      if (url === `${payload.base}/${payload.archiveName}.sha256`) {
        return Buffer.from(payload.sidecar);
      }
      if (url === `${payload.base}/${payload.archiveName}`) return payload.archive;
      throw new Error(`unexpected download ${url}`);
    },
  });
  assert.equal(resolved.version, payload.version, "the seeded cache names the extension version");
  assert.equal(
    resolved.executable.startsWith(`${cacheRoot}${path.sep}`),
    true,
    "the production helper installs the server below the extension cache root",
  );
  const executableHash = digest(fs.readFileSync(resolved.executable));
  if (payload.executableHash)
    assert.equal(executableHash, payload.executableHash, "cached executable matches manifest");
  return {
    version: payload.version,
    asset: payload.asset || ASSET,
    cacheRoot,
    executable: resolved.executable,
    archiveHash: digest(payload.archive),
    executableHash,
  };
}

exports.prepareDownloadCache = prepareDownloadCache;
exports.releasePayload = releasePayload;
