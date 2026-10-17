'use strict';
/* Lane-6 text-tool adapter. Real adapter, command and contract modules over a session double with the real route
 * semantics (see native-object-item-support.cjs). The baseline rows come from the REAL buildVectorTextGroup run in a
 * Node vm by tools/gen_adapters.js; nothing here establishes native or installed execution. */
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Text = require('../src/js/adapters/native-object-text-layout.js');
const Contract = require('../src/js/adapters/native-object-item-contract.js');
const { createFixture, forbidLegacyWriters } = require('./native-object-item-support.cjs');

const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-object-item-fixtures', 'text-layout-oracle.json'), 'utf8'));
const rejection = (promise) => promise.then(() => assert.fail('expected a rejection'), (error) => error);
const adapter = (options) => {
  const fixture = createFixture(options);
  return Object.assign(fixture, { text: Text.create(fixture.session) });
};
const FONTS = ['Roboto-Regular', 'Roboto-Bold', 'DejaVuSans-Regular', 'DejaVuSans-Bold', 'DejaVuSerif-Regular', 'DejaVuSerif-Bold', 'Manrope-Regular', 'Inter-Regular'];
const PLAIN = {
  text: 'Hello world', font: { kind: 'bundled', fontKey: 'Roboto-Regular' }, size: 17, color: '#112233', align: 'left', fixedWidth: null, fixedHeight: null,
  bold: false, italic: false, underline: false, strike: false, letterSpacing: 0, wordSpacing: 0, lineHeightMult: 1.25, textCase: 'none', anchorTopLeft: { x: 10, y: 20 },
};

test('the font table is the baseline VECTOR_FONTS table and legacy keys map without guessing', () => {
  assert.equal(oracle.baseline, 'fbe4b8a33e6d5a229f014154df2ba21988a9fabc');
  assert.deepEqual(oracle.vectorFonts, FONTS);
  assert.deepEqual(Contract.FONTS, FONTS);
  for (const key of FONTS) {
    assert.deepEqual(Text.fontFromLegacyKey(key), { kind: 'bundled', fontKey: key });
    assert.deepEqual(Text.fontFromLegacyKey('vector:' + key), { kind: 'bundled', fontKey: key });
  }
  for (const bad of ['sans-serif', 'Google:Roboto-Regular', 'vector:Unknown', 'vector:vector:Roboto-Regular', 'roboto-regular', '', 'vector:', 42, null, undefined]) {
    assert.throws(() => Text.fontFromLegacyKey(bad), (e) => e.code === 'invalid_request' && e.reason === 'unsupported_font', String(bad));
  }
});

test('tool arguments become the baseline block record: 7 rows from the real buildVectorTextGroup', () => {
  assert.equal(oracle.rows.length, 7);
  for (const row of oracle.rows) {
    const block = Text.blockFromToolArgs(row.args);
    assert.deepEqual(Object.keys(block), Object.keys(row.record), row.name + ' member order');
    assert.deepEqual(block, row.record, row.name);
    // The adapter keeps the AUTHORED font key; the baseline stored the weight-resolved one (the one documented difference).
    assert.equal(row.foldsFont, row.data.vectorFont !== row.args.fontKey, row.name);
    assert.doesNotThrow(() => Contract.payload(Contract.ITEM, { command: 'object.text.create', target: Text.stableTarget('L0', '', 0), block }), row.name);
  }
  const [plain] = oracle.rows;
  assert.deepEqual(Text.blockFromToolArgs(plain.args), PLAIN);
  assert.deepEqual(oracle.rows.filter((row) => row.foldsFont).map((row) => [row.args.fontKey, row.args.bold, row.data.vectorFont]),
    [['Roboto-Regular', true, 'Roboto-Bold'], ['DejaVuSerif-Bold', false, 'DejaVuSerif-Regular']]);
  const coerced = oracle.rows.find((row) => row.name === 'create_falsy_values_take_the_baseline_defaults');
  assert.deepEqual([coerced.args.fixedWidth, coerced.args.fixedHeight, coerced.args.lineHeightMult, coerced.args.textCase], [0, 0, 0, '']);
  assert.deepEqual([coerced.data.fixedWidth, coerced.data.fixedHeight, coerced.data.lineHeightMult, coerced.data.textCase], [null, null, 1.25, 'none']);
  assert.throws(() => Text.blockFromToolArgs(null), (e) => e.code === 'invalid_request');
  assert.throws(() => Text.blockFromToolArgs(Object.assign({}, plain.args, { fontKey: 'sans-serif' })), (e) => e.reason === 'unsupported_font');
  assert.throws(() => Text.blockFromToolArgs(Object.assign({}, plain.args, { topLeft: undefined })), (e) => e.code === 'invalid_request');
});

test('create, edit and delete send the closed payloads and report the owner-allocated id', async () => {
  const f = adapter();
  const created = await f.text.createBlock('L0', 2, PLAIN);
  assert.deepEqual(f.owner.log.received[0], {
    apiVersion: 2, requestId: 'project-item-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation: 'command.document.object.item.edit',
    payload: { command: 'object.text.create', target: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 2 }, layerUid: 'L0', strokeId: '' }, block: PLAIN },
  });
  assert.deepEqual(created, { requestId: 'project-item-1', contentRevision: 8, applied: true, historyEntriesAdded: 1, operation: 'text.create', strokeId: 'native-text:8:0' });
  const target = Text.stableTarget('L0', created.strokeId, 2);
  const edited = await f.text.editBlock(target, { size: 24, fixedWidth: null, font: { kind: 'asset', assetId: 'font-1' } });
  assert.deepEqual(f.owner.log.received[1].payload, { command: 'object.text.edit', stableTarget: target, patch: { size: 24, fixedWidth: null, font: { kind: 'asset', assetId: 'font-1' } } });
  assert.deepEqual([edited.operation, edited.strokeId, edited.applied, edited.contentRevision], ['text.edit', 'native-text:8:0', true, 9]);
  const same = await f.text.editBlock(target, { size: 24 });
  assert.deepEqual([same.applied, same.historyEntriesAdded, same.contentRevision], [false, 0, 9], 'an effective no-op keeps the revision');
  const removed = await f.text.deleteBlock(target);
  assert.deepEqual(f.owner.log.received[3].payload, { command: 'object.text.delete', stableTarget: target });
  assert.deepEqual([removed.operation, removed.applied], ['text.delete', true]);
  assert.deepEqual(f.owner.log.executed, ['project-item-1', 'project-item-2', 'project-item-3', 'project-item-4']);
});

test('the baseline blank-text guards skip the gesture without any dispatch', async () => {
  const f = adapter();
  const target = Text.stableTarget('L0', 'native-text:8:0', 0);
  for (const row of oracle.blankGuards) {
    const create = f.text.createBlock('L0', 0, Object.assign({}, PLAIN, { text: row.text }));
    const edit = f.text.editBlock(target, { text: row.text });
    if (row.blank) {
      assert.deepEqual(await create, { applied: false, skipped: 'blank_text' }, JSON.stringify(row.text));
      assert.deepEqual(await edit, { applied: false, skipped: 'blank_text' }, JSON.stringify(row.text));
    } else {
      assert.equal((await create).applied, true, JSON.stringify(row.text));
      assert.equal((await edit).skipped, undefined, JSON.stringify(row.text));
    }
  }
  assert.deepEqual(oracle.blankGuards.filter((row) => !row.blank).map((row) => row.text), ['​', 'a', ' a ']);
  const blanks = oracle.blankGuards.filter((row) => row.blank).length;
  assert.equal(f.owner.log.received.length, (oracle.blankGuards.length - blanks) * 2);
});

test('an edit can never re-anchor a block (baseline feedback #37) and unknown members are refused locally', async () => {
  const f = adapter();
  const target = Text.stableTarget('L0', 'native-text:8:0', 0);
  for (const patch of [{ anchorTopLeft: { x: 1, y: 2 } }, { isText: true }, { textOverflow: false }, {}, { size: '9' }, { color: 'red' }, { align: 'justify' }, { text: 'a\u0000' }, null]) {
    const error = await rejection(f.text.editBlock(target, patch));
    assert.equal(error.code, 'invalid_request', JSON.stringify(patch));
  }
  for (const [member, value] of [['font', { kind: 'google', family: 'Roboto' }], ['extra', 1], ['anchorTopLeft', { x: 'a', y: 0 }], ['fixedWidth', '120']]) {
    const block = Object.assign({}, PLAIN, { [member]: value });
    assert.equal((await rejection(f.text.createBlock('L0', 0, block))).code, 'invalid_request', member);
  }
  for (const key of Object.keys(PLAIN).filter((k) => ['text', 'font', 'size', 'color', 'align', 'anchorTopLeft'].includes(k))) {
    const block = Object.assign({}, PLAIN);
    delete block[key];
    assert.equal((await rejection(f.text.createBlock('L0', 0, block))).code, 'invalid_request', 'missing ' + key);
  }
  assert.equal((await rejection(f.text.deleteBlock({ layerUid: 'L0' }))).code, 'invalid_request');
  assert.equal((await rejection(f.text.createBlock('', 0, PLAIN))).code, 'invalid_request');
  assert.equal(f.owner.log.received.length, 0);
});

test('owner refusals are typed and unchanged: a missing font provider or asset never becomes a fallback', async () => {
  const refusals = [
    [{ code: 'unavailable', message: 'refused' }, 'unavailable', null],
    [{ code: 'unavailable', message: 'refused', details: { reason: 'font_provider_unavailable' } }, 'unavailable', 'font_provider_unavailable'],
    [{ code: 'not_found', message: 'refused' }, 'not_found', null],
    [{ code: 'invalid_request', message: 'refused', details: { reason: 'not_a_font' } }, 'invalid_request', 'not_a_font'],
  ];
  for (const [error, code, reason] of refusals) {
    const f = adapter({ refuse: () => error });
    const failure = await rejection(f.text.createBlock('L0', 0, Object.assign({}, PLAIN, { font: { kind: 'asset', assetId: 'font-1' } })));
    assert.deepEqual([failure.code, failure.reason, failure.contentRevision], [code, reason, 7]);
    assert.equal(f.owner.log.received.length, 1, 'refusals are not retried');
    assert.equal(f.state.revision, 7);
    assert.equal(f.owner.items.size, 0);
  }
});

test('a lost reply replays the same request and creates exactly one block', async () => {
  const f = adapter({ faults: ['after'] });
  const created = await f.text.createBlock('L0', 0, PLAIN);
  assert.equal(created.strokeId, 'native-text:8:0');
  assert.equal(f.owner.items.size, 1);
  assert.deepEqual(f.owner.log.received[0], f.owner.log.received[1]);
  assert.equal(f.owner.log.executed.length, 1);
  const lost = adapter({ faults: ['after', 'after'] });
  const unknown = await rejection(lost.text.createBlock('L0', 0, PLAIN));
  assert.deepEqual([unknown.code, lost.owner.items.size], ['item_command_indeterminate', 1]);
  assert.equal((await unknown.replay()).strokeId, 'native-text:8:0');
  assert.equal(lost.owner.items.size, 1);
  // other edits published between the lost reply and the replay: the stored receipt keeps its ORIGINAL revision and nothing wedges
  const moved = adapter({ faults: ['after', 'after'] });
  const pending = await rejection(moved.text.createBlock('L0', 0, PLAIN));
  moved.state.revision += 2;
  const stored = await pending.replay();
  assert.deepEqual([stored.strokeId, stored.contentRevision, moved.owner.log.executed.length], ['native-text:8:0', 8, 1]);
  assert.ok(moved.events.reconciled.every((revision) => revision === undefined));
});

test('a stale pin is the owner refusal, passed through, and the next call reads the new revision', async () => {
  const f = adapter({ refuse: (request) => (request.expectedRevision === 7 ? { code: 'stale_revision', message: 'refused' } : undefined) });
  assert.equal((await rejection(f.text.createBlock('L0', 0, PLAIN))).code, 'stale_revision');
  f.state.revision = 9;
  const created = await f.text.createBlock('L0', 0, PLAIN);
  assert.deepEqual([f.owner.log.received[1].expectedRevision, created.contentRevision], [9, 10]);
});

test('no Paper, glyph or legacy text writer is reachable from the adapter', async () => {
  const guard = forbidLegacyWriters(['state', 'paper', 'SMVectorText', 'buildVectorTextGroup', 'opentype', 'userLayers']);
  try {
    const f = adapter();
    const created = await f.text.createBlock('L0', 0, PLAIN);
    await f.text.editBlock(Text.stableTarget('L0', created.strokeId, 0), { text: 'Bye' });
  } finally { guard.restore(); }
  for (const file of ['native-object-text-layout.js', 'native-object-item-command.js', 'native-object-item-contract.js']) {
    const source = fs.readFileSync(path.join(__dirname, '..', 'src/js/adapters', file), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
    assert.doesNotMatch(source, /SMVectorText|buildVectorTextGroup|vector-text-bridge|opentype|\bPaper\b|\bpaper\b|new Path|localStorage|state\./, file);
  }
});

test('desktop binding needs the native session and is cached per host', async () => {
  const f = createFixture();
  let bound = true;
  const host = { NemoNativeObjectProject: { desktop: () => Object.assign({ blocksLegacy: () => bound }, f.session) } };
  const first = Text.desktop(host);
  assert.equal(Text.desktop(host), first);
  assert.equal((await first.createBlock('L0', 0, PLAIN)).strokeId, 'native-text:8:0');
  bound = false;
  assert.equal(Text.desktop({ NemoNativeObjectProject: host.NemoNativeObjectProject }), null);
  assert.equal(Text.desktop({}), null);
  const bare = Text.create(createFixture({ noRoute: true }).session);
  const error = await rejection(bare.createBlock('L0', 0, PLAIN));
  assert.deepEqual([error.code, error.reason], ['unavailable', 'resource_route_unavailable']);
});
