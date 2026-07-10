// ---- CAMERA LAYER (v18) ----
// Classic 2D-animation camera (TVPaint/Callipeg-style, per the user's
// reference mockup): ONE camera per project, keyframed on its own timeline
// row. Each key stores the camera rect (center x,y + width; height is
// derived from the project aspect so resizing IS zooming — the camera
// always frames exactly what the output will show). Between keys, center
// and width interpolate along a per-segment cubic-bezier easing curve
// (editable in the right panel, same mental model as CSS cubic-bezier /
// Animate's custom ease). The canvas shows every key's rect dashed with a
// center cross, corner-to-corner dashed links between consecutive keys
// (matching the reference image), and the interpolated current-frame rect
// solid. "Vue caméra" locks the viewport to the interpolated rect while
// scrubbing/playing; export (export.js) bakes the same transform into
// every rendered frame, so what the camera frames is what the MP4 shows.
//
// Deliberately NOT part of state.layers: a camera key is not stroke data,
// none of the layer.children consumers (CLAUDE.md's "family of bug #1"
// list) should ever see it. It lives as its own top-of-timeline row,
// rendered by hooks called from renderLayerList()/renderTimeline().
(function () {
  var DEFAULT_EASE = [0.42, 0, 0.58, 1]; // easeInOut — the least surprising default for camera moves

  function ensureState() {
    if (!state.cameraKeys) state.cameraKeys = [];       // persisted: [{frame,x,y,w,ease:[x1,y1,x2,y2]}] sorted by frame
    if (state.cameraLayerOn === undefined) state.cameraLayerOn = false; // persisted: the camera row exists
    if (state.cameraView === undefined) state.cameraView = false;       // session-only: viewport locked to camera
  }
  ensureState();

  function aspect() { return state.canvasH / state.canvasW; }
  function keyHeight(k) { return k.w * aspect(); }

  // Solve a CSS-style cubic-bezier((x1,y1),(x2,y2)) for y at x=t.
  // Newton first (fast for well-behaved curves), bisection fallback so a
  // degenerate user-dragged curve can never hang or return NaN.
  function bezierEase(t, x1, y1, x2, y2) {
    if (t <= 0) return 0;
    if (t >= 1) return 1;
    function bx(u) { var v = 1 - u; return 3 * v * v * u * x1 + 3 * v * u * u * x2 + u * u * u; }
    function by(u) { var v = 1 - u; return 3 * v * v * u * y1 + 3 * v * u * u * y2 + u * u * u; }
    function dbx(u) { var v = 1 - u; return 3 * v * v * x1 + 6 * v * u * (x2 - x1) + 3 * u * u * (1 - x2); }
    var u = t;
    for (var i = 0; i < 8; i++) {
      var d = dbx(u);
      if (Math.abs(d) < 1e-6) break;
      var err = bx(u) - t;
      if (Math.abs(err) < 1e-5) return by(u);
      u = Math.max(0, Math.min(1, u - err / d));
    }
    var lo = 0, hi = 1;
    for (var j = 0; j < 24; j++) {
      u = (lo + hi) / 2;
      if (bx(u) < t) lo = u; else hi = u;
    }
    return by(u);
  }

  function sortKeys() { state.cameraKeys.sort(function (a, b) { return a.frame - b.frame; }); }
  function keyAt(frame) {
    return state.cameraKeys.find(function (k) { return k.frame === frame; }) || null;
  }
  function defaultRect() {
    return { x: state.canvasW / 2, y: state.canvasH / 2, w: state.canvasW, rot: 0 };
  }
  function snap(k) { return { x: k.x, y: k.y, w: k.w, rot: k.rot || 0 }; }
  // The camera rect at ANY frame: exact key, interpolated between keys, or
  // clamped to the first/last key outside the keyed range. Null when the
  // camera layer has no keys at all. The CENTER travels along a cubic
  // bezier whose control points are each key's spatial handles (hOut on
  // the departing key, hIn on the arriving one — [0,0] defaults collapse
  // to the straight line, so pre-v19 projects behave identically); width
  // (zoom) and rot (roll) lerp along the same eased t.
  function cameraAtFrame(frame) {
    ensureState();
    var ks = state.cameraKeys;
    if (!ks.length) return null;
    if (frame <= ks[0].frame) return snap(ks[0]);
    var last = ks[ks.length - 1];
    if (frame >= last.frame) return snap(last);
    for (var i = 0; i < ks.length - 1; i++) {
      var a = ks[i], b = ks[i + 1];
      if (frame >= a.frame && frame < b.frame) {
        var t = (frame - a.frame) / (b.frame - a.frame);
        var e = a.ease || DEFAULT_EASE;
        var y = bezierEase(t, e[0], e[1], e[2], e[3]);
        var ho = a.hOut || [0, 0], hi = b.hIn || [0, 0];
        var p1x = a.x + ho[0], p1y = a.y + ho[1], p2x = b.x + hi[0], p2y = b.y + hi[1];
        var v = 1 - y;
        var px = v * v * v * a.x + 3 * v * v * y * p1x + 3 * v * y * y * p2x + y * y * y * b.x;
        var py = v * v * v * a.y + 3 * v * v * y * p1y + 3 * v * y * y * p2y + y * y * y * b.y;
        return { x: px, y: py, w: a.w + (b.w - a.w) * y, rot: (a.rot || 0) + ((b.rot || 0) - (a.rot || 0)) * y };
      }
    }
    return snap(last);
  }
  function setKey(frame, rect) {
    ensureState();
    var k = keyAt(frame);
    if (k) { k.x = rect.x; k.y = rect.y; k.w = rect.w; if (rect.rot !== undefined) k.rot = rect.rot; }
    else {
      state.cameraKeys.push({ frame: frame, x: rect.x, y: rect.y, w: rect.w, rot: rect.rot || 0, hOut: [0, 0], hIn: [0, 0], ease: DEFAULT_EASE.slice() });
      sortKeys();
    }
    return keyAt(frame);
  }
  function removeKey(frame) {
    ensureState();
    var i = state.cameraKeys.findIndex(function (k) { return k.frame === frame; });
    if (i >= 0) state.cameraKeys.splice(i, 1);
  }
  // The segment whose ease governs the CURRENT frame (its LEFT key). On a
  // key exactly: the segment leaving that key. Null when fewer than 2 keys.
  function segmentLeftKey(frame) {
    var ks = state.cameraKeys;
    if (ks.length < 2) return null;
    if (frame >= ks[ks.length - 1].frame) return ks[ks.length - 2];
    for (var i = ks.length - 2; i >= 0; i--) {
      if (frame >= ks[i].frame) return ks[i];
    }
    return ks[0];
  }

  // ---- canvas overlay (engine scene items, non-destructive — same
  // pattern as the revision outlines / comment pins) ----
  function dashedRectItems(k, col, sw, dash, items) {
    var cs = corners(k);
    items.push({
      segments: cs.map(function (p) { return { point: p }; }),
      closed: true, fillColor: null, strokeColor: col, strokeWidth: sw,
      dashPattern: dash || undefined
    });
  }
  function crossItems(cx, cy, s, col, sw, items) {
    items.push({ segments: [{ point: [cx - s, cy] }, { point: [cx + s, cy] }], closed: false, fillColor: null, strokeColor: col, strokeWidth: sw });
    items.push({ segments: [{ point: [cx, cy - s] }, { point: [cx, cy + s] }], closed: false, fillColor: null, strokeColor: col, strokeWidth: sw });
  }
  function rotPt(px, py, cx, cy, deg) {
    if (!deg) return [px, py];
    var a = deg * Math.PI / 180, c = Math.cos(a), s = Math.sin(a);
    var dx = px - cx, dy = py - cy;
    return [cx + dx * c - dy * s, cy + dx * s + dy * c];
  }
  function corners(k) {
    var h = keyHeight(k), r = k.rot || 0;
    return [
      rotPt(k.x - k.w / 2, k.y - h / 2, k.x, k.y, r), rotPt(k.x + k.w / 2, k.y - h / 2, k.x, k.y, r),
      rotPt(k.x + k.w / 2, k.y + h / 2, k.x, k.y, r), rotPt(k.x - k.w / 2, k.y + h / 2, k.x, k.y, r),
    ];
  }
  function buildOverlayItems() {
    ensureState();
    if (!state.cameraLayerOn) return [];
    var items = [];
    var zs = 1 / Math.max(0.0001, view.zoom);
    var keyCol = [160, 160, 170, 220];
    var curCol = [255, 170, 40, 255]; // accent : rect interpolé de la frame courante
    var dash = [6 * zs, 5 * zs];
    state.cameraKeys.forEach(function (k) {
      dashedRectItems(k, keyCol, 1.5 * zs, dash, items);
      crossItems(k.x, k.y, 7 * zs, keyCol, 1.5 * zs, items);
    });
    // liens pointillés coin-à-coin entre clés consécutives (cf. mockup de réf)
    for (var i = 0; i < state.cameraKeys.length - 1; i++) {
      var ca = corners(state.cameraKeys[i]), cb = corners(state.cameraKeys[i + 1]);
      for (var c = 0; c < 4; c++) {
        items.push({ segments: [{ point: ca[c] }, { point: cb[c] }], closed: false, fillColor: null, strokeColor: [130, 130, 140, 130], strokeWidth: 1 * zs, dashPattern: [3 * zs, 4 * zs] });
      }
      // trajectoire réelle du centre (bezier spatiale) échantillonnée —
      // droite quand les poignées sont à [0,0]
      var a2 = state.cameraKeys[i], b2 = state.cameraKeys[i + 1];
      var ho = a2.hOut || [0, 0], hi = b2.hIn || [0, 0];
      if (ho[0] || ho[1] || hi[0] || hi[1]) {
        var pts = [];
        for (var s2 = 0; s2 <= 20; s2++) {
          var t2 = s2 / 20, v2 = 1 - t2;
          pts.push({ point: [
            v2 * v2 * v2 * a2.x + 3 * v2 * v2 * t2 * (a2.x + ho[0]) + 3 * v2 * t2 * t2 * (b2.x + hi[0]) + t2 * t2 * t2 * b2.x,
            v2 * v2 * v2 * a2.y + 3 * v2 * v2 * t2 * (a2.y + ho[1]) + 3 * v2 * t2 * t2 * (b2.y + hi[1]) + t2 * t2 * t2 * b2.y] });
        }
        items.push({ segments: pts, closed: false, fillColor: null, strokeColor: [255, 170, 40, 160], strokeWidth: 1.2 * zs, dashPattern: [4 * zs, 3 * zs] });
      }
    }
    var cam = cameraAtFrame(state.currentFrame);
    if (cam) {
      var isKey = !!keyAt(state.currentFrame);
      dashedRectItems(cam, curCol, 2 * zs, isKey ? undefined : dash, items);
      crossItems(cam.x, cam.y, 9 * zs, curCol, 2 * zs, items);
      if (state.tool === 'camera') {
        // poignées de resize aux 4 coins
        corners(cam).forEach(function (pt) {
          var s = 5 * zs;
          items.push({ segments: [{ point: [pt[0] - s, pt[1] - s] }, { point: [pt[0] + s, pt[1] - s] }, { point: [pt[0] + s, pt[1] + s] }, { point: [pt[0] - s, pt[1] + s] }], closed: true, fillColor: [255, 170, 40, 255], strokeColor: [30, 30, 30, 255], strokeWidth: 1 * zs });
        });
        // poignée de rotation (roll) : tige + pastille au-dessus du bord haut
        var stalk = rotationHandlePos(cam);
        var topMid = rotPt(cam.x, cam.y - keyHeight(cam) / 2, cam.x, cam.y, cam.rot || 0);
        items.push({ segments: [{ point: topMid }, { point: stalk }], closed: false, fillColor: null, strokeColor: curCol, strokeWidth: 1.2 * zs });
        items.push({ segments: circleSegs(stalk[0], stalk[1], 6 * zs), closed: true, fillColor: [255, 255, 255, 255], strokeColor: curCol, strokeWidth: 1.5 * zs });
        // poignées spatiales de trajectoire sur chaque clé (petits ronds pleins)
        state.cameraKeys.forEach(function (k, ki) {
          var hs = [];
          if (ki < state.cameraKeys.length - 1) hs.push(['hOut', k.hOut || [0, 0]]);
          if (ki > 0) hs.push(['hIn', k.hIn || [0, 0]]);
          hs.forEach(function (h) {
            var hx = k.x + h[1][0], hy = k.y + h[1][1];
            items.push({ segments: [{ point: [k.x, k.y] }, { point: [hx, hy] }], closed: false, fillColor: null, strokeColor: [255, 170, 40, 140], strokeWidth: 1 * zs });
            items.push({ segments: circleSegs(hx, hy, 4.5 * zs), closed: true, fillColor: [255, 170, 40, 230], strokeColor: [30, 30, 30, 255], strokeWidth: 1 * zs });
          });
        });
      }
    }
    return items;
  }
  function circleSegs(cx, cy, r) {
    var k = r * 0.5523;
    return [
      { point: [cx + r, cy], handleIn: [0, k], handleOut: [0, -k] },
      { point: [cx, cy - r], handleIn: [k, 0], handleOut: [-k, 0] },
      { point: [cx - r, cy], handleIn: [0, -k], handleOut: [0, k] },
      { point: [cx, cy + r], handleIn: [-k, 0], handleOut: [k, 0] },
    ];
  }
  function rotationHandlePos(cam) {
    var d = keyHeight(cam) / 2 + 30 / Math.max(0.0001, view.zoom);
    return rotPt(cam.x, cam.y - d, cam.x, cam.y, cam.rot || 0);
  }

  // ---- gizmo interactions (Paper-native tool events, delegated from
  // tools.js when state.tool === 'camera') ----
  var _drag = null; // {mode:'move'|'resize'|'rotate'|'handle', ...}
  function hitCorner(pt, cam) {
    var tol = 12 / view.zoom;
    var cs = corners(cam);
    for (var i = 0; i < 4; i++) {
      if (Math.abs(pt.x - cs[i][0]) < tol && Math.abs(pt.y - cs[i][1]) < tol) return i;
    }
    return -1;
  }
  // Poignée spatiale (trajectoire) de n'importe quelle clé sous le pointeur.
  function hitTrajectoryHandle(pt) {
    var tol = 10 / view.zoom;
    for (var i = 0; i < state.cameraKeys.length; i++) {
      var k = state.cameraKeys[i];
      var hs = [];
      if (i < state.cameraKeys.length - 1) hs.push(['hOut', k.hOut || [0, 0]]);
      if (i > 0) hs.push(['hIn', k.hIn || [0, 0]]);
      for (var j = 0; j < hs.length; j++) {
        var hx = k.x + hs[j][1][0], hy = k.y + hs[j][1][1];
        if (Math.hypot(pt.x - hx, pt.y - hy) < tol) return { key: k, which: hs[j][0] };
      }
    }
    return null;
  }
  function onDown(event) {
    ensureState();
    if (!state.cameraLayerOn) return;
    var cam = cameraAtFrame(state.currentFrame) || defaultRect();
    var traj = hitTrajectoryHandle(event.point);
    var rotH = rotationHandlePos(cam);
    var onRot = Math.hypot(event.point.x - rotH[0], event.point.y - rotH[1]) < 12 / view.zoom;
    var corner = !traj && !onRot ? hitCorner(event.point, cam) : -1;
    // inside-test dans le repère du rect (inverse-rotation du point)
    var lp = rotPt(event.point.x, event.point.y, cam.x, cam.y, -(cam.rot || 0));
    var inside = Math.abs(lp[0] - cam.x) < cam.w / 2 && Math.abs(lp[1] - cam.y) < keyHeight(cam) / 2;
    if (!traj && !onRot && corner < 0 && !inside) return;
    pushUndo(); // le snapshot (v19) porte aussi cameraKeys — Cmd+Z restaure le cadrage
    if (traj) {
      _drag = { mode: 'handle', key: traj.key, which: traj.which };
      return;
    }
    // Éditer une frame sans clé crée la clé (à partir du rect interpolé) —
    // le geste naturel des apps d'anim : on se place, on cadre, c'est clé.
    var key = keyAt(state.currentFrame) || setKey(state.currentFrame, cam);
    _drag = { mode: onRot ? 'rotate' : corner >= 0 ? 'resize' : 'move', startPt: event.point.clone(), startRect: snap(key), key: key };
  }
  function onDrag(event) {
    if (!_drag) return;
    var k = _drag.key;
    if (_drag.mode === 'handle') {
      k[_drag.which] = [event.point.x - k.x, event.point.y - k.y];
    } else {
      var s = _drag.startRect;
      if (_drag.mode === 'move') {
        k.x = s.x + (event.point.x - _drag.startPt.x);
        k.y = s.y + (event.point.y - _drag.startPt.y);
      } else if (_drag.mode === 'rotate') {
        // angle pointeur→centre ; la poignée vit au-dessus du bord haut (−90°)
        k.rot = Math.atan2(event.point.y - s.y, event.point.x - s.x) * 180 / Math.PI + 90;
      } else {
        // resize aspect-lock : la distance au centre pilote la largeur (zoom)
        var d0 = Math.max(1, Math.abs(_drag.startPt.x - s.x) + Math.abs(_drag.startPt.y - s.y));
        var d1 = Math.abs(event.point.x - s.x) + Math.abs(event.point.y - s.y);
        k.w = Math.max(20, s.w * (d1 / d0));
      }
    }
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    renderCameraRow();
  }
  function onUp() {
    if (!_drag) return;
    _drag = null;
    if (window.updateCameraPanel) updateCameraPanel();
    renderCameraRow();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }

  // ---- timeline row (layer panel + frame grid, prepended by hooks in
  // renderLayerList()/renderTimeline() so both stay aligned) ----
  function renderPanelRow(list) {
    ensureState();
    if (!state.cameraLayerOn) return;
    var row = document.createElement('div');
    row.className = 'lrow camrow' + (state.tool === 'camera' ? ' act' : '');
    // Meme famille d'icones ligne-pleine que le reste du panneau calques
    // (ICO_EYE/ICO_EYE_CLOSED, timeline.js) — pas d'emoji couleur, qui
    // detonnait dans une UI flat monochrome.
    var ico = document.createElement('div'); ico.className = 'lico';
    ico.innerHTML = '<svg viewBox="0 0 24 24" width="14" height="14"><path d="M4 7a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h9a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2H4Z" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="m17 10 4-2.2v8.4L17 14v-4Z" fill="currentColor"/></svg>';
    var eye = document.createElement('div'); eye.className = 'lico'; eye.title = 'Vue caméra — verrouille le viewport sur la caméra';
    eye.innerHTML = state.cameraView ? ICO_EYE : ICO_EYE_CLOSED;
    eye.style.color = state.cameraView ? '#ffaa28' : '';
    eye.addEventListener('click', function (e) {
      e.stopPropagation();
      state.cameraView = !state.cameraView;
      if (!state.cameraView) { state.canvasRotation = 0; if (window.SMEngineBridge) window.SMEngineBridge.renderNow(true); }
      applyCameraView();
      renderLayerList();
    });
    var nm = document.createElement('div'); nm.className = 'lnm'; nm.textContent = 'Caméra'; nm.style.fontWeight = '600';
    row.appendChild(ico); row.appendChild(eye); row.appendChild(nm);
    row.addEventListener('click', function () { window.SM.setTool('camera'); renderLayerList(); });
    row.addEventListener('contextmenu', function (e) {
      e.preventDefault();
      // Right-click also switches to the camera tool first (matching a
      // plain click) so segmentLeftKey(state.currentFrame) below resolves
      // against the right context, and the shared editor panel is visible.
      window.SM.setTool('camera'); renderLayerList();
      window.showContextMenu(e.clientX, e.clientY, [
        { label: 'Modifier la courbe d\'accélération…', action: openCameraEaseEditor },
        { sep: true },
        { label: 'Supprimer le calque caméra', action: function () { state.cameraLayerOn = false; state.cameraKeys = []; state.cameraView = false; if (state.tool === 'camera') window.SM.setTool('select'); renderLayerList(); renderTimeline(); updateCameraPanel(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); } },
      ]);
    });
    list.appendChild(row);
  }
  var _gridRow = null;
  function renderGridRow(grid) {
    ensureState();
    _gridRow = null;
    if (!state.cameraLayerOn) return;
    var row = document.createElement('div'); row.className = 'frow camrow';
    row.style.position = 'relative';
    for (var i = 0; i < state.totalFrames; i++) {
      var c = document.createElement('div');
      c.className = 'fc' + (i === state.currentFrame ? ' cur' : '');
      c.dataset.frame = i;
      if (keyAt(i)) c.classList.add('camkey');
      (function (fi) {
        c.addEventListener('mousedown', function (e) { e.stopPropagation(); goToFrame(fi); window.SM.setTool('camera'); });
      })(i);
      row.appendChild(c);
    }
    grid.appendChild(row);
    _gridRow = row;
    drawSpeedCurves(row);
  }
  // Vitesse/ease inline dans la ligne caméra (demande explicite : les
  // courbes doivent se voir directement dans la timeline, pas seulement
  // dans l'editeur du panneau droit) — un SVG superposé aux cellules de
  // frame, une polyline par segment entre 2 cles, echantillonnant la
  // VRAIE valeur de bezierEase (pas un graphique decoratif) : plat en bas
  // = depart lent, plat en haut = arrivee lente, diagonale = vitesse
  // constante. Clic sur la courbe = comportement identique a un clic sur
  // la cellule (va a cette frame + ouvre l'editeur de courbe du segment).
  function drawSpeedCurves(row) {
    var old = row.querySelector('svg.cam-speed-svg');
    if (old) old.remove();
    var ks = state.cameraKeys;
    if (ks.length < 2) return;
    var w = state.totalFrames * FC, h = 34;
    var svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('class', 'cam-speed-svg');
    svg.setAttribute('width', w); svg.setAttribute('height', h);
    svg.style.cssText = 'position:absolute;left:0;top:0;pointer-events:none;';
    var pad = 4;
    for (var i = 0; i < ks.length - 1; i++) {
      var a = ks[i], b = ks[i + 1], e = a.ease || DEFAULT_EASE;
      var span = b.frame - a.frame;
      if (span <= 0) continue;
      var pts = [];
      var steps = Math.max(4, Math.min(40, span));
      for (var s = 0; s <= steps; s++) {
        var t = s / steps;
        var y = bezierEase(t, e[0], e[1], e[2], e[3]);
        var px = (a.frame + t * span) * FC + FC / 2;
        var py = h - pad - y * (h - 2 * pad);
        pts.push(px.toFixed(1) + ',' + py.toFixed(1));
      }
      var poly = document.createElementNS('http://www.w3.org/2000/svg', 'polyline');
      poly.setAttribute('points', pts.join(' '));
      poly.setAttribute('fill', 'none');
      poly.setAttribute('stroke', '#ffaa28');
      poly.setAttribute('stroke-width', '1.6');
      poly.setAttribute('opacity', '0.85');
      svg.appendChild(poly);
    }
    row.appendChild(svg);
  }
  // Refresh only the key markers on the existing row (drag feedback) —
  // renderTimeline() is a full rebuild, too heavy per mousemove.
  function renderCameraRow() {
    if (!_gridRow) return;
    for (var i = 0; i < _gridRow.children.length; i++) {
      _gridRow.children[i].classList.toggle('camkey', !!keyAt(i));
    }
    drawSpeedCurves(_gridRow);
  }

  // ---- viewport lock ("Vue caméra") + loadFrame hook ----
  function applyCameraView() {
    if (!state.cameraView) return;
    var cam = cameraAtFrame(state.currentFrame);
    if (!cam) return;
    view.center = new Point(cam.x, cam.y);
    view.zoom = view.viewSize.width / cam.w;
    // Roll : réutilise le canal de rotation de viewport existant (radians,
    // déjà synchronisé au moteur Rust via syncViewport) — négatif car
    // tourner la caméra dans un sens fait tourner la scène dans l'autre.
    state.canvasRotation = -(cam.rot || 0) * Math.PI / 180;
    if (window.updZoom) updZoom();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow(true);
  }

  // ---- export : bake le zoom/pan caméra dans chaque frame rendue ----
  // Appelé par exportBuildFrame (export.js) APRÈS construction du contenu.
  // Transforme tout le layer pour que le rect caméra remplisse le canvas de
  // sortie, puis clippe au rect canvas (un zoom-out au-delà du canvas
  // montre du vide — même comportement que les apps de réf).
  function applyToExportLayer(L, frameIdx) {
    ensureState();
    if (!state.cameraLayerOn || !state.cameraKeys.length) return;
    var cam = cameraAtFrame(frameIdx);
    if (!cam) return;
    var s = state.canvasW / cam.w;
    if (cam.rot) L.rotate(-cam.rot, new Point(cam.x, cam.y));
    L.scale(s, new Point(cam.x, cam.y));
    L.translate(new Point(state.canvasW / 2 - cam.x, state.canvasH / 2 - cam.y));
    var clip = new Path.Rectangle({ point: [0, 0], size: [state.canvasW, state.canvasH], insert: false });
    L.insertChild(0, clip);
    L.clipped = true;
  }

  // ---- right panel : clés + bouton d'édition de la courbe du segment ----
  // La courbe elle-même n'a plus son propre mini-éditeur ici — clic-droit
  // sur la ligne caméra (ou le bouton ci-dessous) ouvre le widget partagé
  // de la section Easing Curve, pointé sur l'ease de ce segment au lieu de
  // state.easingCurve (feedback #5pi90 : réutiliser le même menu/clic-droit
  // plutôt que maintenir un second éditeur de courbe séparé et plus limité).
  function updateCameraPanel() {
    ensureState();
    var sec = document.getElementById('camera-sec');
    if (!sec) return;
    var show = state.cameraLayerOn && state.tool === 'camera';
    sec.style.display = show ? '' : 'none';
    if (!show) {
      // Leaving the camera tool also leaves camera-ease-editing mode, so
      // the shared Easing Curve widget falls back to showing the tween
      // curve again instead of staying stuck mid-edit on a now-hidden
      // segment.
      if (window._curveEditor) window._curveEditor.exitCameraSeg();
      return;
    }
    var isKey = !!keyAt(state.currentFrame);
    var addBtn = document.getElementById('btn-cam-addkey');
    if (addBtn) addBtn.textContent = isKey ? 'Supprimer la clé (frame ' + (state.currentFrame + 1) + ')' : 'Ajouter une clé (frame ' + (state.currentFrame + 1) + ')';
    var info = document.getElementById('cam-key-info');
    if (info) {
      var cam = cameraAtFrame(state.currentFrame);
      info.textContent = state.cameraKeys.length + ' clé(s)' + (cam ? ' — ' + Math.round(cam.w) + '×' + Math.round(cam.w * aspect()) + ' @ ' + Math.round(cam.x) + ',' + Math.round(cam.y) : '');
    }
    var editBtn = document.getElementById('btn-cam-ease-edit');
    var seg = segmentLeftKey(state.currentFrame);
    if (editBtn) editBtn.disabled = !seg;
  }
  function openCameraEaseEditor() {
    var seg = segmentLeftKey(state.currentFrame);
    if (!seg) { showToast('Ajoute au moins 2 clés pour avoir une courbe'); return; }
    var ks = state.cameraKeys, next = ks[ks.indexOf(seg) + 1];
    // pushUndo() happens per-drag-start in ui.js's own camera-mode mousedown
    // handler, not here — opening the editor alone isn't an edit yet.
    if (window._curveEditor) window._curveEditor.editCameraSeg(seg, 'Caméra : clé ' + (seg.frame + 1) + ' → ' + (next.frame + 1));
  }

  function initUI() {
    var btn = document.getElementById('btn-camera');
    if (btn) btn.addEventListener('click', function () {
      ensureState();
      if (!state.cameraLayerOn) {
        state.cameraLayerOn = true;
        if (!state.cameraKeys.length) setKey(0, defaultRect());
        renderLayerList(); renderTimeline();
      }
      window.SM.setTool(state.tool === 'camera' ? 'select' : 'camera');
      renderLayerList();
      updateCameraPanel();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    });
    var addKey = document.getElementById('btn-cam-addkey');
    if (addKey) addKey.addEventListener('click', function () {
      var f = state.currentFrame;
      if (keyAt(f)) removeKey(f);
      else setKey(f, cameraAtFrame(f) || defaultRect());
      renderCameraRow(); updateCameraPanel();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    });
    var editEase = document.getElementById('btn-cam-ease-edit');
    if (editEase) editEase.addEventListener('click', openCameraEaseEditor);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', initUI); else initUI();

  window.SMCamera = {
    cameraAtFrame: cameraAtFrame,
    keyAt: keyAt,
    setKey: setKey,
    removeKey: removeKey,
    buildOverlayItems: buildOverlayItems,
    onDown: onDown, onDrag: onDrag, onUp: onUp,
    renderPanelRow: renderPanelRow,
    renderGridRow: renderGridRow,
    renderCameraRow: renderCameraRow,
    applyCameraView: applyCameraView,
    applyToExportLayer: applyToExportLayer,
    updatePanel: updateCameraPanel,
  };
  window.updateCameraPanel = updateCameraPanel;
})();
