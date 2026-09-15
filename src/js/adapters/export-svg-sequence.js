// SVG sequence export binding (P18/#1020): where the bounded export job
// (application/export-job.js) meets the live document and the Tauri
// filesystem. export.js hands over its existing helpers as ports; this file
// supplies the document fingerprint and the job's filesystem contract and
// maps the terminal job state back to the result shapes render-manager.js
// and the export dialog already consume.
//
// Fingerprint = "is this still the same document view the job started on":
// the application document identity + revision (opacity-application.js
// meta), the scene version app.js bumps on every frame save/load, the
// component/montage context and the canvas size. A change between two
// batches ends the job (document_changed / document_replaced) instead of
// mixing revisions across files — see export-job.js for the contract.
//
// P19/#1021 added the third consumer. The export dialog, the render queue and
// an MCP client now all drive THIS object: `create()` is called once (export.js
// memoises it in `_svgSequenceJob`), so there is one `running` slot, one
// document fingerprint, one cancellation flag and one `evaluateFrame`. The
// only thing that differs per caller is the sink — a directory of files for
// the two UI paths, the SVG text itself for MCP, which is why `export.svg.frame`
// can declare `effects.scope: "none"` while the dialog writes to disk. The
// consequence is the observable proof that they are not two exporters: an MCP
// `start` while the dialog is exporting is refused `busy`, and an MCP `cancel`
// of the dialog's jobId stops the dialog's export.
var NemoExportSvgSequence = (function () {
  'use strict';

  function documentFingerprint() {
    var meta = (window.NemoOpacityApplication && window.NemoOpacityApplication.meta)
      ? window.NemoOpacityApplication.meta() : {};
    return {
      documentId: meta.documentId || '',
      key: [meta.revision || 0, window._sceneVersion || 0, state.activeSymbolId || '', state.activeMontageViewId || '',
        state.canvasW, state.canvasH, state.layers.length].join('|'),
    };
  }

  function newId() { return 'xj' + Date.now().toString(36) + Math.random().toString(36).slice(2, 8); }

  // Today's result shapes, so the callers keep working unchanged.
  function result(terminal, dir) {
    if (terminal.status === 'succeeded') return { ok: true, dir: dir };
    if (terminal.status === 'cancelled') return { cancelled: true };
    return { ok: false, error: (terminal.error && terminal.error.message) || 'export failed' };
  }

  // ---- availability (P19) ---------------------------------------------------
  // ONE typed oracle for both consumers, so "why can't I export" has a single
  // answer instead of a French string on the UI side and silence on the MCP
  // side. `reason` uses capability-v1's vocabulary, the same values
  // engineering/application/capabilities/export-job.json declares.
  //
  // Both codes come from capability-v1.schema.json's own closed `reason` enum
  // (platform-unsupported | backend-disabled | missing-dependency |
  // feature-flag-off) — free text here would be exactly the drift that field
  // exists to prevent, and the schema would reject it.
  //
  //   missing-dependency    — the frame evaluator itself is absent (export.js
  //                           did not hand one over). Neither sink can produce
  //                           a frame, so BOTH paths report exactly this.
  //   platform-unsupported  — the evaluator works, but the Tauri filesystem the
  //                           directory sink needs is not there (browser
  //                           preview). Only the directory mode is gated: an
  //                           inline SVG string needs no filesystem, and
  //                           refusing it outside Tauri would be inventing a
  //                           limitation the code does not have.
  function availabilityFor(ports, mode) {
    if (!ports || typeof ports.evaluateFrame !== 'function') return { state: 'unavailable', reason: 'missing-dependency' };
    if (mode === 'directory') {
      var fs = null;
      try { fs = ports.tauri() && ports.tauri().fs; } catch (e) { fs = null; }
      if (!fs) return { state: 'unavailable', reason: 'platform-unsupported' };
    }
    return { state: 'available', reason: null };
  }

  function unavailableMessage(reason) {
    return reason === 'platform-unsupported'
      ? 'SVG sequence export needs the Nemo desktop app (no filesystem in the browser preview).'
      : 'The SVG frame evaluator is unavailable.';
  }

  // ---- capability descriptor (P19) -----------------------------------------
  // Kept value-equivalent to engineering/application/capabilities/export-job.json
  // by the focused contract test, exactly as opacity-capability.js is to
  // opacity.json: the JSON file is canonical, this runtime copy exists because
  // application boot cannot fetch a descriptor asynchronously before native MCP
  // discovery. Do not edit one without the other.
  var DESCRIPTOR = {
    schemaVersion: 1,
    id: 'export.svg.frame',
    version: 1,
    input: {
      type: 'object',
      additionalProperties: false,
      description: 'What a caller supplies to invoke this capability. "frameIdx" addresses the "start" stage; "jobId" (returned by "start"\'s output) addresses "status" and "cancel". Exactly one is required depending on stage, enforced by the handler per stage rather than by this shape — full per-stage input contracts are D02/#1045.',
      properties: {
        frameIdx: { type: 'integer', minimum: 0, description: '0-based timeline frame to export. Required for the "start" stage.' },
        jobId: { type: 'string', minLength: 1, description: 'Job identifier returned by "start". Required for the "status" and "cancel" stages.' },
      },
    },
    output: {
      type: 'object',
      additionalProperties: false,
      properties: {
        jobId: { type: 'string', minLength: 1 },
        status: { type: 'string', enum: ['running', 'succeeded', 'failed', 'cancelled'] },
        progress: { type: 'number', minimum: 0, maximum: 1 },
        artifact: {
          type: 'object',
          additionalProperties: false,
          properties: {
            mimeType: { const: 'image/svg+xml' },
            data: { type: 'string', description: 'Inline SVG document text.' },
          },
          required: ['mimeType', 'data'],
        },
        error: {
          type: 'object',
          additionalProperties: false,
          properties: { code: { type: 'string' }, message: { type: 'string' } },
          required: ['code', 'message'],
        },
      },
      required: ['jobId', 'status'],
    },
    units: { progress: 'ratio' },
    effects: { kind: 'job', scope: 'none', lifecycle: ['start', 'status', 'cancel'] },
    availability: { state: 'available', reason: null },
    handlerKey: 'application.export.svgFrame',
    fixture: {
      label: 'export frame 10 as SVG, complete',
      input: { frameIdx: 10 },
      output: { jobId: 'job-1', status: 'succeeded', progress: 1, artifact: { mimeType: 'image/svg+xml', data: '<svg></svg>' } },
    },
    examples: [
      { label: 'start', input: { frameIdx: 10 }, output: { jobId: 'job-1', status: 'running', progress: 0 } },
      { label: 'status while rendering', input: { jobId: 'job-1' }, output: { jobId: 'job-1', status: 'running', progress: 0.5 } },
      {
        label: 'status once complete',
        input: { jobId: 'job-1' },
        output: { jobId: 'job-1', status: 'succeeded', progress: 1, artifact: { mimeType: 'image/svg+xml', data: '<svg></svg>' } },
      },
      // Cancellation is a REQUEST: export-job.js applies it at the next batch
      // boundary, and freeing the single-tenant `running` slot synchronously
      // would let a second export start while this one is still mid-write. So
      // `cancel` answers with the state as it stands and the terminal state
      // arrives on the following `status` — which is what these two examples
      // say. The first example claimed `cancelled` outright; that was written
      // before there was an implementation to check it against (P19/#1021).
      { label: 'cancel requested — cancellation takes effect at the next batch boundary, so this response still reports the current state', input: { jobId: 'job-1' }, output: { jobId: 'job-1', status: 'running', progress: 0.5 } },
      { label: 'status after a cancel, once it has taken effect', input: { jobId: 'job-1' }, output: { jobId: 'job-1', status: 'cancelled', progress: 0.5 } },
    ],
  };

  // ports: evaluateFrame(f) -> svg text (export.js exportFrameSVGString),
  //        frameName(i), tauri() -> window.__TAURI__, mkdir(dir), removeDir(dir),
  //        writeText(path, text), saveAllLayerFrames().
  function create(ports) {
    var job = NemoExportJob.create({
      newId: newId,
      fingerprint: documentFingerprint,
      beforeCapture: function () { ports.saveAllLayerFrames(); },
      evaluateFrame: ports.evaluateFrame,
      frameName: ports.frameName,
      // Created-or-not is what decides whether cleanup may remove the directory.
      mkdir: async function (dir) {
        var existed = false;
        try { existed = !!(await ports.tauri().fs.exists(dir)); } catch (e) { /* treat as absent */ }
        await ports.mkdir(dir);
        return { created: !existed };
      },
      write: ports.writeText,
      // Relative names the job wrote, its directory, and whether it created
      // that directory: pre-existing files and directories are never touched.
      remove: async function (names, dir, createdDir) {
        var fs = ports.tauri().fs;
        for (var i = 0; i < names.length; i++) { try { await fs.remove(dir + '/' + names[i]); } catch (e) { /* best effort */ } }
        if (createdDir) await ports.removeDir(dir);
      },
    });

    function availability(mode) { return availabilityFor(ports, mode); }

    // The inline sink: the same frames, kept as text instead of written to
    // disk. One buffer per job so a cancelled or failed job's partial text is
    // discarded by the SAME cleanup path that deletes partial files — an MCP
    // caller can no more read a half-finished sequence than a UI caller can
    // keep half a directory.
    // Retained per jobId for as long as the job itself is (export-job.js keeps
    // its job map for the session's lifetime), so a `status` call after the
    // terminal state still answers with the artifact. Known limit, same shape
    // as the job map's: neither is evicted.
    var inline = {};
    function inlineArtifact(jobId) {
      var texts = inline[jobId];
      if (!texts || texts.length !== 1) return null;
      return { mimeType: 'image/svg+xml', data: texts[0] };
    }

    // Start a single frame into memory, on the SAME session as the dialog and
    // the render queue: `busy`, cancellation and the document fingerprint are
    // shared, not duplicated.
    function beginFrame(input) {
      var gate = availability('inline');
      if (gate.state !== 'available') return { ok: false, error: { code: gate.reason, message: unavailableMessage(gate.reason) } };
      // The buffer is captured by the sink and only keyed once `begin` has
      // told us the job's real id — the job mints ids, this adapter does not.
      // No write can land in between: the job's first write is behind its
      // `await mkdir`, and `begin` returns synchronously.
      var buffer = [];
      var begun = job.begin({
        start: input.frameIdx, end: input.frameIdx, requestId: input.requestId,
        sink: {
          mkdir: function () { return { created: false }; },
          write: function (name, text) { buffer.push(text); },
          remove: function () { buffer.length = 0; },
        },
      });
      // A retry returns the ORIGINAL job; its buffer must not be replaced.
      if (begun.ok && !inline[begun.jobId]) inline[begun.jobId] = buffer;
      return begun;
    }

    // One call for the existing callers: begin, wait for the terminal state,
    // map it. Cancellation and status stay reachable through `job` (P19).
    async function run(input) {
      var gate = availability('directory');
      if (gate.state !== 'available') return { ok: false, error: unavailableMessage(gate.reason), reason: gate.reason };
      var begun = job.begin(input);
      if (!begun.ok) return { ok: false, error: begun.error.message, reason: begun.error.code };
      var terminal = await job.done(begun.jobId);
      return result(terminal, input.dir);
    }

    return { begin: job.begin, status: job.status, cancel: job.cancel, done: job.done, meta: job.meta,
      run: run, availability: availability, beginFrame: beginFrame, inlineArtifact: inlineArtifact };
  }

  // ---- MCP binding (P19) ----------------------------------------------------
  // `capabilities/export-job.json` start/status/cancel, served by the session
  // above. This is deliberately NOT a copy of the opacity core's validator
  // (CLAUDE.md §3 — two near-identical validators drift): every stage here is
  // `effects.scope: "none"`, so there is no expectedRevision / stale-revision /
  // retry-retention path to mirror, only the envelope invariants and the
  // per-stage input the descriptor describes.
  //
  // ports.session() — the live session (export.js `SMExport.svgSequenceJob()`),
  //                   resolved per call so no stale reference survives.
  // ports.meta()    — the application identity, so an export response carries
  //                   the same instanceId/documentId/revision as an opacity one
  //                   instead of inventing a second identity.
  function capability(ports) {
    function respond(request, ok, value) {
      var id = ports.meta();
      var out = { apiVersion: 1, requestId: (request && request.requestId) || '',
        instanceId: id.instanceId, documentId: id.documentId, revision: id.revision, ok: ok };
      out[ok ? 'result' : 'error'] = value;
      return out;
    }
    function fail(request, code, message) { return respond(request, false, { code: code, message: message }); }

    // The declared output shape is `additionalProperties: false`: the job's own
    // view carries evaluated/written/total/partial/cleanup, which are real but
    // are NOT in this contract. Project down rather than leak them.
    function project(session, view) {
      var out = { jobId: view.jobId, status: view.status, progress: view.progress };
      if (view.status === 'succeeded') {
        var artifact = session.inlineArtifact(view.jobId);
        if (artifact) out.artifact = artifact;
      }
      if (view.error) out.error = { code: view.error.code, message: view.error.message };
      return out;
    }

    function handle(request) {
      if (!request || typeof request !== 'object' || Array.isArray(request)) return fail(request, 'invalid_request', 'Invalid application request.');
      if (request.apiVersion !== 1 || typeof request.requestId !== 'string' || !request.requestId
          || request.requestId.length > 128 || !request.payload || typeof request.payload !== 'object'
          || Array.isArray(request.payload) || DESCRIPTOR.effects.lifecycle.indexOf(request.operation) < 0) {
        return fail(request, 'invalid_request', 'Invalid application request.');
      }
      if (request.cancelled === true) return fail(request, 'cancelled', 'Cancelled before dispatch.');
      var id = ports.meta();
      if ((request.instanceId != null && request.instanceId !== id.instanceId)
          || (request.documentId != null && request.documentId !== id.documentId)) {
        return fail(request, 'wrong_document', 'Request targets a different document or instance.');
      }
      var session = ports.session(), payload = request.payload;
      if (!session) return fail(request, 'missing-dependency', 'The export session is unavailable.');
      if (request.operation === 'start') {
        if (!Number.isInteger(payload.frameIdx) || payload.frameIdx < 0) return fail(request, 'invalid_request', 'start requires an integer frameIdx >= 0.');
        var begun = session.beginFrame({ frameIdx: payload.frameIdx, requestId: request.requestId });
        // busy / missing-dependency / desktop-only all land here: a typed code,
        // no jobId, and nothing that could be read as a partial artifact.
        if (!begun.ok) return fail(request, begun.error.code, begun.error.message);
        return respond(request, true, project(session, begun.job));
      }
      if (typeof payload.jobId !== 'string' || !payload.jobId) return fail(request, 'invalid_request', request.operation + ' requires a jobId.');
      var answer = request.operation === 'status' ? session.status(payload.jobId) : session.cancel(payload.jobId);
      if (!answer.ok) return fail(request, answer.error.code, answer.error.message);
      return respond(request, true, project(session, answer.job));
    }

    return { descriptor: DESCRIPTOR, handler: handle, operations: DESCRIPTOR.effects.lifecycle.slice() };
  }

  return { create: create, documentFingerprint: documentFingerprint, result: result,
    availabilityFor: availabilityFor, unavailableMessage: unavailableMessage,
    DESCRIPTOR: DESCRIPTOR, capability: capability };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoExportSvgSequence;
