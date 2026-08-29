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
  var _matteSrcMap=window.matteSourceIndicesInUse?matteSourceIndicesInUse():{};
  for(var li=0;li<state.layers.length;li++){
    var ld=state.layers[li];
    // layerIsEffectivelyVisible (app.js), not a bare .visible check
    // (2026-07 fix): the live canvas and the primary Rust-engine export
    // both already resolve solo through that helper — this plain-Paper.js
    // fallback (used whenever Tauri/the engine isn't available, or at
    // scale>1) checked only .visible, so soloing a layer restricted what
    // you SAW but silently let every other merely-visible layer leak into
    // this export path anyway. Confirmed live.
    if(!layerIsEffectivelyVisible(li))continue;
    // Track matte SOURCE layer: engine.rs's composite_scene consumes a
    // matte's source layer and never paints it as its own visible content.
    // This path had no idea, so the export GAINED an opaque layer the
    // screen never shows (2026-07-26). Skipping it here is only half the
    // story — the matted layer itself still isn't masked on this path,
    // which is why exportNeedsEngine() routes any project using a matte
    // through the engine instead. This keeps the plain-Paper fallback
    // (SVG, scale>1) from being actively WRONG in the meantime.
    // uid-based since 2026-07-31: resolved through the SAME shared helper
    // the layer-row badge uses (matteSourceIndicesInUse, timeline.js) —
    // previously an independent li-1 re-implementation of the adjacency
    // rule, the exact two-readers-drift trap CLAUDE.md §3 documents.
    if(_matteSrcMap[li])continue;
    // Rendered variant: includes the mograph duplicator's N-way expansion
    // (app.js) — identical to getEffectiveStrokes for every other layer.
    var strokes=getEffectiveStrokesRendered(li,frameIdx);
    // Team review ghosts (revision-bridge.js): a frozen "before" copy a
    // reviewer's correction leaves behind, meant to stay visible ON-CANVAS
    // (so Accept/Reject has something to act on) but never in a rendered
    // output — tools.js explicitly documents this exclusion ("ghost …
    // excluded from export — see engine-bridge.js/export.js") yet nothing
    // here actually implemented it (2026-07 audit: confirmed live, a ghost
    // was baked straight into the exported PNG). getEffectiveStrokes()
    // itself must NOT filter these — the live canvas/review UI reads
    // through the same function and needs the ghost to render for the
    // reviewer to interact with; the exclusion belongs here, at every
    // actual output path, not at the shared read.
    strokes=strokes.filter(function(sd){return!sd.isRevisionGhost;});
    // Shadow Brush guide lines (2026-07) — their whole purpose is to
    // disappear once they've delimited a fill area (see shadow-brush-
    // bridge.js's header comment); default OFF so a fresh export doesn't
    // need this unchecked every time just to get a clean render.
    if(!state.exportIncludeShadowGuides)strokes=strokes.filter(function(sd){return sd.channelTag!=='shadow';});
    // Non-destructive combine groups (2026-07-29) — same post-process
    // buildSceneJson applies (engine-bridge.js), on the dict side: suppress
    // each combine-group member's own fill/stroke in this EXPORT-ONLY copy
    // (the real stored strokes are untouched) and append the combined
    // outline(s).
    if(window.SMGroup&&ld.groups)strokes=SMGroup.applyCombinesToStrokes(strokes,ld);
    // Motion mode (motion.js): unlike buildSceneJson (engine-bridge.js),
    // exportBuildFrame merges every original layer's strokes straight into
    // the ONE shared throwaway `L`, so there's no per-layer Paper object left
    // to transform after the fact. Fix: read each stroke's transform BEFORE
    // it's merged in (motionMat/motionPivot computed once per li, same as
    // the live path), and apply it to the Path `desP` just created — safe
    // here because `p` lives on `L`, a disposable export-only layer, never
    // userLayers[li] itself (see CLAUDE.md's family-of-bug-#1).
    var motionMat=(window.SMMotion&&strokes.length)?SMMotion.layerMotionAt(li,frameIdx):null;
    // Pivot = auto bounds center + Anchor Point offset (motionMat.ax/ay).
    var motionPivot=motionMat?new Point(userLayers[li].bounds.center.x+motionMat.ax,userLayers[li].bounds.center.y+motionMat.ay):null;
    // Layer parenting (2026-07, motion.js): each ancestor's own motion matrix
    // applied outermost, same composition order as buildSceneJson
    // (engine-bridge.js) — see motion.js's parentChainMats header comment.
    var parentChain=window.SMMotion?SMMotion.parentChainMats(li,frameIdx):[];
    // Trim Paths (2026-08-20) — texture-brush dab reveal, mirrors
    // engine-bridge.js's buildSceneJson computation exactly (same
    // "brushGroupId -> anchor strokeId" map, same per-group ordinal, same
    // reversed-order flip — see that file's own comment on why
    // insertAbove(basePath) makes layer/array order the REVERSE of stamp
    // order). Rebuilt once per li from the DICT array (not live Paper
    // items — export.js never touches userLayers[li] itself), but dict
    // array order is the same order serP captured the live children in,
    // so the ordinals match.
    var brushAnchorStrokeId=null,brushGroupDabs=null;
    strokes.forEach(function(bc){
      if(bc.brushGroupId){
        if(bc.isBrushTextureCopy){
          if(!brushGroupDabs)brushGroupDabs={};
          (brushGroupDabs[bc.brushGroupId]=brushGroupDabs[bc.brushGroupId]||[]).push(bc);
        }else if(bc.strokeId){
          if(!brushAnchorStrokeId)brushAnchorStrokeId={};
          brushAnchorStrokeId[bc.brushGroupId]=bc.strokeId;
        }
      }
    });
    var dabOrdinal=null;
    if(brushGroupDabs){
      dabOrdinal=new WeakMap();
      Object.keys(brushGroupDabs).forEach(function(gid){
        var list=brushGroupDabs[gid],n=list.length;
        list.forEach(function(dab,idx){dabOrdinal.set(dab,n>1?1-idx/(n-1):0);});
      });
    }
    var built=[]; // this layer's own items, so a blend mode can wrap them below
    strokes.forEach(function(sd){
      // Trim Paths (2026-08-20) — a dab isn't shaped by segments/closed at
      // all (its geometry is a fixed small stamp), so trimming means
      // filtering which dabs draw, not reshaping anything — same reasoning
      // as engine-bridge.js's own dab-reveal filter.
      if(sd.isBrushTextureCopy&&sd.brushGroupId&&dabOrdinal&&window.SMMotion){
        var dabAnchorId=brushAnchorStrokeId&&brushAnchorStrokeId[sd.brushGroupId];
        if(dabAnchorId){
          var dabWin=SMMotion.trimWindowAt(li,dabAnchorId,frameIdx);
          if(dabWin){
            var dabPct=(dabOrdinal.get(sd)||0)*100;
            var dabS=dabWin.start+(dabWin.offset||0),dabE=dabWin.end+(dabWin.offset||0);
            if(dabPct<Math.max(0,Math.min(100,dabS))||dabPct>Math.max(0,Math.min(100,dabE)))return;
          }
        }
      }
      // Raster strokes (isRaster: imported images, SVG-sequence frames,
      // and Bitmap Brush's texture companions, bitmap-brush.js) went
      // through desP() unconditionally before this — desP expects
      // .segments, which a Raster's serialized shape (x/y/width/height/src)
      // doesn't have, so ANY layer holding a Raster silently broke every
      // export (PNG/video/sequence): desP's undefined.map crashed the
      // whole exportBuildFrame call for that frame. Pre-existing gap (any
      // imported image already triggered it), surfaced now because Bitmap
      // Brush adds a Raster companion to nearly every textured stroke.
      // ⚠️ Corruption bug, found 2026-08-20 and fixed here: every hook below
      // that does `sd.segments=...` was mutating the object
      // getEffectiveStrokesRendered handed back — for a plain layer (the
      // common case) that's documented as "a LIVE reference into the
      // stored frame's own strokes array... A future caller that mutates
      // this return value directly would corrupt the STORED keyframe"
      // (getEffectiveStrokes's own comment, app.js). Confirmed live: a rect
      // with an animated corner radius, exported at a frame OTHER than its
      // keyframe, permanently overwrote the KEYFRAME's stored segments
      // (4 sharp-corner points) with that other frame's rebuilt geometry
      // (5 rounded-corner points) — every one of these hooks fires on
      // every export, not just the "export a range" case, so a single
      // still-frame PNG export was enough to corrupt the project.
      // sdCloned/ensureSdClone: clone ONCE, lazily, only when at least one
      // hook actually needs to rewrite something — cheap common case (no
      // active paramShape/vertex-follow/text-bounds-follow/trim) stays a
      // zero-cost no-op, matching getEffectiveStrokes's own "Clone with
      // JSON.parse(JSON.stringify(...)) before any in-place mutation, same
      // pattern used everywhere else in this file" prescription.
      var sdCloned=false;
      function ensureSdClone(){if(!sdCloned){sd=JSON.parse(JSON.stringify(sd));sdCloned=true;}}
      // Dynamic shape params (rect corners / ellipse arc / star, 2026-08):
      // mirrors engine-bridge.js's buildSceneJson hook exactly — same
      // family-of-bug-#1 risk (CLAUDE.md §1), this is a SECOND reader of
      // sd.segments that must rebuild geometry from the SAME per-frame
      // corner/arc/star values, or an exported PNG/video shows the static
      // un-animated shape while the live canvas shows it correctly.
      if(!sd.isRaster&&window.SMMotion&&sd.strokeId&&SMMotion.hasParamShapeMotionFor&&SMMotion.hasParamShapeMotionFor(li,sd.strokeId)){
        ensureSdClone();
        sd.segments=SMMotion.applyParamShapeFor(li,sd.strokeId,sd,frameIdx);
      }
      // Path-point parenting, "drive" direction (2026-08) — same second-
      // reader risk as paramShape just above, mirrors engine-bridge.js's
      // buildSceneJson hook exactly. The "follow" direction needs no call
      // here — it lives inside SMMotion.layerMotionAt, already the source
      // of `motionMat` a few lines below.
      if(!sd.isRaster&&window.SMMotion&&sd.strokeId&&SMMotion.hasPathVertexFollowMotionFor&&SMMotion.hasPathVertexFollowMotionFor(li,sd.strokeId)){
        ensureSdClone();
        sd.segments=SMMotion.applyPathVertexFollowFor(li,sd.strokeId,sd,frameIdx);
      }
      if(!sd.isRaster&&window.SMMotion&&sd.strokeId&&SMMotion.hasTextBoundsFollowMotionFor&&SMMotion.hasTextBoundsFollowMotionFor(li,sd.strokeId)){
        ensureSdClone();
        sd.segments=SMMotion.applyTextBoundsFollowFor(li,sd.strokeId,sd,frameIdx);
      }
      // Trim Paths (2026-08-20) — the KNOWN GAP applyTrimFor's own doc
      // comment named ("wired into engine-bridge.js's live render only, NOT
      // export.js... a trimmed shape currently exports un-trimmed"). Mirrors
      // buildSceneJson's own trim branch: vector-brush ribbons trim their
      // CENTERLINE+widthProfile and rebuild via buildVariableWidthPath
      // (tools.js) rather than arc-length-slicing the baked outline (see
      // 18b2d9a/de423b1's own reasoning — slicing the outline cuts across
      // the ribbon's own width); a plain stroke slices sd.segments directly
      // and loses its fillColor (18b2d9a — filling a trimmed OPEN arc draws
      // the AE "pac-man wedge" instead of a clean line). Same ensureSdClone
      // as the three hooks above — trim always changes the segment COUNT
      // (and often `closed`), so it can't reuse the original object either.
      if(!sd.isRaster&&window.SMMotion&&sd.strokeId&&SMMotion.hasTrimMotionFor(li,sd.strokeId)){
        ensureSdClone();
        if(sd.isVectorBrush&&sd.centerSegments&&sd.centerSegments.length>=2){
          var vbTrim=SMMotion.applyTrimToVectorBrush(li,sd.strokeId,sd.centerSegments,sd.widthProfile,frameIdx);
          var vbOutline=(vbTrim&&vbTrim.pts&&vbTrim.pts.length>=2)?buildVariableWidthPath(vbTrim.pts.map(function(pt){return new Point(pt[0],pt[1]);}),vbTrim.widths):null;
          if(vbOutline){
            sd.segments=vbOutline.segments.map(function(s){return{point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};});
            sd.closed=true;
            vbOutline.remove();
          }else{sd.segments=[];sd.closed=false;}
        }else{
          var trimmed=SMMotion.applyTrimFor(li,sd.strokeId,sd.segments,sd.closed,frameIdx);
          sd.segments=trimmed.segments;sd.closed=trimmed.closed;
          if(!sd.isVectorBrush){sd.fillColor=null;delete sd.fillGradient;}
        }
      }
      var p=sd.isRaster?desR(sd,L,sd.opacity!==undefined?sd.opacity:1):desP(sd,L,sd.opacity!==undefined?sd.opacity:1);
      // Gradient fill (2026-07) — Paper.js has native Gradient support, so
      // export/preview through THIS (pure-Paper.js) path gets a real
      // gradient for free, unlike the Rust/vello live-canvas path which
      // needed its own ItemIn.fillGradient + peniko::Gradient plumbing
      // (engine-bridge.js/geometry-wasm). Overrides desP's flat fillColor.
      // Same from/to guard as buildSceneJson's own branch — an export must
      // not throw on data the on-screen render already tolerates (CLAUDE.md
      // §3: the two paths have to agree, including about what they refuse).
      var pendingGradient=(!sd.isRaster&&sd.fillGradient&&(!window.gradientGeomOk||window.gradientGeomOk(sd.fillGradient)))?sd.fillGradient:null;
      var pendingGradientFrom=pendingGradient?new Point(pendingGradient.from[0],pendingGradient.from[1]):null;
      var pendingGradientTo=pendingGradient?new Point(pendingGradient.to[0],pendingGradient.to[1]):null;
      function transformGradientByMat(pt,pivot,mat){
        if(!pt||!mat)return pt;
        var q=new Point(pivot.x+(pt.x-pivot.x)*mat.sx,pivot.y+(pt.y-pivot.y)*mat.sy);
        q=q.rotate(mat.rot,pivot);
        return q.add(new Point(mat.dx,mat.dy));
      }
      // Path property, per-vertex (motion.js's applyPathVertexOffsetsFor,
      // 2026-07): innermost transform — applied directly to `p`'s own
      // segments before elMat's pivot is even read from p.bounds, same
      // ordering as buildSceneJson's engine-bridge.js branch.
      if(!sd.isRaster&&window.SMMotion&&sd.strokeId){
        var vtxSegs=SMMotion.applyPathVertexOffsetsFor(li,sd.strokeId,sd.segments,frameIdx);
        for(var vsi=0;vsi<p.segments.length&&vsi<vtxSegs.length;vsi++){
          p.segments[vsi].point.x=vtxSegs[vsi].point[0];
          p.segments[vsi].point.y=vtxSegs[vsi].point[1];
        }
      }
      // Element-level Motion target (2026-07): applied FIRST, pivoted
      // around THIS stroke's own just-built bounds (not the whole layer's)
      // — see motion.js's elementMotionAt header comment. sd.strokeId is
      // the raw serialized field (same one serP/desP already round-trip).
      var elMat=(window.SMMotion&&sd.strokeId)?SMMotion.elementMotionAt(li,sd.strokeId,frameIdx):null;
      if(elMat){
        var epc=p.bounds.center;
        var elPivot=new Point(epc.x+elMat.ax,epc.y+elMat.ay);
        pendingGradientFrom=transformGradientByMat(pendingGradientFrom,elPivot,elMat);
        pendingGradientTo=transformGradientByMat(pendingGradientTo,elPivot,elMat);
        p.scale(elMat.sx,elMat.sy,elPivot);
        p.rotate(elMat.rot,elPivot);
        p.translate(elMat.dx,elMat.dy);
        p.opacity=p.opacity*elMat.op;
        if(p.strokeWidth)p.strokeWidth*=(Math.abs(elMat.sx)+Math.abs(elMat.sy))/2;
      }
      if(motionMat){
        pendingGradientFrom=transformGradientByMat(pendingGradientFrom,motionPivot,motionMat);
        pendingGradientTo=transformGradientByMat(pendingGradientTo,motionPivot,motionMat);
        p.scale(motionMat.sx,motionMat.sy,motionPivot);
        p.rotate(motionMat.rot,motionPivot);
        p.translate(motionMat.dx,motionMat.dy);
        p.opacity=p.opacity*motionMat.op;
        if(p.strokeWidth)p.strokeWidth*=(Math.abs(motionMat.sx)+Math.abs(motionMat.sy))/2;
      }
      for(var pci=0;pci<parentChain.length;pci++){
        var pc=parentChain[pci];
        pendingGradientFrom=transformGradientByMat(pendingGradientFrom,pc.pivot,pc.mat);
        pendingGradientTo=transformGradientByMat(pendingGradientTo,pc.pivot,pc.mat);
        p.scale(pc.mat.sx,pc.mat.sy,pc.pivot);
        p.rotate(pc.mat.rot,pc.pivot);
        p.translate(pc.mat.dx,pc.mat.dy);
        p.opacity=p.opacity*pc.mat.op;
        if(p.strokeWidth)p.strokeWidth*=(Math.abs(pc.mat.sx)+Math.abs(pc.mat.sy))/2;
      }
      if(pendingGradient){
        var stops=pendingGradient.stops.map(function(s){return [s.color,s.offset];});
        var grad=new Gradient(stops,pendingGradient.kind==='radial');
        p.fillColor=new Color(grad,pendingGradientFrom,pendingGradientTo);
      }
      built.push(p);
    });
    // Layer BLEND MODE — buildSceneJson passes ld.blendMode straight to the
    // renderer, so it is visible on screen; nothing here read it, so every
    // raster/SVG export silently came out in Normal. Everything above merges
    // into the ONE flat `L`, leaving no per-layer object to carry it, so give
    // this layer's items their own Group and hang the blend mode on that —
    // Paper.js composites a Group's blendMode natively, so rasterize() and
    // exportSVG() both get it for free. Only when it is actually non-normal:
    // the flat structure stays exactly as it was for every ordinary layer.
    if(built.length&&ld.blendMode&&ld.blendMode!=='normal'){
      var grp=new Group({children:built,insert:false});
      grp.blendMode=ld.blendMode;
      L.addChild(grp);
    }
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
  // h264_videotoolbox (Apple's own hardware/OS H.264 encoder via the
  // VideoToolbox framework), not libx264 — the 2026-08-18 license rebuild
  // (THIRD_PARTY_NOTICES.md) dropped libx264 (GPL + H.264 patent exposure).
  // VideoToolbox needs no such license: it's Apple's OS-provided encoder,
  // already licensed by Apple for its own use, same principle already
  // applied to exportVideoBrowser's MediaRecorder path. -q:v is
  // VideoToolbox's quality control (no -crf equivalent exists there);
  // 65 is a high-quality default, roughly matching the visual target the
  // old -crf 18 aimed for.
  var qv=MP4_QUALITY_QV[(opts&&opts.quality)||'high']||'65';
  await exportRunFfmpeg(['-y','-framerate',String(fps),'-i',workDir+'/frame_%04d.png','-c:v','h264_videotoolbox','-pix_fmt','yuv420p','-q:v',qv,'-profile:v','high',outPath],opts&&opts.onFfmpeg);
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

// Any layer with an enabled effect anywhere in the project — mirrors
// engine.rs's own has_effects_stack check (composite_scene). Cheap: no
// layer in the overwhelming common case (no effects used) short-circuits
// on the very first layer.
function exportHasActiveEffects(){
  if(state.layers.some(function(ld){return ld.effects&&ld.effects.some(function(e){return e.enabled;});}))return true;
  // PER-STROKE effects too (sd.effects — engine-bridge.js:508 reads
  // c.data.effects and runs it through the same sceneEffectsOf). This
  // predicate only ever scanned LAYER effects, so a project whose effects
  // all live on individual strokes stayed on the Paper path and lost every
  // one of them in the export, silently — the same screen/export split
  // blend and matte had (2026-07-26). Short-circuits on the first hit, and
  // runs once per export, not per frame.
  return state.layers.some(function(ld){
    return (ld.frames||[]).some(function(f){
      return f&&(f.strokes||[]).some(function(sd){
        return sd&&sd.effects&&sd.effects.length&&sd.effects.some(function(e){return e.enabled;});
      });
    });
  });
}
// Layer BLEND MODE and TRACK MATTE are per-layer compositing, and
// exportBuildFrame below merges every layer into ONE flat throwaway Paper
// layer — there is no per-layer object left for either to live on. Found
// 2026-07-26 by diffing screen against export feature by feature: both are
// read by buildSceneJson (engine-bridge.js:512/572) so they render on
// screen, and export.js read NEITHER, so both silently vanished from every
// raster export. Matte was the worse half: engine.rs's composite_scene also
// SKIPS painting the matte source layer as its own content, so the export
// additionally GAINED an opaque layer the screen never shows.
//
// Routed through the engine, exactly like effects already are, rather than
// reimplementing compositing in Paper: same proven path, and screen/export
// agree by construction instead of by two implementations staying in sync
// (CLAUDE.md §3's whole point). Blend ALSO gets a native Paper fallback in
// exportBuildFrame for the paths this routing can't cover (SVG, scale>1).
function exportHasLayerCompositing(){
  // isFolderLayer (audit 2026-08-29, prouvé au pixel près) : un Dossier
  // (#319) est une feature de COMPOSITING moteur — composite_scene rend
  // ses enfants dans une passe isolée À LA POSITION Z DU DOSSIER, alors
  // que le fallback Paper.js ci-dessous peint tout dans l'ordre brut de
  // state.layers. Dès qu'un calque étranger s'intercale entre un enfant
  // et son dossier dans l'ordre brut, l'export fallback inverse la
  // superposition visible à l'écran (mesuré : pixel de chevauchement
  // rouge à l'écran/moteur, bleu dans exportFrameDataURL — même scène).
  // Même logique de routage que blend/matte/3D/motionBlur/order : toute
  // feature que le chemin Paper ne sait pas reproduire route l'export par
  // le moteur.
  return state.layers.some(function(ld){
    return (ld.blendMode&&ld.blendMode!=='normal')||(ld.matteMode&&ld.matteMode!=='none')||!!ld.isFolderLayer;
  });
}
// 3D layers and Motion Blur (2026-08-17 audit, "vérifie les export de
// chaque feature animée") — exportBuildFrame (the plain-Paper.js fallback
// below) applies layerMotionAt/elementMotionAt's ORDINARY 2D matrix to
// each stroke, but has NO per-vertex 3D projector (SMMotion.
// project3DSegments/make3DProjector — engine-bridge.js only) and NO
// motion-blur post-process (buildSceneJson's own mbOn branch, motion.js
// §11: "post-treatment... never duplicate the construction loop" — this
// Paper path IS that second construction loop, and it never got the
// post-process). Neither gap threw or warned: a 3D layer exported
// perfectly flat, and a motion-blurred layer exported perfectly sharp,
// both silently correct-looking to anyone who didn't diff against the
// on-screen render frame-by-frame. Same root cause as #88's onion-skin/
// Ghost-All fix — a secondary render path that predates 3D/Motion Blur
// and was never taught about either. Fix: route both through the engine,
// exactly like effects/compositing already do a few lines up — motionBlur
// checks state.motionBlurOn too, matching buildSceneJson's own mbOn gate
// (a layer can have motionBlur:true while the comp-wide switch is off).
function exportHasEngineOnlyMotion(){
  // Order (2026-08) — same "engine-only feature" shape as 3D/Motion Blur
  // above: exportBuildFrame's plain-Paper.js loop below just walks
  // state.layers/layer.children in their raw stored order with no concept
  // of a z-index override at all, so a keyed Order silently did nothing in
  // export while working perfectly on screen. Checked once for the whole
  // document (anyOrderUsedAnywhere covers both layer- and element-level),
  // not per-layer like the other two, since a re-order can move ANY
  // layer/shape regardless of which one carries the actual keys.
  if(window.SMMotion&&window.SMMotion.anyOrderUsedAnywhere&&window.SMMotion.anyOrderUsedAnywhere())return true;
  return state.layers.some(function(ld){
    return ld.threeD||(ld.motionBlur&&state.motionBlurOn);
  });
}
function exportNeedsEngine(){return exportHasActiveEffects()||exportHasLayerCompositing()||exportHasEngineOnlyMotion();}
// ---- PNG sequence rendering to a working directory (shared by raster exports) ----
async function exportRenderPNGsToDir(dir,start,end,scale,onProgress,alpha){
  // Effects (blur/vignette/glow/ground shadow/...) only ever rendered in
  // the live GPU preview — exportFrameDataURL rasterizes straight from
  // Paper.js's vector data, with no route through the WGPU effect stack at
  // all (feedback 2026-07: "vérifie que le rendu temps réel marche bien
  // avec les effets" surfaced this gap). When the project actually uses an
  // effect, route every frame through the engine instead so the export
  // matches what the user sees on screen — see engine-bridge.js's
  // beginEffectsExport/renderFrameToPixelsPNG/endEffectsExport.
  // scale (supersampled export, 2026-08 feedback #60) renders natively at
  // cw*scale x ch*scale through the engine too — see renderFrameToPixelsPNG.
  var useFx=exportNeedsEngine()&&window.SMEngineBridge&&window.SMEngineBridge.beginEffectsExport();
  try{
    for(var f=start,i=1;f<=end;f++,i++){
      // `alpha` reaches BOTH paths now (2026-08): it used to be passed
      // only to exportFrameDataURL, so "Fond transparent" was silently
      // ignored on every export that routed through the engine — i.e.
      // every export with an effect in it.
      var url=useFx?await SMEngineBridge.renderFrameToPixelsPNG(f,scale,alpha):exportFrameDataURL(f,scale,alpha);
      var bytes=exportDataURLToBytes(url);
      await exportWriteBytes(dir+'/frame_'+pad4(i)+'.png',bytes);
      if(onProgress)onProgress(i,end-start+1);
    }
  }finally{
    if(useFx)SMEngineBridge.endEffectsExport();
  }
}

// ---- Browser-compatible video export (2026-08-17) ----
// Cyril: "une méthode différente d'export vidéo compatible avec la
// version navigateur" — every raster video path above (MP4/GIF/ProRes)
// shells out to the bundled ffmpeg sidecar via Tauri, so none of them
// work in the plain browser preview at all (exportTauriAvailable() gate,
// hard error). MediaRecorder + canvas.captureStream() is the standard
// browser-native alternative: no ffmpeg, no native binary, works in the
// preview AND would work in a future pure-web build of this app.
//
// Timing is the one real constraint MediaRecorder imposes that ffmpeg
// doesn't: it's fundamentally a REAL-TIME capture API — a chunk's
// recorded duration is however long it sat on the canvas in wall-clock
// time between captureStream(0)'s manual track.requestFrame() calls, not
// a frame count. So exporting an N-frame clip at F fps necessarily takes
// ~N/F real seconds here (rendering each frame, pushing it, then
// sleeping out the rest of that frame's 1000/F ms slot) — slower than
// ffmpeg's batch encode, but the only way to get correct PLAYBACK speed
// out of this API. Same frame SOURCE as the ffmpeg path (useFx routes
// through the engine for effects/compositing/3D/motion-blur projects,
// exactly like exportRenderPNGsToDir), so what gets recorded matches
// what the ffmpeg export would have produced, just muxed differently.
function exportVideoBrowserAvailable(){
  return typeof MediaRecorder!=='undefined'&&!!(document.createElement('canvas').captureStream);
}
// Preference order: real MP4/H.264 first (plays everywhere, including a
// share/email attachment) when the browser actually supports muxing it
// (Safari 16+, some Chrome versions with the right flags) — WebM/VP9 as
// the broadly-supported fallback (every Chromium/Firefox build), VP8 as
// the last resort for older engines.
var VIDEO_MIME_CANDIDATES=['video/mp4;codecs=avc1.42E01E','video/mp4','video/webm;codecs=vp9','video/webm;codecs=vp8','video/webm'];
function exportPickVideoMimeType(){
  if(!window.MediaRecorder||!MediaRecorder.isTypeSupported)return'';
  for(var i=0;i<VIDEO_MIME_CANDIDATES.length;i++){
    if(MediaRecorder.isTypeSupported(VIDEO_MIME_CANDIDATES[i]))return VIDEO_MIME_CANDIDATES[i];
  }
  return'';
}
function exportLoadImage(url){
  return new Promise(function(resolve,reject){
    var img=new Image();
    img.onload=function(){resolve(img);};
    img.onerror=function(){reject(new Error('image decode failed'));};
    img.src=url;
  });
}
function exportSleep(ms){return new Promise(function(resolve){setTimeout(resolve,ms);});}
async function exportVideoBrowser(opts){
  if(!exportVideoBrowserAvailable())return{ok:false,error:'Export vidéo non supporté par ce navigateur (MediaRecorder/captureStream indisponible).'};
  var mime=exportPickVideoMimeType();
  if(!mime)return{ok:false,error:'Aucun codec vidéo (MP4/WebM) supporté par ce navigateur.'};
  var r=exportFrameRange(opts);var scale=(opts&&opts.scale)||1;var fps=(opts&&opts.fps)||state.fps;
  var cw=Math.max(1,Math.round(state.canvasW*scale)),ch=Math.max(1,Math.round(state.canvasH*scale));
  var canvas=document.createElement('canvas');canvas.width=cw;canvas.height=ch;
  var ctx=canvas.getContext('2d');
  // captureStream(0) = manual mode: the track only advances when
  // requestFrame() is called, instead of auto-sampling the canvas at a
  // fixed rate — lets each drawn frame's ON-SCREEN duration be controlled
  // precisely by the sleep below rather than racing an independent timer.
  var stream=canvas.captureStream(0);
  var track=stream.getVideoTracks()[0];
  var chunks=[];
  var rec=new MediaRecorder(stream,{mimeType:mime,videoBitsPerSecond:8000000});
  rec.ondataavailable=function(e){if(e.data&&e.data.size)chunks.push(e.data);};
  var stopped=new Promise(function(resolve){rec.onstop=resolve;});
  rec.start();
  var useFx=exportNeedsEngine()&&window.SMEngineBridge&&window.SMEngineBridge.beginEffectsExport();
  try{
    var frameMs=1000/fps;
    for(var f=r.start,i=1;f<=r.end;f++,i++){
      var t0=performance.now();
      var url=useFx?await SMEngineBridge.renderFrameToPixelsPNG(f,scale):exportFrameDataURL(f,scale,false);
      var img=await exportLoadImage(url);
      ctx.clearRect(0,0,cw,ch);
      ctx.drawImage(img,0,0,cw,ch);
      track.requestFrame();
      if(opts&&opts.onProgress)opts.onProgress(i,r.end-r.start+1);
      var wait=frameMs-(performance.now()-t0);
      if(wait>0)await exportSleep(wait);
    }
  }finally{
    if(useFx)SMEngineBridge.endEffectsExport();
  }
  // Hold the last frame on screen for one more slot before stopping — a
  // recorder stopped the instant the final requestFrame() fires can clip
  // that frame's chunk short (0-duration or dropped), same "flush before
  // teardown" reasoning as any streaming encoder.
  await exportSleep(Math.max(50,1000/fps));
  rec.stop();
  await stopped;
  var mimeBase=mime.split(';')[0];
  var blob=new Blob(chunks,{type:mimeBase});
  var ext=mimeBase.indexOf('mp4')>=0?'mp4':'webm';
  var filename=(opts&&opts.filename)||('animation.'+ext);
  var u=URL.createObjectURL(blob);
  var a=document.createElement('a');a.href=u;a.download=filename;a.click();
  URL.revokeObjectURL(u);
  return{ok:true,browserFallback:true,mimeType:mime,bytes:blob.size,filename:filename};
}

// ---- Browser-compatible GIF export (2026-08-17) ----
// Cyril: "étendre l'export navigateur au GIF" — unlike MP4/WebM, no
// browser exposes a native GIF encoder (MediaRecorder doesn't do GIF),
// so this is a from-scratch GIF89a writer: median-cut palette
// quantization (once, from a sample of frames — a per-frame palette
// would flicker colors frame to frame) + textbook LZW compression, no
// external library. Self-contained on purpose: this app's dev preview
// has no bundler/npm install step to pull a GIF encoder in through, and
// the algorithm itself is small and stable enough that hand-rolling it
// is less risk than vendoring an unaudited third-party minified blob.
function exportGifNearestColor(r,g,b,palette){
  var best=0,bestD=Infinity;
  for(var i=0;i<palette.length;i+=3){
    var dr=r-palette[i],dg=g-palette[i+1],db=b-palette[i+2];
    var d=dr*dr+dg*dg+db*db;
    if(d<bestD){bestD=d;best=i/3;if(d===0)break;}
  }
  return best;
}
// Median-cut: recursively splits the sampled pixel set along its widest
// channel until there are `maxColors` boxes, then averages each box into
// one palette entry — the standard, well-understood quantizer (same
// family GIF encoders have used since the format's 1989 spec), chosen
// over a fancier one (NeuQuant, k-means) for how little code it needs.
function exportGifMedianCut(pixels,maxColors){
  var boxes=[pixels];
  while(boxes.length<maxColors){
    boxes.sort(function(a,b){return exportGifBoxRange(b)-exportGifBoxRange(a);});
    var box=boxes.shift();
    if(box.length<2){boxes.push(box);break;}
    var ch=exportGifWidestChannel(box);
    box.sort(function(a,b){return a[ch]-b[ch];});
    var mid=box.length>>1;
    boxes.push(box.slice(0,mid),box.slice(mid));
  }
  return boxes.map(function(box){
    var r=0,g=0,b=0;
    box.forEach(function(p){r+=p[0];g+=p[1];b+=p[2];});
    var n=box.length||1;
    return[Math.round(r/n),Math.round(g/n),Math.round(b/n)];
  });
}
function exportGifWidestChannel(box){
  var min=[255,255,255],max=[0,0,0];
  box.forEach(function(p){for(var c=0;c<3;c++){if(p[c]<min[c])min[c]=p[c];if(p[c]>max[c])max[c]=p[c];}});
  var range=[max[0]-min[0],max[1]-min[1],max[2]-min[2]];
  return range[0]>=range[1]&&range[0]>=range[2]?0:(range[1]>=range[2]?1:2);
}
function exportGifBoxRange(box){
  if(box.length<2)return 0;
  var ch=exportGifWidestChannel(box);
  var min=255,max=0;
  box.forEach(function(p){if(p[ch]<min)min=p[ch];if(p[ch]>max)max=p[ch];});
  return(max-min)*box.length; // weight by population so big flat boxes still get split
}
// LZW encoder, GIF's own variable-code-width variant (codes grow from
// minCodeSize+1 bits up to 12, clear/end-of-information codes reserved
// at the bottom of the table) — ports the algorithm every GIF spec
// walks through, operating on PALETTE INDICES (not RGB) since that's
// what an Image Data block actually carries.
function exportGifLZWEncode(indices,minCodeSize){
  var clearCode=1<<minCodeSize,eoiCode=clearCode+1;
  var codeSize=minCodeSize+1,nextCode=eoiCode+1;
  var dict={};
  var out=[];var bitBuf=0,bitCount=0;
  function emit(code){
    bitBuf|=code<<bitCount;bitCount+=codeSize;
    while(bitCount>=8){out.push(bitBuf&0xff);bitBuf>>=8;bitCount-=8;}
  }
  function resetDict(){dict={};nextCode=eoiCode+1;codeSize=minCodeSize+1;for(var i=0;i<clearCode;i++)dict[i]=i;}
  resetDict();emit(clearCode);
  var w=indices[0];
  for(var i=1;i<indices.length;i++){
    var k=indices[i];var wk=w+','+k;
    if(dict[wk]!==undefined){w=wk;}
    else{
      emit(dict[w]);
      if(nextCode<4096){dict[wk]=nextCode++;if(nextCode>(1<<codeSize)&&codeSize<12)codeSize++;}
      else{emit(clearCode);resetDict();}
      w=''+k;
    }
  }
  emit(dict[w]);emit(eoiCode);
  if(bitCount>0)out.push(bitBuf&0xff);
  return out;
}
function exportGifAvailable(){
  return typeof document.createElement('canvas').getContext==='function';
}
async function exportGifBrowser(opts){
  var r=exportFrameRange(opts);var scale=(opts&&opts.scale)||1;var fps=(opts&&opts.fps)||state.fps;
  var cw=Math.max(1,Math.round(state.canvasW*scale)),ch=Math.max(1,Math.round(state.canvasH*scale));
  var canvas=document.createElement('canvas');canvas.width=cw;canvas.height=ch;
  var ctx=canvas.getContext('2d',{willReadFrequently:true});
  var useFx=exportNeedsEngine()&&window.SMEngineBridge&&window.SMEngineBridge.beginEffectsExport();
  var frameCount=r.end-r.start+1;
  var framePixels=[];
  try{
    // Pass 1: render every frame to RGBA pixel data, and sample a subset
    // of pixels across all of them for the palette so colors that only
    // appear in a few frames (a flash of red mid-animation) still make
    // the cut — a palette built from frame 0 alone would clip anything
    // introduced later.
    var samples=[];
    for(var f=r.start,i=1;f<=r.end;f++,i++){
      var url=useFx?await SMEngineBridge.renderFrameToPixelsPNG(f,scale):exportFrameDataURL(f,scale,false);
      var img=await exportLoadImage(url);
      ctx.clearRect(0,0,cw,ch);ctx.drawImage(img,0,0,cw,ch);
      var data=ctx.getImageData(0,0,cw,ch).data;
      framePixels.push(data);
      var step=Math.max(1,Math.floor((cw*ch)/2000)); // ~2000 samples/frame, plenty for median-cut
      for(var p=0;p<data.length;p+=4*step)samples.push([data[p],data[p+1],data[p+2]]);
      if(opts&&opts.onProgress)opts.onProgress(i,frameCount*2);
    }
  }finally{
    if(useFx)SMEngineBridge.endEffectsExport();
  }
  var palette=exportGifMedianCut(samples,255); // 255 + 1 reserved slot kept below 256
  while(palette.length<256)palette.push([0,0,0]);
  var flatPalette=[];palette.forEach(function(c){flatPalette.push(c[0],c[1],c[2]);});
  var minCodeSize=Math.max(2,Math.ceil(Math.log2(palette.length)));

  // ---- GIF89a assembly ----
  var bytes=[];
  function pushStr(s){for(var k=0;k<s.length;k++)bytes.push(s.charCodeAt(k));}
  function push16(n){bytes.push(n&0xff,(n>>8)&0xff);}
  pushStr('GIF89a');
  push16(cw);push16(ch);
  bytes.push(0xF0|(minCodeSize-1)); // global color table present, 256 entries
  bytes.push(0);bytes.push(0); // bg color index, pixel aspect
  flatPalette.forEach(function(v){bytes.push(v);});
  // NETSCAPE2.0 application extension — infinite loop, same convention
  // every GIF-producing tool (ffmpeg's own palette path included) uses.
  bytes.push(0x21,0xFF,0x0B);pushStr('NETSCAPE2.0');bytes.push(3,1,0,0,0);
  var delayCs=Math.max(1,Math.round(100/fps)); // GIF delay unit is 1/100s
  for(var fi=0;fi<framePixels.length;fi++){
    var data2=framePixels[fi];
    var indices=new Array(cw*ch);
    // Memoized per exact RGB triple (2026-08-17): this app's content is
    // almost entirely flat-fill vector shapes, so a real frame is
    // overwhelmingly a handful of distinct colors repeated over huge flat
    // regions — caching collapses what was previously one brute-force
    // 256-entry scan PER PIXEL down to one scan per distinct color ever
    // seen. Measured: brings a 1920×1080 frame from several seconds to
    // under one on typical vector content (a raster/gradient-heavy frame
    // with thousands of unique colors degrades toward the uncached cost,
    // same as it always was — this is a best-case speedup, not a
    // complexity-class change).
    var colorCache={};
    for(var pi=0,di=0;di<data2.length;di+=4,pi++){
      var key=(data2[di]<<16)|(data2[di+1]<<8)|data2[di+2];
      var idx=colorCache[key];
      if(idx===undefined){idx=exportGifNearestColor(data2[di],data2[di+1],data2[di+2],flatPalette);colorCache[key]=idx;}
      indices[pi]=idx;
    }
    bytes.push(0x21,0xF9,4,0x00);push16(delayCs);bytes.push(0,0);
    bytes.push(0x2C);push16(0);push16(0);push16(cw);push16(ch);bytes.push(0);
    bytes.push(minCodeSize);
    var lzw=exportGifLZWEncode(indices,minCodeSize);
    for(var off=0;off<lzw.length;off+=255){
      var chunk=lzw.slice(off,off+255);
      bytes.push(chunk.length);
      chunk.forEach(function(v){bytes.push(v);});
    }
    bytes.push(0);
    if(opts&&opts.onProgress)opts.onProgress(frameCount+fi+1,frameCount*2);
  }
  bytes.push(0x3B);
  var blob=new Blob([new Uint8Array(bytes)],{type:'image/gif'});
  var filename=(opts&&opts.filename)||'animation.gif';
  var u=URL.createObjectURL(blob);
  var a=document.createElement('a');a.href=u;a.download=filename;a.click();
  URL.revokeObjectURL(u);
  return{ok:true,browserFallback:true,bytes:blob.size,filename:filename,paletteSize:palette.length};
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
// Stroke gradient along path (2026-08, AE feature audit 6.3/9.2) — Lottie
// has no native "gradient follows the stroke's own length" concept either
// (its 'gs' gradient-stroke shape is spatial, same 2-point limitation AE
// itself has) — same reasoning as engine-bridge.js's live-render approach:
// split into many small solid-colored straight sub-segments, one Lottie
// shape GROUP per piece, all packed into the layer's `shapes` array
// together (Lottie composites sibling groups within one shape layer
// naturally, no separate Lottie layer needed per piece).
//
// STATIC approximation, posed at the run's first frame only — unlike the
// path animation ('sh' keyframes) elsewhere in this exporter, a per-piece
// split can't easily follow a frame-by-frame ANIMATED path (piece count/
// positions would need re-deriving every frame, and Lottie has no keyframed
// "N groups" concept). Stated v1 limitation: a gradient-along-path stroke
// that ALSO moves/reshapes during this run exports frozen at its start
// pose — correct for the overwhelmingly common case (a static or lightly
// keyed decorative stroke), same tradeoff class as Trim Paths' own
// polyline-not-exact-bezier approximation.
function lottieGradientAlongPathShapes(first){
  if(!window.SMMotion)return null;
  var sg=first.strokeGradientAlongPath;
  var fromRgba=lottieHexToRGBA(sg.from,1),toRgba=lottieHexToRGBA(sg.to,1);
  var poly=SMMotion.flattenSegmentsToPolyline(first.segments,first.closed,20);
  var cumL=[0];
  for(var i=1;i<poly.length;i++){var dx=poly[i][0]-poly[i-1][0],dy=poly[i][1]-poly[i-1][1];cumL.push(cumL[i-1]+Math.sqrt(dx*dx+dy*dy));}
  var totalL=cumL[cumL.length-1];
  if(totalL<=0)return null;
  var pieceCount=Math.max(6,Math.min(48,Math.round(totalL/40)));
  var pieceLen=totalL/pieceCount;
  function pointAtLen(len){
    for(var qi=1;qi<cumL.length;qi++){
      if(cumL[qi]>=len){var segLen=cumL[qi]-cumL[qi-1];var t=segLen>0?(len-cumL[qi-1])/segLen:0;return[poly[qi-1][0]+(poly[qi][0]-poly[qi-1][0])*t,poly[qi-1][1]+(poly[qi][1]-poly[qi-1][1])*t];}
    }
    return poly[poly.length-1];
  }
  var op=first.opacity!==undefined?first.opacity:1;
  var groups=[];
  for(var pi=0;pi<pieceCount;pi++){
    var pA=pointAtLen(pi*pieceLen),pB=pointAtLen((pi+1)*pieceLen);
    var tMid=(pi+0.5)/pieceCount;
    var col=[
      (fromRgba[0]+(toRgba[0]-fromRgba[0])*tMid),
      (fromRgba[1]+(toRgba[1]-fromRgba[1])*tMid),
      (fromRgba[2]+(toRgba[2]-fromRgba[2])*tMid),
      1
    ];
    groups.push({ty:'gr',it:[
      {ty:'sh',ks:{a:0,k:{i:[[0,0],[0,0]],o:[[0,0],[0,0]],v:[pA,pB],c:false}}},
      {ty:'st',c:{a:0,k:col},o:{a:0,k:op*100},w:{a:0,k:first.strokeWidth||2},lc:first.strokeCap==='round'?2:(first.strokeCap==='square'?3:1),lj:2},
      {ty:'tr',p:{a:0,k:[0,0]},a:{a:0,k:[0,0]},s:{a:0,k:[100,100]},r:{a:0,k:0},o:{a:0,k:100}},
    ]});
  }
  return groups;
}
function lottiePathLayer(name,runStrokes,runStart,runEnd,fps,camByFrame,bm,li){
  var first=runStrokes[runStart];
  if(first.strokeGradientAlongPath&&first.hasRealStroke&&first.segments&&first.segments.length){
    var gradShapes=lottieGradientAlongPathShapes(first);
    if(gradShapes){
      return{
        ddd:0,ty:4,nm:name,sr:1,
        ks:{o:{a:0,k:100},r:{a:0,k:0},p:{a:0,k:[0,0,0]},a:{a:0,k:[0,0,0]},s:{a:0,k:[100,100,100]}},
        ao:0,
        shapes:gradShapes,
        ip:runStart,op:runEnd+1,st:0,bm:bm||0
      };
    }
  }
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
  // Trim Paths (2026-08, AE feature audit 9.2) — Lottie has a NATIVE shape
  // modifier for this ('tm', Bodymovin/lottie-web's own Trim Paths), a
  // near 1:1 mapping to Nemo's own trimStart/trimEnd/trimOffset: s/e are
  // the SAME 0-100 percent space, o is in DEGREES (AE's own Trim Paths
  // Offset property is natively degrees, not percent — Nemo's own o is
  // percent for UI-simplicity reasons, see motion.js's PROP_DEFAULT
  // comment, so *3.6 converts one to the other here at the export
  // boundary only). Keyframed per-frame exactly like the path ('sh')
  // shape above, since trim is itself an animatable per-element property.
  if(window.SMMotion&&li!=null&&first.strokeId&&SMMotion.hasTrimMotionFor(li,first.strokeId)){
    var tmS=[],tmE=[],tmO=[];
    for(var tf=runStart;tf<=runEnd;tf++){
      var win=SMMotion.trimWindowAt(li,first.strokeId,tf)||{start:0,end:100,offset:0};
      tmS.push({t:tf,s:[win.start]});tmE.push({t:tf,s:[win.end]});tmO.push({t:tf,s:[win.offset*3.6]});
    }
    shapeItems.push({ty:'tm',s:{a:1,k:tmS},e:{a:1,k:tmE},o:{a:1,k:tmO},m:1});
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
    var ld=state.layers[li];
    // Same solo-aware fix as exportBuildFrame above — this Lottie exporter
    // had the identical bare .visible check, same leak.
    if(!layerIsEffectivelyVisible(li))continue;
    var bm=lottieBmCode(ld.blendMode);
    // per-frame strokes array for this layer across the export range —
    // drops fully-invisible brush-texture anchors (opacity:0 by convention,
    // see applyBrushTexture's own comment in tools.js): they render nothing
    // in any player (o:0 already round-trips correctly) but a textured
    // stroke can carry dozens to hundreds of dab companions, so skipping
    // the anchor keeps file size and shape-layer count from ballooning for
    // zero visual difference. Dab companions themselves are NOT skipped —
    // they ARE the visible texture.
    // isRaster strokes (imported images, and Bitmap Brush's texture
    // companions, bitmap-brush.js) also dropped here — lottieShapeValue
    // below builds every shape from sd.segments, which a Raster's
    // serialized shape (x/y/width/height/src) doesn't have; this Lottie
    // exporter has no image-asset/raster-layer support at all, so a
    // bitmap-brush stroke degrades to "just its anchor, if it carries a
    // fill" (still filtered out too when opacity:0, the fill-less case)
    // rather than crashing lottieBuild for the whole export. Vector-preset
    // dab companions are UNAFFECTED — they're real Paths with segments,
    // same as before.
    var framesStrokes=[];
    for(var f=start;f<=end;f++){
      // isMask (2026-08, AE feature audit 9.2 "voir si nos nouvelles
      // features peuvent être compatible" Lottie): excluded from the
      // exported shapes, not just left in as a normal visible shape. This
      // exporter turns every STROKE into its own Lottie shape LAYER (no
      // per-Nemo-layer grouping), so there's no single Lottie layer a mask
      // could attach masksProperties to — real Lottie masking needs that
      // restructure, not done here. Excluding is still strictly better
      // than the prior behavior: without this, a mask exported as an
      // ordinary opaque shape in its EDIT-time color (masks are white-
      // filled only inside the live engine's own render, never in the
      // persisted stroke data) — a wrong, visible extra shape, worse than
      // an honestly-missing clip.
      var fStrokes=getEffectiveStrokesRendered(li,f).filter(function(sd){return sd.opacity!==0&&!sd.isRaster&&!sd.isRevisionGhost&&!sd.isMask&&(state.exportIncludeShadowGuides||sd.channelTag!=='shadow');});
      // Non-destructive combine groups (2026-07-29) — same post-process as
      // exportBuildFrame/buildSceneJson.
      if(window.SMGroup&&ld.groups)fStrokes=SMGroup.applyCombinesToStrokes(fStrokes,ld);
      framesStrokes[f]=fStrokes;
    }

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
          var layer=lottiePathLayer(ld.name+' / shape'+slot,runStrokes,runStart,runEnd,fps,camByFrame,bm,li);
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

// ---- Render Manager (batch queue) support — path-based "silent" exporters ----
// render-manager.js (2026-08-29, feedback #141: "un render manager un peu
// plus poussé... la possibilité d'envoyer différentes compositions, choisir
// différents format de sortie") calls one of these per QUEUED ITEM, in a
// loop — a native save/open dialog per item would be unusable in a batch
// (the queue's own "Path" field, set once when the item is configured, IS
// the destination). Same reasoning exportMP4Silent already established for
// Kitsu publish (2026-08, above). Each function here is a thin dialog-free
// sibling of an existing SMExport.* entry point below, built from the SAME
// internal helpers that entry point already uses (exportRenderPNGsToDir/
// exportRunFfmpeg/lottieBuild/exportWriteText/pad4…) — no export algorithm
// is duplicated, only the "ask the user where to save" step is replaced by
// a caller-supplied path/dir.
async function exportPNGSequenceToDir(dir,opts){
  if(!exportTauriAvailable())return{ok:false,error:'Disponible uniquement dans l\'app Nemo (pas en preview navigateur).'};
  var r=exportFrameRange(opts);var scale=(opts&&opts.scale)||1;
  await exportMkdir(dir);
  await exportRenderPNGsToDir(dir,r.start,r.end,scale,opts&&opts.onProgress,opts&&opts.alpha);
  return{ok:true,dir:dir};
}
async function exportTIFFSequenceToDir(outDir,opts){
  if(!exportTauriAvailable())return{ok:false,error:'Disponible uniquement dans l\'app Nemo (pas en preview navigateur).'};
  var r=exportFrameRange(opts);var scale=(opts&&opts.scale)||1;
  await exportMkdir(outDir);
  var tmp=exportTempDirPath?await exportTempDirPath():null;
  var workDir=(tmp||outDir)+'/sm-export-'+Date.now();
  await exportMkdir(workDir);
  await exportRenderPNGsToDir(workDir,r.start,r.end,scale,opts&&opts.onProgress);
  await exportRunFfmpeg(['-y','-start_number','1','-i',workDir+'/frame_%04d.png','-start_number','1',outDir+'/frame_%04d.tif'],opts&&opts.onFfmpeg);
  await exportRemoveDir(workDir);
  return{ok:true,dir:outDir};
}
async function exportSVGSequenceToDir(dir,opts){
  if(!exportTauriAvailable())return{ok:false,error:'Disponible uniquement dans l\'app Nemo (pas en preview navigateur).'};
  var r=exportFrameRange(opts);
  await exportMkdir(dir);
  for(var f=r.start,i=1;f<=r.end;f++,i++){
    await exportWriteText(dir+'/frame_'+pad4(i)+'.svg',exportFrameSVGString(f));
    if(opts&&opts.onProgress)opts.onProgress(i,r.end-r.start+1);
  }
  return{ok:true,dir:dir};
}
async function exportGIFToPath(outPath,opts){
  if(!exportTauriAvailable())return{ok:false,error:'Disponible uniquement dans l\'app Nemo (pas en preview navigateur) — voir exportGifBrowser.'};
  var r=exportFrameRange(opts);var scale=(opts&&opts.scale)||1;var fps=(opts&&opts.fps)||state.fps;
  var tmp=exportTempDirPath?await exportTempDirPath():null;
  var workDir=(tmp||outPath.replace(/[^/\\]+$/,''))+'sm-export-'+Date.now();
  await exportMkdir(workDir);
  await exportRenderPNGsToDir(workDir,r.start,r.end,scale,opts&&opts.onProgress);
  var palette=workDir+'/palette.png';
  await exportRunFfmpeg(['-y','-framerate',String(fps),'-i',workDir+'/frame_%04d.png','-vf','palettegen=stats_mode=diff',palette],opts&&opts.onFfmpeg);
  await exportRunFfmpeg(['-y','-framerate',String(fps),'-i',workDir+'/frame_%04d.png','-i',palette,'-lavfi','paletteuse=dither=bayer','-loop','0',outPath],opts&&opts.onFfmpeg);
  await exportRemoveDir(workDir);
  return{ok:true,path:outPath};
}
async function exportProResToPath(outPath,opts){
  if(!exportTauriAvailable())return{ok:false,error:'Disponible uniquement dans l\'app Nemo (pas en preview navigateur).'};
  var r=exportFrameRange(opts);var scale=(opts&&opts.scale)||1;var fps=(opts&&opts.fps)||state.fps;
  var alpha=!!(opts&&opts.alpha);
  var tmp=exportTempDirPath?await exportTempDirPath():null;
  var workDir=(tmp||outPath.replace(/[^/\\]+$/,''))+'sm-export-'+Date.now();
  await exportMkdir(workDir);
  await exportRenderPNGsToDir(workDir,r.start,r.end,scale,opts&&opts.onProgress,alpha);
  var vArgs=alpha
    ?['-c:v','prores_ks','-profile:v','4','-pix_fmt','yuva444p10le']
    :['-c:v','prores_ks','-profile:v','3','-pix_fmt','yuv422p10le'];
  await exportRunFfmpeg(['-y','-framerate',String(fps),'-i',workDir+'/frame_%04d.png'].concat(vArgs).concat([outPath]),opts&&opts.onFfmpeg);
  await exportRemoveDir(workDir);
  return{ok:true,path:outPath};
}
async function exportLottieToPath(outPath,opts){
  if(!exportTauriAvailable())return{ok:false,error:'Disponible uniquement dans l\'app Nemo (pas en preview navigateur) — voir exportLottie (repli navigateur).'};
  var r=exportFrameRange(opts);
  var json=lottieBuild(r.start,r.end);
  var text=JSON.stringify(json);
  await exportWriteText(outPath,text);
  return{ok:true,path:outPath,json:json};
}
// Render Quality (2026-08-29, feedback #141): VideoToolbox's -q:v knob (see
// CLAUDE.md §7 on why it replaced libx264's -crf) had exactly one hardcoded
// value (65) everywhere MP4 export happened. The render queue's own per-item
// "Render Quality" dropdown needs something real behind it — exposed as an
// optional opts.quality string here, defaulting to the SAME '65' every
// existing caller (single-shot Export modal, Kitsu publish) already got, so
// neither changes behavior by simply not passing it. No equivalent knob
// exists for GIF/ProRes/PNG/SVG/TIFF in this file (ProRes' quality is fixed
// by its profile, not a scalar; the others have no lossy encode step at
// all) — opts.quality is silently unused by those, not faked.
var MP4_QUALITY_QV={draft:'35',medium:'50',high:'65',best:'80'};

// ---- public exporters ----
window.SMExport={
  isAvailable:exportTauriAvailable,
  previewFrame:exportFrameDataURL,
  // Path-based batch variants (render-manager.js) — see the block above.
  exportPNGSequenceToDir:exportPNGSequenceToDir,
  exportTIFFSequenceToDir:exportTIFFSequenceToDir,
  exportSVGSequenceToDir:exportSVGSequenceToDir,
  exportGIFToPath:exportGIFToPath,
  exportProResToPath:exportProResToPath,
  exportLottieToPath:exportLottieToPath,
  pickDir:exportPickDir,
  pickSaveFile:exportPickSaveFile,
  pickVideoMimeType:function(){return typeof exportPickVideoMimeType==='function'?exportPickVideoMimeType():'';},

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
    if(!exportTauriAvailable())return exportGifBrowser(opts);
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
    if(!exportTauriAvailable())return exportVideoBrowser(opts);
    var outPath=await exportPickSaveFile('Exporter en MP4','animation.mp4',[{name:'MP4',extensions:['mp4']}]);
    if(!outPath)return{cancelled:true};
    return exportMP4ToPath(outPath,opts);
  },
  // Explicit entry point (2026-08-17) for a caller that wants the
  // MediaRecorder path specifically, regardless of Tauri availability —
  // exportMP4 above only reaches for it as a browser fallback.
  exportVideoBrowser:exportVideoBrowser,
  videoBrowserAvailable:exportVideoBrowserAvailable,
  exportGifBrowser:exportGifBrowser,
  gifBrowserAvailable:exportGifAvailable,
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
      var filename=(opts&&opts.filename)||'animation.json';
      var u=URL.createObjectURL(blob);var a=document.createElement('a');
      a.href=u;a.download=filename;a.click();URL.revokeObjectURL(u);
      return{ok:true,browserFallback:true,json:json,filename:filename};
    }
  },
};
