'use strict';
// Row selection and execution. A row is judged only from what it asserted: it passes when every assertion passed AND at least
// one assertion is an independent observation of the row's own (a read, a pixel or a file that is not kit bookkeeping), never
// on "the command returned ok" alone. Setup and the per-command revision and history deltas are bookkeeping and do not count.
const { createWire, EvidenceError } = require('./wire.cjs');
const K = require('./kit.cjs');

const LANE_MODULES = Object.freeze({
  2107: () => require('./lanes/lane-2107.cjs'), 2108: () => require('./lanes/lane-2108.cjs'),
  2112: () => require('./lanes/lane-2112.cjs'), 2113: () => require('./lanes/lane-2113.cjs'),
});
const LANES = Object.freeze(Object.keys(LANE_MODULES));

function allRows(modules = LANE_MODULES) {
  const out = [];
  for (const lane of Object.keys(modules)) {
    const module = modules[lane]();
    for (const row of module.rows) out.push({ ...row, lane: module.lane, issue: module.issue });
  }
  const ids = out.map(r => r.id);
  if (new Set(ids).size !== ids.length) throw new Error('duplicate row id');
  return out;
}

// lanes: array of lane ids (default all); rows: array of row ids (default every row of the chosen lanes). Unknown names refuse.
function selectRows({ lanes, rows } = {}, catalog = allRows()) {
  const known = [...new Set(catalog.map(r => r.lane))];
  if (lanes !== undefined) {
    if (!Array.isArray(lanes) || lanes.length === 0 || new Set(lanes).size !== lanes.length) throw new Error('lanes must be a non-empty list of unique lane ids');
    for (const lane of lanes) if (!known.includes(lane)) throw new Error('unknown lane ' + String(lane).slice(0, 20));
  }
  if (rows !== undefined) {
    if (!Array.isArray(rows) || rows.length === 0 || new Set(rows).size !== rows.length) throw new Error('rows must be a non-empty list of unique row ids');
    for (const id of rows) if (!catalog.some(r => r.id === id)) throw new Error('unknown row ' + String(id).slice(0, 60));
  }
  const chosen = catalog.filter(r => (lanes === undefined || lanes.includes(r.lane)) && (rows === undefined || rows.includes(r.id)));
  if (chosen.length === 0) throw new Error('the selection matches no row');
  const catalogRows = {};
  for (const row of catalog) catalogRows[row.lane] = (catalogRows[row.lane] || 0) + 1;
  return { rows: chosen, lanes: [...new Set(chosen.map(r => r.lane))], omitted: catalog.filter(r => !chosen.includes(r)).map(r => r.id), catalogRows };
}

const brief = error => String((error && error.message) || error).replace(/\s+/g, ' ').slice(0, 160);

// session: { wire, work, recheck }. Runs rows in order; a foreign instance or a lost transport stops the run.
async function runRows({ rows, session, now = () => Date.now() }) {
  const results = [];
  let aborted = null;
  // A row marked `last` can leave the app unusable (see its limitations): it runs after every other selected row.
  for (const row of [...rows.filter(r => !r.last), ...rows.filter(r => r.last)]) {
    const base = { id: row.id, lane: row.lane, issue: row.issue, title: row.title, tasks: row.tasks || [], limitations: row.limitations || [] };
    if (aborted) { results.push({ ...base, verdict: 'unrun', reason: 'run aborted: ' + aborted, operations: [], assertions: [], ms: 0 }); continue; }
    const ctx = K.createContext({ wire: session.wire, work: session.work, rowId: row.id });
    const mark = session.wire.mark(), started = now();
    let verdict = 'pass', reason = null;
    try {
      try { await session.recheck(); } catch (error) { throw new EvidenceError('foreign', 'before the row: ' + brief(error)); }
      await row.run(ctx);
      try { await session.recheck(); } catch (error) { throw new EvidenceError('foreign', 'after the row: ' + brief(error)); }
      if (ctx.assertions.length === 0) { verdict = 'fail'; reason = 'the row made no assertion'; }
      else if (!ctx.assertions.some(a => a.kind !== 'reply' && !a.bookkeeping)) { verdict = 'fail'; reason = 'the row made no independent observation (read, pixel or file) beyond setup and command bookkeeping'; }
    } catch (error) {
      if (error instanceof K.AssertionFailure) { verdict = 'fail'; reason = error.message; }
      else if (error instanceof EvidenceError && error.kind === 'unavailable') { verdict = 'unrun'; reason = brief(error); }
      else if (error instanceof EvidenceError && (error.kind === 'foreign' || error.kind === 'transport')) { verdict = 'fail'; reason = error.kind + ': ' + brief(error); aborted = reason; }
      else if (error instanceof EvidenceError) { verdict = 'fail'; reason = error.kind + ': ' + brief(error); }
      else { verdict = 'fail'; reason = 'row error (' + (error && error.name) + '): ' + brief(error); }
    }
    results.push({ ...base, verdict, reason, operations: session.wire.since(mark), assertions: ctx.assertions, ms: now() - started });
  }
  return results;
}

module.exports = { LANES, allRows, selectRows, runRows, createWire };
