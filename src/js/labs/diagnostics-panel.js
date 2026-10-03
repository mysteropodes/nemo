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
  var panel = null, generation = 0;

  // Independent of the capability's load site: the application identity hint
  // and module counter keep panel mints distinct across refresh/open cycles.
  // Full identity remains in the envelope; the bounded hint leaves room for
  // a safe integer counter and ':replay' within the 128-character contract.
  // This does not reserve IDs against arbitrary callers' retained WRITEs.
  var minted = 0;
  function mintRequestId(identity) {
    if (!Number.isSafeInteger(minted) || minted >= Number.MAX_SAFE_INTEGER) return null;
    minted += 1;
    return 'diagnostics-panel:' + identity.instanceId.slice(0, 64) + ':' + minted;
  }

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

  function currentIdentity() {
    try {
      var identity = window.NemoOpacityApplication.meta();
      return { instanceId: identity.instanceId, documentId: identity.documentId, revision: identity.revision };
    } catch (_) { return null; }
  }

  function sameDocument(a, b) {
    return !!(a && b && typeof a.instanceId === 'string' && a.instanceId
      && typeof a.documentId === 'string' && a.documentId
      && a.instanceId === b.instanceId && a.documentId === b.documentId);
  }

  function traceResult(response, identity) {
    if (!response || !response.ok) return { error: (response && response.error && response.error.message) || 'diagnostics.trace failed' };
    if (!sameDocument(identity, response)) return { error: 'Diagnostics response identity does not match the requested document.' };
    if (!response.result || !Array.isArray(response.result.entries)) return { error: 'diagnostics.trace returned no command list' };
    return { entries: response.result.entries, identity: {
      instanceId: response.instanceId, documentId: response.documentId, revision: response.revision
    } };
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

  // ---- report link (T10/#1399) -------------------------------------------
  // Source-stage only: neither the v1 trace nor T08A's metadata has recoverable
  // starting fixture bytes. T10A owns an opt-in native synthetic recording;
  // T08B owns its panel binding. Never infer provenance from user state/hash.
  async function buildReport(result) {
    return { error: result.error || 'Reproduction unavailable: no native synthetic recording with a recoverable fixture is connected.' };
  }

  function build() {
    if (!SMLabs.isOn('diagnostics-panel')) return;
    if (!panel) {
      panel = document.createElement('div');
      panel.id = 'labs-diagnostics';
      panel.style.cssText =
        'position:fixed;top:60px;right:16px;width:auto;max-width:calc(100vw - 32px);max-height:70vh;overflow:auto;z-index:190;' +
        'background:#201f25;border:1px solid rgba(255,255,255,.12);border-radius:10px;' +
        'font:11px ui-monospace,monospace;color:#eceae7;box-shadow:0 8px 30px rgba(0,0,0,.5);padding:6px 0;';
      document.body.appendChild(panel);
    }
    refresh();
  }

  function refresh() {
    if (!panel) return;
    var target = panel, token = ++generation, identity = currentIdentity();
    function finish(result) {
      if (panel !== target || generation !== token || !SMLabs.isOn('diagnostics-panel')) return;
      if (identity && !sameDocument(identity, currentIdentity())) return;
      if (!result.identity) result.identity = identity;
      render(result, target, token);
    }
    try {
      if (!identity || !window.NemoApplication) return finish({ error: 'diagnostics application not loaded' });
      var requestId = mintRequestId(identity);
      if (requestId === null) return finish({ error: 'Diagnostics request counter exhausted.' });
      var response = window.NemoApplication.handle({ apiVersion: 1, requestId: requestId,
        ...identity, expectedRevision: identity.revision, operation: 'diagnostics.trace', payload: {} });
      // Keep the synchronous compatibility path; delayed adapters share the
      // same generation/document fence without updating detached panels.
      if (response && typeof response.then === 'function') {
        Promise.resolve(response).then(function (value) { finish(traceResult(value, identity)); },
          function () { finish({ error: 'diagnostics.trace failed' }); });
      } else finish(traceResult(response, identity));
    } catch (_) { finish({ error: 'diagnostics.trace failed' }); }
  }

  function render(result, target, token) {
    var selectedLayers = correlatedLayerIndices();
    var t2 = (typeof SM !== 'undefined' && SM.t) ? SM.t : function (k) { return k; };
    var reportLabel = t2('labsDiagnosticsReport');
    if (reportLabel === 'labsDiagnosticsReport') reportLabel = 'Report';
    var body = result.error
      ? '<div style="padding:6px 8px;color:#e08787;">' + esc(result.error) + '</div>'
      : (result.entries.length
        ? '<table style="border-collapse:collapse;"><tr><th style="padding:2px 8px;color:#888;">status</th><th style="padding:2px 8px;color:#888;">operation</th><th style="padding:2px 8px;color:#888;">rev</th><th style="padding:2px 8px;color:#888;">requestId</th></tr>'
          + result.entries.slice().reverse().map(function (e) { return rowHtml(e, selectedLayers); }).join('') + '</table>'
        : '<div data-diag-empty style="padding:6px 8px;color:#888;">' + esc(t2('labsDiagnosticsEmpty')) + '</div>');
    panel.innerHTML =
      '<div style="display:flex;justify-content:space-between;align-items:center;padding:2px 8px 6px;border-bottom:1px solid rgba(255,255,255,.08);">' +
      '<b style="font-size:11px;">' + esc(t2('labsDiagnosticsTitle')) + '</b>' +
      '<span>' +
      '<button type="button" data-diag-report style="cursor:pointer;background:none;border:1px solid rgba(255,255,255,.2);border-radius:4px;color:#eceae7;font:11px ui-monospace,monospace;padding:1px 6px;margin-right:4px;">' + esc(reportLabel) + '</button>' +
      '<button type="button" data-diag-refresh style="cursor:pointer;background:none;border:1px solid rgba(255,255,255,.2);border-radius:4px;color:#eceae7;font:11px ui-monospace,monospace;padding:1px 6px;">' + esc(t2('labsDiagnosticsRefresh')) + '</button>' +
      '</span></div>' + body + '<div data-diag-report-status style="padding:2px 8px;color:#888;"></div>';
    var refreshBtn = panel.querySelector('[data-diag-refresh]');
    if (refreshBtn) refreshBtn.addEventListener('click', refresh);
    var reportBtn = panel.querySelector('[data-diag-report]');
    if (reportBtn) {
      reportBtn.addEventListener('click', function () {
        if (panel !== target || generation !== token) return;
        var status = target.querySelector('[data-diag-report-status]');
        if (result.identity && !sameDocument(result.identity, currentIdentity())) {
          if (status) status.textContent = 'Document changed; refresh diagnostics before reporting.';
          return;
        }
        // Fence completion even when disable/re-enable recreates a panel or
        // Refresh starts another read before this continuation is scheduled.
        buildReport(result).then(function (out) {
          if (!status || panel !== target || generation !== token || !SMLabs.isOn('diagnostics-panel')) return;
          if (result.identity && !sameDocument(result.identity, currentIdentity())) return;
          status.textContent = out.error;
        });
      });
    }
    panel.querySelectorAll('tr[data-layer-idx]').forEach(function (tr) {
      var idx = parseInt(tr.getAttribute('data-layer-idx'), 10);
      if (idx < 0) return;
      tr.addEventListener('click', function () {
        if (panel === target && generation === token && sameDocument(result.identity, currentIdentity())
          && typeof SM !== 'undefined' && typeof SM.setActiveLayer === 'function') SM.setActiveLayer(idx, true);
      });
    });
  }

  SMLabs.register('diagnostics-panel', {
    flag: 'nemo-labs-diagnostics-panel',
    describe: 'labsDescribeDiagnosticsPanel',
    onEnable: function () { build(); },
    onDisable: function () { generation++; if (panel) { panel.remove(); panel = null; } },
  });
  if (SMLabs.isOn('diagnostics-panel')) build();
})();
