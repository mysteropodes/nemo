'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const transport = require('../scripts/nemo/lib/native-transport-schema.cjs');
const drift = require('../scripts/nemo/lib/capability-drift.cjs');
const ROOT = path.resolve(__dirname, '..');
const APP = 'engineering/application';
const read = (root, name) => JSON.parse(fs.readFileSync(path.join(root, name), 'utf8'));
const write = (root, name, value) => fs.writeFileSync(path.join(root, name), JSON.stringify(value, null, 2) + '\n');
const fragment = `${transport.INPUT}/native-opacity.json`;
function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-schema-fragments-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.cpSync(path.join(ROOT, APP), path.join(root, APP), { recursive: true });
  fs.mkdirSync(path.join(root, 'scripts/nemo/lib'), { recursive: true });
  for (const name of ['native-transport-schema.cjs', 'util.cjs']) fs.copyFileSync(path.join(ROOT, 'scripts/nemo/lib', name), path.join(root, 'scripts/nemo/lib', name));
  return root;
}
function cli(root, flag) {
  return spawnSync(process.execPath, [path.join(root, 'scripts/nemo/lib/native-transport-schema.cjs'), flag], { encoding: 'utf8', cwd: root });
}
function mutate(root, name, fn) { const d = read(root, name); fn(d); write(root, name, d); }

test('aggregate exactly preserves the frozen ea1b3c6 schema, order, constraints and references', () => {
  const aggregate = transport.assemble();
  const bytes = JSON.stringify(aggregate, null, 2) + '\n';
  // The independent pre-migration wire contract is the contiguous historical
  // prefix. Additive capabilities append entries and need no central pin edit;
  // changing an existing wire constraint still requires explicit oracle review.
  const frozen = structuredClone(aggregate);
  frozen.$defs = Object.fromEntries(Object.entries(frozen.$defs).slice(0, 387));
  frozen.$defs.Request.oneOf = frozen.$defs.Request.oneOf.slice(0, 151);
  const frozenBytes = JSON.stringify(frozen, null, 2) + '\n';
  assert.equal(crypto.createHash('sha256').update(frozenBytes).digest('hex'), 'ab459a63a0695fea3941fc84e1a970d4bd6afa9d2f0f9dbcfb0a16c3281c4474');
  assert.equal(bytes, fs.readFileSync(path.join(ROOT, transport.OUTPUT), 'utf8'));
  assert.deepEqual(transport.check().problems, []);
});

test('existing descriptor refs resolve through the unchanged aggregate path', () => {
  const schema = transport.assemble();
  let checked = 0;
  function visit(node) {
    if (!node || typeof node !== 'object') return;
    if (node.$ref?.startsWith('../native-transport-v2.schema.json#/')) {
      const pointer = node.$ref.split('#/')[1];
      assert.notEqual(pointer.split('/').reduce((n, key) => n?.[key], schema), undefined, node.$ref);
      checked++;
    }
    Object.values(node).forEach(visit);
  }
  for (const file of drift.nativeDescriptorFiles()) visit(JSON.parse(fs.readFileSync(file, 'utf8')));
  assert.ok(checked > 100, checked);
});

test('CLI write is deterministic and check refuses stale output without rewriting it', (t) => {
  const root = fixture(t);
  const original = fs.readFileSync(path.join(root, transport.OUTPUT), 'utf8');
  assert.equal(cli(root, '--write').status, 0);
  assert.equal(fs.readFileSync(path.join(root, transport.OUTPUT), 'utf8'), original);
  mutate(root, fragment, (d) => { d.requests[0].schema.allOf[1].properties.payload.description = 'negative control'; });
  const stale = cli(root, '--check');
  assert.equal(stale.status, 1);
  assert.match(stale.stderr, /is stale/);
  assert.equal(fs.readFileSync(path.join(root, transport.OUTPUT), 'utf8'), original);
  assert.match(drift.checkNativeCatalog(root).reason, /is stale/);
  assert.equal(cli(root, '--write').status, 0);
  assert.equal(cli(root, '--check').status, 0);
});

test('missing or duplicate variants, duplicate definitions, broken refs and orphan fragments fail closed', (t) => {
  const cases = [
    ['missing variant', (r) => mutate(r, fragment, (d) => d.requests.shift()), /missing request variant/],
    ['duplicate variant', (r) => mutate(r, fragment, (d) => d.requests.push(d.requests[0])), /duplicate request operation/],
    ['duplicate definition', (r) => mutate(r, fragment, (d) => { d.definitions.Identifier = { schema: {} }; }), /duplicate definition Identifier/],
    ['broken local ref', (r) => mutate(r, fragment, (d) => { d.requests[0].schema.allOf[0].$ref = '#/$defs/Absent'; }), /unresolved ref/],
    ['missing external ref', (r) => mutate(r, fragment, (d) => { d.requests[0].schema.allOf[0].$ref = 'native-object-v1.schema.json#/$defs/Absent'; }), /unresolved ref/],
    ['wrong owner', (r) => mutate(r, fragment, (d) => { d.capability = 'wrong'; }), /does not match/],
    ['orphan fragment', (r) => write(r, `${transport.INPUT}/orphan.json`, {}), /without descriptor/],
    ['missing fragment', (r) => fs.rmSync(path.join(r, fragment)), /ENOENT/],
  ];
  for (const [name, change, expected] of cases) {
    const root = fixture(t);
    const before = fs.readFileSync(path.join(root, transport.OUTPUT), 'utf8');
    change(root);
    assert.match(transport.check(root).problems.join('; '), expected, name);
    assert.throws(() => transport.write(root), expected, name);
    assert.equal(fs.readFileSync(path.join(root, transport.OUTPUT), 'utf8'), before, name);
  }
});

test('a new capability adds its own descriptor and fragment without a central registration edit', (t) => {
  const root = fixture(t);
  const before = transport.assemble(root);
  const descriptor = read(root, `${APP}/capabilities-v2/native-opacity.json`);
  descriptor.id = 'native.fixture';
  descriptor.operations = ['query.fixture'];
  write(root, `${APP}/capabilities-v2/native-fixture.json`, descriptor);
  write(root, `${transport.INPUT}/native-fixture.json`, {
    capability: descriptor.id,
    definitions: { FixturePayload: { schema: { type: 'object', additionalProperties: false } } },
    requests: [{ schema: { allOf: [{ $ref: '#/$defs/RequestBase' }, { properties: {
      operation: { const: 'query.fixture' }, payload: { $ref: '#/$defs/FixturePayload' },
    } }] } }],
  });
  assert.match(transport.check(root).problems.join('; '), /is stale/);
  assert.equal(cli(root, '--write').status, 0);
  assert.equal(cli(root, '--check').status, 0);
  const after = read(root, transport.OUTPUT);
  assert.deepEqual(after.$defs.Request.oneOf.slice(0, -1), before.$defs.Request.oneOf);
  assert.equal(after.$defs.Request.oneOf.at(-1).allOf[1].properties.operation.const, 'query.fixture');
  assert.deepEqual(after.$defs.FixturePayload, { type: 'object', additionalProperties: false });
});

test('normal packaged build checks the same aggregate before compiling the sidecar', () => {
  const command = read(ROOT, 'src-tauri/tauri.conf.json').build.beforeBuildCommand;
  assert.equal(command, 'python3 scripts/assert-no-private-labs.py && node scripts/nemo/lib/native-transport-schema.cjs --check && node scripts/build-mcp-sidecar.cjs');
});

test('aggregate publication renames a complete exclusive sibling and cleans up a failed rename', (t) => {
  const root = fixture(t);
  const destination = path.join(root, transport.OUTPUT);
  const original = fs.readFileSync(destination, 'utf8');
  const rename = fs.renameSync;
  let renames = 0;
  try {
    fs.renameSync = (source, target) => {
      assert.equal(path.dirname(source), path.dirname(destination));
      assert.equal(target, destination);
      assert.equal(fs.readFileSync(source, 'utf8'), original);
      assert.equal(fs.readFileSync(destination, 'utf8'), original);
      renames++;
      return rename(source, target);
    };
    transport.write(root);
    assert.equal(renames, 1);
    fs.renameSync = () => { throw new Error('fixture rename failure'); };
    assert.throws(() => transport.write(root), /fixture rename failure/);
    assert.equal(fs.readFileSync(destination, 'utf8'), original);
    assert.deepEqual(fs.readdirSync(path.dirname(destination)).filter((f) => f.endsWith('.tmp')), []);
  } finally { fs.renameSync = rename; }
});
