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

// A faithful copy of exactly the inputs the checker reads.
function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-p09-'));
  fs.mkdirSync(path.join(root, APP), { recursive: true });
  fs.cpSync(path.join(REPO, APP), path.join(root, APP), { recursive: true });
  fs.mkdirSync(path.join(root, path.dirname(RUST)), { recursive: true });
  fs.cpSync(path.join(REPO, RUST), path.join(root, RUST));
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
        'capability-transport-copies': drift.checkTransportCopies, 'schema-examples': drift.checkSchemaExamples }[name];
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
