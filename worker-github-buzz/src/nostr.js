import { finalizeEvent } from "nostr-tools/pure";
import { EVENT_KIND_HTTP_AUTH, EVENT_KIND_STREAM_MESSAGE } from "./constants.js";
import { sha256Hex } from "./crypto.js";
import { eventTags, renderNotification } from "./message.js";

function base64Utf8(value) {
  const bytes = new TextEncoder().encode(value);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

export function buildBuzzEvent(config, notification, createdAt) {
  return finalizeEvent(
    {
      kind: EVENT_KIND_STREAM_MESSAGE,
      created_at: createdAt,
      content: renderNotification(notification),
      tags: eventTags(config, notification),
    },
    config.secretKey,
  );
}

export async function buildNip98Header(config, bodyBytes, createdAt = Math.floor(Date.now() / 1_000)) {
  const authEvent = finalizeEvent(
    {
      kind: EVENT_KIND_HTTP_AUTH,
      created_at: createdAt,
      content: "",
      tags: [
        ["u", config.relayEventsUrl],
        ["method", "POST"],
        ["payload", await sha256Hex(bodyBytes)],
        ["nonce", crypto.randomUUID()],
      ],
    },
    config.secretKey,
  );
  return `Nostr ${base64Utf8(JSON.stringify(authEvent))}`;
}

export async function publishNotification(config, notification, createdAt, fetchImpl = fetch) {
  const event = buildBuzzEvent(config, notification, createdAt);
  const body = new TextEncoder().encode(JSON.stringify(event));
  const headers = {
    Authorization: await buildNip98Header(config, body),
    "Content-Type": "application/json",
  };
  if (config.authTag) headers["x-auth-tag"] = config.authTag;
  const response = await fetchImpl(config.relayEventsUrl, { method: "POST", headers, body });
  if (!response.ok) throw new Error("relay publication failed");
  let result;
  try {
    result = await response.json();
  } catch {
    throw new Error("relay publication failed");
  }
  if (!result?.accepted || result.event_id !== event.id) {
    throw new Error("relay publication failed");
  }
  return { eventId: event.id };
}
