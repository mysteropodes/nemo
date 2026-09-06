'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { JOBS, execute } = require('../scripts/nemo/lib/jobs.cjs');

function executable(dir, name, source) {
  const file = path.join(dir, name);
  fs.writeFileSync(file, `#!${process.execPath}\n${source}\n`, { mode: 0o755 });
  return file;
}

function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-native-ffmpeg-preflight-'));
  const bin = path.join(dir, 'bin');
  fs.mkdirSync(bin);
  const marker = path.join(dir, 'cargo-called.json');
  executable(bin, 'cargo', `
    const fs = require('node:fs');
    fs.writeFileSync(process.env.NEMO_FAKE_CARGO_MARKER,
      JSON.stringify({ args: process.argv.slice(2), override: process.env.NEMO_TEST_FFMPEG_PATH }));
    console.log('test result: ok. 3 passed; 0 failed');
  `);
  const previous = {
    PATH: process.env.PATH,
    override: process.env.NEMO_TEST_FFMPEG_PATH,
    marker: process.env.NEMO_FAKE_CARGO_MARKER,
  };
  process.env.PATH = bin + path.delimiter + previous.PATH;
  process.env.NEMO_FAKE_CARGO_MARKER = marker;
  t.after(() => {
    if (previous.override === undefined) delete process.env.NEMO_TEST_FFMPEG_PATH;
    else process.env.NEMO_TEST_FFMPEG_PATH = previous.override;
    if (previous.marker === undefined) delete process.env.NEMO_FAKE_CARGO_MARKER;
    else process.env.NEMO_FAKE_CARGO_MARKER = previous.marker;
    process.env.PATH = previous.PATH;
    fs.rmSync(dir, { recursive: true, force: true });
  });
  return { dir, bin, marker };
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
    fs.writeFileSync(${JSON.stringify(path.join(f.dir, 'ffmpeg-called.json'))}, JSON.stringify(process.argv.slice(2)));
    console.log('ffmpeg version fixture');
  `);
  process.env.NEMO_TEST_FFMPEG_PATH = ffmpeg;

  const result = runNativeJob();

  assert.equal(result.status, 'pass', result.reason);
  assert.deepEqual(JSON.parse(fs.readFileSync(path.join(f.dir, 'ffmpeg-called.json'))), ['-hide_banner', '-version']);
  const cargo = JSON.parse(fs.readFileSync(f.marker));
  assert.equal(cargo.override, ffmpeg);
  assert.deepEqual(cargo.args, [
    'test', '--manifest-path', path.resolve('src-tauri/Cargo.toml'),
    '--', '--test-threads=1',
  ]);
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
});

test('an executable with the wrong banner blocks before Cargo', t => {
  const f = fixture(t);
  const wrong = executable(f.bin, 'wrong-program', `console.log('not ffmpeg');`);
  process.env.NEMO_TEST_FFMPEG_PATH = wrong;

  const result = runNativeJob();

  assert.equal(result.status, 'blocked');
  assert.match(result.reason, /missing FFmpeg version banner/);
  assert.equal(fs.existsSync(f.marker), false);
});
