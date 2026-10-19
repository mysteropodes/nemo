/* Ordinary cut consumers share the existing native command queue and UI selection. */
(function(root,factory){
  var cut=typeof module==='object'&&module.exports?require('./native-layer-playhead-cut-contract.js'):root.NemoNativeLayerPlayheadCutContract;
  var api=factory(cut);
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeLayerPlayheadCutUI=api;
}(typeof globalThis==='object'?globalThis:this,function(Cut){
  'use strict';
  var bindings=new WeakMap();
  var pinKeys=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  function same(a,b){return !!a&&!!b&&pinKeys.every(function(k){return a[k]===b[k];});}
  function epoch(a,b){return !!a&&!!b&&pinKeys.slice(0,3).every(function(k){return a[k]===b[k];});}
  function confirmed(root,info,value){
    var p=value&&value.presentation;
    return !!info&&!!value&&value.available===true&&value.kind==='native-object'&&same(info,value)&&
      !!p&&p.owner==='native'&&p.status==='presented'&&p.contextId==='scene-root'&&
      p.frame===root.state.currentFrame&&same(info,p);
  }
  function selection(root,info){
    var s=root.state,rows=s&&s.layers,indices=root._layerSel;
    if(!s||!['drawing','motion'].includes(s.appMode)||s.activeSymbolId!=null||!Array.isArray(rows)||
      !Array.isArray(info.layers)||rows.length!==info.layers.length||
      !rows.every(function(row,i){return row&&info.layers[i]&&row.layerUid===info.layers[i].layerUid;})||
      !Number.isInteger(s.activeLayerIdx)||s.activeLayerIdx<0||s.activeLayerIdx>=rows.length||
      !Array.isArray(indices)||!indices.every(function(i){return Number.isInteger(i)&&i>=0&&i<rows.length;})||
      new Set(indices).size!==indices.length)return null;
    var ids=rows.map(function(row){return row.layerUid;});
    if(ids.some(function(uid){return typeof uid!=='string'||!uid;})||new Set(ids).size!==ids.length)return null;
    var anchor=root._layerSelAnchor;
    if(anchor!==null&&anchor!==undefined&&anchor!==-1&&(!Number.isInteger(anchor)||anchor<0||anchor>=rows.length))return null;
    return JSON.stringify([s.appMode,s.activeSymbolId==null?null:s.activeSymbolId,ids[s.activeLayerIdx],
      indices.map(function(i){return ids[i];}),anchor===null?null:anchor===undefined?'unset':anchor===-1?-1:ids[anchor]]);
  }
  function available(root,ports,session,info,value){
    return session&&ports.currentSession()===session&&session.isActive()===true&&session.blocksLegacy()===true&&
      info&&Number.isSafeInteger(info.contentRevision)&&info.contentRevision>=0&&
      Number.isSafeInteger(info.lifecycleGeneration)&&info.lifecycleGeneration>=0&&
      ['instanceId','documentId','documentSnapshotId'].every(function(k){return typeof info[k]==='string'&&info[k].length>0;})&&
      Number.isInteger(root.state.currentFrame)&&root.state.currentFrame>=0&&root.state.currentFrame<=4294967295&&
      root.state.currentFrame<info.totalFrames&&ports.projectionReady(info)===true&&confirmed(root,info,value);
  }
  function bind(root,ports){
    if(bindings.has(root))return bindings.get(root);
    if(!root.SM||typeof root.SM.splitLayerAtPlayhead!=='function'||!Cut)return null;
    var target=root.SM,original=target.splitLayerAtPlayhead,retired=false,pending=null;
    function current(task){
      return !retired&&pending===task&&root.SM===target&&target.splitLayerAtPlayhead===split&&
        ports.currentSession()===task.session&&task.session.isActive()===true&&task.session.blocksLegacy()===true&&
        task.session.readSubscriptionGeneration()===task.transport&&root.state.currentFrame===task.frame;
    }
    function split(index){
      if(retired||pending)return Promise.resolve(false);
      var session,info,signature,transport,frame,source;
      try{
        session=ports.currentSession();info=session&&session.info();
        if(!session||typeof session.runResourceCommand!=='function'||typeof session.readSubscriptionGeneration!=='function'||
          !available(root,ports,session,info,session.readPresentedObjectSession()))return Promise.resolve(false);
        transport=session.readSubscriptionGeneration();signature=selection(root,info);frame=root.state.currentFrame;
        if(!Number.isSafeInteger(transport)||transport<0||signature===null)return Promise.resolve(false);
        index=index==null?root.state.activeLayerIdx:index;
        if(!Number.isInteger(index)||index<0||index>=info.layers.length)return Promise.resolve(false);
        source=info.layers[index].layerUid;
      }catch(_){return Promise.resolve(false);}
      var task={session:session,transport:transport,frame:frame,source:source,signature:signature,pin:{},uids:info.layers.map(function(l){return l.layerUid;})};
      pinKeys.forEach(function(k){task.pin[k]=info[k];});pending=task;
      return Promise.resolve().then(function(){
        if(!current(task)||selection(root,session.info())!==signature)return false;
        return session.runResourceCommand(async function(action){
          var before=session.info();
          if(!current(task)||!same(before,task.pin)||selection(root,before)!==signature||
            !available(root,ports,session,before,action.readPresentedObjectSession()))return false;
          var request={apiVersion:2,requestId:session.requestId('layer-cut'),instanceId:task.pin.instanceId,documentId:task.pin.documentId,
            operation:Cut.OP,expectedRevision:task.pin.contentRevision,
            payload:{command:'object.layer.cut',contextId:'scene-root',layerUid:source,frame:frame}};
          Cut.payload(request.operation,request.payload);
          // The common SDK validates after the transport advances its binding.
          // An uncertain receipt must refresh the existing owner, never replay the cut.
          var receipt;
          try{
            receipt=await action.dispatch(request);
            if(!receipt||receipt.apiVersion!==2||typeof receipt.ok!=='boolean')throw new Error('Native cut response is uncorrelated');
            Cut.correlate(request,receipt);
          }catch(_){
            await action.reconcile();
            return false;
          }
          if(receipt.ok!==true||receipt.result.applied!==true)return false;
          await action.reconcile(receipt.contentRevision);
          return receipt;
        });
      }).then(function(receipt){
        // The public presented getter is intentionally fenced until the queued action settles.
        if(!receipt||!current(task))return false;
        var next=session.info(),presented=session.readPresentedObjectSession();
        if(!epoch(task.pin,next)||next.contentRevision!==receipt.contentRevision||
          next.documentSnapshotId===task.pin.documentSnapshotId||selection(root,next)!==signature||
          !available(root,ports,session,next,presented))return false;
        var uid=receipt.result.layerUid,sourceAt=task.uids.indexOf(source),freshAt=sourceAt+1;
        if(task.uids.includes(uid)||next.layers.length!==task.uids.length+1||
          !next.layers[freshAt]||next.layers[freshAt].layerUid!==uid||next.layers[freshAt].locked===true||
          !task.uids.every(function(old,i){return next.layers[i<freshAt?i:i+1].layerUid===old;}))return false;
        if(ports.selectLayer(freshAt,false,uid,next)!==true)return false;
        var finalInfo=session.info();
        return current(task)&&same(next,finalInfo)&&available(root,ports,session,finalInfo,session.readPresentedObjectSession())&&
          root.state.activeLayerIdx===freshAt&&Array.isArray(root._layerSel)&&root._layerSel.length===1&&
          root._layerSel[0]===freshAt&&root.state.layers[freshAt].layerUid===uid;
      }).catch(function(){return false;}).finally(function(){if(pending===task)pending=null;});
    }
    target.splitLayerAtPlayhead=split;
    var binding=Object.freeze({split:split,dispose:function(){
      retired=true;pending=null;
      // An active or indeterminate native owner must retain a refusing entry.
      var session=ports.currentSession();
      if(session&&session.blocksLegacy()===true)return false;
      if(root.SM===target&&target.splitLayerAtPlayhead===split)target.splitLayerAtPlayhead=original;
      bindings.delete(root);return true;
    }});
    bindings.set(root,binding);return binding;
  }
  function createPorts(root,base){
    var entry=null;
    return Object.freeze({bindCutEntries:function(){
      if(entry)return true;entry=bind(root,base);return !!entry;
    },disposeCutEntries:function(){if(entry&&entry.dispose()===true)entry=null;}});
  }
  function compose(root,base,policy){
    var own=createPorts(root,base);
    return Object.freeze(Object.assign({},policy,{
      bindPolicyEntries:function(){var ready=policy.bindPolicyEntries();return own.bindCutEntries()&&ready;},
      disposePolicyEntries:function(){own.disposeCutEntries();policy.disposePolicyEntries();}
    }));
  }
  function splitSynchronous(index,frame,ports){
    // The synchronous scripting contract must refuse before moving the playhead or dispatching.
    if(ports.nativeBlocked())return ports.fail('native split requires asynchronous scripting support');
    var before=ports.layers().length;
    if(frame!=null)ports.move(frame|0);
    ports.split(index);
    if(ports.layers().length===before)return ports.fail('coupe impossible : la frame est hors du calque');
    return ports.layer(index+1);
  }
  function scripting(ports){
    return function(frame){return splitSynchronous(this.index,frame,ports);};
  }
  return Object.freeze({bind:bind,createPorts:createPorts,compose:compose,splitSynchronous:splitSynchronous,scripting:scripting});
}));
