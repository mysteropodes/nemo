import { createHmac } from "node:crypto";

export const TEST_KEY = "0000000000000000000000000000000000000000000000000000000000000001";
export const TEST_PROJECT_ADDRESS = `30621:${"1".repeat(64)}:nemo`;
export const TEST_CHANNEL_ID = "22222222-2222-4222-8222-222222222222";

export function testEnv(extra = {}) {
  return {
    GITHUB_REPOSITORY: "mysteropodes/nemo",
    GITHUB_WEBHOOK_SECRET: "test-webhook-secret",
    BUZZ_PROJECT_ADDRESS: TEST_PROJECT_ADDRESS,
    BUZZ_HOME_CHANNEL_ID: TEST_CHANNEL_ID,
    BUZZ_RELAY_HTTP_URL: "https://relay.example",
    BUZZ_BRIDGE_PRIVATE_KEY: TEST_KEY,
    RECONCILE_TOKEN: "test-reconcile-token",
    ...extra,
  };
}

export function signature(body, secret = "test-webhook-secret") {
  return `sha256=${createHmac("sha256", secret).update(body).digest("hex")}`;
}

export function issuePayload(overrides = {}) {
  return {
    action: "opened",
    repository: { full_name: "mysteropodes/nemo" },
    issue: {
      id: 101,
      number: 7,
      title: "A title",
      state: "open",
      updated_at: "2026-09-03T20:00:00Z",
    },
    ...overrides,
  };
}

export class MemoryStorage {
  constructor() {
    this.values = new Map();
    this.alarm = null;
  }

  async get(key) {
    return this.values.get(key);
  }

  async put(key, value) {
    if (typeof key === "object") {
      for (const [entryKey, entryValue] of Object.entries(key)) {
        this.values.set(entryKey, structuredClone(entryValue));
      }
      return;
    }
    this.values.set(key, structuredClone(value));
  }

  async delete(key) {
    return this.values.delete(key);
  }

  async list(options = {}) {
    const entries = [...this.values.entries()]
      .sort(([left], [right]) => left.localeCompare(right))
      .filter(([key]) => !options.prefix || key.startsWith(options.prefix))
      .filter(([key]) => !options.end || key < options.end)
      .slice(0, options.limit);
    return new Map(entries);
  }

  async getAlarm() {
    return this.alarm;
  }

  async setAlarm(value) {
    this.alarm = value;
  }
}
