'use strict';
// Shared helpers for the Nemo command surface (R02). Read-only by design:
// nothing in here writes outside the run's own report directory.
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

const ROOT = path.resolve(__dirname, '..', '..', '..');

function run(cmd, args, opts = {}) {
  const t0 = Date.now();
  const r = spawnSync(cmd, args, {
    encoding: 'utf8',
    cwd: opts.cwd || ROOT,
    env: Object.assign({}, process.env, opts.env || {}),
    timeout: opts.timeout || 0,
    maxBuffer: 256 * 1024 * 1024,
    shell: false,
    stdio: opts.inherit ? ['ignore', 'inherit', 'inherit'] : ['ignore', 'pipe', 'pipe'],
  });
  return {
    cmd: [cmd].concat(args).join(' '),
    status: r.status,
    signal: r.signal || null,
    error: r.error ? String(r.error.code || r.error.message) : null,
    stdout: r.stdout || '',
    stderr: r.stderr || '',
    durationMs: Date.now() - t0,
  };
}

function which(bin) {
  const r = run(process.platform === 'win32' ? 'where' : 'which', [bin], { timeout: 5000 });
  return r.status === 0 ? r.stdout.trim().split(/\r?\n/)[0] : null;
}

// First line of `<bin> --version` (or custom args). Never throws.
function probeTool(bin, args, opts = {}) {
  const p = opts.path || which(bin);
  if (!p) return { present: false, path: null, version: null };
  const r = run(p, args || ['--version'], { timeout: opts.timeout || 20000, cwd: opts.cwd });
  const out = (r.stdout + '\n' + r.stderr).trim();
  const line = out.split(/\r?\n/).find((l) => l.trim()) || '';
  return { present: true, path: p, version: line.trim(), exitCode: r.status, error: r.error };
}

function sha256File(file) {
  const h = crypto.createHash('sha256');
  h.update(fs.readFileSync(file));
  return h.digest('hex');
}

function sha256Text(text) {
  return crypto.createHash('sha256').update(text).digest('hex');
}

function exists(p) {
  try { fs.accessSync(p); return true; } catch { return false; }
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

function nowIso() { return new Date().toISOString(); }

function compactStamp(d) {
  return (d || new Date()).toISOString().replace(/[-:]/g, '').replace(/\.\d{3}Z$/, 'Z');
}

function fileInfo(p) {
  if (!exists(p)) return { path: path.relative(ROOT, p), present: false };
  const st = fs.statSync(p);
  return { path: path.relative(ROOT, p), present: true, bytes: st.size, sha256: st.isFile() ? sha256File(p) : null };
}

module.exports = { ROOT, run, which, probeTool, sha256File, sha256Text, exists, readJson, nowIso, compactStamp, fileInfo };
