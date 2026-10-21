/* T12 Time Remap and footage mode entries under the native owner: they read the
   layer's video placement and edit it through the existing placement command. */
(function(root,factory){
  var placement=typeof module==='object'&&module.exports?require('./native-project-placement-contract.js'):root.NemoNativeProjectPlacementContract;
  var api=factory(placement);
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeTimeRemapUI=api;
}(typeof globalThis==='object'?globalThis:this,function(Placement){
  'use strict';
  var bindings=new WeakMap();
  var pinKeys=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  var entries=['toggleTimeRemap','isTimeRemapped','footageInterpMode','setFootageInterpMode'];
  function same(a,b){return !!a&&!!b&&pinKeys.every(function(k){return a[k]===b[k];});}
  function usable(ports,session){
    return !!session&&ports.currentSession()===session&&session.isActive()===true&&session.blocksLegacy()===true&&
      typeof session.runResourceCommand==='function'&&typeof session.readLayerPlacement==='function'&&
      typeof session.readLayerRange==='function'&&typeof session.requestId==='function';
  }
  function presented(session,info){
    var value=session.readPresentedObjectSession();
    return !!value&&value.available===true&&value.kind==='native-object'&&same(info,value);
  }
  function bind(root,ports){
    if(bindings.has(root))return bindings.get(root);
    var target=root.SM;
    if(!target||!Placement||!entries.every(function(name){return typeof target[name]==='function';}))return null;
    var original={},retired=false,pending=null,facts=new Map();
    entries.forEach(function(name){original[name]=target[name];});
    // The layer's placement at the playhead, pinned to the presented snapshot.
    function context(li){
      var session=ports.currentSession();
      if(retired||root.SM!==target||!usable(ports,session))return null;
      var info=session.info(),s=root.state,frame=s&&s.currentFrame;
      if(!info||ports.projectionReady(info)!==true||!presented(session,info)||!Number.isInteger(li)||
        !Number.isInteger(frame)||frame<0||frame>=info.totalFrames)return null;
      var row=s.layers&&s.layers[li],layer=Array.isArray(info.layers)&&info.layers[li];
      if(!row||!layer||typeof layer.layerUid!=='string'||row.layerUid!==layer.layerUid)return null;
      var pin={};pinKeys.forEach(function(k){pin[k]=info[k];});
      return {session:session,info:info,pin:pin,frame:frame,uid:layer.layerUid,
        key:info.documentSnapshotId+'\u0000'+layer.layerUid+'\u0000'+frame};
    }
    function admitted(result,ctx){
      var p=result&&result.placement;
      return !!result&&result.kind==='layerPlacement'&&result.documentSnapshotId===ctx.pin.documentSnapshotId&&
        result.layerUid===ctx.uid&&result.frame===ctx.frame&&
        (p===null||!!p&&typeof p.placementId==='string'&&p.assetKind==='video');
    }
    // Resolves the placement (null without one) or undefined when the read refused.
    function read(ctx){
      return Promise.resolve(ctx.session.readLayerPlacement(ctx.uid,ctx.frame,ctx.info)).then(function(result){
        if(!admitted(result,ctx))return undefined;
        if(facts.size>=64)facts.clear();
        facts.set(ctx.key,result.placement);
        return result.placement;
      },function(){return undefined;});
    }
    // A synchronous answer from the last read of this snapshot; a miss asks once
    // and redraws the panels when the answer differs from the default.
    function known(li,answer,fallback){
      var ctx=context(li);
      if(!ctx)return fallback;
      if(facts.has(ctx.key))return answer(facts.get(ctx.key));
      facts.set(ctx.key,null);
      read(ctx).then(function(p){
        if(p===undefined)facts.delete(ctx.key);
        else if(answer(p)!==fallback&&context(li)&&typeof root.updateUI==='function')root.updateUI();
      });
      return fallback;
    }
    function remapped(p){return !!p&&p.timeRemap!==null;}
    function mode(p){return p&&p.interpMode||(p&&p.frameBlend?'blend':'none');}
    function toast(key,suffix){
      if(typeof root.showToast==='function'&&root.SM&&typeof root.SM.t==='function')root.showToast(root.SM.t(key)+(suffix||''));
    }
    function edit(ctx,payload){
      Placement.payload(Placement.OP,payload);
      return ctx.session.runResourceCommand(async function(action){
        if(!same(ctx.session.info(),ctx.pin)||!presented(action,ctx.pin))return false;
        var request={apiVersion:2,requestId:ctx.session.requestId('time-remap'),instanceId:ctx.pin.instanceId,
          documentId:ctx.pin.documentId,operation:Placement.OP,expectedRevision:ctx.pin.contentRevision,payload:payload};
        // An uncertain receipt refreshes the existing owner; it is never replayed.
        var receipt;
        try{
          receipt=await action.dispatch(request);
          if(!receipt||receipt.apiVersion!==2||typeof receipt.ok!=='boolean')throw new Error('Native placement response is uncorrelated');
          Placement.correlate(request,receipt);
        }catch(_){
          await action.reconcile();
          return false;
        }
        if(receipt.ok!==true||receipt.result.applied!==true)return false;
        await action.reconcile(receipt.contentRevision);
        return true;
      }).then(function(applied){facts.clear();return applied===true;});
    }
    function serialized(task){
      if(pending)return Promise.resolve(false);
      var run=Promise.resolve().then(task).catch(function(){return false;});
      pending=run;
      return run.finally(function(){if(pending===run)pending=null;});
    }
    function toggleTimeRemap(li){
      return serialized(async function(){
        var ctx=context(li);if(!ctx)return false;
        facts.delete(ctx.key);
        var p=await read(ctx);
        if(p===undefined||!context(li))return false;
        if(p===null){toast('toastTimeRemapAppliesToComponents');return false;}
        if(remapped(p)){
          if(!await edit(ctx,{kind:'disableTimeRemap',placementId:p.placementId}))return false;
          toast('toastTimeRemapDisabled');return true;
        }
        // The decoder's own count, bound by the desktop host; unknown stays unavailable.
        var count=p.sourceFrameCount;
        if(!Number.isSafeInteger(count)||count<1)return false;
        var range=await ctx.session.readLayerRange(ctx.uid,ctx.frame,ctx.info),r=range&&range.range;
        if(!r||range.documentSnapshotId!==ctx.pin.documentSnapshotId||range.layerUid!==ctx.uid||
          !Number.isFinite(r.inPoint)||!Number.isFinite(r.outPoint))return false;
        var last=count-1;
        if(!await edit(ctx,{kind:'enableTimeRemap',placementId:p.placementId,inFrame:r.inPoint,outFrame:r.outPoint,lastSourceFrame:last}))return false;
        toast('toastTimeRemapEnabledFromSuffix',last);return true;
      });
    }
    function setFootageInterpMode(li,value){
      return serialized(async function(){
        var ctx=context(li);if(!ctx)return false;
        facts.delete(ctx.key);
        var p=await read(ctx);
        if(!p||!context(li))return false;
        if(p.interpMode===value)return true;
        return edit(ctx,{kind:'setInterpMode',placementId:p.placementId,interpMode:value});
      });
    }
    var own={
      toggleTimeRemap:toggleTimeRemap,
      isTimeRemapped:function(li){return known(li,remapped,false);},
      footageInterpMode:function(li){return known(li,mode,'none');},
      setFootageInterpMode:setFootageInterpMode
    };
    entries.forEach(function(name){target[name]=own[name];});
    var binding=Object.freeze(Object.assign({},own,{dispose:function(){
      retired=true;pending=null;facts.clear();
      // An active or indeterminate native owner must keep the refusing entries.
      var session=ports.currentSession();
      if(session&&session.blocksLegacy()===true)return false;
      if(root.SM===target)entries.forEach(function(name){if(target[name]===own[name])target[name]=original[name];});
      bindings.delete(root);return true;
    }}));
    bindings.set(root,binding);return binding;
  }
  function createPorts(root,base){
    var entry=null;
    return Object.freeze({bindTimeRemapEntries:function(){
      if(entry)return true;entry=bind(root,base);return !!entry;
    },disposeTimeRemapEntries:function(){if(entry&&entry.dispose()===true)entry=null;}});
  }
  function compose(root,base,policy){
    var own=createPorts(root,base);
    return Object.freeze(Object.assign({},policy,{
      bindPolicyEntries:function(){var ready=policy.bindPolicyEntries();return own.bindTimeRemapEntries()&&ready;},
      disposePolicyEntries:function(){own.disposeTimeRemapEntries();policy.disposePolicyEntries();}
    }));
  }
  return Object.freeze({bind:bind,createPorts:createPorts,compose:compose});
}));
