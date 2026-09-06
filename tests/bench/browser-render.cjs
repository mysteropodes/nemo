#!/usr/bin/env node
'use strict';
// Opt-in production renderer measurement; external Playwright, no package changes.
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { sourceIdentity, platformIdentity } = require('../../scripts/nemo/lib/identity.cjs');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');
const { buildWorkload, QUICK_WORKLOAD } = require('./run.cjs');

const root = path.resolve(__dirname, '../..');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const args = process.argv.slice(2);
const option = (name, fallback) => args.includes(name) ? (args[args.indexOf(name) + 1] || fallback) : fallback;
const output = path.resolve(option('--out', path.join(os.tmpdir(), `nemo-r03-render-${process.pid}-${Date.now()}`)));
const report = {
  schema: 'nemo.r03.browser-render.v1', status: 'running', generatedAt: new Date().toISOString(),
  budgets: null, source: {}, platform: platformIdentity(), backend: {}, workload: {},
  samples: [], metrics: null, pageErrors: [], artifacts: [],
  cleanup: { browserClosed: false, runtimeClosed: false, runtimeRootsRemoved: false },
  limits: [
    'Headless software WebGPU; no exclusive reference hardware reservation or performance budget.',
    'Queue completion is not compositor presentation, scanout, GPU timestamp duration, or realtime playback FPS.',
    'Serial production frame navigation plus renderNow; background renderer tick suspended through the production API.',
    'Browser only: no native export, Tauri, rebuild, or packaged-app acceptance.',
  ],
};

function blocked(message) { const error = new Error(message); error.blocked = true; throw error; }
function positive(name, fallback, max) {
  const value = Number(option(name, fallback));
  assert.ok(Number.isSafeInteger(value) && value > 0 && value <= max, `${name} must be an integer in 1..${max}`);
  return value;
}
function chromeExecutable() {
  const candidates = [process.env.NEMO_CHROME_EXECUTABLE, ...(process.platform === 'darwin'
    ? ['/Applications/Google Chrome.app/Contents/MacOS/Google Chrome']
    : process.platform === 'win32'
      ? [path.join(process.env.PROGRAMFILES || '', 'Google/Chrome/Application/chrome.exe')]
      : ['/usr/bin/google-chrome', '/usr/bin/chromium', '/usr/bin/chromium-browser'])];
  const executable = candidates.find(file => file && fs.existsSync(file));
  if (!executable) blocked('Chrome/Chromium missing; set NEMO_CHROME_EXECUTABLE');
  return executable;
}
function stats(samples) {
  const values = samples.slice().sort((a, b) => a - b);
  assert.ok(values.length && values.every(value => Number.isFinite(value) && value >= 0));
  const at = q => values[Math.max(0, Math.ceil(q * values.length) - 1)];
  return { n: values.length, unit: 'ms', p50: at(.5), p95: at(.95), p99: at(.99),
    min: values[0], max: values.at(-1), mean: values.reduce((a, b) => a + b, 0) / values.length };
}
async function until(page, predicate, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (!await page.evaluate(predicate)) {
    if (Date.now() >= deadline) throw new Error('Application readiness timed out');
    await new Promise(resolve => setTimeout(resolve, 100));
  }
}
async function identityCheck(runtime, stage) {
  const response = await fetch(runtime.origin + IDENTITY_PATH);
  const identity = await response.json();
  assert.equal(response.status, 200, `${stage} identity HTTP status`);
  assert.equal(identity.healthy, true, `${stage} runtime healthy`);
  assert.equal(identity.source.current.head, report.source.head, `${stage} source HEAD`);
  report.runtimeIdentity ||= {};
  report.runtimeIdentity[stage] = { healthy: identity.healthy, sourceMatches: identity.source.matches, buildMatches: identity.build.matches };
}
async function verifyServedSource(runtime) {
  const scripts = [...fs.readFileSync(path.join(root, 'src/index.html'), 'utf8')
    .matchAll(/<script\b[^>]*\bsrc=["']([^"']+)["']/gi)].map(match => match[1].split('?')[0]);
  const files = ['index.html', ...scripts, 'wasm/geometry_wasm.js', 'wasm/geometry_wasm_bg.wasm'];
  report.source.files = {};
  for (const file of [...new Set(files)]) {
    assert.ok(!file.includes('://') && !file.includes('..'), `local source asset: ${file}`);
    const local = fs.readFileSync(path.join(root, 'src', file));
    const response = await fetch(`${runtime.origin}/${file}`);
    assert.equal(response.status, 200, `${file} served`);
    assert.equal(hash(Buffer.from(await response.arrayBuffer())), hash(local), `${file} served bytes`);
    report.source.files[`src/${file}`] = hash(local);
  }
  report.source.assetSetSha256 = hash(JSON.stringify(report.source.files));
}

// Observe the actual device/queue requested by production WASM, not a probe
// adapter. Delegating wrappers retain arguments, receiver, result and errors.
function observeGPU() {
  const probe = window.__r03Render = { devices: [], errors: [], active: null, calls: [], queues: new Set() };
  probe.drain = async (queues, timeoutMs = 180000) => {
    let timer;
    try {
      await Promise.race([Promise.all([...queues].map(queue => queue.onSubmittedWorkDone())),
        new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('GPU completion timed out')), timeoutMs); })]);
    } finally { clearTimeout(timer); }
  };
  if (!window.GPUAdapter || !window.GPUQueue) return;
  const request = GPUAdapter.prototype.requestDevice;
  GPUAdapter.prototype.requestDevice = async function (...args) {
    const device = await Reflect.apply(request, this, args);
    const info = this.info;
    const entry = { queue: device.queue, info: info ? {
      vendor: info.vendor, architecture: info.architecture, device: info.device,
      description: info.description, isFallbackAdapter: info.isFallbackAdapter,
    } : null };
    probe.devices.push(entry);
    device.addEventListener('uncapturederror', event => probe.errors.push(event.error.message));
    device.lost.then(info => probe.errors.push(`device lost: ${info.reason}: ${info.message}`));
    return device;
  };
  const submit = GPUQueue.prototype.submit;
  GPUQueue.prototype.submit = function (...args) {
    const result = Reflect.apply(submit, this, args);
    probe.queues.add(this);
    if (probe.active) { probe.active.queues.add(this); probe.active.submissions++; }
    return result;
  };
}

async function instrumentRenderer(page) {
  return page.evaluate(async () => {
    const urls = performance.getEntriesByType('resource').map(entry => entry.name)
      .filter(name => new URL(name).pathname === '/wasm/geometry_wasm.js');
    if (new Set(urls).size !== 1) throw new Error('Cannot identify the production WASM module instance');
    // The exact cache-busted URL is essential: importing a second module would
    // instrument an unused VelloEngine prototype and provide false evidence.
    const module = await import(urls[0]);
    const original = module.VelloEngine.prototype.render;
    const probe = window.__r03Render;
    module.VelloEngine.prototype.render = function (...args) {
      const call = { queues: new Set(), submissions: 0, started: performance.now() };
      if (probe.active) throw new Error('Overlapping VelloEngine.render observation');
      probe.active = call;
      try { return Reflect.apply(original, this, args); }
      finally { call.cpuMs = performance.now() - call.started; probe.calls.push(call); probe.active = null; }
    };
    try {
      SMEngineBridge.suspend();
      SMEngineBridge.setPreviewRenderScale(1);
      await probe.drain(probe.queues);
    } catch (error) {
      if (error.name !== 'OperationError' && error.message !== 'GPU completion timed out') throw error;
      return { unavailable: `Initial production GPU setup failed: ${error.name}: ${error.message}` };
    }
    return { modulePath: new URL(urls[0]).pathname, ready: GeometryWasm.ready,
      enabled: SMEngineBridge.isEnabled() };
  });
}

async function measureFrame(page, frame, timeoutMs) {
  return page.evaluate(async ({ frame, timeoutMs }) => {
    const probe = window.__r03Render;
    const demand = (condition, reason) => { if (!condition) throw new Error(reason); };
    demand(SMEngineBridge.isEnabled(), 'Rust renderer disabled; Paper fallback rejected');
    // Drain startup/previous work before opening this measurement window.
    await probe.drain(probe.queues, timeoutMs);
    probe.calls = [];
    const started = performance.now();
    SM.goToFrame(frame);
    const navigated = performance.now();
    SMEngineBridge.renderNow();
    const submitted = performance.now();
    const calls = probe.calls.slice();
    demand(state.currentFrame === frame, 'Production frame navigation did not reach requested frame');
    demand(calls.length > 0, 'No production VelloEngine.render call observed');
    demand(calls.every(call => call.submissions > 0), 'Rust render returned without observed GPU submissions');
    const queues = [...new Set(calls.flatMap(call => [...call.queues]))];
    const devices = queues.map(queue => probe.devices.findIndex(device => device.queue === queue));
    demand(devices.every(index => index >= 0), 'Render queue was not captured from the production device');
    await probe.drain(queues, timeoutMs);
    const completed = performance.now();
    demand(probe.calls.length === calls.length, 'Additional rendering contaminated the measurement window');
    demand(probe.errors.length === 0, `WebGPU error: ${probe.errors.join('; ')}`);
    demand(SMEngineBridge.isEnabled(), 'Renderer fell back during measurement');
    const scene = JSON.parse(window.__lastSceneJson);
    demand(Math.abs(scene.time - frame / state.fps) < 1e-8, 'Rendered scene time differs from requested frame');
    const livePaths = userLayers.reduce((sum, layer) => sum + layer.children.length, 0);
    demand(livePaths === 200, `Expected 200 materialized workload paths, got ${livePaths}`);
    const canvas = document.querySelector('#rust-canvas');
    demand(canvas && canvas.width > 0 && canvas.height > 0, 'Measured render target must be nonempty');
    demand(SMEngineBridge.getPreviewRenderScale() === 1, 'Preview scale changed during measurement');
    return { frame, sceneTime: scene.time, livePaths, sceneBytes: window.__lastSceneJson.length,
      canvas: { width: canvas.width, height: canvas.height }, previewScale: SMEngineBridge.getPreviewRenderScale(),
      renderCalls: calls.length, queueSubmissions: calls.reduce((sum, call) => sum + call.submissions, 0),
      deviceIndices: devices, completed: true, navigationMs: navigated - started,
      renderCallCpuMs: calls.reduce((sum, call) => sum + call.cpuMs, 0),
      navigationAndSubmitMs: submitted - started, queueWaitMs: completed - submitted,
      frameToQueueCompleteMs: completed - started };
  }, { frame, timeoutMs });
}

async function main() {
  fs.mkdirSync(output, { recursive: true });
  for (let i = 0; i < args.length; i += 2) assert.ok(['--out', '--iterations', '--warmup', '--timeout-ms'].includes(args[i]) && args[i + 1], `unknown or incomplete option ${args[i]}`);
  const iterations = positive('--iterations', 1, 100), warmup = positive('--warmup', 1, 24);
  const timeoutMs = positive('--timeout-ms', 180000, 600000);
  const identity = sourceIdentity();
  const { head, branch, dirty, dirtyDigest, changedPaths } = identity;
  report.source = { head, branch, dirty, dirtyDigest, changedPaths, harnessSha256: hash(fs.readFileSync(__filename)) };
  const built = buildWorkload(QUICK_WORKLOAD);
  report.workload = { ...built.identity, params: built.params, iterations, warmupFrames: warmup, framesPerIteration: built.doc.totalFrames,
    manifestSha256: hash(fs.readFileSync(path.join(root, 'tests/fixtures/manifest.json'))) };
  for (const file of ['tests/fixtures/lib/corpus.cjs', 'tests/fixtures/generate.cjs', 'tests/bench/run.cjs']) {
    report.source[file] = hash(fs.readFileSync(path.join(root, file)));
  }
  for (const file of ['src/wasm/geometry_wasm.js', 'src/wasm/geometry_wasm_bg.wasm']) {
    if (!fs.existsSync(path.join(root, file))) blocked(`Production renderer artifact missing: ${file}`);
  }
  let chromium, executable;
  try {
    const dependency = process.env.NEMO_PLAYWRIGHT_MODULE || 'playwright';
    ({ chromium } = require(dependency));
    const packageFile = path.join(path.dirname(require.resolve(dependency)), 'package.json');
    report.backend.playwright = { version: JSON.parse(fs.readFileSync(packageFile)).version, packageSha256: hash(fs.readFileSync(packageFile)) };
    executable = chromeExecutable();
  } catch (error) { blocked(`Browser prerequisite unavailable: ${error.message.split('\nRequire stack:')[0]}`); }
  const launchArgs = ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--use-webgpu-adapter=swiftshader', '--enable-unsafe-webgpu'];
  report.backend = { ...report.backend, renderer: 'production Rust/vello WASM WebGPU', mode: 'headless-software', launchArgs,
    executableSha256: hash(fs.readFileSync(executable)), completion: 'actual render queues: GPUQueue.onSubmittedWorkDone' };
  report.clock = { timer: 'browser performance.now()', unit: 'ms', percentiles: 'nearest rank',
    schedule: 'serial frames 0..23, no realtime pacing', warmupExcluded: true };
  let runtime, browser;
  try {
    runtime = await startBrowserRuntime({ taskId: `r03-render-${process.pid}-${Date.now()}`, port: 0 });
    await identityCheck(runtime, 'before');
    await verifyServedSource(runtime);
    try { browser = await chromium.launch({ executablePath: executable, headless: true, args: launchArgs }); }
    catch (error) { blocked(`Chromium launch unavailable: ${error.message}`); }
    report.backend.browserVersion = browser.version();
    const context = await browser.newContext({ viewport: { width: 1400, height: 1000 }, deviceScaleFactor: 1 });
    await context.addInitScript(observeGPU);
    const page = await context.newPage();
    page.setDefaultTimeout(timeoutMs);
    page.on('pageerror', error => report.pageErrors.push(error.message));
    await page.goto(runtime.origin, { waitUntil: 'networkidle', timeout: timeoutMs });
    // Paper installs its own window.Symbol; avoid Playwright's polling utility
    // script in that realm, as the existing acceptance harnesses also do.
    await until(page, () => !!(window.SM && window.SMEngineBridge), timeoutMs);
    if (!await page.evaluate(() => !!navigator.gpu)) blocked('WebGPU unavailable; no Paper fallback permitted');
    try {
      await until(page, () => !!(window.GeometryWasm && GeometryWasm.ready && SMEngineBridge.isEnabled()), timeoutMs);
      await page.evaluate(async () => { await SMEngineBridge.setEnabled(true); });
    } catch { blocked('Production Rust/WebGPU engine did not initialize; no fallback benchmark was run'); }
    report.backend.runtime = await instrumentRenderer(page);
    if (report.backend.runtime.unavailable) blocked(report.backend.runtime.unavailable);
    const imported = await page.evaluate(json => {
      SM.importJSON(json, true);
      if (window.SMProject && SMProject.hideStartScreen) SMProject.hideStartScreen();
      return { layers: state.layers.length, frames: state.totalFrames, width: state.canvasW, height: state.canvasH };
    }, built.json);
    assert.deepEqual(imported, { layers: 8, frames: 24, width: 1920, height: 1080 });
    report.workload.imported = imported;
    for (let frame = 0; frame < warmup; frame++) await measureFrame(page, frame, timeoutMs);
    for (let iteration = 0; iteration < iterations; iteration++) {
      for (let frame = 0; frame < 24; frame++) report.samples.push({ iteration, ...await measureFrame(page, frame, timeoutMs) });
    }
    report.backend.devices = await page.evaluate(() => window.__r03Render.devices.map(device => device.info));
    report.backend.runtime.canvas = report.samples[0].canvas;
    report.backend.runtime.previewScale = report.samples[0].previewScale;
    for (const sample of report.samples) assert.deepEqual(sample.canvas, report.backend.runtime.canvas, 'stable measured render-target dimensions');
    for (const index of new Set(report.samples.flatMap(sample => sample.deviceIndices))) {
      const info = report.backend.devices[index];
      assert.ok(info && /swiftshader/i.test(`${info.architecture} ${info.description}`), 'actual render device must identify SwiftShader');
      assert.equal(info.isFallbackAdapter, true, 'software adapter identity');
    }
    report.clock.environment = await page.evaluate(() => ({ timeOrigin: performance.timeOrigin, crossOriginIsolated,
      hardwareConcurrency: navigator.hardwareConcurrency, devicePixelRatio, visibilityState: document.visibilityState }));
    for (const field of ['navigationMs', 'renderCallCpuMs', 'navigationAndSubmitMs', 'queueWaitMs', 'frameToQueueCompleteMs']) {
      report.metrics ||= {}; report.metrics[field] = stats(report.samples.map(sample => sample[field]));
    }
    report.completedFrames = report.samples.filter(sample => sample.completed).length;
    assert.equal(report.completedFrames, iterations * 24);
    assert.deepEqual(report.pageErrors, [], 'application page errors');
    await identityCheck(runtime, 'after');
    report.status = 'pass';
  } finally {
    try { if (browser) { await browser.close(); report.cleanup.browserClosed = true; } }
    finally {
      if (runtime) {
        await runtime.close(); report.cleanup.runtimeClosed = true;
        // Remove only this invocation's exact root, after browser and server close.
        // The generic release API refuses a still-live launcher (this process).
        if (!browser || report.cleanup.browserClosed) {
          fs.rmSync(runtime.roots.root, { recursive: true, force: true });
          report.cleanup.runtimeRootsRemoved = !fs.existsSync(runtime.roots.root);
        }
      }
    }
  }
}

main().catch(error => {
  report.status = error.blocked ? 'blocked' : 'fail';
  report.failure = error.message.replaceAll(root, '<checkout>');
  for (const name of ['NEMO_PLAYWRIGHT_MODULE', 'NEMO_CHROME_EXECUTABLE']) {
    if (process.env[name]) report.failure = report.failure.replaceAll(process.env[name], `<${name}>`);
  }
  process.exitCode = error.blocked ? 2 : 1;
}).finally(() => {
  fs.mkdirSync(output, { recursive: true });
  fs.writeFileSync(path.join(output, 'receipt.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({ status: report.status, completedFrames: report.completedFrames || 0,
    failure: report.failure, receipt: path.join(output, 'receipt.json') }));
});
