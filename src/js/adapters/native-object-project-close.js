/* Closing the last project (#2381). Two pieces, no document authority of their own:
 *  run(hooks)   the native session's Close: release the active project through the existing release route, no replacement.
 *  create(...)  the tab-bar/Cmd+W orchestration: unsaved-changes prompt first, then the project's close, then the home screen. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProjectClose=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function refusal(message){return {code:'replacement_refused',message:message,details:{disposition:'notDispatched',retryExecution:false}};}
  function sameProject(a,b){return !!a&&!!b&&a.instanceId===b.instanceId&&a.documentId===b.documentId&&a.lifecycleGeneration===b.lifecycleGeneration;}
  // hooks are closures of the session (native-object-project.js); this module never sees its private state.
  function run(h){
    if(h.opening()||!h.isActive())return Promise.reject(refusal('Native project Close is unavailable'));
    var saved=h.lock(),previous=saved.info,dispatched=false;
    return h.enqueue(async function(){
      try{
        await h.retire();
        var before=await h.status();
        if(!before||before.apiVersion!==2||before.available!==true||!sameProject(before,previous)||
            !Number.isSafeInteger(before.contentRevision)||before.contentRevision<previous.contentRevision)
          throw refusal('Native project slot changed before Close');
        var request={apiVersion:2,requestId:h.requestId('close'),instanceId:previous.instanceId,documentId:previous.documentId,
          expectedRevision:before.contentRevision};
        dispatched=true;
        var receipt=await h.invoke('nemo_native_release',{request:request});
        if(!receipt||receipt.status!=='succeeded'||receipt.reentryAvailable!==true||receipt.requestId!==request.requestId||
            receipt.instanceId!==request.instanceId||receipt.documentId!==request.documentId)
          throw new Error('Native project Close was not confirmed');
        var after=await h.status();
        if(!after||after.apiVersion!==2||after.available!==false)throw new Error('Native project slot was not released');
        return true;
      }catch(error){
        if(dispatched)throw h.fence(error);
        try{await h.verify(previous);h.restore(saved);await h.attach();}catch(unknown){throw h.fence(unknown);}
        throw error&&error.details?error:refusal(error&&error.message||'Native project Close was not dispatched');
      }
    }).then(function(value){h.done();return value;}).finally(h.unlock);
  }
  // ports: native() isDirty() confirmDiscard() closeNative() closeLegacy() current() finish() notice(message)
  function create(ports){
    var busy=false;
    async function close(){
      var native=ports.native(),pin=ports.current();
      if(ports.isDirty()&&await ports.confirmDiscard()!==true)return false;
      // The prompt is asynchronous: close only the very project the user asked to close.
      if(ports.native()!==native||ports.current()!==pin)return false;
      if(await (native?ports.closeNative():ports.closeLegacy())!==true)return false;
      ports.finish();
      return true;
    }
    return function closeLast(){
      if(busy)return Promise.resolve(false);
      busy=true;
      return close().catch(function(error){
        try{ports.notice('Could not close the project: '+(error&&error.message||error));}catch(_){}
        return false;
      }).finally(function(){busy=false;});
    };
  }
  return Object.freeze({run:run,create:create});
}));
