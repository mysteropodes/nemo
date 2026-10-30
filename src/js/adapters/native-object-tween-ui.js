/* Animation 2D Tween UI -> the native tween commands (#2358). The panel Tween button, the Regenerate Tween menu item, the Tween
 * shortcut target, the per-span easing curve edit, the Reassign flow and the Harmonize re-tween read the panel and the frame
 * selection AT CLICK TIME (the selection is cleared right after the call by the baseline flows) and send one native command each
 * through native-object-tween.js. Nothing here writes the document: no pushUndoLayers, saveAllLayerFrames, loadFrame or
 * generateTweens. Every entry returns null when the host is not in native mode (the baseline path then runs) and otherwise a
 * Promise of a typed result, never a rejection: {status: applied | unchanged | unavailable | refused | unconfirmed, code?, receipt?}.
 * Motion-arc drags and the D23/D24 reads have no native operation and stay typed-unavailable. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else { root.NemoNativeTweenUI = api; }
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const unavailable = (code) => Object.freeze({ status: 'unavailable', code });
  const CODES = Object.freeze({ session: 'tween_session_unavailable', declaration: 'tween_declaration_missing', layer: 'tween_layer_unresolved',
    arc: 'tween_motion_arc_unavailable', easing: 'tween_easing_invalid', span: 'tween_span_unavailable', selection: 'tween_selection_pending' });

  function nativeMode(host) {
    try {
      const cutover = host.NemoNativeOpacityCutover;
      if (cutover && typeof cutover.blocksLegacy === 'function' && cutover.blocksLegacy()) return true;
      const project = host.NemoNativeObjectProject;
      if (!project || typeof project.desktop !== 'function') return false;
      const session = project.desktop(host);
      return !!(session && (session.isActive() || (typeof session.blocksLegacy === 'function' && session.blocksLegacy())));
    } catch (_) { return true; }
  }
  function selectionPending(host) {
    try { const ui = host.NemoNativeObjectUI; return !!(ui && typeof ui.isFrameSelectionPending === 'function' && ui.isFrameSelectionPending(host)); } catch (_) { return true; }
  }
  function t(host, key, fallback) { try { const text = host.SM && host.SM.t && host.SM.t(key); return text && text !== key ? text : fallback; } catch (_) { return fallback; } }
  function toast(host, message) { try { if (typeof host.showToast === 'function') host.showToast(message); } catch (_) { /* a notice failure is not a second writer */ } }

  // The result and the one notice that goes with it.
  function notice(host, result) {
    if (result.status === 'applied' || result.status === 'unchanged') {
      if (result.status === 'unchanged' && result.receipt && result.receipt.applied === false) toast(host, t(host, 'toastNeedAtLeast2DrawnKeyframes', 'Need at least 2 drawn keyframes.'));
      return result;
    }
    toast(host, result.status === 'unconfirmed' ? 'Native tween was not confirmed. Wait for synchronization.'
      : result.status === 'refused' ? 'Native tween was refused' + (result.reason ? ' (' + result.reason + ').' : '.')
      : result.code === CODES.arc ? 'Motion-arc tweens are unavailable in the native document.'
      : result.code === CODES.selection ? 'Finish or cancel the frame selection first.'
      : 'Native tween is unavailable.');
    return result;
  }
  function failure(host, error) {
    const code = error && typeof error.code === 'string' ? error.code : 'unavailable';
    if (/_command_indeterminate$/.test(code) || code === 'uncorrelated' || code === 'stale_owner') return Object.freeze({ status: 'unconfirmed', code });
    if (code === 'unavailable') return unavailable(error.reason || 'unavailable');
    return Object.freeze({ status: 'refused', code, reason: error && error.reason || null });
  }
  // Always a Promise that resolves: `prepare(commands)` runs synchronously (it reads the panel and the selection) and returns the
  // send thunk, or a typed result when the request cannot be built.
  function run(host, prepare) {
    let commands = null;
    try { commands = host.NemoNativeObjectTween && host.NemoNativeObjectTween.desktop(host); } catch (_) { commands = null; }
    if (!commands) return Promise.resolve(notice(host, unavailable(CODES.session)));
    if (selectionPending(host)) return Promise.resolve(notice(host, unavailable(CODES.selection)));
    let step;
    try { step = prepare(commands); } catch (_) { step = unavailable(CODES.layer); }
    if (typeof step !== 'function') return Promise.resolve(notice(host, step));
    return Promise.resolve().then(step).then((receipt) => notice(host, Object.freeze({ status: receipt.applied ? 'applied' : 'unchanged', receipt })),
      (error) => notice(host, failure(host, error)));
  }

  // ---- panel and layer reads (synchronous, at click time) ----
  const whole = (v, lo, hi, fallback) => { const n = Math.floor(Number(v)); return Number.isFinite(n) ? Math.max(lo, Math.min(hi, n)) : fallback; };
  function curvePoints(points) {
    if (!Array.isArray(points) || points.length < 2) return null;
    const out = points.map((p) => {
      const point = { x: p && p.x, y: p && p.y };
      if (p && typeof p.tx === 'number') { point.tx = p.tx; point.ty = typeof p.ty === 'number' ? p.ty : 0; }
      return point;
    });
    const finite = out.every((p, i) => Number.isFinite(p.x) && Number.isFinite(p.y) && p.x >= 0 && p.x <= 1 && (i === 0 || p.x > out[i - 1].x) &&
      (p.tx === undefined || (Number.isFinite(p.tx) && Number.isFinite(p.ty))));
    return finite ? out : null;
  }
  // The four panel members the baseline generateTweens reads: Resample, Step, Skip manual and the global Easing curve.
  function panelOptions(host) {
    const state = host.state || {}, editor = host._curveEditor, shown = editor && typeof editor.getState === 'function' && editor.getState();
    const easing = curvePoints(shown && shown.points) || curvePoints(state.easingCurve && state.easingCurve.points);
    if (!easing) return null;
    return { resamplePoints: whole(state.resamplePts, 2, 256, 50), step: whole(state.tweenStep, 1, 999, 1), skipManual: !!state.tweenSkipManual, easing };
  }
  function layerOf(host, commands, index) {
    const state = host.state, row = state && state.layers && state.layers[index], uid = commands.layerUid(index);
    return typeof uid === 'string' && uid && row && row.layerUid === uid ? uid : null;
  }
  const drawn = (frame) => !!(frame && frame.isKeyframe && frame.strokes && frame.strokes.length > 0);
  function drawnKeys(host, index) {
    const row = host.state.layers[index], keys = [];
    for (let i = 0; i < host.state.totalFrames; i++) if (drawn(row.frames[i])) keys.push(i);
    return keys;
  }
  const frames = (list) => Array.from(new Set(list.filter((f) => Number.isSafeInteger(f) && f >= 0))).sort((a, b) => a - b);

  // generateTweens(explicitRestrictTo, skipUndo): the whole layer, the selected frames' own spans, or an explicit start-key map.
  function generate(host, restrictTo, skipUndo) {
    if (!nativeMode(host)) return null;
    // A motion-arc drag regenerates with its own undo snapshot; arcs are not a native document member.
    if (skipUndo === true) return Promise.resolve(notice(host, unavailable(CODES.arc)));
    return run(host, (commands) => {
      const index = host.state.activeLayerIdx, uid = layerOf(host, commands, index), options = panelOptions(host);
      if (!uid) return unavailable(CODES.layer);
      if (!options) return unavailable(CODES.easing);
      if (!commands.declared('command.document.object.tween.generate')) return unavailable(CODES.declaration);
      const selected = frames(((host._sel && host._sel.frames) || []).filter((s) => s && s.layer === index).map((s) => s.frame));
      if (restrictTo) {
        const starts = frames(Object.keys(restrictTo).filter((k) => restrictTo[k]).map(Number));
        return () => commands.regenerate(uid, { starts }, options);
      }
      return () => commands.generate(uid, selected.length ? { selection: selected } : {}, options);
    });
  }

  // reassign: the span's overrides (UI buffer, already updated by the flow), its own curve and its manual strokes.
  function spanOf(host, index, from, to) {
    const state = host.state, row = state.layers[index];
    const key = typeof host.tweenSpanKey === 'function' ? host.tweenSpanKey(index, from, to) : row.layerUid + ':' + from + '-' + to;
    const span = { from, to, overrides: [] };
    ((state.tweenOverrides && state.tweenOverrides[key]) || []).forEach((entry) => {
      if (entry && typeof entry.bId === 'string' && entry.bId) {
        if (Array.isArray(entry.aIds) && entry.aIds.length) span.overrides.push({ aIds: entry.aIds.slice(), bId: entry.bId });
        else if (typeof entry.aId === 'string' && entry.aId) span.overrides.push({ aId: entry.aId, bId: entry.bId });
      }
    });
    const own = state.tweenEasing && state.tweenEasing[key], easing = curvePoints(own && own.points);
    if (easing) span.easing = easing;
    const start = row.frames[from];
    if (start && start.tweenManualMode) {
      const ids = (start.strokes || []).filter((s) => s && s.tweenOn && typeof s.strokeId === 'string' && s.strokeId).map((s) => s.strokeId);
      if (ids.length) span.manualStrokes = ids;
    }
    return span;
  }
  function reassign(host, index, from, to) {
    if (!nativeMode(host)) return null;
    return run(host, (commands) => {
      const uid = layerOf(host, commands, index), options = panelOptions(host);
      if (!uid) return unavailable(CODES.layer);
      if (!options) return unavailable(CODES.easing);
      if (!Number.isSafeInteger(from) || !Number.isSafeInteger(to) || !(from < to)) return unavailable(CODES.span);
      const span = spanOf(host, index, from, to);
      return () => commands.reassign(uid, span, options);
    });
  }
  // A per-span easing curve edit: the span starting at `from`, up to the next drawn key.
  function pairCurve(host, index, from) {
    if (!nativeMode(host)) return null;
    let to = -1;
    try { const keys = drawnKeys(host, index); const at = keys.indexOf(from); to = at >= 0 && at + 1 < keys.length ? keys[at + 1] : -1; } catch (_) { to = -1; }
    return reassign(host, index, from, to);
  }
  // Harmonize: the edited interpolated frame becomes a key and the two spans around it are re-tweened (opt-in panel toggle).
  function harmonize(host, frame) {
    if (!nativeMode(host)) return null;
    if (!(host.state && host.state.tweenHarmonizeEdits)) return Promise.resolve(Object.freeze({ status: 'unchanged' }));
    return split(host, host.state.activeLayerIdx, frame);
  }
  function split(host, index, frame) {
    if (!nativeMode(host)) return null;
    return run(host, (commands) => {
      const uid = layerOf(host, commands, index), options = panelOptions(host);
      if (!uid) return unavailable(CODES.layer);
      if (!options) return unavailable(CODES.easing);
      if (!Number.isSafeInteger(frame) || frame < 0) return unavailable(CODES.span);
      return () => commands.split(uid, frame, options);
    });
  }
  return Object.freeze({ active: nativeMode, generate, reassign, pairCurve, split, harmonize, CODES });
}));
