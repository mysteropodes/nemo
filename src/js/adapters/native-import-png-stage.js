/* Caller-owned PNG files for one native import attempt. No document writer, decoder,
 * replay or sweep. A possibly invoked attempt keeps its files and obligation. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeImportPngStage = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const MAX_BYTES = 16 * 1024 * 1024, MAX_DIMENSION = 8192, MAX_RGBA = 256 * 1024 * 1024;
  const MAX_SOURCES = 1024;
  const SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10];
  const PIN_KEYS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
  const normalized = value => value.replace(/\\/g, '/').replace(/\/+$/, '');
  function fail(code) { const error = new Error(code); error.code = code; throw error; }
  function identifier(value) {
    return typeof value === 'string' && value.trim().length > 0 && !value.includes('\0') && new TextEncoder().encode(value).length <= 255;
  }
  function dimensions(width, height) {
    if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width < 1 || height < 1 ||
        width > MAX_DIMENSION || height > MAX_DIMENSION || width * height * 4 > MAX_RGBA) fail('png_dimensions');
  }
  // Only bounded IHDR metadata. Native PNG signature/chunks/CRC/decode remain authoritative.
  function header(buffer) {
    if (!(buffer instanceof ArrayBuffer) || buffer.byteLength !== 33) fail('png_header');
    const bytes = new Uint8Array(buffer), view = new DataView(buffer);
    if (!SIGNATURE.every((value, i) => bytes[i] === value) || view.getUint32(8) !== 13 ||
        bytes[12] !== 73 || bytes[13] !== 72 || bytes[14] !== 68 || bytes[15] !== 82) fail('png_header');
    const width = view.getUint32(16), height = view.getUint32(20);
    dimensions(width, height);
    return { width, height };
  }
  function create(ports) {
    if (!ports || !ports.core || ports.core.supportsInvocationMarker !== true || typeof ports.core.submit !== 'function' || !ports.path ||
        !['tempDir', 'join'].every(key => typeof ports.path[key] === 'function') ||
        !ports.fs || !['mkdir', 'writeFile', 'writeTextFile', 'remove', 'exists'].every(key => typeof ports.fs[key] === 'function') ||
        typeof ports.uuid !== 'function') throw new TypeError('PNG staging ports unavailable');
    const fs = ports.fs, path = ports.path;
    function createAttempt(ctx, options) {
      options = options || {};
      if (!ctx || !ctx.before) throw new TypeError('native import origin unavailable');
      const pins = Object.fromEntries(PIN_KEYS.map(key => [key, ctx.before[key]]));
      const id = ports.uuid();
      if (typeof id !== 'string' || !uuidPattern.test(id)) throw new TypeError('unique staging id unavailable');
      let directory = null, journal = null, created = false, journalWritten = false;
      let invoked = false, submitted = false, cancelled = false, cleaned = false, inflight = null;
      const entries = [], leaves = [];
      function current() {
        if (cancelled || (options.signal && options.signal.aborted)) fail('staging_cancelled');
        if (options.current && options.current() !== true) fail('staging_stale');
      }
      async function child(base, name) {
        const joined = await path.join(base, name);
        if (typeof joined !== 'string' || joined.includes('\0') || normalized(joined) !== normalized(base) + '/' + name) fail('staging_path');
        return joined;
      }
      function obligation(phase) {
        return JSON.stringify({ version: 1, attemptId: id, phase, origin: pins,
          files: leaves.slice(), sources: entries.map(entry => entry.descriptor) });
      }
      async function writeJournal(phase) {
        journalWritten = false;
        await fs.writeTextFile(journal, obligation(phase));
        journalWritten = true;
      }
      async function ensureDirectory() {
        if (created) return;
        const temporary = await path.tempDir();
        current();
        if (typeof temporary !== 'string' || temporary.includes('\0') || !/^(?:\/|[A-Za-z]:[\\/]|\\\\)/.test(temporary) ||
            temporary.split(/[\\/]/).includes('..')) fail('staging_path');
        directory = await child(temporary, 'nemo-native-import-' + id);
        current();
        // Nonrecursive exclusive creation: a collision is never reused or deleted.
        await fs.mkdir(directory, { recursive: false });
        created = true;
        journal = await child(directory, 'obligation.json');
        await writeJournal('staging');
        current();
      }
      function exclusive(work) {
        if (inflight) return Promise.reject(Object.assign(new Error('staging_busy'), { code: 'staging_busy' }));
        const pending = Promise.resolve().then(work);
        inflight = pending;
        return pending.finally(() => { if (inflight === pending) inflight = null; });
      }
      async function file(sourceId, name, blob, expected) {
        current();
        if (submitted || cleaned) fail('staging_closed');
        if (!identifier(sourceId) || !identifier(name) || entries.some(entry => entry.descriptor.sourceId === sourceId)) fail('staging_source');
        if (entries.length >= MAX_SOURCES) fail('staging_source_limit');
        if (!blob || !Number.isSafeInteger(blob.size) || blob.size < 33 || blob.size > MAX_BYTES ||
            typeof blob.slice !== 'function' || typeof blob.arrayBuffer !== 'function') fail('png_size');
        const metadata = header(await blob.slice(0, 33).arrayBuffer());
        current();
        if (expected && (metadata.width !== expected.width || metadata.height !== expected.height)) fail('png_dimensions_changed');
        const buffer = await blob.arrayBuffer();
        current();
        if (!(buffer instanceof ArrayBuffer) || buffer.byteLength !== blob.size || buffer.byteLength > MAX_BYTES) fail('png_size');
        // The immutable Blob's full bytes must agree with the admitted header before any file is written.
        const fullHeader = header(buffer.slice(0, 33));
        if (fullHeader.width !== metadata.width || fullHeader.height !== metadata.height) fail('png_header_changed');
        await ensureDirectory();
        current();
        const leaf = await child(directory, 'source-' + String(entries.length).padStart(4, '0') + '.png');
        current();
        leaves.push(leaf);
        const descriptor = Object.freeze({ sourceId, name, mediaType: 'image/png', source: Object.freeze({ kind: 'file', path: leaf }) });
        const entry = { descriptor, metadata: Object.freeze({ ...metadata }), ready: false };
        entries.push(entry);
        await writeJournal('staging');
        current();
        await fs.writeFile(leaf, new Uint8Array(buffer));
        current();
        entry.ready = true;
        return descriptor;
      }
      async function cleanup() {
        if (cleaned || !created) return { status: 'cleaned' };
        const failures = [];
        async function remove(resource) {
          try { if (await fs.exists(resource)) await fs.remove(resource, { recursive: false }); }
          catch (_) { failures.push(resource); }
        }
        for (const leaf of leaves) await remove(leaf);
        // A failed PNG deletion must leave the obligation in place.
        if (failures.length) {
          if (!journalWritten) {
            try { await writeJournal('cleanup_pending'); } catch (_) { journalWritten = false; failures.push(journal); }
          }
          return { status: 'retained', reason: 'cleanup_failed', directory, failures, obligationPersisted: journalWritten };
        }
        if (journal) await remove(journal);
        if (failures.length) {
          try { journalWritten = journalWritten && await fs.exists(journal); } catch (_) { journalWritten = false; }
          if (!journalWritten) { try { await writeJournal('cleanup_pending'); } catch (_) { failures.push(journal); } }
          return { status: 'retained', reason: 'cleanup_failed', directory, failures, obligationPersisted: journalWritten };
        }
        journalWritten = false;
        await remove(directory);
        if (failures.length) {
          // The PNGs are gone, but a directory failure still needs an explicit obligation.
          try { await writeJournal('cleanup_pending'); } catch (_) { failures.push(journal); }
          return { status: 'retained', reason: 'cleanup_failed', directory, failures, obligationPersisted: journalWritten };
        }
        cleaned = true;
        return { status: 'cleaned' };
      }
      function retained(reason) { return { status: 'retained', reason, directory, obligationPersisted: journalWritten }; }
      async function abort() {
        if (cleaned) return { status: 'cleaned' };
        cancelled = true;
        if (invoked) return retained('dispatch_may_have_started');
        if (inflight) { try { await inflight; } catch (_) { /* preinvoke failure still owns its directory */ } }
        if (invoked) return retained('dispatch_may_have_started');
        return cleanup();
      }
      function submit(batch) {
        if (submitted) return Promise.resolve({ status: 'invalid', reason: 'staging_already_submitted', staging: cleaned ? { status: 'cleaned' } : retained('attempt_already_submitted') });
        if (inflight) return Promise.resolve({ status: 'busy', staging: retained('stage_inflight') });
        let sealed, sealError;
        try { sealed = JSON.parse(JSON.stringify(batch)); } catch (error) { sealError = error; }
        submitted = true;
        return exclusive(async () => {
          try {
            current();
            if (sealError) fail('staging_batch_invalid');
            if (!entries.length) fail('staging_empty');
            if (!Array.isArray(sealed.sources) || !entries.every(entry => entry.ready &&
              sealed.sources.filter(source => JSON.stringify(source) === JSON.stringify(entry.descriptor)).length === 1)) fail('staging_sources_mismatch');
            await writeJournal('dispatch_may_have_started');
            current();
          } catch (error) {
            return { status: 'invalid', reason: error.code || 'staging_preparation_failed', staging: await cleanup() };
          }
          let result;
          try {
            result = await ports.core.submit(ctx, sealed, () => { current(); invoked = true; });
          } catch (_) {
            return { status: 'unconfirmed', reason: 'staging_submit_failed', staging: invoked ? retained('dispatch_may_have_started') : await cleanup() };
          }
          // Core reaches imported only after exact owner/revision/result/accounting correlation.
          const staging = result && result.status === 'imported' ? await cleanup() : invoked ? retained('dispatch_may_have_started') : await cleanup();
          return Object.assign({}, result, { staging });
        });
      }
      return Object.freeze({
        stageFile(sourceId, name, blob) { return exclusive(() => file(sourceId, name, blob)); },
        stageCanvas(sourceId, name, canvas) {
          return exclusive(async () => {
            current();
            if (!canvas || typeof canvas.toBlob !== 'function') fail('png_canvas');
            const width = canvas.width, height = canvas.height;
            dimensions(width, height);
            const blob = await new Promise((resolve, reject) => {
              try { canvas.toBlob(resolve, 'image/png'); } catch (error) { reject(error); }
            });
            current();
            return file(sourceId, name, blob, { width, height });
          });
        },
        dimensions(sourceId) {
          const entry = entries.find(value => value.descriptor.sourceId === sourceId);
          if (!entry || !entry.ready) fail('staging_source');
          return entry.metadata;
        },
        submit, abort,
      });
    }
    return Object.freeze({ createAttempt });
  }
  return Object.freeze({ create });
}));
