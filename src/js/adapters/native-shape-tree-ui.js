/* The Elements panel on a native object project (#2376 G26): the baseline panel (1065e9d8 shapes-panel.js) over native reads and commands.
 * It lists the active layer's items and groups at the playhead from the native hit-test listing (select-canvas-intent.presented), and
 *   - a row click selects exactly as a Select-tool click does (select-canvas-intent.applyPick on a freshly read presented path);
 *   - "Select members" selects every member of a group; Shift/Alt extends the selection (the baseline's toggle rule);
 *   - Rename on a group is command.document.object.group.rename; on a plain shape the native record has no name, so it reports unavailable;
 *   - the combine-mode badge on a group row shows its stored mode (the listing's groupPath mode); a click cycles to the next mode and the row
 *     menu sets a chosen one, each one command.document.object.group.mode.set (one history entry, a refusal toasts, nothing is written locally);
 *   - Ungroup is command.document.object.group.ungroup (one history entry) and dissolves the group's own level - the engine dissolves the
 *     OUTERMOST group of the member it is given, so only a top-level group is offered; a nested one reports unavailable.
 * Inside an entered component the same three edits address that component's context (the held id) and its own layer; a refusal from the
 * engine keeps the typed toast. It owns no document state and never reaches the legacy writer (shapes-panel.js stays refused). Refresh follows the project pin (revision),
 * the active layer and the frame; the same key repaints only the selection highlight, and a scrub reads once per settled frame (a read in
 * flight is followed by ONE more for the latest frame), keeping the old rows on screen until the new listing lands. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeShapeTree = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  const pinKey = (pin) => (pin ? PINS.map((k) => pin[k]).join('|') : '');
  const ARROWS = { open: '▾', closed: '▸' };
  const bindings = new WeakMap();
  // The baseline's COMBINE_CYCLE (shapes-panel.js): 'none' is the neutral state the badge cycles through; the menu lists the four real modes then 'none'.
  const COMBINE_CYCLE = ['none', 'unite', 'subtract', 'intersect', 'exclude'];
  const COMBINE_KEYS = { unite: 'combineUnion', subtract: 'combineSubtract', intersect: 'combineIntersect', exclude: 'combineExclude', none: 'combineNone' };
  const COMBINE_WORDS = { unite: 'Union', subtract: 'Subtract', intersect: 'Intersect', exclude: 'Exclude', none: 'Remove combine' };
  const nextMode = (mode) => COMBINE_CYCLE[(Math.max(0, COMBINE_CYCLE.indexOf(mode)) + 1) % COMBINE_CYCLE.length];

  function create(win, deps) {
    const D = deps || {};
    const doc = win.document;
    const expanded = new Set();
    let section = null, list = null, view = null, running = false, dirty = false, shown = '', busy = false, edits = new WeakMap();
    const el = (tag, cls, text) => { const n = doc.createElement(tag); if (cls) n.className = cls; if (text !== undefined) n.textContent = text; return n; };
    const say = (key, fallback) => { try { const v = win.SM && typeof win.SM.t === 'function' ? win.SM.t(key) : null; return v && v !== key ? v : fallback; } catch (_) { return fallback; } };
    const notice = (text) => { if (typeof win.showToast === 'function') try { win.showToast(text); } catch (_) { /* a notice is a nicety */ } };
    const session = D.session || (() => { try { const P = win.NemoNativeObjectProject, s = P && typeof P.desktop === 'function' ? P.desktop(win) : null; return s && s.blocksLegacy() === true && s.isActive() ? s : null; } catch (_) { return null; } });
    const canvas = D.canvas || (() => win.NemoSelectCanvasIntent);
    const model = () => D.model || win.NemoNativeShapeTreeModel;
    const labels = () => ({ shape: say('autoNameShape', 'Shape'), stroke: say('autoNameStroke', 'Stroke'), group: say('autoNameGroup', 'Group'), text: say('autoNameText', 'Text'), image: 'Image' });
    // The group intent over this session's ports (the shape of native-selection-boolean's): pin / requestId / submit are the session's resource route.
    function groupIntent(s) {
      if (edits.has(s)) return edits.get(s);
      const Group = win.NemoNativeObjectGroupIntent, Component = win.NemoNativeObjectComponentIntent, Contract = win.NemoNativeObjectEditContract;
      if (!Group || !Component || !Contract || typeof Component.portsFromSession !== 'function') return null;
      const base = Component.portsFromSession(s);
      const made = Group.createGroupIntent({ pin: base.pin, requestId: base.requestId, submit: base.submit,
        declared: (operation) => Contract.admitted(operation) && base.readDeclared() === true });
      edits.set(s, made);
      return made;
    }
    const D_intent = D.intent || groupIntent;

    function selectedIds() {
      const out = new Set();
      (win.selectedPaths || []).forEach((p) => { if (p && p.data && p.data.strokeId) out.add(p.data.strokeId); });
      return out;
    }
    function paintSelection() {
      if (!list) return;
      const on = selectedIds();
      list.querySelectorAll('.motion-elem-row').forEach((row) => {
        const ids = row.dataset.members ? row.dataset.members.split('\n') : [row.dataset.strokeid];
        row.classList.toggle('act', ids.some((id) => on.has(id)));
      });
    }

    // ---- actions -------------------------------------------------------------------------------------------------------------------------------
    // The strokes as fresh presented paths of the panel's layer, in the order asked; null with a notice when they cannot be proven current.
    async function presentedPaths(ids) {
      const I = canvas(), s = session();
      if (!I || !s || !view) { notice('The elements are not available right now.'); return null; }
      const at = view;
      const ready = await I.presentedSettled(win);
      if (!ready || view !== at || pinKey(s.info()) !== at.pin) { notice('The document changed — select again.'); return null; }
      let layers = null;
      try { layers = await I.layerPaths(win, (item) => ids.indexOf(item.stableTarget.strokeId) >= 0, false, at.layerIdx); } catch (_) { layers = null; }
      if (!layers) { notice('This layer cannot be selected right now (locked or hidden).'); return null; }
      const found = ids.map((id) => layers.paths.find((p) => p.data.strokeId === id)).filter(Boolean);
      if (!found.length) { notice('The document changed — select again.'); return null; }
      return found;
    }
    // The baseline's applySelection: a click selects; Shift/Alt toggles the whole set (all selected -> removed, else the missing ones added).
    async function select(ids, additive) {
      const found = await presentedPaths(ids);
      if (!found) return false;
      const I = canvas();
      if (!additive) { I.applyPick(win, found[0], false); found.slice(1).forEach((p) => I.applyPick(win, p, true)); return true; }
      const on = selectedIds(), all = found.every((p) => on.has(p.data.strokeId));
      if (all) (win.selectedPaths || []).filter((p) => p && p.data && ids.indexOf(p.data.strokeId) >= 0).forEach((p) => I.applyPick(win, p, true));
      else found.filter((p) => !on.has(p.data.strokeId)).forEach((p) => I.applyPick(win, p, true));
      return true;
    }
    async function edit(kind, input, done) {
      const s = session(), intent = s && D_intent(s);
      if (!intent || !view || busy) { notice('This is not available right now.'); return null; }
      if (view.contextId !== heldNow()) { notice('The document changed — nothing was changed.'); return null; }
      busy = true;
      let out;
      try { out = await intent[kind](Object.assign({ layerUid: view.layerUid, contextId: view.contextId }, input)); } catch (_) { out = { ok: false, code: 'unavailable' }; }
      busy = false;
      if (!out || !out.ok) {
        notice(out && (out.code === 'stale_revision' || out.code === 'stale_selection') ? 'The document changed — nothing was changed.' : done.failed);
        return out;
      }
      if (out.applied) { if (typeof win.updateUI === 'function') win.updateUI(); } else notice('Nothing changed.');
      return out;
    }
    // The context the panel reads and edits: the entered component's id (the one the workspace controller holds) or the scene root.
    const heldNow = () => { const I = canvas(); try { return I && typeof I.heldId === 'function' ? I.heldId(win) || 'scene-root' : 'scene-root'; } catch (_) { return 'scene-root'; } };
    function renameGroup(node, value) {
      if (value === node.name && node.named) return;
      edit('rename', { groupId: node.gid, name: value }, { failed: 'The group could not be renamed.' });
    }
    function ungroup(node) {
      if (node.parent) { notice('Only a top-level group can be ungrouped here — ungroup the outer group first.'); return; }
      edit('ungroup', { strokeIds: [node.memberIds[0]] }, { failed: 'The group could not be ungrouped.' });
    }
    function setMode(node, mode) {
      if (mode === node.mode) return;
      edit('setMode', { groupId: node.gid, mode }, { failed: 'The combine mode could not be changed.' });
    }
    const modeLabel = (mode) => say(COMBINE_KEYS[mode] || COMBINE_KEYS.none, COMBINE_WORDS[mode] || COMBINE_WORDS.none);
    function modeIcon(mode, gid) {
      if (mode === 'unite') return win.ICO_COMBINE_UNITE || '';
      if (mode === 'subtract') return typeof win.icoCombineSubtract === 'function' ? win.icoCombineSubtract(gid) : '';
      if (mode === 'intersect') return typeof win.icoCombineIntersect === 'function' ? win.icoCombineIntersect(gid) : '';
      if (mode === 'exclude') return win.ICO_COMBINE_EXCLUDE || '';
      return win.ICO_COMBINE_NONE || '';
    }
    function shapeRename() { notice('Renaming a shape is not available on a native project yet — only groups can be renamed.'); }

    // ---- rows ----------------------------------------------------------------------------------------------------------------------------------
    // The baseline's inline input swap (shapes-panel.js startRename).
    function startRename(row, current, commit) {
      const nm = row.querySelector('.lnm');
      if (!nm) return;
      const input = el('input');
      input.type = 'text'; input.value = current;
      input.style.cssText = 'width:100%;background:var(--bg);border:1px solid var(--accent);color:var(--text);font-size:11px;border-radius:4px;padding:1px 4px;outline:none;';
      nm.textContent = ''; nm.appendChild(input); input.focus(); input.select();
      let done = false;
      const finish = (apply) => { if (done) return; done = true; const v = input.value.trim(); if (apply && v) commit(v); else repaint(true); };
      input.addEventListener('keydown', (e) => { e.stopPropagation(); if (e.key === 'Enter') finish(true); else if (e.key === 'Escape') finish(false); });
      input.addEventListener('blur', () => finish(true));
      ['mousedown', 'dblclick', 'click'].forEach((type) => input.addEventListener(type, (e) => e.stopPropagation()));
    }
    function menu(e, items) {
      e.preventDefault(); e.stopPropagation();
      if (typeof win.showContextMenu === 'function') win.showContextMenu(e.clientX, e.clientY, items);
    }
    function shapeRow(node, indent, top) {
      const row = el('div', 'lrow motion-elem-row');
      if (indent) row.style.paddingLeft = (24 + indent) + 'px';
      if (top) row.dataset.toplevel = '1';
      row.dataset.strokeid = node.strokeId;
      row.appendChild(el('span', 'lico larrow-spacer'));
      const swatch = el('div', 'motion-elem-swatch');
      if (node.role === 'raster') { swatch.classList.add('icon'); swatch.innerHTML = win.ICO_IMAGE || ''; }
      else {
        swatch.style.background = node.fill || 'transparent';
        if (node.stroke) { swatch.style.borderColor = node.stroke; swatch.style.borderWidth = '2px'; }
      }
      row.appendChild(swatch); row.appendChild(el('div', 'lnm', node.label));
      row.addEventListener('click', (e) => { select([node.strokeId], e.shiftKey || e.altKey); });
      row.addEventListener('dblclick', (e) => { e.stopPropagation(); shapeRename(); });
      row.addEventListener('contextmenu', (e) => menu(e, [
        { label: say('elementsRename', 'Rename'), action: shapeRename },
        { label: say('elementsSelect', 'Select'), action: () => select([node.strokeId], false) },
      ]));
      return row;
    }
    function groupRow(node, indent, top) {
      const open = expanded.has(node.gid);
      const row = el('div', 'lrow motion-elem-row motion-elem-group');
      if (indent) row.style.paddingLeft = (24 + indent) + 'px';
      if (top) row.dataset.toplevel = '1';
      row.dataset.gid = node.gid;
      row.dataset.members = node.memberIds.join('\n');
      const arrow = el('span', 'lico larrow', open ? ARROWS.open : ARROWS.closed);
      arrow.addEventListener('click', (e) => { e.stopPropagation(); if (open) expanded.delete(node.gid); else expanded.add(node.gid); repaint(true); });
      const swatch = el('div', 'motion-elem-swatch icon');
      swatch.innerHTML = win.ICO_GROUP || '';
      row.appendChild(arrow); row.appendChild(swatch); row.appendChild(el('div', 'lnm', node.name));
      // The baseline's Combined Shape badge: .lico, so a click here never arms the row; it cycles to the next mode.
      const badge = el('div', 'lico motion-combine-badge');
      badge.innerHTML = modeIcon(node.mode, node.gid); badge.title = modeLabel(node.mode); badge.dataset.mode = node.mode;
      badge.addEventListener('click', (e) => { e.stopPropagation(); setMode(node, nextMode(node.mode)); });
      row.appendChild(badge);
      const modeItem = (mode) => ({ label: modeLabel(mode) + (node.mode === mode ? ' ✓' : ''), action: () => setMode(node, mode) });
      const rename = () => startRename(row, node.name, (v) => renameGroup(node, v));
      row.addEventListener('click', (e) => { select(node.memberIds, e.shiftKey || e.altKey); });
      row.addEventListener('dblclick', (e) => { e.stopPropagation(); rename(); });
      row.addEventListener('contextmenu', (e) => menu(e, [
        { label: say('elementsRename', 'Rename'), action: rename },
        { label: say('elementsSelectMembers', 'Select members'), action: () => select(node.memberIds, false) },
        { sep: true },
        modeItem('unite'), modeItem('subtract'), modeItem('intersect'), modeItem('exclude'), modeItem('none'),
        { sep: true },
        { label: say('elementsUngroup', 'Ungroup'), action: () => ungroup(node) },
      ]));
      return row;
    }
    function paintRows(rows, indent, parent, into) {
      rows.forEach((node) => {
        if (node.type === 'group') {
          node.parent = parent;
          into.appendChild(groupRow(node, indent, !parent));
          if (expanded.has(node.gid)) paintRows(node.children, indent + 20, node, into);
        } else into.appendChild(shapeRow(node, indent, !parent));
      });
    }
    function message(text) {
      if (!list) return;
      const empty = el('div', 'pr', text);
      empty.style.cssText = 'color:var(--text-dim);font-size:10.5px';
      list.replaceChildren(empty); shown = '';
    }
    function repaint(force) {
      if (!list || !view) return;
      const M = model();
      const sig = M.signature(view.tree) + '#' + Array.from(expanded).sort().join(',');
      if (!force && sig === shown) { paintSelection(); return; }
      shown = sig;
      if (!view.tree.length) { message(say('elementsPanelEmpty', 'No elements')); return; }
      const frag = doc.createDocumentFragment();
      paintRows(view.tree, 0, null, frag);
      list.replaceChildren(frag);
      paintSelection();
    }

    // ---- refresh -------------------------------------------------------------------------------------------------------------------------------
    function keyOf() {
      const s = session();
      if (!s) return null;
      const info = s.info(), idx = win.state && win.state.activeLayerIdx, I = canvas(), ctx = heldNow();
      // Inside an entered component the active layer index addresses the component's own layers, as the Select path reads them.
      const rows = info && (ctx !== 'scene-root' && I && typeof I.layerList === 'function' ? I.layerList(win, info) : info.layers), layer = rows && rows[idx];
      return layer ? { s, idx, layerUid: layer.layerUid, contextId: ctx, frame: win.state.currentFrame, pin: pinKey(info), key: [pinKey(info), ctx, idx, layer.layerUid, win.state.currentFrame].join('|') } : { s, none: true, key: 'none|' + ctx + '|' + pinKey(info) };
    }
    async function readOnce() {
      const k = keyOf(), I = canvas();
      if (!k || k.none || !I) { view = null; message(say('elementsPanelEmpty', 'No elements')); return; }
      if (view && view.key === k.key) return;
      const ready = await I.presentedSettled(win);
      const now = keyOf();
      if (!now || now.key !== k.key) { dirty = true; return; }
      if (!ready) { if (!view) message('The elements could not be read just now.'); return; }
      let layers = null;
      try { layers = await I.presented(win, k.idx, () => false); } catch (_) { layers = null; }
      const after = keyOf();
      if (!after || after.key !== k.key) { dirty = true; return; }
      if (!layers) { if (!view) message('The elements could not be read just now.'); return; }
      view = { key: k.key, pin: k.pin, layerIdx: k.idx, layerUid: k.layerUid, contextId: k.contextId, frame: k.frame, tree: model().build(layers.items, labels()) };
      repaint(false);
    }
    function request() {
      if (running) { dirty = true; return; }
      running = true;
      (async function () {
        try { do { dirty = false; await readOnce(); } while (dirty); } catch (_) { /* a read that fails is asked again on the next paint */ } finally { running = false; }
      }());
    }
    // One paint of the host UI. `frameOnly` (nothing but the frame moved) never changes the selection, so the highlight pass is skipped.
    function refresh(frameOnly) {
      if (!section || !list) return;
      const k = keyOf();
      if (!k) { message('Native elements are unavailable.'); view = null; return; }
      if (view && !k.none && view.key === k.key) { if (frameOnly !== true) paintSelection(); return; }
      request();
    }
    function mount(container) {
      section = el('div', 'psec'); section.id = 'native-shapes-sec';
      const head = el('div', 'phdr');
      head.appendChild(el('span', 'ar', '☰')); head.appendChild(doc.createTextNode(' '));
      head.appendChild(el('span', '', say('hdrElements', 'Elements')));
      const body = el('div', 'pbdy');
      list = el('div'); list.id = 'native-shapes-list';
      body.appendChild(list); section.appendChild(head); section.appendChild(body);
      head.addEventListener('click', () => { body.classList.toggle('hid'); head.classList.toggle('closed'); });
      container.appendChild(section);
      refresh(false);
    }
    function retire() {
      if (section && section.parentNode) section.parentNode.removeChild(section);
      section = null; list = null; view = null; shown = '';
    }
    return Object.freeze({ mount, refresh, retire, select, ungroup, renameGroup, setMode, node: () => section, state: () => view });
  }

  // The page-level entry point native-object-ui.js calls on every inspector paint; it answers the binding, or null when the page cannot host it.
  function mount(win, container, frameOnly) {
    if (!win || !container || !win.document) return null;
    const old = bindings.get(win);
    if (old && old.container === container && old.node() && old.node().parentNode === container) { old.refresh(frameOnly); return old; }
    if (old) old.retire();
    const made = create(win);
    const binding = Object.assign({ container }, made);
    bindings.set(win, binding);
    made.mount(container);
    return binding;
  }
  // True while a native object project owns the document: the legacy Elements renderer (shapes-panel.js) then yields to this presenter.
  function owns(win) {
    try { const P = win && win.NemoNativeObjectProject, s = P && typeof P.desktop === 'function' ? P.desktop(win) : null; return !!s && s.blocksLegacy() === true; } catch (_) { return false; }
  }
  function retire(win) {
    const old = bindings.get(win);
    if (old) { old.retire(); bindings.delete(win); }
  }
  return Object.freeze({ create, mount, retire, owns });
}));
