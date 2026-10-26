'use strict';
// Original N20R fixture, trace and pixel oracle; no replacement product oracle.
const fs = require('node:fs'), path = require('node:path');
const { isDeepStrictEqual: same } = require('node:util');
const { requireCheck: check, hash } = require('../native-harness.cjs');
const { read } = require('../cs7-play-evidence.cjs');
const oracle = require('../play-publication-oracle.cjs');
const FIXTURE = 'de1c3782038c51ea04f59978808be895479986d95d9655d9b79e6556f324cf32';
const CLAUSES = ['visible-recovery', 'ordered-telemetry', 'visible-frames'];
function prepare({ config }) {
  const fixture = config.play?.fixture;
  check(config.playObserver === true && typeof fixture === 'string' && path.isAbsolute(fixture) &&
    fs.realpathSync(fixture) === fixture && hash(fixture) === FIXTURE, 'original qualified Play fixture required');
  return createWorkflow({ fixture, reportDirectory: config.reportDirectory });
}
function createWorkflow({ fixture, reportDirectory }) {
  let used = false;
  return { async run({ session, row }) {
    check(!used, 'Play scenario cannot restart'); used = true;
    check(row.issue === 1573 && same(row.checks.map(c => c.id), CLAUSES), 'original Play clauses changed');
    const owner = await session.playContext(), visuals = [];
    const phases = [
      ['open', 'Open the unchanged original keyed fixture through ordinary UI, enter Motion and observe its rectangle. Do not invoke direct preview or product JavaScript.'],
      ['play-minimize', 'Use ordinary Play, then minimize the owned main window while playing. Observe the real minimized state; do not synthesize exposure or receipts.'],
      ['restore-play-stop', 'Restore that same owned window; exercise ordinary Play then Stop and observe visible recovery. Record any toast or failure unchanged.'],
      ...[0, 10, 20].map(frame => ['frame-' + frame, 'Use ordinary Motion frame navigation to frame' + frame + '; independently review the owned window PNG, displayed frame, position and opacity. Supply the original oracle visual record (file relative to the private report directory) with the actual publication linkage. No guessed native identities.']),
    ];
    for (const [phase, instruction] of phases) {
      check(same(await session.playContext(), owner), 'Play owner changed');
      const raw = await session.requestEvidence({ row: row.id, phase, instruction,
        details: { fixture, reportDirectory, sourceSha: owner.identity.sourceSha,
          pid: owner.identity.pid, windowId: owner.windowId, runId: owner.runId,
          oracle: 'tests/desktop/play-publication-oracle.cjs',
          limit: 'Retain raw observations. Source tests and posted input do not establish PASS.' } });
      fs.writeFileSync(path.join(reportDirectory, 'play-' + phase + '.json'), JSON.stringify(raw, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
      // A reported product failure terminates the workflow; missing testimony
      // cannot be interpreted as successful ordinary UI navigation.
      check(typeof raw.observation === 'string' && raw.observation.trim() && typeof raw.productFailure === 'boolean', 'Play observation incomplete');
      if (raw.productFailure) return { checks: CLAUSES.map(id => ({ id, status: id === 'visible-recovery' ? 'fail' : 'unrun' })), safeToContinue: false };
      if (phase.startsWith('frame-')) {
        const v = raw.visual;
        check(v && v.frame === Number(phase.slice(6)) && typeof v.file === 'string' && !path.isAbsolute(v.file), 'Play visual witness missing');
        const file = path.resolve(reportDirectory, v.file), relative = path.relative(reportDirectory, file);
        check(relative && !relative.startsWith('..') && !path.isAbsolute(relative), 'Play visual path refused');
        // Bytes are copied now and retained in memory, so a later row cannot
        // replace the original witnessed screenshot before final validation.
        visuals.push({ ...v, png: read(file) });
      }
    }
    return { checks: CLAUSES.map(id => ({ id, status: 'unrun' })), safeToContinue: true,
      finalize: async () => {
        const retained = session.retainedPlayEvidence();
        check(retained.sourceCommit === owner.identity.sourceSha && retained.pid === owner.identity.pid &&
          retained.taskId === owner.identity.sessionId && retained.windowId === owner.windowId &&
          retained.runId === owner.runId, 'terminal Play owner changed');
        const result = oracle.verifyEvidence({ ...retained, fixtureSha256: FIXTURE, visuals });
        fs.writeFileSync(path.join(reportDirectory, 'play-oracle-result.json'), JSON.stringify(result, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
        return CLAUSES.map(id => ({ id, status: result.status === 'consistent' ? 'pass' : 'fail' }));
      } };
  } };
}
module.exports = { prepare, createWorkflow, FIXTURE };
