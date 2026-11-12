'use strict';
// #2372 presentation-busy-audit: the shared bounded wait for a native presentation that the queue hides while ANY command is pending (queue-pending).
const test = require('node:test'), assert = require('node:assert/strict');
const Wait = require('../src/js/adapters/native-presentation-wait.js');

// A fake session whose presentation is hidden (reason queue-pending) for `hidden` reads, then readable; `then` runs when it becomes readable.
function session(hidden, reason) {
  const log = { reads: 0 };
  return Object.assign(log, { readPresentedObjectSession(onRefusal) {
    log.reads++;
    if (log.reads <= hidden) { if (onRefusal) onRefusal(reason || 'queue-pending'); return null; }
    return { presentation: { frame: 0 } };
  } });
}
const root = () => { const toasts = []; return { toasts, showToast: (t) => toasts.push(t), setTimeout: (fn) => setImmediate(fn) }; };
const read = (s) => (f) => s.readPresentedObjectSession(f);

test('await resolves once the hidden presentation becomes readable, after the queue-pending window', async () => {
  const s = session(5), r = root();
  const out = await Wait.await(r, { read: read(s), holds: () => true, notice: 'busy' });
  assert.equal(out.ready, true);
  assert.equal(s.reads, 6, 'five hidden ticks, then one readable read');
  assert.deepEqual(r.toasts, []);
});

test('await stops the moment the context no longer holds, silently', async () => {
  const s = session(100), r = root();
  let holds = true;
  const waiting = Wait.await(r, { read: read(s), holds: () => holds, notice: 'busy' });
  await new Promise((done) => setImmediate(done));
  holds = false;
  const out = await waiting;
  assert.deepEqual([out.ready, out.status], [false, 'changed']);
  assert.deepEqual(r.toasts, [], 'the user moved on: no notice');
  assert.ok(s.reads < 10);
});

test('await never waits out a refusal that is not the queue: a missing or different presentation refuses at once', async () => {
  for (const reason of ['presentation-missing', 'view-mismatch', 'owner-mismatch', 'synchronizing', 'inactive']) {
    const s = session(100, reason), r = root();
    const out = await Wait.await(r, { read: read(s), holds: () => true, notice: 'busy' });
    assert.deepEqual([out.ready, out.status, out.reason], [false, 'refused', reason]);
    assert.equal(s.reads, 1, reason + ' is not waited out');
    assert.deepEqual(r.toasts, []);
  }
});

test('await gives up after its bound with exactly ONE notice', async () => {
  const s = session(1000), r = root();
  const out = await Wait.await(r, { read: read(s), holds: () => true, notice: 'busy' }, { tries: 4, ms: 1 });
  assert.deepEqual([out.ready, out.status], [false, 'busy']);
  assert.equal(s.reads, 4);
  assert.deepEqual(r.toasts, ['busy']);
});

test('await accepts an async reader and a custom readiness (null is a real answer, undefined is not)', async () => {
  let n = 0;
  const out = await Wait.await(root(), { read: async () => (++n < 3 ? undefined : null), ready: (v) => v !== undefined });
  assert.deepEqual([out.ready, out.value, n], [true, null, 3]);
});

test('busy is true only for a presentation hidden by the queue', () => {
  assert.equal(Wait.busy(read(session(1))), true);
  assert.equal(Wait.busy(read(session(1, 'view-mismatch'))), false);
  assert.equal(Wait.busy(read(session(0))), false);
  assert.equal(Wait.busy(() => { throw new Error('x'); }), false);
});

test('until asks again after the wait and stays false when the readable presentation is wrong', async () => {
  const s = session(3);
  assert.equal(await Wait.until(root(), s, () => s.reads > 3, () => true), true);
  const wrong = session(3);
  assert.equal(await Wait.until(root(), wrong, () => false, () => true), false);
  const other = session(100, 'owner-mismatch');
  assert.equal(await Wait.until(root(), other, () => false, () => true), false);
  assert.equal(other.reads, 1);
});

function handler() {
  const calls = [];
  return { calls, handle(kind, e) { calls.push([kind, e.id]); return true; } };
}
const ev = (id, pointerId = 1) => ({ id, pointerId, stopped: false, stopImmediatePropagation() { this.stopped = true; }, preventDefault() {} });

test('gate passes a press straight through when the presentation is readable', () => {
  const h = handler(), s = session(0), g = Wait.gate(root(), { handle: h.handle, begins: () => true, read: read(s), holds: () => true });
  assert.equal(g.handle('down', ev('d')), true);
  assert.deepEqual(h.calls, [['down', 'd']]);
  assert.equal(g.waiting(), false);
});

test('gate holds a press while the queue is busy and replays it with the events that followed, in order, once', async () => {
  const h = handler(), s = session(4), r = root(), g = Wait.gate(r, { handle: h.handle, begins: () => true, read: read(s), holds: () => true, notice: 'busy' });
  const down = ev('d');
  assert.equal(g.handle('down', down), true);
  assert.equal(down.stopped, true, 'the press is consumed so no legacy handler sees it');
  assert.equal(g.waiting(), true);
  g.handle('move', ev('m'));
  g.handle('up', ev('u'));
  g.handle('move', ev('other-pointer', 2)); // another pointer is not part of the held gesture
  assert.deepEqual(h.calls, [['move', 'other-pointer']]);
  for (let i = 0; i < 20 && g.waiting(); i++) await new Promise((done) => setImmediate(done));
  assert.deepEqual(h.calls.slice(1), [['down', 'd'], ['move', 'm'], ['up', 'u']]);
  assert.equal(g.waiting(), false);
  assert.deepEqual(r.toasts, []);
});

test('gate drops a held press when the context changed during the wait, and when the gesture is cancelled', async () => {
  let tool = 'a';
  const h = handler(), s = session(100), g = Wait.gate(root(), { handle: h.handle, begins: () => true, read: read(s), holds: () => tool === 'a' });
  g.handle('down', ev('d'));
  tool = 'b';
  for (let i = 0; i < 20 && g.waiting(); i++) await new Promise((done) => setImmediate(done));
  assert.deepEqual(h.calls, []);
  assert.equal(g.waiting(), false);
  const h2 = handler(), s2 = session(100), g2 = Wait.gate(root(), { handle: h2.handle, begins: () => true, read: read(s2), holds: () => true });
  g2.handle('down', ev('d'));
  g2.handle('cancel', ev('c'));
  await new Promise((done) => setImmediate(done));
  assert.deepEqual(h2.calls, [['cancel', 'c']]);
  assert.equal(g2.waiting(), false);
});

test('gate lets the handler give its own refusal when the presentation is refused for a reason other than the queue', async () => {
  const h = handler(), s = session(100, 'view-mismatch'), g = Wait.gate(root(), { handle: h.handle, begins: () => true, read: read(s), holds: () => true });
  assert.equal(g.handle('down', ev('d')), true, 'a plain refusal is the handler\'s to word');
  assert.deepEqual(h.calls, [['down', 'd']]);
});
