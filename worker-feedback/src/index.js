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

// Issues land on the code repo, so a report sits next to the code and the
// other reports. Attachments stay on the old feedback repo on purpose:
// committing screenshots into nemo would require Contents:write on the source
// tree -- the one permission that turns a leaked token into a way to reach
// main -- and would bloat the repo (44 PNGs already). Two repos, two roles.
const ISSUE_REPO = 'mysteropodes/nemo';
const ATTACHMENT_REPO = 'mysteropodes/strokemotion-feedback';

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

async function githubFetch(env, repo, path, init) {
  return fetch(`https://api.github.com/repos/${repo}${path}`, {
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

// ---- Content filter (2026-08) ----------------------------------------------
// The Worker is the only mandatory choke point: spam won't arrive through the
// app, it arrives as a direct POST here, so filtering client-side would do
// nothing. Everything below therefore runs server-side.
//
// Tuned to never reject a real tester. Two traps this deliberately avoids:
//  - Nemo ships in en/fr/ja/es. Anything resembling a "non-ASCII = spam" or
//    "must look like English" rule would silently drop every Japanese report.
//    No rule here looks at the script or language of the text.
//  - A genuine report may legitimately carry a link (a repro clip, a paste).
//    So links are counted, not banned — 3+ is the signal, 1 is normal.
// A rejection returns 400 with a readable reason rather than dropping the
// entry silently, so a false positive is visible to the person reporting.

const MAX_BODY = 30000;

// Multi-word, unambiguous commercial spam. Single generic words ("crypto",
// "free") are avoided on purpose — they appear in honest sentences.
const SPAM_PHRASES = [
  /\bbuy\s+(cheap|now|followers|likes|views)\b/i,
  /\b(casino|viagra|cialis|porn|escort)\b/i,
  /\bseo\s+(services?|expert|agency)\b/i,
  /\b(forex|binary\s+options?|bitcoin\s+doubler)\b/i,
  /\b(work\s+from\s+home|make\s+money\s+(fast|online)|earn\s+\$\d)/i,
  /\bclick\s+here\s+to\s+(win|claim)\b/i,
  /\bfree\s+(gift\s?cards?|giveaway|crypto)\b/i,
];

function looksLikeSpam(title, body) {
  const text = `${title}\n${body}`;
  const stripped = text.replace(/\s+/g, '');
  if (stripped.length < 8) return 'message is empty or too short to act on';
  if (body.length > MAX_BODY) return `body exceeds ${MAX_BODY} characters`;

  const links = (text.match(/https?:\/\//gi) || []).length;
  if (links >= 3) return 'too many links';

  for (const re of SPAM_PHRASES) {
    if (re.test(text)) return 'message matched a spam pattern';
  }

  // Keyboard mashing / padding: one character repeated far beyond any real
  // word, or one word repeated far beyond any real sentence.
  if (/(.)\1{29,}/.test(text)) return 'repeated characters';
  const words = text.toLowerCase().match(/[\p{L}\p{N}]+/gu) || [];
  if (words.length >= 20) {
    const counts = new Map();
    for (const w of words) counts.set(w, (counts.get(w) || 0) + 1);
    if (Math.max(...counts.values()) > words.length * 0.5) return 'repeated words';
  }
  return null;
}

// Attachments are rendered inline in issue bodies from raw.githubusercontent,
// so only real image types are accepted — the filename regex below already
// blocks path traversal, this narrows what the file can be.
const IMAGE_EXT = /\.(png|jpe?g|gif|webp)$/i;

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
  const spam = looksLikeSpam(title, body);
  if (spam) {
    return json({ error: `rejected: ${spam}` }, 400, cors);
  }
  const resp = await githubFetch(env, ISSUE_REPO, '/issues', {
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
  if (!IMAGE_EXT.test(filename)) {
    return json({ error: 'attachment must be a png, jpg, gif or webp image' }, 400, cors);
  }
  if (!contentBase64 || typeof contentBase64 !== 'string' || contentBase64.length > 8_000_000) {
    return json({ error: 'invalid or oversized content' }, 400, cors);
  }
  const path = `attachments/${filename}`;
  const resp = await githubFetch(env, ATTACHMENT_REPO, `/contents/${path}`, {
    method: 'PUT',
    body: JSON.stringify({ message: `Feedback attachment: ${filename}`, content: contentBase64 }),
  });
  if (!resp.ok) {
    const text = await resp.text();
    return json({ error: `GitHub upload error ${resp.status}: ${text}` }, 502, cors);
  }
  return json({ url: `https://raw.githubusercontent.com/${ATTACHMENT_REPO}/main/${path}` }, 200, cors);
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
