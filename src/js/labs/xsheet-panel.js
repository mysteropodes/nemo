// ---- LABS PROTOTYPE — X-sheet / exposure sheet panel (TVPaint/Umoupen) ----
// The traditional vertical exposure sheet: one row per frame, one column
// per layer, glyphs for keyframe (●), tween (◆), hold (│) and empty.
// Read-only production view + click a row to jump the playhead there —
// no editing from the sheet in this prototype.
//
//   SMLabs.enable('xsheet')   — opens the floating panel (and on reload)
//   SMLabs.disable('xsheet')  — closes it
//
// Pure DOM overlay; refreshes itself by observing #frame-grid rebuilds
// (renderTimeline runs on every data change worth showing) — same
// observe-don't-monkey-patch approach as timeline-markers.
(function () {
  var panel = null, mo = null;

  function glyphFor(f) {
    if (!f) return ['', ''];
    if (f.isKeyframe) return ['●', '#9FB4FA'];
    if (f.isInterpolated) return ['◆', '#7bd88f'];
    if (f.strokes && f.strokes.length) return ['│', '#888'];
    return ['', ''];
  }

  function build() {
    if (!window.SMLabs.isOn('xsheet')) return;
    if (!window.state || !state.layers || !state.layers.length) return;
    if (!panel) {
      panel = document.createElement('div');
      panel.id = 'labs-xsheet';
      panel.style.cssText =
        'position:fixed;top:60px;right:16px;width:auto;max-height:70vh;overflow:auto;z-index:9999;' +
        'background:#201f25;border:1px solid rgba(255,255,255,.12);border-radius:10px;' +
        'font:11px ui-monospace,monospace;color:#eceae7;box-shadow:0 8px 30px rgba(0,0,0,.5);padding:6px 0;';
      document.body.appendChild(panel);
    }
    var rows = [];
    var head = '<tr><th style="padding:2px 8px;color:#888;">#</th>';
    state.layers.forEach(function (ld) { head += '<th style="padding:2px 8px;color:#888;max-width:70px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">' + (ld.name || '') + '</th>'; });
    head += '</tr>';
    for (var f = 0; f < state.totalFrames; f++) {
      var cur = f === state.currentFrame;
      var tr = '<tr data-frame="' + f + '" style="cursor:pointer;' + (cur ? 'background:rgba(78,111,242,.25);' : (f % state.fps === 0 ? 'background:rgba(255,255,255,.04);' : '')) + '">';
      tr += '<td style="padding:1px 8px;color:' + (cur ? '#9FB4FA' : '#666') + ';text-align:right;">' + (f + 1) + '</td>';
      for (var l = 0; l < state.layers.length; l++) {
        var g = glyphFor(state.layers[l].frames[f]);
        tr += '<td style="padding:1px 8px;text-align:center;color:' + (g[1] || '#444') + ';">' + g[0] + '</td>';
      }
      rows.push(tr + '</tr>');
    }
    panel.innerHTML =
      '<div style="display:flex;justify-content:space-between;align-items:center;padding:2px 8px 6px;border-bottom:1px solid rgba(255,255,255,.08);">' +
      '<b style="font-size:11px;">X-sheet</b><span style="color:#666;">● clé &nbsp;◆ tween &nbsp;│ tenu</span></div>' +
      '<table style="border-collapse:collapse;">' + head + rows.join('') + '</table>';
    panel.querySelectorAll('tr[data-frame]').forEach(function (tr) {
      tr.addEventListener('click', function () { goToFrame(parseInt(tr.dataset.frame, 10)); });
    });
    var curRow = panel.querySelector('tr[data-frame="' + state.currentFrame + '"]');
    if (curRow && curRow.scrollIntoView) curRow.scrollIntoView({ block: 'nearest' });
  }

  function observe() {
    var grid = document.getElementById('frame-grid');
    if (!grid || mo) return;
    mo = new MutationObserver(function () { if (window.SMLabs.isOn('xsheet')) build(); });
    mo.observe(grid, { childList: true });
  }

  window.SMLabs.register('xsheet', {
    flag: 'nemo-labs-xsheet',
    describe: 'Feuille d\'exposition flottante : frames × calques, ●=clé ◆=tween │=tenu, clic = aller à la frame',
    onEnable: function () { observe(); build(); },
    onDisable: function () { if (panel) { panel.remove(); panel = null; } },
  });
  if (window.SMLabs.isOn('xsheet')) { observe(); build(); }
})();
