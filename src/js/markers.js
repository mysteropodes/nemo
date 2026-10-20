// ---- TIMELINE MARKERS (2026-07-25) ----
// After Effects' two marker scopes, which Nemo had neither of:
//   - COMP markers  (state.markers)      — on the ruler strip, project-wide
//   - LAYER markers (ld.markers)         — on that layer's own bar row
// Both are {frame, name, color}. They are annotations only: nothing reads
// them at render time, so they can never desync from the drawing — which is
// also why they live outside the frame data rather than on a frame object.
//
// Why a separate file: markers are additive chrome over the existing grid
// (absolutely positioned overlays anchored to frame*FC), with no hook into
// the stroke/keyframe pipelines at all. Keeping them out of timeline.js's
// already-long render path means the "which reader did I forget" family of
// bug (CLAUDE.md §1) simply doesn't apply here — nothing else consumes them.
(function () {
  var DEFAULT_COLOR = '#e8b64c';
  function native() { return !!(window.__TAURI__ && window.__TAURI__.core); }
  function projection() { var c = window.NemoNativeCompositionController; return c && c.read(); }
  function unavailable() { if (window.showToast) showToast('Native markers are unavailable.'); }
  function intent(action, origin) {
    var c = window.NemoNativeCompositionController;
    if (!c || !origin) { unavailable(); return null; }
    c.edit(action, origin.info); return true;
  }
  function ordinal(m, origin) { return origin && origin.composition.markers.indexOf(m) >= 0 ? m.ordinal : null; }

  function compMarkers() {
    if (native()) { var p = projection(); return p ? p.composition.markers : []; }
    if (!state.markers) state.markers = [];
    return state.markers;
  }
  function layerMarkers(ld) {
    if (native()) return []; // Layer-marker authoring has no admitted native owner yet.
    if (!ld.markers) ld.markers = [];
    return ld.markers;
  }
  function markerAt(list, frame) {
    for (var i = 0; i < list.length; i++) if (list[i].frame === frame) return list[i];
    return null;
  }
  function sortMarkers(list) { list.sort(function (a, b) { return a.frame - b.frame; }); }

  // ---- model ----------------------------------------------------------
  function addCompMarker(frame, name, origin) {
    if (native()) {
      origin = arguments.length > 2 ? origin : projection();
      frame = Math.max(0, Math.min(state.totalFrames - 1, frame == null ? state.currentFrame : frame));
      return intent({ kind: 'addMarker', frame: frame, name: name || '', color: DEFAULT_COLOR }, origin);
    }
    var list = compMarkers();
    frame = Math.max(0, Math.min(state.totalFrames - 1, frame == null ? state.currentFrame : frame));
    if (markerAt(list, frame)) return null; // one per frame, like AE
    if (window.pushUndo) pushUndo();
    var m = { frame: frame, name: name || '', color: DEFAULT_COLOR };
    list.push(m); sortMarkers(list);
    refresh();
    return m;
  }
  function addLayerMarker(li, frame, name) {
    if (native()) { unavailable(); return null; }
    var ld = state.layers[li]; if (!ld) return null;
    var list = layerMarkers(ld);
    frame = Math.max(0, Math.min(state.totalFrames - 1, frame == null ? state.currentFrame : frame));
    if (markerAt(list, frame)) return null;
    if (window.pushUndo) pushUndo();
    var m = { frame: frame, name: name || '', color: ld.color || DEFAULT_COLOR };
    list.push(m); sortMarkers(list);
    refresh();
    return m;
  }
  function removeMarker(list, m, origin) {
    if (native()) { var index = ordinal(m, origin); if (index) intent({ kind: 'removeMarker', ordinal: index }, origin); return; }
    var i = list.indexOf(m);
    if (i < 0) return;
    if (window.pushUndo) pushUndo();
    list.splice(i, 1);
    refresh();
  }
  function renameMarker(list, m, origin) {
    if (native() && !ordinal(m, origin)) { unavailable(); return; }
    var v = prompt('Nom du repère', m.name || '');
    if (v === null) return;
    if (native()) { intent({ kind: 'updateMarker', ordinal: m.ordinal, name: v }, origin); return; }
    if (window.pushUndo) pushUndo();
    m.name = v;
    refresh();
  }
  // Jump to the next/previous marker from the playhead — the reason markers
  // earn their keep on a long timeline. Comp markers only: layer markers
  // belong to one layer, so "next marker" would be ambiguous across a
  // selection.
  function gotoAdjacent(dir) {
    var list = compMarkers().slice();
    sortMarkers(list);
    var target = null;
    for (var i = 0; i < list.length; i++) {
      if (dir > 0 && list[i].frame > state.currentFrame) { target = list[i]; break; }
      if (dir < 0 && list[i].frame < state.currentFrame) target = list[i]; // keep the last one before
    }
    if (!target) { if (window.showToast) showToast(dir > 0 ? SM.t('toastNoMarkerAfter') : SM.t('toastNoMarkerBefore')); return; }
    if (window.goToFrame) goToFrame(target.frame);
    else { state.currentFrame = target.frame; if (window.loadFrame) loadFrame(target.frame); }
  }

  function refresh() {
    render();
    if (window.renderTimeline) renderTimeline();
  }

  // ---- rendering ------------------------------------------------------
  // One pass, called from renderTimeline (so it survives every rebuild) and
  // after any mutation. Overlays are re-created rather than diffed: there
  // are a handful of markers at most, and a rebuild keeps this immune to the
  // stale-DOM issues a diff would introduce across grid re-renders.
  function render() {
    var barsRow = document.getElementById('bars-row');
    if (!barsRow) return;
    Array.prototype.slice.call(document.querySelectorAll('.tl-marker')).forEach(function (el) { el.remove(); });

    // Markers sitting past the end of the CURRENT timeline are skipped rather
    // than clamped (2026-08-16): shrinking the project length leaves marker
    // data alone on purpose — same contract as the frames array, which is
    // never truncated either — but a marker at frame 90 on a 30-frame timeline
    // was still being placed at 90*FC, drawing three ruler-widths past the
    // end. Clamping the DATA instead would pile every out-of-range marker onto
    // the last frame and silently merge them; skipping the OVERLAY keeps each
    // marker exactly where the user put it, ready to reappear if the timeline
    // grows back.
    function inRange(m) { return m.frame >= 0 && m.frame < state.totalFrames; }
    compMarkers().filter(inRange).forEach(function (m) { barsRow.appendChild(buildEl(m, compMarkers(), null)); });

    // Layer markers ride their own row in the grid, so they scroll with it
    // and land exactly on the layer they annotate.
    if (native()) return;
    state.layers.forEach(function (ld, li) {
      if (!ld.markers || !ld.markers.length) return;
      var row = document.querySelector('#frame-grid .frow[data-layer="' + li + '"]');
      if (!row) return;
      if (getComputedStyle(row).position === 'static') row.style.position = 'relative';
      ld.markers.filter(inRange).forEach(function (m) { row.appendChild(buildEl(m, ld.markers, li)); });
    });
  }

  function buildEl(m, list, li) {
    var origin = native() ? projection() : null;
    var el = document.createElement('div');
    el.className = 'tl-marker' + (li == null ? ' tl-marker-comp' : ' tl-marker-layer');
    el.style.left = (m.frame * FC) + 'px';
    el.style.setProperty('--marker-color', m.color || DEFAULT_COLOR);
    el.title = (m.name ? m.name + ' — ' : '') + 'frame ' + (m.frame + 1) +
      '\nGlisser pour déplacer · double-clic pour renommer · clic droit pour supprimer';
    if (m.name) {
      var lab = document.createElement('span');
      lab.className = 'tl-marker-label';
      lab.textContent = m.name;
      el.appendChild(lab);
    }
    el.addEventListener('mousedown', function (e) {
      if (e.button !== 0) return;
      e.stopPropagation(); e.preventDefault();
      startDrag(m, list, e, origin);
    });
    el.addEventListener('dblclick', function (e) { e.stopPropagation(); e.preventDefault(); renameMarker(list, m, origin); });
    el.addEventListener('contextmenu', function (e) {
      e.preventDefault(); e.stopPropagation();
      if (!window.showContextMenu) { removeMarker(list, m, origin); return; }
      window.showContextMenu(e.clientX, e.clientY, [
        { label: 'Renommer le repère…', action: function () { renameMarker(list, m, origin); } },
        { label: 'Aller à ce repère', action: function () { if (window.goToFrame) goToFrame(m.frame); } },
        { sep: true },
        { label: 'Supprimer le repère', action: function () { removeMarker(list, m, origin); } },
      ]);
    });
    return el;
  }

  // Drag to retime a marker. One undo step for the whole drag (taken at
  // mousedown), same convention as the layer bars' own drag.
  var _drag = null;
  function startDrag(m, list, e, origin) {
    if (native()) {
      if (!ordinal(m, origin)) { unavailable(); return; }
      _drag = { m: m, startX: e.clientX, orig: m.frame, frame: m.frame, origin: origin, element: e.currentTarget };
      return;
    }
    if (window.pushUndo) pushUndo();
    _drag = { m: m, list: list, startX: e.clientX, orig: m.frame };
  }
  document.addEventListener('mousemove', function (e) {
    if (!_drag) return;
    var dx = Math.round((e.clientX - _drag.startX) / FC);
    var nf = Math.max(0, Math.min(state.totalFrames - 1, _drag.orig + dx));
    if (_drag.origin) {
      _drag.frame = nf;
      if (_drag.element) _drag.element.style.left = (nf * FC) + 'px';
      return;
    }
    if (nf === _drag.m.frame) return;
    _drag.m.frame = nf;
    render();
  });
  document.addEventListener('mouseup', function () {
    if (!_drag) return;
    if (_drag.origin) {
      var drag = _drag; _drag = null;
      if (drag.frame !== drag.orig) intent({ kind: 'updateMarker', ordinal: drag.m.ordinal, frame: drag.frame }, drag.origin);
      render(); return;
    }
    sortMarkers(_drag.list);
    _drag = null;
    render();
  });

  // ---- entry points ---------------------------------------------------
  // The ruler's own context menu is where "add a marker here" belongs — it
  // is the strip the marker will appear on, and the click already carries
  // the frame.
  function frameFromEvent(e) {
    var wrap = document.getElementById('fg-wrap');
    var grid = document.getElementById('frame-grid');
    if (!wrap || !grid) return state.currentFrame;
    var x = e.clientX - grid.getBoundingClientRect().left;
    return Math.max(0, Math.min(state.totalFrames - 1, Math.floor(x / FC)));
  }
  function initRulerMenu() {
    var bars = document.getElementById('bars-row');
    var hdr = document.getElementById('frame-hdr');
    [bars, hdr].forEach(function (el) {
      if (!el) return;
      el.addEventListener('contextmenu', function (e) {
        if (e.target.closest('.tl-marker, .wa-handle, .onion-marker')) return; // their own menus
        if (!window.showContextMenu) return;
        e.preventDefault(); e.stopPropagation();
        var f = frameFromEvent(e), origin = native() ? projection() : null;
        window.showContextMenu(e.clientX, e.clientY, [
          { label: 'Ajouter un repère ici (frame ' + (f + 1) + ')', action: function () { addCompMarker(f, '', origin); } },
          { label: 'Ajouter un repère nommé…', action: function () {
            var v = prompt('Nom du repère', '');
            if (v !== null) addCompMarker(f, v, origin);
          } },
          { sep: true },
          { label: 'Repère suivant  (Maj+→)', action: function () { gotoAdjacent(1); } },
          { label: 'Repère précédent  (Maj+←)', action: function () { gotoAdjacent(-1); } },
          { sep: true },
          { label: 'Grille BPM…', action: function () { if (window.SMBpm) SMBpm.openSettings(); } },
          { label: 'Réduire la composition à la zone de travail', action: function () { if (window.SM && SM.trimToWorkArea) SM.trimToWorkArea(); } },
          { sep: true },
          { label: 'Supprimer tous les repères', disabled: !compMarkers().length, action: function () {
            if (native()) { intent({ kind: 'clearMarkers' }, origin); return; }
            if (window.pushUndo) pushUndo();
            state.markers = [];
            refresh();
          } },
        ]);
      }, true);
    });
  }

  function initKeys() {
    document.addEventListener('keydown', function (e) {
      if (e.target && /input|textarea|select/i.test(e.target.tagName)) return;
      if (e.metaKey || e.ctrlKey) return;
      // `*` — AE's own "add marker at the playhead" key.
      if (e.key === '*') {
        e.preventDefault();
        var m = addCompMarker(state.currentFrame, '');
        if (!m && window.showToast) showToast(SM.t('toastMarkerAlreadyExistsOnFrame'));
        return;
      }
      if (e.shiftKey && (e.key === 'ArrowRight' || e.key === 'ArrowLeft')) {
        // Only when there ARE comp markers — Shift+Arrow is a common
        // shortcut elsewhere, and silently stealing it when the feature is
        // unused would be worse than not having it.
        if (!compMarkers().length) return;
        e.preventDefault();
        gotoAdjacent(e.key === 'ArrowRight' ? 1 : -1);
      }
    });
  }

  function init() { initRulerMenu(); initKeys(); render(); }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();

  window.SMMarkers = {
    render: render,
    addCompMarker: addCompMarker,
    addLayerMarker: addLayerMarker,
    gotoAdjacent: gotoAdjacent,
    compMarkers: compMarkers,
    layerMarkers: layerMarkers,
  };
})();
