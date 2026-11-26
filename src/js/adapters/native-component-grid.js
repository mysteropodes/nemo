/* #2256: the timeline of an ENTERED native component (baseline timeline.js renderTimeline after enterSymbol, 1065e9d8): one .lrow in #layer-list and
 * one .frow in #frame-grid per component layer, in the same 34px (Motion 22px) rhythm so the two panels stay row-for-row aligned (CLAUDE.md section 11),
 * with the component's OWN frames drawn as the baseline .fc cells (key dot, held span, tween, span end) and its layer range as the in/out bar (editable through native-component-range-edit.js).
 * It owns no state and writes nothing: the cells and the range come from the pinned timing reads of the declared component context (the same
 * owner reads the scene grid uses, asked with the entered context), each answer is applied only while the entered context, its revision and the
 * playhead frame are still the ones asked, and a row whose read was refused stays blank (never a guess). Clicking a row activates the layer through
 * the context controller; no document is written. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeComponentGrid = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  function node(host, tag, cls, text) { var el = host.document.createElement(tag); if (cls) el.className = cls; if (text !== undefined) el.textContent = text; return el; }
  // One baseline cell per frame: the class set renderKeyframeCellsInto gives a key (kf-full/kf-empty + dot), a held frame (span-full/span-empty,
  // span-end on the last cell before the next key or tween), a tween (tw / tw-manual + dot). Runs are [start, length, kind, source, content, manual].
  function applyRuns(cells, runs) {
    var kinds = [];
    runs.forEach(function (run) { for (var at = run[0]; at < run[0] + run[1] && at < cells.length; at += 1) kinds[at] = run; });
    cells.forEach(function (cell, fi) {
      var run = kinds[fi]; cell.className = cell.className.split(' ').filter(function (c) { return ['fc', 'cur', 'outside-wa', 'sel'].indexOf(c) >= 0; }).join(' '); cell.textContent = '';
      if (!run) return;
      var kind = run[2], content = run[4] === true, next = kinds[fi + 1];
      if (kind === 'key') { cell.classList.add(content ? 'kf-full' : 'kf-empty'); cell.appendChild(node({ document: cell.ownerDocument }, 'div', 'km ' + (content ? 'fl' : 'hl'))); }
      else if (kind === 'interpolated') { cell.classList.add(run[5] === true ? 'tw-manual' : 'tw'); cell.appendChild(node({ document: cell.ownerDocument }, 'div', 'km td' + (run[5] === true ? ' manual' : ''))); }
      else if (kind === 'held') {
        cell.classList.add(content ? 'span-full' : 'span-empty');
        if (!next || next[2] === 'key' || next[2] === 'interpolated') cell.classList.add('span-end');
      }
    });
  }
  // The baseline frame objects the legacy readers of state.layers[i].frames expect, from the cells (no strokes: the drawing lives in the owner).
  function framesOf(total, runs) {
    var frames = [];
    for (var i = 0; i < total; i += 1) frames.push({ isKeyframe: false, isInterpolated: false, isManualEdit: false, strokes: [] });
    runs.forEach(function (run) {
      for (var at = run[0]; at < run[0] + run[1] && at < total; at += 1) {
        frames[at].isKeyframe = run[2] === 'key'; frames[at].isInterpolated = run[2] === 'interpolated'; frames[at].isManualEdit = run[5] === true;
      }
    });
    return frames;
  }
  function paintBar(host, row, range) {
    var bar = row.querySelector('.layer-inout-bar') || row.appendChild(node(host, 'div', 'layer-inout-bar'));
    var fc = Number.isFinite(host.FC) && host.FC > 0 ? host.FC : 14;
    bar.setAttribute('aria-disabled', 'true'); bar.style.left = (range.rawInPoint * fc) + 'px'; bar.style.width = Math.max(fc, (range.rawOutPoint - range.rawInPoint + 1) * fc) + 'px';
    bar.classList.toggle('full-range', !range.hasCustomRange); bar.classList.toggle('has-timelink', range.hasTimeLink === true);
    // The bar is editable inside a component too (native-component-range-edit.js): edge handles, drag, selection and the bar menu.
    if (host.NemoNativeComponentRangeEdit) host.NemoNativeComponentRangeEdit.bind(host, bar, row.dataset.layerUid, range);
  }
  function caption(st, frame) { return st.name + ' — Frame ' + (frame + 1) + ' / ' + st.totalFrames; }
  // CLAUDE.md section 5bis (b): the only frame-dependent outputs of the grid are the .cur cell and the caption, so a scrub or a playback tick
  // (frameOnly) moves those two and nothing else, provided the painted view is still what the content asks for (same rows, nodes, selection,
  // work area, size). Anything else repaints in full. Returns true when the playhead was moved in place.
  function moveHead(host, st, held, frame, selKey) {
    var s = host.state, fc = Number.isFinite(host.FC) && host.FC > 0 ? host.FC : 14, list = host.document.getElementById('layer-list'), grid = host.document.getElementById('frame-grid');
    var header = host.document.getElementById('frame-hdr');
    if (held.rows !== st.rows || held.list !== list || held.grid !== grid || held.header !== header || held.active !== s.activeLayerIdx || held.sel !== selKey || held.waIn !== s.waIn ||
      held.waOut !== s.waOut || held.total !== st.totalFrames || held.fc !== fc || held.name !== st.name || frame < 0 || frame >= st.totalFrames) return false;
    if (!held.lrows.every(function (row) { return row.parentNode === list; }) || !held.frows.every(function (row) { return row.parentNode === grid; }) || held.caption.parentNode !== header) return false;
    held.frows.forEach(function (row) {
      if (row.nativeCells[held.frame]) row.nativeCells[held.frame].classList.remove('cur');
      if (row.nativeCells[frame]) row.nativeCells[frame].classList.add('cur');
    });
    held.caption.textContent = caption(st, frame); held.frame = frame;
    return true;
  }
  // Paints the entered component's rows and grid. `st` is the controller's entered view ({name, totalFrames, rows}); `ports`: current() true while that
  // view is still the entered one, read(kind, layerUid) the pinned component read (a promise of the result, or null), select(index) activates a layer.
  // `frameOnly` (a scrub or playback tick, nothing but the frame changed) moves the playhead in place and starts no read.
  function paint(host, st, ports, frameOnly) {
    var doc = host.document, list = doc.getElementById('layer-list'), header = doc.getElementById('frame-hdr'), grid = doc.getElementById('frame-grid');
    if (!list || !header || !grid) return true;
    var s = host.state, frame = s.currentFrame, fc = Number.isFinite(host.FC) && host.FC > 0 ? host.FC : 14, active = s.activeLayerIdx, sel = Array.isArray(host._layerSel) ? host._layerSel : [];
    var selKey = sel.join(',');
    if (frameOnly === true && st.gridView && moveHead(host, st, st.gridView, frame, selKey)) return true;
    var asked = { rows: st.rows };
    var lrows = st.rows.map(function (layer, i) {
      var row = node(host, 'div', 'lrow native-component-layer' + (i === active ? ' act' : '') + (sel.indexOf(i) >= 0 && sel.length > 1 ? ' sel' : ''));
      row.dataset.layer = String(i); row.dataset.layerUid = layer.layerUid;
      var ico = node(host, 'span', 'lico', ''); ico.style.background = layer.color || '#5FA875';
      var name = node(host, 'span', 'lnm', layer.name); name.title = layer.name + ' — ' + layer.layerKind;
      row.appendChild(ico); row.appendChild(name);
      // The shy switch hides the marked rows on both panels alike (CLAUDE.md section 11: the two lists stay row-for-row aligned).
      if (s.shyEnabled === true && layer.shy === true) { row.hidden = true; row.style.display = 'none'; }
      row.addEventListener('click', function () {
        if (host._layerDragJustEnded) { host._layerDragJustEnded = false; return; } // the trailing click of a reorder drag must not select
        if (ports.current()) ports.select(i);
      });
      // The eye, the padlock and the row menu: layer-level edits of the component (native-component-layer-edit.js), one native request each.
      if (host.NemoNativeComponentLayerEdit) host.NemoNativeComponentLayerEdit.decorate(host, row, layer, i);
      return row;
    });
    // The selected cells stay painted across a repaint (the legacy selection helpers keep them in `_sel.frames`).
    var picked = new Set((host._sel && Array.isArray(host._sel.frames) ? host._sel.frames : []).map(function (c) { return c.layer + ':' + c.frame; }));
    var frows = st.rows.map(function (layer, i) {
      var row = node(host, 'div', 'frow native-component-row' + (i === active ? ' act' : '')), cells = [];
      row.dataset.layer = String(i); row.dataset.layerUid = layer.layerUid; row.style.position = 'relative'; row.style.width = (st.totalFrames * fc) + 'px';
      if (s.shyEnabled === true && layer.shy === true) { row.hidden = true; row.style.display = 'none'; }
      if (layer.color) { row.style.setProperty('--dot-color', layer.color); }
      for (var fi = 0; fi < st.totalFrames; fi += 1) {
        var cell = node(host, 'div', 'fc' + (fi === frame ? ' cur' : '') + (fi < s.waIn || fi > s.waOut ? ' outside-wa' : '') + (picked.has(i + ':' + fi) ? ' sel' : '')); cell.dataset.frame = String(fi); cell.dataset.layer = String(i);
        if (layer.color) cell.style.setProperty('--dot-color', layer.color);
        cells.push(cell); row.appendChild(cell);
      }
      row.nativeCells = cells;
      return row;
    });
    list.replaceChildren.apply(list, lrows);
    var cap = node(host, 'div', '', caption(st, frame));
    header.replaceChildren(cap);
    grid.replaceChildren.apply(grid, frows);
    st.gridView = { rows: st.rows, list: list, header: header, grid: grid, lrows: lrows, frows: frows, caption: cap, active: active, sel: selKey, waIn: s.waIn, waOut: s.waOut,
      total: st.totalFrames, fc: fc, name: st.name, frame: frame };
    // The cells and the range are functions of the revision alone (the playhead is not an input), so an answer is applied whatever frame it lands on.
    var current = function () { return ports.current() && st.rows === asked.rows; };
    frows.forEach(function (row, i) {
      var layer = st.rows[i];
      Promise.resolve(ports.read('cells', layer.layerUid)).then(function (runs) {
        if (!runs || !current() || row.parentNode !== grid) return;
        applyRuns(row.nativeCells, runs); layer.frames = framesOf(st.totalFrames, runs);
      }).catch(function () { /* a refused read leaves the row blank */ });
      Promise.resolve(ports.read('range', layer.layerUid)).then(function (result) {
        if (!result || !result.range || !current() || row.parentNode !== grid) return;
        paintBar(host, row, result.range);
      }).catch(function () { /* the bar stays absent */ });
    });
    return true;
  }
  // The SCENE rows of a native project (baseline timeline.js renderTimeline: one .fc cell per frame with its key dot, held span and tween mark).
  // `view` is the range grid's own state (native-layer-range-desktop.js: rows, info, frame), `session` the project session. Display only: the cells
  // take no pointer events, so the frame header, the in/out bars and the property keys keep every gesture they own. The runs are a function of the
  // revision alone, so they are read once per painted row set and a frame-only repaint just moves the .cur cell. A refused read leaves the row blank.
  // The one-line timing text ("Key — Source frame 1 — Content", blanked to "unavailable" at the start of every frame's read) describes what the
  // cells show. While every shown row has its cells it is kept for assistive technology but taken out of the layout (never toggled per text
  // change: it sits above the rows, so showing it for the time of a read would shift the grid against the layer list, CLAUDE.md section 11).
  var OFFSCREEN = { position: 'absolute', width: '1px', height: '1px', overflow: 'hidden', clipPath: 'inset(50%)', whiteSpace: 'nowrap' };
  function sceneStatus(grid, covered) {
    var status = grid && grid.querySelector ? grid.querySelector('.native-object-timing-unavailable') : null;
    if (status) Object.keys(OFFSCREEN).forEach(function (key) { status.style[key] = covered ? OFFSCREEN[key] : ''; });
  }
  function sceneCovered(rows) { return rows.length > 0 && rows.every(function (r) { return r.hidden || r.nativeRuns === true; }); }
  function paintScene(host, view, session) {
    var info = view && view.info, rows = view && view.rows, s = host.state;
    if (!info || !Array.isArray(rows) || !s || !session || typeof session.readLayerCells !== 'function') return false;
    var frame = s.currentFrame, fc = Number.isFinite(host.FC) && host.FC > 0 ? host.FC : 14;
    // Motion rows are property lanes (the baseline Motion layer row has no frame cells, layer-inout.js): none there, and any painted before the switch go.
    if (s.appMode === 'motion') {
      rows.forEach(function (row) { (row.nativeCells || []).forEach(function (c) { if (c.parentNode === row) row.removeChild(c); }); row.nativeCells = null; row.nativeRuns = false; row.nativeAsking = false; /* a read still pending is dropped by its own cells check */ });
      sceneStatus(view.grid, false); return true;
    }
    rows.forEach(function (row, li) {
      var layer = info.layers[li];
      if (!layer) return;
      if (!row.nativeCells) {
        var cells = [];
        for (var fi = 0; fi < info.totalFrames; fi += 1) {
          var cell = node(host, 'div', 'fc' + (fi < s.waIn || fi > s.waOut ? ' outside-wa' : '')); cell.dataset.frame = String(fi); cell.dataset.layer = String(li);
          cell.style.position = 'absolute'; cell.style.left = (fi * fc) + 'px'; cell.style.top = '0'; cell.style.width = fc + 'px'; cell.style.height = '100%'; cell.style.pointerEvents = 'none';
          if (layer.color) cell.style.setProperty('--dot-color', layer.color);
          cells.push(cell);
        }
        row.nativeCells = cells; row.nativeCellFrame = -1;
        // Under the bar and the key nodes the range grid puts in the row.
        var first = row.firstChild; cells.forEach(function (c) { row.insertBefore(c, first); });
      }
      // Asked until answered: a read refused while the commit's own repaint keeps the native queue busy (null) waits for the presentation
      // (the shared bounded wait) and, if still refused, is asked again by the next paint.
      if (row.nativeRuns !== true && row.nativeAsking !== true) {
        row.nativeAsking = true;
        var held = row.nativeCells, W = host.NemoNativePresentationWait, shown = function () { return typeof session.readPresentedObjectSession === 'function' ? session.readPresentedObjectSession() : true; };
        Promise.resolve(W && !shown() ? W.await(host, { read: shown }) : null).then(function () { return session.readLayerCells(layer.layerUid, info); }).then(function (runs) {
          row.nativeAsking = false;
          if (view.info !== info || view.rows !== rows || row.nativeCells !== held) return;
          // Still refused after the wait (a project just opened, a long repaint): asked again a few times on its own, since a still timeline has no next paint.
          if (!runs) { row.nativeTries = (row.nativeTries || 0) + 1; if (row.nativeTries <= 8 && typeof host.setTimeout === 'function') host.setTimeout(function () { if (view.rows === rows) paintScene(host, view, session); }, 250); return; }
          applyRuns(held, runs); row.nativeRuns = true; if (held[row.nativeCellFrame]) held[row.nativeCellFrame].classList.add('cur');
          sceneStatus(view.grid, sceneCovered(rows));
        }).catch(function () { row.nativeAsking = false; /* a refused read leaves the row blank */ });
      }
      if (row.nativeCellFrame !== frame) {
        if (row.nativeCells[row.nativeCellFrame]) row.nativeCells[row.nativeCellFrame].classList.remove('cur');
        if (row.nativeCells[frame]) row.nativeCells[frame].classList.add('cur');
        row.nativeCellFrame = frame;
      }
    });
    sceneStatus(view.grid, sceneCovered(rows));
    return true;
  }
  return Object.freeze({ paint: paint, applyRuns: applyRuns, framesOf: framesOf, paintScene: paintScene });
}));
