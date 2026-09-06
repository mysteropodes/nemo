'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { checkSourceCoverage } = require('./lib/boundaries-coverage.cjs');
const { checkProfile } = require('./lib/boundaries.cjs');
const { compareSizeBaseline } = require('./lib/boundaries-ratchet.cjs');

function makeRoot(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-boundaries-coverage-'));
  fs.mkdirSync(path.join(root, 'src/js'), { recursive: true });
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  return root;
}

function write(root, file, source = 'const value = 1;\n') {
  const target = path.join(root, file);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, source);
}

// The test caller discovers disk independently of all policy inputs. The helper
// must not discover, classify, or exclude sources on this caller's behalf.
function selectSources(root, dir = 'src/js') {
  return fs.readdirSync(path.join(root, dir), { withFileTypes: true }).flatMap((entry) => {
    const file = `${dir}/${entry.name}`;
    return entry.isDirectory() ? selectSources(root, file) : [file];
  }).sort();
}

function profile(files) {
  return {
    modules: files.map((file, i) => ({
      id: `source.${i}`, dir: path.posix.dirname(file), files: [path.posix.basename(file)],
      layer: 'domain', publicApi: [path.posix.basename(file)], sizeProfile: 'small',
    })),
    sizeProfiles: { small: { warn: 5, hardMax: 10 } },
    layerRules: { domain: { allowedLayers: ['domain'] } },
  };
}

function diagnostics(report) {
  return report.violations.map(({ rule, file }) => [rule, file]);
}

test('a newly added unreferenced oversized JS file cannot escape coverage', (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/retained.js');
  const candidate = profile(['src/js/retained.js']);
  assert.equal(checkSourceCoverage(candidate, { root, sourcePaths: selectSources(root) }).ok, true);

  write(root, 'src/js/new-handwritten.js', 'void 0;\n'.repeat(700));
  // Reproduce the real gap: neither existing rule checker nor size ratchet sees
  // an unreferenced path outside their declarations, even when it is oversized.
  assert.equal(checkProfile(candidate, { root }).ok, true);
  assert.equal(compareSizeBaseline(candidate, candidate, { root }).ok, true);
  const report = checkSourceCoverage(candidate, { root, sourcePaths: selectSources(root) });
  assert.equal(report.ok, false);
  assert.deepEqual(diagnostics(report), [['coverage-unprofiled-source', 'src/js/new-handwritten.js']]);
});

test('omitting a retained file fails even if the supplied size baseline also omits it', (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/a.js');
  write(root, 'src/js/retained.js');
  const candidate = profile(['src/js/a.js']);
  assert.equal(checkProfile(candidate, { root }).ok, true);
  assert.equal(compareSizeBaseline(candidate, candidate, { root }).ok, true);
  const report = checkSourceCoverage(candidate, { root, sourcePaths: selectSources(root) });
  assert.equal(report.ok, false);
  assert.deepEqual(diagnostics(report), [['coverage-unprofiled-source', 'src/js/retained.js']]);
});

test('a legitimate new declared file passes coverage, source rules, and the size ratchet', (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/retained.js');
  const baseline = profile(['src/js/retained.js']);
  write(root, 'src/js/labs/new.js');
  const candidate = profile(['src/js/retained.js', 'src/js/labs/new.js']);
  assert.deepEqual(checkSourceCoverage(candidate, { root, sourcePaths: selectSources(root) }), {
    ok: true, sourcePathCount: 2, declaredPathCount: 2, excludedPathCount: 0, violations: [],
  });
  assert.equal(checkProfile(candidate, { root }).ok, true);
  assert.equal(compareSizeBaseline(baseline, candidate, { root }).ok, true);
});

test('an exact vendor exclusion grants nothing to new names, suffixes, or subdirectories', (t) => {
  const root = makeRoot(t);
  const vendor = 'src/js/reviewed.vendor.mjs';
  write(root, vendor);
  write(root, 'src/js/app.js');
  const candidate = profile(['src/js/app.js']);
  const exclusions = [vendor];
  assert.equal(checkSourceCoverage(candidate, { root, sourcePaths: selectSources(root), exclusions }).ok, true);

  for (const file of ['src/js/new.vendor.js', 'src/js/new.min.js', 'src/js/labs/reviewed.vendor.mjs', 'src/js/new.ts']) write(root, file);
  const report = checkSourceCoverage(candidate, { root, sourcePaths: selectSources(root), exclusions });
  assert.equal(report.ok, false);
  assert.equal(report.excludedPathCount, 1);
  assert.deepEqual(diagnostics(report), [
    ['coverage-unprofiled-source', 'src/js/labs/reviewed.vendor.mjs'],
    ['coverage-unprofiled-source', 'src/js/new.min.js'],
    ['coverage-unprofiled-source', 'src/js/new.ts'],
    ['coverage-unprofiled-source', 'src/js/new.vendor.js'],
  ]);
});

test('wildcard text is an exact filename, never a broad exemption', (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/new.js');
  const report = checkSourceCoverage(profile([]), { root, sourcePaths: selectSources(root), exclusions: ['src/js/*.js'] });
  assert.equal(report.ok, false);
  assert.deepEqual(diagnostics(report), [
    ['coverage-missing-file', 'src/js/*.js'],
    ['coverage-unselected-exclusion', 'src/js/*.js'],
    ['coverage-unprofiled-source', 'src/js/new.js'],
  ]);
});

test('duplicate selections, exclusions, and declarations have deterministic diagnostics', (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/a.js');
  write(root, 'src/js/vendor.js');
  const candidate = profile(['src/js/a.js', 'src/js/a.js']);
  candidate.modules[0].files.push('a.js'); // duplication within and across modules
  const opts = {
    root, sourcePaths: ['src/js/vendor.js', 'src/js/a.js', 'src/js/a.js'],
    exclusions: ['src/js/vendor.js', 'src/js/vendor.js'],
  };
  const original = JSON.stringify({ candidate, opts });
  const report = checkSourceCoverage(candidate, opts);
  assert.equal(report.ok, false);
  assert.deepEqual(diagnostics(report), [
    ['coverage-duplicate-declaration', 'src/js/a.js'],
    ['coverage-duplicate-selection', 'src/js/a.js'],
    ['coverage-duplicate-exclusion', 'src/js/vendor.js'],
  ]);
  assert.deepEqual(report.violations[0].detail.modules, ['source.0', 'source.0', 'source.1']);
  assert.equal(JSON.stringify({ candidate, opts }), original);
  const reordered = structuredClone(candidate);
  reordered.modules.reverse();
  assert.deepEqual(checkSourceCoverage(reordered, {
    ...opts, sourcePaths: [...opts.sourcePaths].reverse(), exclusions: [...opts.exclusions].reverse(),
  }), report);
});

test('declared and excluded paths must be selected, and cannot overlap', (t) => {
  const root = makeRoot(t);
  for (const file of ['src/js/app.js', 'src/js/extra.js', 'src/js/vendor.js']) write(root, file);
  const report = checkSourceCoverage(profile(['src/js/app.js', 'src/js/extra.js']), {
    root, sourcePaths: ['src/js/app.js'], exclusions: ['src/js/app.js', 'src/js/vendor.js'],
  });
  assert.equal(report.ok, false);
  assert.deepEqual(diagnostics(report), [
    ['coverage-excluded-declaration', 'src/js/app.js'],
    ['coverage-unselected-declaration', 'src/js/extra.js'],
    ['coverage-unselected-exclusion', 'src/js/vendor.js'],
  ]);
});

test('nonexistent sources fail whether declared, excluded, or only selected', (t) => {
  const root = makeRoot(t);
  const report = checkSourceCoverage(profile(['src/js/declared.js']), {
    root, sourcePaths: ['src/js/declared.js', 'src/js/excluded.js', 'src/js/selected.js'],
    exclusions: ['src/js/excluded.js'],
  });
  assert.equal(report.ok, false);
  assert.deepEqual(diagnostics(report), [
    ['coverage-missing-file', 'src/js/declared.js'],
    ['coverage-missing-file', 'src/js/excluded.js'],
    ['coverage-missing-file', 'src/js/selected.js'],
    ['coverage-unprofiled-source', 'src/js/selected.js'],
  ]);
});

test('rename and deletion require declarations to follow the actual source set', (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/old.js');
  const oldProfile = profile(['src/js/old.js']);
  fs.renameSync(path.join(root, 'src/js/old.js'), path.join(root, 'src/js/new.js'));
  const stale = checkSourceCoverage(oldProfile, { root, sourcePaths: selectSources(root) });
  assert.equal(stale.ok, false);
  assert.deepEqual(diagnostics(stale), [
    ['coverage-unprofiled-source', 'src/js/new.js'],
    ['coverage-missing-file', 'src/js/old.js'],
    ['coverage-unselected-declaration', 'src/js/old.js'],
  ]);
  const renamed = profile(['src/js/new.js']);
  assert.equal(checkSourceCoverage(renamed, { root, sourcePaths: selectSources(root) }).ok, true);

  fs.unlinkSync(path.join(root, 'src/js/new.js'));
  assert.deepEqual(diagnostics(checkSourceCoverage(renamed, { root, sourcePaths: selectSources(root) })), [
    ['coverage-missing-file', 'src/js/new.js'],
    ['coverage-unselected-declaration', 'src/js/new.js'],
  ]);
  assert.equal(checkSourceCoverage(profile([]), { root, sourcePaths: selectSources(root) }).ok, true);
});

test('a removed vendor input leaves a stale exclusion until the caller updates policy', (t) => {
  const root = makeRoot(t);
  const vendor = 'src/js/reviewed.vendor.js';
  write(root, vendor);
  fs.unlinkSync(path.join(root, vendor));
  const report = checkSourceCoverage(profile([]), { root, sourcePaths: selectSources(root), exclusions: [vendor] });
  assert.equal(report.ok, false);
  assert.deepEqual(diagnostics(report), [
    ['coverage-missing-file', vendor], ['coverage-unselected-exclusion', vendor],
  ]);
});

test('directories and broken symlinks cannot masquerade as source files', (t) => {
  const root = makeRoot(t);
  fs.mkdirSync(path.join(root, 'src/js/directory.js'));
  fs.symlinkSync('gone.js', path.join(root, 'src/js/broken.js'));
  const files = ['src/js/directory.js', 'src/js/broken.js'];
  const report = checkSourceCoverage(profile(files), { root, sourcePaths: files });
  assert.equal(report.ok, false);
  assert.deepEqual(diagnostics(report), [
    ['coverage-missing-file', 'src/js/broken.js'], ['coverage-not-file', 'src/js/directory.js'],
  ]);
});

test('symlink aliases are not extra physical sources or transferable exclusions', (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/app.js');
  fs.symlinkSync('app.js', path.join(root, 'src/js/reviewed.vendor.js'));
  const report = checkSourceCoverage(profile([]), {
    root, sourcePaths: selectSources(root), exclusions: ['src/js/reviewed.vendor.js'],
  });
  assert.equal(report.ok, false);
  assert.deepEqual(diagnostics(report), [
    ['coverage-aliased-source', 'src/js/app.js'], ['coverage-unprofiled-source', 'src/js/app.js'],
  ]);
  assert.deepEqual(report.violations[0].detail.paths, ['src/js/app.js', 'src/js/reviewed.vendor.js']);
  const declared = checkSourceCoverage(profile(selectSources(root)), { root, sourcePaths: selectSources(root) });
  assert.deepEqual(diagnostics(declared), [['coverage-aliased-source', 'src/js/app.js']]);
});

test('hard-link declarations fail with deterministic physical-alias diagnostics', (t) => {
  const root = makeRoot(t);
  const files = ['src/js/a.js', 'src/js/b.js'];
  write(root, files[0]);
  const [first, second] = files.map((file) => path.join(root, file));
  fs.linkSync(first, second);
  // Real hard links share file identity but retain distinct canonical paths.
  const [a, b] = [first, second].map((file) => fs.statSync(file, { bigint: true }));
  assert.equal(a.dev, b.dev);
  assert.equal(a.ino, b.ino);
  assert.notEqual(fs.realpathSync(first), fs.realpathSync(second));

  const candidate = profile(files);
  const sourcePaths = selectSources(root);
  const report = checkSourceCoverage(candidate, { root, sourcePaths });
  assert.deepEqual(report, {
    ok: false, sourcePathCount: 2, declaredPathCount: 2, excludedPathCount: 0,
    violations: [{
      rule: 'coverage-aliased-source', module: 'source.0', file: files[0], line: null,
      message: 'Multiple paths resolve to the same physical source', detail: { paths: files },
    }],
  });
  candidate.modules.reverse();
  assert.deepEqual(checkSourceCoverage(candidate, { root, sourcePaths: [...sourcePaths].reverse() }), report);
});

test('hard-link exclusions do not transfer and mixed aliases form one diagnostic group', (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/app.js');
  fs.linkSync(path.join(root, 'src/js/app.js'), path.join(root, 'src/js/reviewed.vendor.js'));
  fs.symlinkSync('reviewed.vendor.js', path.join(root, 'src/js/vendor-alias.js'));
  const report = checkSourceCoverage(profile([]), {
    root, sourcePaths: selectSources(root), exclusions: ['src/js/reviewed.vendor.js', 'src/js/vendor-alias.js'],
  });
  assert.equal(report.ok, false);
  assert.deepEqual(diagnostics(report), [
    ['coverage-aliased-source', 'src/js/app.js'], ['coverage-unprofiled-source', 'src/js/app.js'],
  ]);
  assert.deepEqual(report.violations[0].detail.paths, [
    'src/js/app.js', 'src/js/reviewed.vendor.js', 'src/js/vendor-alias.js',
  ]);
});

test('distinct physical files with identical content remain valid declarations', (t) => {
  const root = makeRoot(t);
  const files = ['src/js/a.js', 'src/js/b.js'];
  for (const file of files) write(root, file);
  const [a, b] = files.map((file) => fs.statSync(path.join(root, file), { bigint: true }));
  assert.equal(a.dev, b.dev);
  assert.notEqual(a.ino, b.ino);
  assert.deepEqual(checkSourceCoverage(profile(files), { root, sourcePaths: selectSources(root) }), {
    ok: true, sourcePathCount: 2, declaredPathCount: 2, excludedPathCount: 0, violations: [],
  });
});

test('symlink traversal outside root fails with only repository-relative diagnostics', (t) => {
  const root = makeRoot(t);
  const outside = makeRoot(t);
  write(outside, 'src/js/outside.js');
  fs.symlinkSync(path.join(outside, 'src/js'), path.join(root, 'src/js/link'), 'dir');
  const file = 'src/js/link/outside.js';
  const report = checkSourceCoverage(profile([file]), { root, sourcePaths: [file] });
  assert.equal(report.ok, false);
  assert.deepEqual(diagnostics(report), [['coverage-outside-root', file]]);
  assert.equal(JSON.stringify(report).includes(outside), false);
});

test('root-relative declarations support nested files, literal characters, and a symlinked root', (t) => {
  const root = makeRoot(t);
  const files = ['src/js/labs/café [1] #.mjs', 'src/js/entry.cjs'];
  for (const file of files) write(root, file);
  const candidate = profile(files);
  candidate.modules[0].dir = '.';
  candidate.modules[0].files = [files[0]];
  const parent = makeRoot(t);
  const alias = path.join(parent, 'root-alias');
  fs.symlinkSync(root, alias, 'dir');
  assert.equal(checkSourceCoverage(candidate, { root: alias, sourcePaths: selectSources(root) }).ok, true);
});

test('coverage does not waive existing global, import, or size violations', (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/app.js', "window.SMState;\nimport './missing.js';\n" + 'void 0;\n'.repeat(11));
  const candidate = profile(['src/js/app.js']);
  assert.equal(checkSourceCoverage(candidate, { root, sourcePaths: selectSources(root) }).ok, true);
  assert.deepEqual(checkProfile(candidate, { root }).violations.map(({ rule }) => rule).sort(), [
    'global-state', 'size', 'unresolved-local-import',
  ]);
});

test('all path inputs reject absolute, escaping, ambiguous, and malformed paths', (t) => {
  const root = makeRoot(t);
  for (const file of ['', '/abs.js', '../out.js', 'src/../out.js', './app.js', 'src//app.js', 'src/', 'C:/app.js', 'C:app.js', 'src\\app.js', 'src/\0.js', 42]) {
    const opts = { root, sourcePaths: [] };
    for (const changed of [{ ...opts, sourcePaths: [file] }, { ...opts, exclusions: [file] }]) {
      assert.throws(() => checkSourceCoverage(profile([]), changed), /normalized repository-relative path/);
    }
    const candidate = profile(['src/js/app.js']);
    candidate.modules[0].dir = file;
    assert.throws(() => checkSourceCoverage(candidate, opts), /normalized repository-relative path/);
    candidate.modules[0].dir = '.';
    candidate.modules[0].files = [file];
    assert.throws(() => checkSourceCoverage(candidate, opts), /normalized repository-relative path/);
  }
});

test('missing source selection, malformed policy, and inaccessible roots fail explicitly', (t) => {
  const root = makeRoot(t);
  const candidate = profile([]);
  for (const opts of [{ root }, { root, sourcePaths: 'src/js' }, { root, sourcePaths: [], exclusions: /vendor/ }, { root, sourcePaths: [], exclude: [] }]) {
    assert.throws(() => checkSourceCoverage(candidate, opts), /invalid source coverage:/);
  }
  assert.throws(() => checkSourceCoverage({}, { root, sourcePaths: [] }), /profile.modules/);
  assert.throws(() => checkSourceCoverage(candidate, { root: '', sourcePaths: [] }), /root must be a nonempty string/);
  assert.throws(() => checkSourceCoverage(candidate, { root: path.join(root, 'absent'), sourcePaths: [] }), /root must be an accessible directory/);
});
