"use strict";
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const repo = path.join(__dirname, '..');
function fixture(t) {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-mcp-bundle-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const dir of ['scripts', 'nemo-mcp', 'src-tauri/binaries', 'bin']) fs.mkdirSync(path.join(root, dir), { recursive: true });
  fs.copyFileSync(path.join(repo, 'scripts/build-mcp-sidecar.cjs'), path.join(root, 'scripts/build-mcp-sidecar.cjs'));
  fs.writeFileSync(path.join(root, 'nemo-mcp/Cargo.toml'), '[package]\nname="fixture"\n');
  fs.writeFileSync(path.join(root, 'bin/cargo'), `#!/usr/bin/env node
const fs=require('node:fs'), path=require('node:path');
const args=process.argv.slice(2);
fs.writeFileSync('cargo-args.json',JSON.stringify(args));
if(process.env.FIXTURE_FAIL)process.exit(4);
const target=args[args.indexOf('--target')+1];
const dir=path.join(process.env.CARGO_TARGET_DIR,target,'release');
fs.mkdirSync(dir,{recursive:true});
fs.writeFileSync(path.join(dir,target.includes('windows')?'nemo-mcp.exe':'nemo-mcp'),'new executable');
`, { mode: 0o755 });
  return { root, run(target, extra = {}) {
    return spawnSync(process.execPath, [path.join(root, 'scripts/build-mcp-sidecar.cjs')], {
      cwd: root, encoding: 'utf8', env: { ...process.env, PATH: path.join(root, 'bin') + path.delimiter + process.env.PATH,
        TAURI_ENV_TARGET_TRIPLE: target, CARGO_TARGET_DIR: path.join(root, 'wrong-output'), ...extra },
    });
  } };
}
test('normal Tauri build keeps FFmpeg and guard while staging MCP', () => {
  const config = JSON.parse(fs.readFileSync(path.join(repo, 'src-tauri/tauri.conf.json')));
  assert.equal(config.build.beforeBuildCommand, 'python3 scripts/assert-no-private-labs.py && node scripts/build-mcp-sidecar.cjs');
  assert.ok(config.bundle.externalBin.includes('binaries/ffmpeg'));
  assert.ok(config.bundle.externalBin.includes('binaries/nemo-mcp'));
});
test('builder runs locked Cargo and stages selected target despite ambient output override', { skip: process.platform === 'win32' }, (t) => {
  const f = fixture(t);
  for (const target of ['aarch64-apple-darwin', 'x86_64-pc-windows-msvc']) {
    const result = f.run(target);
    assert.equal(result.status, 0, result.stderr);
    const filename = `nemo-mcp-${target}${target.includes('windows') ? '.exe' : ''}`;
    assert.equal(fs.readFileSync(path.join(f.root, 'src-tauri/binaries', filename), 'utf8'), 'new executable');
    assert.deepEqual(JSON.parse(fs.readFileSync(path.join(f.root, 'cargo-args.json'))), [
      'build', '--locked', '--release', '--manifest-path', path.join(f.root, 'nemo-mcp/Cargo.toml'), '--bin', 'nemo-mcp', '--target', target,
    ]);
  }
});
test('failed build or missing crate removes a previously staged executable', { skip: process.platform === 'win32' }, (t) => {
  const f = fixture(t), target = 'aarch64-apple-darwin';
  const staged = path.join(f.root, 'src-tauri/binaries', `nemo-mcp-${target}`);
  fs.writeFileSync(staged, 'stale');
  assert.notEqual(f.run(target, { FIXTURE_FAIL: '1' }).status, 0);
  assert.equal(fs.existsSync(staged), false);
  fs.writeFileSync(staged, 'stale');
  fs.rmSync(path.join(f.root, 'nemo-mcp/Cargo.toml'));
  assert.notEqual(f.run(target).status, 0);
  assert.equal(fs.existsSync(staged), false);
});
