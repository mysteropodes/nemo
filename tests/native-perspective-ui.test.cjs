'use strict';
const ProjectFile = require('../src/js/adapters/native-object-project-file.js');
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), vm = require('node:vm');
const C = require('../src/js/adapters/native-perspective-contract.js');
const clone = value => JSON.parse(JSON.stringify(value));
function node(tag = 'div') {
  const listeners = new Map(); let text = '';
  const el = { children: [], parentNode: null, className: '', title: '', disabled: false, dataset: {},
    style: { setProperty(key, value) { this[key] = value; } }, offsetWidth: 100, offsetHeight: 40,
    setAttribute(key, value) { this[key] = value; },
    appendChild(child) { child.parentNode = this; this.children.push(child); return child; },
    contains(child) { return this === child || this.children.some(n => n.contains(child)); },
    addEventListener(kind, action, options) {
      if (!listeners.has(kind)) listeners.set(kind, []);
      listeners.get(kind).push({ action, capture: options === true || !!options && options.capture === true });
    },
    removeEventListener(kind, action) { if (listeners.has(kind)) listeners.set(kind, listeners.get(kind).filter(r => r.action !== action)); },
    listenerCount() { return [...listeners.values()].reduce((n, records) => n + records.length, 0); },
    fire(kind, extra = {}) {
      const event = { target: el, currentTarget: el, pointerId: 1, button: 0, clientX: 0, clientY: 0, key: '', stopped: false,
        preventDefault() {}, stopImmediatePropagation() { this.stopped = true; }, ...extra };
      for (const record of [...(listeners.get(kind) || [])].sort((a, b) => Number(b.capture) - Number(a.capture))) {
        record.action.call(el, event); if (event.stopped) break;
      }
      return event;
    }
  };
  el.classList = { contains: name => el.className.split(' ').includes(name), toggle(name, enabled) {
    const names = new Set(el.className.split(' ').filter(Boolean)); const on = enabled === undefined ? !names.has(name) : enabled;
    if (on) names.add(name); else names.delete(name); el.className = [...names].join(' ');
  } };
  Object.defineProperty(el, 'textContent', { get: () => text, set(value) { text = value; el.children = []; } });
  Object.defineProperty(el, 'innerHTML', { get: () => text, set(value) { el.textContent = value; } });
  return el;
}
function actualOrder() {
  const wanted = ['adapters/labs-floating-ui.js', 'labs/labs-float-panel.js', 'perspective-bridge.js',
    'adapters/native-palette-contract.js', 'adapters/native-guide-contract.js', 'adapters/native-shadow-contract.js',
    'adapters/native-perspective-contract.js', 'adapters/native-application.js',
    'adapters/native-perspective-desktop.js', 'adapters/native-perspective-ui.js'];
  const html = fs.readFileSync(require.resolve('../src/index.html'), 'utf8');
  const actual = [...html.matchAll(/<script\b[^>]*\bsrc="js\/([^"\n]+)"[^>]*>/g)].map(m => m[1]).filter(file => wanted.includes(file));
  assert.deepEqual(actual, wanted, 'exercise the actual coherent Guide + Shadow + Perspective HTML order'); return actual;
}
// Controlled ports exercise the real existing Labs callbacks, bridge, SDK and new use case.
// Their metadata/history/defaults are fixtures, not native or installed acceptance evidence.
function fixture(options = {}) {
  const names = ['canvas-area', 'drawing-canvas', 'labs-float-panel', 'labs-float-icons', 'labs-float-params'];
  const elements = Object.fromEntries(names.map(name => [name, node()])), events = node('window');
  let rect = { left: 10, top: 20, width: 640, height: 360 };
  elements['drawing-canvas'].getBoundingClientRect = () => rect;
  elements['canvas-area'].getBoundingClientRect = () => rect;
  const document = Object.assign(node('document'), { readyState: 'loading', createElement: node,
    getElementById: id => elements[id] || null });
  const publishedBounds = { '--nemo-native-left': '10px', '--nemo-native-top': '20px', '--nemo-native-right': '650px', '--nemo-native-bottom': '380px' };
  document.documentElement = { clientWidth: 1200, clientHeight: 800, classList: { contains: name => name === 'nemo-native-underlay' },
    style: { getPropertyValue: key => publishedBounds[key] } };
  let info = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 's0',
    canvasW: 320, canvasH: 180, fps: 24, totalFrames: 40, layers: [{ layerUid: 'drawing', visible: true }] };
  // Keep the original consumer's legacy-dimension tripwires. The real file
  // projection is a controlled UI port, independent of those forbidden getters.
  const projectionRoot = { state: {} };
  function project() { ProjectFile.projectView(projectionRoot, info); }
  project();
  let metadata = clone(options.metadata === undefined ? { perspectiveEnabled: true, perspectiveMode: '2pt', perspectiveDensity: -0.125,
    perspectiveVPs: [{ x: 60, y: 60 }, { x: 260, y: 60, locked: true }] } : options.metadata);
  let revision = 0, ticket = 1, serial = 0, running = 0, active = true, unavailable = false;
  let queueHidden = false, block = null, corrupt = null, tail = Promise.resolve(), context, controller;
  const calls = [], notices = [], history = [clone(metadata)]; let historyIndex = 0, suspends = 0, resumes = 0;
  const projected = pin => ({ ...pin, presentation: { ...pin, owner: 'native', status: 'presented' } });
  function defaultPoints(mode) {
    const w = info.canvasW, h = info.canvasH, cx = w / 2, cy = h / 2;
    if (mode === '1pt' || mode === 'fisheye') return [{ x: cx, y: cy, locked: false }];
    if (mode === '3pt') return [{ x: cx - w * .6, y: cy - h * .15, locked: false },
      { x: cx + w * .6, y: cy - h * .15, locked: false }, { x: cx, y: cy + h * 1.3, locked: false }];
    return [{ x: cx - w * .65, y: cy, locked: false }, { x: cx + w * .65, y: cy, locked: false }];
  }
  function effective() { return metadata.perspectiveVPs && metadata.perspectiveVPs.length ? metadata.perspectiveVPs : defaultPoints(metadata.perspectiveMode); }
  function read(selector) {
    const raw = metadata.perspectiveVPs, items = effective(), stored = !!(raw && raw.length);
    const counts = { perspectiveVPsPresent: Object.hasOwn(metadata, 'perspectiveVPs'), perspectiveVPsNull: raw === null,
      perspectiveVPCount: raw ? raw.length : 0, effectiveVPCount: items.length, vpSource: stored ? 'stored' : 'default' };
    if (selector.kind === 'settings') return { ...counts, ...Object.fromEntries(['perspectiveEnabled', 'perspectiveMode', 'perspectiveDensity']
      .filter(k => Object.hasOwn(metadata, k)).map(k => [k, metadata[k]])) };
    const end = Math.min(selector.cursor + selector.limit, items.length);
    return { ...counts, kind: 'perspectiveVPPage', cursor: selector.cursor,
      items: items.slice(selector.cursor, end).map((vp, i) => ({ ordinal: selector.cursor + i + 1, ...vp })), nextCursor: end < items.length ? end : null };
  }
  function materialize() { if (!metadata.perspectiveVPs || !metadata.perspectiveVPs.length) metadata.perspectiveVPs = defaultPoints(metadata.perspectiveMode); }
  function apply(action) {
    switch (action.kind) {
      case 'setEnabled': metadata.perspectiveEnabled = action.enabled; if (action.enabled) materialize(); break;
      case 'setMode': metadata.perspectiveMode = action.mode; metadata.perspectiveVPs = defaultPoints(action.mode); break;
      case 'reset': metadata.perspectiveVPs = defaultPoints(metadata.perspectiveMode); break;
      case 'lockAll': materialize(); metadata.perspectiveVPs.forEach(vp => vp.locked = action.locked); break;
      case 'move': materialize(); { const vp = metadata.perspectiveVPs[action.ordinal - 1]; if (!vp.locked) { vp.x = action.x; vp.y = action.y; } } break;
      case 'translate': materialize(); metadata.perspectiveVPs.forEach(vp => { if (!vp.locked) { vp.x += action.dx; vp.y += action.dy; } }); break;
      default: throw Error('unexpected Perspective UI intent');
    }
  }
  async function dispatch(request) {
    calls.push(clone(request));
    if (block) { const held = block; block = null; held.entered(); await held.wait; }
    const response = { apiVersion: 2, requestId: request.requestId, instanceId: info.instanceId, documentId: info.documentId, contentRevision: revision, ok: true };
    if (request.documentId !== info.documentId) { response.ok = false; response.error = { code: 'wrong_document', message: 'Replaced' }; }
    else if (unavailable) { response.ok = false; response.error = { code: 'unavailable', message: 'Unavailable' }; }
    else if (request.operation === C.EDIT) {
      assert.equal(request.expectedRevision, revision); const before = JSON.stringify(metadata); apply(request.payload.action);
      const applied = before !== JSON.stringify(metadata); if (applied) { revision++; history.splice(historyIndex + 1); history.push(clone(metadata)); historyIndex++; }
      response.contentRevision = revision; response.result = { applied, historyEntriesAdded: Number(applied) };
    } else {
      assert.equal(request.operation, C.READ); assert.equal(request.payload.atRevision, revision);
      response.result = { instanceId: info.instanceId, documentId: info.documentId, atRevision: revision,
        documentSnapshotId: 's' + revision, contextId: 'scene-root', result: read(request.payload.selector) };
    }
    if (corrupt) { const fn = corrupt; corrupt = null; fn(response); }
    return vm.runInContext('JSON.parse(' + JSON.stringify(JSON.stringify(response)) + ')', context);
  }
  const session = { info: () => info, isActive: () => active, requestId: () => 'perspective-' + (++serial),
    readSubscriptionGeneration: () => ticket, readPresentedObjectSession: refused => {
      if (queueHidden) { if (typeof refused === 'function') refused('queue-pending'); return null; }
      return active && !running ? projected(info) : null; },
    runResourceCommand(action) {
      running++;
      const work = tail.then(() => { const origin = info;
        return action({ dispatch: q => root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('ui', { dispatch }).dispatch(q),
          readPresentedObjectSession: () => active && running === 1 && info === origin ? projected(origin) : null,
          async reconcile(next) { assert.equal(next, revision); info = { ...info, contentRevision: revision, documentSnapshotId: 's' + revision };
            controller.willProject(info); assert.equal(ProjectFile.refreshLayerTable(projectionRoot, info), true);
            controller.presented(info); return info; } });
      }).finally(() => { running--; controller.presented(info); });
      tail = work.catch(() => {}); return work;
    }
  };
  const state = { tool: 'draw', currentFrame: 0, playing: false };
  for (const key of ['perspectiveEnabled', 'perspectiveMode', 'perspectiveDensity', 'perspectiveVPs', 'canvasW', 'canvasH'])
    Object.defineProperty(state, key, { get() { throw Error('legacy persistent read ' + key); }, set() { throw Error('legacy persistent write ' + key); } });
  const root = { document, state, __TAURI__: { core: {} }, view: { zoom: 2 }, Point: function (x, y) { this.x = x; this.y = y; },
    SM: { t: key => key }, SMLabs: { list: () => [], toggle() { throw Error('wrong Labs writer'); } },
    NemoNativeObjectProject: { desktop: () => session },
    NemoNativeObjectProjectFile: { projectionReadyForUI: (_root, value) => active && !running && ProjectFile.projectionReadyForUI(projectionRoot, value) },
    localStorage: { getItem: () => null, setItem() {} }, setTimeout, clearTimeout,
    addEventListener: events.addEventListener.bind(events), removeEventListener: events.removeEventListener.bind(events),
    showToast: text => notices.push(text), SMEngineBridge: { isEnabled: () => true, renderNow() {}, suspend() { suspends++; }, resume() { resumes++; },
      screenToWorld() { throw Error('legacy viewport mapping'); } }, pushUndo() { throw Error('legacy history'); } };
  root.window = root; root.globalThis = root; context = vm.createContext(root);
  if (options.wait) { root.setTimeout = (fn, ms) => ms ? setImmediate(fn) : setTimeout(fn, 0); vm.runInContext(fs.readFileSync(require.resolve('../src/js/adapters/native-presentation-wait.js'), 'utf8'), context); }
  for (const file of actualOrder()) vm.runInContext(fs.readFileSync(require.resolve('../src/js/' + file), 'utf8'), context, { filename: file });
  controller = root.NemoNativePerspectiveController; document.fire('DOMContentLoaded');
  async function settled() {
    for (let i = 0; i < 30; i++) { await tail; await new Promise(r => setImmediate(r)); if (!controller.isPending()) return; }
    throw Error('Perspective controller did not settle');
  }
  function screen(x, y) { return { clientX: rect.left + x * rect.width / info.canvasW, clientY: rect.top + y * rect.height / info.canvasH }; }
  return { root, elements, controller, events, document, calls, notices, settled, screen,
    ready: async () => { await controller.presented(); await settled(); },
    queue: action => session.runResourceCommand(action),
    metadata: () => clone(metadata), edits: () => calls.filter(q => q.operation === C.EDIT), historyEntries: () => history.length - 1,
    engine: () => ({ suspends, resumes }), points: () => clone(root.ensurePerspectiveVPs()),
    toggle: () => elements['labs-float-icons'].children.find(n => n.title === 'lfsShortPerspective'),
    modeNext: () => elements['labs-float-params'].children[0].children.at(-1),
    modeLabel: () => elements['labs-float-params'].children[0].children[3],
    lock: () => elements['labs-float-params'].children[1].children[1],
    reset: () => elements['labs-float-params'].children[2].children[1],
    pointer(kind, x, y, extra = {}) { return elements['canvas-area'].fire(kind, { ...screen(x, y), ...extra }); },
    hide(value) { queueHidden = value; }, refuse() { unavailable = true; }, resubscribe() { ticket++; }, resize() { rect = { ...rect, left: rect.left + 1 }; },
    corrupt(fn) { corrupt = fn; },
    fullInfo: () => info, projectionReady: value => ProjectFile.projectionReadyForUI(projectionRoot, value),
    drift() { projectionRoot.state.canvasW++; }, repairProjection: project,
    replace(saved = metadata) { metadata = clone(saved); revision++; info = { ...info, documentId: 'reopened-' + revision,
      lifecycleGeneration: info.lifecycleGeneration + 1, contentRevision: revision, documentSnapshotId: 's' + revision };
      controller.willProject(info); project(); controller.presented(info); },
    history(delta) { historyIndex += delta; metadata = clone(history[historyIndex]); revision++;
      info = { ...info, contentRevision: revision, documentSnapshotId: 's' + revision }; controller.willProject(info);
      assert.equal(ProjectFile.refreshLayerTable(projectionRoot, info), true); controller.presented(info); },
    retire() { active = false; controller.dispose(); root.disposePerspectiveBridge(); },
    hold() { let entered, release; const started = new Promise(r => entered = r), wait = new Promise(r => release = r);
      block = { entered, wait }; return { started, release }; }
  };
}
test('actual Labs controls and snap/render getters use native projection without legacy persistent access', async () => {
  const h = fixture(), before = h.metadata(); await h.ready();
  assert.equal(h.toggle().classList.contains('active'), true); assert.equal(h.modeLabel().textContent, 'lfsPersp2pt');
  assert.deepEqual(h.points(), before.perspectiveVPs.map((vp, i) => ({ ordinal: i + 1, ...vp })));
  assert(h.root.buildPerspectiveGuideItems().length > 0); h.root.perspectiveSnapPoint(new h.root.Point(20, 20), new h.root.Point(22, 50));
  assert.deepEqual(h.metadata(), before); assert.equal(h.edits().length, 0);
  assert(Object.isFrozen(h.root.ensurePerspectiveVPs())); assert(Object.isFrozen(h.root.ensurePerspectiveVPs()[0]));
});
test('real mode, lock, reset and toggle callbacks publish only confirmed values and one native command each', async () => {
  const h = fixture(); await h.ready(); const oldLabel = h.modeLabel(), held = h.hold(); h.modeNext().fire('click'); await held.started;
  assert.equal(oldLabel.textContent, 'lfsPersp2pt'); assert.equal(h.historyEntries(), 0); assert.equal(h.toggle().disabled, true);
  held.release(); await h.settled(); assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'setMode', mode: '3pt' });
  assert.equal(h.metadata().perspectiveVPs.length, 3); h.lock().fire('click'); await h.settled();
  assert(h.metadata().perspectiveVPs.every(vp => vp.locked)); assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'lockAll', locked: true });
  h.reset().fire('click'); await h.settled(); assert(h.metadata().perspectiveVPs.every(vp => vp.locked === false));
  h.toggle().fire('click'); await h.settled(); assert.equal(h.metadata().perspectiveEnabled, false);
  assert.equal(h.edits().length, 4); assert.equal(h.metadata().perspectiveDensity, -0.125);
});
test('native missing/null/empty defaults remain read-only until the original enable action materializes them', async () => {
  for (const metadata of [{}, { perspectiveVPs: null }, { perspectiveVPs: [] }]) {
    const h = fixture({ metadata }); await h.ready(); assert.equal(h.points().length, 2); assert.deepEqual(h.metadata(), metadata);
    assert.equal(h.controller.read().perspective.settings.vpSource, 'default'); h.toggle().fire('click'); await h.settled();
    assert.equal(h.metadata().perspectiveEnabled, true); assert.equal(h.metadata().perspectiveVPs.length, 2); assert.equal(h.historyEntries(), 1);
  }
});
test('actual VP drag uses a transient ghost and submits one absolute native move only on release', async () => {
  const h = fixture(); await h.ready(); const before = h.metadata(), overlay = clone(h.root.buildPerspectiveGuideItems());
  assert.equal(h.pointer('pointerdown', 60, 60).stopped, true); h.pointer('pointermove', 80, 75);
  assert.deepEqual(h.metadata(), before); assert.deepEqual(h.points(), before.perspectiveVPs.map((vp, i) => ({ ordinal: i + 1, ...vp })));
  assert.notDeepEqual(clone(h.root.buildPerspectiveGuideItems()), overlay); assert.equal(h.edits().length, 0);
  h.pointer('pointerup', 80, 75); await h.settled(); assert.deepEqual(h.edits()[0].payload.action, { kind: 'move', ordinal: 1, x: 80, y: 75 });
  assert.equal(h.historyEntries(), 1); assert.deepEqual(h.engine(), { suspends: 1, resumes: 1 });
});
test('native child pointerleave does not enqueue a repaint during an unrelated Drawing gesture', async () => {
  const h = fixture({ metadata: {} }); await h.ready();
  const before = h.metadata(), calls = h.calls.length; let renders = 0;
  h.root.SMEngineBridge.renderNow = () => { renders++; };
  assert.equal(h.pointer('pointerdown', 40, 90).stopped, false);
  // Capturing a drawing pointer on canvas-area can retarget it away from
  // drawing-canvas. The ancestor's capture listener receives that leave.
  h.pointer('pointerleave', 40, 90, { target: h.elements['drawing-canvas'] });
  assert.equal(renders, 0, 'a legacy repaint would hide the native drawing presentation as queue-pending');
  assert.equal(h.calls.length, calls); assert.deepEqual(h.metadata(), before);
  assert.equal(h.historyEntries(), 0); assert.deepEqual(h.engine(), { suspends: 0, resumes: 0 });
});
test('native Perspective leave cancels its ghost with one controller-owned repaint', async () => {
  const h = fixture(); await h.ready(); let renders = 0;
  h.root.SMEngineBridge.renderNow = () => { renders++; };
  h.pointer('pointerdown', 60, 60); h.pointer('pointermove', 80, 75); renders = 0;
  h.pointer('pointerleave', 80, 75);
  assert.equal(renders, 1); assert.deepEqual(h.engine(), { suspends: 1, resumes: 1 });
  h.pointer('pointerup', 80, 75); await h.settled();
  assert.equal(h.edits().length, 0); assert.equal(h.historyEntries(), 0);
});
test('legacy Perspective leave retains its cursor repaint', () => {
  const canvas = node(), events = node(); let renders = 0;
  const root = { document: Object.assign(node('document'), { readyState: 'complete', getElementById: () => canvas }),
    addEventListener: events.addEventListener.bind(events), removeEventListener: events.removeEventListener.bind(events),
    SMEngineBridge: { renderNow() { renders++; } } };
  root.window = root;
  vm.runInNewContext(fs.readFileSync(require.resolve('../src/js/perspective-bridge.js'), 'utf8'), root);
  canvas.fire('pointerleave'); assert.equal(renders, 1);
});
test('whole horizon drag translates unlocked VPs and preserves omitted/true locked fields', async () => {
  const h = fixture(); await h.ready(); h.pointer('pointerdown', 160, 60); h.pointer('pointermove', 170, 70);
  const markers = h.root.buildPerspectiveGuideItems().filter(item => item.strokeWidth === 2);
  assert.equal(markers[0].segments[0].point[0], 79); assert.equal(markers[1].segments[0].point[0], 269);
  h.pointer('pointerup', 170, 70); await h.settled(); assert.deepEqual(h.edits()[0].payload.action, { kind: 'translate', dx: 10, dy: 10 });
  assert.deepEqual(h.metadata().perspectiveVPs, [{ x: 70, y: 70 }, { x: 260, y: 60, locked: true }]);
  assert.deepEqual(h.engine(), { suspends: 1, resumes: 1 });
});
test('cancel, leave, blur, stale viewport/frame/owner and retirement drop ghosts with exactly one resume', async () => {
  for (const reason of ['cancel', 'leave', 'blur', 'viewport', 'frame', 'engine', 'owner', 'retire']) {
    const h = fixture(); await h.ready(); h.pointer('pointerdown', 60, 60); h.pointer('pointermove', 80, 70);
    if (reason === 'cancel') h.pointer('pointercancel', 80, 70);
    if (reason === 'leave') h.pointer('pointerleave', 80, 70);
    if (reason === 'blur') h.events.fire('blur');
    if (reason === 'viewport') { h.resize(); h.pointer('pointermove', 80, 70); }
    if (reason === 'frame') { h.root.state.currentFrame = 1; h.pointer('pointermove', 80, 70); }
    if (reason === 'engine') { h.root.SMEngineBridge = { ...h.root.SMEngineBridge }; h.pointer('pointermove', 80, 70); }
    if (reason === 'owner') h.replace();
    if (reason === 'retire') h.retire();
    h.pointer('pointerup', 80, 70); await h.settled();
    assert.equal(h.edits().length, 0, reason); assert.deepEqual(h.engine(), { suspends: 1, resumes: 1 }, reason);
    if (reason === 'retire') assert.equal(h.elements['canvas-area'].listenerCount(), 0);
  }
});
test('retained Labs callbacks and in-flight pointer results cannot edit or publish into a replacement', async () => {
  const h = fixture(); await h.ready(); const oldMode = h.modeNext(), oldLock = h.lock(), oldReset = h.reset(), oldToggle = h.toggle();
  h.replace({ perspectiveEnabled: true, perspectiveMode: '1pt' }); await h.settled();
  for (const button of [oldMode, oldLock, oldReset, oldToggle]) button.fire('click'); await h.settled(); assert.equal(h.edits().length, 0);
  const p = h.points()[0]; h.pointer('pointerdown', p.x, p.y); const held = h.hold(); h.pointer('pointerup', p.x + 5, p.y); await held.started;
  h.replace({}); held.release(); await h.settled(); assert.deepEqual(h.metadata(), {}); assert.match(h.controller.read().info.documentId, /^reopened-/);
  assert.equal(h.edits().length, 1); assert.deepEqual(h.engine(), { suspends: 1, resumes: 1 });
});
test('refused or indeterminate release never mutates a legacy mirror or retries the native command', async () => {
  for (const uncertain of [false, true]) {
    const h = fixture(); await h.ready(); h.pointer('pointerdown', 60, 60);
    if (uncertain) h.corrupt(r => { r.result.historyEntriesAdded = 2; }); else h.refuse();
    h.pointer('pointerup', 80, 70); await h.settled(); assert.equal(h.edits().length, 1); assert.equal(h.controller.read(), null);
    assert.deepEqual(h.engine(), { suspends: 1, resumes: 1 }); assert(h.notices.length > 0);
  }
});
test('bounded pages and controlled history/save-reopen retain exact imported positions and provenance', async () => {
  const perspectiveVPs = Array.from({ length: 35 }, (_, i) => ({ x: -0.125 + i, y: i / 4, ...(i % 2 ? { locked: false } : {}) }));
  const h = fixture({ metadata: { perspectiveEnabled: true, perspectiveMode: 'imported', perspectiveDensity: -4, perspectiveVPs } }); await h.ready();
  assert.equal(h.points().length, 35); assert.deepEqual(h.calls.filter(q => q.payload.selector.kind === 'vpPage').map(q => q.payload.selector.cursor), [0, 16, 32]);
  h.reset().fire('click'); await h.settled(); const saved = h.metadata(); h.history(-1); await h.settled();
  assert.deepEqual(h.metadata().perspectiveVPs, perspectiveVPs); h.history(1); await h.settled(); h.replace(saved); await h.settled();
  assert.deepEqual(h.metadata(), saved); assert.equal(h.controller.read().perspective.settings.perspectiveMode, 'imported');
});
test('unavailable reads, subscription changes and wrong pointers never dispatch or leave an engine suspension', async () => {
  const h = fixture(); h.refuse(); await h.ready(); assert.equal(h.toggle().disabled, true); assert.equal(h.root.ensurePerspectiveVPs().length, 0);
  h.pointer('pointerdown', 60, 60); assert.equal(h.edits().length, 0); assert.deepEqual(h.engine(), { suspends: 0, resumes: 0 });
  const g = fixture(); await g.ready(); g.pointer('pointerdown', 60, 60); g.pointer('pointerup', 80, 70, { pointerId: 2 });
  assert.equal(g.edits().length, 0); assert.equal(g.engine().resumes, 0); g.resubscribe(); g.pointer('pointerup', 80, 70);
  assert.equal(g.edits().length, 0); assert.deepEqual(g.engine(), { suspends: 1, resumes: 1 });
});
test('actual browser Labs callbacks preserve original synchronous mode, lock, reset and enabled behavior', () => {
  const elements = Object.fromEntries(['canvas-area', 'drawing-canvas', 'labs-float-panel', 'labs-float-icons', 'labs-float-params'].map(n => [n, node()]));
  const document = Object.assign(node('document'), { readyState: 'loading', getElementById: id => elements[id] || null, createElement: node });
  const events = node('window'), state = { tool: 'draw', playing: false, perspectiveEnabled: true, perspectiveMode: '2pt',
    perspectiveVPs: null, perspectiveDensity: 24, canvasW: 320, canvasH: 180 };
  const root = { document, state, SM: { t: key => key }, SMLabs: { list: () => [] }, localStorage: { getItem: () => null },
    setTimeout, addEventListener: events.addEventListener.bind(events), removeEventListener: events.removeEventListener.bind(events),
    SMEngineBridge: { renderNow() {}, isEnabled: () => true }, view: { zoom: 1 } };
  root.window = root; root.globalThis = root; vm.createContext(root);
  for (const file of actualOrder()) vm.runInContext(fs.readFileSync(require.resolve('../src/js/' + file), 'utf8'), root);
  document.fire('DOMContentLoaded');
  const params = () => elements['labs-float-params'].children;
  params()[0].children.at(-1).fire('click'); assert.equal(state.perspectiveMode, '3pt'); assert.equal(state.perspectiveVPs.length, 3);
  params()[1].children[1].fire('click'); assert(state.perspectiveVPs.every(vp => vp.locked));
  params()[2].children[1].fire('click'); assert(state.perspectiveVPs.every(vp => vp.locked === false));
  elements['labs-float-icons'].children.find(n => n.title === 'lfsShortPerspective').fire('click'); assert.equal(state.perspectiveEnabled, false);
  root.disposePerspectiveBridge(); root.NemoNativePerspectiveController.dispose();
});

test('already admitted queue work uses only an issued immutable native proof and fences subscription changes', async () => {
  const h = fixture(); await h.ready(); const proof = h.controller.read();
  await h.queue(async () => {
    assert.equal(h.controller.read(), null, 'public presentation remains hidden during the actual shared queue');
    assert.strictEqual(h.controller.readForCommand(proof), proof);
    assert.equal(h.controller.readForCommand(clone(proof)), null, 'a manufactured lookalike is never a native proof');
    h.resubscribe(); assert.equal(h.controller.readForCommand(proof), null);
  });
  await h.settled();
});

test('read publication is strict, once per immutable projection, and unsubscribes without command-proof promotion', async () => {
  const h = fixture(), reads = [], remove = h.controller.onRead(value => reads.push(value));
  await h.ready(); assert.equal(reads.length, 1); await h.ready(); assert.equal(reads.length, 1);
  const proof = h.controller.read(); await h.queue(async () => {
    assert.equal(h.controller.read(), null); assert.strictEqual(h.controller.readForCommand(proof), proof);
    assert.equal(reads.length, 1);
  }); await h.settled(); assert.equal(reads.length, 1);
  h.replace({}); await h.settled(); assert.equal(reads.length, 2); remove(); h.replace({}); await h.settled(); assert.equal(reads.length, 2);
});

test('perspective uses full project readiness and retains narrow immutable authority pins', async () => {
  const f = fixture(); await f.ready();
  assert.equal(f.projectionReady(f.fullInfo()), true);
  const origin = f.controller.read(); assert.ok(origin);
  assert.deepEqual(Object.keys(origin.info).sort(),
    ['contentRevision', 'documentId', 'documentSnapshotId', 'instanceId', 'lifecycleGeneration']);
  assert.equal(Object.isFrozen(origin.info), true);
  assert.equal(f.projectionReady(origin.info), false, 'identity pins cannot stand in for project metadata');
  const calls = f.calls.length;
  f.drift(); assert.equal(f.controller.read(), null);
  assert.equal(await f.controller.presented(), null); await f.settled();
  assert.equal(f.controller.read(), null);
  assert.equal(f.calls.length, calls, 'invalid full projection refuses before dispatch');
  assert.equal(f.edits().length, 0);
  f.repairProjection(); await f.controller.presented(); await f.settled();
  assert.ok(f.controller.read());
  assert.equal(f.edits().length, 0, 'restoring readiness does not replay a gesture');
});

// #2372: a press on a vanishing point while the native queue hides the presentation (queue-pending) waits for it, then drags once.
const ticks = async (n = 8) => { for (let i = 0; i < n; i++) await new Promise(done => setImmediate(done)); };
test('queue-pending: a VP press held for several ticks drags once when the presentation returns', async () => {
  const h = fixture({ wait: true }); await h.ready(); h.root.state.tool = 'perspective'; h.hide(true);
  assert.equal(h.pointer('pointerdown', 60, 60).stopped, true, 'the press is held, not dropped'); h.pointer('pointermove', 80, 75); h.pointer('pointerup', 80, 75);
  await ticks(2); assert.equal(h.edits().length, 0); assert.deepEqual(h.notices, []);
  h.hide(false); await ticks(4); await h.settled();
  assert.deepEqual(h.edits().map(q => q.payload.action), [{ kind: 'move', ordinal: 1, x: 80, y: 75 }]); assert.equal(h.historyEntries(), 1);
  assert.deepEqual(h.engine(), { suspends: 1, resumes: 1 }); assert.deepEqual(h.notices, []);
});
test('queue-pending: a drag already under way survives the window; the release re-proves the origin before the one write', async () => {
  const h = fixture({ wait: true }); await h.ready(); h.pointer('pointerdown', 60, 60); h.hide(true); h.pointer('pointermove', 90, 70);
  assert.equal(h.engine().resumes, 0, 'not cancelled by the hidden presentation'); h.pointer('pointerup', 90, 70); h.hide(false); await ticks(); await h.settled();
  assert.deepEqual(h.edits().map(q => q.payload.action), [{ kind: 'move', ordinal: 1, x: 90, y: 70 }]);
});
test('queue-pending: a real change while the press waits drops it without a write or a notice', async () => {
  for (const change of ['tool', 'frame', 'pin']) {
    const h = fixture({ wait: true }); await h.ready(); h.root.state.tool = 'perspective'; h.hide(true);
    h.pointer('pointerdown', 60, 60); h.pointer('pointerup', 60, 60); await ticks(2);
    if (change === 'tool') h.root.state.tool = 'draw'; if (change === 'frame') h.root.state.currentFrame = 3; if (change === 'pin') h.replace();
    h.hide(false); await ticks(); await h.settled();
    assert.equal(h.edits().length, 0, change); assert.deepEqual(h.notices, [], change);
  }
});
test('queue-pending: a queue that never frees gives one notice and no write', async () => {
  const h = fixture({ wait: true }); await h.ready(); h.root.state.tool = 'perspective'; h.hide(true);
  h.pointer('pointerdown', 60, 60); h.pointer('pointerup', 60, 60); await ticks(60);
  assert.equal(h.edits().length, 0); assert.deepEqual(h.notices, ['The canvas is still busy - try again.']);
});
