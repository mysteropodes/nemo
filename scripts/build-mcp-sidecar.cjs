#!/usr/bin/env node
'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const root = path.resolve(__dirname, '..');
const crate = path.join(root, 'nemo-mcp');
const manifest = path.join(crate, 'Cargo.toml');
let target = process.env.TAURI_ENV_TARGET_TRIPLE || process.env.NEMO_MCP_TARGET;
if (!target) {
  const host = spawnSync('rustc', ['-vV'], { encoding: 'utf8' });
  if (host.error || host.status !== 0) throw host.error || new Error('Cannot determine Rust host target');
  target = /^host: (\S+)$/m.exec(host.stdout)?.[1];
}
if (!target) throw new Error('Rust host target is unavailable');
if (!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(target)) throw new Error(`Invalid MCP sidecar target: ${target}`);

const binary = target.includes('windows') ? 'nemo-mcp.exe' : 'nemo-mcp';
const stagedName = `nemo-mcp-${target}${binary.endsWith('.exe') ? '.exe' : ''}`;
const outDir = path.join(root, 'src-tauri', 'binaries');
const staged = path.join(outDir, stagedName);
fs.mkdirSync(outDir, { recursive: true });
// Remove the selected target's old artifact before building, so a failed build
// cannot leave Tauri believing that a stale executable is valid.
try { fs.rmSync(staged, { force: true }); } catch (error) { throw new Error(`Cannot remove stale sidecar ${staged}: ${error.message}`); }

if (!fs.existsSync(manifest)) throw new Error(`MCP crate is missing: ${manifest}`);
const result = spawnSync('cargo', ['build', '--locked', '--release', '--manifest-path', manifest, '--bin', 'nemo-mcp', '--target', target], {
  cwd: root, stdio: 'inherit', env: { ...process.env, CARGO_TARGET_DIR: path.join(crate, 'target') },
});
if (result.error || result.status !== 0) throw result.error || new Error(`cargo build failed with status ${result.status}`);
const built = path.join(crate, 'target', target, 'release', binary);
if (!fs.existsSync(built)) throw new Error(`Cargo succeeded but produced no executable: ${built}`);
fs.copyFileSync(built, staged);
if (process.platform !== 'win32' && !binary.endsWith('.exe')) fs.chmodSync(staged, 0o755);
console.log(`Staged ${staged}`);
