// ---- BITMAP IMPORT (single images or auto-detected numbered sequences) ----
(function(){
  function tauriOk(){return typeof window.__TAURI__!=='undefined';}
  function extOf(name){var m=/\.([^.]+)$/.exec(name);return m?m[1].toLowerCase():'';}
  function mimeOf(ext){return{png:'image/png',jpg:'image/jpeg',jpeg:'image/jpeg',gif:'image/gif',webp:'image/webp',bmp:'image/bmp'}[ext]||'image/png';}
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
    }
    loadFrame(state.currentFrame);updateUI();
    showToast(paths.length>1?paths.length+' images importées':'Image importée');
  }

  async function importImages(){
    if(!tauriOk()){document.getElementById('image-input').click();return;}
    var paths=await window.__TAURI__.dialog.open({title:'Import Image(s)',multiple:true,filters:[{name:'Images',extensions:['png','jpg','jpeg','gif','webp','bmp']}]});
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
  document.getElementById('image-input').addEventListener('change',async function(e){
    var files=Array.prototype.slice.call(e.target.files);e.target.value='';
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
      }
      loadFrame(state.currentFrame);updateUI();
      showToast(files.length>1?files.length+' images importées':'Image importée');
    }
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
    activateUL(idx);loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
    showToast('Vidéo importée : '+frames.filter(function(f){return f.strokes.length;}).length+' images sur le calque "'+prefix+'"');
  }
  async function importVideo(){
    if(!tauriOk()){document.getElementById('video-input').click();return;}
    var path=await window.__TAURI__.dialog.open({title:'Import Video',multiple:false,filters:[{name:'Videos',extensions:['mp4','mov','webm','m4v']}]});
    if(!path)return;
    showToast('Décodage de la vidéo…');
    var bytes=await window.__TAURI__.fs.readFile(path);
    var ext=extOf(path);
    var mime={mp4:'video/mp4',mov:'video/quicktime',webm:'video/webm',m4v:'video/mp4'}[ext]||'video/mp4';
    var blobUrl=URL.createObjectURL(new Blob([bytes],{type:mime}));
    try{
      var frames=await decodeVideoFrames(blobUrl);
      await importVideoFrames(frames,baseName(path).replace(/\.[^.]+$/,''));
    }finally{URL.revokeObjectURL(blobUrl);}
  }
  document.getElementById('video-input')&&document.getElementById('video-input').addEventListener('change',async function(e){
    var file=e.target.files[0];e.target.value='';if(!file)return;
    showToast('Décodage de la vidéo…');
    var blobUrl=URL.createObjectURL(file);
    try{
      var frames=await decodeVideoFrames(blobUrl);
      await importVideoFrames(frames,file.name.replace(/\.[^.]+$/,''));
    }finally{URL.revokeObjectURL(blobUrl);}
  });
  document.getElementById('btn-import-video')&&document.getElementById('btn-import-video').addEventListener('click',importVideo);

  window.SM=window.SM||{};window.SM.importImages=importImages;window.SM.importVideo=importVideo;
})();
