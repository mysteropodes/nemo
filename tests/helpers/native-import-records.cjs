'use strict';
// L10-J test helper: a minimal admitted `editable-vector-path` record and a mixed batch, as the L10-H wire carries them.
const record = (layerRef, x) => ({ schemaVersion: 1, family: 'editable-vector-path',
  target: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 0 }, layerUid: layerRef, strokeId: '' },
  geometry: { kind: 'cubic-path', closed: false, coordinateSpace: 'document-world', handleSpace: 'relative', segments: [
    { point: { x, y: 0 }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 } }, { point: { x: x + 10, y: 5 }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 } }] },
  fill: null, vector: { stroke: { kind: 'solid', r: 0, g: 0, b: 0, a: 1 }, hasRealStroke: true, strokeWidth: 2, strokeCap: 'butt', strokeJoin: 'miter', opacity: 1 } });
const mixed = () => ({
  command: 'object.import.batch',
  layers: [{ sourceId: 'w', name: 'Wire' }],
  records: [{ sourceId: 'w:0', record: record('w', 1) }, { sourceId: 'w:1', record: record('w', 2) }, { sourceId: 'e:0', record: record('L0', 3) }],
  skipped: [{ kind: 'element', sourceId: 'svg:9', reason: 'unsupported-element:Raster' }], unavailable: [],
});
module.exports = { record, mixed };
