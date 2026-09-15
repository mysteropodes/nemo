// @ts-check
// Bounded export job (P18/#1020): start / status / cancel around a frame
// sequence exporter, per the H01/#1058 evaluation-session contract.
//
// What this module owns: job identity, the monotonic terminal lifecycle
// (running → succeeded | failed | cancelled, never back), bounded progress,
// retry-by-requestId, cancellation, and cleanup of partial output. What it
// does NOT own: rendering. `ports.evaluateFrame(f)` is the EXISTING per-frame
// evaluator (export.js exportFrameSVGString → exportBuildFrame); it is called
// as-is, never reimplemented here.
//
// The mixed-revision guarantee, with today's live-reading evaluator (H01 §1
// found every input read live; §2.3 is the fallback rule this implements):
//
//   * `begin()` captures the document fingerprint and evaluates the FIRST
//     batch synchronously, before returning and before any await. Nothing
//     can run between two frames of a batch, so a batch is always a
//     single-revision read by construction.
//   * Between batches (the only places the event loop runs: the awaited
//     writes) the fingerprint is re-read. A different documentId means the
//     document was replaced under the job (D02 StaleDocument): the job ends
//     `cancelled` with code `document_replaced`. The same document with a
//     different fingerprint means an edit, scrub or resize landed between
//     batches: the job ends `failed` with code `document_changed` rather
//     than silently mixing revisions across files. A caller that wants the
//     whole sequence in one batch passes `batchFrames: Infinity`.
//   * The same check runs once more after the last write, so a late result
//     cannot commit against a replaced document.
//
// Cleanup: the job records every file name it wrote and whether it created
// the directory; on `failed`/`cancelled` it asks `ports.remove(names, dir,
// createdDir)` to delete exactly those (never pre-existing files, and the
// directory only if the job created it) unless `keepPartial` was requested.
// A cancelled or failed job never reports its partial output as an artifact.
//
// Sinks (P19/#1021): WHERE the frames land is a per-job choice, not a second
// job. `begin({sink})` overrides any of `mkdir`/`write`/`remove`/`frameName`
// for that job only; anything it omits falls back to the instance port. The
// export dialog and the render queue write a directory of files through the
// Tauri ports; an MCP client collects the same SVG text in memory. What they
// must NOT have is a second lifecycle — `running` (the single-tenant Paper
// scratch layer, H01 census #17), cancellation, the document fingerprint and
// `evaluateFrame` stay shared, so an MCP `start` during a dialog export is
// refused `busy` and an MCP `cancel` of the dialog's jobId really stops it.
// `dir` is required only for a job that has no sink of its own.
//
// No globals are read here: identity, fingerprint, evaluation and the
// filesystem all come in through `ports`, so the same code runs headlessly.
var NemoExportJob = (function () {
  'use strict';

  var DEFAULT_BATCH_FRAMES = 24;          // ~1 s of UI-thread work between yields
  var DEFAULT_BATCH_BYTES = 64 * 1024 * 1024; // of SVG text held before flushing

  function failure(code, message) { return { ok: false, error: { code: code, message: message } }; }

  function create(ports) {
    var jobs = {};       // jobId -> job
    var byRequest = {};  // requestId -> jobId (retry returns the same job)
    var running = null;  // the single in-flight job (the Paper scratch layer is single-tenant)

    function view(job) {
      var out = { jobId: job.jobId, status: job.status, progress: job.progress,
        evaluated: job.evaluated, written: job.written.length, total: job.total };
      if (job.requestId) out.requestId = job.requestId;
      if (job.status === 'succeeded') out.artifact = { dir: job.dir, files: job.written.slice() };
      if (job.status === 'failed') out.error = job.error;
      if (job.status === 'cancelled' && job.error) out.error = job.error;
      if (job.status === 'failed' || job.status === 'cancelled') out.partial = { written: job.written.slice(), kept: !!job.keepPartial };
      if (job.cleanup) out.cleanup = job.cleanup;
      return out;
    }

    function fingerprint() {
      var fp = ports.fingerprint();
      return { documentId: String(fp.documentId), key: String(fp.key) };
    }

    // Terminal transition: first one wins, later ones are ignored.
    function finish(job, status, error) {
      if (job.status !== 'running') return;
      job.status = status;
      if (error) job.error = error;
      if (running === job) running = null;
    }

    function settle(job) {
      job.resolve(view(job));
    }

    // A job's sink is the instance ports with the per-job overrides applied.
    // Resolved once at begin so a later mutation of the input object cannot
    // change where a running job writes.
    function resolveSink(sink) {
      sink = sink || {};
      return {
        mkdir: typeof sink.mkdir === 'function' ? sink.mkdir : ports.mkdir,
        write: typeof sink.write === 'function' ? sink.write : ports.write,
        remove: typeof sink.remove === 'function' ? sink.remove : ports.remove,
        frameName: typeof sink.frameName === 'function' ? sink.frameName : ports.frameName,
      };
    }

    function cleanupThen(job) {
      if (job.keepPartial || !job.written.length && !job.createdDir) { settle(job); return; }
      var paths = job.written.slice();
      // A synchronously throwing port becomes a rejection here, never an
      // unhandled throw out of the caller's catch block.
      new Promise(function (resolve) { resolve(job.io.remove(paths, job.dir, job.createdDir)); }).then(function () {
        job.cleanup = { removed: paths.length, dir: job.createdDir };
        settle(job);
      }, function (e) {
        job.cleanup = { removed: 0, dir: job.createdDir, error: e && e.message ? e.message : String(e) };
        settle(job);
      });
    }

    function checkDocument(job, phase) {
      var fp = fingerprint();
      if (fp.documentId !== job.identity.documentId) {
        finish(job, 'cancelled', { code: 'document_replaced', message: 'The document was replaced during the export (' + phase + ').' });
        return false;
      }
      if (fp.key !== job.identity.key) {
        finish(job, 'failed', { code: 'document_changed', message: 'The document changed during the export (' + phase + '); partial output is not a valid sequence.' });
        return false;
      }
      return true;
    }

    // Evaluate the next batch synchronously. Returns the batch (may be empty).
    function evaluateBatch(job) {
      var batch = [], bytes = 0;
      while (job.next <= job.end && batch.length < job.batchFrames && bytes < job.batchBytes) {
        if (job.cancelRequested) break;
        var text = ports.evaluateFrame(job.next);
        batch.push({ frame: job.next, ordinal: job.evaluated + 1, text: text });
        bytes += text.length;
        job.next++; job.evaluated++;
      }
      return batch;
    }

    async function run(job, firstBatch) {
      var batch = firstBatch;
      try {
        job.phase = 'mkdir';
        var made = await Promise.resolve(job.io.mkdir(job.dir));
        job.createdDir = !!(made && made.created);
        for (;;) {
          job.phase = 'write';
          for (var i = 0; i < batch.length; i++) {
            if (job.cancelRequested) { finish(job, 'cancelled'); return cleanupThen(job); }
            var name = job.io.frameName(batch[i].ordinal);
            var path = job.dir ? job.dir + '/' + name : name;
            await Promise.resolve(job.io.write(path, batch[i].text));
            job.written.push(name);
            job.progress = job.written.length / job.total;
            if (job.onProgress) job.onProgress(job.written.length, job.total);
          }
          if (job.next > job.end) break;
          if (job.cancelRequested) { finish(job, 'cancelled'); return cleanupThen(job); }
          if (!checkDocument(job, 'between batches')) return cleanupThen(job);
          job.phase = 'evaluate';
          batch = evaluateBatch(job);
        }
        if (job.cancelRequested) { finish(job, 'cancelled'); return cleanupThen(job); }
        var fp = fingerprint();
        if (fp.documentId !== job.identity.documentId) {
          finish(job, 'cancelled', { code: 'document_replaced', message: 'The document was replaced before the export could commit.' });
          return cleanupThen(job);
        }
        finish(job, 'succeeded');
        settle(job);
      } catch (e) {
        var code = job.phase === 'evaluate' ? 'evaluate_failed' : job.phase === 'mkdir' ? 'mkdir_failed' : 'write_failed';
        finish(job, 'failed', { code: code, message: e && e.message ? e.message : String(e) });
        cleanupThen(job);
      }
    }

    function begin(input) {
      input = input || {};
      if (input.requestId && byRequest[input.requestId]) return { ok: true, jobId: byRequest[input.requestId], retried: true, job: view(jobs[byRequest[input.requestId]]) };
      if (running) return failure('busy', 'Another export job is running.');
      if (!input.sink && (typeof input.dir !== 'string' || !input.dir)) return failure('invalid_input', 'dir is required.');
      if (input.dir != null && typeof input.dir !== 'string') return failure('invalid_input', 'dir must be a string.');
      var start = input.start, end = input.end;
      if (!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || end < start) return failure('invalid_range', 'start/end must be integers with 0 <= start <= end.');
      var job = {
        jobId: ports.newId(), requestId: input.requestId || null, status: 'running',
        dir: input.dir || '', io: resolveSink(input.sink), start: start, end: end, next: start, total: end - start + 1,
        batchFrames: input.batchFrames > 0 ? input.batchFrames : DEFAULT_BATCH_FRAMES,
        batchBytes: input.batchBytes > 0 ? input.batchBytes : DEFAULT_BATCH_BYTES,
        keepPartial: !!input.keepPartial, onProgress: typeof input.onProgress === 'function' ? input.onProgress : null,
        evaluated: 0, written: [], progress: 0, createdDir: false, cancelRequested: false, error: null, cleanup: null,
        phase: 'begin', identity: null, resolve: null, done: null,
      };
      job.done = new Promise(function (resolve) { job.resolve = resolve; });
      jobs[job.jobId] = job;
      if (job.requestId) byRequest[job.requestId] = job.jobId;
      running = job;
      // Synchronous capture + first batch: no await has happened yet.
      var first;
      try {
        if (typeof ports.beforeCapture === 'function') ports.beforeCapture();
        job.identity = fingerprint();
        job.phase = 'evaluate';
        first = evaluateBatch(job);
      } catch (e) {
        // Nothing was written and no directory was created: terminal at once.
        finish(job, 'failed', { code: 'evaluate_failed', message: e && e.message ? e.message : String(e) });
        settle(job);
        return { ok: true, jobId: job.jobId, job: view(job) };
      }
      // run() catches everything it awaits; this last net turns any internal
      // fault into a terminal failed job so the slot and done() never hang.
      run(job, first).catch(function (e) {
        finish(job, 'failed', { code: 'internal', message: e && e.message ? e.message : String(e) });
        settle(job);
      });
      return { ok: true, jobId: job.jobId, job: view(job) };
    }

    function status(jobId) {
      var job = jobs[jobId];
      if (!job) return failure('unknown_job', 'No job with id ' + jobId + '.');
      return { ok: true, job: view(job) };
    }

    function cancel(jobId) {
      var job = jobs[jobId];
      if (!job) return failure('unknown_job', 'No job with id ' + jobId + '.');
      if (job.status === 'running') job.cancelRequested = true;
      return { ok: true, job: view(job) };
    }

    // Resolves with the terminal view once the job has finished (and cleaned up).
    function done(jobId) {
      var job = jobs[jobId];
      if (!job) return Promise.reject(new Error('No job with id ' + jobId + '.'));
      return job.done;
    }

    return { begin: begin, status: status, cancel: cancel, done: done,
      meta: function () { return { running: running ? running.jobId : null, jobs: Object.keys(jobs).length }; } };
  }

  return { create: create, DEFAULT_BATCH_FRAMES: DEFAULT_BATCH_FRAMES, DEFAULT_BATCH_BYTES: DEFAULT_BATCH_BYTES };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoExportJob;
