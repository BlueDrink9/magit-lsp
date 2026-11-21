#!/usr/bin/env node

// Regenerate `contributes.semanticTokenTypes` and `contributes.semanticTokenScopes`
// from the canonical server role table (src/semantic_roles.json).
//
// Usage: node clients/vscode/scripts/gen-semantic-roles.mjs [--check]
//
// `--check` compares instead of writing, for CI and for verifying that a
// manifest is current. Every other field in package.json is preserved.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const clientRoot = resolve(here, "..");
const repoRoot = resolve(clientRoot, "..", "..");
const canonicalRolesPath = resolve(repoRoot, "src", "semantic_roles.json");
const manifestPath = join(clientRoot, "package.json");

/** Language id the generated semantic token scopes apply to. */
const LANGUAGE_ID = "magit";

/**
 * Validate canonical role metadata, returning it unchanged.
 *
 * @param {unknown} roles parsed canonical JSON
 * @returns {{name: string, group: string, scope: string}[]} validated roles
 * @throws {Error} on malformed, empty or duplicate role metadata
 */
export function validateRoles(roles) {
  if (!Array.isArray(roles) || roles.length === 0) {
    throw new Error("semantic roles must be a non-empty array");
  }
  const seen = new Set();
  for (const [index, role] of roles.entries()) {
    if (role === null || typeof role !== "object" || Array.isArray(role)) {
      throw new Error(`semantic role ${index} must be an object`);
    }
    for (const field of ["name", "group", "scope"]) {
      if (typeof role[field] !== "string" || role[field].trim() === "") {
        throw new Error(`semantic role ${index} needs a non-empty string ${field}`);
      }
    }
    if (seen.has(role.name)) {
      throw new Error(`duplicate semantic role name: ${role.name}`);
    }
    seen.add(role.name);
  }
  return roles;
}

/**
 * Derive the two VSCode contribution fields from canonical roles.
 *
 * @param {{name: string, group: string, scope: string}[]} roles canonical roles
 * @returns {{semanticTokenTypes: object[], semanticTokenScopes: object[]}} derived contributions
 */
export function deriveSemanticContributions(roles) {
  const validated = validateRoles(roles);
  const scopes = {};
  for (const role of validated) scopes[role.name] = [role.scope];
  return {
    semanticTokenTypes: validated.map((role) => ({ id: role.name, description: role.group })),
    semanticTokenScopes: [{ language: LANGUAGE_ID, scopes }],
  };
}

/**
 * Return a copy of a manifest with only the derived contribution fields replaced.
 *
 * @param {object} pkg package manifest
 * @param {{name: string, group: string, scope: string}[]} roles canonical roles
 * @returns {object} a new manifest; the input is not mutated
 */
export function applySemanticRoles(pkg, roles) {
  const derived = deriveSemanticContributions(roles);
  const { contributions: _obsolete, ...manifest } = pkg;
  return {
    ...manifest,
    contributes: { ...(pkg.contributes || {}), ...derived },
  };
}

/**
 * Serialize a manifest deterministically for writing.
 *
 * @param {object} pkg package manifest
 * @returns {string} indented JSON with a trailing newline
 */
export function serializeManifest(pkg) {
  return `${JSON.stringify(pkg, null, 2)}\n`;
}

/**
 * Whether a manifest's derived contribution fields already match the roles.
 *
 * @param {object} pkg package manifest
 * @param {{name: string, group: string, scope: string}[]} roles canonical roles
 * @returns {boolean} true when both generated fields are current
 */
export function semanticRolesMatch(pkg, roles) {
  const derived = deriveSemanticContributions(roles);
  const contributed = pkg.contributes || {};
  return ["semanticTokenTypes", "semanticTokenScopes"].every(
    (field) => JSON.stringify(contributed[field]) === JSON.stringify(derived[field]),
  );
}

/**
 * Read roles and a manifest, then check or write the derived contribution fields.
 *
 * @param {{rolesPath?: string, manifestPath?: string, checkOnly?: boolean}} [options] file paths and mode
 * @returns {boolean} true when the manifest is current (or was written)
 */
export function generateSemanticRoles({
  rolesPath = canonicalRolesPath,
  manifestPath: target = manifestPath,
  checkOnly = false,
} = {}) {
  const roles = validateRoles(JSON.parse(readFileSync(rolesPath, "utf8")));
  const original = readFileSync(target, "utf8");
  const pkg = JSON.parse(original);
  if (checkOnly) {
    const current = semanticRolesMatch(pkg, roles) && !Object.hasOwn(pkg, "contributions");
    if (!current) {
      console.error(
        "contributes.semanticTokenTypes/semanticTokenScopes are stale; run node clients/vscode/scripts/gen-semantic-roles.mjs",
      );
    }
    return current;
  }
  const next = serializeManifest(applySemanticRoles(pkg, roles));
  if (next !== original) writeFileSync(target, next);
  return true;
}

/** Run the CLI against the default repository paths. */
function main() {
  const checkOnly = process.argv.includes("--check");
  const current = generateSemanticRoles({ checkOnly });
  if (current) {
    console.log(
      checkOnly ? "semantic roles are current" : `wrote semantic roles to ${manifestPath}`,
    );
  }
  process.exitCode = current ? 0 : 1;
}

// Run only when invoked as a script; importing must never write the manifest.
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  main();
}
