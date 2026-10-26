'use strict';
// CS3/#2342 — the pin refresher computes exactly the pins the application policy
// gate verifies, reports drift by name, refuses inconsistent inputs and never
// touches policy/membership. Driven on a fixture with a stubbed git.
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const { refreshedPolicy, driftedPins, POLICY } = require('./refresh-app-pins.cjs');

function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-pins-'));
  fs.mkdirSync(path.join(root, 'src', 'js'), { recursive: true });
  fs.mkdirSync(path.join(root, path.dirname(POLICY)), { recursive: true });
  fs.writeFileSync(path.join(root, 'src', 'index.html'), '<html></html>\n');
  fs.writeFileSync(path.join(root, 'src', 'js', 'a.js'), 'var a = 1;\n');
  fs.writeFileSync(path.join(root, 'profile.json'), '{}\n');
  const policy = { policyId: 'nemo.app-js.coverage', scope: { sourceRoot: 'src', sourceKinds: ['.js'] }, retainedSources: [{ path: 'src/js/a.js' }],
    provenance: { candidateCommit: 'old', sourceRootTree: 'old', applicationTree: { path: 'src/js', gitTree: 'old', lastChangedCommit: 'old' },
      inventoryDigest: { algorithm: 'sha256', value: 'old' }, bootstrap: { path: 'src/index.html', sha256: 'old', gitBlob: 'old' },
      profiles: [{ path: 'profile.json', sha256: 'old', gitBlob: 'old', basedOnCommit: 'old' }] } };
  fs.writeFileSync(path.join(root, POLICY), JSON.stringify(policy, null, 2) + '\n');
  return root;
}
const blob = (bytes) => 'b' + crypto.createHash('sha1').update(bytes).digest('hex').slice(1);
function stubGit(root, { head = 'c0ffee', dirty = '' } = {}) {
  return (...args) => {
    if (args[0] === 'rev-parse') return args[1] === 'HEAD' || args[1] === head ? head : args[1].startsWith(head + ':') ? 'tree-' + args[1].split(':')[1] : 'other';
    if (args[0] === 'status') return dirty;
    if (args[0] === 'hash-object') return blob(fs.readFileSync(path.join(root, args[2])));
    throw new Error('unexpected git ' + args.join(' '));
  };
}

test('refresh fills every derived pin and leaves policy and membership untouched', () => {
  const root = fixture();
  try {
    const { raw, text } = refreshedPolicy(root, 'c0ffee', stubGit(root));
    const before = JSON.parse(raw), after = JSON.parse(text), pv = after.provenance;
    assert.equal(pv.sourceRootTree, 'tree-src'); assert.equal(pv.applicationTree.gitTree, 'tree-src/js');
    assert.equal(pv.bootstrap.gitBlob, blob(fs.readFileSync(path.join(root, 'src/index.html'))));
    assert.match(pv.inventoryDigest.value, /^[a-f0-9]{64}$/); assert.equal(pv.profiles[0].basedOnCommit, 'c0ffee');
    for (const key of Object.keys(before)) if (key !== 'provenance') assert.deepEqual(after[key], before[key], key);
    assert.deepEqual(driftedPins(text, text), []);
    assert.ok(driftedPins(raw, text).includes('inventoryDigest.value'));
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('refresh refuses a non-HEAD commit and uncommitted source or pinned files', () => {
  const root = fixture();
  try {
    assert.throws(() => refreshedPolicy(root, 'deadbeef', stubGit(root)), /refresh at HEAD/);
    assert.throws(() => refreshedPolicy(root, 'c0ffee', stubGit(root, { dirty: ' M src/js/a.js' })), /uncommitted/);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});
