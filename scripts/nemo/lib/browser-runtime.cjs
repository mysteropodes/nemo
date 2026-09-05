'use strict';

const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { isDeepStrictEqual } = require('node:util');
const isolation = require('./isolation.cjs');
const identity = require('./identity.cjs');
const { ROOT } = require('./util.cjs');

const IDENTITY_PATH = '/.well-known/nemo-runtime.json';
const SCHEMA = 'nemo.browser-runtime/1';
const LOOPBACK = new Set(['127.0.0.1', '::1']);
const MIME = new Map([
  ['.css', 'text/css; charset=utf-8'], ['.gif', 'image/gif'],
  ['.html', 'text/html; charset=utf-8'], ['.ico', 'image/x-icon'],
  ['.jpeg', 'image/jpeg'], ['.jpg', 'image/jpeg'],
  ['.js', 'text/javascript; charset=utf-8'], ['.json', 'application/json; charset=utf-8'],
  ['.mjs', 'text/javascript; charset=utf-8'], ['.mp3', 'audio/mpeg'],
  ['.mp4', 'video/mp4'], ['.ogg', 'audio/ogg'], ['.otf', 'font/otf'],
  ['.png', 'image/png'], ['.riv', 'application/octet-stream'],
  ['.svg', 'image/svg+xml'], ['.ttf', 'font/ttf'],
  ['.wasm', 'application/wasm'], ['.wav', 'audio/wav'],
  ['.webm', 'video/webm'], ['.webp', 'image/webp'],
  ['.woff', 'font/woff'], ['.woff2', 'font/woff2'],
]);

function publicSource(source) {
  if (!source) return source;
  const { originUrl: _originUrl, ...safe } = source;
  return safe;
}

function publicRoots(roots) {
  return {
    temp: roots.temp, cache: roots.cache, build: roots.build,
    reports: roots.reports, browserProfile: roots.browserProfile,
  };
}

function applyTaskEnvironment(taskId, roots) {
  process.env.NEMO_TASK_ID = taskId;
  process.env.NEMO_REPORT_DIR = roots.reports;
  process.env.NEMO_BROWSER_PROFILE = roots.browserProfile;
  process.env.XDG_CACHE_HOME = roots.cache;
  process.env.TMPDIR = roots.temp;
  process.env.TMP = roots.temp;
  process.env.TEMP = roots.temp;
}

function browserCandidates() {
  if (process.platform === 'darwin') return [
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge',
  ];
  if (process.platform === 'win32') return [
    path.join(process.env.PROGRAMFILES || '', 'Google', 'Chrome', 'Application', 'chrome.exe'),
    path.join(process.env['PROGRAMFILES(X86)'] || '', 'Microsoft', 'Edge', 'Application', 'msedge.exe'),
  ];
  return ['/usr/bin/google-chrome', '/usr/bin/chromium', '/usr/bin/chromium-browser', '/usr/bin/microsoft-edge'];
}

function browserLaunchConfig(roots, url, options = {}) {
  let executable = options.browser;
  if (!executable || executable === 'auto') executable = browserCandidates().find((file) => file && fs.existsSync(file));
  else executable = path.resolve(executable);
  if (!executable || !fs.existsSync(executable)) throw new Error('browser executable not found; pass --browser auto or an absolute Chromium path');
  fs.accessSync(executable, fs.constants.X_OK);
  const cache = path.join(roots.cache, 'browser');
  fs.mkdirSync(cache, { recursive: true });
  const args = [
    `--user-data-dir=${roots.browserProfile}`,
    `--disk-cache-dir=${cache}`,
    '--no-first-run', '--no-default-browser-check', '--disable-background-networking', '--disable-sync',
  ];
  if (options.headless) args.push('--headless=new');
  if (Array.isArray(options.extraArgs)) args.push(...options.extraArgs);
  args.push(url);
  return {
    executable, args, cwd: ROOT, url,
    env: {
      ...process.env, TMPDIR: roots.temp, TMP: roots.temp, TEMP: roots.temp,
      XDG_CACHE_HOME: roots.cache, NEMO_REPORT_DIR: roots.reports,
    },
    profileDir: roots.browserProfile, cacheDir: cache,
  };
}

function spawnBrowser(config) {
  return new Promise((resolve, reject) => {
    const child = spawn(config.executable, config.args, {
      cwd: config.cwd, env: config.env, stdio: 'ignore',
    });
    child.once('error', reject);
    child.once('spawn', () => { child.off('error', reject); resolve(child); });
  });
}

async function stopBrowser(child) {
  if (!child || child.exitCode != null || child.signalCode != null) return;
  const exited = new Promise((resolve) => child.once('exit', resolve));
  let timer;
  const grace = new Promise((resolve) => { timer = setTimeout(resolve, 2000); });
  child.kill('SIGTERM');
  await Promise.race([exited, grace]);
  clearTimeout(timer);
  if (child.exitCode == null && child.signalCode == null) {
    child.kill('SIGKILL');
    await exited;
  }
}

function json(res, status, value, headOnly) {
  const body = Buffer.from(JSON.stringify(value, null, 2) + '\n');
  res.writeHead(status, {
    'Cache-Control': 'no-store', 'Content-Length': body.length,
    'Content-Type': 'application/json; charset=utf-8', 'X-Content-Type-Options': 'nosniff',
  });
  res.end(headOnly ? undefined : body);
}

function plain(res, status, value, headOnly, extra = {}) {
  const body = Buffer.from(value);
  res.writeHead(status, {
    'Cache-Control': 'no-store', 'Content-Length': body.length,
    'Content-Type': 'text/plain; charset=utf-8', 'X-Content-Type-Options': 'nosniff', ...extra,
  });
  res.end(headOnly ? undefined : body);
}

function inside(root, candidate) {
  const relative = path.relative(root, candidate);
  return relative === '' || (relative !== '..' && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative));
}

function resolveStaticFile(srcRoot, pathname) {
  let decoded;
  try { decoded = decodeURIComponent(pathname); }
  catch { return { status: 400, reason: 'invalid URL encoding' }; }
  if (decoded.includes('\0') || decoded.includes('\\')) return { status: 400, reason: 'invalid path' };
  const lexical = path.resolve(srcRoot, decoded.replace(/^\/+/, '') || 'index.html');
  if (!inside(srcRoot, lexical)) return { status: 403, reason: 'path escapes source root' };
  let real;
  try {
    real = fs.realpathSync(lexical);
    if (fs.statSync(real).isDirectory()) real = fs.realpathSync(path.join(real, 'index.html'));
  } catch (err) {
    if (err.code === 'ENOENT' || err.code === 'ENOTDIR') return { status: 404, reason: 'not found' };
    return { status: 500, reason: 'file lookup failed' };
  }
  if (!inside(srcRoot, real)) return { status: 403, reason: 'symlink escapes source root' };
  const stat = fs.statSync(real);
  return stat.isFile() ? { status: 200, file: real, stat } : { status: 404, reason: 'not found' };
}

function listen(server, host, port) {
  return new Promise((resolve, reject) => {
    const failed = (err) => { server.off('listening', ready); reject(err); };
    const ready = () => { server.off('error', failed); resolve(server.address()); };
    server.once('error', failed); server.once('listening', ready);
    server.listen({ host, port, exclusive: true });
  });
}

function closeServer(server) {
  if (!server.listening) return Promise.resolve();
  return new Promise((resolve, reject) => {
    server.close((err) => err ? reject(err) : resolve());
    if (typeof server.closeAllConnections === 'function') server.closeAllConnections();
  });
}

async function startBrowserRuntime(options = {}) {
  const host = options.host || '127.0.0.1';
  if (!LOOPBACK.has(host)) throw new Error('preview host must be loopback (127.0.0.1 or ::1)');
  const port = options.port == null ? 0 : Number(options.port);
  if (!Number.isSafeInteger(port) || port < 0 || port > 65535) throw new Error('port must be an integer from 0 through 65535');
  const taskId = isolation.resolveTaskId(options.taskId);
  const roots = isolation.taskRoots(taskId);
  applyTaskEnvironment(taskId, roots);
  const srcRoot = fs.realpathSync(options.srcRoot || path.join(ROOT, 'src'));
  const startupSource = identity.sourceIdentity();
  const startupBuild = identity.buildIdentity();
  let launcher = null;
  let browserChild = null;
  let browserError = null;
  let runtime = null;
  let observed = { source: startupSource, build: startupBuild, sourceMatches: true, buildMatches: true, checkedAt: Date.now() };

  function health(force) {
    const local = launcher ? isolation.verifyHandshake(taskId, { ownerToken: launcher.ownerToken }) : { ok: false, reason: 'launcher registration incomplete' };
    if (force || Date.now() - observed.checkedAt >= 1000) {
      const source = identity.sourceIdentity(); const build = identity.buildIdentity();
      observed = {
        source, build, sourceMatches: isDeepStrictEqual(startupSource, source),
        buildMatches: isDeepStrictEqual(startupBuild, build), checkedAt: Date.now(),
      };
    }
    return { ...observed, local, healthy: local.ok && observed.sourceMatches && observed.buildMatches };
  }

  const server = http.createServer((req, res) => {
    const method = req.method || 'GET';
    if (method !== 'GET' && method !== 'HEAD') { plain(res, 405, 'method not allowed\n', false, { Allow: 'GET, HEAD' }); return; }
    let url;
    try { url = new URL(req.url || '/', `http://${host.includes(':') ? `[${host}]` : host}`); }
    catch { plain(res, 400, 'invalid URL\n', method === 'HEAD'); return; }
    const state = health(url.pathname === IDENTITY_PATH);
    if (url.pathname === IDENTITY_PATH) {
      const browserActive = !!browserChild && browserChild.exitCode == null && browserChild.signalCode == null;
      json(res, !launcher ? 503 : (state.healthy ? 200 : 409), {
        schema: SCHEMA, healthy: state.healthy,
        reason: state.healthy ? 'task owner, source and build identities match' : (!state.local.ok ? state.local.reason : 'source or build identity changed'),
        taskId, pid: process.pid, origin: runtime && runtime.origin,
        source: { startup: publicSource(startupSource), current: publicSource(state.source), matches: state.sourceMatches },
        build: { startup: startupBuild, current: state.build, matches: state.buildMatches },
        roots: publicRoots(roots),
        environment: {
          TMPDIR: process.env.TMPDIR, XDG_CACHE_HOME: process.env.XDG_CACHE_HOME,
          NEMO_REPORT_DIR: process.env.NEMO_REPORT_DIR,
          NEMO_BROWSER_PROFILE: process.env.NEMO_BROWSER_PROFILE,
        },
        browser: {
          requested: !!options.browser, integrated: browserActive, active: browserActive,
          pid: browserChild ? browserChild.pid : null, profileDir: roots.browserProfile,
          error: browserError,
        },
      }, method === 'HEAD');
      return;
    }
    // A browser explicitly launched with this server can request its first
    // assets between the socket bind and the synchronous owner registration.
    // Serve that brief startup window; after registration, fail closed on drift.
    if (launcher && !state.healthy) { plain(res, 409, `runtime identity mismatch: ${!state.local.ok ? state.local.reason : 'source or build changed'}\n`, method === 'HEAD'); return; }
    const resolved = resolveStaticFile(srcRoot, url.pathname);
    if (!resolved.file) { plain(res, resolved.status, `${resolved.reason}\n`, method === 'HEAD'); return; }
    res.writeHead(200, {
      'Cache-Control': 'no-store', 'Content-Length': resolved.stat.size,
      'Content-Type': MIME.get(path.extname(resolved.file).toLowerCase()) || 'application/octet-stream',
      'X-Content-Type-Options': 'nosniff',
    });
    if (method === 'HEAD') { res.end(); return; }
    const stream = fs.createReadStream(resolved.file);
    stream.on('error', () => res.destroy()); stream.pipe(res);
  });

  try {
    const address = await listen(server, host, port);
    const origin = `http://${address.family === 'IPv6' ? `[${address.address}]` : address.address}:${address.port}`;
    const browserConfig = options.browser
      ? browserLaunchConfig(roots, origin + '/', { browser: options.browser, headless: options.headless, extraArgs: options.browserArgs })
      : null;
    launcher = isolation.registerLauncher(taskId, { pid: process.pid, label: 'browser-preview' });
    if (options.browser) {
      try {
        browserChild = await spawnBrowser(browserConfig);
        browserChild.once('exit', (code, signal) => {
          browserError = `browser exited (${signal || `code ${code}`})`;
        });
      }
      catch (err) { browserError = err.message; }
    }
    runtime = {
      taskId, roots, server, origin, ownerToken: launcher.ownerToken,
      source: startupSource, build: startupBuild, browserChild, browserError,
      async close() { await stopBrowser(browserChild); await closeServer(server); },
    };
    return runtime;
  } catch (err) {
    await stopBrowser(browserChild).catch(() => {});
    await closeServer(server).catch(() => {});
    const existing = isolation.readLauncher(taskId);
    if (!existing) { try { isolation.releaseTask(taskId); } catch { /* fail closed */ } }
    throw err;
  }
}

module.exports = { IDENTITY_PATH, SCHEMA, browserLaunchConfig, startBrowserRuntime };
