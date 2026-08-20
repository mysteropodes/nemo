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
   — the exact same scope `STROKEMOTION_FEEDBACK_TOKEN` already uses for the
   desktop build. Can be that same token value, or a fresh one.
2. After the first deploy, set `ALLOWED_ORIGINS` (Cloudflare dashboard >
   nemo-feedback > Settings > Variables) to the editor's real origin(s) —
   comma-separated, e.g. `https://editor.nemo.example.com,https://nemo-editor.<subdomain>.workers.dev`.
   Left unset, the Worker reflects whatever `Origin` header it receives
   (works everywhere, including anyone else's site — fine while the real
   origin isn't picked yet, tighten once it is).
3. Point `FEEDBACK_WORKER_URL` in `src/js/feedback-bridge.js` at this
   Worker's real URL (printed in the deploy workflow's Actions log, also on
   its Cloudflare dashboard page).
