'use strict';
// Shared pure P38 operator model for source-only tests.
const HEAD = 'a'.repeat(40), HASH = 'b'.repeat(64);
function config() { return { workflow:'P38', grant:{issue:1645,confirmed:true,id:'new-p38-explicit-grant'},
  taskId:'p38-synthetic',sourceHead:HEAD,timeoutMs:1000 }; }
function fixture(change = {}) {
  const cfg=config(), events=[], receipts=[]; let fire, attempts=0, cleaned=0, used=false;
  const instance={task:cfg.taskId};
  const snapshot={ok:true,taskId:cfg.taskId,source:{matches:true,startup:{head:HEAD}},build:{matches:true},
    runtime:{childPid:123},app:{pid:123,taskId:cfg.taskId,isolated:true,state:'active'}};
  const actions=[{type:'inspect'},{type:'press',element:'0.1'},{type:'inspect',element:'0.2'},
    {type:'capture'},{type:'stop'}];
  const controller={async start(){events.push('start');attempts++;if(change.start)return change.start(fire);return instance;},
    async status(){events.push('status');return {value:change.snapshot?change.snapshot(structuredClone(snapshot)):structuredClone(snapshot)};},
    async cleanup(){events.push('cleanup');cleaned++;if(change.cleanup)throw Error('/private/secret');}};
  const services={async transport(){events.push('transport');if(typeof change.transport==='function')return change.transport(fire);
    if(change.transport)throw Object.assign(Error('transport'),{code:'transport-handshake-refused'});},
    async package(){events.push('package');if(change.package)throw Error('/private/pkg');return {executableSha256:HASH,proofHash:HASH};},
    async journal(){events.push('journal');if(used)throw Object.assign(Error('duplicate'),{code:'grant-already-attempted'});used=true;},
    save(r){events.push(r.firstFailure?'save-failure':'save-'+(r.appPid||'pre-pid'));receipts.push(structuredClone(r));},feedback(){events.push('ephemeral-feedback');},
    controller(){return controller;},validate(){events.push('validate');if(change.validate)throw Error('wrong owner');},
    async window(mode,pid,wid,action){events.push('window-'+mode);if(change.window)return change.window(mode,pid,wid,action,fire);
      return {pid,windowId:45,matchedAX:true,...mode==='capture'?{captureSha256:HASH}:{}};},
    async next(){return actions.shift();},setTimer(fn){events.push('watchdog');fire=fn;return 0;},clearTimer(){events.push('timer-clear');},
    async close(){events.push('helpers-close');if(change.close)throw Error('/private/helper-secret');}};
  return {cfg,services,events,receipts,get attempts(){return attempts;},get cleaned(){return cleaned;}};
}
module.exports={HEAD,HASH,config,fixture};
