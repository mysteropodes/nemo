'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs'), vm = require('node:vm');
const C = require('../../src/js/adapters/native-guide-contract.js');
const clone = x => JSON.parse(JSON.stringify(x));
function node(tag = 'div') {
  const events = new Map(), labels = [], el = { tagName: tag.toUpperCase(), children: [], style: {}, clientWidth: 400, clientHeight: 240, labels,
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
    getContext() { return { setTransform() {}, clearRect() { labels.length = 0; }, fillRect() {}, beginPath() {}, moveTo() {}, lineTo() {}, stroke() {}, fillText(...args) { labels.push(args); },
      save() {}, translate() {}, rotate() {}, restore() {} }; }
  };
  return el;
}
function fixture(options = {}) {
  const area = node(), canvas = node('canvas'), h = node('canvas'), v = node('canvas'), frames = [], prefs = [];
  const roots = [area, canvas, h, v]; area.id = 'canvas-area'; canvas.id = 'drawing-canvas'; h.id = 'ruler-h'; v.id = 'ruler-v';
  let rect = { left: 0, top: 0, width: 400, height: 240, ...options.rect };
  area.getBoundingClientRect = canvas.getBoundingClientRect = () => ({ ...rect });
  area.clientWidth = rect.width; area.clientHeight = rect.height;
  function walk(nodes) { return nodes.flatMap(n => [n, ...walk(n.children)]); }
  const document = Object.assign(node('document'), { readyState: 'loading', hidden: false, body: node('body'), documentElement: node(),
    getElementById: id => walk(roots).find(n => n.id === id) || null, createElement: node });
  let info = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 's0',
    canvasW: 320, canvasH: 180, fps: 24, totalFrames: 40, layers: [{ layerUid: 'drawing', visible: true }] };
  let revision = 0, serial = 0, ticket = 1, running = 0, active = true, tail = Promise.resolve(), held = null;
  let metadata = clone(options.guides || { h: [-12.25], v: [] });
  const calls = [], notices = [], state = { rulersOn: true, guidesLocked: false, guidesSnap: true, smartGuides: false,
    currentFrame: 0, cameraView: options.cameraView === true };
  Object.defineProperty(state, 'guides', { get() { throw Error('native caller read legacy guides'); }, set() { throw Error('native caller wrote legacy guides'); } });
  let root, context, controller, browserRoot;
  let affine = options.affine || [2, 0, 0, 2, 100, 100], viewGeneration = 1, proof, proofPin, invalidProof = null;
  function presented(pin) {
    if (proofPin !== pin) {
      proofPin = pin;
      const [a, b, c, d, e, f] = affine, det = a * d - b * c;
      const forward = [a * 320 / rect.width, b * 180 / rect.height, c * 320 / rect.width,
        d * 180 / rect.height, e * 320 / rect.width, f * 180 / rect.height];
      const [A, B, C, D, E, F] = forward, determinant = A * D - B * C;
      assert.notEqual(det, 0);
      proof = { ...pin, owner: 'native', status: 'presented', contextId: 'scene-root', frame: state.currentFrame, workId: 'view-' + viewGeneration, viewGeneration,
        view: { mode: options.cameraView ? 'stored-camera' : 'canvas', navigation: { zoom: 1, pan: [0, 0] }, documentToComposition: forward,
          compositionToDocument: [D / determinant, -B / determinant, -C / determinant, A / determinant,
            (C * F - D * E) / determinant, (B * E - A * F) / determinant] } };
    }
    return { ...pin, presentation: invalidProof ? { ...proof, ...invalidProof } : proof };
  }
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
            controller.willProject(info);
            assert.equal(root.NemoNativeObjectProjectFile.refreshLayerTable(browserRoot, info), true);
            controller.presented(info); return info; } });
      }).finally(() => { running--; controller.presented(info); });
      tail = work.catch(() => {}); return work;
    }
  };
  function Point(x, y) { this.x = x; this.y = y; }
  root = { document, state, Point, __TAURI__: { core: {} }, devicePixelRatio: 1,
    view: { zoom: 2, center: { x: 0, y: 0 }, projectToView: p => new Point(p.x * 2 + 100, p.y * 2 + 100),
      viewToProject: p => new Point((p.x - 100) / 2, (p.y - 100) / 2) },
    NemoNativeObjectProject: { desktop: () => session },
    getComputedStyle: () => ({ getPropertyValue: () => '' }), requestAnimationFrame: fn => frames.push(fn),
    localStorage: { getItem: key => key === 'nemo-rulers-on' ? '1' : null, setItem: (key, value) => prefs.push([key, value]) },
    showToast: value => notices.push(value), addEventListener() {} };
  const styles = new Map();
  ['left', 'top', 'right', 'bottom'].forEach((key, i) => styles.set('--nemo-native-' + key,
    [rect.left, rect.top, rect.left + rect.width, rect.top + rect.height][i] + 'px'));
  document.documentElement.clientWidth = 1400; document.documentElement.clientHeight = 1000;
  document.documentElement.classList.contains = key => key === 'nemo-native-underlay';
  document.documentElement.style.getPropertyValue = key => styles.get(key) || '';
  if (options.paperPoison) Object.defineProperty(root, 'view', { get() { throw Error('native Guide read Paper view'); } });
  root.window = root; root.globalThis = root; context = vm.createContext(root);
  const needed = ['js/adapters/native-viewport-underlay.js', 'js/adapters/native-object-layer-splice.js', 'js/adapters/native-object-project-file.js',
    'js/adapters/native-guide-contract.js', 'js/adapters/native-application.js', 'js/adapters/native-guide-desktop.js',
    'js/adapters/native-guide-ui.js', 'js/rulers-bridge.js'];
  const html = fs.readFileSync(require.resolve('../../src/index.html'), 'utf8');
  const scripts = [...html.matchAll(/<script\b([^>]*)>/gi)].map(m => ({ src: m[1].match(/\bsrc=["']([^"']+)/)?.[1], tag: m[0] }));
  const actual = scripts.filter(script => needed.includes(script.src));
  assert.deepEqual(actual.map(script => script.src), [
    'js/adapters/native-viewport-underlay.js', 'js/adapters/native-object-layer-splice.js', 'js/adapters/native-object-project-file.js', 'js/rulers-bridge.js',
    'js/adapters/native-guide-contract.js', 'js/adapters/native-application.js',
    'js/adapters/native-guide-desktop.js', 'js/adapters/native-guide-ui.js'
  ]);
  for (const file of needed) {
    const selected = actual.filter(script => script.src === file); assert.equal(selected.length, 1, file);
    assert.equal(selected[0].tag, '<script src="' + file + '">', file + ' must be a synchronous classic script');
  }
  const at = file => actual.findIndex(script => script.src === 'js/adapters/' + file + '.js');
  assert(at('native-guide-contract') < at('native-application'));
  assert(at('native-guide-desktop') < at('native-guide-ui'));
  for (const script of actual)
    vm.runInContext(fs.readFileSync(require.resolve('../../src/' + script.src), 'utf8'), context, { filename: script.src });
  browserRoot = vm.runInContext('globalThis', context);
  function project() { root.NemoNativeObjectProjectFile.projectView(browserRoot, info); }
  project(); controller = root.NemoNativeGuideController; document.fire('DOMContentLoaded');
  function tick() { const frame = frames.shift(); if (frame) frame(); }
  async function settled() {
    for (let i = 0; i < 30; i++) { await tail; await new Promise(r => setImmediate(r)); if (!controller.isPending()) { tick(); return; } }
    throw Error('guide caller did not settle');
  }
  return { root, document, area, h, v, controller, calls, notices, prefs, tick, settled,
    changeView(value) { affine = value; viewGeneration++; proofPin = null; },
    changeBounds(value) { rect = { ...rect, ...value }; },
    invalidatePresentation(value) { invalidProof = value; },
    changeFrame(value) { state.currentFrame = value; },
    fullInfo: () => info, projectionReady: value => root.NemoNativeObjectProjectFile.projectionReadyForUI(browserRoot, value),
    drift() { state.canvasW++; }, repairProjection: project,
    async ready() { await controller.presented(); await settled(); assert(controller.read(), 'Actual production guide SDK admission/presentation is required'); },
    edits: () => calls.filter(q => q.operation === C.EDIT), metadata: () => clone(metadata),
    hits: () => walk(roots).filter(n => /^guide-hit /.test(n.className || '')),
    ghosts: () => walk(roots).filter(n => n.style.opacity === '.6'),
    replace() { info = { ...info, documentId: 'replacement', lifecycleGeneration: 2 }; controller.willProject(info); project(); },
    resubscribe() { ticket++; }, retire() { active = false; controller.dispose(); tick(); },
    hold() { let entered, release; const started = new Promise(r => entered = r), wait = new Promise(r => release = r);
      held = { entered, wait }; return { started, release }; }
  };
}

module.exports = { fixture };
