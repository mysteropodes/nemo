import { HttpError } from "./http.js";
import { sanitizeExternalText } from "./message.js";

const DELIVERY_ID = /^[0-9a-f-]{16,64}$/i;
const SAFE_ACTION = /^[a-z][a-z0-9_]{0,39}$/;
const WORKFLOW_STATUS = new Set([
  "action_required",
  "cancelled",
  "completed",
  "failure",
  "in_progress",
  "neutral",
  "pending",
  "queued",
  "requested",
  "skipped",
  "stale",
  "startup_failure",
  "success",
  "timed_out",
  "waiting",
]);

function positiveInteger(value, label) {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new HttpError(400, `invalid GitHub ${label}`);
  }
  return value;
}

function isoTimestamp(value, label) {
  if (typeof value !== "string" || !Number.isFinite(Date.parse(value))) {
    throw new HttpError(400, `invalid GitHub ${label}`);
  }
  return new Date(value).toISOString();
}

function actionOf(payload) {
  const action = payload && payload.action;
  return typeof action === "string" && SAFE_ACTION.test(action) ? action : "updated";
}

function titleOf(value) {
  return sanitizeExternalText(value, 512);
}

function checkedRepository(payload, expectedRepositoryLower) {
  const actual = payload?.repository?.full_name;
  if (typeof actual !== "string" || actual.toLowerCase() !== expectedRepositoryLower) {
    throw new HttpError(403, "repository is not configured for this bridge");
  }
}

function record(kind, id, number, updatedAt, state, title, action, link, extra = {}) {
  const semanticId = `${kind}:${id}:${updatedAt}:${state}`;
  return { kind, id, number, updatedAt, state, title: titleOf(title), action, link, semanticId, ...extra };
}

export function validateDeliveryId(value) {
  if (!DELIVERY_ID.test(value || "")) {
    throw new HttpError(400, "invalid GitHub delivery id");
  }
  return value.toLowerCase();
}

export function normalizeWebhook(eventName, payload, repository) {
  checkedRepository(payload, repository.toLowerCase());
  if (eventName === "issues") return normalizeIssue(payload.issue, payload.action, repository);
  if (eventName === "pull_request") {
    return normalizePullRequest(payload.pull_request, payload.action, repository);
  }
  if (eventName === "workflow_run") {
    return normalizeWorkflowRun(payload.workflow_run, payload.action, repository);
  }
  throw new HttpError(202, "event is not projected");
}

export function normalizeIssue(issue, action, repository) {
  const id = positiveInteger(issue?.id, "issue id");
  const number = positiveInteger(issue?.number, "issue number");
  const updatedAt = isoTimestamp(issue?.updated_at, "issue timestamp");
  const state = issue?.state === "closed" ? "closed" : "open";
  const link = `https://github.com/${repository}/issues/${number}`;
  return record("issue", id, number, updatedAt, state, issue?.title, actionOf({ action }), link);
}

export function normalizePullRequest(pull, action, repository) {
  const id = positiveInteger(pull?.id, "pull request id");
  const number = positiveInteger(pull?.number, "pull request number");
  const updatedAt = isoTimestamp(pull?.updated_at, "pull request timestamp");
  const state = pull?.merged_at ? "merged" : pull?.state === "closed" ? "closed" : pull?.draft ? "draft" : "open";
  const link = `https://github.com/${repository}/pull/${number}`;
  return record("pull_request", id, number, updatedAt, state, pull?.title, actionOf({ action }), link);
}

export function normalizeWorkflowRun(run, action, repository) {
  const id = positiveInteger(run?.id, "workflow run id");
  const attempt = Number.isSafeInteger(run?.run_attempt) && run.run_attempt > 0 ? run.run_attempt : 1;
  const updatedAt = isoTimestamp(run?.updated_at, "workflow run timestamp");
  const rawStatus = run?.status === "completed" ? run.conclusion || "completed" : run?.status;
  const status = WORKFLOW_STATUS.has(rawStatus) ? rawStatus : "unknown";
  const link = `https://github.com/${repository}/actions/runs/${id}`;
  return record("workflow_run", id, null, updatedAt, status, run?.name, actionOf({ action }), link, {
    attempt,
    semanticId: `workflow_run:${id}:${attempt}:${updatedAt}:${status}`,
  });
}
