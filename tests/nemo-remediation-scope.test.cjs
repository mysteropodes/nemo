'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync, spawnSync } = require('node:child_process');
const checker = require('../scripts/nemo/remediation-scope.cjs');

const commit = 'a'.repeat(40);
const source = [
  { path: 'README.md', mode: '100644', blob: '1'.repeat(40) },
  { path: 'src/main.js', mode: '100644', blob: '2'.repeat(40) },
];
const census = {
  path: 'engineering/inventory/census/C01.json', blob: '3'.repeat(40),
  document: { census_id: 'C01', source_sha: commit,
    areas: [{ area: 'runtime', packets: [{ id: 'C01.main' }] }],
    uncovered_responsibilities: ['A real unresolved consumer'],
  },
};
function manifest() {
  return {
    schema: 'nemo.remediation-scope/1', complete: false,
    source: { commit, fileCount: 2, sha256: checker.digest(source) },
    censuses: [{ id: 'C01', path: census.path, blob: census.blob, sourceCommit: commit }],
    packets: [{ id: 'C01.main', census: 'C01', admission: 'pending' }],
    uncoveredResponsibilities: [{ id: 'C01:1', note: 'A real unresolved consumer', disposition: 'needs-reconciliation' }],
    counts: { documentation: 1, 'handwritten-runtime': 1 },
    files: source.map((entry, index) => ({ ...entry,
      classification: index ? 'handwritten-runtime' : 'documentation',
      censusRefs: index ? ['C01.main'] : [], reason: index ? 'Executable source' : 'Prose',
    })),
  };
}
function verify(scope = manifest(), entries = source, partitions = [census]) {
  return checker.verifyScope(scope, entries, partitions);
}

test('frozen integrity is separate from unresolved responsibility/packet acceptance', () => {
  const result = verify();
  assert.equal(result.integrity, 'pass');
  assert.equal(result.complete, false);
  assert.equal(result.pendingPackets, 1);
  assert.equal(result.unreconciledResponsibilities, 1);
  assert.deepEqual(result.unmapped, []);
  const scope = manifest(); scope.files[1].censusRefs = [];
  assert.deepEqual(verify(scope).unmapped, ['src/main.js']);
});

test('independent digest covers ordering, path, mode and blob identity', () => {
  assert.equal(checker.digest(source), '2489be4ffbf9ce6bb4b53ed4a44c367ad332fcba29dadaea289f3dca41e6ab64');
  for (const key of ['path', 'mode', 'blob']) {
    const changed = structuredClone(source); changed[1][key] += 'x';
    assert.notEqual(checker.digest(changed), checker.digest(source));
  }
  assert.notEqual(checker.digest([...source].reverse()), checker.digest(source));
});

const corruptions = {
  'missing file': (x) => x.files.pop(),
  'duplicate file': (x) => x.files.push(x.files[1]),
  'extra file': (x) => x.files.push({ ...x.files[1], path: 'src/unaccounted.js' }),
  'unsorted paths': (x) => x.files.reverse(),
  'changed content': (x) => { x.files[1].blob = '9'.repeat(40); },
  'changed mode': (x) => { x.files[1].mode = '100755'; },
  'changed count': (x) => { x.source.fileCount++; },
  'changed digest': (x) => { x.source.sha256 = '0'.repeat(64); },
  'changed classification count': (x) => { x.counts.documentation++; },
  'unknown classification': (x) => { x.files[1].classification = 'trust-me'; },
  'missing rationale': (x) => { delete x.files[1].reason; },
  'missing references': (x) => { delete x.files[1].censusRefs; },
  'invented packet': (x) => { x.files[1].censusRefs = ['fake']; },
  'missing census': (x) => { x.censuses = []; },
  'duplicate census pin': (x) => x.censuses.push(x.censuses[0]),
  'changed census pin': (x) => { x.censuses[0].blob = '4'.repeat(40); },
  'changed census source': (x) => { x.censuses[0].sourceCommit = '5'.repeat(40); },
  'missing packet': (x) => { x.packets = []; },
  'duplicate packet': (x) => x.packets.push(x.packets[0]),
  'changed packet provenance': (x) => { x.packets[0].census = 'C09'; },
  'unreviewed admission': (x) => { x.packets[0].admission = 'accepted'; },
  'missing gap': (x) => { x.uncoveredResponsibilities = []; },
  'duplicate gap': (x) => x.uncoveredResponsibilities.push(x.uncoveredResponsibilities[0]),
  'changed gap': (x) => { x.uncoveredResponsibilities[0].note = 'silently repaired'; },
  'unreviewed gap closure': (x) => { x.uncoveredResponsibilities[0].disposition = 'resolved'; },
  'false complete': (x) => { x.complete = true; },
  'unknown schema': (x) => { x.schema = 'future'; },
};
for (const [name, corrupt] of Object.entries(corruptions)) {
  test(`rejects ${name}`, () => {
    const scope = manifest(); corrupt(scope);
    const result = verify(scope);
    assert.equal(result.integrity, 'fail');
    assert.equal(result.complete, false);
    assert.ok(result.errors.length > 0);
  });
}

test('candidate additions, deletions and byte changes cannot reuse the frozen pass', () => {
  assert.equal(verify(manifest(), [...source, { ...source[1], path: 'src/added.js' }]).integrity, 'fail');
  assert.equal(verify(manifest(), [source[0]]).integrity, 'fail');
  assert.equal(verify(manifest(), [source[0], { ...source[1], blob: '0'.repeat(40) }]).integrity, 'fail');
});

test('C08 documentation references survive while supplemental drafts do not count twice', () => {
  const c08 = structuredClone(census); c08.document.census_id = 'C08';
  c08.document.areas = [
    { area: 'remaining-browser-modules', packets: [{ id: 'C08.superseded' }] },
    { area: 'documentation-and-process', packets: [{ id: 'C08.docs' }] },
  ];
  c08.document.uncovered_responsibilities = [];
  const scope = manifest();
  scope.censuses.push({ id: 'C08', path: c08.path, blob: c08.blob, sourceCommit: commit });
  scope.packets.push({ id: 'C08.docs', census: 'C08', admission: 'pending' });
  assert.equal(verify(scope, source, [census, c08]).integrity, 'pass');
  assert.equal(verify(scope, source, [census, census]).integrity, 'fail');
});

test('real Git/CLI checks fixed source and rejects changed candidates and malformed input', (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-scope-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
  git('init', '--quiet');
  fs.mkdirSync(path.join(root, 'engineering/inventory/census'), { recursive: true });
  fs.mkdirSync(path.join(root, 'src'), { recursive: true });
  fs.writeFileSync(path.join(root, 'src/main.js'), 'module.exports = 7;\n');
  fs.writeFileSync(path.join(root, census.path), JSON.stringify(census.document));
  git('add', '.');
  git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '--quiet', '-m', 'fixture');
  const sha = git('rev-parse', 'HEAD');
  const entries = checker.trackedSource(root, sha);
  const scope = manifest();
  scope.source = { commit: sha, fileCount: entries.length, sha256: checker.digest(entries) };
  scope.censuses[0].blob = entries[0].blob;
  scope.files = entries.map((entry, i) => ({ ...entry, classification: i ? 'handwritten-runtime' : 'documentation', censusRefs: i ? ['C01.main'] : [], reason: 'fixture' }));
  fs.writeFileSync(path.join(root, 'engineering/inventory/remediation-scope.json'), JSON.stringify(scope));
  assert.equal(checker.inspect(root, scope).integrity, 'pass');
  const run = (args) => spawnSync(process.execPath, ['-e',
    `process.exitCode = require(${JSON.stringify(require.resolve('../scripts/nemo/remediation-scope.cjs'))}).main(JSON.parse(process.argv[1]), process.argv[2]);`,
    JSON.stringify(args), root], { encoding: 'utf8' });
  assert.equal(run(['--integrity-only']).status, 0);
  assert.equal(run([]).status, 1);
  for (const args of [['--bad'], ['--source'], ['--source', 'HEAD'], ['--integrity-only', '--integrity-only']]) {
    assert.equal(run(args).status, 1);
  }
  fs.writeFileSync(path.join(root, 'src/added.js'), 'module.exports = 9;\n');
  git('add', 'src/added.js');
  git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '--quiet', '-m', 'added');
  const candidate = git('rev-parse', 'HEAD');
  const result = run(['--integrity-only', '--source', candidate]);
  assert.equal(result.status, 1);
  assert.match(result.stdout, /missing source path: src\/added.js/);
  assert.throws(() => checker.trackedSource(root, '--help'), /full Git SHA/);
  fs.writeFileSync(path.join(root, 'engineering/inventory/remediation-scope.json'), '{');
  assert.equal(run([]).status, 1);
});
