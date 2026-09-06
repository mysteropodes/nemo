const assert = require('node:assert/strict');
const test = require('node:test');
const { create } = require('../src/js/application/motion-application.js');

function fixture() {
  const layer = { id: 'layer-a', name: 'A', opacity: 100, animated: false, keys: [] };
  let undo = [], redo = [];
  const port = {
    documentId: () => 'doc-a', totalFrames: () => 24,
    layer: (id) => id === layer.id ? layer : null,
    isAnimated: (l) => l.animated,
    snapshot: (revision) => ({ documentId: 'doc-a', revision, frame: 0, fps: 24, totalFrames: 24, context: {}, layers: [JSON.parse(JSON.stringify(layer))] }),
    beginHistory: () => undo.push(JSON.parse(JSON.stringify(layer))), cancelHistory: () => undo.pop(),
    wouldSetOpacity: (l, value, frame, key) => l.opacity !== value || (key && !l.keys.some((k) => k.frame === frame)),
    setOpacity: (l, value, frame, key) => {
      if (l.opacity === value && !(key && !l.keys.some((k) => k.frame === frame))) return false;
      l.opacity = value;
      if (key || l.animated) { l.animated = true; const old = l.keys.find((k) => k.frame === frame); if (old) old.value = value; else l.keys.push({ frame, value }); }
      return true;
    }, refresh: () => {},
    undo: () => { if (!undo.length) return { changed: false }; redo.push(JSON.parse(JSON.stringify(layer))); Object.assign(layer, undo.pop()); return { changed: true }; },
    redo: () => { if (!redo.length) return { changed: false }; undo.push(JSON.parse(JSON.stringify(layer))); Object.assign(layer, redo.pop()); return { changed: true }; }
  };
  return { app: create(port), layer };
}
function request(revision, command, args, requestId = 'r1') { return { apiVersion: 1, requestId, documentId: 'doc-a', expectedRevision: revision, command, args }; }

test('writes evaluated opacity and records one undoable transaction', () => {
  const { app, layer } = fixture();
  assert.equal(app.dispatch(request(0, 'property.set', { layerId: 'layer-a', property: 'opacity', value: 45 })).ok, true);
  assert.equal(layer.opacity, 45); assert.equal(app.query().revision, 1);
  assert.equal(app.dispatch(request(1, 'history.undo', {}, 'undo')).ok, true);
  assert.equal(layer.opacity, 100); assert.equal(app.query().revision, 2);
  app.dispatch(request(2, 'history.redo', {}, 'redo'));
  assert.equal(layer.opacity, 45); assert.equal(app.query().revision, 3);
});
test('keyframes retain stable IDs and the immutable query projection', () => {
  const { app } = fixture();
  const out = app.dispatch(request(0, 'property.keyframe', { layerId: 'layer-a', property: 'opacity', frame: 6, value: 20 }));
  assert.equal(out.ok, true);
  const q = app.query(); assert.equal(q.layers[0].id, 'layer-a'); assert.equal(q.layers[0].animated, true); assert.deepEqual(q.layers[0].keys, [{ frame: 6, value: 20 }]);
  q.layers[0].keys[0].value = 99; assert.equal(app.query().layers[0].keys[0].value, 20);
});
test('rejects stale and malformed writes before history effects', () => {
  const { app, layer } = fixture();
  const stale = app.dispatch(request(4, 'property.set', { layerId: 'layer-a', property: 'opacity', value: 30 }));
  assert.equal(stale.error.code, 'stale_revision'); assert.equal(layer.opacity, 100);
  const bad = app.dispatch(request(0, 'property.set', { layerId: 'layer-a', property: 'opacity', value: 101 }, 'bad'));
  assert.equal(bad.error.code, 'invalid_opacity'); assert.equal(layer.opacity, 100);
});
test('records exact retries and rejects reused request IDs with different content', () => {
  const { app, layer } = fixture();
  const first = request(0, 'property.set', { layerId: 'layer-a', property: 'opacity', value: 35 }, 'same');
  assert.deepEqual(app.dispatch(first), app.dispatch(first)); assert.equal(layer.opacity, 35); assert.equal(app.revision(), 1);
  const changed = app.dispatch(request(1, 'property.set', { layerId: 'layer-a', property: 'opacity', value: 36 }, 'same'));
  assert.equal(changed.error.code, 'request_id_reused'); assert.equal(layer.opacity, 35);
});
test('animated property.set requires an explicit frame and UI mutations stale API writers', () => {
  const { app, layer } = fixture();
  app.dispatch(request(0, 'property.keyframe', { layerId: 'layer-a', property: 'opacity', frame: 2, value: 60 }));
  const missing = app.dispatch(request(1, 'property.set', { layerId: 'layer-a', property: 'opacity', value: 50 }, 'missing'));
  assert.equal(missing.error.code, 'invalid_frame');
  assert.equal(app.applyUiOpacity(layer, [55]), true);
  assert.equal(app.revision(), 2);
  assert.equal(app.dispatch(request(1, 'property.set', { layerId: 'layer-a', property: 'opacity', frame: 3, value: 50 }, 'stale')).error.code, 'stale_revision');
});
