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
//
// It matches 172 of the 174 `src=` tags; the two it misses are the wasm
// loaders, which are `type="module"` and so correctly absent from a
// classic-script list. Note WHY they are absent: their `type=` happens to
// precede their `src=`, not because this pattern filters on type. A tag
// written `<script src="x.js" type="module">` would be wrongly included, and
// a module script calling create() would evade the only-caller test below.
// Zero such tags today (also checked: zero commented-out `<script src=>`).
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

// The declaration form is NOT guarded by executing the two scripts as a pair,
// for a measured reason: Node's vm realm and Chrome disagree there. In this
// realm a cross-script `let X` after a `var X` is accepted and silently
// shadows the global property (verified: `var X=1` then `let X=2` runs, and
// `typeof X` becomes the let binding), while Chrome rejects the same pair with
// "Identifier has already been declared" -- in BOTH directions, which is the
// spec-correct side (GlobalDeclarationInstantiation throws when a lexical name
// collides with an existing var-declared global). So an assertion that runs
// the mutated pair is engine-dependent.
//
// It is also not guarded by a `/^var /` source match, which would be a false
// positive waiting to happen: what actually shares the binding here is the
// GLOBAL OBJECT -- the consumer's browser branch literally reads
// `window.NemoOpacityDiagnostics` -- so publishing the provider as
// `window.NemoOpacityDiagnostics = (function () {...})()` keeps the invariant
// intact while failing any var-shaped regex. That is not hypothetical: nine
// files in src/js already publish their global that way.
//
// So the assertion runs each file ALONE in a fresh realm and asks the only
// question that matters: did this source publish the name as an own property
// of the global object? That never touches redeclaration semantics, so the
// engine divergence cannot arise. Measured identical in Node 26.9 and real
// Chrome 154 across the eight declaration forms below.
function publishesGlobalBinding(label, source) {
  const context = vm.createContext({});
  vm.runInContext('var window = this;', context);
  vm.runInContext(source, context, { filename: `index.html:${label}` });
  return vm.runInContext(
    "Object.prototype.hasOwnProperty.call(this, 'NemoOpacityDiagnostics')", context);
}

function assertSharedGlobalBinding(label, source) {
  assert.ok(publishesGlobalBinding(label, source),
    `the ${label} must publish NemoOpacityDiagnostics as an own property of the `
    + 'global object -- a top-level `var`, or an explicit `window.` assignment. A '
    + '`let` or `const` binding is lexical, so the two files would no longer share '
    + 'one binding and the inverted load order would stop working.');
}

test('both files publish the global property, which is what shares the binding', () => {
  assertSharedGlobalBinding('provider', read(PROVIDER));
  assertSharedGlobalBinding('consumer', read(CONSUMER));
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

// Declaration-form mutations: caught by the own-property probe above, in
// whichever of the two files is rewritten.
for (const [label, file] of [['provider', PROVIDER], ['consumer', CONSUMER]]) {
  for (const keyword of ['let', 'const']) {
    test(`the guard fails when the ${label} declaration becomes \`${keyword}\``, () => {
      const mutated = read(file).replace(/^var NemoOpacityDiagnostics\b/m, `${keyword} NemoOpacityDiagnostics`);
      assert.ok(mutated.includes(`${keyword} NemoOpacityDiagnostics`) && mutated !== read(file),
        'the declaration-form mutation must actually have applied');
      assert.throws(() => assertSharedGlobalBinding(label, mutated), assert.AssertionError);
    });
  }
}

// The other half of the probe, and the reason it replaced a source match: the
// `window.X =` publication form -- already used by nine other src/js files,
// and the exact form the consumer READS -- must stay ACCEPTED. For the
// provider that is proved end to end, not just by the probe: the real
// bootstrap create() still reaches the real module through it.
for (const [label, file] of [['provider', PROVIDER], ['consumer', CONSUMER]]) {
  test(`the guard accepts a \`window.\` publication in the ${label}`, () => {
    const viaWindow = read(file).replace(/^var NemoOpacityDiagnostics\b/m, 'window.NemoOpacityDiagnostics');
    assert.ok(viaWindow.includes('window.NemoOpacityDiagnostics =') && viaWindow !== read(file),
      'the window-publication mutation must actually have applied');
    assertSharedGlobalBinding(label, viaWindow);
    if (file !== PROVIDER) return;
    const probe = firstCreateAtBootstrapPosition([[CONSUMER, read(CONSUMER)], [PROVIDER, viaWindow]]);
    assert.strictEqual(probe.wroteOk, true, 'the invariant still holds through the window form');
    assert.strictEqual(probe.entries.length, 1, 'the real ring buffer still recorded the write');
    assert.strictEqual(probe.traceRetention, 32, 'the real module LIMIT still reached capabilities');
  });
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
