/* Public v2 contract of the native linked-video import/replace operation (T12 point 8).
 * Pure validation of the closed payload, the closed receipt and its correlation to the
 * request: no state, no transport, no document writer. The native owner stays the only
 * authority; the host probes the file and the engine repeats every rule on the bytes. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeVideoAssetContract=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  const LINK='command.document.object.asset.video.link';
  const MAX_PATH_BYTES=4096,MAX_FACT=4294967295;
  function fail(message){throw new TypeError(message);}
  function plain(v){return !!v&&typeof v==='object'&&!Array.isArray(v);}
  function own(v,k){return Object.prototype.hasOwnProperty.call(v,k);}
  function exact(v,required,label){
    if(!plain(v))fail(label+' must be an object');
    if(!required.every(k=>own(v,k))||!Object.keys(v).every(k=>required.includes(k)))fail(label+' fields invalid');
  }
  function bytes(v){return new TextEncoder().encode(v).length;}
  function int(v,min,max){if(!Number.isSafeInteger(v)||v<min||v>max)fail('integer invalid');}
  function snapshot(v){if(typeof v!=='string'||!/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(v))fail('documentSnapshotId invalid');}
  // Opaque owner ids: non-blank, no NUL, at most 255 UTF-8 bytes (the engine's placement rule).
  function opaque(v,label){if(typeof v!=='string'||!v.trim()||v.includes('\0')||bytes(v)>255)fail(label+' invalid');}
  // A host file path is only non-empty: the native validator does not trim it.
  function filePath(v){if(typeof v!=='string'||!v.length||v.includes('\0')||bytes(v)>MAX_PATH_BYTES)fail('source path invalid');}
  function intent(v){
    if(plain(v)&&v.kind==='newLayer'){exact(v,['kind'],'video intent');return;}
    exact(v,['kind','placementId'],'video intent');
    if(v.kind!=='replace')fail('video intent kind invalid');
    opaque(v.placementId,'placementId');
  }
  function payload(op,p){
    if(op!==LINK)fail('video operation invalid');
    exact(p,['documentSnapshotId','lifecycleGeneration','source','intent'],'video link');
    snapshot(p.documentSnapshotId);int(p.lifecycleGeneration,1,Number.MAX_SAFE_INTEGER);
    exact(p.source,['kind','path'],'video source');if(p.source.kind!=='file')fail('video source kind invalid');filePath(p.source.path);
    intent(p.intent);
  }
  function result(op,v){
    if(op!==LINK)fail('video operation invalid');
    exact(v,['applied','historyEntriesAdded','assetId','placementId','target','newLayer','sourceWidth','sourceHeight','sourceFrameCount'],'video result');
    if(v.applied!==true||v.historyEntriesAdded!==1)fail('video link must apply once');
    opaque(v.assetId,'assetId');opaque(v.placementId,'placementId');
    exact(v.target,['contextId','layerUid'],'video target');
    if(v.target.contextId!=='scene-root')fail('video target context invalid');opaque(v.target.layerUid,'layerUid');
    if(typeof v.newLayer!=='boolean')fail('newLayer invalid');
    int(v.sourceWidth,1,MAX_FACT);int(v.sourceHeight,1,MAX_FACT);int(v.sourceFrameCount,1,MAX_FACT);
  }
  function correlate(q,r){
    if(!plain(q)||q.operation!==LINK)fail('video operation invalid');
    if(!plain(r)||r.requestId!==q.requestId)fail('response request mismatch');
    // A typed owner refusal names the owner that answered: wrong_instance from another instance, wrong_document from the same one.
    const refusal=r.ok===false&&r.result===undefined&&plain(r.error);
    const ownerRefusal=refusal&&(r.error.code==='wrong_instance'&&r.instanceId!==q.instanceId||r.error.code==='wrong_document'&&r.instanceId===q.instanceId);
    if((r.instanceId!==q.instanceId||r.documentId!==q.documentId)&&!ownerRefusal)fail('response owner mismatch');
    if(r.ok!==true)return;
    result(LINK,r.result);
    const next=q.expectedRevision+1,res=r.result,want=q.payload.intent;
    if(r.contentRevision!==next)fail('receipt revision mismatch');
    if(res.newLayer!==(want.kind==='newLayer'))fail('video intent disposition mismatch');
    if(want.kind==='newLayer'){
      // The owner mints the placement from the request id and creates the layer at the published revision.
      if(res.placementId!=='placement-'+q.requestId||!res.target.layerUid.startsWith('native-layer:'+next+':'))fail('video layer target mismatch');
    }else if(res.placementId!==want.placementId)fail('video replace target mismatch');
  }
  function paths(op){
    return op===LINK?new Set(['payload.source.path','payload.intent.placementId','result.assetId','result.placementId','result.target.layerUid']):new Set();
  }
  const api={admitted:op=>op===LINK,requiresRevision:op=>op===LINK,payload:payload,result:result,correlate:correlate,paths:paths};
  // The common SDK consumes one contract object; this joins it in front of another contract of the same interface
  // and keeps that contract's other members (such as requestByteLimit).
  function compose(base){
    if(!base)return Object.freeze(Object.assign({},api));
    return Object.freeze(Object.assign({},base,{admitted:op=>api.admitted(op)||base.admitted(op),requiresRevision:op=>api.admitted(op)?true:base.requiresRevision(op),
      payload:(op,p,e)=>api.admitted(op)?api.payload(op,p):base.payload(op,p,e),result:(op,v)=>api.admitted(op)?api.result(op,v):base.result(op,v),
      correlate:(q,r)=>api.admitted(q.operation)?api.correlate(q,r):base.correlate(q,r),paths:op=>api.admitted(op)?api.paths(op):base.paths(op)}));
  }
  return Object.freeze(Object.assign({compose:compose,LINK:LINK,MAX_PATH_BYTES:MAX_PATH_BYTES},api));
}));
