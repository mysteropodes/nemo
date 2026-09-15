'use strict';
const { test } = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');

const SCHEMA_PATH = path.join(__dirname, '..', 'engineering', 'application', 'capability-v1.schema.json');
const OPACITY_PATH = path.join(__dirname, '..', 'engineering', 'application', 'capabilities', 'opacity.json');
const EXPORT_JOB_PATH = path.join(__dirname, '..', 'engineering', 'application', 'capabilities', 'export-job.json');

function load(p) { return JSON.parse(fs.readFileSync(p, 'utf8')); }

// The minimal JSON Schema subset validator used to live here, as the only copy.
// P09 moved it into the SHIPPED checker (scripts/nemo/lib/capability-drift.cjs),
// which runs it inside `npm run check` over these same schemas. Keeping a second
// copy here would be the twin-function divergence CLAUDE.md §3 warns about: the
// test could keep passing while the gate an author actually runs drifted.
const { validate } = require('../scripts/nemo/lib/capability-drift.cjs');

test('capability-v1.schema.json is well-formed (root object, expected $defs present)', () => {
  const schema = load(SCHEMA_PATH);
  assert.strictEqual(schema.$schema, 'https://json-schema.org/draft/2020-12/schema');
  assert.strictEqual(schema.type, 'object');
  assert.ok(schema.$defs.Effects && schema.$defs.Availability && schema.$defs.Example);
});

test('opacity descriptor fixture validates against CapabilityDescriptor', () => {
  assert.deepStrictEqual(validate(load(SCHEMA_PATH), load(OPACITY_PATH)), []);
});

test('export-job descriptor fixture validates against CapabilityDescriptor', () => {
  assert.deepStrictEqual(validate(load(SCHEMA_PATH), load(EXPORT_JOB_PATH)), []);
});

test('malformed id is rejected', () => {
  const bad = load(OPACITY_PATH);
  bad.id = 'Not Valid ID!';
  const errors = validate(load(SCHEMA_PATH), bad);
  assert.ok(errors.some((e) => e.includes('.id') && e.includes('pattern')), errors.join('\n'));
});

test('unsupported schemaVersion is rejected', () => {
  const bad = load(OPACITY_PATH);
  bad.schemaVersion = 2;
  const errors = validate(load(SCHEMA_PATH), bad);
  assert.ok(errors.some((e) => e.includes('schemaVersion')), errors.join('\n'));
});

test('unknown top-level property is rejected (additionalProperties: false)', () => {
  const bad = load(OPACITY_PATH);
  bad.unexpectedField = true;
  const errors = validate(load(SCHEMA_PATH), bad);
  assert.ok(errors.some((e) => e.includes('unexpectedField')), errors.join('\n'));
});

// Cross-item uniqueness cannot be expressed inside one descriptor's own schema
// (JSON Schema validates one instance at a time); it is a registration-time
// rule instead, exercised here as capability-v1.schema.json's own description
// specifies. A future registry (P05/#1007) should keep this exact rule.
function assertNoDuplicateIds(descriptors) {
  const seen = new Set();
  for (const d of descriptors) {
    if (seen.has(d.id)) throw new Error(`duplicate capability id "${d.id}" in one registration set`);
    seen.add(d.id);
  }
}

test('registration rejects two descriptors sharing one id', () => {
  assert.throws(() => assertNoDuplicateIds([load(OPACITY_PATH), load(OPACITY_PATH)]), /duplicate capability id "opacity"/);
});

test('registration accepts distinct ids', () => {
  assert.doesNotThrow(() => assertNoDuplicateIds([load(OPACITY_PATH), load(EXPORT_JOB_PATH)]));
});

test('every fixture/example conforms to its own descriptor input/output', () => {
  for (const p of [OPACITY_PATH, EXPORT_JOB_PATH]) {
    const d = load(p);
    for (const ex of [d.fixture, ...d.examples]) {
      assert.deepStrictEqual(validate(d.input, ex.input), [], `${d.id} / ${ex.label} input`);
      assert.deepStrictEqual(validate(d.output, ex.output), [], `${d.id} / ${ex.label} output`);
    }
  }
});
