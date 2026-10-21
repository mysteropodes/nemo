/* CustomBrush presentation use case. Ports supply the existing session, queue and native SDK. */
(function (root, factory) {
  var api = factory(typeof module === 'object' && module.exports ? require('./native-custom-brush-contract.js') : root.NemoNativeCustomBrushContract);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeCustomBrushDesktop = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Contract) {
  'use strict';
  var keys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  function same(a, b) { return !!a && !!b && keys.every(function (key) { return a[key] === b[key]; }); }
  function freeze(value) {
    if (value && typeof value === 'object') { Object.keys(value).forEach(function (key) { freeze(value[key]); }); Object.freeze(value); }
    return value;
  }
  function fault(code) { var error = new Error('Native customBrush is unavailable or unconfirmed'); error.code = code || 'unavailable'; return error; }
  function presented(value, info) {
    return value && same(value, info) && same(value.presentation, info) && value.presentation.owner === 'native' && value.presentation.status === 'presented';
  }
  function create(ports) {
    var generation = 0, retired = false, cache = null, proofs = new WeakMap();
    function invalidate() { generation++; cache = null; }
    function capture() {
      var session = ports.currentSession(), info = session && session.info();
      if (retired || !session || !session.isActive() || !info || typeof session.runResourceCommand !== 'function' ||
          typeof session.readSubscriptionGeneration !== 'function' || !presented(session.readPresentedObjectSession(), info) ||
          ports.available(session, info) !== true) throw fault();
      var pin = {}; keys.forEach(function (key) { pin[key] = info[key]; });
      return { session: session, info: Object.freeze(pin), ticket: session.readSubscriptionGeneration(), generation: generation };
    }
    function current(stamp) {
      return !retired && stamp.generation === generation && ports.currentSession() === stamp.session && stamp.session.isActive() &&
        same(stamp.session.info(), stamp.info) && stamp.session.readSubscriptionGeneration() === stamp.ticket;
    }
    function guard(stamp) { if (!current(stamp)) throw fault('stale_revision'); }
    function ready(stamp, context) {
      guard(stamp);
      if (!context || !presented(context.readPresentedObjectSession(), stamp.info)) throw fault();
    }
    function response(request, value) {
      if (!value || value.apiVersion !== 2 || typeof value.ok !== 'boolean') throw fault();
      Contract.correlate(request, value);
      var encoded = JSON.stringify(value);
      var bytes = typeof TextEncoder === 'function' ? new TextEncoder().encode(encoded).length : unescape(encodeURIComponent(encoded)).length;
      if (bytes > 4096) throw fault();
      if (!value.ok) { var error=fault(value.error&&value.error.code);error.correlated=true;throw error; }
      return value;
    }
    async function query(stamp, context, selector) {
      ready(stamp, context);
      var pin = stamp.info;
      var request = { apiVersion: 2, requestId: stamp.session.requestId('customBrush-read'), instanceId: pin.instanceId,
        documentId: pin.documentId, operation: Contract.READ, payload: { atRevision: pin.contentRevision,
          documentSnapshotId: pin.documentSnapshotId, contextId: 'scene-root', selector: selector } };
      Contract.payload(request.operation, request.payload);
      var raw=await context.dispatch(request),reply;
      try { reply=response(request,raw); }
      catch(error) {
        guard(stamp);
        if(error.correlated===true&&selector.kind==='record'&&raw&&raw.ok===false&&raw.error&&raw.error.code==='unavailable'&&raw.contentRevision===pin.contentRevision)error.recordUnavailable=true;
        throw error;
      }
      guard(stamp);return reply.result.result;
    }
    async function collect(stamp,context) {
      var settings=await query(stamp,context,{kind:'settings'}),items=[],records=[],cursor=0,seen=new Set();
      do {
        var page=await query(stamp,context,{kind:'catalogPage',cursor:cursor,limit:16});
        if(page.customBrushPresetsPresent!==settings.customBrushPresetsPresent||page.presetCount!==settings.presetCount)throw fault();
        page.items.forEach(function(item){if(seen.has(item.id))throw fault();seen.add(item.id);items.push(item);});cursor=page.nextCursor;
      }while(cursor!==null);
      if(items.length!==settings.presetCount)throw fault();
      for(var i=0;i<items.length;i++) {
        try {
          var record=await query(stamp,context,{kind:'record',id:items[i].id});
          if(Object.prototype.hasOwnProperty.call(record.preset,'label')!==Object.prototype.hasOwnProperty.call(items[i],'label')||record.preset.label!==items[i].label)throw fault();
          records.push(record);
        }
        catch(error) { if(!error.recordUnavailable)throw error;records.push({id:items[i].id,unavailable:true}); }
      }
      return freeze({settings:settings,items:items,records:records});
    }
    async function refresh(expected) {
      cache = null; var stamp = capture();
      if (expected && !same(expected, stamp.info)) throw fault('stale_revision');
      var data = await stamp.session.runResourceCommand(async function (context) {
        return collect(stamp,context);
      });
      guard(stamp);
      if (!presented(stamp.session.readPresentedObjectSession(), stamp.info) || ports.available(stamp.session, stamp.info) !== true) throw fault();
      var projection = freeze({ info: stamp.info, catalog: data });
      if (typeof ports.publish === 'function' && ports.publish(projection) !== true) throw fault();
      guard(stamp); proofs.set(projection, stamp); cache = { stamp: stamp, value: projection }; return projection;
    }
    function read() {
      try {
        if (cache && current(cache.stamp)) {
          if (presented(cache.stamp.session.readPresentedObjectSession(), cache.stamp.info) &&
              ports.available(cache.stamp.session, cache.stamp.info) === true) return cache.value;
          return null; // The existing queue may temporarily hide presentation without changing native metadata.
        }
      } catch (_) {}
      cache = null; return null;
    }
    function readForCommand(value) {
      var stamp = value && proofs.get(value);
      // Only a previously issued immutable proof is usable by an already admitted queued command.
      // The command's own context still fences frame, tool, selection and viewport presentation.
      try { return stamp && current(stamp) ? value : null; } catch (_) { return null; }
    }
    async function edit(value, expected) {
      Contract.action(value);
      var intent = freeze(JSON.parse(JSON.stringify(value))), shown = read();
      if (!expected || shown !== expected) throw fault('stale_revision');
      invalidate(); var stamp = capture(), receipt = null, after = null;
      if (!same(expected.info, stamp.info)) throw fault('stale_revision');
      try {
        await stamp.session.runResourceCommand(async function (context) {
          ready(stamp, context); var pin = stamp.info;
          var request = { apiVersion: 2, requestId: stamp.session.requestId('customBrush-edit'), instanceId: pin.instanceId,
            documentId: pin.documentId, operation: Contract.EDIT, expectedRevision: pin.contentRevision,
            payload: { command: 'object.project.customBrushPreset.edit', action: intent } };
          Contract.payload(request.operation, request.payload);
          receipt = response(request, await context.dispatch(request)); guard(stamp);
          after = await context.reconcile(receipt.contentRevision);
          if (!after || after.instanceId !== pin.instanceId || after.documentId !== pin.documentId || after.lifecycleGeneration !== pin.lifecycleGeneration ||
              after.contentRevision !== receipt.contentRevision || typeof after.documentSnapshotId !== 'string' ||
              !same(stamp.session.info(), after) || stamp.session.readSubscriptionGeneration() !== stamp.ticket ||
              (!receipt.result.applied && after.documentSnapshotId !== pin.documentSnapshotId) ||
              (receipt.result.applied && after.documentSnapshotId === pin.documentSnapshotId)) throw fault();
        });
        if (retired || stamp.generation !== generation || ports.currentSession() !== stamp.session) throw fault();
        var projection = await refresh(after);
        return Object.freeze({ status: receipt.result.applied ? 'applied-presented' : 'unchanged', applied: receipt.result.applied, projection: projection });
      } catch (error) {
        cache = null;
        return Object.freeze({ status: receipt ? 'applied-unconfirmed' : 'unconfirmed', applied: receipt ? receipt.result.applied : null, code: error && error.code || 'unavailable' });
      }
    }
    return Object.freeze({ refresh: refresh, read: read, readForCommand: readForCommand, edit: edit, invalidate: invalidate,
      dispose: function () { retired = true; invalidate(); } });
  }
  return Object.freeze({ create: create });
}));
