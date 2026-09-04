import { MAX_INTERNAL_BYTES, MAX_WEBHOOK_BYTES } from "./constants.js";
import { readConfig } from "./config.js";
import { sha256Hex, verifyGitHubSignature } from "./crypto.js";
import { normalizeWebhook, validateDeliveryId } from "./github.js";
import { errorResponse, HttpError, jsonResponse, parseJsonBytes, readBodyBounded } from "./http.js";
import { GitHubBuzzDeliveryLedger } from "./ledger.js";
import { authenticateReconciliation, collectReconciliation } from "./reconcile.js";

function ledgerStub(env, config) {
  if (!env.DELIVERY_LEDGER) throw new HttpError(500, "bridge is not configured");
  return env.DELIVERY_LEDGER.get(env.DELIVERY_LEDGER.idFromName(`${config.repositoryLower}:${config.projectAddress}`));
}

async function forward(stub, path, body) {
  const response = await stub.fetch(`https://ledger.invalid${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (response.status >= 500) return jsonResponse({ error: "notification delivery failed" }, 502);
  return response;
}

async function handleWebhook(request, env) {
  const config = readConfig(env);
  const bytes = await readBodyBounded(request, MAX_WEBHOOK_BYTES);
  const signature = request.headers.get("x-hub-signature-256");
  if (!(await verifyGitHubSignature(bytes, signature, env.GITHUB_WEBHOOK_SECRET))) {
    throw new HttpError(401, "invalid webhook signature");
  }
  const deliveryId = validateDeliveryId(request.headers.get("x-github-delivery"));
  const eventName = request.headers.get("x-github-event") || "";
  let notification;
  try {
    notification = normalizeWebhook(eventName, parseJsonBytes(bytes), config.repository);
  } catch (error) {
    if (error instanceof HttpError && error.status === 202) {
      return jsonResponse({ ok: true, ignored: true }, 202);
    }
    throw error;
  }
  const bodyHash = await sha256Hex(bytes);
  return forward(ledgerStub(env, config), "/ingest", { deliveryId, bodyHash, notification });
}

async function handleReconcile(request, env) {
  const config = readConfig(env);
  authenticateReconciliation(request, env);
  const bytes = await readBodyBounded(request, MAX_INTERNAL_BYTES);
  const body = bytes.byteLength ? parseJsonBytes(bytes) : {};
  const batch = await collectReconciliation(request, env, config.repository, body);
  const response = await forward(ledgerStub(env, config), "/batch", { items: batch.items });
  if (!response.ok) return response;
  const result = await response.json();
  return jsonResponse({
    ok: true,
    processed: result.results?.length || 0,
    duplicates: result.results?.filter((item) => item.duplicate).length || 0,
    source: batch.source,
    page: batch.page,
    possibleMore: batch.possibleMore,
  });
}

export { GitHubBuzzDeliveryLedger };

export default {
  async fetch(request, env) {
    try {
      const { pathname } = new URL(request.url);
      if (request.method === "GET" && pathname === "/health") return jsonResponse({ ok: true });
      if (request.method !== "POST") throw new HttpError(405, "method not allowed");
      if (pathname === "/github") return await handleWebhook(request, env);
      if (pathname === "/reconcile") return await handleReconcile(request, env);
      throw new HttpError(404, "not found");
    } catch (error) {
      return errorResponse(error);
    }
  },
};
