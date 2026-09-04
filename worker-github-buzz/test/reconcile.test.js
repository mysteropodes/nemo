import assert from "node:assert/strict";
import test from "node:test";
import { collectReconciliation } from "../src/reconcile.js";
import { testEnv } from "./helpers.js";

function request(token = "test-reconcile-token") {
  return new Request("https://bridge.example/reconcile", {
    method: "POST",
    headers: { Authorization: `Bearer ${token}` },
  });
}

test("reconciliation is authenticated, bounded, and normalizes each source", async (context) => {
  const updatedAt = new Date(Date.now() - 60_000).toISOString();
  const since = new Date(Date.now() - 3_600_000).toISOString();
  const responses = {
    issues: [{ id: 1, number: 2, title: "Issue", state: "open", updated_at: updatedAt }],
    pull_requests: [
      { id: 3, number: 4, title: "PR", state: "open", draft: false, merged_at: null, updated_at: updatedAt },
    ],
    workflow_runs: {
      workflow_runs: [
        { id: 5, run_attempt: 1, name: "CI", status: "completed", conclusion: "failure", updated_at: updatedAt },
      ],
    },
  };
  const calls = [];
  const originalFetch = globalThis.fetch;
  let nextResponse;
  globalThis.fetch = async (url, options) => {
    calls.push({ url, options });
    return Response.json(nextResponse);
  };
  context.after(() => { globalThis.fetch = originalFetch; });

  const normalized = [];
  for (const source of Object.keys(responses)) {
    nextResponse = responses[source];
    const result = await collectReconciliation(request(), testEnv(), "mysteropodes/nemo", {
      since,
      source,
      limit: 3,
      page: 1,
    });
    normalized.push(result.items[0].notification);
  }
  assert.deepEqual(normalized.map((item) => item.kind), ["issue", "pull_request", "workflow_run"]);
  assert.equal(normalized[2].state, "failure");
  assert.equal(calls.length, 3);
  assert.equal(calls.every((call) => call.options.redirect === "error"), true);
  assert.equal(calls.every((call) => !call.options.headers.Authorization), true);
});

test("unauthorized reconciliation fails before GitHub access", async (context) => {
  let called = false;
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async () => {
    called = true;
    return Response.json([]);
  };
  context.after(() => { globalThis.fetch = originalFetch; });
  await assert.rejects(
    () => collectReconciliation(request("wrong"), testEnv(), "mysteropodes/nemo", {
      since: new Date(Date.now() - 60_000).toISOString(),
      source: "issues",
    }),
    /unauthorized/,
  );
  assert.equal(called, false);
});
