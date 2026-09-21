'use strict';

const assert = require('node:assert/strict');
const path = require('node:path');
const test = require('node:test');

const watchedGlobals = [
  'NemoNativeOpacityEditorAdapter', 'NemoNativeOpacitySelectionAdapter',
  'NemoApplication', 'SMMotion', 'pushUndo', '__TAURI__',
].map((name) => [name, globalThis[name]]);
const editor = require('../src/js/adapters/native-opacity-editor.js');
const { projectSelection } = require('../src/js/adapters/native-opacity-selection.js');
const { createNativeApplicationAdapter } = require('../src/js/adapters/native-application.js');

const LAYER = 'r08_curve_layer';
const INSTANCE = 'instance-a';
const DOCUMENT = 'native-document-1';
const STATIC = Object.freeze({ stableTarget: Object.freeze({ layerUid: LAYER }), opacityMode: 'static' });
const KEYED = Object.freeze({ stableTarget: Object.freeze({ layerUid: LAYER }), opacityMode: 'keyed' });

test('common evaluated read responses feed the existing selection projection unchanged', async () => {
  for (const [frame, value] of [[0, 20], [10, 50], [20, 80]]) {
    const evaluation = { documentSnapshotId: `native-opacity:${DOCUMENT}:0`, documentId: DOCUMENT,
      contentRevision: 0, contextId: 'scene-root', frame, layers: [{ layerUid: LAYER, value }] };
    const adapter = createNativeApplicationAdapter('read-selection', { dispatch: (request) => ({
      apiVersion: 2, requestId: request.requestId, instanceId: INSTANCE, documentId: DOCUMENT,
      contentRevision: 2, ok: true, result: evaluation,
    }) });
    const response = await adapter.dispatch({ apiVersion: 2, requestId: `selection-${frame}`,
      instanceId: INSTANCE, documentId: DOCUMENT, operation: 'query.document.evaluate',
      payload: { atRevision: 0, contextId: 'scene-root', frame } });
    const selected = projectSelection(response.result, {
      activeLayerUid: LAYER, selected: [{ layerUid: LAYER, opacityMode: 'keyed' }],
    });
    assert.equal(selected.selected[0].value, value);
    assert.equal(selected.selected[0].editable, false);
    assert.equal(selected.documentSnapshotId, evaluation.documentSnapshotId);
    assert.equal(selected.documentId, DOCUMENT);
    assert.equal(selected.contentRevision, 0);
    assert.equal(selected.frame, frame);
    assert.equal(selected.contextId, 'scene-root');
  }
});

function identity(contentRevision = 0, documentId = DOCUMENT) {
  return { instanceId: INSTANCE, documentId, contentRevision };
}

class NativeFixturePort {
  constructor(value = 25) {
    this.value = value;
    this.revision = 0;
    this.seen = [];
    this.undo = [];
    this.redo = [];
    this.transaction = null;
    this.nextTransaction = 0;
  }

  envelope(request, ok, value) {
    const response = {
      apiVersion: 2,
      requestId: request.requestId,
      instanceId: INSTANCE,
      documentId: DOCUMENT,
      contentRevision: this.revision,
      ok,
    };
    response[ok ? 'result' : 'error'] = value;
    return response;
  }

  failure(request, code, message, details) {
    const error = { code, message };
    if (details !== undefined) error.details = details;
    return this.envelope(request, false, error);
  }

  transactionResult(transaction, terminalDisposition = null, committedRevision = null, historyEntriesAdded = 0) {
    return {
      transactionId: transaction.id,
      baseRevision: transaction.baseRevision,
      workingGeneration: transaction.generation,
      workingState: { stableTarget: { layerUid: LAYER }, value: transaction.workingValue },
      committedRevision,
      terminalDisposition,
      historyEntriesAdded,
    };
  }

  async dispatch(request) {
    this.seen.push(structuredClone(request));
    if (request.instanceId !== INSTANCE) {
      return this.failure(request, 'wrong_instance', 'wrong instance');
    }
    if (request.documentId !== DOCUMENT) {
      return this.failure(request, 'wrong_document', 'replaced document', {
        requestedDocumentId: request.documentId,
      });
    }
    if (request.cancelledBeforeDispatch === true) {
      return this.failure(request, 'cancelled_before_dispatch', 'cancelled before dispatch');
    }
    if (['command.document.apply', 'transaction.begin', 'history.undo', 'history.redo']
      .includes(request.operation) && request.expectedRevision !== this.revision) {
      return this.failure(request, 'stale_revision', 'read current content revision');
    }
    if (request.operation === 'query.document.opacity') {
      return this.envelope(request, true, {
        atRevision: request.payload.atRevision === undefined ? this.revision : request.payload.atRevision,
        layerUid: LAYER,
        value: this.value,
      });
    }
    if (request.operation === 'command.document.apply') {
      const before = this.value;
      const applied = before !== request.payload.value;
      if (applied) {
        this.undo.push(before);
        this.redo.length = 0;
        this.value = request.payload.value;
        this.revision += 1;
      }
      return this.envelope(request, true, {
        applied,
        historyEntriesAdded: applied ? 1 : 0,
      });
    }
    if (request.operation === 'transaction.begin') {
      this.nextTransaction += 1;
      this.transaction = {
        id: `native-transaction-${this.nextTransaction}`,
        baseRevision: this.revision,
        baseValue: this.value,
        workingValue: this.value,
        generation: 0,
      };
      return this.envelope(request, true, this.transactionResult(this.transaction));
    }
    if (request.operation === 'transaction.update') {
      if (!this.transaction || request.payload.transactionId !== this.transaction.id) {
        return this.failure(request, 'not_found', 'transaction not found');
      }
      this.transaction.workingValue = request.payload.value;
      this.transaction.generation += 1;
      return this.envelope(request, true, this.transactionResult(this.transaction));
    }
    if (request.operation === 'transaction.commit') {
      if (!this.transaction || request.payload.transactionId !== this.transaction.id) {
        return this.failure(request, 'not_found', 'transaction not found');
      }
      const transaction = this.transaction;
      if (transaction.baseRevision !== this.revision) {
        return this.failure(request, 'stale_revision', 'transaction base is stale');
      }
      this.undo.push(transaction.baseValue);
      this.redo.length = 0;
      this.value = transaction.workingValue;
      this.revision += 1;
      this.transaction = null;
      return this.envelope(request, true,
        this.transactionResult(transaction, 'succeeded', this.revision, 1));
    }
    if (request.operation === 'transaction.cancel') {
      if (!this.transaction || request.payload.transactionId !== this.transaction.id) {
        return this.failure(request, 'not_found', 'transaction not found');
      }
      const transaction = this.transaction;
      this.transaction = null;
      return this.envelope(request, true,
        this.transactionResult(transaction, 'cancelled', null, 0));
    }
    if (request.operation === 'history.undo') {
      if (!this.undo.length) return this.failure(request, 'unavailable', 'history unavailable');
      this.redo.push(this.value);
      this.value = this.undo.pop();
      this.revision += 1;
      return this.envelope(request, true, { applied: true, historyEntriesAdded: 0 });
    }
    if (request.operation === 'history.redo') {
      if (!this.redo.length) return this.failure(request, 'unavailable', 'history unavailable');
      this.undo.push(this.value);
      this.value = this.redo.pop();
      this.revision += 1;
      return this.envelope(request, true, { applied: true, historyEntriesAdded: 0 });
    }
    return this.failure(request, 'invalid_request', 'unsupported fixture operation');
  }
}

function application(port) {
  return createNativeApplicationAdapter('n17-fixture', port);
}

test('modules are explicit-only and do not discover or activate legacy, host, or MCP globals', () => {
  for (const [name, prior] of watchedGlobals) assert.strictEqual(globalThis[name], prior, name);
  assert.equal(path.basename(require.resolve('../src/js/adapters/native-opacity-editor.js')),
    'native-opacity-editor.js');
  assert.equal(Object.isFrozen(editor), true);
});

test('projects the exact 25→40→60→undo→40 v2 sequence with one gesture history entry', async () => {
  const port = new NativeFixturePort();
  const app = application(port);
  let current = identity();

  const initialRequest = editor.queryOpacity(current, 'query-25', STATIC);
  assert.equal(Object.isFrozen(initialRequest), true);
  assert.equal(Object.isFrozen(initialRequest.payload.stableTarget), true);
  const initial = await app.dispatch(initialRequest);
  assert.equal(initial.result.value, 25);

  const set = await app.dispatch(editor.setOpacity(current, 'set-40', STATIC, 40));
  assert.deepEqual(set.result, { applied: true, historyEntriesAdded: 1 });
  current = identity(set.contentRevision);

  const begun = await app.dispatch(editor.beginGesture(current, 'gesture-begin', STATIC));
  const transactionId = begun.result.transactionId;
  assert.equal(begun.result.workingState.value, 40);
  const updated = await app.dispatch(
    editor.updateGesture(current, 'gesture-update', transactionId, 60));
  assert.equal(updated.contentRevision, 1);
  assert.equal(updated.result.workingGeneration, 1);
  assert.equal(updated.result.workingState.value, 60);
  assert.equal(updated.result.historyEntriesAdded, 0);

  const committed = await app.dispatch(
    editor.commitGesture(current, 'gesture-commit', transactionId));
  assert.equal(committed.contentRevision, 2);
  assert.equal(committed.result.committedRevision, 2);
  assert.equal(committed.result.terminalDisposition, 'succeeded');
  assert.equal(committed.result.historyEntriesAdded, 1);
  current = identity(committed.contentRevision);

  const undone = await app.dispatch(editor.undo(current, 'undo-gesture'));
  assert.deepEqual(undone.result, { applied: true, historyEntriesAdded: 0 });
  current = identity(undone.contentRevision);
  const final = await app.dispatch(editor.queryOpacity(current, 'query-after-undo', STATIC));
  assert.equal(final.result.value, 40);

  assert.deepEqual(port.seen.map((request) => request.operation), [
    'query.document.opacity',
    'command.document.apply',
    'transaction.begin',
    'transaction.update',
    'transaction.commit',
    'history.undo',
    'query.document.opacity',
  ]);
  assert.deepEqual(port.seen.map((request) => Object.hasOwn(request, 'expectedRevision')), [
    false, true, true, false, false, true, false,
  ]);
  assert.equal(port.seen.every((request) => request.documentId === DOCUMENT), true);
  assert.equal(port.seen[1].payload.stableTarget.layerUid, LAYER);
});

test('cancelled gesture is revision neutral and adds no history', async () => {
  const port = new NativeFixturePort(40);
  const app = application(port);
  const current = identity();
  const begun = await app.dispatch(editor.beginGesture(current, 'cancel-begin', STATIC));
  const transactionId = begun.result.transactionId;
  await app.dispatch(editor.updateGesture(current, 'cancel-update', transactionId, 80));
  const cancelled = await app.dispatch(
    editor.cancelGesture(current, 'cancel-gesture', transactionId));
  assert.equal(cancelled.contentRevision, 0);
  assert.equal(cancelled.result.terminalDisposition, 'cancelled');
  assert.equal(cancelled.result.historyEntriesAdded, 0);
  const after = await app.dispatch(editor.queryOpacity(current, 'query-after-cancel', STATIC));
  assert.equal(after.result.value, 40);
  assert.equal(port.undo.length, 0);
});

test('stale, wrong-document, and pre-dispatch cancellation stay explicit and never retry', async () => {
  const stalePort = new NativeFixturePort();
  stalePort.revision = 1;
  const stale = await application(stalePort).dispatch(
    editor.setOpacity(identity(0), 'stale-set', STATIC, 40));
  assert.equal(stale.ok, false);
  assert.equal(stale.error.code, 'stale_revision');
  assert.equal(stalePort.seen.length, 1);
  assert.equal(stalePort.value, 25);

  const wrongPort = new NativeFixturePort();
  const wrong = await application(wrongPort).dispatch(
    editor.queryOpacity(identity(0, 'replaced-document'), 'wrong-document', STATIC));
  assert.equal(wrong.ok, false);
  assert.equal(wrong.error.code, 'wrong_document');
  assert.equal(wrong.documentId, DOCUMENT);
  assert.equal(wrong.error.details.requestedDocumentId, 'replaced-document');
  assert.equal(wrongPort.seen.length, 1);

  const cancelledPort = new NativeFixturePort();
  const projected = editor.setOpacity(identity(), 'cancelled-set', STATIC, 40);
  const cancelledRequest = editor.cancelBeforeDispatch(projected);
  assert.equal(Object.isFrozen(cancelledRequest), true);
  assert.equal(projected.cancelledBeforeDispatch, undefined);
  const cancelled = await application(cancelledPort).dispatch(cancelledRequest);
  assert.equal(cancelled.ok, false);
  assert.equal(cancelled.error.code, 'cancelled_before_dispatch');
  assert.equal(cancelledPort.seen.length, 1);
  assert.equal(cancelledPort.value, 25);
});

test('keyed selection stays read-only and preserves native N10 20/50/80 evaluations', () => {
  const values = [
    [0, 20],
    [10, 50],
    [20, 80],
  ].map(([frame, value]) => {
    const evaluation = {
      documentSnapshotId: 'native-opacity:native-document-1:0',
      documentId: DOCUMENT,
      contentRevision: 0,
      contextId: 'scene-root',
      frame,
      layers: [{ layerUid: LAYER, value }],
    };
    const descriptor = {
      activeLayerUid: LAYER,
      selected: [{ layerUid: LAYER, opacityMode: 'keyed' }],
    };
    const before = JSON.stringify({ evaluation, descriptor });
    const projected = projectSelection(evaluation, descriptor);
    assert.equal(JSON.stringify({ evaluation, descriptor }), before);
    assert.equal(projected.activeTarget.layerUid, LAYER);
    assert.equal(projected.selected[0].stableTarget.layerUid, LAYER);
    assert.equal(projected.selected[0].opacityMode, 'keyed');
    assert.equal(projected.selected[0].editable, false);
    assert.equal(Object.isFrozen(projected), true);
    assert.equal(Object.isFrozen(projected.selected[0].stableTarget), true);
    return projected.selected[0].value;
  });
  assert.deepEqual(values, [20, 50, 80]);

  for (const project of [editor.queryOpacity, editor.setOpacity, editor.beginGesture]) {
    assert.throws(() => project(identity(), `keyed-${project.name}`, KEYED, 40),
      /unavailable for read-only keyed opacity/);
  }
});

test('selection projection rejects index identity, duplicates, missing targets, and unknown targets', () => {
  const evaluation = {
    documentSnapshotId: 'native-opacity:native-document-1:0',
    documentId: DOCUMENT,
    contentRevision: 0,
    contextId: 'scene-root',
    frame: 10,
    layers: [{ layerUid: LAYER, value: 50 }],
  };
  assert.throws(() => projectSelection(evaluation, {
    activeLayerUid: LAYER,
    selected: [{ layerUid: LAYER, opacityMode: 'keyed', layerIndex: 0 }],
  }), /unknown fields/);
  assert.throws(() => projectSelection(evaluation, {
    activeLayerUid: LAYER,
    selected: [
      { layerUid: LAYER, opacityMode: 'keyed' },
      { layerUid: LAYER, opacityMode: 'keyed' },
    ],
  }), /repeats layerUid/);
  assert.throws(() => projectSelection(evaluation, {
    activeLayerUid: 'missing-layer',
    selected: [{ layerUid: LAYER, opacityMode: 'keyed' }],
  }), /activeLayerUid/);
  assert.throws(() => projectSelection(evaluation, {
    activeLayerUid: 'missing-layer',
    selected: [{ layerUid: 'missing-layer', opacityMode: 'keyed' }],
  }), /absent from evaluation/);
  const repeatedEvaluation = structuredClone(evaluation);
  repeatedEvaluation.layers.push(structuredClone(repeatedEvaluation.layers[0]));
  assert.throws(() => projectSelection(repeatedEvaluation, {
    activeLayerUid: null,
    selected: [],
  }), /evaluation repeats layerUid/);
});
