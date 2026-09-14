'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync, spawnSync } = require('node:child_process');
const checker = require('../scripts/nemo/remediation-scope.cjs');
const census = require('../scripts/nemo/lib/remediation-scope-census.cjs');

const commit = 'a'.repeat(40);
const older = 'b'.repeat(40);
const source = [
  { path: 'README.md', mode: '100644', blob: '1'.repeat(40) },
  { path: 'src/main.js', mode: '100644', blob: '2'.repeat(40) },
];
const partition = {
  path: 'engineering/inventory/census/C01.json', blob: '3'.repeat(40),
  document: { census_id: 'C01', source_sha: commit,
    areas: [{ area: 'runtime', packets: [{ id: 'C01.main', files: ['src/main.js:1-2'] }] }],
    uncovered_responsibilities: ['A real unresolved consumer'],
  },
};
const coverage = [{ path: 'src/main.js', lines: 4, uncovered: 1, overlap: 0, spans: [[3, 4]] }];
function manifest() {
  return {
    schema: 'nemo.remediation-scope/2', complete: false,
    source: { commit, fileCount: 2, sha256: checker.digest(source) },
    amendments: [
      { id: 'P03A', issue: 1116, commit: older, fileCount: 1, sha256: '0'.repeat(64), reason: 'first' },
      { id: 'P03B', issue: 1170, commit, from: older, fileCount: 2, sha256: checker.digest(source),
        added: ['src/main.js'], removed: [], modified: [], reason: 'second' },
    ],
    censuses: [{ id: 'C01', path: partition.path, blob: partition.blob, sourceCommit: commit }],
    packets: [{ id: 'C01.main', census: 'C01', admission: 'pending' }],
    uncoveredResponsibilities: [{ id: 'C01:1', note: 'A real unresolved consumer', disposition: 'needs-reconciliation' }],
    rangeCoverage: structuredClone(coverage),
    counts: { documentation: 1, 'handwritten-runtime': 1 },
    files: source.map((entry, index) => ({ ...entry,
      classification: index ? 'handwritten-runtime' : 'documentation',
      censusRefs: index ? ['C01.main'] : [], reason: index ? 'Executable source' : 'Prose',
    })),
  };
}
const expected = { previous: [source[0]], rangeCoverage: coverage };
function verify(scope = manifest(), entries = source, partitions = [partition], extra = expected) {
  return checker.verifyScope(scope, entries, partitions, extra);
}
const leaf = { leaf: 'P20', issue: 1022, pullRequest: 1109, evidence: 'merged' };

test('frozen integrity is separate from unresolved responsibility/packet/span acceptance', () => {
  const result = verify();
  assert.equal(result.integrity, 'pass');
  assert.equal(result.complete, false);
  assert.deepEqual(result.remaining, { pendingPackets: 1, unreconciledResponsibilities: 1, unmappedPaths: 0, undispositionedSpans: 1 });
  assert.deepEqual(result.unmapped, []);
  const scope = manifest(); scope.files[1].censusRefs = [];
  assert.deepEqual(verify(scope).unmapped, ['src/main.js']);
  assert.equal(verify(scope).integrity, 'pass');
});

test('reviewed dispositions with evidence are accepted and completeness needs all four queues empty', () => {
  const scope = manifest();
  scope.packets[0] = { ...scope.packets[0], admission: 'covered', ...leaf };
  scope.uncoveredResponsibilities[0].disposition = 'covered-by-packet';
  scope.uncoveredResponsibilities[0].packets = ['C01.main'];
  scope.uncoveredResponsibilities[0].evidence = 'declared';
  scope.files[1].censusRefs = [];
  scope.files[1].coverage = { kind: 'leaf', ...leaf };
  scope.rangeCoverage[0].dispositions = [{ span: [3, 4], kind: 'admitted', issue: 1200, evidence: 'leaf opened' }];
  const open = verify(scope);
  assert.equal(open.integrity, 'pass');
  assert.deepEqual(open.remaining, { pendingPackets: 0, unreconciledResponsibilities: 0, unmappedPaths: 0, undispositionedSpans: 0 });
  assert.equal(open.complete, false);
  scope.complete = true;
  assert.equal(verify(scope).complete, true);
  scope.rangeCoverage[0].dispositions = [];
  assert.equal(verify(scope).integrity, 'fail');
  assert.equal(verify(scope).complete, false);
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
  'covered packet without evidence': (x) => { x.packets[0].admission = 'covered'; },
  'covered packet with a bare leaf name': (x) => { Object.assign(x.packets[0], { admission: 'covered', leaf: 'P20' }); },
  'admitted packet without an issue': (x) => { x.packets[0].admission = 'admitted'; },
  'deferred packet without a reason': (x) => { x.packets[0].admission = 'deferred'; },
  'missing gap': (x) => { x.uncoveredResponsibilities = []; },
  'duplicate gap': (x) => x.uncoveredResponsibilities.push(x.uncoveredResponsibilities[0]),
  'changed gap': (x) => { x.uncoveredResponsibilities[0].note = 'silently repaired'; },
  'unreviewed gap closure': (x) => { x.uncoveredResponsibilities[0].disposition = 'closed'; },
  'gap closure without evidence': (x) => { x.uncoveredResponsibilities[0].disposition = 'resolved'; },
  'gap covered by an unknown packet': (x) => { Object.assign(x.uncoveredResponsibilities[0],
    { disposition: 'covered-by-packet', packets: ['C01.other'], evidence: 'x' }); },
  'coverage of an unknown kind': (x) => { x.files[1].censusRefs = []; x.files[1].coverage = { kind: 'trust-me' }; },
  'leaf coverage without a pull request': (x) => { x.files[1].censusRefs = []; x.files[1].coverage = { kind: 'leaf', leaf: 'P20', issue: 1022, evidence: 'x' }; },
  'oracle coverage citing an unknown packet': (x) => { x.files[1].censusRefs = []; x.files[1].coverage = { kind: 'oracle', packets: ['nope'], evidence: 'x' }; },
  'missing amendments': (x) => { delete x.amendments; },
  'broken amendment chain': (x) => { x.amendments[1].from = '6'.repeat(40); },
  'amendment not describing the source': (x) => { x.amendments[1].fileCount = 3; },
  'amendment delta disagreeing with Git': (x) => { x.amendments[1].added = []; },
  'range coverage edited by hand': (x) => { x.rangeCoverage[0].uncovered = 0; },
  'range coverage dropped': (x) => { x.rangeCoverage = []; },
  'span disposition on an unknown span': (x) => { x.rangeCoverage[0].dispositions = [{ span: [1, 1], kind: 'boundary', evidence: 'x' }]; },
  'span disposition without evidence': (x) => { x.rangeCoverage[0].dispositions = [{ span: [3, 4], kind: 'boundary' }]; },
  'false complete': (x) => { x.complete = true; },
  'unknown schema': (x) => { x.schema = 'nemo.remediation-scope/1'; },
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

test('candidate additions, deletions, byte changes and unpinned censuses cannot reuse the frozen pass', () => {
  assert.equal(verify(manifest(), [...source, { ...source[1], path: 'src/added.js' }]).integrity, 'fail');
  assert.equal(verify(manifest(), [source[0]]).integrity, 'fail');
  assert.equal(verify(manifest(), [source[0], { ...source[1], blob: '0'.repeat(40) }]).integrity, 'fail');
  const unpinned = { path: 'engineering/inventory/census/C02.json', mode: '100644', blob: '7'.repeat(40) };
  const scope = manifest();
  scope.files.push({ ...unpinned, classification: 'static-data', censusRefs: [], reason: 'census' });
  scope.source = { commit, fileCount: 3, sha256: checker.digest([...source, unpinned]) };
  scope.amendments[1] = { ...scope.amendments[1], fileCount: 3, sha256: scope.source.sha256, added: ['engineering/inventory/census/C02.json', 'src/main.js'] };
  scope.counts['static-data'] = 1;
  const result = verify(scope, [...source, unpinned]);
  assert.match(result.errors.join('\n'), /census file not pinned/);
});

test('C08 documentation references survive while supplemental drafts do not count twice', () => {
  const c08 = structuredClone(partition); c08.document.census_id = 'C08';
  c08.document.areas = [
    { area: 'remaining-browser-modules', packets: [{ id: 'C08.superseded' }] },
    { area: 'documentation-and-process', packets: [{ id: 'C08.docs' }] },
  ];
  c08.document.uncovered_responsibilities = [];
  const scope = manifest();
  scope.censuses.push({ id: 'C08', path: c08.path, blob: c08.blob, sourceCommit: commit });
  scope.packets.push({ id: 'C08.docs', census: 'C08', admission: 'pending' });
  assert.equal(verify(scope, source, [partition, c08]).integrity, 'pass');
  assert.equal(verify(scope, source, [partition, partition]).integrity, 'fail');
});

test('census declarations parse every recorded shape without inventing paths', () => {
  const parse = (text) => census.parseDeclaration(text);
  assert.deepEqual(parse('src/js/project.js:20,25-30,40-50'), [{ path: 'src/js/project.js', ranges: [[20, 20], [25, 30], [40, 50]] }]);
  assert.deepEqual(parse('src/js/timeline.js:459'), [{ path: 'src/js/timeline.js', ranges: [[459, 459]] }]);
  assert.deepEqual(parse('src/js/motion.js:11141-11426 (selectLayerFromGrid, renderTimelineMotion)'), [{ path: 'src/js/motion.js', ranges: [[11141, 11426]] }]);
  assert.deepEqual(parse('src/js/linked-media.js:~40-396'), [{ path: 'src/js/linked-media.js', ranges: [[40, 396]] }]);
  assert.deepEqual(parse('src-tauri/src/lib.rs: start_tablet_pressure_monitor (136-175, macOS only)'), [{ path: 'src-tauri/src/lib.rs', ranges: [[136, 175]] }]);
  assert.deepEqual(parse('src/js/figma-import.js: convertFileJson, walkNode, resolveFontKey'), [{ path: 'src/js/figma-import.js', ranges: null }]);
  assert.deepEqual(parse('src/js/motion-graph.js (whole, 726 lines)'), [{ path: 'src/js/motion-graph.js', ranges: null }]);
  assert.deepEqual(parse('package.json, .c8rc.json, engineering/ci/README.md (3)'),
    [{ path: 'package.json', ranges: null }, { path: '.c8rc.json', ranges: null }, { path: 'engineering/ci/README.md', ranges: null }]);
  assert.deepEqual(parse('engineering/remediation/** (14 files)'), [{ path: 'engineering/remediation/**', ranges: null }]);
});

test('line ranges written against an older tree are mapped through real git hunk shapes', () => {
  const hunks = census.parseHunks('@@ -5,2 +4,0 @@\n-x\n-y\n@@ -10 +8,3 @@\n-z\n+a\n+b\n+c\n');
  assert.deepEqual(hunks, [[5, 2, 4, 0], [10, 1, 8, 3]]);
  assert.equal(census.mapLine(hunks, 4), 4);
  assert.equal(census.mapLine(hunks, 5), null);
  assert.equal(census.mapLine(hunks, 7), 5);
  assert.equal(census.mapLine(hunks, 10), null);
  assert.equal(census.mapLine(hunks, 11), 11);
  assert.deepEqual(census.mapRanges(hunks, [[3, 8]]), [[3, 6]]);
  // Insert-only after old line 5 (`-5,0 +6,3`): 5 stays, 6 shifts by the three new lines.
  assert.equal(census.mapLine([[5, 0, 6, 3]], 5), 5);
  assert.equal(census.mapLine([[5, 0, 6, 3]], 6), 9);
  // Insert at the top (`-0,0 +1`) and delete the first line (`-1 +0,0`).
  assert.equal(census.mapLine([[0, 0, 1, 1]], 1), 2);
  assert.equal(census.mapLine([[1, 1, 0, 0]], 1), null);
  assert.equal(census.mapLine([[1, 1, 0, 0]], 2), 1);
});

test('range coverage reports code gaps, ignores layout-only spans and whole-file claims', () => {
  const body = ['a', 'b', '', '}', 'c', 'd', 'e', '', 'f'];
  const tools = { lines: (file) => (file === 'x.js' ? body : null), hunks: () => [] };
  const rows = [
    { packet: 'C.a', census: 'C', sourceCommit: commit, path: 'x.js', ranges: [[1, 2]] },
    { packet: 'C.b', census: 'C', sourceCommit: commit, path: 'x.js', ranges: [[2, 2], [5, 5]] },
    { packet: 'C.c', census: 'C', sourceCommit: commit, path: 'whole.js', ranges: null },
    { packet: 'C.d', census: 'C', sourceCommit: commit, path: 'whole.js', ranges: [[1, 3]] },
    { packet: 'C.e', census: 'C', sourceCommit: commit, path: 'missing.js', ranges: [[1, 3]] },
  ];
  assert.deepEqual(census.rangeCoverage(rows, tools), [{ path: 'x.js', lines: 9, uncovered: 3, overlap: 1, spans: [[6, 9]] }]);
});

const fixtureDocument = (sha) => ({ ...partition.document, source_sha: sha });
function repo(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-scope-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const git = (...args) => execFileSync('git', ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', ...args], { cwd: root, encoding: 'utf8' }).trim();
  git('init', '--quiet');
  fs.mkdirSync(path.join(root, 'engineering/inventory/census'), { recursive: true });
  fs.mkdirSync(path.join(root, 'src'), { recursive: true });
  fs.writeFileSync(path.join(root, 'src/main.js'), 'module.exports = 7;\nconst x = 1;\nconst y = 2;\nconst z = 3;\n');
  git('add', '.');
  git('commit', '--quiet', '-m', 'source');
  const sha = git('rev-parse', 'HEAD');
  fs.writeFileSync(path.join(root, partition.path), JSON.stringify(fixtureDocument(sha)));
  git('add', '.');
  git('commit', '--quiet', '-m', 'census');
  return { root, git, censusSha: git('rev-parse', 'HEAD') };
}
const run = (root, args) => spawnSync(process.execPath, ['-e',
  `process.exitCode = require(${JSON.stringify(require.resolve('../scripts/nemo/remediation-scope.cjs'))}).main(JSON.parse(process.argv[1]), process.argv[2]);`,
  JSON.stringify(args), root], { encoding: 'utf8' });
const readIndex = (root) => JSON.parse(fs.readFileSync(path.join(root, 'engineering/inventory/remediation-scope.json'), 'utf8'));

test('real Git refreeze chains amendments, keeps reviewed fields and rejects drifted candidates', (t) => {
  const { root, git, censusSha } = repo(t);
  const entries = checker.trackedSource(root, censusSha);
  const seed = { schema: 'nemo.remediation-scope/1', complete: false, issue: 'x',
    source: { commit: censusSha, fileCount: entries.length, sha256: checker.digest(entries) },
    censuses: [{ id: 'C01', path: partition.path, blob: entries[0].blob, sourceCommit: censusSha }],
    packets: [{ id: 'C01.main', census: 'C01', admission: 'pending' }],
    uncoveredResponsibilities: [{ id: 'C01:1', note: 'A real unresolved consumer', disposition: 'needs-reconciliation' }],
    counts: { 'static-data': 1, 'handwritten-runtime': 1 },
    files: entries.map((entry, i) => ({ ...entry, classification: i ? 'handwritten-runtime' : 'static-data', censusRefs: i ? ['C01.main'] : [], reason: 'fixture' })) };
  fs.writeFileSync(path.join(root, 'engineering/inventory/remediation-scope.json'), JSON.stringify(seed));
  fs.writeFileSync(path.join(root, 'src/added.js'), 'module.exports = 9;\n');
  git('add', 'src/added.js');
  git('commit', '--quiet', '-m', 'added');
  const candidate = git('rev-parse', 'HEAD');
  assert.equal(run(root, ['--refreeze', candidate, '--id', 'P03B', '--issue', '1170', '--reason', 'test']).status, 0);
  const scope = readIndex(root);
  assert.equal(scope.schema, 'nemo.remediation-scope/2');
  assert.deepEqual(scope.amendments.map((entry) => [entry.id, entry.commit]), [['P03A', censusSha], ['P03B', candidate]]);
  assert.deepEqual(scope.amendments[1].added, ['src/added.js']);
  assert.equal(scope.files.find((entry) => entry.path === 'src/added.js').classification, 'handwritten-runtime');
  assert.equal(scope.files.find((entry) => entry.path === 'src/main.js').reason, 'fixture');
  assert.deepEqual(scope.rangeCoverage, [{ path: 'src/main.js', lines: 4, uncovered: 2, overlap: 0, spans: [[3, 4]] }]);
  assert.equal(run(root, ['--integrity-only']).status, 0);
  assert.equal(run(root, []).status, 1);
  const unmapped = JSON.parse(run(root, ['--integrity-only']).stdout);
  assert.deepEqual(unmapped.unmapped, ['src/added.js']);
  scope.files.find((entry) => entry.path === 'src/added.js').coverage = { kind: 'leaf', ...leaf };
  fs.writeFileSync(path.join(root, 'engineering/inventory/remediation-scope.json'), JSON.stringify(scope));
  assert.deepEqual(JSON.parse(run(root, ['--integrity-only']).stdout).unmapped, []);
  for (const args of [['--bad'], ['--source'], ['--source', 'HEAD'], ['--integrity-only', '--integrity-only'],
    ['--refreeze', candidate], ['--refreeze', candidate, '--id', 'X', '--issue', 'n', '--reason', 'r'], ['--id', 'X']]) {
    assert.equal(run(root, args).status, 1, args.join(' '));
  }
  fs.appendFileSync(path.join(root, 'src/main.js'), 'const w = 4;\n');
  git('commit', '--quiet', '-am', 'drift');
  const drifted = git('rev-parse', 'HEAD');
  const result = run(root, ['--integrity-only', '--source', drifted]);
  assert.equal(result.status, 1);
  assert.match(result.stdout, /source identity mismatch: src\/main.js/);
  assert.equal(run(root, ['--refreeze', drifted, '--id', 'P03C', '--issue', '1171', '--reason', 'again']).status, 0);
  const again = readIndex(root);
  assert.equal(again.amendments.length, 3);
  assert.deepEqual(again.amendments[2].modified, ['src/main.js']);
  assert.equal(again.files.find((entry) => entry.path === 'src/added.js').coverage.leaf, 'P20');
  again.amendments[2].modified = [];
  fs.writeFileSync(path.join(root, 'engineering/inventory/remediation-scope.json'), JSON.stringify(again));
  assert.match(run(root, ['--integrity-only']).stdout, /amendment delta does not match/);
  assert.throws(() => checker.trackedSource(root, '--help'), /full Git SHA/);
  fs.writeFileSync(path.join(root, 'engineering/inventory/remediation-scope.json'), '{');
  assert.equal(run(root, []).status, 1);
});
