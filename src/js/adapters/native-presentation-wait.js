/* Bounded wait for the native presentation (#2372). The native queue reports NO presentation while any command is pending - the app's own hover
 * read, the repaint of the previous edit, the press's own read - so `readPresentedObjectSession()` answers null with the reason 'queue-pending'
 * for a few frames. That is "cannot prove it right now", never "it changed": an action that needs the presented canvas must outlast the window
 * instead of dropping silently or refusing with a notice about something that did not happen.
 *
 *   NemoNativePresentationWait.await(root, ctx, { tries, ms })  one shared bounded wait. ctx: { read(onRefusal), holds(), ready(v), notice }.
 *       Resolves { ready: true, value } once read() is usable and holds() still does; { ready: false, status: 'changed' } the moment holds()
 *       is false (the tool, frame, pin or selection the action began with moved on: silent, the user has left); { ready: false, status:
 *       'refused', reason } when the presentation is unreadable for any reason other than the queue (a missing presentation, a different view or
 *       owner is a real refusal, never waited out); { ready: false, status: 'busy' } after the bounds, with ONE notice (ctx.notice).
 *   .busy(read)  true only when read() answers null for 'queue-pending' (a mid-gesture tick may then skip itself instead of ending the gesture).
 *   .until(root, session, ok, holds)  true once ok() holds, waiting out a busy queue first (an async action's own presented() check).
 *   .gate(root, o)  wraps a pointer handler: an event that needs the presentation while the queue is busy is held with the events that follow it
 *       (same pointer) and replayed in order through the handler once the presentation is readable, so a click or drag is neither lost nor
 *       half-started. Staleness is untouched: the replayed press runs the handler's own strict checks against what is readable then. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativePresentationWait = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var PENDING = 'queue-pending', TRIES = 16, MS = 60;
  function notice(root, text) { if (text && root && typeof root.showToast === 'function') try { root.showToast(text); } catch (_) {} }
  function sleep(root, ms) { return new Promise(function (done) { ((root && root.setTimeout) || setTimeout)(done, ms); }); }
  // The value and, when it is unreadable, why (the queue's reason code, or undefined for a reader that gives none).
  function sample(ctx) {
    var why;
    function got(value) { return { value: value, reason: why }; }
    var value = ctx.read(function (code) { why = code; });
    return value && typeof value.then === 'function' ? value.then(got) : got(value);
  }
  function usable(ctx, value) { return ctx.ready ? ctx.ready(value) : value !== null && value !== undefined; }
  function busy(read) {
    var why, value;
    try { value = read(function (code) { why = code; }); } catch (_) { return false; }
    return !value && why === PENDING;
  }
  async function wait(root, ctx, opts) {
    var tries = Math.max(1, (opts && opts.tries) || TRIES), ms = (opts && opts.ms) || MS, last;
    for (var attempt = 0; attempt < tries; attempt++) {
      if (attempt) await sleep(root, ms);
      if (ctx.holds && !ctx.holds()) return { ready: false, status: 'changed' };
      var seen;
      try { seen = await sample(ctx); } catch (_) { seen = { value: undefined, reason: 'exception' }; }
      if (ctx.holds && !ctx.holds()) return { ready: false, status: 'changed' };
      if (usable(ctx, seen.value)) return { ready: true, value: seen.value };
      last = seen.reason;
      if (last !== undefined && last !== PENDING) return { ready: false, status: 'refused', reason: last };
    }
    notice(root, ctx.notice);
    return { ready: false, status: 'busy', reason: last };
  }
  // True once ok() holds. When it does not at first, a presentation hidden only by the queue is waited out (bounded) and ok() is asked again; a presentation
  // that is readable and wrong, or unreadable for any other reason, is false at once. `holds()` ends the wait early when the action's context moved on.
  function until(root, session, ok, holds, opts) {
    if (ok()) return Promise.resolve(true);
    if (!session || typeof session.readPresentedObjectSession !== 'function') return Promise.resolve(false);
    return wait(root, { read: function (f) { return session.readPresentedObjectSession(f); }, holds: holds, notice: opts && opts.notice }, opts).then(function (r) { return r.ready && ok(); });
  }
  // o: { handle(kind, e), begins(kind, e) -> true when this event needs the presentation (a press that starts a gesture, or a move / release of one that already did), read(onRefusal), snapshot() -> the context token taken when the press is held, holds(token) -> whether that context still holds, notice, tries, ms, ends: ['cancel', 'lost'] }
  function gate(root, o) {
    var held = null, ends = o.ends || ['cancel', 'lost'];
    function pointer(e) { return e && e.pointerId; }
    function replay(events) { events.forEach(function (x) { try { o.handle(x[0], x[1]); } catch (_) {} }); }
    function handle(kind, e) {
      if (held) {
        if (ends.indexOf(kind) >= 0) { held = null; return o.handle(kind, e); }
        if (pointer(e) === held.pointer) { held.events.push([kind, e]); return true; }
        return o.handle(kind, e);
      }
      if (!busy(o.read) || !o.begins(kind, e)) return o.handle(kind, e); // the cheap busy test first: a handler's own event reads must stay in their original order
      var token = o.snapshot ? o.snapshot() : undefined, mine = held = { pointer: pointer(e), events: [[kind, e]] };
      if (e && e.stopImmediatePropagation) e.stopImmediatePropagation();
      if (e && e.preventDefault) e.preventDefault();
      wait(root, { read: o.read, holds: function () { return held === mine && (!o.holds || o.holds(token)); }, notice: o.notice },
        { tries: o.tries, ms: o.ms }).then(function (r) {
        if (held !== mine) return;
        held = null;
        // Ready, or refused for a reason other than the queue (the handler's own refusal then says why); a changed context or a busy give-up drops it.
        if (r.ready || r.status === 'refused') replay(mine.events);
      });
      return true;
    }
    return { handle: handle, waiting: function () { return !!held; }, cancel: function () { held = null; } };
  }
  return Object.freeze({ await: wait, busy: busy, gate: gate, until: until, PENDING: PENDING });
}));
