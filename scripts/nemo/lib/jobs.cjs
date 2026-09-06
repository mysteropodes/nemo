'use strict';
// Named jobs. Each returns { status, reason, exitCode?, artifacts?, limitations?, details?, log? }.
// A job that needs a tool, target, suite or artifact that is absent returns
// `blocked` with the exact missing thing. A job with nothing defined to run
// yet returns `not-run` and names the work package that will define it.
const fs = require('node:fs');
const path = require('node:path');
const { ROOT, run, which, exists, readJson, fileInfo } = require('./util.cjs');
const { STATUS } = require('./receipt.cjs');
const caps = require('./capabilities.cjs');

function pass(reason, extra) { return Object.assign({ status: STATUS.PASS, reason }, extra); }
function fail(reason, extra) { return Object.assign({ status: STATUS.FAIL, reason }, extra); }
function blocked(reason, extra) { return Object.assign({ status: STATUS.BLOCKED, reason }, extra); }
function notRun(reason, extra) { return Object.assign({ status: STATUS.NOT_RUN, reason }, extra); }

function logOf(r) { return `$ ${r.cmd}\n(exit ${r.status}${r.signal ? ' signal ' + r.signal : ''}${r.error ? ' error ' + r.error : ''}, ${r.durationMs} ms)\n--- stdout ---\n${r.stdout}\n--- stderr ---\n${r.stderr}\n`; }

// ---- doctor ------------------------------------------------------------
function jobDoctor(ctx) {
  const c = caps.collect(ctx.receipt.build);
  ctx.receipt.tools = c.tools;
  ctx.receipt.capabilities = c.capabilities;
  const missing = Object.entries(c.tools).filter(([, t]) => !t.present).map(([k]) => k);
  return pass(`probed ${Object.keys(c.tools).length} tools, ${missing.length} absent (${missing.join(', ') || 'none'})`, {
    details: { absentTools: missing, gpu: c.capabilities.gpu.status, sidecar: c.capabilities.ffmpegSidecar.present ? (c.capabilities.ffmpegSidecar.runs ? 'runs' : 'present but does not run') : 'absent' },
    limitations: ['WebGPU availability is only observable inside a browser or the packaged app (test:browser / test:desktop)'],
  });
}

// ---- check -------------------------------------------------------------
function checkVersionSync(b) {
  const problems = [];
  if (b.packageVersion !== b.tauriVersion) problems.push(`package.json ${b.packageVersion} != src-tauri/tauri.conf.json ${b.tauriVersion}`);
  if (b.indexHtmlTitleVersion !== b.packageVersion) problems.push(`src/index.html <title> fallback ${b.indexHtmlTitleVersion} != ${b.packageVersion} (CLAUDE.md §7 step 2)`);
  if (b.indexHtmlStatusVersion !== b.packageVersion) problems.push(`src/index.html #status-text fallback ${b.indexHtmlStatusVersion} != ${b.packageVersion} (CLAUDE.md §7 step 2)`);
  return problems.length ? fail(problems.join('; ')) : pass(`all four version strings say ${b.packageVersion}`);
}

function checkJsonValid() {
  const files = ['package.json', 'package-lock.json', 'src-tauri/tauri.conf.json', 'src/wasm/package.json', 'src/wasm-vectorize/package.json']
    .concat(exists(path.join(ROOT, 'src-tauri', 'capabilities')) ? fs.readdirSync(path.join(ROOT, 'src-tauri', 'capabilities')).filter((f) => f.endsWith('.json')).map((f) => 'src-tauri/capabilities/' + f) : []);
  const bad = [];
  for (const f of files) {
    const p = path.join(ROOT, f);
    if (!exists(p)) { bad.push(`${f}: missing`); continue; }
    try { readJson(p); } catch (e) { bad.push(`${f}: ${e.message}`); }
  }
  return bad.length ? fail(bad.join('; ')) : pass(`${files.length} JSON files parse`);
}

function checkJsSyntax(ctx) {
  const dir = path.join(ROOT, 'src', 'js');
  const files = [];
  (function walk(d) { for (const e of fs.readdirSync(d, { withFileTypes: true })) { const p = path.join(d, e.name); if (e.isDirectory()) walk(p); else if (e.name.endsWith('.js')) files.push(p); } })(dir);
  // ES modules (module scripts and module workers) fail `node --check` as
  // CommonJS on `import.meta` / top-level import. They are checked from a
  // .mjs copy inside the run's report directory, never in place.
  const isModule = (src) => /^\s*(import|export)\s/m.test(src) || /import\.meta/.test(src);
  const scratch = path.join(ctx.reportDir, 'check-modules');
  const bad = [];
  let modules = 0;
  for (const f of files) {
    const src = fs.readFileSync(f, 'utf8');
    let target = f;
    if (isModule(src)) {
      modules++;
      fs.mkdirSync(scratch, { recursive: true });
      target = path.join(scratch, path.basename(f, '.js') + '.mjs');
      fs.writeFileSync(target, src);
    }
    const r = run(process.execPath, ['--check', target], { timeout: 60000 });
    if (r.status !== 0) bad.push(`${path.relative(ROOT, f)}: ${(r.stderr.split(/\r?\n/).find((l) => /SyntaxError/.test(l)) || r.stderr.trim().split(/\r?\n/)[0] || 'exit ' + r.status)}`);
  }
  return bad.length ? fail(bad.join('; ')) : pass(`${files.length} scripts under src/js parse (${modules} as ES modules)`);
}

function checkScriptRefs() {
  const html = fs.readFileSync(path.join(ROOT, 'src', 'index.html'), 'utf8');
  const refs = [];
  const re = /<script[^>]*\ssrc="([^"]+)"/g;
  let m; while ((m = re.exec(html))) if (!/^https?:/.test(m[1])) refs.push(m[1]);
  const missing = refs.filter((r) => !exists(path.join(ROOT, 'src', r.split('?')[0])));
  return missing.length ? fail('index.html references missing scripts: ' + missing.join(', ')) : pass(`${refs.length} <script src> references in index.html resolve`);
}

function checkPrivateLabs() {
  if (!which('python3')) return blocked('python3 not found; scripts/assert-no-private-labs.py cannot run');
  const r = run('python3', ['scripts/assert-no-private-labs.py'], { timeout: 60000 });
  return r.status === 0 ? pass('no private-labs leakage under src/') : fail((r.stdout + r.stderr).trim().split(/\r?\n/).slice(-1)[0] || 'assert-no-private-labs.py exit ' + r.status);
}

function checkArtifacts(b) {
  const missing = [];
  if (!b.artifacts.geometryWasm.present) missing.push('src/wasm/geometry_wasm_bg.wasm');
  if (!b.artifacts.vectorizeWasm.present) missing.push('src/wasm-vectorize/vectorize_wasm_bg.wasm');
  if (!b.hostTriple) return blocked('rustc missing, cannot name the host sidecar', { limitations: missing.map((m) => 'also missing: ' + m) });
  if (!b.artifacts.ffmpegSidecar.present) missing.push(b.artifacts.ffmpegSidecar.path || 'src-tauri/binaries/ffmpeg-' + b.hostTriple);
  return missing.length ? fail('committed build artifacts missing: ' + missing.join(', ')) : pass('committed wasm bundles and host ffmpeg sidecar present');
}

function jobCheck(ctx) {
  const b = ctx.receipt.build;
  const sub = {
    'version-sync': checkVersionSync(b),
    'json-valid': checkJsonValid(),
    'js-syntax': checkJsSyntax(ctx),
    'script-refs': checkScriptRefs(),
    'private-labs-guard': checkPrivateLabs(),
    'artifacts-present': checkArtifacts(b),
  };
  const failed = Object.entries(sub).filter(([, r]) => r.status === STATUS.FAIL);
  const blockedOnes = Object.entries(sub).filter(([, r]) => r.status === STATUS.BLOCKED);
  const log = Object.entries(sub).map(([k, r]) => `${r.status.toUpperCase().padEnd(8)} ${k}: ${r.reason}`).join('\n') + '\n';
  const details = Object.fromEntries(Object.entries(sub).map(([k, r]) => [k, { status: r.status, reason: r.reason }]));
  if (failed.length) return fail(failed.map(([k, r]) => `${k}: ${r.reason}`).join(' | '), { details, log });
  if (blockedOnes.length) return blocked(blockedOnes.map(([k, r]) => `${k}: ${r.reason}`).join(' | '), { details, log });
  return pass(`${Object.keys(sub).length} checks pass`, { details, log });
}

// ---- tests -------------------------------------------------------------
function jobTestUnit() {
  const files = fs.readdirSync(path.join(ROOT, 'tests')).filter((f) => f.endsWith('.test.cjs'));
  if (!files.length) return notRun('no tests/*.test.cjs files');
  const r = run(process.execPath, ['--test'].concat(files.map((f) => 'tests/' + f)), { timeout: 10 * 60 * 1000 });
  const m = r.stdout.match(/^ℹ pass (\d+)/m), mf = r.stdout.match(/^ℹ fail (\d+)/m), mt = r.stdout.match(/^ℹ tests (\d+)/m);
  const summary = mt ? `${mt[1]} tests, ${m ? m[1] : '?'} pass, ${mf ? mf[1] : '?'} fail` : `exit ${r.status}`;
  return (r.status === 0 ? pass : fail)(`node --test: ${summary}`, { exitCode: r.status, log: logOf(r), details: { files } });
}

function jobTestRust(ctx, crateDir, label) {
  if (!which('cargo')) return blocked('cargo not found');
  const manifest = path.join(ROOT, crateDir, 'Cargo.toml');
  if (!exists(manifest)) return blocked(`${crateDir}/Cargo.toml missing`);
  if (crateDir === 'src-tauri') {
    const sidecar = caps.nativeFixtureSidecarProbe();
    if (!sidecar.runs) return blocked(`native fixture sidecar unavailable (${sidecar.source}): ${sidecar.path}: ${sidecar.failure}`, {
      details: { nativeFixtureSidecar: sidecar },
    });
  }
  const args = ['test'];
  // The Tauri crate includes wall-clock decoder regression tests. Running
  // those through an unoptimized test binary or beside dozens of other
  // FFmpeg processes measures the harness, not production decoder latency.
  // Keep every assertion and threshold, but exercise optimized code serially.
  if (crateDir === 'src-tauri') args.push('--release');
  args.push('--manifest-path', manifest);
  if (crateDir === 'src-tauri') args.push('--', '--test-threads=1');
  const r = run('cargo', args, { timeout: 60 * 60 * 1000 });
  const results = [...r.stdout.matchAll(/^test result: (\w+)\. (\d+) passed; (\d+) failed/gm)];
  const passed = results.reduce((a, m) => a + Number(m[2]), 0), failed = results.reduce((a, m) => a + Number(m[3]), 0);
  const summary = results.length ? `${results.length} binaries, ${passed} passed, ${failed} failed` : `exit ${r.status}`;
  return (r.status === 0 ? pass : fail)(`cargo test ${label}: ${summary}`, { exitCode: r.status, log: logOf(r) });
}

function jobTestIntegration() {
  const dir = path.join(ROOT, 'tests', 'integration');
  if (!exists(dir)) return notRun('no tests/integration suite defined yet (R12/R13 add document, command/history and persistence contracts)');
  const files = fs.readdirSync(dir).filter((f) => /\.test\.(c?js|mjs)$/.test(f));
  if (!files.length) return notRun('tests/integration exists but holds no *.test.* files');
  const r = run(process.execPath, ['--test'].concat(files.map((f) => 'tests/integration/' + f)), { timeout: 30 * 60 * 1000 });
  return (r.status === 0 ? pass : fail)(`node --test tests/integration: exit ${r.status}`, { exitCode: r.status, log: logOf(r) });
}

function jobTestBrowser(ctx) {
  const runner = caps.resolvable('@playwright/test');
  if (!runner) return blocked('@playwright/test is not installed (not in devDependencies); browser workflows cannot run', { limitations: ['install is a dependency change owned by R07/R03, not done implicitly here'] });
  const dir = path.join(ROOT, 'tests', 'browser');
  if (!exists(dir) || !fs.readdirSync(dir).some((f) => /\.spec\.(c?js|mjs|ts)$/.test(f))) return notRun('runner present but no tests/browser/*.spec.* defined yet (R03 fixtures / R07 gates)');
  const bin = caps.localBin('playwright');
  const output = path.join(ctx.reportDir, 'playwright-results');
  const r = run(bin || process.execPath,
    bin ? ['test', 'tests/browser', '--output', output] : [runner, 'test', 'tests/browser', '--output', output],
    { timeout: 60 * 60 * 1000 });
  return (r.status === 0 ? pass : fail)(`playwright test tests/browser: exit ${r.status}`, { exitCode: r.status, log: logOf(r) });
}

function jobTestDesktop(ctx) {
  const app = caps.findBuiltApp();
  const directory = path.join(ROOT, 'tests', 'desktop');
  const files = exists(directory) ? fs.readdirSync(directory).filter((name) => name.endsWith('.test.cjs')).sort() : [];
  const missing = [];
  if (!app) missing.push('no packaged app found (src-tauri/target/*/release/bundle/macos/Nemo.app or NEMO_DESKTOP_APP)');
  if (!files.length) missing.push('no tests/desktop/*.test.cjs harness defined');
  if (!app) return blocked(missing.join('; '));
  if (!files.length) return blocked(missing.join('; '), { artifacts: [{ path: path.relative(ROOT, app) }] });
  const r = run(process.execPath, ['--test', '--test-reporter=tap', '--test-concurrency=1'].concat(files.map((name) => 'tests/desktop/' + name)), {
    timeout: 30 * 60 * 1000,
    env: { NEMO_DESKTOP_APP: path.resolve(app), NEMO_DESKTOP_REPORT_DIR: ctx.reportDir, NODE_TEST_CONTEXT: undefined },
  });
  const counts = Object.fromEntries(['tests', 'pass', 'fail', 'cancelled', 'skipped', 'todo'].map((name) => {
    const match = r.stdout.match(new RegExp(`^# ${name} (\\d+)$`, 'm'));
    return [name, match ? Number(match[1]) : null];
  }));
  // Node synthesizes a passing file-level test for an empty/comment-only file.
  const syntheticNames = new Set(files.flatMap(name => {
    const relative = 'tests/desktop/' + name; const absolute = path.join(ROOT, relative);
    return [relative, absolute, fs.realpathSync(absolute)];
  }));
  const emptyFiles = [...r.stdout.matchAll(/^# Subtest: (.+)$/gm)]
    .map(match => match[1]).filter(name => syntheticNames.has(name));
  const complete = emptyFiles.length === 0 && counts.tests > 0 && counts.pass === counts.tests
    && ['fail', 'cancelled', 'skipped', 'todo'].every((name) => counts[name] === 0);
  return (r.status === 0 && complete ? pass : fail)(`packaged-native Node harness: exit ${r.status}, ${counts.pass}/${counts.tests} passing`, {
    exitCode: r.status === 0 && !complete ? 1 : r.status, log: logOf(r), details: { files, counts, emptyFiles },
    artifacts: [{ path: path.relative(ROOT, app) }],
    limitations: ['Native process and storage-isolation checks do not establish UI save/reload, rendering, or release acceptance.'],
  });
}

function jobBench(ctx) {
  const runner = path.join(ROOT, 'tests', 'bench', 'run.cjs');
  if (!exists(runner)) return notRun('no tests/bench/run.cjs (R03 initial workloads, R19 budgets)');
  fs.mkdirSync(ctx.reportDir, { recursive: true });
  const out = path.join(ctx.reportDir, 'bench.json');
  const r = run(process.execPath, ['--expose-gc', runner, '--out', out], { timeout: 30 * 60 * 1000 });
  if (r.status !== 0 || !exists(out)) return fail(`bench runner exit ${r.status}`, { exitCode: r.status, log: logOf(r) });
  const b = readJson(out);
  const ran = b.workloads.filter((w) => w.status === 'ran');
  const skipped = b.workloads.filter((w) => w.status !== 'ran');
  const details = Object.fromEntries(b.workloads.map((w) => [w.id, w.status === 'ran' ? { median: w.stats.median, p90: w.stats.p90, unit: w.stats.unit } : { status: w.status, reason: w.reason }]));
  return pass(`${ran.length} workloads measured, ${skipped.length} not-run (no render backend); budgets: none (R19)`, { exitCode: 0, log: logOf(r), details, artifacts: [{ path: path.relative(ROOT, out) }], limitations: skipped.map((w) => `${w.id}: ${w.reason}`) });
}

// ---- builds ------------------------------------------------------------
// build:desktop passes --no-sign: a local verification build never has
// TAURI_SIGNING_PRIVATE_KEY (personal secret, CLAUDE.md §9), and without the
// flag tauri still tries to sign the updater artifact that
// bundle.createUpdaterArtifacts requests, failing AFTER Nemo.app was built.
function jobBuildWasm(ctx) {
  if (!which('wasm-pack')) return blocked('wasm-pack not found (release.yml installs it from rustwasm.github.io); the committed src/wasm bundle cannot be regenerated here');
  const targets = (ctx.receipt.capabilities || {}).rustTargets || [];
  if (targets.length && !targets.includes('wasm32-unknown-unknown')) return blocked('rust target wasm32-unknown-unknown not installed');
  const out = path.join(ctx.reportDir, 'build-wasm');
  const r = run('wasm-pack', ['build', '--target', 'web', '--out-dir', out], { cwd: path.join(ROOT, 'geometry-wasm'), timeout: 60 * 60 * 1000 });
  if (r.status !== 0) return fail(`wasm-pack build exit ${r.status}`, { exitCode: r.status, log: logOf(r) });
  const built = fileInfo(path.join(out, 'geometry_wasm_bg.wasm'));
  const committed = ctx.receipt.build.artifacts.geometryWasm;
  const same = built.sha256 === committed.sha256;
  return pass(`wasm-pack build ok; ${same ? 'byte-identical to' : 'differs from'} committed src/wasm bundle`, {
    exitCode: 0, log: logOf(r), artifacts: [built],
    limitations: same ? [] : ['built bundle differs from the committed one — wasm-pack output is not guaranteed reproducible across toolchains; treat as information, not failure'],
  });
}

function jobBuildDesktop(ctx) {
  const tauri = caps.localBin('tauri');
  const missing = [];
  if (!tauri) missing.push('node_modules/.bin/tauri (run npm ci)');
  if (!which('cargo')) missing.push('cargo');
  if (!ctx.receipt.build.hostTriple) missing.push('rustc host triple');
  if (!which('python3')) missing.push('python3 (scripts/bundle-ffmpeg-dylibs.py)');
  if (process.platform !== 'darwin') missing.push('macOS (isolated desktop build is not validated on this platform)');
  else if (!which('xcode-select')) missing.push('Xcode command line tools');
  if (missing.length) return blocked('missing: ' + missing.join(', '));
  // The controller owns build/copy/finalization deadlines and cleanup.
  // Await its receipt so an outer timer cannot strand the owned launcher.
  const r = run(process.execPath, ['scripts/nemo/build-job.cjs', ctx.reportDir]);
  let result;
  try { result = JSON.parse(r.stdout); } catch { return fail('isolated desktop build controller returned no valid receipt', { exitCode: 1, log: r.stderr }); }
  const expectedExit = result.status === 'pass' ? 0 : result.status === 'blocked' ? 2 : 1;
  if (result.schema !== 'nemo.build-job/1' || !['pass', 'fail', 'blocked'].includes(result.status) || r.status !== expectedExit) {
    return fail('isolated desktop build controller receipt and exit disagree', { exitCode: 1, log: r.stderr });
  }
  if (result.status === 'pass') {
    const app = Array.isArray(result.artifacts) && result.artifacts.find(item => typeof item.path === 'string' && item.path.endsWith('.app'));
    if (!app || !result.details || !result.details.cleanup || !result.details.cleanup.released || !exists(path.resolve(ROOT, app.path))) {
      return fail('isolated desktop build did not preserve a released package', { exitCode: 1 });
    }
    // A later test:desktop in this same job runner must consume this package,
    // not an older shared-target artifact. This does not change the caller's shell.
    process.env.NEMO_DESKTOP_APP = path.resolve(ROOT, app.path);
  }
  const { schema: _schema, ...job } = result;
  return job;
}

// ---- registry ----------------------------------------------------------
// ---- inventory (R03) -------------------------------------------------------
// engineering/inventory/{surfaces.json,surfaces.csv,SURFACES.md} are generated
// from src/ by scripts/nemo/inventory.cjs. This job is the staleness gate
// (`--check`, ~3 s, no network); `npm run inventory` regenerates. It is in
// every verify profile so a UI or handler change cannot land with an
// out-of-date inventory.
function jobInventory() {
  const script = path.join(ROOT, 'scripts', 'nemo', 'inventory.cjs');
  if (!exists(script)) return blocked('scripts/nemo/inventory.cjs is missing');
  const r = run(process.execPath, [script, '--check'], { timeout: 5 * 60 * 1000 });
  const artifacts = ['surfaces.json', 'surfaces.csv', 'SURFACES.md'].map((f) => ({ path: 'engineering/inventory/' + f }));
  const last = (r.stdout + r.stderr).trim().split('\n').filter(Boolean).pop() || `exit ${r.status}`;
  return (r.status === 0 ? pass : fail)(last, { exitCode: r.status, log: logOf(r), artifacts });
}

const JOBS = {
  doctor: { run: jobDoctor, required: true },
  check: { run: jobCheck, required: true },
  inventory: { run: jobInventory, required: true },
  'test:unit': { run: jobTestUnit, required: true },
  'test:rust': { run: (ctx) => jobTestRust(ctx, 'geometry-wasm', 'geometry-wasm'), required: true },
  'test:rust-tauri': { run: (ctx) => jobTestRust(ctx, 'src-tauri', 'src-tauri'), required: false },
  'test:integration': { run: jobTestIntegration, required: false },
  'test:browser': { run: jobTestBrowser, required: false },
  'test:desktop': { run: jobTestDesktop, required: true },
  bench: { run: jobBench, required: false },
  'build:wasm': { run: jobBuildWasm, required: false },
  'build:desktop': { run: jobBuildDesktop, required: true },
};

const PROFILES = {
  quick: ['doctor', 'check', 'inventory', 'test:unit', 'test:rust'],
  full: ['doctor', 'check', 'inventory', 'test:unit', 'test:rust', 'test:rust-tauri', 'test:integration', 'build:wasm', 'test:browser', 'bench', 'build:desktop', 'test:desktop'],
};

function execute(name, ctx) {
  const def = JOBS[name];
  if (!def) throw new Error('unknown job ' + name);
  const t0 = Date.now();
  let res;
  try { res = def.run(ctx) || fail('job returned nothing'); }
  catch (e) { res = fail('exception: ' + (e && e.stack || e)); }
  const job = Object.assign({ name, required: !!def.required, exitCode: null, artifacts: [], limitations: [], details: null, log: null }, res, { durationMs: Date.now() - t0 });
  ctx.receipt.jobs.push(job);
  return job;
}

module.exports = { JOBS, PROFILES, execute };
