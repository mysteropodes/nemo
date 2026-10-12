/* Private saved-UI acknowledgement contract; no producer, loader or file IO. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectSavedAcknowledgement=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function object(v){return v!==null&&typeof v==='object'&&!Array.isArray(v);}
  function fields(v,names){return object(v)&&Object.keys(v).length===names.length&&names.every(function(n){return Object.prototype.hasOwnProperty.call(v,n);});}
  function id(v){return typeof v==='string'&&/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(v);}
  function integer(v,min){return Number.isSafeInteger(v)&&v>=min;}
  function utf8(v){
    var count=0;
    for(var i=0;i<v.length;i++){
      var c=v.codePointAt(i);
      if(c>=0xd800&&c<=0xdfff)return -1;
      count+=c<0x80?1:c<0x800?2:c<0x10000?3:4;
      if(c>0xffff)i++;
    }
    return count;
  }
  function ui(v){
    return fields(v,['handle','generation'])&&(id(v.handle)||object(v.handle))&&integer(v.generation,1)?
      Object.freeze({handle:v.handle,generation:v.generation}):null;
  }
  function sameUI(a,b){return !!(a&&b&&a.handle===b.handle&&a.generation===b.generation);}
  function pin(v){
    if(!fields(v,['instanceId','documentId','contentRevision','lifecycleGeneration','documentSnapshotId'])||
        !id(v.instanceId)||!id(v.documentId)||!id(v.documentSnapshotId)||
        !integer(v.contentRevision,0)||!integer(v.lifecycleGeneration,1))return null;
    return Object.freeze({instanceId:v.instanceId,documentId:v.documentId,contentRevision:v.contentRevision,
      lifecycleGeneration:v.lifecycleGeneration,documentSnapshotId:v.documentSnapshotId});
  }
  function samePin(a,b){return !!(a&&b&&a.instanceId===b.instanceId&&a.documentId===b.documentId&&
    a.contentRevision===b.contentRevision&&a.lifecycleGeneration===b.lifecycleGeneration&&a.documentSnapshotId===b.documentSnapshotId);}
  function presented(v){
    if(!object(v)||v.apiVersion!==2||v.available!==true||v.kind!=='native-object')return null;
    var value=pin({instanceId:v.instanceId,documentId:v.documentId,contentRevision:v.contentRevision,
      lifecycleGeneration:v.lifecycleGeneration,documentSnapshotId:v.documentSnapshotId}), p=v.presentation;
    return value&&object(p)&&p.owner==='native'&&p.status==='presented'&&p.contextId==='scene-root'&&
      integer(p.frame,0)&&integer(p.viewGeneration,1)&&id(p.workId)&&samePin(value,p)?value:null;
  }
  function publication(ticket,value){
    if(!fields(value,['written','serialization']))return null;
    var w=value.written,s=value.serialization;
    if(!(fields(w,['status','eligibleCurrentSave','pin','destinationPath','receipt'])||
        fields(w,['status','eligibleCurrentSave','pin','destinationPath','receipt','reason']))||
        !['published-current','published-stale','published-cleanup-pending','refused','indeterminate'].includes(w.status)||
        typeof w.eligibleCurrentSave!=='boolean'||typeof w.destinationPath!=='string'||
        w.destinationPath[0]!=='/'||w.destinationPath.indexOf('\0')!==-1)return null;
    var p=pin(w.pin),r=w.receipt;
    if(!p||!fields(r,['apiVersion','requestId','instanceId','documentId','atRevision','documentSnapshotId','byteCount','published','cleanupPending'])||
        r.apiVersion!==2||r.requestId!==ticket.saveRequestId||r.instanceId!==p.instanceId||r.documentId!==p.documentId||
        r.atRevision!==p.contentRevision||r.documentSnapshotId!==p.documentSnapshotId||typeof r.published!=='boolean'||typeof r.cleanupPending!=='boolean'||
        !integer(r.byteCount,1)||r.byteCount>4096)return null;
    if(!fields(s,['apiVersion','requestId','instanceId','documentId','contentRevision','ok','result'])||s.apiVersion!==2||
        s.requestId!==ticket.serializerRequestId||s.instanceId!==p.instanceId||s.documentId!==p.documentId||
        s.contentRevision!==p.contentRevision||s.ok!==true||!fields(s.result,['atRevision','documentSnapshotId','documentJson'])||
        s.result.atRevision!==p.contentRevision||s.result.documentSnapshotId!==p.documentSnapshotId||
        typeof s.result.documentJson!=='string'||utf8(s.result.documentJson)!==r.byteCount)return null;
    var bytes=s.result.documentJson;
    var envelope={apiVersion:2,requestId:ticket.serializerRequestId,instanceId:p.instanceId,documentId:p.documentId,
      contentRevision:p.contentRevision,ok:true,result:{atRevision:p.contentRevision,documentSnapshotId:p.documentSnapshotId,documentJson:bytes}};
    if(utf8(JSON.stringify(envelope))>4096)return null;
    return Object.freeze({ui:ticket.ui,pin:p,destinationPath:w.destinationPath,documentJson:bytes,
      eligibility:w.status==='published-current'&&w.eligibleCurrentSave===true,
      receipt:Object.freeze({apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
        atRevision:r.atRevision,documentSnapshotId:r.documentSnapshotId,byteCount:r.byteCount,published:r.published,cleanupPending:r.cleanupPending})});
  }
  function result(status,written){return Object.freeze({status:status,written:written===undefined?null:written,acknowledged:status==='acknowledged'});}
  function create(ports){
    // Port implementations own actual UI state. The sink must synchronously
    // compare-and-consume expected UI/pin/receipt, return true on full commit,
    // or false with ZERO effects. Unknown/partial failures are indeterminate.
    var readUI=ports.readUISession.bind(ports),readPresented=ports.readPresentedObjectSession.bind(ports);
    var commit=ports.commitSavedMetadata.bind(ports),invalidate=ports.markSavedIndeterminate.bind(ports);
    var owned=new WeakSet(),used=new WeakSet(),requestIds=new Set(),active=null;
    function currentUI(){try{return ui(readUI());}catch(error){return null;}}
    function isCurrent(ticket){var now=currentUI();return owned.has(ticket)&&active===ticket&&sameUI(ticket.ui,now);}
    function begin(options){
      if(!object(options)||!id(options.saveRequestId)||!id(options.serializerRequestId)||
          options.saveRequestId===options.serializerRequestId)throw new Error('Distinct save and serializer request IDs required');
      if(requestIds.has(options.saveRequestId))throw new Error('Save request ID already belongs to an attempt');
      var original=currentUI();if(!original)throw new Error('Original UI session unavailable');
      // Call before E4A starts; never recapture another UI after publication.
      active=Object.freeze({ui:original,saveRequestId:options.saveRequestId,serializerRequestId:options.serializerRequestId});
      requestIds.add(active.saveRequestId);owned.add(active);return active;
    }
    function cancelled(options){
      try{var current=typeof options.isCurrent!=='function'||options.isCurrent()===true;return !current||!!(options.signal&&options.signal.aborted);}
      catch(error){return true;}
    }
    function indeterminate(plan){
      // Invalidation is scoped to the original session and pin. It is NOT a
      // rollback; its implementation must not dirty a replacement document.
      if(sameUI(plan.ui,currentUI())){try{invalidate(plan);}catch(error){}}
      return result('written-indeterminate',true);
    }
    async function acknowledge(ticket,input,options){
      options=options||{};
      options={signal:options.signal,isCurrent:options.isCurrent};
      if(!object(ticket)||!owned.has(ticket))return result('refused');
      var plan;try{plan=publication(ticket,input);}catch(error){return result('refused');}
      if(!plan)return result('refused');
      if(!plan.receipt.published)return result('not-published',false);
      if(plan.receipt.cleanupPending)return result('written-cleanup-pending',true);
      if(!plan.eligibility)return result('written-stale',true);
      if(used.has(ticket))return result('refused',true);
      if(!isCurrent(ticket)||cancelled(options))return result('written-stale',true);
      var current;
      try{current=presented(await readPresented());}catch(error){return result('written-stale',true);}
      // Check AFTER both the awaited read and every synchronous UI read.
      if(!samePin(plan.pin,current)||!isCurrent(ticket)||cancelled(options))return result('written-stale',true);
      if(used.has(ticket))return result('refused',true);
      used.add(ticket);
      try{
        var applied=commit(plan);
        if(applied===false)return result('written-stale',true);
        if(object(applied)&&typeof applied.then==='function')Promise.resolve(applied).catch(function(){});
        if(applied!==true||!isCurrent(ticket)||cancelled(options))return indeterminate(plan);
        return result('acknowledged',true);
      }catch(error){return indeterminate(plan);}
    }
    return Object.freeze({begin:begin,isCurrent:isCurrent,acknowledge:acknowledge});
  }
  return Object.freeze({create:create});
}));
