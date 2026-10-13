'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const os = require('node:os');
const zlib = require('node:zlib');
const { pngPixels, artifactEvidence, ownedReportDirectory, ack, sanitizedProbe } = require('./object-export-installed.cjs');
const script = fs.readFileSync(path.join(__dirname, 'object-export-probe-bootstrap.js'), 'utf8');
const runId = '11111111-1111-4111-8111-111111111111';
const destinationDirectory = '/private/tmp/nemo-runtime/tasks/owned/reports/object-export-' + runId;
const config = { runId, pid: 42, taskKey: 'a'.repeat(64), sourceSha: 'b'.repeat(40), destinationDirectory };
async function scenario(options = {}) {
  const calls = [], phases = [], exports = [];
  let resume, released = false;
  const invoke = async (name, args = {}) => {
    calls.push({ name, args });
    if (name === 'nemo_task_runtime') return { pid: options.foreign ? 43 : 42, taskKey: config.taskKey, windows: ['main'] };
    if (name === 'nemo_test_object_window_probe') {
      const packet = JSON.parse(args.payloadJson); phases.push(packet);
      if (packet.phase === options.blockAt) await new Promise(resolve => { resume = resolve; });
      if (packet.phase === options.failCheckpoint) throw new Error('checkpoint refused');
      return;
    }
    if (name === 'nemo_mcp_identity') return { instanceId: 'owned-instance' };
    if (name === 'nemo_native_object_bootstrap') {
      const request = JSON.parse(args.requestJson);
      assert.equal(request.instanceId, 'owned-instance');
      assert.deepEqual(JSON.parse(request.documentJson).objects[0].fill,
        { kind: 'solid', r: 0, g: 0, b: 1, a: 1 });
      return { instanceId: 'owned-instance', documentId: 'owned-document', contentRevision: 0 };
    }
    if (name === 'nemo_native_object_client_status') return { available: true, instanceId: 'owned-instance',
      documentId: 'owned-document', contentRevision: 0, lifecycleGeneration: 7 };
    if (name === 'nemo_native_object_dispatch') {
      const request = JSON.parse(args.requestJson);
      assert.equal(request.operation, 'query.document.object');
      assert.equal(request.payload.atRevision, 0);
      return { ok: true, instanceId: 'owned-instance', documentId: 'owned-document',
        result: { documentSnapshotId: 'owned-snapshot' } };
    }
    if (name === 'nemo_native_object_preview') {
      const request = JSON.parse(args.requestJson);
      assert.equal(request.documentSnapshotId, 'owned-snapshot');
      assert.equal(request.contentRevision, 0);
      return { status: options.failPreview ? 'failed' : 'presented', workId: 'native-work-1' };
    }
    if (name === 'nemo_native_object_export') {
      const request = JSON.parse(args.requestJson); exports.push(request);
      assert.equal(request.destinationDirectory, destinationDirectory);
      assert.equal(request.contentRevision, 0);
      assert.equal(request.lifecycleGeneration, 7);
      assert.deepEqual(request.outputSpec, { kind: 'frame', format: 'rgba8', width: 320,
        height: 180, colorInterpretation: 'srgb', alphaMode: 'straight' });
      if (request.documentSnapshotId.endsWith('-wrong') && !options.staleReturnsSuccess) throw new Error('snapshot refused');
      if (options.failExport) throw new Error('export failed');
      return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId,
        documentId: request.documentId, contentRevision: 0, documentSnapshotId: 'owned-snapshot',
        status: 'succeeded', cleanupStatus: 'complete', externalEffectDisposition: 'committed',
        errorCode: null, artifactFiles: ['frame_0001.png'], published: !options.falsePublish,
        cleanupComplete: true };
    }
    if (name === 'nemo_native_release') {
      assert.equal(args.request.expectedRevision, 0);
      if (options.failRelease) throw new Error('release failed');
      released = true; return { status: 'succeeded', reentryAvailable: true };
    }
    throw new Error('unexpected invoke ' + name);
  };
  const window = { __NEMO_OBJECT_EXPORT_PROBE_CONFIG__: { ...config, ...options.config },
    __TAURI__: { core: { invoke } }, devicePixelRatio: 2 };
  vm.runInNewContext(script, { window, setTimeout: fn => { Promise.resolve().then(fn); return 1; } });
  for (let tick = 0; tick < 300 && !phases.some(p => p.phase === 'released' || p.phase === 'failed'); tick++) await Promise.resolve();
  return { calls, phases, exports, released, resume };
}
test('fixed owned original export uses presented pins, refuses stale snapshot, then publishes one artifact', async () => {
  const result = await scenario();
  assert.deepEqual(result.phases.map(p => p.phase), ['ready', 'blue', 'refused', 'exported', 'released']);
  assert.equal(result.phases[1].workId, 'native-work-1');
  assert.equal(result.phases[3].export.published, true);
  assert.deepEqual(result.phases[3].export.artifactFiles, ['frame_0001.png']);
  assert.equal(result.exports.length, 2);
  assert.equal(result.exports[0].documentSnapshotId, 'owned-snapshot-wrong');
  assert.equal(result.exports[1].documentSnapshotId, 'owned-snapshot');
  assert.equal(result.exports[0].destinationDirectory, result.exports[1].destinationDirectory);
  assert.equal(result.released, true);
});
test('blue checkpoint blocks both export invocations until external owned ACK', async () => {
  const result = await scenario({ blockAt: 'blue' });
  assert.deepEqual(result.phases.map(p => p.phase), ['ready', 'blue']);
  assert.equal(result.exports.length, 0);
  assert.equal(typeof result.resume, 'function'); result.resume();
  for (let tick = 0; tick < 150 && !result.phases.some(p => p.phase === 'released'); tick++) await Promise.resolve();
  assert.equal(result.exports.length, 2);
  assert.equal(result.phases.at(-1).phase, 'released');
});
test('foreign runtime, arbitrary config and failed export never claim publication', async () => {
  assert.equal((await scenario({ foreign: true })).calls.some(c => c.name === 'nemo_native_object_bootstrap'), false);
  for (const bad of [{ eval: 'arbitrary' }, { sourceSha: 'wrong' }, { destinationDirectory: '/tmp/foreign' }]) {
    assert.equal((await scenario({ config: bad })).calls.length, 0);
  }
  for (const options of [{ failPreview: true }, { failExport: true }, { falsePublish: true }]) {
    const result = await scenario(options);
    assert.equal(result.phases.at(-1).phase, 'failed');
    assert.equal(result.phases.at(-1).failureStage,
      options.failPreview ? 'preview' : 'export');
    assert.equal(result.phases.some(p => p.phase === 'exported'), false);
    assert.equal(result.released, true);
  }
  const staleSuccess = await scenario({ staleReturnsSuccess: true });
  assert.equal(staleSuccess.phases.at(-1).failureStage, 'stale-refusal');
  assert.equal(staleSuccess.exports.length, 1);
  const blueAck = await scenario({ failCheckpoint: 'blue' });
  assert.equal(blueAck.phases.at(-1).failureStage, 'blue-checkpoint');
  const failedRelease = await scenario({ failExport: true, failRelease: true });
  assert.equal(failedRelease.phases.at(-1).cleanupSucceeded, false);
});

test('ACK replacement never exposes a truncated control file and never follows a target symlink', () => {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-object-export-ack-'));
  const target = path.join(folder, 'object-window-probe-control.json');
  const foreign = path.join(folder, 'foreign');
  const receipt = { runId, sourceSha: config.sourceSha, checkpoint: 1 };
  try {
    ack(folder, { ...receipt, checkpoint: 0 }, 'private-owner');
    const before = fs.readFileSync(target, 'utf8');
    const rename = fs.renameSync;
    try {
      fs.renameSync = (temporary, destination) => {
        assert.equal(destination, target);
        assert.equal(fs.readFileSync(target, 'utf8'), before);
        assert.equal(JSON.parse(fs.readFileSync(temporary, 'utf8')).checkpoint, 1);
        return rename(temporary, destination);
      };
      ack(folder, receipt, 'private-owner', 'a'.repeat(64));
    } finally { fs.renameSync = rename; }
    assert.equal(JSON.parse(fs.readFileSync(target, 'utf8')).checkpoint, 1);
    assert.equal(fs.statSync(target).mode & 0o777, 0o600);
    fs.writeFileSync(foreign, 'preserve');
    fs.rmSync(target); fs.symlinkSync(foreign, target);
    ack(folder, receipt, 'private-owner');
    assert.equal(fs.readFileSync(foreign, 'utf8'), 'preserve');
    assert.equal(fs.lstatSync(target).isSymbolicLink(), false);
    assert.deepEqual(fs.readdirSync(folder).sort(), ['foreign', 'object-window-probe-control.json']);
  } finally { fs.rmSync(folder, { recursive: true, force: true }); }
});

test('failure snapshot keeps only fixed stages and phase names, rejecting private fields', () => {
  const receipt = { checkpoint: 2, waiting: false, failed: true, complete: false,
    ownerToken: 'private-owner', records: [
      { phase: 'ready', request: 'private' },
      { phase: 'blue', workId: 'private' },
      { phase: 'failed', failureStage: 'stale-refusal', cleanupSucceeded: true,
        rawError: 'private' }] };
  const diagnostic = sanitizedProbe(receipt);
  assert.deepEqual(diagnostic.phases, [{ phase: 'ready' }, { phase: 'blue' },
    { phase: 'failed', failureStage: 'stale-refusal', cleanupSucceeded: true }]);
  assert.equal(JSON.stringify(diagnostic).includes('private'), false);
  assert.throws(() => sanitizedProbe({ ...receipt, records: [
    { phase: 'failed', failureStage: 'arbitrary', cleanupSucceeded: true }] }),
  /failure stage refused/);
});

test('literal installed PNG oracle checks white outside and blue inside the fixed object',
  { skip: process.platform !== 'darwin' }, () => {
    const width = 320, height = 180;
    const crc = bytes => {
      let value = 0xffffffff;
      for (const byte of bytes) {
        value ^= byte;
        for (let bit = 0; bit < 8; bit++) value = (value >>> 1) ^ (value & 1 ? 0xedb88320 : 0);
      }
      return (value ^ 0xffffffff) >>> 0;
    };
    const chunk = (name, data) => {
      const type = Buffer.from(name);
      const length = Buffer.alloc(4), checksum = Buffer.alloc(4);
      length.writeUInt32BE(data.length); checksum.writeUInt32BE(crc(Buffer.concat([type, data])));
      return Buffer.concat([length, type, data, checksum]);
    };
    const makePng = inverted => {
      const raw = Buffer.alloc(height * (1 + width * 4));
      for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
        const inside = x >= 16 && x < 120 && y >= 16 && y < 120;
        const blue = inverted ? !inside : inside;
        const offset = y * (1 + width * 4) + 1 + x * 4;
        raw.set(blue ? [0, 0, 255, 255] : [255, 255, 255, 255], offset);
      }
      const ihdr = Buffer.alloc(13);
      ihdr.writeUInt32BE(width); ihdr.writeUInt32BE(height, 4); ihdr[8] = 8; ihdr[9] = 6;
      return Buffer.concat([Buffer.from('89504e470d0a1a0a', 'hex'), chunk('IHDR', ihdr),
        chunk('IDAT', zlib.deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
    };
    const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-object-export-png-'));
    try {
      const destination = path.join(folder, 'published');
      const cache = path.join(folder, 'cache');
      fs.mkdirSync(destination); fs.mkdirSync(cache);
      const file = path.join(destination, 'frame_0001.png');
      const receipt = { apiVersion: 2, requestId: 'object-export-' + runId,
        status: 'succeeded', published: true, cleanupComplete: true,
        cleanupStatus: 'complete', externalEffectDisposition: 'committed',
        contentRevision: 0, artifactFiles: ['frame_0001.png'] };
      const app = { dirs: { appCache: cache } };
      fs.writeFileSync(file, makePng(false));
      assert.deepEqual(pngPixels(file), { width, height, blue: [0, 0, 255, 255],
        white: [255, 255, 255, 255] });
      assert.equal(artifactEvidence(destination, app, receipt, runId).pixels.blue.join(','), '0,0,255,255');
      fs.writeFileSync(path.join(destination, 'extra.png'), Buffer.from('extra'));
      assert.throws(() => artifactEvidence(destination, app, receipt, runId), /published manifest differs/);
      fs.rmSync(path.join(destination, 'extra.png'));
      fs.mkdirSync(path.join(cache, 'native-export-staging'));
      fs.writeFileSync(path.join(cache, 'native-export-staging', 'residue'), Buffer.from('residue'));
      assert.throws(() => artifactEvidence(destination, app, receipt, runId), /staging cleanup differs/);
      fs.rmSync(path.join(cache, 'native-export-staging'), { recursive: true });
      fs.writeFileSync(file, makePng(true));
      assert.throws(() => pngPixels(file), /literal pinned PNG pixels differ/);
    } finally { fs.rmSync(folder, { recursive: true, force: true }); }
  });

test('owned report root admits only the real macOS var/folders alias after task binding',
  { skip: process.platform !== 'darwin' }, () => {
    const rawTask = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-export-report-'));
    const canonicalTask = fs.realpathSync(rawTask);
    const aliasTask = canonicalTask.replace(/^\/private(?=\/var\/folders\/)/, '');
    const foreign = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-export-foreign-'));
    const arbitrary = fs.mkdtempSync('/tmp/nemo-export-other-alias-');
    try {
      assert.notEqual(aliasTask, canonicalTask, 'real macOS var/folders alias required');
      fs.mkdirSync(path.join(aliasTask, 'reports'));
      fs.mkdirSync(path.join(foreign, 'reports'));
      fs.mkdirSync(path.join(arbitrary, 'reports'));
      const canonicalReports = path.join(canonicalTask, 'reports');
      assert.equal(ownedReportDirectory(path.join(aliasTask, 'reports'), aliasTask), canonicalReports);
      assert.equal(ownedReportDirectory(canonicalReports, canonicalTask), canonicalReports);
      assert.throws(() => ownedReportDirectory(path.join(foreign, 'reports'), aliasTask),
        /owned report root differs from task/);
      assert.throws(() => ownedReportDirectory(path.join(arbitrary, 'reports'), arbitrary),
        /owned report alias refused/);
      fs.rmSync(canonicalReports, { recursive: true });
      fs.symlinkSync(path.join(foreign, 'reports'), path.join(aliasTask, 'reports'));
      assert.throws(() => ownedReportDirectory(path.join(aliasTask, 'reports'), aliasTask),
        /owned report path symlink refused/);
    } finally {
      fs.rmSync(rawTask, { recursive: true, force: true });
      fs.rmSync(foreign, { recursive: true, force: true });
      fs.rmSync(arbitrary, { recursive: true, force: true });
    }
  });
