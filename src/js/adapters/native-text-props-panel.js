/* Native Text properties panel (#2372, insp-text-props). With exactly ONE native text block selected by Select on a native object project, the
 * baseline tp-* section (1065e9d8 timeline.js textPropsRoot / updateTextPropsPanel / applyTextPropsEdit) shows that block's stored settings and each
 * change commits as ONE `object.text.edit` patch (one revision, one history entry) through the retained native text writer. The values shown are the
 * native text inspect read (`query.document.object.text`), never a Paper item; this module holds no document state beyond a per-selection read cache,
 * reaches no Paper writer and has no fallback: an unavailable route, a stale selection or a refused edit is a notice and nothing is written.
 * The caller names the ONE field that changed (a stale panel default can never overwrite a setting the user did not touch) and the patch carries
 * only the members that differ from the stored block. Edits are serialised: one waits for the commit before it and its re-read selection. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeTextPropsPanel = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var FIELDS = ['text', 'font', 'size', 'color', 'align', 'bold', 'italic', 'underline', 'strike', 'letterSpacing', 'wordSpacing', 'lineHeightMult', 'textCase', 'fixedWidth'];
  var ALIGN = ['left', 'center', 'right'], CASES = ['none', 'upper', 'lower', 'capitalize'];
  // The baseline reads an absent member with these defaults (d.letterSpacing||0, d.lineHeightMult||1.25, d.textCase||'none' ...).
  var DEFAULTS = { bold: false, italic: false, underline: false, strike: false, letterSpacing: 0, wordSpacing: 0, lineHeightMult: 1.25, textCase: 'none', fixedWidth: null };
  var chains = new WeakMap(), caches = new WeakMap(), serial = 0;
  function t(root, key, fallback) { try { return root.SM && typeof root.SM.t === 'function' ? root.SM.t(key) || fallback : fallback; } catch (_) { return fallback; } }
  function notice(root, text) { if (typeof root.showToast === 'function') try { root.showToast(text); } catch (_) {} }
  function session(root) { var P = root.NemoNativeObjectProject; try { var s = P && typeof P.desktop === 'function' ? P.desktop(root) : null; return s && s.blocksLegacy() === true ? s : null; } catch (_) { return null; } }
  function writer(root) { var W = root.NemoNativeObjectTextLayout; try { return W && typeof W.desktop === 'function' ? W.desktop(root) : null; } catch (_) { return null; } }
  function reader(root, s) { var R = root.NemoNativeTextRead; return R && typeof R.create === 'function' && s ? R.create(s) : null; }
  // The one selected native text block (null for anything else: nothing, several, a shape, an image or a legacy item).
  function block(root) {
    var sel = root.selectedPaths, p = sel && sel.length === 1 ? sel[0] : null, d = p && p.data;
    return d && d.nativeRole === 'text' && d.nativeStableTarget && root.state && root.state.tool === 'select' ? p : null;
  }
  // True when the panel's fields commit natively: a text block is selected on a native object project that has the retained text routes.
  function owns(root) { return !!block(root) && !!session(root) && !!writer(root) && !!reader(root, session(root)); }
  function key(root, p) {
    var I = root.NemoSelectCanvasIntent, tg = p.data.nativeStableTarget;
    return JSON.stringify([I ? I.pickContext(root) : null, tg.layerUid, tg.frameScope && tg.frameScope.frame, tg.strokeId]);
  }
  // Reads the selected block's stored settings; null when it cannot be proven current. The result is a detached copy.
  async function read(root, p) {
    var I = root.NemoSelectCanvasIntent, s = session(root), R = reader(root, s), shown = I && I.presentedState(root), tg = p.data.nativeStableTarget;
    if (!R || !shown || !tg || tg.frameScope.kind !== 'authored' || tg.contextId !== 'scene-root' || I.heldId(root) !== 'scene-root') return null;
    var v = await R.readBlock({ contextId: tg.contextId, layerUid: tg.layerUid, frameScope: { kind: 'authored', frame: tg.frameScope.frame }, strokeId: tg.strokeId }, shown.pin);
    return JSON.parse(JSON.stringify(v.block));
  }
  // Shows the stored block through `apply(block)` (the panel's field filler). A read lands asynchronously and is dropped when the selection moved on;
  // a block already read for this very selection and revision is applied at once. Returns true when the panel is native-owned.
  function sync(root, apply) {
    if (!owns(root)) return false;
    var p = block(root), k = key(root, p), c = caches.get(root);
    if (c && c.key === k && c.p === p) { if (c.block) apply(c.block); return true; }
    c = { key: k, p: p, block: null, id: ++serial }; caches.set(root, c);
    read(root, p).then(function (b) {
      if (caches.get(root) !== c || !b || !block(root) || block(root) !== p || key(root, p) !== k) return;
      c.block = b; apply(b);
    }, function () { if (caches.get(root) === c) caches.delete(root); }); // a failed read is tried again by the next panel update
    return true;
  }
  function same(a, b) { return JSON.stringify(a) === JSON.stringify(b); }
  // The font member the panel's family name stands for: the stored weight suffix is kept when that family has it, else Regular.
  function fontFor(root, name, stored) {
    var C = root.NemoNativeObjectItemContract, fonts = C && C.FONTS || [], storedKey = stored && stored.kind === 'bundled' ? stored.fontKey : '';
    var bold = /-Bold$/.test(storedKey), bare = String(name || '').replace(/-(Bold|Regular)$/, '');
    var pick = [bare + (bold ? '-Bold' : '-Regular'), bare + '-Regular'].filter(function (k) { return fonts.indexOf(k) >= 0; })[0];
    return pick ? { kind: 'bundled', fontKey: pick } : null;
  }
  // The patch of ONE panel edit against the stored block, with the baseline coercions; {error} names a refused value, {} an edit that changes nothing.
  function patchFor(root, b, field, raw) {
    var v, stored = Object.prototype.hasOwnProperty.call(b, field) && b[field] !== undefined ? b[field] : DEFAULTS[field];
    if (FIELDS.indexOf(field) < 0) return { error: 'field' };
    if (field === 'text') { v = String(raw == null ? '' : raw); if (!v.trim()) return { error: 'blank' }; }
    else if (field === 'font') { v = fontFor(root, raw, b.font); if (!v) return { error: 'font' }; }
    else if (field === 'size') { v = parseInt(raw, 10); if (!(v > 0)) return { error: 'number' }; v = Math.max(4, Math.min(800, v)); }
    else if (field === 'color') {
      if (!/^#[0-9a-fA-F]{6}(?:[0-9a-fA-F]{2})?$/.test(String(raw))) return { error: 'color' };
      v = String(raw).toLowerCase(); if (v.length === 7 && typeof b.color === 'string' && b.color.length === 9) v += b.color.slice(7); // the stored alpha stays
      stored = typeof stored === 'string' ? stored.toLowerCase() : stored;
    } else if (field === 'align') { if (ALIGN.indexOf(raw) < 0) return { error: 'value' }; v = raw; }
    else if (field === 'textCase') { if (CASES.indexOf(raw) < 0) return { error: 'value' }; v = raw; }
    else if (field === 'bold' || field === 'italic' || field === 'underline' || field === 'strike') v = !!raw;
    else if (field === 'lineHeightMult') v = Math.max(0.5, Math.min(4, parseFloat(raw) || 1.25));
    else if (field === 'fixedWidth') { if (raw === null) v = null; else { v = parseFloat(raw); v = Math.max(10, Number.isFinite(v) && v > 0 ? v : 300); } }
    else { v = parseFloat(raw) || 0; } // letterSpacing, wordSpacing
    var out = {}; if (!same(v, stored)) out[field] = v; return out;
  }
  function refresh(root) {
    var O = root.NemoNativeSelectOverlay;
    caches.delete(root);
    if (O && typeof O.show === 'function') O.show(root);
    if (typeof root.updateUI === 'function') root.updateUI();
    if (root.SMEngineBridge && typeof root.SMEngineBridge.renderNow === 'function') root.SMEngineBridge.renderNow();
  }
  var REFUSALS = { blank: 'Text cannot be empty.', font: 'That font is not available for native text.', number: 'Enter a valid number.', color: 'Enter a color as #rrggbb.', value: 'That setting is not valid.', field: 'That setting does not belong to text.' };
  async function commit(root, field, raw) {
    var I = root.NemoSelectCanvasIntent, O = root.NemoNativeSelectOverlay, p = block(root), W = writer(root), s = session(root);
    if (!p || !W || !s || !I) { notice(root, 'The text settings are not available right now.'); return { status: 'unavailable' }; }
    var ctx = I.pickContext(root);
    // A selection that outlived a frame, layer, tool or revision change is dropped by the overlay: it is never edited.
    if (!(O && typeof O.selectionContext === 'function' && O.selectionContext(root) === ctx)) { notice(root, 'The selection no longer applies here — select the text again.'); return { status: 'unavailable' }; }
    var tg = p.data.nativeStableTarget, shown = I.presentedState(root);
    if (!tg || tg.frameScope.kind !== 'authored' || I.heldId(root) !== 'scene-root' || tg.contextId !== 'scene-root') { notice(root, 'Text settings cannot be edited here yet.'); return { status: 'unavailable' }; }
    if (!shown) { notice(root, 'The text settings are not available right now.'); return { status: 'unavailable' }; }
    var stored, patch, receipt;
    try {
      stored = await read(root, p);
      patch = stored && patchFor(root, stored, field, raw);
    } catch (_) { stored = null; }
    if (!stored) { notice(root, 'The text settings are not available right now.'); refresh(root); return { status: 'unavailable' }; }
    if (patch.error) { notice(root, REFUSALS[patch.error]); refresh(root); return { status: 'invalid' }; } // the panel shows the stored values again
    if (!Object.keys(patch).length) { refresh(root); return { status: 'noop' }; }
    var target = { contextId: tg.contextId, frameScope: { kind: 'authored', frame: tg.frameScope.frame }, layerUid: tg.layerUid, strokeId: tg.strokeId };
    try {
      receipt = await W.editBlock(target, patch, shown.pin, function () { return I.pickContext(root) === ctx; });
    } catch (error) {
      notice(root, t(root, 'toastTextEditRebuildFailed', 'The text could not be changed.'));
      refresh(root);
      return { status: error && error.code === 'stale_revision' ? 'stale' : 'unavailable' };
    }
    if (!receipt || receipt.applied === false) { refresh(root); return { status: 'noop' }; }
    // The selection is read again so the box and the panel show what the engine stored (the re-laid-out block).
    var fresh = null;
    try { fresh = await I.reread(root, [p], false); } catch (_) { fresh = null; }
    // The user may have picked something else while the commit ran: their selection stands.
    if (root.selectedPaths && root.selectedPaths[0] === p) {
      root.selectedPaths = fresh && fresh.paths.length ? fresh.paths : [];
      if (root.state) root.state.selectedStrokeIndices = [];
    }
    refresh(root);
    return { status: 'applied', patch: patch };
  }
  // Commits one panel field of the selected native text block: `field` names the setting, `raw` the field's value (text, a number's text, a bool).
  function edit(root, field, raw) {
    var run = (chains.get(root) || Promise.resolve()).then(function () { return commit(root, field, raw); }, function () { return commit(root, field, raw); });
    chains.set(root, run.then(function () {}, function () {}));
    return run.catch(function () { notice(root, t(root, 'toastTextEditRebuildFailed', 'The text could not be changed.')); return { status: 'unavailable' }; });
  }
  return Object.freeze({ owns: owns, sync: sync, edit: edit, patchFor: patchFor, FIELDS: FIELDS });
}));
