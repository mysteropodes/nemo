// nemo-editor's Worker script. Static assets stay the default path (via the
// ASSETS binding, see wrangler.jsonc) — this file only intercepts the one
// dynamic route the web build needs: /api/google-font, a same-origin proxy
// for live Google Fonts lookups (vector-text-bridge.js's fetchGoogleFontWeight).
//
// Why a proxy at all: fonts.googleapis.com's /css2 endpoint returns a
// different @font-face src depending on the request's User-Agent — modern
// browsers get .woff2, but an old-Android UA gets back a plain .ttf URL on
// fonts.gstatic.com (CORS-open, no API key needed). Browsers forbid
// overriding the User-Agent header from fetch(), so this trick only works
// server-side. The desktop build does the same thing in Rust
// (fetch_google_font, src-tauri/src/lib.rs) via reqwest, which can set
// arbitrary headers.

// ---- Security headers (2026-08) --------------------------------------------
// Added after an audit found the site sent none at all.
//
// There is deliberately NO script-src/style-src here, and that is the whole
// point of this comment. The app needs 'unsafe-inline' (one inline <script>,
// 492 style="" attributes), 'unsafe-eval' (the nemo.* scripting API runs user
// code through new Function) and wasm eval (the Rust render engine). A CSP
// permissive enough to keep all of that working would protect nothing while
// looking like it does. So only the directives that are both safe here and
// genuinely useful are set:
//   frame-ancestors  no one can frame the app -> no clickjacking
//   object-src       kills <object>/<embed> as an injection vector
//   base-uri         stops a <base> tag from re-pointing every relative URL,
//                    which is what turns a small injection into a big one
// Permissions-Policy leaves clipboard alone on purpose: the app uses it.
const SECURITY_HEADERS = {
  'Content-Security-Policy': "frame-ancestors 'none'; object-src 'none'; base-uri 'self'",
  'X-Frame-Options': 'DENY',
  'X-Content-Type-Options': 'nosniff',
  'Referrer-Policy': 'strict-origin-when-cross-origin',
  'Permissions-Policy': 'camera=(), microphone=(), geolocation=(), payment=(), usb=()',
  'Strict-Transport-Security': 'max-age=31536000',
};

function withSecurityHeaders(resp) {
  const out = new Response(resp.body, resp);
  for (const [k, v] of Object.entries(SECURITY_HEADERS)) out.headers.set(k, v);
  return out;
}

const ANDROID_UA = 'Mozilla/5.0 (Linux; U; Android 2.2)';

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    if (url.pathname === '/api/google-font') {
      return withSecurityHeaders(await handleGoogleFont(url));
    }
    return withSecurityHeaders(await env.ASSETS.fetch(request));
  },
};

async function handleGoogleFont(url) {
  const family = (url.searchParams.get('family') || '').trim();
  const weight = parseInt(url.searchParams.get('weight') || '400', 10) || 400;
  const italic = url.searchParams.get('italic') === '1';
  if (!family) return new Response('missing family', { status: 400 });

  const axis = italic ? 'ital,wght@1,' : 'wght@';
  const cssUrl = 'https://fonts.googleapis.com/css2?family=' +
    encodeURIComponent(family).replace(/%20/g, '+') + ':' + axis + weight + '&display=swap';

  let cssResp;
  try {
    cssResp = await fetch(cssUrl, { headers: { 'User-Agent': ANDROID_UA } });
  } catch (e) {
    return new Response('upstream fetch failed: ' + e.message, { status: 502 });
  }
  if (!cssResp.ok) {
    return new Response("font family '" + family + "' not found on Google Fonts", { status: 404 });
  }
  const css = await cssResp.text();
  const m = css.match(/url\(([^)]+)\)/);
  if (!m) return new Response('no font URL found in Google Fonts response', { status: 404 });
  const ttfUrl = m[1].replace(/^['"]|['"]$/g, '');
  // ttfUrl is read out of the upstream response rather than built here, so it
  // is only as trustworthy as that response. Pin the host instead of fetching
  // whatever came back.
  if (!ttfUrl.startsWith('https://fonts.gstatic.com/')) {
    return new Response('unexpected font host', { status: 502 });
  }

  let ttfResp;
  try {
    ttfResp = await fetch(ttfUrl);
  } catch (e) {
    return new Response('font download failed: ' + e.message, { status: 502 });
  }
  if (!ttfResp.ok) return new Response('font download failed', { status: 502 });

  return new Response(ttfResp.body, {
    headers: {
      'Content-Type': 'font/ttf',
      'Cache-Control': 'public, max-age=604800',
    },
  });
}
