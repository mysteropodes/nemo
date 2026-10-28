/* #2510: project lifecycle over the application transport (the MCP v1 project.* operations). Each operation runs the
 * same app flow the menus use: the ordinary Open path and, when a project is already open, the inactive-tab prepare,
 * switch and close steps; New Project; the native Save As workflow; and the Render Manager's path exporters. Only the
 * file picker is replaced by the request's path. A dirty project is never replaced: the app would ask in a modal
 * dialog, so the request refuses with unsaved_changes instead. One operation runs at a time, and a retried requestId
 * with the same body returns the first answer instead of repeating the work. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoProjectLifecycle = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var OPERATIONS = ['project.open', 'project.new', 'project.saveAs', 'project.export'];
  var FORMATS = ['png', 'svg', 'lottie'];

  function plain(value) { return !!value && typeof value === 'object' && !Array.isArray(value); }
  function absolute(value) { return typeof value === 'string' && value.charAt(0) === '/' && value.length <= 4096; }
  function integer(value, min, max) { return Number.isSafeInteger(value) && value >= min && value <= max; }
  function exact(payload, required, optional) {
    var keys = Object.keys(payload);
    return required.every(function (key) { return keys.indexOf(key) >= 0; }) &&
      keys.every(function (key) { return required.indexOf(key) >= 0 || optional.indexOf(key) >= 0; });
  }
  function payloadValid(operation, payload) {
    if (!plain(payload)) return false;
    if (operation === 'project.open' || operation === 'project.saveAs') return exact(payload, ['path'], []) && absolute(payload.path);
    if (operation === 'project.new') return exact(payload, ['canvasW', 'canvasH', 'fps'], []) &&
      integer(payload.canvasW, 1, 16384) && integer(payload.canvasH, 1, 16384) && integer(payload.fps, 1, 240);
    return exact(payload, ['format', 'destination'], ['start', 'end']) && FORMATS.indexOf(payload.format) >= 0 &&
      absolute(payload.destination) && (payload.start === undefined || integer(payload.start, 0, Number.MAX_SAFE_INTEGER)) &&
      (payload.end === undefined || integer(payload.end, 0, Number.MAX_SAFE_INTEGER)) &&
      (payload.start === undefined || payload.end === undefined || payload.start <= payload.end);
  }

  function create(ports) {
    var busy = false, retained = new Map();
    function project() { var p = ports.objectProject(); return p && p.isActive() ? p : null; }
    function current() { return project() ? ports.info() : null; }
    function response(request, ok, value) {
      var info = current();
      var out = { apiVersion: 1, requestId: request.requestId, instanceId: request.instanceId || (info && info.instanceId) || '',
        documentId: info ? info.documentId : '', revision: info ? info.contentRevision : 0, ok: ok };
      out[ok ? 'result' : 'error'] = value;
      return out;
    }
    function fail(request, code, message) { return response(request, false, { code: code, message: message }); }
    // What a client needs to continue: the open document and its layer state.
    function summary() {
      var info = current();
      if (!info) return null;
      return { documentId: info.documentId, contentRevision: info.contentRevision, canvasW: info.canvasW,
        canvasH: info.canvasH, fps: info.fps, totalFrames: info.totalFrames,
        layers: (info.layers || []).map(function (layer) {
          return { layerUid: layer.layerUid, name: layer.name, visible: layer.visible, locked: layer.locked, shy: layer.shy };
        }) };
    }
    function replaceable(request) {
      var p = project();
      if (p && p.isDirty && p.isDirty()) return fail(request, 'unsaved_changes', 'The open project has unsaved changes; save it first.');
      var tabs = ports.tabs();
      if (tabs && tabs.held && tabs.held()) return fail(request, 'unavailable', 'An inactive project is retained; close it first.');
      return null;
    }
    async function open(request) {
      var refused = replaceable(request); if (refused) return refused;
      var path = request.payload.path;
      if (!project()) {
        await ports.openPath(path);
        return project() ? response(request, true, summary()) : fail(request, 'unavailable', 'The project could not be opened.');
      }
      var tabs = ports.tabs();
      var prepared = await tabs.openPath(path);
      if (!prepared || prepared.status !== 'prepared') return fail(request, 'unavailable', 'The project was not prepared (' + (prepared && prepared.status) + ').');
      var activated = await tabs.activateClick(tabs.held());
      if (!activated || activated.status !== 'activated') return fail(request, 'unavailable', 'The prepared project was not activated (' + (activated && activated.status) + ').');
      // The previous project is now the retained inactive tab; it was clean, so closing it discards nothing.
      var closed = await tabs.close(tabs.held());
      if (!closed || closed.status !== 'closed') return fail(request, 'indeterminate', 'The project opened, but the previous project was not closed (' + (closed && closed.status) + ').');
      return response(request, true, summary());
    }
    async function createProject(request) {
      var refused = replaceable(request); if (refused) return refused;
      var payload = request.payload, failed = null;
      var created = await ports.newProject({ w: payload.canvasW, h: payload.canvasH, fps: payload.fps, name: 'Untitled' },
        function () { return true; }, function (error) { failed = error; });
      var info = current();
      if (failed || created === false || !info || info.canvasW !== payload.canvasW || info.canvasH !== payload.canvasH)
        return fail(request, 'unavailable', 'The new project was not created' + (failed && failed.message ? ': ' + failed.message : '.'));
      return response(request, true, summary());
    }
    async function saveAs(request) {
      var p = project(); if (!p) return fail(request, 'unavailable', 'No native project is open.');
      var saved = await p.saveAs(request.payload.path);
      if (!saved || saved.acknowledged !== true) return fail(request, 'unavailable', 'The project was not saved (' + (saved && saved.status) + ').');
      return response(request, true, Object.assign({ path: request.payload.path }, summary()));
    }
    async function exportFrames(request) {
      var p = project(); if (!p) return fail(request, 'unavailable', 'No native project is open.');
      var payload = request.payload, info = ports.info();
      var range = { start: payload.start === undefined ? 0 : payload.start,
        end: payload.end === undefined ? info.totalFrames - 1 : payload.end };
      if (range.end >= info.totalFrames) return fail(request, 'invalid_request', 'The frame range is outside the timeline.');
      var exported = await ports.exporters[payload.format](payload.destination, range);
      if (!exported || exported.ok !== true) return fail(request, exported && exported.unavailable ? 'unavailable' : 'export_failed',
        (exported && exported.error) || 'The export did not complete.');
      return response(request, true, { format: payload.format, start: range.start, end: range.end,
        output: exported.dir || exported.path || payload.destination });
    }
    var RUN = { 'project.open': open, 'project.new': createProject, 'project.saveAs': saveAs, 'project.export': exportFrames };

    async function handle(request) {
      if (!plain(request) || typeof request.requestId !== 'string' || !request.requestId || OPERATIONS.indexOf(request.operation) < 0)
        return fail(plain(request) ? request : { requestId: '' }, 'invalid_request', 'Unsupported project operation.');
      var body = JSON.stringify({ operation: request.operation, payload: request.payload });
      var prior = retained.get(request.requestId);
      if (prior) return prior.body === body ? prior.result : fail(request, 'invalid_request', 'requestId was reused with a changed body.');
      if (!payloadValid(request.operation, request.payload)) return fail(request, 'invalid_request', 'Operation payload is invalid.');
      if (!ports.available()) return fail(request, 'unavailable', 'Project operations need the desktop app.');
      if (busy) return fail(request, 'unavailable', 'A project operation is already in progress.');
      busy = true;
      var result = (async function () {
        try { return await RUN[request.operation](request); }
        catch (error) { return fail(request, 'unavailable', (error && error.message) || 'The project operation failed.'); }
        finally { busy = false; }
      }());
      retained.set(request.requestId, { body: body, result: result });
      if (retained.size > 64) retained.delete(retained.keys().next().value);
      return result;
    }
    return Object.freeze({ handle: handle, operations: OPERATIONS.slice() });
  }
  return Object.freeze({ create: create, OPERATIONS: OPERATIONS.slice(), payloadValid: payloadValid });
}));
