'use strict';
// Source, build and platform identity. Every receipt carries all three so a
// result can never be quoted without the bytes it was measured on.
const os = require('node:os');
const path = require('node:path');
const fs = require('node:fs');
const { ROOT, run, readJson, exists, fileInfo, sha256Text } = require('./util.cjs');

function git(args) {
  const r = run('git', args, { timeout: 30000 });
  return r.status === 0 ? r.stdout.trim() : null;
}

function sourceIdentity() {
  const head = git(['rev-parse', 'HEAD']);
  const branch = git(['rev-parse', '--abbrev-ref', 'HEAD']);
  const describe = git(['describe', '--tags', '--always', '--dirty']);
  const porcelain = git(['status', '--porcelain', '--untracked-files=all']) || '';
  const originUrl = git(['config', '--get', 'remote.origin.url']);
  const upstream = git(['rev-parse', '--abbrev-ref', '--symbolic-full-name', '@{upstream}']);
  const worktree = git(['rev-parse', '--show-toplevel']);
  const commitDate = git(['log', '-1', '--format=%cI']);
  const subject = git(['log', '-1', '--format=%s']);
  const entries = porcelain ? porcelain.split(/\r?\n/).filter(Boolean) : [];
  const tracked = entries.filter((l) => !l.startsWith('??'));
  const untracked = entries.filter((l) => l.startsWith('??'));
  let dirtyDigest = null;
  if (entries.length) {
    const diff = run('git', ['diff', 'HEAD', '--binary'], { timeout: 60000 }).stdout;
    dirtyDigest = sha256Text(diff + '\n' + entries.join('\n'));
  }
  return {
    head, branch, describe, commitDate, subject, originUrl, upstream, worktree,
    dirty: entries.length > 0,
    modifiedTracked: tracked.length,
    untracked: untracked.length,
    dirtyDigest,
    changedPaths: entries.slice(0, 200),
  };
}

function readVersionFromCargo(file) {
  if (!exists(file)) return null;
  const m = fs.readFileSync(file, 'utf8').match(/^\s*version\s*=\s*"([^"]+)"/m);
  return m ? m[1] : null;
}

function hostTriple() {
  const r = run('rustc', ['-vV'], { timeout: 15000 });
  if (r.status !== 0) return null;
  const m = r.stdout.match(/^host:\s*(\S+)/m);
  return m ? m[1] : null;
}

function buildIdentity() {
  const pkg = readJson(path.join(ROOT, 'package.json'));
  const tauriConf = readJson(path.join(ROOT, 'src-tauri', 'tauri.conf.json'));
  const indexHtml = fs.readFileSync(path.join(ROOT, 'src', 'index.html'), 'utf8');
  const titleM = indexHtml.match(/<title>\s*Nemo v([^<\s]+)\s*<\/title>/);
  const statusM = indexHtml.match(/id="status-text">\s*Nemo v([^<\s]+)\s*</);
  const triple = hostTriple();
  const sidecarPath = triple ? path.join(ROOT, 'src-tauri', 'binaries', 'ffmpeg-' + triple) : null;
  return {
    packageVersion: pkg.version,
    tauriVersion: tauriConf.version,
    indexHtmlTitleVersion: titleM ? titleM[1] : null,
    indexHtmlStatusVersion: statusM ? statusM[1] : null,
    productName: tauriConf.productName || null,
    identifier: tauriConf.identifier || null,
    tauriCliRange: (pkg.devDependencies || {})['@tauri-apps/cli'] || null,
    crates: {
      'geometry-wasm': readVersionFromCargo(path.join(ROOT, 'geometry-wasm', 'Cargo.toml')),
      'vectorize-wasm': readVersionFromCargo(path.join(ROOT, 'vectorize-wasm', 'Cargo.toml')),
      'src-tauri': readVersionFromCargo(path.join(ROOT, 'src-tauri', 'Cargo.toml')),
    },
    hostTriple: triple,
    artifacts: {
      geometryWasm: fileInfo(path.join(ROOT, 'src', 'wasm', 'geometry_wasm_bg.wasm')),
      vectorizeWasm: fileInfo(path.join(ROOT, 'src', 'wasm-vectorize', 'vectorize_wasm_bg.wasm')),
      ffmpegSidecar: sidecarPath ? fileInfo(sidecarPath) : { present: false, path: null, note: 'host triple unknown (rustc missing)' },
    },
  };
}

function platformIdentity() {
  const cpus = os.cpus();
  return {
    os: process.platform,
    osRelease: os.release(),
    osVersion: typeof os.version === 'function' ? os.version() : null,
    arch: process.arch,
    hostname: null, // deliberately not recorded: receipts are shareable
    cpuModel: cpus.length ? cpus[0].model : null,
    cpuCount: cpus.length,
    memoryBytes: os.totalmem(),
    node: process.version,
    user: null, // deliberately not recorded
  };
}

module.exports = { sourceIdentity, buildIdentity, platformIdentity, hostTriple };
