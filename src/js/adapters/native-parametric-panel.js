/* Native Rounded corners / Ellipse arc / Star panels (#2372, insp-parametric). With ONE dynamic rectangle, ellipse or star selected on a native
 * object project, a panel field commits the shape's new parameters as ONE `object.parametric.set` (one revision, one history entry), the baseline
 * commitCornerEdit / commitArcEdit / commitStarEdit over `data.paramShape` (1065e9d8 timeline.js). The values the panel shows are the selection's
 * listed `paramShape` (kind and the members that kind owns); the engine rebuilds the baked geometry from them, so this module holds no document
 * state, reaches no Paper writer and has no fallback: an unavailable route, a stale selection or a refused edit is a notice and nothing is written.
 * A typed value commits on change (Enter / blur) and a scrub commits once at release, so a gesture is never one entry per keystroke or per tick.
 * Edits are serialised: one waits for the commit before it and its re-read selection. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeParametricPanel = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var MEMBERS = { rect: ['tl', 'tr', 'br', 'bl'], ellipse: ['startAngle', 'sweep', 'innerRadius'], star: ['pointCount', 'innerRatio', 'cornerRadius'] };
  // The baseline builders read an absent member with these defaults (ps.tl||0, ps.sweep!==undefined?ps.sweep:359.9, ps.pointCount||5 ...).
  var DEFAULTS = { tl: 0, tr: 0, br: 0, bl: 0, startAngle: 0, sweep: 359.9, innerRadius: 0, pointCount: 5, innerRatio: 0.5, cornerRadius: 0 };
  var chains = new WeakMap();
  function notice(root, text) { if (typeof root.showToast === 'function') try { root.showToast(text); } catch (_) {} }
  function vector(root) { var V = root.NemoNativeObjectVector; try { return V && typeof V.desktop === 'function' ? V.desktop(root) : null; } catch (_) { return null; } }
  // The one selected native path that carries a listed dynamic shape (null for anything else).
  function shape(root, path) {
    var p = path || (root.selectedPaths && root.selectedPaths.length === 1 ? root.selectedPaths[0] : null), d = p && p.data;
    return d && d.nativeStableTarget && d.paramShape && MEMBERS[d.paramShape.kind] ? p : null;
  }
  // True when `path` is such a selection on a native object project: the panel's fields then commit natively and never through a Paper item.
  function owns(root, path) { return !!shape(root, path) && !!vector(root); }
  // Applies the baseline clamps of one panel edit to a copy of the listed shape; the result names every member of its kind (the set body, no box).
  function next(listed, field, raw, linked) {
    var v = parseFloat(raw) || 0, body = { kind: listed.kind };
    MEMBERS[listed.kind].forEach(function (k) { body[k] = typeof listed[k] === 'number' && Number.isFinite(listed[k]) ? listed[k] : DEFAULTS[k]; });
    if (listed.kind === 'rect') {
      v = Math.max(0, v);
      if (linked) MEMBERS.rect.forEach(function (k) { body[k] = v; }); else body[field] = v;
    } else if (listed.kind === 'ellipse') body[field] = field === 'innerRadius' ? Math.max(0, Math.min(95, v)) / 100 : v;
    else if (field === 'pointCount') body.pointCount = Math.max(3, Math.round(v));
    else if (field === 'innerRatio') body.innerRatio = Math.max(0.05, Math.min(1, v / 100));
    else body.cornerRadius = Math.max(0, v);
    return MEMBERS[listed.kind].indexOf(field) < 0 ? null : body;
  }
  function refresh(root) {
    var O = root.NemoNativeSelectOverlay;
    if (O && typeof O.show === 'function') O.show(root);
    if (typeof root.updateUI === 'function') root.updateUI();
    if (root.SMEngineBridge && typeof root.SMEngineBridge.renderNow === 'function') root.SMEngineBridge.renderNow();
  }
  async function commit(root, field, raw, linked) {
    var I = root.NemoSelectCanvasIntent, O = root.NemoNativeSelectOverlay, p = shape(root), V = vector(root);
    if (!p || !V || !I) { notice(root, 'The shape settings are not available right now.'); return { status: 'unavailable' }; }
    // A selection that outlived a frame, layer, tool or revision change is dropped by the overlay: it is never edited.
    if (!(O && typeof O.selectionContext === 'function' && O.selectionContext(root) === I.pickContext(root))) {
      notice(root, 'The selection no longer applies here — select the shape again.');
      return { status: 'unavailable' };
    }
    var t = p.data.nativeStableTarget, shown = I.presentedState(root), body = next(p.data.paramShape, field, raw, linked);
    if (!body) { notice(root, 'That setting does not belong to this shape.'); return { status: 'invalid' }; }
    if (!t || t.frameScope.kind !== 'authored' || I.heldId(root) !== 'scene-root' || t.contextId !== 'scene-root') {
      notice(root, 'Shape settings cannot be edited here yet.');
      return { status: 'unavailable' };
    }
    if (!shown) { notice(root, 'The shape settings are not available right now.'); return { status: 'unavailable' }; }
    var result;
    try { result = await V.setParametric({ layerUid: t.layerUid, frame: t.frameScope.frame, strokeId: t.strokeId }, body, { origin: shown.pin }); } catch (_) { result = null; }
    if (!result || (result.status !== 'applied' && result.status !== 'noop')) {
      notice(root, 'The shape could not be changed' + (result && result.status === 'stale' ? ': the document moved on.' : '.'));
      refresh(root); // the panel shows the stored values again
      return { status: result ? result.status : 'unavailable' };
    }
    if (result.status === 'applied' && root.state) {
      if (field === 'pointCount') root.state.starPointCount = body.pointCount;
      else if (field === 'innerRatio') root.state.starInnerRatio = body.innerRatio;
    }
    // The selection is read again so the panel and the box show what the engine stored (the rebuilt geometry and the clamped values).
    var before = [p], fresh = null;
    try { fresh = await I.reread(root, before, false); } catch (_) { fresh = null; }
    // The user may have picked something else while the commit ran: their selection stands.
    if (root.selectedPaths && root.selectedPaths[0] === p) {
      root.selectedPaths = fresh && fresh.paths.length ? fresh.paths : [];
      if (root.state) root.state.selectedStrokeIndices = [];
    }
    refresh(root);
    return { status: result.status };
  }
  // Commits one panel field of the selected dynamic shape: `field` is the member name, `raw` the field's text, `opts.linked` the corner link switch.
  function edit(root, field, raw, opts) {
    var run = (chains.get(root) || Promise.resolve()).then(function () { return commit(root, field, raw, !!(opts && opts.linked)); }, function () { return commit(root, field, raw, !!(opts && opts.linked)); });
    chains.set(root, run.then(function () {}, function () {}));
    return run.catch(function () { notice(root, 'The shape could not be changed.'); return { status: 'unavailable' }; });
  }
  // "Rendre dynamique" has no native route yet: a notice, never a Paper conversion.
  function convertUnavailable(root) { notice(root, 'Making a shape dynamic is not available on native projects yet.'); }
  return Object.freeze({ owns: owns, edit: edit, next: next, convertUnavailable: convertUnavailable });
}));
