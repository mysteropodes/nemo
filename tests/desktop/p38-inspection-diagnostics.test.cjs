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
  schema: 'nemo.p38-inspection-completeness/1',
  flags: { complete: 'false', truncated: 'false', depthClipped: 'false', childrenUnreadable: 'true', foreignChildren: 'true' },
  nodeCount: 3, missingChildCount: 2,
  blocked: { categories: { 'child-owner': 1, 'children-unavailable': 1 }, absent: 1, unclassified: 0 },
  ownerQuery: { categories: { matched: 2, mismatch: 0, 'query-failed': 1 }, absent: 0, unclassified: 0 },
  childrenQuery: { categories: { success: 1, 'success-empty': 0, unsupported: 0, 'no-value': 1, failed: 0, 'wrong-type': 0 },
    absent: 1, unclassified: 0 },
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
