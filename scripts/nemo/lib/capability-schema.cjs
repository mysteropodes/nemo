'use strict';
// P09 — the schema-reading half of the capability drift checker: a minimal JSON
// Schema validator, the traversal that finds embedded examples, and a structural
// diff. Split out of capability-drift.cjs to stay inside the 400-line hard max
// for "Domain/application JS or TS"; same module (nemo.lib.capabilityDrift),
// same pattern as nemo.lib.baseline's baseline.cjs + baseline-verdicts.cjs.
//
// `validate` used to be the private copy inside tests/capability-v1-schema.test.cjs.
// It was MOVED here rather than copied: a second implementation in the shipped
// path is the twin-function divergence CLAUDE.md §3 warns about, where the test
// keeps passing while the gate an author actually runs drifts.

// Minimal, dependency-free validator for exactly the JSON Schema subset these
// schemas use (type/const/enum/pattern/minimum/maximum/minLength/minItems/
// required/properties/additionalProperties/items/$ref/anyOf). Not a
// general-purpose validator; do not reuse for other schemas without review.
function validate(schema, value, defs, at) {
  at = at || '$';
  // Boolean schemas are valid JSON Schema: `true` accepts anything, `false` accepts nothing.
  // capability-v1.schema.json's Example.input/output use `true` for "any JSON value".
  if (schema === true) return [];
  if (schema === false) return [`${at}: rejected by schema \`false\``];
  defs = defs || schema.$defs || {};
  if (schema.$ref) {
    const target = defs[schema.$ref.replace('#/$defs/', '')];
    // An unresolved $ref used to validate as "no constraints", so a broken
    // reference read as a pass. It is a finding, not a silence.
    if (target === undefined) return [`${at}: unresolved $ref ${schema.$ref}`];
    return validate(target, value, defs, at);
  }
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

// Data-bearing keywords. Their contents are VALUES, so descending into them
// would find `examples` keys belonging to example payloads rather than to
// schemas — a payload that happens to contain the word would be "validated"
// against itself.
const DATA_KEYS = new Set(['examples', 'example', 'const', 'default', 'enum']);
// Keywords whose value is a map of NAME → schema. At such a node an `examples`
// key is a property called "examples", not an annotation. capability-v1 has
// exactly that, and conflating the two reports a phantom example that does not
// exist — the naive walk counted one there.
const MAP_KEYS = new Set(['properties', '$defs', 'definitions', 'patternProperties', 'dependentSchemas']);
const CONSTRAINT_KEYS = ['type', 'properties', 'items', '$ref', 'anyOf', 'allOf', 'oneOf', 'enum', 'const',
  'required', 'pattern', 'additionalProperties', 'minimum', 'maximum', 'minLength', 'minItems', 'prefixItems'];

function isSchemaLike(node) { return CONSTRAINT_KEYS.some((k) => k in node); }

// Traversal is GENERIC, not a whitelist of JSON Schema keywords. transport-v1
// nests two complete schemas under non-standard root sections (`request`,
// `response`) and carries descriptor copies under an `x-nemo-` extension array;
// a keyword whitelist walked straight past all 14 of its embedded examples and
// still reported a confident pass. Anything object-shaped is a candidate
// position; only DATA_KEYS are refused.
//
// Entries with `schema: null` sit where no constraints exist, so they cannot be
// validated there. The caller reports them rather than counting them as checked.
function eachSchemaExample(node, at, out, inMap) {
  if (Array.isArray(node)) {
    node.forEach((sub, i) => eachSchemaExample(sub, `${at}[${i}]`, out, false));
    return out;
  }
  if (!node || typeof node !== 'object') return out;
  if (!inMap) {
    if (Array.isArray(node.examples)) {
      node.examples.forEach((ex, i) => out.push({ schema: isSchemaLike(node) ? node : null, value: ex, at: `${at}.examples[${i}]` }));
    }
    if ('example' in node && node.example !== undefined) {
      out.push({ schema: isSchemaLike(node) ? node : null, value: node.example, at: `${at}.example` });
    }
  }
  for (const [k, v] of Object.entries(node)) {
    // Inside a name→schema map every key is a property name, including one
    // literally called "examples", so the data-keyword refusal must not apply.
    if (!inMap && DATA_KEYS.has(k)) continue;
    if (v && typeof v === 'object') eachSchemaExample(v, `${at}.${k}`, out, MAP_KEYS.has(k));
  }
  return out;
}

// Every `$defs` map in the file, wherever it sits. transport-v1 keeps its
// definitions on the `request` section rather than the root, so resolving
// `$ref: "#/$defs/..."` from the root map alone would fail on every reference.
function collectDefs(node, acc) {
  acc = acc || {};
  if (!node || typeof node !== 'object') return acc;
  if (Array.isArray(node)) { node.forEach((n) => collectDefs(n, acc)); return acc; }
  for (const [k, v] of Object.entries(node)) {
    if (DATA_KEYS.has(k)) continue;
    if (k === '$defs' && v && typeof v === 'object') Object.assign(acc, v);
    if (v && typeof v === 'object') collectDefs(v, acc);
  }
  return acc;
}

// Structural difference, reported as paths rather than a boolean, so a drifted
// transport copy names the field that moved instead of just "not equal".
function deepDiff(a, b, at, out) {
  at = at || '';
  out = out || [];
  if (a === b) return out;
  const ta = a === null ? 'null' : Array.isArray(a) ? 'array' : typeof a;
  const tb = b === null ? 'null' : Array.isArray(b) ? 'array' : typeof b;
  if (ta !== tb) { out.push(`${at || '$'} (${ta} vs ${tb})`); return out; }
  if (ta === 'array') {
    if (a.length !== b.length) out.push(`${at}.length (${a.length} vs ${b.length})`);
    for (let i = 0; i < Math.min(a.length, b.length); i++) deepDiff(a[i], b[i], `${at}[${i}]`, out);
    return out;
  }
  if (ta === 'object') {
    for (const k of new Set([...Object.keys(a), ...Object.keys(b)])) {
      if (!(k in a)) { out.push(`${at}.${k} (absent in descriptor)`); continue; }
      if (!(k in b)) { out.push(`${at}.${k} (absent in transport copy)`); continue; }
      deepDiff(a[k], b[k], `${at}.${k}`, out);
    }
    return out;
  }
  out.push(`${at || '$'} (${JSON.stringify(a)} vs ${JSON.stringify(b)})`);
  return out;
}

module.exports = { validate, eachSchemaExample, collectDefs, deepDiff, isSchemaLike, DATA_KEYS, MAP_KEYS };
