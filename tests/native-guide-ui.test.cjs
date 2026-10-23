'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), vm = require('node:vm');
const C = require('../src/js/adapters/native-guide-contract.js');
const Project = require('../src/js/adapters/native-object-project.js');
const Ports = require('../src/js/adapters/native-object-project-desktop.js');
const Underlay = require('../src/js/adapters/native-viewport-underlay.js');
const Files = require('../src/js/adapters/native-object-project-file.js');
const Application = require('../src/js/adapters/native-application.js');
const GuideUI = require('../src/js/adapters/native-guide-ui.js');
const CommentsUI = require('../src/js/adapters/native-comments-ui.js');
const clone = x => JSON.parse(JSON.stringify(x));
const { fixture } = require('./helpers/native-guide-ui.cjs');

test('actual ruler pointer release submits one native guide edit and never authors a projected or legacy value', async () => {
  const f = fixture(); await f.ready(); const before = f.metadata();
  f.document.fire('pointerdown', { target: f.h }); f.document.fire('pointermove', { clientY: 48 });
  assert.equal(f.ghosts().length, 1); assert.equal(f.edits().length, 0); assert.deepEqual(f.metadata(), before);
  f.document.fire('pointerup', { clientY: 48 }); await f.settled();
  assert.deepEqual(f.edits()[0].payload.action, { kind: 'add', axis: 'h', position: -26 });
  assert.equal(f.ghosts().length, 0); assert.deepEqual(f.metadata().h, [-12.25, -26]);
  const hit = f.hits()[0]; f.document.fire('pointerdown', { target: hit }); f.document.fire('pointerup', { clientY: 70 }); await f.settled();
  assert.deepEqual(f.edits()[1].payload.action, { kind: 'move', axis: 'h', ordinal: 1, position: -15 });
  f.document.fire('pointerdown', { target: f.hits()[0] }); f.document.fire('pointerup', { clientY: 8 }); await f.settled();
  assert.deepEqual(f.edits()[2].payload.action, { kind: 'remove', axis: 'h', ordinal: 1 });
});

test('guide capture listeners stay bounded through native invalidation/readback and detached hits cannot edit', async () => {
  const f = fixture(); await f.ready(); assert.equal(f.document.count('pointerdown'), 3);
  const old = f.hits()[0];
  for (let i = 0; i < 8; i++) {
    f.controller.willProject(); f.tick(); assert.equal(f.document.count('pointerdown'), 2);
    await f.ready(); assert.equal(f.document.count('pointerdown'), 3);
  }
  f.document.fire('pointerdown', { target: old }); f.document.fire('pointerup'); await f.settled(); assert.equal(f.edits().length, 0);
  f.document.fire('pointerdown', { target: f.hits()[0] }); f.document.fire('pointerup', { clientY: 70 }); await f.settled();
  assert.equal(f.edits().length, 1); assert.equal(f.document.count('pointerdown'), 3);
});

test('ruler visibility stays a local preference and return-to-ruler, cancel and mismatched pointer never create a guide', async () => {
  const f = fixture(); await f.ready();
  f.root.SMRulers.toggleOn(false); f.root.SMRulers.toggleOn(true); f.tick();
  assert.deepEqual(f.prefs, [['nemo-rulers-on', '0'], ['nemo-rulers-on', '1']]); assert.equal(f.edits().length, 0);
  f.document.fire('pointerdown', { target: f.h }); f.document.fire('pointercancel', { pointerId: 2 }); assert.equal(f.ghosts().length, 1);
  f.document.fire('pointerup', { clientY: 8 }); await f.settled(); assert.equal(f.edits().length, 0);
  f.document.fire('pointerdown', { target: f.h }); f.document.fire('pointercancel'); await f.settled(); assert.equal(f.ghosts().length, 0);
});

test('replacement, subscription changes and retirement prevent old pointer gestures from committing', async () => {
  for (const change of ['replace', 'resubscribe', 'retire']) {
    const f = fixture(); await f.ready(); const before = f.metadata(); f.document.fire('pointerdown', { target: f.h });
    f[change](); f.document.fire('pointerup', { clientY: 70 }); await f.settled();
    assert.equal(f.edits().length, 0); assert.deepEqual(f.metadata(), before); assert.equal(f.ghosts().length, 0);
  }
});

test('pending guide edit clears the gesture and rejects overlapping pointer edits without optimistic metadata', async () => {
  const f = fixture(); await f.ready(); const before = f.metadata(), held = f.hold();
  f.document.fire('pointerdown', { target: f.h }); f.document.fire('pointerup', { clientY: 48 }); await held.started;
  assert.equal(f.controller.read(), null); assert.equal(f.ghosts().length, 0); assert.deepEqual(f.metadata(), before);
  f.document.fire('pointerdown', { target: f.h }); f.document.fire('pointerup', { clientY: 70 }); assert.equal(f.edits().length, 1);
  held.release(); await f.settled(); assert.equal(f.edits().length, 1);
});

test('the existing snap consumer reads pinned guide coordinates and refuses unconfirmed guide state', async () => {
  const f = fixture(); await f.ready();
  const snapped = f.root.SMRulers.snapPoint(new f.root.Point(0, -10), 8);
  assert.equal(snapped.y, -12.25); assert.equal(snapped.x, 0); assert.equal(f.edits().length, 0);
  f.controller.willProject(); f.tick();
  assert.throws(() => f.root.SMRulers.snapPoint(new f.root.Point(0, -10), 8), e => e.code === 'unavailable');
  f.root.SMRulers.toggleOn(false);
  const point = new f.root.Point(0, -10); assert.strictEqual(f.root.SMRulers.snapPoint(point, 8), point);
});


test('actual ProjectFile readiness publishes guides without widening their authority pin', async () => {
  const f = fixture(); assert.equal(f.projectionReady(f.fullInfo()), true); await f.ready();
  const shown = f.controller.read();
  assert.deepEqual(Object.keys(shown.info).sort(), ['contentRevision', 'documentId', 'documentSnapshotId', 'instanceId', 'lifecycleGeneration']);
  assert.equal(f.projectionReady(shown.info), false, 'five authority pins cannot stand in for full native UI metadata');
  assert.deepEqual(clone(shown.guides.h), [-12.25]); assert.equal(f.hits().length, 1);
  const before = f.metadata(); f.drift(); assert.equal(f.controller.read(), null);
  f.document.fire('pointerdown', { target: f.h }); f.document.fire('pointerup', { clientY: 48 }); await f.settled();
  assert.equal(f.ghosts().length, 0); assert.equal(f.edits().length, 0); assert.deepEqual(f.metadata(), before);
  assert.throws(() => f.root.SMRulers.snapPoint(new f.root.Point(0, -10), 8), e => e.code === 'unavailable');
  f.repairProjection(); await f.ready(); assert.equal(f.edits().length, 0, 'restoring projection does not replay the refused gesture');
  f.document.fire('pointerdown', { target: f.h }); f.document.fire('pointerup', { clientY: 48 }); await f.settled();
  assert.equal(f.edits().length, 1); assert.deepEqual(f.edits()[0].payload.action, { kind: 'add', axis: 'h', position: -26 });
  assert.deepEqual(f.metadata().h, [-12.25, -26]);
});

test('full UI projection drift during a held guide read prevents publication', async () => {
  const f = fixture(), held = f.hold(), loading = f.controller.presented(); await held.started;
  f.drift(); held.release(); assert.equal(await loading, null); await f.settled();
  assert.equal(f.controller.read(), null); assert.equal(f.edits().length, 0); assert.equal(f.ghosts().length, 0);
});


const turn = () => new Promise(setImmediate);
const timerTurn = () => new Promise(resolve => setTimeout(resolve, 0));
function gate() {
  let resolve;
  const promise = new Promise(r => { resolve = r; });
  return { promise, resolve };
}
async function boundedGuideObservation(promise, label) {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(Error(label + ' did not settle')), 1000);
    })]);
  } finally { clearTimeout(timer); }
}

// Native/DOM replies are controlled; the Project queue, ports, SDK,
// ProjectFile and both installed metadata controllers are production modules.
function projectFixture() {
  let installed = false, generation = 0, documentId = null, commentsReplyHold = null;
  const calls = [], transports = [], classes = new Set(), styles = new Map();
  const info = () => ({ apiVersion: 2, instanceId: 'instance', documentId, contentRevision: 0,
    documentSnapshotId: 'snapshot-' + generation + '-0', lifecycleGeneration: generation,
    documentDigest: 'a'.repeat(64), byteCount: 100, canvasW: 320, canvasH: 180,
    fps: 24, totalFrames: 4, layers: [{ layerUid: 'drawing', visible: true }], reference: null });
  const document = {
    documentElement: { clientWidth: 1200, clientHeight: 900,
      classList: { contains: key => classes.has(key), add: key => classes.add(key), remove: key => classes.delete(key) },
      style: { getPropertyValue: key => styles.get(key) || '', getPropertyPriority: () => '',
        setProperty: (key, value) => styles.set(key, value), removeProperty: key => styles.delete(key) } },
    getElementById(id) {
      if (id === 'fb-avatars-pop' || id === 'zoom-scrub') return null;
      assert.equal(id, 'drawing-canvas');
      return { getBoundingClientRect: () => ({ left: 20.5, top: 30.25, width: 320.5, height: 180.25 }) };
    }
  };
  const root = { document, stageLayer: { visible: true }, paper: { view: { update() {} } },
    state: { currentFrame: 0 }, devicePixelRatio: 2, NemoNativeViewportUnderlay: Underlay,
    NemoNativeObjectProject: Project, NemoNativeObjectProjectDesktop: Ports,
    NemoNativeObjectProjectFile: Files, NemoNativeApplicationAdapter: Application,
    NemoNativeObjectUI: { refreshTiming(value) { assert.equal(value, root); },
      refreshPolicy(value) { assert.equal(value, root); } },
    SMReference: { refreshNative() {} }, SMProject: { refreshActiveTabDirtyDot() {} }, updateUI() {} };
  async function invoke(command, args) {
    const r = args && (args.request || JSON.parse(args.requestJson));
    calls.push({ command, r });
    if (command === 'nemo_native_object_client_status') return { ...info(), available: installed };
    if (command === 'nemo_native_object_bootstrap') {
      installed = true; generation++; documentId = 'document-' + generation;
      return { ...info(), requestId: r.requestId, resourceCount: 0, viewportAvailable: false };
    }
    if (command === 'nemo_native_object_reference') {
      assert.equal(r.action.kind, 'inspect');
      return { ...info(), requestId: r.requestId };
    }
    if (command === 'nemo_native_object_preview')
      return { status: 'presented', workId: 'work-' + calls.length, viewGeneration: 1 };
    if (command === 'nemo_native_revision_sync') {
      assert.equal(r.action, 'acknowledge'); return null;
    }
    assert.equal(command, 'nemo_native_dispatch', 'only the existing admitted native boundary is available');
    const pin = info(), p = r.payload;
    assert.equal(p.atRevision, pin.contentRevision);
    assert.equal(p.documentSnapshotId, pin.documentSnapshotId);
    assert.equal(p.contextId, 'scene-root');
    let value;
    if (r.operation === C.READ) {
      if (p.selector.kind === 'settings') {
        value = { guidesPresent: false, hPresent: false, vPresent: false, hCount: 0, vCount: 0 };
      } else {
        assert.equal(p.selector.kind, 'axisPage'); assert.ok(['h', 'v'].includes(p.selector.axis));
        assert.equal(p.selector.cursor, 0); assert.equal(p.selector.limit, 16);
        value = { kind: 'guideAxisPage', axis: p.selector.axis, guidesPresent: false, axisPresent: false,
          guideCount: 0, cursor: 0, items: [], nextCursor: null };
      }
    } else {
      assert.equal(r.operation, 'query.document.object.project.comments.read');
      if (p.selector.kind === 'settings') {
        if (commentsReplyHold) {
          const own = commentsReplyHold; commentsReplyHold = null; own.entered(); await own.promise;
        }
        value = { commentsPresent: false, commentCount: 0 };
      } else {
        assert.equal(p.selector.kind, 'commentsPage');
        assert.equal(p.selector.cursor, 0); assert.equal(p.selector.limit, 16);
        value = { kind: 'projectCommentsPage', commentsPresent: false, commentCount: 0,
          cursor: 0, items: [], nextCursor: null };
      }
    }
    return { apiVersion: 2, requestId: r.requestId, instanceId: pin.instanceId, documentId: pin.documentId,
      contentRevision: pin.contentRevision, ok: true, result: { instanceId: pin.instanceId,
        documentId: pin.documentId, atRevision: pin.contentRevision, documentSnapshotId: pin.documentSnapshotId,
        contextId: 'scene-root', result: value } };
  }
  root.__TAURI__ = { core: { invoke }, event: { listen: async () => () => {} } };
  // Copied existing desktop fixture transport: no alternate session or resource queue.
  root.NemoApplicationMcpTransport = { createNativeTauriTransport: call => {
    const transport = { call, connected: false, binding: null, synchronize: null, disconnected: null,
      connect: async () => { transport.connected = true; },
      subscribeRevisions: async (listen, synchronize, disconnected) => {
        assert.equal(typeof listen, 'function');
        transport.synchronize = synchronize; transport.disconnected = disconnected;
        transport.binding = { ...info(), subscriptionId: 'subscription-' + generation };
        return transport.binding;
      },
      status: () => transport.connected ? transport.binding : null, isSynchronizing: () => false,
      disconnect: async () => { transport.connected = false; },
      dispatch: r => call('nemo_native_dispatch', { request: r }),
      dispatchObject: (command, r) => call(command, { requestJson: JSON.stringify(r) }) };
    transports.push(transport); return transport;
  } };
  const session = Project.desktop(root);
  async function open() {
    const value = await session.openPath('/fake.json', { reveal: async pin => { Files.projectView(root, pin); root.updateUI(); } });
    await timerTurn(); return value;
  }
  return { root, session, calls, transports, open, commentsReplyGate: value => { commentsReplyHold = value; } };
}

test('ordinary ruler reads during held Comments work retain the same immutable Guide projection', { timeout: 15000 }, async () => {
  const f = projectFixture(), firstGuide = gate(), firstComments = gate();
  let guide = null, comments = null, original = null, guidePublications = 0;
  function readGate() {
    const held = gate(), started = gate();
    return { ...held, started: started.promise, entered: started.resolve };
  }
  const first = readGate(), second = readGate();
  async function bounded(promise, label) {
    let timer;
    try {
      return await Promise.race([promise, new Promise((_, reject) => {
        timer = setTimeout(() => reject(Error(label + ' did not settle')), 1000);
      })]);
    } finally { clearTimeout(timer); }
  }
  f.root.document.addEventListener = () => {};
  f.root.document.removeEventListener = () => {};
  f.root.addEventListener = () => {};
  f.root.removeEventListener = () => {};
  f.root.SMRulers = { render() {
    const value = guide && guide.read();
    if (value) {
      guidePublications++;
      if (!original) { original = value; firstGuide.resolve(value); }
    }
  } };
  f.root.refreshFbAvatars = () => {
    const value = comments && comments.read();
    if (value) firstComments.resolve(value);
  };
  try {
    guide = GuideUI.install(f.root);
    comments = CommentsUI.install(f.root);
    await turn();
    f.commentsReplyGate(first);
    await bounded(f.open(), 'ordinary Open');
    const shown = await bounded(firstGuide.promise, 'first ordinary Guide projection');
    await bounded(first.started, 'held Comments settings read');
    const owner = f.session.info(), ticket = f.session.readSubscriptionGeneration();
    assert.strictEqual(original, shown);
    assert.equal(Object.isFrozen(shown), true);
    assert.equal(shown.info.contentRevision, 0);
    assert.deepEqual(shown.guides, {
      settings: { guidesPresent: false, hPresent: false, vPresent: false, hCount: 0, vCount: 0 }, h: [], v: []
    });
    assert.equal(f.session.readPresentedObjectSession(), null, 'Comments must fence the actual public presentation');
    assert.equal(guide.read(), null, 'ordinary ruler consumption must remain unavailable while native work is pending');
    assert.equal(guide.read(), null, 'repeated ruler reads must not expose the pending presentation');
    const beforeRelease = f.calls.filter(call => call.command === 'nemo_native_dispatch');
    assert.deepEqual(beforeRelease.filter(call => call.r.operation === 'query.document.object.project.guide.read')
      .map(call => call.r.payload.selector.kind), ['settings', 'axisPage', 'axisPage']);
    // A repeated Comments query is held only if production erroneously requests it.
    // No presented(), retry or manual refresh is called after the original release.
    f.commentsReplyGate(second);
    first.resolve();
    const commentValue = await bounded(firstComments.promise, 'first complete Comments projection');
    await timerTurn();
    await turn();
    const recovered = guide.read();
    const queries = f.calls.filter(call => call.command === 'nemo_native_dispatch');
    const guideQueries = queries.filter(call => call.r.operation === 'query.document.object.project.guide.read');
    const commentQueries = queries.filter(call => call.r.operation === 'query.document.object.project.comments.read');
    assert.strictEqual(recovered, shown, 'readonly work must retain the same Guide projection; observations=' + JSON.stringify({
      guidePublications, guideQueryCount: guideQueries.length, guideQueries: guideQueries.slice(-12).map(call => call.r.payload.selector),
      commentQueryCount: commentQueries.length, commentQueries: commentQueries.slice(-12).map(call => call.r.payload.selector),
      publicPresented: !!f.session.readPresentedObjectSession()
    }));
    assert.deepEqual(guideQueries.map(call => call.r.payload.selector), [
      { kind: 'settings' }, { kind: 'axisPage', axis: 'h', cursor: 0, limit: 16 },
      { kind: 'axisPage', axis: 'v', cursor: 0, limit: 16 }
    ]);
    assert.deepEqual(commentQueries.map(call => call.r.payload.selector.kind), ['settings', 'commentsPage']);
    assert.deepEqual(commentValue.info, shown.info);
    assert.ok(queries.every(call => call.r.operation.startsWith('query.') && !Object.hasOwn(call.r, 'expectedRevision')));
    assert.strictEqual(f.session.info(), owner);
    assert.equal(f.session.readSubscriptionGeneration(), ticket);
    assert.equal(owner.contentRevision, 0);
    assert.ok(f.session.readPresentedObjectSession());
    assert.equal(f.transports.length, 1);
    assert.equal(f.session.isIndeterminate(), false);
    const count = f.calls.length;
    await timerTurn();
    await turn();
    assert.equal(f.calls.length, count, 'retained projections must not schedule additional native work');
    assert.strictEqual(guide.read(), shown);
  } finally {
    // Retire controllers before release so a failed identity oracle cannot leak callbacks.
    if (guide) guide.dispose();
    if (comments) comments.dispose();
    first.resolve();
    second.resolve();
    await turn();
    await timerTurn();
    await turn();
  }
});


test('ordinary completion restores pruned ruler guides without changing geometry or rereading metadata', { timeout: 15000 }, async () => {
  const f = fixture();
  await f.ready();
  const shown = f.controller.read(), count = f.calls.length, held = gate(), entered = gate();
  const geometry = () => [f.root.view.zoom, f.root.view.center.x, f.root.view.center.y,
    f.area.clientWidth, f.area.clientHeight];
  const beforeGeometry = geometry();
  let resource = null;
  try {
    assert.equal(f.hits().length, 1);
    resource = f.root.NemoNativeObjectProject.desktop(f.root).runResourceCommand(async () => {
      entered.resolve(); await held.promise;
    });
    await boundedGuideObservation(entered.promise, 'unrelated resource admission');
    assert.equal(f.controller.read(), null);
    f.root.SMRulers.render();
    f.tick(); // Real RAF loop reads while fenced and prunes the existing hit DOM.
    assert.equal(f.hits().length, 0);
    assert.deepEqual(geometry(), beforeGeometry);
    held.resolve();
    await boundedGuideObservation(resource, 'ordinary resource completion');
    assert.strictEqual(f.controller.read(), shown);
    f.tick(); // No manual render/invalidation follows completion.
    assert.equal(f.hits().length, 1);
    assert.deepEqual(geometry(), beforeGeometry);
    assert.strictEqual(f.controller.read(), shown);
    assert.equal(f.calls.length, count, 'restoring Guide DOM must not reread native metadata');
    assert.equal(f.edits().length, 0);
    assert.equal(f.ghosts().length, 0);
    assert.equal(f.notices.length, 0);
  } finally {
    f.retire();
    held.resolve();
    await Promise.allSettled([resource]);
  }
});
