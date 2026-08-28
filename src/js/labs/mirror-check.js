// ---- LABS PROTOTYPE — Mirror check (the animator's mirror) ----
// Hold M: the canvas view flips horizontally — the centuries-old trick of
// checking a drawing in a mirror to reset your eye and expose proportion
// drift. Release M: back to normal.
//
// Display-only by construction: a CSS scaleX(-1) on the canvas AREA
// element, never a viewport/document change — the engine, Paper and every
// tool keep their real coordinates. Because pointer coordinates are NOT
// remapped while flipped, drawing in the mirror would land mirrored —
// so any pointerdown on the canvas while flipped instantly unflips
// instead (check, don't draw — matching how animators actually use a
// physical mirror).
(function () {
  var flipped = false, target = null;

  function area() {
    return document.getElementById('canvas-area') ||
           (document.getElementById('drawing-canvas') && document.getElementById('drawing-canvas').parentElement);
  }
  function flip(on) {
    var t = area();
    if (!t) return;
    if (on === flipped) return;
    flipped = on;
    if (on) { target = t; t.style.transform = 'scaleX(-1)'; }
    else if (target) { target.style.transform = ''; target = null; }
  }

  document.addEventListener('keydown', function (e) {
    if (!window.SMLabs.isOn('mirror-check')) return;
    var tag = (e.target && e.target.tagName) || '';
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || (e.target && e.target.isContentEditable)) return;
    if (e.key !== 'm' && e.key !== 'M') return;
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    // M is also the default Fill/Stroke-Select tool shortcut (timeline.js
    // runToolShortcut) — without stopPropagation the core bubble-phase
    // onKeyDown still sees this keydown and switches the active tool.
    e.stopPropagation();
    e.preventDefault();
    if (!e.repeat) flip(true);
  }, true);
  document.addEventListener('keyup', function (e) {
    if (e.key === 'm' || e.key === 'M') flip(false);
  }, true);
  // Check, don't draw: pointerdown while mirrored snaps back to normal
  // BEFORE any tool sees the event's (unmirrored) coordinates.
  document.addEventListener('pointerdown', function () { if (flipped) flip(false); }, true);
  window.addEventListener('blur', function () { flip(false); });

  window.SMLabs.register('mirror-check', {
    flag: 'nemo-labs-mirror',
    describe: 'labsDescribeMirrorCheck',
    onDisable: function () { flip(false); },
  });
})();
