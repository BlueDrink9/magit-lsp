"use strict";

const fs = require("node:fs");

const { resolveServer } = require("../lib/server-download");

/**
 * Independent resolver process sharing one cache root with its parent test.
 *
 * The parent sends one `resolve` plan and, for the gated racing case, a single
 * `continue` message. Every transport call waits for that release, so the
 * parent controls the order in which two extension hosts attempt publication.
 * The process stays alive after reporting its outcome so the parent can read
 * the result; the parent kills it when the check finishes.
 */

let releaseGate;
const gate = new Promise((resolve) => {
  releaseGate = resolve;
});

/**
 * Report one result message to the parent.
 *
 * @param {object} message
 * @returns {void}
 */
function report(message) {
  if (process.send) process.send(message);
}

process.on("message", (message) => {
  if (message?.type === "continue") {
    releaseGate();
    return;
  }
  if (message?.type !== "resolve") return;
  run(message).catch((error) => {
    report({ type: "failed", name: error?.name, message: error?.message });
  });
});

/**
 * Resolve the server from the release bytes named in the plan.
 *
 * @param {{cacheRoot: string, version: string, runtime: object, archivePath: string, sidecarPath: string, failure?: string}} plan
 * @returns {Promise<void>}
 */
async function run(plan) {
  const archive = fs.readFileSync(plan.archivePath);
  const sidecar = fs.readFileSync(plan.sidecarPath);
  /** @type {import('../lib/server-download').Transport} */
  const transport = async (url) => {
    if (plan.failure) throw new Error(plan.failure);
    report({ type: "transport", url });
    await gate;
    return url.endsWith(".sha256") ? sidecar : archive;
  };
  try {
    const result = await resolveServer({
      version: plan.version,
      cacheRoot: plan.cacheRoot,
      transport,
      runtime: plan.runtime,
    });
    report({ type: "resolved", executable: result.executable, version: result.version });
  } catch (error) {
    report({ type: "failed", name: error?.name, message: error?.message });
  }
}
