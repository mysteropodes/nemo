import { HttpError } from "./http.js";

const encoder = new TextEncoder();

export function bytesToHex(bytes) {
  return [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function hexToBytes(value) {
  if (!/^[0-9a-f]{64}$/i.test(value || "")) {
    throw new Error("private key must be exactly 32 bytes of hex");
  }
  return Uint8Array.from(value.match(/.{2}/g), (byte) => Number.parseInt(byte, 16));
}

export async function sha256Hex(bytes) {
  return bytesToHex(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)));
}

export function constantTimeEqual(left, right) {
  const a = encoder.encode(left);
  const b = encoder.encode(right);
  let difference = a.length ^ b.length;
  const length = Math.max(a.length, b.length);
  for (let index = 0; index < length; index += 1) {
    difference |= (a[index] || 0) ^ (b[index] || 0);
  }
  return difference === 0;
}

export async function verifyGitHubSignature(bytes, signature, secret) {
  if (!secret) throw new HttpError(500, "webhook verifier is not configured");
  if (!/^sha256=[0-9a-f]{64}$/.test(signature || "")) return false;
  const key = await crypto.subtle.importKey(
    "raw",
    encoder.encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const digest = bytesToHex(new Uint8Array(await crypto.subtle.sign("HMAC", key, bytes)));
  return constantTimeEqual(signature, `sha256=${digest}`);
}
