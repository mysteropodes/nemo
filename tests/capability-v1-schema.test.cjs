'use strict';
const { test } = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');

const SCHEMA_PATH = path.join(__dirname, '..', 'engineering', 'application', 'capability-v1.schema.json');
const OPACITY_PATH = path.join(__dirname, '..', 'engineering', 'application', 'capabilities', 'opacity.json');
const EXPORT_JOB_PATH = path.join(__dirname, '..', 'engineering', 'application', 'capabilities', 'export-job.json');

function load(p) { return JSON.parse(fs.readFileSync(p, 'utf8')); }

// Minimal, dependency-free validator for exactly the JSON Schema subset
// capability-v1.schema.json uses (type/const/enum/pattern/minimum/maximum/
// minLength/minItems/required/properties/additionalProperties/items/$ref/anyOf).
// Not a general-purpose validator; do not reuse for other schemas without review.
function validate(schema, value, defs, at) {
  at = at || '$';
  // Boolean schemas are valid JSON Schema: `true` accepts anything, `false` accepts nothing.
  // capability-v1.schema.json's Example.input/output use `true` for "any JSON value".
  if (schema === true) return [];
  if (schema === false) return [`${at}: rejected by schema \`false\``];
  defs = defs || schema.$defs || {};
  if (schema.$ref) return validate(defs[schema.$ref.replace('#/$defs/', '')], value, defs, at);
  if (schema.anyOf) {
    const branches = schema.anyOf.map((s) => validate(s, value, defs, at));
    return branches.some((e) => e.length === 0) ? [] : [`${at}: matches none of anyOf (${JSON.stringify(branches)})`];
  }
  const errors = [];
  if ('const' in schema) {
    if (value !== schema.const) errors.push(`${at}: expected const ${JSON.stringify(schema.const)}, got ${JSON.stringify(value)}`);
    return errors;
  }
  if (schema.enum && !schema.enum.includes(value)) {
    errors.push(`${at}: ${JSON.stringify(value)} is not one of ${JSON.stringify(schema.enum)}`);
    return errors;
  }
  if (schema.type) {
    const types = Array.isArray(schema.type) ? schema.type : [schema.type];
    const actual = value === null ? 'null' : Array.isArray(value) ? 'array'
      : (typeof value === 'number' && Number.isInteger(value)) ? 'integer' : typeof value;
    if (!types.includes(actual) && !(actual === 'integer' && types.includes('number'))) {
      errors.push(`${at}: expected type ${types.join('|')}, got ${actual}`);
      return errors;
    }
  }
  if (typeof value === 'string') {
    if (schema.pattern && !new RegExp(schema.pattern).test(value)) errors.push(`${at}: ${JSON.stringify(value)} does not match pattern ${schema.pattern}`);
    if (schema.minLength != null && value.length < schema.minLength) errors.push(`${at}: shorter than minLength ${schema.minLength}`);
  }
  if (typeof value === 'number') {
    if (schema.minimum != null && value < schema.minimum) errors.push(`${at}: ${value} < minimum ${schema.minimum}`);
    if (schema.maximum != null && value > schema.maximum) errors.push(`${at}: ${value} > maximum ${schema.maximum}`);
  }
  if (Array.isArray(value)) {
    if (schema.minItems != null && value.length < schema.minItems) errors.push(`${at}: fewer than minItems ${schema.minItems}`);
    if (schema.items) value.forEach((item, i) => errors.push(...validate(schema.items, item, defs, `${at}[${i}]`)));
  } else if (value && typeof value === 'object') {
    (schema.required || []).forEach((key) => { if (!(key in value)) errors.push(`${at}: missing required property "${key}"`); });
    Object.keys(value).forEach((key) => {
      const propSchema = schema.properties && schema.properties[key];
      if (propSchema) errors.push(...validate(propSchema, value[key], defs, `${at}.${key}`));
      else if (schema.additionalProperties === false) errors.push(`${at}: unexpected property "${key}"`);
      else if (schema.additionalProperties && typeof schema.additionalProperties === 'object') {
        errors.push(...validate(schema.additionalProperties, value[key], defs, `${at}.${key}`));
      }
    });
  }
  return errors;
}

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
