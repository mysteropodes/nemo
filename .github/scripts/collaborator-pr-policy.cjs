'use strict';

// Automatic policy runs from default main; manual dispatch may use the protected
// remediation branch. Neither route reads or executes PR code.
const TRUSTED_BASES = new Set(['main', 'codex/native-remediation']);
const MARKER = '<!-- nemo-collaborator-policy:v1 -->';
const BOT = 'github-actions[bot]';
const WRITE_PERMISSIONS = new Set(['write', 'maintain', 'admin']);

function policyReview(review) {
  return review.user?.login === BOT && review.state === 'APPROVED' &&
    review.body?.startsWith(MARKER);
}

function hasChangeRequest(reviews) {
  const latest = new Map();
  for (const review of [...reviews].sort((a, b) => a.id - b.id)) {
    if (review.user?.login !== BOT &&
        ['APPROVED', 'CHANGES_REQUESTED', 'DISMISSED'].includes(review.state)) {
      latest.set(review.user?.login, review.state);
    }
  }
  return [...latest.values()].includes('CHANGES_REQUESTED');
}

function eligible(pr, permission) {
  return pr.state === 'open' && !pr.draft && TRUSTED_BASES.has(pr.base.ref) &&
    pr.user?.type === 'User' && WRITE_PERMISSIONS.has(permission);
}

async function reconcile({ github, repo, number, log, expectedBase }) {
  const args = { ...repo, pull_number: number };
  const readPR = async () => (await github.rest.pulls.get(args)).data;
  const readReviews = () => github.paginate(github.rest.pulls.listReviews, args);
  const permission = async pr => {
    try {
      return (await github.rest.repos.getCollaboratorPermissionLevel({
        ...repo, username: pr.user.login,
      })).data.permission;
    } catch (error) {
      if (error.status === 404) return 'none';
      throw error; // No approval on unavailable/forbidden permission evidence.
    }
  };
  const dismiss = async reviews => {
    for (const review of reviews.filter(policyReview)) {
      await github.rest.pulls.dismissReview({ ...args, review_id: review.id,
        message: 'Automated collaborator policy no longer applies; normal review is required.' });
    }
  };

  const pr = await readPR();
  if (pr.state !== 'open') return log(`#${number}: closed; unchanged`);
  if (expectedBase && pr.base.ref !== expectedBase) {
    return log(`#${number}: base differs from trusted execution branch; unchanged`);
  }
  const reviews = await readReviews();
  if (!eligible(pr, await permission(pr))) {
    await dismiss(reviews);
    return log(`#${number}: no policy approval; collaborator review required`);
  }
  if (hasChangeRequest(reviews)) {
    return log(`#${number}: changes requested; resolve the review`);
  }

  // Reconcile retries and concurrent pushes against fresh metadata before any approval.
  const current = await readPR();
  if (current.head.sha !== pr.head.sha || current.user.login !== pr.user.login ||
      current.base.ref !== pr.base.ref ||
      !eligible(current, await permission(current))) {
    return log(`#${number}: changed during evaluation; rerun against current state`);
  }
  const currentReviews = await readReviews();
  if (hasChangeRequest(currentReviews)) return log(`#${number}: changes requested`);
  if (currentReviews.some(r => policyReview(r) && r.commit_id === current.head.sha)) {
    return log(`#${number}: current-head policy approval already present`);
  }

  // Never auto-merge, dismiss a human review, or claim technical acceptance.
  await github.rest.pulls.createReview({ ...args, commit_id: current.head.sha,
    event: 'APPROVE', body: `${MARKER}\nAutomated **merge-policy acknowledgement**, not a technical review or test result.\n\n` +
      `The author currently has repository write/maintain/admin permission. Under the policy agreed by Ilya and Cyrill, ` +
      `the author's team owns technical review and local validation and may merge this PR itself once its evidence is complete. ` +
      `No approval from the other human team is needed. External-author PRs receive no policy approval. ` +
      `Human change requests and unresolved conversations still apply.\n\nCandidate: \`${current.head.sha}\`. ` +
      `[Workflow](https://github.com/mysteropodes/nemo/blob/${current.base.ref}/engineering/remediation/EXECUTION_PLAN.en.md#7-integration-and-branch-cleanup).`,
  });
  log(`#${number}: policy acknowledged for ${current.head.sha}`);
}

async function run({ github, context, core }) {
  const repo = context.repo;
  const base = [...TRUSTED_BASES].find(name => context.ref === `refs/heads/${name}`);
  if (repo.owner !== 'mysteropodes' || repo.repo !== 'nemo' || !base) {
    throw new Error('Policy may run only from an allowlisted mysteropodes/nemo protected base');
  }
  let numbers;
  if (context.eventName === 'pull_request_target') {
    if (base !== 'main') throw new Error('Automatic policy must run from default main');
    numbers = [context.payload.pull_request.number];
  } else if (context.eventName === 'workflow_dispatch') {
    const input = context.payload.inputs?.pull_request?.trim() || '';
    if (input && !/^[1-9]\d*$/.test(input)) throw new Error('Expected a positive PR number');
    numbers = input ? [Number(input)] :
      (await github.paginate(github.rest.pulls.list, { ...repo, state: 'open', base }))
        .map(pr => pr.number);
  } else {
    throw new Error('Unsupported policy event');
  }
  let failures = 0;
  for (const number of numbers) {
    try {
      await reconcile({ github, repo, number, expectedBase: base, log: message => core.info(message) });
    } catch (error) {
      failures++;
      core.error(`#${number}: policy evaluation failed: ${error.message}`);
    }
  }
  if (failures) core.setFailed(`${failures} policy evaluations failed; inspect and reconcile before retrying`);
}

module.exports = { run, reconcile, eligible, hasChangeRequest, MARKER };
