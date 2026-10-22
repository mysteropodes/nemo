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
  // Placeholders the legacy owner need not define: rows read their own track there.
  var optional=['timeRemapKeyFacts','timeRemapKeyIntent','nativeVideoPlacement'];
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
    var original={},retired=false,pending=null;
    // Placement facts by snapshot and layer (frame independent), the remap facts
    // by frame, and the last answer for a layer of this document: a row stays
    // while a new snapshot or frame is read, and only remapped layers read per frame.
    var layers=new Map(),frames=new Map(),last=new Map(),inflight=new Set();
    entries.concat(optional).forEach(function(name){original[name]=target[name];});
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
      var at=info.documentSnapshotId+'\u0000'+layer.layerUid;
      return {session:session,info:info,pin:pin,frame:frame,uid:layer.layerUid,layerKey:at,key:at+'\u0000'+frame,
        docKey:info.instanceId+'\u0000'+info.documentId+'\u0000'+layer.layerUid};
    }
    function admitted(result,ctx){
      var p=result&&result.placement;
      return !!result&&result.kind==='layerPlacement'&&result.documentSnapshotId===ctx.pin.documentSnapshotId&&
        result.layerUid===ctx.uid&&result.frame===ctx.frame&&
        (p===null||!!p&&typeof p.placementId==='string'&&p.assetKind==='video');
    }
    function remember(map,key,value){if(map.size>=256)map.clear();map.set(key,value);}
    // Resolves the placement (null without one) or undefined when the read refused.
    function read(ctx){
      return Promise.resolve(ctx.session.readLayerPlacement(ctx.uid,ctx.frame,ctx.info)).then(function(result){
        if(!admitted(result,ctx))return undefined;
        var p=result.placement;
        remember(layers,ctx.layerKey,p);remember(last,ctx.docKey,p);
        if(remapped(p))remember(frames,ctx.key,p.timeRemap);
        return p;
      },function(){return undefined;});
    }
    function signature(p){return JSON.stringify(p&&[p.placementId,p.interpMode,p.frameBlend,p.sourceFrameCount,remapped(p)&&[p.timeRemap.keyCount,p.timeRemap.keys]]);}
    // One read per miss; the redraw is frame-only unless the row set or its keys changed.
    function ask(ctx,li,before){
      if(inflight.has(ctx.key))return;
      inflight.add(ctx.key);
      var shown=JSON.stringify(remapFacts(before,ctx));
      read(ctx).then(function(p){
        inflight.delete(ctx.key);
        if(p===undefined||!context(li)||typeof root.updateUI!=='function')return;
        if(signature(p)!==signature(before))root.updateUI();
        else if(JSON.stringify(remapFacts(p,ctx))!==shown)root.updateUI(true);
      });
    }
    // The layer's placement now, or the last one read for it while this snapshot is read.
    function placementAt(ctx,li){
      if(layers.has(ctx.layerKey))return layers.get(ctx.layerKey);
      var before=last.get(ctx.docKey);
      ask(ctx,li,before===undefined?null:before);
      return before===undefined?null:before;
    }
    function known(li,answer,fallback){
      var ctx=context(li);
      return ctx?answer(placementAt(ctx,li)):fallback;
    }
    function remapped(p){return !!p&&p.timeRemap!==null;}
    // The Time Remap facts at the playhead: the frame's own read, else the layer's
    // keys with the value unknown (null) until the frame is read.
    function remapFacts(p,ctx){
      if(!remapped(p))return null;
      if(frames.has(ctx.key))return frames.get(ctx.key);
      var r=p.timeRemap,keys=Array.isArray(r.keys)?r.keys:null;
      return {keyCount:r.keyCount,keyAtFrame:keys?keys.some(function(k){return k.frame===ctx.frame;}):false,value:null,keys:keys};
    }
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
      }).then(function(applied){
        // A refused, stale or unconfirmed edit is reported; the projection stays the owner's.
        if(applied!==true)toast('toastNativeTimeRemapNotApplied');
        return applied===true;
      });
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
        var p=await read(ctx);
        if(!p||!context(li))return false;
        if(p.interpMode===value)return true;
        return edit(ctx,{kind:'setInterpMode',placementId:p.placementId,interpMode:value});
      });
    }
    // The Time Remap row's stopwatch (value null) or value field at the playhead,
    // as the baseline row: a key here is removed, the last one turns remapping off.
    function timeRemapKeyIntent(li,value){
      return serialized(async function(){
        var ctx=context(li);if(!ctx)return false;
        var p=await read(ctx);
        if(!p||!remapped(p)||!context(li))return false;
        var r=p.timeRemap,id=p.placementId;
        if(value!==null){
          if(typeof value!=='number'||!Number.isFinite(value))return false;
          return edit(ctx,{kind:'setTimeRemapKey',placementId:id,frame:ctx.frame,value:value});
        }
        // No key here: key the eased source frame the native read evaluated at the playhead.
        if(!r.keyAtFrame){
          if(r.value===null){toast('toastNativeTimeRemapValueUnavailable');return false;}
          return edit(ctx,{kind:'setTimeRemapKey',placementId:id,frame:ctx.frame,value:r.value});
        }
        if(r.keyCount>1)return edit(ctx,{kind:'removeTimeRemapKey',placementId:id,frame:ctx.frame});
        if(!await edit(ctx,{kind:'disableTimeRemap',placementId:id}))return false;
        toast('toastTimeRemapDisabled');return true;
      });
    }
    var own={
      timeRemapKeyFacts:function(li){
        var ctx=context(li);if(!ctx)return null;
        var p=placementAt(ctx,li),r=remapFacts(p,ctx);
        // Only a remapped layer reads per frame; other layers answer from their snapshot read.
        if(r&&!frames.has(ctx.key))ask(ctx,li,p);
        return r;
      },
      timeRemapKeyIntent:timeRemapKeyIntent,
      // The footage panel's source facts: the layer's video placement at the playhead, or null.
      nativeVideoPlacement:function(li){return known(li,function(p){return p||null;},null);},
      toggleTimeRemap:toggleTimeRemap,
      isTimeRemapped:function(li){return known(li,remapped,false);},
      footageInterpMode:function(li){return known(li,mode,'none');},
      setFootageInterpMode:setFootageInterpMode
    };
    entries.concat(optional).forEach(function(name){target[name]=own[name];});
    var binding=Object.freeze(Object.assign({},own,{dispose:function(){
      retired=true;pending=null;[layers,frames,last,inflight].forEach(function(m){m.clear();});
      // An active or indeterminate native owner must keep the refusing entries.
      var session=ports.currentSession();
      if(session&&session.blocksLegacy()===true)return false;
      if(root.SM===target)entries.concat(optional).forEach(function(name){
        if(target[name]!==own[name])return;
        if(original[name]===undefined)delete target[name];else target[name]=original[name];
      });
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
  // The Time Remap grid row from the read keys, inert display data like native opacity
  // (keys null past the read bound: no diamonds). g: fc, rowH, totalFrames, currentFrame, label.
  function trackRow(doc,rowEl,facts,g){
    rowEl.style.pointerEvents='none';
    var keys=facts.keys||[],svg=doc.createElementNS('http://www.w3.org/2000/svg','svg');
    svg.setAttribute('width',g.totalFrames*g.fc);svg.setAttribute('height',g.rowH);
    svg.style.cssText='position:absolute;left:0;top:0;pointer-events:none;';
    for(var i=0;i+1<keys.length;i++){
      if(keys[i].value===keys[i+1].value)continue;
      var conn=doc.createElementNS('http://www.w3.org/2000/svg','rect');
      conn.setAttribute('class','native-remap-connector');
      conn.setAttribute('x',keys[i].frame*g.fc+g.fc/2);conn.setAttribute('y',Math.round((g.rowH-5)/2));
      conn.setAttribute('width',(keys[i+1].frame-keys[i].frame)*g.fc);conn.setAttribute('height',5);conn.setAttribute('rx',2);
      conn.style.cssText='fill:#2E6B60;pointer-events:none;';svg.appendChild(conn);
    }
    rowEl.appendChild(svg);
    for(var frame=0;frame<g.totalFrames;frame++){
      var cell=doc.createElement('div'),here=frame===g.currentFrame;
      cell.className='fc motion-fc'+(here?' cur':'');cell.dataset.frame=frame;
      if(keys.some(function(key){return key.frame===frame;})){
        var diamond=doc.createElement('div');diamond.className='native-remap-diamond';
        diamond.style.cssText='position:relative;z-index:2;width:7px;height:7px;border-radius:2px;background:'+(here?'#ffaa28':'transparent')+
          ';border:1.5px solid '+(here?'#ffaa28':'#cfcfcf')+';transform:rotate(45deg);pointer-events:none;';
        diamond.title=g.label+' · '+(frame+1);cell.appendChild(diamond);
      }
      rowEl.appendChild(cell);
    }
  }
  return Object.freeze({bind:bind,createPorts:createPorts,compose:compose,trackRow:trackRow});
}));
