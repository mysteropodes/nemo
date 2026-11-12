'use strict';
// Request builders for the operations the rows exercise. Each returns {operation, payload}; the payload shapes are the
// ones native-transport-v2.schema.json (and the item / bitmap-brush schemas) declare, and a unit test validates every
// builder's output against those schemas, so a builder cannot drift from the contract unnoticed.
const OBJ = 'command.document.object.';
const QOBJ = 'query.document.object';
const ROOT = 'scene-root';

const frameScope = frame => ({ kind: 'authored', frame });
const target = (layerUid, strokeId, frame = 0) => ({ contextId: ROOT, frameScope: frameScope(frame), layerUid, strokeId });
const seg = (x, y, handleIn = [0, 0], handleOut = [0, 0]) => ({ point: { x, y }, handleIn: { x: handleIn[0], y: handleIn[1] }, handleOut: { x: handleOut[0], y: handleOut[1] } });
const rectSegments = (x0, y0, x1, y1) => [seg(x0, y0), seg(x1, y0), seg(x1, y1), seg(x0, y1)];
const solid = ([r, g, b, a = 1]) => ({ kind: 'solid', r, g, b, a });

// An editable-vector-path record. fill / stroke are [r,g,b,a] in 0..1 or null.
function vectorRecord({ layerUid, segments, closed = true, fill = null, stroke = null, width = 4, strokeId = '', frame = 0, opacity = 1, cap = 'round', join = 'round', ext }) {
  const record = {
    schemaVersion: 1, family: 'editable-vector-path', target: target(layerUid, strokeId, frame),
    geometry: { kind: 'cubic-path', closed, coordinateSpace: 'document-world', handleSpace: 'relative', segments },
    fill: fill ? solid(fill) : null,
    vector: { stroke: stroke ? solid(stroke) : null, hasRealStroke: stroke !== null, strokeWidth: width, strokeCap: cap, strokeJoin: join, opacity },
  };
  if (ext) record.vectorExt = ext;
  return record;
}
const vectorCreate = (record, extra = {}) => ({ operation: OBJ + 'vector.edit', payload: { command: 'object.vector.create', record, ...extra } });
const vectorReplace = record => ({ operation: OBJ + 'vector.edit', payload: { command: 'object.vector.replace', record } });
const vectorDelete = stableTarget => ({ operation: OBJ + 'vector.edit', payload: { command: 'object.vector.delete', stableTarget } });
const parametricSet = (stableTarget, paramShape) => ({ operation: OBJ + 'parametric.edit', payload: { command: 'object.parametric.set', stableTarget, paramShape } });
const parametricTranslate = (stableTarget, delta) => ({ operation: OBJ + 'parametric.edit', payload: { command: 'object.parametric.translate', stableTarget, delta } });
const parametricScale = (stableTarget, scale, anchor) => ({ operation: OBJ + 'parametric.edit', payload: { command: 'object.parametric.scale', stableTarget, scale, anchor } });
const pathEffects = (layerUid, stack) => ({ operation: OBJ + 'path.effects.edit', payload: { command: 'object.path.effects.set', layerUid, stack } });
const layerPolicy = (layerUid, flag, value) => ({ operation: OBJ + 'layer.policy.edit', payload: { command: 'object.layer.policy.edit', layerUid, flag, value } });
const undo = (steps) => ({ operation: OBJ + 'undo', payload: steps === undefined ? { command: 'object.undo' } : { command: 'object.undo', steps } });
const redo = (steps) => ({ operation: OBJ + 'redo', payload: steps === undefined ? { command: 'object.redo' } : { command: 'object.redo', steps } });

const scoped = (layerUid, extra) => ({ layerUid, contextId: ROOT, ...extra });
const groupCreate = (layerUid, groupId, name, strokeIds) => ({ operation: OBJ + 'group.create', payload: { command: 'object.group.create', ...scoped(layerUid, { groupId, name, strokeIds }) } });
const groupUngroup = (layerUid, strokeIds) => ({ operation: OBJ + 'group.ungroup', payload: { command: 'object.group.ungroup', ...scoped(layerUid, { strokeIds }) } });
const groupCombine = (layerUid, groupId, mode, strokeIds) => ({ operation: OBJ + 'group.combine', payload: { command: 'object.group.combine', ...scoped(layerUid, { groupId, mode, strokeIds }) } });
const groupMode = (layerUid, groupId, mode) => ({ operation: OBJ + 'group.mode.set', payload: { command: 'object.group.mode.set', ...scoped(layerUid, { groupId, mode }) } });
const groupRename = (layerUid, groupId, name) => ({ operation: OBJ + 'group.rename', payload: { command: 'object.group.rename', ...scoped(layerUid, { groupId, name }) } });
const groupMemberRemove = (layerUid, strokeId) => ({ operation: OBJ + 'group.member.remove', payload: { command: 'object.group.member.remove', ...scoped(layerUid, { strokeId }) } });
const booleanApply = (layerUid, mode, strokeIds, frame = 0) => ({ operation: OBJ + 'boolean.apply', payload: { command: 'object.boolean.apply', mode, ...scoped(layerUid, { frame, strokeIds }) } });
const booleanErase = (layerUid, strokeId, footprint, frame = 0) => ({ operation: OBJ + 'boolean.erase', payload: { command: 'object.boolean.erase', ...scoped(layerUid, { frame, strokeId, footprint }) } });
const booleanFlatten = (layerUid, groupId, frame = 0) => ({ operation: OBJ + 'boolean.flatten', payload: { command: 'object.boolean.flatten', ...scoped(layerUid, { frame, groupId }) } });

const layerCreate = name => ({ operation: OBJ + 'layer.create', payload: { command: 'object.layer.create', name } });
const layerDuplicate = layerUid => ({ operation: OBJ + 'layer.duplicate', payload: { command: 'object.layer.duplicate', layerUid } });
const layerRename = (layerUid, name) => ({ operation: OBJ + 'layer.rename', payload: { command: 'object.layer.rename', layerUid, name } });
const layerReorder = (layerUids, gap, active) => ({ operation: OBJ + 'layer.reorder', payload: { command: 'object.layer.reorder', layerUids, gap, selection: { active, selected: layerUids } } });
const layerDelete = (active, selected = [active]) => ({ operation: OBJ + 'layer.delete', payload: { command: 'object.layer.delete', selection: { active, selected } } });
const layerVisibility = (layerUid, visible) => ({ operation: OBJ + 'layer.visibility.set', payload: { command: 'object.layer.visibility.set', layerUid, visible } });
const layerSplit = (layerUid, frame = 0) => ({ operation: OBJ + 'layer.split', payload: { command: 'object.layer.split', ...scoped(layerUid, { frame, labels: { shape: 'Shape', stroke: 'Stroke' } }) } });
const selectionDelete = (layerUid, strokeIds, frame = 0) => ({ operation: OBJ + 'selection.delete', payload: { command: 'object.selection.delete', ...scoped(layerUid, { frame, mode: 'animation2d', strokeIds }) } });
const selectionDuplicate = (layerUid, strokeIds, offset, frame = 0) => ({ operation: OBJ + 'selection.duplicate', payload: { command: 'object.selection.duplicate', ...scoped(layerUid, { frame, mode: 'animation2d', strokeIds, ...(offset ? { offset } : {}) }) } });
const clipboardCopy = (layerUid, strokeIds, frame = 0) => ({ operation: OBJ + 'clipboard.copy', payload: { command: 'object.clipboard.copy', ...scoped(layerUid, { frame, mode: 'animation2d', strokeIds }) } });
const clipboardCut = (layerUid, strokeIds, frame = 0) => ({ operation: OBJ + 'clipboard.cut', payload: { command: 'object.clipboard.cut', ...scoped(layerUid, { frame, mode: 'animation2d', strokeIds }) } });
const clipboardPaste = (layerUid, clip, frame = 0) => ({ operation: OBJ + 'clipboard.paste', payload: { command: 'object.clipboard.paste', ...scoped(layerUid, { frame, mode: 'animation2d', clip }) } });

const assetAttach = ({ pin, name, path, lifecycleGeneration, placement = { kind: 'newLayer' } }) => ({
  operation: OBJ + 'asset.attach', payload: { documentSnapshotId: pin, name, mediaType: 'image/png', source: { kind: 'file', path }, lifecycleGeneration, placement } });
const assetRemove = (pin, assetId) => ({ operation: OBJ + 'asset.remove', payload: { documentSnapshotId: pin, assetId } });
const ITEM = OBJ + 'item.edit';
const rasterMove = (stableTarget, x, y) => ({ operation: ITEM, payload: { command: 'object.raster.move', stableTarget, x, y } });
const rasterResize = (stableTarget, width, height) => ({ operation: ITEM, payload: { command: 'object.raster.resize', stableTarget, width, height } });
const rasterRotate = (stableTarget, rotation) => ({ operation: ITEM, payload: { command: 'object.raster.rotate', stableTarget, rotation } });
const rasterOpacity = (stableTarget, opacity) => ({ operation: ITEM, payload: { command: 'object.raster.opacity', stableTarget, opacity } });
const textBlock = (text, over = {}) => ({ text, font: { kind: 'bundled', fontKey: 'Roboto-Regular' }, size: 48, color: '#cc2200', align: 'left', fixedWidth: null, fixedHeight: null,
  bold: false, italic: false, underline: false, strike: false, letterSpacing: 0, wordSpacing: 0, lineHeightMult: 1.25, textCase: 'none', anchorTopLeft: { x: 20, y: 30 }, ...over });
const textCreate = (layerUid, block, frame = 0) => ({ operation: ITEM, payload: { command: 'object.text.create', target: target(layerUid, '', frame), block } });
const textEdit = (stableTarget, patch) => ({ operation: ITEM, payload: { command: 'object.text.edit', stableTarget, patch } });
const textDelete = stableTarget => ({ operation: ITEM, payload: { command: 'object.text.delete', stableTarget } });
const textTargets = (revision, pin, layerUid, frame = 0) => ({ operation: 'query.document.object.text',
  payload: { atRevision: revision, documentSnapshotId: pin, view: { kind: 'targets', scope: { contextId: ROOT, layerUid, frameScope: frameScope(frame) }, offset: 0, limit: 8 } } });
const textInspect = (revision, pin, stableTarget) => ({ operation: 'query.document.object.text', payload: { atRevision: revision, documentSnapshotId: pin, view: { kind: 'inspect', target: stableTarget } } });
const textBody = (revision, pin, stableTarget, offset = 0) => ({ operation: 'query.document.object.text', payload: { atRevision: revision, documentSnapshotId: pin, view: { kind: 'text', target: stableTarget, offset, limit: 1024 } } });

const brushPublish = (pin, assetId, brushId, width, height) => ({ operation: OBJ + 'brush.bitmap.publish', payload: { command: 'object.brush.bitmap.publish', documentSnapshotId: pin, assetId, brushId, width, height } });
const brushInspect = (revision, pin, brushId) => ({ operation: 'query.document.object.brush.bitmap', payload: { action: 'inspect', atRevision: revision, documentSnapshotId: pin, brushId } });
function brushStroke({ pin, lifecycleGeneration, layerUid, groupId, path, companion, segments, frame = 0 }) {
  const record = { schemaVersion: 1, family: 'editable-vector-path', target: target(layerUid, '', frame),
    geometry: { kind: 'cubic-path', closed: false, coordinateSpace: 'document-world', handleSpace: 'relative', segments },
    fill: null,
    vector: { stroke: solid([0, 0, 1, 1]), hasRealStroke: true, strokeWidth: 8, strokeCap: 'round', strokeJoin: 'round', miterLimit: 10, dashOffset: 0, opacity: 0 },
    vectorExt: { bitmapBrush: { bitmapBrushSpec: { tip: 'soft', size: 8, spacing: 15, scatter: 20, opacity: 1, color: '#0000ffff', pressure: false, seed: 7 }, brushGroupId: groupId, preTextureOpacity: 1 } } };
  return { operation: OBJ + 'brush.bitmap.stroke', payload: { documentSnapshotId: pin, lifecycleGeneration, record, companion, source: { kind: 'file', path } } };
}
const compositingBlend = (layerUid, mode) => ({ operation: OBJ + 'layer.compositing.edit', payload: { command: 'object.layer.compositing.edit', layerUid, edit: 'blend', mode } });
const objectRead = (revision, stableTarget) => ({ operation: QOBJ, payload: { atRevision: revision, stableTarget } });
const propertyOpacity = (layerUid, values) => ({ operation: OBJ + 'property.edit',
  payload: { command: 'object.property.edit', target: { contextId: ROOT, layerUid }, property: 'opacity', action: { kind: 'setValue', frame: 0, values } } });
const meshAttach = (stableTarget, mesh) => ({ operation: OBJ + 'image.mesh.edit', payload: { command: 'object.image.mesh.edit', action: { kind: 'attach', target: stableTarget, mesh } } });
const meshSetOffset = (meshId, index, du, dv) => ({ operation: OBJ + 'image.mesh.edit', payload: { command: 'object.image.mesh.edit', meshId, action: { kind: 'setOffset', index, du, dv } } });
const snapshotAcquire = () => ({ operation: 'query.document.snapshot.acquire', payload: {} });
const historyRead = (revision, pin) => ({ operation: QOBJ + '.history', payload: { atRevision: revision, documentSnapshotId: pin } });
const evaluate = (revision, frame = 0) => ({ operation: 'query.document.evaluate', payload: { atRevision: revision, contextId: ROOT, frame } });
const serialize = revision => ({ operation: 'query.document.serialize', payload: { atRevision: revision } });

module.exports = { OBJ, ROOT, target, seg, rectSegments, solid, vectorRecord, vectorCreate, vectorReplace, vectorDelete, parametricSet, parametricTranslate, parametricScale,
  pathEffects, layerPolicy, undo, redo, groupCreate, groupUngroup, groupCombine, groupMode, groupRename, groupMemberRemove, booleanApply, booleanErase, booleanFlatten,
  layerCreate, layerDuplicate, layerRename, layerReorder, layerDelete, layerVisibility, layerSplit, selectionDelete, selectionDuplicate, clipboardCopy, clipboardCut, clipboardPaste,
  assetAttach, assetRemove, rasterMove, rasterResize, rasterRotate, rasterOpacity, textBlock, textCreate, textEdit, textDelete, textTargets, textInspect, textBody,
  brushPublish, brushInspect, brushStroke, compositingBlend, objectRead, propertyOpacity, meshAttach, meshSetOffset, snapshotAcquire, historyRead, evaluate, serialize };
