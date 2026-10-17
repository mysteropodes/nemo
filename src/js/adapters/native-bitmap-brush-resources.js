/* Bitmap-brush resources as native operations (lane 7, G06/G17). A thin client of the native project session:
 *  - publishAsset: ask the native owner to publish an admitted embedded PNG asset as the brush bitmap `brushId`;
 *  - publishCanvas: hand the baked canvas, as PNG bytes, to the host byte route and the asset catalog, then publish it;
 *  - inspect / readScene: read the store row of one brush and the brush part of one pinned scene;
 *  - republishMissing: re-publish the bakes the pinned scene reports as `ink-missing`.
 * It forwards ids, sizes and PNG bytes and reports the owner's typed answers. It holds no bitmap, no cache, no byte
 * budget and no eviction list, it never calls the render engine, and no Paper, bake or legacy upload writer is reachable
 * from here. Absent native support is typed unavailability, never a fallback. */
(function (root, factory) {
  var contract = typeof module === 'object' && module.exports ? require('./native-bitmap-brush-resources-contract.js') : root.NemoNativeBitmapBrushResourcesContract;
  var api = factory(contract);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeBitmapBrushResources = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Contract) {
  'use strict';

  function fault(code, message, extra) {
    var error = new Error(message);
    error.code = code;
    return Object.assign(error, extra || {});
  }
  function invalid(message) { return fault('invalid_request', message, { reason: 'local_validation' }); }
  function unavailable(reason, message) { return fault('unavailable', message, { reason: reason }); }
  function ownerKey(pin) { return JSON.stringify([pin.instanceId, pin.documentId, pin.lifecycleGeneration]); }

  // The baker clamps every canvas to 1..4096 per side, so anything else is not a brush bake.
  function checkSize(width, height) {
    if (!Contract.validSide(width) || !Contract.validSide(height)) throw invalid('A brush bitmap is 1..' + Contract.MAX_SIDE + ' pixels per side');
    return Object.freeze({ width: width, height: height, bytes: Contract.bytesFor(width, height) });
  }

  function encodeWithToBlob(canvas) {
    return new Promise(function (resolve, reject) {
      if (!canvas || typeof canvas.toBlob !== 'function') {
        reject(unavailable('png_encoder_unavailable', 'This canvas cannot be encoded as PNG'));
        return;
      }
      canvas.toBlob(function (blob) {
        if (!blob) { reject(unavailable('png_encode_failed', 'The canvas produced no PNG')); return; }
        blob.arrayBuffer().then(function (buffer) { resolve(new Uint8Array(buffer)); }, reject);
      }, 'image/png');
    });
  }

  function create(session, ports) {
    ports = ports || {};
    var issued = new WeakSet();

    function owner() {
      var pin = session.info();
      if (!session.isActive() || !pin) throw unavailable('no_native_project', 'Native project is unavailable');
      return pin;
    }
    function refusal(response, request) {
      var error = response.error || {};
      return fault(typeof error.code === 'string' ? error.code : 'unavailable', 'Native brush resource request refused: ' + (error.code || 'unavailable'),
        { reason: error.details && error.details.reason || null, requestId: request.requestId, contentRevision: response.contentRevision });
    }
    function uncorrelated(request, why) {
      return fault('uncorrelated', 'Native brush resource response is uncorrelated' + (why ? ': ' + why : ''), { requestId: request.requestId });
    }
    function run(action) {
      var route = session.runResourceCommand || session.runFontResource;
      if (typeof route !== 'function') return Promise.reject(unavailable('resource_route_unavailable', 'Native resource command route is unavailable'));
      var started = false, queued;
      try { owner(); } catch (error) { return Promise.reject(error); }
      // A rejection before the action ran is the shared route's own refusal (opening, synchronizing, inactive): typed busy.
      try { queued = route.call(session, function (ctx) { started = true; return action(ctx); }); } catch (error) { return Promise.reject(error); }
      return Promise.resolve(queued).catch(function (error) {
        if (started || (error && typeof error.code === 'string')) throw error;
        throw unavailable('native_project_busy', 'Native brush resource route is busy or unavailable: ' + (error && error.message || error));
      });
    }
    function build(operation, pin, payload, expectedRevision) {
      try { Contract.payload(operation, payload); } catch (bad) { throw invalid(bad.message); }
      var request = { apiVersion: 2, requestId: session.requestId('brush'), instanceId: pin.instanceId, documentId: pin.documentId, operation: operation, payload: payload };
      if (expectedRevision !== undefined) request.expectedRevision = expectedRevision;
      return request;
    }

    // The closed payload shape, checked before anything with a side effect (a staged file, a catalog entry) happens.
    function precheck(brushId, assetId, size) {
      try {
        Contract.payload(Contract.PUBLISH, { command: 'object.brush.bitmap.publish', documentSnapshotId: 'x', assetId: assetId, brushId: brushId, width: size.width, height: size.height });
      } catch (bad) { throw invalid(bad.message); }
    }

    // ---- command: publish ----
    // A publication changes no document revision, so a lost reply cannot be told apart from a lost request by the revision:
    // an ambiguous outcome is always reported as indeterminate and replayed with the SAME request id (the owner returns the
    // stored receipt or executes once).
    async function send(ctx, request, key, sent) {
      try { return await ctx.dispatch(request); } catch (first) {
        sent.second = true;
        try { return await ctx.dispatch(request); } catch (second) {
          await resync(ctx);
          throw fault('brush_publish_indeterminate', 'Native brush publication outcome is indeterminate',
            { requestId: request.requestId, replay: function () { return replay(request, key); } });
        }
      }
    }
    // A replay and the answer to a second send are the owner's STORED receipt; any edit since moved the session past it, so
    // they reconcile WITHOUT the revision equality and never wedge the session on a legitimate stored answer.
    async function reconcile(ctx, request, revision, late) {
      try { return await (late ? ctx.reconcile() : ctx.reconcile(revision)); } catch (error) {
        if (error && typeof error.code === 'string') throw error;
        throw unavailable('reconcile_failed', 'Native brush session could not be reconciled: ' + (error && error.message || error));
      }
    }
    async function resync(ctx) {
      try { await ctx.reconcile(); } catch (_) { /* the session fences itself; the typed error below still reaches the caller */ }
    }
    async function settle(ctx, request, key, late) {
      var sent = { second: false };
      var response = await send(ctx, request, key, sent);
      late = late || sent.second;
      if (!response || response.apiVersion !== 2 || response.requestId !== request.requestId || typeof response.ok !== 'boolean') {
        await resync(ctx);
        throw uncorrelated(request);
      }
      if (!response.ok) { await reconcile(ctx, request, response.contentRevision, late); throw refusal(response, request); }
      try { Contract.result(request.operation, response.result); Contract.correlate(request, response); } catch (bad) {
        await resync(ctx);
        throw uncorrelated(request, bad.message);
      }
      await reconcile(ctx, request, response.contentRevision, late);
      return Object.freeze(Object.assign({ requestId: request.requestId, contentRevision: response.contentRevision }, response.result));
    }
    function replay(request, key) {
      return run(function (ctx) {
        if (ownerKey(owner()) !== key) throw fault('stale_owner', 'Native project changed before the replay', { requestId: request.requestId });
        return settle(ctx, request, key, true);
      });
    }
    function publishAsset(brushId, assetId, dimensions) {
      var checked;
      try { checked = checkSize(dimensions && dimensions.width, dimensions && dimensions.height); precheck(brushId, assetId, checked); } catch (error) { return Promise.reject(error); }
      return run(function (ctx) {
        // The pin is read at execution, so a queued call never sends a revision or snapshot captured at click time.
        var pin = owner();
        var request = build(Contract.PUBLISH, pin, {
          command: 'object.brush.bitmap.publish', documentSnapshotId: pin.documentSnapshotId, assetId: assetId, brushId: brushId,
          width: checked.width, height: checked.height,
        }, pin.contentRevision);
        return settle(ctx, request, ownerKey(pin), false);
      });
    }

    // ---- queries ----
    async function ask(ctx, pin, key, payload) {
      var request = build(Contract.QUERY, pin, payload);
      var response;
      try { response = await ctx.dispatch(request); } catch (_) {
        throw unavailable('transport_failure', 'Native brush read could not be delivered');
      }
      if (!response || response.apiVersion !== 2 || response.requestId !== request.requestId || typeof response.ok !== 'boolean') throw uncorrelated(request);
      if (!response.ok) throw refusal(response, request);
      try { Contract.result(Contract.QUERY, response.result); Contract.correlate(request, response); } catch (bad) { throw uncorrelated(request, bad.message); }
      var after = session.info();
      if (!after || ownerKey(after) !== key || after.contentRevision !== pin.contentRevision || after.documentSnapshotId !== pin.documentSnapshotId) {
        throw fault('stale_snapshot', 'Native project moved while the brush read was running', { requestId: request.requestId });
      }
      return response.result;
    }
    function inspect(brushId) {
      return run(async function (ctx) {
        var pin = owner();
        var result = await ask(ctx, pin, ownerKey(pin), { action: 'inspect', atRevision: pin.contentRevision, documentSnapshotId: pin.documentSnapshotId, brushId: brushId });
        return Object.freeze(Object.assign({}, result.brush));
      });
    }
    function entryOf(raw) {
      return Object.freeze({
        sourceObjectIndex: raw.sourceObjectIndex, layerUid: raw.layerUid, strokeId: raw.strokeId, brushId: raw.brushId,
        ink: raw.ink ? Object.freeze(Object.assign({}, raw.ink)) : null,
        unavailable: raw.unavailable ? raw.unavailable.code : null,
      });
    }
    // The brush part of the scene pinned to the current snapshot and frame, in the order the owner reports it (the packet's
    // numeric order plan). An unavailable ink is kept as a typed entry: it is never dropped from the report.
    function readScene(options) {
      options = options || {};
      var frame = options.frame, contextId = options.contextId === undefined ? 'scene-root' : options.contextId;
      try { Contract.payload(Contract.QUERY, { action: 'scene', atRevision: 0, documentSnapshotId: 'x', contextId: contextId, frame: frame, offset: 0, limit: 1 }); } catch (bad) {
        return Promise.reject(invalid(bad.message));
      }
      return run(async function (ctx) {
        var pin = owner(), key = ownerKey(pin), entries = [], seen = new Set(), resources = new Map(), total = null, offset = 0;
        while (offset !== null) {
          var page = await ask(ctx, pin, key, { action: 'scene', atRevision: pin.contentRevision, documentSnapshotId: pin.documentSnapshotId, contextId: contextId, frame: frame, offset: offset, limit: 1 });
          if (total !== null && page.totalEntries !== total) throw uncorrelated({ requestId: 'scene' }, 'entry total changed between pages');
          total = page.totalEntries;
          page.entries.forEach(function (raw) {
            if (seen.has(raw.sourceObjectIndex)) throw fault('scene_duplicate_claim', 'The scene claims one record twice', { sourceObjectIndex: raw.sourceObjectIndex });
            seen.add(raw.sourceObjectIndex);
            if (raw.ink) {
              var held = resources.get(raw.brushId);
              if (held && ['version', 'width', 'height', 'bytes'].some(function (k) { return held[k] !== raw.ink[k]; })) {
                throw fault('scene_resource_conflict', 'One brush id is declared at two versions in one scene', { brushId: raw.brushId });
              }
              resources.set(raw.brushId, raw.ink);
            }
            entries.push(entryOf(raw));
          });
          offset = page.nextOffset;
        }
        var report = Object.freeze({
          pin: Object.freeze({ instanceId: pin.instanceId, documentId: pin.documentId, lifecycleGeneration: pin.lifecycleGeneration, contentRevision: pin.contentRevision, documentSnapshotId: pin.documentSnapshotId, contextId: contextId, frame: frame }),
          entries: Object.freeze(entries),
          resources: Object.freeze(Array.from(resources.keys()).sort(Contract.compareUtf8).map(function (id) { return Object.freeze({ brushId: id, version: resources.get(id).version }); })),
          unavailable: Object.freeze(entries.filter(function (e) { return e.unavailable !== null; })),
          complete: entries.every(function (e) { return e.unavailable === null; }),
        });
        issued.add(report);
        return report;
      });
    }

    // canvas -> PNG bytes -> host byte route -> asset catalog -> publish. Every port is checked before the first side effect.
    async function publishCanvas(brushId, canvas) {
      var size = checkSize(canvas && canvas.width, canvas && canvas.height);
      precheck(brushId, 'x', size);
      var assets = typeof ports.assets === 'function' ? ports.assets() : ports.assets;
      if (typeof ports.stage !== 'function') throw unavailable('brush_stage_route_unavailable', 'Native brush byte route is unavailable');
      if (!assets || typeof assets.attachPath !== 'function') throw unavailable('asset_adapter_unavailable', 'Native asset catalog adapter is unavailable');
      var bytes = await (typeof ports.encodePng === 'function' ? ports.encodePng : encodeWithToBlob)(canvas);
      var declared = Contract.pngSize(bytes);
      if (!declared) throw invalid('The encoder returned bytes that are not a PNG');
      if (declared.width !== size.width || declared.height !== size.height) {
        throw fault('invalid_request', 'The PNG is ' + declared.width + 'x' + declared.height + ' but the canvas is ' + size.width + 'x' + size.height, { reason: 'png_dimension_mismatch' });
      }
      var path = await ports.stage(bytes, { brushId: brushId, width: size.width, height: size.height });
      if (typeof path !== 'string' || !path.length) throw unavailable('brush_stage_failed', 'The host byte route returned no file path');
      var attached = await assets.attachPath(path);
      if (!attached || typeof attached.assetId !== 'string' || !attached.assetId) {
        // An exact duplicate of a held asset has no new id to derive; the caller names it with publishAsset.
        throw fault('asset_id_unavailable', 'The catalog receipt names no asset', { reason: 'duplicate_attach_without_id' });
      }
      try {
        var published = await publishAsset(brushId, attached.assetId, size);
        return Object.freeze({ brushId: brushId, assetId: attached.assetId, attach: attached, publish: published });
      } catch (error) {
        // The asset is already in the catalog: report it so the caller retries only the publication.
        throw Object.assign(error, { assetId: attached.assetId });
      }
    }

    // Re-publish every bake the scene reports as `ink-missing`: one attempt per brush id, in report order, a typed outcome each.
    async function republishMissing(report, supply) {
      if (!issued.has(report)) throw invalid('republishMissing takes a report produced by readScene');
      if (typeof supply !== 'function') throw invalid('republishMissing needs a canvas supplier');
      var pin = owner();
      if (ownerKey(pin) !== ownerKey(report.pin) || pin.contentRevision !== report.pin.contentRevision || pin.documentSnapshotId !== report.pin.documentSnapshotId) {
        throw fault('stale_snapshot', 'The report belongs to an earlier snapshot of the native project');
      }
      var done = new Set(), outcomes = [];
      for (var i = 0; i < report.unavailable.length; i++) {
        var entry = report.unavailable[i];
        if (entry.unavailable !== 'ink-missing' || done.has(entry.brushId)) continue;
        done.add(entry.brushId);
        try {
          var canvas = await supply(entry);
          if (!canvas) { outcomes.push(Object.freeze({ brushId: entry.brushId, ok: false, code: 'no_bake', reason: 'supplier_returned_nothing' })); continue; }
          outcomes.push(Object.freeze({ brushId: entry.brushId, ok: true, receipt: await publishCanvas(entry.brushId, canvas) }));
        } catch (error) {
          outcomes.push(Object.freeze({ brushId: entry.brushId, ok: false, code: error && error.code || 'unavailable', reason: error && error.reason || null, error: error }));
        }
      }
      return Object.freeze(outcomes);
    }

    return Object.freeze({ publishAsset: publishAsset, publishCanvas: publishCanvas, inspect: inspect, readScene: readScene, republishMissing: republishMissing });
  }

  var bindings = new WeakMap();
  // One binding per host; the host byte route and the asset adapter are read at CALL time from the latest options, so a first
  // caller without them must not pin an unrouted binding once Main registers the route.
  function desktop(host, options) {
    var module = host.NemoNativeObjectProject, session = module && module.desktop(host);
    if (!session || !session.blocksLegacy()) return null;
    var entry = bindings.get(host);
    if (!entry) {
      entry = { stage: null, encodePng: null };
      // Accessors, so a route Main registers later is seen by a binding created earlier. The asset adapter keeps its own
      // host-route gate: this client never opens it.
      entry.binding = create(session, {
        get stage() { return entry.stage; },
        get encodePng() { return entry.encodePng; },
        assets: function () { var raster = host.NemoNativeObjectRasterAsset; return raster && raster.desktop(host); },
      });
      bindings.set(host, entry);
    }
    if (options && typeof options.stage === 'function') entry.stage = options.stage;
    if (options && typeof options.encodePng === 'function') entry.encodePng = options.encodePng;
    return entry.binding;
  }
  return Object.freeze({ create: create, desktop: desktop, checkSize: checkSize });
}));
