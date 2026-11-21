// Tests for the semantic-role declaration generator.
// Run with: node --test clients/vscode/test/semantic-roles.test.mjs

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  applySemanticRoles,
  deriveSemanticContributions,
  generateSemanticRoles,
  semanticRolesMatch,
  serializeManifest,
  validateRoles,
} from "../scripts/gen-semantic-roles.mjs";

/** A small sample role table used by the temporary-fixture tests. */
const SAMPLE_ROLES = [
  { name: "heading", group: "Title", scope: "entity.name.section" },
  { name: "muted", group: "Comment", scope: "comment" },
  { name: "added", group: "Added", scope: "markup.inserted" },
];

/** A manifest whose unrelated fields must survive generation untouched. */
function samplePkg() {
  return {
    name: "magit-lsp",
    version: "0.2.0",
    unrelated: { keep: true },
    contributes: {
      languages: [{ id: "magit" }],
      commands: [{ command: "magit.open" }],
      semanticTokenTypes: [{ id: "obsolete" }],
      semanticTokenScopes: [{ language: "magit", scopes: { obsolete: ["stale"] } }],
    },
    contributions: { semanticTokenTypes: [{ id: "wrong-root" }] },
  };
}

/**
 * Run a callback with temporary roles/manifest files, removing them afterwards.
 *
 * @param {(paths: {dir: string, rolesPath: string, manifestPath: string}) => void} body
 */
function withFixture(body) {
  const dir = mkdtempSync(join(tmpdir(), "magit-semantic-roles-"));
  try {
    const rolesPath = join(dir, "roles.json");
    const manifestPath = join(dir, "package.json");
    writeFileSync(rolesPath, `${JSON.stringify(SAMPLE_ROLES, null, 2)}\n`);
    writeFileSync(manifestPath, serializeManifest(samplePkg()));
    body({ dir, rolesPath, manifestPath });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test("maps every canonical name and scope into the magit contributions", () => {
  const rolesPath = new URL("../../../src/semantic_roles.json", import.meta.url);
  const roles = validateRoles(JSON.parse(readFileSync(rolesPath, "utf8")));
  const derived = deriveSemanticContributions(roles);
  const names = roles.map((role) => role.name);

  assert.deepEqual(
    derived.semanticTokenTypes.map((type) => type.id),
    names,
    "every canonical name is a token type",
  );
  assert.equal(derived.semanticTokenScopes.length, 1, "one scope contribution");
  assert.equal(
    derived.semanticTokenScopes[0].language,
    "magit",
    "scopes target the magit language",
  );
  const scopes = derived.semanticTokenScopes[0].scopes;
  assert.deepEqual(Object.keys(scopes), names, "every canonical name has a scope entry");
  for (const role of roles) {
    assert.deepEqual(scopes[role.name], [role.scope], `${role.name} maps to ${role.scope}`);
  }
  assert.deepEqual(
    derived.semanticTokenTypes.map((type) => type.description),
    roles.map((role) => role.group),
    "each token type description comes from its canonical group",
  );
});

test("derivation preserves unrelated package fields and does not mutate input", () => {
  const pkg = samplePkg();
  const before = structuredClone(pkg);
  const next = applySemanticRoles(pkg, SAMPLE_ROLES);
  const derived = deriveSemanticContributions(SAMPLE_ROLES);

  assert.deepEqual(pkg, before, "the source manifest is unchanged");
  assert.deepEqual(next.semanticTokenTypes, undefined, "top-level fields are not polluted");
  assert.equal(Object.hasOwn(next, "contributions"), false, "obsolete root is removed");
  assert.equal(next.name, pkg.name, "top-level name is preserved");
  assert.equal(next.version, pkg.version, "top-level version is preserved");
  assert.deepEqual(next.unrelated, { keep: true }, "unrelated top-level field is preserved");
  assert.deepEqual(
    next.contributes.languages,
    pkg.contributes.languages,
    "languages are preserved",
  );
  assert.deepEqual(next.contributes.commands, pkg.contributes.commands, "commands are preserved");
  assert.deepEqual(
    next.contributes.semanticTokenTypes,
    derived.semanticTokenTypes,
    "token types replaced",
  );
  assert.deepEqual(
    next.contributes.semanticTokenScopes,
    derived.semanticTokenScopes,
    "scopes replaced",
  );
  assert.match(serializeManifest(next), /\n$/, "serialized manifest ends with a newline");
});

test("rejects malformed, empty and duplicate role metadata", () => {
  const cases = [
    [null, /non-empty array/],
    [[], /non-empty array/],
    [{}, /non-empty array/],
    [[42], /must be an object/],
    [[{ group: "Title", scope: "comment" }], /name/],
    [[{ name: "", group: "Title", scope: "comment" }], /name/],
    [[{ name: "x", group: 1, scope: "comment" }], /group/],
    [[{ name: "x", group: "Title", scope: "  " }], /scope/],
    [
      [
        { name: "x", group: "Title", scope: "comment" },
        { name: "x", group: "Comment", scope: "comment" },
      ],
      /duplicate semantic role name: x/,
    ],
  ];
  for (const [input, pattern] of cases) {
    assert.throws(() => validateRoles(input), pattern, `rejects ${JSON.stringify(input)}`);
  }
  withFixture(({ rolesPath, manifestPath }) => {
    writeFileSync(rolesPath, JSON.stringify([SAMPLE_ROLES[0], { ...SAMPLE_ROLES[0] }]));
    assert.throws(
      () => generateSemanticRoles({ rolesPath, manifestPath }),
      /duplicate semantic role name/,
      "the generator validates roles read from disk",
    );
  });
});

test("generator writes only the derived fields and is idempotent", () => {
  withFixture(({ rolesPath, manifestPath }) => {
    assert.equal(
      generateSemanticRoles({ rolesPath, manifestPath }),
      true,
      "generator reports success",
    );
    const written = readFileSync(manifestPath, "utf8");
    const pkg = JSON.parse(written);
    const derived = deriveSemanticContributions(SAMPLE_ROLES);

    assert.deepEqual(
      pkg.contributes.semanticTokenTypes,
      derived.semanticTokenTypes,
      "types written",
    );
    assert.deepEqual(
      pkg.contributes.semanticTokenScopes,
      derived.semanticTokenScopes,
      "scopes written",
    );
    assert.deepEqual(pkg.unrelated, { keep: true }, "unrelated field survives");
    assert.deepEqual(pkg.contributes.languages, [{ id: "magit" }], "other contributions survive");
    assert.equal(Object.hasOwn(pkg, "contributions"), false, "obsolete root is removed");
    assert.equal(written, serializeManifest(pkg), "file is the serialized manifest");

    assert.equal(generateSemanticRoles({ rolesPath, manifestPath }), true, "second run is a no-op");
    assert.equal(
      readFileSync(manifestPath, "utf8"),
      written,
      "idempotent write leaves identical bytes",
    );
  });
});

test("checker detects a stale manifest, accepts current data and never writes", () => {
  withFixture(({ rolesPath, manifestPath }) => {
    const stale = readFileSync(manifestPath, "utf8");
    assert.equal(
      generateSemanticRoles({ rolesPath, manifestPath, checkOnly: true }),
      false,
      "obsolete fields are stale",
    );
    assert.equal(readFileSync(manifestPath, "utf8"), stale, "check did not write");

    generateSemanticRoles({ rolesPath, manifestPath });
    const current = readFileSync(manifestPath, "utf8");
    assert.equal(
      generateSemanticRoles({ rolesPath, manifestPath, checkOnly: true }),
      true,
      "derived data is current",
    );
    assert.equal(readFileSync(manifestPath, "utf8"), current, "successful check did not write");

    const tamperedScope = JSON.parse(current);
    tamperedScope.contributes.semanticTokenScopes[0].scopes.added = ["stale"];
    writeFileSync(manifestPath, serializeManifest(tamperedScope));
    assert.equal(
      generateSemanticRoles({ rolesPath, manifestPath, checkOnly: true }),
      false,
      "wrong scope is stale",
    );
    assert.equal(
      readFileSync(manifestPath, "utf8"),
      serializeManifest(tamperedScope),
      "stale check did not repair",
    );

    const tamperedType = JSON.parse(current);
    tamperedType.contributes.semanticTokenTypes.pop();
    writeFileSync(manifestPath, serializeManifest(tamperedType));
    assert.equal(
      semanticRolesMatch(tamperedType, SAMPLE_ROLES),
      false,
      "missing token type is stale",
    );
  });
});

test("importing the generator does not write the committed manifest", () => {
  const manifest = new URL("../package.json", import.meta.url);
  const before = readFileSync(manifest);
  const moduleUrl = new URL("../scripts/gen-semantic-roles.mjs", import.meta.url).href;
  const exports = execFileSync(
    process.execPath,
    [
      "--input-type=module",
      "-e",
      `import(${JSON.stringify(moduleUrl)}).then(m => process.stdout.write(Object.keys(m).sort().join(',')))`,
    ],
    { encoding: "utf8" },
  );
  assert.match(exports, /generateSemanticRoles/, "pure generator is exported");
  assert.deepEqual(readFileSync(manifest), before, "a bare import leaves package.json untouched");
});
