// ---- Duplicate canvas viewer (2026-08, AE feature audit 8.4) ----
// "New Viewer" — a second floating panel on the SAME comp, independently
// panned/zoomed, optionally LOCKED to a specific frame while the main
// timeline keeps scrubbing (compare frame 1 and frame 50 side by side, or
// scrub freely while a locked reference frame stays visible).
//
// Own VelloEngine instance on its own <canvas>, entirely independent of
// the main view's engine — the Rust engine already takes a plain
// (canvas,width,height) at creation and a scene-JSON string per render()
// call, nothing ties it to a single global canvas, so a second instance
// is just "do ensureEngine's own dance a second time" with its own pan/
// zoom state instead of Paper.js's view. Scene content comes from
// SMEngineBridge.buildSceneJsonForFrame (engine-bridge.js's one narrow
// seam into its own closure) — same document, same buildSceneJson, just a
// different frame/viewport, never a second parallel scene-serialization
// path (CLAUDE.md §1).
(function () {
  var panel = null, canvas = null, engine2 = null, rafId = null;
  var zoom = 1, centerX = 0, centerY = 0; // world-space viewport state, independent of the main view
  var locked = false, lockedFrame = 0;
  var lastKey = ''; // dirty-check: scene version + frame + viewport, same spirit as engine-bridge's own tick()
  var dragging = false, dragStartX = 0, dragStartY = 0, dragCenterX0 = 0, dragCenterY0 = 0;

  function fitZoom() {
    if (!canvas || !state.canvasW) return 1;
    return Math.min(canvas.width / state.canvasW, canvas.height / state.canvasH) * 0.9;
  }

  function syncViewport2() {
    var pivotWX = state.canvasW / 2, pivotWY = state.canvasH / 2;
    var panX = canvas.width / 2 - centerX * zoom;
    var panY = canvas.height / 2 - centerY * zoom;
    var panAdjX = panX + pivotWX * (zoom - 1);
    var panAdjY = panY + pivotWY * (zoom - 1);
    engine2.set_viewport(panAdjX, panAdjY, zoom, 0, pivotWX, pivotWY, zoom);
  }

  function screenToWorld2(sx, sy) {
    var r = canvas.getBoundingClientRect();
    var dpx = (sx - r.left) * (canvas.width / r.width), dpy = (sy - r.top) * (canvas.height / r.height);
    var pivotWX = state.canvasW / 2, pivotWY = state.canvasH / 2;
    var panX = canvas.width / 2 - centerX * zoom, panY = canvas.height / 2 - centerY * zoom;
    var panAdjX = panX + pivotWX * (zoom - 1), panAdjY = panY + pivotWY * (zoom - 1);
    return { x: (dpx - panAdjX) / zoom, y: (dpy - panAdjY) / zoom };
  }

  function tick2() {
    if (!panel || !engine2) return;
    var frame = locked ? lockedFrame : state.currentFrame;
    var key = window._sceneVersion + '|' + frame + '|' + zoom + '|' + centerX + ',' + centerY + '|' + canvas.width + 'x' + canvas.height;
    if (key !== lastKey) {
      lastKey = key;
      try {
        syncViewport2();
        var json = window.SMEngineBridge.buildSceneJsonForFrame(frame);
        engine2.render(json);
      } catch (e) {
        console.error('[second-viewer] render failed', e);
      }
    }
    rafId = requestAnimationFrame(tick2);
  }

  function updateFrameLabel() {
    var lab = document.getElementById('sv-frame-label');
    if (lab) lab.textContent = (locked ? 'Frame ' + (lockedFrame + 1) + ' (verrouillée)' : 'Frame ' + (state.currentFrame + 1) + ' (suit le playhead)');
  }

  async function ensureEngine2() {
    if (engine2) return true;
    if (!window.GeometryWasm || !window.GeometryWasm.ready) { showToast('Moteur Rust indisponible (wasm non chargé)'); return false; }
    try {
      engine2 = await window.GeometryWasm.create_engine(canvas, canvas.width, canvas.height);
      return true;
    } catch (e) {
      console.error('[second-viewer] engine creation failed', e);
      showToast('Nouvelle vue : échec WebGPU — ' + e);
      return false;
    }
  }

  function buildPanel() {
    panel = document.createElement('div');
    panel.id = 'second-viewer-panel';
    panel.style.cssText = 'position:fixed;top:80px;right:24px;width:420px;height:320px;background:var(--panel,#1c1c26);border:1px solid var(--border2,#333);border-radius:8px;box-shadow:0 8px 32px rgba(0,0,0,.5);z-index:500;display:flex;flex-direction:column;overflow:hidden;resize:both;min-width:240px;min-height:180px;';
    var header = document.createElement('div');
    header.style.cssText = 'display:flex;align-items:center;gap:8px;padding:6px 8px;background:var(--panel2,#242430);cursor:move;user-select:none;font-size:11px;color:var(--text-dim,#999);';
    header.innerHTML = '<span style="font-weight:600;color:var(--text,#ddd)">Nouvelle vue</span><span style="flex:1"></span>';
    var lockLabel = document.createElement('label');
    lockLabel.style.cssText = 'display:flex;align-items:center;gap:4px;cursor:pointer;';
    lockLabel.innerHTML = '<input type="checkbox" id="sv-lock"> Verrouiller sur la frame';
    var closeBtn = document.createElement('button');
    closeBtn.textContent = '✕';
    closeBtn.title = 'Fermer';
    closeBtn.style.cssText = 'background:none;border:none;color:inherit;cursor:pointer;font-size:14px;line-height:1;padding:2px 4px;';
    closeBtn.addEventListener('click', function () { window.SMSecondViewer.close(); });
    header.appendChild(lockLabel);
    header.appendChild(closeBtn);
    var frameLabel = document.createElement('div');
    frameLabel.id = 'sv-frame-label';
    frameLabel.style.cssText = 'padding:3px 8px;font-size:10px;color:var(--text-dim,#999);background:var(--panel2,#242430);border-top:1px solid var(--border2,#333);';
    var canvasWrap = document.createElement('div');
    canvasWrap.style.cssText = 'flex:1;position:relative;background:#2a2a35;';
    canvas = document.createElement('canvas');
    canvas.style.cssText = 'position:absolute;inset:0;width:100%;height:100%;';
    canvasWrap.appendChild(canvas);
    panel.appendChild(header);
    panel.appendChild(frameLabel);
    panel.appendChild(canvasWrap);
    document.body.appendChild(panel);

    // Header drag-to-move — plain pointer events on the header only, the
    // canvas keeps its own pointer events for pan/zoom below.
    var dragPanelX = 0, dragPanelY = 0, draggingPanel = false;
    header.addEventListener('pointerdown', function (e) {
      if (e.target === closeBtn) return;
      draggingPanel = true;
      var r = panel.getBoundingClientRect();
      dragPanelX = e.clientX - r.left; dragPanelY = e.clientY - r.top;
      header.setPointerCapture(e.pointerId);
    });
    header.addEventListener('pointermove', function (e) {
      if (!draggingPanel) return;
      panel.style.left = (e.clientX - dragPanelX) + 'px';
      panel.style.top = (e.clientY - dragPanelY) + 'px';
      panel.style.right = 'auto';
    });
    header.addEventListener('pointerup', function () { draggingPanel = false; });

    document.getElementById('sv-lock').addEventListener('change', function () {
      locked = this.checked;
      if (locked) lockedFrame = state.currentFrame;
      updateFrameLabel();
    });

    // Independent pan (drag) / zoom (wheel) — same math class as the main
    // canvas's own tools.js pan/zoom, but against THIS panel's own
    // zoom/centerX/centerY, never touching Paper.js's view.
    canvas.addEventListener('pointerdown', function (e) {
      dragging = true; dragStartX = e.clientX; dragStartY = e.clientY;
      dragCenterX0 = centerX; dragCenterY0 = centerY;
      canvas.setPointerCapture(e.pointerId);
    });
    canvas.addEventListener('pointermove', function (e) {
      if (!dragging) return;
      centerX = dragCenterX0 - (e.clientX - dragStartX) / zoom;
      centerY = dragCenterY0 - (e.clientY - dragStartY) / zoom;
    });
    canvas.addEventListener('pointerup', function () { dragging = false; });
    canvas.addEventListener('wheel', function (e) {
      e.preventDefault();
      var before = screenToWorld2(e.clientX, e.clientY);
      var factor = Math.pow(1.0015, -e.deltaY);
      zoom = Math.max(0.02, Math.min(40, zoom * factor));
      var after = screenToWorld2(e.clientX, e.clientY);
      centerX += before.x - after.x;
      centerY += before.y - after.y;
    }, { passive: false });

    // Resizable via CSS `resize:both` — keep the canvas's own device-pixel
    // backing store in sync with its CSS box, same reasoning as the main
    // engine's own ResizeObserver (engine-bridge.js's handleResize doc
    // comment: a stale backing-store size gives a blurry/stretched
    // picture AND wrong screen_to_world math).
    // Debounced + threshold-gated (2026-08, found live): a plain "resize
    // whenever the observed rect changes at all" retriggers itself in a
    // tight loop — sub-pixel layout jitter (fractional getBoundingClientRect
    // values that wobble by ~1px between successive reflows, e.g. from the
    // frame-label text updating on its own interval) kept forcing
    // lastKey='' every tick, so tick2 never got a stable frame to actually
    // present — the panel looked intermittently blank/blinking instead of
    // showing the scene. A >1px threshold plus a short settle delay
    // collapses the jitter into one real resize instead of a redraw storm.
    var resizeSettleTimer = null;
    var ro = new ResizeObserver(function () {
      if (resizeSettleTimer) clearTimeout(resizeSettleTimer);
      resizeSettleTimer = setTimeout(function () {
        var r = canvasWrap.getBoundingClientRect();
        var w = Math.max(1, Math.round(r.width)), h = Math.max(1, Math.round(r.height));
        if (Math.abs(canvas.width - w) <= 1 && Math.abs(canvas.height - h) <= 1) return;
        canvas.width = w; canvas.height = h;
        if (engine2) { try { engine2.resize(w, h); } catch (e) { console.error('[second-viewer] resize failed', e); } }
        lastKey = ''; // force re-render at the new (real, settled) size
      }, 80);
    });
    ro.observe(canvasWrap);
  }

  window.SMSecondViewer = {
    open: async function () {
      if (panel) return; // already open — the close button is the only way to get rid of it, opening again is a no-op
      buildPanel();
      var r = canvas.parentNode.getBoundingClientRect();
      canvas.width = Math.max(1, Math.round(r.width));
      canvas.height = Math.max(1, Math.round(r.height));
      var ok = await ensureEngine2();
      if (!ok) { panel.remove(); panel = null; return; }
      zoom = fitZoom();
      centerX = state.canvasW / 2; centerY = state.canvasH / 2;
      updateFrameLabel();
      rafId = requestAnimationFrame(tick2);
      // Keep the frame label current even when NOT locked (playhead moves
      // don't otherwise touch this panel's own DOM) — cheap, once/sec is
      // plenty, no need to piggyback the render tick's own rAF cadence.
      window.__svLabelInterval = setInterval(updateFrameLabel, 200);
    },
    close: function () {
      if (rafId) cancelAnimationFrame(rafId);
      if (window.__svLabelInterval) clearInterval(window.__svLabelInterval);
      rafId = null; engine2 = null;
      if (panel) panel.remove();
      panel = null; canvas = null;
    },
    isOpen: function () { return !!panel; },
  };
})();
