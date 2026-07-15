// ---- LABS PROTOTYPE — "Lagoon" radial tool menu (Autodesk SketchBook) ----
// Press Q with the cursor over the canvas: a radial menu of the most-used
// tools appears around the cursor; click one to switch, Escape/Q/click-
// outside to dismiss. Pure DOM overlay — zero interaction with the
// drawing pipeline (no pointer interception on the canvas itself, the
// menu only exists while visible), so there is nothing for CLAUDE.md §1's
// consumer list to learn about.
//
// Q rather than SketchBook's press-and-hold-on-canvas: pointerdown on the
// canvas already MEANS "start drawing" in every Nemo tool (draw-bridge and
// friends intercept in the capture phase), so a hold gesture would race
// every bridge. A keyboard trigger sidesteps that entirely.
(function () {
  var TOOLS = [
    { tool: 'draw', label: 'Pinceau', glyph: 'B' },
    { tool: 'eraser', label: 'Gomme', glyph: 'E' },
    { tool: 'fill', label: 'Pot', glyph: 'G' },
    { tool: 'select', label: 'Sélection', glyph: 'V' },
    { tool: 'pen', label: 'Plume', glyph: 'P' },
    { tool: 'hand', label: 'Main', glyph: 'H' },
  ];
  var R = 78; // ring radius px
  var el = null, lastMouse = { x: 0, y: 0 };

  function close() { if (el) { el.remove(); el = null; } }

  function open(cx, cy) {
    close();
    el = document.createElement('div');
    el.style.cssText = 'position:fixed;inset:0;z-index:99999;';
    // Backdrop click closes without selecting.
    el.addEventListener('pointerdown', function (e) { if (e.target === el) close(); });
    TOOLS.forEach(function (t, i) {
      var a = -Math.PI / 2 + i * 2 * Math.PI / TOOLS.length;
      var b = document.createElement('button');
      b.textContent = t.glyph;
      b.title = t.label;
      b.style.cssText =
        'position:fixed;width:44px;height:44px;border-radius:50%;border:1px solid rgba(255,255,255,.14);' +
        'background:#26252c;color:#eceae7;font:600 14px system-ui;cursor:pointer;box-shadow:0 4px 14px rgba(0,0,0,.4);' +
        'display:flex;align-items:center;justify-content:center;transform:translate(-50%,-50%);' +
        'left:' + (cx + R * Math.cos(a)) + 'px;top:' + (cy + R * Math.sin(a)) + 'px;';
      b.addEventListener('pointerdown', function (e) {
        e.stopPropagation();
        close();
        if (window.SM && window.SM.setTool) window.SM.setTool(t.tool);
      });
      b.addEventListener('pointerenter', function () { b.style.background = '#4E6FF2'; });
      b.addEventListener('pointerleave', function () { b.style.background = '#26252c'; });
      el.appendChild(b);
    });
    // Center dot marks the invocation point.
    var dot = document.createElement('div');
    dot.style.cssText = 'position:fixed;width:8px;height:8px;border-radius:50%;background:#4E6FF2;transform:translate(-50%,-50%);left:' + cx + 'px;top:' + cy + 'px;';
    el.appendChild(dot);
    document.body.appendChild(el);
  }

  document.addEventListener('pointermove', function (e) { lastMouse.x = e.clientX; lastMouse.y = e.clientY; }, true);
  document.addEventListener('keydown', function (e) {
    if (!window.SMLabs.isOn('lagoon-menu')) return;
    // Never steal Q while typing in a field.
    var tag = (e.target && e.target.tagName) || '';
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || (e.target && e.target.isContentEditable)) return;
    if (e.key === 'Escape' && el) { close(); return; }
    if (e.key !== 'q' && e.key !== 'Q') return;
    if (el) { close(); return; }
    open(lastMouse.x, lastMouse.y);
  }, true);

  window.SMLabs.register('lagoon-menu', {
    flag: 'nemo-labs-lagoon',
    describe: 'Menu radial d\'outils autour du curseur sur la touche Q (SketchBook "lagoon")',
    onDisable: close,
  });
})();
