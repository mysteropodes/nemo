/* #2256 Q5: the Component Instance inspector (#comp-instance-sec) over a NATIVE object project.
 * Baseline (1065e9d8, timeline.js updateCompInstancePanel + renderCompFrameStrip + the section's listeners): the section shows for the active
 * layer when it is a component instance; Playback / Frame / Speed / Offset write ld.symPlayMode / symSingleFrame / symSpeed / symPlacedAt and
 * reload the frame, a click on a cell of the frame strip picks Single Frame mode on that frame, and "Edit component" enters the component.
 * On a native project the layer row is a stub (symbolId 'x'), so those writers changed a row the next repaint discarded.
 * Here the section is shown from the native layer row (kind component, the Q3 read), filled from ONE pinned instance read
 * (query.document.object.component.read, selector instance, at the playhead frame: mode, single frame, speed, offset, the component length and the
 * frame the playback kernel resolves), and every field commits ONE command.document.object.component.instance.set naming only the members that
 * changed (a scrub previews in the field and sends its single request on release, the data-native-opacity-scrub contract of ui.js). The baseline
 * coercions are kept: single frame max(0, parseInt || 0), speed max(0.1, parseFloat || 1), offset parseInt || 0. A pick made while the playhead is
 * on an outer keyframe also names that frame (atFrame): the owner then writes the key's component frame like setSymbolSingleFrame did.
 * "Edit component" enters the real context of the row's component (the stub symbolId would name nothing). The preview window, the break-apart
 * and the lip-sync analysis have no native owner yet: they say so instead of acting on a stub row. ld.symPlayMode and its siblings, loadFrame and
 * resolveSymbolFrameIdx are never touched. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeComponentInstancePanel = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const MODES = ['loop', 'once', 'pingpong', 'single'];
  const MIN_SPEED = 0.1, I32_MIN = -2147483648, I32_MAX = 2147483647, U32_MAX = 4294967295;
  const FIELDS = Object.freeze(['comp-playmode', 'comp-singleframe', 'comp-speed', 'comp-offset']);
  const SCRUBBED = Object.freeze(['comp-singleframe', 'comp-speed', 'comp-offset']);
  // Controls with no native owner yet: said, never run against the stub row.
  const UNAVAILABLE = Object.freeze({
    'btn-comp-preview': 'The component preview window',
    'btn-comp-detach': 'Breaking a component apart',
    'btn-lip-apply': 'Lip-sync analysis',
  });
  const states = new WeakMap();

  const el = (host, id) => (host.document && typeof host.document.getElementById === 'function' ? host.document.getElementById(id) : null);
  const toast = (host, message) => { if (typeof host.showToast === 'function') host.showToast(message); };
  const text = (value) => typeof value === 'string' && value.length > 0;
  function session(host) {
    const project = host.NemoNativeObjectProject;
    const found = project && typeof project.desktop === 'function' ? project.desktop(host) : null;
    return found && typeof found.blocksLegacy === 'function' && found.blocksLegacy() === true ? found : null;
  }
  function info(host) {
    const found = session(host), pin = found && typeof found.info === 'function' ? found.info() : null;
    return pin && typeof pin === 'object' ? pin : null;
  }
  function stateOf(host) {
    if (!states.has(host)) states.set(host, { seq: 0, key: null, busy: false, again: false, installed: false, view: null });
    return states.get(host);
  }
  // The active layer's native row when it is a component instance and the only selected layer, else null. An entered component never shows it.
  function instanceRow(host) {
    const state = host.state || {}, pin = info(host), at = state.activeLayerIdx;
    if (!pin || !Array.isArray(pin.layers) || !Number.isInteger(at) || state.activeSymbolId) return null;
    if (Array.isArray(host._layerSel) && host._layerSel.length > 1) return null;
    const row = pin.layers[at];
    return row && row.kind === 'component' && text(row.layerUid) && text(row.componentId) ? row : null;
  }
  const frameNow = (host) => (host.state && Number.isSafeInteger(host.state.currentFrame) && host.state.currentFrame >= 0 ? host.state.currentFrame : 0);
  const keyOf = (revision, documentId, layerUid, frame) => [revision, documentId, layerUid, frame].join(':');
  function refusalText(outcome) {
    if (outcome && outcome.code === 'stale_selection') return 'The layer is no longer in the project.';
    const detail = outcome && (text(outcome.message) ? outcome.message : text(outcome.reason) ? outcome.reason : outcome.code);
    return 'Component playback refused' + (text(detail) ? ': ' + detail : '.');
  }

  function show(host, id, display) { const node = el(host, id); if (node && node.style) node.style.display = display; }
  function focused(host, node) { return !!host.document && host.document.activeElement === node; }
  // A field the user is typing in is not overwritten under their hands.
  function set(host, id, value) { const node = el(host, id); if (node && !focused(host, node)) node.value = String(value); }

  // renderCompFrameStrip: one cell per frame of the component's own timeline; the resolved frame is outlined, the explicit pick (an outer key naming
  // a frame, or Single Frame mode) is filled with the accent. A cell click is handled by the section's capture listener (data-native-comp-frame).
  function renderStrip(host, view) {
    const strip = el(host, 'comp-frame-strip');
    if (!strip || !host.document || typeof host.document.createElement !== 'function') return;
    strip.innerHTML = '';
    const total = Math.max(1, view.totalFrames), current = view.resolvedFrame;
    for (let i = 0; i < total; i += 1) {
      const cell = host.document.createElement('div');
      const isCur = i === current, selected = (view.explicit || view.mode === 'single') && isCur;
      cell.style.cssText = 'width:14px;height:14px;flex:0 0 auto;border-radius:2px;cursor:pointer;font-size:8px;display:flex;align-items:center;justify-content:center;';
      cell.style.background = selected ? 'var(--accent)' : (isCur ? 'rgba(255,255,255,.25)' : 'var(--bg)');
      cell.style.border = '1px solid ' + (isCur ? 'var(--accent)' : 'rgba(255,255,255,.15)');
      cell.title = 'Frame ' + i;
      if (cell.dataset) cell.dataset.nativeCompFrame = String(i);
      strip.appendChild(cell);
    }
  }

  // updateCompInstancePanel's body, over the stored facts.
  function render(host, view) {
    set(host, 'comp-playmode', view.mode);
    show(host, 'comp-singleframe-row', view.mode === 'single' ? 'flex' : 'none');
    set(host, 'comp-singleframe', view.resolvedFrame === null ? view.singleFrame : view.resolvedFrame);
    set(host, 'comp-speed', view.speed);
    set(host, 'comp-offset', view.placedAt);
    renderStrip(host, view);
  }

  // Marks the controls: scrubbable numbers commit once on release; unmigrated controls say why they do nothing.
  function mark(host, on) {
    SCRUBBED.forEach((id) => {
      const node = el(host, id);
      if (!node || !node.dataset) return;
      if (on) node.dataset.nativeOpacityScrub = 'static'; else if (node.dataset.nativeOpacityScrub === 'static') delete node.dataset.nativeOpacityScrub;
    });
    Object.keys(UNAVAILABLE).forEach((id) => {
      const node = el(host, id);
      if (!node || !node.dataset) return;
      if (on) {
        if (node.dataset.nativeUnavailable !== '1') { node.dataset.nativeUnavailable = '1'; node.dataset.nativeBaseTitle = node.title || ''; }
        node.title = UNAVAILABLE[id] + ' is not available on this project yet.';
        if (typeof node.setAttribute === 'function') node.setAttribute('aria-disabled', 'true');
      } else if (node.dataset.nativeUnavailable === '1') {
        node.title = node.dataset.nativeBaseTitle || '';
        delete node.dataset.nativeUnavailable; delete node.dataset.nativeBaseTitle;
        if (typeof node.removeAttribute === 'function') node.removeAttribute('aria-disabled');
      }
    });
  }

  // Reads the stored playback facts of `layerUid` at the playhead and paints them. Reads are coalesced: while one is in flight a newer request
  // waits and runs once after it (a scrub or playback never queues a read per tick); an answer for a panel that moved on is dropped.
  function paint(host, layerUid, frame) {
    const state = stateOf(host);
    if (state.busy) { state.again = true; return Promise.resolve(false); }
    const component = host.NemoNativeObjectComponent, adapter = component && typeof component.desktop === 'function' ? component.desktop(host) : null;
    if (!adapter || typeof adapter.read !== 'function') return Promise.resolve(false);
    const token = ++state.seq;
    state.busy = true;
    const finish = (value) => {
      state.busy = false;
      if (state.again) { state.again = false; const row = instanceRow(host); if (row) paint(host, row.layerUid, frameNow(host)); }
      return value;
    };
    return Promise.resolve().then(() => adapter.read({ kind: 'instance', layerUid, frame })).then((outcome) => {
      const row = instanceRow(host);
      if (token !== state.seq || !outcome || outcome.ok !== true || !row || row.layerUid !== layerUid) return finish(false);
      state.view = outcome.result;
      state.key = keyOf(outcome.revision, outcome.documentId, layerUid, frame);
      render(host, outcome.result);
      return finish(true);
    }, () => finish(false));
  }

  // The members one field commits, with the baseline coercions; null when the event names no field of the panel.
  function patchOf(host, target) {
    const frame = frameNow(host);
    if (target.dataset && target.dataset.nativeCompFrame !== undefined) {
      const picked = Math.max(0, parseInt(target.dataset.nativeCompFrame, 10) || 0);
      return { mode: 'single', singleFrame: Math.min(U32_MAX, picked), atFrame: frame };
    }
    switch (target.id) {
      case 'comp-playmode': return MODES.includes(target.value) ? { mode: target.value } : null;
      case 'comp-singleframe': return { singleFrame: Math.min(U32_MAX, Math.max(0, parseInt(target.value, 10) || 0)), atFrame: frame };
      case 'comp-speed': return { speed: Math.max(MIN_SPEED, parseFloat(target.value) || 1) };
      case 'comp-offset': return { placedAt: Math.max(I32_MIN, Math.min(I32_MAX, parseInt(target.value, 10) || 0)) };
      default: return null;
    }
  }
  const isCell = (target) => !!target && !!target.dataset && target.dataset.nativeCompFrame !== undefined;

  function enter(host, row) {
    const context = host.NemoNativeComponentContext;
    if (!context || typeof context.enter !== 'function') { toast(host, 'Native component context is unavailable.'); return Promise.resolve(false); }
    return Promise.resolve().then(() => context.enter(host, row.componentId, null, row.layerUid)).catch((error) => {
      toast(host, 'Native component context: ' + String(error && error.message || error));
      return false;
    });
  }

  function onEvent(host, event) {
    const target = event && event.target, id = target && target.id;
    if (!target || !session(host)) return;
    const cell = isCell(target), unavailable = !!id && Object.prototype.hasOwnProperty.call(UNAVAILABLE, id);
    const field = FIELDS.includes(id), enterButton = id === 'btn-comp-enter';
    if (!cell && !unavailable && !field && !enterButton) return;
    if ((event.type === 'click') !== (cell || unavailable || enterButton)) return;
    event.stopImmediatePropagation();
    event.stopPropagation();
    if (unavailable) { toast(host, UNAVAILABLE[id] + ' is not available on this project yet.'); return; }
    const row = instanceRow(host);
    if (!row) { toast(host, 'The active layer is not a component instance.'); return; }
    if (enterButton) { enter(host, row); return; }
    const adapter = host.NemoNativeObjectComponent && host.NemoNativeObjectComponent.desktop(host), patch = patchOf(host, target);
    if (!adapter || typeof adapter.setInstance !== 'function') { toast(host, 'Native component playback is unavailable.'); return; }
    if (!patch) { stateOf(host).key = null; paint(host, row.layerUid, frameNow(host)); return; }
    Promise.resolve().then(() => adapter.setInstance(row.layerUid, patch)).then((outcome) => {
      if (!outcome || outcome.ok !== true) toast(host, refusalText(outcome));
      else if (typeof host.nativeCompositingRefresh === 'function') host.nativeCompositingRefresh();
      stateOf(host).key = null;
      return paint(host, row.layerUid, frameNow(host));
    }, (error) => {
      toast(host, refusalText({ message: String(error && error.message || error) }));
      stateOf(host).key = null;
      return paint(host, row.layerUid, frameNow(host));
    });
  }

  function install(host) {
    const state = stateOf(host), section = el(host, 'comp-instance-sec');
    if (state.installed || !section || typeof section.addEventListener !== 'function') return;
    state.installed = true;
    // Capture on the section runs before the legacy listeners bound on the fields themselves, so a native project never reaches them.
    ['change', 'click'].forEach((type) => section.addEventListener(type, (event) => onEvent(host, event), true));
  }

  // updateCompInstancePanel's native branch: true when the native project owns the panel (the legacy body must not run).
  function sync(host) {
    if (!host || !session(host)) {
      if (host && stateOf(host).installed) mark(host, false);
      return false;
    }
    install(host);
    const section = el(host, 'comp-instance-sec'), row = instanceRow(host), state = stateOf(host);
    if (!section) return true;
    if (!row) { state.seq += 1; state.key = null; section.style.display = 'none'; return true; }
    section.style.display = 'block';
    mark(host, true);
    // One read per (revision, layer, playhead frame): a repaint that changed none of them asks nothing.
    const pin = info(host), frame = frameNow(host);
    if (state.key === keyOf(pin.contentRevision, pin.documentId, row.layerUid, frame) && state.view) return true;
    paint(host, row.layerUid, frame);
    return true;
  }
  return Object.freeze({ sync, FIELDS, UNAVAILABLE, patchOf, refusalText });
}));
