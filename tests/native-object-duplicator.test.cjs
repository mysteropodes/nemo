'use strict';
// L04-N duplicator setup intents: enable/disable, object pool and path layer are one closed typed request each. Every request
// crosses the REAL common v2 SDK. Seeds, pool order and the toggle defaults are pinned to rows executed from the baseline JS.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const { createHarness, createSession } = require('./native-object-component-harness.cjs');
const duplicator = require('../src/js/adapters/native-object-duplicator.js');
const core = require('../src/js/adapters/native-object-component-intent.js');

const ROOT = path.resolve(__dirname, '..');
const P = 'command.document.object.';
const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-object-component-fixtures', 'oracle.json'), 'utf8'));

function make(options, hooks) {
  const h = createHarness(options);
  return Object.assign(h, { intent: duplicator.createDuplicatorIntent(h.ports, hooks) });
}

test('the drawn seed is Math.floor(random * 1e6) for every baseline row of both baseline writers', () => {
  assert.equal(oracle.seeds.length >= 8, true);
  for (const row of oracle.seeds) {
    assert.equal(duplicator.drawSeed(() => row.random), row.toggleSeed, 'toggleLayerDuplicator ' + row.random);
    assert.equal(duplicator.drawSeed(() => row.random), row.objectsSeed, 'convertSelectionToObjectDuplicator ' + row.random);
    assert.equal(row.toggleSeed >= 0 && row.toggleSeed <= 999999, true);
  }
  assert.equal(oracle.seeds.some((row) => row.toggleSeed === 999999), true, 'the largest drawable seed is among the rows');
});

test('enabling sends the baseline target state with a fresh seed; the baseline also locks the layer, which the owner does', async () => {
  for (const row of oracle.seeds) {
    const h = make({}, { random: () => row.random });
    const out = await h.intent.setEnabled('layer-b', true);
    assert.deepEqual(h.state.sent[0].payload, { command: 'object.duplicator.set', layerUid: 'layer-b', enabled: true, seed: row.toggleSeed });
    assert.equal(h.state.sent[0].operation, P + 'duplicator.set');
    assert.deepEqual([out.ok, out.applied, out.layerUid], [true, true, undefined]);
    assert.equal(row.toggleLocked, true, 'baseline: enabling locks the layer');
    assert.equal(row.toggleMode, 'grid');
  }
});

test('disabling sends no seed, and the baseline row confirms it removes the descriptor and unlocks the layer', async () => {
  const h = make({});
  const out = await h.intent.setEnabled('layer-b', false);
  assert.deepEqual(h.state.sent[0].payload, { command: 'object.duplicator.set', layerUid: 'layer-b', enabled: false });
  assert.equal(out.ok, true);
  for (const row of oracle.seeds) { assert.equal(row.disabledDuplicator, null); assert.equal(row.disabledLocked, false); }
  const noop = make({ step: () => ({ applied: false }) });
  const same = await noop.intent.setEnabled('layer-b', false);
  assert.deepEqual([same.ok, same.applied, same.revision, same.synchronized], [true, false, 7, true], 'a no-op publishes nothing and keeps the revision');
});

test('toggle follows the native state and is typed unavailability without it', async () => {
  const on = make({}, { random: () => 0.5, isDuplicator: (uid) => uid === 'layer-a' });
  await on.intent.toggle('layer-a');
  assert.equal(on.state.sent[0].payload.enabled, false, 'an enabled layer toggles off');
  await on.intent.toggle('layer-b');
  assert.deepEqual(on.state.sent[1].payload, { command: 'object.duplicator.set', layerUid: 'layer-b', enabled: true, seed: 500000 });
  for (const hooks of [{}, { isDuplicator: () => null }, { isDuplicator: () => { throw new Error('x'); } }]) {
    const h = make({}, hooks);
    const out = await h.intent.toggle('layer-a');
    assert.deepEqual([out.ok, out.code, out.reason], [false, 'unavailable', 'duplicator_state_unavailable']);
    assert.equal(h.state.sent.length, 0);
  }
});

test('an unusable random source or a stale layer is refused before anything is sent', async () => {
  for (const random of [() => 1, () => -0.1, () => NaN, () => { throw new Error('no random'); }]) {
    const h = make({}, { random });
    const out = await h.intent.setEnabled('layer-a', true);
    assert.deepEqual([out.ok, out.code], [false, 'invalid_request']);
    assert.equal(h.state.sent.length, 0);
  }
  const h = make({});
  assert.deepEqual([(await h.intent.setEnabled('layer-x', true)).code, (await h.intent.setEnabled('layer-x', false)).ids], ['stale_selection', ['layer-x']]);
  for (const [uid, enabled] of [['', true], [7, true], ['layer-a', 'yes'], [undefined, false]]) assert.equal((await h.intent.setEnabled(uid, enabled)).code, 'invalid_request');
  assert.equal(h.state.sent.length, 0);
});

test('the object pool is de-duplicated and sent in document order with a fresh seed; the minted host is reported', async () => {
  const h = make({ layers: ['layer-a', 'layer-b', 'layer-c'] }, { random: () => 0.25 });
  const out = await h.intent.fromObjects(['layer-c', 'layer-a', 'layer-c']);
  assert.deepEqual(h.state.sent[0].payload, { command: 'object.duplicator.objects', layerUids: ['layer-a', 'layer-c'], seed: 250000 });
  assert.equal(h.state.sent[0].operation, P + 'duplicator.objects');
  assert.deepEqual([out.ok, out.applied, out.layerUid, out.hostLayerUid], [true, true, 'native-layer:8:0', 'native-layer:8:0']);
});

test('the pool rows of the baseline: order, repeats, fewer than two layers and the seed', async () => {
  const byName = (name) => oracle.pools.find((row) => row.name === name);
  const unsorted = byName('unsorted with a repeat');
  assert.deepEqual(unsorted.sourceLayerUids, ['u1', 'u2', 'u3']);
  const h = make({ layers: unsorted.uids });
  await h.intent.fromObjects(unsorted.indices.map((i) => unsorted.uids[i]));
  assert.deepEqual(h.state.sent[0].payload.layerUids, unsorted.sourceLayerUids, 'the adapter sends the pool the baseline built');
  const two = byName('two layers');
  const t = make({ layers: two.uids });
  await t.intent.fromObjects(two.indices.map((i) => two.uids[i]));
  assert.deepEqual(t.state.sent[0].payload.layerUids, two.sourceLayerUids);
  for (const name of ['a single layer', 'the same layer twice', 'empty']) {
    const row = byName(name);
    assert.equal(row.result, -1, 'baseline: ' + name + ' returns -1');
    const lone = make({ layers: row.uids });
    const out = await lone.intent.fromObjects(row.indices.map((i) => row.uids[i]));
    assert.deepEqual([out.ok, out.code, out.reason], [false, 'invalid_request', 'too_few_layers'], name);
    assert.equal(lone.state.sent.length, 0);
  }
  // A duplicator host inside the selection is skipped by the baseline; the owner makes that decision, so the request is sent whole.
  const hosts = byName('a host is skipped but the rest pools');
  assert.deepEqual(hosts.sourceLayerUids, ['u0', 'u2']);
  const skipping = make({ layers: hosts.uids });
  await skipping.intent.fromObjects(hosts.indices.map((i) => hosts.uids[i]));
  assert.deepEqual(skipping.state.sent[0].payload.layerUids, ['u0', 'u1', 'u2']);
  assert.equal(byName('a host leaves a pool of one').result, -1);
  const refused = make({ layers: ['u0', 'u1'], step: () => ({ error: { code: 'invalid_request', message: 'Select at least 2 layers.' } }) });
  const out = await refused.intent.fromObjects(['u0', 'u1']);
  assert.deepEqual([out.ok, out.code, out.message], [false, 'invalid_request', 'Select at least 2 layers.']);
  for (const row of oracle.pools.filter((r) => r.result !== -1)) assert.equal(row.hidden.filter(Boolean).length, row.sourceLayerUids.length, 'baseline hides exactly the pooled layers');
});

test('a stale pool member is refused before sending and bad input is invalid', async () => {
  const h = make({});
  const stale = await h.intent.fromObjects(['layer-a', 'layer-x']);
  assert.deepEqual([stale.ok, stale.code, stale.ids], [false, 'stale_selection', ['layer-x']]);
  for (const bad of [null, 'layer-a', [7, 'layer-a'], ['', 'layer-a']]) assert.equal((await h.intent.fromObjects(bad)).code, 'invalid_request', JSON.stringify(bad));
  assert.equal(h.state.sent.length, 0);
});

test('the path picker sets a layer, clears with null, maps the empty select value like the baseline, and refuses stale layers', async () => {
  const h = make({});
  await h.intent.setPath('layer-a', 'layer-b');
  await h.intent.setPath('layer-a', null);
  await h.intent.setPath('layer-a', '');
  assert.deepEqual(h.state.sent.map((r) => r.payload), [
    { command: 'object.duplicator.path', layerUid: 'layer-a', pathLayerUid: 'layer-b' },
    { command: 'object.duplicator.path', layerUid: 'layer-a', pathLayerUid: null },
    { command: 'object.duplicator.path', layerUid: 'layer-a', pathLayerUid: null },
  ]);
  assert.equal(h.state.sent[0].operation, P + 'duplicator.path');
  const stale = await h.intent.setPath('layer-a', 'layer-x');
  assert.deepEqual([stale.ok, stale.code, stale.ids], [false, 'stale_selection', ['layer-x']]);
  assert.equal((await h.intent.setPath('layer-x', null)).code, 'stale_selection');
  for (const [uid, p] of [['', null], [7, null], ['layer-a', 7], ['layer-a', undefined]]) assert.equal((await h.intent.setPath(uid, p)).code, 'invalid_request');
  assert.equal(h.state.sent.length, 3);
  // The owner's typed refusals (itself, a chained duplicator, a layer that is not a duplicator) arrive unchanged.
  for (const [code, message] of [['invalid_request', 'A duplicator cannot path along itself.'], ['invalid_request', 'The path layer is a duplicator.'], ['not_found', 'Layer carries no duplicator.']]) {
    const refused = make({ step: () => ({ error: { code, message } }) });
    const out = await refused.intent.setPath('layer-a', 'layer-a');
    assert.deepEqual([out.ok, out.code, out.message], [false, code, message]);
  }
  const noop = make({ step: () => ({ applied: false }) });
  assert.deepEqual([(await noop.intent.setPath('layer-a', null)).applied], [false]);
});

test('unavailability is typed and requests are serialized with transport failures never retried', async () => {
  const noDecl = make({ declared: () => false });
  assert.deepEqual([(await noDecl.intent.setPath('layer-a', null)).reason], ['native_declaration_absent']);
  const noSession = make({ noSession: true });
  assert.deepEqual([(await noSession.intent.setEnabled('layer-a', false)).reason], ['no_presented_native_session']);
  const h = make({}, { random: () => 0.5 });
  const outcomes = await Promise.all([h.intent.setEnabled('layer-a', true), h.intent.fromObjects(['layer-a', 'layer-b']), h.intent.setPath('layer-a', null)]);
  assert.deepEqual(h.state.sent.map((r) => r.expectedRevision), [7, 8, 9]);
  assert.deepEqual(outcomes.map((o) => o.revision), [8, 9, 10]);
  const dropped = make({ step: () => ({ drop: 'socket closed' }) });
  const lost = await dropped.intent.setPath('layer-a', null);
  assert.deepEqual([lost.ok, lost.code, lost.indeterminate, dropped.state.sent.length], [false, 'transport_failed', true, 1]);
  const stale = make({ sessionStyle: true, step: () => ({ error: { code: 'stale_revision', message: 'stale' } }) });
  const refused = await stale.intent.setPath('layer-a', null);
  assert.deepEqual([refused.ok, refused.code, refused.stale], [false, 'stale_revision', true]);
});

test('desktop() binds once per host and reaches the common resource route', async () => {
  const s = createSession({});
  const host = { NemoNativeObjectProject: { desktop: () => s.session } };
  const bound = duplicator.desktop(host, { random: () => 0.5 });
  assert.equal(duplicator.desktop(host, {}), bound);
  const out = await bound.setEnabled('layer-a', false);
  assert.deepEqual([out.ok, s.log.dispatched[0].operation, s.log.dispatched[0].payload], [true, P + 'duplicator.set', { command: 'object.duplicator.set', layerUid: 'layer-a', enabled: false }]);
  assert.equal(duplicator.desktop({ NemoNativeObjectProject: { desktop: () => createSession({ blocks: false }).session } }, {}), null);
  assert.equal(duplicator.desktop(null, {}), null);
  void core;
});

test('no legacy duplicator writer is reachable: spies stay silent and the sources never name them', async () => {
  const names = ['toggleLayerDuplicator', 'convertSelectionToObjectDuplicator', 'applyLayerDuplicator', 'getEffectiveStrokesRendered', 'dupRefresh', 'updateDuplicatorPanel',
    'setDuplicatorEditSource', '_resolveDuplicatorPath', 'convertLayerToComponent'];
  const hits = [];
  names.forEach((name) => { globalThis[name] = () => { hits.push(name); }; });
  try {
    const h = make({}, { random: () => 0.5, isDuplicator: () => false });
    await h.intent.setEnabled('layer-a', true); await h.intent.toggle('layer-a'); await h.intent.fromObjects(['layer-a', 'layer-b']); await h.intent.setPath('layer-a', 'layer-b');
  } finally { names.forEach((name) => { delete globalThis[name]; }); }
  assert.deepEqual(hits, []);
  const text = fs.readFileSync(path.join(ROOT, 'src/js/adapters/native-object-duplicator.js'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
  for (const name of names.concat(['state.layers', 'Paper', 'paper.', 'SMMotion', 'pushUndo', 'loadFrame'])) assert.equal(text.includes(name), false, 'the duplicator adapter must not name ' + name);
});
