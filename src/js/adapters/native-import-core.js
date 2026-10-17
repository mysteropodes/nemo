/* L10-J shared submit runner of the native import adapters. UI intent only: it owns no document, no history and no fallback
 * writer. One import = ONE typed batch pinned to the presented native revision and sent through the session's pinned mutation entry
 * (`session.importBatch`); the receipt is correlated against the batch (every layer, record and skip accounted for, revision advance
 * exactly the applied edit) and the result is re-read from the session. The pin / receipt discipline is the one of
 * native-drawing-add.js: apiVersion 2, historyEntriesAdded 1, and the statuses unavailable / busy / unconfirmed are never retried.
 * An absent declaration, an absent presented session or a native refusal is a typed status, never a legacy importer. */
(function (root, factory) {
  const contract = typeof module === 'object' && module.exports ? require('./native-import-contract.js') : root.NemoNativeImportContract;
  const api = factory(contract);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeImportCore = api;
}(typeof globalThis === 'object' ? globalThis : this, function (contract) {
  'use strict';

  const PIN_KEYS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  const REFUSED = /^Native command refused: ([a-z_]+)/;
  const same = (a, b) => !!a && !!b && PIN_KEYS.every((k) => a[k] === b[k]);
  const sameIds = (a, b) => Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((v, i) => v === b[i]);
  const triple = (rows) => rows.map((r) => `${r.kind}\u0000${r.sourceId}\u0000${r.reason}`);

  function presented(session, info, frame) {
    const value = session.readPresentedObjectSession();
    return !!value && value.available === true && same(info, value) && same(info, value.presentation) &&
      value.presentation.owner === 'native' && value.presentation.status === 'presented' && value.presentation.frame === frame;
  }
  function wire(batch) {
    return JSON.parse(JSON.stringify({
      command: contract.COMMAND,
      layers: batch.layers || [], records: batch.records || [], skipped: batch.skipped || [], unavailable: batch.unavailable || [],
    }));
  }
  // The receipt must account for exactly what the batch declared: created + skipped = declared source entries.
  function accounting(applied, result, batch) {
    if (!applied) return result.layers.length === 0 && result.objects.length === 0 && batch.layers.length === 0 && batch.records.length === 0 &&
      sameIds(triple(result.skipped), triple(batch.skipped));
    return sameIds(result.layers.map((r) => r.sourceId), batch.layers.map((l) => l.sourceId)) &&
      sameIds(result.objects.map((r) => r.sourceId), batch.records.map((r) => r.sourceId)) &&
      new Set(result.layers.map((r) => r.layerUid)).size === result.layers.length &&
      new Set(result.objects.map((r) => r.strokeId)).size === result.objects.length &&
      sameIds(triple(result.skipped), triple(batch.skipped));
  }

  function create(ports) {
    let pending = false, generation = 0, retired = false;

    // Reads the presented native context once; the caller builds its batch from this snapshot and submits it without an await between.
    function begin() {
      if (retired) return { ok: false, status: 'unavailable', reason: 'retired' };
      if (pending) return { ok: false, status: 'busy' };
      let session, before, frame, selection, write;
      try {
        session = ports.currentSession(); before = session && session.info(); frame = ports.frame(); selection = ports.selection();
        if (!session || !before || !Array.isArray(before.layers) || !Number.isSafeInteger(frame) || frame < 0 || frame >= before.totalFrames ||
          !Array.isArray(selection) || ports.available(session, before) !== true || !presented(session, before, frame)) {
          return { ok: false, status: 'unavailable', reason: 'no_presented_native_session' };
        }
        write = session.importBatch;
      } catch (_) { return { ok: false, status: 'unavailable', reason: 'no_presented_native_session' }; }
      if (typeof write !== 'function') return { ok: false, status: 'unavailable', reason: 'native_declaration_absent' };
      return { ok: true, ctx: Object.freeze({ session, before, frame, write, selection: selection.slice() }) };
    }

    async function submit(ctx, batch) {
      if (retired) return { status: 'unavailable', reason: 'retired' };
      if (pending) return { status: 'busy' };
      const { session, before, frame, write } = ctx;
      let payload;
      try {
        payload = wire(batch);
        contract.payload(contract.OP, payload);
      } catch (error) { return { status: 'invalid', reason: String(error && error.message || error) }; }
      const probe = { apiVersion: 2, requestId: 'project-import-batch-999999999', instanceId: before.instanceId, documentId: before.documentId,
        operation: contract.OP, expectedRevision: before.contentRevision, payload };
      const size = contract.bytes(JSON.stringify(probe)), limit = ports.maxRequestBytes === undefined ? contract.MAX_REQUEST_BYTES : ports.maxRequestBytes;
      if (!(size <= limit)) return { status: 'too_large', bytes: size, limit };
      try {
        if (ports.currentSession() !== session || !same(session.info(), before) || ports.frame() !== frame) return { status: 'stale', reason: 'context_changed' };
      } catch (_) { return { status: 'unavailable', reason: 'no_presented_native_session' }; }
      const ticket = ++generation;
      pending = true;
      let receipt;
      try {
        receipt = await write.call(session, payload, before);
      } catch (error) {
        const message = String(error && error.message || error), refused = REFUSED.exec(message);
        if (refused) {
          const code = refused[1];
          return code === 'stale_revision' ? { status: 'stale', code } : code === 'unavailable' ? { status: 'unavailable', reason: 'native_refused', code } : { status: 'refused', code };
        }
        // Raised before dispatch by the session itself: nothing was sent.
        if (/intent is stale or invalid/.test(message)) return { status: 'stale', reason: 'intent_stale' };
        return { status: 'unconfirmed', indeterminate: true, message };
      } finally { pending = false; }
      return settle(ctx, batch, receipt, ticket);
    }

    function settle(ctx, batch, receipt, ticket) {
      const { session, before, frame } = ctx;
      const result = receipt && receipt.result;
      try {
        if (!receipt || receipt.apiVersion !== 2 || receipt.ok !== true || receipt.instanceId !== before.instanceId || receipt.documentId !== before.documentId) return { status: 'unconfirmed', reason: 'receipt_uncorrelated' };
        contract.result(contract.OP, result);
        if (receipt.contentRevision !== before.contentRevision + (result.applied ? 1 : 0)) return { status: 'unconfirmed', reason: 'revision_uncorrelated' };
        if (!accounting(result.applied, result, batch)) return { status: 'unconfirmed', reason: 'accounting_mismatch', receipt: result };
      } catch (error) { return { status: 'unconfirmed', reason: 'receipt_invalid', message: String(error && error.message || error) }; }
      const out = {
        status: 'imported', applied: result.applied, revision: receipt.contentRevision, historyEntriesAdded: result.historyEntriesAdded,
        layers: result.layers, objects: result.objects, skipped: result.skipped, readback: 'unconfirmed', selected: false,
      };
      try {
        if (ticket !== generation || ports.currentSession() !== session) return out;
        const after = session.info(), rows = after && after.layers, made = result.layers;
        if (!after || after.instanceId !== before.instanceId || after.documentId !== before.documentId || after.lifecycleGeneration !== before.lifecycleGeneration ||
          after.contentRevision !== receipt.contentRevision || !Array.isArray(rows) || rows.length !== before.layers.length + made.length ||
          !before.layers.every((row, i) => rows[i].layerUid === row.layerUid)) return out;
        const added = rows.slice(before.layers.length);
        const names = batch.layers.map((l) => l.name);
        if (!made.every((m, i) => added[i].layerUid === m.layerUid && added[i].name === names[i] && added[i].visible === true &&
          added[i].locked === false && added[i].parentLayerUid === null)) return out;
        out.readback = 'confirmed';
        if (!made.length || typeof ports.select !== 'function') return out;
        if (ports.available(session, after) !== true || ports.frame() !== frame || !sameIds(ctx.selection, ports.selection()) || !presented(session, after, frame)) return out;
        const target = made[made.length - 1].layerUid;
        out.selected = ports.select(target, after, session) === true && ports.currentSession() === session && same(session.info(), after) &&
          presented(session, after, frame) && ports.frame() === frame && sameIds([target], ports.selection());
      } catch (_) { out.selected = false; }
      return out;
    }

    return Object.freeze({
      begin, submit,
      dispose() { retired = true; generation++; },
      isPending() { return pending; },
    });
  }
  return Object.freeze({ create, accounting, wire, presented });
}));
