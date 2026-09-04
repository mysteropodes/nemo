import { hexToBytes } from "./crypto.js";
import { HttpError } from "./http.js";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;
const PROJECT_ADDRESS = /^30621:[0-9a-f]{64}:(.+)$/;

function required(env, name) {
  const value = typeof env[name] === "string" ? env[name].trim() : "";
  if (!value) throw new HttpError(500, "bridge is not configured");
  return value;
}

function relayEventsUrl(raw) {
  let url;
  try {
    url = new URL(raw);
  } catch {
    throw new HttpError(500, "bridge is not configured");
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    (url.pathname !== "/" && url.pathname !== "")
  ) {
    throw new HttpError(500, "bridge is not configured");
  }
  return `${url.origin}/events`;
}

export function readConfig(env) {
  const repository = required(env, "GITHUB_REPOSITORY");
  const projectAddress = required(env, "BUZZ_PROJECT_ADDRESS");
  const channelId = required(env, "BUZZ_HOME_CHANNEL_ID");
  const projectMatch = PROJECT_ADDRESS.exec(projectAddress);
  if (
    repository.length > 200 ||
    !REPOSITORY.test(repository) ||
    !projectMatch ||
    new TextEncoder().encode(projectMatch[1]).byteLength > 1_024 ||
    !UUID.test(channelId)
  ) {
    throw new HttpError(500, "bridge is not configured");
  }
  let secretKey;
  try {
    secretKey = hexToBytes(required(env, "BUZZ_BRIDGE_PRIVATE_KEY"));
  } catch {
    throw new HttpError(500, "bridge is not configured");
  }
  return {
    repository,
    repositoryLower: repository.toLowerCase(),
    projectAddress,
    channelId: channelId.toLowerCase(),
    relayEventsUrl: relayEventsUrl(required(env, "BUZZ_RELAY_HTTP_URL")),
    secretKey,
    authTag: typeof env.BUZZ_AUTH_TAG === "string" ? env.BUZZ_AUTH_TAG.trim() : "",
  };
}
