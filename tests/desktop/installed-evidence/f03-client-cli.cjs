'use strict';
// The two agent clients, run headless for #1049: `codex exec` and `claude -p`, each with a per-invocation MCP configuration that
// points at the candidate package's own nemo-mcp and the isolated session's registry. Nothing here edits a user or project
// client configuration, and no keyboard or mouse input exists on this route.
//
// What this module can say: which client binary and version ran, the exact command line, how it ended, and (informationally) the
// tool calls its own event stream shows. It never says whether the workflow succeeded; the judge reads the application for that.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn, spawnSync } = require('node:child_process');

const NAMES = Object.freeze(['codex', 'claude']);
// The model each half is asked to use. Claude Code is always given an explicit `--model` (default `sonnet`, so the evidence does not depend on whatever the
// installed client's own default is this month); the runner overrides it with the `claudeModel` option (`--claude-model`). Codex gets `--model` only when the runner
// sets `codexModel` (`--codex-model`); otherwise it runs on its own configuration and the receipt says 'client default'.
const DEFAULT_CLAUDE_MODEL = 'sonnet', CLIENT_DEFAULT = 'client default';
const MODEL = /^[A-Za-z0-9][A-Za-z0-9._:\/\[\]-]{0,63}$/;
const validModel = value => typeof value === 'string' && MODEL.test(value);
function requestedModel(name, config = {}) {
  if (name === 'claude') return config.claudeModel === undefined ? DEFAULT_CLAUDE_MODEL : config.claudeModel;
  return config.codexModel === undefined ? CLIENT_DEFAULT : config.codexModel;
}
const LIMIT = 8 * 1024 * 1024;
const digest = value => crypto.createHash('sha256').update(value).digest('hex');
const scrubbedEnv = (env = process.env) => Object.fromEntries(Object.entries(env).filter(([k, v]) => !/^NEMO_/.test(k) && typeof v === 'string'));

function onPath(command, env = process.env) {
  for (const dir of String(env.PATH || '').split(path.delimiter).filter(Boolean)) {
    const file = path.join(dir, command);
    try { if (fs.statSync(file).isFile()) { fs.accessSync(file, fs.constants.X_OK); return file; } } catch { /* next entry */ }
  }
  return null;
}

// Name, version and bytes of the client that would run. `run` is injectable for tests.
function probeClient(name, { env = process.env, run = spawnSync } = {}) {
  if (!NAMES.includes(name)) throw new Error('unknown client');
  const found = onPath(name, env);
  if (!found) return { name, available: false, reason: name + ' is not installed (not on PATH)' };
  const out = run(found, ['--version'], { env: scrubbedEnv(env), encoding: 'utf8', timeout: 30000 });
  const text = String((out && out.stdout) || '').trim().split('\n')[0] || '';
  const version = (text.match(/\d+\.\d+\.\d+[\w.-]*/) || [])[0];
  if (!out || out.error || out.status !== 0 || !version) return { name, available: false, reason: name + ' did not report a version', path: found };
  let real = found, bytes = null;
  try { real = fs.realpathSync(found); bytes = digest(fs.readFileSync(real)); } catch { /* version and path still identify it */ }
  return { name, available: true, version, versionText: text.slice(0, 120), path: found, realPath: real, sha256: bytes };
}

// The command line, the standard input (the exact prompt) and the private files for one invocation. Pure.
// `tag` names one invocation of several in the same report directory (a session of the workflow, the identification session); without it the
// private files keep their plain names.
// The identification session (`identify`) reads the repository documentation, so Claude Code gets `Read` (the repository folder added) and a shell
// limited to `shasum` and `ps`, the two commands the documented rule uses; the workflow sessions keep MCP tools only. Codex already has its read-only shell.
function invocation(name, { command, mcpCommand, mcpRegistry, prompt, cwd, reportDir, maxTurns = 80, model, tag, identify = false, docsRoot }) {
  const asked = model === undefined ? requestedModel(name) : model;
  if (name === 'codex') {
    const lastMessage = path.join(reportDir, tag ? 'codex.' + tag + '.last-message.txt' : 'codex.last-message.txt');
    const set = (key, value) => ['-c', key + '=' + value];
    return { command, input: prompt, cwd, lastMessage, model: asked, files: {}, args: ['exec', '--json', '--skip-git-repo-check', '--sandbox', 'read-only', '--cd', cwd,
      ...(asked === CLIENT_DEFAULT ? [] : ['--model', asked]),
      ...set('approval_policy', '"never"'),
      ...set('mcp_servers.nemo.command', JSON.stringify(mcpCommand)), ...set('mcp_servers.nemo.args', '[]'),
      ...set('mcp_servers.nemo.env.NEMO_MCP_REGISTRY', JSON.stringify(mcpRegistry)),
      ...set('mcp_servers.nemo.default_tools_approval_mode', '"approve"'), ...set('mcp_servers.nemo.startup_timeout_sec', '60'), ...set('mcp_servers.nemo.tool_timeout_sec', '300'),
      '--output-last-message', lastMessage, '-'] };
  }
  const config = path.join(reportDir, tag ? 'claude.' + tag + '.mcp-config.json' : 'claude.mcp-config.json');
  return { command, input: prompt, cwd, lastMessage: null, model: asked,
    files: { [config]: JSON.stringify({ mcpServers: { nemo: { command: mcpCommand, args: [], env: { NEMO_MCP_REGISTRY: mcpRegistry } } } }, null, 2) + '\n' },
    args: ['-p', '--model', asked, '--mcp-config', config, '--strict-mcp-config', '--tools', identify ? 'Read,Bash' : '', ...(identify && docsRoot ? ['--add-dir', docsRoot] : []),
      '--allowedTools', 'mcp__nemo__nemo_discover', 'mcp__nemo__nemo_query', 'mcp__nemo__nemo_command', ...(identify ? ['Read', 'Bash(shasum:*)', 'Bash(ps:*)'] : []),
      '--output-format', 'stream-json', '--verbose', '--max-turns', String(maxTurns), '--no-session-persistence'] };
}

// Run it. Resolves with how it ended; rejects never. `spawnImpl` is injectable for tests.
function runClient(spec, { timeoutMs = 20 * 60 * 1000, env = process.env, spawnImpl = spawn } = {}) {
  for (const [file, text] of Object.entries(spec.files || {})) fs.writeFileSync(file, text, { flag: 'wx', mode: 0o600 });
  return new Promise(resolve => {
    const started = Date.now();
    const out = { stdout: [], stderr: [], size: 0, truncated: false };
    let child, timedOut = false, settled = false;
    const finish = result => { if (settled) return; settled = true; clearTimeout(timer); resolve({ ms: Date.now() - started, timedOut, truncated: out.truncated,
      stdout: Buffer.concat(out.stdout).toString('utf8'), stderr: Buffer.concat(out.stderr).toString('utf8'), ...result }); };
    const keep = which => chunk => { if (out.size + chunk.length > LIMIT) { out.truncated = true; return; } out.size += chunk.length; out[which].push(chunk); };
    try { child = spawnImpl(spec.command, spec.args, { cwd: spec.cwd, env: scrubbedEnv(env), stdio: ['pipe', 'pipe', 'pipe'] }); }
    catch (error) { return resolve({ ms: 0, spawnError: String(error.code || error.name), stdout: '', stderr: '', timedOut: false, truncated: false }); }
    const timer = setTimeout(() => { timedOut = true; child.kill('SIGTERM'); setTimeout(() => { try { child.kill('SIGKILL'); } catch { /* gone */ } }, 5000).unref(); }, timeoutMs);
    child.stdout.on('data', keep('stdout')); child.stderr.on('data', keep('stderr'));
    child.on('error', error => finish({ spawnError: String(error.code || error.name) }));
    child.on('close', (exitCode, signal) => finish({ exitCode, signal }));
    child.stdin.on('error', () => {}); child.stdin.end(spec.input);
  });
}

// Never a pass on its own: `unavailable` makes the half unrun; anything else is judged from the application's state.
const AUTH = /not logged in|log ?in required|please (run )?\S*\s*(log ?in|auth)|unauthori[sz]ed|authentication|invalid (api )?key|api key|credentials|\b401\b|sign in/i;
const QUOTA = /quota|rate.?limit|usage limit|limit reached|too many requests|\b429\b|insufficient|credit balance|billing|exceeded/i;
const FLAGS = /unknown (option|argument|flag)|unexpected argument|unrecognized (option|argument)|invalid value for|no such option/i;
function classify(result) {
  if (result.spawnError) return { kind: 'unavailable', reason: 'the client could not be started (' + result.spawnError + ')' };
  const text = (result.stderr + '\n' + result.stdout.slice(-20000));
  if (result.exitCode !== 0 && !result.timedOut) {
    if (AUTH.test(text)) return { kind: 'unavailable', reason: 'the client is not authenticated' };
    if (QUOTA.test(text)) return { kind: 'unavailable', reason: 'the client is over its quota or rate limit' };
    if (FLAGS.test(text)) return { kind: 'unavailable', reason: 'the client rejected the harness command line (flag mismatch with this client version)' };
    return { kind: 'failed', reason: 'the client exited with status ' + result.exitCode };
  }
  return result.timedOut ? { kind: 'timeout', reason: 'the client did not finish within the time limit' } : { kind: 'ok', reason: null };
}

// Informational only: tool calls and application error codes the client's own event stream shows, and its last message.
const CODES = ['stale_revision', 'wrong_document', 'wrong_instance', 'invalid_request', 'unavailable', 'unsupported_capability'];
const eventsOf = stdout => {
  const out = [];
  for (const line of String(stdout).split('\n')) {
    if (!line.startsWith('{')) continue;
    try { out.push(JSON.parse(line)); } catch { /* not an event */ }
  }
  return out;
};
const callOf = (tool, input) => {
  const args = input && typeof input === 'object' ? input : {}, payload = args.payload && typeof args.payload === 'object' ? args.payload : {};
  return { tool: 'nemo_' + tool, operation: typeof args.operation === 'string' ? args.operation : null, path: typeof payload.path === 'string' ? payload.path : null };
};
// The client's own MCP calls to the nemo server, in order, one entry per call, from its stream. Claude Code streams `tool_use` blocks in assistant
// messages (tool `mcp__nemo__nemo_command`); Codex streams `mcp_tool_call` items (tool `nemo_command`), once started and once completed: only the
// completed item counts. Calls to any other server or tool are not counted.
function mcpCalls(stdout) {
  const calls = [];
  for (const e of eventsOf(stdout)) {
    if (e.type === 'assistant' && e.message && Array.isArray(e.message.content)) {
      for (const block of e.message.content) {
        const hit = block && block.type === 'tool_use' && typeof block.name === 'string' ? block.name.match(/(?:^|__)nemo_(discover|query|command)$/) : null;
        if (hit) calls.push(callOf(hit[1], block.input));
      }
    } else if (e.type === 'item.completed' && e.item && e.item.type === 'mcp_tool_call') {
      const hit = typeof e.item.tool === 'string' && (e.item.server === undefined || e.item.server === 'nemo') ? e.item.tool.match(/^nemo_(discover|query|command)$/) : null;
      if (hit) calls.push(callOf(hit[1], e.item.arguments));
    }
  }
  return calls;
}
// Calls per step. `steps` is [{n, label, match(call)}]: step 1 starts the session (no marker); each later step starts at the first call its `match`
// accepts AFTER the previous step started, strictly in order, and owns every call until the next step starts. A client that skips or reorders a step
// leaves its calls on the earlier step, so the totals are always right and the split is only as good as the client's order (informational).
function stepCounts(calls, steps) {
  const out = steps.map(step => ({ step: step.n, label: step.label, calls: 0, byTool: {} }));
  let current = 0;
  for (const call of calls) {
    if (current + 1 < steps.length && steps[current + 1].match(call)) current++;
    out[current].calls++; out[current].byTool[call.tool] = (out[current].byTool[call.tool] || 0) + 1;
  }
  return out;
}
// What the application told the client when it asked what is running: each `nemo_discover` answer in the stream, reduced to the open document.
function discoverReplies(stdout) {
  const texts = [];
  for (const e of eventsOf(stdout)) {
    if (e.type === 'user' && e.message && Array.isArray(e.message.content)) {
      for (const block of e.message.content) {
        if (!block || block.type !== 'tool_result') continue;
        if (typeof block.content === 'string') texts.push(block.content);
        else if (Array.isArray(block.content)) for (const part of block.content) if (part && typeof part.text === 'string') texts.push(part.text);
      }
    } else if (e.type === 'item.completed' && e.item && e.item.type === 'mcp_tool_call' && e.item.result && Array.isArray(e.item.result.content)) {
      for (const part of e.item.result.content) if (part && typeof part.text === 'string') texts.push(part.text);
    }
  }
  const out = [];
  for (const text of texts) {
    if (!text.includes('"instances"')) continue;
    let reply; try { reply = JSON.parse(text); } catch { continue; }
    const [instance] = Array.isArray(reply && reply.instances) ? reply.instances : [];
    if (!instance || typeof instance !== 'object') continue;
    const native = instance.nativeHostStatus && typeof instance.nativeHostStatus === 'object' ? instance.nativeHostStatus : {};
    out.push({ instanceId: instance.instanceId ?? null, buildId: instance.buildId ?? null, documentId: native.documentId ?? instance.documentId ?? null,
      revision: Number.isSafeInteger(native.contentRevision) ? native.contentRevision : (Number.isSafeInteger(instance.revision) ? instance.revision : null) });
  }
  return out;
}
// The model the client really used: Claude reports it in its `init` event; Codex's stream carries none, so its configuration names it (only the top-level
// `model` and `model_reasoning_effort` keys are read, nothing else of the file is kept). `requested` is what the harness asked for ('client default' = nothing).
function modelUsed(name, stdout, { requested, env = process.env, readFile = fs.readFileSync, home = require('node:os').homedir() } = {}) {
  for (const e of eventsOf(stdout)) {
    const field = name === 'claude' ? (e.type === 'system' && e.subtype === 'init' ? e.model : undefined) : (e.model || (e.item && e.item.model) || (e.turn && e.turn.model));
    if (typeof field === 'string' && field) return { value: field, source: 'the client\'s own stream' + (name === 'claude' ? ' (init event)' : '') };
  }
  if (name === 'codex' && requested && requested !== CLIENT_DEFAULT) return { value: requested, source: 'requested with --model (not echoed in the stream)' };
  if (name === 'codex') {
    const file = path.join(env.CODEX_HOME || path.join(home, '.codex'), 'config.toml');
    try {
      const keys = {};
      for (const line of String(readFile(file, 'utf8')).split('\n')) {
        if (/^\s*\[/.test(line)) break;
        const hit = line.match(/^\s*(model|model_reasoning_effort)\s*=\s*"([^"\n]{1,80})"/);
        if (hit) keys[hit[1]] = hit[2];
      }
      if (keys.model) return { value: keys.model, ...(keys.model_reasoning_effort ? { reasoningEffort: keys.model_reasoning_effort } : {}), source: 'the Codex configuration file (' + file.replace(home, '~') + ', top-level model key; the stream does not name it)' };
    } catch { /* no readable configuration */ }
  }
  return { value: null, source: null };
}
function trace(result, lastMessageFile, steps) {
  const codes = {}, calls = mcpCalls(result.stdout), byTool = {};
  for (const call of calls) byTool[call.tool] = (byTool[call.tool] || 0) + 1;
  let last = null;
  for (const event of eventsOf(result.stdout)) if (event.type === 'result' && typeof event.result === 'string') last = event.result;
  for (const line of result.stdout.split('\n')) {
    if (!line.startsWith('{')) continue;
    for (const code of CODES) { const n = (line.match(new RegExp('\\\\?"code\\\\?":\\\\?"' + code + '\\\\?"', 'g')) || []).length; if (n) codes[code] = (codes[code] || 0) + n; }
  }
  if (lastMessageFile) { try { last = fs.readFileSync(lastMessageFile, 'utf8'); } catch { /* none written */ } }
  return { informational: true, toolCalls: byTool, mcpCallCount: calls.length, ...(steps ? { stepCalls: stepCounts(calls, steps) } : {}), errorCodesSeen: codes,
    lastMessage: last === null ? null : String(last).slice(0, 4000) };
}

module.exports = { mcpCalls, stepCounts, discoverReplies, modelUsed, eventsOf, NAMES, DEFAULT_CLAUDE_MODEL, CLIENT_DEFAULT, validModel, requestedModel, scrubbedEnv, onPath, probeClient, invocation, runClient, classify, trace, digest };
