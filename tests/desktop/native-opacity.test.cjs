'use strict';

// Operator-assisted installed acceptance. A human or native computer-use agent
// drives the *real* window named by phase.json. No DOM injection, fake Tauri host,
// replayed success receipt or file of pass/fail flags can advance protocol checks.
// Run with NEMO_N21_INTERACTIVE=1, NEMO_DESKTOP_APP and NEMO_DESKTOP_REPORT_DIR.
// The sibling build-proof.json must identify the executable. UI capture records
// are separate manual-review evidence; they do not prove GPU pixel correctness.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const net = require('node:net');
const { execFileSync } = require('node:child_process');
const { randomUUID } = require('node:crypto');
const { createController, packagedApp, hash, alive } = require('./native-harness.cjs');
const runtime = require('../../scripts/nemo/lib/native-runtime.cjs');
const identity = require('../../scripts/nemo/lib/identity.cjs');
const { decode } = require('../fixtures/lib/png.cjs');

const ROOT = path.resolve(__dirname, '../..');
const FIXTURE = path.join(ROOT, 'tests/animation/fixtures/curve-workflow.json');
const FIXTURE_SHA = 'dceb05d13576a4dda0eb1a1a9d8c0184e8617e9a3a2662150ee54f4badedf08d';
const OWNED = new Set(['tests/desktop/native-opacity.test.cjs', 'tests/browser/native-opacity-availability.spec.cjs']);
const read = file => JSON.parse(fs.readFileSync(file, 'utf8'));
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const write = (file, value) => fs.writeFileSync(file, JSON.stringify(value, null, 2), { mode: 0o600 });

// Authentication stays inside the owned app's registry and this private call.
function wire(endpoint, member, request) {
  return new Promise((resolve, reject) => {
    let bytes = '', settled = false;
    const socket = net.connect(endpoint.port, '127.0.0.1');
    const finish = (error, response) => {
      if (settled) return;
      settled = true; socket.destroy();
      if (error) reject(error); else resolve(response);
    };
    socket.setTimeout(15000, () => finish(new Error('installed command transport timed out')));
    socket.on('error', () => finish(new Error('installed command transport unavailable')));
    socket.on('close', () => { if (!settled) finish(new Error('installed transport closed without a response')); });
    socket.on('connect', () => socket.write(JSON.stringify({ secret: endpoint.secret, [member]: request }) + '\n'));
    socket.on('data', chunk => {
      bytes += chunk;
      if (bytes.length > 65536) return finish(new Error('installed response exceeded bound'));
      const end = bytes.indexOf('\n');
      if (end < 0) return;
      try {
        const value = JSON.parse(bytes.slice(0, end));
        assert.equal(value.requestId, request.requestId);
        assert.equal(value.instanceId, endpoint.instanceId);
        assert.equal(value.apiVersion, request.apiVersion);
        finish(null, value);
      } catch { finish(new Error('installed response identity or framing is invalid')); }
    });
  });
}
function pngOracle(file, opacity, left) {
  const image = decode(fs.readFileSync(file));
  assert.deepEqual([image.width, image.height], [320, 180]);
  let count = 0;
  const bounds = [320, 180, -1, -1];
  for (let y = 0; y < 180; y++) for (let x = 0; x < 320; x++) {
    const offset = (y * 320 + x) * 4;
    const [r, g, b, a] = image.rgba.subarray(offset, offset + 4);
    if (r === 255 && g === 255 && b === 255 && a === 255) continue;
    assert.deepEqual([r, a], [255, 255]);
    assert.ok(Math.abs(g - 255 * (1 - opacity / 100)) <= 1);
    assert.equal(b, g);
    count++; bounds[0] = Math.min(bounds[0], x); bounds[1] = Math.min(bounds[1], y);
    bounds[2] = Math.max(bounds[2], x + 1); bounds[3] = Math.max(bounds[3], y + 1);
  }
  assert.equal(count, 400);
  assert.deepEqual(bounds, [left, 60, left + 20, 80]);
  return { opacity, count, bounds, sha256: hash(file) };
}

test('installed native opacity: live command oracles and externally driven UI checkpoints', { timeout: 1800000 }, async t => {
  assert.equal(process.env.NEMO_N21_INTERACTIVE, '1',
    'BLOCKED: N21 requires an operator driving the installed UI; set NEMO_N21_INTERACTIVE=1');
  const app = packagedApp(process.env.NEMO_DESKTOP_APP, runtime);
  const proofFile = path.join(path.dirname(app.bundle), 'build-proof.json');
  const proof = read(proofFile), source = identity.sourceIdentity();
  assert.equal(proof.status, 'pass'); assert.equal(proof.details.source.dirty, false);
  assert.ok(source.head && !source.dirty, 'Commit the acceptance candidate before runtime validation');
  const buildSource = proof.details.source.head;
  assert.match(buildSource, /^[a-f0-9]{40}$/);
  const differences = execFileSync('git', ['diff', '--name-only', buildSource, source.head],
    { cwd: ROOT, encoding: 'utf8' }).trim().split('\n').filter(Boolean);
  assert.ok(differences.every(file => OWNED.has(file)), 'App proof and acceptance candidate differ in product bytes');
  const executableProof = proof.artifacts.find(item => item.sha256 === app.executableSha256
    && item.path.endsWith('/Contents/MacOS/nemo'));
  assert.ok(executableProof, 'Installed executable is not the successful build artifact');
  assert.ok(proof.details.cleanup.stopped && proof.details.cleanup.released);
  assert.equal(hash(FIXTURE), FIXTURE_SHA);
  assert.ok(process.env.NEMO_DESKTOP_REPORT_DIR, 'A private desktop report directory is required');
  const reportDir = path.resolve(process.env.NEMO_DESKTOP_REPORT_DIR, `n21-${Date.now()}-${process.pid}`);
  fs.mkdirSync(reportDir, { recursive: true, mode: 0o700 });
  const fixtures = { static: path.join(reportDir, 'static.json'), keyed: path.join(reportDir, 'keyed.json'),
    unsupported: path.join(reportDir, 'unsupported.json'), saved: path.join(reportDir, 'saved.json'),
    export: path.join(reportDir, 'export') };
  const shell = read(FIXTURE); shell.layers[0].motionStatic = { opacity: [25] };
  write(fixtures.static, shell);
  shell.layers[0].motion.opacity = read(path.join(ROOT, 'native-engine/tests/fixtures/opacity-v2/project.json')).layers[0].motion.opacity;
  write(fixtures.keyed, shell); fs.copyFileSync(FIXTURE, fixtures.unsupported); fs.mkdirSync(fixtures.export);
  const report = { schema: 'nemo.n21-desktop/1', result: 'in-progress', sourceSha: source.head,
    buildSourceSha: buildSource, executableSha256: app.executableSha256, buildProofSha256: hash(proofFile),
    fixtureSha256: FIXTURE_SHA, checks: [], uiEvidence: [],
    limitations: ['Visual preview and input captures require independent manual review.',
      'A value and history check cannot establish that a continuous pointer scrub occurred; review the gesture evidence separately.',
      'Resource-loss control terminates the owned app host; it does not inject a hardware GPU device fault.'] };
  const controller = createController({ root: ROOT, app: app.bundle });
  let instance, endpoint, stage = 'launch', sequence = 0;
  async function waitFor(check, label, timeout = 180000) {
    const deadline = Date.now() + timeout;
    do {
      if (fs.existsSync(path.join(reportDir, 'abort.json'))) throw new Error('Operator stopped runtime acceptance: ' + read(path.join(reportDir, 'abort.json')).reason);
      if (await check()) return;
      if (instance && instance.process.finished) throw new Error('Installed launcher exited during ' + label);
      await delay(300);
    } while (Date.now() < deadline);
    throw new Error('Runtime checkpoint not reached: ' + label);
  }
  function phase(name, instruction) {
    stage = name;
    write(path.join(reportDir, 'phase.json'), { checkpoint: name, instruction, appPid: instance.snapshot.app.pid,
      executableSha256: app.executableSha256, fixtures, reportDir });
    t.diagnostic('N21 UI checkpoint: ' + name + ' — ' + instruction);
  }
  function status() { return wire(endpoint, 'nativeStatus', { apiVersion: 2,
    requestId: `n21-status-${++sequence}`, instanceId: endpoint.instanceId }); }
  async function dispatch(operation, payload = {}, extra = {}) {
    const current = await status();
    if (current.available !== true) {
      const snapshotRequest = { apiVersion: 1, requestId: randomUUID(),
        instanceId: endpoint.instanceId, operation: 'snapshot', payload: {} };
      const loss = { checkpoint: stage, operation, nativeStatus: current, snapshotRequest };
      const file = path.join(reportDir, 'native-loss.json');
      write(file, loss);
      // One diagnostic read of the actual current owner, without the retired
      // document selector. Neither a legacy reply nor transport failure recovers
      // native availability or changes the original assertion below.
      try { loss.snapshotResponse = await wire(endpoint, 'request', snapshotRequest); }
      catch (error) { loss.snapshotError = error.message; }
      write(file, loss);
    }
    assert.equal(current.available, true, 'Native document is not available for ' + operation);
    const request = { apiVersion: 2, requestId: randomUUID(), instanceId: current.instanceId,
      documentId: current.documentId, operation, payload, ...extra };
    if (['command.document.apply', 'history.undo', 'history.redo', 'transaction.begin'].includes(operation)
      && !Object.hasOwn(extra, 'expectedRevision')) request.expectedRevision = current.contentRevision;
    return wire(endpoint, 'nativeRequest', request);
  }
  const target = { layerUid: 'r08_curve_layer' };
  async function opacity() {
    const response = await dispatch('query.document.opacity', { stableTarget: target });
    assert.equal(response.ok, true); return response.result.value;
  }
  // This is compatibility API parity, not a read of the visible Motion control.
  async function compatibilityValue(waitForAdmission = false) {
    const current = await status();
    let response, attempts = 0;
    try { await waitFor(async () => {
      attempts++;
      const request = { apiVersion: 1, requestId: randomUUID(),
        instanceId: current.instanceId, documentId: current.documentId, operation: 'property.get',
        payload: { layerId: target.layerUid, property: 'opacity' } };
      response = await wire(endpoint, 'request', request);
      // Preserve every admission response, including temporary rejections, without
      // the registry secret. A native-query result never substitutes for this read.
      const observed = { checkpoint: stage, nativeStatus: current, request, response };
      write(path.join(reportDir, 'last-ui-read.json'), observed);
      fs.appendFileSync(path.join(reportDir, 'ui-reads.jsonl'), JSON.stringify(observed) + '\n', { mode: 0o600 });
      // Rust publishes its owner before the UI finishes installing its consumers.
      // Only document admission may wait for that exact guard, and only boundedly.
      if (waitForAdmission && response.ok === false && response.error?.code === 'unavailable'
        && response.error.message === 'Native opacity ownership is not dispatchable.') return false;
      assert.equal(response.ok, true, 'Live v1 property.get failed: ' +
        (response.error && response.error.code || 'missing error code') + '; see private last-ui-read.json');
      return true;
    }, 'same-document UI admission', 10000); }
    catch (error) {
      if (!waitForAdmission || error.message !== 'Runtime checkpoint not reached: same-document UI admission') throw error;
      let finalStatus;
      try { finalStatus = await status(); } catch (statusError) { finalStatus = { error: statusError.message }; }
      write(path.join(reportDir, 'admission-failure.json'), {
        checkpoint: stage, attempts, initialStatus: current, finalStatus, lastResponse: response });
      throw new Error('Native UI admission did not complete after ' + attempts +
        ' property reads; last response ' + (response?.error?.code || 'missing error code'));
    }
    assert.equal(response.documentId, current.documentId, 'Live UI read belongs to a different document');
    assert.equal(response.revision, current.contentRevision, 'Live UI read belongs to a different revision');
    return response.result.value;
  }
  async function valueCheckpoint(name, instruction, value) {
    phase(name, instruction); await waitFor(async () => await opacity() === value, name);
    assert.equal(await compatibilityValue(), value);
    await capture(name, 'Show the actual Motion Opacity control at ' + value + '% after the requested UI action. Record visibleOpacity from that control.');
    assert.equal(read(path.join(reportDir, name + '.json')).visibleOpacity, value,
      'The visible Motion control must match the native value');
    report.checks.push({ checkpoint: name, value, evidence: name + '.json' });
  }
  async function capture(name, instruction) {
    phase(name, instruction + ' Save the real screenshot and ' + name + '.json in the report directory.');
    const file = path.join(reportDir, name + '.json');
    await waitFor(() => fs.existsSync(file), name);
    const evidence = read(file);
    assert.equal(evidence.checkpoint, name); assert.equal(evidence.appPid, instance.snapshot.app.pid);
    assert.equal(evidence.executableSha256, app.executableSha256);
    assert.ok(typeof evidence.observation === 'string' && evidence.observation.trim().length > 0);
    assert.ok(typeof evidence.image === 'string' && path.basename(evidence.image) === evidence.image);
    assert.equal(hash(path.join(reportDir, evidence.image)), evidence.sha256);
    report.uiEvidence.push(evidence);
  }
  try {
    instance = await controller.start(`n21-opacity-${process.pid}-${Date.now()}`, ['desktop-input', 'gpu-reference']);
    const owned = await controller.status(instance); assert.equal(owned.code, 0); instance.snapshot = owned.value;
    assert.equal(fs.realpathSync(instance.snapshot.app.executable), app.executable);
    report.appPid = instance.snapshot.app.pid;
    const registry = path.join(instance.info.roots.tauriDataDir, 'mcp');
    await waitFor(() => fs.existsSync(registry) && fs.readdirSync(registry).filter(file => file.endsWith('.json')).length === 1, 'owned MCP registration', 30000);
    endpoint = read(path.join(registry, fs.readdirSync(registry).find(file => file.endsWith('.json'))));
    assert.equal((await status()).available, false);
    phase('admit-static', 'Open static.json from the start screen. Wait for the Motion preparation checkpoint before editing.');
    await waitFor(async () => (await status()).available, stage);
    assert.equal(await opacity(), 25); assert.equal(await compatibilityValue(true), 25);
    report.checks.push({ checkpoint: stage, value: 25 });
    const admittedDocument = await status();
    await capture('motion-ready', 'Close the automatic tutorial and any introductory overlays, enter Motion, and select R08 rectangle. Capture the actual Motion surface and record visible: {canvasW, canvasH, fps, totalFrames, layerName, selectedLayer, opacity} from its controls before editing.');
    const visible = read(path.join(reportDir, 'motion-ready.json')).visible;
    assert.deepEqual(visible, { canvasW: 320, canvasH: 180, fps: 24, totalFrames: 21,
      layerName: 'R08 rectangle', selectedLayer: 'R08 rectangle', opacity: 25 },
    'The visible Motion/editor controls must project the opened native fixture');
    const projectedDocument = await status();
    assert.deepEqual([projectedDocument.instanceId, projectedDocument.documentId, projectedDocument.contentRevision],
      [admittedDocument.instanceId, admittedDocument.documentId, admittedDocument.contentRevision],
      'The visible Motion projection must still belong to the admitted native document');
    assert.equal(await opacity(), 25); assert.equal(await compatibilityValue(), 25);
    await valueCheckpoint('edit-40', 'Set the Motion layer Opacity field to 40 and commit with Tab.', 40);
    await valueCheckpoint('edit-60', 'Set the same Opacity field to 60 and commit with Tab.', 60);
    await valueCheckpoint('undo-40', 'Use the actual UI undo command.', 40);
    await valueCheckpoint('redo-60', 'Use the actual UI redo command.', 60);
    await valueCheckpoint('gesture-40', 'Drag the Motion Opacity scrub field from 60 to 40 (one continuous gesture).', 40);
    assert.equal((await dispatch('history.undo')).ok, true); assert.equal(await opacity(), 60);
    assert.equal((await dispatch('history.redo')).ok, true); assert.equal(await opacity(), 40);
    report.checks.push({ checkpoint: 'post-40-one-history-entry', value: 40,
      limitation: 'History alone does not prove the input was a continuous pointer gesture.' });
    assert.equal((await dispatch('command.document.apply', { command: 'layer.opacity.set', stableTarget: target, value: 25 })).ok, true);
    assert.equal(await compatibilityValue(), 25);
    await capture('mcp-ui-parity', 'Verify the visible Motion Opacity field changed to 25 after the external MCP write.');
    const unchanged = await status();
    const mutation = { command: 'layer.opacity.set', stableTarget: target, value: 99 };
    const stale = await dispatch('command.document.apply', mutation, { expectedRevision: unchanged.contentRevision - 1 });
    assert.equal(stale.ok, false); assert.equal(stale.error.code, 'stale_revision');
    const cancelled = await dispatch('command.document.apply', mutation, { cancelledBeforeDispatch: true });
    assert.equal(cancelled.ok, false); assert.equal(cancelled.error.code, 'cancelled_before_dispatch');
    assert.equal((await status()).contentRevision, unchanged.contentRevision); assert.equal(await opacity(), 25);
    const transaction = await dispatch('transaction.begin', { stableTarget: target }); assert.equal(transaction.ok, true);
    const transactionId = transaction.result.transactionId;
    assert.equal((await dispatch('transaction.update', { transactionId, value: 99 })).ok, true);
    const cancellation = await dispatch('transaction.cancel', { transactionId }); assert.equal(cancellation.ok, true);
    assert.equal(cancellation.result.terminalDisposition, 'cancelled'); assert.equal(cancellation.result.historyEntriesAdded, 0);
    assert.equal(await opacity(), 25); assert.equal((await status()).contentRevision, unchanged.contentRevision);
    const unsupportedAction = await dispatch('command.document.apply', {
      command: 'layer.unsupported.set', stableTarget: target, value: 99 });
    assert.equal(unsupportedAction.ok, false);
    assert.equal(await opacity(), 25); assert.equal((await status()).contentRevision, unchanged.contentRevision);
    report.checks.push({ checkpoint: 'stale-cancel-unsupported-preserve-document', value: 25 });
    const prior = await status();
    phase('save', 'Use Save As to save the project to the saved.json path in phase.json.');
    await waitFor(() => fs.existsSync(fixtures.saved), stage);
    const saved = read(fixtures.saved); assert.equal(saved.layers[0].layerUid, target.layerUid);
    assert.deepEqual(saved.layers[0].motionStatic.opacity, [25]);
    phase('reopen', 'Open the saved.json file through the real project-open UI.');
    await waitFor(async () => { const s = await status(); return s.available && s.documentId !== prior.documentId; }, stage);
    assert.equal(await opacity(), 25); assert.equal(await compatibilityValue(true), 25);
    const replaced = await dispatch('command.document.apply', mutation, { documentId: prior.documentId, expectedRevision: prior.contentRevision });
    assert.equal(replaced.ok, false); assert.equal(replaced.error.code, 'wrong_document'); assert.equal(await opacity(), 25);
    report.checks.push({ checkpoint: 'save-reopen-old-document-rejected', savedSha256: hash(fixtures.saved) });
    const staticDocument = await status();
    phase('open-keyed', 'Open keyed.json, enter Motion and select R08 rectangle.');
    await waitFor(async () => { const s = await status(); return s.available && s.documentId !== staticDocument.documentId; }, stage);
    assert.equal(await compatibilityValue(true), 20);
    const pinned = await status();
    const beforeExport = await dispatch('query.document.serialize', { atRevision: pinned.contentRevision });
    assert.equal(beforeExport.ok, true);
    for (const [frame, value] of [[0, 20], [10, 50], [20, 80]]) {
      const evaluated = await dispatch('query.document.evaluate', { atRevision: pinned.contentRevision, contextId: 'scene-root', frame });
      assert.equal(evaluated.ok, true); assert.equal(evaluated.result.layers[0].value, value);
      await capture('preview-' + frame, 'Scrub to zero-based frame ' + frame + ' (displayed frame ' + (frame + 1) + '); observe the red rectangle and capture its preview.');
    }
    await capture('resize-input', 'Resize the actual app window, scrub between frames 0 and 20 and back to 0; record whether preview and controls stay responsive.');
    phase('png-export', 'Export PNG sequence, full 21-frame range, scale 1, opaque background, into the export path in phase.json.');
    const pngs = () => fs.readdirSync(fixtures.export, { recursive: true }).filter(file => /\.png$/i.test(file)).sort();
    await waitFor(() => pngs().length === 21, stage);
    const files = pngs(); assert.equal(files.length, 21);
    report.pngs = [[0, 20, 20], [10, 50, 84], [20, 80, 148]].map(([frame, value, left]) => ({
      frame, ...pngOracle(path.join(fixtures.export, files[frame]), value, left) }));
    const afterExport = await dispatch('query.document.serialize', { atRevision: pinned.contentRevision });
    assert.equal(afterExport.ok, true); assert.deepEqual(afterExport.result, beforeExport.result);
    assert.equal((await status()).contentRevision, pinned.contentRevision);
    report.checks.push({ checkpoint: 'pinned-png-export-preserves-document', revision: pinned.contentRevision });
    await capture('unsupported-denied', 'Attempt to open unsupported.json through the real project-open UI. Capture the visible refusal; do not reopen another document. The existing native document must stay selected and unchanged.');
    const afterDeniedOpen = await status();
    assert.equal(afterDeniedOpen.available, true, 'Unsupported content must be denied before native ownership changes');
    assert.equal(afterDeniedOpen.instanceId, pinned.instanceId);
    assert.equal(afterDeniedOpen.documentId, pinned.documentId);
    assert.equal(afterDeniedOpen.contentRevision, pinned.contentRevision);
    const afterDeniedSerialize = await dispatch('query.document.serialize', { atRevision: pinned.contentRevision });
    assert.equal(afterDeniedSerialize.ok, true);
    assert.deepEqual(afterDeniedSerialize.result, beforeExport.result);
    assert.equal(await compatibilityValue(), 20);
    report.checks.push({ checkpoint: 'unsupported-open-denied-before-mutation',
      documentId: pinned.documentId, revision: pinned.contentRevision });
    stage = 'host-resource-loss';
    // Terminate only the verified child of this owned launcher. This exercises
    // transport/host loss, not a synthetic successful GPU-device-loss callback.
    process.kill(instance.snapshot.app.pid, 'SIGTERM');
    await waitFor(() => !alive(instance.snapshot.app.pid), stage, 15000);
    await assert.rejects(status(), /transport/);
    report.checks.push({ checkpoint: stage, disposition: 'transport unavailable after owned host exit' });
    report.result = 'protocol-pass-pending-visual-review';
  } catch (error) {
    report.result = 'fail'; report.failedCheckpoint = stage; report.reason = error.message;
    throw error;
  } finally {
    try {
      await controller.cleanup(); report.cleanup = 'complete';
      assert.equal(hash(app.executable), app.executableSha256);
      assert.equal(identity.sourceIdentity().head, source.head);
      assert.equal(identity.sourceIdentity().dirty, false);
    } catch (error) { report.result = 'fail'; report.cleanup = 'incomplete'; throw error; }
    finally { write(path.join(reportDir, 'result.json'), report); }
  }
});
