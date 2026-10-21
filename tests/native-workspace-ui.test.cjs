'use strict';
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), vm = require('node:vm');
const C = require('../src/js/adapters/native-workspace-contract.js');
const copy = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
function node(id = '') {
  const listeners = new Map(), names = new Set();
  const el = { id, style: {}, dataset: {}, children: [], parentNode: null, offsetWidth: 280, offsetHeight: 180,
    addEventListener(kind, fn) { if (!listeners.has(kind)) listeners.set(kind, []); listeners.get(kind).push(fn); },
    removeEventListener(kind, fn) { listeners.set(kind, (listeners.get(kind) || []).filter(f => f !== fn)); },
    fire(type, extra = {}) { const event = { type, target: el, clientX: 0, clientY: 0, preventDefault() {}, stopPropagation() {}, ...extra };
      for (const fn of [...(listeners.get(type) || [])]) fn.call(el, event); return event; },
    listenerCount() { return [...listeners.values()].reduce((n, list) => n + list.length, 0); },
    appendChild(child) { return this.insertBefore(child, null); },
    insertBefore(child, next) { if (child.parentNode) child.parentNode.children.splice(child.parentNode.children.indexOf(child), 1);
      const i = next ? this.children.indexOf(next) : -1; this.children.splice(i < 0 ? this.children.length : i, 0, child); child.parentNode = this; return child; },
    getBoundingClientRect() { return { left: 500, right: 900, top: 0, bottom: 400, width: 400, height: 400 }; },
    querySelectorAll(selector) { return this.children.filter(n => n.classList.contains('psec') && (!selector.includes('not') || !n.classList.contains('floating'))); },
    querySelector(selector) { return selector === '.phdr' ? this.header : null; },
    closest(selector) { return selector === '.phdr' && this.isHeader ? this : selector === '.psec' && names.has('psec') ? this : null; }
  };
  el.classList = { add(...values) { values.forEach(v => names.add(v)); }, remove(...values) { values.forEach(v => names.delete(v)); },
    contains: value => names.has(value), toggle(value, flag) { if (flag === undefined) flag = !names.has(value); if (flag) names.add(value); else names.delete(value); return flag; } };
  Object.defineProperty(el, 'nextSibling', { get() { const p = this.parentNode; return p && p.children[p.children.indexOf(this) + 1] || null; } });
  Object.defineProperty(el, 'parentElement', { get() { return this.parentNode; } });
  return el;
}
function actualOrder() {
  const wanted = ['adapters/native-workspace-ui.js', 'ui.js', 'timeline.js', 'motion.js',
    'adapters/native-workspace-contract.js', 'adapters/native-application.js', 'adapters/native-workspace-desktop.js'];
  const html = fs.readFileSync(require.resolve('../src/index.html'), 'utf8');
  const found = [...html.matchAll(/<script\b[^>]*src="js\/([^"\n]+)"[^>]*>/g)].map(m => m[1]).filter(x => wanted.includes(x));
  assert.deepEqual(found, wanted); return found;
}
// Controlled native metadata, resource-queue, DOM and file ports. These are not installed/native acceptance receipts.
async function fixture(initial, browser = false) {
  const ids = ['tl-resize', 'timeline-area', 'layer-panel-resize', 'layer-panel', 'tools-panel-resize', 'tools-panel', 'props-panel-resize', 'props-panel', 'props-panel-collapse-btn'];
  const elements = Object.fromEntries(ids.map(id => [id, node(id)])), events = node('window'), doc = node('document'), body = node('body');
  elements['layer-panel'].style.width = '180px'; elements['tools-panel'].style.width = '70px';
  elements['props-panel'].style.width = '300px'; elements['timeline-area'].style.height = '180px';
  for (const id of ['effects', 'transform']) {
    const sec = node(id); sec.classList.add('psec'); sec.header = node(); sec.header.isHeader = true; sec.header.textContent = id;
    sec.appendChild(sec.header); elements[id] = sec; elements['props-panel'].appendChild(sec);
  }
  doc.body = body; ids.forEach(id => body.appendChild(elements[id])); doc.getElementById = id => elements[id] || null;
  doc.createElement = () => node(); doc.querySelectorAll = selector => Object.values(elements).filter(n => n.classList.contains('psec') &&
    (!selector.includes('drag-over') || n.classList.contains('drag-over-sec')) && (!selector.includes('floating') || n.classList.contains('floating')));
  doc.querySelector = selector => doc.querySelectorAll(selector)[0] || null; doc.elementFromPoint = () => elements.transform;
  const storage = new Map(), notices = [], calls = [], history = [copy(initial)]; let historyAt = 0, revision = 0, serial = 0, running = 0;
  let metadata = copy(initial), active = true, refused = false, ticket = 1, held = null, tail = Promise.resolve(), controller, context;
  let info = { apiVersion: 2, available: true, kind: 'native-object', instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 's0' };
  const shown = pin => ({ ...pin, presentation: { ...pin, owner: 'native', status: 'presented', frame: 0, contextId: 'scene-root', viewGeneration: 1, workId: 'work-' + pin.documentSnapshotId } });
  const root = { document: doc, localStorage: { getItem: key => storage.has(key) ? storage.get(key) : null,
    setItem: (key, value) => storage.set(key, value), removeItem: key => storage.delete(key) }, Event: function (type) { this.type = type; },
    innerHeight: 900, innerWidth: 1200, state: {}, SM: { t: key => key }, showToast: text => notices.push(text),
    addEventListener: events.addEventListener.bind(events), removeEventListener: events.removeEventListener.bind(events),
    dispatchEvent: event => events.fire(event.type, event), syncPropsPanelCollapseTitle() {}, renderPropsPanelRail() {} };
  if (!browser) root.__TAURI__ = { core: {} };
  Object.defineProperty(root.state, 'workspace', { get() { throw Error('legacy workspace read'); }, set() { throw Error('legacy workspace write'); } });
  async function dispatch(q) {
    calls.push(copy(q)); if (held) { const gate = held; held = null; await gate; }
    const response = { apiVersion: 2, requestId: q.requestId, instanceId: info.instanceId, documentId: info.documentId, contentRevision: revision, ok: true };
    if (q.documentId !== info.documentId) { response.ok = false; response.error = { code: 'wrong_document', message: 'replaced' }; }
    else if (refused) { response.ok = false; response.error = { code: 'unavailable', message: 'refused' }; }
    else if (q.operation === C.READ) response.result = { instanceId: info.instanceId, documentId: info.documentId, atRevision: revision,
      documentSnapshotId: info.documentSnapshotId, contextId: 'scene-root', result: metadata === undefined ? { workspacePresent: false } : { workspacePresent: true, workspace: metadata } };
    else {
      assert.equal(q.operation, C.EDIT); assert.equal(q.expectedRevision, revision);
      const changed = metadata === undefined || !C.equal(metadata, copy(q.payload.action.workspace));
      if (changed) { metadata = copy(q.payload.action.workspace); revision++; history.splice(historyAt + 1); history.push(copy(metadata)); historyAt++; }
      response.contentRevision = revision; response.result = { applied: changed, historyEntriesAdded: Number(changed) };
    }
    return vm.runInContext('JSON.parse(' + JSON.stringify(JSON.stringify(response)) + ')', context);
  }
  const session = { info: () => info, isActive: () => active, requestId: () => 'workspace-' + (++serial), readSubscriptionGeneration: () => ticket,
    readPresentedObjectSession: () => active && !running ? shown(info) : null,
    runResourceCommand(fn) {
      running++; const job = tail.then(() => { const origin = info;
        return fn({ dispatch: q => root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('workspace-ui', { dispatch }).dispatch(q),
          readPresentedObjectSession: () => active && running === 1 && origin === info ? shown(origin) : null,
          reconcile: async next => { assert.equal(next, revision); info = { ...info, contentRevision: revision, documentSnapshotId: 's' + revision };
            controller.willProject(info); controller.presented(info); return info; } });
      }).finally(() => { running--; if (controller) controller.presented(info); }); tail = job.catch(() => {}); return job;
    } };
  root.NemoNativeObjectProject = { desktop: () => session };
  root.NemoNativeObjectProjectFile = { projectionReadyForUI: () => active && !running };
  root.window = root; root.globalThis = root; context = vm.createContext(root);
  for (const file of actualOrder()) {
    let source = fs.readFileSync(require.resolve('../src/js/' + file), 'utf8');
    if (file === 'ui.js') {
      const start = source.indexOf("  var rh=document.getElementById('tl-resize')"), end = source.indexOf('\n\n  // No local FC', start);
      const dragStart = source.indexOf('  var secDrag='), dragEnd = source.indexOf('  // Rive-style scrubbable', dragStart);
      source = source.slice(start, end) + '\n' + source.slice(dragStart, dragEnd);
    } else if (file === 'timeline.js') {
      const start = source.indexOf('function togglePropsPanelCollapse(force){'), end = source.indexOf('\n// STATE-dependent', start);
      const register = source.match(/  if\(btn\)btn\.addEventListener\('click',function\(\)\{togglePropsPanelCollapse\(\);\}\);/);
      assert(register, 'use the actual original collapse-button callback');
      source = source.slice(start, end) + "\nvar btn=document.getElementById('props-panel-collapse-btn');\n" + register[0];
    } else if (file === 'motion.js') {
      const start = source.indexOf('  window.SMWorkspace.bindMotion({'), end = source.indexOf('  function propHasContent', start);
      const colStart = source.indexOf('        var column=NemoNativeWorkspaceUI.motionColumns('), colEnd = source.indexOf('        var snapRow', colStart);
      source = "var _motionColumnPreset='3d',panel=document.getElementById('layer-panel'),pop=document.createElement('div');\n" + source.slice(start, end) + source.slice(colStart, colEnd) + '\nwindow.fixtureColumn=column;';
    }
    vm.runInContext(source, context, { filename: file });
  }
  doc.fire('DOMContentLoaded'); controller = root.NemoNativeWorkspaceController;
  async function settled() { for (let i = 0; i < 25; i++) { await tail; await new Promise(resolve => setImmediate(resolve));
    if (!running) return; } throw Error('workspace queue did not settle'); }
  if (!browser) await settled();
  return { root, elements, events, doc, storage, calls, notices, session, controller, settled, metadata: () => copy(metadata),
    edits: () => calls.filter(q => q.operation === C.EDIT), checkpoints: () => history.length - 1,
    resize(kind, move = true) { const handle = elements[{ layer: 'layer-panel-resize', tools: 'tools-panel-resize', props: 'props-panel-resize', timeline: 'tl-resize' }[kind]];
      handle.fire('mousedown', { clientX: 100, clientY: 200 }); if (move) events.fire('mousemove', { clientX: 125, clientY: 170 }); events.fire('mouseup'); },
    refuse() { refused = true; }, hold() { let resolve; held = new Promise(r => resolve = r); return resolve; },
    replace(value) { metadata = copy(value); revision++; info = { ...info, documentId: 'other', lifecycleGeneration: info.lifecycleGeneration + 1,
      contentRevision: revision, documentSnapshotId: 's' + revision }; controller.willProject(info); controller.presented(info); },
    history(delta) { historyAt += delta; metadata = copy(history[historyAt]); revision++; info = { ...info, contentRevision: revision, documentSnapshotId: 's' + revision };
      controller.willProject(info); controller.presented(info); }, resubscribe() { ticket++; },
    unavailable() { active = false; }, column: root.fixtureColumn };
}
test('actual four resize callbacks capture at mouseup only; stationary mouseup never materializes absent workspace', async () => {
  for (const kind of ['timeline', 'layer', 'tools', 'props']) {
    const f = await fixture(); f.resize(kind, false); await f.settled(); assert.equal(f.edits().length, 0); assert.equal(f.metadata(), undefined);
    const handle = f.elements[{ timeline: 'tl-resize', layer: 'layer-panel-resize', tools: 'tools-panel-resize', props: 'props-panel-resize' }[kind]];
    handle.fire('mousedown', { clientX: 100, clientY: 200 }); f.events.fire('mousemove', { clientX: 125, clientY: 170 });
    assert.equal(f.edits().length, 0, 'mousemove has no workspace command'); f.events.fire('mouseup');
    await f.settled(); assert.equal(f.edits().length, 1, kind); assert.equal(f.checkpoints(), 1);
    assert.equal(f.metadata().propsPanelCollapsed, false); assert(Array.isArray(f.metadata().panelOrder));
  }
});
test('actual changed layout rolls back while pending then applies only confirmed native projection', async () => {
  const f = await fixture(), before = f.elements['layer-panel'].style.width, release = f.hold(); f.resize('layer');
  assert.equal(f.elements['layer-panel'].style.width, before); release(); await f.settled();
  assert.equal(f.elements['layer-panel'].style.width, f.metadata().layerPanelWidth); assert.equal(f.edits().length, 1);
  f.resize('layer', false); await f.settled(); assert.equal(f.edits().length, 1);
});
test('actual collapse and motion columns callbacks use the same native capture owner and restore suppression', async () => {
  const f = await fixture(); f.elements['props-panel-collapse-btn'].fire('click'); await f.settled();
  assert.equal(f.metadata().propsPanelCollapsed, true); f.column.select.value = 'compact'; f.column.select.fire('change'); await f.settled();
  assert.equal(f.metadata().motionColumns, 'compact'); assert.equal(f.elements['layer-panel'].dataset.motionColumns, 'compact');
  assert.equal(f.edits().length, 2); const count = f.edits().length;
  f.replace({ propsPanelCollapsed: false, motionColumns: 'unknown imported spelling', layerPanelWidth: '123.50px' }); await f.settled();
  assert.equal(f.elements['props-panel'].classList.contains('collapsed'), false);
  assert.equal(f.elements['layer-panel'].dataset.motionColumns, 'unknown imported spelling'); assert.equal(f.column.select.value, 'unknown imported spelling');
  assert.equal(f.edits().length, count, 'open/restore never capture');
});
test('actual docked section drop commits once and canceled floating drag cannot capture', async () => {
  const f = await fixture(), header = f.elements.effects.header;
  f.doc.fire('mousedown', { target: header, clientX: 520, clientY: 10 }); f.events.fire('mousemove', { clientX: 520, clientY: 220 });
  assert.equal(f.edits().length, 0); f.events.fire('mouseup', { clientX: 520, clientY: 220 }); await f.settled();
  assert.deepEqual(f.metadata().panelOrder, ['transform', 'effects']); assert.equal(f.edits().length, 1);
  f.doc.fire('mousedown', { target: header, clientX: 520, clientY: 10 }); f.events.fire('mousemove', { clientX: 20, clientY: 50 });
  f.events.fire('keydown', { key: 'Escape' }); f.events.fire('mouseup', { clientX: 20, clientY: 50 }); await f.settled(); assert.equal(f.edits().length, 1);
});
test('unavailable read, rejected edit, stale ticket and replacement cannot become absent or rewrite a project', async () => {
  const f = await fixture({ layerPanelWidth: '145px', panelOrder: null }); assert.equal(f.elements['layer-panel'].style.width, '145px');
  f.refuse(); f.resize('layer'); await f.settled(); assert.equal(f.metadata().layerPanelWidth, '145px'); assert.equal(f.checkpoints(), 0);
  const g = await fixture(); g.elements['layer-panel-resize'].fire('mousedown'); g.events.fire('mousemove', { clientX: 20 }); g.resubscribe();
  g.events.fire('mouseup'); await g.settled(); assert.equal(g.edits().length, 0);
  const h = await fixture(); h.elements['layer-panel-resize'].fire('mousedown'); h.events.fire('mousemove', { clientX: 20 }); h.replace({ layerPanelWidth: '99px' });
  h.events.fire('mouseup'); await h.settled(); assert.equal(h.edits().length, 0); assert.equal(h.elements['layer-panel'].style.width, '99px');
  const u = await fixture(); u.unavailable(); u.resize('layer'); await u.settled(); assert.equal(u.edits().length, 0);
  assert.equal(u.root.SMWorkspace.apply({ layerPanelWidth: '1px' }), false); assert.equal(u.elements['layer-panel'].style.width, '180px');
});
test('undo to absent preserves the original current-layout fallback without capture; redo projects only native data', async () => {
  const f = await fixture(); f.resize('layer'); await f.settled(); const width = f.elements['layer-panel'].style.width;
  f.history(-1); await f.settled(); assert.equal(f.metadata(), undefined); assert.equal(f.elements['layer-panel'].style.width, width); assert.equal(f.edits().length, 1);
  f.history(1); await f.settled(); assert.equal(f.elements['layer-panel'].style.width, f.metadata().layerPanelWidth); assert.equal(f.edits().length, 1);
});
test('cancel and disposal roll back transient geometry and remove native listeners/picker handlers', async () => {
  for (const kind of ['blur', 'pointercancel', 'mouseleave']) {
    const f = await fixture(); f.elements['tools-panel-resize'].fire('mousedown'); f.events.fire('mousemove', { clientX: 100 }); f.events.fire(kind);
    f.events.fire('mouseup'); await f.settled(); assert.equal(f.edits().length, 0); assert.equal(f.elements['tools-panel'].style.width, '70px');
    const before = f.events.listenerCount(); f.controller.dispose(); assert(f.events.listenerCount() < before); assert.equal(f.column.select.listenerCount(), 0);
  }
});
test('browser capture/apply retains original raw storage presentation semantics', async () => {
  const f = await fixture(undefined, true); f.resize('layer'); assert.equal(f.edits().length, 0);
  assert.equal(f.root.SMWorkspace.apply({ layerPanelWidth: '111px', timelineHeight: null, propsPanelCollapsed: false, motionColumns: '[]' }), true);
  assert.equal(f.elements['layer-panel'].style.width, '111px'); assert.equal(f.elements['timeline-area'].style.height, '180px');
  assert.equal(f.root.SMWorkspace.capture().motionColumns, '[]');
});
test('actual Save As keeps its acquired authoritative pin and cancel/failure add no workspace edit', async () => {
  const Files = require('../src/js/adapters/native-object-project-file.js');
  const Workflow = require('../src/js/application/native-object-save-as-workflow.js');
  for (const mode of ['cancel', 'picker-failure', 'save-failure', 'saved']) {
    const f = await fixture(), log = [], pin = f.session.info();
    f.root.SMWorkspace = { capture() { throw Error('Save must not capture a workspace'); } };
    const session = { ...f.session, blocksLegacy: () => true,
      acquirePin: async request => { log.push('pin'); return { apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId,
        documentId: pin.documentId, contentRevision: pin.contentRevision, ok: true, result: { atRevision: pin.contentRevision,
          documentSnapshotId: pin.documentSnapshotId, documentDigest: 'a'.repeat(64), byteCount: 99 } }; },
      savePinned: async request => { log.push('save'); assert.equal(request.documentSnapshotId, pin.documentSnapshotId);
        if (mode === 'save-failure') throw Error('controlled disk failure'); const { destinationPath, ...receipt } = request; return { ...receipt, byteCount: 99, published: true, cleanupPending: false }; } };
    const acknowledgement = { create: () => ({ begin: () => ({}), isCurrent: () => true,
      acknowledge: async (ticket, result) => ({ status: result.written.status, acknowledged: false }) }) };
    const file = Files.create(session, { readUI: () => ({}), commit: () => true, repaint() {}, notice() {},
      pick: async () => { log.push('picker'); if (mode === 'picker-failure') throw Error('controlled picker failure'); return mode === 'cancel' ? null : '/private/tmp/workspace-controlled-save.json'; } }, Workflow, acknowledgement);
    const result = await file.saveAs(); assert.equal(log[0], 'pin'); assert.equal(log[1], 'picker');
    assert.equal(result.status, { cancel: 'cancelled', 'picker-failure': 'refused', 'save-failure': 'indeterminate', saved: 'published-current' }[mode]);
    assert.equal(f.edits().length, 0); assert.equal(f.checkpoints(), 0); assert.equal(f.session.info().contentRevision, pin.contentRevision);
  }
});
test('oversized native record is unavailable rather than absent, and held old-owner completion cannot repaint replacement', async () => {
  const large = await fixture({ motionColumns: 'x'.repeat(4096) });
  assert.equal(large.controller.read(), null); large.resize('layer'); await large.settled(); assert.equal(large.edits().length, 0);
  const f = await fixture(), release = f.hold(); f.resize('layer'); await Promise.resolve();
  f.replace({ layerPanelWidth: '88px', motionColumns: 'data:raw saved value' }); release(); await f.settled();
  assert.equal(f.elements['layer-panel'].style.width, '88px'); assert.equal(f.metadata().layerPanelWidth, '88px');
  assert.equal(f.elements['layer-panel'].dataset.motionColumns, 'data:raw saved value'); assert.equal(f.checkpoints(), 0);
});

test('ordinary outside section drop retains transient floating presentation without a workspace command', async () => {
  const f = await fixture(), section = f.elements.effects;
  f.doc.fire('mousedown', { target: section.header, clientX: 520, clientY: 10 });
  f.events.fire('mousemove', { clientX: 20, clientY: 50 }); f.events.fire('mouseup', { clientX: 20, clientY: 50 }); await f.settled();
  assert.equal(section.classList.contains('floating'), true); assert.equal(f.edits().length, 0); assert.equal(f.metadata(), undefined);
});

test('replacement with truly absent workspace retires the old transient drag before preserving layout fallback', async () => {
  const f = await fixture(); f.elements['layer-panel-resize'].fire('mousedown'); f.events.fire('mousemove', { clientX: 50 });
  f.replace(undefined); f.events.fire('mouseup'); await f.settled();
  assert.equal(f.metadata(), undefined); assert.equal(f.elements['layer-panel'].style.width, '180px'); assert.equal(f.edits().length, 0);
});
