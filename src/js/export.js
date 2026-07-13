// ---- EXPORT ENGINE ----
// Sequence/video exports (PNG, TIFF, GIF, MP4, ProRes) require the Tauri
// runtime (fs + shell sidecar to a bundled ffmpeg). SVG sequence and Lottie
// JSON are pure-JS and also work in the plain-browser dev preview.
var _exportLayer = null;
function exportTauriAvailable(){return typeof window.__TAURI__!=='undefined';}

function exportEnsureLayer(){
  if(!_exportLayer){_exportLayer=new Layer({name:'__export__'});_exportLayer.visible=false;}
  return _exportLayer;
}
// Builds the frame's full visual content (bg + all visible layers, in
// z-order) into the hidden export layer. Synchronous, no repaint occurs
// before callers consume it, so it never flashes on screen.
function exportBuildFrame(frameIdx,alpha){
  var L=exportEnsureLayer();L.removeChildren();
  var prev=project.activeLayer;L.activate();
  if(!alpha){
    new Path.Rectangle({point:[0,0],size:[state.canvasW,state.canvasH],fillColor:state.canvasBg,insert:true});
  }else{
    // A fully-transparent rect (alpha:0), not "no rect at all" — an empty
    // Layer has no bounds for rasterize()/exportSVG() to work from, so a
    // frame with zero visible content on an alpha export (a blank keyframe
    // in the middle of an otherwise-drawn timeline is a completely normal
    // thing to hit exporting a whole range) produced an invalid zero-size
    // raster (toDataURL silently returning the empty "data:," placeholder,
    // i.e. a corrupt frame in the sequence) instead of a valid fully-
    // transparent PNG at the real canvas size. Zero alpha still paints
    // nothing but keeps the layer's bounds pinned to the canvas rect.
    new Path.Rectangle({point:[0,0],size:[state.canvasW,state.canvasH],fillColor:new Color(0,0,0,0),insert:true});
  }
  for(var li=0;li<state.layers.length;li++){
    var ld=state.layers[li];if(!ld.visible)continue;
    var strokes=getEffectiveStrokes(li,frameIdx);
    // Motion mode (motion.js): unlike buildSceneJson (engine-bridge.js),
    // exportBuildFrame merges every original layer's strokes straight into
    // the ONE shared throwaway `L`, so there's no per-layer Paper object left
    // to transform after the fact. Fix: read each stroke's transform BEFORE
    // it's merged in (motionMat/motionPivot computed once per li, same as
    // the live path), and apply it to the Path `desP` just created — safe
    // here because `p` lives on `L`, a disposable export-only layer, never
    // userLayers[li] itself (see CLAUDE.md's family-of-bug-#1).
    var motionMat=(window.SMMotion&&strokes.length)?SMMotion.layerMotionAt(li,frameIdx):null;
    var motionPivot=motionMat?userLayers[li].bounds.center:null;
    strokes.forEach(function(sd){
      var p=desP(sd,L,sd.opacity!==undefined?sd.opacity:1);
      if(motionMat){
        p.scale(motionMat.sx,motionMat.sy,motionPivot);
        p.rotate(motionMat.rot,motionPivot);
        p.translate(motionMat.dx,motionMat.dy);
        p.opacity=p.opacity*motionMat.op;
        if(p.strokeWidth)p.strokeWidth*=(Math.abs(motionMat.sx)+Math.abs(motionMat.sy))/2;
      }
    });
  }
  // Caméra (v18) : bake le zoom/pan de la frame dans le rendu exporté —
  // le rect caméra interpolé remplit exactement le canvas de sortie.
  if(window.SMCamera)SMCamera.applyToExportLayer(L,frameIdx);
  prev.activate();
  return L;
}
function exportFrameDataURL(frameIdx,scale,alpha){
  var L=exportBuildFrame(frameIdx,alpha);
  // Paper.js's rasterize() skips invisible items entirely (same rule as
  // on-screen rendering), so the hidden export layer must be flipped
  // visible for the actual rasterize call — synchronously flipped back off
  // immediately after, before any repaint or await can happen, so it never
  // flashes on screen (matches exportBuildFrame's "no repaint in between"
  // guarantee).
  L.visible=true;
  var raster=L.rasterize({resolution:72*(scale||1),insert:false});
  L.visible=false;
  var url=raster.canvas.toDataURL('image/png');
  raster.remove();
  return url;
}
function exportFrameSVGString(frameIdx){
  var L=exportBuildFrame(frameIdx);
  // Same rule as rasterize(): exportSVG() skips invisible items too, so the
  // hidden export layer must be flipped visible for the call itself (see
  // exportFrameDataURL for why this never flashes on screen).
  L.visible=true;
  var inner=L.exportSVG({asString:true,bounds:new Rectangle(0,0,state.canvasW,state.canvasH)});
  L.visible=false;
  return '<?xml version="1.0" encoding="UTF-8"?>\n'+
    '<svg xmlns="http://www.w3.org/2000/svg" width="'+state.canvasW+'" height="'+state.canvasH+'" '+
    'viewBox="0 0 '+state.canvasW+' '+state.canvasH+'">\n'+inner+'\n</svg>';
}
function exportDataURLToBytes(dataURL){
  var b64=dataURL.split(',')[1];
  var bin=atob(b64);var bytes=new Uint8Array(bin.length);
  for(var i=0;i<bin.length;i++)bytes[i]=bin.charCodeAt(i);
  return bytes;
}

// ---- frame range resolution ----
function exportFrameRange(opts){
  var start=(opts&&opts.start!==undefined)?opts.start:state.waIn;
  var end=(opts&&opts.end!==undefined)?opts.end:state.waOut;
  return{start:start,end:end};
}

// ---- Tauri fs/dialog/shell helpers ----
function exportTauri(){return window.__TAURI__;}
async function exportPickDir(title){
  var t=exportTauri();
  return await t.dialog.open({directory:true,title:title||'Choisir un dossier'});
}
async function exportPickSaveFile(title,defaultName,filters){
  var t=exportTauri();
  return await t.dialog.save({title:title||'Enregistrer',defaultPath:defaultName,filters:filters});
}
async function exportWriteBytes(path,bytes){
  var t=exportTauri();
  await t.fs.writeFile(path,bytes);
}
async function exportWriteText(path,text){
  var t=exportTauri();
  await t.fs.writeTextFile(path,text);
}
async function exportMkdir(path){
  var t=exportTauri();
  try{await t.fs.mkdir(path,{recursive:true});}catch(e){}
}
async function exportRemoveDir(path){
  var t=exportTauri();
  try{await t.fs.remove(path,{recursive:true});}catch(e){}
}
function exportTempDirPath(){
  return (window.__TAURI__.path?window.__TAURI__.path.tempDir():null);
}
function pad4(n){var s=''+n;while(s.length<4)s='0'+s;return s;}
async function exportMP4ToPath(outPath,opts){
  var r=exportFrameRange(opts);var scale=(opts&&opts.scale)||1;var fps=(opts&&opts.fps)||state.fps;
  var tmp=exportTempDirPath?await exportTempDirPath():null;
  var workDir=(tmp||outPath.replace(/[^/\\]+$/,''))+'sm-export-'+Date.now();
  await exportMkdir(workDir);
  await exportRenderPNGsToDir(workDir,r.start,r.end,scale,opts&&opts.onProgress);
  await exportRunFfmpeg(['-y','-framerate',String(fps),'-i',workDir+'/frame_%04d.png','-c:v','libx264','-pix_fmt','yuv420p','-crf','18',outPath],opts&&opts.onFfmpeg);
  await exportRemoveDir(workDir);
  return{ok:true,path:outPath};
}

// ---- ffmpeg sidecar ----
async function exportRunFfmpeg(args,onProgress){
  var t=exportTauri();
  var unlisten=null;
  if(onProgress){unlisten=await t.event.listen('ffmpeg-progress',function(e){onProgress(e.payload);});}
  try{
    await t.core.invoke('run_ffmpeg',{args:args});
  }finally{
    if(unlisten)unlisten();
  }
}

// ---- PNG sequence rendering to a working directory (shared by raster exports) ----
async function exportRenderPNGsToDir(dir,start,end,scale,onProgress,alpha){
  for(var f=start,i=1;f<=end;f++,i++){
    var url=exportFrameDataURL(f,scale,alpha);
    var bytes=exportDataURLToBytes(url);
    await exportWriteBytes(dir+'/frame_'+pad4(i)+'.png',bytes);
    if(onProgress)onProgress(i,end-start+1);
  }
}

// ---- LOTTIE / BODYMOVIN CONVERTER ----
// Frame-by-frame "baked" export: rather than trying to re-derive Lottie's
// own bezier-shape morphing (which would require the same fragile
// stroke-to-stroke matching across arbitrary keyframe spans that the
// in-app tween engine already does), each already-resolved per-frame pose
// (post generateTweens) is emitted as its own keyframe with linear timing,
// so any Lottie player reproduces exactly what Nemo shows. A new
// set of shape layers starts wherever a layer's stroke count changes
// (e.g. a redrawn keyframe) since Lottie path keyframes require a stable
// vertex/shape count within one animated property.
function lottieHexToRGBA(hex,opacity){
  if(!hex)return[1,1,1,1];
  var h=hex.replace('#','');
  if(h.length===3)h=h[0]+h[0]+h[1]+h[1]+h[2]+h[2];
  var r=parseInt(h.substr(0,2),16)/255,g=parseInt(h.substr(2,2),16)/255,b=parseInt(h.substr(4,2),16)/255;
  return[r,g,b,opacity!==undefined?opacity:1];
}
// bm (blend mode) codes, in the exact order lottie-web/dotLottie expect —
// same list as the app's own BlendMode type (see CLAUDE.md's renderer
// types), 'srcOver'/anything unrecognized falls back to 0 (Normal) rather
// than emitting a bogus code a player would silently ignore anyway.
var LOTTIE_BM_MAP={multiply:1,screen:2,overlay:3,darken:4,lighten:5,colorDodge:6,colorBurn:7,hardLight:8,softLight:9,difference:10,exclusion:11,hue:12,saturation:13,color:14,luminosity:15};
function lottieBmCode(blendMode){return LOTTIE_BM_MAP[blendMode]||0;}
// Camera bake: Lottie has no concept of a separate camera layer (3D layers
// aren't supported by lottie-web) — reproduce cameraAtFrame's pan/zoom/roll
// by transforming each shape's own vertex/handle coordinates per frame,
// same math as SMCamera.applyToExportLayer (camera.js) but applied to our
// plain [x,y] arrays instead of mutating a live Paper Layer. Handles are
// RELATIVE offsets from their point (Paper/Lottie's shared bezier
// convention) — rotation+scale apply to them, translation must not.
function lottieCamMatrix(cam){
  var rot=-(cam.rot||0)*Math.PI/180;
  return{cx:cam.x,cy:cam.y,cos:Math.cos(rot),sin:Math.sin(rot),s:state.canvasW/cam.w,tx:state.canvasW/2-cam.x,ty:state.canvasH/2-cam.y};
}
function lottieCamPoint(p,cm){
  var dx=p[0]-cm.cx,dy=p[1]-cm.cy;
  var rx=(dx*cm.cos-dy*cm.sin)*cm.s,ry=(dx*cm.sin+dy*cm.cos)*cm.s;
  return[cm.cx+rx+cm.tx,cm.cy+ry+cm.ty];
}
function lottieCamVector(v,cm){
  return[(v[0]*cm.cos-v[1]*cm.sin)*cm.s,(v[0]*cm.sin+v[1]*cm.cos)*cm.s];
}
function lottieShapeValue(sd,camMatrix){
  // sd.closed reflects the Paper.js path's own .closed (serP, app.js) — this
  // was hardcoded to false, so every closed shape (rectangles, ellipses, any
  // filled region) exported as an OPEN path. lottie-web/most players still
  // attempt to fill an open path, but the strict validators several online
  // previewers use (e.g. lottielab) reject/blank a shape whose last vertex
  // doesn't explicitly close back to the first, which is exactly why nothing
  // rendered there.
  if(!camMatrix){
    return{
      i:sd.segments.map(function(s){return s.handleIn;}),
      o:sd.segments.map(function(s){return s.handleOut;}),
      v:sd.segments.map(function(s){return s.point;}),
      c:!!sd.closed
    };
  }
  return{
    i:sd.segments.map(function(s){return lottieCamVector(s.handleIn,camMatrix);}),
    o:sd.segments.map(function(s){return lottieCamVector(s.handleOut,camMatrix);}),
    v:sd.segments.map(function(s){return lottieCamPoint(s.point,camMatrix);}),
    c:!!sd.closed
  };
}
function lottiePathLayer(name,runStrokes,runStart,runEnd,fps,camByFrame,bm){
  var first=runStrokes[runStart];
  // sd.strokeColor defaults to '#ffffff' as a legacy fallback even when the
  // path never had a real stroke (serP, app.js) — CLAUDE.md's documented
  // hasRealStroke field exists precisely so consumers can tell the two
  // apart. Reading strokeColor's truthiness directly (as this did) gave
  // every fill-only shape a phantom white 1px stroke in the export.
  var isStroke=!!first.hasRealStroke;
  var shapeItems=[
    {ty:'sh',ks:{a:1,k:[]}}
  ];
  var kfs=shapeItems[0].ks.k;
  for(var f=runStart;f<=runEnd;f++){
    var sd=runStrokes[f];
    var cm=camByFrame?camByFrame[f]:null;
    kfs.push({t:f,s:[lottieShapeValue(sd,cm)],i:{x:[1],y:[1]},o:{x:[0],y:[0]}});
  }
  if(isStroke){
    shapeItems.push({ty:'st',c:{a:0,k:lottieHexToRGBA(first.strokeColor,1)},o:{a:0,k:(first.opacity!==undefined?first.opacity:1)*100},w:{a:0,k:first.strokeWidth||2},lc:first.strokeCap==='round'?2:(first.strokeCap==='square'?3:1),lj:first.strokeJoin==='round'?2:(first.strokeJoin==='bevel'?3:1)});
  }
  if(first.fillColor){
    shapeItems.push({ty:'fl',c:{a:0,k:lottieHexToRGBA(first.fillColor,1)},o:{a:0,k:(first.opacity!==undefined?first.opacity:1)*100}});
  }
  shapeItems.push({ty:'tr',p:{a:0,k:[0,0]},a:{a:0,k:[0,0]},s:{a:0,k:[100,100]},r:{a:0,k:0},o:{a:0,k:100}});
  return{
    ddd:0,ty:4,nm:name,sr:1,
    ks:{o:{a:0,k:100},r:{a:0,k:0},p:{a:0,k:[0,0,0]},a:{a:0,k:[0,0,0]},s:{a:0,k:[100,100,100]}},
    ao:0,
    shapes:[{ty:'gr',it:shapeItems}],
    ip:runStart,op:runEnd+1,st:0,bm:bm||0
  };
}
function lottieBuild(start,end){
  var fps=state.fps;
  var layers=[];
  var ind=1;
  var bg={ddd:0,ty:1,nm:'Background',sr:1,sc:state.canvasBg,sw:state.canvasW,sh:state.canvasH,
    ks:{o:{a:0,k:100},r:{a:0,k:0},p:{a:0,k:[0,0,0]},a:{a:0,k:[0,0,0]},s:{a:0,k:[100,100,100]}},
    ao:0,ip:start,op:end+1,st:0,bm:0,ind:ind++};
  layers.push(bg);

  // Camera keyframes bake into every shape's own vertex coordinates (see
  // lottieCamMatrix) since Lottie players have no 3D/camera-layer concept —
  // computed once per exported frame, reused across every layer/slot below.
  var camActive=!!(window.SMCamera&&state.cameraLayerOn&&state.cameraKeys.length);
  var camByFrame=null;
  if(camActive){
    camByFrame={};
    for(var cf=start;cf<=end;cf++){
      var cam=SMCamera.cameraAtFrame(cf);
      camByFrame[cf]=cam?lottieCamMatrix(cam):null;
    }
  }

  for(var li=state.layers.length-1;li>=0;li--){
    var ld=state.layers[li];if(!ld.visible)continue;
    var bm=lottieBmCode(ld.blendMode);
    // per-frame strokes array for this layer across the export range —
    // drops fully-invisible brush-texture anchors (opacity:0 by convention,
    // see applyBrushTexture's own comment in tools.js): they render nothing
    // in any player (o:0 already round-trips correctly) but a textured
    // stroke can carry dozens to hundreds of dab companions, so skipping
    // the anchor keeps file size and shape-layer count from ballooning for
    // zero visual difference. Dab companions themselves are NOT skipped —
    // they ARE the visible texture.
    var framesStrokes=[];
    for(var f=start;f<=end;f++)framesStrokes[f]=getEffectiveStrokes(li,f).filter(function(sd){return sd.opacity!==0;});

    // figure out the max stroke-slot count and, for each slot, the
    // contiguous frame runs where that slot exists (count stable)
    var maxSlots=0;
    for(f=start;f<=end;f++)maxSlots=Math.max(maxSlots,framesStrokes[f].length);

    for(var slot=0;slot<maxSlots;slot++){
      var runStart=null;
      for(f=start;f<=end+1;f++){
        var has=f<=end&&framesStrokes[f]&&framesStrokes[f][slot];
        if(has&&runStart===null)runStart=f;
        if((!has||f===end+1)&&runStart!==null){
          var runEnd=f-1;
          var runStrokes={};
          for(var rf=runStart;rf<=runEnd;rf++)runStrokes[rf]=framesStrokes[rf][slot];
          var layer=lottiePathLayer(ld.name+' / shape'+slot,runStrokes,runStart,runEnd,fps,camByFrame,bm);
          layer.ind=ind++;
          layers.push(layer);
          runStart=null;
        }
      }
    }
  }

  // Lottie's layers array is FIRST-ENTRY-ON-TOP (same convention as an AE
  // layer panel) — the opposite of the back-to-front order this function
  // builds in (Background pushed first meaning "furthest back", each user
  // layer/stroke-slot appended after in back-to-front z-order). Left
  // un-reversed, the opaque Background solid — being first in the array —
  // painted OVER every single shape layer, so nothing but a blank canvas
  // ever showed in any real Lottie player (confirmed: lottielab.com showed
  // literally nothing until this reverse was added, despite every layer's
  // own geometry/color being completely correct in isolation).
  layers.reverse();

  return{
    v:'5.9.0',fr:fps,ip:start,op:end+1,
    w:state.canvasW,h:state.canvasH,
    nm:'Nemo Export',ddd:0,assets:[],layers:layers
  };
}

// ---- public exporters ----
window.SMExport={
  isAvailable:exportTauriAvailable,
  previewFrame:exportFrameDataURL,

  exportSVGSequence:async function(opts){
    var r=exportFrameRange(opts);
    if(exportTauriAvailable()){
      var dir=await exportPickDir('Dossier de séquence SVG');
      if(!dir)return{cancelled:true};
      for(var f=r.start,i=1;f<=r.end;f++,i++){
        await exportWriteText(dir+'/frame_'+pad4(i)+'.svg',exportFrameSVGString(f));
        if(opts&&opts.onProgress)opts.onProgress(i,r.end-r.start+1);
      }
      return{ok:true,dir:dir};
    }else{
      // browser fallback: download a single representative frame so the
      // feature is still testable outside Tauri
      var svg=exportFrameSVGString(r.start);
      var blob=new Blob([svg],{type:'image/svg+xml'});
      var u=URL.createObjectURL(blob);var a=document.createElement('a');
      a.href=u;a.download='frame_'+pad4(1)+'.svg';a.click();URL.revokeObjectURL(u);
      return{ok:true,browserFallback:true};
    }
  },

  exportPNGSequence:async function(opts){
    if(!exportTauriAvailable())return{ok:false,error:'Disponible uniquement dans l\'app Nemo (pas en preview navigateur).'};
    var r=exportFrameRange(opts);var scale=(opts&&opts.scale)||1;
    var dir=await exportPickDir('Dossier de séquence PNG');
    if(!dir)return{cancelled:true};
    await exportRenderPNGsToDir(dir,r.start,r.end,scale,opts&&opts.onProgress,opts&&opts.alpha);
    return{ok:true,dir:dir};
  },

  exportTIFFSequence:async function(opts){
    if(!exportTauriAvailable())return{ok:false,error:'Disponible uniquement dans l\'app Nemo (pas en preview navigateur).'};
    var r=exportFrameRange(opts);var scale=(opts&&opts.scale)||1;
    var outDir=await exportPickDir('Dossier de séquence TIFF');
    if(!outDir)return{cancelled:true};
    var tmp=exportTempDirPath?await exportTempDirPath():null;
    var workDir=(tmp||outDir)+'/sm-export-'+Date.now();
    await exportMkdir(workDir);
    await exportRenderPNGsToDir(workDir,r.start,r.end,scale,opts&&opts.onProgress);
    await exportRunFfmpeg(['-y','-start_number','1','-i',workDir+'/frame_%04d.png','-start_number','1',outDir+'/frame_%04d.tif'],opts&&opts.onFfmpeg);
    await exportRemoveDir(workDir);
    return{ok:true,dir:outDir};
  },

  exportGIF:async function(opts){
    if(!exportTauriAvailable())return{ok:false,error:'Disponible uniquement dans l\'app Nemo (pas en preview navigateur).'};
    var r=exportFrameRange(opts);var scale=(opts&&opts.scale)||1;var fps=(opts&&opts.fps)||state.fps;
    var outPath=await exportPickSaveFile('Exporter en GIF','animation.gif',[{name:'GIF',extensions:['gif']}]);
    if(!outPath)return{cancelled:true};
    var tmp=exportTempDirPath?await exportTempDirPath():null;
    var workDir=(tmp||outPath.replace(/[^/\\]+$/,''))+'sm-export-'+Date.now();
    await exportMkdir(workDir);
    await exportRenderPNGsToDir(workDir,r.start,r.end,scale,opts&&opts.onProgress);
    var palette=workDir+'/palette.png';
    await exportRunFfmpeg(['-y','-framerate',String(fps),'-i',workDir+'/frame_%04d.png','-vf','palettegen=stats_mode=diff',palette],opts&&opts.onFfmpeg);
    await exportRunFfmpeg(['-y','-framerate',String(fps),'-i',workDir+'/frame_%04d.png','-i',palette,'-lavfi','paletteuse=dither=bayer','-loop','0',outPath],opts&&opts.onFfmpeg);
    await exportRemoveDir(workDir);
    return{ok:true,path:outPath};
  },

  exportMP4:async function(opts){
    if(!exportTauriAvailable())return{ok:false,error:'Disponible uniquement dans l\'app Nemo (pas en preview navigateur).'};
    var outPath=await exportPickSaveFile('Exporter en MP4','animation.mp4',[{name:'MP4',extensions:['mp4']}]);
    if(!outPath)return{cancelled:true};
    return exportMP4ToPath(outPath,opts);
  },
  // Kitsu publish (Phase 4) needs the same H.264 render but writing to a
  // caller-chosen temp path with no save dialog — the publish flow already
  // asked the user to confirm once, a second native file picker mid-publish
  // would be a confusing extra step.
  exportMP4Silent:function(outPath,opts){
    if(!exportTauriAvailable())return Promise.resolve({ok:false,error:'Disponible uniquement dans l\'app Nemo (pas en preview navigateur).'});
    return exportMP4ToPath(outPath,opts);
  },

  exportProRes:async function(opts){
    if(!exportTauriAvailable())return{ok:false,error:'Disponible uniquement dans l\'app Nemo (pas en preview navigateur).'};
    var r=exportFrameRange(opts);var scale=(opts&&opts.scale)||1;var fps=(opts&&opts.fps)||state.fps;
    var alpha=!!(opts&&opts.alpha);
    var outPath=await exportPickSaveFile('Exporter en ProRes','animation.mov',[{name:'QuickTime',extensions:['mov']}]);
    if(!outPath)return{cancelled:true};
    var tmp=exportTempDirPath?await exportTempDirPath():null;
    var workDir=(tmp||outPath.replace(/[^/\\]+$/,''))+'sm-export-'+Date.now();
    await exportMkdir(workDir);
    await exportRenderPNGsToDir(workDir,r.start,r.end,scale,opts&&opts.onProgress,alpha);
    // ProRes 4444 (profile 4) is the alpha-capable variant — regular ProRes
    // (profile 3, "HQ") has no alpha channel at all, same as any other
    // standard video codec, so a real alpha export needs both the profile
    // AND pixel format switched together, not just the pix_fmt.
    var vArgs=alpha
      ?['-c:v','prores_ks','-profile:v','4','-pix_fmt','yuva444p10le']
      :['-c:v','prores_ks','-profile:v','3','-pix_fmt','yuv422p10le'];
    await exportRunFfmpeg(['-y','-framerate',String(fps),'-i',workDir+'/frame_%04d.png'].concat(vArgs).concat([outPath]),opts&&opts.onFfmpeg);
    await exportRemoveDir(workDir);
    return{ok:true,path:outPath};
  },

  exportLottie:async function(opts){
    var r=exportFrameRange(opts);
    var json=lottieBuild(r.start,r.end);
    var text=JSON.stringify(json);
    if(exportTauriAvailable()){
      var outPath=await exportPickSaveFile('Exporter en Lottie JSON','animation.json',[{name:'Lottie JSON',extensions:['json']}]);
      if(!outPath)return{cancelled:true};
      await exportWriteText(outPath,text);
      return{ok:true,path:outPath,json:json};
    }else{
      var blob=new Blob([text],{type:'application/json'});
      var u=URL.createObjectURL(blob);var a=document.createElement('a');
      a.href=u;a.download='animation.json';a.click();URL.revokeObjectURL(u);
      return{ok:true,browserFallback:true,json:json};
    }
  },
};
