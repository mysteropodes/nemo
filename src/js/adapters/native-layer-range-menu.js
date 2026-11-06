/* #2371 / #2372 D27: the in/out bar context menu of src/js/layer-inout.js @1065e9d8 over the native document. Right-click on a layer bar
 * lists "Réinitialiser (pleine durée)", "Parent in Time — lier le temps à…" (the native Link time picker of native-layer-parent.js)
 * and, with two or more bars selected, align left / centre / right, distribute, stagger, flip and "Sélectionner 1 sur 2", then
 * "Inverser la sélection". Every write is ONE command.document.object.layer.range.arrange over the listed layers: the engine computes
 * the targets from its own range evaluator and writes them atomically as one revision and one history entry. The selection items only
 * change the bar selection of the timeline UI. The adapter holds no document state and has no fallback writer. A view that carries its own
 * `send(action, uids, step)` (the entered component's bars) is written through it with the held contextId, and offers no Parent in Time. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const command = commonjs ? require('./native-object-rig-command.js') : root.NemoNativeObjectRigCommand;
  const contract = commonjs ? require('./native-object-rig-contract.js') : root.NemoNativeObjectRigContract;
  const api = factory(command, contract);
  if (commonjs) module.exports = api;
  else root.NemoNativeLayerRangeMenu = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Command, Contract) {
  'use strict';
  const LIMIT = 4096, ROOT_CONTEXT = 'scene-root', PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  const same = (a, b) => !!a && !!b && PINS.every((key) => a[key] === b[key]);
  const toast = (host, message) => { if (typeof host.showToast === 'function') host.showToast(message); };
  const tr = (host, key, fallback) => { const value = host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : null; return typeof value === 'string' && value && value !== key ? value : fallback; };
  const REFUSALS = {
    not_found: 'The layer is no longer in the project.',
    unavailable: 'Layer bar edit refused: an edge cannot be moved exactly (an expression-driven or fractional offset).',
    invalid_request: 'Layer bar edit refused.',
  };
  const bindings = new WeakMap();
  function bound(host) {
    const module = host.NemoNativeObjectProject, session = module && module.desktop(host);
    if (!session || !session.blocksLegacy()) return null;
    const previous = bindings.get(host);
    if (previous && previous.session === session) return previous;
    const next = { session, commands: Command.create(session, 'range', Contract, LIMIT) };
    bindings.set(host, next);
    return next;
  }
  function refresh(host) {
    if (typeof host.renderLayerList === 'function') host.renderLayerList();
    if (typeof host.renderTimeline === 'function') host.renderTimeline();
    if (typeof host.nativeCompositingRefresh === 'function') host.nativeCompositingRefresh();
  }
  // One native edit over `uids` (listed order); `view.info` is the revision the menu was built from, so a stale menu writes nothing.
  function send(host, view, action, uids, step) {
    // A bar menu opened inside an entered component sends through the component route (native-component-range-edit.js).
    if (typeof view.send === 'function') return view.send(action, uids, step);
    const route = bound(host);
    if (!route || !Contract) { toast(host, 'Native layer range editing is unavailable.'); return Promise.resolve(false); }
    if (!same(route.session.info(), view.info)) { toast(host, 'Native layer range editing is unavailable or stale.'); return Promise.resolve(false); }
    const frame = Math.max(0, Math.floor(Number(host.state && host.state.currentFrame) || 0));
    return route.commands.execute(Contract.RANGE_ARRANGE, () => {
      const payload = { command: 'object.layer.range.arrange', contextId: ROOT_CONTEXT, layerUids: uids.slice(), action, frame };
      if (action === 'stagger') payload.step = step;
      return payload;
    }).then(() => { refresh(host); return true; }, (error) => { toast(host, REFUSALS[error && error.code] || 'Native layer bar command was refused.'); return false; });
  }
  // staggerBars asks for the per-layer offset; window.prompt() is silently ignored by Tauri's WKWebView, so it is typed in an inline field.
  function askStep(host, x, y) {
    return new Promise((resolve) => {
      const doc = host.document, box = doc.createElement('div'), label = doc.createElement('span'), input = doc.createElement('input');
      const ok = doc.createElement('button'), cancel = doc.createElement('button');
      box.className = 'ctx-menu native-range-step'; box.style.left = x + 'px'; box.style.top = y + 'px'; box.style.display = 'flex'; box.style.gap = '6px'; box.style.alignItems = 'center';
      label.textContent = 'Décalage entre calques (frames)';
      input.type = 'number'; input.className = 'pi scrub'; input.value = '2'; input.step = '1'; input.dataset.step = '1'; input.style.width = '56px';
      input.setAttribute('aria-label', label.textContent);
      ok.type = 'button'; ok.className = 'pbtn'; ok.textContent = 'OK'; cancel.type = 'button'; cancel.className = 'pbtn'; cancel.textContent = tr(host, 'btnCancel', 'Cancel');
      let settled = false;
      const done = (value) => {
        if (settled) return;
        settled = true;
        doc.removeEventListener('mousedown', outside, true);
        if (box.parentNode) box.parentNode.removeChild(box);
        resolve(value);
      };
      function outside(event) { if (!box.contains(event.target)) done(null); }
      const commit = () => { const step = parseInt(input.value, 10); done(Number.isNaN(step) || step === 0 ? null : step); };
      input.addEventListener('keydown', (event) => {
        if (event.stopPropagation) event.stopPropagation();
        if (event.key === 'Enter') { if (event.preventDefault) event.preventDefault(); commit(); } else if (event.key === 'Escape') done(null);
      });
      ok.addEventListener('click', (event) => { if (event.stopPropagation) event.stopPropagation(); commit(); });
      cancel.addEventListener('click', (event) => { if (event.stopPropagation) event.stopPropagation(); done(null); });
      [label, input, ok, cancel].forEach((node) => box.appendChild(node));
      doc.body.appendChild(box);
      doc.addEventListener('mousedown', outside, true);
      if (typeof input.focus === 'function') input.focus();
    });
  }
  function sortedByIn(view) {
    return view.selection.map((v) => ({ v, at: view.rows.find((row) => row.layerUid === v.layerUid).rawIn })).sort((a, b) => a.at - b.at).map((entry) => entry.v);
  }
  function selectEveryNth(host, view, n) {
    const kept = sortedByIn(view).filter((_, i) => i % n === 0);
    view.setSelection(kept);
    toast(host, kept.length + tr(host, 'toastLayersSelectedSuffix', ' layer(s) selected'));
  }
  // The same universe Box Select draws its marquee over: every row with a bar that is not already selected, always the whole bar.
  function invertSelection(view) {
    const chosen = new Set(view.selection.map((v) => v.layerUid));
    view.setSelection(view.rows.filter((row) => !row.hidden && !chosen.has(row.layerUid)).map((row) => ({ layerUid: row.layerUid, part: 'both' })));
  }
  function items(host, view, x, y) {
    const uids = view.selection.map((v) => v.layerUid), parent = host.NemoNativeLayerParent, adapter = parent && parent.desktop(host);
    const batch = (action, step) => {
      if (uids.length < 2) { toast(host, tr(host, 'toastSelectAtLeast2Layers', 'Select at least 2 layers')); return Promise.resolve(false); }
      return send(host, view, action, uids, step);
    };
    const menu = [{ label: 'Réinitialiser (pleine durée)', action: () => { send(host, view, 'reset', [view.uid]); } }];
    if (adapter && typeof view.send !== 'function') {
      menu.push({ label: 'Parent in Time — lier le temps à…', action: () => {
        host.showContextMenu(x + 8, y + 8, parent.timeLinkMenuItems(host, adapter, view.li, () => refresh(host)));
      } });
    }
    if (uids.length >= 2) {
      menu.push({ sep: true });
      menu.push({ label: 'Aligner à gauche (in)', action: () => { batch('alignLeft'); } });
      menu.push({ label: 'Aligner au centre', action: () => { batch('alignCenter'); } });
      menu.push({ label: 'Aligner à droite (out)', action: () => { batch('alignRight'); } });
      menu.push({ label: 'Distribuer uniformément', action: () => { batch('distribute'); } });
      menu.push({ label: 'Échelonner (stagger)…', action: () => { askStep(host, x, y).then((step) => { if (step !== null) batch('stagger', step); }); } });
      menu.push({ label: 'Inverser l’ordre (flip)', action: () => { batch('flip'); } });
      menu.push({ label: 'Sélectionner 1 sur 2', action: () => { selectEveryNth(host, view, 2); } });
    }
    if (uids.length >= 1) menu.push({ label: 'Inverser la sélection', action: () => { invertSelection(view); } });
    return menu;
  }
  // `view` is built by the bar owner (native-layer-range-desktop.js): { info, li, uid, selection: [{layerUid, part}], rows: [{layerUid, hidden, rawIn}], setSelection }.
  function open(host, event, view) {
    if (typeof host.showContextMenu !== 'function') return false;
    host.showContextMenu(event.clientX, event.clientY, items(host, view, event.clientX, event.clientY));
    return true;
  }
  return Object.freeze({ open, items });
}));
