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

  // Native authority: each column is the session's whole-row cells read, the same native timing query the
  // inspector uses, so held and blank cells come from the engine and never from the legacy frames (U31, #2374).
  // A refused read leaves its column unavailable instead of inventing blank cells.
  var nativeRows = null, nativeKey = '', reading = 0, retry = { key: '', count: 0 };
  function nativeSession() {
    var P = window.NemoNativeObjectProject, s = P && typeof P.desktop === 'function' && P.desktop(window);
    return s && typeof s.blocksLegacy === 'function' && s.blocksLegacy() && typeof s.readLayerCells === 'function' ? s : null;
  }
  function nativeGlyph(run) {
    if (!run) return ['', ''];
    if (run[2] === 'key') return ['●', '#9FB4FA'];
    if (run[2] === 'interpolated') return ['◆', '#7bd88f'];
    if (run[2] === 'held' && run[4]) return ['│', '#888'];
    return ['', ''];
  }
  function nativeColumns(session, pin) {
    var key = pin.documentId + '|' + pin.documentSnapshotId;
    if (key === nativeKey) return nativeRows;
    var ticket = ++reading;
    Promise.all(pin.layers.map(function (layer) {
      return Promise.resolve(session.readLayerCells(layer.layerUid, pin)).catch(function () { return null; });
    })).then(function (rows) {
      if (ticket !== reading || session.info() !== pin) return;
      // A revision change rebuilds the grid at once, so the first read can be refused while the presentation catches up;
      // it is retried a few times before its column is shown unavailable for that snapshot.
      if (retry.key !== key) retry = { key: key, count: 0 };
      if (rows.indexOf(null) >= 0 && retry.count < 3 && typeof setTimeout === 'function') { retry.count++; setTimeout(build, 150); return; }
      nativeKey = key;
      nativeRows = rows.map(function (runs) {
        if (!runs) return null;
        var cells = [];
        runs.forEach(function (run) { for (var f = run[0]; f < run[0] + run[1]; f++) cells[f] = run; });
        return cells;
      });
      build();
    });
    return null;
  }

  function build() {
    if (!window.SMLabs.isOn('xsheet')) return;
    var session = nativeSession(), pin = session && session.info(), columns = null;
    if (session) {
      if (!pin || !pin.layers || !pin.layers.length) return;
      columns = nativeColumns(session, pin);
      if (!columns) return;
    } else if (!window.state || !state.layers || !state.layers.length) return;
    var layers = session ? pin.layers : state.layers, total = session ? pin.totalFrames : state.totalFrames, fps = session ? pin.fps : state.fps;
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
    layers.forEach(function (ld, l) {
      var off = columns && !columns[l];
      head += '<th' + (off ? ' title="Native timing is unavailable."' : '') + ' style="padding:2px 8px;color:' + (off ? '#555' : '#888') + ';max-width:70px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">' + (ld.name || '') + (off ? ' —' : '') + '</th>';
    });
    head += '</tr>';
    for (var f = 0; f < total; f++) {
      var cur = f === state.currentFrame;
      var tr = '<tr data-frame="' + f + '" style="cursor:pointer;' + (cur ? 'background:rgba(78,111,242,.25);' : (f % fps === 0 ? 'background:rgba(255,255,255,.04);' : '')) + '">';
      tr += '<td style="padding:1px 8px;color:' + (cur ? '#9FB4FA' : '#666') + ';text-align:right;">' + (f + 1) + '</td>';
      for (var l = 0; l < layers.length; l++) {
        var g = columns ? nativeGlyph(columns[l] && columns[l][f]) : glyphFor(state.layers[l].frames[f]);
        tr += '<td style="padding:1px 8px;text-align:center;color:' + (g[1] || '#444') + ';">' + g[0] + '</td>';
      }
      rows.push(tr + '</tr>');
    }
    // Guarded SM.t (not a bare call): this build() can run at SCRIPT PARSE
    // TIME, before timeline.js has even created window.SM (see index.html's
    // script order — labs/*.js loads before timeline.js/i18n.js), whenever
    // the xsheet flag was already on from a previous session (the
    // "resume after reload" call at the bottom of this file).
    var t2 = (window.SM && SM.t) ? SM.t : function (k) { return k; };
    panel.innerHTML =
      '<div style="display:flex;justify-content:space-between;align-items:center;padding:2px 8px 6px;border-bottom:1px solid rgba(255,255,255,.08);">' +
      '<b style="font-size:11px;">X-sheet</b><span style="color:#666;">● ' + t2('labsXsheetKeyWord') + ' &nbsp;◆ tween &nbsp;│ ' + t2('labsXsheetHeldWord') + '</span></div>' +
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
    describe: 'labsDescribeXsheet',
    onEnable: function () { observe(); build(); },
    // Disable removes the panel AND the grid observer, and drops any pending native read.
    onDisable: function () {
      if (panel) { panel.remove(); panel = null; }
      if (mo) { mo.disconnect(); mo = null; }
      reading++; nativeRows = null; nativeKey = '';
    },
  });
  if (window.SMLabs.isOn('xsheet')) { observe(); build(); }
})();
