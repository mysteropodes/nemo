/* Provisional Open only: exposure hints wake retained native work, never publish it. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectOpenExposure=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function valid(receipt){return receipt&&typeof receipt.workId==='string'&&
    /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(receipt.workId)&&Number.isSafeInteger(receipt.viewGeneration)&&receipt.viewGeneration>0;}
  function receiptFailure(receipt,ownerMatches){
    if(!ownerMatches)return 'owner';
    if(!receipt)return 'receipt';
    var status=receipt.status;
    if(status!=='presented')return ['stale-discarded','deferred-timeout','deferred-occluded','failed-validation',
      'failed-device-lost','failed-retry-lost','failed-retry-outdated','failed-recreate','failed-reconfigure'].includes(status)?'status-'+status:'status';
    if(!receipt.workId||typeof receipt.workId!=='string'||!/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(receipt.workId))return 'work-id';
    if(!Number.isSafeInteger(receipt.viewGeneration)||receipt.viewGeneration<1)return 'view-generation';
    return null;
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
    async function run(invoke,verifyOwner){
      var retained=null;
      for(;;){
        check();var seen=epoch,receipt=await join(invoke);
        if(retained&&(!valid(receipt)||receipt.workId!==retained.workId||receipt.viewGeneration!==retained.viewGeneration))
          throw new Error('Native project deferred preview work changed');
        if(!receipt||receipt.status!=='deferred-occluded'||!connected)return receipt;
        if(!valid(receipt))throw new Error('Native project deferred preview identity is invalid');
        retained={workId:receipt.workId,viewGeneration:receipt.viewGeneration};
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
  return Object.freeze({create:create,receiptFailure:receiptFailure});
}));
