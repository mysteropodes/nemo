import { MAX_RECONCILE_AGE_MS, MAX_RECONCILE_LIMIT } from "./constants.js";
import { constantTimeEqual, sha256Hex } from "./crypto.js";
import { normalizeIssue, normalizePullRequest, normalizeWorkflowRun } from "./github.js";
import { HttpError } from "./http.js";

const SOURCES = new Set(["issues", "pull_requests", "workflow_runs"]);

export function authenticateReconciliation(request, env) {
  const configured = typeof env.RECONCILE_TOKEN === "string" ? env.RECONCILE_TOKEN : "";
  const supplied = request.headers.get("authorization") || "";
  if (!configured || !constantTimeEqual(supplied, `Bearer ${configured}`)) {
    throw new HttpError(401, "unauthorized");
  }
}

function optionsOf(body) {
  const since = new Date(body?.since);
  const limit = body?.limit ?? 10;
  const page = body?.page ?? 1;
  const source = body?.source;
  if (
    !Number.isFinite(since.valueOf()) ||
    Date.now() - since.valueOf() > MAX_RECONCILE_AGE_MS ||
    since.valueOf() > Date.now()
  ) {
    throw new HttpError(400, "since must be within the last seven days");
  }
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > MAX_RECONCILE_LIMIT) {
    throw new HttpError(400, `limit must be 1-${MAX_RECONCILE_LIMIT}`);
  }
  if (!Number.isSafeInteger(page) || page < 1 || page > 10) {
    throw new HttpError(400, "page must be 1-10");
  }
  if (!SOURCES.has(source)) {
    throw new HttpError(400, "source must be issues, pull_requests, or workflow_runs");
  }
  return { since: since.toISOString(), limit, page, source };
}

async function githubJson(env, repository, path) {
  const headers = {
    Accept: "application/vnd.github+json",
    "User-Agent": "nemo-github-buzz-bridge",
    "X-GitHub-Api-Version": "2022-11-28",
  };
  if (env.GITHUB_READ_TOKEN) headers.Authorization = `Bearer ${env.GITHUB_READ_TOKEN}`;
  const response = await fetch(`https://api.github.com/repos/${repository}${path}`, {
    headers,
    redirect: "error",
  });
  if (!response.ok) throw new HttpError(502, "GitHub reconciliation failed");
  const declared = Number(response.headers.get("content-length"));
  if (Number.isFinite(declared) && declared > 2_000_000) {
    throw new HttpError(502, "GitHub reconciliation response is too large");
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (bytes.byteLength > 2_000_000) throw new HttpError(502, "GitHub reconciliation response is too large");
  try {
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    throw new HttpError(502, "GitHub reconciliation returned an invalid response");
  }
}

export async function collectReconciliation(request, env, repository, body) {
  authenticateReconciliation(request, env);
  const { since, limit, page, source } = optionsOf(body);
  const query = `state=all&sort=updated&direction=desc&per_page=${limit}&page=${page}`;
  const result = source === "issues"
    ? await githubJson(env, repository, `/issues?${query}&since=${encodeURIComponent(since)}`)
    : source === "pull_requests"
      ? await githubJson(env, repository, `/pulls?${query}`)
      : await githubJson(env, repository, `/actions/runs?per_page=${limit}&page=${page}`);
  const records = source === "workflow_runs" ? result?.workflow_runs : result;
  if (!Array.isArray(records)) {
    throw new HttpError(502, "GitHub reconciliation returned an invalid response");
  }
  const cutoff = Date.parse(since);
  const notifications = records
    .filter((item) => source !== "issues" || !item.pull_request)
    .map((item) => source === "issues"
      ? normalizeIssue(item, "reconciled", repository)
      : source === "pull_requests"
        ? normalizePullRequest(item, "reconciled", repository)
        : normalizeWorkflowRun(item, "reconciled", repository))
    .filter((item) => Date.parse(item.updatedAt) >= cutoff)
    .sort((left, right) => Date.parse(left.updatedAt) - Date.parse(right.updatedAt));
  const items = [];
  for (const notification of notifications) {
    const bodyHash = await sha256Hex(new TextEncoder().encode(JSON.stringify(notification)));
    items.push({ deliveryId: `reconcile-${bodyHash.slice(0, 40)}`, bodyHash, notification });
  }
  return {
    items,
    source,
    page,
    limit,
    possibleMore: records.length === limit,
  };
}
