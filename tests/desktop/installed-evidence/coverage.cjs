'use strict';
// What each lane assigns and what the rows cover. A lane total counts executed rows only, so "3/3 rows passed" must never read as
// "the lane is covered", and a row that FAILED or never ran does not cover anything. Per assigned id the receipt says exactly one of:
//   covered              at least one row naming the id PASSED (the rows are listed);
//   attemptedNotPassed   rows naming the id ran, none passed (failed rows are listed with their verdict);
//   neverAttempted       no row naming the id ran: it has no row, its rows were left out of the selection, or they were unrun.
// `alsoNotPassed` lists the failing rows of an id that is covered by other rows, so a lane is never read as covered while one of its rows fails.
// `ASSIGNED` is the ids each lane issue lists; a row names the ids it exercises in `tasks`; `NOT_COVERED` names what no row observes
// yet (an id with no row at all, or a behaviour of a covered id that the lane text requires) and, for each, the read operation that
// would be needed when the gap is a missing read. The receipt carries this table, and a unit test fails when a lane assigns an id that
// is neither covered nor listed here.
const ASSIGNED = Object.freeze({
  2107: ['G14', 'G22', 'G13'],
  2108: ['G24', 'G18', 'G19', 'F07', 'F08', 'F15'],
  2112: ['G01', 'G02', 'G27', 'G38a', 'G38b'],
  2113: ['D32', 'F16', 'G03a', 'I16a', 'G06', 'G17'],
});

// `id` is set when the whole id is unobserved whatever its rows do; otherwise the entry is a behaviour of an id that has a row.
const NOT_COVERED = Object.freeze({
  2107: [
    { what: 'Preview (viewport) rendering of records, effects and parametric shapes: only the PNG export is observed', reason: 'no preview read is reachable through nemo-mcp' },
  ],
  2108: [
    { what: 'Clipboard remap of rig, component and mesh references (group and stroke ids are remapped and observed)', reason: 'no row builds a rig, a component or a mesh-bound raster on the copied layer' },
    { what: 'Layer lifecycle on component and folder layers', reason: 'only plain drawing layers are exercised' },
  ],
  2112: [
    { what: 'Glyph identity (per-glyph ids and advances) of a retained text block', reason: 'no public read returns glyphs: the hit-test listing carries one bounds box per text block, never per glyph. Needed: a text read view of kind `layout` (glyph ids, advances and line boxes) on query.document.object.text' },
    { what: 'Media and text assets other than PNG, and fonts supplied as project assets', reason: 'only PNG assets and the bundled font are exercised; other codecs are unavailable by contract' },
  ],
  2113: [
    { what: 'Component evaluated-shape producers and rig producers in the pinned scene (D32, F16, I16a)', reason: 'no row builds a component or a rig; the mixed scene joins vector, gradient, text and raster producers only' },
    { what: 'The document evaluate read (the pinned evaluated layer values) of an object project', reason: 'query.document.evaluate answers "Unsupported object read operation." on an object project (it is an opacity-document operation); evaluated output is observed as export alpha on a transparent canvas and as the authored sample of the property read. Needed: an object-project evaluate query returning evaluated layer values at a pinned revision, context and frame' },
  ],
});

const rank = verdict => (verdict === 'pass' ? 2 : verdict === 'fail' ? 1 : 0);
// rows: the row RESULTS of the run (a row that was not selected is simply absent, which makes its ids never attempted).
function laneCoverage(lane, rows) {
  const assigned = ASSIGNED[lane] || [], mine = rows.filter(r => String(r.lane) === String(lane));
  const entries = NOT_COVERED[lane] || [];
  const coveredByRows = {}, attemptedNotPassed = {}, alsoNotPassed = {}, neverAttempted = [], status = {};
  for (const id of assigned) {
    const named = mine.filter(r => (r.tasks || []).includes(id));
    coveredByRows[id] = named.filter(r => r.verdict === 'pass').map(r => r.id);
    const failed = named.filter(r => r.verdict === 'fail');
    if (coveredByRows[id].length === 0 && failed.length > 0) attemptedNotPassed[id] = failed.map(r => ({ row: r.id, verdict: r.verdict, reason: r.reason || null }));
    // A covered id can still have a failing row (a behaviour of the id that does not hold): it is listed, never averaged away.
    const unpassed = named.filter(r => r.verdict !== 'pass' && r.verdict !== 'unrun');
    if (coveredByRows[id].length > 0 && unpassed.length > 0) alsoNotPassed[id] = unpassed.map(r => ({ row: r.id, verdict: r.verdict, reason: r.reason || null }));
    status[id] = coveredByRows[id].length > 0 ? 'covered' : failed.length > 0 ? 'attempted-not-passed' : 'never-attempted';
    if (status[id] === 'never-attempted') neverAttempted.push(id);
  }
  const listed = new Set(entries.filter(e => e.id).map(e => e.id));
  const uncoveredIds = assigned.filter(id => status[id] !== 'covered' || listed.has(id));
  return { assignedIds: assigned, status, coveredByRows, attemptedNotPassed, alsoNotPassed, neverAttempted,
    coveredIds: assigned.filter(id => status[id] === 'covered' && !listed.has(id)), uncoveredIds, notCovered: entries,
    laneComplete: uncoveredIds.length === 0 && entries.length === 0 };
}
function coverageTable(rows, lanes = Object.keys(ASSIGNED)) {
  const out = {};
  for (const lane of lanes) if (ASSIGNED[lane]) out[lane] = laneCoverage(lane, rows);
  return out;
}

module.exports = { ASSIGNED, NOT_COVERED, laneCoverage, coverageTable, rank };
