// ---- Dockable tools panel (2026-07) ----
// "Ajoute la possibilité en drag du panel de réorganiser la position du
// panel gauche en haut/à droite/en bas" — dragging #tools-panel-handle
// shows 4 edge drop zones over #top-area; dropping on one re-docks the
// whole panel there. Persisted to localStorage so it survives reload.
(function () {
  var DOCK_KEY = 'nemo-tools-dock';
  var DOCKS = ['top', 'right', 'bottom']; // 'left' is the default, no class needed

  function applyDock(pos) {
    var panel = document.getElementById('tools-panel');
    var area = document.getElementById('top-area');
    if (!panel || !area) return;
    DOCKS.forEach(function (d) { panel.classList.remove('tools-dock-' + d); });
    if (pos !== 'left') panel.classList.add('tools-dock-' + pos);
    // Right dock stays fully in-flow (no CSS order trick — that landed the
    // panel PAST #props-panel, at the outer screen edge, when the actual
    // ask was "entre le canvas et le menu droite": tucked between
    // #canvas-col and #props-panel, same spot a real docking app like
    // Krita/Photoshop would put it). Always reset to the natural default
    // (first child) before deciding where it really belongs, so this stays
    // idempotent regardless of which dock we're coming from.
    if (area.firstChild !== panel) area.insertBefore(panel, area.firstChild);
    if (pos === 'right') {
      var propsResize = document.getElementById('props-panel-resize');
      if (propsResize) area.insertBefore(panel, propsResize);
    }
    // Hides #tools-panel-resize (only meaningful for the left dock) and
    // lets #canvas-col/#props-panel reclaim the space the panel isn't
    // occupying in the flex flow anymore once docked elsewhere.
    document.body.classList.toggle('tools-docked-away', pos !== 'left');
    // Top/bottom float as an overlay (position:absolute — see the CSS
    // comment) rather than staying in-flow like left/right, so their
    // siblings need compensating padding or the project-tabs bar (top) /
    // bottom of the canvas (bottom) ends up hidden underneath it.
    document.body.classList.toggle('tools-dock-top-active', pos === 'top');
    document.body.classList.toggle('tools-dock-bottom-active', pos === 'bottom');
    try { localStorage.setItem(DOCK_KEY, pos); } catch (e) {}
  }
  function currentDock() {
    try {
      var v = localStorage.getItem(DOCK_KEY);
      return (v === 'left' || DOCKS.indexOf(v) >= 0) ? v : 'left';
    } catch (e) { return 'left'; }
  }
  // Applied immediately (not waiting for DOMContentLoaded) so there's no
  // visible flash of the default left position before the saved one kicks
  // in — #tools-panel already exists in the initial HTML parse by the time
  // this script tag runs (it's placed after the body markup).
  applyDock(currentDock());

  var zonesEl = null, dragging = false;
  function buildZones() {
    var area = document.getElementById('top-area');
    if (!area) return null;
    var wrap = document.createElement('div');
    wrap.id = 'tools-dock-zones';
    ['left', 'top', 'right', 'bottom'].forEach(function (d) {
      var z = document.createElement('div');
      z.className = 'tools-dock-zone tools-dock-zone-' + d;
      z.dataset.dock = d;
      wrap.appendChild(z);
    });
    area.appendChild(wrap);
    return wrap;
  }
  function zoneAt(x, y) {
    if (!zonesEl) return null;
    var els = zonesEl.querySelectorAll('.tools-dock-zone');
    for (var i = 0; i < els.length; i++) {
      var r = els[i].getBoundingClientRect();
      if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) return els[i];
    }
    return null;
  }
  function onMove(e) {
    if (!dragging) return;
    var hit = zoneAt(e.clientX, e.clientY);
    var els = zonesEl.querySelectorAll('.tools-dock-zone');
    for (var i = 0; i < els.length; i++) els[i].classList.toggle('active', els[i] === hit);
  }
  function endDrag(e) {
    if (!dragging) return;
    dragging = false;
    var hit = e ? zoneAt(e.clientX, e.clientY) : null;
    if (hit) applyDock(hit.dataset.dock);
    if (zonesEl) { zonesEl.remove(); zonesEl = null; }
    document.removeEventListener('pointermove', onMove);
    document.removeEventListener('pointerup', endDrag);
    document.removeEventListener('pointercancel', endDrag);
  }
  function initHandle() {
    // One delegated listener on #top-area rather than re-binding to the
    // handle every time it moves between docks — the handle element
    // itself is never recreated (applyDock only toggles classes), so a
    // direct binding would work too, but delegation is one less thing to
    // keep in sync if that ever changes.
    document.addEventListener('pointerdown', function (e) {
      if (!e.target.closest || !e.target.closest('#tools-panel-handle')) return;
      e.preventDefault();
      dragging = true;
      zonesEl = buildZones();
      document.addEventListener('pointermove', onMove);
      document.addEventListener('pointerup', endDrag);
      document.addEventListener('pointercancel', endDrag);
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', initHandle);
  else initHandle();
})();
