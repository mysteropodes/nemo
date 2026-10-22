'use strict';
// Proposed NEW tests/native-camera-held-first.test.cjs; absent at source base
// a6db565968903766aaa2bfc5dc11a85e121cb15f. UNAPPLIED and UNRUN.
// JS composition against fixed transport replies, never installed/native proof.
// Reuses the existing composed fixture without registering/changing its 3 tests.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const { createRequire } = require('node:module');
const { extractFunction } = require('./fixtures/lib/sandbox.cjs');
const Application = require('../src/js/adapters/native-application.js');
const turn = () => new Promise(setImmediate);
const plain = value => JSON.parse(JSON.stringify(value));
const read = file => fs.readFileSync(path.join(__dirname, '../src/js', file), 'utf8');
function gate() { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { promise, resolve }; }

function composedHarness() {
  const file = path.join(__dirname, 'native-camera-navigation-composed.test.cjs');
  let source = fs.readFileSync(file, 'utf8');
  const boundary = "\ntest('composed native Camera initialization";
  assert.equal(source.split(boundary).length, 2, 'known fixture/test boundary');
  source = source.slice(0, source.indexOf(boundary));
  // These exact adaptations affect only the reused fake DOM/transport fixture.
  // Every production module is loaded unchanged through its ordinary API.
  function replace(before, after) {
    assert.equal(source.split(before).length, 2, 'unique existing fixture seam: ' + before);
    source = source.replace(before, after);
  }
  replace("'btn-nf', 'tl-cf'", "'btn-nf', 'btn-ff', 'zoom-scrub', 'tl-cf'");
  replace('function fixture(previewFixtures = PREVIEWS)', 'function fixture(previewFixtures = PREVIEWS, timingFixtures = [])');
  replace('currentFrame: 0, totalFrames: 21, playing: false,', 'currentFrame: 0, totalFrames: 21, playing: false, waIn: 0,');
  replace('state, document, devicePixelRatio: 2,', 'state, document, devicePixelRatio: 2, NemoNativeApplicationAdapter: application,');
  replace('createNativeTauriTransport() {', 'createNativeTauriTransport(call) {');
  replace("dispatchObject: forbidden('subscription mutation')", "dispatch: request => call('nemo_native_dispatch', { request }), dispatchObject: forbidden('subscription mutation')");
  replace("    assert.equal(command, 'nemo_native_object_preview', 'no native writer/query fallback');", `
    if (command === 'nemo_native_dispatch') {
      assert.equal(request.operation, 'query.document.object.layer.timing', 'only explicit timing reads');
      const expected = timingFixtures.shift();
      assert.ok(expected, 'no additional application queries');
      assert.deepEqual(plain(request.payload), expected.selector);
      return { apiVersion: 2, requestId: request.requestId,
        instanceId: 'composed-camera-instance', documentId: 'composed-camera-document',
        contentRevision: 0, ok: true, result: expected.result };
    }
    assert.equal(command, 'nemo_native_object_preview', 'no native writer/query fallback');`);
  replace("    assert.equal(Object.hasOwn(request, 'navigation'), false);", `
    assert.deepEqual(plain(request.navigation || { zoom: 1, pan: [0, 0] }),
      expected.navigation || { zoom: 1, pan: [0, 0] }, 'literal requested navigation');
    if (expected.entered) expected.entered.resolve();
    if (expected.release) await expected.release.promise;`);
  const module = { exports: {} };
  // Compile the test fixture prefix in this realm so strict plain-object
  // protocol validators keep the same boundaries as the original test file.
  Function('module', 'require', '__dirname', 'application', source +
    '\nmodule.exports = {fixture, installOrdinaryOpen, installActualCutoverRouter, PREVIEWS, CANVAS, LOCKED_FOUR};')
    (module, createRequire(file), __dirname, Application);
  return module.exports;
}
const H = composedHarness();

// Fixed transport projections. Inverse arithmetic uses these literals only;
// neither mode nor frame/navigation oracles are derived from outgoing requests.
function fixed(forward) {
  const [a, b, c, d, e, f] = forward, determinant = a * d - b * c;
  return { mode: 'stored-camera', cameraAvailable: true, navigation: { zoom: 1.1, pan: [0, 0] },
    documentToComposition: forward,
    compositionToDocument: [d / determinant, -b / determinant, -c / determinant, a / determinant,
      (c * f - d * e) / determinant, (b * e - a * f) / determinant] };
}
const HELD_FOUR = fixed([2.2, 0, 0, 2.2, -192, -108]);
const HELD_ZERO = fixed([1.1, 0, 0, 1.1, -16, -9]);
const held = (frame, view, extra = {}) => ({ frame, view, navigation: { zoom: 1.1, pan: [0, 0] }, ...extra });
function timing(frame) {
  const selector = { atRevision: 0, documentSnapshotId: 'composed-camera-snapshot', contextId: 'scene-root', layerUid: 'drawing', frame };
  return { selector, result: { instanceId: 'composed-camera-instance', documentId: 'composed-camera-document', ...selector,
    cellKind: frame === 0 ? 'key' : 'held', sourceFrame: 0, hasContent: true, manualEdit: false } };
}
function installInputs(f) {
  const { root, context, document } = f;
  // Load the actual browser controller as well: its existing passive observer
  // supplies completion waits without wrapping or replacing the actual entry.
  vm.runInContext(read('adapters/native-opacity-motion-surface.js'), context);
  let navigationWait = null;
  root.NemoTestPlayObserver = {
    navigationRequest: (intent, frame) => ({ intent, frame }),
    navigationReturn(record, value) {
      if (navigationWait && record && record.frame === navigationWait.frame) {
        const pending = navigationWait; navigationWait = null; pending.resolve(value);
      }
    }
  };
  assert.equal(f.nodes['btn-camera'].count('click'), 1, 'ordinary Open retained its single Camera listener');
  H.installActualCutoverRouter(root, f.session);
  const timeline = read('timeline.js');
  vm.runInContext('var _nativeFrameNavigator = null;\n' + extractFunction(read('app.js'), 'goToFrame') + '\n' +
    extractFunction(read('tools.js'), 'nativeViewportNavigation') + '\n' + extractFunction(timeline, 'onKeyDown'),
  context, { filename: 'production Camera navigation entries' });
  for (const id of ['btn-ff', 'btn-nf']) {
    const lines = timeline.split('\n').filter(line => line.startsWith("document.getElementById('" + id + "').addEventListener('click',"));
    assert.equal(lines.length, 1, id + ' exact existing additive listener');
    vm.runInContext(lines[0], context);
    assert.equal(f.nodes[id].count('click'), 1);
  }
  const keyRegistration = "document.addEventListener('keydown',onKeyDown);";
  assert.equal(timeline.split(keyRegistration).length, 2);
  vm.runInContext(keyRegistration, context);
  assert.equal(document.count('keydown'), 1);
  return {
    navigate(id, frame) {
      assert.equal(navigationWait, null); const pending = gate(); navigationWait = { ...pending, frame };
      f.nodes[id].dispatch('click'); return pending.promise;
    },
    keyboardZoom() {
      let prevented = 0;
      const event = { key: '=', code: 'Equal', metaKey: true, ctrlKey: false, altKey: true, shiftKey: false,
        target: { tagName: 'BODY', isContentEditable: false }, preventDefault() { prevented++; } };
      // Preserve every distinct additive listener; no singleton-handler stand-in.
      for (const { listener } of document.listeners.get('keydown')) listener.call(document, event);
      assert.equal(prevented, 1);
    }
  };
}
async function opened(rows, timings = []) {
  const f = H.fixture(rows, timings);
  // Camera initialization precedes ordinary Open as in the existing composed case.
  // Cutover publication requires the active owner, so install it after Open.
  vm.runInContext(read('camera.js'), f.context);
  f.document.readyState = 'interactive'; f.document.dispatch('DOMContentLoaded');
  const open = H.installOrdinaryOpen(f, f.context), painted = f.waitForPolicyPaint();
  assert.equal(await f.root.SMProject.openPath('/Camera-view.json'), true); await painted;
  assert.equal(f.nodes['btn-camera'].count('click'), 1);
  return { f, open };
}
function assertOutput(f, frame, expectedView) {
  const shown = f.session.readPresentedObjectSession(); assert.ok(shown, 'public confirmed presentation');
  assert.equal(shown.presentation.frame, frame);
  assert.equal(shown.presentation.view.mode, 'stored-camera');
  assert.deepEqual(plain(shown.presentation.view.navigation), { zoom: 1.1, pan: [0, 0] });
  assert.deepEqual(plain(shown.presentation.view.documentToComposition), expectedView.documentToComposition);
  assert.equal(f.state.currentFrame, frame); assert.equal(f.root._curFrame, frame);
  assert.equal(Number(f.nodes['tl-cf'].value), frame + 1);
  assert.equal(f.state.cameraView, true); assert.equal(f.nodes['btn-camera'].getAttribute('aria-pressed'), 'true');
  assert.equal(f.session.info().contentRevision, 0); assert.equal(f.session.isIndeterminate(), false);
  assert.equal(typeof f.session.editLayerPolicy, 'function', 'existing policy gate retains current presentation');
}
async function toFour(f, input) {
  const painted = f.waitForPolicyPaint();
  await Promise.all(f.nodes['btn-camera'].dispatch('click')); await painted;
  for (let frame = 1; frame <= 4; frame++) assert.equal(await input.navigate('btn-nf', frame), true);
}
function lastPreview(f) { return f.calls.filter(call => call.command === 'nemo_native_object_preview').at(-1).request; }
async function assertTiming(f, frame) {
  const result = await f.session.readLayerTiming('drawing', frame, f.session.info());
  assert.ok(result, 'timing read remains admitted after navigation');
  assert.equal(result.frame, frame); assert.equal(result.atRevision, 0); assert.equal(result.sourceFrame, 0);
}

test('Camera relative keyboard 1.1 override survives the actual First listener with frame0 output and current read gates',
  { timeout: 3000 }, async () => {
    const rows = [...H.PREVIEWS, held(4, HELD_FOUR), held(0, HELD_ZERO)];
    const { f } = await opened(rows, [timing(0)]), input = installInputs(f);
    await toFour(f, input);
    const painted = f.waitForPolicyPaint(); input.keyboardZoom(); await painted;
    assertOutput(f, 4, HELD_FOUR);
    assert.equal(await input.navigate('btn-ff', 0), true); await turn();
    assertOutput(f, 0, HELD_ZERO); assert.equal(lastPreview(f).frame, 0);
    await assertTiming(f, 0);
    assert.equal(f.previewCount(), 9);
    assert.deepEqual(f.calls.filter(call => call.command === 'nemo_native_object_preview').slice(-2)
      .map(call => [call.request.frame, call.request.viewMode, call.request.navigation.zoom]),
    [[4, 'stored-camera', 1.1], [0, 'stored-camera', 1.1]]);
  });
test('a held keyboard preview with coalesced frame4 refresh cannot overwrite a later actual First frame0',
  { timeout: 3000 }, async () => {
    const entered = gate(), release = gate();
    // Optional trailing fixed row lets the old stale refresh complete if it
    // occurs, so final-state assertions expose it instead of transport refusal.
    const rows = [...H.PREVIEWS, held(4, HELD_FOUR, { entered, release }), held(0, HELD_ZERO), held(4, HELD_FOUR)];
    const { f } = await opened(rows, [timing(0)]), input = installInputs(f);
    await toFour(f, input);
    input.keyboardZoom(); await entered.promise;
    const coalesced = f.session.renderPreview(4); // Actual production coalescing API; no fixture queue.
    const first = input.navigate('btn-ff', 0);
    await turn(); // Enqueue the real navigator while the transport still holds its predecessor.
    release.resolve(); await first; await coalesced; await turn();
    assertOutput(f, 0, HELD_ZERO); assert.equal(lastPreview(f).frame, 0, 'no stale frame4 request after First');
    await assertTiming(f, 0);
    assert.equal(f.previewCount(), 9, 'superseded coalesced intent adds no trailing preview');
  });
test('a mismatched held-navigation First receipt refuses publication, restores frame4, then permits a valid First',
  { timeout: 3000 }, async () => {
    const rows = [...H.PREVIEWS, held(4, HELD_FOUR), held(0, H.LOCKED_FOUR), held(4, HELD_FOUR), held(0, HELD_ZERO)];
    const { f } = await opened(rows, [timing(4), timing(0)]), input = installInputs(f);
    await toFour(f, input);
    const painted = f.waitForPolicyPaint(); input.keyboardZoom(); await painted;
    assert.equal(await input.navigate('btn-ff', 0), false); await turn();
    assertOutput(f, 4, HELD_FOUR); await assertTiming(f, 4);
    assert.equal(f.notices.filter(message => message === 'Could not display this frame').length, 1);
    assert.equal(await input.navigate('btn-ff', 0), true); await turn();
    assertOutput(f, 0, HELD_ZERO); await assertTiming(f, 0);
    assert.deepEqual(f.calls.filter(call => call.command === 'nemo_native_object_preview').slice(-3).map(call => call.request.frame), [0, 4, 0]);
  });
