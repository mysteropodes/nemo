// ---- BITMAP IMPORT (single images or auto-detected numbered sequences) ----
(function(){
  function tauriOk(){return typeof window.__TAURI__!=='undefined';}
  function extOf(name){var m=/\.([^.]+)$/.exec(name);return m?m[1].toLowerCase():'';}
  // svg/avif added (2026-07, user request "gestion de tout format image") —
  // both decode natively via plain <img> in every Tauri webview target
  // (WebKit/macOS, WebView2/Windows), so no new decode code, just widening
  // the filter. heic deliberately still excluded: the bundled ffmpeg sidecar
  // has no libheif compiled in (`ffmpeg -decoders | grep heic` — nothing),
  // so there's no decode path for it at all right now, native or otherwise.
  function mimeOf(ext){return{png:'image/png',jpg:'image/jpeg',jpeg:'image/jpeg',gif:'image/gif',webp:'image/webp',bmp:'image/bmp',svg:'image/svg+xml',avif:'image/avif'}[ext]||'image/png';}
  function baseName(path){var parts=path.split(/[\\/]/);return parts[parts.length-1];}

  function bytesToBase64(bytes){
    var binary='',chunk=0x8000;
    for(var i=0;i<bytes.length;i+=chunk)binary+=String.fromCharCode.apply(null,bytes.subarray(i,i+chunk));
    return btoa(binary);
  }
  // "Pro" still-image formats (2026-07, "formats image/vidéo élargis") —
  // WebKit's native <img> can't decode any of these, but the bundled ffmpeg
  // sidecar can (verified against the actual binary: `ffmpeg -decoders`
  // lists tiff/exr/psd/dpx). Same Tauri-only ffmpeg pattern already used
  // for video import (decodeVideoFramesFfmpeg below) and the rotoscopy
  // reference video path (reference-bridge.js) — a plain-browser preview
  // has no sidecar, so these stay unavailable there (image-input's
  // accept="image/*" is left as-is; an unsupported pick there just falls
  // through naturalSize's existing onerror->1x1 fallback, same as before).
  var PRO_IMAGE_EXTS={tiff:1,tif:1,exr:1,psd:1,dpx:1};
  async function decodeProImageFfmpeg(path){
    showToast(SM.t('toastDecodingImageFfmpeg'));
    var tmp=window.exportTempDirPath?await exportTempDirPath():null;
    var workDir=(tmp||'')+'sm-img-import-'+Date.now();
    await exportMkdir(workDir);
    try{
      // -pix_fmt rgba preserves alpha where the source has it (EXR/PSD both
      // commonly do) — harmless (alpha=255) for formats that don't.
      await exportRunFfmpeg(['-y','-i',path,'-frames:v','1','-pix_fmt','rgba',workDir+'/frame.png']);
      var bytes;
      try{bytes=await window.__TAURI__.fs.readFile(workDir+'/frame.png');}
      catch(e){throw new Error('ffmpeg n’a produit aucune image — format non décodable');}
      return 'data:image/png;base64,'+bytesToBase64(bytes);
    }finally{
      await exportRemoveDir(workDir);
    }
  }
  async function readAsDataUrl(path){
    var ext=extOf(path);
    if(PRO_IMAGE_EXTS[ext])return decodeProImageFfmpeg(path);
    var bytes=await window.__TAURI__.fs.readFile(path);
    return 'data:'+mimeOf(ext)+';base64,'+bytesToBase64(bytes);
  }
  function naturalSize(dataUrl){
    return new Promise(function(resolve){
      var img=new Image();
      img.onload=function(){resolve({w:img.naturalWidth||1,h:img.naturalHeight||1});};
      img.onerror=function(){resolve({w:1,h:1});};
      img.src=dataUrl;
    });
  }
  // Fit within the document bounds (never upscale) so a dropped-in photo
  // doesn't dwarf the canvas — the user can still scale it up manually.
  function fitSize(w,h){
    var maxW=state.canvasW,maxH=state.canvasH;
    var s=Math.min(1,maxW/w,maxH/h);
    return{w:w*s,h:h*s};
  }
  // Small JPEG preview for a LINKED image's media-library entry (2026-08-29,
  // linked-media.js) — same 96px-wide idea as native-video-bridge.js's own
  // video thumbnail. Re-embedding the FULL dataUrl as `thumb` (the embedded-
  // mode convention, media-library.js's own header comment) would silently
  // put the exact weight linking is meant to avoid right back into the
  // project JSON via mediaLibrary's own persisted `thumb` field.
  function makeSmallThumb(dataUrl,natW,natH){
    return new Promise(function(resolve){
      var img=new Image();
      img.onload=function(){
        try{
          var tw=96,th=Math.max(1,Math.round(96*(natH||img.naturalHeight||1)/(natW||img.naturalWidth||1)));
          var c=document.createElement('canvas');c.width=tw;c.height=th;
          c.getContext('2d').drawImage(img,0,0,tw,th);
          resolve(c.toDataURL('image/jpeg',0.7));
        }catch(e){resolve(null);}
      };
      img.onerror=function(){resolve(null);};
      img.src=dataUrl;
    });
  }
  // Linked-mode gate (2026-08-29) — desktop only checks the project setting
  // (Tauri always has a real filesystem path); the web dispatch lives in
  // importImages()/importImageFiles() below since it needs a completely
  // different picker (showOpenFilePicker, not <input type=file>).
  function isLinkedMode(){return state.mediaMode==='linked';}

  // Detects a numeric image sequence: same prefix/extension, digits differ.
  // Returns {isSeq, items:[{path,num}] sorted, prefix} — a "sequence" needs
  // at least 2 files sharing the same prefix+ext, and to cover every file
  // that was selected (mixing a sequence with unrelated stray images falls
  // back to plain multi-import instead of guessing).
  function detectSequence(paths){
    var re=/^(.*?)(\d+)(\.[^.]+)$/;
    var buckets={};
    paths.forEach(function(p){
      var name=baseName(p);var m=re.exec(name);
      if(!m)return;
      var key=m[1]+'||'+m[3].toLowerCase();
      (buckets[key]=buckets[key]||[]).push({path:p,num:parseInt(m[2],10),prefix:m[1]});
    });
    var bestKey=null;
    Object.keys(buckets).forEach(function(k){if(buckets[k].length>=2&&(!bestKey||buckets[k].length>buckets[bestKey].length))bestKey=k;});
    if(bestKey&&buckets[bestKey].length===paths.length){
      var items=buckets[bestKey].slice().sort(function(a,b){return a.num-b.num;});
      var prefix=items[0].prefix.replace(/[\s_\-]+$/,'')||'Sequence';
      return{isSeq:true,items:items,prefix:prefix};
    }
    return{isSeq:false};
  }

  // Prompt Frame Rate (2026-08, AE feature audit 8.3, "prevent mismatch
  // with project settings") — asks what rate the SEQUENCE was captured at
  // before importing it, defaulting to the project's own fps (so hitting
  // Enter with no change reproduces the old silent 1-image-per-project-
  // frame behavior exactly). A native prompt() — same pattern bpm-grid.js
  // already uses for a single numeric value, not worth a bespoke modal for
  // one field. Returns a positive number, or null if cancelled/invalid
  // (caller treats null as "same as project fps", i.e. a no-op).
  function promptSequenceFps(){
    var v=window.prompt('Fréquence de la séquence (images par seconde) ? Laisser tel quel = '+state.fps+' fps (fréquence du projet).',String(state.fps));
    if(v===null)return null;
    var n=parseFloat(v);
    return(n>0&&isFinite(n))?n:null;
  }
  // Repeats each sequence entry to approximate the requested playback
  // speed relative to the project's own fps — e.g. a 12fps sequence in a
  // 24fps project shows each source image for 2 project frames. Simple
  // nearest-integer repeat rather than true resampling (no dropped-frame
  // case for seqFps > project fps beyond rounding to 1) — matches AE's own
  // "prevent mismatch" framing (get the DURATION right) without building a
  // full retiming engine for what's fundamentally an import-time nicety.
  function seqRepeatCount(seqFps){
    if(!seqFps||seqFps===state.fps)return 1;
    return Math.max(1,Math.round(state.fps/seqFps));
  }
  async function importSequence(items,prefix,seqFps){
    showToast(SM.t('toastImportingSequence'));
    var rep=seqRepeatCount(seqFps);
    var linked=isLinkedMode();
    var frames=[];
    var firstDataUrl=null,firstNat=null;
    for(var i=0;i<items.length;i++){
      var dataUrl=await readAsDataUrl(items[i].path);
      var nat=await naturalSize(dataUrl);
      var fit=fitSize(nat.w,nat.h);
      if(i===0){firstDataUrl=dataUrl;firstNat=nat;}
      // Linked mode: each numbered file gets its OWN linkedPath (unlike
      // importStandalone's single still, a sequence is genuinely a
      // different image per frame-group) — see importStandalone's own
      // comment for why `src` is never written here.
      var stroke=linked
        ?{isRaster:true,linked:true,linkedPath:items[i].path,x:state.canvasW/2,y:state.canvasH/2,width:fit.w,height:fit.h,opacity:1}
        :{isRaster:true,src:dataUrl,x:state.canvasW/2,y:state.canvasH/2,width:fit.w,height:fit.h,opacity:1};
      if(linked&&window.SMLinkedMedia)SMLinkedMedia.primeCache({linkedPath:items[i].path},dataUrl);
      for(var r=0;r<rep;r++)frames.push({strokes:[stroke],isKeyframe:r===0,isInterpolated:r!==0});
    }
    saveAllLayerFrames();pushUndoLayers(true);
    if(frames.length>state.totalFrames)window.SM.setTotalFrames(frames.length);
    var idx=createUserLayer(prefix);
    while(frames.length<state.totalFrames)frames.push({strokes:[],isKeyframe:false,isInterpolated:false});
    state.layers[idx].frames=frames;
    // Say what this layer IS, rather than leaving every reader to infer it
    // from its contents (see layer-kind.js). Purely descriptive — it
    // changes no stroke and no renderer reads it — but it is what lets
    // the timeline label a sequence as a sequence instead of guessing.
    state.layers[idx].footage={kind:'sequence',count:frames.length};
    activateUL(idx);loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
    if(linked){
      var seqThumb=await makeSmallThumb(firstDataUrl,firstNat.w,firstNat.h);
      if(window.SMMediaLibrary)SMMediaLibrary.addEntry(state.layers[idx].name,'image',seqThumb,state.layers[idx].name,{layerUid:state.layers[idx].layerUid,linked:true,path:items[0].path,naturalW:firstNat.w,naturalH:firstNat.h});
    }else if(window.SMMediaLibrary)SMMediaLibrary.addEntry(state.layers[idx].name,'image',frames[0].strokes[0].src,state.layers[idx].name,{layerUid:state.layers[idx].layerUid});
    showToast(SM.t('toastSequenceImportedSuffix')+items.length+' images sur le calque "'+prefix+'"');
  }

  // A still used to be pushed onto whatever layer happened to be active,
  // which made it the only importer that did NOT produce a layer — a video
  // and a sequence both create one, so an image was the odd case with no
  // type, no source to inspect and nothing to replace (2026-07-27 audit).
  // One layer per image now, tagged as footage, same as its siblings; the
  // layer is the thing you then move, key and swap.
  async function importStandalone(paths){
    saveAllLayerFrames();pushUndoLayers(true);
    var linked=isLinkedMode();
    for(var i=0;i<paths.length;i++){
      var dataUrl=await readAsDataUrl(paths[i]);
      var nat=await naturalSize(dataUrl);
      var fit=fitSize(nat.w,nat.h);
      var nm=baseName(paths[i]);
      var idx=createUserLayer(nm);
      var ldN=state.layers[idx];
      // Present on EVERY frame, not just the current one: a still is a
      // still for the layer's whole length, and a one-frame raster would
      // vanish the moment the playhead moved.
      //
      // Linked mode (2026-08-29, linked-media.js): the stroke NEVER carries
      // `src` here — only linked:true + linkedPath. Writing the full
      // dataUrl into every frame's literal (like the embedded branch below)
      // would defeat linking entirely: saveAllLayerFrames() only ever
      // re-serializes the CURRENT frame through serR (app.js) — every OTHER
      // frame's dict is whatever THIS loop wrote, verbatim, forever. desR
      // resolves the actual pixels back from disk at render time (cached).
      for(var f=0;f<ldN.frames.length;f++){
        ldN.frames[f].strokes=linked
          ?[{isRaster:true,linked:true,linkedPath:paths[i],x:state.canvasW/2,y:state.canvasH/2,width:fit.w,height:fit.h,opacity:1}]
          :[{isRaster:true,src:dataUrl,x:state.canvasW/2,y:state.canvasH/2,width:fit.w,height:fit.h,opacity:1}];
        ldN.frames[f].isKeyframe=(f===0);
        ldN.frames[f].isInterpolated=(f!==0);
      }
      ldN.footage={kind:'image',name:nm,w:nat.w,h:nat.h};
      activateUL(idx);
      if(linked){
        if(window.SMLinkedMedia)SMLinkedMedia.primeCache({linkedPath:paths[i]},dataUrl); // instant first render, no resolve round-trip
        var smallThumb=await makeSmallThumb(dataUrl,nat.w,nat.h);
        if(window.SMMediaLibrary)SMMediaLibrary.addEntry(nm,'image',smallThumb,ldN.name,{layerUid:ldN.layerUid,linked:true,path:paths[i],naturalW:nat.w,naturalH:nat.h});
      }else if(window.SMMediaLibrary)SMMediaLibrary.addEntry(nm,'image',dataUrl,ldN.name,{layerUid:ldN.layerUid});
    }
    loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
    showToast(paths.length>1?paths.length+SM.t('toastImagesImportedOwnLayersSuffix'):SM.t('toastImageImportedOwnLayer'));
  }

  async function importImages(){
    if(!tauriOk()){
      // Web + linked mode (2026-08-29, linked-media.js): the classic
      // <input type=file> below gives back plain File objects with no
      // durable handle — nothing a reload could re-open. showOpenFilePicker
      // is the ONLY web API that returns a FileSystemFileHandle worth
      // stashing for later, so linked mode routes through it instead, when
      // the browser supports it (Chromium only — Safari/Firefox fall
      // through to the same <input> flow as embedded mode, degrading
      // gracefully rather than silently doing nothing).
      if(state.mediaMode==='linked'&&window.SMLinkedMedia&&SMLinkedMedia.isWebLinkingSupported()){
        await importStandaloneWebLinked();
        return;
      }
      document.getElementById('image-input').click();return;
    }
    var paths=await window.__TAURI__.dialog.open({title:'Import Image(s)',multiple:true,filters:[
      {name:'Images',extensions:['png','jpg','jpeg','gif','webp','bmp','svg','avif']},
      {name:'Images pro (via ffmpeg)',extensions:Object.keys(PRO_IMAGE_EXTS)},
      {name:'Tous les fichiers',extensions:['*']},
    ]});
    if(!paths)return;
    paths=Array.isArray(paths)?paths:[paths];
    var seq=paths.length>=2?detectSequence(paths):{isSeq:false};
    if(seq.isSeq)await importSequence(seq.items.map(function(it){return{path:it.path};}),seq.prefix,promptSequenceFps());
    else await importStandalone(paths);
  }

  // Web-linked standalone import (2026-08-29, linked-media.js) — the
  // File System Access counterpart to importStandalone's desktop dialog
  // path above. One layer per picked file, same "present on every frame"
  // shape, just keyed by webHandleId instead of a filesystem path. No
  // sequence-detection equivalent here (scoped out — a multi-file pick
  // lands as N standalone linked layers instead of one sequence layer;
  // documented limitation, not a crash, see the PR description).
  async function importStandaloneWebLinked(){
    var picked;
    try{picked=await SMLinkedMedia.pickWebImages(true);}
    catch(e){if(window.showToast)showToast(String((e&&e.message)||e),'warn');return;}
    if(!picked||!picked.length)return;
    saveAllLayerFrames();pushUndoLayers(true);
    for(var i=0;i<picked.length;i++){
      var p=picked[i];
      var nm=p.name.replace(/\.[^.]+$/,'');
      var fit=fitSize(p.naturalW,p.naturalH);
      var idx=createUserLayer(nm);
      var ldN=state.layers[idx];
      for(var f=0;f<ldN.frames.length;f++){
        ldN.frames[f].strokes=[{isRaster:true,linked:true,linkedHandleId:p.webHandleId,x:state.canvasW/2,y:state.canvasH/2,width:fit.w,height:fit.h,opacity:1}];
        ldN.frames[f].isKeyframe=(f===0);
        ldN.frames[f].isInterpolated=(f!==0);
      }
      ldN.footage={kind:'image',name:nm,w:p.naturalW,h:p.naturalH};
      activateUL(idx);
      var smallThumb=await makeSmallThumb(p.dataUrl,p.naturalW,p.naturalH);
      if(window.SMMediaLibrary)SMMediaLibrary.addEntry(nm,'image',smallThumb,ldN.name,{layerUid:ldN.layerUid,linked:true,webHandleId:p.webHandleId,naturalW:p.naturalW,naturalH:p.naturalH});
    }
    loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
    showToast(picked.length>1?picked.length+SM.t('toastImagesImportedOwnLayersSuffix'):SM.t('toastImageImportedOwnLayer'));
  }

  // Legacy fallback for plain-browser testing (no Tauri fs/dialog): the
  // <input type=file multiple> can't give real filesystem paths, so
  // sequence auto-detection uses the files' own names instead, reading
  // each via FileReader rather than Tauri's fs plugin.
  function fileToDataUrl(file){
    return new Promise(function(resolve){var r=new FileReader();r.onload=function(){resolve(r.result);};r.readAsDataURL(file);});
  }
  // Factored out of the <input> change handler (below) so media-library.js's
  // OS drag-and-drop zone — which only ever has real `File` objects, never
  // filesystem paths — can feed the exact same import logic instead of
  // duplicating it a third time (Tauri-path importImages/importStandalone
  // above is the second copy; CLAUDE.md's "avoid duplicating a whole
  // pipeline" applies here even though this codebase already tolerates a
  // couple of small JS/Rust math pairs staying separate).
  async function importImageFiles(files){
    if(!files.length)return;
    var names=files.map(function(f){return f.name;});
    var seq=files.length>=2?detectSequence(names):{isSeq:false};
    if(seq.isSeq){
      var seqFps=promptSequenceFps(),rep=seqRepeatCount(seqFps);
      showToast(SM.t('toastImportingSequence'));
      var frames=[];
      for(var i=0;i<seq.items.length;i++){
        var file=files[names.indexOf(seq.items[i].path)];
        var dataUrl=await fileToDataUrl(file);
        var nat=await naturalSize(dataUrl);var fit=fitSize(nat.w,nat.h);
        for(var r=0;r<rep;r++)frames.push({strokes:[{isRaster:true,src:dataUrl,x:state.canvasW/2,y:state.canvasH/2,width:fit.w,height:fit.h,opacity:1}],isKeyframe:r===0,isInterpolated:r!==0});
      }
      saveAllLayerFrames();pushUndoLayers(true);
      if(frames.length>state.totalFrames)window.SM.setTotalFrames(frames.length);
      var idx=createUserLayer(seq.prefix);
      while(frames.length<state.totalFrames)frames.push({strokes:[],isKeyframe:false,isInterpolated:false});
      state.layers[idx].frames=frames;
      state.layers[idx].footage={kind:'sequence',count:frames.length};
      activateUL(idx);loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
      if(window.SMMediaLibrary)SMMediaLibrary.addEntry(state.layers[idx].name,'image',frames[0].strokes[0].src,state.layers[idx].name,{layerUid:state.layers[idx].layerUid});
      showToast(SM.t('toastSequenceImportedSuffix')+seq.items.length+' images');
    }else{
      saveAllLayerFrames();pushUndoLayers(true);
      var ld=state.layers[state.activeLayerIdx];
      if(!ld.frames[state.currentFrame].isKeyframe&&!ld.frames[state.currentFrame].isInterpolated){
        ld.frames[state.currentFrame].strokes=JSON.parse(JSON.stringify(getEffectiveStrokes(state.activeLayerIdx,state.currentFrame)));
        ld.frames[state.currentFrame].isKeyframe=true;
      }
      for(var j=0;j<files.length;j++){
        var du=await fileToDataUrl(files[j]);
        var n=await naturalSize(du);var ft=fitSize(n.w,n.h);
        var off=j*24;
        ld.frames[state.currentFrame].strokes.push({isRaster:true,src:du,x:state.canvasW/2+off,y:state.canvasH/2+off,width:ft.w,height:ft.h,opacity:1});
        if(window.SMMediaLibrary)SMMediaLibrary.addEntry(files[j].name,'image',du,ld.name,{layerUid:ld.layerUid});
      }
      loadFrame(state.currentFrame);updateUI();
      showToast(files.length>1?files.length+SM.t('toastImagesImportedSuffix'):SM.t('toastImageImported'));
    }
  }
  document.getElementById('image-input').addEventListener('change',function(e){
    var files=Array.prototype.slice.call(e.target.files);e.target.value='';
    importImageFiles(files);
  });

  document.getElementById('btn-import-img').addEventListener('click',importImages);

  // ---- VIDEO IMPORT (v14) — decodes to a per-frame image sequence and
  // feeds the EXACT SAME isRaster-per-frame layer pipeline as importSequence
  // above, instead of inventing a new "video layer" item type. Deliberately
  // NOT a new consumer-of-layer.children type (see CLAUDE.md §1 — every new
  // item/tag on layer.children has to be re-verified against ~7 separate
  // consumers, which is the real cost of a from-scratch video layer, not
  // the UI work) — a video import becomes indistinguishable from an
  // imported image sequence the moment the frames exist, so it's already
  // transformable/scalable/opacity-capable via the same Raster+layer path
  // images already use, with zero new code needed anywhere else.
  function seekTo(video,t){
    return new Promise(function(resolve){
      function onSeeked(){video.removeEventListener('seeked',onSeeked);resolve();}
      video.addEventListener('seeked',onSeeked);
      video.currentTime=t;
    });
  }
  async function decodeVideoFrames(blobUrl){
    var video=document.createElement('video');
    video.src=blobUrl;video.muted=true;video.playsInline=true;video.preload='auto';
    await new Promise(function(resolve,reject){
      video.addEventListener('loadedmetadata',resolve,{once:true});
      video.addEventListener('error',function(){reject(new Error('video decode failed'));},{once:true});
    });
    var duration=video.duration||0;
    var fps=state.fps;
    // Capped at the same 999 ceiling proj-frames itself allows — a longer
    // clip just gets sampled at the project's own fps up to that many
    // frames rather than failing outright.
    var frameCount=Math.max(1,Math.min(999,Math.round(duration*fps)));
    var fit=fitSize(video.videoWidth||1,video.videoHeight||1);
    var canvas=document.createElement('canvas');
    canvas.width=video.videoWidth||1;canvas.height=video.videoHeight||1;
    var ctx=canvas.getContext('2d');
    var frames=[];
    for(var i=0;i<frameCount;i++){
      await seekTo(video,Math.min(i/fps,Math.max(0,duration-0.001)));
      ctx.drawImage(video,0,0,canvas.width,canvas.height);
      // JPEG, not PNG — a few hundred full-frame PNGs would bloat the
      // project JSON far more than this session's frame counts warrant;
      // 0.85 quality is visually clean enough for a rotoscope/reference
      // source layer.
      var dataUrl=canvas.toDataURL('image/jpeg',0.85);
      frames.push({strokes:[{isRaster:true,src:dataUrl,x:state.canvasW/2,y:state.canvasH/2,width:fit.w,height:fit.h,opacity:1}],isKeyframe:true,isInterpolated:false});
    }
    return frames;
  }
  async function importVideoFrames(frames,prefix){
    saveAllLayerFrames();pushUndoLayers(true);
    if(frames.length>state.totalFrames)window.SM.setTotalFrames(frames.length);
    var idx=createUserLayer(prefix);
    while(frames.length<state.totalFrames)frames.push({strokes:[],isKeyframe:false,isInterpolated:false});
    state.layers[idx].frames=frames;
    state.layers[idx].footage={kind:'sequence',from:'video',count:frames.length};
    activateUL(idx);
    // convertLayerToComponent() below calls saveAllLayerFrames(), which
    // re-serializes the CURRENT frame from the live Paper.js document for
    // every layer — loadFrame() must run first so that live document
    // actually reflects the frame data just assigned above, or the current
    // frame's raster gets clobbered with whatever userLayers[idx] happened
    // to hold before (empty, since this layer was only just created).
    loadFrame(state.currentFrame);
    // desR() (app.js) sizes a Raster asynchronously in its own onLoad —
    // r.bounds is 0x0 until that fires. convertLayerToComponent() below
    // calls saveAllLayerFrames() synchronously right after this, which would
    // otherwise serialize the still-unsized raster (serR reads r.bounds),
    // baking a 0x0 component permanently into frame 0. Wait for the actual
    // decoded size before proceeding; the 1.5s fallback is a safety net in
    // case some raster never fires 'load' (shouldn't happen for a data URL,
    // but a silently-stuck import is worse than a rare redundant wait).
    await new Promise(function(resolve){
      var r=userLayers[idx].children.filter(function(c){return c instanceof Raster;})[0];
      if(!r){resolve();return;}
      var done=false;
      r.on('load',function(){if(!done){done=true;resolve();}});
      setTimeout(function(){if(!done){done=true;resolve();}},1500);
    });
    // Each decoded frame is its own independent baked raster at a fixed x/y/
    // width/height — "transformable via the same Raster+layer path images
    // use" (see this file's top comment) was true in the narrow sense that
    // a Raster IS selectable/draggable, but dragging or resizing it only
    // ever touched THAT ONE frame's copy; every other frame kept the
    // original centered placement, so the clip as a whole read as "stuck"
    // (had to redo the transform by hand on every single frame). Wrapping
    // the fresh layer as a component reuses the SAME symbolId/symMatrix
    // pipeline every other component instance already goes through (single
    // shared affine applied uniformly across all frames on move/scale/
    // rotate, getEffectiveStrokes' ld.symbolId branch, export.js, etc.) —
    // the video becomes one rigid, uniformly transformable object exactly
    // like the user expects "comme un symbole", with no new per-item type
    // or extra consumer to keep in sync (CLAUDE.md §1's actual cost of a
    // bespoke video-layer type, avoided entirely).
    var firstFrameThumb=(frames.filter(function(f){return f.strokes.length;})[0]||{}).strokes;
    firstFrameThumb=firstFrameThumb&&firstFrameThumb[0]&&firstFrameThumb[0].src;
    convertLayerToComponent(idx);
    if(window.SMMediaLibrary&&firstFrameThumb)SMMediaLibrary.addEntry(state.layers[idx].name,'video',firstFrameThumb,state.layers[idx].name,{layerUid:state.layers[idx].layerUid});
    showToast(SM.t('toastVideoImportedSuffix')+frames.filter(function(f){return f.strokes.length;}).length+' images sur le calque "'+prefix+'"');
  }
  async function importVideoFile(file){
    // WebCodecs instant-import path (2026-07, browser-only — the Tauri
    // path above this function is untouched): same "instant import + live
    // scrub" architecture as native-video-bridge.js's Tauri backend, just
    // demuxed/decoded via MP4Box.js + VideoDecoder instead of a piped
    // ffmpeg subprocess. Falls through to the old bake-every-frame-as-JPEG
    // importer below for anything it can't handle (unsupported container/
    // codec, or a browser without WebCodecs — e.g. older Firefox).
    if(window.SMNativeVideo){
      try{await SMNativeVideo.importAsLayer(file);return;}
      catch(e){showToast(SM.t('toastWebCodecsUnavailable')+(e&&e.message||e)+') — import classique…');}
    }
    showToast(SM.t('toastDecodingVideo'));
    var blobUrl=URL.createObjectURL(file);
    try{
      var frames=await decodeVideoFrames(blobUrl);
      await importVideoFrames(frames,file.name.replace(/\.[^.]+$/,''));
    }finally{URL.revokeObjectURL(blobUrl);}
  }
  async function importVideo(){
    if(!tauriOk()){document.getElementById('video-input').click();return;}
    // Broad container/codec list (2026-07) — under Tauri, decode goes
    // through the bundled ffmpeg sidecar (see decodeVideoFramesFfmpeg
    // below), not the webview's <video> element, so the format ceiling is
    // "whatever this ffmpeg build supports" (effectively everything common)
    // rather than "whatever WebKit/WebView2 happen to decode natively" —
    // mkv/avi/flv/wmv/etc. that silently failed before now just work. The
    // dialog filter below is a convenience default, not an enforced
    // whitelist — "All files" is offered too since ffmpeg itself is the
    // real gate, not this list.
    var path=await window.__TAURI__.dialog.open({title:'Import Video',multiple:false,filters:[
      {name:'Videos',extensions:['mp4','mov','webm','m4v','ogv','mkv','avi','flv','wmv','mts','m2ts','3gp','mpg','mpeg','vob','ts','webp']},
      {name:'All files',extensions:['*']},
    ]});
    if(!path)return;
    // EXPERIMENTAL (native-video-decode branch): instant import via a live
    // native decode session (SMNativeVideo.importAsLayer) instead of baking
    // the whole file to per-frame JPEGs — the JPEG path below remains the
    // fallback if the native decoder can't open this particular file
    // (odd container, or a build without the ffmpeg libs linked).
    if(window.SMNativeVideo){
      try{await SMNativeVideo.importAsLayer(path);return;}
      catch(e){showToast(SM.t('toastNativeDecoderUnavailable')+(e&&e.message||e)+') — import classique…');}
    }
    var frames=await decodeVideoFramesFfmpeg(path);
    await importVideoFrames(frames,baseName(path).replace(/\.[^.]+$/,''));
  }
  // ffmpeg-sidecar decode (Tauri-only — the sidecar isn't available in the
  // plain-browser preview, which keeps using decodeVideoFrames/<video>
  // above for that path). Reuses export.js's own temp-dir/ffmpeg-invoke
  // helpers (exportTempDirPath/exportMkdir/exportRemoveDir/exportRunFfmpeg —
  // plain top-level globals, not IIFE-scoped, already used for the export
  // side of the exact same sidecar) rather than duplicating that plumbing.
  async function decodeVideoFramesFfmpeg(path){
    showToast(SM.t('toastDecodingVideoFfmpeg'));
    var tmp=window.exportTempDirPath?await exportTempDirPath():null;
    var workDir=(tmp||'')+'sm-video-import-'+Date.now();
    await exportMkdir(workDir);
    try{
      // Capped at the same 999-frame ceiling the <video>-based decoder
      // already uses (decodeVideoFrames above) — -frames:v enforces it
      // directly in ffmpeg rather than post-hoc, so a long clip doesn't
      // spend minutes decoding frames that would just get discarded.
      await exportRunFfmpeg(['-y','-i',path,'-vf','fps='+Math.max(1,state.fps),'-frames:v','999','-q:v','3',workDir+'/frame_%04d.jpg']);
      var frames=[];
      var fit=null;
      for(var i=1;i<=999;i++){
        var framePath=workDir+'/frame_'+String(i).padStart(4,'0')+'.jpg';
        var bytes;
        try{bytes=await window.__TAURI__.fs.readFile(framePath);}catch(e){break;} // no more frames — ffmpeg stopped here
        var dataUrl='data:image/jpeg;base64,'+bytesToBase64(bytes);
        if(!fit){var nat=await naturalSize(dataUrl);fit=fitSize(nat.w,nat.h);}
        frames.push({strokes:[{isRaster:true,src:dataUrl,x:state.canvasW/2,y:state.canvasH/2,width:fit.w,height:fit.h,opacity:1}],isKeyframe:true,isInterpolated:false});
      }
      if(!frames.length)throw new Error('ffmpeg n’a produit aucune image — format non décodable');
      return frames;
    }finally{
      await exportRemoveDir(workDir);
    }
  }
  document.getElementById('video-input')&&document.getElementById('video-input').addEventListener('change',function(e){
    var file=e.target.files[0];e.target.value='';if(!file)return;
    importVideoFile(file);
  });
  document.getElementById('btn-import-video')&&document.getElementById('btn-import-video').addEventListener('click',importVideo);

  // Swap a footage layer's image without touching the layer: its transform,
  // its Motion keys, its in/out range and its place in the stack all stay,
  // only the pixels change. Rewrites every raster on the layer, so a still
  // (same image on all frames) and a sequence-turned-still both land right.
  async function replaceFootageSource(){
    var li=state.activeLayerIdx, ld=state.layers[li];
    if(!ld)return;
    var dataUrl=null,nm=null;
    if(tauriOk()){
      var path=await window.__TAURI__.dialog.open({title:'Remplacer la source',multiple:false,
        filters:[{name:'Images',extensions:['png','jpg','jpeg','webp','gif','tif','tiff','exr','psd','dpx','bmp']}]});
      if(!path)return;
      dataUrl=await readAsDataUrl(path);nm=baseName(path);
    }else{
      dataUrl=await new Promise(function(res){
        var inp=document.createElement('input');inp.type='file';inp.accept='image/*';inp.style.display='none';
        document.body.appendChild(inp);
        inp.addEventListener('change',function(e){
          var f=e.target.files&&e.target.files[0];inp.remove();
          if(!f){res(null);return;}
          nm=f.name;var r=new FileReader();r.onload=function(){res(r.result);};r.readAsDataURL(f);
        });
        inp.click();
      });
      if(!dataUrl)return;
    }
    var nat=await naturalSize(dataUrl);
    saveAllLayerFrames();pushUndoLayers(true);
    var n=0;
    (ld.frames||[]).forEach(function(f){
      (f&&f.strokes||[]).forEach(function(st){
        if(!st||!st.isRaster)return;
        // Keep the placement the user gave it — only the pixels and the
        // aspect change, so a footage layer already positioned in a shot
        // does not jump when its source is swapped.
        var ratio=nat.h?nat.w/nat.h:1;
        st.src=dataUrl;
        st.height=st.width/ratio;
        n++;
      });
    });
    if(ld.footage){ld.footage.name=nm||ld.footage.name;ld.footage.w=nat.w;ld.footage.h=nat.h;}
    loadFrame(state.currentFrame);updateUI();
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
    showToast(n?SM.t('toastSourceReplacedSuffix')+n+' image(s))':SM.t('toastNoImageToReplace'));
  }
  // A video layer's source is a decode session (ld.nativeVideo), not a
  // per-frame raster src — dispatched to native-video-bridge.js's own
  // relink flow instead of this file's replaceFootageSource, which only
  // knows how to swap raster bytes (2026-07-31, updateFootagePanel sets
  // data-kind on this same button each time the panel refreshes).
  document.getElementById('btn-footage-replace')&&document.getElementById('btn-footage-replace').addEventListener('click',function(){
    if(this.dataset.kind==='video'){if(window.SMNativeVideo)SMNativeVideo.replaceNativeVideoSource(state.activeLayerIdx);return;}
    replaceFootageSource();
  });

  window.SM=window.SM||{};window.SM.importImages=importImages;window.SM.importVideo=importVideo;
  window.SM.replaceFootageSource=replaceFootageSource;
  window.SM.importImageFiles=importImageFiles;window.SM.importVideoFile=importVideoFile;
  // Exposed for linked-media.js's desktop resolve path (2026-08-29) — reuses
  // the SAME decode (incl. the PRO_IMAGE_EXTS ffmpeg conversion above)
  // instead of a second, narrower reimplementation that would silently
  // mis-decode a linked .psd/.tiff/.exr/.dpx on every resolve after the
  // in-memory cache is cold (a fresh session, or after eviction) — the
  // ONE-TIME import-time read already went through this same function, so
  // reusing it keeps both reads in phase (CLAUDE.md §3's "duplicated pair
  // must stay identical" applies just as much to two calls to ONE function
  // as to two hand-written copies).
  window.SM._readImageAsDataUrl=readAsDataUrl;
})();
