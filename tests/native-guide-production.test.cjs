'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { fixture } = require('./helpers/native-guide-production.cjs');

test('ordinary ruler cancellation retains absent guides at R0 through actual Project readiness and native Reset', async () => {
  const f = fixture();
  try {
    await f.prepare();
    const controller = f.root.NemoNativeGuideController, origin = controller.read();
    assert(origin, 'Canonical Project notifications must publish the native Guide projection');
    assert.equal(origin.guides.settings.guidesPresent, false);
    assert.equal(origin.guides.settings.hPresent, false);
    assert.equal(origin.guides.settings.vPresent, false);
    assert.deepEqual(Array.from(origin.guides.h), []);
    assert.deepEqual(Array.from(origin.guides.v), []);
    assert.equal(f.session.info().contentRevision, 0);
    assert.equal(f.root.NemoNativeObjectProjectFile.projectionReadyForUI(f.root, f.session.info()), true);
    const proof = controller.coordinates();
    assert(proof && proof.current(), 'Painted native ruler proof must remain current at ordinary pointer admission');
    const before = f.trace.length;
    const down = f.dispatch(f.h, 'pointerdown', { clientX: 466, clientY: 111 });
    assert.equal(down.defaultPrevented, true);
    assert.equal(f.ghosts().length, 1, 'The actual ruler callback must admit one transient guide gesture');
    f.dispatch(f.h, 'pointermove', { clientX: 516, clientY: 111 });
    f.dispatch(f.h, 'pointerup', { clientX: 566, clientY: 111 });
    await f.settle();
    assert.deepEqual(f.notices, [], 'A back-to-ruler cancellation must not report synchronization refusal');
    assert.equal(f.ghosts().length, 0);
    assert.equal(f.edits().length, 0);
    assert.equal(f.session.info().contentRevision, 0);
    assert.equal(f.saved(), f.originalDocument, 'Cancellation must not materialize absent guide metadata');
    assert.strictEqual(controller.read(), origin);
    assert.equal(proof.current(), true);
    assert(f.trace.slice(before).some(record => record.source === 'js/rulers-bridge.js' && record.phase === 1));
    assert.equal(f.trace.slice(before).some(record => record.source === 'js/draw-bridge.js'), false,
      'Document-capture Guide handling must precede the actual default Draw canvas-area listener');
  } finally { await f.dispose(); }
});

test('ordinary ruler entry refuses while the actual Project resource queue hides presentation', async () => {
  const f = fixture(); let release;
  try {
    await f.prepare();
    let entered;
    const started = new Promise(resolve => { entered = resolve; });
    const held = new Promise(resolve => { release = resolve; });
    const pending = f.session.runResourceCommand(async context => {
      assert(context.readPresentedObjectSession()); entered(); await held;
    });
    await started;
    assert.equal(f.session.readPresentedObjectSession(), null);
    assert.equal(f.root.NemoNativeGuideController.coordinates(), null);
    f.dispatch(f.h, 'pointerdown', { clientX: 466, clientY: 111 });
    assert.equal(f.ghosts().length, 0);
    assert.equal(f.notices.length, 1);
    release(); await pending; await f.settle();
    assert.equal(f.edits().length, 0);
    assert.equal(f.session.info().contentRevision, 0);
    assert.equal(f.saved(), f.originalDocument);
  } finally { if (release) release(); await f.dispose(); }
});

for (const change of ['presentation', 'owner']) {
  test('ordinary ruler gesture retires after a real Project ' + change + ' replacement', async () => {
    const f = fixture();
    try {
      await f.prepare();
      const prior = f.root.NemoNativeGuideController.coordinates(); assert(prior && prior.current());
      f.dispatch(f.h, 'pointerdown', { clientX: 466, clientY: 111 });
      assert.equal(f.ghosts().length, 1);
      if (change === 'presentation') await f.session.renderPreview(0);
      else await f.open();
      await f.settle(); assert.equal(prior.current(), false);
      f.dispatch(f.h, 'pointermove', { clientX: 516, clientY: 111 });
      f.dispatch(f.h, 'pointerup', { clientX: 566, clientY: 111 });
      await f.settle();
      assert.equal(f.ghosts().length, 0);
      assert.equal(f.edits().length, 0);
      assert.equal(f.session.info().contentRevision, 0);
      assert.equal(f.saved(), f.originalDocument);
    } finally { await f.dispose(); }
  });
}

test('ordinary ruler entry refuses during the actual registered same-bounds window resize preview', async () => {
  const f = fixture(); let held;
  try {
    await f.prepare();
    assert(f.root.NemoNativeGuideController.coordinates().current());
    held = f.holdPreview();
    const before = f.calls.filter(call => call.command === 'nemo_native_object_preview').length;
    f.dispatch(f.root, 'resize');
    await held.started;
    assert.equal(f.calls.filter(call => call.command === 'nemo_native_object_preview').length, before + 1,
      'The actual window callback, not a modeled competing task, must request this preview');
    assert.equal(f.session.readPresentedObjectSession(), null);
    assert.equal(f.root.NemoNativeGuideController.coordinates(), null);
    f.dispatch(f.h, 'pointerdown', { clientX: 466, clientY: 111 });
    assert.equal(f.notices.length, 1);
    assert.equal(f.ghosts().length, 0);
    held.release(); await f.settle();
    assert(f.root.NemoNativeGuideController.coordinates().current());
    assert.equal(f.edits().length, 0);
    assert.equal(f.session.info().contentRevision, 0);
    assert.equal(f.saved(), f.originalDocument);
  } finally { if (held) held.release(); await f.dispose(); }
});

test('actual Composition and Guide fanout conserves sealed Guide projection after busy admission refuses', async () => {
  const f = fixture({ composition: true }); let release, pending;
  try {
    await f.prepare();
    const guide = f.root.NemoNativeGuideController, composition = f.root.NemoNativeCompositionController;
    const original = guide.read(), originalComposition = composition.read();
    assert(original && originalComposition);
    assert(guide.coordinates().current(), 'A valid painted native Guide projection must precede contention');
    assert.equal(original.guides.settings.guidesPresent, false);
    assert.equal(originalComposition.composition.effectiveBpm, 120);
    const reads = () => f.calls.filter(call => call.request && call.request.operation === 'query.document.object.project.guide.read').length;
    const before = reads();
    let entered;
    const started = new Promise(resolve => { entered = resolve; });
    const held = new Promise(resolve => { release = resolve; });
    pending = f.session.runResourceCommand(async context => {
      assert(context.readPresentedObjectSession()); entered(); await held;
    });
    await started;
    assert.equal(guide.read(), null);
    assert.equal(composition.read(), null);
    await f.notifyWhilePending();
    assert.equal(guide.read(), null, 'Cache conservation must not bypass genuine busy presentation refusal');
    release(); await pending; await f.settle();
    assert.strictEqual(guide.read(), original, 'Rejected refresh admission must retain the sealed same-pin Guide projection');
    assert.equal(composition.read().composition.effectiveBpm, 120);
    assert.equal(reads(), before, 'No metadata changed, so quiet recovery must not requery Guide or manually warm its cache');
    assert(guide.coordinates().current());
    assert.deepEqual(f.notices, []);
    assert.equal(f.edits().length, 0);
    assert.equal(f.session.info().contentRevision, 0);
    assert.equal(f.saved(), f.originalDocument);
  } finally { if (release) release(); if (pending) await pending; await f.dispose(); }
});
