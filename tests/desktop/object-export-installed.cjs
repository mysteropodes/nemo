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
  const body = { runId: receipt.runId, sourceSha: receipt.sourceSha,
    checkpoint: receipt.checkpoint, ownerToken: owner, screenshotSha256: captureSha256 };
  fs.writeFileSync(file, JSON.stringify(body), { mode: 0o600, flag: 'w' });
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
      let blue = image.colorAt(x: 8, y: 8)?.usingColorSpace(.deviceRGB),
      let white = image.colorAt(x: 40, y: 40)?.usingColorSpace(.deviceRGB) else { fatalError("PNG decode refused") }
func pixel(_ c: NSColor) -> [Int] { [c.redComponent, c.greenComponent, c.blueComponent, c.alphaComponent].map { Int(($0 * 255).rounded()) } }
let value: [String: Any] = ["width": image.pixelsWide, "height": image.pixelsHigh,
  "blue": pixel(blue), "white": pixel(white)]
print(String(decoding: try JSONSerialization.data(withJSONObject: value), as: UTF8.self))`;
  const result = spawnSync('/usr/bin/swift', ['-e', source, file], { encoding: 'utf8', timeout: 30_000, maxBuffer: 8192 });
  requireCheck(result.status === 0, 'literal PNG decoder failed');
  const pixels = JSON.parse(result.stdout.trim());
  requireCheck(pixels.width === 320 && pixels.height === 180 &&
    pixels.blue.join(',') === '0,0,255,255' && pixels.white.join(',') === '255,255,255,255',
  'literal pinned PNG pixels differ');
  return pixels;
}
async function main() {
  requireCheck(process.platform === 'darwin' && process.env.NEMO_OBJECT_EXPORT_RUN_APPROVED === '1',
    'isolated macOS installed-run grant required');
  const args = argumentsOf(process.argv.slice(2));
  directory(args['--evidence-dir']);
  const capture = path.join(args['--evidence-dir'], 'blue-window.png');
  const evidenceFile = path.join(args['--evidence-dir'], 'object-export-installed.json');
  requireCheck(!fs.existsSync(capture) && !fs.existsSync(evidenceFile), 'fresh evidence paths required');
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
  let instance, packet, stopped = false, error;
  try {
    instance = await controller.start(task, slots);
    const status = await controller.status(instance);
    requireCheck(status.code === 0, 'owned native status refused');
    validateSnapshot(status.value, instance, { isolation, runtime, source, build, app });
    const reportRoot = instance.snapshot.runtime.roots.reports;
    directory(reportRoot);
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
    const destination = path.join(reportRoot, 'object-export-' + refused.runId);
    requireCheck(!fs.existsSync(destination) && stageEmpty(instance.snapshot.app),
      'wrong-snapshot request left an artifact or staging residue');
    ack(reportRoot, refused, instance.info.ownerToken);
    const exported = await phase(reportRoot, source, instance, 'exported', 3);
    const receipt = exported.records[3].export;
    requireCheck(receipt && receipt.apiVersion === 2 && receipt.requestId === 'object-export-' + exported.runId &&
      receipt.status === 'succeeded' && receipt.published === true && receipt.cleanupComplete === true &&
      receipt.cleanupStatus === 'complete' && receipt.externalEffectDisposition === 'committed' &&
      receipt.contentRevision === 0 && receipt.artifactFiles?.join(',') === 'frame_0001.png',
    'native export receipt not committed to one pinned PNG');
    requireCheck(fs.readdirSync(destination).join(',') === 'frame_0001.png', 'published manifest differs');
    const png = path.join(destination, 'frame_0001.png');
    const bytes = fs.readFileSync(png);
    requireCheck(bytes.subarray(0, 8).equals(Buffer.from('89504e470d0a1a0a', 'hex')) && stageEmpty(instance.snapshot.app),
      'published PNG signature or staging cleanup differs');
    const pixels = pngPixels(png);
    const pngSha256 = hash(png);
    ack(reportRoot, exported, instance.info.ownerToken);
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
  } catch (cause) { error = cause; }
  try { await controller.cleanup(); } catch (cause) { error ||= cause; }
  if (error || !stopped || !packet) throw error || new Error('installed export remained incomplete');
  fs.writeFileSync(evidenceFile, JSON.stringify(packet, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
  process.stdout.write(JSON.stringify({ status: 'pass', sourceSha: packet.sourceSha,
    appExecutableSha256: packet.appExecutableSha256, evidence: evidenceFile }) + '\n');
}
main().catch(error => { process.stderr.write('installed object export FAIL: ' + error.message + '\n'); process.exitCode = 1; });
