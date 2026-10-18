/* Correlated inactive native-project preparation for the existing project tabs. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectInactiveProjectTabs=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var ID=/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  var HEX=/^[a-f0-9]{64}$/;
  function object(v){return !!v&&typeof v==='object'&&!Array.isArray(v);}
  function exact(v,keys){return object(v)&&Object.keys(v).length===keys.length&&keys.every(function(k){
    var d=Object.getOwnPropertyDescriptor(v,k);return d&&Object.hasOwn(d,'value');});}
  function int(v,min){return Number.isSafeInteger(v)&&v>=min;}
  function create(root,ports){
    if(!ports||['owner','invoke','commit','remove','notice','baseName'].some(function(k){return typeof ports[k]!=='function';}))
      throw new TypeError('Inactive native-project tab ports are unavailable');
    var busy=false,held=null,uncertain=null,lastId=null;
    function newId(){var value=ports.requestId?ports.requestId():root.crypto&&root.crypto.randomUUID&&root.crypto.randomUUID();
      if(typeof value!=='string'||!ID.test(value)||value===lastId)throw new Error('Native inactive request identity unavailable');lastId=value;return value;}
    function pins(){var p=ports.owner();
      if(!object(p)||p.apiVersion!==2||!ID.test(p.instanceId)||!ID.test(p.documentId)||!int(p.contentRevision,0)||
        !ID.test(p.documentSnapshotId)||!int(p.lifecycleGeneration,1))throw new Error('Active native project pins are unavailable');
      return {apiVersion:2,instanceId:p.instanceId,documentId:p.documentId,contentRevision:p.contentRevision,
        documentSnapshotId:p.documentSnapshotId,lifecycleGeneration:p.lifecycleGeneration};}
    function sameEpoch(a,b){return a.instanceId===b.instanceId&&a.documentId===b.documentId&&a.lifecycleGeneration===b.lifecycleGeneration;}
    function samePins(a,b){return sameEpoch(a,b)&&a.contentRevision===b.contentRevision&&a.documentSnapshotId===b.documentSnapshotId;}
    function request(p,action){return {apiVersion:2,requestId:newId(),instanceId:p.instanceId,documentId:p.documentId,
      expectedRevision:p.contentRevision,documentSnapshotId:p.documentSnapshotId,lifecycleGeneration:p.lifecycleGeneration,action:action};}
    function send(req){return Promise.resolve().then(function(){return ports.invoke('nemo_native_object_inactive_project',{requestJson:JSON.stringify(req)});});}
    function receipt(value,req){
      var required=['apiVersion','requestId','instanceId','documentId','contentRevision','documentSnapshotId','lifecycleGeneration','state','preparationToken','project','retryExecution'];
      var allowed=required.concat(['pendingRequestId','recoveredRequest']);
      if(!object(value)||required.some(function(k){return !Object.hasOwn(value,k);})||
        Object.keys(value).some(function(k){return allowed.indexOf(k)<0;})||
        value.apiVersion!==2||value.requestId!==req.requestId||value.instanceId!==req.instanceId||value.documentId!==req.documentId||
        value.contentRevision!==req.expectedRevision||value.documentSnapshotId!==req.documentSnapshotId||
        value.lifecycleGeneration!==req.lifecycleGeneration||value.retryExecution!==false||
        Object.hasOwn(value,'pendingRequestId')&&!ID.test(value.pendingRequestId)||
        Object.hasOwn(value,'recoveredRequest')&&(!object(value.recoveredRequest)||!ID.test(value.recoveredRequest.requestId)||!Object.hasOwn(value.recoveredRequest,'outcome')))
        throw new Error('Native inactive receipt correlation failed');
      return value;
    }
    function safeProject(value,req,expectedRequest){
      if(!object(value)||!ID.test(value.documentSnapshotId)||!ID.test(value.preparationToken)||
        !ID.test(value.preparationRequestId)||value.instanceId!==req.instanceId||!int(value.contentRevision,0)||
        !HEX.test(value.documentDigest)||!int(value.byteCount,1)||!int(value.canvasW,1)||!int(value.canvasH,1)||
        !int(value.totalFrames,1)||typeof value.fps!=='number'||!Number.isFinite(value.fps)||value.fps<=0||
        !Array.isArray(value.layers)||!object(value.origin)||value.origin.documentId!==req.documentId||
        !ID.test(value.origin.documentSnapshotId)||!int(value.origin.contentRevision,0))throw new Error('Native inactive project metadata is invalid');
      if(expectedRequest&&value.preparationRequestId!==expectedRequest)throw new Error('Native inactive project belongs to another request');
      return Object.freeze({kind:'native-object-inactive',path:null,name:'Inactive project',preparationToken:value.preparationToken,
        preparationRequestId:value.preparationRequestId,instanceId:req.instanceId,ownerDocumentId:req.documentId,
        lifecycleGeneration:req.lifecycleGeneration,
        documentSnapshotId:value.documentSnapshotId,contentRevision:value.contentRevision,originSnapshotId:value.origin.documentSnapshotId,
        originRevision:value.origin.contentRevision,
        digest:value.documentDigest,byteCount:value.byteCount,canvasW:value.canvasW,canvasH:value.canvasH,fps:value.fps,totalFrames:value.totalFrames});
    }
    function tabMeta(project,path){var name=path?ports.baseName(path):'Inactive project';
      return Object.freeze(Object.assign({},project,{path:path||null,name:name}));}
    function add(project,path){
      var meta=tabMeta(project,path);
      held=meta;
      try{if(ports.commit(meta)!==true)throw new Error('Inactive tab row was not committed');return true;}
      catch(error){uncertain={kind:'prepare',requestId:meta.preparationRequestId,path:path||null};throw error;}
    }
    function show(message){try{ports.notice(message);}catch(_){} }
    function requireReceipt(value,req){return receipt(value,req);}
    function inspect(p,token,recoverId){var action={kind:'inspect'};
      if(token)action.preparationToken=token;if(recoverId)action.recoverRequestId=recoverId;
      var req=request(p,action);return send(req).then(function(v){return {request:req,value:requireReceipt(v,req)};});}
    function recoveredError(value,id){var recovered=value.recoveredRequest;
      if(!object(recovered)||recovered.requestId!==id||!Object.hasOwn(recovered,'outcome'))return null;
      if(!object(recovered.outcome)||recovered.outcome.ok!==false||!object(recovered.outcome.error))return null;
      return recovered.outcome.error;
    }
    function recoveredClose(value,id,token){var recovered=value.recoveredRequest,outcome=recovered&&recovered.outcome,result=outcome&&outcome.value;
      return !!(value.state==='inactive-absent'&&value.project===null&&exact(recovered,['requestId','outcome'])&&recovered.requestId===id&&
        exact(outcome,['ok','value'])&&outcome.ok===true&&exact(result,['state','preparationToken'])&&
        result.state==='inactive-closed'&&result.preparationToken===token);
    }
    function recover(pending,p){
      if(!sameEpoch(pending.pins,p))throw new Error('Active native project owner changed during inactive request');
      return inspect(p,pending.kind==='close'?null:pending.token,pending.request.requestId).then(function(result){
        var v=result.value,project=null;
        if(v.project)project=safeProject(v.project,result.request,
          pending.kind==='prepare'?pending.request.requestId:null);
        if(pending.kind==='prepare'){
          if(project&&project.preparationToken===v.preparationToken&&project.originSnapshotId===pending.request.documentSnapshotId&&
            project.originRevision===pending.request.expectedRevision){add(project,pending.path);uncertain=null;return {status:'prepared',meta:held};}
          var error=recoveredError(v,pending.request.requestId);
          if(v.state==='inactive-absent'&&error&&['notDispatched','closed'].includes(error.details&&error.details.disposition)){
            uncertain=null;return {status:'absent'};
          }
          if(project&&project.preparationRequestId!==pending.request.requestId){add(project,null);return {status:'indeterminate',meta:held};}
          return {status:'indeterminate'};
        }
        var error=recoveredError(v,pending.request.requestId);
        if(v.state==='inactive-absent'&&recoveredClose(v,pending.request.requestId,pending.token)){
          if(held&&held.preparationToken===pending.token&&ports.remove(held)!==true)throw new Error('Inactive row removal was not confirmed');
          held=null;uncertain=null;return {status:'closed'};
        }
        if(project&&project.preparationToken===pending.token){
          if(error&&error.details&&error.details.disposition==='notDispatched'){uncertain=null;return {status:'retained',meta:held};}
          return {status:'indeterminate',meta:held};
        }
        if(project&&(!held||project.preparationToken!==held.preparationToken))add(project,null);
        return {status:'indeterminate',meta:held};
      });
    }
    function invokeWithReplay(req){return send(req).catch(function(first){
      if(first&&first.details&&first.details.disposition==='notDispatched')throw first;
      return send(req);
    });}
    function reconcile(){
      if(!uncertain)return Promise.resolve({status:'idle'});
      var pending=uncertain,p=pins();
      return recover(pending,p);
    }
    async function openPath(path){
      if(busy)return {status:'busy'};busy=true;
      try{
        if(typeof path!=='string'||!(/^(\/|[A-Za-z]:[\\/])/.test(path)))throw new Error('Native project path must be absolute');
        var p=pins();
        if(uncertain){var recovery=await recover(uncertain,p);if(recovery.status==='indeterminate'){
          show('Inactive project state is still being reconciled; Open is unavailable.');return recovery;}
          if(recovery.status==='prepared'||recovery.status==='retained')return recovery;
        }
        if(held){show('An inactive project is already retained. Close it before preparing another.');return {status:'retained',meta:held};}
        var current=await inspect(p,null,null),existing=current.value;
        if(existing.project){var project=safeProject(existing.project,current.request,null);add(project,null);
          show(existing.state==='inactive-pending'?'An inactive project request is still pending; Open is blocked.':'An inactive project is retained; close it before preparing another.');
          return {status:existing.state==='inactive-pending'?'indeterminate':'retained',meta:held};}
        if(existing.state!=='inactive-absent'||existing.project!==null||existing.preparationToken!==null){
          show('Inactive project state is unresolved; Open is unavailable.');return {status:'indeterminate'};}
        p=pins();var req=request(p,{kind:'prepare',sourcePath:path});var pending={kind:'prepare',request:req,pins:p,path:path};uncertain=pending;
        try{
          var v=requireReceipt(await invokeWithReplay(req),req);
          if(v.state!=='inactive-prepared'||!v.project||!ID.test(v.preparationToken)){throw new Error('Unexpected inactive prepare receipt');}
          var prepared=safeProject(v.project,req,req.requestId);
          if(prepared.preparationToken!==v.preparationToken||prepared.originSnapshotId!==req.documentSnapshotId||
            prepared.originRevision!==req.expectedRevision)throw new Error('Inactive prepare token or owner mismatch');
          add(prepared,path);uncertain=null;show('Project prepared; switching remains unavailable in this build.');return {status:'prepared',meta:held};
        }catch(error){
          if(error&&error.details&&['notDispatched','closed'].includes(error.details.disposition)){
            uncertain=null;show('Inactive project was not prepared: '+(error.message||'request refused'));return {status:'refused'};
          }
          var result=await recover(pending,pins());
          if(result.status==='prepared'||result.status==='retained')return result;
          show('Inactive project preparation is indeterminate; A is unchanged and another Open is blocked.');return result;
        }
      }catch(error){show('Inactive project preparation is unavailable: '+(error&&error.message||'native state unavailable'));return {status:'refused'};}
      finally{busy=false;}
    }
    async function close(meta){
      if(busy||uncertain||!held||meta!==held)return {status:'unavailable'};busy=true;
      try{
        var p=pins(),req=request(p,{kind:'close',preparationToken:held.preparationToken});
        var pending={kind:'close',request:req,pins:p,token:held.preparationToken};uncertain=pending;
        try{
          var v=requireReceipt(await invokeWithReplay(req),req);
          if(v.state!=='inactive-closed'||v.preparationToken!==pending.token||v.project!==null)throw new Error('Inactive close receipt refused');
          if(ports.remove(held)!==true)throw new Error('Inactive row removal was not confirmed');
          held=null;uncertain=null;show('Inactive project closed.');return {status:'closed'};
        }catch(error){
          if(error&&error.details&&['notDispatched','closed'].includes(error.details.disposition)){
            uncertain=null;show('Inactive project remains open: '+(error.message||'close refused'));return {status:'refused'};
          }
          var result=await recover(pending,pins());
          if(result.status==='closed'){show('Inactive project closed.');return result;}
          show('Inactive project close is unconfirmed; its tab remains visible.');return result;
        }
      }catch(error){show('Inactive project close is unavailable: '+(error&&error.message||'native state unavailable'));return {status:'refused'};}
      finally{busy=false;}
    }
    async function activateClick(meta){
      if(!held||meta!==held){show('Inactive project identity is unavailable; switching is unavailable in this build.');return {status:'unavailable'};}
      try{
        var p=pins();if(!sameEpoch({instanceId:held.instanceId,documentId:held.ownerDocumentId,lifecycleGeneration:held.lifecycleGeneration},p))throw new Error('Native owner changed');
        var result=await inspect(p,held.preparationToken,null),v=result.value;
        if(v.state!=='inactive-retained'||!v.project||v.project.preparationToken!==held.preparationToken)throw new Error('Inactive project retention is unconfirmed');
        show('Switching project tabs is unavailable in this build.');return {status:'unavailable'};
      }catch(error){show('Inactive project remains visible, but switching is unavailable: '+(error&&error.message||'retention is unconfirmed'));return {status:'indeterminate'};}
    }
    return Object.freeze({openPath:openPath,close:close,activateClick:activateClick,reconcile:reconcile,
      // Replacement drops this owner's retained project; uncertainty is not
      // absence, even when a lost reply has not produced a visible row.
      canReplaceActiveProject:function(){return !busy&&!held&&!uncertain;},
      held:function(){return held;},isBusy:function(){return busy;}});
  }
  return Object.freeze({create:create});
}));
