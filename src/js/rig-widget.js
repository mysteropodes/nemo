// ---- RIG CONTROL WIDGETS (2026-08-30) ----
//
// An on-canvas joystick / slider you drag with the mouse to drive a rig,
// living as its own layer kind (ld.isWidgetLayer) next to Null, Guide and
// Folder. Moho's Smart Bone dials and Rive's Joystick, expressed entirely
// in vocabulary Nemo already ships.
//
// ---- THE ONE DECISION EVERYTHING ELSE FOLLOWS ----
//
// A widget does NOT have a value of its own. Each axis POINTS AT an
// ordinary expression control (motion.js's ld.exprControls, key `xc_…`)
// that lives on this same layer. The widget is an on-canvas EDITOR for a
// control that already exists — not a second value system parallel to the
// first.
//
// That single choice is what makes the following come along with no code
// here at all: a real keyable Motion property row (propsFor lists control
// keys last, CLAUDE.md §11's panel/grid alignment invariant included), the
// stopwatch, ease curves, the graph editor, the expression pickwhip,
// `layerControl("uid","Turn")` read-back from any other layer's expression,
// and persistence through the exprControls line exportJSON already writes.
// Anything that stored the widget's value on the side would have to fight
// every one of those.
//
// min/max/rest are WIDGET PRESENTATION — how far the puck travels and what
// number that maps to. Deliberately not part of the control's declaration:
// the same control read by an expression is just a number, and a second
// widget could map the same control over a different range.
//
// ---- POSITION ----
//
// ld.widget.pos is a WORLD anchor, exactly like ld.guidePos / ld.nullPos.
// The layer's own Position track offsets it and parentChainMats composes
// the parent chain, so a widget is parentable, keyable and animatable with
// zero new machinery (buildGuideLayerItems / buildNullLayerItems,
// engine-bridge.js, do the identical thing — this is the third instance of
// that pattern, not a new one).
//
// NO live object references anywhere in ld.widget (CLAUDE.md §1's `_live`
// rule): every field is a number, a string or an array of those, so the
// whole thing survives JSON.stringify — which is also what makes undo work
// for free, since _cloneLayersForUndo (tweens.js) deep-clones state.layers
// wholesale. There is deliberately no state.widgets store.
//
// ---- NEVER RENDERED ----
//
// Four touch points, the Guide layer precedent exactly, and no Rust change
// at all (engine.rs's LayerIn never learns the concept, so CLAUDE.md §3's
// twin-function hazard is not triggered):
//   1. getEffectiveStrokes (app.js) returns [] — which closes PNG, video,
//      Lottie and Rive export in one line, because export.js and
//      rive-export.js all read through it.
//   2. buildSceneJson's per-layer loop (engine-bridge.js) pushes an empty
//      slot and continues, so stack indices stay correct for mattes and
//      parenting.
//   3. buildRigWidgetOverlayItems (below) is pushed INSIDE
//      `includeEditorOverlays`, which renderFrameRawPixels already sets to
//      false for the single GPU-readback path used by PNG export AND the
//      playback bake.
//   4. Nothing else.
//
// ---- HANDLE CONVENTIONS ----
//
// Copied from buildNodeHandleItems (engine-bridge.js) via
// image-mesh-bridge.js, not invented here: idle blue [74,158,255], accent
// orange [255,184,108], white outline, and every stroke width / hit
// tolerance in N / view.zoom so it stays screen-constant.
(function () {
  'use strict';

  var IDLE = [74, 158, 255, 255];
  var ACCENT = [255, 184, 108, 255];
  var WHITE = [255, 255, 255, 255];
  var TRACK = [120, 170, 255, 170];
  var GHOST = [120, 170, 255, 90];

  var DEFAULTS = {
    joystick: { size: [160, 160], min: -100, max: 100, rest: 0, names: ['X', 'Y'] },
    slider: { size: [200, 26], min: 0, max: 100, rest: 0, names: ['Value'] },
  };

  var drag = null;   // {li, mode:'puck'|'move', moved:boolean, ...}

  function engineOn() { return window.SMEngineBridge && SMEngineBridge.isEnabled() && !state.playing; }

  // ---- data ------------------------------------------------------------
  function widgetOf(ld) {
    if (!ld || !ld.isWidgetLayer) return null;
    var w = ld.widget;
    if (!w || (w.kind !== 'joystick' && w.kind !== 'slider')) return null;
    return w;
  }
  function widgetLayerIndices() {
    var out = [];
    for (var i = 0; i < state.layers.length; i++) if (widgetOf(state.layers[i])) out.push(i);
    return out;
  }
  function controlByKey(ld, key) {
    var list = (window.SMMotion && SMMotion.exprControls) ? SMMotion.exprControls(ld) : (ld.exprControls || []);
    for (var i = 0; i < list.length; i++) if (list[i].key === key) return list[i];
    return null;
  }
  // The label to DRAW. Stored `label` is only the seed: renaming the
  // control from its own Motion row (openExprControlsMenu) has to show up
  // on the canvas, so the live control's name wins whenever it resolves.
  function axisLabel(ld, ax) {
    var c = ax && ax.key ? controlByKey(ld, ax.key) : null;
    return (c && c.name) || (ax && ax.label) || '';
  }
  function axisValue(ld, ax, frame) {
    if (!ax || !ax.key || !window.SMMotion) return 0;
    var v = SMMotion.valueAtFrame(ld, ax.key, frame === undefined ? state.currentFrame : frame);
    return (v && v.length) ? v[0] : 0;
  }
  function span(ax) {
    var s = (ax.max - ax.min);
    return Math.abs(s) < 1e-9 ? 1 : s;
  }
  function clamp01(t) { return t < 0 ? 0 : (t > 1 ? 1 : t); }

  // ---- geometry --------------------------------------------------------
  // Rendered anchor + total rotation, composed the SAME way
  // buildNullLayerItems composes a Null's marker: the layer's own
  // Motion transform (layerMotionAt — a flat dx/dy/rot, no pivot needed
  // since a widget has no content bounds) then the full parent chain.
  // Returns null when this isn't a widget layer.
  function geomFor(li, frame) {
    var ld = state.layers[li], w = widgetOf(ld);
    if (!w || !window.SMMotion) return null;
    var f = frame === undefined ? state.currentFrame : frame;
    var base = w.pos || [state.canvasW / 2, state.canvasH / 2];
    var ownMat = SMMotion.layerMotionAt(li, f);
    var pt = [{ point: [base[0] + (ownMat ? ownMat.dx : 0), base[1] + (ownMat ? ownMat.dy : 0)], handleIn: [0, 0], handleOut: [0, 0] }];
    var angle = (ownMat ? ownMat.rot : 0);
    var chain = SMMotion.parentChainMats(li, f);
    for (var pc = 0; pc < chain.length; pc++) {
      pt = SMMotion.transformSegments(pt, chain[pc].pivot, chain[pc].mat);
      angle += (chain[pc].mat.rot || 0);
    }
    var size = w.size || DEFAULTS[w.kind].size;
    var wd = Math.max(8, size[0]), ht = Math.max(8, size[1]);
    var a = angle * Math.PI / 180;
    return {
      li: li, ld: ld, widget: w, frame: f,
      cx: pt[0].point[0], cy: pt[0].point[1],
      angle: angle, cos: Math.cos(a), sin: Math.sin(a),
      w: wd, h: ht,
    };
  }
  function toWorld(g, lx, ly) {
    return [g.cx + lx * g.cos - ly * g.sin, g.cy + lx * g.sin + ly * g.cos];
  }
  function toLocal(g, wx, wy) {
    var dx = wx - g.cx, dy = wy - g.cy;
    return [dx * g.cos + dy * g.sin, -dx * g.sin + dy * g.cos];
  }
  // Value -> local offset. X grows right; Y is INVERTED so that the axis'
  // max sits at the TOP of the pad, which is what "push the stick up"
  // means to everyone who has ever held one.
  function puckLocal(g) {
    var w = g.widget, ld = g.ld;
    var tx = clamp01((axisValue(ld, w.x, g.frame) - w.x.min) / span(w.x));
    var lx = (tx - 0.5) * g.w;
    var ly = 0;
    if (w.kind === 'joystick' && w.y) {
      var ty = clamp01((axisValue(ld, w.y, g.frame) - w.y.min) / span(w.y));
      ly = (0.5 - ty) * g.h;
    }
    return [lx, ly];
  }
  // Local offset -> value, the exact inverse of puckLocal.
  function valueFromLocal(ax, t) {
    var v = ax.min + clamp01(t) * span(ax);
    return Math.round(v * 1000) / 1000;
  }
  function puckRadius(g) {
    return Math.max(6 / view.zoom, Math.min(g.w, g.h) * 0.12);
  }

  // ---- overlay ---------------------------------------------------------
  function lineItem(a, b, color, sw) {
    return { segments: [{ point: [a[0], a[1]] }, { point: [b[0], b[1]] }], closed: false, fillColor: null, strokeColor: color, strokeWidth: sw };
  }
  function polyItem(pts, color, sw, fill) {
    return { segments: pts.map(function (p) { return { point: [p[0], p[1]] }; }), closed: true, fillColor: fill || null, strokeColor: color, strokeWidth: sw };
  }
  function discItem(c, r, fill, sw) {
    var segs = [];
    for (var i = 0; i < 16; i++) { var a = (i / 16) * Math.PI * 2; segs.push({ point: [c[0] + Math.cos(a) * r, c[1] + Math.sin(a) * r] }); }
    return { segments: segs, closed: true, fillColor: fill, strokeColor: WHITE, strokeWidth: sw };
  }

  function buildRigWidgetOverlayItems() {
    if (!engineOn() || !window.SMMotion) return [];
    var items = [], idxs = widgetLayerIndices();
    for (var n = 0; n < idxs.length; n++) {
      var li = idxs[n], ld = state.layers[li];
      if (ld.visible === false) continue;
      var g = geomFor(li);
      if (!g) continue;
      var sw = 1.4 / view.zoom, hw = g.w / 2, hh = g.h / 2;
      var col = (drag && drag.li === li) ? ACCENT : IDLE;
      // Frame. A joystick is a square pad, a slider a flat track — the
      // silhouette alone should say which one you are looking at before
      // you find the puck.
      items.push(polyItem([toWorld(g, -hw, -hh), toWorld(g, hw, -hh), toWorld(g, hw, hh), toWorld(g, -hw, hh)], col, sw, null));
      if (g.widget.kind === 'joystick') {
        items.push(lineItem(toWorld(g, -hw, 0), toWorld(g, hw, 0), GHOST, sw * 0.7));
        items.push(lineItem(toWorld(g, 0, -hh), toWorld(g, 0, hh), GHOST, sw * 0.7));
      } else {
        items.push(lineItem(toWorld(g, -hw, 0), toWorld(g, hw, 0), TRACK, sw));
      }
      // Rest marker — where the puck returns to on a double-click, drawn
      // faintly so it reads as a reference and not as a second handle.
      var rl = restLocal(g);
      items.push(lineItem(toWorld(g, rl[0] - 4 / view.zoom, rl[1]), toWorld(g, rl[0] + 4 / view.zoom, rl[1]), GHOST, sw * 0.7));
      var pl = puckLocal(g), pw = toWorld(g, pl[0], pl[1]), pr = puckRadius(g);
      if (g.widget.kind === 'joystick') items.push(lineItem([g.cx, g.cy], pw, TRACK, sw));
      items.push(discItem(pw, pr, col, pr * 0.22));
    }
    return items;
  }
  function restLocal(g) {
    var w = g.widget;
    var tx = clamp01(((w.x.rest === undefined ? w.x.min : w.x.rest) - w.x.min) / span(w.x));
    var lx = (tx - 0.5) * g.w, ly = 0;
    if (w.kind === 'joystick' && w.y) {
      var ty = clamp01(((w.y.rest === undefined ? w.y.min : w.y.rest) - w.y.min) / span(w.y));
      ly = (0.5 - ty) * g.h;
    }
    return [lx, ly];
  }
  window.buildRigWidgetOverlayItems = buildRigWidgetOverlayItems;

  // ---- writing ---------------------------------------------------------
  // ONE write path: SMMotion.setLayerValue -> setValue, which already keys
  // at the playhead when the stopwatch is on and writes motionStatic when
  // it isn't (the AE convention every other value field in this app uses).
  // There is deliberately no second writer here.
  function writeFromLocal(g, lx, ly) {
    var w = g.widget;
    SMMotion.setLayerValue(g.li, w.x.key, [valueFromLocal(w.x, lx / g.w + 0.5)]);
    if (w.kind === 'joystick' && w.y) SMMotion.setLayerValue(g.li, w.y.key, [valueFromLocal(w.y, 0.5 - ly / g.h)]);
  }

  // ---- drag interaction ------------------------------------------------
  // A widget is grabbable whatever the ACTIVE layer is — that is the whole
  // point of a rig control: you are animating the character, and you reach
  // over and push the stick. So this scans every widget layer rather than
  // only state.activeLayerIdx.
  //
  // The consequence, measured rather than assumed: a pen stroke that STARTS
  // inside a pad is taken by the widget, not by the brush (0 strokes added,
  // the axis moved instead). That is the right trade for a control you put
  // beside the character the way Rive and Moho do — and the escape hatch is
  // the padlock the row already has: with the widget layer locked, the same
  // gesture draws normally (2 strokes added, the axis untouched). Both
  // halves verified live.
  function hitTest(clientX, clientY) {
    if (!engineOn() || !window.SMEngineBridge) return null;
    var wpt = SMEngineBridge.screenToWorld(clientX, clientY);
    var idxs = widgetLayerIndices();
    // Topmost first: layer 0 is the bottom of the stack everywhere else in
    // this app, so a widget drawn over another wins the click.
    for (var n = idxs.length - 1; n >= 0; n--) {
      var li = idxs[n], ld = state.layers[li];
      if (ld.visible === false || ld.locked) continue;
      var g = geomFor(li);
      if (!g) continue;
      var loc = toLocal(g, wpt[0], wpt[1]);
      var pl = puckLocal(g);
      var tol = Math.max(puckRadius(g), 10 / view.zoom);
      if (Math.hypot(loc[0] - pl[0], loc[1] - pl[1]) <= tol) return { g: g, local: loc, onPuck: true };
      // Clicking the pad anywhere OTHER than the puck moves the whole
      // widget instead (feedback #184: "le bouton rond... ne devrait
      // bouger que si on clic sur le rond") — see onDown below for the
      // drag-mode split. No more trough-jump-to-value on a body click.
      if (Math.abs(loc[0]) <= g.w / 2 && Math.abs(loc[1]) <= g.h / 2) return { g: g, local: loc, onPuck: false };
    }
    return null;
  }

  function onDown(e) {
    if (e.button !== undefined && e.button !== 0) return;
    var hit = hitTest(e.clientX, e.clientY);
    if (!hit) return;
    e.stopImmediatePropagation(); e.preventDefault();
    pushUndo();
    SMEngineBridge.suspend();
    if (hit.onPuck) {
      drag = { li: hit.g.li, mode: 'puck', moved: false };
    } else {
      // Body click (anywhere in the pad but not the puck) drags the WHOLE
      // widget — writes the layer's own Position track, exactly like
      // dragging a Null marker. t.strokeId stays null: outerLocalPoint only
      // needs to undo the PARENT chain here, the widget's own Position is
      // what's being edited, not composed into "local" already (mirrors
      // geomFor: ownMat applied first, parent chain on top).
      var t = { li: hit.g.li, strokeId: null };
      var wpt0 = SMEngineBridge.screenToWorld(e.clientX, e.clientY);
      var startLocal = SMMotion.outerLocalPoint(t, { x: wpt0[0], y: wpt0[1] });
      var ownMat0 = SMMotion.layerMotionAt(hit.g.li, state.currentFrame);
      drag = { li: hit.g.li, mode: 'move', moved: false, t: t, startLocal: startLocal, startDx: ownMat0 ? ownMat0.dx : 0, startDy: ownMat0 ? ownMat0.dy : 0 };
    }
    SMEngineBridge.renderNow();
  }
  function onMove(e) {
    if (!drag) return;
    e.stopImmediatePropagation(); e.preventDefault();
    if (drag.mode === 'move') {
      var wpt = SMEngineBridge.screenToWorld(e.clientX, e.clientY);
      var curLocal = SMMotion.outerLocalPoint(drag.t, { x: wpt[0], y: wpt[1] });
      var ndx = drag.startDx + (curLocal.x - drag.startLocal.x);
      var ndy = drag.startDy + (curLocal.y - drag.startLocal.y);
      SMMotion.setLayerValue(drag.li, 'position', [ndx, ndy]);
      drag.moved = true;
      // feedback #211 — same cheap in-place field sync motion.js's own
      // onDrag does after every tick, needed here too since this writes
      // Position through the identical setLayerValue path.
      if (window.SMMotion && SMMotion.liveRefreshVisiblePropertyFields) SMMotion.liveRefreshVisiblePropertyFields();
      SMEngineBridge.renderNow();
      return;
    }
    var g = geomFor(drag.li);
    if (!g) { drag = null; return; }
    var wpt2 = SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var loc = toLocal(g, wpt2[0], wpt2[1]);
    writeFromLocal(g, loc[0], loc[1]);
    drag.moved = true;
    // feedback #211 — the axis value(s) just written live in an
    // exprControls row (CLAUDE.md §13), same panel-field-lag bug as every
    // other canvas drag.
    if (window.SMMotion && SMMotion.liveRefreshVisiblePropertyFields) SMMotion.liveRefreshVisiblePropertyFields();
    SMEngineBridge.renderNow();
  }
  function onUp(e) {
    if (!drag) return;
    e.stopImmediatePropagation(); e.preventDefault();
    var moved = drag.moved, li = drag.li;
    drag = null;
    SMEngineBridge.resume();
    // A drag on an animated axis may have created a keyframe, which only
    // shows up once the timeline is rebuilt — the panel/grid pair is never
    // repainted incrementally (motion.js), so nothing else would show it.
    if (moved && typeof updateUI === 'function') updateUI();
    SMEngineBridge.renderNow();
    return li;
  }
  // Double-click returns the axis to its rest value — the "let go of the
  // stick" gesture. Goes through the same writeFromLocal, so it keys at the
  // playhead exactly like a drag does when the stopwatch is on.
  function onDblClick(e) {
    var hit = hitTest(e.clientX, e.clientY);
    if (!hit) return;
    e.stopImmediatePropagation(); e.preventDefault();
    pushUndo();
    var rl = restLocal(hit.g);
    writeFromLocal(hit.g, rl[0], rl[1]);
    if (typeof updateUI === 'function') updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }

  // ---- creation --------------------------------------------------------
  // Creating the layer is what CREATES the controls: one addExprControl per
  // axis, and the returned key is what ld.widget.x/.y point at. The widget
  // never invents a value holder of its own.
  function makeAxis(ld, name, d) {
    var c = SMMotion.addExprControl(ld, 'number', name);
    if (!c) return null;
    // Seed the control's static value at rest, so a fresh widget's puck
    // starts on its own rest marker instead of at 0 when rest isn't 0.
    if (!ld.motionStatic) ld.motionStatic = {};
    ld.motionStatic[c.key] = [d.rest];
    return { key: c.key, min: d.min, max: d.max, rest: d.rest, label: c.name };
  }
  function addWidgetLayer(kind) {
    if (kind !== 'joystick' && kind !== 'slider') kind = 'joystick';
    if (!window.SMMotion || typeof createUserLayer !== 'function') return -1;
    var d = DEFAULTS[kind];
    saveAllLayerFrames(); pushUndoLayers(true);
    var nm = (typeof nextLayerName === 'function' ? nextLayerName() : 'Layer')
      .replace(/^Layer/, kind === 'slider' ? 'Slider' : 'Joystick');
    var idx = createUserLayer(nm);
    var ld = state.layers[idx];
    ld.isWidgetLayer = true;
    ld.color = '#ffb86c';
    var ax = makeAxis(ld, d.names[0], d);
    var ay = kind === 'joystick' ? makeAxis(ld, d.names[1], d) : null;
    if (!ax || (kind === 'joystick' && !ay)) { ld.isWidgetLayer = false; return -1; }
    ld.widget = {
      kind: kind,
      pos: [state.canvasW / 2, state.canvasH / 2],
      size: d.size.slice(),
      // Reserved: v1 always places the widget in WORLD space (it follows
      // pan/zoom/parenting like a Null's marker). The field is declared and
      // round-trips so a future screen-pinned mode doesn't need a format
      // change; nothing branches on it yet.
      screenSpace: false,
      x: ax,
    };
    if (ay) ld.widget.y = ay;
    if (typeof activateUL === 'function') activateUL(idx);
    if (typeof loadFrame === 'function') loadFrame(state.currentFrame);
    if (typeof updateUI === 'function') updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (window.showToast) showToast(SM.t('toastWidgetLayerAdded'));
    return idx;
  }

  // ---- wiring menu (the point of the feature) --------------------------
  // Every (widget layer, axis) pair currently in the project, as the
  // {uid, name} an expression actually needs. Shared by both menu entries
  // below AND by motion.js's property-row menu, so the two can never offer
  // a different list.
  function axisChoices() {
    var out = [];
    widgetLayerIndices().forEach(function (li) {
      var ld = state.layers[li], w = ld.widget;
      var uid = SMMotion.ensureLayerUid(ld);
      ['x', 'y'].forEach(function (k) {
        if (!w[k]) return;
        var nmA = axisLabel(ld, w[k]);
        if (!nmA) return;
        out.push({ li: li, layerName: ld.name, uid: uid, axis: k, name: nmA, kind: w.kind, min: w[k].min, max: w[k].max });
      });
    });
    return out;
  }
  // The expression text for one axis. `layerControl` (motion.js) is the
  // lean accessor — layer(uid).control(name) resolves the same value but
  // builds a whole layerSnapshot first (an O(n) name scan plus six
  // valueAtFrame calls per read), and a rig is many reads per scrub tick
  // (CLAUDE.md §5bis).
  function axisRef(choice) {
    return 'layerControl(' + JSON.stringify(choice.uid) + ', ' + JSON.stringify(choice.name) + ')';
  }
  // ---- range / size editing --------------------------------------------
  // min/max/rest are PRESENTATION (how far the puck travels and what number
  // that maps to), so they are edited here and not on the control itself.
  // Deliberately the same nested showContextMenu + prompt() idiom
  // openExprControlsMenu already uses for renaming a control: one place to
  // look for anything about a layer, one interaction to learn, and no new
  // settings surface for four numbers.
  //
  // The range matters most for the pose-library gesture — driving
  // `self.at(layerControl(...))` means the axis value IS a frame number, so
  // an axis that runs 0…24 scrubs a 24-frame pose track.
  function openWidgetMenu(x, y, li) {
    var ld = state.layers[li], w = widgetOf(ld);
    if (!w || !window.showContextMenu) return;
    function axisItem(k) {
      var ax = w[k];
      if (!ax) return null;
      return { label: SM.t('widgetMenuAxisRangePrefix') + axisLabel(ld, ax) + '  (' + ax.min + '…' + ax.max + ')', action: function () {
        var s = prompt(SM.t('promptWidgetRange'), ax.min + ', ' + ax.max + ', ' + ax.rest);
        if (s === null) return;
        var parts = String(s).split(',').map(function (p) { return parseFloat(p); });
        if (parts.length < 2 || !isFinite(parts[0]) || !isFinite(parts[1])) return;
        pushUndo();
        ax.min = parts[0]; ax.max = parts[1];
        if (parts.length > 2 && isFinite(parts[2])) ax.rest = parts[2];
        afterWidgetEdit(ld);
      } };
    }
    var items = [];
    var ix = axisItem('x'); if (ix) items.push(ix);
    var iy = axisItem('y'); if (iy) items.push(iy);
    items.push({ label: SM.t('widgetMenuSizeEllipsis'), action: function () {
      var s = prompt(SM.t('promptWidgetSize'), w.size[0] + ', ' + w.size[1]);
      if (s === null) return;
      var p2 = String(s).split(',').map(function (p) { return parseFloat(p); });
      if (p2.length < 2 || !isFinite(p2[0]) || !isFinite(p2[1])) return;
      pushUndo();
      w.size = [Math.max(8, p2[0]), Math.max(8, p2[1])];
      afterWidgetEdit(ld);
    } });
    window.showContextMenu(x, y, items);
  }
  function afterWidgetEdit(ld) {
    if (typeof saveActiveLayerFrame === 'function') saveActiveLayerFrame();
    if (typeof renderLayerList === 'function') renderLayerList();
    if (typeof renderTimeline === 'function') renderTimeline();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }

  // ---- right-panel properties (feedback #184, "avoir leurs properties
  // dans layer properties, pouvoir ajuster leur taille/longueur") ----
  // Same section-that-only-exists-for-one-selection-kind pattern as
  // image-mesh-bridge.js's own panel (renderImageMeshPanel) — a static
  // #p-widget-sec block in index.html, shown/hidden and populated from
  // here, hooked into updatePropsContext() (timeline.js) right next to it.
  function el(id) { return document.getElementById(id); }
  function activeWidget() {
    var li = state.activeLayerIdx, ld = state.layers[li], w = widgetOf(ld);
    return w ? { li: li, ld: ld, w: w } : null;
  }
  function renderWidgetPanel() {
    var sec = el('p-widget-sec');
    if (!sec) return;
    var aw = activeWidget();
    if (!aw) { sec.style.display = 'none'; return; }
    sec.style.display = '';
    var ld = aw.ld, w = aw.w, size = w.size || DEFAULTS[w.kind].size;
    var wEl = el('p-widget-w'), hEl = el('p-widget-h');
    if (wEl) wEl.value = Math.round(size[0]);
    if (hEl) hEl.value = Math.round(size[1]);
    var xl = el('p-widget-xlabel'); if (xl) xl.textContent = axisLabel(ld, w.x) || 'X';
    var xmin = el('p-widget-xmin'), xmax = el('p-widget-xmax'), xrest = el('p-widget-xrest');
    if (xmin) xmin.value = w.x.min;
    if (xmax) xmax.value = w.x.max;
    if (xrest) xrest.value = (w.x.rest === undefined ? w.x.min : w.x.rest);
    var hasY = w.kind === 'joystick' && !!w.y;
    var yRangeRow = el('p-widget-yrange-row'), yRestRow = el('p-widget-yrest-row');
    if (yRangeRow) yRangeRow.style.display = hasY ? '' : 'none';
    if (yRestRow) yRestRow.style.display = hasY ? '' : 'none';
    if (hasY) {
      var yl = el('p-widget-ylabel'); if (yl) yl.textContent = axisLabel(ld, w.y) || 'Y';
      var ymin = el('p-widget-ymin'), ymax = el('p-widget-ymax'), yrest = el('p-widget-yrest');
      if (ymin) ymin.value = w.y.min;
      if (ymax) ymax.value = w.y.max;
      if (yrest) yrest.value = (w.y.rest === undefined ? w.y.min : w.y.rest);
    }
  }
  window.renderRigWidgetPanel = renderWidgetPanel;
  // One shared handler shape: read the active widget fresh (never captured
  // by closure — the panel section can end up bound to a different widget
  // layer between the field's creation and any given edit), undo-snapshot,
  // apply, refresh. Mirrors bindWidgetField's sibling in image-mesh-bridge.js.
  function bindWidgetField(id, apply) {
    var fld = el(id);
    if (!fld) return;
    fld.addEventListener('change', function () {
      var aw = activeWidget();
      if (!aw) return;
      var v = parseFloat(this.value);
      if (!isFinite(v)) return;
      pushUndo();
      apply(aw.w, v);
      afterWidgetEdit(aw.ld);
    });
  }
  function initWidgetPanelFields() {
    bindWidgetField('p-widget-w', function (w, v) { w.size = [Math.max(8, v), (w.size || DEFAULTS[w.kind].size)[1]]; });
    bindWidgetField('p-widget-h', function (w, v) { w.size = [(w.size || DEFAULTS[w.kind].size)[0], Math.max(8, v)]; });
    bindWidgetField('p-widget-xmin', function (w, v) { w.x.min = v; });
    bindWidgetField('p-widget-xmax', function (w, v) { w.x.max = v; });
    bindWidgetField('p-widget-xrest', function (w, v) { w.x.rest = v; });
    bindWidgetField('p-widget-ymin', function (w, v) { if (w.y) w.y.min = v; });
    bindWidgetField('p-widget-ymax', function (w, v) { if (w.y) w.y.max = v; });
    bindWidgetField('p-widget-yrest', function (w, v) { if (w.y) w.y.rest = v; });
    var xAdd = el('p-widget-xadd');
    if (xAdd) xAdd.addEventListener('click', function (e) { var aw = activeWidget(); if (aw) openAddLinkMenu(e.clientX, e.clientY, aw.li, 'x'); });
    var yAdd = el('p-widget-yadd');
    if (yAdd) yAdd.addEventListener('click', function (e) { var aw = activeWidget(); if (aw) openAddLinkMenu(e.clientX, e.clientY, aw.li, 'y'); });
  }

  // ---- add-link button (feedback #185, "lier ce qu'ils contrôlent avec
  // des boutons add... dans layer properties") ----
  // The mirror image of widgetWiringMenuItems (motion.js): THAT is a
  // right-click on a property row picking a widget axis to link to; THIS
  // is a "+" on a widget's own axis row picking a PROPERTY to link. Same
  // two modes (plain link / drive-a-pose), same underlying writer
  // (SMMotion.setExpressionCode) — one writer, two entry points, so a
  // link made either way is indistinguishable to every other reader
  // (graph editor, expression textarea, export).
  //
  // propertyChoices() deliberately reuses SMMotion.propsFor — the SAME
  // single decider every Motion row (panel AND grid) already calls before
  // it can render (§11) — instead of guessing a parallel list of keyable
  // properties that would silently drift from the real one.
  function propertyChoices() {
    if (!window.SMMotion || !SMMotion.propsFor) return [];
    var out = [];
    for (var li = 0; li < state.layers.length; li++) {
      var ld = state.layers[li];
      var props = SMMotion.propsFor(ld);
      for (var i = 0; i < props.length; i++) {
        out.push({ li: li, ld: ld, layerName: ld.name, prop: props[i], label: SMMotion.propLabel(props[i]) });
      }
    }
    return out;
  }
  function openAddLinkMenu(x, y, li, axisKey) {
    var ld = state.layers[li], w = widgetOf(ld);
    if (!w || !w[axisKey] || !window.SMMotion || !window.showContextMenu) return;
    var refStr = axisRef({ uid: SMMotion.ensureLayerUid(ld), name: axisLabel(ld, w[axisKey]) });
    function sub(label, wrap, toastKey) {
      return { label: label, action: function () {
        var choices = propertyChoices();
        if (!choices.length) return;
        window.showContextMenu(x + 8, y + 8, choices.map(function (c) {
          return { label: c.layerName + ' · ' + c.label, action: function () {
            SMMotion.setExpressionCode(c.ld, c.prop, wrap(refStr));
            if (window.showToast) showToast(SM.t(toastKey));
          } };
        }));
      } };
    }
    window.showContextMenu(x, y, [
      sub(SM.t('ctxWidgetAddLinkPropertyEllipsis'), function (r) { return r; }, 'toastLinkedToWidget'),
      sub(SM.t('ctxWidgetAddDrivePoseEllipsis'), function (r) { return 'self.at(' + r + ')'; }, 'toastPoseDrivenByWidget'),
    ]);
  }

  window.SMRigWidget = {
    addWidgetLayer: addWidgetLayer,
    openWidgetMenu: openWidgetMenu,
    widgetOf: widgetOf,
    widgetLayerIndices: widgetLayerIndices,
    axisChoices: axisChoices,
    axisRef: axisRef,
    axisLabel: axisLabel,
    // Diagnostics/tests: the resolved on-canvas frame and puck position,
    // which a screenshot alone cannot tell you.
    geomFor: geomFor,
    puckWorld: function (li) { var g = geomFor(li); if (!g) return null; var p = puckLocal(g); return toWorld(g, p[0], p[1]); },
    isDragging: function () { return !!drag; },
  };

  // ---- listeners -------------------------------------------------------
  function init() {
    initWidgetPanelFields();
    // Capture phase on DOCUMENT, not on #canvas-area — see
    // image-mesh-bridge.js's own long note: motion.js registers a
    // capture-phase pointerdown on #canvas-area and loads FIRST, and among
    // listeners on the SAME element capture order is registration order, so
    // a listener on #canvas-area loses the gesture in Motion mode and the
    // whole layer gets dragged instead. A capture listener on an ANCESTOR
    // always runs first whatever the registration order.
    //
    // image-mesh-bridge now has a document-capture listener of its own, so
    // both have to bail cleanly when not armed: every handler here returns
    // WITHOUT stopImmediatePropagation unless hitTest actually found a
    // widget under the pointer.
    function inCanvas(e) {
      var area = document.getElementById('canvas-area');
      return !!(area && e.target && area.contains(e.target));
    }
    document.addEventListener('pointerdown', function (e) { if (inCanvas(e)) onDown(e); }, { capture: true });
    document.addEventListener('pointermove', function (e) { if (drag) onMove(e); }, { capture: true });
    document.addEventListener('pointerup', function (e) { if (drag) onUp(e); }, { capture: true });
    document.addEventListener('pointercancel', function (e) { if (drag) onUp(e); }, { capture: true });
    document.addEventListener('dblclick', function (e) { if (inCanvas(e)) onDblClick(e); }, { capture: true });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
