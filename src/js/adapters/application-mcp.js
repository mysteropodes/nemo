// Native transport adapter. All commands use the same application service as UI.
(function () {
  'use strict';
  var tauri = window.__TAURI__;
  if (!tauri || !tauri.core || !tauri.event) return;
  var pending = new Map();
  var unlisten = [];

  async function start() {
    var service = window.NemoApplication;
    if (!service || typeof service.handle !== 'function') throw new Error('Application service unavailable');
    var identity = await tauri.core.invoke('nemo_mcp_identity');
    var initialized = service.setInstanceId(identity.instanceId);
    if (initialized && initialized.ok === false) throw new Error(initialized.error.message);
    unlisten.push(await tauri.event.listen('nemo-application-cancel', function (event) {
      var timer = pending.get(event.payload.connectionId);
      if (timer !== undefined) { clearTimeout(timer); pending.delete(event.payload.connectionId); }
    }));
    unlisten.push(await tauri.event.listen('nemo-application-request', function (event) {
      var payload = event.payload;
      // Yield once so cancellation already queued by the native transport wins
      // before a synchronous document mutation begins. Committed work is not rolled back.
      var timer = setTimeout(async function () {
        pending.delete(payload.connectionId);
        var response;
        try { response = await service.handle(payload.request); }
        catch (error) {
          // A transport fault must not invent a document identity or successful write.
          console.error('Nemo application transport failed:', error.message);
          return;
        }
        try { await tauri.core.invoke('nemo_mcp_reply', { connectionId: payload.connectionId, response: response }); }
        catch (error) { console.error('Nemo application reply failed:', error.message); }
      }, 0);
      pending.set(payload.connectionId, timer);
    }));
    await tauri.core.invoke('nemo_mcp_ready');
  }

  window.addEventListener('beforeunload', function () {
    pending.forEach(clearTimeout);
    pending.clear();
    unlisten.forEach(function (stop) { stop(); });
  });
  start().catch(function (error) { console.error('Nemo MCP unavailable:', error.message); });
})();
