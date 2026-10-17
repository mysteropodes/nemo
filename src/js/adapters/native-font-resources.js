/* PRIVATE G41 proposal. Common SDK/font registration and installed acceptance pending.
 * Immutable native font resources; no glyph, layout or document-state writer. */
(function(root,factory){
  var contract=typeof module==='object'&&module.exports?require('./native-font-contract.js'):root.NemoNativeFontContract;
  var api=factory(contract);
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeFontResources=api;
}(typeof globalThis==='object'?globalThis:this,function(contract){
  'use strict';
  var IMPORT='command.document.object.asset.font.import',QUERY='query.document.object.asset.font';
  function sameOwner(a,b){return a&&b&&['instanceId','documentId','lifecycleGeneration'].every(function(k){return a[k]===b[k];});}
  function samePin(a,b){return sameOwner(a,b)&&a.contentRevision===b.contentRevision&&a.documentSnapshotId===b.documentSnapshotId;}
  function canonical(value){if(Array.isArray(value))return value.map(canonical);if(value&&typeof value==='object'){
    var result={};Object.keys(value).sort().forEach(function(k){result[k]=canonical(value[k]);});return result;}return value;}
  function fingerprintInput(r){return JSON.stringify({apiVersion:r.apiVersion,requestId:r.requestId,instanceId:r.instanceId,
    documentId:r.documentId,expectedRevision:r.expectedRevision,operation:r.operation,payload:canonical(r.payload)});}
  function indeterminate(){var e=new Error('Native font import outcome is indeterminate; receipt unavailable');e.code='font_import_indeterminate';return e;}
  function create(options){
    var session=options.session,owner=null,cache=new Map(),disposed=false,uncertain=false;
    var digest=options.digest||async function(bytes){var out=await globalThis.crypto.subtle.digest('SHA-256',bytes);
      return Array.from(new Uint8Array(out),function(b){return b.toString(16).padStart(2,'0');}).join('');};
    function current(){var p=session.info();if(disposed||!session.isActive()||!p){cache.clear();throw new Error('Native font owner unavailable');}
      if(!sameOwner(owner,p)){cache.clear();uncertain=false;owner=Object.assign({},p);}return p;}
    function guard(pin){if(!samePin(current(),pin))throw new Error('Native font reply belongs to a stale document snapshot');}
    function envelope(pin,op,payload){return {apiVersion:2,requestId:session.requestId('font'),instanceId:pin.instanceId,
      documentId:pin.documentId,operation:op,payload:payload};}
    async function query(ctx,pin,payload){guard(pin);var request=envelope(pin,QUERY,payload),response=await ctx.dispatch(request);guard(pin);
      if(!response.ok)throw new Error('Native font read refused: '+response.error.code);return response.result;}
    async function pinned(ctx,pin,action,extra){return query(ctx,pin,Object.assign({action:action,atRevision:pin.contentRevision,
      documentSnapshotId:pin.documentSnapshotId},extra));}
    async function read(ctx,pin,assetId,expected,signal){
      function selection(){if(signal&&signal.aborted)throw new Error('Native font selection cancelled');}selection();
      guard(pin);var existing=cache.get(assetId);
      var row=(await pinned(ctx,pin,'inspect',{assetId:assetId})).font;
      if(row.assetId!==assetId||row.available!==true||row.sourceKind!=='embedded')throw new Error('Native font is unavailable');
      if(expected&&['assetId','encodedBytes','encodedSha256','mediaType'].some(function(k){return expected[k]!==row[k];}))
        throw new Error('Native imported font metadata mismatch');
      // Revalidate native membership at the current pin before reusing parsed bytes.
      if(existing&&existing.encodedBytes===row.encodedBytes&&existing.encodedSha256===row.encodedSha256)return existing.font;
      var bytes=new Uint8Array(row.encodedBytes),offset=0;
      while(offset<bytes.length){
        var length=Math.min(1536,bytes.length-offset),chunk=await pinned(ctx,pin,'read',{assetId:assetId,offset:offset,length:length});
        if(chunk.assetId!==assetId||chunk.offset!==offset||chunk.returnedLength<1||chunk.returnedLength>length||
          chunk.totalLength!==bytes.length||chunk.encodedSha256!==row.encodedSha256)throw new Error('Native font chunk mismatch');
        var binary=atob(chunk.base64);if(binary.length!==chunk.returnedLength)throw new Error('Native font chunk length mismatch');
        for(var i=0;i<binary.length;i++)bytes[offset+i]=binary.charCodeAt(i);offset+=binary.length;
      }
      if(await digest(bytes)!==row.encodedSha256)throw new Error('Native font digest mismatch');
      guard(pin);selection();var font=await options.parse(bytes.buffer);guard(pin);selection();
      if(!font)throw new Error('font parser returned no font');
      cache.set(assetId,{encodedBytes:row.encodedBytes,encodedSha256:row.encodedSha256,font:font});return font;
    }
    function run(action){try{current();return session.runFontResource(action);}catch(error){return Promise.reject(error);}}
    function loadAsset(assetId){return run(async function(ctx){var pin=Object.assign({},current());return read(ctx,pin,assetId);});}
    function discover(){return run(async function(ctx){var pin=Object.assign({},current()),rows=[],offset=0;
      do{var page=await pinned(ctx,pin,'list',{offset:offset,limit:1});
        if(page.offset!==offset||page.rows.length>1||page.totalFontRecords<offset+page.rows.length||
          (page.nextOffset!==null&&(page.nextOffset!==offset+1||page.rows.length!==1||page.nextOffset>=page.totalFontRecords))||
          (page.nextOffset===null&&offset+page.rows.length!==page.totalFontRecords))throw new Error('Native font catalog mismatch');
        rows.push.apply(rows,page.rows);offset=page.nextOffset;
      }while(offset!==null);guard(pin);return rows;});}
    function importSource(source,signal){source=Object.assign({},source);return run(async function(ctx){
      var pin=Object.assign({},current());if(uncertain)throw indeterminate();if(signal&&signal.aborted)throw new Error('Native font selection cancelled');
      var request=envelope(pin,IMPORT,{command:'object.asset.font.import',documentSnapshotId:pin.documentSnapshotId,
        lifecycleGeneration:pin.lifecycleGeneration,source:source});request.expectedRevision=pin.contentRevision;
      if(!contract)throw new Error('Common native font contract unavailable');contract.payload(IMPORT,request.payload);
      async function unknown(){uncertain=true;await ctx.reconcile();throw indeterminate();}
      var result,response,ambiguous=false;
      try{response=await ctx.dispatch(request);}catch(error){ambiguous=true;}
      if(response&&response.ok===false&&response.error&&response.error.details&&response.error.details.disposition==='indeterminate')ambiguous=true;
      if(ambiguous){
        // Never replay or create a second import after an ambiguous dispatch.
        guard(pin);var receipt;
        try{receipt=await query(ctx,pin,{action:'receipt',forRequestId:request.requestId});}catch(_){return unknown();}
        if(receipt.status==='not_found')return unknown();
        if(receipt.forRequestId!==request.requestId||receipt.requestFingerprintSha256!==await digest(new TextEncoder().encode(fingerprintInput(request))))
          return unknown();
        if(receipt.status==='refused'){await ctx.reconcile(receipt.contentRevision);throw new Error('Native font import refused: '+receipt.code);}
        if(receipt.status!=='committed')return unknown();
        result=receipt;response={contentRevision:receipt.contentRevision};
      }
      if(!sameOwner(current(),pin))throw new Error('Native font owner replaced');
      if(!result){if(!response.ok){await ctx.reconcile(response.contentRevision);throw new Error('Native font import refused: '+response.error.code);}result=response.result;}
      await ctx.reconcile(response.contentRevision);var after=Object.assign({},current());
      if(!sameOwner(after,pin)||after.contentRevision!==response.contentRevision)throw new Error('Native font import revision changed');
      if(signal&&signal.aborted)throw new Error('Native font selection cancelled');
      return {assetId:result.assetId,font:await read(ctx,after,result.assetId,result,signal)};
    });}
    return Object.freeze({importSource:importSource,loadAsset:loadAsset,discover:discover,
      ownerKey:function(){var p=current();return JSON.stringify([p.instanceId,p.documentId,p.lifecycleGeneration]);},
      dispose:function(){disposed=true;cache.clear();owner=null;}});
  }
  function desktop(root,parse){var session=root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root);
    if(!session)throw new Error('Native font session unavailable');return create({session:session,parse:parse});}
  return Object.freeze({create:create,desktop:desktop,fingerprintInput:fingerprintInput});
}));
