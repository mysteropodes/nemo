/* #2256: the timeline of an ENTERED native component (baseline timeline.js renderTimeline after enterSymbol, 1065e9d8): one .lrow in #layer-list and
 * one .frow in #frame-grid per component layer, in the same 34px (Motion 22px) rhythm so the two panels stay row-for-row aligned (CLAUDE.md section 11),
 * with the component's OWN frames drawn as the baseline .fc cells (key dot, held span, tween, span end) and its layer range as the read-only in/out bar.
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
  }
  // Paints the entered component's rows and grid. `st` is the controller's entered view ({name, totalFrames, rows}); `ports`: current() true while that
  // view is still the entered one, read(kind, layerUid) the pinned component read (a promise of the result, or null), select(index) activates a layer.
  function paint(host, st, ports) {
    var doc = host.document, list = doc.getElementById('layer-list'), header = doc.getElementById('frame-hdr'), grid = doc.getElementById('frame-grid');
    if (!list || !header || !grid) return true;
    var s = host.state, frame = s.currentFrame, fc = Number.isFinite(host.FC) && host.FC > 0 ? host.FC : 14, active = s.activeLayerIdx, sel = Array.isArray(host._layerSel) ? host._layerSel : [];
    var asked = { rows: st.rows, frame: frame };
    var lrows = st.rows.map(function (layer, i) {
      var row = node(host, 'div', 'lrow native-component-layer' + (i === active ? ' act' : '') + (sel.indexOf(i) >= 0 && sel.length > 1 ? ' sel' : ''));
      row.dataset.layer = String(i); row.dataset.layerUid = layer.layerUid;
      var ico = node(host, 'span', 'lico', ''); ico.style.background = layer.color || '#5FA875';
      var name = node(host, 'span', 'lnm', layer.name); name.title = layer.name + ' — ' + layer.layerKind;
      row.appendChild(ico); row.appendChild(name);
      row.addEventListener('click', function () { if (ports.current()) ports.select(i); });
      return row;
    });
    var frows = st.rows.map(function (layer, i) {
      var row = node(host, 'div', 'frow native-component-row' + (i === active ? ' act' : '')), cells = [];
      row.dataset.layer = String(i); row.dataset.layerUid = layer.layerUid; row.style.position = 'relative'; row.style.width = (st.totalFrames * fc) + 'px';
      if (layer.color) { row.style.setProperty('--dot-color', layer.color); }
      for (var fi = 0; fi < st.totalFrames; fi += 1) {
        var cell = node(host, 'div', 'fc' + (fi === frame ? ' cur' : '') + (fi < s.waIn || fi > s.waOut ? ' outside-wa' : '')); cell.dataset.frame = String(fi); cell.dataset.layer = String(i);
        if (layer.color) cell.style.setProperty('--dot-color', layer.color);
        cells.push(cell); row.appendChild(cell);
      }
      row.nativeCells = cells;
      return row;
    });
    list.replaceChildren.apply(list, lrows);
    header.replaceChildren(node(host, 'div', '', st.name + ' — Frame ' + (frame + 1) + ' / ' + st.totalFrames));
    grid.replaceChildren.apply(grid, frows);
    var current = function () { return ports.current() && st.rows === asked.rows && host.state.currentFrame === asked.frame; };
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
  return Object.freeze({ paint: paint, applyRuns: applyRuns, framesOf: framesOf });
}));
