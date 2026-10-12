'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

const baseline = process.env.N25E4A_BASELINE === '1';
const options = { requestId: 'save-1', pinRequestId: 'pin-1' };
const destination = '/allowed/new-object.json';
const documentJson = '{"nativeOpaque":"authored and Reference records; preserve exact bytes"}';
const pin = Object.freeze({ instanceId: 'instance-a', documentId: 'object-a',
  contentRevision: 7, lifecycleGeneration: 4, documentSnapshotId: 'snapshot-7' });
const expectedRequest = Object.freeze({ apiVersion: 2, requestId: 'save-1',
  instanceId: 'instance-a', documentId: 'object-a', atRevision: 7,
  documentSnapshotId: 'snapshot-7', destinationPath: destination });
function deferred() { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; }
const turn = () => new Promise(setImmediate);
function session() {
  return { apiVersion: 2, available: true, kind: 'native-object', ...pin,
    presentation: { owner: 'native', status: 'presented', ...pin,
      contextId: 'scene-root', frame: 3, viewGeneration: 9, workId: 'work-9' } };
}
function receipt(request = expectedRequest) {
  return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId,
    documentId: request.documentId, atRevision: request.atRevision,
    documentSnapshotId: request.documentSnapshotId, byteCount: 89,
    published: true, cleanupPending: false };
}
function harness() {
  const calls = [], writes = []; let current = session();
  const ports = {
    readPresentedObjectSession: () => structuredClone(current),
    acquirePin: async request => {
      calls.push(['pin', request]);
      return { apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId,
        documentId: pin.documentId, contentRevision: 7, ok: true,
        result: { atRevision: 7, documentSnapshotId: 'snapshot-7', documentJson } };
    },
    pickDestination: async () => { calls.push(['picker']); return destination; },
    savePinned: async request => {
      calls.push(['save', request]); writes.push(documentJson); return receipt(request);
    },
  };
  return { ports, calls, writes, current: () => current,
    change: fn => fn(current), replace: value => { current = value; },
    count: name => calls.filter(c => c[0] === name).length };
}
function legacyRun(ports) {
  const source = fs.readFileSync('src/js/project.js', 'utf8');
  const start = source.indexOf('  async function saveAs(){');
  const end = source.indexOf('  async function save(){', start);
  assert.ok(start >= 0 && end > start);
  const context = { window: { __TAURI__: { dialog: { save: ports.pickDestination } },
    SMProjectDocument: { baseName: () => 'new-object' } },
    tauriOk: () => true, saveFramesIfLegacy: () => {}, currentName: 'Old',
    writeProjectTo: path => ports.savePinned({ legacy: true, destinationPath: path }),
    showToast() {}, SM: { t: () => 'failure' } };
  vm.runInNewContext(source.slice(start, end) + '\nresult = saveAs;', context);
  return context.result();
}
const run = baseline ? legacyRun : (...args) => require('../src/js/application/native-object-save-as-workflow.js').run(...args);

test('current legacy Save As source has no object-session fence after its deferred picker', async () => {
  const h = harness(), picker = deferred(); h.ports.pickDestination = () => picker.promise;
  const pending = legacyRun(h.ports); h.change(s => { s.documentId = 'object-b'; });
  picker.resolve(destination); await pending;
  assert.equal(h.count('save'), 1, 'retained SOURCE characterization, not installed acceptance');
});
test('native owner drift during picker refuses before disk dispatch', async () => {
  const h = harness(), picker = deferred();
  h.ports.pickDestination = () => { h.calls.push(['picker']); return picker.promise; };
  const pending = run(h.ports, options); await turn();
  h.change(s => { s.documentId = 'object-b'; }); picker.resolve(destination);
  await pending;
  assert.equal(h.count('save'), 0, 'independent stale/no-write oracle on actual code');
});
test('stable presented object pin emits exactly one private request and no document bytes', async () => {
  const h = harness(); const result = await run(h.ports, options);
  assert.equal(result.status, 'published-current'); assert.equal(result.eligibleCurrentSave, true);
  assert.equal(h.count('picker'), 1); assert.equal(h.count('save'), 1);
  assert.deepEqual(h.calls.find(c => c[0] === 'save')[1], expectedRequest);
  assert.deepEqual(h.calls.find(c => c[0] === 'pin')[1], { apiVersion: 2, requestId: 'pin-1',
    instanceId: 'instance-a', documentId: 'object-a', operation: 'query.document.object.serialize',
    payload: { atRevision: 7 } });
  assert.deepEqual(h.writes, [documentJson]);
  assert.equal('documentJson' in result, false);
  assert.equal(Object.isFrozen(h.calls.find(c => c[0] === 'save')[1]), true);
  assert.equal(Object.isFrozen(result.pin), true);
});
const invalidSessions = [
  ['absent', h => h.replace(null)],
  ['unavailable', h => h.change(s => { s.available = false; })],
  ['opacity', h => h.change(s => { s.kind = 'native-opacity'; })],
  ['status-only', h => h.change(s => { delete s.presentation; })],
  ['not presented', h => h.change(s => { s.presentation.status = 'deferred'; })],
  ['wrong presented owner', h => h.change(s => { s.presentation.owner = 'javascript'; })],
  ['wrong presented document', h => h.change(s => { s.presentation.documentId = 'object-b'; })],
  ['wrong presented revision', h => h.change(s => { s.presentation.contentRevision = 8; })],
  ['wrong generation', h => h.change(s => { s.presentation.lifecycleGeneration = 5; })],
  ['wrong snapshot', h => h.change(s => { s.presentation.documentSnapshotId = 'snapshot-8'; })],
  ['unsafe revision', h => h.change(s => { s.contentRevision = Number.MAX_SAFE_INTEGER + 1; })],
  ['missing work', h => h.change(s => { delete s.presentation.workId; })],
];
for (const [name, mutate] of invalidSessions) test('unavailable before picker/save: ' + name, async () => {
  const h = harness(); mutate(h); const result = await run(h.ports, options);
  assert.equal(result.status, 'unavailable'); assert.equal(result.eligibleCurrentSave, false);
  assert.equal(h.count('pin'), 0); assert.equal(h.count('picker'), 0); assert.equal(h.count('save'), 0);
});
for (const field of ['instanceId', 'documentId', 'contentRevision', 'lifecycleGeneration', 'documentSnapshotId']) {
  test('pin remains stale across picker when ' + field + ' changes', async () => {
    const h = harness(); h.ports.pickDestination = async () => {
      h.change(s => { s[field] = typeof s[field] === 'number' ? s[field] + 1 : s[field] + '-new'; });
      return destination;
    };
    const result = await run(h.ports, options);
    assert.equal(result.status, 'stale-before-dispatch'); assert.equal(h.count('save'), 0);
  });
}
test('frame/view advancement with unchanged content pin is allowed', async () => {
  const h = harness(); h.ports.pickDestination = async () => {
    h.change(s => { s.presentation.frame = 4; s.presentation.viewGeneration = 10; s.presentation.workId = 'work-10'; });
    return destination;
  };
  assert.equal((await run(h.ports, options)).status, 'published-current');
});
for (const mode of ['null', 'throw', 'bad-path']) test('picker ' + mode + ' never dispatches disk', async () => {
  const h = harness(); h.ports.pickDestination = async () => {
    if (mode === 'throw') throw new Error('picker failed');
    return mode === 'null' ? null : ['not', 'path'];
  };
  const result = await run(h.ports, options);
  assert.equal(result.status, mode === 'null' ? 'cancelled' : 'refused');
  assert.equal(result.eligibleCurrentSave, false); assert.equal(h.count('save'), 0);
});
for (const mutate of [r => { r.requestId = 'other'; }, r => { r.instanceId = 'other'; },
  r => { r.documentId = 'other'; }, r => { r.contentRevision = 8; }, r => { r.ok = false; },
  r => { r.result.documentSnapshotId = 'other'; }, r => { delete r.result.documentJson; }]) {
  test('invalid native serializer pin never reaches picker', async () => {
    const h = harness(), acquire = h.ports.acquirePin;
    h.ports.acquirePin = async request => { const response = await acquire(request); mutate(response); return response; };
    assert.equal((await run(h.ports, options)).status, 'refused');
    assert.equal(h.count('picker'), 0); assert.equal(h.count('save'), 0);
  });
}
test('native pin acquisition failure has zero picker/disk effects', async () => {
  const h = harness(); h.ports.acquirePin = async () => { throw new Error('owner gone'); };
  assert.equal((await run(h.ports, options)).status, 'refused'); assert.equal(h.count('save'), 0);
});
test('drift while acquiring the pin refuses before picker', async () => {
  const h = harness(), acquire = h.ports.acquirePin;
  h.ports.acquirePin = async request => { const response = await acquire(request); h.replace(null); return response; };
  assert.equal((await run(h.ports, options)).status, 'stale-before-dispatch'); assert.equal(h.count('picker'), 0);
});
for (const field of ['apiVersion', 'requestId', 'instanceId', 'documentId', 'atRevision', 'documentSnapshotId', 'byteCount', 'published', 'cleanupPending']) {
  test('malformed disk receipt is indeterminate/no retry: ' + field, async () => {
    const h = harness(); h.ports.savePinned = async request => {
      h.calls.push(['save', request]); const r = receipt(request); r[field] = 'invalid'; return r;
    };
    const result = await run(h.ports, options); assert.equal(result.status, 'indeterminate');
    assert.equal(result.eligibleCurrentSave, false); assert.equal(h.count('save'), 1);
  });
}
test('host refused new-file/no-overwrite publication gives no current eligibility', async () => {
  const h = harness(); h.ports.savePinned = async request => {
    h.calls.push(['save', request]); return { ...receipt(request), published: false };
  };
  assert.equal((await run(h.ports, options)).status, 'refused'); assert.equal(h.count('save'), 1);
});
test('transport exception after dispatch is indeterminate, never retried', async () => {
  const h = harness(); h.ports.savePinned = async request => { h.calls.push(['save', request]); throw new Error('reply lost'); };
  const result = await run(h.ports, options); assert.equal(result.status, 'indeterminate');
  assert.equal(result.eligibleCurrentSave, false); assert.equal(h.count('save'), 1);
});
test('published cleanup-pending preserves written pin/path without current eligibility', async () => {
  const h = harness(); h.ports.savePinned = async request => ({ ...receipt(request), cleanupPending: true });
  const result = await run(h.ports, options);
  assert.equal(result.status, 'published-cleanup-pending'); assert.equal(result.eligibleCurrentSave, false);
  assert.equal(result.destinationPath, destination); assert.equal(result.pin.documentSnapshotId, 'snapshot-7');
});
for (const field of ['instanceId', 'documentId', 'contentRevision', 'lifecycleGeneration', 'documentSnapshotId']) {
  test('publication then ' + field + ' drift retains old written pin and denies current save', async () => {
    const h = harness(); h.ports.savePinned = async request => {
      h.calls.push(['save', request]); h.change(s => { s[field] = typeof s[field] === 'number' ? s[field] + 1 : s[field] + '-new'; });
      return receipt(request);
    };
    const result = await run(h.ports, options); assert.equal(result.status, 'published-stale');
    assert.equal(result.eligibleCurrentSave, false); assert.equal(result.pin.contentRevision, 7);
    assert.equal(result.pin.documentId, 'object-a'); assert.equal(result.destinationPath, destination);
    assert.equal(h.count('save'), 1);
  });
}
test('unreadable current owner after publication denies acknowledgement', async () => {
  const h = harness(); let unreadable = false;
  h.ports.readPresentedObjectSession = () => { if (unreadable) throw new Error('session indeterminate'); return h.current(); };
  h.ports.savePinned = async request => {
    unreadable = true; return receipt(request);
  };
  assert.equal((await run(h.ports, options)).status, 'published-stale');
});
for (const boundary of ['before', 'pin', 'picker', 'save']) test('abort at ' + boundary + ' reconciles without fallback', async () => {
  const h = harness(), signal = { aborted: boundary === 'before' };
  for (const [name, stage] of [['acquirePin', 'pin'], ['pickDestination', 'picker'], ['savePinned', 'save']]) {
    const original = h.ports[name]; h.ports[name] = async arg => {
      const result = await original(arg); if (stage === boundary) signal.aborted = true; return result;
    };
  }
  const result = await run(h.ports, { ...options, signal });
  assert.equal(result.status, boundary === 'save' ? 'published-stale' : 'cancelled');
  assert.equal(result.eligibleCurrentSave, false); assert.equal(h.count('save'), boundary === 'save' ? 1 : 0);
});
test('superseded caller generation before dispatch refuses; after dispatch retains published result', async () => {
  for (const stage of ['picker', 'save']) {
    const h = harness(); let current = true;
    const name = stage === 'picker' ? 'pickDestination' : 'savePinned', original = h.ports[name];
    h.ports[name] = async arg => { const value = await original(arg); current = false; return value; };
    const result = await run(h.ports, { ...options, isCurrent: () => current });
    assert.equal(result.status, stage === 'save' ? 'published-stale' : 'cancelled');
    assert.equal(result.eligibleCurrentSave, false);
  }
});
test('helper never calls poisoned legacy/UI/authority effects', async () => {
  const h = harness();
  for (const name of ['releaseCurrent', 'newOwner', 'writeTextFile', 'legacyExport', 'markSaved', 'publishUi', 'remove', 'retry']) {
    h.ports[name] = () => { throw new Error('forbidden effect ' + name); };
  }
  assert.equal((await run(h.ports, options)).status, 'published-current');
});
test('bad request IDs refuse without any effects', async () => {
  const h = harness(); const result = await run(h.ports, { requestId: '../bad', pinRequestId: 'pin-1' });
  assert.equal(result.status, 'refused'); assert.deepEqual(h.calls, []);
});
test('request IDs and native authority port bindings cannot change during picker', async () => {
  const h = harness(), input = { ...options };
  h.ports.pickDestination = async () => {
    input.requestId = 'different'; input.pinRequestId = 'different-pin';
    h.ports.savePinned = () => { throw new Error('alternate writer'); };
    return destination;
  };
  const result = await run(h.ports, input);
  assert.equal(result.status, 'published-current');
  assert.deepEqual(h.calls.find(c => c[0] === 'save')[1], expectedRequest);
});
test('unknown serializer result fields refuse without picker', async () => {
  const h = harness(), original = h.ports.acquirePin;
  h.ports.acquirePin = async request => { const r = await original(request); r.result.extra = true; return r; };
  assert.equal((await run(h.ports, options)).status, 'refused'); assert.equal(h.count('picker'), 0);
});
for (const value of [0, 4097, Number.MAX_SAFE_INTEGER + 1]) {
  test('disk byteCount bound refuses eligibility: ' + value, async () => {
    const h = harness(); h.ports.savePinned = async request => ({ ...receipt(request), byteCount: value });
    assert.equal((await run(h.ports, options)).status, 'indeterminate');
  });
}
test('unknown publication receipt fields are indeterminate', async () => {
  const h = harness(); h.ports.savePinned = async request => ({ ...receipt(request), extra: true });
  assert.equal((await run(h.ports, options)).status, 'indeterminate');
});
test('returned receipt is detached from the port response', async () => {
  const h = harness(), response = receipt(); h.ports.savePinned = async () => response;
  const result = await run(h.ports, options); response.published = false;
  assert.equal(result.receipt.published, true); assert.equal(Object.isFrozen(result.receipt), true);
});
test('unknown serializer envelope fields refuse before picker', async () => {
  const h = harness(), original = h.ports.acquirePin;
  h.ports.acquirePin = async request => { const r = await original(request); r.extra = true; return r; };
  assert.equal((await run(h.ports, options)).status, 'refused'); assert.equal(h.count('picker'), 0);
});
test('maximum E3 disk byteCount boundary is accepted', async () => {
  const h = harness(); h.ports.savePinned = async request => ({ ...receipt(request), byteCount: 4096 });
  assert.equal((await run(h.ports, options)).status, 'published-current');
});
for (const path of ['relative.json', '/allowed/bad\0.json']) {
  test('invalid destination never reaches native publisher: ' + JSON.stringify(path), async () => {
    const h = harness(); h.ports.pickDestination = async () => path;
    assert.equal((await run(h.ports, options)).status, 'refused'); assert.equal(h.count('save'), 0);
  });
}
test('destination and written pin stay bound if caller mutates its selection', async () => {
  const h = harness(); let selected = destination;
  h.ports.pickDestination = async () => selected;
  h.ports.savePinned = async request => { selected = '/different/latest.json'; return receipt(request); };
  const result = await run(h.ports, options);
  assert.equal(result.destinationPath, destination); assert.equal(result.pin.documentSnapshotId, 'snapshot-7');
});
for (const [readNumber, label, expected] of [
  [1, 'first', { pin: 0, picker: 0, save: 0 }],
  [2, 'second', { pin: 1, picker: 0, save: 0 }],
  [3, 'pre-dispatch', { pin: 1, picker: 1, save: 0 }],
]) {
  for (const interruption of ['abort', 'supersession']) {
    test('deferred ' + label + ' read ' + interruption + ' prevents subsequent effects', async () => {
      const h = harness(), reader = deferred(), reached = deferred();
      const signal = { aborted: false }; let active = true, reads = 0;
      h.ports.readPresentedObjectSession = () => {
        if (++reads === readNumber) { reached.resolve(); return reader.promise; }
        return structuredClone(h.current());
      };
      const pending = run(h.ports, { ...options, signal, isCurrent: () => active });
      await reached.promise;
      for (const [effect, count] of Object.entries(expected)) assert.equal(h.count(effect), count);
      if (interruption === 'abort') signal.aborted = true; else active = false;
      reader.resolve(structuredClone(h.current())); // Same valid native tuple; cancellation alone must fence effects.
      const result = await pending;
      assert.equal(result.status, 'cancelled'); assert.equal(result.eligibleCurrentSave, false);
      for (const [effect, count] of Object.entries(expected)) {
        assert.equal(h.count(effect), count, label + ' read cannot dispatch ' + effect + ' after ' + interruption);
      }
    });
  }
}
for (const interruption of ['abort', 'supersession']) {
  test('deferred final read ' + interruption + ' keeps published file but denies current acknowledgement', async () => {
    const h = harness(), reader = deferred(), reached = deferred();
    const signal = { aborted: false }; let active = true, reads = 0;
    h.ports.readPresentedObjectSession = () => {
      if (++reads === 4) { reached.resolve(); return reader.promise; }
      return structuredClone(h.current());
    };
    const pending = run(h.ports, { ...options, signal, isCurrent: () => active });
    await reached.promise; assert.equal(h.count('save'), 1);
    if (interruption === 'abort') signal.aborted = true; else active = false;
    reader.resolve(structuredClone(h.current()));
    const result = await pending;
    assert.equal(result.status, 'published-stale'); assert.equal(result.eligibleCurrentSave, false);
    assert.equal(result.destinationPath, destination); assert.equal(result.pin.contentRevision, 7);
    assert.equal(h.count('save'), 1); assert.deepEqual(h.writes, [documentJson]);
  });
}
for (const [readNumber, label, pickers] of [[2, 'pre-picker', 0], [3, 'pre-save', 1]]) {
  test('deferred ' + label + ' native owner drift refuses subsequent effects', async () => {
    const h = harness(), reader = deferred(), reached = deferred(); let reads = 0;
    h.ports.readPresentedObjectSession = () => {
      if (++reads === readNumber) { reached.resolve(); return reader.promise; }
      return structuredClone(h.current());
    };
    const pending = run(h.ports, options); await reached.promise;
    h.change(s => { s.documentId = s.presentation.documentId = 'object-b'; });
    reader.resolve(structuredClone(h.current()));
    assert.equal((await pending).status, 'stale-before-dispatch');
    assert.equal(h.count('picker'), pickers); assert.equal(h.count('save'), 0);
  });
}
