// ---- LABS PROTOTYPE — Symmetry drawing ----
// Two modes, each its own flag (see labs-core.js for the console API):
//   'symmetry'        — vertical-axis mirror across the canvas center
//   'radial-symmetry' — N rotated copies around the canvas center
//                       (mandala mode, 2..16 sectors;
//                       SMLabs.setRadialSectors(n) to change, default 6)
//
// Deliberately produces ORDINARY Paths with no data.* tags of their own —
// per CLAUDE.md §1 (new item/tag must be handled by every layer.children
// consumer), the safest way to add something new without auditing every
// consumer is to not be structurally new at all: each copy behaves exactly
// like another hand-drawn stroke to fill matching, tween matching,
// save/export, undo — everything already treats a plain Path correctly.
// The copies land inside the same commitStroke undo snapshot, so one
// Cmd+Z reverts original + copies together (verified live).
//
// Live preview while drawing (2026-07, "il faut que la symétrie soit
// visible pendant le dessin en direct pas que au relâchement"): onPreview
// mirrors/rotates the in-progress `samples` array and hands it to
// draw-bridge.js's own overlayItem() via overlayItemFor — same shaping
// logic as the real stroke, so the live reflection looks pixel-identical
// to what commitStroke will produce, not an approximation.
(function () {
  function canvasCenter() {
    return new Point(
      (window.state && state.canvasW ? state.canvasW : 1920) / 2,
      (window.state && state.canvasH ? state.canvasH : 1080) / 2
    );
  }

  // -- vertical mirror --------------------------------------------------
  function mirrorStroke(path) {
    var ax = canvasCenter().x;
    var mirrored = path.clone({ insert: false });
    mirrored.segments.forEach(function (seg) {
      seg.point = new Point(2 * ax - seg.point.x, seg.point.y);
      seg.handleIn = new Point(-seg.handleIn.x, seg.handleIn.y);
      seg.handleOut = new Point(-seg.handleOut.x, seg.handleOut.y);
    });
    // Mirroring flips winding — re-orient closed shapes so fills read the
    // same way as the original, not inside-out.
    if (mirrored.closed) mirrored.reverse();
    mirrored.insertAbove(path);
    if (typeof tagOwner === 'function') tagOwner(mirrored);
  }

  window.SMLabs.register('symmetry', {
    flag: 'nemo-labs-symmetry',
    describe: 'Miroir vertical : chaque trait est dupliqué de l\'autre côté de l\'axe central du canvas',
    onStroke: function (path) {
      if (!path.segments || !path.segments.length) return;
      mirrorStroke(path);
    },
    onPreview: function (samples, overlayItemFor) {
      if (!samples || samples.length < 2) return null;
      var ax = canvasCenter().x;
      var mirrored = samples.map(function (s) { return [2 * ax - s[0], s[1], s[2]]; });
      return overlayItemFor(mirrored);
    },
  });

  // -- radial (mandala) -------------------------------------------------
  var SECT_KEY = 'nemo-labs-radial-sectors';
  function sectors() {
    var n = parseInt(localStorage.getItem(SECT_KEY) || '6', 10);
    return Math.max(2, Math.min(16, isNaN(n) ? 6 : n));
  }
  window.SMLabs.setRadialSectors = function (n) {
    localStorage.setItem(SECT_KEY, String(Math.max(2, Math.min(16, n))));
    if (typeof showToast === 'function') showToast('Labs — symétrie radiale : ' + sectors() + ' secteurs');
    return sectors();
  };

  window.SMLabs.register('radial-symmetry', {
    flag: 'nemo-labs-radial',
    describe: 'Mandala : chaque trait est répété en rotation autour du centre (SMLabs.setRadialSectors(2..16), défaut 6)',
    onStroke: function (path) {
      if (!path.segments || !path.segments.length) return;
      var n = sectors(), c = canvasCenter();
      // Paper's own rotate() handles points + both handles correctly.
      for (var k = 1; k < n; k++) {
        var copy = path.clone({ insert: false });
        copy.rotate(k * 360 / n, c);
        copy.insertAbove(path);
        if (typeof tagOwner === 'function') tagOwner(copy);
      }
    },
    onPreview: function (samples, overlayItemFor) {
      if (!samples || samples.length < 2) return null;
      var n = sectors(), c = canvasCenter();
      var out = [];
      for (var k = 1; k < n; k++) {
        var ang = k * 2 * Math.PI / n;
        var cos = Math.cos(ang), sin = Math.sin(ang);
        var rotated = samples.map(function (s) {
          var dx = s[0] - c.x, dy = s[1] - c.y;
          return [c.x + dx * cos - dy * sin, c.y + dx * sin + dy * cos, s[2]];
        });
        out = out.concat(overlayItemFor(rotated));
      }
      return out;
    },
  });

  // Back-compat with the first prototype's console API.
  window.SMLabs.toggleSymmetry = function () { return window.SMLabs.toggle('symmetry'); };
  window.SMLabs.symmetryEnabled = function () { return window.SMLabs.isOn('symmetry'); };
})();
