'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const diagnostics = require('./p38-inspection-diagnostics.cjs');
const { safeWindowEvidence } = require('./p38-installed-driver.cjs');
const semantic = require('./p38-fixture-open-installed.cjs');
const { runObserver, writeOutcome } = require('./p38-fixture-open-owner.cjs');

function rawEvidence() {
  return { status: 'ok', type: 'inspect', pid: 42, windowId: 9, matchedAX: true,
    complete: false, truncated: false, depthClipped: false, childrenUnreadable: true, foreignChildren: true,
    private: 'PRIVATE-RAW-SENTINEL', token: 'PRIVATE-TOKEN-SENTINEL', nodes: [
      { path: '', role: 'AXWindow', childCount: 2, ownerQuery: 'matched', childrenQuery: 'success' },
      { path: '0', role: 'other', blocked: 'child-owner', ownerQuery: 'query-failed', value: 'PRIVATE-VALUE-SENTINEL' },
      { path: '1', role: 'other', blocked: 'children-unavailable', ownerQuery: 'matched', childrenQuery: 'no-value',
        title: 'PRIVATE-TITLE-SENTINEL', privatePath: '/PRIVATE-PATH-SENTINEL' },
    ] };
}
// Independent literal oracle: three nodes, two missing counts, one blocked node
// of each kind. The actual helper/source is not used to regenerate these values.
const expected = {
  schema: 'nemo.p38-inspection-completeness/2',
  flags: { complete: 'false', truncated: 'false', depthClipped: 'false', childrenUnreadable: 'true', foreignChildren: 'true' },
  nodeCount: 3, missingChildCount: 2,
  blocked: { categories: { 'child-owner': 1, 'children-unavailable': 1 }, absent: 1, unclassified: 0 },
  ownerQuery: { categories: { matched: 2, mismatch: 0, 'query-failed': 1 }, absent: 0, unclassified: 0 },
  childrenQuery: { categories: { success: 1, 'success-empty': 0, unsupported: 0, 'no-value': 1, failed: 0, 'wrong-type': 0 },
    absent: 1, unclassified: 0 },
  childrenCountQuery: { categories: { 'success-zero': 0, 'success-positive': 0, unsupported: 0, 'no-value': 0, failed: 0, invalid: 0 },
    absent: 3, unclassified: 0 },
  childrenWitness: { categories: { 'empty-count-confirmed': 0 }, absent: 3, unclassified: 0 },
};

test('opt-in sanitizer preserves only bounded categorical totals and leaves default consumers unchanged', () => {
  const raw = rawEvidence(), clean = safeWindowEvidence(raw, false, true);
  assert.deepEqual(clean.completenessSummary, expected);
  assert.equal(Object.hasOwn(safeWindowEvidence(raw), 'completenessSummary'), false);
  assert.equal(Object.hasOwn(safeWindowEvidence(raw, true, true), 'completenessSummary'), false);
  assert.equal(Object.hasOwn(clean.nodes[1], 'ownerQuery'), false);
  assert.doesNotMatch(JSON.stringify(clean.completenessSummary), /PRIVATE|"(?:path|title|value|pid|windowId|token)"/);
  assert.deepEqual(diagnostics.validateCompletenessSummary(clean.completenessSummary), expected);
});

test('flag presence and values are finite; unexpected node categories become counts, never copied strings', () => {
  const raw = rawEvidence();
  delete raw.foreignChildren;
  raw.complete = 'PRIVATE-FLAG-SENTINEL';
  raw.nodes[1].ownerQuery = 'PRIVATE-OWNER-SENTINEL';
  raw.nodes[1].childrenQuery = { private: 'PRIVATE-CHILD-SENTINEL' };
  raw.nodes[1].blocked = '__proto__';
  const result = diagnostics.summarizeCompleteness(raw);
  assert.equal(result.flags.foreignChildren, 'absent');
  assert.equal(result.flags.complete, 'invalid');
  assert.deepEqual(result.ownerQuery, { categories: { matched: 2, mismatch: 0, 'query-failed': 0 }, absent: 0, unclassified: 1 });
  assert.equal(result.childrenQuery.absent, 0);
  assert.equal(result.childrenQuery.unclassified, 1);
  assert.deepEqual(result.blocked, { categories: { 'child-owner': 0, 'children-unavailable': 1 }, absent: 1, unclassified: 1 });
  assert.doesNotMatch(JSON.stringify(result), /PRIVATE|__proto__/);
});

test('every existing helper query category has a separate literal count', () => {
  for (const value of ['matched', 'mismatch', 'query-failed']) {
    const raw = rawEvidence(); raw.nodes[1].ownerQuery = value;
    const result = safeWindowEvidence(raw, false, true).completenessSummary;
    assert.equal(result.ownerQuery.categories[value], value === 'matched' ? 3 : 1);
    assert.equal(result.ownerQuery.absent, 0);
    assert.equal(result.ownerQuery.unclassified, 0);
  }
  for (const value of ['success', 'success-empty', 'unsupported', 'no-value', 'failed', 'wrong-type']) {
    const raw = rawEvidence(); raw.nodes[2].childrenQuery = value;
    const result = safeWindowEvidence(raw, false, true).completenessSummary;
    assert.equal(result.childrenQuery.categories[value], value === 'success' ? 2 : 1);
    assert.equal(result.childrenQuery.absent, 1);
    assert.equal(result.childrenQuery.unclassified, 0);
  }
});

test('summary validator independently rejects malformed counts and flags and strips extra private fields', () => {
  for (const mutate of [
    x => x.schema = 'PRIVATE-SCHEMA-SENTINEL', x => x.nodeCount = 4097, x => x.nodeCount = 1.5,
    x => x.missingChildCount = -1, x => x.missingChildCount = 4, x => delete x.flags.complete,
    x => x.flags.complete = 'PRIVATE-FLAG-SENTINEL', x => x.ownerQuery.categories.matched = 1,
    x => x.childrenQuery.absent = 3, x => x.blocked.unclassified = 'PRIVATE-COUNT-SENTINEL',
    x => delete x.ownerQuery.categories.mismatch, x => x.boundary = '/PRIVATE-BOUNDARY-SENTINEL',
  ]) {
    const bad = structuredClone(expected); mutate(bad);
    assert.equal(diagnostics.validateCompletenessSummary(bad), undefined);
  }
  const extra = structuredClone(expected);
  extra.private = 'PRIVATE-SUMMARY-SENTINEL'; extra.flags.private = 'PRIVATE-FLAG-SENTINEL';
  extra.ownerQuery.categories.private = 'PRIVATE-QUERY-SENTINEL';
  assert.deepEqual(diagnostics.validateCompletenessSummary(extra), expected);
  for (const bad of [null, [], {}, { nodes: Array(4097).fill({}) }, { nodes: [null] }])
    assert.equal(diagnostics.summarizeCompleteness(bad), undefined);
  const maximum = diagnostics.summarizeCompleteness({ nodes: Array.from({ length: 4096 }, () => ({})) });
  assert.equal(maximum.nodeCount, 4096);
  assert.equal(maximum.missingChildCount, 4096);
  assert.ok(diagnostics.validateCompletenessSummary(maximum));
});

async function actualRefusal(raw) {
  let reads = 0;
  try {
    await diagnostics.inspectProjectTabs(async () => { reads++; return raw; }, { pid: 42, windowId: 9 },
      { actions: 0 }, semantic);
    assert.fail('inspection must refuse');
  } catch (error) {
    return { error, reads };
  }
}

test('actual semantic inspection refuses each original completeness subcondition with safe evidence', async () => {
  const changes = [
    x => x.foreignChildren = true, x => delete x.foreignChildren,
    x => x.childrenUnreadable = true, x => delete x.childrenUnreadable,
    x => x.nodes[0].blocked = 'child-owner', x => x.nodes[0].blocked = 'children-unavailable',
    x => delete x.nodes[0].childCount,
  ];
  for (const mutate of changes) {
    const raw = { status: 'ok', type: 'inspect', pid: 42, windowId: 9, matchedAX: true,
      complete: true, truncated: false, depthClipped: false, childrenUnreadable: false, foreignChildren: false,
      nodes: [{ path: '', role: 'AXWindow', childCount: 0, ownerQuery: 'matched', childrenQuery: 'success-empty' }] };
    mutate(raw);
    const { error, reads } = await actualRefusal(raw);
    assert.equal(reads, 1);
    assert.equal(error.observerCode, 'inspection-completeness-refused');
    assert.equal(error.completeness.boundary, 'projected-nodes');
    assert.equal(error.completeness.nodeCount, 1);
    assert.doesNotMatch(JSON.stringify(error), /PRIVATE|"(?:path|pid|windowId|token)"/);
  }
});

test('the separate paging-shape refusal is distinguished without reading a page', async () => {
  const raw = { status: 'ok', type: 'inspect', pid: 42, windowId: 9, matchedAX: true,
    truncated: false, depthClipped: false, childrenUnreadable: false, foreignChildren: false,
    nodes: [{ path: '', role: 'AXWindow', childCount: 0 }] };
  const { error, reads } = await actualRefusal(raw);
  assert.equal(reads, 1);
  assert.equal(error.observerCode, 'inspection-completeness-refused');
  assert.equal(error.completeness.boundary, 'paging-flags');
  assert.equal(error.completeness.flags.complete, 'absent');
});

function observerServices(directory, cleanupFails = false) {
  const config = { taskId: 'p38-diagnostic-task', grantId: 'p38-diagnostic-grant', sourceHead: 'b'.repeat(40),
    helperPath: '/tmp/p38-diagnostic-helper', helperSha256: 'e'.repeat(64), timeoutMs: 10000 };
  const manifest = { schema: 'nemo.p38-fixture-open/1', runId: '12345678-1234-1234-1234-123456789abc',
    taskKey: 'a'.repeat(64), sourceSha: config.sourceHead, buildSha: config.sourceHead, pid: 42, window: 'main',
    origin: 'tauri://localhost/index.html', fixtures: [{ id: 'A' }, { id: 'B' }], manifestSha256: 'c'.repeat(64),
    waiting: true, failed: false, checkpoint: 0 };
  const calls = { reads: 0, checkpoints: 0, acknowledgments: 0, captures: 0, closes: 0 }, saved = [];
  let now = 1000;
  const services = {
    now: () => now++, taskKey: () => manifest.taskKey,
    waitForDriver: async () => ({ schema: 'nemo.p38-operator/1', grant: config.grantId,
      task: config.taskId, source: config.sourceHead, appPid: 42, windowId: 9 }),
    readOwner: async () => ({ taskId: config.taskId, ownerToken: 'd'.repeat(32) }),
    waitForProbe: async () => ({ root: '/tmp/p38-fixture-open-diagnostic', manifest }),
    waitForCheckpoint: async () => { calls.checkpoints++; return manifest; },
    owner: {
      receipt: () => ({ value: manifest }),
      visibleEvidence: () => assert.fail('visibility after first inspection refusal'),
      acknowledge: () => { calls.acknowledgments++; assert.fail('ACK after first inspection refusal'); },
    },
    startHelper: async () => ({
      next: async () => ({ status: 'pinned', pid: 42, windowId: 9, matchedAX: true }),
      inspect: identity => diagnostics.inspectProjectTabs(async () => { calls.reads++; return rawEvidence(); },
        identity, { actions: 0 }, semantic),
      capture: async () => { calls.captures++; assert.fail('capture after first inspection refusal'); },
      quit: async () => assert.fail('success quit after first inspection refusal'),
      close: async () => { calls.closes++; if (cleanupFails) throw new Error('PRIVATE-CLEANUP-SENTINEL'); },
    }),
    save: value => { saved.push(structuredClone(value)); writeOutcome(directory, value); },
  };
  return { config, services, calls, saved };
}

test('actual sanitizer/semantic/helper-adapter/owner path durably retains the first safe failure before cleanup', async () => {
  for (const cleanupFails of [false, true]) {
    const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'p38-diagnostic-receipt-')));
    try {
      const fixture = observerServices(directory, cleanupFails);
      const result = await runObserver(fixture.config, fixture.services);
      const persisted = JSON.parse(fs.readFileSync(path.join(directory, 'owner-observation.json')));
      assert.equal(result.firstFailure.code, 'inspection-completeness-refused');
      assert.equal(result.firstFailure.stage, 'inspection');
      assert.deepEqual(persisted.firstFailure.completeness, { ...expected, boundary: 'projected-nodes' });
      assert.deepEqual(fixture.saved[0].firstFailure, persisted.firstFailure);
      assert.deepEqual(fixture.calls, { reads: 1, checkpoints: 1, acknowledgments: 0, captures: 0, closes: 1 });
      assert.equal(result.observations.length, 0); assert.equal(result.captures.length, 0);
      assert.equal(result.cleanup, cleanupFails ? 'incomplete' : 'complete');
      assert.equal(result.installedAcceptance, 'not-established');
      assert.doesNotMatch(JSON.stringify(persisted), /PRIVATE/);
    } finally { fs.rmSync(directory, { recursive: true, force: true }); }
  }
});

test('failure adapters reject private or malformed diagnostics without changing the terminal code', async () => {
  for (const completeness of [null, { private: 'PRIVATE-ERROR-SENTINEL' }, { ...expected, nodeCount: -1 },
    { ...expected, boundary: 'PRIVATE-BOUNDARY-SENTINEL' }]) {
    const original = Object.assign(new Error('PRIVATE-EXCEPTION-SENTINEL'), {
      code: 'inspection-completeness-refused', completeness, private: 'PRIVATE-ERROR-SENTINEL' });
    assert.deepEqual(diagnostics.failureCompleteness(original), {});
    await assert.rejects(diagnostics.inspectProjectTabs(async () => {}, {}, {}, {
      observeProjectTabs: async () => { throw original; },
    }), error => {
      assert.equal(error.observerCode, 'inspection-completeness-refused');
      assert.equal(Object.hasOwn(error, 'completeness'), false);
      assert.doesNotMatch(JSON.stringify(error), /PRIVATE/);
      return true;
    });
  }
  const safe = { ...expected, boundary: 'projected-nodes', private: 'PRIVATE-EXTRA-SENTINEL' };
  for (const original of [{ code: 'PRIVATE-ERROR-SENTINEL', completeness: safe },
    { observerStage: 'checkpoint', observerCode: 'inspection-completeness-refused', completeness: safe }])
    assert.deepEqual(diagnostics.failureCompleteness(original), {});
  assert.deepEqual(diagnostics.failureCompleteness({ code: 'inspection-completeness-refused', completeness: safe }),
    { completeness: { ...expected, boundary: 'projected-nodes' } });
  assert.doesNotThrow(() => diagnostics.requireCompleteness(true, { private: 'PRIVATE' }, 'projected-nodes'));
});
test('P38 and Reference preserve raw no-value only with the exact same owned zero-count witness', async () => {
  const node = { path: '0', role: 'AXGroup', control: 'Project tabs', ownerQuery: 'matched', childrenQuery: 'no-value',
    childrenCountQuery: 'success-zero', childrenWitness: 'empty-count-confirmed', childCount: 0 };
  const raw = { status: 'ok', type: 'inspect', pid: 42, windowId: 9, matchedAX: true,
    complete: true, truncated: false, depthClipped: false, childrenUnreadable: false, foreignChildren: false, nodes: [node] };
  for (const reference of [false, true]) {
    const first = safeWindowEvidence(raw, reference);
    assert.equal(first.nodes[0].childrenQuery, 'no-value');
    assert.equal(first.nodes[0].childrenWitness, 'empty-count-confirmed');
    assert.deepEqual(safeWindowEvidence(first, reference), first); // Actual repeated projection stays valid.
  }
  for (const mutate of [x => delete x.childrenWitness, x => delete x.childrenCountQuery,
    x => x.childrenCountQuery = 'success-positive', x => x.childrenCountQuery = 'no-value',
    x => x.ownerQuery = 'mismatch', x => x.ownerQuery = 'query-failed', x => x.childrenQuery = 'success-empty',
    x => x.childCount = 1, x => x.blocked = 'children-unavailable', x => x.childrenWitness = 'PRIVATE-WITNESS']) {
    const bad = structuredClone(raw); mutate(bad.nodes[0]);
    assert.throws(() => safeWindowEvidence(bad), /ax-evidence-refused/);
  }
  const snapshot = await semantic.observeProjectTabs(async request => request.projectTabs ?
    { ...raw, projectTabs: { path: '0', token: '12345678-1234-1234-1234-123456789abc' } } :
    { ...raw, nodes: [{ path: '', role: 'AXWindow', childCount: 1 }, node] }, { pid: 42, windowId: 9 });
  assert.doesNotThrow(() => semantic.visibleEvidence(snapshot, { phase: 'ready', pid: 42, windowId: 9 }));
  const summary = safeWindowEvidence(raw, false, true).completenessSummary;
  assert.equal(summary.childrenQuery.categories['no-value'], 1);
  assert.equal(summary.childrenCountQuery.categories['success-zero'], 1);
  assert.equal(summary.childrenWitness.categories['empty-count-confirmed'], 1);
  assert.equal(summary.missingChildCount, 0);
  assert.equal(diagnostics.validateCompletenessSummary({ ...summary, schema: 'nemo.p38-inspection-completeness/1' }), undefined);
});

// Independent adjacency-list oracle and faithful bounded depth-first wire.
// The oracle never invokes the collector to generate expected paths/counts.
function projectionWire(graph, mutate = () => {}) {
  let cursor;
  const observed = new Set(['']), requests = [];
  const wire = request => {
    const base = request.element || '', page = request.childOffset !== undefined, offset = request.childOffset || 0;
    assert.ok(observed.has(base), 'only an explicitly observed root may be requested');
    assert.equal(request.maxNodes, 128); assert.equal(request.depth, page ? 1 : 5);
    assert.ok(graph.has(base)); requests.push({ ...request });
    const root = graph.get(base), nodes = []; let truncated = false, depthClipped = false;
    if (page) {
      if (!offset) { assert.equal(request.pageToken, undefined); cursor = { base, next: 0, clipped: false, children: [...root.children] }; }
      else { assert.equal(request.pageToken, '12345678-1234-1234-1234-123456789abc'); assert.equal(cursor.base, base);
        assert.equal(cursor.next, offset); assert.deepEqual(cursor.children, root.children); }
    } else cursor = undefined;
    function visit(route, level) {
      const value = graph.get(route);
      nodes.push({ path: route, role: route ? 'AXGroup' : 'AXWindow', childCount: value.children.length, ...value.facts });
      if (level === request.depth) { depthClipped ||= value.children.length > 0; return; }
      for (const child of value.children.slice(level === 0 ? offset : 0)) {
        if (nodes.length === 128) { truncated = true; return; }
        visit(child, level + 1);
      }
    }
    visit(base, 0);
    let paging = {};
    if (page) { cursor.clipped ||= depthClipped; depthClipped = cursor.clipped; cursor.next = offset + nodes.length - 1;
      paging = { childOffset: offset, nextChildOffset: cursor.next, childCount: root.children.length,
        pageToken: '12345678-1234-1234-1234-123456789abc', childrenComplete: cursor.next === root.children.length }; }
    const raw = { status: 'ok', type: 'inspect', pid: 42, windowId: 9, matchedAX: true, nodes,
      complete: !truncated && !depthClipped, truncated, depthClipped, childrenUnreadable: false, foreignChildren: false,
      private: 'PRIVATE-RAW-SENTINEL', ...paging,
      ...(request.projectTabs ? { projectTabs: { path: base, token: '87654321-1234-1234-1234-123456789abc' } } : {}) };
    mutate(raw, request); for (const node of raw.nodes) observed.add(node.path);
    return raw;
  };
  return Object.assign(wire, { observed, requests });
}
function chain(graph, base, length) {
  for (let i = 0, route = base; i < length; i++, route += '.0')
    graph.set(route, { children: i + 1 < length ? [route + '.0'] : [] });
}
function budgetWire(boundary) {
  const graph = new Map(), width = boundary === 'read-action-cap' ? 41 : 4095;
  graph.set('', { children: Array.from({ length: width }, (_, i) => String(i)) });
  for (let i = 0; i < width; i++) chain(graph, String(i), boundary === 'read-action-cap' ? 31 : i === 0 ? 2 : 1);
  return projectionWire(graph);
}

function acceptanceGraph(kind, phase) {
  const graph = new Map(), marker = kind === 'deep' ? '0' : '129';
  graph.set('', { children: kind === 'deep' ? ['0', '1', '2'] : Array.from({ length: 130 }, (_, i) => String(i)) });
  if (kind === 'deep') { chain(graph, '1', 31); graph.set('2', { children: [] }); }
  else for (let i = 0; i < 130; i++) graph.set(String(i), { children: [] });
  const rows = phase === 'ready' ? [] : phase === 'b-opened' ? ['A', 'B'] : ['A'];
  graph.set(marker, { children: rows.map((_, i) => marker + '.' + i), facts: { control: 'Project tabs' } });
  for (const [i, control] of rows.entries()) graph.set(marker + '.' + i,
    { children: [], facts: { control, selected: i === 0 } });
  if (phase === 'b-opened') graph.get(kind === 'deep' ? '2' : '0').facts = { alert: 'switch-unavailable' };
  return graph;
}

test('independent deep and broad graph oracles close all four observations and count three captures within63', async () => {
  for (const [kind, expectedReads] of [['deep', [8, 8, 8, 8]], ['broad', [4, 5, 5, 5]]]) {
    const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'p38-frontier-oracle-')));
    try {
      const f = observerServices(directory), state = { actions: 0 }, wires = [], events = [];
      let phase, wire, graph;
      const checkpoint = f.services.waitForCheckpoint, receipt = f.services.owner.receipt;
      f.services.waitForCheckpoint = async (...args) => {
        phase = args[3]; graph = acceptanceGraph(kind, phase); wire = projectionWire(graph);
        wires.push(wire); return { ...await checkpoint(...args), checkpoint: args[4] };
      };
      f.services.owner.receipt = () => ({ value: { ...receipt().value,
        checkpoint: ['ready', 'a-opened', 'b-opened', 'b-closed'].indexOf(phase || 'ready') } });
      f.services.owner.visibleEvidence = semantic.visibleEvidence;
      f.services.owner.acknowledge = (_root, expected, _token, decision, snapshot) => {
        assert.equal(decision, 'accepted'); semantic.visibleEvidence(snapshot, expected);
        events.push('ack:' + expected.phase); f.calls.acknowledgments++;
      };
      f.services.startHelper = async () => ({
        next: async () => ({ status: 'pinned', pid: 42, windowId: 9, matchedAX: true }),
        inspect: identity => diagnostics.inspectProjectTabs(request => {
          if (request.projectTabs) {
            assert.deepEqual([...wire.observed].sort(), [...graph.keys()].sort(), 'no scoped acceptance before the ENTIRE oracle graph was observed');
            assert.equal(request.projectTabsToken, phase === 'ready' ? undefined : '87654321-1234-1234-1234-123456789abc');
          }
          f.calls.reads++; return wire(request);
        }, identity, state, semantic),
        capture: async (window, index) => {
          assert.ok(state.actions++ < 63); f.calls.captures++; events.push('capture:' + phase);
          return { phase, pid: window.pid, windowId: window.windowId, capture: 'capture-' + (index + 1) + '.png',
            sha256: 'e'.repeat(64), width: 800, height: 600 };
        },
        quit: async () => { assert.ok(state.actions + 1 <= 64); events.push('quit'); },
        close: async () => { f.calls.closes++; },
      });
      const outcome = await runObserver(f.config, f.services);
      assert.equal(outcome.status, 'observed'); assert.equal(outcome.firstFailure, null);
      assert.deepEqual(wires.map(w => w.requests.length), expectedReads);
      assert.equal(state.actions, expectedReads.reduce((a, b) => a + b, 0) + 3);
      assert.ok(state.actions <= 63); assert.equal(outcome.observations.length, 4); assert.equal(outcome.captures.length, 3);
      assert.deepEqual(events, ['ack:ready', 'capture:a-opened', 'ack:a-opened', 'capture:b-opened', 'ack:b-opened',
        'capture:b-closed', 'ack:b-closed', 'quit']);
      assert.equal(outcome.cleanup, 'complete'); assert.equal(outcome.installedAcceptance, 'not-established');
    } finally { fs.rmSync(directory, { recursive: true, force: true }); }
  }
});

test('malformed partial topology and changed overlaps refuse before scoped acceptance', async () => {
  const faults = {
    alias: raw => { raw.nodes[1].path = '00'; },
    orphan: raw => { raw.nodes[1].path = '0.0'; },
    duplicate: raw => { raw.nodes[2].path = raw.nodes[1].path; },
    missing: raw => { raw.nodes.splice(2, 1); },
    overlap: (raw, request) => { if (request.childOffset !== undefined) raw.nodes[0].control = 'A'; },
  };
  for (const [fault, mutate] of Object.entries(faults)) {
    const wire = projectionWire(acceptanceGraph('broad', 'ready'), mutate);
    await assert.rejects(diagnostics.inspectProjectTabs(wire, { pid: 42, windowId: 9 }, { actions: 0 }, semantic),
      error => { assert.equal(error.observerCode, 'inspection-page-refused', fault); return true; });
    assert.equal(wire.requests.some(r => r.projectTabs), false, fault);
  }
});

test('a new observation rediscovers topology and refuses a duplicate marker beyond the first partial read', async () => {
  const state = { actions: 0 }, first = projectionWire(acceptanceGraph('broad', 'ready'));
  await diagnostics.inspectProjectTabs(first, { pid: 42, windowId: 9 }, state, semantic);
  const graph = acceptanceGraph('broad', 'ready'); graph.get('128').facts = { control: 'Project tabs' };
  const second = projectionWire(graph);
  await assert.rejects(diagnostics.inspectProjectTabs(second, { pid: 42, windowId: 9 }, state, semantic),
    error => { assert.equal(error.observerCode, 'inspection-marker-refused'); return true; });
  assert.deepEqual([...second.observed].sort(), [...graph.keys()].sort());
  assert.equal(second.requests[0].element, ''); assert.equal(second.requests.some(r => r.childOffset === 127), true);
  assert.equal(second.requests.some(r => r.projectTabs), false);
  assert.equal(state.token, '87654321-1234-1234-1234-123456789abc');
});

test('an infeasible mixed graph still refuses at63 without scoped acceptance or a cap increase', async () => {
  const graph = new Map([['', { children: Array.from({ length: 40 }, (_, i) => String(i)) }]]);
  for (let i = 0; i < 40; i++) {
    graph.set(String(i), { children: Array.from({ length: 8 }, (_, j) => i + '.' + j), facts: i ? {} : { control: 'Project tabs' } });
    for (let j = 0; j < 8; j++) graph.set(i + '.' + j, { children: [] });
  }
  const state = { actions: 0 };
  for (let pass = 0; pass < 3; pass++) {
    const wire = projectionWire(graph);
    if (pass < 2) {
      const result = await diagnostics.inspectProjectTabs(wire, { pid: 42, windowId: 9 }, state, semantic);
      assert.equal(result.projectTabs.unique, true); assert.deepEqual([...wire.observed].sort(), [...graph.keys()].sort());
      if (pass) state.actions++; // Count the prior successful capture slot.
    } else await assert.rejects(diagnostics.inspectProjectTabs(wire, { pid: 42, windowId: 9 }, state, semantic), error => {
      assert.equal(error.observerCode, 'inspection-budget-refused'); assert.equal(error.budget.sharedActionsUsed, 63);
      assert.equal(wire.requests.some(r => r.projectTabs), false); return true;
    });
  }
});

test('actual budget producer, adapter and persisted owner distinguish both unchanged caps without a later effect', async () => {
  for (const [boundary, reads, collectedNodes, priorActions] of [
    ['read-action-cap', 63, 448, 0], ['collected-node-cap', 34, 4097, 0], ['collected-node-cap', 34, 4097, 2],
  ]) {
    for (const cleanupFails of [false, true]) {
      const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'p38-budget-receipt-')));
      try {
        const f = observerServices(directory, cleanupFails), state = { actions: priorActions }, wire = budgetWire(boundary);
        f.services.startHelper = async () => ({
          next: async () => ({ status: 'pinned', pid: 42, windowId: 9, matchedAX: true }),
          inspect: identity => diagnostics.inspectProjectTabs(request => { f.calls.reads++; return wire(request); }, identity, state, semantic),
          capture: async () => { f.calls.captures++; assert.fail('capture after budget refusal'); },
          quit: async () => assert.fail('quit after budget refusal'),
          close: async () => { f.calls.closes++; if (cleanupFails) throw Error('PRIVATE-CLEANUP'); },
        });
        const outcome = await runObserver(f.config, f.services);
        const saved = JSON.parse(fs.readFileSync(path.join(directory, 'owner-observation.json')));
        assert.equal(saved.firstFailure.code, 'inspection-budget-refused');
        assert.deepEqual(saved.firstFailure.budget, { schema: 'nemo.p38-inspection-budget/1', boundary,
          sharedActionsUsed: reads + priorActions, collectedNodes });
        assert.deepEqual(f.saved[0].firstFailure, saved.firstFailure);
        assert.deepEqual(f.calls, { reads, checkpoints: 1, acknowledgments: 0, captures: 0, closes: 1 });
        assert.equal(outcome.cleanup, cleanupFails ? 'incomplete' : 'complete');
        assert.deepEqual(outcome.observations, []); assert.deepEqual(outcome.captures, []);
        assert.equal(outcome.installedAcceptance, 'not-established');
        assert.doesNotMatch(JSON.stringify(saved), /PRIVATE/);
      } finally { fs.rmSync(directory, { recursive: true, force: true }); }
    }
  }
});

test('the pre-existing shared cap refuses before another read and reports the pre-increment count', async () => {
  let reads = 0;
  const state = { actions: 63 };
  await assert.rejects(diagnostics.inspectProjectTabs(() => { reads++; }, { pid: 42, windowId: 9 }, state, semantic), error => {
    assert.deepEqual(error.budget, { schema: 'nemo.p38-inspection-budget/1', boundary: 'read-action-cap',
      sharedActionsUsed: 63, collectedNodes: 0 });
    return true;
  });
  assert.equal(reads, 0); assert.equal(state.actions, 64, 'existing post-increment semantics remain unchanged');
  assert.doesNotThrow(() => diagnostics.requireBudget(true, 'PRIVATE', -1, Infinity));
});

test('budget metadata stays finite, branch-consistent and inspection-code scoped through every adapter', async () => {
  const good = { schema: 'nemo.p38-inspection-budget/1', boundary: 'read-action-cap', sharedActionsUsed: 63, collectedNodes: 4096 };
  for (const bad of [null, {}, { ...good, schema: 'future' }, { ...good, boundary: 'PRIVATE' },
    { ...good, sharedActionsUsed: 62 }, { ...good, sharedActionsUsed: 64 }, { ...good, collectedNodes: 4097 },
    ...[-1, 0.5, Infinity, NaN, '63', 1e20].map(value => ({ ...good, sharedActionsUsed: value })),
    ...[-1, 0.5, Infinity, NaN, '4096', 4098].map(value => ({ ...good, collectedNodes: value })),
    { ...good, boundary: 'collected-node-cap', collectedNodes: 4096 }]) {
    assert.equal(diagnostics.validateBudget(bad), undefined);
    const original = { code: 'inspection-budget-refused', budget: bad, private: 'PRIVATE' };
    assert.deepEqual(diagnostics.failureBudget(original), {});
    await assert.rejects(diagnostics.inspectProjectTabs(() => {}, {}, {}, {
      observeProjectTabs: async () => { throw original; },
    }), error => error.observerCode === 'inspection-budget-refused' && !Object.hasOwn(error, 'budget'));
  }
  for (const budget of [good, { ...good, boundary: 'collected-node-cap', sharedActionsUsed: 0, collectedNodes: 4097 },
    { ...good, boundary: 'collected-node-cap', collectedNodes: 4097 }]) {
    const original = { code: 'inspection-budget-refused', budget: { ...budget, token: 'PRIVATE' }, message: 'PRIVATE' };
    assert.deepEqual(diagnostics.failureBudget(original), { budget });
    await assert.rejects(diagnostics.inspectProjectTabs(() => {}, {}, {}, { observeProjectTabs: async () => { throw original; } }), error => {
      assert.deepEqual(error.budget, budget); assert.deepEqual(diagnostics.failureBudget(error), { budget });
      assert.doesNotMatch(JSON.stringify(error), /PRIVATE/); return true;
    });
    for (const other of [{ ...original, code: 'inspection-refused' },
      { ...original, observerStage: 'capture', observerCode: 'inspection-budget-refused' }])
      assert.deepEqual(diagnostics.failureBudget(other), {});
  }
});
