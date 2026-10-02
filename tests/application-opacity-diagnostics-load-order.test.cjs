'use strict';

// T13 -- guard the NemoOpacityDiagnostics browser load-order invariant.
//
// In `src/index.html` the consumer `js/application/opacity-application.js`
// loads BEFORE its provider `js/domain/diagnostics/opacity-diagnostics.js`,
// and the only caller of `NemoOpacityApplicationCore.create()` --
// `js/bootstrap/opacity-application.js` -- loads after both. That order only
// works because of two facts that nothing else in the suite observes:
//
//   1. both files declare a TOP-LEVEL `var NemoOpacityDiagnostics`, so in a
//      classic script they share one global binding: the consumer's eager
//      read at load time stores `undefined` into that same global, and the
//      provider's later assignment overwrites it;
//   2. the consumer only dereferences the module LAZILY, inside `create()`,
//      so by the time the bootstrap calls it the real module is in place.
//
// Every existing opacity test reaches these modules through `require()`,
// which takes the `typeof require === 'function'` branch on the consumer's
// line 11 and resolves the provider directly. A Node test therefore CANNOT
// observe a break in the browser order. This test loads the real sources,
// in their real `src/index.html` order, into one shared vm context with no
// `require` available -- the browser's actual execution model -- and then
// performs the first `create()` at the bootstrap's position.
//
// This file adds no product code and touches no shared generated artifact.

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const SRC = path.join(__dirname, '..', 'src');
const PROVIDER = 'js/domain/diagnostics/opacity-diagnostics.js';
const CONSUMER = 'js/application/opacity-application.js';
const BOOTSTRAP = 'js/bootstrap/opacity-application.js';

function read(relative) {
  return fs.readFileSync(path.join(SRC, relative), 'utf8');
}

// The ordered list of classic-script sources `index.html` executes. Derived
// from the real document rather than restated, so a moved tag is observed
// here instead of silently invalidating the premise of this test.
function scriptOrder() {
  const html = read('index.html');
  return [...html.matchAll(/<script\s+src="([^"]+)"/g)].map(match => match[1]);
}

// One shared global object, each source run in it in turn, and -- the point
// of the exercise -- no `require` and no `module`, so the consumer takes its
// browser branch.
function runInBrowserOrder(sources) {
  const context = vm.createContext({});
  vm.runInContext('var window = this;', context);
  assert.strictEqual(vm.runInContext('typeof require', context), 'undefined',
    'the harness must not expose require, or the consumer takes its Node branch');
  assert.strictEqual(vm.runInContext('typeof module', context), 'undefined',
    'the harness must not expose module');
  for (const [name, source] of sources) {
    vm.runInContext(source, context, { filename: `index.html:${name}` });
  }
  return context;
}

// Stands in for `bootstrap/opacity-application.js`: the smallest set of ports
// that lets one real `property.set` write through, followed by a real
// `diagnostics.trace` read. Nothing here supplies a diagnostics module -- the
// core must resolve `NemoOpacityDiagnostics` itself, which is the invariant
// under test.
const BOOTSTRAP_PROBE = `(function () {
  var ids = 0;
  var state = {
    layers: [{ layerUid: 'layer-1', name: 'L1', motion: null, motionStatic: null }],
    currentFrame: 0, totalFrames: 24
  };
  var app = NemoOpacityApplicationCore.create({
    newId: function () { return 'id-' + (++ids); },
    state: function () { return state; },
    valueAtFrame: function () { return [42]; },
    write: function () { return true; },
    context: function () { return 'ctx'; },
    canMutate: function () { return true; },
    snapshot: function () { return { layers: [], frame: 0, totalFrames: 24 }; },
    history: { checkpoint: function () {}, undo: function () { return false; }, redo: function () { return false; } },
    afterMutation: function () {},
    capabilities: function () { return []; }
  });
  var meta = app.meta();
  var wrote = app.handle({
    apiVersion: 1, requestId: 'req-1', instanceId: meta.instanceId, documentId: meta.documentId,
    expectedRevision: meta.revision, operation: 'property.set',
    payload: { property: 'opacity', layerId: 'layer-1', value: 7 }
  });
  var traced = app.handle({
    apiVersion: 1, requestId: 'req-2', operation: 'diagnostics.trace', payload: {}
  });
  var replay = app.handle({
    apiVersion: 1, requestId: 'req-3', instanceId: app.meta().instanceId,
    documentId: app.meta().documentId, expectedRevision: app.meta().revision,
    operation: 'diagnostics.replay',
    payload: { request: { operation: 'property.set', payload: { property: 'opacity', layerId: 'layer-1', value: 9 } } }
  });
  return {
    wroteOk: wrote.ok,
    tracedOk: traced.ok,
    entries: traced.ok ? traced.result.entries : null,
    traceRetention: app.handle({ apiVersion: 1, requestId: 'req-4', operation: 'capabilities', payload: {} }).result.traceRetention,
    replayOk: replay.ok
  };
})()`;

function firstCreateAtBootstrapPosition(sources) {
  return vm.runInContext(BOOTSTRAP_PROBE, runInBrowserOrder(sources));
}

const REAL_ORDER = () => [[CONSUMER, read(CONSUMER)], [PROVIDER, read(PROVIDER)]];

// --- (a) the invariant holds on the real sources, in the real order ---------

test('index.html loads the consumer before its provider, and the bootstrap after both', () => {
  const order = scriptOrder();
  const consumer = order.indexOf(CONSUMER);
  const provider = order.indexOf(PROVIDER);
  const bootstrap = order.indexOf(BOOTSTRAP);
  assert.ok(consumer >= 0 && provider >= 0 && bootstrap >= 0,
    `all three tags must exist in index.html (got ${consumer}, ${provider}, ${bootstrap})`);
  // This is the hazard, stated as an assertion: the consumer really does run
  // before the module it names. If someone repairs the order, this test is
  // the place that says so out loud.
  assert.ok(consumer < provider,
    'premise: the consumer tag precedes the provider tag');
  assert.ok(provider < bootstrap,
    'the provider must run before the bootstrap that calls create()');
});

test('the bootstrap is the only script that calls NemoOpacityApplicationCore.create', () => {
  const callers = scriptOrder().filter(relative => {
    const file = path.join(SRC, relative);
    if (!fs.existsSync(file)) return false;
    return /NemoOpacityApplicationCore\s*\.\s*create\s*\(/.test(fs.readFileSync(file, 'utf8'));
  });
  assert.deepStrictEqual(callers, [BOOTSTRAP],
    'if another script gains a create() call, its position must be guarded too');
});

// The declaration FORM is guarded statically rather than by execution, for a
// measured reason: Node's vm realm and Chrome disagree here. In this realm a
// cross-script `let X` after a `var X` is accepted and silently shadows the
// global property (verified: `var X=1` then `let X=2` runs, and `typeof X`
// becomes the let binding), while Chrome rejects the same pair with
// "Identifier has already been declared". Only the reverse pair -- `let`
// first, then `var` -- throws in both. So a runtime assertion would catch a
// provider-side `let` by accident in one engine and not at all in the other.
// The source assertion catches it in every engine, for the real reason: the
// shared binding only exists because both declarations are `var`.
function assertDeclarationForm(label, source) {
  assert.match(source, /^var NemoOpacityDiagnostics\b/m,
    `the ${label} must declare NemoOpacityDiagnostics with a top-level var; `
    + 'a let or const binding is lexical, so the two files would no longer share one binding');
}

test('both declarations are top-level `var`, which is what shares the binding', () => {
  assertDeclarationForm('provider', read(PROVIDER));
  assertDeclarationForm('consumer', read(CONSUMER));
});

test('the first create(), at the bootstrap position, gets the real module', () => {
  const probe = firstCreateAtBootstrapPosition(REAL_ORDER());
  assert.strictEqual(probe.wroteOk, true, 'the probe write must succeed');
  assert.strictEqual(probe.tracedOk, true, 'diagnostics.trace must succeed');
  // A stub or a fallback would not have recorded the write.
  assert.strictEqual(probe.entries.length, 1, 'the real ring buffer recorded the one write');
  assert.strictEqual(probe.entries[0].request.operation, 'property.set');
  assert.strictEqual(probe.entries[0].ok, true);
  assert.strictEqual(probe.traceRetention, 32, 'the real module LIMIT reached the capability summary');
  assert.strictEqual(probe.replayOk, true, 'the real prepareReplay shaped a replayable envelope');
});

test('the consumer resolves the provider lazily, not at load time', () => {
  // Running the consumer alone must not throw, even though the global it
  // reads is still undefined -- that is what makes the inverted order
  // survivable at all.
  const context = runInBrowserOrder([[CONSUMER, read(CONSUMER)]]);
  assert.strictEqual(vm.runInContext('typeof NemoOpacityDiagnostics', context), 'undefined');
  assert.strictEqual(vm.runInContext('typeof NemoOpacityApplicationCore.create', context), 'function');
});

// --- (b) each mutation of the invariant must be caught --------------------

// Declaration-form mutations: caught by the static guard above, in whichever
// of the two files is rewritten.
for (const [label, file] of [['provider', PROVIDER], ['consumer', CONSUMER]]) {
  for (const keyword of ['let', 'const']) {
    test(`the guard fails when the ${label} declaration becomes \`${keyword}\``, () => {
      const mutated = read(file).replace(/^var NemoOpacityDiagnostics\b/m, `${keyword} NemoOpacityDiagnostics`);
      assert.ok(mutated.includes(`${keyword} NemoOpacityDiagnostics`) && mutated !== read(file),
        'the declaration-form mutation must actually have applied');
      assert.throws(() => assertDeclarationForm(label, mutated), assert.AssertionError);
    });
  }
}

// Order and resolution-timing mutations: caught by actually executing the
// real sources, where Node's realm is faithful to the browser.
const MUTATIONS = [
  {
    name: 'the provider tag is moved after the bootstrap tag',
    // Modelled as it actually presents: create() runs with the provider
    // not yet executed.
    sources: () => [[CONSUMER, read(CONSUMER)]]
  },
  {
    name: 'the consumer captures the module eagerly at load time',
    // The consumer's line 11 already READS the global eagerly; it survives
    // only because the value lands back in the same shared `var`. Capturing
    // it into a module-private alias instead -- the ordinary refactor that
    // would look harmless -- freezes `undefined` in place.
    sources: () => {
      const eager = read(CONSUMER)
        .replace("var NemoOpacityApplicationCore = (function () {\n  'use strict';",
          "var NemoOpacityApplicationCore = (function () {\n  'use strict';\n  var EAGER_DIAGNOSTICS = NemoOpacityDiagnostics;")
        .replace('NemoOpacityDiagnostics.create(PROPERTY_WRITES)', 'EAGER_DIAGNOSTICS.create(PROPERTY_WRITES)');
      assert.ok(eager.includes('EAGER_DIAGNOSTICS.create(PROPERTY_WRITES)')
        && eager.includes('var EAGER_DIAGNOSTICS = NemoOpacityDiagnostics;'),
        'the eager-capture mutation must actually have applied');
      return [[CONSUMER, eager], [PROVIDER, read(PROVIDER)]];
    }
  }
];

for (const mutation of MUTATIONS) {
  test(`the guard fails when ${mutation.name}`, () => {
    assert.throws(() => firstCreateAtBootstrapPosition(mutation.sources()),
      'the first create() must not be able to reach a real diagnostics module');
  });
}
