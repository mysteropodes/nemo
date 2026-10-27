/* #2361 bitmap-brush Draw (draw-bridge.js onStrokeCommitted -> bitmap-brush.js applyBitmapBrushTexture @1065e9d8) as one
 * native command: the stroke is baked on a detached path with the baseline's own bake, the PNG is staged as a scoped
 * file, and command.document.object.brush.bitmap.stroke lands the camouflaged anchor, its companion raster above it and
 * the PNG in one revision and one undo step. No Raster is inserted and no Paper document is written. */
(function(root,factory){var node=typeof module==='object'&&module.exports;
  // Resolved at commit, so a page or fixture that never draws with the bitmap brush need not load the command modules.
  var api=factory(node?function(){return {Command:require('./native-object-vector-command.js'),Stroke:require('./native-object-vector-stroke.js'),
    Contract:require('./native-bitmap-brush-resources-contract.js'),VectorContract:require('./native-object-vector-contract.js')};}
    :function(){return {Command:root.NemoNativeObjectVectorCommand,Stroke:root.NemoNativeObjectVectorStroke,Contract:root.NemoNativeBitmapBrushResourcesContract,
      VectorContract:root.NemoNativeObjectVectorContract};});
  if(node)module.exports=api;else root.NemoNativeDrawBitmap=api;
}(typeof globalThis==='object'?globalThis:this,function(deps){
  'use strict';
  var pinKeys=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  function same(a,b){return !!a&&!!b&&pinKeys.every(function(k){return a[k]===b[k];});}
  function fail(message){throw new Error(message);}
  function unavailable(reason){return Object.freeze({status:'unavailable',reason:reason});}
  // The baseline bakes only a plain Draw stroke with Stroke on (draw-bridge.js: not the pressure ribbon, the Fill Brush or
  // Shadow mode). Bitmap-brush settings are read at the press, with the rest of the native Draw style.
  function style(root,state,out){
    if(!(state.tool==='draw'&&state.bitmapBrushOn&&state.strokeEnabled&&!state.vectorBrush&&!state.shadowMode))return out;
    if(!root.SMBitmapBrush||typeof root.SMBitmapBrush.bake!=='function')fail('The bitmap brush is unavailable in native Drawing.');
    // Symmetry copies, Trim, Mask and Labs would each need the companion to follow the anchor: not native yet.
    if(state.symmetryEnabled||state.trimStrokeEnds||state.maskMode||
        root.SMLabs&&root.SMLabs.list().some(function(p){return p.on;}))
      fail('This bitmap brush combination is unavailable in native Drawing.');
    out.bitmap={tip:state.bitmapTip||'soft',spacing:state.bitmapSpacing!=null?state.bitmapSpacing:15,
      scatter:state.bitmapScatter!=null?state.bitmapScatter:20,opacity:(state.bitmapOpacity!=null?state.bitmapOpacity:100)/100,
      pressure:!!state.bitmapPressure};
    return out;
  }
  function hex8(color){var h=color.slice(1),full=h.length===3?h.replace(/./g,function(c){return c+c;}):h;
    return '#'+(full.length===6?full+'ff':full).toLowerCase();}
  // applyBitmapBrushTexture's spec, pressure profile and bake, on a path that is never inserted.
  function bake(root,t,dict){
    var s=t.style,b=s.bitmap,spec={tip:b.tip,size:s.brushSize||40,spacing:b.spacing,scatter:b.scatter,opacity:b.opacity,
      color:hex8(dict.strokeColor),pressure:b.pressure,seed:Math.floor(Math.random()*0xffffffff)};
    var profile=spec.pressure&&t.samples&&t.samples.length?t.samples.map(function(x){
      return [x[0],x[1],Math.max(0.2,Math.min(2.5,x[2]/(s.brushSize||1)))];}):null;
    var path=new root.Path({insert:false});
    try{
      dict.segments.forEach(function(g){path.add(new root.Segment(new root.Point(g.point[0],g.point[1]),
        new root.Point(g.handleIn[0],g.handleIn[1]),new root.Point(g.handleOut[0],g.handleOut[1])));});
      path.closed=!!dict.closed;
      if(profile)path.data.bitmapPressureProfile=profile;
      return {spec:spec,profile:profile,baked:root.SMBitmapBrush.bake(path,spec)};
    }finally{path.remove();}
  }
  // The anchor camouflage: with a fill only the stroke goes (remembered); without one the whole path goes invisible.
  function anchor(record,dict,made){
    var out=JSON.parse(JSON.stringify(record)),ext={bitmapBrushSpec:made.spec,
      brushGroupId:'bmb'+Date.now().toString(36)+'_'+Math.floor(Math.random()*1e6)};
    if(dict.fillColor){ext.preTextureStroke=made.spec.color;out.vector.stroke=null;out.vector.hasRealStroke=false;}
    else{ext.preTextureOpacity=out.vector.opacity;out.vector.opacity=0;}
    if(made.profile)ext.bitmapPressureProfile=made.profile;
    out.vectorExt=Object.assign({},out.vectorExt,{bitmapBrush:ext});
    return out;
  }
  function commit(root,session,t,dict,statusOf){
    var d=deps(),Command=d.Command,Stroke=d.Stroke,Contract=d.Contract,VectorContract=d.VectorContract;
    // The scoped PNG stage is the import's own page module, read like native-import-desktop reads it.
    var Stage=root.NemoNativeImportPngStage,tauri=root.__TAURI__;
    if(!Command||!Stroke||!Contract||!VectorContract||!Stage||!tauri||!tauri.fs||!tauri.path||!root.crypto||typeof root.crypto.randomUUID!=='function')
      return Promise.resolve(unavailable('bitmap_stage_unavailable'));
    if(root.state.maskMode)return Promise.resolve(unavailable('bitmap_mask_unavailable'));
    var map=Stroke.toRecord(dict,{layerUid:t.layerUid,frame:t.frame,strokeId:''});
    if(!map.ok)return Promise.resolve(Object.freeze({status:map.code==='unavailable'?'unavailable':'unmapped',code:map.code,reason:map.reason}));
    var made=bake(root,t,dict),record=anchor(map.record,dict,made),b=made.baked;
    // The record rules the owner's codec repeats (bitmapBrush aside, which the stroke command owns), checked before any file.
    var others=Object.assign({},record.vectorExt),plainRecord=Object.assign({},record);delete others.bitmapBrush;
    if(Object.keys(others).length)plainRecord.vectorExt=others;else delete plainRecord.vectorExt;
    try{VectorContract.record(plainRecord,true);}catch(bad){return Promise.resolve(Object.freeze({status:'invalid',reason:bad.message}));}
    var companion={x:b.minX+b.w/2,y:b.minY+b.h/2,width:b.w,height:b.h,opacity:1};
    var contract=Object.freeze({payload:Contract.payload,result:Contract.result,correlate:Contract.correlate,bytes:VectorContract.bytes,
      requestByteLimit:function(){return Contract.STROKE_REQUEST_BYTES;}});
    var pin=session.info();
    if(!same(pin,t.pin))return Promise.resolve(Object.freeze({status:'stale',reason:'origin_changed'}));
    // The stage's core: the marker runs synchronously as the request is built inside the resource action, so the files
    // stay retained whenever the owner may have read them.
    var core={supportsInvocationMarker:true,submit:function(ctx,sealed,mark){
      return Command.create(session,'bitmap',contract).execute(Contract.STROKE,function(now){
        if(!same(now,t.pin))throw Command.fault('stale_origin','The presented native revision is not the origin the stroke was drawn on',{reason:'origin_changed'});
        mark();
        var body={documentSnapshotId:now.documentSnapshotId,lifecycleGeneration:now.lifecycleGeneration,record:record,companion:companion,
          source:sealed.sources[0].source};
        // drawMode 'behind' (read at the press): the anchor under the layer's artwork, its companion directly above it.
        if(t.behind)body.placement='behind';
        return body;
      }).then(function(receipt){return {status:'imported',applied:receipt.applied};},function(error){return {status:'failed',error:statusOf(error)};});
    }};
    var attempt=Stage.create({core:core,fs:tauri.fs,path:tauri.path,uuid:function(){return root.crypto.randomUUID();}})
      .createAttempt({before:pin},{current:function(){return same(session.info(),t.pin);}});
    return attempt.stageCanvas('bitmap-stroke','bitmap-brush.png',b.canvas).then(function(source){
      return attempt.submit({sources:[source]});
    },function(error){return attempt.abort().then(function(){return unavailable(error&&error.code||'bitmap_stage_failed');});})
      .then(function(out){
        if(out.status==='imported')return Object.freeze({status:out.applied?'applied':'noop'});
        if(out.status==='failed')return out.error;
        return out.status==='unavailable'||out.status==='stale'?out:unavailable(out.reason||out.status);
      });
  }
  return Object.freeze({style:style,commit:commit});
}));
