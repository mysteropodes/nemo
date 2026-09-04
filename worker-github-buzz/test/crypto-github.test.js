import assert from "node:assert/strict";
import test from "node:test";
import { sha256Hex, verifyGitHubSignature } from "../src/crypto.js";
import { normalizeWebhook, validateDeliveryId } from "../src/github.js";
import { issuePayload, signature } from "./helpers.js";

test("GitHub HMAC verification accepts the exact body and rejects changes", async () => {
  const body = Buffer.from(JSON.stringify(issuePayload()));
  const signed = signature(body);
  assert.equal(await verifyGitHubSignature(body, signed, "test-webhook-secret"), true);
  assert.equal(
    await verifyGitHubSignature(Buffer.concat([body, Buffer.from(" ")]), signed, "test-webhook-secret"),
    false,
  );
  assert.equal(await verifyGitHubSignature(body, "sha256=xyz", "test-webhook-secret"), false);
});

test("normalization binds the configured repo and constructs canonical links", () => {
  const item = normalizeWebhook("issues", issuePayload(), "mysteropodes/nemo");
  assert.equal(item.semanticId, "issue:101:2026-09-03T20:00:00.000Z:open");
  assert.equal(item.link, "https://github.com/mysteropodes/nemo/issues/7");
  assert.throws(
    () => normalizeWebhook("issues", issuePayload({ repository: { full_name: "someone/else" } }), "mysteropodes/nemo"),
    /repository is not configured/,
  );
});

test("delivery ids are bounded and stable", async () => {
  assert.equal(validateDeliveryId("01234567-89ab-cdef-0123-456789abcdef"), "01234567-89ab-cdef-0123-456789abcdef");
  assert.throws(() => validateDeliveryId("../../delivery"), /invalid GitHub delivery id/);
  assert.equal(await sha256Hex(new Uint8Array()), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
});
