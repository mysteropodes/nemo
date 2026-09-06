'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { checkApplicationSize } = require('./lib/boundaries-application.cjs');
const { checkSourceSizes } = require('./lib/boundaries-size.cjs');
const { compareSizeBaseline } = require('./lib/boundaries-ratchet.cjs');
const ROOT = path.resolve(__dirname, '../..');
const now = new Date('2026-09-06T12:00:00Z');
function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-source-size-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  return root;
}
function write(root, file, source) {
  const target = path.join(root, file);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, source);
}
function profile(file, hardMax = 2, warn = Math.min(1, hardMax)) {
  return { modules: [{ id: 'sample', layer: 'domain', dir: path.posix.dirname(file),
    files: [path.posix.basename(file)], publicApi: [], sizeProfile: 'bounded' }],
  sizeProfiles: { bounded: { warn, hardMax } }, exceptions: [] };
}
function exception(file, rule = 'size') {
  return { path: file, rule, ...(rule === 'size' ? { ceiling: 3 } : {}),
    owner: 'maintainer', issue: '901', expires: '2026-09-07', reason: 'Regression fixture' };
}
const cases = [
  ['css', 'a { background: url(/x); }\n', 'src/css/style.css'],
  ['py', "# user's note\npass\n", 'scripts/dev_server.py'],
  ['sh', "# user's note\n:\n", 'scripts/rebuild-ffmpeg-lgpl.sh'],
  ['html', '<p>x</p>\n', 'src/index.html'],
];
for (const [language, minimal, real] of cases) {
  for (const [kind, file, source] of [
    ['minimal', `sample.${language}`, minimal],
    ['real', real, fs.readFileSync(path.join(ROOT, real), 'utf8')],
  ]) test(`${language}/${kind}: source at ceiling and +1 growth`, (t) => {
    const root = fixture(t);
    const count = source.split(/\r?\n/).filter((line) => line.trim()).length;
    const policy = profile(file, count);
    write(root, file, source);
    const report = checkSourceSizes(policy, { root, now });
    assert.equal(report.ok, true);
    assert.equal(report.warnings[0]?.detail.lines, count > 1 ? count : undefined);
    write(root, file, source + '\ncounted extra line\n');
    const grown = checkSourceSizes(policy, { root, now });
    assert.equal(grown.ok, false);
    assert.deepEqual(grown.violations[0].detail, { lines: count + 1, hardMax: count });
  });
}
for (const [language, source] of [
  ['rs', "fn borrow<'a>(s: &'a str) -> &'a str { s }\n// user's note\n"],
  ['wgsl', '@fragment fn main() -> @location(0) vec4f { return vec4f(1.0); }\n// counted\n'],
  ...cases.map(([language, source]) => [language, source]),
]) for (const eol of ['\n', '\r\n']) test(`${language}: ${JSON.stringify(eol)} text counting`, (t) => {
  const root = fixture(t), file = `sample.${language}`;
  const sourceLines = source.trimEnd().split('\n');
  write(root, file, [' \t', ...sourceLines, '', ''].join(eol));
  assert.equal(checkSourceSizes(profile(file, sourceLines.length), { root, now }).ok, true);
});

test('supported JS reports preserve warnings, ceilings, expiry and exception metadata', (t) => {
  const root = fixture(t), file = 'sample.cjs';
  for (const count of [0, 1, 2, 3, 4]) for (const mode of ['none', 'active', 'expired', 'graph']) {
    write(root, file, 'const value = 1;\n'.repeat(count));
    const policy = profile(file);
    if (mode !== 'none') policy.exceptions.push(exception(file, mode === 'graph' ? 'cycle' : 'size'));
    if (mode === 'expired') policy.exceptions[0].expires = '2026-09-06';
    assert.deepEqual(checkSourceSizes(policy, { root, now }), checkApplicationSize(policy, { root, now }));
  }
  const policy = profile(file);
  policy.exceptions = [exception(file, 'cycle'), exception(file)];
  policy.exceptions[0].expires = '2026-09-06';
  assert.deepEqual(checkSourceSizes(policy, { root, now }), checkApplicationSize(policy, { root, now }));
  const exactExpiry = new Date('2026-09-07T00:00:00Z');
  assert.equal(checkSourceSizes(policy, { root, now: exactExpiry }).violations.filter(v => v.rule === 'expired-exception').length, 2);
});

test('real app profile has exact wrapper report parity', () => {
  const policy = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/boundaries/profiles/app-js.profile.json')));
  assert.deepEqual(checkSourceSizes(policy, { root: ROOT, now }), checkApplicationSize(policy, { root: ROOT, now }));
});

test('size acceptance and independent baseline ratchet remain separate', (t) => {
  const root = fixture(t), file = 'sample.py';
  write(root, file, "# user's note\npass\n");
  const baseline = profile(file, 3);
  for (const ceiling of [1, 2, 4]) {
    const candidate = profile(file, ceiling);
    const size = checkSourceSizes(candidate, { root, now });
    const ratchet = compareSizeBaseline(baseline, candidate, { root });
    assert.equal(size.ok, ceiling >= 2);
    assert.equal(ratchet.ok, ceiling <= 3);
    if (ceiling < 3) assert.equal(ratchet.reductions[0].candidateCeiling, ceiling);
  }
  baseline.exceptions = [exception(file)];
  const candidate = structuredClone(baseline);
  candidate.exceptions[0].ceiling++;
  assert.equal(checkSourceSizes(candidate, { root, now }).ok, true);
  assert.equal(compareSizeBaseline(baseline, candidate, { root }).ok, false);
});

test('exact-path exception cannot shield a sibling and never reports graph waivers', (t) => {
  const root = fixture(t), policy = profile('a.py', 1);
  policy.modules[0].files.push('b.py');
  for (const file of policy.modules[0].files) write(root, file, "# user's note\npass\n");
  policy.exceptions = [exception('a.py'), exception('b.py', 'cycle')];
  const report = checkSourceSizes(policy, { root, now });
  assert.deepEqual(report.violations.map(v => v.file), ['b.py']);
  assert.deepEqual(report.exceptionsApplied, [{ ...policy.exceptions[0], actualLines: 2 }]);
});

test('malformed profile, exception metadata and clocks fail closed', (t) => {
  const root = fixture(t), file = 'a.py';
  write(root, file, 'pass\n');
  for (const key of ['owner', 'issue', 'reason', 'expires']) {
    const policy = profile(file); policy.exceptions = [exception(file)];
    delete policy.exceptions[0][key];
    assert.throws(() => checkSourceSizes(policy, { root, now }), /invalid profile/);
  }
  for (const expires of ['2026-02-30', 'tomorrow']) {
    const policy = profile(file); policy.exceptions = [{ ...exception(file), expires }];
    assert.throws(() => checkSourceSizes(policy, { root, now }), /invalid exception expiry/);
  }
  const policy = profile(file); policy.modules.push({ ...policy.modules[0], id: 'other' });
  assert.throws(() => checkSourceSizes(policy, { root, now }), /multiple modules/);
  for (const clock of [new Date(NaN), '2026-09-06', null, 0]) {
    assert.throws(() => checkSourceSizes(profile(file), { root, now: clock }), /invalid check clock/);
  }
});

test('unsafe paths, missing sources, directories, aliases and invalid UTF-8 fail closed', (t) => {
  const root = fixture(t), outside = fixture(t);
  write(root, 'a.py', 'pass\n'); write(outside, 'external.py', 'pass\n');
  for (const file of ['../external.py', '/absolute.py', 'C:/drive.py', 'bad\u0000.py', 'a\\b.py']) {
    assert.throws(() => checkSourceSizes(profile(file), { root, now }));
  }
  assert.throws(() => checkSourceSizes(profile('missing.py'), { root, now }));
  fs.mkdirSync(path.join(root, 'directory'));
  assert.throws(() => checkSourceSizes(profile('directory'), { root, now }), /regular file/);
  fs.symlinkSync(path.join(outside, 'external.py'), path.join(root, 'external.py'));
  fs.symlinkSync(outside, path.join(root, 'linked'));
  fs.symlinkSync(path.join(root, 'a.py'), path.join(root, 'alias.py'));
  for (const file of ['external.py', 'linked/external.py', 'alias.py']) {
    assert.throws(() => checkSourceSizes(profile(file), { root, now }), /symlink/);
  }
  fs.linkSync(path.join(root, 'a.py'), path.join(root, 'hard.py'));
  const policy = profile('a.py'); policy.modules[0].files.push('hard.py');
  assert.throws(() => checkSourceSizes(policy, { root, now }), /physical source/);
  write(root, 'invalid.py', Buffer.from([0xff]));
  assert.throws(() => checkSourceSizes(profile('invalid.py'), { root, now }), /utf-8/i);
  for (const invalidRoot of ['', null, 42, path.join(root, 'a.py')]) {
    assert.throws(() => checkSourceSizes(profile('a.py'), { root: invalidRoot, now }));
  }
});

test('preflight rejects an unsafe later path before any source read', (t) => {
  const root = fixture(t), outside = fixture(t);
  write(root, 'a.py', 'pass\n');
  fs.symlinkSync(outside, path.join(root, 'escape'));
  const policy = profile('a.py'); policy.modules[0].files.push('escape/external.py');
  const original = fs.readFileSync;
  let reads = 0;
  fs.readFileSync = () => { reads++; throw new Error('unexpected content read'); };
  try { assert.throws(() => checkSourceSizes(policy, { root, now }), /symlink/); }
  finally { fs.readFileSync = original; }
  assert.equal(reads, 0);
});

test('source remains opaque, with no evaluation or import resolution', (t) => {
  const root = fixture(t), file = 'opaque.sh';
  write(root, file, "require('/outside/never-read');\nwindow.SMState = import(dynamic);\n'\n");
  assert.equal(checkSourceSizes(profile(file, 3), { root, now }).ok, true);
  assert.equal(fs.readdirSync(root).length, 1);
});
