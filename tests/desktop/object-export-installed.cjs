#!/usr/bin/env node
'use strict';
// Manual, isolated installed acceptance driver. Never invoked by test:desktop discovery.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const isolation = require('../../scripts/nemo/lib/isolation.cjs');
const runtime = require('../../scripts/nemo/lib/native-runtime.cjs');
const identity = require('../../scripts/nemo/lib/identity.cjs');
const { requireCheck, alive, until, hash, packagedApp, createController, validateSnapshot } = require('./native-harness.cjs');
const root = path.resolve(__dirname, '../..');
const slots = ['desktop-input', 'gpu-reference'];

function argumentsOf(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i];
    requireCheck(['--app', '--executable-sha256', '--evidence-dir'].includes(key) &&
      argv[i + 1] && !Object.hasOwn(args, key), 'exact app, executable hash and evidence directory required');
    args[key] = argv[i + 1];
  }
  requireCheck(Object.keys(args).length === 3, 'exact app, executable hash and evidence directory required');
  return args;
}
function directory(pathname) {
  requireCheck(path.isAbsolute(pathname), 'evidence directory must be absolute');
  const stat = fs.lstatSync(pathname);
  requireCheck(stat.isDirectory() && !stat.isSymbolicLink() && fs.realpathSync(pathname) === pathname,
    'evidence directory must be a canonical real directory');
}
function ownedReportDirectory(reportRoot, taskRoot) {
  requireCheck(path.isAbsolute(reportRoot) && path.isAbsolute(taskRoot) &&
    path.normalize(reportRoot) === reportRoot && path.normalize(taskRoot) === taskRoot,
  'owned report path refused');
  for (const pathname of [reportRoot, taskRoot]) {
    const stat = fs.lstatSync(pathname);
    requireCheck(stat.isDirectory() && !stat.isSymbolicLink(), 'owned report path symlink refused');
  }
  const canonical = fs.realpathSync(reportRoot);
  const knownMacAlias = reportRoot.startsWith('/var/folders/') &&
    canonical === '/private' + reportRoot && fs.realpathSync('/var') === '/private/var';
  requireCheck(canonical === reportRoot || knownMacAlias, 'owned report alias refused');
  requireCheck(path.basename(canonical) === 'reports' &&
    path.dirname(canonical) === fs.realpathSync(taskRoot), 'owned report root differs from task');
  directory(canonical);
  return canonical;
}
function ownedTempDirectory(tempRoot, reportRoot, taskRoot) {
  requireCheck(path.isAbsolute(tempRoot) && path.normalize(tempRoot) === tempRoot,
    'owned temp path refused');
  const stat = fs.lstatSync(tempRoot);
  requireCheck(stat.isDirectory() && !stat.isSymbolicLink(), 'owned temp root symlink refused');
  const canonical = fs.realpathSync(tempRoot);
  const knownMacAlias = tempRoot.startsWith('/var/folders/') &&
    canonical === '/private' + tempRoot && fs.realpathSync('/var') === '/private/var';
  const task = fs.realpathSync(taskRoot);
  requireCheck((canonical === tempRoot || knownMacAlias) &&
    canonical === path.join(task, 'tmp') && path.dirname(reportRoot) === task,
  'owned temp scope binding refused');
  directory(canonical);
  return canonical;
}
function readProbe(reportRoot, source, instance) {
  const file = path.join(reportRoot, 'object-window-probe.json');
  const stat = fs.lstatSync(file);
  requireCheck(stat.isFile() && !stat.isSymbolicLink() && stat.size <= 8192, 'probe receipt unavailable');
  const receipt = JSON.parse(fs.readFileSync(file, 'utf8'));
  requireCheck(receipt.schema === 'nemo.object-window-export-probe/1' &&
    receipt.sourceSha === source.head && receipt.taskKey === isolation.idKey(instance.task) &&
    receipt.pid === instance.snapshot.app.pid && receipt.window === 'main' &&
    /^[a-f0-9-]{36}$/.test(receipt.runId) && Array.isArray(receipt.records) &&
    receipt.records.length <= 6, 'probe owner/source/window identity refused');
  return receipt;
}
async function phase(reportRoot, source, instance, expected, checkpoint) {
  return until(() => {
    let receipt;
    try { receipt = readProbe(reportRoot, source, instance); }
    catch (error) { if (error.code === 'ENOENT') return false; throw error; }
    requireCheck(receipt.failed === false && receipt.checkpoint <= checkpoint &&
      receipt.records.length <= checkpoint + 1, 'probe failed or advanced unexpectedly');
    if (receipt.records.length <= checkpoint) return false;
    const last = receipt.records.at(-1);
    requireCheck(last.phase === expected, 'probe phase differs from fixed order');
    return receipt.waiting === true ? receipt : false;
  }, 'owned export checkpoint timed out', 90_000);
}
function ack(reportRoot, receipt, owner, captureSha256 = null) {
  const file = path.join(reportRoot, 'object-window-probe-control.json');
  const temporary = path.join(reportRoot, `.object-window-probe-control-${crypto.randomUUID()}.tmp`);
  const body = { runId: receipt.runId, sourceSha: receipt.sourceSha,
    checkpoint: receipt.checkpoint, ownerToken: owner, screenshotSha256: captureSha256 };
  try {
    fs.writeFileSync(temporary, JSON.stringify(body), { mode: 0o600, flag: 'wx' });
    fs.renameSync(temporary, file);
  } finally {
    if (fs.existsSync(temporary)) fs.unlinkSync(temporary);
  }
}
function sanitizedProbe(receipt) {
  const phases = new Set(['ready', 'blue', 'refused', 'exported', 'released', 'failed']);
  const stages = new Set(['runtime-binding', 'ready', 'identity', 'bootstrap', 'pins',
    'preview', 'blue-checkpoint', 'stale-refusal', 'refusal-checkpoint', 'export',
    'export-checkpoint', 'release', 'release-checkpoint']);
  const rows = receipt.records.map(row => {
    requireCheck(phases.has(row.phase), 'probe diagnostic phase refused');
    const result = { phase: row.phase };
    if (row.phase === 'failed') {
      requireCheck(stages.has(row.failureStage) && typeof row.cleanupSucceeded === 'boolean',
        'probe diagnostic failure stage refused');
      result.failureStage = row.failureStage;
      result.cleanupSucceeded = row.cleanupSucceeded;
    }
    return result;
  });
  return { schema: 'nemo.object-export-failure-diagnostic/1',
    checkpoint: receipt.checkpoint, waiting: receipt.waiting === true,
    failed: receipt.failed === true, complete: receipt.complete === true, phases: rows };
}
function stageEmpty(app) {
  const stage = path.join(app.dirs.appCache, 'native-export-staging');
  return !fs.existsSync(stage) || fs.readdirSync(stage).length === 0;
}
function pngPixels(file) {
  const source = `import AppKit
import Foundation
let data = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))
guard let image = NSBitmapImageRep(data: data),
      image.pixelsWide == 320, image.pixelsHigh == 180,
      image.bitsPerSample == 8, image.samplesPerPixel == 4, !image.isPlanar,
      image.bytesPerRow >= 320 * 4,
      let data = image.bitmapData else { fatalError("PNG decode refused") }
func pixel(_ x: Int, _ y: Int) -> [Int] {
  let offset = y * image.bytesPerRow + x * 4
  return (0..<4).map { Int(data[offset + $0]) }
}
let value: [String: Any] = ["width": image.pixelsWide, "height": image.pixelsHigh,
  "white": pixel(8, 8), "blue": pixel(40, 40)]
print(String(decoding: try JSONSerialization.data(withJSONObject: value), as: UTF8.self))`;
  const result = spawnSync('/usr/bin/swift', ['-e', source, file], { encoding: 'utf8', timeout: 30_000, maxBuffer: 8192 });
  requireCheck(result.status === 0, 'literal PNG decoder failed');
  const pixels = JSON.parse(result.stdout.trim());
  requireCheck(pixels.width === 320 && pixels.height === 180 &&
    pixels.blue.join(',') === '0,0,255,255' && pixels.white.join(',') === '255,255,255,255',
  'literal pinned PNG pixels differ');
  return pixels;
}
function artifactEvidence(destination, app, receipt, runId) {
  requireCheck(receipt && receipt.apiVersion === 2 && receipt.requestId === 'object-export-' + runId &&
    receipt.status === 'succeeded' && receipt.published === true && receipt.cleanupComplete === true &&
    receipt.cleanupStatus === 'complete' && receipt.externalEffectDisposition === 'committed' &&
    receipt.contentRevision === 0 && receipt.artifactFiles?.join(',') === 'frame_0001.png',
  'native export receipt not committed to one pinned PNG');
  requireCheck(fs.readdirSync(destination).join(',') === 'frame_0001.png', 'published manifest differs');
  const png = path.join(destination, 'frame_0001.png');
  const bytes = fs.readFileSync(png);
  requireCheck(bytes.subarray(0, 8).equals(Buffer.from('89504e470d0a1a0a', 'hex')) && stageEmpty(app),
    'published PNG signature or staging cleanup differs');
  return { pngSha256: hash(png), pixels: pngPixels(png) };
}
async function main() {
  requireCheck(process.platform === 'darwin' && process.env.NEMO_OBJECT_EXPORT_RUN_APPROVED === '1',
    'isolated macOS installed-run grant required');
  const args = argumentsOf(process.argv.slice(2));
  directory(args['--evidence-dir']);
  const capture = path.join(args['--evidence-dir'], 'blue-window.png');
  const evidenceFile = path.join(args['--evidence-dir'], 'object-export-installed.json');
  const failureFile = path.join(args['--evidence-dir'], 'object-export-failure.json');
  requireCheck(!fs.existsSync(capture) && !fs.existsSync(evidenceFile) && !fs.existsSync(failureFile),
    'fresh evidence paths required');
  const source = identity.sourceIdentity();
  requireCheck(source.head && !source.dirty && source.worktree && fs.realpathSync(source.worktree) === fs.realpathSync(root),
    'exact clean checked-out source required');
  const build = identity.buildIdentity();
  const app = packagedApp(args['--app'], runtime);
  requireCheck(app.executableSha256 === args['--executable-sha256'], 'package executable hash differs');
  const task = 'object-export-' + Date.now().toString(36) + '-' + crypto.randomBytes(5).toString('hex');
  const env = { ...process.env, NEMO_TEST_OBJECT_EXPORT_PROBE: '1', NEMO_TEST_OBJECT_WINDOW_PROBE: '0',
    NEMO_OBJECT_PROBE_EXPECTED_SOURCE: source.head };
  const controller = createController({ root, app: app.bundle, env, timeout: 120_000 });
  let instance, reportRoot, packet, stopped = false, error;
  try {
    instance = await controller.start(task, slots);
    const status = await controller.status(instance);
    requireCheck(status.code === 0, 'owned native status refused');
    validateSnapshot(status.value, instance, { isolation, runtime, source, build, app });
    reportRoot = ownedReportDirectory(instance.snapshot.runtime.roots.reports,
      instance.snapshot.runtime.roots.root);
    const tempRoot = ownedTempDirectory(instance.snapshot.runtime.roots.temp,
      reportRoot, instance.snapshot.runtime.roots.root);
    requireCheck(instance.info.reservations.join(',') === slots.join(','), 'app/GPU slot reservation missing');
    const ready = await phase(reportRoot, source, instance, 'ready', 0);
    ack(reportRoot, ready, instance.info.ownerToken);
    const blue = await phase(reportRoot, source, instance, 'blue', 1);
    process.stdout.write(JSON.stringify({ stage: 'capture-blue-window', capture }) + '\n');
    await until(() => fs.existsSync(capture), 'independent blue-window capture timed out', 85_000);
    const captureStat = fs.lstatSync(capture);
    requireCheck(captureStat.isFile() && !captureStat.isSymbolicLink() && captureStat.size > 100,
      'fresh original-window capture refused');
    const captureHash = hash(capture);
    ack(reportRoot, blue, instance.info.ownerToken, captureHash);
    const refused = await phase(reportRoot, source, instance, 'refused', 2);
    const destination = path.join(tempRoot, 'object-export-' + refused.runId);
    requireCheck(!fs.existsSync(destination) && stageEmpty(instance.snapshot.app),
      'wrong-snapshot request left an artifact or staging residue');
    ack(reportRoot, refused, instance.info.ownerToken);
    const exported = await phase(reportRoot, source, instance, 'exported', 3);
    const receipt = exported.records[3].export;
    const { pixels, pngSha256 } = artifactEvidence(destination, instance.snapshot.app, receipt, exported.runId);
    ack(reportRoot, exported, instance.info.ownerToken);
    const released = await phase(reportRoot, source, instance, 'released', 4);
    requireCheck(released.records[4].cleanupSucceeded === true, 'native owner release refused');
    ack(reportRoot, released, instance.info.ownerToken);
    await until(() => {
      const value = readProbe(reportRoot, source, instance);
      return value.complete === true && value.failed === false && value.checkpoint === 5 &&
        value.records.at(-1)?.phase === 'released' ? value : false;
    }, 'original owner release or probe completion timed out', 90_000);
    const stop = await controller.stop(instance);
    requireCheck(stop.released?.released === true && !alive(instance.snapshot.app.pid) &&
      !fs.existsSync(instance.info.roots.root), 'owned app/process/task cleanup incomplete');
    stopped = true;
    packet = { schema: 'nemo.object-export-installed/1', sourceSha: source.head,
      appExecutableSha256: app.executableSha256, runId: exported.runId,
      captureSha256: captureHash, pngSha256, pixels,
      phases: ['ready', 'blue', 'refused', 'exported', 'released'],
      privateRoute: true, published: true, ownerReleased: true, taskRemoved: true };
  } catch (cause) {
    error = cause;
    if (instance && reportRoot) {
      try {
        const probe = readProbe(reportRoot, source, instance);
        fs.writeFileSync(failureFile, JSON.stringify(sanitizedProbe(probe)) + '\n',
          { flag: 'wx', mode: 0o600 });
      } catch (_) { /* Preserve the original refusal; never copy raw private probe data. */ }
    }
  }
  try { await controller.cleanup(); } catch (cause) { error ||= cause; }
  if (error || !stopped || !packet) throw error || new Error('installed export remained incomplete');
  fs.writeFileSync(evidenceFile, JSON.stringify(packet, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
  process.stdout.write(JSON.stringify({ status: 'pass', sourceSha: packet.sourceSha,
    appExecutableSha256: packet.appExecutableSha256, evidence: evidenceFile }) + '\n');
}
if (require.main === module) {
  main().catch(error => { process.stderr.write('installed object export FAIL: ' + error.message + '\n'); process.exitCode = 1; });
}
module.exports = { pngPixels, artifactEvidence, ownedReportDirectory, ownedTempDirectory, ack, sanitizedProbe };
