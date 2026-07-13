// ---- BITMAP IMPORT (single images or auto-detected numbered sequences) ----
(function(){
  function tauriOk(){return typeof window.__TAURI__!=='undefined';}
  function extOf(name){var m=/\.([^.]+)$/.exec(name);return m?m[1].toLowerCase():'';}
  // svg/avif added (2026-07, user request "gestion de tout format image") —
  // both decode natively via plain <img> in every Tauri webview target
  // (WebKit/macOS, WebView2/Windows), so no new decode code, just widening
  // the filter. heic/tiff deliberately excluded: WebKit has no reliable
  // native <img> decode for either (would silently fail via naturalSize's
  // onerror -> 1x1 fallback, worse than just not offering them).
  function mimeOf(ext){return{png:'image/png',jpg:'image/jpeg',jpeg:'image/jpeg',gif:'image/gif',webp:'image/webp',bmp:'image/bmp',svg:'image/svg+xml',avif:'image/avif'}[ext]||'image/png';}
  function baseName(path){var parts=path.split(/[\\/]/);return parts[parts.length-1];}

  function bytesToBase64(bytes){
    var binary='',chunk=0x8000;
    for(var i=0;i<bytes.length;i+=chunk)binary+=String.fromCharCode.apply(null,bytes.subarray(i,i+chunk));
    return btoa(binary);
  }
  async function readAsDataUrl(path){
    var bytes=await window.__TAURI__.fs.readFile(path);
    var ext=extOf(path);
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

  async function importSequence(items,prefix){
    showToast('Import de la séquence…');
    var frames=[];
    for(var i=0;i<items.length;i++){
      var dataUrl=await readAsDataUrl(items[i].path);
      var nat=await naturalSize(dataUrl);
      var fit=fitSize(nat.w,nat.h);
      frames.push({strokes:[{isRaster:true,src:dataUrl,x:state.canvasW/2,y:state.canvasH/2,width:fit.w,height:fit.h,opacity:1}],isKeyframe:true,isInterpolated:false});
    }
    saveAllLayerFrames();pushUndoLayers();
    if(frames.length>state.totalFrames)window.SM.setTotalFrames(frames.length);
    var idx=createUserLayer(prefix);
    while(frames.length<state.totalFrames)frames.push({strokes:[],isKeyframe:false,isInterpolated:false});
    state.layers[idx].frames=frames;
    activateUL(idx);loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
    if(window.SMMediaLibrary)SMMediaLibrary.addEntry(state.layers[idx].name,'image',frames[0].strokes[0].src,state.layers[idx].name);
    showToast('Séquence importée: '+items.length+' images sur le calque "'+prefix+'"');
  }

  async function importStandalone(paths){
    saveAllLayerFrames();pushUndoLayers();
    var ld=state.layers[state.activeLayerIdx];
    if(!ld.frames[state.currentFrame].isKeyframe&&!ld.frames[state.currentFrame].isInterpolated){
      ld.frames[state.currentFrame].strokes=JSON.parse(JSON.stringify(getEffectiveStrokes(state.activeLayerIdx,state.currentFrame)));
      ld.frames[state.currentFrame].isKeyframe=true;
    }
    for(var i=0;i<paths.length;i++){
      var dataUrl=await readAsDataUrl(paths[i]);
      var nat=await naturalSize(dataUrl);
      var fit=fitSize(nat.w,nat.h);
      var offset=i*24;
      ld.frames[state.currentFrame].strokes.push({isRaster:true,src:dataUrl,x:state.canvasW/2+offset,y:state.canvasH/2+offset,width:fit.w,height:fit.h,opacity:1});
      if(window.SMMediaLibrary)SMMediaLibrary.addEntry(baseName(paths[i]),'image',dataUrl,ld.name);
    }
    loadFrame(state.currentFrame);updateUI();
    showToast(paths.length>1?paths.length+' images importées':'Image importée');
  }

  async function importImages(){
    if(!tauriOk()){document.getElementById('image-input').click();return;}
    var paths=await window.__TAURI__.dialog.open({title:'Import Image(s)',multiple:true,filters:[{name:'Images',extensions:['png','jpg','jpeg','gif','webp','bmp','svg','avif']}]});
    if(!paths)return;
    paths=Array.isArray(paths)?paths:[paths];
    var seq=paths.length>=2?detectSequence(paths):{isSeq:false};
    if(seq.isSeq)await importSequence(seq.items.map(function(it){return{path:it.path};}),seq.prefix);
    else await importStandalone(paths);
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
      showToast('Import de la séquence…');
      var frames=[];
      for(var i=0;i<seq.items.length;i++){
        var file=files[names.indexOf(seq.items[i].path)];
        var dataUrl=await fileToDataUrl(file);
        var nat=await naturalSize(dataUrl);var fit=fitSize(nat.w,nat.h);
        frames.push({strokes:[{isRaster:true,src:dataUrl,x:state.canvasW/2,y:state.canvasH/2,width:fit.w,height:fit.h,opacity:1}],isKeyframe:true,isInterpolated:false});
      }
      saveAllLayerFrames();pushUndoLayers();
      if(frames.length>state.totalFrames)window.SM.setTotalFrames(frames.length);
      var idx=createUserLayer(seq.prefix);
      while(frames.length<state.totalFrames)frames.push({strokes:[],isKeyframe:false,isInterpolated:false});
      state.layers[idx].frames=frames;
      activateUL(idx);loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
      if(window.SMMediaLibrary)SMMediaLibrary.addEntry(state.layers[idx].name,'image',frames[0].strokes[0].src,state.layers[idx].name);
      showToast('Séquence importée: '+seq.items.length+' images');
    }else{
      saveAllLayerFrames();pushUndoLayers();
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
        if(window.SMMediaLibrary)SMMediaLibrary.addEntry(files[j].name,'image',du,ld.name);
      }
      loadFrame(state.currentFrame);updateUI();
      showToast(files.length>1?files.length+' images importées':'Image importée');
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
    saveAllLayerFrames();pushUndoLayers();
    if(frames.length>state.totalFrames)window.SM.setTotalFrames(frames.length);
    var idx=createUserLayer(prefix);
    while(frames.length<state.totalFrames)frames.push({strokes:[],isKeyframe:false,isInterpolated:false});
    state.layers[idx].frames=frames;
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
    if(window.SMMediaLibrary&&firstFrameThumb)SMMediaLibrary.addEntry(state.layers[idx].name,'video',firstFrameThumb,state.layers[idx].name);
    showToast('Vidéo importée : '+frames.filter(function(f){return f.strokes.length;}).length+' images sur le calque "'+prefix+'"');
  }
  async function importVideoFile(file){
    showToast('Décodage de la vidéo…');
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
    showToast('Décodage de la vidéo (ffmpeg)…');
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

  window.SM=window.SM||{};window.SM.importImages=importImages;window.SM.importVideo=importVideo;
  window.SM.importImageFiles=importImageFiles;window.SM.importVideoFile=importVideoFile;
})();
