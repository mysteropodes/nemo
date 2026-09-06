'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { JOBS, execute } = require('../scripts/nemo/lib/jobs.cjs');
const { finalize } = require('../scripts/nemo/lib/receipt.cjs');

function fixture(t, stdout, exitCode) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-rust-test-count-'));
  const marker = path.join(dir, 'cargo-args.json');
  const previous = { PATH: process.env.PATH, NEMO_TEST_FFMPEG_PATH: process.env.NEMO_TEST_FFMPEG_PATH };
  t.after(() => {
    for (const [key, value] of Object.entries(previous)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
    fs.rmSync(dir, { recursive: true, force: true });
  });
  fs.writeFileSync(path.join(dir, 'cargo'), `#!${process.execPath}
    require('node:fs').writeFileSync(${JSON.stringify(marker)}, JSON.stringify(process.argv.slice(2)));
    process.stdout.write(${JSON.stringify(stdout)});
    process.exitCode = ${exitCode};
  `, { mode: 0o755 });
  const ffmpeg = path.join(dir, 'ffmpeg');
  fs.writeFileSync(ffmpeg, `#!${process.execPath}\nconsole.log('ffmpeg version fixture');\n`, { mode: 0o755 });
  process.env.PATH = dir + path.delimiter + previous.PATH;
  process.env.NEMO_TEST_FFMPEG_PATH = ffmpeg;
  return marker;
}

const zero = 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n';
const real = 'test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n';
const cases = [
  { name: 'zero tests', stdout: zero, status: 'fail', exitCode: 1 },
  { name: 'missing result summaries', stdout: 'Finished test profile\n', status: 'fail', exitCode: 1 },
  { name: 'multiple empty binaries', stdout: zero + zero, status: 'fail', exitCode: 1 },
  { name: 'ignored and filtered tests only', stdout: zero.replace('0 ignored', '2 ignored').replace('0 filtered', '5 filtered'), status: 'fail', exitCode: 1 },
  { name: 'real tests plus zero doctests', stdout: real + zero, status: 'pass', exitCode: 0 },
  { name: 'empty binary before real tests', stdout: zero + real + real, status: 'pass', exitCode: 0 },
  { name: 'reported failure despite zero Cargo exit', stdout: 'test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n', status: 'fail', exitCode: 1, reason: /1 failed/ },
  { name: 'Cargo failure after passing tests', stdout: real, cargoExit: 101, status: 'fail', exitCode: 101 },
  { name: 'Cargo failure without summaries', stdout: '', cargoExit: 101, status: 'fail', exitCode: 101 },
];

for (const name of ['test:rust', 'test:rust-tauri']) {
  for (const scenario of cases) {
    test(`${name}: ${scenario.name}`, t => {
      const marker = fixture(t, scenario.stdout, scenario.cargoExit || 0);
      const required = JOBS[name].required;
      JOBS[name].required = true;
      t.after(() => { JOBS[name].required = required; });
      const receipt = { jobs: [], startedAt: new Date().toISOString() };

      const result = execute(name, { receipt });
      finalize(receipt);

      assert.equal(result.required, true);
      assert.equal(result.status, scenario.status, result.reason);
      assert.equal(result.exitCode, scenario.exitCode);
      assert.equal(receipt.summary.overall, scenario.status);
      assert.equal(receipt.summary.exitCode, scenario.status === 'pass' ? 0 : 1);
      assert.ok(result.log.includes(scenario.stdout), 'original Cargo output is retained');
      if (scenario.status === 'fail' && !scenario.cargoExit) assert.match(result.reason, scenario.reason || /no executed tests/i);
      const native = name === 'test:rust-tauri';
      assert.deepEqual(JSON.parse(fs.readFileSync(marker)), [
        'test', ...(native ? ['--release'] : []),
        '--manifest-path', path.resolve(__dirname, '..', native ? 'src-tauri' : 'geometry-wasm', 'Cargo.toml'),
        ...(native ? ['--', '--test-threads=1'] : []),
      ]);
    });
  }
}
