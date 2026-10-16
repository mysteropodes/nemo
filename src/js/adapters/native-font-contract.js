/* PRIVATE G41 proposal: joined by the original common v2 SDK only. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeFontContract=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  const IMPORT='command.document.object.asset.font.import',QUERY='query.document.object.asset.font';
  const fonts=new Set(['Roboto-Regular','Roboto-Bold','DejaVuSans-Regular','DejaVuSans-Bold','DejaVuSerif-Regular','DejaVuSerif-Bold','Manrope-Regular','Inter-Regular']);
  function admitted(op){return op===IMPORT||op===QUERY;}
  function fail(message){throw new TypeError(message);}
  function exact(v,keys){if(!v||typeof v!=='object'||Array.isArray(v)||Object.keys(v).length!==keys.length||!keys.every(k=>Object.prototype.hasOwnProperty.call(v,k)))fail('font fields invalid');}
  function int(v,min,max=Number.MAX_SAFE_INTEGER){if(!Number.isSafeInteger(v)||v<min||v>max)fail('font integer invalid');}
  function opaque(v){if(typeof v!=='string'||!v.trim()||v.includes('\0')||new TextEncoder().encode(v).length>255)fail('font opaque string invalid');}
  function id(v){if(typeof v!=='string'||!/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(v))fail('font identifier invalid');}
  function hash(v){if(typeof v!=='string'||! /^[a-f0-9]{64}$/.test(v))fail('font digest invalid');}
  function data(v){opaque(v.assetId);if(v.mediaType!=='font/ttf')fail('font media type invalid');int(v.encodedBytes,1,16*1024*1024);hash(v.encodedSha256);}
  function row(v){
    if(v.available===true){exact(v,['assetId','name','sourceKind','available','mediaType','encodedBytes','encodedSha256']);data(v);if(v.sourceKind!=='embedded')fail('font source invalid');}
    else{exact(v,['assetId','name','sourceKind','available','reason']);opaque(v.assetId);if(v.available!==false||v.sourceKind!=='linked'||v.reason!=='linked_font_unavailable')fail('font linked result invalid');}
    // Names use the document AssetRecord contract; exact spelling is retained.
    if(typeof v.name!=='string'||!v.name.trim()||v.name.includes('\0')||new TextEncoder().encode(v.name).length>255)fail('font name invalid');
  }
  function payload(op,p){
    if(op===IMPORT){exact(p,['command','documentSnapshotId','lifecycleGeneration','source']);if(p.command!=='object.asset.font.import')fail('font command invalid');id(p.documentSnapshotId);int(p.lifecycleGeneration,1);
      if(p.source&&p.source.kind==='bundled'){exact(p.source,['kind','fontKey']);if(!fonts.has(p.source.fontKey))fail('font key invalid');}
      else{exact(p.source,['kind','family','weight','italic']);if(p.source.kind!=='google'||typeof p.source.italic!=='boolean')fail('font source invalid');opaque(p.source.family);int(p.source.weight,1,1000);}return;}
    if(p&&p.action==='receipt'){exact(p,['action','forRequestId']);id(p.forRequestId);return;}
    const fields={list:['offset','limit'],inspect:['assetId'],read:['assetId','offset','length']};
    if(!p||!Object.prototype.hasOwnProperty.call(fields,p.action))fail('font action invalid');
    exact(p,['action','atRevision','documentSnapshotId',...fields[p.action]]);int(p.atRevision,0);id(p.documentSnapshotId);
    if(p.action==='list'){int(p.offset,0,1024);if(p.limit!==1)fail('font list bound invalid');}
    else{opaque(p.assetId);if(p.action==='read'){int(p.offset,0,16*1024*1024-1);int(p.length,1,1536);}}
  }
  function result(op,v){
    if(op===IMPORT){exact(v,['applied','historyEntriesAdded','assetId','mediaType','encodedBytes','encodedSha256']);if(v.applied!==true||v.historyEntriesAdded!==1)fail('font disposition invalid');data(v);return;}
    if(v&&Object.prototype.hasOwnProperty.call(v,'status')){
      if(v.status==='not_found')exact(v,['status','forRequestId']);
      else{if(v.status==='committed'){exact(v,['status','forRequestId','requestFingerprintSha256','contentRevision','assetId','mediaType','encodedBytes','encodedSha256']);data(v);}
        else if(v.status==='refused'){exact(v,['status','forRequestId','requestFingerprintSha256','contentRevision','code']);if(!['invalid_request','wrong_instance','wrong_document','stale_revision','busy_conflict','unavailable','not_found','cancelled_before_dispatch','internal'].includes(v.code))fail('font refusal code invalid');}
        else fail('font receipt status invalid');hash(v.requestFingerprintSha256);int(v.contentRevision,0);}id(v.forRequestId);return;}
    int(v.atRevision,0);id(v.documentSnapshotId);
    if(Object.prototype.hasOwnProperty.call(v,'rows')){exact(v,['atRevision','documentSnapshotId','offset','nextOffset','totalFontRecords','rows']);int(v.offset,0);int(v.totalFontRecords,0,1024);if(v.nextOffset!==null)int(v.nextOffset,0);if(!Array.isArray(v.rows)||v.offset>v.totalFontRecords||v.rows.length!==(v.offset<v.totalFontRecords?1:0)||v.nextOffset!==(v.offset+v.rows.length<v.totalFontRecords?v.offset+v.rows.length:null))fail('font list invalid');v.rows.forEach(row);}
    else if(Object.prototype.hasOwnProperty.call(v,'font')){exact(v,['atRevision','documentSnapshotId','font']);row(v.font);}
    else{exact(v,['atRevision','documentSnapshotId','assetId','offset','returnedLength','totalLength','encodedSha256','base64']);opaque(v.assetId);int(v.offset,0);int(v.returnedLength,1,1536);int(v.totalLength,1,16*1024*1024);hash(v.encodedSha256);
      if(typeof v.base64!=='string'||! /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(v.base64)||atob(v.base64).length!==v.returnedLength||btoa(atob(v.base64))!==v.base64||v.offset+v.returnedLength>v.totalLength)fail('font chunk invalid');}
  }
  function correlate(q,r){
    if(r.instanceId!==q.instanceId||r.documentId!==q.documentId)fail('font response owner mismatch');if(!r.ok)return;
    const p=q.payload,v=r.result;
    if(q.operation===IMPORT){if(r.contentRevision!==q.expectedRevision+1)fail('font import revision mismatch');return;}
    if(p.action==='receipt'){if(v.forRequestId!==p.forRequestId||!Object.prototype.hasOwnProperty.call(v,'status')||Object.prototype.hasOwnProperty.call(v,'contentRevision')&&v.contentRevision>r.contentRevision)fail('font receipt correlation mismatch');return;}
    if(v.atRevision!==p.atRevision||r.contentRevision!==p.atRevision||v.documentSnapshotId!==p.documentSnapshotId)fail('font read snapshot mismatch');
    if(p.action==='inspect'&&(!v.font||v.font.assetId!==p.assetId)||p.action==='list'&&(!v.rows||v.offset!==p.offset)||p.action==='read'&&(v.assetId!==p.assetId||v.offset!==p.offset||v.returnedLength>p.length))fail('font read selector mismatch');
  }
  function paths(op){if(!admitted(op))return null;return new Set(['payload.assetId','payload.source.family','result.assetId','result.font.assetId','result.font.name','result.rows.0.assetId','result.rows.0.name']);}
  return Object.freeze({admitted:admitted,requiresRevision:op=>op===IMPORT,payload:payload,result:result,correlate:correlate,paths:paths});
}));
