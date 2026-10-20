'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), vm = require('node:vm');
const C = require('../src/js/adapters/native-guide-contract.js');
const clone = x => JSON.parse(JSON.stringify(x));
function node(tag = 'div') {
  const events = new Map(), el = { tagName: tag.toUpperCase(), children: [], style: {}, clientWidth: 400, clientHeight: 240,
    classList: { toggle() {} }, width: 0, height: 0,
    addEventListener(k, fn) { if (!events.has(k)) events.set(k, []); events.get(k).push(fn); },
    removeEventListener(k, fn) { events.set(k, (events.get(k) || []).filter(f => f !== fn)); },
    count(k) { return (events.get(k) || []).length; },
    fire(k, extra = {}) {
      let stopped = false;
      const e = { target: el, pointerId: 1, clientX: 40, clientY: 60, preventDefault() {}, stopPropagation() {},
        stopImmediatePropagation() { stopped = true; }, ...extra };
      for (const fn of [...events.get(k) || []]) { fn(e); if (stopped) break; }
    },
    appendChild(child) { child.parent = this; this.children.push(child); return child; },
    remove() { if (this.parent) this.parent.children = this.parent.children.filter(n => n !== this); },
    getBoundingClientRect: () => ({ left: 0, top: 0 }),
    getContext() { return { setTransform() {}, clearRect() {}, fillRect() {}, beginPath() {}, moveTo() {}, lineTo() {}, stroke() {}, fillText() {},
      save() {}, translate() {}, rotate() {}, restore() {} }; }
  };
  return el;
}
function fixture() {
  const area = node(), h = node('canvas'), v = node('canvas'), frames = [], prefs = [];
  const roots = [area, h, v]; area.id = 'canvas-area'; h.id = 'ruler-h'; v.id = 'ruler-v';
  function walk(nodes) { return nodes.flatMap(n => [n, ...walk(n.children)]); }
  const document = Object.assign(node('document'), { readyState: 'loading', hidden: false, body: node('body'), documentElement: node(),
    getElementById: id => walk(roots).find(n => n.id === id) || null, createElement: node });
  let info = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 's0' };
  let revision = 0, serial = 0, ticket = 1, running = 0, active = true, tail = Promise.resolve(), held = null;
  let metadata = { h: [-12.25], v: [] };
  const calls = [], notices = [], state = { rulersOn: true, guidesLocked: false, guidesSnap: true, smartGuides: false };
  Object.defineProperty(state, 'guides', { get() { throw Error('native caller read legacy guides'); }, set() { throw Error('native caller wrote legacy guides'); } });
  let root, context, controller;
  const presented = pin => ({ ...pin, presentation: { ...pin, owner: 'native', status: 'presented' } });
  async function dispatch(q) {
    calls.push(clone(q));
    if (held) { const own = held; held = null; own.entered(); await own.wait; }
    const reply = { apiVersion: 2, requestId: q.requestId, instanceId: info.instanceId, documentId: info.documentId, contentRevision: revision, ok: true };
    if (q.documentId !== info.documentId) { reply.ok = false; reply.error = { code: 'wrong_document', message: 'Document replaced' }; }
    else if (q.operation === C.EDIT) {
      assert.equal(q.expectedRevision, revision); const before = JSON.stringify(metadata), a = q.payload.action;
      if (a.kind === 'add') (metadata[a.axis] ||= []).push(a.position);
      else if (a.kind === 'move') metadata[a.axis][a.ordinal - 1] = a.position;
      else if (a.kind === 'remove') metadata[a.axis].splice(a.ordinal - 1, 1);
      else assert.fail('unexpected guide intent');
      const applied = before !== JSON.stringify(metadata); if (applied) revision++;
      reply.contentRevision = revision; reply.result = { applied, historyEntriesAdded: Number(applied) };
    } else {
      assert.equal(q.operation, C.READ); assert.equal(q.payload.atRevision, revision); assert.equal(q.payload.documentSnapshotId, 's' + revision);
      const s = q.payload.selector, values = metadata[s.axis] || [];
      const value = s.kind === 'settings' ? { guidesPresent: true, hPresent: Object.hasOwn(metadata, 'h'), vPresent: Object.hasOwn(metadata, 'v'),
        hCount: (metadata.h || []).length, vCount: (metadata.v || []).length } :
        { kind: 'guideAxisPage', axis: s.axis, guidesPresent: true, axisPresent: Object.hasOwn(metadata, s.axis), guideCount: values.length,
          cursor: s.cursor, items: values.slice(s.cursor, s.cursor + s.limit).map((position, i) => ({ ordinal: s.cursor + i + 1, position })),
          nextCursor: s.cursor + s.limit < values.length ? s.cursor + s.limit : null };
      reply.result = { instanceId: info.instanceId, documentId: info.documentId, atRevision: revision, documentSnapshotId: 's' + revision, contextId: 'scene-root', result: value };
    }
    return vm.runInContext('JSON.parse(' + JSON.stringify(JSON.stringify(reply)) + ')', context);
  }
  const session = { info: () => info, isActive: () => active, requestId: () => 'guide-' + (++serial), readSubscriptionGeneration: () => ticket,
    readPresentedObjectSession: () => active && !running ? presented(info) : null,
    runResourceCommand(action) {
      running++;
      const work = tail.then(() => { const origin = info;
        return action({ dispatch: q => root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('ui', { dispatch }).dispatch(q),
          readPresentedObjectSession: () => active && running === 1 && info === origin ? presented(origin) : null,
          async reconcile(r) { assert.equal(r, revision); info = { ...info, contentRevision: r, documentSnapshotId: 's' + r };
            controller.willProject(info); controller.presented(info); return info; } });
      }).finally(() => { running--; controller.presented(info); });
      tail = work.catch(() => {}); return work;
    }
  };
  function Point(x, y) { this.x = x; this.y = y; }
  root = { document, state, Point, __TAURI__: { core: {} }, devicePixelRatio: 1,
    view: { zoom: 2, center: { x: 0, y: 0 }, projectToView: p => new Point(p.x * 2 + 100, p.y * 2 + 100),
      viewToProject: p => new Point((p.x - 100) / 2, (p.y - 100) / 2) },
    NemoNativeObjectProject: { desktop: () => session }, NemoNativeObjectProjectFile: { projectionReadyForUI: () => active && !running },
    getComputedStyle: () => ({ getPropertyValue: () => '' }), requestAnimationFrame: fn => frames.push(fn),
    localStorage: { getItem: key => key === 'nemo-rulers-on' ? '1' : null, setItem: (key, value) => prefs.push([key, value]) },
    showToast: value => notices.push(value), addEventListener() {} };
  root.window = root; root.globalThis = root; context = vm.createContext(root);
  const needed = ['js/adapters/native-guide-contract.js', 'js/adapters/native-application.js', 'js/adapters/native-guide-desktop.js',
    'js/adapters/native-guide-ui.js', 'js/rulers-bridge.js'];
  const html = fs.readFileSync(require.resolve('../src/index.html'), 'utf8');
  const scripts = [...html.matchAll(/<script\b([^>]*)>/gi)].map(m => ({ src: m[1].match(/\bsrc=["']([^"']+)/)?.[1], tag: m[0] }));
  const actual = scripts.filter(script => needed.includes(script.src));
  for (const file of needed) {
    const selected = actual.filter(script => script.src === file); assert.equal(selected.length, 1, file);
    assert.equal(selected[0].tag, '<script src="' + file + '">', file + ' must be a synchronous classic script');
  }
  const at = file => actual.findIndex(script => script.src === 'js/adapters/' + file + '.js');
  assert(at('native-guide-contract') < at('native-application'));
  assert(at('native-guide-desktop') < at('native-guide-ui'));
  for (const script of actual)
    vm.runInContext(fs.readFileSync(require.resolve('../src/' + script.src), 'utf8'), context, { filename: script.src });
  controller = root.NemoNativeGuideController; document.fire('DOMContentLoaded');
  function tick() { const frame = frames.shift(); if (frame) frame(); }
  async function settled() {
    for (let i = 0; i < 30; i++) { await tail; await new Promise(r => setImmediate(r)); if (!controller.isPending()) { tick(); return; } }
    throw Error('guide caller did not settle');
  }
  return { root, document, area, h, v, controller, calls, notices, prefs, tick, settled,
    async ready() { await controller.presented(); await settled(); assert(controller.read(), 'Actual production guide SDK admission/presentation is required'); },
    edits: () => calls.filter(q => q.operation === C.EDIT), metadata: () => clone(metadata),
    hits: () => walk(roots).filter(n => /^guide-hit /.test(n.className || '')),
    ghosts: () => walk(roots).filter(n => n.style.opacity === '.6'),
    replace() { info = { ...info, documentId: 'replacement', lifecycleGeneration: 2 }; controller.willProject(info); },
    resubscribe() { ticket++; }, retire() { active = false; controller.dispose(); tick(); },
    hold() { let entered, release; const started = new Promise(r => entered = r), wait = new Promise(r => release = r);
      held = { entered, wait }; return { started, release }; }
  };
}

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
