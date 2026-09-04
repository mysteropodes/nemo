import assert from "node:assert/strict";
import test from "node:test";
import { GitHubBuzzDeliveryLedger } from "../src/ledger.js";
import { MemoryStorage, testEnv } from "./helpers.js";

const notification = {
  kind: "issue",
  id: 101,
  number: 7,
  updatedAt: "2026-09-03T20:00:00.000Z",
  state: "open",
  title: "A title",
  action: "opened",
  link: "https://github.com/mysteropodes/nemo/issues/7",
  semanticId: "issue:101:2026-09-03T20:00:00.000Z:open",
};

function request(item) {
  return new Request("https://ledger.invalid/ingest", { method: "POST", body: JSON.stringify(item) });
}

test("ledger publishes once across exact and semantic duplicates", async (context) => {
  const storage = new MemoryStorage();
  const ledger = new GitHubBuzzDeliveryLedger({ storage }, testEnv());
  let calls = 0;
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (_url, options) => {
    calls += 1;
    const event = JSON.parse(new TextDecoder().decode(options.body));
    return Response.json({ accepted: true, event_id: event.id, message: "ok" });
  };
  context.after(() => { globalThis.fetch = originalFetch; });

  const first = await ledger.fetch(
    request({ deliveryId: "11111111-1111-4111-8111-111111111111", bodyHash: "a", notification }),
  );
  assert.equal(first.status, 200);
  assert.equal((await first.json()).duplicate, false);
  const retry = await ledger.fetch(
    request({ deliveryId: "11111111-1111-4111-8111-111111111111", bodyHash: "a", notification }),
  );
  assert.equal((await retry.json()).duplicate, true);
  const reconciled = await ledger.fetch(
    request({
      deliveryId: "reconcile-0123456789abcdef0123456789abcdef01234567",
      bodyHash: "b",
      notification,
    }),
  );
  assert.equal((await reconciled.json()).duplicate, true);
  assert.equal(calls, 1);
});

test("failed publication retries the same signed event id", async (context) => {
  const storage = new MemoryStorage();
  const ledger = new GitHubBuzzDeliveryLedger({ storage }, testEnv());
  const ids = [];
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (_url, options) => {
    const event = JSON.parse(new TextDecoder().decode(options.body));
    ids.push(event.id);
    return ids.length === 1
      ? new Response("no", { status: 503 })
      : Response.json({ accepted: true, event_id: event.id, message: "ok" });
  };
  context.after(() => { globalThis.fetch = originalFetch; });

  const envelope = { deliveryId: "22222222-2222-4222-8222-222222222222", bodyHash: "c", notification };
  assert.equal((await ledger.fetch(request(envelope))).status, 502);
  const pending = await ledger.fetch(request(envelope));
  assert.equal((await pending.json()).pending, true);
  for (const [key, record] of storage.values) {
    if (key.startsWith("semantic:")) await storage.put(key, { ...record, nextAttemptAt: 0 });
  }
  await ledger.alarm();
  assert.equal((await ledger.fetch(request(envelope))).status, 200);
  assert.deepEqual(ids, [ids[0], ids[0]]);
});

test("delivery id reuse with different bytes fails closed", async (context) => {
  const storage = new MemoryStorage();
  const ledger = new GitHubBuzzDeliveryLedger({ storage }, testEnv());
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (_url, options) => {
    const event = JSON.parse(new TextDecoder().decode(options.body));
    return Response.json({ accepted: true, event_id: event.id, message: "ok" });
  };
  context.after(() => { globalThis.fetch = originalFetch; });
  const deliveryId = "33333333-3333-4333-8333-333333333333";
  await ledger.fetch(request({ deliveryId, bodyHash: "first", notification }));
  const response = await ledger.fetch(request({ deliveryId, bodyHash: "changed", notification }));
  assert.equal(response.status, 409);
});
