'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { reconcile, run, MARKER } = require('./collaborator-pr-policy.cjs');
const SHA = 'a'.repeat(40);
const repo = { owner: 'mysteropodes', repo: 'nemo' };

function fixture(options = {}) {
  const pr = { number: 42, state: 'open', draft: false, base: { ref: 'main' },
    head: { sha: SHA }, user: { login: 'developer', type: 'User' }, ...options.pr };
  const reviews = options.reviews || [];
  const calls = { approvals: [], dismissals: [], logs: [], permission: 0, reads: 0 };
  const github = {
    rest: {
      repos: { getCollaboratorPermissionLevel: async () => {
        calls.permission++;
        if (options.permissionError) throw options.permissionError;
        return { data: { permission: options.permissionAfter && calls.permission > 1 ?
          options.permissionAfter : options.permission || 'write' } };
      } },
      pulls: {
        get: async () => {
          calls.reads++;
          return { data: calls.reads > 1 && options.prAfter ? { ...pr, ...options.prAfter } : pr };
        },
        listReviews: 'reviews', list: 'pulls',
        createReview: async value => {
          calls.approvals.push(value);
          reviews.push({ id: 100, user: { login: 'github-actions[bot]' },
            state: 'APPROVED', body: value.body, commit_id: value.commit_id });
          if (options.ambiguousWrite) throw new Error('Connection lost after server write');
        },
        dismissReview: async value => { calls.dismissals.push(value); },
      },
    },
    paginate: async method => {
      if (options.reviewError && method === 'reviews') throw new Error('Reviews unavailable');
      return method === 'reviews' ? reviews : [pr];
    },
  };
  return { github, calls, invoke: () => reconcile({ github, repo, number: 42,
    log: message => calls.logs.push(message) }) };
}

for (const permission of ['write', 'maintain', 'admin']) {
  test(`${permission} collaborator gets an exact-head policy acknowledgement`, async () => {
    const f = fixture({ permission });
    await f.invoke();
    assert.equal(f.calls.approvals.length, 1);
    assert.equal(f.calls.approvals[0].commit_id, SHA);
    assert.match(f.calls.approvals[0].body, /not a technical review or test result/);
    assert.equal(f.calls.permission, 2);
    assert.equal(f.calls.dismissals.length, 0);
  });
}
for (const permission of ['read', 'triage', 'none', 'unknown']) {
  test(`${permission} author cannot gain policy approval through association`, async () => {
    const f = fixture({ permission, pr: { author_association: 'COLLABORATOR' } });
    await f.invoke();
    assert.equal(f.calls.approvals.length, 0);
  });
}
for (const pr of [{ draft: true }, { state: 'closed' }, { base: { ref: 'release' } },
  { user: { login: 'some-bot', type: 'Bot' } }]) {
  test(`ineligible PR ${JSON.stringify(pr)} remains unapproved`, async () => {
    const f = fixture({ pr });
    await f.invoke();
    assert.equal(f.calls.approvals.length, 0);
  });
}
for (const status of [403, 404, 500]) {
  test(`permission API ${status} never permits approval`, async () => {
    const f = fixture({ permissionError: Object.assign(new Error('Unavailable'), { status }) });
    if (status === 404) await f.invoke();
    else await assert.rejects(f.invoke(), /Unavailable/);
    assert.equal(f.calls.approvals.length, 0);
  });
}
test('unavailable review history fails closed', async () => {
  const f = fixture({ reviewError: true });
  await assert.rejects(f.invoke(), /Reviews unavailable/);
  assert.equal(f.calls.approvals.length, 0);
});
test('permission removed between reads prevents approval', async () => {
  const f = fixture({ permissionAfter: 'none' });
  await f.invoke();
  assert.equal(f.calls.approvals.length, 0);
});
for (const prAfter of [{ head: { sha: 'b'.repeat(40) } }, { draft: true },
  { state: 'closed' }, { user: { login: 'different', type: 'User' } }]) {
  test(`concurrent PR change ${JSON.stringify(prAfter)} prevents approval`, async () => {
    const f = fixture({ prAfter });
    await f.invoke();
    assert.equal(f.calls.approvals.length, 0);
  });
}
test('human changes requested survives a subsequent comment', async () => {
  const f = fixture({ reviews: [
    { id: 1, user: { login: 'reviewer' }, state: 'CHANGES_REQUESTED' },
    { id: 2, user: { login: 'reviewer' }, state: 'COMMENTED' },
  ] });
  await f.invoke();
  assert.equal(f.calls.approvals.length, 0);
  assert.equal(f.calls.dismissals.length, 0);
});
test('reviewer can resolve their own changes-requested review', async () => {
  const f = fixture({ reviews: [
    { id: 1, user: { login: 'reviewer' }, state: 'CHANGES_REQUESTED' },
    { id: 2, user: { login: 'reviewer' }, state: 'APPROVED' },
  ] });
  await f.invoke();
  assert.equal(f.calls.approvals.length, 1);
});
test('uncertain response after approval is reconciled without a duplicate write', async () => {
  const f = fixture({ ambiguousWrite: true });
  await assert.rejects(f.invoke(), /Connection lost/);
  await f.invoke();
  assert.equal(f.calls.approvals.length, 1);
});
test('old head policy acknowledgement does not approve a new commit', async () => {
  const f = fixture({ reviews: [{ id: 1, user: { login: 'github-actions[bot]' },
    state: 'APPROVED', body: MARKER, commit_id: 'b'.repeat(40) }] });
  await f.invoke();
  assert.equal(f.calls.approvals[0].commit_id, SHA);
});
test('revoked collaborator loses only marked policy approvals, never human reviews', async () => {
  const f = fixture({ permission: 'none', reviews: [
    { id: 1, user: { login: 'github-actions[bot]' }, state: 'APPROVED', body: MARKER },
    { id: 2, user: { login: 'human' }, state: 'APPROVED', body: MARKER },
    { id: 3, user: { login: 'github-actions[bot]' }, state: 'APPROVED', body: 'Other review' },
  ] });
  await f.invoke();
  assert.deepEqual(f.calls.dismissals.map(r => r.review_id), [1]);
  assert.equal(f.calls.approvals.length, 0);
});
for (const context of [
  { eventName: 'pull_request', ref: 'refs/heads/main', repo },
  { eventName: 'workflow_dispatch', ref: 'refs/heads/untrusted', repo },
  { eventName: 'workflow_dispatch', ref: 'refs/heads/main', repo: { ...repo, owner: 'outsider' } },
]) {
  test(`reject untrusted entry context ${JSON.stringify(context)}`, async () => {
    await assert.rejects(run({ ...fixture(), context, core: {} }));
  });
}
test('manual reconciliation records failure while evaluating remaining PRs', async () => {
  const f = fixture({ permissionError: new Error('Unavailable') });
  const failures = [];
  await run({ github: f.github, context: { repo, ref: 'refs/heads/main',
    eventName: 'workflow_dispatch', payload: { inputs: { pull_request: '42' } } },
  core: { info() {}, error() {}, setFailed: message => failures.push(message) } });
  assert.equal(failures.length, 1);
  assert.equal(f.calls.approvals.length, 0);
});

test('automatic policy workflow cannot execute PR code or gain unrelated write scopes', () => {
  const fs = require('node:fs');
  const path = require('node:path');
  const workflow = fs.readFileSync(path.join(__dirname, '../workflows/collaborator-pr-policy.yml'), 'utf8');
  const triggers = workflow.match(/^on:\n((?:[ \t].*\n|#.*\n|\n)*)/m)[1];
  assert.deepEqual([...triggers.matchAll(/^  ([\w]+):/gm)].map(m => m[1]),
    ['pull_request_target', 'workflow_dispatch']);
  assert.match(triggers, /branches: \[main\]/);
  assert.match(workflow, /^permissions: \{\}$/m);
  assert.deepEqual([...workflow.matchAll(/^\s+([\w-]+): write$/gm)].map(m => m[1]), ['pull-requests']);
  assert.match(workflow, /github\.ref == 'refs\/heads\/main'/);
  assert.match(workflow, /ref: context\.sha,/);
  assert.match(workflow, /path: '\.github\/scripts\/collaborator-pr-policy\.cjs'/);
  assert.deepEqual([...workflow.matchAll(/uses: (.*)/g)].map(m => m[1]),
    ['actions/github-script@ed597411d8f924073f98dfc5c65a23a2325f34cd # v8']);
  assert.doesNotMatch(workflow, /^\s*(?:- )?run:|secrets\.|pull_request\.head|actions\/checkout/m);
});
