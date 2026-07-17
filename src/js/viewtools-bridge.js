// ---- C7 CUTOVER: Hand/Zoom/Eyedropper tools on the Rust engine ----
// Same capture-phase interception architecture as every other bridge.
// These three are lightweight on purpose — Hand/Zoom only ever touch the
// viewport (view.center/view.zoom), never Paper's own scene graph, and
// Eyedropper only READS colors off whatever's hit-tested and writes them
// into state + a handful of color-picker DOM elements. None of them needed
// their own suspend()/renderWithOverlayItem live-preview machinery the way
// Draw/Shapes do, since there's no in-progress geometry to keep off
// Paper's scene graph during a drag — panning IS the "drag", and it only
// ever mutates the viewport, which engine-bridge.js's own tick() already
// diffs and re-renders on its own.
//
// Loaded BEFORE draw-bridge.js/etc. in index.html (matches tools.js's own
// onMouseDown, which checks `state.tool==='hand'||state.spaceDown` as its
// very FIRST, unconditional check, ahead of every other tool branch) so
// that holding Space to pan mid-drag with another tool active is checked
// first here too, rather than being swallowed by a later bridge's own
// stopImmediatePropagation for whatever tool happens to be active.
(function () {
  var panning = false;
  // Zoom-tool drag state: a click alone still zooms one fixed step (old
  // behavior), but a click-DRAG now ramps the zoom continuously while
  // holding the point under the INITIAL click fixed on screen — the same
  // "drag right to zoom in, drag left to zoom out, anchored where you
  // grabbed" convention as Photoshop/Illustrator's zoom tool. `anchorWorld`
  // is computed once at pointerdown and never re-read from the current
  // (moving) cursor position, which is what makes the anchor point hold
  // still throughout the whole drag instead of chasing the cursor.
  var zooming = false, zoomStartX = 0, zoomStartZoom = 1, zoomAnchorWorld = null, zoomMoved = false;
  // Rotate-tool/Alt-drag state: angle-based, not horizontal-distance-based
  // like zoom — dragging in a circular motion around the stage center is
  // the natural gesture for rotation (Animate's Rotate Stage tool, Photoshop's
  // Rotate View tool), so what's tracked is the ANGLE from the pivot to the
  // cursor at drag-start vs. the current angle; the difference is added to
  // whatever state.canvasRotation already was, so a rotate gesture never
  // "snaps" the stage to a new absolute angle at drag-start.
  var rotating = false, rotateStartAngle = 0, rotateStartRotation = 0;

  function engineOn() { return window.SMEngineBridge && window.SMEngineBridge.isEnabled() && !state.playing; }
  function shouldPan() { return engineOn() && (state.tool === 'hand' || state.spaceDown); }
  function shouldZoom() { return engineOn() && state.tool === 'zoom'; }
  function shouldPick() { return engineOn() && state.tool === 'eyedropper'; }
  // The zoom tool already owns Alt for its own "click = zoom out" modifier
  // (see onDown below) — the global Alt-drag-to-rotate shortcut deliberately
  // excludes it so the two don't fight over the same key while zoom is
  // selected. The brushes (draw/fillbrush) own Alt+drag too: it's their
  // resize-with-visual-circle gesture (draw-bridge.js, feedback #24), the
  // standard brush shortcut in every drawing app. Every other tool gets
  // Alt+drag as a temporary rotate, exactly like Space is a temporary Hand
  // regardless of the active tool.
  // 'fill' added: Alt+drag on the paint-bucket tool draws a temporary
  // closing stroke (fill-bridge.js) — without this exclusion, this
  // capture-phase handler (registered before fill-bridge.js's own) stole
  // every Alt+drag as a canvas rotate before the fill tool ever saw it.
  // 'eraser' added (2026-07, live feedback: "alt+glisser fait tourner le
  // canvas pas la taille de la gomme") — eraser-bridge.js got the exact
  // same Alt+drag resize gesture as draw/fillbrush right after this list
  // was last touched, but was never added here, so this handler (also
  // capture-phase, also registered first) kept stealing the gesture
  // before eraser-bridge.js ever saw it. Same fix as 'fill' above, same
  // root cause.
  // 'select' added (2026-07, "alt+glisser le point d'ancrage fait tourner
  // le canvas") — select-bridge.js's Alt+drag now has its own meaning
  // (relocate the rotate/scale anchor and keep it following the pointer),
  // same recurring root cause as 'fill'/'eraser' above: this handler is
  // capture-phase AND registered before select-bridge.js in index.html, so
  // it stole the gesture regardless of what select-bridge.js's own onDown
  // did with the SAME pointerdown/pointermove events.
  function shouldRotate() { return engineOn() && (state.tool === 'rotate' || (state.altDown && ['zoom', 'draw', 'fillbrush', 'fill', 'eraser', 'select'].indexOf(state.tool) < 0 && !state.spaceDown)); }

  function canvasLocal(clientX, clientY) {
    var r = document.getElementById('drawing-canvas').getBoundingClientRect();
    return new Point(clientX - r.left, clientY - r.top);
  }
  // Stage pivot in CLIENT (screen) coordinates — the artboard center,
  // projected through Paper's current view then offset by the canvas's own
  // position on the page. This is what the rotate gesture visually spins
  // around; recomputed fresh each read since pan/zoom can change between
  // drag-start and drag-move.
  function pivotClient() {
    var local = view.projectToView(new Point(state.canvasW / 2, state.canvasH / 2));
    var r = document.getElementById('drawing-canvas').getBoundingClientRect();
    return { x: r.left + local.x, y: r.top + local.y };
  }
  function angleFromPivot(clientX, clientY) {
    var p = pivotClient();
    return Math.atan2(clientY - p.y, clientX - p.x);
  }
  // Sets zoom to `newZoom` then shifts view.center so `anchorWorld` (a world-
  // space point, captured once at drag-start) still projects to the same
  // canvas-local pixel it was at originally — same before/after-viewToProject
  // trick the wheel handler (tools.js) already uses, factored out here so
  // both the wheel and this drag-to-zoom share identical zoom-to-point math.
  function recenterOn(anchorWorld, localPt, newZoom) {
    view.zoom = Math.max(0.05, Math.min(20, newZoom));
    var nowWorld = view.viewToProject(localPt);
    view.center = view.center.add(anchorWorld.subtract(nowWorld));
  }

  // Mirrors tools.js's own eyedropper DOM updates for the stroke/fill color
  // pickers (swatch value, well background, on/off checkbox for fill).
  function setColorUI(kind, css) {
    if (kind === 'stroke') {
      state.strokeColor = css;
      // .dataset.hex8 alongside .value: the native <input type=color> would
      // otherwise silently truncate an alpha-bearing hex to 6 digits, and
      // every other reader of this input prefers dataset.hex8 for exactly
      // that reason (see color-picker.js's own comment on this).
      ['color-stroke', 'pm-stroke-c'].forEach(function (id) { var el = document.getElementById(id); if (el) { el.value = css; el.dataset.hex8 = css; } });
      ['stroke-well', 'pm-stroke'].forEach(function (id) { var el = document.getElementById(id); if (el) el.style.background = css; });
    } else {
      state.fillColor = css; state.fillEnabled = true;
      ['color-fill', 'pm-fill-c'].forEach(function (id) { var el = document.getElementById(id); if (el) { el.value = css; el.dataset.hex8 = css; } });
      var pmFill = document.getElementById('pm-fill'); if (pmFill) pmFill.style.background = css;
      var fillWell = document.getElementById('fill-well'); if (fillWell) { fillWell.style.background = css; fillWell.classList.remove('none'); }
      var onCb = document.getElementById('p-fill-on'); if (onCb) onCb.checked = true;
    }
  }

  function pick(e) {
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);
    var layer = userLayers[state.activeLayerIdx];
    var hit = layer.hitTest(pt, { stroke: true, fill: true, tolerance: 8 / view.zoom });
    if (!(hit && hit.item instanceof Path)) return;
    var ep = hit.item;
    var isVB = !!(ep.data && ep.data.isVectorBrush);
    if (isVB && ep.fillColor) {
      setColorUI('stroke', ep.fillColor.toCSS(true));
    } else {
      if (ep.strokeColor) setColorUI('stroke', ep.strokeColor.toCSS(true));
      if (ep.fillColor) setColorUI('fill', ep.fillColor.toCSS(true));
    }
    if (ep.strokeWidth) {
      state.brushSize = ep.strokeWidth;
      var sw = document.getElementById('p-sw'); if (sw) sw.value = Math.round(ep.strokeWidth);
    }
    showToast('Color picked');
  }

  function onDown(e) {
    if (shouldPan()) {
      e.stopImmediatePropagation();
      e.preventDefault();
      panning = true;
      state.isPanning = true;
      window.SMEngineBridge.suspend();
      return;
    }
    if (shouldRotate()) {
      e.stopImmediatePropagation();
      e.preventDefault();
      rotating = true;
      rotateStartAngle = angleFromPivot(e.clientX, e.clientY);
      rotateStartRotation = state.canvasRotation || 0;
      return;
    }
    if (shouldZoom()) {
      e.stopImmediatePropagation();
      e.preventDefault();
      zooming = true;
      zoomMoved = false;
      zoomStartX = e.clientX;
      zoomStartZoom = view.zoom;
      zoomAnchorWorld = view.viewToProject(canvasLocal(e.clientX, e.clientY));
      return;
    }
    if (shouldPick()) {
      e.stopImmediatePropagation();
      e.preventDefault();
      pick(e);
      window.SMEngineBridge.renderNow();
      return;
    }
  }
  function onMove(e) {
    if (rotating) {
      e.stopImmediatePropagation();
      e.preventDefault();
      var angleNow = angleFromPivot(e.clientX, e.clientY);
      var newRotation = rotateStartRotation + (angleNow - rotateStartAngle);
      // Shift held: snap to 15° increments, matching every classic 2D-anim
      // app's canvas-rotate gesture (TVPaint/Animate) — makes it trivial to
      // return to a clean 0°/45°/90° angle instead of eyeballing it.
      if (e.shiftKey) {
        var snapStep = Math.PI / 12;
        newRotation = Math.round(newRotation / snapStep) * snapStep;
      }
      state.canvasRotation = newRotation;
      // viewport-only: rotating the stage changes no scene item (overlay
      // handle sizes depend on zoom, not rotation) — reuse the scene JSON
      // instead of a full rebuild per pointermove (see renderNow's comment)
      window.SMEngineBridge.renderNow(true);
      return;
    }
    if (zooming) {
      e.stopImmediatePropagation();
      e.preventDefault();
      var dx = e.clientX - zoomStartX;
      if (Math.abs(dx) > 2) zoomMoved = true;
      if (!zoomMoved) return;
      // Exponential ramp (not linear) so it feels like a continuous zoom
      // gesture rather than a ruler — every ~120px of drag roughly doubles/
      // halves zoom, independent of the zoom level you started from.
      var newZoom = zoomStartZoom * Math.pow(2, dx / 120);
      recenterOn(zoomAnchorWorld, canvasLocal(e.clientX, e.clientY), newZoom);
      updZoom(); renderArcs();
      window.SMEngineBridge.renderNow();
      return;
    }
    if (!panning) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var dx2 = e.movementX || 0, dy2 = e.movementY || 0;
    view.center = view.center.subtract(new Point(dx2, dy2).divide(view.zoom));
    // viewport-only: panning changes no scene item — same reuse as rotate
    window.SMEngineBridge.renderNow(true);
  }
  function onUp(e) {
    if (rotating) {
      e.stopImmediatePropagation();
      e.preventDefault();
      rotating = false;
      window.SMEngineBridge.renderNow();
      return;
    }
    if (zooming) {
      e.stopImmediatePropagation();
      e.preventDefault();
      // A plain click (never dragged past the threshold) still zooms one
      // fixed step, same gesture as before — but now correctly anchored on
      // the clicked point instead of always re-centering on view.center.
      if (!zoomMoved) {
        var factor = e.altKey ? 0.8 : 1.25;
        recenterOn(zoomAnchorWorld, canvasLocal(e.clientX, e.clientY), view.zoom * factor);
        updZoom(); renderArcs();
        window.SMEngineBridge.renderNow();
      }
      zooming = false;
      return;
    }
    if (!panning) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    panning = false;
    state.isPanning = false;
    window.SMEngineBridge.renderNow();
    window.SMEngineBridge.resume();
  }

  function init() {
    var target = document.getElementById('canvas-area') || document.getElementById('drawing-canvas');
    target.addEventListener('pointerdown', onDown, { capture: true });
    target.addEventListener('pointermove', onMove, { capture: true });
    target.addEventListener('pointerup', onUp, { capture: true });
    target.addEventListener('pointercancel', onUp, { capture: true });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
