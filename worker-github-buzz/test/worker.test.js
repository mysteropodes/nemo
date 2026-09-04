import assert from "node:assert/strict";
import test from "node:test";
import worker from "../src/index.js";
import { issuePayload, signature, testEnv } from "./helpers.js";

function workerEnv(captured) {
  const stub = {
    async fetch(_url, options) {
      captured.push(JSON.parse(options.body));
      return Response.json({ ok: true, duplicate: false, eventId: "event" });
    },
  };
  return testEnv({
    DELIVERY_LEDGER: {
      idFromName(value) { return value; },
      get() { return stub; },
    },
  });
}

function webhookRequest(body, overrides = {}) {
  return new Request("https://bridge.example/github", {
    method: "POST",
    headers: {
      "x-github-event": "issues",
      "x-github-delivery": "44444444-4444-4444-8444-444444444444",
      "x-hub-signature-256": signature(body),
      ...overrides,
    },
    body,
  });
}

test("webhook verifies before forwarding a bounded normalized envelope", async () => {
  const captured = [];
  const body = JSON.stringify(issuePayload());
  const response = await worker.fetch(webhookRequest(body), workerEnv(captured));
  assert.equal(response.status, 200);
  assert.equal(captured.length, 1);
  assert.equal(captured[0].notification.kind, "issue");
  assert.equal("body" in captured[0].notification, false);
});

test("bad signatures and wrong repositories never reach the ledger", async () => {
  const captured = [];
  const body = JSON.stringify(issuePayload());
  const badSignature = await worker.fetch(
    webhookRequest(body, { "x-hub-signature-256": `sha256=${"0".repeat(64)}` }),
    workerEnv(captured),
  );
  assert.equal(badSignature.status, 401);
  const wrongBody = JSON.stringify(issuePayload({ repository: { full_name: "someone/else" } }));
  const wrongRepo = await worker.fetch(
    webhookRequest(wrongBody, { "x-hub-signature-256": signature(wrongBody) }),
    workerEnv(captured),
  );
  assert.equal(wrongRepo.status, 403);
  assert.equal(captured.length, 0);
});

test("unsupported signed events are acknowledged without publication", async () => {
  const captured = [];
  const body = JSON.stringify({ repository: { full_name: "mysteropodes/nemo" } });
  const response = await worker.fetch(webhookRequest(body, {
    "x-github-event": "push",
    "x-hub-signature-256": signature(body),
  }), workerEnv(captured));
  assert.equal(response.status, 202);
  assert.equal(captured.length, 0);
});

test("reconciliation authenticates before parsing its body", async () => {
  const response = await worker.fetch(
    new Request("https://bridge.example/reconcile", { method: "POST", body: "{" }),
    workerEnv([]),
  );
  assert.equal(response.status, 401);
});
