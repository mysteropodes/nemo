'use strict';
// P09 — capability/schema drift checks, shipped inside normal local validation.
//
// These run as sub-checks of the `check` job (`npm run check`, and `check` is in
// both verify profiles). They are deliberately NOT a test-only validator: the
// drift they look for is introduced by editing source, so the signal has to come
// from the same gate an author already runs, not from a suite someone may not.
//
// Two hand-maintained second copies motivated this leaf, and neither had a gate:
//   * `CAPABILITY_SOURCES` in nemo-mcp/src/capabilities.rs — an `include_str!`
//     list. Adding engineering/application/capabilities/<new>.json without
//     adding it there compiles, passes every existing test, and silently omits
//     the capability from the Rust catalog.
//   * `x-nemo-registeredCapabilities` in transport-v1.schema.json — whole copies
//     of the descriptors. Editing a descriptor leaves the transport schema
//     advertising the old shape, again with nothing failing.
// Nothing broke in either case; that is precisely why they need a checker.
//
// The validator, the example traversal and the structural diff live in
// ./capability-schema.cjs — same module (nemo.lib.capabilityDrift), split only
// to stay inside the 400-line hard max for this size profile. `validate` there
// was MOVED out of tests/capability-v1-schema.test.cjs, which used to own the
// only copy; a second copy in the shipped path would be the twin-function
// divergence CLAUDE.md §3 warns about. That test now requires it from the lib
// (through this module's re-export), so there is still exactly one.
//
// Every check takes an optional `root` so the negative controls can be driven
// against a fixture tree. Mutating the real working tree to prove a gate fails
// is how a crashed test leaves a dirty checkout behind.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { ROOT, exists } = require('./util.cjs');
const { validate, eachSchemaExample, collectDefs, deepDiff } = require('./capability-schema.cjs');
const { STATUS } = require('./receipt.cjs');

// Every schema whose embedded examples must hold. value-contract-v1 carries
// none today; it is listed so that adding one starts being checked immediately
// rather than silently joining the untested set this leaf exists to close.
const SCHEMA_FILES = [
  'capability-v1.schema.json',
  'transport-v1.schema.json',
  'operation-contract-v1.schema.json',
  'value-contract-v1.schema.json',
];

// The sub-checks this module promises to produce. jobCheck compares the names it
// actually got against this list and fails on any absence, so a checker that
// stops registering itself cannot quietly shrink the gate into a smaller green
// one — the omission is the failure.
const CHECKS = [
  'capability-catalog',
  'capability-surface',
  'capability-schema-version',
  'capability-ids',
  'capability-examples',
  'capability-transport-copies',
  'schema-examples',
];

function pass(reason, extra) { return Object.assign({ status: STATUS.PASS, reason }, extra); }
function fail(reason, extra) { return Object.assign({ status: STATUS.FAIL, reason }, extra); }
function blocked(reason, extra) { return Object.assign({ status: STATUS.BLOCKED, reason }, extra); }

function paths(root) {
  const app = path.join(root, 'engineering', 'application');
  return {
    app,
    caps: path.join(app, 'capabilities'),
    rust: path.join(root, 'nemo-mcp', 'src', 'capabilities.rs'),
    descriptorSchema: path.join(app, 'capability-v1.schema.json'),
    transport: path.join(app, 'transport-v1.schema.json'),
  };
}

function readJsonFile(p) { return JSON.parse(fs.readFileSync(p, 'utf8')); }
function rel(p, root) { return path.relative(root || ROOT, p).split(path.sep).join('/'); }

// ---- catalog -----------------------------------------------------------
function descriptorFiles(root) {
  const p = paths(root || ROOT);
  if (!exists(p.caps)) return [];
  return fs.readdirSync(p.caps).filter((f) => f.endsWith('.json')).sort()
    .map((f) => path.join(p.caps, f));
}

// The `include_str!` paths inside the CAPABILITY_SOURCES const only. Scoping to
// that block matters: capabilities.rs may legitimately include_str! something
// else later, and a file-wide regex would then read it as a declared capability.
function declaredSources(root) {
  root = root || ROOT;
  const p = paths(root);
  if (!exists(p.rust)) return null;
  const src = fs.readFileSync(p.rust, 'utf8');
  const start = src.indexOf('CAPABILITY_SOURCES');
  if (start < 0) return null;
  const end = src.indexOf('];', start);
  if (end < 0) return null;
  const block = src.slice(start, end);
  const out = [];
  const re = /include_str!\s*\(\s*"([^"]+)"\s*\)/g;
  let m;
  while ((m = re.exec(block))) out.push(rel(path.resolve(path.dirname(p.rust), m[1]), root));
  return out.sort();
}

// The catalog the Rust MCP builds, plus a digest over the exact descriptor bytes
// it is built from. The receipt carries both so a run can be tied to the source
// it actually saw, rather than to a file list that may since have moved.
function catalog(root) {
  root = root || ROOT;
  const files = descriptorFiles(root);
  const h = crypto.createHash('sha256');
  const entries = [];
  for (const f of files) {
    const bytes = fs.readFileSync(f);
    h.update(rel(f, root)); h.update('\0'); h.update(bytes); h.update('\0');
    let id = null;
    try { id = JSON.parse(bytes.toString('utf8')).id ?? null; } catch { /* reported by capability-examples */ }
    entries.push({ path: rel(f, root), id, bytes: bytes.length });
  }
  return { sources: entries, digest: h.digest('hex') };
}

// ---- the checks --------------------------------------------------------
// 1. Missing declared surface: a descriptor the Rust catalog never sees, or a
//    declared path that no longer exists. Both directions, because a rename
//    produces one of each and only reporting one half hides the other.
function checkSurface(root) {
  root = root || ROOT;
  const p = paths(root);
  const declared = declaredSources(root);
  if (declared === null) return blocked(`cannot read CAPABILITY_SOURCES from ${rel(p.rust, root)}`);
  const present = descriptorFiles(root).map((f) => rel(f, root));
  const undeclared = present.filter((f) => !declared.includes(f));
  const dangling = declared.filter((f) => !exists(path.join(root, f)));
  const problems = [];
  if (undeclared.length) problems.push(`descriptor(s) not declared in ${rel(p.rust, root)} CAPABILITY_SOURCES: ${undeclared.join(', ')}`);
  if (dangling.length) problems.push(`CAPABILITY_SOURCES declares missing file(s): ${dangling.join(', ')}`);
  return problems.length ? fail(problems.join('; '))
    : pass(`${present.length} descriptor(s) declared in ${rel(p.rust, root)}`);
}

// 2. Stale generated schema: a descriptor pinned to a schemaVersion its schema
//    no longer accepts. The accepted set is read from the schema itself
//    (const/enum), falling back to the version in its $id, so bumping the schema
//    without migrating a descriptor fails here instead of at some consumer.
function acceptedSchemaVersions(schema) {
  const p = schema && schema.properties && schema.properties.schemaVersion;
  if (p && 'const' in p) return [p.const];
  if (p && Array.isArray(p.enum)) return p.enum.slice();
  const m = /-v(\d+)\.schema\.json/.exec(String(schema && schema.$id || ''));
  return m ? [Number(m[1])] : null;
}

function checkSchemaVersion(root) {
  root = root || ROOT;
  const p = paths(root);
  if (!exists(p.descriptorSchema)) return blocked(`missing ${rel(p.descriptorSchema, root)}`);
  let schema;
  try { schema = readJsonFile(p.descriptorSchema); } catch (e) { return fail(`${rel(p.descriptorSchema, root)}: ${e.message}`); }
  const accepted = acceptedSchemaVersions(schema);
  if (!accepted) return blocked(`cannot determine accepted schemaVersion from ${rel(p.descriptorSchema, root)}`);
  const bad = [];
  for (const f of descriptorFiles(root)) {
    let d;
    try { d = readJsonFile(f); } catch (e) { bad.push(`${rel(f, root)}: ${e.message}`); continue; }
    if (!accepted.includes(d.schemaVersion)) {
      bad.push(`${rel(f, root)}: schemaVersion ${JSON.stringify(d.schemaVersion)} not accepted by ${rel(p.descriptorSchema, root)} (accepts ${JSON.stringify(accepted)})`);
    }
  }
  return bad.length ? fail(bad.join('; '))
    : pass(`every descriptor targets an accepted schemaVersion (${JSON.stringify(accepted)})`);
}

// 3. Duplicate capability id. Two descriptors sharing an id is not a conflict
//    the Rust catalog reports: it registers both and lookups resolve to one of
//    them, so the other silently never runs.
function checkIds(root) {
  root = root || ROOT;
  const byId = new Map();
  const broken = [];
  for (const f of descriptorFiles(root)) {
    let d;
    try { d = readJsonFile(f); } catch (e) { broken.push(`${rel(f, root)}: ${e.message}`); continue; }
    const id = d.id;
    if (typeof id !== 'string' || !id) { broken.push(`${rel(f, root)}: missing or non-string id`); continue; }
    if (!byId.has(id)) byId.set(id, []);
    byId.get(id).push(rel(f, root));
  }
  const problems = broken.slice();
  for (const [id, files] of [...byId.entries()].filter(([, f]) => f.length > 1)) {
    problems.push(`duplicate capability id ${JSON.stringify(id)}: ${files.join(', ')}`);
  }
  return problems.length ? fail(problems.join('; ')) : pass(`${byId.size} distinct capability id(s)`);
}

// 4. Missing example/fixture. A descriptor without them declares a surface no
//    consumer can exercise, which is how a capability reaches the MCP with
//    nothing proving its shape.
function checkExamples(root) {
  root = root || ROOT;
  const files = descriptorFiles(root);
  const problems = [];
  let fixtures = 0;
  let examples = 0;
  for (const f of files) {
    let d;
    try { d = readJsonFile(f); } catch (e) { problems.push(`${rel(f, root)}: ${e.message}`); continue; }
    if (!d.fixture || typeof d.fixture !== 'object') problems.push(`${rel(f, root)}: missing fixture`);
    else fixtures++;
    if (!Array.isArray(d.examples) || d.examples.length === 0) problems.push(`${rel(f, root)}: missing or empty examples[]`);
    else examples += d.examples.length;
  }
  return problems.length ? fail(problems.join('; '))
    : pass(`${fixtures} fixture(s) and ${examples} example(s) across ${files.length} descriptor(s)`);
}

// 5. transport-v1 embeds whole COPIES of the capability descriptors under the
//    `x-nemo-registeredCapabilities` extension. Nothing regenerates them and
//    nothing compared them until now.
function embeddedCopies(node, at, out) {
  out = out || [];
  if (Array.isArray(node)) { node.forEach((n, i) => embeddedCopies(n, `${at}[${i}]`, out)); return out; }
  if (!node || typeof node !== 'object') return out;
  for (const [k, v] of Object.entries(node)) {
    if (k === 'x-nemo-registeredCapabilities' && Array.isArray(v)) {
      v.forEach((copy, i) => out.push({ value: copy, at: `${at}.${k}[${i}]` }));
      continue;
    }
    if (v && typeof v === 'object') embeddedCopies(v, `${at}.${k}`, out);
  }
  return out;
}

function checkTransportCopies(root) {
  root = root || ROOT;
  const p = paths(root);
  if (!exists(p.transport)) return blocked(`missing ${rel(p.transport, root)}`);
  let schema;
  try { schema = readJsonFile(p.transport); } catch (e) { return fail(`${rel(p.transport, root)}: ${e.message}`); }
  const copies = embeddedCopies(schema, '$', []);
  if (!copies.length) return pass(`${rel(p.transport, root)} embeds no capability copies`);
  const real = new Map();
  for (const f of descriptorFiles(root)) {
    try { const d = readJsonFile(f); if (d && d.id) real.set(d.id, { file: rel(f, root), doc: d }); } catch { /* checkExamples reports */ }
  }
  const problems = [];
  for (const { value, at } of copies) {
    const id = value && value.id;
    const match = real.get(id);
    if (!match) { problems.push(`${at}: embedded capability ${JSON.stringify(id)} has no descriptor file`); continue; }
    const diff = deepDiff(match.doc, value, '', []);
    if (diff.length) {
      const head = diff.slice(0, 5);
      problems.push(`${at} drifted from ${match.file} at ${head.join(', ')}${diff.length > head.length ? ` … and ${diff.length - head.length} more` : ''}`);
    }
  }
  return problems.length ? fail(problems.join('; '))
    : pass(`${copies.length} embedded descriptor copy(ies) in ${rel(p.transport, root)} match their descriptor file byte-for-byte`);
}

// 6. Embedded schema examples. This is the debt the leaf repairs: the examples
//    written inside the schema files were only ever read by humans.
function checkSchemaExamples(root) {
  root = root || ROOT;
  const p = paths(root);
  const problems = [];
  const unconstrained = [];
  let checked = 0;
  let files = 0;
  for (const name of SCHEMA_FILES) {
    const file = path.join(p.app, name);
    if (!exists(file)) { problems.push(`${rel(file, root)}: missing`); continue; }
    let schema;
    try { schema = readJsonFile(file); } catch (e) { problems.push(`${rel(file, root)}: ${e.message}`); continue; }
    files++;
    // $defs resolution is per FILE: transport-v1's sit on its `request`
    // section, not the root, so a root-only defs map breaks every $ref.
    const defs = collectDefs(schema, Object.assign({}, schema.$defs || {}));
    for (const { schema: sub, value, at } of eachSchemaExample(schema, '$', [], false)) {
      // An example sitting at a position that carries no constraints cannot be
      // validated there. Counting it as checked would be a lie, so it is
      // counted separately and named. The descriptor copies in transport-v1
      // land here by design — capability-transport-copies holds those.
      if (!sub) { unconstrained.push(`${name}${at}`); continue; }
      // The annotation itself is not a constraint; validating the example
      // against a node that still carries it would be harmless but confusing in
      // an error path, so it is stripped for the comparison only.
      const { examples, example, ...constraints } = sub;
      const errors = validate(constraints, value, defs, `${name}${at}`);
      checked++;
      if (errors.length) problems.push(...errors);
    }
  }
  if (problems.length) {
    const head = problems.slice(0, 8);
    return fail(`${problems.length} embedded example problem(s): ${head.join('; ')}${problems.length > head.length ? ` … and ${problems.length - head.length} more` : ''}`);
  }
  const tail = unconstrained.length
    ? `; ${unconstrained.length} at positions carrying no constraints (held by capability-transport-copies instead)`
    : '';
  return pass(`${checked} embedded example(s) across ${files} schema file(s) validate against their own schema${tail}`);
}

// Produce every declared sub-check plus the catalog the receipt records.
// Order follows CHECKS so the printed block is stable between runs.
function capabilityChecks(root) {
  root = root || ROOT;
  const cat = catalog(root);
  const results = {
    // Not a gate — the receipt line that names what the other checks judged. It
    // is in CHECKS so it cannot be dropped: a receipt reporting capability
    // acceptance without naming the bytes it accepted is exactly what this
    // leaf's fifth completion check is about.
    'capability-catalog': pass(`${cat.sources.length} source(s) [${cat.sources.map((s) => s.id || path.basename(s.path)).join(', ')}], digest ${cat.digest.slice(0, 12)}`),
    'capability-surface': checkSurface(root),
    'capability-schema-version': checkSchemaVersion(root),
    'capability-ids': checkIds(root),
    'capability-examples': checkExamples(root),
    'capability-transport-copies': checkTransportCopies(root),
    'schema-examples': checkSchemaExamples(root),
  };
  return { results, catalog: cat };
}

module.exports = {
  CHECKS,
  capabilityChecks,
  catalog,
  validate,
  // exported for focused tests of the individual gates
  checkSurface,
  checkSchemaVersion,
  checkIds,
  checkExamples,
  checkTransportCopies,
  checkSchemaExamples,
  declaredSources,
  descriptorFiles,
};
