'use strict';
// P05 — deterministic capability registration.
//
// Most of these are mutation tests: every rejection is proven to FIRE, not
// merely declared. A registry whose guards cannot be shown to reject is the
// same defect class this leaf exists to close — see the availability test
// below, which is the P04/#1086 blocker turned into an executable check.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const registryModule = require('../src/js/application/capability-registry.js');
const ROOT = path.resolve(__dirname, '..');
const CAPS = path.join(ROOT, 'engineering', 'application', 'capabilities');

function realDescriptor(name) {
  return JSON.parse(fs.readFileSync(path.join(CAPS, name + '.json'), 'utf8'));
}

// A minimal descriptor carrying exactly the fields registration inspects.
// Deliberately NOT a copy of capability-v1's full shape: this module does not
// re-validate conformance (tests/capability-v1-schema.test.cjs owns that), so
// a fixture that pretended to would be testing the wrong thing.
function descriptor(overrides) {
  return Object.assign({
    schemaVersion: 1, id: 'sample', version: '1.0.0',
    input: {}, output: {}, units: [],
    effects: { kind: 'read', scope: 'none' },
    availability: { state: 'unavailable', reason: 'missing-dependency' },
    handlerKey: 'application.sample.thing', fixture: {}, examples: [],
  }, overrides);
}

function codeOf(fn) {
  try { fn(); } catch (error) { return error.code; }
  return null;
}

// ---- outcome check 1: explicit, distinguishable failures -----------------

test('a duplicate id is refused', () => {
  const registry = registryModule.create();
  registry.register(descriptor({ id: 'dup', handlerKey: 'application.a.one' }), () => 1);
  assert.equal(codeOf(() => registry.register(descriptor({ id: 'dup', handlerKey: 'application.b.two' }), () => 2)),
    'duplicate_id');
});

test('an unsupported schemaVersion is refused before anything else is read', () => {
  const registry = registryModule.create();
  for (const bad of [0, 2, '1', null, undefined]) {
    assert.equal(codeOf(() => registry.register(descriptor({ schemaVersion: bad }), () => 1)),
      'schema_version_unsupported', `schemaVersion ${JSON.stringify(bad)} was accepted`);
  }
  // And the supported one is genuinely accepted — otherwise the check above
  // would pass for the wrong reason.
  assert.equal(registry.register(descriptor({ schemaVersion: 1 }), () => 1), 'sample');
});

test('a descriptor with no input/output schema is refused', () => {
  const registry = registryModule.create();
  const noInput = descriptor({ id: 'a' }); delete noInput.input;
  const noOutput = descriptor({ id: 'b' }); delete noOutput.output;
  assert.equal(codeOf(() => registry.register(noInput, () => 1)), 'descriptor_schema_missing');
  assert.equal(codeOf(() => registry.register(noOutput, () => 1)), 'descriptor_schema_missing');
});

test('a structurally unusable descriptor is refused', () => {
  const registry = registryModule.create();
  const noId = descriptor({}); delete noId.id;
  const noKey = descriptor({ id: 'x' }); delete noKey.handlerKey;
  const noAvail = descriptor({ id: 'y' }); delete noAvail.availability;
  assert.equal(codeOf(() => registry.register(null, () => 1)), 'descriptor_invalid');
  assert.equal(codeOf(() => registry.register(noId, () => 1)), 'descriptor_invalid');
  assert.equal(codeOf(() => registry.register(noKey, () => 1)), 'descriptor_invalid');
  assert.equal(codeOf(() => registry.register(noAvail, () => 1)), 'descriptor_invalid');
});

test('a non-function handler is refused, and a missing one is caught at dispatch', () => {
  const registry = registryModule.create();
  assert.equal(codeOf(() => registry.register(descriptor({ id: 'bad' }), 'not-a-function')), 'handler_invalid');
  // A descriptor may register with NO handler while it declares itself
  // unavailable — that is how a not-yet-implemented capability is honest.
  registry.register(descriptor({ id: 'declared', handlerKey: 'application.declared.thing' }));
  assert.equal(codeOf(() => registry.handlerFor('declared')), 'handler_missing');
  assert.equal(codeOf(() => registry.handlerFor('never-registered')), 'unknown_capability');
});

// ---- the P04/#1086 blocker, as an executable check -----------------------

test('claiming availability "available" with no handler is refused', () => {
  const registry = registryModule.create();
  assert.equal(
    codeOf(() => registry.register(descriptor({ id: 'claims', availability: { state: 'available', reason: null } }))),
    'availability_unbacked');
  // The same descriptor is accepted the moment a handler actually backs it.
  assert.equal(
    registry.register(descriptor({ id: 'claims', availability: { state: 'available', reason: null } }), () => 1),
    'claims');
});

test('the real export-job descriptor is honest, and flipping it to "available" is now caught', () => {
  // export-job.json declares `unavailable` because P17-P19 are unbuilt. That
  // registers cleanly with no handler.
  const registry = registryModule.create();
  const exportJob = realDescriptor('export-job');
  assert.equal(exportJob.availability.state, 'unavailable');
  assert.equal(registry.register(exportJob), 'export.svg.frame');

  // THE regression this leaf closes. Reviewing P04 I mutated this exact field
  // back to "available" and every schema test still passed 9/9, because it is
  // a syntactically valid enum value. Registration now rejects it.
  const lying = realDescriptor('export-job');
  lying.availability = { state: 'available', reason: null };
  assert.equal(codeOf(() => registryModule.create().register(lying)), 'availability_unbacked');
});

test('the real opacity descriptor registers with its handler', () => {
  const registry = registryModule.create();
  const opacity = realDescriptor('opacity');
  assert.equal(opacity.availability.state, 'available');
  assert.equal(registry.register(opacity, () => ({ ok: true })), 'opacity');
  assert.equal(typeof registry.handlerFor('opacity'), 'function');
  assert.equal(registry.get('opacity').handlerKey, 'application.opacity.property');
});

// ---- outcome check 2: deterministic discovery ---------------------------

test('discovery does not depend on registration order', () => {
  const specs = [
    [descriptor({ id: 'zulu', handlerKey: 'application.z.one' }), () => 1],
    [descriptor({ id: 'alpha', handlerKey: 'application.a.one' }), () => 2],
    [descriptor({ id: 'mike', handlerKey: 'application.m.one' }), () => 3],
  ];
  const forward = registryModule.create();
  specs.forEach(([d, h]) => forward.register(d, h));
  const reverse = registryModule.create();
  specs.slice().reverse().forEach(([d, h]) => reverse.register(d, h));

  assert.deepEqual(forward.ids(), ['alpha', 'mike', 'zulu']);
  assert.deepEqual(forward.ids(), reverse.ids());
  // Byte-identical discovery output, not merely the same set.
  assert.equal(JSON.stringify(forward.list()), JSON.stringify(reverse.list()));
});

test('the registry holds the feature\'s own handler rather than wrapping it', () => {
  const registry = registryModule.create();
  const handler = function featureOwned() { return 'from the feature'; };
  registry.register(descriptor({ id: 'owned' }), handler);
  assert.equal(registry.handlerFor('owned'), handler, 'handler identity was not preserved');
  assert.equal(registry.handlerFor('owned')(), 'from the feature');
});

test('a handlerKey cannot be replaced by another function and rejection is atomic', () => {
  const registry = registryModule.create();
  const first = () => 'first';
  const replacement = () => 'replacement';
  registry.register(descriptor({ id: 'alpha', handlerKey: 'application.shared.property' }), first);

  assert.equal(codeOf(() => registry.register(
    descriptor({ id: 'beta', handlerKey: 'application.shared.property' }), replacement)),
  'handler_key_conflict');
  assert.deepEqual(registry.ids(), ['alpha'], 'rejected registration leaked an id');
  assert.equal(registry.handlerFor('alpha'), first, 'rejected registration replaced the live handler');

  // Multiple descriptors may deliberately share one feature-owned handler.
  assert.equal(registry.register(
    descriptor({ id: 'gamma', handlerKey: 'application.shared.property' }), first), 'gamma');
  assert.equal(registry.handlerFor('gamma'), first);
});

test('binding a previously declared handlerKey updates discovery for every sharing descriptor', () => {
  const registry = registryModule.create();
  const shared = 'application.deferred.property';
  registry.register(descriptor({ id: 'declared', handlerKey: shared }));
  assert.equal(registry.list()[0].bound, false);

  const handler = () => 'ready';
  registry.register(descriptor({ id: 'provider', handlerKey: shared }), handler);
  assert.equal(registry.handlerFor('declared'), handler);
  assert.deepEqual(registry.list().map(entry => entry.bound), [true, true]);
});

// ---- outcome check 3: opaque data travels as a handle -------------------

test('raw image/geometry data inlined in a payload is refused', () => {
  const registry = registryModule.create();
  assert.equal(codeOf(() => registry.assertNoInlineOpaque({ imageData: 'data:image/png;base64,iVBORw0KG' })),
    'opaque_inline');
  assert.equal(codeOf(() => registry.assertNoInlineOpaque({ vertices: [0, 1, 2, 3] })), 'opaque_inline');
  // Size is not the test: a tiny data: URI is refused too, so the rule cannot
  // silently start accepting the same mistake below a threshold.
  assert.equal(codeOf(() => registry.assertNoInlineOpaque({ pixels: 'data:,' })), 'opaque_inline');
  // An ordinary payload, and a handle reference, both pass.
  const clean = { layerId: 'ly_1', value: 50 };
  assert.equal(registry.assertNoInlineOpaque(clean), clean);
  assert.equal(codeOf(() => registry.assertNoInlineOpaque({ handleId: 'handle-1' })), null);
});

test('a handle is acquired, resolved, and stops resolving once released', () => {
  const registry = registryModule.create();
  const acquired = registry.acquireHandle('req-42', { width: 2, height: 2 });
  assert.equal(acquired.disposition, 'held');
  assert.equal(acquired.ownerId, 'req-42');
  assert.deepEqual(registry.resolveHandle(acquired.handleId), { width: 2, height: 2 });

  const released = registry.releaseHandle(acquired.handleId);
  assert.equal(released.disposition, 'released');
  assert.equal('ownerId' in released, false, 'a released handle still names an owner');
  assert.equal(codeOf(() => registry.resolveHandle(acquired.handleId)), 'invalid_request');
  assert.equal(codeOf(() => registry.releaseHandle('handle-nope')), 'invalid_request');
  assert.equal(codeOf(() => registry.acquireHandle('', {})), 'invalid_request');
});

test('handles use D02\'s ResourceHandle vocabulary, not a parallel one', () => {
  const contract = JSON.parse(fs.readFileSync(
    path.join(ROOT, 'engineering', 'application', 'operation-contract-v1.schema.json'), 'utf8'));
  const shape = contract.$defs.ResourceHandle;
  const registry = registryModule.create();
  const held = registry.acquireHandle('req-1', {});
  const releasedHandle = registry.releaseHandle(held.handleId);

  for (const value of [held, releasedHandle]) {
    for (const key of Object.keys(value)) {
      assert.ok(Object.prototype.hasOwnProperty.call(shape.properties, key),
        `"${key}" is not a field D02's ResourceHandle declares`);
    }
    for (const required of shape.required) {
      assert.ok(required in value, `missing D02-required field "${required}"`);
    }
    assert.ok(shape.properties.disposition.enum.includes(value.disposition));
  }
});

// ---- isolation ----------------------------------------------------------

test('two registries do not share state', () => {
  const a = registryModule.create();
  const b = registryModule.create();
  a.register(descriptor({ id: 'only-in-a' }), () => 1);
  assert.deepEqual(a.ids(), ['only-in-a']);
  assert.deepEqual(b.ids(), []);
});
