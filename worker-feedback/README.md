# nemo-feedback — Cloudflare Worker

Browser-only transport for the beta-tester feedback pipeline. On desktop
(Tauri), `submit_feedback_issue`/`upload_feedback_attachment`
(`src-tauri/src/lib.rs`) create GitHub Issues in
[`mysteropodes/strokemotion-feedback`](https://github.com/mysteropodes/strokemotion-feedback)
directly from Rust, keeping the write-scoped token out of the webview. A
browser has no Rust backend to hide that token behind — this Worker is the
same trust boundary, just running on Cloudflare instead of on the user's
machine.

2026-08: added after the web beta shipped without it — a tester's feedback
looked "saved" (it *was*, in `localStorage`) but silently never reached
GitHub, because `feedback-bridge.js` used to gate the whole publish step on
`tauriOk()`. See `src/js/feedback-bridge.js`'s `workerPost`/`FEEDBACK_WORKER_URL`
for the client side of this.

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
   Use a fine-grained GitHub PAT scoped ONLY to
   `mysteropodes/strokemotion-feedback`, `Issues: write` + `Contents: write`
   — the exact same scope `NEMO_FEEDBACK_TOKEN` already uses for the
   desktop build. Can be that same token value, or a fresh one.
2. `ALLOWED_ORIGINS` is set in `wrangler.jsonc`'s `vars` (currently
   `https://nemo-editor.mysteropodes-auth.workers.dev`, comma-separated if
   more origins are added later — e.g. a custom domain). Update it there
   and redeploy if the editor's origin changes.
3. `FEEDBACK_WORKER_URL` in `src/js/feedback-bridge.js` points at this
   Worker's real URL (`https://nemo-feedback.mysteropodes-auth.workers.dev`).
   Update it there if this Worker is ever renamed or moved to a custom
   domain.
