// ---- LABS — Opacity diagnostics inspector (T08/#1057) ----
// A thin, read-only floating panel over the recorded opacity command trace
// (T05's NemoOpacityDiagnostics, reached through the same
// NemoApplication.handle('diagnostics.trace', ...) path the diagnostics
// capability (T08) also uses -- panel and capability observe the exact
// same underlying trace, never a second copy of it).
//
//   SMLabs.enable('diagnostics-panel')   — opens the floating panel
//   SMLabs.disable('diagnostics-panel')  — closes it
//
// On-demand only: entries are fetched when the panel opens or Refresh is
// clicked, never on a timer or on every document edit -- the same "do not
// dump everything, fetch bounded detail on demand" property the diagnostics
// capability's own `limit` input documents. Read-only: a row click SELECTS
// the layer the entry touched (existing navigation, same as
// xsheet-panel.js's "click a row to jump the playhead"); nothing here ever
// calls diagnostics.replay or any property-writing operation.
(function () {
  var panel = null;

  // Trace fields are NOT trusted text. `requestId` is caller-supplied and only
  // length-checked by opacity-application.js's validate(), so it reaches here
  // verbatim from whatever drove the command -- including an MCP client. This
  // panel builds its rows as an innerHTML string, so every interpolated value
  // has to be escaped or a requestId can close an attribute and inject markup
  // into the Tauri webview, where window.__TAURI__ is in scope.
  var ESCAPES = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
  function esc(value) { return String(value).replace(/[&<>"']/g, function (c) { return ESCAPES[c]; }); }

  function correlatedLayerIndices() {
    var idx = {};
    if (window.state && Array.isArray(window._layerSel) && window._layerSel.length) {
      window._layerSel.forEach(function (i) { idx[i] = true; });
    } else if (window.state && Number.isInteger(window.state.activeLayerIdx)) {
      idx[window.state.activeLayerIdx] = true;
    }
    return idx;
  }

  function layerIndexOf(layerId) {
    if (!window.state || !Array.isArray(window.state.layers)) return -1;
    for (var i = 0; i < window.state.layers.length; i++) {
      if (window.state.layers[i].layerUid === layerId) return i;
    }
    return -1;
  }

  function fetchEntries() {
    if (!window.NemoApplication || !window.NemoOpacityApplication) return { error: 'diagnostics application not loaded' };
    var identity = window.NemoOpacityApplication.meta();
    var response = window.NemoApplication.handle({ apiVersion: 1, requestId: 'diagnostics-panel:' + Math.random(),
      ...identity, expectedRevision: identity.revision, operation: 'diagnostics.trace', payload: {} });
    if (!response.ok) return { error: (response.error && response.error.message) || 'diagnostics.trace failed' };
    return { entries: response.result.entries };
  }

  function rowHtml(entry, selectedLayers) {
    var req = entry.request;
    var layerId = req.payload && req.payload.layerId;
    var layerIdx = layerId ? layerIndexOf(layerId) : -1;
    var correlated = layerIdx >= 0 && selectedLayers[layerIdx];
    var statusColor = entry.ok ? '#7bd88f' : '#e08787';
    return '<tr data-layer-idx="' + layerIdx + '" style="cursor:' + (layerIdx >= 0 ? 'pointer' : 'default') + ';' +
      (correlated ? 'background:rgba(78,111,242,.25);' : '') + '">' +
      '<td style="padding:1px 8px;color:' + statusColor + ';">' + (entry.ok ? 'ok' : 'fail') + '</td>' +
      '<td style="padding:1px 8px;">' + esc(req.operation) + '</td>' +
      '<td style="padding:1px 8px;color:#888;">' + esc(entry.revision) + '</td>' +
      '<td style="padding:1px 8px;color:#888;max-width:120px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">' + esc(req.requestId) + '</td>' +
      '</tr>';
  }

  function build() {
    if (!SMLabs.isOn('diagnostics-panel')) return;
    if (!panel) {
      panel = document.createElement('div');
      panel.id = 'labs-diagnostics';
      panel.style.cssText =
        'position:fixed;top:60px;right:16px;width:auto;max-height:70vh;overflow:auto;z-index:9999;' +
        'background:#201f25;border:1px solid rgba(255,255,255,.12);border-radius:10px;' +
        'font:11px ui-monospace,monospace;color:#eceae7;box-shadow:0 8px 30px rgba(0,0,0,.5);padding:6px 0;';
      document.body.appendChild(panel);
    }
    refresh();
  }

  function refresh() {
    if (!panel) return;
    var result = fetchEntries();
    var selectedLayers = correlatedLayerIndices();
    var t2 = (typeof SM !== 'undefined' && SM.t) ? SM.t : function (k) { return k; };
    var body = result.error
      ? '<div style="padding:6px 8px;color:#e08787;">' + esc(result.error) + '</div>'
      : (result.entries.length
        ? '<table style="border-collapse:collapse;"><tr><th style="padding:2px 8px;color:#888;">status</th><th style="padding:2px 8px;color:#888;">operation</th><th style="padding:2px 8px;color:#888;">rev</th><th style="padding:2px 8px;color:#888;">requestId</th></tr>'
          + result.entries.slice().reverse().map(function (e) { return rowHtml(e, selectedLayers); }).join('') + '</table>'
        : '<div style="padding:6px 8px;color:#888;">' + t2('labsDiagnosticsEmpty') + '</div>');
    panel.innerHTML =
      '<div style="display:flex;justify-content:space-between;align-items:center;padding:2px 8px 6px;border-bottom:1px solid rgba(255,255,255,.08);">' +
      '<b style="font-size:11px;">' + t2('labsDiagnosticsTitle') + '</b>' +
      '<button type="button" data-diag-refresh style="cursor:pointer;background:none;border:1px solid rgba(255,255,255,.2);border-radius:4px;color:#eceae7;font:11px ui-monospace,monospace;padding:1px 6px;">' + t2('labsDiagnosticsRefresh') + '</button>' +
      '</div>' + body;
    var refreshBtn = panel.querySelector('[data-diag-refresh]');
    if (refreshBtn) refreshBtn.addEventListener('click', refresh);
    panel.querySelectorAll('tr[data-layer-idx]').forEach(function (tr) {
      var idx = parseInt(tr.getAttribute('data-layer-idx'), 10);
      if (idx < 0) return;
      tr.addEventListener('click', function () {
        if (typeof SM !== 'undefined' && typeof SM.setActiveLayer === 'function') SM.setActiveLayer(idx, true);
      });
    });
  }

  SMLabs.register('diagnostics-panel', {
    flag: 'nemo-labs-diagnostics-panel',
    describe: 'labsDescribeDiagnosticsPanel',
    onEnable: function () { build(); },
    onDisable: function () { if (panel) { panel.remove(); panel = null; } },
  });
  if (SMLabs.isOn('diagnostics-panel')) build();
})();
