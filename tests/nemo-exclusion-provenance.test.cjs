'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { checkApplicationPolicy } = require('../scripts/nemo/lib/boundaries-application.cjs');

const EXCLUDED = 'src/vendor.js';
const SUPPORT = 'support.txt';
const CONTENT = {
  'src/app.js': 'module.exports = 1;\n',
  [EXCLUDED]: '/* vendor fixture */\n',
  [SUPPORT]: 'Independent provenance evidence.\n',
  'index.html': '<script src="src/app.js"></script>\n',
};
const digest = (algorithm, bytes) => crypto.createHash(algorithm).update(bytes).digest('hex');

// Compute pins from known fixture bytes, independently of production helpers.
function pins(file) {
  const bytes = Buffer.from(CONTENT[file]);
  return {
    sha256: digest('sha256', bytes),
    gitBlob: digest('sha1', Buffer.concat([Buffer.from(`blob ${bytes.length}\0`), bytes])),
  };
}

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-exclusion-provenance-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const git = (...args) => execFileSync('git', ['-C', root, ...args], {
    encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
  }).trim();
  for (const [file, content] of Object.entries(CONTENT)) {
    fs.mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    fs.writeFileSync(path.join(root, file), content);
  }
  git('init', '--quiet', '--object-format=sha1');
  git('config', 'core.autocrlf', 'false');
  git('config', 'core.attributesFile', '/dev/null');
  git('add', '--', ...Object.keys(CONTENT));
  // A local synthetic HEAD needs no host identity, hooks or branch publication.
  const tree = git('write-tree');
  // Use commit-tree for a valid tree-bearing commit, with fixture-only identity.
  const head = execFileSync('git', ['-C', root, 'commit-tree', tree, '-m', 'fixture'], {
    encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, GIT_AUTHOR_NAME: 'Fixture', GIT_AUTHOR_EMAIL: 'fixture@example.invalid',
      GIT_COMMITTER_NAME: 'Fixture', GIT_COMMITTER_EMAIL: 'fixture@example.invalid' },
  }).trim();
  git('update-ref', 'HEAD', head);

  const profile = { modules: [{ id: 'app', dir: 'src', files: ['app.js'] }] };
  const policy = {
    schemaVersion: 1, policyId: 'nemo.app-js.coverage', status: 'adopted',
    scope: { sourceRoot: 'src', sourceKinds: ['.js'] },
    retainedSources: [{ path: 'src/app.js', moduleId: 'app' }],
    exclusions: [{ path: EXCLUDED, category: 'vendor', component: 'fixture',
      reason: 'Synthetic vendor source', evidence: [SUPPORT], provenance: pins(EXCLUDED) }],
    snapshotCounts: { selectedSources: 2, retainedSources: 1, exclusions: 1 },
    provenance: {
      sourceRootTree: git('rev-parse', 'HEAD:src'),
      applicationTree: { path: 'src', gitTree: git('rev-parse', 'HEAD:src') },
      bootstrap: { path: 'index.html', ...pins('index.html') },
      exclusionSupport: [{ path: SUPPORT, ...pins(SUPPORT) }],
      inventoryDigest: { algorithm: 'sha256', value: digest('sha256',
        `src/app.js\0${pins('src/app.js').gitBlob}\n${EXCLUDED}\0${pins(EXCLUDED).gitBlob}\n`) },
    },
  };
  return { policy, check: () => checkApplicationPolicy(profile, policy, { root }) };
}

function rejectsPin(check, label, file, pin) {
  assert.throws(check, { message: `invalid application coverage policy: ${label} ${file} ${pin} changed` });
}

test('valid exclusion pins and independently pathed exclusion support pass', (t) => {
  const { check } = fixture(t);
  const result = check();
  assert.equal(result.ok, true);
  assert.deepEqual([result.sourcePathCount, result.retainedPathCount, result.excludedPathCount], [2, 1, 1]);
  assert.deepEqual(result.coverage.violations, []);
});

test('nested provenance.path cannot validate another existing file with its SHA-256 pins', (t) => {
  const { policy, check } = fixture(t);
  policy.exclusions[0].provenance = { path: SUPPORT, ...pins(SUPPORT) };
  rejectsPin(check, 'exclusion', EXCLUDED, 'SHA-256');
});

test('wrong Git blob pin is checked on the excluded path despite nested provenance.path', (t) => {
  const { policy, check } = fixture(t);
  policy.exclusions[0].provenance = {
    path: SUPPORT, sha256: pins(EXCLUDED).sha256, gitBlob: pins(SUPPORT).gitBlob,
  };
  // Exact error matters: failing SHA-256 on support.txt still validates the wrong path.
  rejectsPin(check, 'exclusion', EXCLUDED, 'Git blob');
});

test('valid excluded-file pins remain valid with a different nested provenance.path', (t) => {
  const { policy, check } = fixture(t);
  policy.exclusions[0].provenance.path = SUPPORT;
  assert.equal(check().ok, true);
});

for (const [field, label, wrong] of [
  ['sha256', 'SHA-256', '0'.repeat(64)],
  ['gitBlob', 'Git blob', '0'.repeat(40)],
]) {
  test(`wrong exclusion ${label} pin fails without a nested path`, (t) => {
    const { policy, check } = fixture(t);
    policy.exclusions[0].provenance[field] = wrong;
    rejectsPin(check, 'exclusion', EXCLUDED, label);
  });

  test(`independent exclusion support ${label} pin remains enforced on its own path`, (t) => {
    const { policy, check } = fixture(t);
    policy.provenance.exclusionSupport[0][field] = wrong;
    rejectsPin(check, 'exclusion support', SUPPORT, label);
  });
}
