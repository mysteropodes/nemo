'use strict';
// What each lane assigns and what the rows cover. A lane total counts executed rows only, so "3/3 rows passed" must never read as
// "the lane is covered". `ASSIGNED` is the ids each lane issue lists; a row names the ids it exercises in `tasks`; `NOT_COVERED`
// names what no row observes yet (an id with no row at all, or a behaviour of a covered id that the lane text requires). The
// receipt carries this table, and a unit test fails when a lane assigns an id that is neither covered nor listed here.
const ASSIGNED = Object.freeze({
  2107: ['G14', 'G22', 'G13'],
  2108: ['G24', 'G18', 'G19', 'F07', 'F08', 'F15'],
  2112: ['G01', 'G02', 'G27', 'G38a', 'G38b'],
  2113: ['D32', 'F16', 'G03a', 'I16a', 'G06', 'G17'],
});

// `id` is set when the whole id has no row; otherwise the entry is a behaviour of an id that has a row.
const NOT_COVERED = Object.freeze({
  2107: [
    { what: 'Preview (viewport) rendering of records, effects and parametric shapes: only the PNG export is observed', reason: 'no preview read is reachable through nemo-mcp' },
    { what: 'Parametric kinds other than the rectangle (ellipse, polygon, star) and parametric edits on a rotated box', reason: 'only the rectangle is exercised' },
  ],
  2108: [
    { what: 'Nested combined children as one operand, group reorder, linked-fill joins', reason: 'no row builds a nested group, reorders a group or authors a linked fill' },
    { what: 'Flatten consuming current element Motion', reason: 'boolean.flatten carries no mode and no row authors element Motion, so flatten is observed on Animation 2D records only' },
    { what: 'Clipboard remap of rig, component and mesh references (group and stroke ids are remapped and observed)', reason: 'no row builds a rig, a component or a mesh-bound raster on the copied layer' },
    { what: 'Layer lifecycle on component and folder layers', reason: 'only plain drawing layers are exercised' },
  ],
  2112: [
    { id: 'G38b', what: 'Retained glyph and layout output (glyph identity, layout boxes)', reason: 'no public read returns glyph or layout boxes; the text row compares ink bounds with ranges derived from the declared size, which does not observe G38b' },
    { what: 'Media and text assets other than PNG, and fonts supplied as project assets', reason: 'only PNG assets and the bundled font are exercised; other codecs are unavailable by contract' },
  ],
  2113: [
    { what: 'Component evaluated-shape producers and rig producers in the pinned scene (D32, F16, I16a)', reason: 'no row builds a component or a rig; the mixed scene joins vector, gradient, text and raster producers only' },
    { what: 'Bitmap-brush quota exhaustion and publication cancel', reason: 'the quota and cancellation need resources or timing this runner does not drive; size refusal (wrong-size asset) and replacement are exercised' },
  ],
});

function laneCoverage(lane, rows) {
  const assigned = ASSIGNED[lane] || [], mine = rows.filter(r => String(r.lane) === String(lane));
  const covered = {};
  for (const id of assigned) covered[id] = mine.filter(r => (r.tasks || []).includes(id)).map(r => r.id);
  const entries = NOT_COVERED[lane] || [];
  const uncoveredIds = assigned.filter(id => covered[id].length === 0 || entries.some(e => e.id === id));
  return { assignedIds: assigned, coveredByRows: covered, uncoveredIds, notCovered: entries, laneComplete: uncoveredIds.length === 0 && entries.length === 0 };
}
function coverageTable(rows, lanes = Object.keys(ASSIGNED)) {
  const out = {};
  for (const lane of lanes) if (ASSIGNED[lane]) out[lane] = laneCoverage(lane, rows);
  return out;
}

module.exports = { ASSIGNED, NOT_COVERED, laneCoverage, coverageTable };
