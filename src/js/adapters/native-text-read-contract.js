/* Closed immutable text-read DTOs. No text layout, font bytes or writable state. */
(function(root,factory){
  var item=typeof module==='object'&&module.exports?require('./native-object-item-contract.js'):root.NemoNativeObjectItemContract;
  var api=factory(item);if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeTextReadContract=api;
}(typeof globalThis==='object'?globalThis:this,function(Item){
  'use strict';
  var OP='query.document.object.text',MAX_TEXT_BYTES=196608;
  function fail(message){throw new TypeError(message);}
  function plain(v){return !!v&&typeof v==='object'&&!Array.isArray(v);}
  function exact(v,keys){if(!plain(v)||Object.keys(v).length!==keys.length||!keys.every(function(k){return Object.prototype.hasOwnProperty.call(v,k);}))fail('text read fields invalid');}
  function bytes(v){return new TextEncoder().encode(v).length;}
  function uint(v,max){if(!Number.isSafeInteger(v)||v<0||v>max)fail('text read integer invalid');}
  function opaque(v){if(typeof v!=='string'||!v.trim()||v.includes('\0')||bytes(v)>255)fail('text identity invalid');}
  function scope(v){exact(v,['contextId','layerUid','frameScope']);if(v.contextId!=='scene-root')fail('text context unavailable');opaque(v.layerUid);
    exact(v.frameScope,['kind','frame']);if(!['authored','reference'].includes(v.frameScope.kind))fail('text frame scope invalid');uint(v.frameScope.frame,4294967295);}
  function target(v){exact(v,['contextId','layerUid','frameScope','strokeId']);scope({contextId:v.contextId,layerUid:v.layerUid,frameScope:v.frameScope});opaque(v.strokeId);}
  function same(a,b){return JSON.stringify(canonical(a))===JSON.stringify(canonical(b));}
  function canonical(v){if(Array.isArray(v))return v.map(canonical);if(plain(v)){var out={};Object.keys(v).sort().forEach(function(k){out[k]=canonical(v[k]);});return out;}return v;}
  function payload(op,p){
    exact(p,['atRevision','documentSnapshotId','view']);uint(p.atRevision,Number.MAX_SAFE_INTEGER);opaque(p.documentSnapshotId);
    var v=p.view;if(!plain(v))fail('text view invalid');
    if(v.kind==='animators'){exact(v,['kind','layerUid','frame','offset','limit']);opaque(v.layerUid);if(!Number.isFinite(v.frame)||v.frame<0)fail('animator frame invalid');uint(v.offset,4294967295);uint(v.limit,1024);if(v.limit<1)fail('animator chunk limit invalid');}
    else if(v.kind==='targets'){exact(v,['kind','scope','offset','limit']);scope(v.scope);uint(v.offset,4294967295);uint(v.limit,8);if(v.limit<1)fail('text page limit invalid');}
    else if(v.kind==='inspect'){exact(v,['kind','target']);target(v.target);}
    else if(v.kind==='text'){exact(v,['kind','target','offset','limit']);target(v.target);uint(v.offset,MAX_TEXT_BYTES);uint(v.limit,1024);if(v.limit<1)fail('text chunk limit invalid');}
    else fail('text view kind invalid');
  }
  function result(op,r){
    if(!plain(r))fail('text read result invalid');
    var base=['instanceId','documentId','atRevision','documentSnapshotId','kind'];
    ['instanceId','documentId','documentSnapshotId'].forEach(function(k){opaque(r[k]);});uint(r.atRevision,Number.MAX_SAFE_INTEGER);
    if(r.kind==='targets'){
      exact(r,base.concat(['scope','offset','total','items','nextOffset']));scope(r.scope);uint(r.offset,4294967295);uint(r.total,4294967295);
      if(!Array.isArray(r.items)||r.items.length>8||r.offset+r.items.length>r.total)fail('text page invalid');
      var seen=new Set();r.items.forEach(function(row){exact(row,['target']);target(row.target);
        if(!same(r.scope,{contextId:row.target.contextId,layerUid:row.target.layerUid,frameScope:row.target.frameScope})||seen.has(row.target.strokeId))fail('text page identity invalid');seen.add(row.target.strokeId);});
      if(r.nextOffset===null?r.offset+r.items.length!==r.total:!r.items.length||r.nextOffset!==r.offset+r.items.length||r.nextOffset>=r.total)fail('text page progress invalid');
    }else if(r.kind==='inspect'){
      exact(r,base.concat(['target','textBytes','block']));target(r.target);uint(r.textBytes,MAX_TEXT_BYTES);if(!r.textBytes)fail('empty native text');
      var required=['font','size','color','align','anchorTopLeft'],allowed=required.concat(['fixedWidth','fixedHeight','bold','italic','underline','strike','letterSpacing','wordSpacing','lineHeightMult','textCase','rotation']);
      if(!plain(r.block)||!required.every(function(k){return Object.prototype.hasOwnProperty.call(r.block,k);})||!Object.keys(r.block).every(function(k){return allowed.includes(k);}))fail('text metadata fields invalid');
      function positive(v){return typeof v==='number'&&Number.isFinite(v)&&v>0;}
      if(!positive(r.block.size)||['fixedWidth','fixedHeight'].some(function(k){return Object.prototype.hasOwnProperty.call(r.block,k)&&r.block[k]!==null&&!positive(r.block[k]);})||Object.prototype.hasOwnProperty.call(r.block,'lineHeightMult')&&!positive(r.block.lineHeightMult))fail('text metadata positive dimension invalid');
      // Reuse the exact item block validator, with a local non-persistent text sentinel.
      if(!Item)fail('native item contract unavailable');
      Item.payload(Item.ITEM,{command:'object.text.create',target:Object.assign({},r.target,{strokeId:''}),block:Object.assign({},r.block,{text:'validation'})});
    }else if(r.kind==='text'||r.kind==='animators'){
      exact(r,base.concat([r.kind==='text'?'target':'layerUid','textBytes','offset','text','nextOffset']).concat(r.kind==='animators'?['frame']:[]));if(r.kind==='animators'&&(!Number.isFinite(r.frame)||r.frame<0))fail('animator frame invalid');if(r.kind==='text')target(r.target);else opaque(r.layerUid);uint(r.textBytes,r.kind==='text'?MAX_TEXT_BYTES:4294967295);uint(r.offset,r.textBytes);
      if(typeof r.text!=='string'||r.text.includes('\0')||bytes(r.text)>1024||/[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/u.test(r.text))fail('text chunk invalid');
      var end=r.offset+bytes(r.text);if(end>r.textBytes||(r.nextOffset===null?end!==r.textBytes:!r.text.length||r.nextOffset!==end||end>=r.textBytes))fail('text chunk progress invalid');
    }else fail('text result kind invalid');
    if(bytes(JSON.stringify(r))>3072)fail('text result exceeds budget');
  }
  function correlate(q,r){
    // Only these two genuine native refusals disclose the actual owner. The common
    // SDK has already enforced requestId, closed envelope/error syntax and4096 bytes.
    if(q.operation===OP&&r.ok===false&&r.error&&r.error.code==='wrong_instance')return;
    if(r.instanceId!==q.instanceId)fail('text response owner mismatch');
    if(q.operation===OP&&r.ok===false&&r.error&&r.error.code==='wrong_document'){
      if(!r.error.details||r.error.details.requestedDocumentId!==q.documentId)fail('text refusal document mismatch');return;
    }
    if(r.documentId!==q.documentId)fail('text response owner mismatch');if(!r.ok)return;
    var p=q.payload,v=p.view,x=r.result;
    if(r.contentRevision!==p.atRevision||x.instanceId!==q.instanceId||x.documentId!==q.documentId||x.atRevision!==p.atRevision||x.documentSnapshotId!==p.documentSnapshotId||x.kind!==v.kind)fail('text response pin mismatch');
    if(v.kind==='targets'){if(!same(v.scope,x.scope)||x.offset!==v.offset||x.items.length>v.limit)fail('text page selector mismatch');}
    else if(v.kind==='animators'){if(v.layerUid!==x.layerUid||v.frame!==x.frame||x.offset!==v.offset||bytes(x.text)>v.limit)fail('animator chunk selector mismatch');}
    else if(!same(v.target,x.target)||v.kind==='text'&&(x.offset!==v.offset||bytes(x.text)>v.limit))fail('text target selector mismatch');
  }
  var own={admitted:function(op){return op===OP;},requiresRevision:function(){return false;},payload:payload,result:result,correlate:correlate,
    paths:function(){return {has:function(p){return /^payload\.(?:documentSnapshotId|view\.(?:layerUid|scope\.layerUid|target\.(?:layerUid|strokeId)))$/.test(p)||/^result\.(?:documentSnapshotId|layerUid|scope\.layerUid|target\.(?:layerUid|strokeId)|items\.\d+\.target\.(?:layerUid|strokeId)|text|block\.font\.assetId)$/.test(p);}};}};
  function compose(base){if(!base)return Object.freeze(own);return Object.freeze({admitted:function(op){return own.admitted(op)||base.admitted(op);},requiresRevision:function(op){return own.admitted(op)?false:base.requiresRevision(op);},
    payload:function(op,p,e){return own.admitted(op)?payload(op,p):base.payload(op,p,e);},result:function(op,r){return own.admitted(op)?result(op,r):base.result(op,r);},
    correlate:function(q,r){return own.admitted(q.operation)?correlate(q,r):base.correlate(q,r);},paths:function(op){return own.admitted(op)?own.paths(op):base.paths(op);}});}
  return Object.freeze(Object.assign({OP:OP,MAX_TEXT_BYTES:MAX_TEXT_BYTES,compose:compose,same:same},own));
}));
