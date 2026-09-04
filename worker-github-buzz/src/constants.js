export const MAX_WEBHOOK_BYTES = 1_048_576;
export const MAX_INTERNAL_BYTES = 131_072;
export const DELIVERY_RETENTION_MS = 30 * 24 * 60 * 60 * 1_000;
export const MAX_RECONCILE_LIMIT = 20;
export const MAX_RECONCILE_AGE_MS = 7 * 24 * 60 * 60 * 1_000;
export const PENDING_RETRY_MAX_MS = 60 * 60 * 1_000;
export const PENDING_SCAN_LIMIT = 100;
export const EVENT_KIND_STREAM_MESSAGE = 9;
export const EVENT_KIND_HTTP_AUTH = 27_235;

export const JSON_HEADERS = Object.freeze({
  "Content-Type": "application/json; charset=utf-8",
  "Cache-Control": "no-store",
});
