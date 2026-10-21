/* Provisional Open only: exposure hints wake retained native work, never publish it. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectOpenExposure=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  // Inspect each returned receipt once; no later guard reads hostile accessors again.
  function viewMode(read){var mode=read?read():'canvas';if(!['canvas','stored-camera'].includes(mode))throw new Error('Native preview view is unavailable');return mode;}
  function matchesView(value,read){try{return !!value&&!!value.view&&value.view.mode===viewMode(read);}catch(_){return false;}}
  function projection(value,mode){
    if(!['canvas','stored-camera'].includes(mode))return null;
    if(value===undefined&&mode==='canvas')return {mode:'canvas',documentToComposition:[1,0,0,1,0,0],compositionToDocument:[1,0,0,1,0,0]};
    if(!value||Object.keys(value).sort().join(',')!=='cameraAvailable,compositionToDocument,documentToComposition,mode')return null;
    var actualMode=value.mode,available=value.cameraAvailable,forward=value.documentToComposition,inverse=value.compositionToDocument;
    if(actualMode!==mode||typeof available!=='boolean'||![forward,inverse].every(function(m){return Array.isArray(m)&&m.length===6;}))return null;
    forward=forward.slice();inverse=inverse.slice();
    if(![forward,inverse].every(function(m){return m.every(Number.isFinite);}))return null;
    var a=forward[0],b=forward[1],c=forward[2],d=forward[3],e=forward[4],f=forward[5],det=a*d-b*c;
    if(!Number.isFinite(det)||det===0)return null;
    var expected=[d/det,-b/det,-c/det,a/det,(c*f-d*e)/det,(b*e-a*f)/det];
    if(!inverse.every(function(n,i){return n===expected[i];})||(mode==='canvas'||!available)&&!forward.every(function(n,i){return n===[1,0,0,1,0,0][i];}))return null;
    return {mode:mode,cameraAvailable:available,documentToComposition:forward,compositionToDocument:inverse};
  }
  function inspectReceipt(receipt,ownerMatches,allowDeferred,viewMode){
    if(!ownerMatches)return {failure:'owner'};
    if(!receipt)return {failure:'receipt'};
    var status=receipt.status;
    if(status!=='presented'&&!(allowDeferred&&status==='deferred-occluded'))return {failure:
      ['stale-discarded','deferred-timeout','deferred-occluded','failed-validation','failed-device-lost',
        'failed-retry-lost','failed-retry-outdated','failed-recreate','failed-reconfigure'].includes(status)?'status-'+status:'status'};
    var workId=receipt.workId;
    if(typeof workId!=='string'||!/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(workId))return {failure:'work-id'};
    var viewGeneration=receipt.viewGeneration;
    if(!Number.isSafeInteger(viewGeneration)||viewGeneration<1)return {failure:'view-generation'};
    var view=projection(receipt.view,viewMode||'canvas');if(!view)return {failure:'view'};
    return {failure:null,status:status,workId:workId,viewGeneration:viewGeneration,view:view};
  }
  function create(subscribe,guard){
    var active=true,connected=false,epoch=0,visible=null,binding=null,detached=false,waiter=null,reason=null,rejectStop;
    var stopped=new Promise(function(_,reject){rejectStop=reject;});stopped.catch(function(){});
    function assertActive(){if(!active)throw reason;}
    function check(){assertActive();guard();}
    function cancel(error){
      if(!active)return;
      reason=error instanceof Error?error:new Error('Native project Open was cancelled');active=false;
      if(waiter){var pending=waiter;waiter=null;pending.reject(reason);}
      rejectStop(reason);
    }
    function hint(value){
      if(!active||detached||!value||typeof value.visible!=='boolean'||Object.keys(value).length!==1)return;
      try{check();}catch(error){cancel(error);return;}
      if(visible===value.visible)return;
      visible=value.visible;if(visible){epoch++;if(waiter){var pending=waiter;waiter=null;pending.resolve();}}
    }
    async function join(action){
      check();var work=Promise.resolve().then(function(){check();return action();});
      var result=await Promise.race([work,stopped]);check();return result;
    }
    async function connect(){
      check();if(typeof subscribe!=='function')return false;
      var registration=Promise.resolve().then(function(){check();return subscribe(hint,cancel);}).then(function(value){
        if(!value||Object.keys(value).length!==2||typeof value.detach!=='function'||typeof value.close!=='function')
          throw new Error('Native window exposure subscription is unavailable');
        if(!active){value.close();return;}
        binding=value;connected=true;
      });
      await Promise.race([registration,stopped]);check();return true;
    }
    function wait(seen){
      check();if(epoch!==seen)return Promise.resolve();
      return new Promise(function(resolve,reject){
        waiter={resolve:resolve,reject:reject};
        try{check();if(epoch!==seen){waiter=null;resolve();}}catch(error){waiter=null;reject(error);}
      });
    }
    async function run(invoke,verifyOwner,inspect){
      var retained=null;
      for(;;){
        check();var seen=epoch,receipt=inspect(await join(invoke),connected);
        if(receipt.failure)return receipt;
        if(retained&&(receipt.workId!==retained.workId||receipt.viewGeneration!==retained.viewGeneration||JSON.stringify(receipt.view)!==JSON.stringify(retained.view)))
          throw new Error('Native project deferred preview work changed');
        if(receipt.status!=='deferred-occluded'||!connected)return receipt;
        retained={workId:receipt.workId,viewGeneration:receipt.viewGeneration,view:receipt.view};
        await join(verifyOwner);
        // Only a new actual visible exposure can wake this wait. No timer retries.
        await wait(seen);check();await join(verifyOwner);
      }
    }
    function detach(){check();detached=true;if(binding)binding.detach();}
    function close(){
      cancel(new Error('Native project Open exposure scope ended'));
      var previous=binding;binding=null;if(previous)previous.close();
    }
    return Object.freeze({connect:connect,join:join,run:run,cancel:cancel,detach:detach,close:close,
      assertActive:assertActive,isStopped:function(){return !active;}});
  }
  return Object.freeze({create:create,inspectReceipt:inspectReceipt,viewMode:viewMode,matchesView:matchesView});
}));
