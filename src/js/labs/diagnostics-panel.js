// ---- LABS — Opacity diagnostics inspector (T08/#1057) ----
// A thin, read-only floating panel over the native shared-v2 trace (T08B).
// The async adapter reaches the same native authority as the bundled MCP.
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

  // Native validation rejects hostile identifiers before rendering. Keep every
  // interpolated trace field escaped as a separate presentation boundary: the
  // Tauri webview must never interpret caller-supplied text as markup.
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
      return window.NemoNativeDiagnosticsQuery.capture(window);
    } catch (_) { return null; }
  }

  function sameDocument(a, b) {
    return !!(window.NemoNativeDiagnosticsQuery && window.NemoNativeDiagnosticsQuery.same(a, b));
  }

  function rowHtml(entry, selectedLayers) {
    var layerId = entry.targetId;
    var layerIdx = layerId ? layerIndexOf(layerId) : -1;
    var correlated = layerIdx >= 0 && selectedLayers[layerIdx];
    var statusColor = entry.ok ? '#7bd88f' : '#e08787';
    return '<tr data-layer-idx="' + layerIdx + '" style="cursor:' + (layerIdx >= 0 ? 'pointer' : 'default') + ';' +
      (correlated ? 'background:rgba(78,111,242,.25);' : '') + '">' +
      '<td style="padding:1px 8px;color:' + statusColor + ';">' + (entry.ok ? 'ok' : 'fail') + '</td>' +
      '<td style="padding:1px 8px;">' + esc(entry.operation) + '</td>' +
      '<td style="padding:1px 8px;color:#888;">' + esc(entry.contentRevision) + '</td>' +
      '<td style="padding:1px 8px;color:#888;max-width:120px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">' + esc(entry.requestIdRedacted ? '[redacted]' : entry.requestId) + '</td>' +
      '</tr>';
  }

  // ---- report link (T10/#1399) -------------------------------------------
  // Native catalog provenance is granted only by an explicit host admission.
  // The displayed revision/attempt sequence are checked atomically by Rust.
  async function buildReport(result) {
    if (result.error || !result.identity || !result.identity.catalog || !result.reportToken) {
      throw new Error('Reproduction unavailable: no native synthetic recording with a recoverable fixture is connected.');
    }
    var requestId = mintRequestId(result.identity);
    if (requestId === null) throw new Error('Diagnostics request counter exhausted.');
    return window.NemoNativeDiagnosticsQuery.report(window, result.identity, result.reportToken, requestId);
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
    if (window.NemoNativeReproduction) window.NemoNativeReproduction.disposeDownloads(window);
    var target = panel, token = ++generation, identity = currentIdentity();
    function finish(result) {
      if (panel !== target || generation !== token || !SMLabs.isOn('diagnostics-panel')) return;
      if (identity && !sameDocument(identity, currentIdentity())) return;
      if (!result.identity) result.identity = identity;
      render(result, target, token);
    }
    // Controls exist immediately, including when persisted Labs opens before
    // host scripts load. A later manual Refresh discovers the ready service.
    render({ error: 'Loading native diagnostics…', identity: identity }, target, token);
    try {
      var service = window.NemoNativeDiagnosticsQuery;
      if (!identity || !service || !service.available(window)) return finish({ error: 'Native diagnostics unavailable in browser/WASM or without an active native document.' });
      var requestId = mintRequestId(identity);
      if (requestId === null) return finish({ error: 'Diagnostics request counter exhausted.' });
      service.recent(window, identity, requestId).then(function (value) {
        var records = value.result.records;
        finish({ entries: records, truncated: value.result.truncated, identity: identity,
          reportToken: Object.freeze({ expectedContentRevision: value.contentRevision,
            expectedSequence: records.length ? records[records.length - 1].sequence : 0 }) });
      }, function () { finish({ error: 'Native diagnostics query failed. Refresh to retry.' }); });
    } catch (_) { finish({ error: 'Native diagnostics query failed. Refresh to retry.' }); }
  }

  function render(result, target, token) {
    var selectedLayers = correlatedLayerIndices();
    var t2 = (typeof SM !== 'undefined' && SM.t) ? SM.t : function (k) { return k; };
    function text(key, fallback) { var value = t2(key); return value === key ? fallback : value; }
    var reportLabel = t2('labsDiagnosticsReport');
    if (reportLabel === 'labsDiagnosticsReport') reportLabel = 'Report';
    var body = result.error
      ? '<div style="padding:6px 8px;color:#e08787;">' + esc(result.error) + '</div>'
      : (result.entries.length
        ? '<table style="border-collapse:collapse;"><tr><th style="padding:2px 8px;color:#888;">status</th><th style="padding:2px 8px;color:#888;">operation</th><th style="padding:2px 8px;color:#888;">rev</th><th style="padding:2px 8px;color:#888;">requestId</th></tr>'
          + result.entries.slice().reverse().map(function (e) { return rowHtml(e, selectedLayers); }).join('') + '</table>'
        : '<div data-diag-empty style="padding:6px 8px;color:#888;">' + esc(t2('labsDiagnosticsEmpty')) + '</div>');
    if (result.truncated) body += '<div data-diag-truncated style="padding:6px 8px;color:#888;">Earlier operations omitted by the native trace limit.</div>';
    var catalog = window.NemoNativeReproduction && window.NemoNativeReproduction.capture(window);
    var sessionHtml = catalog
      ? '<div style="padding:6px 8px;">' + esc(text('labsDiagnosticsSyntheticSession', 'Synthetic catalog session · opacity (0–100)')) +
        ' <input data-diag-opacity type="number" min="0" max="100" step="1" value="40" aria-label="' + esc(text('labsDiagnosticsSyntheticOpacity', 'Synthetic opacity')) +
        '" style="width:55px;"> <button type="button" data-diag-apply>' + esc(text('labsDiagnosticsSyntheticApply', 'Apply')) + '</button> <button type="button" data-diag-end>' + esc(text('labsDiagnosticsSyntheticEnd', 'End session')) + '</button></div>'
      : '<div style="padding:6px 8px;color:#aaa;">' + esc(text('labsDiagnosticsSyntheticHelp', 'Start an opt-in disposable synthetic session after closing the active native document.')) +
        ' <button type="button" data-diag-start>' + esc(text('labsDiagnosticsStartSynthetic', 'Start synthetic session')) + '</button></div>';
    panel.innerHTML =
      '<div style="display:flex;justify-content:space-between;align-items:center;padding:2px 8px 6px;border-bottom:1px solid rgba(255,255,255,.08);">' +
      '<b style="font-size:11px;">' + esc(t2('labsDiagnosticsTitle')) + '</b>' +
      '<span>' +
      '<button type="button" data-diag-report style="cursor:pointer;background:none;border:1px solid rgba(255,255,255,.2);border-radius:4px;color:#eceae7;font:11px ui-monospace,monospace;padding:1px 6px;margin-right:4px;">' + esc(reportLabel) + '</button>' +
      '<button type="button" data-diag-refresh style="cursor:pointer;background:none;border:1px solid rgba(255,255,255,.2);border-radius:4px;color:#eceae7;font:11px ui-monospace,monospace;padding:1px 6px;">' + esc(t2('labsDiagnosticsRefresh')) + '</button>' +
      '</span></div>' + sessionHtml + body + '<div data-diag-report-status style="padding:2px 8px;color:#888;"></div>';
    function current() { return panel === target && generation === token && SMLabs.isOn('diagnostics-panel'); }
    var sessionPending = false;
    function sessionAction(action) {
      if (!current() || sessionPending) return;
      var status = target.querySelector('[data-diag-report-status]');
      sessionPending = true;
      Promise.resolve().then(function () { if (!current()) throw new Error('Panel changed.'); return action(); }).then(function () { if (current()) refresh(); }, function () {
        if (current() && status) status.textContent = text('labsDiagnosticsSyntheticUnavailable', 'Native synthetic session unavailable or changed. Refresh to retry.');
      }).finally(function () { sessionPending = false; });
    }
    var start = panel.querySelector('[data-diag-start]');
    if (start) start.addEventListener('click', function () { sessionAction(function () { return window.NemoNativeReproduction.start(window, true); }); });
    var apply = panel.querySelector('[data-diag-apply]');
    if (apply) apply.addEventListener('click', function () {
      var input = target.querySelector('[data-diag-opacity]'), value = input && input.value.trim();
      sessionAction(function () {
        var id = catalog && mintRequestId(catalog);
        if (!id || !value) throw new Error('Invalid synthetic opacity.');
        return window.NemoNativeReproduction.setOpacity(window, catalog, Number(value), id);
      });
    });
    var end = panel.querySelector('[data-diag-end]');
    if (end) end.addEventListener('click', function () { sessionAction(function () {
      var id = catalog && mintRequestId(catalog);
      if (!id) throw new Error('Invalid session identity.');
      return window.NemoNativeReproduction.end(window, catalog, id);
    }); });
    var refreshBtn = panel.querySelector('[data-diag-refresh]');
    if (refreshBtn) refreshBtn.addEventListener('click', refresh);
    var reportBtn = panel.querySelector('[data-diag-report]');
    var reportPending = false;
    if (reportBtn) {
      reportBtn.addEventListener('click', function () {
        if (!current() || reportPending) return;
        var status = target.querySelector('[data-diag-report-status]');
        if (result.identity && !sameDocument(result.identity, currentIdentity())) {
          if (status) status.textContent = 'Document changed; refresh diagnostics before reporting.';
          return;
        }
        // Fence completion even when disable/re-enable recreates a panel or
        // Refresh starts another read before this continuation is scheduled.
        reportPending = true;
        function valid() { return current() && sameDocument(result.identity, currentIdentity()); }
        buildReport(result).then(function (bundle) {
          if (!status || !valid()) return;
          window.NemoNativeReproduction.download(window, bundle, valid);
          status.textContent = text('labsDiagnosticsReportDownloaded', 'Native reproduction downloaded.');
        }).catch(function () {
          if (!status || !current() || (result.identity && !sameDocument(result.identity, currentIdentity()))) return;
          status.textContent = result.identity && result.identity.catalog
            ? text('labsDiagnosticsReportStale', 'Native report unavailable or stale. Refresh diagnostics to retry.')
            : text('labsDiagnosticsReproductionUnavailable', 'Reproduction unavailable: no native synthetic recording with a recoverable fixture is connected.');
        }).finally(function () { reportPending = false; });
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
    onDisable: function () { generation++; if (window.NemoNativeReproduction) window.NemoNativeReproduction.disposeDownloads(window); if (panel) { panel.remove(); panel = null; } },
  });
  if (SMLabs.isOn('diagnostics-panel')) build();
})();
