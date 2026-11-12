'use strict';
// The candidate: an already built package, started once in a fresh isolated session, bound by identity to what the runner
// talks to. Nothing here builds, and nothing is accepted on a name: the package proof lists the app executable and the bundled
// nemo-mcp by sha256, the registered build id is the app version plus the clean source head, and the process that listens on
// the registered port is the process this runner started.
//
// Reused, not reimplemented: scripts/nemo/native.cjs start/stop through native-harness.createController, the package-proof
// pin of the bundled nemo-mcp (requireMcpPinned) and the registry and listener reads of the CS7 read route (cs7-read.cjs).
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const harness = require('../native-harness.cjs');
const { requireCheck: check, hash } = harness;
const { requireMcpPinned } = require('../cs7-session.cjs');
const { readRegistration, listenerPids } = require('../cs7-read.cjs');
const ROOT = path.resolve(__dirname, '../../..');

// Package proof against the clean checkout and the very bytes that will run. Pure: every input is a value.
function verifyPackage({ proof, source, executableSha256, mcpSha256 }) {
  check(proof && proof.status === 'pass' && proof.details && proof.details.source, 'package proof must be a passing standard build proof');
  check(!source.dirty && /^[0-9a-f]{40}$/.test(source.head || ''), 'clean committed source required');
  check(proof.details.source.head === source.head && proof.details.source.dirty === false, 'package proof is for another source head');
  check(proof.details.cleanup && proof.details.cleanup.stopped === true && proof.details.cleanup.released === true, 'package proof lacks a completed build cleanup');
  const artifacts = Array.isArray(proof.artifacts) ? proof.artifacts : [];
  const listed = suffix => artifacts.find(a => a && typeof a.path === 'string' && a.path.endsWith(suffix));
  const app = listed('/Contents/MacOS/nemo'), mcp = listed('/Contents/MacOS/nemo-mcp');
  check(app && /^[0-9a-f]{64}$/.test(app.sha256 || '') && app.sha256 === executableSha256, 'app executable differs from the package proof');
  check(mcp && /^[0-9a-f]{64}$/.test(mcp.sha256 || '') && mcp.sha256 === mcpSha256, 'bundled nemo-mcp differs from the package proof');
  return { executableSha256, mcpSha256 };
}

// The five bindings of an MCP endpoint to the process this runner started. Pure given the reads.
//   record: registry record {instanceId, port, buildId}; discovered: the nemo_discover instance list;
//   listeners: pids listening on record.port; identity: {pid, sourceHead, appVersion}.
function verifyBinding({ record, discovered, listeners, identity }) {
  check(record && typeof record.buildId === 'string', 'candidate registration required');
  check(/^[0-9a-f]{40}$/.test(identity.sourceHead || '') && record.buildId === identity.appVersion + ':' + identity.sourceHead, 'registered build is not the candidate build');
  check(Array.isArray(discovered) && discovered.length === 1, 'exactly one discovered application required');
  const [instance] = discovered;
  check(instance && instance.instanceId === record.instanceId && instance.buildId === record.buildId, 'discovered application is not the candidate');
  check(Array.isArray(listeners) && listeners.length === 1 && listeners[0] === identity.pid, 'MCP endpoint is not owned by the process this runner started');
  return { instanceId: record.instanceId, buildId: record.buildId, port: record.port, pid: identity.pid };
}

const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

async function openCandidate(config, deps = {}) {
  const identityLib = deps.identity || require('../../../scripts/nemo/lib/identity.cjs');
  const runtime = deps.runtime || require('../../../scripts/nemo/lib/native-runtime.cjs');
  const isolation = deps.isolation || require('../../../scripts/nemo/lib/isolation.cjs');
  const McpClient = deps.McpClient || require('../../mcp-parity/mcp-client.cjs').McpClient;
  const source = identityLib.sourceIdentity(), build = identityLib.buildIdentity();
  check(!source.dirty, 'clean committed source required');
  check(config.sourceSha === undefined || config.sourceSha === source.head, 'source head differs from the frozen source');
  const app = harness.packagedApp(config.app, runtime);
  const mcpBinary = path.join(app.bundle, 'Contents', 'MacOS', 'nemo-mcp');
  const mcpStat = fs.lstatSync(mcpBinary);
  check(mcpStat.isFile() && !mcpStat.isSymbolicLink() && (mcpStat.mode & 0o111), 'packaged nemo-mcp executable required');
  const proofBytes = fs.readFileSync(config.buildProof);
  const proof = JSON.parse(proofBytes.toString('utf8'));
  const mcpSha = hash(mcpBinary);
  verifyPackage({ proof, source, executableSha256: app.executableSha256, mcpSha256: mcpSha });
  requireMcpPinned(proof, mcpBinary);
  const taskId = config.taskId || 'installed-evidence-' + Date.now().toString(36) + '-' + crypto.randomBytes(3).toString('hex');
  check(!fs.existsSync(isolation.taskRoot(taskId)), 'a fresh isolated data directory is required: this task id already has one');
  const env = { ...process.env };
  for (const key of Object.keys(env)) if (/^NEMO_(UI32|P38|REFERENCE|PNG_UI|TEST)_/.test(key)) delete env[key];
  const controller = (deps.createController || harness.createController)({ root: ROOT, app: app.bundle, env });
  let client = null, instance = null, cleaned = false;
  async function cleanup() {
    if (cleaned) return; cleaned = true;
    let failed = false;
    if (client) { try { await client.close(); } catch { failed = true; } }
    try { await controller.cleanup(); } catch { failed = true; }
    const pid = instance && instance.snapshot && instance.snapshot.app && instance.snapshot.app.pid;
    if (pid && harness.alive(pid)) failed = true;
    check(!failed, 'installed session cleanup incomplete; reconcile this run before retrying');
  }
  try {
    instance = await controller.start(taskId, config.reserve === undefined ? ['gpu-reference'] : config.reserve);
    const status = await controller.status(instance);
    check(status.code === 0, 'candidate status refused');
    harness.validateSnapshot(status.value, instance, { isolation, runtime, source, build, app });
    const snapshot = status.value, pid = snapshot.app.pid;
    const registry = path.join(snapshot.runtime.roots.tauriDataDir, 'mcp');
    check(fs.existsSync(registry), 'the candidate has not registered its MCP endpoint');
    client = new McpClient(mcpBinary, [], { NEMO_MCP_REGISTRY: registry });
    const init = await client.initialize();
    check(init && init.result && init.result.protocolVersion, 'bundled nemo-mcp did not initialize');
    const { structured } = require('../cs7-read.cjs');
    const identity = { pid, sourceHead: source.head, appVersion: snapshot.app.appVersion };
    async function bind() {
      const record = readRegistration(fs.realpathSync(registry));
      const found = structured(await client.callTool('nemo_discover', {}));
      return verifyBinding({ record, discovered: found.instances, listeners: await (deps.listeners || listenerPids)(record.port), identity });
    }
    const bound = await bind();
    const work = fs.realpathSync(snapshot.runtime.roots.temp);
    return {
      client, work, bound, bind, cleanup,
      // How another process (an agent client) reaches the very same endpoint: the bundled nemo-mcp and the isolated registry it reads.
      mcpCommand: mcpBinary, mcpRegistry: fs.realpathSync(registry),
      // Same process, same checkout, same bytes, same registration: refused when anything differs from the start.
      async recheck() {
        check(identityLib.sourceIdentity().head === source.head && !identityLib.sourceIdentity().dirty, 'source changed during the run');
        check(hash(app.executable) === app.executableSha256 && hash(mcpBinary) === mcpSha, 'candidate executable changed during the run');
        const again = await controller.status(instance);
        check(again.code === 0 && again.value && again.value.app && again.value.app.pid === pid, 'candidate process changed during the run');
        const now = await bind();
        check(now.instanceId === bound.instanceId && now.buildId === bound.buildId, 'candidate instance changed during the run');
        return now;
      },
      identity: { sourceHead: source.head, tree: tree(), appVersion: snapshot.app.appVersion, buildId: bound.buildId, instanceId: bound.instanceId, pid, taskId,
        executableSha256: app.executableSha256, mcpSha256: mcpSha, proofSha256: sha256(proofBytes), app: path.basename(app.bundle) },
    };
  } catch (error) {
    try { await cleanup(); } catch { /* the original failure is the one to report */ }
    throw error;
  }
}
function tree() {
  try { return require('node:child_process').execFileSync('git', ['rev-parse', 'HEAD^{tree}'], { cwd: ROOT, encoding: 'utf8' }).trim(); } catch { return null; }
}

module.exports = { verifyPackage, verifyBinding, openCandidate };
