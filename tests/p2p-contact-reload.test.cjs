'use strict';
// CS8-D08 I45 (#2381): the local P2P contact model survives a reload through its own storage keys, never touches
// project/document state, and claims no remote synchronization; no transport exists to exercise.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Model = require('../src/js/adapters/p2p-contact-model.js');

function session(storage, clock = 1000) {
  const state = { userProfile: { name: 'Ana' }, layers: [{ frames: [] }], totalFrames: 24, contentRevision: 7 };
  const before = JSON.stringify({ layers: state.layers, totalFrames: state.totalFrames, contentRevision: state.contentRevision });
  const model = Model.create({ state, storage, now: () => clock++,
    crypto: { getRandomValues: (a) => { for (let i = 0; i < a.length; i++) a[i] = (i * 37 + clock) & 255; return a; } },
    encodeBase64: (v) => Buffer.from(v, 'utf8').toString('base64'), decodeBase64: (v) => Buffer.from(v, 'base64').toString('utf8') });
  const untouched = () => assert.equal(JSON.stringify({ layers: state.layers, totalFrames: state.totalFrames, contentRevision: state.contentRevision }), before);
  return { state, model, untouched };
}
function memory() {
  const map = new Map();
  return { map, getItem: (k) => (map.has(k) ? map.get(k) : null), setItem: (k, v) => map.set(k, String(v)) };
}
const code = (k, n) => Buffer.from(JSON.stringify({ k, n, t: 1 }), 'utf8').toString('base64');

test('add and remove survive a reload; the identity stays stable across sessions', () => {
  const storage = memory();
  const first = session(storage);
  assert.equal(first.model.addContact(code('peer-a', 'A')).ok, true);
  assert.equal(first.model.addContact(code('peer-b', 'B')).ok, true);
  const identity = first.state.p2pIdentity.publicKey;
  const second = session(storage);
  assert.deepEqual(second.model.contacts().map((c) => c.publicKey), ['peer-a', 'peer-b']);
  assert.equal(second.state.p2pIdentity.publicKey, identity);
  second.model.removeContact('peer-a');
  const third = session(storage);
  assert.deepEqual(third.model.contacts().map((c) => c.publicKey), ['peer-b']);
  assert.deepEqual([...storage.map.keys()].sort(), ['nemo-p2p-contacts', 'nemo-p2p-identity'], 'only its own two keys');
});

test('no contact action touches the document or claims a remote synchronization', () => {
  const storage = memory(), s = session(storage);
  const added = s.model.addContact(code('peer-a', 'A'));
  assert.equal(added.contact.status, 'pending', 'pairing is local; nothing is reported as synchronized');
  assert.equal(s.model.addContact(code('peer-a', 'A')).ok, false);
  assert.equal(s.model.addContact(code(s.state.p2pIdentity.publicKey, 'Me')).ok, false);
  assert.equal(s.model.addContact('not base64 json').ok, false);
  s.model.removeContact('peer-a'); s.model.removeContact('absent');
  s.untouched();
  assert.equal(JSON.parse(Buffer.from(s.model.myInviteCode(), 'base64').toString('utf8')).n, 'Ana');
});

test('the P2P sources carry no transport, handshake or network call', () => {
  for (const file of ['src/js/adapters/p2p-contact-model.js', 'src/js/p2p-contacts.js']) {
    const text = fs.readFileSync(path.join(__dirname, '..', file), 'utf8');
    assert.doesNotMatch(text, /\bfetch\s*\(|WebSocket|RTCPeerConnection|XMLHttpRequest|__TAURI__\.(?:http|core)/, file);
  }
});
