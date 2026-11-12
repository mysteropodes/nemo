'use strict';
// P09 — negative controls for the SHIPPED capability/schema drift checker.
//
// Every control drives scripts/nemo/lib/capability-drift.cjs itself, the same
// module `npm run check` runs. A test-only re-implementation would prove only
// that the test agrees with itself; the point of this leaf is that the gate an
// author already runs fails on drift.
//
// Controls mutate a COPY of the tree under os.tmpdir(), never the working tree.
// Proving a gate fails by editing real files is how a crashed run leaves a dirty
// checkout behind — and this suite has to be safe to interrupt.
const { test } = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const drift = require('../scripts/nemo/lib/capability-drift.cjs');

const REPO = path.join(__dirname, '..');
const APP = path.join('engineering', 'application');
const RUST = path.join('nemo-mcp', 'src', 'capabilities.rs');
const CLIENT = path.join('src-tauri', 'src', 'native_object_client.rs');

// A faithful copy of exactly the inputs the checker reads.
function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-p09-'));
  fs.mkdirSync(path.join(root, APP), { recursive: true });
  fs.cpSync(path.join(REPO, APP), path.join(root, APP), { recursive: true });
  fs.mkdirSync(path.join(root, path.dirname(RUST)), { recursive: true });
  fs.cpSync(path.join(REPO, RUST), path.join(root, RUST));
  fs.mkdirSync(path.join(root, path.dirname(CLIENT)), { recursive: true });
  fs.cpSync(path.join(REPO, CLIENT), path.join(root, CLIENT));
  return root;
}

function withFixture(fn) {
  const root = fixture();
  try { return fn(root); } finally { fs.rmSync(root, { recursive: true, force: true }); }
}

function capPath(root, name) { return path.join(root, APP, 'capabilities', name); }
function readJson(p) { return JSON.parse(fs.readFileSync(p, 'utf8')); }
function writeJson(p, v) { fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n'); }

// ---- the tree as committed --------------------------------------------
// If this fails, the repository has drifted and one of the controls below is
// no longer measuring what it claims to.
test('every declared check passes against the working tree', () => {
  const { results } = drift.capabilityChecks();
  for (const name of drift.CHECKS) {
    assert.ok(results[name], `missing declared check ${name}`);
    assert.strictEqual(results[name].status, 'pass', `${name}: ${results[name].reason}`);
  }
});

// The completeness contract jobCheck enforces: it fails the whole job when a
// declared name is absent from the produced set, so that set must cover CHECKS.
test('capabilityChecks produces exactly the declared CHECKS', () => {
  const { results } = drift.capabilityChecks();
  assert.deepStrictEqual(Object.keys(results).sort(), [...drift.CHECKS].sort());
});

test('the receipt payload names the catalog and its source digest', () => {
  const { catalog } = drift.capabilityChecks();
  assert.ok(catalog.sources.length >= 2, 'expected the committed descriptors');
  assert.match(catalog.digest, /^[0-9a-f]{64}$/, 'digest must be a full sha256');
  for (const s of catalog.sources) {
    assert.ok(s.path.startsWith('engineering/application/capabilities/'), s.path);
    assert.ok(typeof s.id === 'string' && s.id.length, `${s.path} has no id`);
  }
  // The digest must follow the bytes, or it cannot witness anything.
  withFixture((root) => {
    const before = drift.catalog(root).digest;
    const f = capPath(root, 'opacity.json');
    const d = readJson(f);
    d.version = String(d.version) + '-nudged';
    writeJson(f, d);
    assert.notStrictEqual(drift.catalog(root).digest, before, 'digest ignored a descriptor edit');
  });
});

test('a fresh fixture reproduces the working tree verdict (controls start green)', () => {
  withFixture((root) => {
    for (const name of drift.CHECKS) {
      const fn = { 'capability-surface': drift.checkSurface, 'capability-schema-version': drift.checkSchemaVersion,
        'capability-ids': drift.checkIds, 'capability-examples': drift.checkExamples,
        'capability-transport-copies': drift.checkTransportCopies, 'schema-examples': drift.checkSchemaExamples,
        'native-capability-catalog': drift.checkNativeCatalog, 'native-raw-client-allowlist': drift.checkRawClient }[name];
      if (!fn) continue; // capability-catalog is a receipt line, not a gate
      assert.strictEqual(fn(root).status, 'pass', `${name} did not start green: ${fn(root).reason}`);
    }
  });
});

// ---- control 1: missing declared surface ------------------------------
// A new descriptor that nobody added to CAPABILITY_SOURCES. This compiles and
// passes every other test; the Rust catalog simply never sees the capability.
test('NEGATIVE — a descriptor absent from CAPABILITY_SOURCES fails capability-surface', () => {
  withFixture((root) => {
    const ghost = readJson(capPath(root, 'opacity.json'));
    ghost.id = 'ghost.capability';
    writeJson(capPath(root, 'ghost.json'), ghost);

    const r = drift.checkSurface(root);
    assert.strictEqual(r.status, 'fail', 'undeclared descriptor did not fail the gate');
    assert.match(r.reason, /not declared/);
    assert.match(r.reason, /ghost\.json/);
  });
});

test('NEGATIVE — CAPABILITY_SOURCES naming a file that does not exist fails capability-surface', () => {
  withFixture((root) => {
    // The other half of a rename: the declaration outlives the file.
    fs.rmSync(capPath(root, 'opacity.json'));
    const r = drift.checkSurface(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, /declares missing file/);
    assert.match(r.reason, /opacity\.json/);
  });
});

// ---- control 2: stale generated schema --------------------------------
// The schema moves to v2, the descriptors are not migrated.
test('NEGATIVE — a descriptor left on an unaccepted schemaVersion fails capability-schema-version', () => {
  withFixture((root) => {
    const p = path.join(root, APP, 'capability-v1.schema.json');
    const schema = readJson(p);
    schema.properties = schema.properties || {};
    schema.properties.schemaVersion = { const: 2 };
    writeJson(p, schema);

    const r = drift.checkSchemaVersion(root);
    assert.strictEqual(r.status, 'fail', 'stale schemaVersion did not fail the gate');
    assert.match(r.reason, /schemaVersion 1 not accepted/);
  });
});

// ---- control 3: duplicate capability id -------------------------------
// The Rust catalog registers both and resolves lookups to one; the other never
// runs, and nothing says so.
test('NEGATIVE — two descriptors sharing an id fail capability-ids', () => {
  withFixture((root) => {
    const dup = readJson(capPath(root, 'opacity.json'));
    writeJson(capPath(root, 'opacity-again.json'), dup);

    const r = drift.checkIds(root);
    assert.strictEqual(r.status, 'fail', 'duplicate id did not fail the gate');
    assert.match(r.reason, /duplicate capability id "opacity"/);
    assert.match(r.reason, /opacity-again\.json/);
  });
});

// ---- control 4: missing example / fixture -----------------------------
test('NEGATIVE — a descriptor with no examples fails capability-examples', () => {
  withFixture((root) => {
    const f = capPath(root, 'export-job.json');
    const d = readJson(f);
    delete d.examples;
    writeJson(f, d);

    const r = drift.checkExamples(root);
    assert.strictEqual(r.status, 'fail', 'missing examples did not fail the gate');
    assert.match(r.reason, /missing or empty examples/);
    assert.match(r.reason, /export-job\.json/);
  });
});

test('NEGATIVE — a descriptor with no fixture fails capability-examples', () => {
  withFixture((root) => {
    const f = capPath(root, 'opacity.json');
    const d = readJson(f);
    delete d.fixture;
    writeJson(f, d);

    const r = drift.checkExamples(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, /missing fixture/);
  });
});

// ---- control 5: transport copy drift ----------------------------------
// transport-v1 embeds whole copies of the descriptors. Editing the descriptor
// alone leaves the transport schema advertising the old shape.
test('NEGATIVE — an embedded transport copy that drifts from its descriptor fails capability-transport-copies', () => {
  withFixture((root) => {
    const f = capPath(root, 'opacity.json');
    const d = readJson(f);
    d.version = String(d.version) + '.1';
    writeJson(f, d);

    const r = drift.checkTransportCopies(root);
    assert.strictEqual(r.status, 'fail', 'transport copy drift did not fail the gate');
    assert.match(r.reason, /drifted from/);
    assert.match(r.reason, /version/);
  });
});

// ---- the embedded-example gate actually validates ----------------------
// The debt this leaf repairs: these examples were only ever read by humans.
// A gate that reports a count without validating would pass this file too, so
// the control breaks an example and demands a failure.
test('NEGATIVE — an embedded schema example that violates its own schema fails schema-examples', () => {
  withFixture((root) => {
    const p = path.join(root, APP, 'operation-contract-v1.schema.json');
    const schema = readJson(p);
    const def = Object.values(schema.$defs).find((d) => Array.isArray(d.examples) && d.examples.length);
    assert.ok(def, 'expected operation-contract-v1 to carry embedded examples');
    def.examples[0] = { nemoP09Bogus: 'not a valid example' };
    writeJson(p, schema);

    const r = drift.checkSchemaExamples(root);
    assert.strictEqual(r.status, 'fail', 'a bogus embedded example did not fail the gate');
    assert.match(r.reason, /embedded example problem/);
  });
});

// Traversal regression: transport-v1 nests its schemas under non-standard root
// sections, and an earlier keyword-whitelist walk skipped all of them while
// still reporting a confident pass. Count the positions, not just the verdict.
test('schema-examples reaches transport-v1, whose schemas sit under non-standard root sections', () => {
  const r = drift.checkSchemaExamples();
  assert.strictEqual(r.status, 'pass');
  const m = /^(\d+) embedded example/.exec(r.reason);
  assert.ok(m, `unexpected reason: ${r.reason}`);
  // 11 in operation-contract-v1 ($defs) + 7 in transport-v1 (request.properties
  // .payload) — the latter is the half a $defs-only walk missed entirely.
  assert.ok(Number(m[1]) >= 18, `expected at least 18 validated examples, got ${m[1]}`);
});

// ---- CS1/#2341: native v2 catalog discovered by nemo-mcp/build.rs --------
function nativePath(root, name) { return path.join(root, APP, 'capabilities-v2', name); }

test('native-capability-catalog fails on an operation declared by two descriptors', () => {
  withFixture((root) => {
    const [a, b] = drift.nativeDescriptorFiles(root).map((f) => path.basename(f));
    const first = readJson(nativePath(root, a));
    const second = readJson(nativePath(root, b));
    second.operations = second.operations.concat(first.operations[0]);
    writeJson(nativePath(root, b), second);
    const r = drift.checkNativeCatalog(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, /duplicate native operation/);
  });
});

test('native-capability-catalog fails on a descriptor that is not v2', () => {
  withFixture((root) => {
    const name = path.basename(drift.nativeDescriptorFiles(root)[0]);
    const d = readJson(nativePath(root, name));
    d.schemaVersion = 1;
    writeJson(nativePath(root, name), d);
    const r = drift.checkNativeCatalog(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, /schemaVersion\/apiVersion 2/);
  });
});

test('native-capability-catalog fails when a declaration has no v2 transport branch', () => {
  withFixture((root) => {
    const name = path.basename(drift.nativeDescriptorFiles(root)[0]);
    const d = readJson(nativePath(root, name));
    d.operations = d.operations.concat('command.document.object.cs1.unschemed');
    writeJson(nativePath(root, name), d);
    const r = drift.checkNativeCatalog(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, /missing from the v2 transport union: command\.document\.object\.cs1\.unschemed/);
  });
});

test('native-capability-catalog fails if capabilities.rs regrows a hand-maintained native list', () => {
  withFixture((root) => {
    const rust = path.join(root, RUST);
    const src = fs.readFileSync(rust, 'utf8')
      .replace(/include!\(concat!\(env!\("OUT_DIR"\), "\/native_capability_sources\.rs"\)\);/, 'pub const NATIVE_CAPABILITY_SOURCES: &[&str] = &[];');
    fs.writeFileSync(rust, src);
    const r = drift.checkNativeCatalog(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, /hand-maintained NATIVE_CAPABILITY_SOURCES/);
    assert.match(r.reason, /does not include the build\.rs-generated native catalog/);
  });
});

test('native-capability-catalog fails on a capability id declared by two descriptor files', () => {
  withFixture((root) => {
    const name = path.basename(drift.nativeDescriptorFiles(root)[0]);
    const d = readJson(nativePath(root, name));
    d.operations = ['command.document.object.cs1.duplicate-id'];
    writeJson(nativePath(root, 'zz-cs1-duplicate-id.json'), d);
    const r = drift.checkNativeCatalog(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, new RegExp('duplicate native capability id ' + d.id.replace(/\./g, '\\.')));
  });
});

test('native-capability-catalog fails on a v2 transport operation that no descriptor declares', () => {
  withFixture((root) => {
    const p = path.join(root, APP, 'native-transport-v2.schema.json');
    const schema = readJson(p);
    const branch = JSON.parse(JSON.stringify(schema.$defs.Request.oneOf[0]));
    branch.allOf[1].properties.operation = { const: 'query.document.cs1.stale' };
    schema.$defs.Request.oneOf.push(branch);
    writeJson(p, schema);
    const r = drift.checkNativeCatalog(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, /v2 transport operation\(s\) with no declaration: query\.document\.cs1\.stale/);
  });
});

test('native-capability-catalog fails on an unparsable descriptor', () => {
  withFixture((root) => {
    fs.writeFileSync(nativePath(root, 'zz-cs1-broken.json'), '{ not json');
    const r = drift.checkNativeCatalog(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, /zz-cs1-broken\.json: invalid JSON/);
  });
});

test('native-capability-catalog fails when the declaration directory is empty', () => {
  withFixture((root) => {
    for (const f of drift.nativeDescriptorFiles(root)) fs.rmSync(f);
    const r = drift.checkNativeCatalog(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, /no native declarations/);
  });
});

// ---- CS1b-a/#2363: Tauri raw-client allowlist derived from `rawClient` flags ----
function setClient(root, fn) {
  const p = path.join(root, CLIENT);
  fs.writeFileSync(p, fn(fs.readFileSync(p, 'utf8')));
}
function editDescriptor(root, id, fn) {
  const f = drift.nativeDescriptorFiles(root).find((x) => readJson(x).id === id);
  const d = readJson(f);
  fn(d);
  writeJson(f, d);
}

test('native-raw-client-allowlist passes and names the 11 flagged operations', () => {
  const r = drift.checkRawClient();
  assert.strictEqual(r.status, 'pass', r.reason);
  assert.match(r.reason, /^11 rawClient operation\(s\)/);
});

test('native-raw-client-allowlist MUTATION — unflagging any operation fails (OPERATIONS lists it)', () => {
  const all = drift.nativeDescriptorFiles(REPO).flatMap((f) => Object.keys(readJson(f).rawClient || {}));
  assert.strictEqual(all.length, 11);
  for (const op of all) {
    withFixture((root) => {
      for (const f of drift.nativeDescriptorFiles(root)) {
        const d = readJson(f);
        if (d.rawClient && op in d.rawClient) { delete d.rawClient[op]; writeJson(f, d); }
      }
      const r = drift.checkRawClient(root);
      assert.strictEqual(r.status, 'fail', op);
      assert.ok(r.reason.includes(`no rawClient flag in any descriptor: ${op}`), r.reason);
    });
  }
});

test('native-raw-client-allowlist fails on a flagged operation OPERATIONS does not list', () => {
  withFixture((root) => {
    editDescriptor(root, 'native.object.fill', (d) => { d.rawClient['command.document.object.fill.trace'] = { revisionDelta: 1 }; });
    const r = drift.checkRawClient(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, /missing from OPERATIONS: command\.document\.object\.fill\.trace/);
  });
});

test('native-raw-client-allowlist fails when OPERATIONS grows or its length goes stale', () => {
  withFixture((root) => {
    setClient(root, (src) => src.replace('"command.document.object.layer.cut",\n];', '"command.document.object.layer.cut",\n    "command.document.object.sneaky",\n];'));
    const r = drift.checkRawClient(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, /declares length 11 but lists 12/);
    assert.match(r.reason, /no rawClient flag in any descriptor: command\.document\.object\.sneaky/);
  });
});

test('native-raw-client-allowlist rejects invalid flag and revisionDelta values', () => {
  const cases = [
    ['missing delta', (d) => { d.rawClient['command.document.object.layer.cut'] = {}; }, /revisionDelta/],
    ['non-integer delta', (d) => { d.rawClient['command.document.object.layer.cut'].revisionDelta = 1.5; }, /expected type integer/],
    ['delta above the single-transaction bound', (d) => { d.rawClient['command.document.object.layer.cut'].revisionDelta = 2; }, /maximum/],
    ['command with delta 0', (d) => { d.rawClient['command.document.object.layer.cut'].revisionDelta = 0; }, /command .* must declare revisionDelta 1/],
    ['boolean instead of entry', (d) => { d.rawClient['command.document.object.layer.cut'] = true; }, /expected type object/],
    ['unknown entry key', (d) => { d.rawClient['command.document.object.layer.cut'].extra = 1; }, /unexpected property/],
    ['flag on an undeclared operation', (d) => { d.rawClient['command.document.object.ghost'] = { revisionDelta: 1 }; }, /neither operations nor hostOperations/],
  ];
  for (const [label, mutate, expected] of cases) {
    withFixture((root) => {
      editDescriptor(root, 'native.layer-playhead-cut', mutate);
      const r = drift.checkRawClient(root);
      assert.strictEqual(r.status, 'fail', label);
      assert.match(r.reason, expected, label);
    });
  }
  withFixture((root) => {
    editDescriptor(root, 'native.object', (d) => { d.rawClient['query.document.object'].revisionDelta = 1; });
    const r = drift.checkRawClient(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, /read query\.document\.object must declare revisionDelta 0/);
  });
});

test('native-raw-client-allowlist keeps host operations out of the public transport', () => {
  withFixture((root) => {
    editDescriptor(root, 'native.object', (d) => { d.operations.push('query.document.object.serialize'); });
    const r = drift.checkRawClient(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, /host operation query\.document\.object\.serialize is also declared as a transport operation/);
  });
  withFixture((root) => {
    editDescriptor(root, 'native.layer-playhead-cut', (d) => { d.rawClient['command.document.object.fill.set'] = { revisionDelta: 1 }; });
    const r = drift.checkRawClient(root);
    assert.strictEqual(r.status, 'fail');
    assert.match(r.reason, /duplicate rawClient operation command\.document\.object\.fill\.set/);
  });
});

test('native-raw-client-allowlist fails when the schema or the Rust allowlist is missing', () => {
  withFixture((root) => {
    fs.rmSync(path.join(root, APP, 'capability-v2.schema.json'));
    assert.match(drift.checkRawClient(root).reason, /missing .*capability-v2\.schema\.json/);
  });
  withFixture((root) => {
    setClient(root, (src) => src.replace('const OPERATIONS', 'const NOT_OPERATIONS'));
    assert.match(drift.checkRawClient(root).reason, /cannot find the OPERATIONS const/);
  });
});
