'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { JOBS, execute } = require('../scripts/nemo/lib/jobs.cjs');

function executable(dir, name, source) {
  const file = path.join(dir, name);
  fs.writeFileSync(file, `#!/usr/bin/env node\n${source}\n`, { mode: 0o755 });
  return file;
}

function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-native-ffmpeg-preflight-'));
  const bin = path.join(dir, 'bin');
  fs.mkdirSync(bin);
  const marker = path.join(dir, 'cargo-called.json');
  const events = path.join(dir, 'events.jsonl');
  const target = `fixture-${process.pid}-${path.basename(dir).slice(-6)}`;
  const builderTargets = new Set([target]);
  executable(bin, 'cargo', `
    const fs = require('node:fs');
    const args = process.argv.slice(2);
    const event = data => fs.appendFileSync(process.env.NEMO_FAKE_EVENTS, JSON.stringify(data) + '\\n');
    if (args[0] === 'build') {
      const target = args[args.indexOf('--target') + 1];
      event({ type: 'builder', target });
      if (process.env.NEMO_FAKE_MCP_BUILD_MODE === 'fail') process.exit(17);
      if (process.env.NEMO_FAKE_MCP_BUILD_MODE !== 'missing') {
        const built = require('node:path').join('nemo-mcp', 'target', target, 'release', 'nemo-mcp');
        fs.mkdirSync(require('node:path').dirname(built), { recursive: true });
        fs.writeFileSync(built, 'built sidecar', { mode: 0o755 });
      }
      process.exit(0);
    }
    event({ type: 'cargo', args, override: process.env.NEMO_TEST_FFMPEG_PATH });
    fs.writeFileSync(process.env.NEMO_FAKE_CARGO_MARKER,
      JSON.stringify({ args, override: process.env.NEMO_TEST_FFMPEG_PATH }));
    console.log('test result: ' + (process.env.NEMO_FAKE_CARGO_TEST_MODE === 'fail' ? 'FAILED. 2 passed; 1 failed' : 'ok. 3 passed; 0 failed'));
    if (process.env.NEMO_FAKE_CARGO_TEST_MODE === 'fail') process.exit(23);
  `);
  const previous = {
    PATH: process.env.PATH,
    override: process.env.NEMO_TEST_FFMPEG_PATH,
    marker: process.env.NEMO_FAKE_CARGO_MARKER,
    events: process.env.NEMO_FAKE_EVENTS,
    target: process.env.NEMO_MCP_TARGET,
    tauriTarget: process.env.TAURI_ENV_TARGET_TRIPLE,
    buildMode: process.env.NEMO_FAKE_MCP_BUILD_MODE,
    cargoMode: process.env.NEMO_FAKE_CARGO_TEST_MODE,
  };
  process.env.PATH = bin + path.delimiter + previous.PATH;
  process.env.NEMO_FAKE_CARGO_MARKER = marker;
  process.env.NEMO_FAKE_EVENTS = events;
  process.env.NEMO_MCP_TARGET = target;
  t.after(() => {
    for (const builderTarget of builderTargets) {
      fs.rmSync(path.resolve('nemo-mcp', 'target', builderTarget), { recursive: true, force: true });
    }
    for (const [name, value] of Object.entries({
      NEMO_TEST_FFMPEG_PATH: previous.override,
      NEMO_FAKE_CARGO_MARKER: previous.marker,
      NEMO_FAKE_EVENTS: previous.events,
      NEMO_MCP_TARGET: previous.target,
      TAURI_ENV_TARGET_TRIPLE: previous.tauriTarget,
      NEMO_FAKE_MCP_BUILD_MODE: previous.buildMode,
      NEMO_FAKE_CARGO_TEST_MODE: previous.cargoMode,
    })) {
      if (value === undefined) delete process.env[name];
      else process.env[name] = value;
    }
    process.env.PATH = previous.PATH;
    fs.rmSync(dir, { recursive: true, force: true });
  });
  return { dir, bin, marker, events, target, builderTargets, sidecar: path.resolve('src-tauri/binaries', `nemo-mcp-${target}`) };
}

function runNativeJob() {
  const receipt = { jobs: [] };
  const originalRequired = JOBS['test:rust-tauri'].required;
  JOBS['test:rust-tauri'].required = true;
  try {
    return execute('test:rust-tauri', { receipt });
  } finally {
    JOBS['test:rust-tauri'].required = originalRequired;
  }
}

test('explicit hosted FFmpeg is preflighted and inherited by Cargo', t => {
  const f = fixture(t);
  const ffmpeg = executable(f.bin, 'hosted ffmpeg', `
    const fs = require('node:fs');
    fs.appendFileSync(process.env.NEMO_FAKE_EVENTS, JSON.stringify({ type: 'ffmpeg', args: process.argv.slice(2) }) + '\\n');
    console.log('ffmpeg version fixture');
  `);
  process.env.NEMO_TEST_FFMPEG_PATH = ffmpeg;

  const result = runNativeJob();

  assert.equal(result.status, 'pass', result.reason);
  const events = fs.readFileSync(f.events, 'utf8').trim().split('\n').map(JSON.parse);
  assert.deepEqual(events.map((event) => event.type), ['ffmpeg', 'builder', 'cargo']);
  assert.deepEqual(events[0].args, ['-hide_banner', '-version']);
  assert.equal(events[1].target, f.target);
  const cargo = JSON.parse(fs.readFileSync(f.marker));
  assert.equal(cargo.override, ffmpeg);
  assert.deepEqual(cargo.args, [
    // --no-fail-fast (#1318): cargo otherwise stops at the first failing test
    // binary, and the job's `test result:` summing then under-reports the crate.
    'test', '--no-fail-fast', '--release', '--manifest-path', path.resolve('src-tauri/Cargo.toml'),
    '--', '--test-threads=1',
  ]);
  assert.equal(fs.existsSync(f.sidecar), false, 'the runner removes its staged sidecar after Cargo succeeds');
});

test('bad explicit FFmpeg blocks before Cargo without PATH or bundle fallback', t => {
  const f = fixture(t);
  const pathDecoy = executable(f.bin, 'ffmpeg', `
    require('node:fs').writeFileSync(${JSON.stringify(path.join(f.dir, 'path-ffmpeg-called'))}, 'unexpected');
    console.log('ffmpeg version decoy');
  `);
  process.env.NEMO_TEST_FFMPEG_PATH = pathDecoy + '-missing';

  const result = runNativeJob();

  assert.equal(result.status, 'blocked');
  assert.match(result.reason, /NEMO_TEST_FFMPEG_PATH.*ENOENT/);
  assert.equal(fs.existsSync(f.marker), false, 'Cargo must not run after failed preflight');
  assert.equal(fs.existsSync(path.join(f.dir, 'path-ffmpeg-called')), false, 'PATH ffmpeg must not run');
  assert.equal(fs.existsSync(f.events), false, 'the builder must not run after failed FFmpeg preflight');
});

test('an executable with the wrong banner blocks before Cargo', t => {
  const f = fixture(t);
  const wrong = executable(f.bin, 'wrong-program', `console.log('not ffmpeg');`);
  process.env.NEMO_TEST_FFMPEG_PATH = wrong;

  const result = runNativeJob();

  assert.equal(result.status, 'blocked');
  assert.match(result.reason, /missing FFmpeg version banner/);
  assert.equal(fs.existsSync(f.marker), false);
  assert.equal(fs.existsSync(f.events), false, 'the builder must not run after failed FFmpeg preflight');
});

test('a job-created MCP sidecar is removed after Cargo failure', t => {
  const f = fixture(t);
  const ffmpeg = executable(f.bin, 'ffmpeg', `console.log('ffmpeg version fixture');`);
  process.env.NEMO_TEST_FFMPEG_PATH = ffmpeg;
  process.env.NEMO_FAKE_CARGO_TEST_MODE = 'fail';

  const result = runNativeJob();

  assert.equal(result.status, 'fail');
  assert.equal(result.exitCode, 23);
  assert.deepEqual(fs.readFileSync(f.events, 'utf8').trim().split('\n').map(JSON.parse).map((event) => event.type), ['builder', 'cargo']);
  assert.equal(fs.existsSync(f.sidecar), false, 'the runner removes its staged sidecar after Cargo fails');
});

test('a pre-existing MCP sidecar is not rebuilt or modified on Cargo pass or failure', t => {
  const f = fixture(t);
  const ffmpeg = executable(f.bin, 'ffmpeg', `console.log('ffmpeg version fixture');`);
  process.env.NEMO_TEST_FFMPEG_PATH = ffmpeg;
  fs.mkdirSync(path.dirname(f.sidecar), { recursive: true });
  fs.writeFileSync(f.sidecar, 'sentinel bytes', { mode: 0o701 });
  const before = { bytes: fs.readFileSync(f.sidecar), mode: fs.statSync(f.sidecar).mode & 0o777 };
  t.after(() => fs.rmSync(f.sidecar, { force: true }));

  let result = runNativeJob();
  assert.equal(result.status, 'pass', result.reason);
  assert.deepEqual(fs.readFileSync(f.sidecar), before.bytes);
  assert.equal(fs.statSync(f.sidecar).mode & 0o777, before.mode);
  assert.deepEqual(fs.readFileSync(f.events, 'utf8').trim().split('\n').map(JSON.parse).map((event) => event.type), ['cargo']);

  process.env.NEMO_FAKE_CARGO_TEST_MODE = 'fail';
  result = runNativeJob();
  assert.equal(result.status, 'fail');
  assert.deepEqual(fs.readFileSync(f.sidecar), before.bytes);
  assert.equal(fs.statSync(f.sidecar).mode & 0o777, before.mode);
  assert.deepEqual(fs.readFileSync(f.events, 'utf8').trim().split('\n').map(JSON.parse).map((event) => event.type), ['cargo', 'cargo']);
});

test('builder failure fails before Cargo and leaves no sidecar', t => {
  const f = fixture(t);
  const ffmpeg = executable(f.bin, 'ffmpeg', `console.log('ffmpeg version fixture');`);
  process.env.NEMO_TEST_FFMPEG_PATH = ffmpeg;
  process.env.NEMO_FAKE_MCP_BUILD_MODE = 'fail';

  const result = runNativeJob();

  assert.equal(result.status, 'fail');
  assert.match(result.reason, /builder failed \(1\)/);
  assert.match(result.log, /cargo build failed with status 17/);
  assert.equal(fs.existsSync(f.marker), false, 'Cargo must not run after builder failure');
  assert.equal(fs.existsSync(f.sidecar), false, 'no job-created sidecar may remain after builder failure');
});

test('a missing staged artifact fails before Cargo and leaves no sidecar', t => {
  const f = fixture(t);
  const ffmpeg = executable(f.bin, 'ffmpeg', `console.log('ffmpeg version fixture');`);
  process.env.NEMO_TEST_FFMPEG_PATH = ffmpeg;
  const originalLstat = fs.lstatSync;
  let targetLookups = 0;
  fs.lstatSync = function removeStagedArtifact(file, ...args) {
    if (file === f.sidecar && ++targetLookups === 2) fs.unlinkSync(f.sidecar);
    return originalLstat.call(this, file, ...args);
  };
  t.after(() => { fs.lstatSync = originalLstat; });

  const result = runNativeJob();

  assert.equal(result.status, 'fail');
  assert.match(result.reason, /no regular expected artifact/);
  assert.equal(fs.existsSync(f.marker), false, 'Cargo must not run after a missing staged artifact');
  assert.equal(fs.existsSync(f.sidecar), false, 'no job-created sidecar may remain after a missing artifact');
});

test('an exception after staging still removes the job-created MCP sidecar', t => {
  const f = fixture(t);
  const ffmpeg = executable(f.bin, 'ffmpeg', `console.log('ffmpeg version fixture');`);
  process.env.NEMO_TEST_FFMPEG_PATH = ffmpeg;
  const originalLstat = fs.lstatSync;
  let targetLookups = 0;
  fs.lstatSync = function patchedLstat(file, ...args) {
    if (file === f.sidecar && ++targetLookups === 2) throw new Error('injected post-stage exception');
    return originalLstat.call(this, file, ...args);
  };
  t.after(() => { fs.lstatSync = originalLstat; });

  const result = runNativeJob();

  assert.equal(result.status, 'fail');
  assert.match(result.reason, /injected post-stage exception/);
  assert.equal(fs.existsSync(f.marker), false, 'Cargo must not run after the injected exception');
  assert.equal(fs.existsSync(f.sidecar), false, 'finally removes a sidecar staged before an exception');
});

test('a post-stage exception retains its cleanup failure diagnostic', t => {
  const f = fixture(t);
  const ffmpeg = executable(f.bin, 'ffmpeg', `console.log('ffmpeg version fixture');`);
  process.env.NEMO_TEST_FFMPEG_PATH = ffmpeg;
  const originalLstat = fs.lstatSync;
  const originalUnlink = fs.unlinkSync;
  let targetLookups = 0;
  fs.lstatSync = function primaryFailure(file, ...args) {
    if (file === f.sidecar && ++targetLookups === 2) throw new Error('injected post-stage primary exception');
    return originalLstat.call(this, file, ...args);
  };
  fs.unlinkSync = function cleanupFailure(file, ...args) {
    if (file === f.sidecar) throw new Error('injected cleanup failure');
    return originalUnlink.call(this, file, ...args);
  };

  let result;
  try {
    result = runNativeJob();
  } finally {
    fs.lstatSync = originalLstat;
    fs.unlinkSync = originalUnlink;
  }

  assert.equal(result.status, 'fail');
  assert.match(result.reason, /injected post-stage primary exception/);
  assert.match(result.reason, /MCP sidecar cleanup failed: injected cleanup failure/);
  assert.equal(fs.existsSync(f.sidecar), true, 'the forced cleanup failure leaves the staged sidecar for this test to remove');
  fs.rmSync(f.sidecar, { force: true });
});

test('TAURI_ENV_TARGET_TRIPLE takes precedence over NEMO_MCP_TARGET', t => {
  const f = fixture(t);
  const ffmpeg = executable(f.bin, 'ffmpeg', `console.log('ffmpeg version fixture');`);
  const tauriTarget = `tauri-${f.target}`;
  process.env.NEMO_TEST_FFMPEG_PATH = ffmpeg;
  process.env.TAURI_ENV_TARGET_TRIPLE = tauriTarget;
  f.builderTargets.add(tauriTarget);
  const selected = path.resolve('src-tauri/binaries', `nemo-mcp-${tauriTarget}`);

  const result = runNativeJob();

  assert.equal(result.status, 'pass', result.reason);
  const events = fs.readFileSync(f.events, 'utf8').trim().split('\n').map(JSON.parse);
  assert.equal(events[0].target, tauriTarget);
  assert.equal(fs.existsSync(selected), false);
  assert.equal(fs.existsSync(f.sidecar), false);
});
