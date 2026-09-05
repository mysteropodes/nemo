'use strict';
// Read-only prerequisite and capability probes. Each probe only runs
// `--version`-style commands or reads files; none installs, writes or
// mutates workstation state. "unknown" is a legitimate answer: a capability
// that can only be established inside a browser or a packaged app says so
// instead of guessing.
const path = require('node:path');
const fs = require('node:fs');
const { ROOT, run, which, probeTool, exists } = require('./util.cjs');

function localBin(name) {
  const p = path.join(ROOT, 'node_modules', '.bin', name);
  return exists(p) ? p : null;
}

function resolvable(mod) {
  try { return require.resolve(mod, { paths: [ROOT] }); } catch { return null; }
}

function rustTargets() {
  const r = run('rustup', ['target', 'list', '--installed'], { timeout: 15000 });
  return r.status === 0 ? r.stdout.trim().split(/\r?\n/).filter(Boolean) : [];
}

function sidecarProbe(build) {
  const info = build.artifacts.ffmpegSidecar;
  if (!info || !info.present) return { present: false, path: info ? info.path : null };
  const abs = path.join(ROOT, info.path);
  const out = { present: true, path: info.path, bytes: info.bytes, sha256: info.sha256 };
  const v = run(abs, ['-version'], { timeout: 20000 });
  out.runs = v.status === 0;
  out.exitCode = v.status;
  out.error = v.error;
  if (!out.runs) out.failure = (v.stderr || v.stdout || '').trim().split(/\r?\n/).filter(Boolean).slice(0, 4).join(' | ') || `exit ${v.status}${v.signal ? ' signal ' + v.signal : ''}`;
  if (v.stdout) {
    const lines = v.stdout.split(/\r?\n/);
    out.versionLine = lines[0] || null;
    out.licenseLine = lines.find((l) => /license/i.test(l)) || null;
    const cfg = lines.find((l) => /configuration:/.test(l)) || '';
    out.enableGpl = /--enable-gpl/.test(cfg);
    out.enableVideotoolbox = /--enable-videotoolbox/.test(cfg);
  }
  if (process.platform === 'darwin' && which('otool')) {
    const o = run('otool', ['-L', abs], { timeout: 20000 });
    if (o.status === 0) {
      const libs = o.stdout.split(/\r?\n/).slice(1).map((l) => l.trim().split(' ')[0]).filter(Boolean);
      const external = libs.filter((l) => !l.startsWith('/usr/lib/') && !l.startsWith('/System/') && !l.startsWith('@'));
      out.dynamicLibs = { total: libs.length, external: external.map((l) => ({ path: l, present: exists(l) })) };
      out.externalDylibsMissing = out.dynamicLibs.external.filter((e) => !e.present).length;
    }
  }
  return out;
}

function gpuProbe() {
  if (process.platform !== 'darwin' || !which('system_profiler')) {
    return { status: 'unknown', reason: 'no read-only GPU probe on this platform yet' };
  }
  const r = run('system_profiler', ['SPDisplaysDataType', '-json'], { timeout: 30000 });
  if (r.status !== 0) return { status: 'unknown', reason: 'system_profiler failed: ' + (r.error || r.stderr.trim()) };
  try {
    const d = JSON.parse(r.stdout);
    const gpus = (d.SPDisplaysDataType || []).map((g) => ({
      model: g.sppci_model || g._name || null,
      vendor: g.spdisplays_vendor || null,
      metal: g.spdisplays_mtlgpufamilysupport || g.spdisplays_metal || null,
      cores: g.sppci_cores || null,
      vram: g.spdisplays_vram || g.spdisplays_vram_shared || null,
    }));
    return { status: gpus.length ? 'ok' : 'unknown', gpus, webgpu: 'unknown outside a browser; see test:browser' };
  } catch (e) {
    return { status: 'unknown', reason: 'could not parse system_profiler output' };
  }
}

function collect(build) {
  const tools = {
    node: { present: true, path: process.execPath, version: process.version },
    npm: probeTool('npm'),
    git: probeTool('git'),
    rustc: probeTool('rustc'),
    cargo: probeTool('cargo'),
    rustup: probeTool('rustup'),
    'wasm-pack': probeTool('wasm-pack'),
    'wasm-bindgen': probeTool('wasm-bindgen'),
    'tauri-cli': localBin('tauri') ? probeTool('tauri', ['--version'], { path: localBin('tauri') }) : { present: false, path: null, version: null, note: 'node_modules/.bin/tauri missing — run npm ci' },
    ffmpeg: probeTool('ffmpeg', ['-version']),
    ffprobe: probeTool('ffprobe', ['-version']),
    python3: probeTool('python3'),
    playwright: (function () {
      const m = resolvable('@playwright/test') || resolvable('playwright');
      if (!m) return { present: false, path: null, version: null, note: 'neither @playwright/test nor playwright resolvable from the repo root' };
      return probeTool('playwright', ['--version'], { path: localBin('playwright') || m });
    })(),
    xcodeSelect: process.platform === 'darwin' ? probeTool('xcode-select', ['-p']) : { present: false, path: null, version: null, note: 'darwin only' },
    otool: process.platform === 'darwin' ? probeTool('otool', ['--version']) : { present: false, path: null, version: null, note: 'darwin only' },
    brew: probeTool('brew'),
  };
  const targets = rustTargets();
  const caps = {
    rustTargets: targets,
    wasm32Target: targets.includes('wasm32-unknown-unknown'),
    hostTargetInstalled: build.hostTriple ? targets.includes(build.hostTriple) : false,
    nodeModulesInstalled: exists(path.join(ROOT, 'node_modules', '.package-lock.json')),
    committedWasmPresent: !!(build.artifacts.geometryWasm.present && build.artifacts.vectorizeWasm.present),
    ffmpegSidecar: sidecarProbe(build),
    gpu: gpuProbe(),
    browserSuite: { defined: exists(path.join(ROOT, 'tests', 'browser')), runnerPresent: !!(resolvable('@playwright/test') || resolvable('playwright')) },
    desktopSuite: { defined: exists(path.join(ROOT, 'tests', 'desktop')), appBundle: findBuiltApp() },
    integrationSuite: { defined: exists(path.join(ROOT, 'tests', 'integration')) },
    benchWorkloads: { defined: exists(path.join(ROOT, 'tests', 'bench')) },
  };
  return { tools, capabilities: caps };
}

function findBuiltApp() {
  if (process.env.NEMO_DESKTOP_APP && exists(process.env.NEMO_DESKTOP_APP)) return process.env.NEMO_DESKTOP_APP;
  const base = path.join(ROOT, 'src-tauri', 'target');
  if (!exists(base)) return null;
  const candidates = [];
  for (const sub of fs.readdirSync(base)) {
    for (const rel of [['release', 'bundle', 'macos', 'Nemo.app'], ['bundle', 'macos', 'Nemo.app']]) {
      const p = path.join(base, sub, ...rel);
      if (exists(p)) candidates.push(p);
    }
  }
  return candidates[0] || null;
}

module.exports = { collect, findBuiltApp, resolvable, localBin };
