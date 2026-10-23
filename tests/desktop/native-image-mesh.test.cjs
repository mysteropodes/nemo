'use strict';

// Operator-assisted installed-app acceptance. Saved native project files and
// nativeStatus are the state oracles; screenshots establish presentation only
// and remain pending visual review. Project workspace metadata is presentation-owned.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), http = require('node:http');
const { randomUUID } = require('node:crypto');
const { execFileSync } = require('node:child_process');
const ROOT = path.resolve(__dirname, '../..');
const { createController, packagedApp, hash } = require(path.join(ROOT, 'tests/desktop/native-harness.cjs'));
const { McpClient } = require(path.join(ROOT, 'tests/mcp-parity/mcp-client.cjs'));
const { createNativeApplicationAdapter } = require(path.join(ROOT, 'src/js/adapters/native-application.js'));
const runtime = require(path.join(ROOT, 'scripts/nemo/lib/native-runtime.cjs'));
const identity = require(path.join(ROOT, 'scripts/nemo/lib/identity.cjs'));
const PNG_ORACLE = 'native-engine/tests/fixtures/object-raster-asset-attach/oracle.txt';
const PNG_SHA = 'bfbc8e4b0151746424b5618cc64b0c5ede76f51b88970778223b2065eaa1efa7';
const OWNED = new Set(['tests/desktop/native-image-mesh.test.cjs']);
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
const read = file => JSON.parse(fs.readFileSync(file, 'utf8'));
const write = (file, value) => fs.writeFileSync(file, JSON.stringify(value, null, 2), { mode: 0o600 });
function structured(response, allowError = false) {
  if (response?.result?.isError && !allowError) throw new Error('bundled MCP tool returned an error');
  if (response?.result?.structuredContent) return response.result.structuredContent;
  for (const item of response?.result?.content || []) {
    if (item.type === 'text') { try { return JSON.parse(item.text); } catch { /* inspect next block */ } }
  }
  return response?.result ?? response;
}

function fixture() {
  const row = fs.readFileSync(path.join(ROOT, PNG_ORACLE), 'utf8').split('\n')
    .map(line => { try { return JSON.parse(line); } catch { return null; } })
    .find(value => value && value.name === 'blue-2x1');
  assert.ok(row && row.sha256 === PNG_SHA && row.b64, 'fixed admitted blue-2x1 raster fixture changed');
  const rasters = Array.from({ length: 24 }, (_, frame) => ({
    target: { contextId: 'scene-root', layerUid: 'L0', strokeId: 'pixels-' + frame,
      frameScope: { kind: 'authored', frame } },
    item: { assetId: 'pixels', x: 3, y: 5, width: 20, height: 10, opacity: 1 },
  }));
  const scopes = Array.from({ length: 24 }, (_, frame) => ({ frameScope: { kind: 'authored', frame },
    elements: [{ layerUid: 'L0', strokeId: 'pixels-' + frame, naturalPosition: 0, order: {} }] }));
  return { format: 'nemo.native-project-document', formatVersion: 2,
    project: { canvasW: 64, canvasH: 48, fps: 24 }, viewportReference: null,
    assets: [{ id: 'pixels', name: 'blue-2x1', kind: 'image',
      source: { kind: 'embedded', mediaType: 'image/png', encoding: 'base64', data: row.b64 } }],
    assetInventory: { assetIds: ['pixels'], placements: [] },
    drawing: { format: 'nemo.native-object-document', formatVersion: 2, totalFrames: 24,
      layers: [{ layerUid: 'L0', timing: { format: 'nemo.native-layer-timing', formatVersion: 1,
        family: 'drawing', contextId: 'scene-root', keys: Array.from({ length: 24 }, (_, frame) => ({ frame })), samples: [] } }],
      objects: [], items: { raster: rasters },
      compositionOrder: { format: 'nemo.native-object-order-plan', formatVersion: 1, documentFormatVersion: 2,
        migration: 'explicit-plan-required', referencePolicy: 'unresolved',
        contexts: [{ contextId: 'scene-root', layers: [{ layerUid: 'L0', naturalPosition: 0, order: {} }], scopes }] } } };
}

test('installed native image mesh: binding, gesture, history and project-file persistence',
  { timeout: 1800000 }, async t => {
    assert.equal(process.env.NEMO_T11_IMAGE_MESH_INTERACTIVE, '1',
      'BLOCKED: set NEMO_T11_IMAGE_MESH_INTERACTIVE=1 for operator-driven installed UI acceptance');
    const app = packagedApp(process.env.NEMO_DESKTOP_APP, runtime);
    const mcpBinary = path.join(app.bundle, 'Contents', 'MacOS', 'nemo-mcp');
    assert.ok(fs.existsSync(mcpBinary), 'packaged nemo-mcp executable is required');
    const mcpBinarySha256 = hash(mcpBinary), mcpStat = fs.statSync(mcpBinary);
    assert.ok(mcpStat.isFile() && (mcpStat.mode & 0o111), 'bundled nemo-mcp must be an executable regular file');
    const proofFile = path.join(path.dirname(app.bundle), 'build-proof.json'), proof = read(proofFile);
    const source = identity.sourceIdentity();
    assert.equal(proof.status, 'pass'); assert.equal(proof.details.source.dirty, false);
    assert.match(proof.details.source.head, /^[a-f0-9]{40}$/);
    assert.ok(proof.details.cleanup.stopped && proof.details.cleanup.released);
    assert.ok(source.head && /^[a-f0-9]{40}$/.test(source.head) && !source.dirty, 'acceptance source must be committed');
    assert.ok(execFileSync('git', ['diff', '--name-only', proof.details.source.head, source.head],
      { cwd: ROOT, encoding: 'utf8' }).trim().split('\n').filter(Boolean).every(file => OWNED.has(file)),
    'installed package differs from the candidate outside this acceptance driver');
    assert.equal(proof.artifacts.filter(item => item.sha256 === app.executableSha256 && item.path.endsWith('/Contents/MacOS/nemo')).length,
      1, 'build proof must identify exactly the supplied executable');
    assert.ok(process.env.NEMO_DESKTOP_REPORT_DIR, 'private report directory is required');

    const reportDir = path.resolve(process.env.NEMO_DESKTOP_REPORT_DIR, `t11-mesh-${Date.now()}-${process.pid}`);
    fs.mkdirSync(reportDir, { recursive: true, mode: 0o700 });
    const fixturePath = path.join(reportDir, 'blue-2x1.native.json');
    const projectDir = path.join(reportDir, 'projects');
    fs.mkdirSync(projectDir, { mode: 0o700 });
    const files = Object.fromEntries(['baseline', 'attached', 'sculpted', 'cancelled', 'undo-sculpt', 'redo-sculpt',
      'undo-binding', 'redo-binding', 'pre-reopen', 'reopened', 'detached'].map(name => [name, path.join(projectDir, name + '.json')]));
    files.mcpSculpted = path.join(projectDir, 'mcp-sculpted.json');
    write(fixturePath, fixture());
    const report = { schema: 'nemo.t11-image-mesh-installed/1', result: 'in-progress',
      sourceSha: source.head, buildSourceSha: proof.details.source.head, executableSha256: app.executableSha256,
      buildProofSha256: hash(proofFile), fixture: path.basename(fixturePath), fixtureSha256: hash(fixturePath),
      checks: [], uiEvidence: [], limitations: [
        'Public Rust MCP document serialization remains opacity-only; exact state assertions use actual Save As project files.',
        'This raster fixture does not establish scene-component reference release or deduplication.',
        'Screenshots and operator observations establish presentation/input evidence and await independent visual review.',
      ], pendingInputEvidence: ['genuine platform pointercancel or lostpointercapture for the cancellation checkpoint'] };
    const controller = createController({ root: ROOT, app: app.bundle });
    let instance, mcp, sdk, mcpInstanceId, registry, stage = 'launch';
    let serverStarted = false;
    const cleanup = { serverClosed: false, appStopped: false, launcherStopped: false, reservationsReleased: false, mcpClosed: true };
    const captureToken = randomUUID();
    const captureServer = http.createServer((req, res) => {
      const send = (code, value) => { res.writeHead(code, { 'content-type': 'application/json' }); res.end(JSON.stringify(value)); };
      if (req.method !== 'POST' || req.url !== '/capture/' + encodeURIComponent(stage) ||
          req.headers['x-nemo-capture-token'] !== captureToken) { req.resume(); send(403, { error: 'inactive checkpoint' }); return; }
      const chunks = []; let size = 0;
      req.on('data', chunk => { size += chunk.length; if (size <= 8 * 1024 * 1024) chunks.push(chunk); });
      req.on('end', () => {
        const bytes = Buffer.concat(chunks), png = bytes.subarray(0, 8).equals(Buffer.from([137,80,78,71,13,10,26,10]));
        const jpeg = bytes.length > 3 && bytes[0] === 255 && bytes[1] === 216 && bytes[2] === 255;
        if (size > 8 * 1024 * 1024) return send(413, { error: 'capture too large' });
        if (!png && !jpeg) return send(415, { error: 'expected screenshot bytes' });
        const image = stage + (png ? '.png' : '.jpg'), file = path.join(reportDir, image);
        try { fs.writeFileSync(file, bytes, { flag: 'wx', mode: 0o600 }); send(201, { checkpoint: stage, image, sha256: hash(file) }); }
        catch { send(409, { error: 'capture already exists or could not be stored' }); }
      });
    });
    captureServer.requestTimeout = 15000; captureServer.headersTimeout = 10000; captureServer.timeout = 15000;
    async function discover() {
      const result = structured(await mcp.callTool('nemo_discover', {}));
      assert.equal(result.instances?.length, 1, 'the isolated MCP registry must expose exactly one application');
      const found = result.instances[0];
      if (mcpInstanceId) assert.equal(found.instanceId, mcpInstanceId, 'MCP owner identity changed');
      mcpInstanceId ||= found.instanceId;
      const native = found.nativeHostStatus;
      assert.ok(native && native.instanceId === mcpInstanceId, 'discovery must include actual nativeStatus');
      return { apiVersion: native.apiVersion, requestId: native.requestId, instanceId: native.instanceId,
        available: native.available, documentId: native.documentId, contentRevision: native.contentRevision };
    }
    async function status() { return discover(); }
    async function presentation(pin) {
      const request = { apiVersion: 2, requestId: randomUUID(), instanceId: pin.instanceId, documentId: pin.documentId,
        operation: 'query.document.object', payload: { atRevision: pin.contentRevision, view: { kind: 'presentation' } } };
      const reply = await sdk.dispatch(request);
      assert.equal(reply.ok, true, 'public presentation bootstrap must return the current tuple');
      assert.equal(reply.contentRevision, pin.contentRevision);
      return reply.result;
    }
    function meshRead(pin, view, layerUid, page) {
      return { apiVersion: 2, requestId: randomUUID(), instanceId: pin.instanceId, documentId: pin.documentId,
        operation: 'query.document.object', payload: { atRevision: view.atRevision, documentSnapshotId: view.documentSnapshotId,
          view: { kind: 'imageMesh', contextId: view.contextId, layerUid, frame: view.frame,
            workId: view.workId, viewGeneration: view.viewGeneration, page } } };
    }
    async function until(check, label, timeout = 300000) {
      const end = Date.now() + timeout;
      do { if (t.signal.aborted) throw new Error('installed acceptance timed out or was cancelled');
        if (fs.existsSync(path.join(reportDir, 'abort.json'))) throw new Error('operator aborted acceptance');
        if (await check()) return; if (instance && instance.process.finished) throw new Error('app exited during ' + label);
        await wait(300);
      } while (Date.now() < end);
      throw new Error('checkpoint timed out: ' + label);
    }
    function phase(name, instruction, savePath) {
      stage = name; write(path.join(reportDir, 'phase.json'), { checkpoint: name, instruction, appPid: instance.snapshot.app.pid,
        executableSha256: app.executableSha256, fixturePath, savePath, reportDir,
        screenshotCapture: { url: 'http://127.0.0.1:' + captureServer.address().port + '/capture/' + encodeURIComponent(name), token: captureToken } });
      t.diagnostic('T11 Mesh checkpoint: ' + name + ' — ' + instruction);
    }
    let selectedLayerLabel = null;
    async function capture(name, instruction, savePath, expected) {
      phase(name, instruction, savePath); const file = path.join(reportDir, name + '.json');
      await until(() => fs.existsSync(file), name);
      const evidence = read(file); assert.equal(evidence.checkpoint, name);
      assert.equal(evidence.appPid, instance.snapshot.app.pid); assert.equal(evidence.executableSha256, app.executableSha256);
      assert.ok(typeof evidence.observation === 'string' && evidence.observation.trim());
      assert.ok(typeof evidence.image === 'string' && path.basename(evidence.image) === evidence.image);
      assert.equal(hash(path.join(reportDir, evidence.image)), evidence.sha256);
      if (expected) {
        const wanted = Object.assign({}, expected);
        if (wanted.selectedLayer === '$observed') {
          assert.ok(typeof evidence.visible?.selectedLayer === 'string' && evidence.visible.selectedLayer.trim(),
            'record the exact visible selected layer label');
          selectedLayerLabel ||= evidence.visible.selectedLayer;
          wanted.selectedLayer = selectedLayerLabel;
        }
        assert.deepEqual(evidence.visible, wanted, 'record the actual visible UI checkpoint');
      }
      report.uiEvidence.push(evidence);
      return evidence;
    }
    function savedDocument(file) {
      assert.ok(fs.existsSync(file), 'operator must Save As the actual native document at ' + file);
      const raw = fs.readFileSync(file); const document = JSON.parse(raw.toString('utf8'));
      assert.equal(document.format, 'nemo.native-project-document'); return document;
    }
    function stableDocument(document) {
      const copy = structuredClone(document);
      if (copy.project) delete copy.project.workspace;
      return copy;
    }
    function withoutMesh(document) {
      const copy = stableDocument(document);
      delete copy.drawing.imageMeshes;
      for (const entry of copy.drawing.items.raster || []) delete entry.item.meshId;
      return copy;
    }
    function withoutMeshOffsets(document) {
      const copy = stableDocument(document);
      for (const mesh of Object.values(copy.drawing.imageMeshes || {})) delete mesh.offsets;
      return copy;
    }
    function assertPreservedExceptMesh(before, after, label) {
      assert.deepEqual(withoutMesh(after), withoutMesh(before), label + ': only raster mesh links and mesh store may change');
    }
    function assertPreservedExceptOffsets(before, after, label) {
      assert.deepEqual(withoutMeshOffsets(after), withoutMeshOffsets(before), label + ': only mesh offsets may change');
    }
    function assertSameDocument(before, after, label) {
      assert.deepEqual(stableDocument(after), stableDocument(before), label + ': full native project must be preserved');
    }
    function rasterFrames(document) {
      const items = document.drawing.items.raster; assert.ok(Array.isArray(items));
      const selected = items.filter(entry => entry.target?.contextId === 'scene-root' && entry.target?.layerUid === 'L0');
      assert.equal(selected.length, 24, 'fixture must retain all authored raster frames');
      const frames = new Set();
      selected.forEach(entry => {
        const scope = entry.target.frameScope; assert.equal(scope.kind, 'authored');
        assert.ok(Number.isInteger(scope.frame) && scope.frame >= 0 && scope.frame < 24);
        assert.equal(entry.target.strokeId, 'pixels-' + scope.frame);
        assert.ok(!frames.has(scope.frame), 'duplicate target for frame ' + scope.frame); frames.add(scope.frame);
      });
      assert.equal(frames.size, 24, 'one unique raster target is required per authored frame');
      return selected.sort((a, b) => a.target.frameScope.frame - b.target.frameScope.frame);
    }
    function meshState(document) {
      const frames = rasterFrames(document);
      return { links: frames.map(entry => entry.item.meshId ?? null), meshes: document.drawing.imageMeshes || {} };
    }
    function assertBinding(document, id) {
      assert.ok(rasterFrames(document).every(entry => entry.item.meshId === id), 'binding must cover all 24 raster frames');
      assert.ok(document.drawing.imageMeshes && document.drawing.imageMeshes[id], 'mesh record must be present');
      assert.equal(Object.keys(document.drawing.imageMeshes).length, 1, 'one raster owns exactly one mesh record');
    }
    function equalState(file, expected, label) { assert.deepEqual(meshState(savedDocument(file)), expected, label); }
    let failure = null;
    const conservationFailures = [];
    try {
      await new Promise((resolve, reject) => { captureServer.once('error', reject); captureServer.listen(0, '127.0.0.1', resolve); });
      serverStarted = true;
      instance = await controller.start(`t11-mesh-${process.pid}-${Date.now()}`, ['desktop-input', 'gpu-reference']);
      const owned = await controller.status(instance); assert.equal(owned.code, 0); instance.snapshot = owned.value;
      assert.equal(fs.realpathSync(instance.snapshot.app.executable), app.executable);
      registry = path.join(instance.info.roots.tauriDataDir, 'mcp');
      await until(() => fs.existsSync(registry) && fs.readdirSync(registry).some(name => name.endsWith('.json')), 'app registration');
      const registrations = fs.readdirSync(registry).filter(name => name.endsWith('.json'));
      assert.equal(registrations.length, 1, 'exactly one owned application registration is expected');
      mcp = new McpClient(mcpBinary, [], { NEMO_MCP_REGISTRY: registry });
      const initialized = await mcp.initialize();
      assert.ok(initialized.result?.protocolVersion, 'bundled process must complete MCP initialization');
      const toolList = await mcp.listTools();
      assert.equal((toolList.result?.tools || []).filter(tool => tool.name === 'nemo_command').length, 1,
        'bundled process must expose exactly one command endpoint');
      const discovery = structured(await mcp.callTool('nemo_discover', {}));
      assert.equal(discovery.instances?.length, 1, 'bundled process must discover exactly the one owned app');
      mcpInstanceId = discovery.instances[0].instanceId;
      sdk = createNativeApplicationAdapter('installed-mcp', {
        dispatch: async request => structured(await mcp.callTool('nemo_command', request), true),
      });
      await capture('open-fixture', 'Open the supplied 24-frame blue-2x1 native JSON; select raster L0. Source frame 0 appears as displayed frame 1.',
        undefined, { selectedLayer: '$observed', underlyingFrame: 0, displayedFrame: 1, meshAttached: false, meshEditing: false });
      report.selectedLayerLabel = selectedLayerLabel;
      await until(async () => (await status()).available === true, 'native fixture admission');
      const opened = await status(); assert.equal(opened.available, true);
      await capture('baseline', 'Use real Save As to write the admitted fixture to the path shown in phase.json.', files.baseline,
        { selectedLayer: '$observed', displayedFrame: 1, meshAttached: false, meshEditing: false });
      const baselineStatus = await status(); assert.equal(baselineStatus.documentId, opened.documentId);
      report.checks.push({ checkpoint: 'initial-save', contentRevisionBefore: opened.contentRevision,
        contentRevisionAfter: baselineStatus.contentRevision, note: 'Save As may materialize workspace before establishing the Mesh baseline.' });
      const baselineDoc = savedDocument(files.baseline), baselineState = meshState(baselineDoc);
      assert.deepEqual(baselineState, { links: Array(24).fill(null), meshes: {} });
      assert.equal(Object.hasOwn(baselineDoc.drawing, 'imageMeshes'), false, 'baseline preserves the absent mesh store');
      const baselinePresentation = await presentation(baselineStatus);
      const initialSelection = await sdk.dispatch(meshRead(baselineStatus, baselinePresentation, 'L0', { kind: 'selection' }));
      assert.equal(initialSelection.ok, true, 'strict imageMesh selection query must use the public shared SDK');
      assert.equal(initialSelection.result.mesh, null, 'newly admitted raster begins unbound');

      await capture('attach-action', 'Enable Mesh for the selected raster and wait for the native receipt.',
        undefined, { selectedLayer: '$observed', underlyingFrame: 0, displayedFrame: 1, meshAttached: true, meshEditing: false });
      const attachedStatus = await status(); assert.equal(attachedStatus.documentId, opened.documentId);
      assert.equal(attachedStatus.contentRevision, baselineStatus.contentRevision + 1, 'attach must add exactly one native revision');
      await capture('attached-save', 'Save As the attached project to the path shown in phase.json.', files.attached);
      assert.equal((await status()).contentRevision, attachedStatus.contentRevision, 'Save As is separate from the attach revision');
      const attached = savedDocument(files.attached), attachedState = meshState(attached);
      assertPreservedExceptMesh(baselineDoc, attached, 'attach');
      const meshId = attachedState.links[0]; assert.ok(meshId); assertBinding(attached, meshId);
      const staleSelection = await sdk.dispatch(meshRead(baselineStatus, baselinePresentation, 'L0', { kind: 'selection' }));
      assert.equal(staleSelection.ok, false, 'the pre-mutation presentation tuple must become stale');
      assert.equal(staleSelection.error?.code, 'stale_revision',
        'stale bootstrap tuple must be explicitly refused');
      const attachedPresentation = await presentation(attachedStatus);
      const attachedSelection = await sdk.dispatch(meshRead(attachedStatus, attachedPresentation, 'L0', { kind: 'selection' }));
      assert.equal(attachedSelection.ok, true);
      assert.equal(attachedSelection.result.mesh.meshId, meshId, 'presentation tuple resolves the actual attached mesh');
      const outlinePage = await sdk.dispatch(meshRead(attachedStatus, attachedPresentation, 'L0',
        { kind: 'outline', meshId, cursor: 0, limit: 16 }));
      assert.equal(outlinePage.ok, true, 'strict outline page is read through production query validation');
      assert.ok(outlinePage.result.page.items.length > 0);
      report.checks.push({ checkpoint: 'attach', contentRevision: attachedStatus.contentRevision, meshId, frameBindings: 24 });

      await capture('scrub', 'Scrub source frames 0, 7 and 23 (displayed frames 1, 8 and 24); verify binding, then return to source frame 0/displayed frame 1.',
        undefined, { selectedLayer: '$observed', underlyingFrame: 0, displayedFrame: 1, meshAttached: true, meshEditing: false,
          scrubbedUnderlyingFrames: [0, 7, 23], scrubbedDisplayedFrames: [1, 8, 24] });
      const scrubbed = await status(); assert.equal(scrubbed.documentId, opened.documentId);
      assert.equal(scrubbed.contentRevision, attachedStatus.contentRevision, 'timeline scrubbing must not mutate the document');

      await capture('sculpt-action', 'Enable Edit points, drag one handle through multiple pointer moves and release once.',
        undefined, { selectedLayer: '$observed', underlyingFrame: 0, displayedFrame: 1, meshAttached: true, meshEditing: true });
      const sculptedStatus = await status(); assert.equal(sculptedStatus.contentRevision, attachedStatus.contentRevision + 1,
        'one continuous drag must commit one native revision');
      await capture('sculpt-save', 'Save As the sculpted project to the path shown in phase.json.', files.sculpted);
      assert.equal((await status()).contentRevision, sculptedStatus.contentRevision, 'Save As is separate from the sculpt revision');
      const sculptedDoc = savedDocument(files.sculpted), sculptedState = meshState(sculptedDoc); assertBinding(sculptedDoc, meshId);
      assertPreservedExceptOffsets(attached, sculptedDoc, 'sculpt');
      const preMesh = attachedState.meshes[meshId], sculptedMesh = sculptedState.meshes[meshId];
      const withoutOffsets = mesh => Object.fromEntries(Object.entries(mesh).filter(([key]) => key !== 'offsets'));
      assert.deepEqual(withoutOffsets(sculptedMesh), withoutOffsets(preMesh), 'gesture must preserve outline, topology, and density');
      assert.ok(Array.isArray(preMesh.offsets) && Array.isArray(sculptedMesh.offsets) && preMesh.offsets.length === sculptedMesh.offsets.length);
      const changedOffsets = sculptedMesh.offsets.map((p, i) => JSON.stringify(p) === JSON.stringify(preMesh.offsets[i]) ? -1 : i).filter(i => i >= 0);
      assert.equal(changedOffsets.length, 1, 'one gesture changes exactly one vertex offset');
      report.checks.push({ checkpoint: 'sculpt', contentRevision: sculptedStatus.contentRevision });

      const cancelEvidence = await capture('cancel-action', 'Start another handle drag and move it, then produce a genuine platform pointer cancellation or lost-pointer-capture. Record cancellationMethod as pointercancel or lostpointercapture; do not use Escape or inject an event.',
        undefined, { selectedLayer: '$observed', underlyingFrame: 0, displayedFrame: 1, meshAttached: true, meshEditing: true });
      assert.ok(['pointercancel', 'lostpointercapture'].includes(cancelEvidence.cancellationMethod),
        'cancellation evidence must identify a genuine pointercancel or lostpointercapture');
      report.pendingInputEvidence = report.pendingInputEvidence.filter(item => !item.startsWith('genuine platform pointercancel'));
      const cancelledStatus = await status(); assert.equal(cancelledStatus.contentRevision, sculptedStatus.contentRevision);
      await capture('cancel-save', 'Save As after the real cancelled gesture to the path shown in phase.json.', files.cancelled);
      assert.equal((await status()).contentRevision, cancelledStatus.contentRevision, 'cancel and its Save As add no revision');
      const cancelledDoc = savedDocument(files.cancelled);
      assert.deepEqual(meshState(cancelledDoc), sculptedState, 'cancel leaves every binding and mesh field unchanged');
      assertSameDocument(sculptedDoc, cancelledDoc, 'cancel');

      await capture('undo-sculpt-action', 'Use the actual app Undo once to restore the attached mesh.', undefined);
      const undoSculptStatus = await status(); assert.equal(undoSculptStatus.contentRevision, cancelledStatus.contentRevision + 1);
      await capture('undo-sculpt-save', 'Save As the undo result to the shown path.', files['undo-sculpt']);
      assert.equal((await status()).contentRevision, undoSculptStatus.contentRevision);
      equalState(files['undo-sculpt'], attachedState, 'undo restores the complete attached mesh');
      assertSameDocument(attached, savedDocument(files['undo-sculpt']), 'undo sculpt');
      await capture('redo-sculpt-action', 'Use the actual app Redo once to restore the sculpt.', undefined);
      const redoSculptStatus = await status(); assert.equal(redoSculptStatus.contentRevision, cancelledStatus.contentRevision + 2);
      await capture('redo-sculpt-save', 'Save As the redo result to the shown path.', files['redo-sculpt']);
      assert.equal((await status()).contentRevision, redoSculptStatus.contentRevision);
      equalState(files['redo-sculpt'], sculptedState, 'redo restores the complete sculpted mesh');
      assertSameDocument(sculptedDoc, savedDocument(files['redo-sculpt']), 'redo sculpt');

      const beforeBindingUndo = await status();
      await capture('undo-binding-action', 'Undo the sculpt, then undo the original Mesh attach.', undefined);
      const bindingUndoStatus = await status();
      assert.equal(bindingUndoStatus.contentRevision, beforeBindingUndo.contentRevision + 2, 'two undo commands advance exactly two revisions');
      await capture('undo-binding-save', 'Save As the detached undo result to the shown path.', files['undo-binding']);
      assert.equal((await status()).contentRevision, bindingUndoStatus.contentRevision);
      assert.deepEqual(meshState(savedDocument(files['undo-binding'])), baselineState, 'binding undo clears all frame tags and releases the mesh');
      assertSameDocument(baselineDoc, savedDocument(files['undo-binding']), 'undo binding');
      const beforeBindingRedo = await status();
      await capture('redo-binding-action', 'Redo the mesh attach, then redo the sculpt.', undefined);
      const bindingRedoStatus = await status();
      assert.equal(bindingRedoStatus.contentRevision, beforeBindingRedo.contentRevision + 2, 'two redo commands advance exactly two revisions');
      await capture('redo-binding-save', 'Save As the redone sculpted result to the shown path.', files['redo-binding']);
      assert.equal((await status()).contentRevision, bindingRedoStatus.contentRevision);
      equalState(files['redo-binding'], sculptedState, 'redo restores the binding, store and offsets');
      assertSameDocument(sculptedDoc, savedDocument(files['redo-binding']), 'redo binding');

      const preReopenStatus = await status();
      await capture('pre-reopen-save', 'Save the current sculpted native document through the UI to the shown path.', files['pre-reopen'],
        { selectedLayer: '$observed', displayedFrame: 1, meshAttached: true, meshEditing: true });
      assert.equal((await status()).contentRevision, preReopenStatus.contentRevision, 'Save As must not change content revision');
      equalState(files['pre-reopen'], sculptedState, 'saved bytes contain the exact sculpted document state');
      assertSameDocument(sculptedDoc, savedDocument(files['pre-reopen']), 'pre-reopen save');
      await capture('open-other-document', 'Open the supplied original fixture again through the real project-open UI.', undefined,
        { selectedLayer: '$observed', displayedFrame: 1, meshAttached: false, meshEditing: false });
      await until(async () => { const current = await status(); return current.available && current.documentId !== preReopenStatus.documentId; }, 'open distinct project');
      const otherStatus = await status();
      await capture('reopen-saved-mesh', 'Reopen the saved sculpted file. Select its raster, show the real Mesh overlay, and Save As to the NEW path shown in phase.json.',
        files.reopened, { selectedLayer: '$observed', displayedFrame: 1, meshAttached: true, meshEditing: true });
      await until(async () => { const current = await status(); return current.available && current.documentId !== otherStatus.documentId; }, 'reopen saved mesh project');
      const reopenedStatus = await status(); assert.notEqual(reopenedStatus.documentId, otherStatus.documentId);
      const reopenedDoc = savedDocument(files.reopened);
      equalState(files.reopened, sculptedState, 'new post-reopen Save As preserves exact binding, store and offsets');
      assertSameDocument(sculptedDoc, reopenedDoc, 'post-reopen save');
      const oldDocumentRead = await sdk.dispatch(meshRead(baselineStatus, baselinePresentation, 'L0', { kind: 'selection' }));
      assert.equal(oldDocumentRead.ok, false, 'the old document presentation must not survive project replacement');
      assert.equal(oldDocumentRead.error?.code, 'wrong_document');
      const reopenedPresentation = await presentation(reopenedStatus);
      const reopenedSelection = await sdk.dispatch(meshRead(reopenedStatus, reopenedPresentation, 'L0', { kind: 'selection' }));
      assert.equal(reopenedSelection.ok, true, 'a fresh public bootstrap recovers the current document presentation');
      assert.equal(reopenedSelection.result.mesh.meshId, meshId, 'fresh presentation reads the persisted mesh binding');
      report.checks.push({ checkpoint: 'save-reopen', documentIdChanged: true, contentRevision: reopenedStatus.contentRevision });

      const mcpBefore = await status();
      assert.equal(mcpBefore.documentId, reopenedStatus.documentId, 'MCP command must address the currently reopened project');
      const reopenedMesh = reopenedDoc.drawing.imageMeshes[meshId];
      const mcpOffset = [0.125, -0.0625];
      const mcpIndex = reopenedMesh.offsets.findIndex(point => JSON.stringify(point) !== JSON.stringify(mcpOffset));
      assert.ok(mcpIndex >= 0, 'choose a mesh vertex whose existing offset differs from the fixed MCP offset');
      const mcpRequest = { apiVersion: 2, requestId: randomUUID(), instanceId: mcpInstanceId,
        documentId: mcpBefore.documentId, expectedRevision: mcpBefore.contentRevision,
        operation: 'command.document.object.image.mesh.edit',
        payload: { command: 'object.image.mesh.edit', meshId,
          action: { kind: 'setOffset', index: mcpIndex, du: mcpOffset[0], dv: mcpOffset[1] } } };
      const nativeReply = await sdk.dispatch(mcpRequest);
      assert.equal(nativeReply.apiVersion, 2); assert.equal(nativeReply.requestId, mcpRequest.requestId);
      assert.equal(nativeReply.instanceId, mcpRequest.instanceId); assert.equal(nativeReply.documentId, mcpRequest.documentId);
      assert.equal(nativeReply.ok, true, 'native command owner must acknowledge the MCP command');
      assert.equal(nativeReply.contentRevision, mcpBefore.contentRevision + 1, 'MCP reply must report its one committed revision');
      const afterMcp = await status(); assert.equal(afterMcp.documentId, mcpBefore.documentId);
      assert.equal(afterMcp.contentRevision, mcpBefore.contentRevision + 1, 'MCP command advances one native revision');
      await capture('mcp-sculpted', 'Save As the reopened document to the new path in phase.json after the MCP acknowledgement.',
        files.mcpSculpted, { selectedLayer: '$observed', displayedFrame: 1, meshAttached: true, meshEditing: true });
      const mcpDoc = savedDocument(files.mcpSculpted), mcpState = meshState(mcpDoc); assertBinding(mcpDoc, meshId);
      const expectedMcpState = structuredClone(sculptedState);
      expectedMcpState.meshes[meshId].offsets[mcpIndex] = mcpOffset;
      assert.deepEqual(mcpState, expectedMcpState, 'MCP changes only the requested literal offset and retains all bindings');
      const expectedMcpDocument = stableDocument(reopenedDoc);
      expectedMcpDocument.drawing.imageMeshes[meshId].offsets[mcpIndex] = mcpOffset;
      assert.deepEqual(stableDocument(mcpDoc), expectedMcpDocument, 'MCP preserves the complete project except the requested mesh offset');
      assert.equal((await status()).contentRevision, afterMcp.contentRevision, 'post-MCP Save As must not add a revision');
      report.checks.push({ checkpoint: 'bundled-mcp', binarySha256: mcpBinarySha256,
        request: mcpRequest, response: nativeReply, contentRevision: afterMcp.contentRevision,
        frameBindings: 24, changedOffsetIndex: mcpIndex });

      const beforeDetach = await status();
      await capture('detach-action', 'Disable Mesh for the selected raster and wait for the native receipt.', undefined,
        { selectedLayer: '$observed', underlyingFrame: 0, displayedFrame: 1, meshAttached: false, meshEditing: false });
      const detachedStatus = await status();
      assert.equal(detachedStatus.contentRevision, beforeDetach.contentRevision + 1, 'detach must add exactly one native revision');
      await capture('detach-save', 'Save As the detached project to the shown path.', files.detached);
      assert.equal((await status()).contentRevision, detachedStatus.contentRevision);
      const detachedState = meshState(savedDocument(files.detached));
      assert.deepEqual(detachedState, baselineState, 'detach clears all 24 links and releases the unreferenced mesh');
      // A real attach/detach authors the store; releasing its last mesh preserves present-empty.
      const expectedDetachedDocument = structuredClone(baselineDoc);
      expectedDetachedDocument.drawing.imageMeshes = {};
      assertSameDocument(expectedDetachedDocument, savedDocument(files.detached), 'detach');
      report.checks.push({ checkpoint: 'detach', contentRevision: detachedStatus.contentRevision,
        limitation: 'Rust MCP serializer remains unavailable for Mesh; project-file persistence was verified through the actual UI Save As consumer.' });
      const mcpExit = await mcp.close(); mcp = null;
      assert.equal(mcpExit.code, 0, 'bundled MCP process must close cleanly');
      report.result = 'protocol-pass-pending-visual-review';
    } catch (error) {
      failure = error; report.result = 'fail'; report.error = error.message;
    } finally {
      if (mcp) {
        try { const exit = await mcp.close(); cleanup.mcpClosed = exit.code === 0; }
        catch (error) { cleanup.mcpClosed = false; cleanup.mcpError = error.message; }
      }
      if (serverStarted) {
        await new Promise(resolve => captureServer.close(() => resolve()));
        cleanup.serverClosed = !captureServer.listening;
      }
      try {
        await controller.cleanup();
        cleanup.appStopped = !instance || instance.stopped === true;
        cleanup.launcherStopped = !instance || instance.process.finished === true;
        cleanup.reservationsReleased = cleanup.appStopped && cleanup.launcherStopped;
      } catch (error) { cleanup.error = error.message; }
      report.cleanup = cleanup;
      if (!cleanup.serverClosed || !cleanup.appStopped || !cleanup.launcherStopped || !cleanup.reservationsReleased || !cleanup.mcpClosed) {
        report.result = 'fail'; report.error ||= 'owned app, launcher, capture server or reservations did not clean up';
      } else if (report.result === 'in-progress') report.result = 'protocol-pass-pending-visual-review';
      let finalSource;
      try { finalSource = identity.sourceIdentity();
        report.finalSourceSha = finalSource.head; report.sourceCleanAtEnd = finalSource.dirty === false;
      } catch (error) { conservationFailures.push('could not read final source identity: ' + error.message); }
      try { report.executableSha256AtEnd = hash(app.executable); }
      catch (error) { conservationFailures.push('could not hash installed executable: ' + error.message); }
      try { report.mcpBinarySha256AtEnd = hash(mcpBinary); }
      catch (error) { conservationFailures.push('could not hash bundled MCP executable: ' + error.message); }
      try { report.buildProofSha256AtEnd = hash(proofFile); }
      catch (error) { conservationFailures.push('could not hash build proof: ' + error.message); }
      if (finalSource && finalSource.head !== source.head) conservationFailures.push('source identity changed during installed acceptance');
      if (finalSource && finalSource.dirty !== false) conservationFailures.push('source became dirty during installed acceptance');
      if (report.executableSha256AtEnd !== app.executableSha256) conservationFailures.push('installed executable changed during acceptance');
      if (report.mcpBinarySha256AtEnd !== mcpBinarySha256) conservationFailures.push('bundled MCP executable changed during acceptance');
      if (report.buildProofSha256AtEnd !== report.buildProofSha256) conservationFailures.push('build proof changed during acceptance');
      if (conservationFailures.length) {
        report.conservationFailures = conservationFailures;
        report.result = 'fail'; report.error ||= conservationFailures.join('; ');
      }
      write(path.join(reportDir, 'result.json'), report);
    }
    if (failure) throw failure;
    if (report.result === 'fail') throw new Error(report.error || 'installed acceptance failed');
  });
