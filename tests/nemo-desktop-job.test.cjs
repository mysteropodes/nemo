'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const root = path.resolve(__dirname, '..');
function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo desktop job '));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const lib = path.join(dir, 'scripts/nemo/lib');
  fs.mkdirSync(lib, { recursive: true });
  for (const file of ['jobs.cjs', 'util.cjs', 'receipt.cjs', 'identity.cjs', 'capabilities.cjs']) {
    fs.copyFileSync(path.join(root, 'scripts/nemo/lib', file), path.join(lib, file));
  }
  const app = path.join(dir, 'Candidate app.app');
  const report = path.join(dir, 'reports', 'current');
  fs.mkdirSync(app); fs.mkdirSync(report, { recursive: true });
  return { dir, app, report, tests: path.join(dir, 'tests/desktop') };
}
function execute(f, app = f.app) {
  const result = spawnSync(process.execPath, ['-e', `
    const jobs = require('./scripts/nemo/lib/jobs.cjs');
    const ctx = { reportDir: process.env.NEMO_DESKTOP_REPORT_DIR, receipt: { jobs: [] } };
    console.log(JSON.stringify(jobs.execute('test:desktop', ctx)));
  `], {
    cwd: f.dir, encoding: 'utf8', timeout: 30_000,
    env: { ...process.env, NEMO_DESKTOP_APP: app, NEMO_DESKTOP_REPORT_DIR: f.report },
  });
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}

test('desktop job executes discovered tests with the selected package and report directory', (t) => {
  const f = fixture(t);
  fs.mkdirSync(f.tests, { recursive: true });
  fs.writeFileSync(path.join(f.tests, 'package identity.test.cjs'), `
    const test = require('node:test');
    const fs = require('node:fs');
    const path = require('node:path');
    test('selected package', () => {
      if (!fs.statSync(process.env.NEMO_DESKTOP_APP).isDirectory()) throw new Error('package absent');
      fs.writeFileSync(path.join(process.env.NEMO_DESKTOP_REPORT_DIR, 'observed.json'),
        JSON.stringify({ app: process.env.NEMO_DESKTOP_APP, cwd: process.cwd() }));
    });
  `);
  fs.writeFileSync(path.join(f.tests, 'not-a-test.cjs'), 'throw new Error("must not run");');
  const result = execute(f);
  assert.equal(result.status, 'pass', result.reason);
  assert.equal(result.exitCode, 0);
  assert.deepEqual(result.details.files, ['package identity.test.cjs']);
  assert.ok(fs.existsSync(path.join(f.report, 'observed.json')), result.log);
  assert.deepEqual(JSON.parse(fs.readFileSync(path.join(f.report, 'observed.json'))), { app: f.app, cwd: fs.realpathSync(f.dir) });
});

test('desktop harness failures become failed named-job receipts', (t) => {
  const f = fixture(t);
  fs.mkdirSync(f.tests, { recursive: true });
  fs.writeFileSync(path.join(f.tests, 'negative.test.cjs'), `
    require('node:test')('native acceptance counterexample', () => { throw new Error('intentional rejection'); });
  `);
  const result = execute(f);
  assert.equal(result.status, 'fail');
  assert.notEqual(result.exitCode, 0);
  assert.match(result.log, /intentional rejection/);
});

test('a missing package or empty harness cannot pass the desktop job', (t) => {
  const f = fixture(t);
  assert.equal(execute(f).status, 'not-run');
  fs.mkdirSync(f.tests, { recursive: true });
  fs.writeFileSync(path.join(f.tests, 'README.md'), 'No executable checks here.');
  assert.equal(execute(f).status, 'not-run');
  assert.equal(execute(f, path.join(f.dir, 'absent.app')).status, 'blocked');
});

test('an unavailable native test reported as skipped still fails the desktop job', (t) => {
  const f = fixture(t);
  fs.mkdirSync(f.tests, { recursive: true });
  fs.writeFileSync(path.join(f.tests, 'unavailable.test.cjs'), "require('node:test').skip('native prerequisite unavailable', () => {});");
  const result = execute(f);
  assert.equal(result.status, 'fail');
  assert.equal(result.exitCode, 1);
  assert.equal(result.details.counts.skipped, 1);
});
