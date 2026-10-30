/* Crash-recovery cleanup triggers for native projects (#2360 R4). A recovery exists to survive a crash, so it is
   dropped once the work it protects is saved or knowingly replaced, and at a clean quit. A confirmed
   quit-without-saving keeps it (baseline parity, the approved default): that path never calls in here.
   Every call is best effort: a failure here never fails Save, Open, New or Quit. The Rust list also drops a slot whose
   origin file is newer than the recovery, so the start screen sweeps what a failed discard left behind. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectRecoveryCleanup=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var DISCARD='nemo_native_object_recovery_discard';
  var LIMITS=Object.freeze({discardMs:3000,quitMs:1500});
  var SLOT=/^[a-z0-9-]{1,64}$/;
  // ports: scheduler{claim,release,quiesce,stop}, instanceId(), invoke(command,args), requestId(kind),
  //        setTimer(fn,ms), clearTimer(id)
  function create(ports){
    function bounded(promise,ms){
      return new Promise(function(resolve){
        var timer=ports.setTimer(function(){resolve({timedOut:true});},ms);
        function clear(){try{ports.clearTimer(timer);}catch(_){}}
        promise.then(function(v){clear();resolve(v);},function(){clear();resolve({failed:true});});
      });
    }
    async function discardSlots(owner,slots,instanceId){
      var out={discarded:[],failed:[]};
      for(var i=0;i<slots.length;i++){
        var slot=slots[i];
        if(typeof slot!=='string'||!SLOT.test(slot))continue;
        try{
          var request={apiVersion:2,requestId:ports.requestId('recovery-discard'),instanceId:instanceId,slotKey:slot};
          var receipt=await ports.invoke(DISCARD,{requestJson:JSON.stringify(request)});
          if(!receipt||receipt.requestId!==request.requestId||receipt.slotKey!==slot||typeof receipt.discarded!=='boolean')throw new Error('invalid receipt');
          out.discarded.push(slot);
          if(owner!==null)ports.scheduler.release(owner,[slot]);
        }catch(_){out.failed.push(slot);}
      }
      return out;
    }
    // The document's recovery slots (plus any caller-known extra, e.g. the slot a resumed session came from). The claim is
    // synchronous with the call, so a write that starts later is never mistaken for one this cleanup covers.
    function discardDocument(documentId,extra){
      var claimed=ports.scheduler.claim(documentId).concat(Array.isArray(extra)?extra:[]);
      var instanceId=ports.instanceId();
      if(!claimed.length||typeof instanceId!=='string'||!instanceId)return Promise.resolve({discarded:[],failed:[]});
      var work=(async function(){
        await ports.scheduler.quiesce();
        return discardSlots(documentId,claimed.filter(function(k,i){return claimed.indexOf(k)===i;}),instanceId);
      })();
      return bounded(work,LIMITS.discardMs).then(function(r){return r.discarded?r:{discarded:[],failed:claimed};});
    }
    // Clean quit: stop writing, let an in-flight write land, drop the slots of the quitting project. Bounded.
    function cleanQuit(){
      try{ports.scheduler.stop();}catch(_){}
      var claimed=ports.scheduler.claim(),instanceId=ports.instanceId();
      if(!claimed.length||typeof instanceId!=='string'||!instanceId)return Promise.resolve({discarded:[],failed:[]});
      var work=(async function(){
        await ports.scheduler.quiesce();
        return discardSlots(null,claimed,instanceId);
      })();
      return bounded(work,LIMITS.quitMs).then(function(r){return r.discarded?r:{discarded:[],failed:claimed};});
    }
    return Object.freeze({discardDocument:discardDocument,cleanQuit:cleanQuit});
  }
  function bind(root,ports){
    var Project=root.NemoNativeObjectProject,session=Project&&Project.desktop&&Project.desktop(root);
    if(!session||!root.__TAURI__||!root.__TAURI__.core||!ports.scheduler)return null;
    return create({
      scheduler:ports.scheduler,
      instanceId:function(){var info=session.info();return info&&info.instanceId||null;},
      invoke:function(command,args){return root.__TAURI__.core.invoke(command,args);},
      requestId:function(kind){return session.requestId(kind);},
      setTimer:function(fn,ms){return root.setTimeout(fn,ms);},clearTimer:function(id){root.clearTimeout(id);}
    });
  }
  return Object.freeze({create:create,bind:bind,DISCARD:DISCARD,LIMITS:LIMITS});
}));
