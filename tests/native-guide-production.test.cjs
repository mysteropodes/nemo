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

test('Guide observations stay silent for presentation reads and ordinary no-op cancellation', async () => {
  const f = fixture();
  try {
    await f.prepare(); f.diagnostics.length = 0;
    const controller = f.root.NemoNativeGuideController, origin = controller.read();
    const proof = controller.coordinates(), calls = f.calls.length;
    assert(proof.current()); assert(proof.project({ x: 0, y: 20 }));
    assert(proof.unproject({ clientX: 466, clientY: 222 })); assert(proof.segment('h', 20));
    f.root.SMRulers.render(); f.tick();
    f.dispatch(f.h, 'pointerdown'); f.dispatch(f.h, 'pointerup');
    assert.strictEqual(controller.read(), origin);
    assert.deepEqual(f.diagnostics, []); assert.deepEqual(f.notices, []);
    assert.equal(f.calls.length, calls); assert.equal(f.edits().length, 0);
    assert.equal(f.saved(), f.originalDocument);
  } finally { await f.dispose(); }
});

for (const stage of ['entry', 'release']) {
  test('Guide ' + stage + ' observes the first existing busy refusal after the original toast', async () => {
    const f = fixture(); let release, pending;
    try {
      await f.prepare(); f.diagnostics.length = 0;
      const origin = f.root.NemoNativeGuideController.read(), calls = f.calls.length;
      const order = [], toast = f.root.showToast, observer = f.root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__;
      f.root.showToast = value => { order.push('toast'); toast(value); };
      f.root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__ = (event, code) => {
        order.push('diagnostic'); observer(event, code);
      };
      if (stage === 'release') f.dispatch(f.h, 'pointerdown');
      let entered;
      const started = new Promise(resolve => { entered = resolve; });
      const held = new Promise(resolve => { release = resolve; });
      pending = f.session.runResourceCommand(async context => {
        assert(context.readPresentedObjectSession()); entered(); await held;
      });
      await started;
      f.dispatch(f.h, stage === 'entry' ? 'pointerdown' : 'pointerup');
      assert.deepEqual(order, ['toast', 'diagnostic']);
      assert.deepEqual(f.diagnostics, [['guide-' + stage + '-refused', 'presentation-missing']]);
      assert.equal(f.notices.length, 1); assert.equal(f.ghosts().length, 0);
      assert.equal(f.calls.length, calls); assert.equal(f.edits().length, 0);
      assert.equal(f.root.NemoNativeGuideController.read(), null);
      release(); await pending; await f.settle();
      assert.strictEqual(f.root.NemoNativeGuideController.read(), origin);
      assert.deepEqual(f.diagnostics, [['guide-' + stage + '-refused', 'presentation-missing'], ['camera-presented', 'canvas']]);
      assert.equal(f.saved(), f.originalDocument);
    } finally { if (release) release(); if (pending) await pending; await f.dispose(); }
  });
}

for (const failure of ['projection-unready', 'frame-mismatch', 'exception']) {
  test('Guide operation records only the fixed ' + failure + ' predicate without a native edit', async () => {
    const f = fixture(); let restore = () => {};
    try {
      await f.prepare(); f.diagnostics.length = 0;
      const calls = f.calls.length;
      if (failure === 'frame-mismatch') {
        f.dispatch(f.h, 'pointerdown'); f.root.state.currentFrame = 1;
        restore = () => { f.root.state.currentFrame = 0; };
      } else if (failure === 'projection-unready') {
        const width = f.root.state.canvasW; f.root.state.canvasW++;
        restore = () => { f.root.state.canvasW = width; };
      } else {
        const bounds = f.area.getBoundingClientRect;
        f.area.getBoundingClientRect = () => { throw Error('/PRIVATE/path: document-id and coordinates'); };
        restore = () => { f.area.getBoundingClientRect = bounds; };
      }
      f.dispatch(f.h, failure === 'frame-mismatch' ? 'pointerup' : 'pointerdown');
      const event = failure === 'frame-mismatch' ? 'guide-release-refused' : 'guide-entry-refused';
      assert.deepEqual(f.diagnostics, [[event, failure]]);
      assert.equal(f.notices.length, 1); assert.equal(f.calls.length, calls);
      assert.equal(f.ghosts().length, 0); assert.equal(f.edits().length, 0);
      assert.equal(f.session.info().contentRevision, 0); assert.equal(f.saved(), f.originalDocument);
      restore(); assert(f.root.NemoNativeGuideController.coordinates().current());
    } finally { restore(); await f.dispose(); }
  });
}

test('Guide diagnostic sinks cannot change synchronous refusal or authorize a stale edit', async () => {
  const f = fixture();
  try {
    await f.prepare(); f.diagnostics.length = 0;
    const desktop = f.root.NemoNativeObjectProjectDesktop, calls = f.calls.length;
    const width = f.root.state.canvasW; f.root.state.canvasW++;
    try {
      for (const sink of [() => { throw Error('PRIVATE observer failure'); }, () => new Promise(() => {})]) {
        f.root.NemoNativeObjectProjectDesktop = { ...desktop, openDiagnostic: sink };
        f.dispatch(f.h, 'pointerdown');
        assert.equal(f.ghosts().length, 0);
      }
    } finally { f.root.NemoNativeObjectProjectDesktop = desktop; f.root.state.canvasW = width; }
    assert.equal(f.notices.length, 2); assert.deepEqual(f.diagnostics, []);
    const result = await f.root.NemoNativeGuideController.edit({ kind: 'add', axis: 'h', position: 20 }, null);
    assert.equal(result.status, 'unavailable');
    assert.deepEqual(f.diagnostics, [['guide-edit-refused', 'origin-missing']]);
    assert.equal(f.notices.length, 3); assert.equal(f.calls.length, calls);
    assert.equal(f.edits().length, 0); assert.equal(f.saved(), f.originalDocument);
  } finally { await f.dispose(); }
});

test('the existing diagnostic collector bounds and sanitizes Guide events before storage and its optional observer', async () => {
  const f = fixture(), writes = [];
  try {
    const desktop = f.root.NemoNativeObjectProjectDesktop, calls = f.calls.length;
    f.root.__TAURI__.path = { appDataDir: async () => '/controlled-app-data' };
    f.root.__TAURI__.fs = { async mkdir() {}, async writeTextFile(path, bytes) { writes.push(bytes); }, async rename() {} };
    const secret = '/PRIVATE/document-id?x=123&y=456';
    desktop.openDiagnostic(f.root, 'guide-entry-refused', secret);
    assert.deepEqual(f.diagnostics, [['guide-entry-refused', 'unknown']]);
    desktop.openDiagnostic(f.root, secret, 'cache-empty');
    assert.equal(f.diagnostics.length, 1);
    for (let i = 0; i < 40; i++) desktop.openDiagnostic(f.root, 'guide-release-refused', 'presentation-missing');
    desktop.openDiagnostic(f.root, 'guide-edit-refused', { code: secret });
    await new Promise(setImmediate);
    assert(writes.length > 0);
    const record = JSON.parse(writes.at(-1));
    assert.equal(record.events.length, 32);
    assert(record.events.every(event => Object.keys(event).join(',') === 'sequence,attempt,event,code'));
    assert.equal(record.events.at(-1).code, 'unknown');
    assert.equal(f.diagnostics.at(-1)[1], 'unknown');
    assert.equal(writes.some(bytes => bytes.includes(secret)), false);
    f.root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__ = () => { throw Error(secret); };
    assert.doesNotThrow(() => desktop.openDiagnostic(f.root, 'guide-entry-refused', 'cache-empty'));
    assert.equal(f.calls.length, calls); assert.deepEqual(f.notices, []);
  } finally { await f.dispose(); }
});

test('optional Desktop read observations preserve guard order, early-null cache retention and stale clearing', async () => {
  // Controlled native ports isolate getter counts; the ordinary pointer cases above use canonical Project ports.
  const Desktop = require('../src/js/adapters/native-guide-desktop.js');
  const trace = [], codes = [];
  const info = { instanceId: 'instance', documentId: 'document', lifecycleGeneration: 1,
    contentRevision: 0, documentSnapshotId: 'snapshot' };
  let ticket = 1, available = true, active = true, throwing = false, queries = 0;
  let shown = { ...info, presentation: { ...info, owner: 'native', status: 'presented' } };
  const session = {
    info() { trace.push('info'); return info; }, isActive() { trace.push('active'); return active; },
    readSubscriptionGeneration() { trace.push('ticket'); return ticket; },
    readPresentedObjectSession() { trace.push('shown'); return shown; }, requestId: () => 'read',
    async runResourceCommand(action) { return action({ readPresentedObjectSession: () => shown, async dispatch(request) {
      queries++;
      const selector = request.payload.selector;
      const value = selector.kind === 'settings' ? { guidesPresent: false, hPresent: false, vPresent: false, hCount: 0, vCount: 0 } :
        { kind: 'guideAxisPage', axis: selector.axis, guidesPresent: false, axisPresent: false,
          guideCount: 0, cursor: 0, items: [], nextCursor: null };
      return { apiVersion: 2, requestId: request.requestId, instanceId: info.instanceId, documentId: info.documentId,
        contentRevision: 0, ok: true, result: { instanceId: info.instanceId, documentId: info.documentId,
          atRevision: 0, documentSnapshotId: info.documentSnapshotId, contextId: 'scene-root', result: value } };
    } }); }
  };
  const model = Desktop.create({ currentSession() {
    trace.push('current'); if (throwing) throw Error('PRIVATE'); return session;
  }, available() { trace.push('available'); return available; } });
  const observe = code => { codes.push(code); };
  try {
    const value = await model.refresh(); trace.length = 0;
    assert.strictEqual(model.read(observe), value); assert.deepEqual(codes, []);
    const full = ['current', 'active', 'info', 'ticket', 'shown', 'info', 'available'];
    assert.deepEqual(trace, full);
    const prior = shown; shown = null; trace.length = 0;
    assert.equal(model.read(observe), null); assert.deepEqual(codes, ['presentation-missing']);
    assert.deepEqual(trace, full.slice(0, 5));
    shown = prior; trace.length = 0; assert.strictEqual(model.read(observe), value); assert.equal(queries, 3);
    available = false; trace.length = 0;
    assert.equal(model.read(() => { throw Error('PRIVATE'); }), null); assert.deepEqual(trace, full);
    assert.equal(model.read(() => new Promise(() => {})), null);
    available = true; assert.strictEqual(model.read(), value);
    ticket++; trace.length = 0; codes.length = 0;
    assert.equal(model.read(observe), null); assert.deepEqual(codes, ['subscription-stale']);
    assert.deepEqual(trace, full.slice(0, 4));
    ticket--; trace.length = 0; codes.length = 0;
    assert.equal(model.read(observe), null); assert.deepEqual(codes, ['cache-empty']); assert.deepEqual(trace, []);
    await model.refresh(); active = false; trace.length = 0; codes.length = 0;
    assert.equal(model.read(observe), null); assert.deepEqual(codes, ['inactive']); assert.deepEqual(trace, full.slice(0, 2));
    active = true; await model.refresh(); throwing = true; trace.length = 0; codes.length = 0;
    assert.equal(model.read(observe), null); assert.deepEqual(codes, ['exception']); assert.deepEqual(trace, ['current']);
  } finally { model.dispose(); }
});

test('real completion fanout converges after one refused Composition read while preserving the loaded Guide owner', async () => {
  const f = fixture({ composition: true }); let preview, refusal;
  try {
    await f.prepare(); f.diagnostics.length = 0;
    const guide = f.root.NemoNativeGuideController, composition = f.root.NemoNativeCompositionController;
    const origin = guide.read(), info = f.session.info(), ticket = f.session.readSubscriptionGeneration();
    assert(origin && composition.read()); assert(guide.coordinates().current());
    const queries = () => f.calls.filter(call => call.request &&
      call.request.operation === 'query.document.object.project.composition.read');
    const guideQueries = () => f.calls.filter(call => call.request &&
      call.request.operation === 'query.document.object.project.guide.read').length;
    const previews = () => f.calls.filter(call => call.command === 'nemo_native_object_preview').length;
    const before = queries().length, beforeGuide = guideQueries(), beforePreview = previews();
    preview = f.holdPreview(); f.dispatch(f.root, 'resize'); await preview.started;
    assert.equal(previews(), beforePreview + 1, 'The registered same-bounds window callback starts the real preview');
    assert.strictEqual(f.session.info(), info); assert.equal(f.session.readPresentedObjectSession(), null);
    // Explicit public read boundary: real readonly consumers such as SMBpm.cfg use this same controller.read.
    assert.equal(composition.read(), null);
    assert.equal(guide.read(), null, 'A loaded Guide cache still cannot authorize input during real pending work');
    refusal = f.refuseNextCompositionRead(); preview.release(); await refusal.started;
    assert.equal(queries().length, before + 1);
    assert.equal(queries().at(-1).request.payload.selector.kind, 'settings');
    assert.equal(f.session.readPresentedObjectSession(), null);
    f.dispatch(f.h, 'pointerdown');
    assert.deepEqual(f.diagnostics, [['camera-presented', 'canvas'], ['guide-entry-refused', 'presentation-missing']]);
    assert.equal(f.notices.length, 1); assert.equal(f.ghosts().length, 0); assert.equal(f.edits().length, 0);
    refusal.release(); await f.settle();
    assert.equal(queries().length, before + 1,
      'Unchanged read-only completion must not cause another unsolicited same-pin Composition read');
    assert.equal(previews(), beforePreview + 1, 'Metadata refusal must not manufacture another native presentation');
    assert.equal(composition.read(), null, 'Unavailable Composition metadata must remain unavailable');
    assert.strictEqual(guide.read(), origin); assert(guide.coordinates().current());
    assert.equal(guideQueries(), beforeGuide); assert.strictEqual(f.session.info(), info);
    assert.equal(f.session.readSubscriptionGeneration(), ticket); assert.equal(info.contentRevision, 0);
    const noticeCount = f.notices.length;
    f.dispatch(f.h, 'pointerdown'); assert.equal(f.ghosts().length, 1);
    f.dispatch(f.h, 'pointermove', { clientX: 516, clientY: 111 });
    f.dispatch(f.h, 'pointerup', { clientX: 566, clientY: 111 });
    assert.equal(f.notices.length, noticeCount); assert.equal(f.ghosts().length, 0);
    assert.equal(f.edits().length, 0); assert.equal(f.saved(), f.originalDocument);
    await f.settle(); assert.equal(queries().length, before + 1);
  } finally { if (refusal) refusal.release(); if (preview) preview.release(); await f.dispose(); }
});

test('real completion fanout converges after one refused Shadow read while preserving the loaded Guide owner', async () => {
  const f = fixture({ shadow: true }); let preview, refusal;
  try {
    await f.prepare(); f.diagnostics.length = 0;
    const guide = f.root.NemoNativeGuideController, shadow = f.root.NemoNativeShadowController;
    const origin = guide.read(), info = f.session.info(), ticket = f.session.readSubscriptionGeneration();
    assert(origin && shadow.read()); assert(guide.coordinates().current());
    const state = JSON.stringify(f.root.state);
    const reads = operation => f.calls.filter(call => call.request && call.request.operation === operation).length;
    const shadowOperation = 'query.document.object.project.shadow.read', guideOperation = 'query.document.object.project.guide.read';
    const previews = () => f.calls.filter(call => call.command === 'nemo_native_object_preview').length;
    const before = reads(shadowOperation), beforeGuide = reads(guideOperation), beforePreview = previews();
    preview = f.holdPreview(); f.dispatch(f.root, 'resize'); await preview.started;
    assert.equal(previews(), beforePreview + 1, 'The registered same-bounds window callback starts the real preview');
    assert.strictEqual(f.session.info(), info); assert.equal(f.session.readPresentedObjectSession(), null);
    // Explicit public read boundary: the actual Shadow palette/active-swatch caller uses this controller.read.
    assert.equal(shadow.read(), null);
    assert.equal(guide.read(), null, 'A loaded Guide cache cannot authorize input during genuine pending work');
    refusal = f.refuseNextShadowRead(); preview.release(); await refusal.started;
    assert.equal(reads(shadowOperation), before + 1);
    assert.equal(f.calls.at(-1).request.payload.selector.kind, 'settings');
    assert.equal(f.session.readPresentedObjectSession(), null);
    f.dispatch(f.h, 'pointerdown');
    assert.deepEqual(f.diagnostics, [['camera-presented', 'canvas'], ['guide-entry-refused', 'presentation-missing']]);
    assert.equal(f.notices.length, 1); assert.equal(f.ghosts().length, 0); assert.equal(f.edits().length, 0);
    refusal.release(); await f.settle();
    assert.equal(reads(shadowOperation), before + 1,
      'Unchanged readonly completion must not issue another unsolicited same-pin Shadow read');
    assert.equal(previews(), beforePreview + 1, 'A metadata refusal must not manufacture a native presentation');
    assert.equal(shadow.read(), null, 'Unavailable Shadow metadata remains unavailable');
    assert.strictEqual(guide.read(), origin); assert(guide.coordinates().current());
    assert.equal(reads(guideOperation), beforeGuide); assert.strictEqual(f.session.info(), info);
    assert.equal(f.session.readSubscriptionGeneration(), ticket); assert.equal(info.contentRevision, 0);
    const noticeCount = f.notices.length;
    f.dispatch(f.h, 'pointerdown'); assert.equal(f.ghosts().length, 1);
    f.dispatch(f.h, 'pointermove', { clientX: 516, clientY: 111 });
    f.dispatch(f.h, 'pointerup', { clientX: 566, clientY: 111 });
    assert.equal(f.notices.length, noticeCount); assert.equal(f.ghosts().length, 0);
    assert.equal(f.edits().length, 0); assert.equal(f.saved(), f.originalDocument);
    assert.equal(JSON.stringify(f.root.state), state, 'Readonly refusal and ruler cancellation preserve all UI state');
    await f.settle(); assert.equal(reads(shadowOperation), before + 1);
  } finally { if (refusal) refusal.release(); if (preview) preview.release(); await f.dispose(); }
});
