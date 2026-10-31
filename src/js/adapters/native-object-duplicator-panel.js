/* #2256 Q4: the Duplicator inspector (#duplicator-sec) over a NATIVE object project.
 * Baseline (1065e9d8, timeline.js updateDuplicatorPanel + the Mograph duplicator panel listeners): the section shows for the active layer
 * when it hosts a duplicator, each field writes ld.duplicator.* (wireNum clamps rows/cols 1..30 and count 1..500) and reloads the frame.
 * Here the section is shown from the native layer row (hasDuplicator, the Q3 read) and filled from ONE pinned descriptor read
 * (query.document.object.component.read, selector duplicator); every field commits ONE command.document.object.duplicator.config naming only
 * the member that changed (a scrub previews in the field and sends its single request on release, the data-native-opacity-scrub contract of
 * ui.js), the path-layer select keeps the existing duplicator.path command, and the reseed button draws Math.floor(random()*1e6). A refusal
 * toasts the owner's typed reason and the panel is repainted from the stored descriptor. ld.duplicator, dupOf and dupRefresh are never
 * touched, and an unmigrated control (effectors, the source pool, "edit source") says so instead of writing a stub row that is discarded. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectDuplicatorPanel = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  // Field id -> { member of the config request, kind, bounds }. Bounds are the baseline wireNum arguments.
  const FIELDS = Object.freeze({
    'dup-mode': { key: 'mode', kind: 'text' },
    'dup-rows': { key: 'rows', kind: 'number', min: 1, max: 30 },
    'dup-cols': { key: 'cols', kind: 'number', min: 1, max: 30 },
    'dup-spacingx': { key: 'spacingX', kind: 'number' },
    'dup-spacingy': { key: 'spacingY', kind: 'number' },
    'dup-count': { key: 'count', kind: 'number', min: 1, max: 500 },
    'dup-radius': { key: 'radius', kind: 'number' },
    'dup-startangle': { key: 'startAngle', kind: 'number' },
    'dup-seed': { key: 'seed', kind: 'seed' },
    'dup-radial-orient': { key: 'radialOrient', kind: 'flag' },
    'dup-path-align': { key: 'pathAlignTangent', kind: 'flag' },
    'dup-rand-pos': { group: 'staggerRandom', key: 'position', kind: 'flag' },
    'dup-rand-rot': { group: 'staggerRandom', key: 'rotation', kind: 'flag' },
    'dup-rand-scale': { group: 'staggerRandom', key: 'scale', kind: 'flag' },
    'dup-rand-op': { group: 'staggerRandom', key: 'opacity', kind: 'flag' },
    'dup-rand-hue': { group: 'staggerRandom', key: 'hue', kind: 'flag' },
    'dup-anim-enabled': { group: 'timeOffset', key: 'enabled', kind: 'flag' },
    'dup-anim-offset': { group: 'timeOffset', key: 'offsetFrames', kind: 'number' },
    'dup-anim-direction': { group: 'timeOffset', key: 'direction', kind: 'text' },
  });
  // Controls with no native owner yet (effectors, the object-mode source pool, edit source): said, never written.
  const UNAVAILABLE = Object.freeze({
    'btn-dup-add-effector': 'Duplicator effectors',
    'btn-dup-edit-source': 'Editing the duplicator source shape',
    'dup-source-add': 'The duplicator source pool',
  });
  const CLICKS = new Set(['btn-dup-reseed', 'btn-dup-add-effector', 'btn-dup-edit-source']); // buttons act on click, every field on change
  const SEED_SPAN = 1e6;
  const states = new WeakMap();

  const el = (host, id) => (host.document && typeof host.document.getElementById === 'function' ? host.document.getElementById(id) : null);
  const toast = (host, message) => { if (typeof host.showToast === 'function') host.showToast(message); };
  const text = (value) => typeof value === 'string' && value.length > 0;
  function session(host) {
    const project = host.NemoNativeObjectProject;
    const found = project && typeof project.desktop === 'function' ? project.desktop(host) : null;
    return found && typeof found.blocksLegacy === 'function' && found.blocksLegacy() === true ? found : null;
  }
  function rows(host) {
    const found = session(host), info = found && typeof found.info === 'function' ? found.info() : null;
    return info && Array.isArray(info.layers) ? info.layers : [];
  }
  function stateOf(host) {
    if (!states.has(host)) states.set(host, { seq: 0, layerUid: null, installed: false });
    return states.get(host);
  }
  // The active layer's native row when it hosts a duplicator, else null.
  function hostRow(host) {
    const at = host.state && host.state.activeLayerIdx, row = Number.isInteger(at) ? rows(host)[at] : null;
    return row && row.hasDuplicator === true && text(row.layerUid) ? row : null;
  }
  function refusalText(outcome) {
    if (outcome && outcome.code === 'stale_selection') return 'The layer is no longer in the project.';
    const detail = outcome && (text(outcome.message) ? outcome.message : text(outcome.reason) ? outcome.reason : outcome.code);
    return 'Duplicator refused' + (text(detail) ? ': ' + detail : '.');
  }

  function show(host, id, on) { const node = el(host, id); if (node && node.style) node.style.display = on ? 'flex' : 'none'; }
  function set(host, id, value) { const node = el(host, id); if (node) node.value = String(value); }
  function check(host, id, on) { const node = el(host, id); if (node) node.checked = on === true; }

  // The path select lists every OTHER layer that is not itself a duplicator and can carry a path (the baseline excluded only those two).
  function fillPathSelect(host, view, own) {
    const select = el(host, 'dup-path-layer');
    if (!select || !host.document || typeof host.document.createElement !== 'function') return;
    select.innerHTML = '';
    const add = (value, label) => { const option = host.document.createElement('option'); option.value = value; option.textContent = label; select.appendChild(option); };
    add('', '—');
    let listed = false;
    rows(host).forEach((row, at) => {
      if (row.layerUid === own || row.hasDuplicator === true || (row.kind !== undefined && row.kind !== 'draw')) return;
      add(row.layerUid, row.name || ('Layer ' + (at + 1)));
      if (row.layerUid === view.pathLayerUid) listed = true;
    });
    if (view.pathLayerUid && !listed) add(view.pathLayerUid, view.pathLayerUid); // stored but not listable: shown as stored, not dropped
    select.value = view.pathLayerUid || '';
  }

  // Fills every control from the stored descriptor and applies the baseline row visibility (updateDuplicatorPanel).
  function render(host, view) {
    const mode = view.mode, objects = view.sourceMode === 'objects', time = view.timeOffset;
    show(host, 'dup-anim-hdr-row', !objects);
    show(host, 'dup-edit-source-row', !objects);
    const pool = el(host, 'dup-source-sec');
    if (pool && pool.style) pool.style.display = objects ? 'block' : 'none';
    set(host, 'dup-mode', mode);
    show(host, 'dup-grid-row', mode === 'grid');
    show(host, 'dup-grid-spacing-row', mode === 'grid');
    show(host, 'dup-count-row', mode !== 'grid');
    show(host, 'dup-radial-row', mode === 'radial');
    show(host, 'dup-radial-orient-row', mode === 'radial');
    show(host, 'dup-path-row', mode === 'path');
    show(host, 'dup-path-align-row', mode === 'path');
    [['dup-rows', view.rows], ['dup-cols', view.cols], ['dup-spacingx', view.spacingX], ['dup-spacingy', view.spacingY], ['dup-count', view.count],
      ['dup-radius', view.radius], ['dup-startangle', view.startAngle], ['dup-seed', view.seed], ['dup-anim-offset', time.offsetFrames],
      ['dup-anim-direction', time.direction]].forEach(([id, value]) => set(host, id, value));
    check(host, 'dup-radial-orient', view.radialOrient);
    check(host, 'dup-path-align', view.pathAlignTangent);
    [['dup-rand-pos', 'position'], ['dup-rand-rot', 'rotation'], ['dup-rand-scale', 'scale'], ['dup-rand-op', 'opacity'], ['dup-rand-hue', 'hue']]
      .forEach(([id, key]) => check(host, id, view.staggerRandom[key]));
    check(host, 'dup-anim-enabled', time.enabled);
    show(host, 'dup-anim-row', time.enabled === true && !objects);
    fillPathSelect(host, view, view.layerUid);
  }

  // Marks the controls: scrubbable numbers commit once on release; unmigrated controls say why they do nothing.
  function mark(host, on) {
    Object.keys(FIELDS).forEach((id) => {
      const node = el(host, id);
      if (!node || !node.dataset || FIELDS[id].kind === 'flag' || FIELDS[id].kind === 'text') return;
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

  // Reads the stored descriptor of `layerUid` at the presented revision and paints it, unless the panel moved on meanwhile.
  function paint(host, layerUid) {
    const component = host.NemoNativeObjectComponent, adapter = component && typeof component.desktop === 'function' ? component.desktop(host) : null;
    const state = stateOf(host), token = ++state.seq;
    state.layerUid = layerUid;
    if (!adapter || typeof adapter.read !== 'function') return Promise.resolve(false);
    return Promise.resolve().then(() => adapter.read({ kind: 'duplicator', layerUid })).then((outcome) => {
      if (token !== state.seq || !outcome || outcome.ok !== true || !hostRow(host) || hostRow(host).layerUid !== layerUid) return false;
      render(host, outcome.result);
      return true;
    }, () => false);
  }

  function onEvent(host, event) {
    const target = event && event.target, id = target && target.id;
    if (!id || !session(host) || !(FIELDS[id] || UNAVAILABLE[id] || id === 'btn-dup-reseed' || id === 'dup-path-layer')) return;
    if ((event.type === 'click') !== CLICKS.has(id)) return;
    event.stopImmediatePropagation();
    event.stopPropagation();
    if (UNAVAILABLE[id]) {
      toast(host, UNAVAILABLE[id] + ' is not available on this project yet.');
      if (target.tagName === 'SELECT') target.value = '';
      return;
    }
    const row = hostRow(host), adapter = host.NemoNativeObjectDuplicator && host.NemoNativeObjectDuplicator.desktop(host);
    if (!row || !adapter) { toast(host, 'Native duplicator is unavailable.'); return; }
    let start;
    if (id === 'btn-dup-reseed') start = () => adapter.reseed(row.layerUid);
    else if (id === 'dup-path-layer') { const wanted = target.value || null; start = () => adapter.setPath(row.layerUid, wanted); }
    else {
      const patch = patchOf(FIELDS[id], target);
      if (!patch) { paint(host, row.layerUid); return; } // not a number: the panel shows the stored value again
      start = () => adapter.configure(row.layerUid, patch);
    }
    Promise.resolve().then(start).then((outcome) => {
      if (!outcome || outcome.ok !== true) toast(host, refusalText(outcome));
      else if (typeof host.nativeCompositingRefresh === 'function') host.nativeCompositingRefresh();
      return paint(host, row.layerUid);
    }, (error) => { toast(host, refusalText({ message: String(error && error.message || error) })); return paint(host, row.layerUid); });
  }
  // The config members one field commits: the baseline coercion (parseFloat then the wireNum bounds); null when a number does not parse.
  function patchOf(field, node) {
    let value;
    if (field.kind === 'flag') value = node.checked === true;
    else if (field.kind === 'text') value = String(node.value);
    else {
      value = parseFloat(node.value);
      if (!Number.isFinite(value)) return null;
      if (field.kind === 'seed') value = Math.max(0, Math.min(SEED_SPAN - 1, Math.floor(value)));
      if (field.min !== undefined) value = Math.max(field.min, value);
      if (field.max !== undefined) value = Math.min(field.max, value);
    }
    return field.group ? { [field.group]: { [field.key]: value } } : { [field.key]: value };
  }

  function install(host) {
    const state = stateOf(host), section = el(host, 'duplicator-sec');
    if (state.installed || !section || typeof section.addEventListener !== 'function') return;
    state.installed = true;
    // Capture on the section runs before the legacy listeners bound on the fields themselves, so a native project never reaches them.
    ['change', 'click'].forEach((type) => section.addEventListener(type, (event) => onEvent(host, event), true));
  }

  // updateDuplicatorPanel's native branch: true when the native project owns the panel (the legacy body must not run).
  function sync(host) {
    if (!host || !session(host)) {
      if (host && stateOf(host).installed) { mark(host, false); }
      return false;
    }
    install(host);
    const section = el(host, 'duplicator-sec'), row = hostRow(host);
    if (!section) return true;
    if (!row) { stateOf(host).seq += 1; section.style.display = 'none'; return true; }
    section.style.display = 'block';
    mark(host, true);
    paint(host, row.layerUid);
    return true;
  }
  return Object.freeze({ sync, FIELDS, UNAVAILABLE, patchOf, refusalText });
}));
