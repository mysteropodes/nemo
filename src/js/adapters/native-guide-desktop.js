/* Guide presentation use case. Ports supply the existing session, queue and native SDK. */
(function (root, factory) {
  var api = factory(typeof module === 'object' && module.exports ? require('./native-guide-contract.js') : root.NemoNativeGuideContract);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeGuideDesktop = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Contract) {
  'use strict';
  var keys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  function same(a, b) { return !!a && !!b && keys.every(function (key) { return a[key] === b[key]; }); }
  function freeze(value) {
    if (value && typeof value === 'object') { Object.keys(value).forEach(function (key) { freeze(value[key]); }); Object.freeze(value); }
    return value;
  }
  function fault(code) { var error = new Error('Native guide is unavailable or unconfirmed'); error.code = code || 'unavailable'; return error; }
  function presented(value, info) {
    return value && same(value, info) && same(value.presentation, info) && value.presentation.owner === 'native' && value.presentation.status === 'presented';
  }
  function create(ports) {
    var generation = 0, retired = false, cache = null;
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
    // Keep action identity narrow; validate full current metadata for UI readiness.
    function available(stamp) {
      var info = stamp.session.info();
      return same(info, stamp.info) && ports.available(stamp.session, info) === true;
    }
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
      if (!value.ok) throw fault(value.error && value.error.code);
      return value;
    }
    async function query(stamp, context, selector) {
      ready(stamp, context);
      var pin = stamp.info;
      var request = { apiVersion: 2, requestId: stamp.session.requestId('guide-read'), instanceId: pin.instanceId,
        documentId: pin.documentId, operation: Contract.READ, payload: { atRevision: pin.contentRevision,
          documentSnapshotId: pin.documentSnapshotId, contextId: 'scene-root', selector: selector } };
      Contract.payload(request.operation, request.payload);
      var reply = response(request, await context.dispatch(request));
      guard(stamp); return reply.result.result;
    }
    async function collect(stamp, context, settings, axis) {
      var positions = [], cursor = 0, count = settings[axis + 'Count'];
      do {
        var page = await query(stamp, context, { kind: 'axisPage', axis: axis, cursor: cursor, limit: 16 });
        if (page.guideCount !== count || page.guidesPresent !== settings.guidesPresent ||
            page.axisPresent !== settings[axis + 'Present']) throw fault();
        page.items.forEach(function (item) { positions.push(item.position); }); cursor = page.nextCursor;
      } while (cursor !== null);
      if (positions.length !== count) throw fault(); return positions;
    }
    async function refresh(expected) {
      cache = null; var stamp = capture();
      if (expected && !same(expected, stamp.info)) throw fault('stale_revision');
      var data = await stamp.session.runResourceCommand(async function (context) {
        var settings = await query(stamp, context, { kind: 'settings' });
        var h = await collect(stamp, context, settings, 'h'), v = await collect(stamp, context, settings, 'v');
        return freeze({ settings: settings, h: h, v: v });
      });
      guard(stamp);
      if (!presented(stamp.session.readPresentedObjectSession(), stamp.info) || !available(stamp)) throw fault();
      var projection = freeze({ info: stamp.info, guides: data });
      if (typeof ports.publish === 'function' && ports.publish(projection) !== true) throw fault();
      guard(stamp); cache = { stamp: stamp, value: projection }; return projection;
    }
    function read() {
      try {
        if (cache && current(cache.stamp)) {
          if (presented(cache.stamp.session.readPresentedObjectSession(), cache.stamp.info) &&
            available(cache.stamp)) return cache.value;
          return null; // The existing queue may temporarily hide presentation without changing native metadata.
        }
      } catch (_) {}
      cache = null; return null;
    }
    async function edit(value, expected) {
      var intent = Contract.action(value), shown = read();
      if (!expected || shown !== expected) throw fault('stale_revision');
      invalidate(); var stamp = capture(), receipt = null, after = null;
      if (!same(expected.info, stamp.info)) throw fault('stale_revision');
      try {
        await stamp.session.runResourceCommand(async function (context) {
          ready(stamp, context); var pin = stamp.info;
          var request = { apiVersion: 2, requestId: stamp.session.requestId('guide-edit'), instanceId: pin.instanceId,
            documentId: pin.documentId, operation: Contract.EDIT, expectedRevision: pin.contentRevision,
            payload: { command: 'object.project.guide.edit', action: intent } };
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
    return Object.freeze({ refresh: refresh, read: read, edit: edit, invalidate: invalidate,
      dispose: function () { retired = true; invalidate(); } });
  }
  return Object.freeze({ create: create });
}));
