/* The video provider of the native desktop importer (T12 point 8): the ordinary video menu and
 * the footage panel's Replace control reach ONE native linked-video command through
 * native-video-asset.js. DOM Files have no admitted native handle, so drops stay unavailable.
 * An unknown outcome latches the exact owner/request until it is resolved by the exact replay,
 * by presentation of the receipt's revision chain (checked on every status read), or by a reopened
 * owner. UI intent only: there is no legacy relink fallback. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeImportVideo = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const pinKeys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  const samePins = (a, b) => !!a && !!b && pinKeys.every((key) => a[key] === b[key]);
  const sameOwner = (a, b) => !!a && !!b && pinKeys.slice(0, 3).every((key) => a[key] === b[key]);
  const sameSelection = (a, b) => Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((uid, i) => uid === b[i]);

  // ports: root, core (native-import-core instance), presented(session, info, frame), current(), selection(), declared (default true).
  function create(ports) {
    const root = ports.root, core = ports.core, current = ports.current, selection = ports.selection;
    const declared = ports.declared === undefined || ports.declared === true;
    let pending = false, retired = false, uncertain = null;
    const projected = (info) => root.NemoNativeObjectProjectFile && root.NemoNativeObjectProjectFile.projectionReadyForUI(root, info) === true;
    const settled = () => { try { return !!(uncertain && uncertain.receipt && !pending && confirmed(uncertain.ctx, uncertain.state, uncertain.receipt)); } catch (_) { return false; } };
    function status() {
      if (!uncertain) return null;
      try {
        const session = current(), now = session && session.info();
        // Absence, temporary unavailability or an ordinary revision/frame change never proves a reopen.
        if (session && session.isActive() && now && typeof now.instanceId === 'string' && now.instanceId.length > 0 && typeof now.documentId === 'string' && now.documentId.length > 0 &&
          Number.isSafeInteger(now.lifecycleGeneration) && now.lifecycleGeneration > 0 && !sameOwner(now, uncertain.ctx.before)) uncertain = null;
      } catch (_) { /* preserve the unresolved native disposition */ }
      // A known receipt resolves itself as soon as the same owner presents its revision or a later one with the target in place.
      if (settled()) uncertain = null;
      if (!uncertain) return null;
      const { ctx, error, receipt, lastFailure } = uncertain;
      return { status: 'unconfirmed', code: error && error.code || 'video_presentation_unconfirmed', intent: ctx.intent.kind,
        requestId: error && error.requestId || receipt && receipt.requestId || null,
        owner: Object.fromEntries(pinKeys.slice(0, 3).map((key) => [key, ctx.before[key]])),
        receipt: receipt || null, details: lastFailure && lastFailure.details || error && error.details || null,
        recovery: error && typeof error.replay === 'function' ? 'exact-request' : receipt ? 'presentation' : 'reopen-owner' };
    }
    function uncertainError(error) {
      const details = error && error.details, code = error && error.code;
      const disposition = details && (Object.prototype.hasOwnProperty.call(details, 'disposition') || Object.prototype.hasOwnProperty.call(details, 'committed'));
      return !!(error && typeof error.replay === 'function') || code === 'video_command_indeterminate' || code === 'uncorrelated' ||
        error && error.reason === 'reconcile_failed' || !code || !!(disposition &&
          !(details.disposition === 'notDispatched' && details.committed === false));
    }
    // The receipt's revision is in the presented chain: same owner, at or past that revision, presented at the CURRENT frame (the
    // playhead is presentation, not document state), and the target still present. Returns the presented info or null.
    // #2256: the component the import joined, when that component is the one presented now (its rows are the component read, not the scene table).
    const held = () => { const c = root.NemoNativeComponentContext; return c && typeof c.presented === 'function' ? c.presented(root) : null; };
    const heldRows = (contextId) => { const c = root.NemoNativeComponentContext, here = held(); return here && here.contextId === contextId && typeof c.layers === 'function' ? c.layers(root) : null; };
    function confirmed(ctx, state, receipt) {
      const after = ctx.session.info(), replace = ctx.intent.kind === 'replace', inside = ctx.intent.contextId !== undefined && !replace;
      const owner = !retired && current() === ctx.session && ctx.session.isActive() && sameOwner(after, ctx.before) &&
        receipt.contentRevision === ctx.before.contentRevision + 1 && Number.isSafeInteger(after.contentRevision) && after.contentRevision >= receipt.contentRevision &&
        root.state === state && projected(after);
      if (inside) {
        // The owner acknowledged the revision chain; the layer is verified against the component read while that component is presented, and the
        // receipt must name the component the request asked for (a scene presentation after leaving it cannot show a component layer).
        if (!owner || receipt.target.contextId !== ctx.intent.contextId) return null;
        const here = held(), rows = heldRows(ctx.intent.contextId);
        if (here && here.contextId !== ctx.intent.contextId) return null;
        return !here || (rows && rows.some((row) => row.layerUid === receipt.target.layerUid)) ? after : null;
      }
      const shown = owner && ports.presented(ctx.session, after, state.currentFrame);
      // A replace must name the exact target the pinned placement read presented.
      const placed = Array.isArray(after.layers) && after.layers.some((row) => row.layerUid === receipt.target.layerUid) &&
        (!replace || receipt.target.layerUid === ctx.intent.layerUid && receipt.target.contextId === ctx.intent.contextId);
      return shown && placed ? after : null;
    }
    function finish(ctx, state, receipt, recovering) {
      const replace = ctx.intent.kind === 'replace', after = confirmed(ctx, state, receipt), frame = state.currentFrame;
      if (!after) {
        uncertain = { ctx, state, receipt, error: null };
        return status() || { status: 'unconfirmed', code: 'video_owner_changed', receipt };
      }
      uncertain = null;
      const out = { status: replace ? 'replaced' : 'imported', readback: 'confirmed', receipt, selected: false };
      // A replacement keeps its layer; a late recovery resolves the receipt without taking the user's current selection.
      if (!replace && !recovering && sameSelection(selection(), ctx.selection)) {
        if (ctx.intent.contextId !== undefined) {
          // Inside the component the new layer is selected through the context controller (its rows are the component's).
          const rows = heldRows(ctx.intent.contextId), index = rows ? rows.findIndex((row) => row.layerUid === receipt.target.layerUid) : -1;
          const consumed = index >= 0 && root.NemoNativeComponentContext.selectLayer(root, index) === true;
          out.selected = consumed && current() === ctx.session && samePins(ctx.session.info(), after) && state.currentFrame === frame &&
            sameSelection(selection(), [receipt.target.layerUid]);
        } else {
          const index = after.layers.findIndex((row) => row.layerUid === receipt.target.layerUid);
          const consumed = root.NemoNativeObjectProject.selectLayerUI(root, index, false, receipt.target.layerUid, after) === true;
          out.selected = consumed && current() === ctx.session && samePins(ctx.session.info(), after) && state.currentFrame === frame &&
            sameSelection(selection(), [receipt.target.layerUid]) && ports.presented(ctx.session, after, frame);
        }
      }
      return out;
    }
    async function recover() {
      if (retired) return { status: 'unavailable', reason: 'retired' };
      if (pending) return { status: 'busy' };
      if (settled()) return finish(uncertain.ctx, uncertain.state, uncertain.receipt, true);
      const unresolved = status();
      if (!unresolved) return { status: 'nothing' };
      const held = uncertain;
      if (!held.receipt && !(held.error && typeof held.error.replay === 'function')) return unresolved;
      pending = true;
      try {
        const receipt = held.receipt || await held.error.replay();
        return finish(held.ctx, held.state, receipt, true);
      } catch (error) {
        // Even a later refusal cannot erase the earlier unknown commit or its exact-request recovery.
        held.lastFailure = error;
        return status() || { status: 'unavailable', reason: 'owner_reopened' };
      } finally { pending = false; }
    }
    // The pinned origin of an import inside the presented component: the owner pins, the playhead and the component's own selection (no scene lease).
    function beginInside(here) {
      try {
        const session = current(), before = session && session.info(), rows = heldRows(here.contextId), chosen = selection(), frame = root.state.currentFrame;
        if (!session || !session.isActive() || !before || !rows || !Array.isArray(chosen) || !Number.isSafeInteger(frame) || frame < 0 || frame >= here.totalFrames ||
            typeof session.runComponentCommand !== 'function' || !projected(before)) return { ok: false, status: 'unavailable', reason: 'no_presented_native_session' };
        return { ok: true, ctx: Object.freeze({ session, before, frame, selection: chosen.slice() }) };
      } catch (_) { return { ok: false, status: 'unavailable', reason: 'no_presented_native_session' }; }
    }
    // The component is still the presented one (its projection settled at the pinned revision); inside the queue the lease must show that very context at the frame.
    function shownInside(queueScope, ctx) {
      const here = held(), c = root.NemoNativeComponentContext;
      if (!here || here.contextId !== ctx.intent.contextId || c.projectionReady(root, ctx.before) !== true) return false;
      if (!queueScope) return true;
      if (typeof queueScope.readPresentedObjectSession !== 'function') return false;
      const shown = queueScope.readPresentedObjectSession();
      return !!shown && !!shown.presentation && shown.presentation.contextId === ctx.intent.contextId && shown.presentation.frame === ctx.frame && samePins(shown, ctx.before);
    }
    // One picker, one native link. `placementId` (replace only) is the placement the footage panel read at the playhead.
    async function run(placementId) {
      if (retired || !declared) return { status: 'unavailable', reason: 'native_video_route_absent' };
      if (pending) return { status: 'busy' };
      const unresolved = status();
      if (unresolved) return unresolved;
      // A new linked layer is made in the scene root, or (#2256) inside the component the editor presents: the context controller's held id is sent.
      const inside = placementId === undefined && root.state && root.state.activeSymbolId ? held() : null;
      if (placementId === undefined && root.state && root.state.activeSymbolId && !inside) return { status: 'unavailable', reason: 'component_context_import_unavailable' };
      const started = inside ? beginInside(inside) : core.begin();
      if (!started.ok) return { status: started.status, reason: started.reason };
      const base = started.ctx, state = root.state, symbol = state.activeSymbolId, activeLayer = state.activeLayerIdx;
      let intent = inside ? { kind: 'newLayer', contextId: inside.contextId } : { kind: 'newLayer' };
      if (placementId !== undefined) {
        const row = base.before.layers[activeLayer], read = root.SM && typeof root.SM.nativeVideoPlacement === 'function' ? root.SM.nativeVideoPlacement(activeLayer) : null;
        if (typeof placementId !== 'string' || !placementId) return { status: 'invalid', reason: 'placement_id_required' };
        // The pinned placement read must still name this placement on the active layer of the presented snapshot.
        if (!row || !state.layers[activeLayer] || state.layers[activeLayer].layerUid !== row.layerUid || !read || read.placementId !== placementId)
          return { status: 'stale', reason: 'placement_changed' };
        // The pinned read is the layer timing view of the scene-root context (native-object-project readLayerView).
        intent = { kind: 'replace', placementId, layerUid: row.layerUid, contextId: 'scene-root' };
      }
      const ctx = Object.freeze(Object.assign({}, base, { intent: Object.freeze(intent) }));
      if (typeof ctx.session.readSubscriptionGeneration !== 'function') return { status: 'unavailable', reason: 'native_subscription_identity_absent' };
      const subscriptionGeneration = ctx.session.readSubscriptionGeneration();
      const currentIntent = (queueScope) => !retired && current() === ctx.session && ctx.session.isActive() && root.state === state &&
        state.currentFrame === ctx.frame && state.activeSymbolId === symbol && state.activeLayerIdx === activeLayer &&
        ctx.session.readSubscriptionGeneration() === subscriptionGeneration && samePins(ctx.session.info(), ctx.before) && sameSelection(selection(), ctx.selection) &&
        projected(ctx.before) && (inside ? shownInside(queueScope, ctx) : (!queueScope || typeof queueScope.readPresentedObjectSession === 'function') && ports.presented(queueScope || ctx.session, ctx.before, ctx.frame));
      const api = root.NemoNativeVideoAsset;
      const client = api && api.desktop(root, { videoRoute: () => !retired && declared });
      if (!client) return { status: 'unavailable', reason: 'native_video_client_absent' };
      pending = true;
      try {
        const options = { origin: ctx.before, current: currentIntent };
        if (inside) options.contextId = inside.contextId;
        const receipt = intent.kind === 'replace' ? await client.replacePicked(placementId, options) : await client.importPicked(options);
        return receipt ? finish(ctx, state, receipt, false) : { status: 'cancelled' };
      } catch (error) {
        const code = error && error.code;
        if (uncertainError(error)) {
          uncertain = { ctx, state, error, receipt: null };
          return status() || { status: 'unconfirmed', code: code || 'native_video_failed', requestId: error && error.requestId };
        }
        if (code === 'stale_revision' || code === 'stale_owner') return { status: 'stale', code };
        if (code === 'unavailable') return { status: 'unavailable', code, reason: error.reason || null };
        if (code === 'invalid_request') return { status: 'invalid', code, reason: error.reason || null };
        return { status: 'refused', code };
      } finally { pending = false; }
    }
    return Object.freeze({
      available: () => false,
      declared: () => !retired && declared,
      importFiles: async () => ({ status: 'unavailable', reason: 'native_drop_handle_absent' }),
      status, recover,
      openFile: () => run(undefined),
      replaceActive: (placementId) => run(typeof placementId === 'string' ? placementId : null),
      dispose() { retired = true; },
    });
  }
  return Object.freeze({ create });
}));
