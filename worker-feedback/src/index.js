// Feedback relay for the web build — does exactly what submit_feedback_issue
// / upload_feedback_attachment (src-tauri/src/lib.rs) do on desktop, since a
// browser has no Rust backend to hide the GitHub token behind. Same target
// repo, same payload shapes, same token scope (Issues:write + Contents:write
// on mysteropodes/strokemotion-feedback ONLY — see that repo's own README
// for why it deliberately contains no app source). The token lives as a
// Worker secret (GITHUB_FEEDBACK_TOKEN), never sent to or readable by the
// browser — this Worker is the trust boundary, same role the Rust command
// plays on desktop.
//
// Endpoints (POST only):
//   /issue      { title, body, labels } -> creates a GitHub Issue
//   /attachment { filename, contentBase64 } -> commits a file, returns its
//                raw.githubusercontent.com URL
//
// ALLOWED_ORIGINS (Worker var, comma-separated) restricts which origins may
// call this — set to the deployed editor's real origin(s) once known
// (workers.dev preview + any custom domain); wide open by default only
// because that origin isn't picked yet.

const REPO = 'mysteropodes/strokemotion-feedback';

function corsHeaders(request, env) {
  const origin = request.headers.get('Origin') || '';
  const allowed = (env.ALLOWED_ORIGINS || '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  const allowOrigin = allowed.length === 0 || allowed.includes(origin) ? (origin || '*') : allowed[0];
  return {
    'Access-Control-Allow-Origin': allowOrigin,
    'Access-Control-Allow-Methods': 'POST, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type',
    Vary: 'Origin',
  };
}

function json(data, status, headers) {
  return new Response(JSON.stringify(data), {
    status,
    headers: Object.assign({ 'Content-Type': 'application/json' }, headers),
  });
}

async function githubFetch(env, path, init) {
  return fetch(`https://api.github.com/repos/${REPO}${path}`, {
    ...init,
    headers: Object.assign(
      {
        Authorization: `Bearer ${env.GITHUB_FEEDBACK_TOKEN}`,
        Accept: 'application/vnd.github+json',
        'User-Agent': 'nemo-feedback-worker',
        'Content-Type': 'application/json',
      },
      init && init.headers
    ),
  });
}

async function handleIssue(request, env, cors) {
  let payload;
  try {
    payload = await request.json();
  } catch (e) {
    return json({ error: 'invalid JSON body' }, 400, cors);
  }
  const title = payload && payload.title;
  const body = (payload && payload.body) || '';
  const labels = Array.isArray(payload && payload.labels) ? payload.labels : [];
  if (!title || typeof title !== 'string' || title.length > 300) {
    return json({ error: 'title is required (max 300 chars)' }, 400, cors);
  }
  const resp = await githubFetch(env, '/issues', {
    method: 'POST',
    body: JSON.stringify({ title, body, labels }),
  });
  if (!resp.ok) {
    const text = await resp.text();
    return json({ error: `GitHub API error ${resp.status}: ${text}` }, 502, cors);
  }
  return json({ ok: true }, 200, cors);
}

async function handleAttachment(request, env, cors) {
  let payload;
  try {
    payload = await request.json();
  } catch (e) {
    return json({ error: 'invalid JSON body' }, 400, cors);
  }
  const filename = payload && payload.filename;
  const contentBase64 = payload && payload.contentBase64;
  // Filename comes from a fixed pattern client-side (entry.id + extension,
  // see feedback-bridge.js) — reject anything that isn't that shape rather
  // than trust it, since it becomes part of a repo file path.
  if (!filename || typeof filename !== 'string' || !/^[\w-]+\.[a-zA-Z0-9]+$/.test(filename)) {
    return json({ error: 'invalid filename' }, 400, cors);
  }
  if (!contentBase64 || typeof contentBase64 !== 'string' || contentBase64.length > 8_000_000) {
    return json({ error: 'invalid or oversized content' }, 400, cors);
  }
  const path = `attachments/${filename}`;
  const resp = await githubFetch(env, `/contents/${path}`, {
    method: 'PUT',
    body: JSON.stringify({ message: `Feedback attachment: ${filename}`, content: contentBase64 }),
  });
  if (!resp.ok) {
    const text = await resp.text();
    return json({ error: `GitHub upload error ${resp.status}: ${text}` }, 502, cors);
  }
  return json({ url: `https://raw.githubusercontent.com/${REPO}/main/${path}` }, 200, cors);
}

export default {
  async fetch(request, env) {
    const cors = corsHeaders(request, env);
    if (request.method === 'OPTIONS') return new Response(null, { status: 204, headers: cors });
    if (request.method !== 'POST') return json({ error: 'method not allowed' }, 405, cors);
    const { pathname } = new URL(request.url);
    try {
      if (pathname === '/issue') return await handleIssue(request, env, cors);
      if (pathname === '/attachment') return await handleAttachment(request, env, cors);
      return json({ error: 'not found' }, 404, cors);
    } catch (e) {
      return json({ error: String((e && e.message) || e) }, 500, cors);
    }
  },
};
