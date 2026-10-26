'use strict';
// Reuse the independently reviewed original actor, SDK collector and all twenty
// phase validators. Private packet paths and runtime evidence stay outside Git.
const fs = require('node:fs'), path = require('node:path');
const { isDeepStrictEqual: same } = require('node:util');
const { hash, requireCheck: check } = require('../native-harness.cjs');
const { OperatorInputError } = require('../cs7-runner.cjs');
const pins = require('./text20-pins.json');
const phases = require('./text20-phases.json');
const HEX = { 'point-create': '#123456', 'point-edit': '#224466', 'area-create': '#336699' };

function packetFile(directory, name) {
  const file = path.join(directory, name), stat = fs.lstatSync(file);
  check(stat.isFile() && !stat.isSymbolicLink() && stat.size <= 4 * 1024 * 1024 &&
    fs.realpathSync(file) === file, 'canonical bounded Text20 packet file required');
  return file;
}
function prepare({ config, root }) {
  const packet = config.text20;
  check(packet && typeof packet.directory === 'string' && path.isAbsolute(packet.directory) &&
    fs.realpathSync(packet.directory) === packet.directory, 'reviewed Text20 packet required');
  for (const [name, expected] of Object.entries(pins.files)) {
    check(hash(packetFile(packet.directory, name)) === expected, 'original Text20 actor/fixture/collector changed');
  }
  const inputsFile = packetFile(packet.directory, 'inputs.json');
  check(/^[a-f0-9]{64}$/.test(packet.inputsSha256) && hash(inputsFile) === packet.inputsSha256,
    'qualified Text20 input binding required');
  check(typeof packet.hexRoute === 'string' && path.isAbsolute(packet.hexRoute) &&
    hash(packetFile(path.dirname(packet.hexRoute), path.basename(packet.hexRoute))) ===
      '1f27f56ee9a3535c5af73185e821829cdaa65c61a72db818e1247cdd7b6c12ff', 'original R4 Hex route required');
  const inputs = JSON.parse(fs.readFileSync(inputsFile));
  check(same(Object.keys(inputs.requiredSourcePins).sort(), [...pins.sourcePaths].sort()), 'Text20 source roles changed');
  check(inputs.fixtureSha256 === pins.files['Initial-text.json'] && inputs.anchorObjectSha256 === pins.files['anchor-object.json'] &&
    same(inputs.font, { fontKey: 'Roboto-Regular', mediaType: 'font/ttf', encodedBytes: 44724,
      encodedSha256: 'e9ff3730797df6aaf76d37c48234a6c4875032eaad8f1aaccd7daefa6dca9e7a' }), 'Text20 fixture/font oracle changed');
  const original = require(packetFile(packet.directory, 'driver.cjs'));
  check(same(original.phases, phases), 'original Text20 phase order changed');
  const c = { appRoot: root, sourceHead: config.sourceSha, app: config.app, buildProof: config.buildProof,
    executableSha256: config.executableSha256, taskId: config.taskId, deadlineMs: packet.deadlineMs,
    reportRoot: path.join(config.reportDirectory, 'text20'), operatorReadRoute: packet.operatorReadRoute,
    mcpSidecar: packet.mcpSidecar, grant: { issue: 2267, confirmed: true, booking: config.grant } };
  // All original source/tree, clean-package, sidecar, grant, fixture and duration
  // checks run before the batch starts. Never call the original separate launcher.
  const modules = original.preflight(c);
  const workflow = createWorkflow({ packet, original, c, modules, inputs });
  return require('./text20-shared.cjs').sharedWorkflow(workflow, {
    config: c,
    inputsSha256: packet.inputsSha256, directory: packet.directory, hexRoute: packet.hexRoute,
  });
}
function createWorkflow({ packet, original, c, modules, inputs }) {
  let used = false;
  return {
    async run({ session, row }) {
      check(!used, 'Text20 cannot restart in a batch'); used = true;
      check(row.issue === 2267 && row.checks.length === 1 && row.checks[0].id === 'original-text20', 'Text20 clause mapping differs');
      const first = await session.runtimeContext();
      check(first.identity.sourceSha === c.sourceHead && first.identity.sessionId === c.taskId, 'Text20 batch owner differs');
      fs.mkdirSync(c.reportRoot, { mode: 0o700 });
      const report = { schema: 'nemo.text-installed-private/1', issue: 2267, status: 'partial',
        installedAcceptance: 'not-established', sourceHead: c.sourceHead, reportRoot: c.reportRoot,
        mcpSidecar: c.mcpSidecar, pid: first.identity.pid, taskId: c.taskId, batchIdentity: first.identity,
        actions: [], phases: phases.map(p => ({ phase: p.name, status: 'unrun' })), limitations: inputs.limits };
      const save = () => fs.writeFileSync(path.join(c.reportRoot, 'receipt.json'), JSON.stringify(report, null, 2) + '\n', { mode: 0o600 });
      const ctx = { ids: {}, geometry: {}, saved: [], pin: null }, deadline = performance.now() + c.deadlineMs;
      save();
      for (const phase of phases) {
        const current = await session.runtimeContext();
        check(same(first.identity, current.identity), 'Text20 installed lifetime changed');
        const runtimeFile = path.join(c.reportRoot, phase.name + '-runtime-private.json');
        fs.writeFileSync(runtimeFile, JSON.stringify(current.snapshot, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
        const budgetMs = Math.min(180_000, Math.floor(deadline - performance.now()));
        if (budgetMs <= 0) throw new OperatorInputError('text20-deadline');
        const instruction = phase.instruction + (HEX[phase.name] ?
          ' For color, use the original R4 route: freshly observe the editable app-owned “Text color hex” field in this Text draft/editor, replace its value once with ' +
          HEX[phase.name] + ', observe, then use the original Apply/Place control. No external color panel, fallback or retry.' : '');
        const evidence = await session.requestEvidence({ row: row.id, phase: phase.name, instruction,
          budgetMs, details: { fixture: path.join(packet.directory, 'Initial-text.json'), packet: packet.directory,
            runtimeFile, reportRoot: c.reportRoot, sourceHead: c.sourceHead, taskId: c.taskId, pid: report.pid, hexRoute: packet.hexRoute,
            note: 'Use the original public SDK collector and complete its subprocess cleanup. Supply raw UI/native evidence; no pass flag.' } });
        const evidenceFile = path.join(c.reportRoot, phase.name + '-evidence.json');
        fs.writeFileSync(evidenceFile, JSON.stringify(evidence, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
        report.inFlight = { phase: phase.name, evidenceFile, sha256: hash(evidenceFile) }; save();
        if (evidence.operatorInputFailure) throw new OperatorInputError('text20-operator-failed');
        try { report.actions.push(original.validatePhase(phase, evidence, ctx, modules, report)); }
        catch {
          report.phases.find(p => p.phase === phase.name).status = 'fail';
          report.failure = { phase: phase.name, classification: 'original-phase-oracle-refused' }; save();
          return { checks: [{ id: 'original-text20', status: 'fail' }], safeToContinue: false,
            phaseResults: structuredClone(report.phases),
            finalize: async () => {
              // Preserve the original failure and unrun tail while recording
              // the same verified cleanup required by earned provider clauses.
              report.cleanup = 'complete'; save();
              return [{ id: 'original-text20', status: 'fail' }];
            } };
        }
        report.phases.find(p => p.phase === phase.name).status = 'pass'; delete report.inFlight; save();
      }
      report.saved = ctx.saved; report.status = 'operator-complete'; report.ownerReview = 'required'; save();
      return { checks: [{ id: 'original-text20', status: 'unrun' }], safeToContinue: true,
        phaseResults: structuredClone(report.phases),
        finalize: async () => {
          // The batch invokes this only after verified normal owner cleanup.
          report.cleanup = 'complete'; save();
          return [{ id: 'original-text20', status: 'pass' }];
        } };
    },
  };
}
module.exports = { prepare, createWorkflow };
