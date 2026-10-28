'use strict';
// OFFLINE evidence driver. Never launches, builds, captures or drives a product.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const zlib = require('node:zlib');
const { decode } = require('../fixtures/lib/png.cjs');
const { srgbPixelReader } = require('./play-png-color.cjs');
const MAX_BYTES = 32 * 1024 * 1024;
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const safe = n => Number.isSafeInteger(n) && n >= 0;
const sha = s => typeof s === 'string' && /^[0-9a-f]{64}$/.test(s);
const uuid = s => typeof s==='string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(s);
const pins = ['instanceId','documentId','contentRevision','documentSnapshotId','contextId','frame',
  'jsLifecycleGeneration','hostGeneration','sessionOrdinal','hostCallOrdinal'];
const same = (a,b,keys = pins) => keys.every(k => a[k] !== undefined && a[k] === b[k]);
function requireEvidence(condition, code) { if (!condition) throw Object.assign(new Error(code), { code }); }
function parse(bytes, expected) {
  requireEvidence(Buffer.isBuffer(bytes) && bytes.length <= MAX_BYTES && bytes.at(-1) === 10, 'incomplete-log');
  const records = [], native = [];
  for (const line of bytes.toString('utf8').split('\n')) {
    if (line.startsWith('NEMO_PLAY_OBSERVER ')) records.push(JSON.parse(line.slice(19)));
    else if (line.startsWith('{')) { const r = JSON.parse(line); if (r.diagnostic === 'native-occlusion-mapping/v1') native.push(r); }
  }
  requireEvidence(records.length >= 4 && records.length <= 4099, 'framing-count');
  const header = records.shift(), footer = records.pop(), cut = records.pop();
  requireEvidence(header.kind === 'header' && header.schema === 'nemo.play-observer/1' && header.window === 'main' &&
    header.taskId === expected.taskId && header.pid === expected.pid, 'owner-header');
  requireEvidence([header,...records,cut,footer].every(r => r.runId === expected.runId), 'run-mismatch');
  requireEvidence(records.every((r,i) => r.kind === 'event' && r.sequence === i+1 && r.event &&
    (i===0) === (r.event.event==='ready')), 'event-order');
  requireEvidence(cut.kind === 'seal' && cut.status === 'provisional' && footer.kind === 'footer' &&
    uuid(cut.cutId) && footer.cutId === cut.cutId && footer.terminalFence === true && footer.status === 'complete' && footer.pending === 0,
    'terminal-fence');
  for (const r of [cut,footer]) requireEvidence(r.accepted === records.length && r.generated === records.length &&
    r.dropped === 0 && r.failed === 0 && r.rejected === 0, 'partial-collection');
  requireEvidence(native.length > 0 && native.length < 4096 && native.every((r,i) => r.sequence === i+1 && r.event !== 'truncated'), 'native-trace');
  return { events:records.map(r=>r.event), native };
}
function semantics(events) {
  const calls = new Map(), navs = new Map(), latest = new Map(), steps = new Map();
  let playing = false, sawStart = false, sawStop = false, publications = 0, failures = 0, activeRun = null, stopGate = null, playSteps = 0;
  const navKey = e => `${e.instanceId}|${e.documentId}|${e.intent}`;
  const nativeKey = e => `${e.instanceId}|${e.hostCallOrdinal}`;
  const stages = ['commit-enter','ui-publication','commit-return','paint-enter','paint-return','sync-enter','sync-return'];
  const passive = new Set(['ready','receipt-rejected','receipt-deferred','receipt-stale']);
  const workPins = pins.concat(['workId','viewGeneration','status']);
  const validWork = e => typeof e.workId==='string' && /^native-work-[1-9][0-9]{0,19}$/.test(e.workId) &&
    BigInt(e.workId.slice(12))<=18446744073709551615n && safe(e.viewGeneration) && e.viewGeneration>0;
  for (const e of events) {
    if (e.event === 'host-request') {
      requireEvidence(pins.every(k=>e[k]!==undefined) && e.hostCallOrdinal>0 && e.sessionOrdinal>0 && e.hostGeneration>0 &&
        e.jsLifecycleGeneration>0 && !calls.has(nativeKey(e)), 'host-context');
      calls.set(nativeKey(e), { request:e });
    } else if (e.event === 'host-receipt') {
      const c = calls.get(nativeKey(e)); requireEvidence(c && !c.receipt && same(c.request,e), 'raw-receipt-context'); c.receipt=e;
    } else if (e.event === 'presented-return') {
      const c = calls.get(nativeKey(e)); requireEvidence(c?.receipt?.status==='presented' && e.status==='presented' &&
        same(c.receipt,e,pins.concat(['workId','viewGeneration'])), 'presented-context'); c.presented=true;
    } else if (e.event === 'navigate-request') {
      requireEvidence(safe(e.intent) && e.intent>0 && safe(e.frame) && !navs.has(navKey(e)), 'navigation-request');
      const n={ request:e, stage:0, done:false, rollback:false };
      const pending=[...steps.values()].filter(s=>s.pending&&!s.nav&&same(s.request,e,['instanceId','documentId','contentRevision','frame']));
      requireEvidence(pending.length<=1,'ambiguous-play-step');if(pending.length)n.step=pending[0],pending[0].nav=n;
      if(stopGate){requireEvidence(stopGate.schedulerStopped&&same(stopGate,e,['instanceId','documentId','contentRevision','frame']), 'stop-restore-context');n.stopRestore=true;}
      navs.set(navKey(e),n); latest.set(e.instanceId,e.intent);
    } else if (stages.includes(e.event)) {
      const n = navs.get(navKey(e)), c = calls.get(nativeKey(e));
      requireEvidence(n && !n.done && !n.rollback && latest.get(e.instanceId)===e.intent && c?.presented &&
        same(c.receipt,e,pins.concat(['workId','viewGeneration'])) &&
        same(n.request,e,['instanceId','documentId','contentRevision','frame']), 'publication-context');
      requireEvidence(stages[n.stage]===e.event, 'publication-order'); n.stage++; n.pins=e;
      if (e.event==='ui-publication') requireEvidence(e.currentFrame===e.frame, 'actual-ui-frame');
      if (e.event==='sync-return') requireEvidence(e.currentFrame===e.frame && e.result===true, 'actual-input-frame');
    } else if (e.event==='receipt-retry') {
      const n=navs.get(navKey(e)),c=calls.get(nativeKey(e));
      requireEvidence(n && !n.done && !n.superseded && latest.get(e.instanceId)===e.intent && n.rollback &&
        typeof e.result==='boolean' && safe(e.priorFrame) && e.priorFrame===n.rollbackFrame && validWork(e), 'restore-intent');
      if(e.result===false){
        requireEvidence(!n.restore && c?.receipt && same(c.receipt,e,workPins) &&
          same(n.request,e,['instanceId','documentId','contentRevision','frame']), 'restore-cause');
        n.restore={cause:e,afterCall:Math.max(...[...calls.values()].map(c=>c.request.hostCallOrdinal))};
      }else{
        requireEvidence(n.restore && !n.restore.presented && c?.presented && e.status==='presented' &&
          e.frame===n.rollbackFrame && e.hostCallOrdinal>n.restore.afterCall && same(c.receipt,e,workPins) &&
          same(n.restore.cause,e,pins.filter(k=>k!=='frame'&&k!=='hostCallOrdinal')), 'restore-receipt');
        n.restore.presented=e;
      }
    } else if (e.event==='rollback') {
      const n=navs.get(navKey(e)); requireEvidence(n && !n.done &&
        same(n.pins||n.request,e,['instanceId','documentId','contentRevision']) &&
        safe(e.frame) && e.currentFrame===e.frame, 'rollback-context');
      if(e.result===undefined){requireEvidence(!n.rollback,'duplicate-rollback');n.rollback=true;n.rollbackFrame=e.frame;}
      else{
        requireEvidence(e.result===true && !n.restored && n.restore?.presented && !n.superseded &&
          latest.get(e.instanceId)===e.intent && e.frame===n.rollbackFrame && e.priorFrame===n.rollbackFrame &&
          same(n.restore.presented,e,workPins), 'restore-acknowledgement');n.restored=true;
      }
    } else if (e.event==='failure') {
      const n=navs.get(navKey(e)), c=calls.get(nativeKey(e));
      requireEvidence(n && !n.done && c && same(c.request,e) &&
        same(n.request,e,['instanceId','documentId','contentRevision','frame']), 'failure-context');
      if (e.workId!==undefined) requireEvidence(c.receipt && same(c.receipt,e,pins.concat(['workId','viewGeneration','status'])), 'failure-work');
      n.failed=true; n.failure=e; failures++;
      if(n.restore)requireEvidence(same(n.restore.cause,e,workPins),'restore-failure-join');
    } else if (e.event==='navigate-superseded') {
      const n=navs.get(navKey(e)); requireEvidence(n && !n.done && !n.failed && !n.rollback &&
        latest.get(e.instanceId)>e.intent && same(n.request,e,['instanceId','documentId','contentRevision','frame','intent']) &&
        pins.concat(['workId','viewGeneration','status']).filter(k=>!['instanceId','documentId','contentRevision','frame'].includes(k))
          .every(k=>e[k]===undefined), 'superseded-context'); n.superseded=true;
    } else if (e.event==='navigate-return') {
      const n=navs.get(navKey(e)); requireEvidence(n && !n.done && typeof e.result==='boolean', 'terminal-navigation'); n.done=true;n.result=e.result;
      if (e.result) { requireEvidence(!n.rollback && !n.failed && !n.superseded && n.stage===stages.length &&
        latest.get(e.instanceId)===e.intent && same(n.pins,e,pins.concat(['workId','viewGeneration'])), 'false-publication'); publications++; }
      else if(n.failed){
        requireEvidence(n.restore && n.restored && same(n.failure,e,workPins) &&
          same(n.restore.cause,e,workPins),'failed-terminal-context');
      }else requireEvidence(n.superseded && latest.get(e.instanceId)>e.intent &&
        same(n.request,e,['instanceId','documentId','contentRevision','frame','intent']) &&
        pins.concat(['workId','viewGeneration','status']).filter(k=>!['instanceId','documentId','contentRevision','frame'].includes(k))
          .every(k=>e[k]===undefined),'unattributed-failure');
      requireEvidence(!n.rollback || n.restored,'rollback-needs-independent-review');
      if(n.stopRestore){requireEvidence(e.result===true,'stop-restore-failed');stopGate=null;}
    } else if (e.event==='play-state') {
      requireEvidence(typeof e.playing==='boolean'&&safe(e.currentFrame), 'play-state');
      if(e.playing){requireEvidence(activeRun&&!playing&&!stopGate,'play-state-start');sawStart=true;}
      else{requireEvidence(activeRun&&playing&&!stopGate,'play-state-stop');stopGate={...activeRun,frame:e.currentFrame,schedulerStopped:false};sawStop=true;}
      playing=e.playing;
    } else if(e.event==='play-start'){
      requireEvidence(!activeRun&&!stopGate&&safe(e.runGeneration)&&e.runGeneration>0&&!playing,'scheduler-start');activeRun=e;
    } else if(e.event==='play-stop'){
      requireEvidence(activeRun&&stopGate&&!playing&&same(activeRun,e,['instanceId','documentId','contentRevision'])&&
        e.runGeneration>activeRun.runGeneration,'scheduler-stop');stopGate.schedulerStopped=true;activeRun=null;
    } else if (e.event==='play-step') {
      const key=`${e.instanceId}|${e.runGeneration}|${e.frame}`;
      if(e.result===undefined) { requireEvidence(activeRun&&playing&&same(activeRun,e,['instanceId','documentId','contentRevision','runGeneration'])&&
        !steps.get(key)?.pending,'duplicate-play-step');steps.set(key,{request:e,pending:true}); }
      else { const s=steps.get(key);requireEvidence(s?.pending&&s.nav?.done&&s.nav.result===e.result&&typeof e.result==='boolean','unmatched-play-step');s.pending=false;playSteps++; }
    } else requireEvidence(passive.has(e.event), 'unknown-event');
  }
  requireEvidence(sawStart && sawStop && playSteps>0 && !playing && !activeRun&&!stopGate&&[...navs.values()].every(n=>n.done) &&
    ![...steps.values()].some(s=>s.pending), 'product-not-quiescent');
  requireEvidence(publications>0, 'no-publication'); return { publications, failures };
}
function nativeEvidence(native, events, windowId) {
  const exposures=native.filter(r=>r.event==='exposure');
  requireEvidence(exposures.every(r=>r.fields?.windowNumber===windowId), 'native-window');
  const minimized=exposures.find(r=>r.fields.minimized===true && r.fields.occlusionVisible===false);
  const restored=minimized && exposures.find(r=>r.sequence>minimized.sequence && r.exposureEpoch>minimized.exposureEpoch &&
    r.fields.minimized===false && r.fields.occlusionVisible===true);
  requireEvidence(minimized && restored, 'no-minimize-restore');
  const ends=native.filter(r=>r.event==='preview-end');
  for(const e of events.filter(e=>e.event==='host-receipt')) {
    requireEvidence(typeof e.workId==='string' && /^native-work-[1-9][0-9]*$/.test(e.workId), 'work-encoding');
    requireEvidence(ends.some(r=>r.preview && r.preview.instanceId===e.instanceId && r.preview.documentId===e.documentId &&
      r.preview.revision===e.contentRevision && r.preview.frame===e.frame && safe(r.preview.workId) &&
      `native-work-${r.preview.workId}`===e.workId && r.preview.viewGeneration===e.viewGeneration && r.fields?.status===e.status), 'native-js-join');
  }
  requireEvidence(ends.some(r=>r.sequence>minimized.sequence && r.sequence<restored.sequence && r.fields?.status==='deferred-occluded'), 'no-native-occlusion');
  const restoredIds=new Set(ends.filter(r=>r.sequence>restored.sequence && r.fields?.status==='presented').map(r=>`native-work-${r.preview?.workId}`));
  requireEvidence(events.some(e=>e.event==='navigate-return' && e.result===true && restoredIds.has(e.workId)), 'no-restored-publication');
  return restoredIds;
}
function visualEvidence(visuals, expected, events, restoredIds) {
  requireEvidence(Array.isArray(visuals) && visuals.length===3, 'visual-count');
  for(const [frame,x,opacity,color] of [[0,0,20,204],[10,64,50,128],[20,128,80,51]]) {
    const v=visuals.find(v=>v.frame===frame); requireEvidence(v && v.pid===expected.pid && v.windowId===expected.windowId &&
      v.sourceCommit===expected.sourceCommit && v.executableSha256===expected.executableSha256 && v.windowOnly===true &&
      v.reviewed===true && typeof v.reviewer==='string' && v.reviewer.trim() && v.displayedFrame===frame+1 &&
      v.positionX===x && v.opacity===opacity && hash(v.png)===v.sha256, 'visual-identity-review');
    requireEvidence(restoredIds.has(v.workId) && events.some(e=>e.event==='navigate-return' && e.result===true && e.frame===frame &&
      same(e,v,['instanceId','documentId','contentRevision','hostCallOrdinal','workId','viewGeneration'])), 'visual-publication-link');
    const bytes=v.png; requireEvidence(Buffer.isBuffer(bytes) && bytes.length<=MAX_BYTES && bytes.length>33 &&
      bytes.subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10])), 'visual-png');
    const width=bytes.readUInt32BE(16),height=bytes.readUInt32BE(20);
    requireEvidence(width>0 && height>0 && width<=8192 && height<=8192 && width*height<=16_777_216, 'visual-size');
    const idats=[]; for(let offset=8;offset+12<=bytes.length;) {
      const n=bytes.readUInt32BE(offset); requireEvidence(offset+12+n<=bytes.length, 'png-truncated');
      if(bytes.toString('ascii',offset+4,offset+8)==='IDAT')idats.push(bytes.subarray(offset+8,offset+8+n)); offset+=12+n;
    }
    zlib.inflateSync(Buffer.concat(idats),{maxOutputLength:(width*4+1)*height});
    const image=decode(bytes),roi=v.viewport;
    requireEvidence(roi && ['x','y','width','height'].every(k=>safe(roi[k])) && roi.width>0 && roi.height>0 &&
      roi.x+roi.width<=width && roi.y+roi.height<=height, 'visual-viewport');
    const pixel=srgbPixelReader(bytes,image);
    const at=(cx,cy)=>pixel(roi.x+Math.floor(cx*roi.width/320),roi.y+Math.floor(cy*roi.height/180));
    const center=at(30+x,70),outside=at(10+x,70);
    requireEvidence(center.every((n,i)=>Math.abs(n-[255,color,color,255][i])<=4) &&
      outside.every(n=>Math.abs(n-255)<=4), 'visual-pixels');
  }
}
function verifyEvidence(evidence) {
  try {
    const e=evidence;
    requireEvidence(e?.schema==='nemo.n20r-evidence/1' && /^[0-9a-f]{40}$/.test(e.sourceCommit) &&
      sha(e.executableSha256) && sha(e.fixtureSha256) && uuid(e.runId) && typeof e.taskId==='string' && e.taskId.length>0 &&
      safe(e.pid) && e.pid>0 && safe(e.windowId) && e.windowId>0,
      'evidence-identity');
    requireEvidence(e.package?.sourceCommit===e.sourceCommit && e.package.executableSha256===e.executableSha256 &&
      e.package.observerFeature===true && e.package.unsigned===true, 'package-proof');
    requireEvidence(e.collection?.phase==='after-owner-stop' && e.collection.status==='complete' &&
      e.collection.source===e.sourceCommit && e.collection.executableSha256===e.executableSha256 &&
      e.collection.pid===e.pid && e.collection.taskId===e.taskId && e.collection.runId===e.runId &&
      e.collection.stderrSha256===hash(e.stderr), 'retained-copy');
    requireEvidence(e.cleanup?.taskId===e.taskId && e.cleanup.pid===e.pid &&
      ['appExited','launcherExited','processTreeStopped','slotsReleased','taskRemoved'].every(k=>e.cleanup[k]===true), 'cleanup-proof');
    const {events,native}=parse(e.stderr,e),summary=semantics(events),restored=nativeEvidence(native,events,e.windowId); visualEvidence(e.visuals,e,events,restored);
    return {status:'consistent',...summary,limitations:['Recorded evidence is consistent; independent installed review remains required.','This validator never launches or certifies remediation completion.']};
  } catch(error) { return {status:'partial',code:error.code||'invalid-evidence'}; }
}
function readFile(base, relative) {
  requireEvidence(typeof relative==='string' && !path.isAbsolute(relative), 'evidence-path');
  const file=path.resolve(base,relative),rel=path.relative(base,file);
  requireEvidence(rel && !rel.startsWith('..') && !path.isAbsolute(rel), 'evidence-path');
  let current=base; for(const part of rel.split(path.sep)){current=path.join(current,part);requireEvidence(!fs.lstatSync(current).isSymbolicLink(),'evidence-symlink');}
  const stat=fs.statSync(file); requireEvidence(stat.isFile() && stat.size<=MAX_BYTES,'evidence-size'); return fs.readFileSync(file);
}
function runEvidenceFile(file) {
  try {
    const absolute=path.resolve(file),base=fs.realpathSync(path.dirname(absolute));
    requireEvidence(!fs.lstatSync(absolute).isSymbolicLink(),'evidence-symlink');
    const manifest=JSON.parse(readFile(base,path.basename(absolute)));
    manifest.stderr=readFile(base,manifest.stderrFile);
    manifest.visuals=manifest.visuals.map(v=>({...v,png:readFile(base,v.file)}));
    return verifyEvidence(manifest);
  } catch(error){return {status:'partial',code:error.code||'invalid-evidence'};}
}
function plan(){return ['Resolve O1 smoke authority and integrated observer/driver gates first.','Build and identify one exact local observer package; no build is performed here.','Pin fresh owned PID/main/window before ordinary Open/Motion/Play/minimize/restore/Stop.','Finish every product navigation and Stop restore before sealing; keep 4096-event budget.','Preserve records/captures/review and official cleanup, then run this offline evidence check.','Independent installed review decides acceptance; no automatic second launch.'];}
if(require.main===module){const args=process.argv.slice(2);const result=args.length===1&&args[0]==='--plan'?{plan:plan()}:
  args.length===2&&args[0]==='--evidence'?runEvidenceFile(args[1]):{status:'partial',code:'usage'};
  process.stdout.write(JSON.stringify(result)+'\n');process.exitCode=result.status==='partial'?1:0;}
module.exports={verifyEvidence,runEvidenceFile,plan};
