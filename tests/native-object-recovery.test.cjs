'use strict';
// #2360 R2: idle-timed recovery writer scheduler, driven by a fake clock and fake ports.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const Recovery = require('../src/js/adapters/native-object-recovery.js');

const hex = (n) => n.toString(16).padStart(64, '0');
const flush = () => new Promise(setImmediate);
function deferred() { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; }

function harness(options = {}) {
  let now = 1_000_000, timerId = 0, serial = 0;
  const timers = [];
  const h = { writes: [], notices: [], duration: 0, reply: null, gate: null, fail: null, pinThrows: false,
    rev: 1, baseline: hex(0), documentId: 'document-1', generation: 1, busy: false, presented: true, active: true, blocked: false,
    slot: { path: null, name: 'Untitled' }, ...options };
  h.edit = (to) => { h.rev = to === undefined ? h.rev + 1 : to; };
  h.clock = () => now;
  const info = () => ({ apiVersion: 2, instanceId: 'instance', documentId: h.documentId, contentRevision: h.rev,
    documentSnapshotId: 'snapshot-' + h.rev, lifecycleGeneration: h.generation, documentDigest: hex(h.rev), byteCount: 1000,
    layers: [] });
  const ports = {
    now: () => now,
    setTimer: (fn, ms) => { timers.push({ id: ++timerId, at: now + ms, fn }); return timerId; },
    clearTimer: (id) => { const i = timers.findIndex((t) => t.id === id); if (i >= 0) timers.splice(i, 1); },
    pin: () => { if (h.pinThrows) throw new Error('pin failed'); return { active: h.active, info: info(), busy: h.busy, presented: h.presented, reference: h.baseline }; },
    blocked: () => h.blocked, slot: () => h.slot, notice: (m) => h.notices.push(m),
    requestId: (kind) => 'project-' + kind + '-' + (++serial), hash: () => 'h4sh', randomHex: () => 'deadbeef',
    supersedes: () => h.supersedes || null,
    write: async (command, request) => {
      assert.equal(command, 'nemo_native_object_recovery_write');
      h.writes.push({ at: now, request });
      if (h.gate) await h.gate.promise;
      if (h.fail) throw new Error(h.fail);
      now += h.duration;
      if (h.reply) return h.reply(request);
      return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId,
        atRevision: request.atRevision, documentSnapshotId: request.documentSnapshotId, slotKey: request.slotKey,
        published: true, byteCount: 1000, savedAtMs: now, cleanupPending: false, supersededRemoved: false };
    }
  };
  h.scheduler = Recovery.create(ports);
  // Advance the fake clock, firing due timers (and letting their async work settle) in order.
  h.advance = async (ms) => {
    const target = now + ms;
    for (;;) {
      timers.sort((a, b) => a.at - b.at);
      if (!timers.length || timers[0].at > target) break;
      const timer = timers.shift(); now = Math.max(now, timer.at); timer.fn(); await flush(); await flush();
    }
    now = Math.max(now, target);
  };
  h.timers = timers;
  return h;
}
const S = 1000;

test('no write while the project is clean or the digest equals the baseline or the last recovered digest', async () => {
  const h = harness({ rev: 0 }); h.scheduler.start();
  await h.advance(120 * S);
  assert.equal(h.writes.length, 0);
  h.edit(); await h.advance(10 * S);
  assert.equal(h.writes.length, 1);
  await h.advance(120 * S);
  assert.equal(h.writes.length, 1, 'unchanged digest after a recovered write is not rewritten');
  h.edit(0); await h.advance(60 * S);
  assert.equal(h.writes.length, 1, 'edit undone back to the baseline digest is clean');
});

test('first write waits for 5 s of idle-dirty, observed on the 5 s tick chain', async () => {
  const h = harness({ rev: 0 }); h.scheduler.start();
  await h.advance(5 * S); h.edit();
  await h.advance(5 * S);
  assert.equal(h.writes.length, 0, 'the tick that first sees the dirty state only starts the idle clock');
  await h.advance(5 * S);
  assert.equal(h.writes.length, 1);
  const r = h.writes[0].request;
  assert.equal(r.atRevision, 1); assert.equal(r.documentSnapshotId, 'snapshot-1'); assert.equal(r.documentDigest, hex(1));
});

test('steady state: not before 30 s after the last write; 50 revisions shorten that to 10 s', async () => {
  const h = harness({ rev: 0 }); h.scheduler.start();
  h.edit(); await h.advance(10 * S);
  assert.equal(h.writes.length, 1);
  h.edit(); await h.advance(20 * S);
  assert.equal(h.writes.length, 1, '20 s after the write: not yet');
  await h.advance(10 * S);
  assert.equal(h.writes.length, 2, '30 s after the write: due');
  // 49 revisions: the gap rule still applies; 50: 10 s is enough.
  h.edit(h.rev + 49); await h.advance(15 * S); assert.equal(h.writes.length, 2);
  h.edit(h.rev + 1); await h.advance(5 * S);
  assert.equal(h.writes.length, 3, '50 revisions and 10 s since the previous write');
  assert.equal(h.writes[2].at - h.writes[1].at >= 10 * S && h.writes[2].at - h.writes[1].at < 30 * S, true);
});

test('adaptive backoff: never closer than 20x the last write duration, for both rules', async () => {
  const h = harness({ rev: 0, duration: 4 * S }); h.scheduler.start();
  h.edit(); await h.advance(10 * S);
  assert.equal(h.writes.length, 1);
  assert.equal(h.scheduler.state().lastDuration, 4 * S);
  h.edit(h.rev + 60); await h.advance(60 * S);
  assert.equal(h.writes.length, 1, '20 x 4 s = 80 s floor beats both the 30 s and the 50-revision rule');
  await h.advance(25 * S);
  assert.equal(h.writes.length, 2);
  assert.ok(h.writes[1].at - h.writes[0].at >= 80 * S);
});

test('a write in flight suppresses every other write and ticks keep running', async () => {
  const h = harness({ rev: 0 }); h.scheduler.start();
  h.edit(); h.gate = deferred();
  await h.advance(10 * S);
  assert.equal(h.writes.length, 1);
  h.edit(); await h.advance(60 * S);
  assert.equal(h.writes.length, 1, 'mid-flight: no second write however long it takes');
  assert.equal(h.scheduler.state().writing, true);
  assert.ok(h.timers.length >= 1, 'the tick chain stays armed');
  h.gate.resolve(); await flush(); await flush();
  assert.equal(h.scheduler.state().writing, false);
  assert.equal(h.scheduler.state().recovered, hex(1), 'the digest the write carried is recovered, not the newer one');
  await h.advance(35 * S);
  assert.equal(h.writes.length, 1, 'the 60 s the write took sets a 20x backoff floor (1200 s)');
  await h.advance(1200 * S);
  assert.equal(h.writes.length, 2);
});

test('a stale receipt records nothing and the next tick retries at the current revision', async () => {
  const h = harness({ rev: 0 }); h.scheduler.start();
  h.reply = (r) => ({ requestId: r.requestId, documentId: r.documentId, published: false, reason: 'stale', byteCount: 0 });
  h.edit(); await h.advance(10 * S);
  assert.equal(h.writes.length, 1);
  assert.equal(h.scheduler.state().lastWriteAt, null); assert.equal(h.scheduler.state().recovered, null);
  assert.deepEqual(h.notices, []);
  h.reply = null; h.edit(); await h.advance(5 * S);
  assert.equal(h.writes.length, 2);
  assert.equal(h.writes[1].request.atRevision, 2);
  assert.equal(h.scheduler.state().recovered, hex(2));
});

test('no write while busy, blocked (playing or pointer held), not presented, or inactive; idle restarts afterwards', async () => {
  for (const flag of ['busy', 'blocked']) {
    const h = harness({ rev: 0 }); h.scheduler.start(); h.edit(); h[flag] = true;
    await h.advance(120 * S);
    assert.equal(h.writes.length, 0, flag);
    h[flag] = false; await h.advance(5 * S);
    assert.equal(h.writes.length, 0, flag + ': idle clock restarts when it clears');
    await h.advance(5 * S);
    assert.equal(h.writes.length, 1, flag);
  }
  const h = harness({ rev: 0 }); h.scheduler.start(); h.edit(); h.presented = false;
  await h.advance(60 * S); assert.equal(h.writes.length, 0, 'synchronizing / queue pending / presentation not current');
  h.presented = true; h.active = false; await h.advance(60 * S); assert.equal(h.writes.length, 0, 'inactive session');
  h.active = true; await h.advance(10 * S); assert.equal(h.writes.length, 1);
});

test('a session switch resets the cadence; a write that returns after a switch is not recorded', async () => {
  const h = harness({ rev: 0 }); h.scheduler.start();
  h.edit(); await h.advance(10 * S);
  assert.equal(h.writes.length, 1);
  h.documentId = 'document-2'; h.generation = 2; h.rev = 5; h.baseline = hex(5); await h.advance(60 * S);
  assert.equal(h.writes.length, 1, 'the new session is clean at its own baseline');
  h.edit(); await h.advance(10 * S);
  assert.equal(h.writes.length, 2, 'first-write rule again, not the 30 s gap of the old session');
  assert.notEqual(h.writes[1].request.documentId, h.writes[0].request.documentId);
  h.gate = deferred(); h.edit(); await h.advance(40 * S);
  assert.equal(h.writes.length, 3);
  h.documentId = 'document-3'; h.generation = 3; h.baseline = hex(0);
  h.gate.resolve(); await flush(); await flush();
  await h.advance(5 * S);
  assert.equal(h.scheduler.state().recovered, null, 'late receipt of the previous session does not leak into the new one');
});

test('write failure: previous state kept, one toast per session, retried on the next tick', async () => {
  const h = harness({ rev: 0 }); h.scheduler.start();
  h.edit(); h.fail = 'No space left on device';
  await h.advance(30 * S);
  assert.ok(h.writes.length >= 3, 'retried every tick');
  assert.equal(h.notices.length, 1);
  assert.match(h.notices[0], /No space left on device/);
  assert.equal(h.scheduler.state().recovered, null);
  h.fail = null; await h.advance(5 * S);
  assert.equal(h.scheduler.state().recovered, hex(1));
  assert.equal(h.notices.length, 1);
});

test('too-large and unsupported: no retry for the same digest, one toast for too-large', async () => {
  const h = harness({ rev: 0 }); h.scheduler.start();
  h.reply = (r) => ({ requestId: r.requestId, documentId: r.documentId, published: false, reason: 'too-large', byteCount: 0 });
  h.edit(); await h.advance(60 * S);
  assert.equal(h.writes.length, 1); assert.equal(h.notices.length, 1);
  h.edit(); await h.advance(40 * S);
  assert.equal(h.writes.length, 2, 'a changed digest is tried again'); assert.equal(h.notices.length, 1, 'toast once');
  h.reply = (r) => ({ requestId: r.requestId, documentId: r.documentId, published: false, reason: 'unsupported', byteCount: 0 });
  h.edit(); await h.advance(60 * S); const n = h.writes.length;
  await h.advance(60 * S); assert.equal(h.writes.length, n); assert.equal(h.notices.length, 1);
});

test('flush (window hidden) writes the dirty state now but keeps the backoff floor', async () => {
  const h = harness({ rev: 0, duration: 2 * S }); h.scheduler.start();
  h.edit();
  const first = await h.scheduler.flush();
  assert.equal(first.wrote, true, 'no 5 s idle wait on flush'); assert.equal(h.writes.length, 1);
  h.edit(); await h.advance(10 * S);
  const early = await h.scheduler.flush();
  assert.equal(early.wrote, false, '20 x 2 s = 40 s floor still holds'); assert.equal(early.why, 'not-due');
  await h.advance(35 * S);
  assert.equal(h.writes.length, 2, 'the tick chain writes once the floor has passed');
  assert.ok(h.writes[1].at - h.writes[0].at >= 40 * S);
  const clean = harness({ rev: 0 }); clean.scheduler.start();
  assert.equal((await clean.scheduler.flush()).why, 'clean');
});

test('stop clears the timer; a throwing tick never ends the chain', async () => {
  const h = harness({ rev: 0 }); h.scheduler.start(); h.edit(); h.pinThrows = true;
  await h.advance(30 * S); assert.equal(h.writes.length, 0);
  assert.equal(h.timers.length, 1, 'chain survived the throwing ticks');
  h.pinThrows = false; await h.advance(10 * S); assert.equal(h.writes.length, 1);
  h.scheduler.stop(); assert.equal(h.timers.length, 0);
  h.edit(); await h.advance(120 * S); assert.equal(h.writes.length, 1);
});

test('request shape: unique ids, slot key from session tag or path, sanitized name, origin path, supersedes once', async () => {
  const h = harness({ rev: 0, slot: { path: null, name: '  A\u0000\u0007B  ' }, supersedes: 'untitled-cafebabe' });
  h.scheduler.start(); h.edit(); await h.advance(10 * S);
  const first = h.writes[0].request;
  assert.deepEqual(Object.keys(first).sort(), ['apiVersion', 'atRevision', 'displayName', 'documentDigest', 'documentId',
    'documentSnapshotId', 'instanceId', 'requestId', 'slotKey', 'supersedes']);
  assert.equal(first.apiVersion, 2); assert.equal(first.slotKey, 'untitled-deadbeef'); assert.equal(first.displayName, 'A  B');
  assert.equal(first.supersedes, 'untitled-cafebabe');
  h.slot = { path: '/home/user/My Project.json', name: 'My Project' };
  h.edit(); await h.advance(30 * S);
  const second = h.writes[1].request;
  assert.match(second.slotKey, /^[a-z0-9-]{1,64}$/); assert.equal(second.slotKey, 'my-project-json-h4sh');
  assert.equal(second.originPath, '/home/user/My Project.json'); assert.equal('supersedes' in second, false, 'sent once, after durable publish');
  assert.notEqual(first.requestId, second.requestId);
  h.slot = { path: '/' + 'x'.repeat(2000) + '/Name.json', name: 'n'.repeat(300) };
  h.edit(); await h.advance(30 * S);
  const third = h.writes[2].request;
  assert.equal('originPath' in third, false, 'over-long path is omitted, never truncated');
  assert.match(third.slotKey, /^[a-z0-9-]{1,64}$/); assert.equal(third.displayName.length, 128);
  assert.ok(JSON.stringify(third).length <= 4096);
});

test('bind: invokes the private command with requestJson, held pointer blocks, hidden window flushes', async () => {
  const listeners = {}, invoked = [], timers = [];
  const document = { visibilityState: 'visible', addEventListener: (n, f) => { (listeners[n] = listeners[n] || []).push(f); } };
  const info = { apiVersion: 2, instanceId: 'instance', documentId: 'document-1', contentRevision: 3, documentSnapshotId: 'snapshot-3',
    lifecycleGeneration: 1, documentDigest: hex(3), byteCount: 10 };
  const root = { document, state: { playing: false }, crypto: undefined,
    setTimeout: (fn, ms) => { timers.push({ fn, ms }); return timers.length; }, clearTimeout: () => {},
    addEventListener: () => {},
    __TAURI__: { core: { invoke: async (command, args) => { invoked.push({ command, args }); const r = JSON.parse(args.requestJson);
      return { requestId: r.requestId, documentId: r.documentId, published: true }; } } },
    NemoNativeObjectProject: { desktop: () => ({ requestId: (k) => 'project-' + k + '-7' }) } };
  const scheduler = Recovery.bind(root, { pin: () => ({ active: true, info, busy: false, presented: true, reference: hex(0) }),
    slot: () => ({ path: null, name: 'Untitled' }), notice: () => {} });
  assert.ok(scheduler); scheduler.start(); assert.equal(timers[0].ms, 5000);
  listeners.pointerdown[0]();
  assert.equal((await scheduler.flush()).why, 'busy', 'pointer held');
  listeners.pointerup[0]();
  document.visibilityState = 'hidden'; listeners.visibilitychange[0](); await flush();
  assert.equal(invoked.length, 1); assert.equal(invoked[0].command, 'nemo_native_object_recovery_write');
  assert.deepEqual(Object.keys(invoked[0].args), ['requestJson']);
  const body = JSON.parse(invoked[0].args.requestJson);
  assert.equal(body.requestId, 'project-recovery-7'); assert.equal(body.atRevision, 3); assert.match(body.slotKey, /^untitled-[0-9a-f]{8}$/);
  root.state.playing = true; assert.equal((await scheduler.flush()).why, 'clean', 'recovered digest: nothing more to write');
  assert.equal(Recovery.bind({ __TAURI__: {} }, {}), null);
});

test('the scheduler is native-only: it never touches the legacy autosave, serializer or storage paths', () => {
  const source = fs.readFileSync('src/js/adapters/native-object-recovery.js', 'utf8');
  for (const forbidden of ['nemo-auto', 'persistenceJSON', 'documentJSON', 'exportJSON', 'autosaveWrite', 'pushVersionSnapshot', 'localStorage', 'SMIdb', 'writeTextFile'])
    assert.equal(source.includes(forbidden), false, forbidden);
  assert.ok(source.split('\n').filter((l) => l.trim()).length <= 500);
});

test('timeline timer stays a native no-op: its documentJSON path still throws and writes nothing', () => {
  const timeline = fs.readFileSync('src/js/timeline.js', 'utf8');
  assert.match(timeline, /Timed autosave unavailable/);
  const project = fs.readFileSync('src/js/adapters/native-object-project.js', 'utf8');
  assert.match(project, /persistenceJSON:function\(\)\{throw new Error\('Native project files must be saved through the native file command'\);\}/);
});
