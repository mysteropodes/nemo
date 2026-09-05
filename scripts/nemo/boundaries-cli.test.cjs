'use strict';

const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const CLI = path.join(__dirname, 'boundaries.cjs');
const TAIL = 'zz-cli-tail-sentinel.cjs';

function fixture(t, count = 1) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-boundaries-cli-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const files = Array.from({ length: count }, (_, i) => i === count - 1
    ? TAIL : `module-${i}-${'x'.repeat(120)}.cjs`);
  for (const file of files) fs.writeFileSync(path.join(root, file), 'module.exports = 1;\n// second line\n');
  const profile = {
    modules: [{ id: 'fixture', layer: 'domain', dir: '.', files, publicApi: [], sizeProfile: 'small' }],
    sizeProfiles: { small: { warn: 2, hardMax: 2 } },
  };
  return { root, profile };
}

function writeJson(root, name, value) {
  const file = path.join(root, name);
  fs.writeFileSync(file, JSON.stringify(value));
  return file;
}

function run(args, options = {}) {
  const result = spawnSync(process.execPath, [CLI, ...args], {
    encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, timeout: 15_000, ...options,
  });
  assert.ifError(result.error);
  assert.equal(result.signal, null);
  return result;
}

function assertError(result, expected) {
  assert.equal(result.status, 2);
  assert.equal(result.stdout, '', 'input errors must not print a report');
  if (typeof expected === 'string') {
    assert.equal(Buffer.byteLength(result.stderr), Buffer.byteLength(expected), 'complete diagnostic byte count');
    assert.equal(result.stderr, expected);
  }
  else assert.match(result.stderr, expected);
  assert.equal(result.stderr.split('\n').length, 2, 'stop after the first diagnostic');
}

test('large reports drain through subprocess pipes for both exit 0 and exit 1', async (t) => {
  const { root, profile } = fixture(t, 1200);
  for (const [status, hardMax, field] of [[0, 2, 'warnings'], [1, 1, 'violations']]) {
    profile.sizeProfiles.small = { warn: 0, hardMax };
    const profilePath = writeJson(root, 'profile.json', profile);
    for (const json of [true, false]) {
      await t.test(`exit ${status}, ${json ? 'JSON' : 'text'}`, () => {
        const args = [profilePath, '--root', root, ...(json ? ['--json'] : [])];
        const outputPath = path.join(root, 'report.out');
        const fd = fs.openSync(outputPath, 'w');
        let fileResult;
        try {
          fileResult = run(args, { stdio: ['ignore', fd, 'pipe'] });
        } finally {
          fs.closeSync(fd);
        }
        const expected = fs.readFileSync(outputPath, 'utf8');
        const piped = run(args, { stdio: ['ignore', 'pipe', 'pipe'] });
        assert.equal(fileResult.status, status);
        assert.equal(fileResult.stderr, '');
        assert.equal(piped.status, status);
        assert.equal(piped.stderr, '');
        assert.ok(Buffer.byteLength(expected) > 64 * 1024, 'fixture must exceed the reported pipe boundary');
        // A real child-process pipe, compared with the complete file-descriptor output.
        assert.equal(Buffer.byteLength(piped.stdout), Buffer.byteLength(expected), 'piped report byte count');
        assert.equal(piped.stdout, expected);
        if (json) {
          const report = JSON.parse(piped.stdout);
          assert.equal(report.ok, status === 0);
          assert.equal(report[field].length, 1200);
          assert.equal(report[field].at(-1).file, TAIL);
          assert.ok(piped.stdout.endsWith('}\n'));
        } else {
          assert.ok(piped.stdout.trimEnd().split('\n').at(-1).includes(TAIL));
        }
      });
    }
  }
});

test('clean and baseline-violation reports preserve exit 0 and exit 1', (t) => {
  const { root, profile } = fixture(t);
  const baseline = writeJson(root, 'baseline.json', profile);
  const clean = run([baseline, '--root', root, '--baseline', baseline, '--json']);
  assert.equal(clean.status, 0);
  assert.equal(clean.stderr, '');
  assert.equal(JSON.parse(clean.stdout).ratchet.ok, true);

  profile.sizeProfiles.small.hardMax = 3;
  const candidate = writeJson(root, 'candidate.json', profile);
  const violation = run([candidate, '--root', root, '--baseline', baseline, '--json']);
  assert.equal(violation.status, 1);
  assert.equal(violation.stderr, '');
  const report = JSON.parse(violation.stdout);
  assert.deepEqual(report.violations, []);
  assert.equal(report.ok, false);
  assert.equal(report.ratchet.ok, false);
});

test('usage errors stop with exit 2 and one complete diagnostic', () => {
  for (const [args, diagnostic] of [
    [[], 'usage: node scripts/nemo/boundaries.cjs <profile.json> [--baseline <prior-profile.json>] [--root <dir>] [--json]'],
    [['--baseline'], 'bad usage: --baseline requires a prior profile'],
    [['--root'], 'bad usage: --root requires a directory'],
    [['--unknown'], 'bad usage: unknown option: --unknown'],
    [['first.json', 'second.json'], 'bad usage: unexpected argument: second.json'],
  ]) assertError(run(args), `${diagnostic}\n`);
});

test('missing and malformed profile/baseline input stops before a report or further errors', (t) => {
  const { root, profile } = fixture(t);
  const valid = writeJson(root, 'valid.json', profile);
  const missing = path.join(root, 'missing.json');
  const malformed = path.join(root, 'malformed.json');
  fs.writeFileSync(malformed, '{');
  for (const input of [missing, malformed]) {
    const profileError = run([input, '--baseline', missing, '--root', root, '--json']);
    assertError(profileError, /^could not read\/parse profile ".+": .+\n$/);
    const baselineError = run([valid, '--baseline', input, '--root', root, '--json']);
    assertError(baselineError, /^could not read\/parse baseline ".+": .+\n$/);
    if (input === missing) {
      assert.match(profileError.stderr, /ENOENT/);
      assert.match(baselineError.stderr, /ENOENT/);
    }
  }
  const invalid = writeJson(root, 'invalid.json', null);
  assertError(run([invalid, '--baseline', missing, '--root', root, '--json']),
    'boundaries check could not run: invalid profile: profile must be an object\n');
  assertError(run([valid, '--baseline', invalid, '--root', root, '--json']),
    'baseline comparison could not run: invalid baseline profile: baseline is required\n');
});

test('large checker and baseline diagnostics drain stderr completely with exit 2', (t) => {
  const { root, profile } = fixture(t);
  const valid = writeJson(root, 'valid.json', profile);
  // A real invalid policy produces a large diagnostic without long argv or path limits.
  const key = 'x'.repeat(512 * 1024) + 'CLI_ERROR_TAIL_SENTINEL';
  const invalid = writeJson(root, 'invalid.json', { [key]: true });
  assertError(run([invalid, '--root', root, '--json']),
    `boundaries check could not run: invalid profile: profile: unknown field ${key}\n`);
  assertError(run([valid, '--baseline', invalid, '--root', root, '--json']),
    `baseline comparison could not run: invalid baseline profile: invalid profile: profile: unknown field ${key}\n`);
});
