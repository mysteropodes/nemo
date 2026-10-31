/* Comments presentation use case. Ports supply the existing session, queue and native SDK. */
(function (root, factory) {
  var api = factory(typeof module === 'object' && module.exports ? require('./native-comments-contract.js') : root.NemoNativeCommentsContract);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeCommentsDesktop = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Contract) {
  'use strict';
  var keys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  function same(a, b) { return !!a && !!b && keys.every(function (key) { return a[key] === b[key]; }); }
  function freeze(value) {
    if (value && typeof value === 'object') { Object.keys(value).forEach(function (key) { freeze(value[key]); }); Object.freeze(value); }
    return value;
  }
  function fault(code) { var error = new Error('Native comments is unavailable or unconfirmed'); error.code = code || 'unavailable'; return error; }
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
    // Identity stamps remain narrow; readiness receives same-pin full native metadata.
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
      var request = { apiVersion: 2, requestId: stamp.session.requestId('comments-read'), instanceId: pin.instanceId,
        documentId: pin.documentId, operation: Contract.READ, payload: { atRevision: pin.contentRevision,
          documentSnapshotId: pin.documentSnapshotId, contextId: 'scene-root', selector: selector } };
      Contract.payload(request.operation, request.payload);
      var reply = response(request, await context.dispatch(request));
      guard(stamp); return reply.result.result;
    }
    async function refresh(expected) {
      var stamp = capture(); cache = null;
      if (expected && !same(expected, stamp.info)) throw fault('stale_revision');
      var data = await (stamp.session.runResourceQuery || stamp.session.runResourceCommand).call(stamp.session, async function (context) {
        var settings = await query(stamp, context, { kind: 'settings' });
        var items = [], cursor = 0;
        do {
          var page = await query(stamp, context, { kind: 'commentsPage', cursor: cursor, limit: 16 });
          if (page.commentsPresent !== settings.commentsPresent || page.commentCount !== settings.commentCount) throw fault();
          page.items.forEach(function (item) {
            items.push(Object.freeze({ ordinal: item.ordinal, comment: Contract.comment(item.comment, false) }));
          });
          cursor = page.nextCursor;
        } while (cursor !== null);
        if (items.length !== settings.commentCount) throw fault();
        return freeze({ settings: settings, items: items });
      });
      guard(stamp);
      if (!presented(stamp.session.readPresentedObjectSession(), stamp.info) || !available(stamp)) throw fault();
      var projection = freeze({ info: stamp.info, comments: data });
      if (typeof ports.publish === 'function' && ports.publish(projection) !== true) throw fault();
      guard(stamp); cache = { stamp: stamp, value: projection }; return projection;
    }
    // #2372: a presentation hidden by a PENDING native command (queue-pending) is "cannot prove it right now", not "changed": read() stays null - it authorizes input,
    // so it never answers while work is pending - but the sealed projection is kept; an edit waits for the presentation (readSettled) instead of refusing as stale.
    function read(onRefusal) {
      try {
        if (cache && current(cache.stamp)) {
          var why = null, shown = cache.stamp.session.readPresentedObjectSession(function (c) { why = c; });
          if (presented(shown, cache.stamp.info) && available(cache.stamp)) return cache.value;
          if (typeof onRefusal === 'function') try { onRefusal(!shown && why === 'queue-pending' ? 'queue-pending' : 'presentation-mismatch'); } catch (_) {}
          return null; // The existing queue may temporarily hide presentation without changing native metadata.
        }
      } catch (_) {}
      cache = null; return null;
    }
    // #2372: the native queue hides the presentation while ANY command is pending (the previous edit's repaint, a hover read). An edit that finds it hidden
    // waits for it (bounded, native-presentation-wait.js) before judging the cache; a presentation that really changed still refuses as stale.
    function readSettled() {
      var first = read(), W = typeof globalThis === 'object' ? globalThis.NemoNativePresentationWait : null;
      if (first || !W || !cache) return Promise.resolve(first);
      return W.await(null, { read: read, holds: function () { return !!cache; } }).then(function (r) { return r.ready ? r.value : null; });
    }
    async function edit(value, expected, current) {
      var intent = Contract.action(value), shown = await readSettled();
      if (!expected || shown !== expected) throw fault('stale_revision');
      invalidate(); var stamp = capture(), receipt = null, after = null;
      if (!same(expected.info, stamp.info)) throw fault('stale_revision');
      try {
        await stamp.session.runResourceCommand(async function (context) {
          ready(stamp, context); var pin = stamp.info;
          var request = { apiVersion: 2, requestId: stamp.session.requestId('comments-edit'), instanceId: pin.instanceId,
            documentId: pin.documentId, operation: Contract.EDIT, expectedRevision: pin.contentRevision,
            payload: { command: 'object.project.comments.edit', action: intent } };
          Contract.payload(request.operation, request.payload);
          if (typeof current === 'function' && current(context) !== true) throw fault('stale_revision');
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
    return Object.freeze({ refresh: refresh, read: read, readSettled: readSettled, edit: edit, invalidate: invalidate,
      dispose: function () { retired = true; invalidate(); } });
  }
  return Object.freeze({ create: create });
}));
