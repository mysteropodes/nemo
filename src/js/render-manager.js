// ---- Render Manager — batch render queue (2026-08-29, feedback #141) ----
// Cyril: "il faudrait un render manager un peu plus poussé, la possibilité
// d'envoyer différentes compositions, choisir différents format de sortie
// ... conforme l'ui design dans notre style de l'app" — a reference
// screenshot (another tool's render queue) showed the FUNCTIONALITY wanted:
// queue N render jobs, each with its own format/frame-range/output
// settings, "Render All" walks the queue, a Dynamic Index auto-numbers
// batch filenames.
//
// "Composition" mapping: Nemo has no After-Effects-style multiple named
// comps living inside one project file — one project = one canvas/timeline
// (state.layers), exactly what export.js already renders. A StoryBoard
// "montage" only becomes exportable once explicitly placed as a Component
// LAYER in the current document (SMStoryboard.placeMontageAsLayer,
// storyboard.js) — at that point it's just ordinary layer content, no
// separate export path needed. Multiple fully independent PROJECTS exist
// only as separate project tabs (project.js), which is a different, larger
// investigation (parallel #140 work on cross-project-tab access) — out of
// scope here. So each queue item below is "a render job against the
// CURRENT document, over some frame range, with its own output settings" —
// exactly what the reference screenshot's own primary action is titled:
// "+ Current Composition". Queuing the SAME document multiple times (e.g.
// once as MP4, once as a PNG sequence, or different frame ranges) is fully
// supported; queuing a genuinely different PROJECT is not (deferred, see
// above).
//
// This file is pure UI + queue orchestration — it never encodes a frame
// itself. Every actual render goes through export.js's existing pipeline
// (SMExport.*), including the new dialog-free "ToPath"/"ToDir" siblings
// added there for exactly this batch use (see export.js's own comment on
// them). Reused, not rebuilt, per this app's own §3 "two implementations
// must never drift apart" principle applied to "don't build a second
// export engine".
(function(){
  var _queue=[];
  var _globals={dynamicIndex:0,dynamicIndexOffset:0};
  var _rendering=false;
  var _itemCounter=0;
  var _bound=false;

  var SEQUENCE_FORMATS=['svg','png','tiff'];
  var ALPHA_FORMATS=['png','prores'];
  var EXT_FOR_FORMAT={mp4:'mp4',gif:'gif',prores:'mov',lottie:'json'};
  var FILTERS_FOR_FORMAT={
    mp4:[{name:'MP4',extensions:['mp4']}],
    gif:[{name:'GIF',extensions:['gif']}],
    prores:[{name:'QuickTime',extensions:['mov']}],
    lottie:[{name:'Lottie JSON',extensions:['json']}],
  };

  function t(key){return (window.SM&&SM.t)?SM.t(key):key;}

  function padNum(n,digits){
    var s=String(Math.max(0,Math.trunc(n||0)));
    digits=Math.max(1,digits||1);
    while(s.length<digits)s='0'+s;
    return s;
  }
  function ensureExt(path,ext){
    if(!path)return path;
    if(new RegExp('\\.'+ext+'$','i').test(path))return path;
    return path.replace(/[\\/]+$/,'')+'.'+ext;
  }
  // Optional {index} token in the File Name — replaced with the Dynamic
  // Index (+Offset, +this item's position in THIS Render All run),
  // zero-padded to the item's own Padding digit count. A name with no
  // token is used as-is (index isn't forced on anyone, matching the
  // reference UI's own "IF a queue item's filename template includes a
  // numbering placeholder" wording).
  function resolveFileName(item,dynIndexValue){
    var base=(item.name||'').trim()||t('rmNewItemDefaultName');
    if(base.indexOf('{index}')>=0)return base.split('{index}').join(padNum(dynIndexValue,item.padding||1));
    return base;
  }
  function resolveRange(item){
    if(item.frameRangeMode==='all')return{start:0,end:Math.max(0,(state.totalFrames||1)-1)};
    if(item.frameRangeMode==='custom')return{start:item.rangeStart||0,end:item.rangeEnd!=null?item.rangeEnd:Math.max(0,(state.totalFrames||1)-1)};
    return{start:state.waIn||0,end:state.waOut||0};
  }
  function computeOutputPreview(item){
    if(!item.outputPath)return t('rmOutputPreviewEmpty');
    var dyn=(_globals.dynamicIndex||0)+(_globals.dynamicIndexOffset||0);
    var seq=SEQUENCE_FORMATS.indexOf(item.format)>=0;
    if(seq){
      var seqExt=item.format==='svg'?'svg':item.format==='tiff'?'tif':'png';
      return item.outputPath.replace(/[/\\]+$/,'')+'/frame_0001.'+seqExt+' … ('+resolveFileName(item,dyn)+')';
    }
    var ext=EXT_FOR_FORMAT[item.format]||'';
    var path=item.outputPath;
    if(path.indexOf('{index}')>=0)path=path.split('{index}').join(padNum(dyn,item.padding||1));
    return ensureExt(path,ext);
  }

  // ---- queue item model ----
  function newItem(){
    _itemCounter++;
    return{
      id:'rq'+Date.now()+'_'+_itemCounter,
      enabled:true,
      name:_itemCounter===1?t('rmNewItemDefaultName'):(t('rmNewItemDefaultName')+' '+_itemCounter),
      format:'mp4',
      frameRangeMode:'playback',
      rangeStart:state.waIn||0,
      rangeEnd:state.waOut||Math.max(0,(state.totalFrames||1)-1),
      resolutionScalePct:100,
      quality:'high',
      outputPath:'',
      padding:4,
      alpha:false,
      expanded:true,
      status:'idle', // idle|rendering|done|error
      progress:0,
      error:null,
      outPathResolved:null,
    };
  }
  function addCurrentComposition(){
    var it=newItem();
    _queue.push(it);
    renderQueueList();
  }
  function duplicateItem(item){
    var copy=JSON.parse(JSON.stringify(item));
    _itemCounter++;
    copy.id='rq'+Date.now()+'_'+_itemCounter;
    copy.status='idle';copy.progress=0;copy.error=null;copy.outPathResolved=null;copy.expanded=false;
    var idx=_queue.indexOf(item);
    _queue.splice(idx+1,0,copy);
    renderQueueList();
  }
  function deleteItem(item){
    if(_rendering)return;
    var idx=_queue.indexOf(item);
    if(idx>=0)_queue.splice(idx,1);
    renderQueueList();
  }
  function deleteAll(){
    if(_rendering||!_queue.length)return;
    if(!confirm(t('rmConfirmDeleteAll')))return;
    _queue=[];
    renderQueueList();
  }

  // ---- per-item render (calls into export.js — see file header) ----
  async function renderItem(item,dynIndexValue){
    var tauri=!!(window.SMExport&&SMExport.isAvailable&&SMExport.isAvailable());
    var range=resolveRange(item);
    var scale=Math.max(0.01,(item.resolutionScalePct||100)/100);
    var alpha=!!item.alpha&&ALPHA_FORMATS.indexOf(item.format)>=0;
    var opts={
      start:range.start,end:range.end,scale:scale,fps:state.fps,alpha:alpha,quality:item.quality,
      onProgress:function(i,n){item.progress=n?Math.round(i/n*100):0;renderQueueList();},
      onFfmpeg:function(){},
    };
    var fileBase=resolveFileName(item,dynIndexValue);
    var res;
    switch(item.format){
      case 'png':
        if(!tauri)throw new Error(t('rmErrDesktopOnly'));
        if(!item.outputPath)throw new Error(t('rmErrNoPath'));
        res=await SMExport.exportPNGSequenceToDir(item.outputPath,opts);
        break;
      case 'tiff':
        if(!tauri)throw new Error(t('rmErrDesktopOnly'));
        if(!item.outputPath)throw new Error(t('rmErrNoPath'));
        res=await SMExport.exportTIFFSequenceToDir(item.outputPath,opts);
        break;
      case 'svg':
        if(tauri){
          if(!item.outputPath)throw new Error(t('rmErrNoPath'));
          res=await SMExport.exportSVGSequenceToDir(item.outputPath,opts);
        }else{
          res=await SMExport.exportSVGSequence(opts); // browser: single representative frame (existing constraint)
        }
        break;
      case 'gif':
        if(tauri){
          if(!item.outputPath)throw new Error(t('rmErrNoPath'));
          res=await SMExport.exportGIFToPath(ensureExt(item.outputPath,'gif'),opts);
        }else{
          if(!(SMExport.gifBrowserAvailable&&SMExport.gifBrowserAvailable()))throw new Error(t('rmErrBrowserUnsupported'));
          opts.filename=fileBase+'.gif';
          res=await SMExport.exportGifBrowser(opts);
        }
        break;
      case 'prores':
        if(!tauri)throw new Error(t('rmErrDesktopOnly'));
        if(!item.outputPath)throw new Error(t('rmErrNoPath'));
        res=await SMExport.exportProResToPath(ensureExt(item.outputPath,'mov'),opts);
        break;
      case 'lottie':
        if(tauri){
          if(!item.outputPath)throw new Error(t('rmErrNoPath'));
          res=await SMExport.exportLottieToPath(ensureExt(item.outputPath,'json'),opts);
        }else{
          opts.filename=fileBase+'.json';
          res=await SMExport.exportLottie(opts);
        }
        break;
      case 'mp4':
      default:
        if(tauri){
          if(!item.outputPath)throw new Error(t('rmErrNoPath'));
          res=await SMExport.exportMP4Silent(ensureExt(item.outputPath,'mp4'),opts);
        }else{
          if(!(SMExport.videoBrowserAvailable&&SMExport.videoBrowserAvailable()))throw new Error(t('rmErrBrowserUnsupported'));
          var mime=(SMExport.pickVideoMimeType&&SMExport.pickVideoMimeType())||'';
          var vext=mime.indexOf('mp4')>=0?'mp4':'webm';
          opts.filename=fileBase+'.'+vext;
          res=await SMExport.exportVideoBrowser(opts);
        }
        break;
    }
    if(!res)throw new Error(t('rmErrDesktopOnly'));
    if(res.cancelled)throw new Error(t('rmErrCancelled'));
    if(!res.ok)throw new Error(res.error||t('rmErrUnknown'));
    item.outPathResolved=res.path||res.dir||res.filename||item.outputPath;
    return res;
  }

  function updateOverallProgress(items){
    var fill=document.getElementById('rm-progress-fill'),label=document.getElementById('rm-progress-label');
    var totalPct=items.reduce(function(sum,it){
      if(it.status==='done')return sum+100;
      if(it.status==='rendering')return sum+(it.progress||0);
      return sum;
    },0);
    var pct=items.length?Math.round(totalPct/items.length):0;
    if(fill)fill.style.width=pct+'%';
    if(label)label.textContent=pct+'%';
  }
  function setToolbarBusy(busy){
    ['rm-add-current','rm-delete-all','rm-render-all'].forEach(function(id){
      var el=document.getElementById(id);if(el)el.disabled=busy;
    });
  }

  async function runRenderAll(){
    if(_rendering)return;
    var items=_queue.filter(function(it){return it.enabled;});
    if(!items.length){if(window.showToast)showToast(t('rmToastEmptyQueue'));return;}
    _rendering=true;setToolbarBusy(true);
    if(window.saveAllLayerFrames){try{saveAllLayerFrames();}catch(e){}}
    var done=0,failed=0;
    for(var i=0;i<items.length;i++){
      var item=items[i];
      item.status='rendering';item.progress=0;item.error=null;item.outPathResolved=null;
      renderQueueList();updateOverallProgress(items);
      var dynVal=(_globals.dynamicIndex||0)+(_globals.dynamicIndexOffset||0)+i;
      try{
        await renderItem(item,dynVal);
        item.status='done';item.progress=100;done++;
      }catch(err){
        item.status='error';item.error=(err&&err.message)?err.message:String(err);failed++;
      }
      updateOverallProgress(items);
      renderQueueList();
    }
    _rendering=false;setToolbarBusy(false);
    if(window.showToast)showToast(t('rmToastBatchDone').replace('{done}',done).replace('{err}',failed));
  }

  // ---- browse (folder/save dialog — Tauri only, same pickers export.js's
  // own dialog-based exporters use) ----
  async function browsePath(item,pathInp,refreshPreview){
    if(!(window.SMExport&&SMExport.isAvailable&&SMExport.isAvailable())){
      if(window.showToast)showToast(t('rmErrDesktopOnly'));
      return;
    }
    var seq=SEQUENCE_FORMATS.indexOf(item.format)>=0;
    var path;
    if(seq){
      path=await SMExport.pickDir(t('rmPathBrowseTitle'));
    }else{
      var ext=EXT_FOR_FORMAT[item.format]||'mp4';
      var dyn=(_globals.dynamicIndex||0)+(_globals.dynamicIndexOffset||0);
      var defaultName=resolveFileName(item,dyn)+'.'+ext;
      path=await SMExport.pickSaveFile(t('rmPathBrowseTitle'),defaultName,FILTERS_FOR_FORMAT[item.format]);
    }
    if(path){
      item.outputPath=path;
      pathInp.value=path;
      refreshPreview();
    }
  }

  // ---- DOM building ----
  function fieldRow(labelKey){
    var r=document.createElement('div');r.className='pr';
    var l=document.createElement('span');l.className='pl';l.textContent=t(labelKey);
    r.appendChild(l);
    return r;
  }
  function numInput(value,step,min,max){
    var inp=document.createElement('input');
    inp.type='number';inp.className='pi scrub';inp.value=value;inp.dataset.step=step||1;
    if(min!=null)inp.min=min;if(max!=null)inp.max=max;
    return inp;
  }
  function selectFrom(options,value){
    var sel=document.createElement('select');sel.className='psel';
    options.forEach(function(o){
      var op=document.createElement('option');op.value=o[0];op.textContent=o[1];
      sel.appendChild(op);
    });
    sel.value=value;
    return sel;
  }

  function buildItemBox(item){
    var box=document.createElement('div');
    box.className='rm-item'+(item.expanded?' expanded':'')+(item.enabled?'':' disabled');

    var row=document.createElement('div');row.className='rm-item-row';

    var chk=document.createElement('input');chk.type='checkbox';chk.checked=item.enabled;chk.disabled=_rendering;
    chk.title=t('rmEnabledTitle');
    chk.addEventListener('click',function(e){e.stopPropagation();});
    chk.addEventListener('change',function(){item.enabled=chk.checked;box.classList.toggle('disabled',!item.enabled);});
    row.appendChild(chk);

    var chevron=document.createElement('div');chevron.className='rm-item-chevron';
    chevron.innerHTML='<svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2.4"><path d="M9 6l6 6-6 6"/></svg>';
    row.appendChild(chevron);

    var nameEl=document.createElement('span');nameEl.className='rm-item-name';nameEl.textContent=item.name||t('rmNewItemDefaultName');
    row.appendChild(nameEl);

    var badge=document.createElement('span');badge.className='rm-item-badge';badge.textContent=item.format.toUpperCase();
    row.appendChild(badge);

    var status=document.createElement('span');status.className='rm-item-status';
    if(item.status==='rendering'){status.classList.add('rendering');status.textContent=(item.progress||0)+'%';status.title=t('rmStatusRendering');}
    else if(item.status==='done'){status.classList.add('done');status.textContent='✓';status.title=item.outPathResolved||t('rmStatusDone');}
    else if(item.status==='error'){status.classList.add('error');status.textContent='✕';status.title=item.error||t('rmStatusError');}
    row.appendChild(status);

    var dup=document.createElement('div');dup.className='rm-item-dup';dup.title=t('rmDuplicateTitle');
    dup.innerHTML='<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2"><rect x="7" y="7" width="12" height="12" rx="2"/><path d="M5 15V5a2 2 0 012-2h10"/></svg>';
    dup.addEventListener('click',function(e){e.stopPropagation();duplicateItem(item);});
    row.appendChild(dup);

    var del=document.createElement('div');del.className='rm-item-del';del.textContent='×';del.title=t('rmDeleteItemTitle');
    del.addEventListener('click',function(e){e.stopPropagation();deleteItem(item);});
    row.appendChild(del);

    row.addEventListener('click',function(){item.expanded=!item.expanded;renderQueueList();});
    box.appendChild(row);

    if(item.expanded)box.appendChild(buildFieldsFor(item,nameEl,badge));
    if(item.status==='error'&&item.error){
      var errEl=document.createElement('div');errEl.className='rm-item-error';errEl.textContent=item.error;
      box.appendChild(errEl);
    }
    return box;
  }

  function buildFieldsFor(item,nameEl,badgeEl){
    var wrap=document.createElement('div');wrap.className='rm-item-fields';

    // Format
    var rFmt=fieldRow('rmFieldFormat');
    var fmtSel=selectFrom([
      ['svg',t('expSeqSvg')],['png',t('expSeqPng')],['tiff',t('expSeqTiff')],
      ['gif','GIF'],['mp4','MP4 (H.264)'],['prores','ProRes (.mov)'],['lottie','Lottie JSON'],
    ],item.format);
    fmtSel.addEventListener('change',function(){item.format=fmtSel.value;badgeEl.textContent=item.format.toUpperCase();renderQueueList();});
    rFmt.appendChild(fmtSel);wrap.appendChild(rFmt);

    // File Name
    var rName=fieldRow('rmFieldFileName');
    var nameInp=document.createElement('input');nameInp.type='text';nameInp.className='pi';nameInp.value=item.name||'';nameInp.title=t('rmFileNameTokenHint');
    var preview; // forward-declared, refreshed below
    nameInp.addEventListener('input',function(){
      item.name=nameInp.value;
      nameEl.textContent=item.name||t('rmNewItemDefaultName');
      if(preview)preview.textContent=computeOutputPreview(item);
    });
    rName.appendChild(nameInp);wrap.appendChild(rName);

    // Frame Range Mode
    var rMode=fieldRow('rmFieldFrameRangeMode');
    var modeSel=selectFrom([
      ['playback',t('rmFrameRangePlayback')],['all',t('rmFrameRangeAll')],['custom',t('rmFrameRangeCustom')],
    ],item.frameRangeMode);
    modeSel.addEventListener('change',function(){item.frameRangeMode=modeSel.value;renderQueueList();});
    rMode.appendChild(modeSel);wrap.appendChild(rMode);

    // Frame Range Start/End — greyed out (and live-computed) unless Custom
    var rRange=fieldRow('rmFieldFrameRange');
    var resolved=resolveRange(item);
    var isCustom=item.frameRangeMode==='custom';
    var startInp=numInput(isCustom?item.rangeStart:resolved.start,1,0);
    var endInp=numInput(isCustom?item.rangeEnd:resolved.end,1,0);
    startInp.disabled=!isCustom;endInp.disabled=!isCustom;
    var sLabel=document.createElement('span');sLabel.textContent='S';sLabel.style.cssText='font-size:9px;color:var(--text-dim);flex:none;';
    var eLabel=document.createElement('span');eLabel.textContent='E';eLabel.style.cssText='font-size:9px;color:var(--text-dim);flex:none;margin-left:6px;';
    startInp.addEventListener('change',function(){item.rangeStart=parseInt(startInp.value,10)||0;});
    endInp.addEventListener('change',function(){item.rangeEnd=parseInt(endInp.value,10)||0;});
    rRange.appendChild(sLabel);rRange.appendChild(startInp);rRange.appendChild(eLabel);rRange.appendChild(endInp);
    wrap.appendChild(rRange);

    // Padding (dynamic-index zero-pad digit count)
    var rPad=fieldRow('rmFieldPadding');rPad.title=t('rmPaddingTitle');
    var padInp=numInput(item.padding||4,1,1,9);padInp.title=t('rmPaddingTitle');
    padInp.addEventListener('input',function(){item.padding=Math.max(1,parseInt(padInp.value,10)||1);if(preview)preview.textContent=computeOutputPreview(item);});
    rPad.appendChild(padInp);wrap.appendChild(rPad);

    // Path
    var rPath=fieldRow('rmFieldPath');
    var pathRow=document.createElement('div');pathRow.className='rm-path-row';
    var pathInp=document.createElement('input');pathInp.type='text';pathInp.className='pi';pathInp.value=item.outputPath||'';
    var tauriOk=!!(window.SMExport&&SMExport.isAvailable&&SMExport.isAvailable());
    if(!tauriOk)pathInp.placeholder=t('rmErrDesktopOnly');
    pathInp.addEventListener('input',function(){item.outputPath=pathInp.value;if(preview)preview.textContent=computeOutputPreview(item);});
    var browseBtn=document.createElement('button');browseBtn.type='button';browseBtn.className='rm-path-browse';browseBtn.title=t('rmPathBrowseTitle');
    browseBtn.innerHTML='<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z"/></svg>';
    browseBtn.addEventListener('click',function(){browsePath(item,pathInp,function(){if(preview)preview.textContent=computeOutputPreview(item);});});
    pathRow.appendChild(pathInp);pathRow.appendChild(browseBtn);
    rPath.appendChild(pathRow);wrap.appendChild(rPath);

    // Resolution Scale
    var rScale=fieldRow('rmFieldResolutionScale');
    var scaleInp=numInput(item.resolutionScalePct||100,5,1,800);
    var scaleComputed=document.createElement('span');scaleComputed.className='rm-scale-computed';
    function updateScaleComputed(){
      var s=(item.resolutionScalePct||100)/100;
      scaleComputed.textContent=Math.round((state.canvasW||0)*s)+' × '+Math.round((state.canvasH||0)*s);
    }
    updateScaleComputed();
    scaleInp.addEventListener('input',function(){item.resolutionScalePct=parseFloat(scaleInp.value)||100;updateScaleComputed();});
    rScale.appendChild(scaleInp);rScale.appendChild(scaleComputed);wrap.appendChild(rScale);

    // Render Quality — only MP4 actually honors this (export.js's -q:v knob)
    var rQual=fieldRow('rmFieldRenderQuality');rQual.title=t('rmQualityTitle');
    var qualSel=selectFrom([
      ['draft',t('rmQualityDraft')],['medium',t('rmQualityMedium')],['high',t('rmQualityHigh')],['best',t('rmQualityBest')],
    ],item.quality||'high');
    qualSel.title=t('rmQualityTitle');
    qualSel.addEventListener('change',function(){item.quality=qualSel.value;});
    rQual.appendChild(qualSel);wrap.appendChild(rQual);

    // Transparent Background — only for formats with an alpha channel
    if(ALPHA_FORMATS.indexOf(item.format)>=0){
      var rAlpha=document.createElement('div');rAlpha.className='pr';
      var alphaLabel=document.createElement('label');alphaLabel.style.cssText='font-size:11px;color:var(--text-dim);display:flex;align-items:center;gap:6px;cursor:pointer;';
      var alphaChk=document.createElement('input');alphaChk.type='checkbox';alphaChk.checked=!!item.alpha;alphaChk.style.accentColor='var(--accent)';
      alphaChk.addEventListener('change',function(){item.alpha=alphaChk.checked;});
      var alphaTxt=document.createElement('span');alphaTxt.textContent=t('rmFieldAlpha');
      alphaLabel.appendChild(alphaChk);alphaLabel.appendChild(alphaTxt);
      rAlpha.appendChild(alphaLabel);wrap.appendChild(rAlpha);
    }

    // Live output-path preview
    preview=document.createElement('div');preview.className='rm-output-preview';
    preview.textContent=computeOutputPreview(item);
    wrap.appendChild(preview);

    return wrap;
  }

  function renderQueueList(){
    var listEl=document.getElementById('rm-queue-list');
    var emptyHint=document.getElementById('rm-empty-hint');
    if(!listEl)return;
    listEl.innerHTML='';
    if(emptyHint)emptyHint.style.display=_queue.length?'none':'';
    _queue.forEach(function(item){listEl.appendChild(buildItemBox(item));});
  }

  // ---- modal open/close/bind ----
  function close(){var m=document.getElementById('render-manager-modal');if(m)m.style.display='none';}
  function open(){
    bindOnce();
    renderQueueList();
    var m=document.getElementById('render-manager-modal');
    if(m)m.style.display='flex';
  }
  function bindOnce(){
    if(_bound)return;_bound=true;
    var modal=document.getElementById('render-manager-modal');
    if(!modal)return;
    var closeBtn=document.getElementById('rm-close');if(closeBtn)closeBtn.addEventListener('click',close);
    modal.addEventListener('click',function(e){if(e.target===modal)close();});
    var addBtn=document.getElementById('rm-add-current');if(addBtn)addBtn.addEventListener('click',addCurrentComposition);
    var delAllBtn=document.getElementById('rm-delete-all');if(delAllBtn)delAllBtn.addEventListener('click',deleteAll);
    var renderAllBtn=document.getElementById('rm-render-all');if(renderAllBtn)renderAllBtn.addEventListener('click',runRenderAll);
    var dynIndexInp=document.getElementById('rm-dyn-index');
    if(dynIndexInp)dynIndexInp.addEventListener('input',function(){_globals.dynamicIndex=parseInt(dynIndexInp.value,10)||0;});
    var dynOffsetInp=document.getElementById('rm-dyn-offset');
    if(dynOffsetInp)dynOffsetInp.addEventListener('input',function(){_globals.dynamicIndexOffset=parseInt(dynOffsetInp.value,10)||0;});
  }

  window.SMRenderManager={
    open:open,
    close:close,
    // exposed for tests/debugging (console) — mirrors other window.SM* modules' habit of exposing internals read-only-ish
    _queue:function(){return _queue;},
    _globals:_globals,
  };
})();
