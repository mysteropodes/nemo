/* Animation 2D Tween UI -> the native tween commands (#2358). The panel Tween button, the Regenerate Tween menu item, the Tween
 * shortcut target, the per-span easing curve edit, the Reassign flow and the Harmonize re-tween read the panel and the frame
 * selection AT CLICK TIME (the selection is cleared right after the call by the baseline flows) and send one native command each
 * through native-object-tween.js. Nothing here writes the document: no pushUndoLayers, saveAllLayerFrames, loadFrame or
 * generateTweens. Every entry returns null when the host is not in native mode (the baseline path then runs) and otherwise a
 * Promise of a typed result, never a rejection: {status: applied | unchanged | unavailable | refused | unconfirmed, code?, receipt?}.
 * The Reassign flow takes its key span from the native cells read and its clicked identities from the native hit-test listing
 * (reassignSpan, pickStroke), never from frames[] or Paper items. Motion-arc drags and the D23/D24 reads have no native
 * operation and stay typed-unavailable. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else { root.NemoNativeTweenUI = api; }
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const unavailable = (code) => Object.freeze({ status: 'unavailable', code });
  const CODES = Object.freeze({ session: 'tween_session_unavailable', declaration: 'tween_declaration_missing', layer: 'tween_layer_unresolved',
    arc: 'tween_motion_arc_unavailable', easing: 'tween_easing_invalid', span: 'tween_span_unavailable', selection: 'tween_selection_pending',
    cells: 'tween_cells_unavailable', noKey: 'tween_no_key_before', noNext: 'tween_no_next_key', presentation: 'tween_presentation_unavailable' });

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
    if (result.status === 'applied' || result.status === 'unchanged') return result;
    toast(host, result.status === 'unconfirmed' ? 'Native tween was not confirmed. Wait for synchronization.'
      : result.status === 'refused' ? 'Native tween was refused' + (result.reason ? ' (' + result.reason + ').' : '.')
      : result.code === CODES.arc ? 'Motion-arc tweens are unavailable in the native document.'
      : result.code === CODES.selection ? 'Finish or cancel the frame selection first.'
      : result.code === CODES.noKey ? t(host, 'toastPlayheadOnKeyframeHint', 'Put the playhead on or after a keyframe.')
      : result.code === CODES.noNext ? t(host, 'toastNoNextKeyframeToReassign', 'No next keyframe to reassign to.')
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
  function run(host, prepare, layerIndex) {
    let commands = null;
    try { commands = host.NemoNativeObjectTween && host.NemoNativeObjectTween.desktop(host); } catch (_) { commands = null; }
    if (!commands) return Promise.resolve(notice(host, unavailable(CODES.session)));
    if (selectionPending(host)) return Promise.resolve(notice(host, unavailable(CODES.selection)));
    let step;
    try { step = prepare(commands); } catch (_) { step = unavailable(CODES.layer); }
    if (typeof step !== 'function') return Promise.resolve(notice(host, step));
    return Promise.resolve().then(step).then((receipt) => explainUnchanged(host, notice(host, Object.freeze({ status: receipt.applied ? 'applied' : 'unchanged', receipt })), layerIndex),
      (error) => notice(host, failure(host, error)));
  }
  // An unchanged receipt has several causes (nothing to regenerate, an identical result, too few keys). The baseline notice names
  // only the last one, so it is shown only when the native cells read confirms fewer than two drawn keys on that layer.
  async function explainUnchanged(host, result, layerIndex) {
    if (result.status !== 'unchanged' || !result.receipt || result.receipt.applied !== false) return result;
    const keys = await readKeys(host, layerIndex === undefined ? host.state.activeLayerIdx : layerIndex);
    if (keys && keys.filter((k) => k.drawn).length < 2) toast(host, t(host, 'toastNeedAtLeast2DrawnKeyframes', 'Need at least 2 drawn keyframes.'));
    return result;
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
  const sleep = (host, ms) => new Promise((done) => (host.setTimeout || setTimeout)(done, ms));
  // The layer's key frames from the native whole-row cells read (the x-sheet's query), never from the legacy frames[] a native
  // project projects empty: [{frame, drawn}] for every 'key' run, `drawn` = the key holds content. A refusal while the
  // presentation catches up a revision is retried; null when the row stays unavailable.
  async function readKeys(host, index) {
    let session, uid;
    try { session = host.NemoNativeObjectProject.desktop(host); uid = layerOf(host, host.NemoNativeObjectTween.desktop(host), index); } catch (_) { return null; }
    if (!uid || !session || typeof session.readLayerCells !== 'function') return null;
    for (let attempt = 0; attempt < 3; attempt++) {
      let runs = null;
      try { runs = await session.readLayerCells(uid, session.info()); } catch (_) { runs = null; }
      if (Array.isArray(runs)) return runs.filter((run) => run[2] === 'key').map((run) => ({ frame: run[0], drawn: run[4] === true }));
      if (attempt < 2) await sleep(host, 120);
    }
    return null;
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

  // reassign: the span's overrides (UI buffer, already updated by the flow) and its own curve. A native document projects no
  // frames[] and its hit-test listing carries no per-stroke tween flag, so there is no manual-stroke source to send here.
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
    }, index);
  }
  // A per-span easing curve edit: the span starting at the drawn key `from`, up to the next drawn key of the native cells read.
  function pairCurve(host, index, from) {
    if (!nativeMode(host)) return null;
    return readKeys(host, index).then((keys) => {
      const drawnKeys = (keys || []).filter((k) => k.drawn).map((k) => k.frame), at = drawnKeys.indexOf(from);
      return reassign(host, index, from, at >= 0 && at + 1 < drawnKeys.length ? drawnKeys[at + 1] : -1);
    });
  }
  // Reassign step 0: the key span the playhead is in (the nearest key at or before it, and the next key), from the cells read.
  // Resolves {status:'span', from, to} or a noticed typed unavailable (no key before the playhead, no next key, cells unavailable).
  function reassignSpan(host, index, frame) {
    if (!nativeMode(host)) return null;
    return readKeys(host, index).then((keys) => {
      if (!keys) return notice(host, unavailable(CODES.cells));
      const before = keys.filter((k) => k.frame <= frame).pop(), next = before && keys.find((k) => k.frame > before.frame);
      if (!before) return notice(host, unavailable(CODES.noKey));
      if (!next) return notice(host, unavailable(CODES.noNext));
      return Object.freeze({ status: 'span', from: before.frame, to: next.frame });
    });
  }
  // Reassign clicks: the native identity of the top-most presented element of layer `index` under the client point, read from
  // the native hit-test listing (a linked fill answers its ribbon, a texture dab its anchor) once the canvas presents `frame`.
  // Resolves {status:'hit', strokeId} (strokeId null on empty canvas) or a typed unavailable when the presentation never settled.
  async function pickStroke(host, index, clientX, clientY, frame) {
    const Intent = host.NemoSelectCanvasIntent;
    for (let attempt = 0; Intent && attempt < 5; attempt++) {
      let shown = null, hit;
      try { shown = Intent.presentedState(host); } catch (_) { shown = null; }
      if (shown && shown.presentation.frame === frame) {
        try { hit = await Intent.pick(host, clientX, clientY, index); } catch (_) { hit = undefined; }
        if (hit !== undefined) return Object.freeze({ status: 'hit', strokeId: hit && hit.data && typeof hit.data.strokeId === 'string' ? hit.data.strokeId : null });
      }
      await sleep(host, 120);
    }
    return unavailable(CODES.presentation);
  }
  // ---- the Reassign flow's native steps, over tweens.js's own `_reassign` state `st` (see startReassign / reassignHandleClick) ----
  // Start: arm the flow from the cells read, only if the playhead and layer did not move meanwhile. Returns null off native mode.
  function startReassign(host, st, begin) {
    if (!nativeMode(host)) return null;
    if (st.starting) return true;
    const index = host.state.activeLayerIdx, frame = host.state.currentFrame;
    st.starting = true;
    reassignSpan(host, index, frame).then((r) => {
      st.starting = false;
      if (!st.active && r && r.status === 'span' && host.state.activeLayerIdx === index && host.state.currentFrame === frame) begin(index, r.from, r.to);
    });
    return true;
  }
  // The picks and Enter run in click order: a result that lands after the flow ended, changed step or layer is dropped.
  function queue(st, step) { st.chain = (st.chain || Promise.resolve()).then(step).catch(() => {}); }
  // A click: the gesture is swallowed at once (true) and the element read from the native hit-test listing; `apply(strokeId)` takes
  // its identity (null on empty canvas). Returns null off native mode.
  function reassignClick(host, st, e, apply) {
    if (!nativeMode(host)) return null;
    const step = st.step, layer = st.layer, frame = step === 1 ? st.frameA : st.frameB, here = () => st.active && st.step === step && st.layer === layer;
    queue(st, () => here() && pickStroke(host, layer, e && e.clientX, e && e.clientY, frame).then((r) => {
      if (!here() || host.state.currentFrame !== frame) return;
      if (r.status === 'hit') apply(r.strokeId); else notice(host, r);
    }));
    return true;
  }
  // Enter: `confirm` runs after the picks already in flight. Returns null off native mode.
  function afterPicks(host, st, confirm) { if (!nativeMode(host)) return null; queue(st, confirm); return true; }
  // Harmonize (baseline #126): the two spans around the edited interpolated frame are re-tweened with it as a key, and the frame
  // then stays a soft, manually edited in-between; one native split does all of it (opt-in panel toggle).
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
    }, index);
  }
  return Object.freeze({ active: nativeMode, generate, reassign, pairCurve, reassignSpan, pickStroke, startReassign, reassignClick, afterPicks, split, harmonize, CODES });
}));
