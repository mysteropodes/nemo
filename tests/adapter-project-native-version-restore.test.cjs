'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { create } = require('../src/js/adapters/project-native-version-restore.js');

const PREVIOUS = '{"opacity":60,"label":"☃\\r\\n"}\r\n';
const VERSION = '{"opacity":40}';
function deferred() { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; }
function harness() {
  const events = [];
  let pin = { instanceId: 'instance', documentId: 'A', contentRevision: 2, path: '/A.json', name: 'A' };
  const first = { owner: 'native', status: 'presented', frame: 0, instanceId: 'instance', documentId: 'B', contentRevision: 0 };
  const ports = {
    pin: () => pin,
    read: async () => VERSION,
    validate: value => assert.equal(value, VERSION),
    serialize: () => PREVIOUS,
    backupTarget: async () => '/history/A',
    importNative: async value => { assert.equal(value, VERSION); events.push('import'); pin = { ...pin, documentId: 'B', contentRevision: 0 }; return first; },
    ready: value => value === first && pin && pin.documentId === 'B' && pin.contentRevision === 0,
    publish: async value => { assert.equal(value, first); events.push('presented'); return true; },
    backup: async (bytes, target) => events.push(['backup', bytes, target]),
    restored: () => events.push('restored'),
    unavailable: error => events.push(['unavailable', error.message]),
    backupFailed: error => events.push(['backup-failed', error.message]),
  };
  return { ports, events, first, get pin() { return pin; }, set pin(value) { pin = value; } };
}
function hasPublication(events) { return events.some(e => e === 'restored' || Array.isArray(e) && e[0] === 'backup'); }

test('records exact previous pin only after final presentation, then reports restore', async () => {
  const h = harness(), visible = deferred();
  h.ports.publish = () => visible.promise;
  const controller = create(h.ports), pending = controller.restore('/version.json');
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(h.events, ['import']);
  assert.equal(await controller.restore('/other-version.json'), false, 'only one UI restore is admitted');
  visible.resolve(true);
  assert.equal(await pending, true);
  assert.deepEqual(h.events, ['import', ['backup', PREVIOUS, '/history/A'], 'restored']);
  assert.equal(h.pin.documentId, 'B');
});

for (const invalid of [null, { instanceId: '', documentId: 'A', contentRevision: 2 },
  { instanceId: 'instance', documentId: 'A', contentRevision: -1 },
  { instanceId: 'instance', documentId: 'A', contentRevision: 0.5 }]) {
  test(`unavailable/malformed native identity rejects before file read: ${JSON.stringify(invalid)}`, async () => {
    const h = harness(); h.pin = invalid;
    h.ports.read = () => { throw Error('must not read'); };
    assert.equal(await create(h.ports).restore('/version.json'), false);
    assert.equal(hasPublication(h.events), false);
    assert.equal(h.events.includes('import'), false);
  });
}

for (const field of ['instanceId', 'documentId', 'contentRevision', 'path', 'name']) {
  test(`drift of ${field} during async read rejects without touching the intervening document`, async () => {
    const h = harness();
    h.ports.read = async () => { h.pin = { ...h.pin, [field]: field === 'contentRevision' ? 3 : 'changed' }; return VERSION; };
    assert.equal(await create(h.ports).restore('/version.json'), false);
    assert.equal(h.pin[field], field === 'contentRevision' ? 3 : 'changed');
    assert.equal(h.events.includes('import'), false);
    assert.equal(hasPublication(h.events), false);
  });
}

for (const operation of ['read', 'validate', 'serialize', 'backupTarget']) {
  test(`${operation} failure preserves A and produces no replacement or history`, async () => {
    const h = harness(); h.ports[operation] = () => { throw Error(operation + ' failed'); };
    assert.equal(await create(h.ports).restore('/version.json'), false);
    assert.equal(h.pin.documentId, 'A');
    assert.equal(h.events.includes('import'), false);
    assert.equal(hasPublication(h.events), false);
  });
}

test('revision drift while acquiring backup target or serialization denies import', async () => {
  for (const operation of ['serialize', 'backupTarget']) {
    const h = harness(), original = h.ports[operation];
    h.ports[operation] = (...args) => { h.pin = { ...h.pin, contentRevision: 3 }; return original(...args); };
    assert.equal(await create(h.ports).restore('/version.json'), false);
    assert.equal(h.pin.contentRevision, 3);
    assert.equal(h.events.includes('import'), false);
    assert.equal(hasPublication(h.events), false);
  }
});

for (const bytes of [null, '', {}]) {
  test(`unready persistence bytes are not backed up: ${JSON.stringify(bytes)}`, async () => {
    const h = harness(); h.ports.serialize = () => bytes;
    assert.equal(await create(h.ports).restore('/version.json'), false);
    assert.equal(h.events.includes('import'), false);
    assert.equal(hasPublication(h.events), false);
  });
}

test('rejected admission preserves A and never publishes', async () => {
  const h = harness(); h.ports.importNative = async () => false;
  assert.equal(await create(h.ports).restore('/version.json'), false);
  assert.equal(h.pin.documentId, 'A');
  assert.equal(hasPublication(h.events), false);
});

test('post-host failed presentation leaves fenced B, without JS rollback or history/success', async () => {
  const h = harness();
  h.ports.publish = async () => { h.pin = { ...h.pin, fenced: true }; throw Error('native final presentation failed'); };
  assert.equal(await create(h.ports).restore('/version.json'), false);
  assert.equal(h.pin.documentId, 'B');
  assert.equal(h.pin.fenced, true);
  assert.equal(hasPublication(h.events), false);
});

for (const stage of ['initial', 'final']) {
  test(`unverified ${stage} receipt is not a completed restore`, async () => {
    const h = harness();
    if (stage === 'initial') h.ports.ready = () => false;
    else h.ports.publish = async () => false;
    assert.equal(await create(h.ports).restore('/version.json'), false);
    assert.equal(hasPublication(h.events), false);
  });
}

test('best-effort backup failure is reported after genuine restoration, without rollback', async () => {
  const h = harness(); h.ports.backup = async () => { throw Error('disk unavailable'); };
  assert.equal(await create(h.ports).restore('/version.json'), true);
  assert.equal(h.pin.documentId, 'B');
  assert.deepEqual(h.events, ['import', 'presented', ['backup-failed', 'disk unavailable'], 'restored']);
});

test('shared backup writer keeps exact bytes, post-mkdir clock and existing retention/failure order', async () => {
  const { writeSnapshot } = require('../src/js/adapters/project-native-version-restore.js');
  const events = [];
  await writeSnapshot(PREVIOUS, '/history/A', {
    mkdir: async dir => events.push(['mkdir', dir]),
    writeTextFile: async (path, bytes) => events.push(['write', path, bytes]),
    readDir: async () => [{ name: '003.json' }, { name: 'ignore.txt' }, { name: '001.json' }, { name: '002.json' }],
    remove: async path => { events.push(['remove', path]); throw Error('existing removal failure is tolerated'); },
  }, 2, () => { events.push('clock'); return 4; });
  assert.deepEqual(events, [['mkdir', '/history/A'], 'clock', ['write', '/history/A/4.json', PREVIOUS], ['remove', '/history/A/001.json']]);
  await assert.rejects(writeSnapshot(PREVIOUS, '/history/A', {
    mkdir: async () => {}, writeTextFile: async () => { throw Error('write failed'); },
    readDir: async () => { throw Error('must not list after failed write'); },
  }, 120, () => 4), /write failed/);
});
