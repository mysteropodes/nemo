/* T12 Time Remap and footage mode entries under the native owner: they read the
   layer's video placement and edit it through the existing placement command.
   mountInspector is their native session surface (the legacy Footage panel and
   Motion row are unreachable there); the range grid draws the read keys inert. */
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
  // Native-only reads/intents the legacy owner does not define.
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
        // The decoder's own count, bound by the desktop host; unknown stays unavailable and is reported.
        var count=p.sourceFrameCount;
        if(!Number.isSafeInteger(count)||count<1){toast('toastNativeTimeRemapNotApplied');return false;}
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
        if(mode(p)===value)return true;
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
      // The Footage inspector's source facts: the layer's video placement at the playhead, or null.
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
  // The Footage section of the native inspector for the ACTIVE layer's video placement.
  // Every value is the binding's native read; edits go through its intents only, and
  // refresh() updates the mounted nodes in place (frame-only paints rebuild nothing).
  var inspectors=new WeakMap();
  function mountInspector(root,container,ports){
    var old=inspectors.get(root);
    if(old&&old.container===container&&!old.retired){old.ports=ports;old.refresh();return old;}
    if(old)old.retire();
    var doc=root.document,b={container:container,ports:ports,retired:false};
    function tr(key,fallback){var t=root.SM&&root.SM.t,v=typeof t==='function'?t(key):null;return typeof v==='string'&&v?v:fallback;}
    function el(tag,text,cls){var n=doc.createElement(tag);if(text!==undefined)n.textContent=text;if(cls)n.className=cls;return n;}
    function line(label,control){var row=el('div',undefined,'pr');row.appendChild(el('span',label,'pl'));row.appendChild(control);return row;}
    var section=el('section',undefined,'psec'),source=el('div','—','psel'),count=el('div','—','psel'),mode=el('select',undefined,'psel');
    var replace=el('button',tr('footageReplace','Replace source…'),'pbtn'),toggle=el('button','','pbtn'),key=el('button','','pbtn'),value=el('input',undefined,'pi scrub');
    var remapBox=el('div'),interpRow;
    section.id='native-footage-inspector';section.hidden=true;
    [replace,toggle,key].forEach(function(n){n.type='button';});
    [['none','footageInterpNone','None (nearest frame)'],['blend','footageInterpBlend','Cross dissolve'],['motion','footageInterpMotion','Motion compensated']].forEach(function(o){
      var opt=el('option',tr(o[1],o[2]));opt.value=o[0];mode.appendChild(opt);});
    value.type='number';value.id='native-time-remap-value';value.dataset.step='1';value.dataset.nativeScrub='commit';
    value.setAttribute('aria-label',tr('propTimeRemap','Time Remap'));
    section.appendChild(el('div',tr('hdrFootage','Footage'),'phdr'));
    section.appendChild(line(tr('fieldSource','Source'),source));section.appendChild(line(tr('fieldFrames','Frames'),count));
    remapBox.appendChild(el('div',tr('propTimeRemap','Time Remap'),'pl'));remapBox.appendChild(toggle);
    remapBox.appendChild(interpRow=line(tr('footageInterp','Interpolation'),mode));remapBox.appendChild(key);remapBox.appendChild(value);
    section.appendChild(remapBox);section.appendChild(replace);container.appendChild(section);
    function binding(){return b.retired||inspectors.get(root)!==b?null:bindings.get(root)||null;}
    function active(){var s=root.state;return s&&Number.isInteger(s.activeLayerIdx)?s.activeLayerIdx:-1;}
    function video(){var c=root.NemoNativeImportController;return c&&c.video?c:null;}
    var shown=null;
    function after(run){return Promise.resolve(run).catch(function(){return false;}).then(function(r){b.refresh();return r;});}
    toggle.addEventListener('click',function(){var own=binding();if(own)return after(own.toggleTimeRemap(active()));});
    key.addEventListener('click',function(){var own=binding();if(own)return after(own.timeRemapKeyIntent(active(),null));});
    mode.addEventListener('change',function(){var own=binding();if(own)return after(own.setFootageInterpMode(active(),mode.value));});
    value.addEventListener('change',function(){
      var own=binding(),n=Number(value.value);if(!own)return;
      if(value.value===''||!Number.isFinite(n)){b.refresh();return;}
      return after(own.timeRemapKeyIntent(active(),n));
    });
    replace.addEventListener('click',function(){
      var c=video(),p=shown;if(!c||!p||c.video.declared()!==true)return;
      return after(Promise.resolve(c.video.replaceActive(p.placementId)).then(function(r){return typeof c.surface==='function'?c.surface(r):r;}));
    });
    b.refresh=function(){
      var own=binding(),li=active(),p=own&&li>=0?own.nativeVideoPlacement(li):null;
      shown=p||null;section.hidden=!p;section.style.display=p?'':'none';if(!p)return;
      source.textContent=p.assetId||p.placementId;
      count.textContent=Number.isSafeInteger(p.sourceFrameCount)&&p.sourceFrameCount>0?String(p.sourceFrameCount):'—';
      var facts=own.timeRemapKeyFacts(li),on=!!facts;
      toggle.textContent=tr(on?'ctxDisableTimeRemap':'ctxEnableTimeRemap',on?'Disable time remapping':'Enable time remapping');
      toggle.setAttribute('aria-pressed',String(on));
      interpRow.hidden=!on;key.hidden=!on;value.hidden=!on;
      if(doc.activeElement!==mode)mode.value=own.footageInterpMode(li);
      var c=video(),routed=!!c&&c.video.declared()===true;
      replace.disabled=!routed;if(routed)replace.removeAttribute('title');else replace.title=tr('titleNativeVideoReplaceUnavailable','');
      if(!on)return;
      key.textContent=tr(facts.keyAtFrame?'ctxClearKeyframe':'ctxInsertKeyframe',facts.keyAtFrame?'Clear the keyframe':'Insert a keyframe');
      key.setAttribute('aria-pressed',String(!!facts.keyAtFrame));
      // The eased source frame the native read evaluated; blank (typing only) until it is read.
      var known=typeof facts.value==='number'&&Number.isFinite(facts.value);
      value.classList.toggle('scrub',known);
      if(doc.activeElement!==value&&!value.classList.contains('scrubbing'))value.value=known?String(facts.value):'';
    };
    b.retire=function(){if(b.retired)return;b.retired=true;if(section.parentNode)section.parentNode.removeChild(section);if(inspectors.get(root)===b)inspectors.delete(root);};
    inspectors.set(root,b);b.refresh();return b;
  }
  function retireInspector(root){var b=inspectors.get(root);if(b)b.retire();}
  return Object.freeze({bind:bind,createPorts:createPorts,compose:compose,mountInspector:mountInspector,retire:retireInspector});
}));
