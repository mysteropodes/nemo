# nemo-feedback — Cloudflare Worker

Transport for the beta-tester feedback pipeline, for BOTH desktop (Tauri)
and web. This Worker is the sole trust boundary holding the write-scoped
GitHub token — neither build embeds one. On desktop, `feedback-bridge.js`
calls it through `tauri_plugin_http`'s fetch (`window.__TAURI__.http.fetch`)
instead of `window.fetch`, so the request is issued from Rust and isn't
subject to the webview's CSP `connect-src` or this Worker's own CORS
allowlist (neither lists `tauri://localhost`).

2026-08: added after the web beta shipped without it — a tester's feedback
looked "saved" (it *was*, in `localStorage`) but silently never reached
GitHub, because `feedback-bridge.js` used to gate the whole publish step on
`tauriOk()`. See `src/js/feedback-bridge.js`'s `workerPost`/`FEEDBACK_WORKER_URL`
for the client side of this.

2026-09: desktop stopped embedding its own token (`submit_feedback_issue`/
`upload_feedback_attachment` in `src-tauri/src/lib.rs` are gone) and now
posts through this same Worker — the only reason that Rust-side path ever
existed was "a browser can't hide a token, Rust can," but `tauri_plugin_http`
means Rust doesn't need to hide one at all here, it just relays the request.
This also means desktop submissions are now covered by this Worker's spam
filter (`looksLikeSpam` in `src/index.js`), which the old direct-from-Rust
path bypassed entirely.

## Endpoints

Both POST-only, JSON in and out:

- `/issue` — `{ title, body, labels }` → creates a GitHub Issue
- `/attachment` — `{ filename, contentBase64 }` → commits a file to
  `attachments/`, returns its `raw.githubusercontent.com` URL

## One-time setup (not doable from a workflow file)

1. Set the Cloudflare-side secret (this is a **Worker** secret, not a
   GitHub Actions secret):
   ```bash
   cd worker-feedback
   npx wrangler secret put GITHUB_FEEDBACK_TOKEN --name nemo-feedback
   ```
   Use a fine-grained GitHub PAT covering both repos: `mysteropodes/nemo`
   with `Issues: write` (reports live with the code) and
   `mysteropodes/strokemotion-feedback` with `Contents: write` (screenshots
   stay there so no token needs write access to the source tree). This is
   now the ONLY feedback token anywhere — neither build embeds its own.
2. `ALLOWED_ORIGINS` is set in `wrangler.jsonc`'s `vars` (currently
   `https://nemo-editor.mysteropodes-auth.workers.dev`, comma-separated if
   more origins are added later — e.g. a custom domain). Update it there
   and redeploy if the editor's origin changes.
3. `FEEDBACK_WORKER_URL` in `src/js/feedback-bridge.js` points at this
   Worker's real URL (`https://nemo-feedback.mysteropodes-auth.workers.dev`).
   Update it there if this Worker is ever renamed or moved to a custom
   domain.
