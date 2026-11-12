'use strict';
// #2363: capability-owned schema inputs; the committed aggregate remains the
// public contract and feeds the cached MCP request-schema validators.
const fs = require('node:fs');
const path = require('node:path');
const { randomUUID } = require('node:crypto');
const { ROOT } = require('./util.cjs');

const APP = 'engineering/application';
const OUTPUT = `${APP}/native-transport-v2.schema.json`;
const INPUT = `${APP}/native-transport-v2`;
const read = (root, name) => JSON.parse(fs.readFileSync(path.join(root, name), 'utf8'));
const json = (value) => JSON.stringify(value, null, 2) + '\n';
const demand = (condition, message) => { if (!condition) throw new Error(message); };

function operations(branch) {
  demand(branch && Object.keys(branch).length === 1 && Array.isArray(branch.allOf) && branch.allOf.length === 2,
    'request variant must use the supported envelope/payload allOf representation');
  const [envelope, body] = branch.allOf;
  demand(envelope && Object.keys(envelope).length === 1 && /^#\/\$defs\/[A-Za-z][A-Za-z0-9]*$/.test(envelope.$ref),
    'request variant must reference a local envelope definition');
  demand(body && Object.keys(body).length === 1 && body.properties &&
    Object.keys(body.properties).length === 2 && Object.hasOwn(body.properties, 'payload') && Object.hasOwn(body.properties, 'operation'),
    'request variant needs only operation/payload properties; envelope projection changes need explicit support');
  const op = branch?.allOf?.[1]?.properties?.operation;
  const values = op && ('const' in op ? [op.const] : op.enum);
  demand(Array.isArray(values) && values.length && values.every((v) => typeof v === 'string' && v), 'request variant needs operation const/enum');
  return values;
}

// Historical ordinals preserve public array order and the original aggregate's
// bytes. New definitions/variants can omit order; their stable file/key order
// follows the historical entries, with no central registration or ordinal edit.
function ordered(entries) {
  for (const e of entries) demand(e.order === undefined || (Number.isSafeInteger(e.order) && e.order >= 0), `${e.name}: invalid order`);
  const ranks = new Set();
  for (const e of entries.filter((v) => v.order !== undefined)) {
    demand(!ranks.has(e.order), `${e.name}: duplicate order ${e.order}`);
    ranks.add(e.order);
  }
  return entries.sort((a, b) => (a.order ?? Infinity) - (b.order ?? Infinity) || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
}

function assemble(root = ROOT) {
  const shared = read(root, `${INPUT}/shared.json`);
  demand(shared.schema && !shared.schema.$defs, 'shared schema must be the envelope without $defs');
  const definitions = [], requests = [], names = new Set(), declared = new Map(), schemed = new Set(), ids = new Set();
  const addDefinitions = (fragment, owner) => {
    demand(fragment.definitions && typeof fragment.definitions === 'object' && !Array.isArray(fragment.definitions), `${owner}: missing definitions`);
    for (const [name, entry] of Object.entries(fragment.definitions)) {
      demand(!names.has(name), `duplicate definition ${name} (${owner})`);
      demand(entry && typeof entry.schema === 'object' && entry.schema !== null, `${owner}: invalid definition ${name}`);
      names.add(name);
      definitions.push({ ...entry, name });
    }
  };
  addDefinitions(shared, 'shared');
  const descriptors = fs.readdirSync(path.join(root, APP, 'capabilities-v2')).filter((f) => f.endsWith('.json')).sort();
  demand(descriptors.length, 'no native capability descriptors');
  const expected = new Set(['shared.json']);
  for (const file of descriptors) {
    const d = read(root, `${APP}/capabilities-v2/${file}`);
    demand(typeof d.id === 'string' && !ids.has(d.id), `missing/duplicate capability id ${d.id}`);
    ids.add(d.id);
    demand(Array.isArray(d.operations) && d.operations.length, `${file}: missing operations`);
    const fragment = read(root, `${INPUT}/${file}`);
    expected.add(file);
    demand(fragment.capability === d.id, `${file}: fragment capability does not match ${d.id}`);
    addDefinitions(fragment, d.id);
    for (const op of d.operations) {
      demand(typeof op === 'string' && !declared.has(op), `duplicate/invalid declared operation ${op}`);
      declared.set(op, d.id);
    }
    demand(Array.isArray(fragment.requests), `${file}: missing requests`);
    fragment.requests.forEach((entry, i) => {
      for (const op of operations(entry.schema)) {
        demand(d.operations.includes(op), `${file}: request operation ${op} is not owned by ${d.id}`);
        demand(!schemed.has(op), `duplicate request operation ${op}`);
        schemed.add(op);
      }
      requests.push({ ...entry, name: `${file}:${String(i).padStart(6, '0')}` });
    });
  }
  const extra = fs.readdirSync(path.join(root, INPUT)).filter((f) => f.endsWith('.json') && !expected.has(f));
  demand(!extra.length, `schema fragment(s) without descriptor: ${extra.join(', ')}`);
  for (const op of declared.keys()) demand(schemed.has(op), `missing request variant for ${op}`);
  const schema = { ...shared.schema, $defs: Object.fromEntries(ordered(definitions).map((e) => [e.name, e.schema])) };
  demand(Array.isArray(schema.$defs.Request?.oneOf) && schema.$defs.Request.oneOf.length === 0, 'shared Request.oneOf must be empty; variants belong to capabilities');
  schema.$defs.Request.oneOf = ordered(requests).map((e) => e.schema);
  // Resolve every reference from the aggregate's location, including sibling
  // contracts. Moving inputs must not change reference bases or silently leave
  // a missing definition accepted as an unconstrained schema.
  function refs(node) {
    if (!node || typeof node !== 'object') return;
    if (typeof node.$ref === 'string') {
      const [file, pointer = ''] = node.$ref.split('#');
      demand(!file || /^(?:\.\/)?[A-Za-z0-9._-]+\.schema\.json$/.test(file),
        `schema resource must be a sibling .schema.json file: ${node.$ref}`);
      let target = file ? read(root, path.posix.join(APP, file)) : schema;
      demand(!pointer || pointer.startsWith('/'), `unsupported ref ${node.$ref}`);
      for (const part of pointer ? pointer.slice(1).split('/') : []) {
        const key = part.replace(/~1/g, '/').replace(/~0/g, '~');
        demand(target !== null && typeof target === 'object' && Object.hasOwn(target, key), `unresolved ref ${node.$ref}`);
        target = target[key];
      }
      demand(target !== undefined, `unresolved ref ${node.$ref}`);
    }
    for (const value of Object.values(node)) refs(value);
  }
  refs(schema);
  return schema;
}

function check(root = ROOT) {
  try {
    const output = json(assemble(root));
    demand(fs.readFileSync(path.join(root, OUTPUT), 'utf8') === output,
      `${OUTPUT} is stale; run node scripts/nemo/lib/native-transport-schema.cjs --write`);
    return { problems: [], summary: 'native v2 transport aggregate matches capability-owned fragments' };
  } catch (error) { return { problems: [error.message] }; }
}

function write(root = ROOT) {
  const output = json(assemble(root)); // Validate everything before touching output.
  const destination = path.join(root, OUTPUT);
  const temporary = `${destination}.${randomUUID()}.tmp`;
  let fd;
  let created = false;
  try {
    fd = fs.openSync(temporary, 'wx');
    created = true;
    fs.writeFileSync(fd, output);
    fs.closeSync(fd);
    fd = undefined;
    fs.renameSync(temporary, destination);
  } finally {
    if (fd !== undefined) fs.closeSync(fd);
    if (created) fs.rmSync(temporary, { force: true });
  }
}

if (require.main === module) {
  try {
    demand(process.argv.length === 3 && ['--write', '--check'].includes(process.argv[2]), 'usage: node scripts/nemo/lib/native-transport-schema.cjs --write|--check');
    if (process.argv[2] === '--write') write();
    else { const result = check(); demand(!result.problems.length, result.problems.join('; ')); }
    console.log('native v2 transport schema: ' + process.argv[2].slice(2) + ' passed');
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
module.exports = { assemble, check, write, operations, OUTPUT, INPUT };
