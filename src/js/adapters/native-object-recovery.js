/* Idle-timed crash-recovery writer scheduler (#2360 R2). Reads only the cached
   native session info; the bytes are written Rust-side by the recovery store and
   never cross JS. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectRecovery=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var CADENCE=Object.freeze({
    tickMs:5000,        // setTimeout chain period; a tick does no IPC unless it writes
    firstIdleMs:5000,   // first write of a session: dirty and quiescent for this long
    gapMs:30000,        // steady-state minimum gap between writes
    revisionCount:50,   // this many revisions since the last write ...
    revisionGapMs:10000,// ... shortens the gap to this
    backoff:20,         // never closer than backoff x the last write duration
    maxBytes:32*1024*1024});
  var COMMAND='nemo_native_object_recovery_write';
  var SLOT=/^[a-z0-9-]{1,64}$/,HEX64=/^[a-f0-9]{64}$/,ID=/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  // Pin of the session info() fields the scheduler consumes. A change that
  // drops or retypes one fails tests/native-object-recovery-contract.test.cjs.
  var INFO_FIELDS=Object.freeze(['instanceId','documentId','lifecycleGeneration','contentRevision',
    'documentSnapshotId','documentDigest','byteCount']);
  function id(v){return typeof v==='string'&&ID.test(v);}
  function integer(v,min){return Number.isSafeInteger(v)&&v>=min;}
  function readInfo(info){
    if(!info||typeof info!=='object'||!id(info.instanceId)||!id(info.documentId)||!id(info.documentSnapshotId)||
      !integer(info.lifecycleGeneration,1)||!integer(info.contentRevision,0)||
      typeof info.documentDigest!=='string'||!HEX64.test(info.documentDigest)||!integer(info.byteCount,1))return null;
    var out={};INFO_FIELDS.forEach(function(k){out[k]=info[k];});return out;
  }
  function slotKey(path,sessionTag,hash){
    // Mirrors project.js historyKey() for saved projects, clamped to the Rust regex ^[a-z0-9-]{1,64}$.
    if(typeof path!=='string'||!path)return 'untitled-'+sessionTag;
    var base=path.replace(/^.*[\\/]/,'').replace(/[^a-z0-9]+/gi,'-').toLowerCase();
    var key=(base+'-'+hash).replace(/[^a-z0-9-]/g,'-').slice(-64).replace(/^-+/,'');
    return key||'untitled-'+sessionTag;
  }
  function plainName(name){
    var text=typeof name==='string'?name.replace(/[\u0000-\u001f\u007f]/g,' ').trim():'';
    return (text||'Untitled').slice(0,128);
  }
  function create(ports){
    var timer=null,running=false,writing=false,s=null,noticed={},inflight=null,owned=new Map();
    // Slots this scheduler may have published, per document (#2360 R4). A slot is claimed when its write is requested, so a
    // write still in flight when a cleanup runs is covered; a failed or stale write leaves a name whose discard is a no-op.
    function own(documentId,slot){
      var set=owned.get(documentId);if(!set){set=new Set();owned.set(documentId,set);}set.add(slot);
    }
    function fresh(info){
      return {instanceId:info.instanceId,documentId:info.documentId,generation:info.lifecycleGeneration,
        tag:ports.randomHex(8),lastWriteAt:null,lastDuration:0,lastRevision:null,recovered:null,
        idleSince:null,skipDigest:null,supersedes:ports.supersedes?ports.supersedes()||null:null};
    }
    function sameSession(a,b){return !!a&&!!b&&a.instanceId===b.instanceId&&a.documentId===b.documentId&&a.generation===b.lifecycleGeneration;}
    // Why not to write now, or null. Reads cached state only.
    function check(){
      if(!running)return {why:'stopped'};
      var pin=ports.pin();
      if(!pin||!pin.active)return {why:'inactive'};
      var info=readInfo(pin.info);
      if(!info)return {why:'info-missing'};
      if(!s||!sameSession(s,info))s=fresh(info);
      // A resumed session (#2360 R3) names the recovery slot it replaces; it rides with the first published write.
      s.pinSupersedes=typeof pin.supersedes==='string'&&SLOT.test(pin.supersedes)&&!s.supersedeDone?pin.supersedes:null;
      if(writing)return {why:'in-flight',info:info};
      var reference=pin.reference||null;
      if(info.documentDigest===reference||info.documentDigest===s.recovered||info.documentDigest===s.skipDigest){
        s.idleSince=null;return {why:'clean',info:info};}
      if(pin.busy||!pin.presented||ports.blocked()){s.idleSince=null;return {why:'busy',info:info};}
      return {why:null,info:info};
    }
    function due(info,now,force){
      if(s.idleSince===null)s.idleSince=now;
      if(s.lastWriteAt===null)return force||now-s.idleSince>=CADENCE.firstIdleMs;
      var floor=CADENCE.backoff*s.lastDuration,gap=now-s.lastWriteAt;
      if(gap<floor)return false;
      if(force)return true;
      if(gap>=CADENCE.gapMs)return true;
      return info.contentRevision-s.lastRevision>=CADENCE.revisionCount&&gap>=CADENCE.revisionGapMs;
    }
    function request(info){
      var slot=ports.slot()||{},path=typeof slot.path==='string'&&slot.path.charAt(0)==='/'&&slot.path.length<=1024&&slot.path.indexOf('\0')<0?slot.path:null;
      var key=slotKey(slot.path,s.tag,ports.hash(slot.path||''));
      var r={apiVersion:2,requestId:ports.requestId('recovery'),instanceId:info.instanceId,documentId:info.documentId,
        atRevision:info.contentRevision,documentSnapshotId:info.documentSnapshotId,slotKey:key,
        displayName:plainName(slot.name),documentDigest:info.documentDigest};
      if(path)r.originPath=path;
      var replaced=s.supersedes||s.pinSupersedes;
      if(replaced&&replaced!==key)r.supersedes=replaced;
      own(info.documentId,key);
      return r;
    }
    function notifyOnce(flag,message){
      if(noticed[flag])return;noticed[flag]=true;
      try{ports.notice(message);}catch(_){}
    }
    async function attempt(info,now){
      var mine=s,req=request(info),began=now,done=null;
      writing=true;
      inflight=new Promise(function(resolve){done=resolve;});
      try{
        var receipt=await ports.write(COMMAND,req);
        if(s!==mine)return {wrote:false,why:'session-changed'};
        var ended=ports.now();
        if(!receipt||receipt.requestId!==req.requestId||receipt.documentId!==req.documentId||typeof receipt.published!=='boolean')
          throw new Error('Native recovery receipt is invalid');
        if(!receipt.published){
          // stale: the revision moved under the write; wait for the next tick.
          if(receipt.reason==='too-large'){mine.skipDigest=info.documentDigest;
            notifyOnce('too-large','Crash recovery skipped: this project is larger than the recovery limit.');}
          else if(receipt.reason==='unsupported')mine.skipDigest=info.documentDigest;
          return {wrote:false,why:receipt.reason||'unpublished'};
        }
        mine.lastWriteAt=ended;mine.lastDuration=Math.max(0,ended-began);mine.lastRevision=info.contentRevision;
        mine.recovered=info.documentDigest;mine.idleSince=null;mine.supersedes=null;mine.supersedeDone=true;mine.pinSupersedes=null;
        return {wrote:true,receipt:receipt};
      }catch(error){
        // Failure (ENOSPC and similar): the previous generation is untouched; retry next tick.
        if(s===mine)notifyOnce('failed','Crash recovery could not be saved: '+(error&&error.message||error));
        return {wrote:false,why:'failed'};
      }finally{writing=false;inflight=null;done();}
    }
    // Cleanup surface (#2360 R4). claim() is synchronous: the slots of one document (or of all) at this instant.
    function claim(documentId){
      var out=[];
      owned.forEach(function(set,id){if(documentId===undefined||id===documentId)set.forEach(function(k){if(out.indexOf(k)<0)out.push(k);});});
      return out;
    }
    function release(documentId,slots){
      var set=owned.get(documentId);if(!set)return;
      slots.forEach(function(k){set.delete(k);});if(!set.size)owned.delete(documentId);
    }
    async function tick(options){
      var now=ports.now(),c=check();
      if(c.why)return {wrote:false,why:c.why};
      if(!due(c.info,now,!!(options&&options.force)))return {wrote:false,why:'not-due'};
      return attempt(c.info,now);
    }
    function schedule(){
      if(!running)return;
      timer=ports.setTimer(function(){timer=null;
        // Re-arm first: neither a throwing tick nor a long write may end or stall the chain.
        schedule();Promise.resolve().then(function(){return tick();}).catch(function(){});
      },CADENCE.tickMs);
    }
    return Object.freeze({
      start:function(){if(running)return;running=true;schedule();},
      stop:function(){running=false;if(timer!==null){ports.clearTimer(timer);timer=null;}},
      tick:tick,
      claim:claim,release:release,
      // Resolves once any write in flight has settled (it never rejects), so a discard cannot be overtaken by it.
      quiesce:function(){return inflight||Promise.resolve();},
      // Hidden window / blur: write the dirty state once now, still honoring the 20x backoff floor.
      flush:function(){return tick({force:true});},
      state:function(){return s?{lastWriteAt:s.lastWriteAt,lastDuration:s.lastDuration,lastRevision:s.lastRevision,
        recovered:s.recovered,idleSince:s.idleSince,writing:writing}:null;}
    });
  }
  function simpleHash(s){var h=0;for(var i=0;i<s.length;i++)h=(h*31+s.charCodeAt(i))|0;return (h>>>0).toString(36);}
  function randomHex(root,n){
    var out='';
    try{var bytes=root.crypto.getRandomValues(new Uint8Array(Math.ceil(n/2)));
      bytes.forEach(function(b){out+=('0'+b.toString(16)).slice(-2);});}catch(_){}
    while(out.length<n)out+=Math.floor(Math.random()*16).toString(16);
    return out.slice(0,n);
  }
  // Binds the scheduler to the desktop session: cached pin from the file UI, the private Tauri
  // command for the write, and the page signals (playing, pointer held, hidden) that gate it.
  function bind(root,ports){
    var Project=root.NemoNativeObjectProject,session=Project&&Project.desktop&&Project.desktop(root);
    if(!session||!root.__TAURI__||!root.__TAURI__.core||typeof ports.pin!=='function')return null;
    var held=false,scheduler=create({
      now:function(){return Date.now();},
      setTimer:function(fn,ms){return root.setTimeout(fn,ms);},clearTimer:function(id){root.clearTimeout(id);},
      pin:ports.pin,slot:ports.slot,notice:ports.notice,supersedes:ports.supersedes,
      blocked:function(){return held||!!(root.state&&root.state.playing);},
      requestId:function(kind){return session.requestId(kind);},hash:simpleHash,randomHex:function(n){return randomHex(root,n);},
      write:function(command,request){return root.__TAURI__.core.invoke(command,{requestJson:JSON.stringify(request)});}
    });
    var doc=root.document;
    if(doc&&doc.addEventListener){
      doc.addEventListener('pointerdown',function(){held=true;},true);
      ['pointerup','pointercancel'].forEach(function(name){doc.addEventListener(name,function(){held=false;},true);});
      doc.addEventListener('visibilitychange',function(){if(doc.visibilityState==='hidden')scheduler.flush().catch(function(){});});
    }
    if(root.addEventListener)root.addEventListener('blur',function(){held=false;scheduler.flush().catch(function(){});});
    return scheduler;
  }
  return Object.freeze({create:create,bind:bind,readInfo:readInfo,slotKey:slotKey,CADENCE:CADENCE,INFO_FIELDS:INFO_FIELDS,COMMAND:COMMAND});
}));
