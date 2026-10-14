'use strict';
// P38 owns this stateless diagnostic projection and inspection-failure adapter.
// Inputs are already bounded helper facts; outputs contain only finite categories
// and counts. No filesystem, native/UI authority, persistent state or new reads.
// inspectProjectTabs wraps the existing observer port once; it never retries it.
const SCHEMA = 'nemo.p38-inspection-completeness/2';
const LIMIT = 4096;
const FLAGS = Object.freeze(['complete', 'truncated', 'depthClipped', 'childrenUnreadable', 'foreignChildren']);
const FLAG_STATES = Object.freeze(['absent', 'true', 'false', 'invalid']);
const BOUNDARIES = Object.freeze(['projected-nodes', 'paging-flags']);
const CATEGORIES = Object.freeze({
  blocked: Object.freeze(['child-owner', 'children-unavailable']),
  ownerQuery: Object.freeze(['matched', 'mismatch', 'query-failed']),
  childrenQuery: Object.freeze(['success', 'success-empty', 'unsupported', 'no-value', 'failed', 'wrong-type']),
  childrenCountQuery: Object.freeze(['success-zero', 'success-positive', 'unsupported', 'no-value', 'failed', 'invalid']),
  childrenWitness: Object.freeze(['empty-count-confirmed']),
});
const INSPECTION_CODES = Object.freeze(['inspection-budget-refused', 'inspection-transport-refused',
  'inspection-status-refused', 'inspection-identity-refused', 'inspection-projection-refused',
  'inspection-root-refused', 'inspection-completeness-refused', 'inspection-page-refused',
  'inspection-marker-refused', 'inspection-region-refused']);
const INSPECTION_FAILURE_CODES = Object.freeze(['helper-input-ended', 'helper-malformed',
  'inspection-refused', ...INSPECTION_CODES, 'ready-visible-state-refused', 'ready-window-pin-refused',
  'ready-ax-projection-refused', 'ready-phase-refused', 'ready-visible-unclassified']);
const record = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const count = (value, limit) => Number.isInteger(value) && value >= 0 && value <= limit;
function childrenWitnessFacts(node) {
  const result = {}, query = node.childrenCountQuery, witness = node.childrenWitness;
  const refused = () => { throw new Error('children-witness-refused'); };
  if (query !== undefined) {
    if (!CATEGORIES.childrenCountQuery.includes(query) || node.ownerQuery !== 'matched' ||
      node.childrenQuery !== 'no-value' || node.blocked === 'child-owner') refused();
    result.childrenCountQuery = query;
    result.ownerQuery = 'matched'; result.childrenQuery = 'no-value';
  }
  if (witness !== undefined) {
    if (witness !== 'empty-count-confirmed' || query !== 'success-zero' || node.ownerQuery !== 'matched' ||
      node.childrenQuery !== 'no-value' || node.childCount !== 0 || node.blocked !== undefined) refused();
    result.childrenWitness = witness;
  }
  if (query === 'success-zero' && witness !== 'empty-count-confirmed') refused();
  if (node.childrenQuery === 'no-value' && node.childCount !== undefined && witness !== 'empty-count-confirmed') refused();
  if (query !== undefined && query !== 'success-zero' && (witness !== undefined || node.childCount !== undefined)) refused();
  return result;
}

function categoryCounts(nodes, key) {
  const categories = Object.fromEntries(CATEGORIES[key].map(category => [category, 0]));
  let absent = 0, unclassified = 0;
  for (const node of nodes) {
    if (!Object.hasOwn(node, key)) absent++;
    else if (typeof node[key] === 'string' && Object.hasOwn(categories, node[key])) categories[node[key]]++;
    else unclassified++;
  }
  return { categories, absent, unclassified };
}

function summarizeCompleteness(evidence) {
  if (!record(evidence) || !Array.isArray(evidence.nodes) || evidence.nodes.length > LIMIT ||
    !evidence.nodes.every(record)) return undefined;
  const flags = Object.fromEntries(FLAGS.map(key => [key, !Object.hasOwn(evidence, key) ? 'absent' :
    evidence[key] === true ? 'true' : evidence[key] === false ? 'false' : 'invalid']));
  const result = { schema: SCHEMA, flags, nodeCount: evidence.nodes.length,
    missingChildCount: evidence.nodes.filter(node => !Number.isInteger(node.childCount)).length };
  for (const key of Object.keys(CATEGORIES)) result[key] = categoryCounts(evidence.nodes, key);
  return result;
}

// Reproject every layer's diagnostic input. Unknown keys/values never enter a
// receipt, and malformed counts cannot masquerade as a valid bounded summary.
function validateCompletenessSummary(value) {
  if (!record(value) || value.schema !== SCHEMA || !count(value.nodeCount, LIMIT) ||
    !count(value.missingChildCount, value.nodeCount) || !record(value.flags)) return undefined;
  const flags = {};
  for (const key of FLAGS) {
    if (!FLAG_STATES.includes(value.flags[key])) return undefined;
    flags[key] = value.flags[key];
  }
  const result = { schema: SCHEMA, flags, nodeCount: value.nodeCount, missingChildCount: value.missingChildCount };
  for (const [key, allowed] of Object.entries(CATEGORIES)) {
    const group = value[key];
    if (!record(group) || !record(group.categories) || !count(group.absent, value.nodeCount) ||
      !count(group.unclassified, value.nodeCount)) return undefined;
    const categories = {};
    let total = group.absent + group.unclassified;
    for (const category of allowed) {
      if (!count(group.categories[category], value.nodeCount)) return undefined;
      categories[category] = group.categories[category];
      total += categories[category];
    }
    if (total !== value.nodeCount) return undefined;
    result[key] = { categories, absent: group.absent, unclassified: group.unclassified };
  }
  if (Object.hasOwn(value, 'boundary')) {
    if (!BOUNDARIES.includes(value.boundary)) return undefined;
    result.boundary = value.boundary;
  }
  return result;
}

function requireCompleteness(ok, evidence, boundary) {
  if (ok) return;
  const error = Object.assign(new Error('inspection-completeness-refused'), { code: 'inspection-completeness-refused' });
  const summary = validateCompletenessSummary(evidence.completenessSummary);
  if (summary && BOUNDARIES.includes(boundary)) error.completeness = { ...summary, boundary };
  throw error;
}

function failureCompleteness(error) {
  if (error?.observerStage !== undefined && error.observerStage !== 'inspection') return {};
  const code = error?.observerStage === 'inspection' ? error.observerCode : error?.code;
  if (code !== 'inspection-completeness-refused') return {};
  const summary = validateCompletenessSummary(error.completeness);
  return summary?.boundary ? { completeness: summary } : {};
}

async function inspectProjectTabs(inspect, identity, state, owner) {
  try { return await owner.observeProjectTabs(inspect, identity, state); }
  catch (error) {
    const candidate = error?.observerStage === 'inspection' ? error.observerCode : error?.code;
    const allowed = error?.observerStage === 'inspection' ? INSPECTION_FAILURE_CODES : INSPECTION_CODES;
    const code = allowed.includes(candidate) ? candidate : 'inspection-refused';
    throw Object.assign(new Error('observer-failed'), { observerStage: 'inspection', observerCode: code,
      ...(code === 'inspection-completeness-refused' ? failureCompleteness(error) : {}) });
  }
}

module.exports = { SCHEMA, INSPECTION_CODES, INSPECTION_FAILURE_CODES, summarizeCompleteness, childrenWitnessFacts,
  validateCompletenessSummary, requireCompleteness, failureCompleteness, inspectProjectTabs };
