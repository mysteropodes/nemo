'use strict';

// P11 (#1013) — Rust source classification and size ratchet.
//
// Discovery is the Git candidate inventory, not a filesystem walk: ignored build
// output (*/target/) is never a candidate, so generated Rust stays outside the
// selection by reviewed ignore rules instead of a filename heuristic.
//
// Only the path/size helpers run here. checkProfile is a JavaScript lexer and is
// never applied to Rust source; this packet proves discovery and size, and makes
// no claim about Cargo dependency or module-graph enforcement.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { countNonBlankLines, validateProfile } = require('../scripts/nemo/lib/boundaries.cjs');
const { discoverRepositoryFiles } = require('../scripts/nemo/lib/boundaries-repository.cjs');
const { checkSourceCoverage } = require('../scripts/nemo/lib/boundaries-coverage.cjs');
const { checkSourceSizes } = require('../scripts/nemo/lib/boundaries-size.cjs');
const { compareSizeBaseline } = require('../scripts/nemo/lib/boundaries-ratchet.cjs');

const ROOT = path.resolve(__dirname, '..');
const PROFILES = path.join(ROOT, 'engineering/boundaries/profiles');
const POLICY_FILES = ['rust.profile.json', 'rust.baseline.json', 'rust.coverage.json'];

// Read the committed bytes once. Every negative case below works on a deep clone;
// the final test proves that by re-reading these files and comparing.
const committedBytes = new Map(POLICY_FILES.map((name) => [name, fs.readFileSync(path.join(PROFILES, name))]));
const read = (name) => JSON.parse(committedBytes.get(name));
const profile = read('rust.profile.json');
const baseline = read('rust.baseline.json');
const coverage = read('rust.coverage.json');

const declaredPaths = () => profile.modules
  .flatMap((module) => module.files.map((file) => path.posix.join(module.dir, file))).sort();

function discoverRust(root = ROOT) {
  const report = discoverRepositoryFiles({ root });
  assert.equal(report.ok, true, `repository discovery failed: ${JSON.stringify(report.diagnostics)}`);
  return { report, rust: report.entries.filter((entry) => entry.path.endsWith('.rs')).map((entry) => entry.path).sort() };
}

function scratch(t, label) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), `nemo rust boundaries ${label} `));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }));
  return dir;
}

function writeSource(root, relative, nonblankLines) {
  const absolute = path.join(root, ...relative.split('/'));
  fs.mkdirSync(path.dirname(absolute), { recursive: true });
  // A blank line between each pair proves the counter measures nonblank lines,
  // not file length: a length-based check would see twice this number.
  fs.writeFileSync(absolute, `${Array.from({ length: nonblankLines }, (_, i) => `let x${i} = ${i};`).join('\n\n')}\n`);
  assert.equal(countNonBlankLines(fs.readFileSync(absolute, 'utf8')), nonblankLines);
  return relative;
}

const ORDINARY = coverage.sizePolicy.ordinaryProfile;
const LIMITS = profile.sizeProfiles[ORDINARY];

// A fixture profile is built from the committed size budget, so lowering the real
// hard maximum cannot quietly turn these negative cases into passes.
function fixtureProfile(files) {
  return {
    modules: [{ id: 'fixture.crate', layer: 'engine', dir: 'crate/src', files, publicApi: [], sizeProfile: ORDINARY }],
    sizeProfiles: structuredClone(profile.sizeProfiles),
    exceptions: [],
  };
}

test('committed Rust policy is a valid profile and its recorded budget matches the coverage policy', () => {
  validateProfile(profile);
  validateProfile(baseline);
  assert.deepEqual(profile, baseline, 'baseline must be the reviewed candidate at adoption');
  assert.equal(LIMITS.warn, coverage.sizePolicy.warn);
  assert.equal(LIMITS.hardMax, coverage.sizePolicy.hardMax);
  assert.equal(profile.layerRules, undefined, 'layer rules are not declared because no Rust dependency analyzer runs');
  assert.equal(coverage.layerEnforcement.claimed, false);
});

test('every tracked Rust source is discovered independently and adopted or exactly excluded', () => {
  const { report, rust } = discoverRust();
  const declared = declaredPaths();
  const exclusions = coverage.exclusions.map((entry) => entry.path);

  const result = checkSourceCoverage(profile, { sourcePaths: rust, exclusions, root: ROOT });
  assert.equal(result.ok, true, JSON.stringify(result.violations, null, 2));
  assert.equal(result.sourcePathCount, rust.length);
  assert.equal(result.declaredPathCount, declared.length);
  assert.equal(result.excludedPathCount, exclusions.length);

  // Guard against a discovery regression that silently returns one crate: the
  // packet names native, geometry and nemo-mcp explicitly.
  const areas = { 'src-tauri/': 0, 'geometry-wasm/': 0, 'nemo-mcp/': 0, 'vectorize-core/': 0, 'vectorize-wasm/': 0 };
  for (const file of rust) for (const prefix of Object.keys(areas)) if (file.startsWith(prefix)) areas[prefix]++;
  for (const [prefix, count] of Object.entries(areas)) assert.ok(count > 0, `no Rust discovered under ${prefix}`);

  assert.equal(rust.length, coverage.snapshotCounts.selectedSources);
  assert.equal(declared.length, coverage.snapshotCounts.declaredSources);
  assert.equal(exclusions.length, coverage.snapshotCounts.exclusions);
  assert.equal(profile.modules.length, coverage.snapshotCounts.modules);

  // Check 3, first half: emit the discovered count and the classified/excluded list.
  const byModule = profile.modules.map((module) =>
    `    ${module.id} [${module.layer}] ${module.files.length} file(s): ${module.dir}/{${module.files.join(', ')}}`);
  console.log([
    `  Rust discovery at HEAD ${report.head}:`,
    `    ${report.entries.length} repository candidate(s), ${rust.length} .rs, ${areas['src-tauri/']} native / `
      + `${areas['geometry-wasm/']} geometry / ${areas['nemo-mcp/']} mcp / `
      + `${areas['vectorize-core/'] + areas['vectorize-wasm/']} vectorize`,
    `  Classified into ${profile.modules.length} module(s):`,
    ...byModule,
    `  Excluded (${exclusions.length}): ${exclusions.length ? exclusions.join(', ') : '(none — every discovered path is adopted)'}`,
  ].join('\n'));
});

test('no declared Rust source exceeds its effective ceiling, and retained ceilings are exact', () => {
  const result = checkSourceSizes(profile, { root: ROOT });
  assert.equal(result.ok, true, JSON.stringify(result.violations, null, 2));

  const retained = new Map(coverage.retainedCeilings.map((entry) => [entry.path, entry]));
  assert.equal(profile.exceptions.length, retained.size);
  assert.equal(profile.exceptions.length, coverage.snapshotCounts.retainedCeilings);
  for (const exception of profile.exceptions) {
    const record = retained.get(exception.path);
    assert.ok(record, `retained ceiling for ${exception.path} is missing from the coverage policy`);
    assert.equal(exception.rule, 'size');
    assert.equal(exception.ceiling, record.ceiling);
    // A no-growth baseline is the measured count, not a rounded allowance.
    const actual = countNonBlankLines(fs.readFileSync(path.join(ROOT, exception.path), 'utf8'));
    assert.equal(actual, record.nonblankLinesAtAdoption);
    assert.equal(exception.ceiling, actual);
    assert.ok(exception.ceiling > LIMITS.hardMax, `${exception.path} does not need a waiver`);
  }
  const applied = result.exceptionsApplied.map((entry) => entry.path).sort();
  assert.deepEqual(applied, [...retained.keys()].sort());
});

test('committed policy has no ratchet regression against its own baseline', () => {
  const result = compareSizeBaseline(baseline, profile, { root: ROOT });
  assert.equal(result.ok, true, JSON.stringify(result.violations, null, 2));
  assert.equal(result.baselinePathCount, result.candidatePathCount);
});

test('the stricter MCP overlay stays consistent with the whole-tree profile', () => {
  const mcp = JSON.parse(fs.readFileSync(path.join(PROFILES, 'mcp-rust.profile.json'), 'utf8'));
  const declared = new Set(declaredPaths());
  const mcpPaths = mcp.modules.flatMap((module) => module.files.map((file) => path.posix.join(module.dir, file)));
  for (const file of mcpPaths) assert.ok(declared.has(file), `${file} is in the MCP overlay but not the whole-tree profile`);
  // The overlay's own guarantee is that the MCP slice carries no size waivers.
  assert.deepEqual(mcp.exceptions, []);
  const waived = new Set(profile.exceptions.map((exception) => exception.path));
  for (const file of mcpPaths) assert.equal(waived.has(file), false, `${file} acquired a waiver the MCP overlay forbids`);
});

test('an oversized in-scope .rs file fails', (t) => {
  const root = scratch(t, 'size');
  writeSource(root, 'crate/src/small.rs', 10);
  writeSource(root, 'crate/src/big.rs', LIMITS.hardMax + 1);

  const atLimit = checkSourceSizes(fixtureProfile(['small.rs']), { root });
  assert.equal(atLimit.ok, true);

  const result = checkSourceSizes(fixtureProfile(['small.rs', 'big.rs']), { root });
  assert.equal(result.ok, false);
  const violation = result.violations.find((entry) => entry.file === 'crate/src/big.rs');
  assert.ok(violation, JSON.stringify(result.violations));
  assert.equal(violation.rule, 'size');
  assert.equal(violation.detail.lines, LIMITS.hardMax + 1);
  assert.equal(violation.detail.hardMax, LIMITS.hardMax);
});

test('an undeclared .rs file fails, both as a new source and as a dropped declaration', (t) => {
  const root = scratch(t, 'coverage');
  const selected = [
    writeSource(root, 'crate/src/small.rs', 10),
    writeSource(root, 'crate/src/new.rs', 5),
  ].sort();

  const declaredOnly = checkSourceCoverage(fixtureProfile(['small.rs']), { sourcePaths: [selected[1]], root });
  assert.equal(declaredOnly.ok, true);

  const undeclared = checkSourceCoverage(fixtureProfile(['small.rs']), { sourcePaths: selected, root });
  assert.equal(undeclared.ok, false);
  assert.deepEqual(undeclared.violations.map((entry) => [entry.rule, entry.file]),
    [['coverage-unprofiled-source', 'crate/src/new.rs']]);

  // Same rule against the real tree: dropping a module orphans its real sources.
  const { rust } = discoverRust();
  const incomplete = structuredClone(profile);
  const dropped = incomplete.modules.find((module) => module.id === 'rust.geometry.engine');
  incomplete.modules = incomplete.modules.filter((module) => module.id !== dropped.id);
  incomplete.exceptions = incomplete.exceptions.filter((exception) => exception.path !== 'geometry-wasm/src/engine.rs');
  const result = checkSourceCoverage(incomplete, { sourcePaths: rust, root: ROOT });
  assert.equal(result.ok, false);
  assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source'
    && entry.file === 'geometry-wasm/src/engine.rs'), JSON.stringify(result.violations));
});

test('a raised legacy ceiling fails, per path and as a raised ordinary budget', () => {
  const perPath = structuredClone(profile);
  const raised = perPath.exceptions.find((exception) => exception.path === 'src-tauri/src/video_decode.rs');
  raised.ceiling += 1;
  const growth = compareSizeBaseline(baseline, perPath, { root: ROOT });
  assert.equal(growth.ok, false);
  const violation = growth.violations.find((entry) => entry.file === 'src-tauri/src/video_decode.rs');
  assert.ok(violation, JSON.stringify(growth.violations));
  assert.equal(violation.rule, 'size-baseline-growth');
  assert.equal(violation.detail.candidateCeiling, violation.detail.priorCeiling + 1);

  const ordinary = structuredClone(profile);
  ordinary.sizeProfiles[ORDINARY].hardMax += 1;
  const widened = compareSizeBaseline(baseline, ordinary, { root: ROOT });
  assert.equal(widened.ok, false);
  assert.ok(widened.violations.some((entry) => entry.rule === 'size-baseline-profile-growth'),
    JSON.stringify(widened.violations));

  // Deleting a waiver is the opposite of growth: the allowance drops to the
  // ordinary budget, so the ratchet records a reduction. The debt does not
  // vanish quietly, though — the size check then fails on the same file.
  const dropped = structuredClone(profile);
  dropped.exceptions = dropped.exceptions.filter((exception) => exception.path !== 'geometry-wasm/src/fill.rs');
  const removal = compareSizeBaseline(baseline, dropped, { root: ROOT });
  assert.equal(removal.ok, true);
  assert.deepEqual(removal.reductions.map((entry) => [entry.file, entry.priorCeiling, entry.candidateCeiling]),
    [['geometry-wasm/src/fill.rs', 998, LIMITS.hardMax]]);
  const unwaived = checkSourceSizes(dropped, { root: ROOT });
  assert.equal(unwaived.ok, false);
  assert.deepEqual(unwaived.violations.map((entry) => [entry.rule, entry.file]),
    [['size', 'geometry-wasm/src/fill.rs']]);
});

test('generated build output stays outside the selection because it is ignored, not name-matched', (t) => {
  const root = scratch(t, 'ignored');
  const git = (...args) => {
    const result = spawnSync('git', ['-C', root, ...args], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
  };
  git('init', '-q');
  git('config', 'user.name', 'P11 fixture');
  git('config', 'user.email', 'p11@example.invalid');
  fs.writeFileSync(path.join(root, '.gitignore'), 'target/\n');
  writeSource(root, 'src/lib.rs', 4);
  git('add', '-A');
  git('-c', 'commit.gpgsign=false', 'commit', '-q', '-m', 'fixture');
  writeSource(root, 'target/generated.rs', 4);

  const { report, rust } = discoverRust(root);
  assert.deepEqual(rust, ['src/lib.rs']);
  assert.equal(report.entries.some((entry) => entry.path === 'target/generated.rs'), false);
  assert.ok(fs.existsSync(path.join(root, 'target/generated.rs')), 'the generated file must really exist on disk');

  // The exclusion is the ignore rule, not the directory name: an ignored file
  // that Git tracks anyway is still a candidate, and would need real coverage.
  git('add', '-f', 'target/generated.rs');
  git('-c', 'commit.gpgsign=false', 'commit', '-q', '-m', 'force-add');
  assert.deepEqual(discoverRust(root).rust, ['src/lib.rs', 'target/generated.rs']);
});

test('negative fixtures left the committed policy, retained ceilings and exclusions untouched', () => {
  for (const name of POLICY_FILES) {
    assert.deepEqual(fs.readFileSync(path.join(PROFILES, name)), committedBytes.get(name),
      `${name} was modified while the negative cases ran`);
  }
  // structuredClone, not a shared reference: the in-memory policy is intact too.
  assert.deepEqual(profile, read('rust.profile.json'));
  assert.deepEqual(baseline, read('rust.baseline.json'));
  assert.deepEqual(coverage, read('rust.coverage.json'));
  assert.equal(profile.sizeProfiles[ORDINARY].hardMax, LIMITS.hardMax);
  assert.deepEqual(coverage.exclusions, []);
  assert.deepEqual(profile.exceptions.map((exception) => [exception.path, exception.ceiling]).sort(),
    coverage.retainedCeilings.map((entry) => [entry.path, entry.ceiling]).sort());
});
