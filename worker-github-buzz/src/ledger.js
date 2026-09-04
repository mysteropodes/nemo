import {
  DELIVERY_RETENTION_MS,
  MAX_INTERNAL_BYTES,
  PENDING_RETRY_MAX_MS,
  PENDING_SCAN_LIMIT,
} from "./constants.js";
import { readConfig } from "./config.js";
import { sha256Hex } from "./crypto.js";
import { errorResponse, HttpError, jsonResponse, parseJsonBytes, readBodyBounded } from "./http.js";
import { publishNotification } from "./nostr.js";

const encoder = new TextEncoder();

async function storageKey(prefix, value) {
  return `${prefix}:${await sha256Hex(encoder.encode(value))}`;
}

function retryDelay(attempts) {
  return Math.min(PENDING_RETRY_MAX_MS, 30_000 * 2 ** Math.min(attempts, 7));
}

function dueKey(recordKey, dueAt) {
  return `due:${String(Math.floor(dueAt)).padStart(13, "0")}:${recordKey}`;
}

function dueTime(key) {
  const value = Number(key.split(":", 2)[1]);
  return Number.isFinite(value) ? value : null;
}

export class GitHubBuzzDeliveryLedger {
  constructor(state, env) {
    this.state = state;
    this.env = env;
  }

  async fetch(request) {
    try {
      const { pathname } = new URL(request.url);
      if (request.method !== "POST" || (pathname !== "/ingest" && pathname !== "/batch")) {
        throw new HttpError(404, "not found");
      }
      const body = parseJsonBytes(await readBodyBounded(request, MAX_INTERNAL_BYTES));
      if (pathname === "/ingest") return jsonResponse(await this.ingest(body));
      if (!Array.isArray(body.items) || body.items.length > 20) {
        throw new HttpError(400, "invalid reconciliation batch");
      }
      const results = [];
      for (const item of body.items) results.push(await this.ingest(item));
      return jsonResponse({ ok: true, results });
    } catch (error) {
      return errorResponse(error);
    }
  }

  async ingest(item) {
    if (
      !item ||
      typeof item.deliveryId !== "string" ||
      typeof item.bodyHash !== "string" ||
      !item.notification?.semanticId
    ) {
      throw new HttpError(400, "invalid delivery envelope");
    }
    const deliveryKey = await storageKey("delivery", item.deliveryId);
    const semanticKey = await storageKey("semantic", item.notification.semanticId);
    const existingDelivery = await this.state.storage.get(deliveryKey);
    if (existingDelivery && existingDelivery.bodyHash !== item.bodyHash) {
      throw new HttpError(409, "delivery id was reused with different content");
    }
    if (existingDelivery?.status === "delivered") {
      return { ok: true, duplicate: true, eventId: existingDelivery.eventId };
    }
    let record = await this.state.storage.get(semanticKey);
    if (record?.status === "delivered") {
      await this.storeDelivery(deliveryKey, item.bodyHash, semanticKey, "delivered", record.eventId);
      return { ok: true, duplicate: true, eventId: record.eventId };
    }
    if (!record) {
      record = {
        status: "pending",
        notification: item.notification,
        createdAt: Math.floor(Date.now() / 1_000),
        attempts: 0,
        nextAttemptAt: Date.now(),
      };
      await this.storeRecord(semanticKey, record, record.nextAttemptAt);
    } else if (record.nextAttemptAt > Date.now()) {
      await this.storeDelivery(deliveryKey, item.bodyHash, semanticKey, "pending");
      await this.scheduleMaintenance(record.nextAttemptAt);
      return { ok: true, pending: true };
    }
    await this.storeDelivery(deliveryKey, item.bodyHash, semanticKey, "pending");
    return this.publish(semanticKey, deliveryKey, item.bodyHash, record);
  }

  async publish(semanticKey, deliveryKey, bodyHash, record) {
    try {
      const result = await publishNotification(readConfig(this.env), record.notification, record.createdAt);
      const delivered = {
        ...record,
        status: "delivered",
        eventId: result.eventId,
        deliveredAt: Date.now(),
        expiresAt: Date.now() + DELIVERY_RETENTION_MS,
      };
      await this.storeRecord(semanticKey, delivered, delivered.expiresAt);
      if (deliveryKey) {
        await this.storeDelivery(deliveryKey, bodyHash, semanticKey, "delivered", result.eventId);
      }
      await this.scheduleMaintenance(delivered.expiresAt);
      return { ok: true, duplicate: false, eventId: result.eventId };
    } catch {
      const attempts = record.attempts + 1;
      const nextAttemptAt = Date.now() + retryDelay(attempts);
      await this.storeRecord(semanticKey, {
        ...record,
        attempts,
        nextAttemptAt,
      }, nextAttemptAt);
      await this.scheduleMaintenance(nextAttemptAt);
      throw new HttpError(502, "Buzz publication failed");
    }
  }

  async storeDelivery(key, bodyHash, semanticKey, status, eventId = null) {
    const expiresAt = Date.now() + DELIVERY_RETENTION_MS;
    await this.storeRecord(key, {
      bodyHash,
      semanticKey,
      status,
      eventId,
      expiresAt,
    }, expiresAt);
  }

  async storeRecord(key, value, dueAt) {
    const current = await this.state.storage.get(key);
    const nextDueKey = dueKey(key, dueAt);
    const stored = { ...value, dueKey: nextDueKey };
    await this.state.storage.put({
      [key]: stored,
      [nextDueKey]: { recordKey: key },
    });
    if (current?.dueKey && current.dueKey !== nextDueKey) {
      await this.state.storage.delete(current.dueKey);
    }
  }

  async scheduleMaintenance(desired) {
    const current = await this.state.storage.getAlarm();
    if (current === null || current > desired) await this.state.storage.setAlarm(desired);
  }

  async alarm() {
    const now = Date.now();
    const due = await this.state.storage.list({
      prefix: "due:",
      end: `due:${String(Math.floor(now + 1)).padStart(13, "0")}`,
      limit: PENDING_SCAN_LIMIT,
    });
    for (const [indexKey, index] of due) {
      const record = await this.state.storage.get(index.recordKey);
      if (!record || record.dueKey !== indexKey) {
        await this.state.storage.delete(indexKey);
        continue;
      }
      if (index.recordKey.startsWith("semantic:") && record.status === "pending") {
        try {
          await this.publish(index.recordKey, null, null, record);
        } catch {
          // The persisted pending record and next alarm carry retry state.
        }
      } else {
        await this.state.storage.delete(index.recordKey);
        await this.state.storage.delete(indexKey);
      }
    }
    const next = await this.state.storage.list({ prefix: "due:", limit: 1 });
    const nextAt = next.size ? dueTime(next.keys().next().value) : null;
    if (nextAt !== null) {
      const continuation = due.size === PENDING_SCAN_LIMIT ? Date.now() + 1_000 : nextAt;
      await this.state.storage.setAlarm(Math.max(Date.now() + 1_000, continuation));
    }
  }
}
