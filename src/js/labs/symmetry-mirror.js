// ---- LABS PROTOTYPE — Symmetry/Mirror drawing (Autodesk SketchBook-style) ----
// Not wired into index.html or any core tool file beyond a single guarded
// hook in draw-bridge.js's commitStroke (`window.SMLabs.onStrokeCommitted`,
// no-op when this file isn't loaded or the toggle is off). See
// docs/feature-scouting.md for the full audit this prototype came out of.
//
// Off by default (localStorage flag) — enable from the JS console for now:
//   window.SMLabs.toggleSymmetry()
// A real Réglages entry can be added once/if this is picked for real.
//
// Deliberately produces an ORDINARY Path with no data.* tags of its own —
// per CLAUDE.md §1 (new item/tag must be handled by every layer.children
// consumer), the safest way to add something new without auditing every
// consumer is to not be structurally new at all: the mirrored copy behaves
// exactly like a second hand-drawn stroke to fill matching, tween matching,
// save/export, undo — everything already treats a plain Path correctly.
(function () {
  var FLAG = 'nemo-labs-symmetry';
  var state_ = { enabled: localStorage.getItem(FLAG) === '1', axisX: null };

  function axisX() {
    if (state_.axisX != null) return state_.axisX;
    // Default: center of the current artboard/canvas, same convention as
    // export.js's own canvas-size read.
    return (window.state && window.state.canvasW ? window.state.canvasW : 1920) / 2;
  }

  function mirrorPoint(p) { return new Point(2 * axisX() - p.x, p.y); }

  // Clones the just-committed stroke, reflects every segment (point +
  // both handles, so curve shape mirrors correctly not just endpoints)
  // across the vertical axis, and inserts it as a plain sibling — same
  // layer, right next to the original, no special data tag.
  function onStrokeCommitted(path, layer) {
    if (!state_.enabled) return;
    if (!path || !path.segments || !path.segments.length) return;
    // Skip mirroring something already sitting exactly on the axis (avoids
    // drawing an invisible zero-width duplicate on top of itself) and skip
    // our own mirrored output if this ever gets called on it (defensive —
    // current hook only fires from real commitStroke, not from here).
    if (path.data && path.data._labsSymmetryCopy) return;
    var mirrored = path.clone({ insert: false });
    mirrored.segments.forEach(function (seg) {
      var p = mirrorPoint(seg.point);
      var hi = new Point(-seg.handleIn.x, seg.handleIn.y);
      var ho = new Point(-seg.handleOut.x, seg.handleOut.y);
      seg.point = p; seg.handleIn = hi; seg.handleOut = ho;
    });
    // Mirroring flips winding order (x negated) — re-close/orient so fills
    // and stroke direction read the same as the original, not inside-out.
    if (mirrored.closed) mirrored.reverse();
    mirrored.insertAbove(path);
    if (typeof tagOwner === 'function') tagOwner(mirrored);
    if (typeof saveActiveLayerFrame === 'function') saveActiveLayerFrame();
  }

  function toggleSymmetry() {
    state_.enabled = !state_.enabled;
    localStorage.setItem(FLAG, state_.enabled ? '1' : '0');
    if (typeof showToast === 'function') showToast('Labs — Symétrie miroir : ' + (state_.enabled ? 'activée' : 'désactivée'));
    return state_.enabled;
  }

  window.SMLabs = window.SMLabs || {};
  window.SMLabs.onStrokeCommitted = onStrokeCommitted;
  window.SMLabs.toggleSymmetry = toggleSymmetry;
  window.SMLabs.symmetryEnabled = function () { return state_.enabled; };
})();
