// ---- PLAYBACK (optimized: no DOM rebuild during play) ----
var playInt=null;
function startPlay(){if(state.playing)return;state.playing=true;
  document.getElementById('btn-play').innerHTML='<span class="material-symbols-rounded">\u{e034}</span>';
  document.getElementById('btn-play').classList.add('playing');
  playInt=setInterval(function(){var next=state.currentFrame+1;
    if(next>state.waOut){if(state.loopPlayback)next=state.waIn;else{stopPlay();return;}}
    saveAllLayerFrames();state.currentFrame=next;window._curFrame=next;loadFrame(next);updatePlayhead();
  },1000/state.fps);
}
function stopPlay(){if(!state.playing)return;state.playing=false;clearInterval(playInt);playInt=null;
  document.getElementById('btn-play').innerHTML='<span class="material-symbols-rounded">\u{e037}</span>';
  document.getElementById('btn-play').classList.remove('playing');
  renderOS();renderArcs();updateUI();
}
function togglePlay(){if(state.playing)stopPlay();else startPlay();}

function updatePlayhead(){
  document.getElementById('tl-cf').textContent=state.currentFrame+1;
  document.getElementById('info-frame').textContent=state.currentFrame+1;
  document.getElementById('playhead').style.left=(state.currentFrame*FC)+'px';
  document.querySelectorAll('.fc.cur').forEach(function(el){el.classList.remove('cur');});
  document.querySelectorAll('.fhc.cur').forEach(function(el){el.classList.remove('cur');});
  document.querySelectorAll('.fc[data-frame="'+state.currentFrame+'"]').forEach(function(el){el.classList.add('cur');});
  var hdr=document.getElementById('frame-hdr').children;if(hdr[state.currentFrame])hdr[state.currentFrame].classList.add('cur');
  var f=state.layers[state.activeLayerIdx].frames[state.currentFrame];var badge=document.getElementById('info-badge');
  if(f&&f.isKeyframe){badge.style.display='inline-block';badge.className='badge key';badge.textContent='KEY';}
  else if(f&&f.isInterpolated){badge.style.display='inline-block';badge.className='badge tw';badge.textContent='TWEEN';}
  else badge.style.display='none';
}

// ---- FRAME SELECTION (Animate-style) ----
var _sel={frames:[],clipboard:null,clipOp:null};
function selClear(){_sel.frames=[];document.querySelectorAll('.fc.sel').forEach(function(el){el.classList.remove('sel');});}
function selHas(li,fi){return _sel.frames.some(function(s){return s.layer===li&&s.frame===fi;});}
function selAdd(li,fi){if(!selHas(li,fi))_sel.frames.push({layer:li,frame:fi});}
function selRemove(li,fi){_sel.frames=_sel.frames.filter(function(s){return !(s.layer===li&&s.frame===fi);});}
function selToggle(li,fi){if(selHas(li,fi))selRemove(li,fi);else selAdd(li,fi);}
function selRange(li,fi){
  if(!_sel.frames.length){selAdd(li,fi);return;}
  var last=_sel.frames[_sel.frames.length-1];
  var l0=Math.min(last.layer,li),l1=Math.max(last.layer,li);
  var f0=Math.min(last.frame,fi),f1=Math.max(last.frame,fi);
  for(var l=l0;l<=l1;l++)for(var f=f0;f<=f1;f++)selAdd(l,f);
}
function selApplyCSS(){
  document.querySelectorAll('.fc.sel').forEach(function(el){el.classList.remove('sel');});
  _sel.frames.forEach(function(s){
    var cell=document.querySelector('.fc[data-layer="'+s.layer+'"][data-frame="'+s.frame+'"]');
    if(cell)cell.classList.add('sel');
  });
}
function selBounds(){
  if(!_sel.frames.length)return null;
  var minL=Infinity,maxL=-1,minF=Infinity,maxF=-1;
  _sel.frames.forEach(function(s){
    if(s.layer<minL)minL=s.layer;if(s.layer>maxL)maxL=s.layer;
    if(s.frame<minF)minF=s.frame;if(s.frame>maxF)maxF=s.frame;
  });
  return{minL:minL,maxL:maxL,minF:minF,maxF:maxF};
}

// ---- API ----
window.SM={
  goToFrame:function(idx){goToFrame(idx);},togglePlay:togglePlay,stopPlay:stopPlay,undo:undo,redo:redo,
  setTool:function(t){if(t!=='select'&&t!=='subselect')clearSel();if(t!=='pen'&&_pen.path)finalizePen();if(t!=='eraser'&&typeof _eraserCursor!=='undefined'&&_eraserCursor){_eraserCursor.remove();_eraserCursor=null;}state.tool=t;renderArcs();
    document.querySelectorAll('.tool-btn').forEach(function(b){b.classList.toggle('active',b.dataset.tool===t);});
    var cc={draw:'crosshair',pen:'crosshair',line:'crosshair',rect:'crosshair',ellipse:'crosshair',select:'default',subselect:'default',eraser:'pointer',fill:'crosshair',fillbrush:'crosshair',eyedropper:'crosshair',hand:'grab',zoom:'zoom-in'};
    canvasEl.style.cursor=cc[t]||'default';
    updatePropsContext();},
  toggleOnion:function(){state.onionSkin=!state.onionSkin;renderOS();var el=document.getElementById('os-st');el.textContent=state.onionSkin?'ON':'OFF';el.style.color=state.onionSkin?'var(--green)':'var(--text-dim)';var b=document.getElementById('btn-os');if(b)b.classList.toggle('active',state.onionSkin);},
  toggleLoopPlayback:function(){state.loopPlayback=!state.loopPlayback;var b=document.getElementById('btn-loop');if(b)b.classList.toggle('active',state.loopPlayback);},
  setPointType:setPointType,booleanOp:booleanOp,
  generateTweens:generateTweens,insertFrame:insertFrame,insertKeyframe:insertKeyframe,insertBlankKeyframe:insertBlankKeyframe,removeFrame:removeFrame,
  clearKeyframe:clearKeyframe,convertToKeyframes:convertToKeyframes,removeFrameSpan:removeFrameSpan,duplicateSelectedFrames:duplicateSelectedFrames,
  // Tool settings double as selection editors (Animate behavior): with the
  // Select tool active and strokes selected, changing width/style/color/
  // cap/join restyles the selection instead of only future strokes.
  setBrushSize:function(v){state.brushSize=v;
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){
      pushUndo();
      selectedPaths.forEach(function(p){
        if(p.data&&p.data.isVectorBrush&&p.data.centerSegments){
          // pressure-brush width is a per-point profile (taper), not a
          // single number — rescale every sample by the same ratio so the
          // taper shape is preserved, using the profile's own peak as the
          // "current size" baseline (matches what the size actually reads as).
          var cs=p.data.centerSegments;
          var peak=0;cs.forEach(function(s){if((s.width||0)>peak)peak=s.width||0;});
          var ratio=peak>0?v/peak:1;
          cs.forEach(function(s){s.width=(s.width||v)*ratio;});
          rebuildVectorBrushOutline(p);
        }else{p.strokeWidth=v;applyStrokeStyle(p);}
      });
      saveActiveLayerFrame();updateUI();
    }},
  setStrokeColor:function(v){state.strokeColor=v;document.getElementById('stroke-well').style.background=v;document.getElementById('pm-stroke').style.background=v;
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(p.data&&p.data.isVectorBrush)p.fillColor=v;else if(p.strokeColor)p.strokeColor=v;});saveActiveLayerFrame();updateUI();}},
  setFillColor:function(v){state.fillColor=v;
    // Background is written unconditionally (even while fill is disabled) so
    // the swatch shows the last-picked color as soon as fill is re-enabled —
    // the .none overlay (red diagonal) is what actually communicates "off".
    document.getElementById('fill-well').style.background=v;document.getElementById('pm-fill').style.background=v;document.getElementById('pm-fill-c').value=v;
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(p.fillColor)p.fillColor=v;});saveActiveLayerFrame();updateUI();}},
  setFillEnabled:function(v){state.fillEnabled=v;var fw=document.getElementById('fill-well'),pf=document.getElementById('pm-fill');fw.classList.toggle('none',!v);pf.classList.toggle('none',!v);document.getElementById('p-fill-on').checked=v;
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(p.data&&p.data.isVectorBrush)return;p.fillColor=v?state.fillColor:null;});saveActiveLayerFrame();updateUI();}},
  setSmoothing:function(v){state.smoothing=v;},setStabilizer:function(v){state.stabilizer=parseInt(v);},
  setStrokeCap:function(v){state.strokeCap=v;
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(!(p.data&&p.data.isVectorBrush))p.strokeCap=v;});saveActiveLayerFrame();}},
  setStrokeJoin:function(v){state.strokeJoin=v;
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(!(p.data&&p.data.isVectorBrush))p.strokeJoin=v;});saveActiveLayerFrame();}},
  setMiterLimit:function(v){state.miterLimit=Math.max(1,parseFloat(v)||10);
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(!(p.data&&p.data.isVectorBrush))p.miterLimit=state.miterLimit;});saveActiveLayerFrame();}},
  setPaintOrder:function(v){state.paintOrder=v;
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){p.data=p.data||{};p.data.paintOrder=v;});saveActiveLayerFrame();updateUI();}},
  setDashOffset:function(v){state.dashOffset=parseFloat(v)||0;
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(!(p.data&&p.data.isVectorBrush)&&p.dashArray&&p.dashArray.length)p.dashOffset=state.dashOffset;});saveActiveLayerFrame();}},
  setStrokeStyle:function(v){state.strokeStyle=v;
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(!(p.data&&p.data.isVectorBrush)&&p.strokeColor)applyStrokeStyle(p);});saveActiveLayerFrame();updateUI();}},
  // One-shot "smooth this already-drawn stroke more" action (Stroke panel's
  // Smooth+Apply), distinct from Tool Options' Smooth which only shapes NEW
  // strokes as state.smoothing at draw time. Plain paths just get Paper's
  // own simplify() re-run with a larger tolerance (safe to call repeatedly
  // on an already-simplified path — it re-fits whatever segments are
  // currently there, no raw point cloud needed). Pressure-brush paths can't
  // simplify() the visible outline directly (that's the two-sided variable-
  // width band, not the authored shape) — the centerline anchors
  // (data.centerSegments) are re-simplified instead, then the outline is
  // regenerated from them; the dense pressure/width curve
  // (data.widthProfile) is left untouched so the taper/pressure feel
  // survives even as the centerline geometry itself gets smoother.
  smoothSelectedStroke:function(amount){
    if(!((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length))return;
    amount=Math.max(0,parseFloat(amount)||0);
    pushUndo();
    selectedPaths.forEach(function(p){
      if(p.data&&p.data.isVectorBrush&&p.data.centerSegments&&p.data.centerSegments.length>1){
        var raw=p.data.centerSegments.map(function(s){return new Point(s.point[0],s.point[1]);});
        var widths=p.data.centerSegments.map(function(s){return s.width;});
        var profile=p.data.widthProfile;
        var cs=buildCenterSegmentsFromRawStroke(raw,widths,amount);
        if(profile)cs.widthProfile=profile;
        p.data.centerSegments=cs;
        if(profile)p.data.widthProfile=profile;
        rebuildVectorBrushOutline(p);
      }else{
        p.simplify(amount);
      }
    });
    fillRegenerateLinked(userLayers[state.activeLayerIdx],null);
    saveActiveLayerFrame();updateUI();
  },
  setVectorBrush:function(v){state.vectorBrush=v;},setTaperEnds:function(v){state.taperEnds=v;},
  setDrawMode:function(v){state.drawMode=v;},
  setFillBrushMode:function(v){state.fillBrushMode=v;},
  setEraserSize:function(v){state.eraserSize=Math.max(2,parseInt(v)||24);},
  setPressureMin:function(v){state.pressureMin=Math.max(0,Math.min(100,parseInt(v)||0));},
  setPressureMax:function(v){state.pressureMax=Math.max(50,Math.min(300,parseInt(v)||170));},
  setPressureInvert:function(v){state.pressureInvert=!!v;},
  setOpacity:function(v){state.opacity=parseInt(v);},
  setFps:function(v){state.fps=Math.max(1,Math.min(60,v));if(state.playing){stopPlay();startPlay();}syncDocFields();},
  setCurve:function(points){state.easingCurve={points:points};},
  setResamplePts:function(v){state.resamplePts=v;},setTweenStep:function(v){state.tweenStep=parseInt(v);},
  setCanvasSize:function(w,h){state.canvasW=w;state.canvasH=h;drawStage();syncDocFields();},setCanvasBg:function(c){state.canvasBg=c;drawStage();syncDocFields();},
  fitCanvas:fitCanvas,resetView:resetView,
  setOnionRange:function(inF,outF){state.onionIn=inF;state.onionOut=outF;renderOS();},
  setOnionPrevOp:function(v){state.onionPrevOpacity=v;renderOS();},setOnionNextOp:function(v){state.onionNextOpacity=v;renderOS();},
  setOnionMode:function(v){state.onionMode=v;renderOS();},
  setWorkArea:function(inF,outF){state.waIn=inF;state.waOut=outF;},
  setTotalFrames:function(v){v=Math.max(1,Math.min(999,v));saveAllLayerFrames();for(var i=0;i<state.layers.length;i++){while(state.layers[i].frames.length<v)state.layers[i].frames.push({strokes:[],isKeyframe:false,isInterpolated:false});}state.totalFrames=v;window._totalF=v;if(state.waOut>=v)state.waOut=v-1;window._waOut=state.waOut;if(state.currentFrame>=v)goToFrame(v-1);updateUI();},
  addLayer:function(){saveAllLayerFrames();pushUndoLayers();var idx=createUserLayer(nextLayerName());activateUL(idx);loadFrame(state.currentFrame);updateUI();},
  deleteLayer:function(){
    if(state.layers.length<=1)return;saveAllLayerFrames();
    pushUndoLayers();
    var sel=(_layerSel.length?_layerSel.slice():[state.activeLayerIdx]).sort(function(a,b){return b-a;});
    sel.forEach(function(idx){
      if(state.layers.length<=1||idx<0||idx>=state.layers.length)return;
      userLayers[idx].remove();userLayers.splice(idx,1);state.layers.splice(idx,1);
    });
    _layerSel=[];
    if(state.activeLayerIdx>=state.layers.length)state.activeLayerIdx=state.layers.length-1;
    activateUL(state.activeLayerIdx);loadFrame(state.currentFrame);updateUI();showToast('Calque(s) supprimé(s) — ⌘Z pour annuler');
  },
  duplicateLayer:function(){saveAllLayerFrames();pushUndoLayers();var src=state.layers[state.activeLayerIdx];var ni=createUserLayer(src.name+' copy');state.layers[ni].frames=JSON.parse(JSON.stringify(src.frames));activateUL(ni);loadFrame(state.currentFrame);updateUI();},
  setActiveLayer:function(idx){if(idx<0||idx>=state.layers.length)return;saveAllLayerFrames();activateUL(idx);clearSel();renderArcs();updateUI();},
  toggleLayerVis:function(idx){state.layers[idx].visible=!state.layers[idx].visible;loadFrame(state.currentFrame);updateUI();},
  toggleLayerLock:function(idx){state.layers[idx].locked=!state.layers[idx].locked;updateUI();},
  renameLayer:function(idx,n){state.layers[idx].name=n;updateUI();},
  reorderLayer:function(fromIdx,toIdx){reorderLayer(fromIdx,toIdx);},
  reorderLayersBatch:function(fromIndices,toIdx){reorderLayersBatch(fromIndices,toIdx);},
  convertActiveLayerToComponent:function(){
    if(_layerSel.length>1)convertLayersToComponent(_layerSel);
    else convertLayerToComponent(state.activeLayerIdx);
  },
  convertComponentToLayer:function(){convertComponentToLayer(state.activeLayerIdx);},
  convertActiveLayerToLFSGroup:function(){convertLayerToLFSGroup(state.activeLayerIdx);},
  convertLFSGroupToLayer:function(){convertLFSGroupToLayer(state.activeLayerIdx);},
  propagateLFSFill:function(which){propagateLFSFill(state.activeLayerIdx,which);},
  enterSymbol:function(symId){enterSymbol(symId);},
  exitToScene:function(){exitToScene();},
  closeSymbolTab:function(symId){closeSymbolTab(symId);},
  setSymbolPlayMode:function(v){var ld=state.layers[state.activeLayerIdx];if(!ld||!ld.symbolId)return;ld.symPlayMode=v;loadFrame(state.currentFrame);renderOS();updateUI();},
  setSymbolSpeed:function(v){var ld=state.layers[state.activeLayerIdx];if(!ld||!ld.symbolId)return;ld.symSpeed=Math.max(0.1,parseFloat(v)||1);loadFrame(state.currentFrame);renderOS();updateUI();},
  setSymbolSingleFrame:function(v){var ld=state.layers[state.activeLayerIdx];if(!ld||!ld.symbolId)return;ld.symSingleFrame=Math.max(0,parseInt(v)||0);loadFrame(state.currentFrame);renderOS();updateUI();},
  setSymbolPlacedAt:function(v){var ld=state.layers[state.activeLayerIdx];if(!ld||!ld.symbolId)return;ld.symPlacedAt=parseInt(v)||0;loadFrame(state.currentFrame);renderOS();updateUI();},
  moveFrames:function(sel,dLayer,dFrame){
    if(!sel.length)return;
    pushUndo();saveAllLayerFrames();
    var b=selBounds();if(!b)return;
    var offsetL=dLayer-b.minL,offsetF=dFrame-b.minF;
    if(offsetL===0&&offsetF===0)return;
    var data=[];
    sel.forEach(function(s){
      var ld=state.layers[s.layer];if(!ld)return;
      data.push({layer:s.layer,frame:s.frame,content:JSON.parse(JSON.stringify(ld.frames[s.frame]))});
    });
    sel.forEach(function(s){
      var ld=state.layers[s.layer];if(!ld)return;
      ld.frames[s.frame]={strokes:[],isKeyframe:false,isInterpolated:false};
    });
    data.forEach(function(d){
      var tl=d.layer+offsetL,tf=d.frame+offsetF;
      if(tl<0||tl>=state.layers.length||tf<0||tf>=state.totalFrames)return;
      state.layers[tl].frames[tf]=d.content;
    });
    _sel.frames=[];
    data.forEach(function(d){
      var tl=d.layer+offsetL,tf=d.frame+offsetF;
      if(tl>=0&&tl<state.layers.length&&tf>=0&&tf<state.totalFrames)selAdd(tl,tf);
    });
    loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
    showToast('Frames déplacées');
  },
  copyFrames:function(){
    saveAllLayerFrames();
    if(!_sel.frames.length){showToast('Aucune sélection');return;}
    var b=selBounds();
    _sel.clipboard=[];
    _sel.frames.forEach(function(s){
      var ld=state.layers[s.layer];if(!ld)return;
      _sel.clipboard.push({rl:s.layer-b.minL,rf:s.frame-b.minF,content:JSON.parse(JSON.stringify(ld.frames[s.frame]))});
    });
    _sel.clipOp='copy';
    showToast('Copié ('+_sel.frames.length+' frames)');
  },
  cutFrames:function(){
    saveAllLayerFrames();
    if(!_sel.frames.length){showToast('Aucune sélection');return;}
    pushUndo();
    var b=selBounds();
    _sel.clipboard=[];
    _sel.frames.forEach(function(s){
      var ld=state.layers[s.layer];if(!ld)return;
      _sel.clipboard.push({rl:s.layer-b.minL,rf:s.frame-b.minF,content:JSON.parse(JSON.stringify(ld.frames[s.frame]))});
      ld.frames[s.frame]={strokes:[],isKeyframe:false,isInterpolated:false};
    });
    _sel.clipOp='cut';
    selClear();loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
    showToast('Coupé ('+_sel.clipboard.length+' frames)');
  },
  pasteFrames:function(){
    if(!_sel.clipboard||!_sel.clipboard.length){showToast('Rien à coller');return;}
    pushUndo();saveAllLayerFrames();
    var baseL=state.activeLayerIdx,baseF=state.currentFrame;
    _sel.clipboard.forEach(function(d){
      var tl=baseL+d.rl,tf=baseF+d.rf;
      if(tl<0||tl>=state.layers.length||tf<0||tf>=state.totalFrames)return;
      state.layers[tl].frames[tf]=JSON.parse(JSON.stringify(d.content));
    });
    selClear();
    _sel.clipboard.forEach(function(d){
      var tl=baseL+d.rl,tf=baseF+d.rf;
      if(tl>=0&&tl<state.layers.length&&tf>=0&&tf<state.totalFrames)selAdd(tl,tf);
    });
    loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
    showToast('Collé ('+_sel.clipboard.length+' frames)');
  },
  deleteSelectedFrames:function(){
    if(!_sel.frames.length)return;
    pushUndo();saveAllLayerFrames();
    _sel.frames.forEach(function(s){
      var ld=state.layers[s.layer];if(!ld)return;
      ld.frames[s.frame]={strokes:[],isKeyframe:false,isInterpolated:false};
    });
    selClear();loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
    showToast('Frames supprimées');
  },
  moveKeyframe:function(layerIdx,fromFrame,toFrame){
    if(fromFrame===toFrame)return;var ld=state.layers[layerIdx];if(!ld)return;
    var src=ld.frames[fromFrame];if(!src||!src.isKeyframe)return;
    var dst=ld.frames[toFrame];if(!dst)return;
    pushUndo();
    ld.frames[toFrame]={strokes:src.strokes,isKeyframe:true,isInterpolated:false};
    ld.frames[fromFrame]={strokes:[],isKeyframe:false,isInterpolated:false};
    loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast('Keyframe déplacée → '+(toFrame+1));
  },
  deleteSelStrokes:function(){if(selectedPaths.length>0){pushUndo();selectedPaths.forEach(function(p){p.remove();});clearSel();saveActiveLayerFrame();updateUI();}},
  flipPreview:function(){
    var prev=state.currentFrame;var wa=state.waIn;var wo=state.waOut;
    if(state._flipping){state._flipping=false;return;}
    state._flipping=true;var dir=1;var cur=prev;
    var flipInt=setInterval(function(){
      if(!state._flipping){clearInterval(flipInt);goToFrame(prev);return;}
      cur+=dir;if(cur>wo){dir=-1;cur=wo-1;}if(cur<wa){dir=1;cur=wa+1;}
      goToFrame(cur);
    },1000/state.fps);
  },
  duplicateKeyframe:function(){
    saveAllLayerFrames();var li=state.activeLayerIdx;var ld=state.layers[li];var cf=state.currentFrame;
    var strokes=getEffectiveStrokes(li,cf);if(!strokes.length){showToast('Rien à dupliquer');return;}
    pushUndo();for(var i=0;i<state.layers.length;i++)state.layers[i].frames.splice(cf+1,0,{strokes:[],isKeyframe:false,isInterpolated:false});
    state.totalFrames++;if(state.waOut<state.totalFrames-1)state.waOut++;window._waOut=state.waOut;window._totalF=state.totalFrames;
    ld.frames[cf+1]={strokes:JSON.parse(JSON.stringify(strokes)),isKeyframe:true,isInterpolated:false};
    goToFrame(cf+1);showToast('Keyframe dupliquée');
  },
  extendExposure:function(n){
    saveAllLayerFrames();var li=state.activeLayerIdx;var cf=state.currentFrame;
    pushUndo();n=n||1;for(var x=0;x<n;x++){for(var i=0;i<state.layers.length;i++)state.layers[i].frames.splice(cf+1,0,{strokes:[],isKeyframe:false,isInterpolated:false});state.totalFrames++;}
    if(state.waOut<state.totalFrames-1)state.waOut=state.totalFrames-1;window._waOut=state.waOut;window._totalF=state.totalFrames;
    updateUI();showToast('Exposition étendue +'+n);
  },
  flipHorizontal:function(){
    if(state.tool!=='select'||!selectedPaths.length){showToast('Sélectionnez des traits');return;}
    pushUndo();var bounds=null;selectedPaths.forEach(function(p){if(!bounds)bounds=p.bounds.clone();else bounds=bounds.unite(p.bounds);});
    var cx=bounds.center.x;
    selectedPaths.forEach(function(p){p.scale(-1,1,new Point(cx,bounds.center.y));
      if(p.data&&p.data.isVectorBrush&&p.data.centerSegments){p.data.centerSegments.forEach(function(s){
        s.point=[2*cx-s.point[0],s.point[1]];s.handleIn=[-s.handleIn[0],s.handleIn[1]];s.handleOut=[-s.handleOut[0],s.handleOut[1]];
      });}
    });
    saveActiveLayerFrame();updateUI();showToast('Flip horizontal');
  },
  flipVertical:function(){
    if(state.tool!=='select'||!selectedPaths.length){showToast('Sélectionnez des traits');return;}
    pushUndo();var bounds=null;selectedPaths.forEach(function(p){if(!bounds)bounds=p.bounds.clone();else bounds=bounds.unite(p.bounds);});
    var cy=bounds.center.y;
    selectedPaths.forEach(function(p){p.scale(1,-1,new Point(bounds.center.x,cy));
      if(p.data&&p.data.isVectorBrush&&p.data.centerSegments){p.data.centerSegments.forEach(function(s){
        s.point=[s.point[0],2*cy-s.point[1]];s.handleIn=[s.handleIn[0],-s.handleIn[1]];s.handleOut=[s.handleOut[0],-s.handleOut[1]];
      });}
    });
    saveActiveLayerFrame();updateUI();showToast('Flip vertical');
  },
  exportJSON:function(){
    // Never exits an open component: the 30s autosave calls this, and the
    // old exitToScene() here silently kicked the user out of the component
    // they were editing every autosave tick. While a component is open,
    // state.layers IS the symbol's own layers array (shared reference), so
    // saveAllLayerFrames keeps state.symbols current, and the scene's real
    // layers are read from the snapshot taken by enterSymbol.
    saveAllLayerFrames();
    var inSym=!!(state.activeSymbolId&&_sceneSnapshot);
    var sceneLayers=inSym?_sceneSnapshot.layers:state.layers;
    var sceneTotal=inSym?_sceneSnapshot.totalFrames:state.totalFrames;
    var sceneFps=inSym?_sceneSnapshot.fps:state.fps;
    var sceneWaIn=inSym?_sceneSnapshot.waIn:state.waIn;
    var sceneWaOut=inSym?_sceneSnapshot.waOut:state.waOut;
    return JSON.stringify({version:13,totalFrames:sceneTotal,fps:sceneFps,canvasW:state.canvasW,canvasH:state.canvasH,canvasBg:state.canvasBg,waIn:sceneWaIn,waOut:sceneWaOut,
      layers:sceneLayers.map(function(l){return{name:l.name,visible:l.visible,locked:l.locked,frames:l.frames,symbolId:l.symbolId,symPlayMode:l.symPlayMode,symSpeed:l.symSpeed,symPlacedAt:l.symPlacedAt,symSingleFrame:l.symSingleFrame,lfsGroup:l.lfsGroup,lfsIds:l.lfsIds,lfsSettings:l.lfsSettings};}),
      symbols:state.symbols,
      motionArcs:state.motionArcs,easingCurve:state.easingCurve,resamplePts:state.resamplePts,tweenStep:state.tweenStep});
  },
  importJSON:function(json,silent){
    try{var d=JSON.parse(json);if(!d.layers&&!d.frames)throw new Error('Invalid');
    if(state.activeSymbolId)exitToScene();
    if(!d.layers)d.layers=[{name:'Layer 1',visible:true,locked:false,frames:d.frames}];
    state.totalFrames=d.totalFrames||d.layers[0].frames.length;state.fps=d.fps||12;state.canvasW=d.canvasW||1920;state.canvasH=d.canvasH||1080;state.canvasBg=d.canvasBg||'#ffffff';
    state.waIn=d.waIn||0;state.waOut=d.waOut!==undefined?d.waOut:state.totalFrames-1;
    window._waIn=state.waIn;window._waOut=state.waOut;window._totalF=state.totalFrames;
    while(userLayers.length>0)userLayers.pop().remove();state.layers=[];
    Object.keys(_symbolPaperLayers).forEach(function(k){_symbolPaperLayers[k].forEach(function(l){l.remove();});});_symbolPaperLayers={};
    state.symbols=d.symbols||{};state.openSymbolTabs=[];state.activeSymbolId=null;
    d.layers.forEach(function(ld){var idx=createUserLayer(ld.name);state.layers[idx].visible=ld.visible!==false;state.layers[idx].locked=ld.locked||false;state.layers[idx].frames=ld.frames;
      if(ld.symbolId){state.layers[idx].symbolId=ld.symbolId;state.layers[idx].symPlayMode=ld.symPlayMode||'loop';state.layers[idx].symSpeed=ld.symSpeed||1;state.layers[idx].symPlacedAt=ld.symPlacedAt||0;state.layers[idx].symSingleFrame=ld.symSingleFrame||0;}
      if(ld.lfsGroup){state.layers[idx].lfsGroup=true;state.layers[idx].lfsIds=ld.lfsIds;state.layers[idx].lfsSettings=ld.lfsSettings;}
      ld.frames.forEach(function(f){if(!f.isInterpolated)f.isInterpolated=false;});while(state.layers[idx].frames.length<state.totalFrames)state.layers[idx].frames.push({strokes:[],isKeyframe:false,isInterpolated:false});});
    state.motionArcs=d.motionArcs||{};if(d.easingCurve){state.easingCurve=d.easingCurve;if(window._curveEditor)window._curveEditor.setState(d.easingCurve);}
    if(d.resamplePts)state.resamplePts=d.resamplePts;if(d.tweenStep)state.tweenStep=d.tweenStep;
    state.currentFrame=0;state.activeLayerIdx=0;activateUL(0);drawStage();loadFrame(0);renderOS();renderArcs();updateUI();renderSymbolTabs();
    syncDocFields();
    if(!silent)showToast('Projet chargé');}catch(e){showToast('Erreur: '+e.message);}},
  getState:function(){return state;},
};

// ---- UI UPDATE ----
// Width/Height/FPS/Frame-count are editable from two places (the Canvas/
// Timeline toolbar fields and the Project panel's Document fields) — both
// write through the same SM setters, so just re-stamp every duplicate field
// from state each time updateUI runs rather than tracking each writer.
function syncDocFields(){
  var els={'p-cw':state.canvasW,'p-ch':state.canvasH,'p-cbg':state.canvasBg,'tl-fps':state.fps,'tl-total':state.totalFrames,
    'proj-w':state.canvasW,'proj-h':state.canvasH,'proj-fps':state.fps,'proj-frames':state.totalFrames};
  Object.keys(els).forEach(function(id){var el=document.getElementById(id);if(el&&document.activeElement!==el)el.value=els[id];});
}
function updateUI(){
  syncDocFields();
  var strokes=getEffectiveStrokes(state.activeLayerIdx,state.currentFrame);
  document.getElementById('info-frame').textContent=state.currentFrame+1;
  document.getElementById('info-strokes').textContent=strokes.length+' trait'+(strokes.length!==1?'s':'');
  document.getElementById('tl-cf').textContent=state.currentFrame+1;
  document.getElementById('tl-tf').textContent=state.totalFrames;
  var f=state.layers[state.activeLayerIdx].frames[state.currentFrame];
  var badge=document.getElementById('info-badge');
  if(f&&f.isKeyframe){badge.style.display='inline-block';badge.className='badge key';badge.textContent='KEY';}
  else if(f&&f.isInterpolated){badge.style.display='inline-block';badge.className='badge tw';badge.textContent='TWEEN';}
  else badge.style.display='none';
  document.getElementById('info-sel').textContent=state.tool==='select'&&selectedPaths.length>0?selectedPaths.length+' selected':'';
  window._totalF=state.totalFrames;window._waIn=state.waIn;window._waOut=state.waOut;window._curFrame=state.currentFrame;
  window.updateWaBar();window.updateOmMarkers(state.currentFrame,state.totalFrames);
  renderTimeline();renderLayerList();updateCompInstancePanel();updateSelPropsPanel();updatePropsContext();
}

// ---- UNIFIED PROPERTIES PANEL (Figma/Graphite-style: one contextual panel
// instead of scattered always-visible sections) ----
// Priority, matching the explicit spec: 1) a selection exists -> Transform/
// Fill/Stroke/Effects describe THAT selection (reusing the exact same
// fields that already double as tool-default setters — see
// window.SM.setStrokeColor/setFillColor/etc, each already branches on
// `selectedPaths.length` to edit the live selection instead of just the
// future-draw default); 2) no selection but a drawing/shape tool is active
// -> the same Fill/Stroke fields now edit that tool's OWN defaults, plus
// Tool Options for the tool-specific knobs; 3) nothing selected, no
// special tool -> Document properties (canvas size/bg).
var FILL_STROKE_TOOLS=['draw','pen','eraser','fillbrush','line','rect','ellipse','fill'];
var TOOL_OPTS_TOOLS=['draw','pen','eraser','fillbrush'];
var TOOL_LABELS={draw:'Draw',pen:'Pen',eraser:'Eraser',fillbrush:'Fill Brush',line:'Line',rect:'Rectangle',ellipse:'Ellipse',fill:'Fill'};
var _selPropsSig='';
var _propsCtxSig=null;
function updatePropsContext(){
  var hasSel=(state.tool==='select'||state.tool==='subselect')&&selectedPaths.length>0;
  var ctx,hdrText;
  var show={'sel-props-sec':false,'fill-sec':false,'stroke-sec':false,'tool-opts-sec':false,'effects-sec':false,'canvas-sec':false};
  if(hasSel){
    ctx='selection';
    show['sel-props-sec']=show['fill-sec']=show['stroke-sec']=show['effects-sec']=true;
    hdrText=selectedPaths.length+(selectedPaths.length>1?' éléments sélectionnés':' élément sélectionné');
  }else if(FILL_STROKE_TOOLS.indexOf(state.tool)>=0){
    ctx='tool:'+state.tool;
    show['fill-sec']=show['stroke-sec']=true;
    if(TOOL_OPTS_TOOLS.indexOf(state.tool)>=0)show['tool-opts-sec']=true;
    hdrText=(TOOL_LABELS[state.tool]||state.tool)+' — Options';
  }else{
    ctx='document';
    show['canvas-sec']=true;
    hdrText='Document';
  }
  var hdrEl=document.getElementById('props-context-hdr');if(hdrEl)hdrEl.textContent=hdrText;
  Object.keys(show).forEach(function(id){var sec=document.getElementById(id);if(sec)sec.style.display=show[id]?'block':'none';});
  // state.drawMode (Front/Behind) has no effect on Fill Brush — it's always
  // inserted at the back regardless (see draw-bridge.js/tools.js commit) —
  // so showing that dropdown while Fill Brush is active was a dead control.
  // Swap it for the icon-based Above/Below/Merge row instead, which IS
  // wired to this tool specifically.
  var isFillBrush=state.tool==='fillbrush';
  var dmRow=document.getElementById('p-drawmode-row'),fbRow=document.getElementById('p-fillbrushmode-row');
  if(dmRow)dmRow.style.display=isFillBrush?'none':'flex';
  if(fbRow)fbRow.style.display=isFillBrush?'flex':'none';
  // Only auto-(re)expand sections on an actual context CHANGE — every
  // updateUI() tick calls this, and forcing every visible section back open
  // on each call would fight a user who deliberately collapsed one while
  // the selection/tool stays the same.
  if(ctx!==_propsCtxSig){
    _propsCtxSig=ctx;
    Object.keys(show).forEach(function(id){
      if(!show[id])return;
      var sec=document.getElementById(id);if(!sec)return;
      var h=sec.querySelector('.phdr'),b=sec.querySelector('.pbdy');
      if(h&&b){b.classList.remove('hid');h.classList.remove('closed');}
    });
  }
}
// Populates the Transform section's fields (position/size/rotation/point-
// type/boolean-op rows) — visibility itself is now owned by
// updatePropsContext(), called separately.
function updateSelPropsPanel(){
  if((state.tool!=='select'&&state.tool!=='subselect')||!selectedPaths.length){_selPropsSig='';return;}
  var b=xformSelBounds();if(!b)return;
  var sig=selectedPaths.map(function(p){return p.id;}).sort(function(a,c){return a-c;}).join(',');
  if(sig!==_selPropsSig){
    state.selRotAccum=0;_selPropsSig=sig;
    // Reflect the (first) selected path's actual fill into the panel swatch
    // — without this the swatch just keeps showing whatever the tool-default
    // fill last was, which goes stale the moment you select something with a
    // different (or no) fill. This only paints the DOM/state, it does not
    // go through setFillColor/setFillEnabled (those mutate every selected
    // path's fill, which is for user edits, not for passively reflecting
    // the existing selection).
    var ref=selectedPaths[0];
    if(ref){
      var hasFill=!!ref.fillColor;
      var css=hasFill?ref.fillColor.toCSS(true):state.fillColor;
      state.fillColor=css;state.fillEnabled=hasFill;
      document.getElementById('pm-fill').style.background=css;
      document.getElementById('pm-fill').classList.toggle('none',!hasFill);
      document.getElementById('pm-fill-c').value=css;
      document.getElementById('p-fill-on').checked=hasFill;
      // Same staleness fix for Cap/Join/Paint Order/Miter Limit/Dash Offset —
      // reflect the selected path's actual values instead of leaving
      // whatever the tool-default last was.
      if(!(ref.data&&ref.data.isVectorBrush)){
        state.strokeCap=ref.strokeCap||'round';state.strokeJoin=ref.strokeJoin||'round';
        state.miterLimit=ref.miterLimit||10;state.dashOffset=ref.dashOffset||0;
        state.paintOrder=(ref.data&&ref.data.paintOrder)||'fillFirst';
        paintIconGroup('p-cap-grp',state.strokeCap);paintIconGroup('p-join-grp',state.strokeJoin);
        paintIconGroup('p-paintorder-grp',state.paintOrder);syncMiterLimitEnabled();
        document.getElementById('p-miterlimit').value=state.miterLimit;
        document.getElementById('p-dashoffset').value=state.dashOffset;
      }
    }
  }
  document.getElementById('sel-count').textContent=selectedPaths.length+(selectedPaths.length>1?' selected':' item');
  var ax=document.activeElement;
  if(ax!==document.getElementById('sp-x'))document.getElementById('sp-x').value=Math.round(b.x);
  if(ax!==document.getElementById('sp-y'))document.getElementById('sp-y').value=Math.round(b.y);
  if(ax!==document.getElementById('sp-w'))document.getElementById('sp-w').value=Math.round(b.width);
  if(ax!==document.getElementById('sp-h'))document.getElementById('sp-h').value=Math.round(b.height);
  if(ax!==document.getElementById('sp-rot'))document.getElementById('sp-rot').value=Math.round(state.selRotAccum||0);
  document.getElementById('sp-pointtype-row').style.display=(state.tool==='subselect'&&_nodeSel.length)?'flex':'none';
  document.getElementById('sp-boolean-row').style.display=(state.tool==='select'&&selectedPaths.length>=2)?'flex':'none';
}
function selPropsApplyMove(dx,dy){
  if((!dx&&!dy)||!selectedPaths.length)return;
  pushUndo();
  selectedPaths.forEach(function(p){
    p.position=p.position.add(new Point(dx,dy));
    if(p.data&&p.data.isVectorBrush&&p.data.centerSegments)p.data.centerSegments.forEach(function(s){s.point=[s.point[0]+dx,s.point[1]+dy];});
    if(p.data&&p.data.linkedFill&&!p.data.linkedFill.removed)p.data.linkedFill.position=p.data.linkedFill.position.add(new Point(dx,dy));
  });
  fillRegenerateLinked(userLayers[state.activeLayerIdx],null);
  saveActiveLayerFrame();renderArcs();updateUI();
}
function selPropsApplyScale(sx,sy,anchor){
  if((sx===1&&sy===1)||!selectedPaths.length)return;
  pushUndo();
  selectedPaths.forEach(function(p){
    p.scale(sx,sy,anchor);
    if(p.data&&p.data.isVectorBrush&&p.data.centerSegments){scaleCenterSegments(p.data.centerSegments,sx,sy,anchor.x,anchor.y);rebuildVectorBrushOutline(p);}
  });
  fillRegenerateLinked(userLayers[state.activeLayerIdx],null);
  saveActiveLayerFrame();renderArcs();updateUI();
}
function selPropsApplyRotate(deltaDeg,center){
  if(!deltaDeg||!selectedPaths.length)return;
  pushUndo();
  selectedPaths.forEach(function(p){
    p.rotate(deltaDeg,center);
    if(p.data&&p.data.isVectorBrush&&p.data.centerSegments){rotateCenterSegments(p.data.centerSegments,deltaDeg,center.x,center.y);rebuildVectorBrushOutline(p);}
  });
  fillRegenerateLinked(userLayers[state.activeLayerIdx],null);
  saveActiveLayerFrame();renderArcs();updateUI();
}

// ---- TIMELINE DRAG STATE ----
var _tlDrag={active:false,startL:-1,startF:-1,ghost:null,moved:false,mode:null};

function renderTimeline(){
  var hdr=document.getElementById('frame-hdr'),grid=document.getElementById('frame-grid');hdr.innerHTML='';grid.innerHTML='';
  var fps=Math.max(1,state.fps);
  for(var i=0;i<state.totalFrames;i++){
    var c=document.createElement('div');c.className='fhc';if(i===state.currentFrame)c.classList.add('cur');
    // Second-boundary ticks (Animate-style ruler: "1s"/"2s" labels take
    // priority over the plain every-5-frames tick when they land on the
    // same cell) — gives a time-based read of the timeline, not just frames.
    if((i+1)%fps===0){c.classList.add('sec');c.textContent=Math.round((i+1)/fps)+'s';}
    else if((i+1)%5===0)c.textContent=i+1;
    hdr.appendChild(c);
  }
  // rows rendered top-to-bottom from the HIGHEST layer index — matching the
  // layer panel, which lists topmost (last-drawn-above) layers first. The
  // two lists previously ran in opposite orders, so after any reorder the
  // keyframe rows appeared to belong to the wrong layer.
  for(var li=state.layers.length-1;li>=0;li--){var row=document.createElement('div');row.className='frow';var ld=state.layers[li];
    for(var fi=0;fi<state.totalFrames;fi++){var cell=document.createElement('div');cell.className='fc';cell.dataset.frame=fi;cell.dataset.layer=li;
      if(fi===state.currentFrame)cell.classList.add('cur');if(fi<state.waIn||fi>state.waOut)cell.classList.add('outside-wa');
      if(selHas(li,fi))cell.classList.add('sel');
      var fr=ld.frames[fi];
      if(fr.isKeyframe){
        var mk=document.createElement('div');mk.className='km '+(fr.strokes.length>0?'fl':'hl');cell.appendChild(mk);
        // the keyframe cell itself carries the span tint so the band reads
        // as starting AT the key, not one cell after it (the hollow
        // end-rectangle only ever sits on held cells — a lone keyframe
        // shows just its dot, like Animate)
        cell.classList.add(fr.strokes.length>0?'kf-full':'kf-empty');
      }
      else if(fr.isInterpolated){cell.classList.add(fr.isManualEdit?'tw-manual':'tw');var td=document.createElement('div');td.className='km td'+(fr.isManualEdit?' manual':'');cell.appendChild(td);}
      else{
        // Extended ("held") frames read differently depending on whether
        // they trace back to an empty or a drawn keyframe, and the last
        // extended cell before the next keyframe gets an end-of-span tick
        // — both distinctions Animate shows and this app previously didn't.
        var hc=false,srcFound=false;
        for(var pi=fi;pi>=0;pi--){if(ld.frames[pi].isKeyframe){hc=ld.frames[pi].strokes.length>0;srcFound=true;break;}}
        if(srcFound){
          cell.classList.add(hc?'span-full':'span-empty');
          var nextFr=ld.frames[fi+1];
          if(!nextFr||nextFr.isKeyframe||nextFr.isInterpolated)cell.classList.add('span-end');
        }
      }
      row.appendChild(cell);}grid.appendChild(row);}
  document.getElementById('playhead').style.left=(state.currentFrame*FC)+'px';document.getElementById('playhead').style.height=(30+state.layers.length*24)+'px';
}

// ---- HELD-KEYFRAME SPAN SHRINK HANDLE ----
// A held/extended span's end (the small bracket drawn by .fc.span-end::after
// in CSS) has no stored length of its own — it's always wherever the next
// real keyframe happens to sit (see renderTimeline() above). Dragging this
// handle leftward "shortens the hold" by literally dropping a new keyframe
// at the drop frame (insertKeyframeAt), which becomes the new boundary —
// there's no other way to shrink a purely-derived span. Capture-phase so it
// runs before the existing mousedown handler below and can stopPropagation
// to prevent that handler's own select/drag logic from also firing when the
// user is grabbing this handle specifically (only the last ~6px of a
// .span-end cell counts as grabbing it).
var _spanShrink={active:false,li:-1,srcFi:-1,maxFi:-1};
document.getElementById('frame-grid').addEventListener('mousedown',function(e){
  if(e.button!==0)return;
  var cell=e.target.closest('.fc.span-end');if(!cell)return;
  var r=cell.getBoundingClientRect();
  if(e.clientX<r.right-6)return; // not grabbing the handle itself — let the normal cell click/drag handler run
  e.preventDefault();e.stopPropagation();
  var fi=parseInt(cell.dataset.frame),li=parseInt(cell.dataset.layer);
  var ld=state.layers[li];
  var srcFi=fi;for(var pi=fi;pi>=0;pi--){if(ld.frames[pi].isKeyframe){srcFi=pi;break;}}
  _spanShrink={active:true,li:li,srcFi:srcFi,maxFi:fi};
},true);
function _spanShrinkFrameAt(e){
  var wrap=document.getElementById('fg-wrap');var rect=wrap.getBoundingClientRect();
  var x=e.clientX-rect.left+wrap.scrollLeft;
  return Math.max(_spanShrink.srcFi+1,Math.min(_spanShrink.maxFi,Math.floor(x/FC)));
}
window.addEventListener('mousemove',function(e){
  if(!_spanShrink.active)return;
  var fi=_spanShrinkFrameAt(e);
  document.querySelectorAll('.fc.span-drag-preview').forEach(function(c){c.classList.remove('span-drag-preview');});
  var prevCell=document.querySelector('.fc[data-layer="'+_spanShrink.li+'"][data-frame="'+fi+'"]');
  if(prevCell)prevCell.classList.add('span-drag-preview');
});
window.addEventListener('mouseup',function(e){
  if(!_spanShrink.active)return;
  document.querySelectorAll('.fc.span-drag-preview').forEach(function(c){c.classList.remove('span-drag-preview');});
  var fi=_spanShrinkFrameAt(e);
  var li=_spanShrink.li;
  _spanShrink.active=false;
  if(fi<=_spanShrink.srcFi||fi>=_spanShrink.maxFi)return; // dropped back on the source key or the existing end — no-op
  insertKeyframeAt(li,fi);
});

// ---- FRAME GRID MOUSE HANDLERS (multi-select + drag) ----
// Matches Animate's convention: dragging from an *empty*/not-yet-selected
// frame extends a range SELECTION (marquee-style), while dragging from a
// cell that already holds a keyframe (or is already part of the current
// multi-selection) RELOCATES it. Without this split, every drag tried to
// move frames, which made simple "select a range" gestures relocate content.
document.getElementById('frame-grid').addEventListener('mousedown',function(e){
  if(e.button!==0)return;
  var cell=e.target.closest('.fc');if(!cell)return;
  var fi=parseInt(cell.dataset.frame),li=parseInt(cell.dataset.layer);
  if(state.playing)stopPlay();

  if(e.shiftKey){
    selRange(li,fi);selApplyCSS();
    if(li!==state.activeLayerIdx)window.SM.setActiveLayer(li);
    goToFrame(fi);return;
  }
  if(e.metaKey||e.ctrlKey){
    selToggle(li,fi);selApplyCSS();
    if(li!==state.activeLayerIdx)window.SM.setActiveLayer(li);
    goToFrame(fi);return;
  }

  // One predictable rule (Animate's): dragging from a cell that was
  // ALREADY selected before this press MOVES the selection; dragging from
  // anything else range-SELECTS. The old "any keyframe drag moves" variant
  // made range-selecting a run of keyframes impossible and moved content
  // when the user only meant to select it.
  var wasSelected=selHas(li,fi);

  if(!wasSelected){selClear();selAdd(li,fi);selApplyCSS();}

  if(li!==state.activeLayerIdx)window.SM.setActiveLayer(li);
  goToFrame(fi);

  // Clicking directly on a full (non-empty) keyframe also selects every
  // stroke it holds on the canvas, like clicking a layer's thumbnail in
  // Animate/After Effects selects its whole content — saves having to
  // marquee-select everything by hand right after jumping to that frame.
  // Only for a plain select-click, not a drag-to-move/range gesture (which
  // reuses this same mousedown before _tlDrag.mode is known — matches the
  // existing `!wasSelected` "this press starts a fresh selection" rule).
  if(!wasSelected){
    var ld0=state.layers[li];
    var fr0=ld0&&ld0.frames[fi];
    if(fr0&&fr0.isKeyframe&&fr0.strokes.length>0&&userLayers[li]){
      window.SM.setTool('select');
      selectedPaths=userLayers[li].children.filter(function(c){return c instanceof Path;});
      state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
      renderArcs();updateUI();
    }else if(selectedPaths.length){
      // Landing on a frame that does NOT qualify (empty keyframe, or a
      // held/tweened continuation cell) must drop whatever was selected on
      // the PREVIOUS frame — otherwise the transform box from an earlier
      // full-keyframe click kept showing on top of a completely different
      // frame's (possibly empty) content, even after clicking well away
      // from that original keyframe.
      clearSel();renderArcs();updateUI();
    }
  }

  _tlDrag.active=true;_tlDrag.startL=li;_tlDrag.startF=fi;_tlDrag.moved=false;_tlDrag.ghost=null;
  _tlDrag.mode=wasSelected?'move':'select';
});

// Double-click a cell selects its whole span — the run of frames belonging
// to the same keyframe (held frames + the key itself), or the whole tween
// stretch for interpolated cells. Matches Animate's span double-click.
document.getElementById('frame-grid').addEventListener('dblclick',function(e){
  var cell=e.target.closest('.fc');if(!cell)return;
  var fi=parseInt(cell.dataset.frame),li=parseInt(cell.dataset.layer);
  var ld=state.layers[li];if(!ld)return;
  var start=fi;
  while(start>0&&!ld.frames[start].isKeyframe)start--;
  var end=fi;
  while(end+1<state.totalFrames&&!ld.frames[end+1].isKeyframe)end++;
  selClear();
  for(var f=start;f<=end;f++)selAdd(li,f);
  selApplyCSS();
});

document.getElementById('frame-grid').addEventListener('contextmenu',function(e){
  e.preventDefault();
  var cell=e.target.closest('.fc');if(!cell)return;
  var fi=parseInt(cell.dataset.frame),li=parseInt(cell.dataset.layer);
  if(!selHas(li,fi)){selClear();selAdd(li,fi);selApplyCSS();}
  if(li!==state.activeLayerIdx)window.SM.setActiveLayer(li);
  goToFrame(fi);
  var hasClip=!!(_sel.clipboard&&_sel.clipboard.length);
  window.showContextMenu(e.clientX,e.clientY,[
    {label:'Copier',shortcut:'⌘C',action:function(){window.SM.copyFrames();}},
    {label:'Couper',shortcut:'⌘X',action:function(){window.SM.cutFrames();}},
    {label:'Coller',shortcut:'⌘V',disabled:!hasClip,action:function(){window.SM.pasteFrames();}},
    {label:'Dupliquer la sélection',action:function(){window.SM.duplicateSelectedFrames();}},
    {sep:true},
    {label:'Insérer une image',shortcut:'F5',action:function(){insertFrame();}},
    {label:'Insérer une image clé',shortcut:'F6',action:function(){insertKeyframe();}},
    {label:'Insérer une image clé vide',shortcut:'F7',action:function(){insertBlankKeyframe();}},
    {label:'Convertir en images clés',action:function(){window.SM.convertToKeyframes();}},
    {label:'Effacer l\'image clé',action:function(){window.SM.clearKeyframe();}},
    {sep:true},
    {label:'Supprimer les images',action:function(){window.SM.removeFrameSpan();}},
  ]);
});

document.getElementById('frame-grid').addEventListener('mousemove',function(e){
  if(!_tlDrag.active)return;
  var grid=document.getElementById('frame-grid');
  var gridRect=grid.getBoundingClientRect();
  // grid.getBoundingClientRect() already reflects current scroll position of fg-wrap,
  // so cursor-to-cell mapping needs no manual scroll/offset correction here.
  var xRel=e.clientX-gridRect.left;
  var yRel=e.clientY-gridRect.top;
  var toF=Math.max(0,Math.min(state.totalFrames-1,Math.floor(xRel/FC)));
  // grid rows run top-to-bottom from the highest layer index (see
  // renderTimeline), so the row under the cursor maps to a flipped index
  var toL=Math.max(0,Math.min(state.layers.length-1,state.layers.length-1-Math.floor(yRel/24)));

  if(!_tlDrag.moved){
    var dist=Math.abs(toF-_tlDrag.startF)+Math.abs(toL-_tlDrag.startL);
    if(dist<1)return;
    _tlDrag.moved=true;
    if(_tlDrag.mode==='move'&&!_sel.frames.length)selAdd(_tlDrag.startL,_tlDrag.startF);
  }

  if(_tlDrag.mode==='select'){
    _sel.frames=[];selAdd(_tlDrag.startL,_tlDrag.startF);
    var l0=Math.min(_tlDrag.startL,toL),l1=Math.max(_tlDrag.startL,toL);
    var f0=Math.min(_tlDrag.startF,toF),f1=Math.max(_tlDrag.startF,toF);
    for(var lx=l0;lx<=l1;lx++)for(var fx=f0;fx<=f1;fx++)selAdd(lx,fx);
    selApplyCSS();
    _tlDrag._toL=toL;_tlDrag._toF=toF;
    return;
  }

  if(!_tlDrag.ghost){
    _tlDrag.ghost=document.createElement('div');
    _tlDrag.ghost.className='tl-drag-ghost';
    _tlDrag.ghost.style.position='absolute';_tlDrag.ghost.style.top='0';_tlDrag.ghost.style.left='0';_tlDrag.ghost.style.pointerEvents='none';_tlDrag.ghost.style.zIndex='12';
    document.getElementById('fg-wrap').appendChild(_tlDrag.ghost);
  }
  if(!_tlDrag.cursorDot){
    _tlDrag.cursorDot=document.createElement('div');
    _tlDrag.cursorDot.style.cssText='position:absolute;top:0;left:0;width:9px;height:9px;border-radius:50%;background:var(--accent);box-shadow:0 0 0 3px rgba(74,158,255,.3);pointer-events:none;z-index:13;transform:translate(-50%,-50%);';
    document.getElementById('fg-wrap').appendChild(_tlDrag.cursorDot);
  }

  var b=selBounds();if(!b){_tlDrag.active=false;return;}
  var offL=toL-_tlDrag.startL,offF=toF-_tlDrag.startF;
  // ghost is a child of fg-wrap, not frame-grid, so its absolute coords must
  // include grid's own offset (bars-row + frame-hdr height) within fg-wrap.
  // offsetLeft/offsetTop are layout-space (scroll-independent), unlike getBoundingClientRect().
  var gridOffLeft=grid.offsetLeft,gridOffTop=grid.offsetTop;

  // free-floating dot tracks the literal mouse position continuously
  // (not snapped to frame columns), so the drag visually feels like the
  // keyframe point is following the cursor across the timeline, while the
  // ghost cell below still shows the precise snapped drop target.
  var clampedX=Math.max(0,Math.min(state.totalFrames*FC,xRel));
  var clampedY=Math.max(12,Math.min(state.layers.length*24-12,yRel));
  _tlDrag.cursorDot.style.left=(clampedX+gridOffLeft)+'px';
  _tlDrag.cursorDot.style.top=(clampedY+gridOffTop)+'px';

  _tlDrag.ghost.innerHTML='';
  _sel.frames.forEach(function(s){
    var gl=s.layer+offL,gf=s.frame+offF;
    if(gl<0||gl>=state.layers.length||gf<0||gf>=state.totalFrames)return;
    var d=document.createElement('div');
    d.style.cssText='position:absolute;width:'+FC+'px;height:24px;background:rgba(74,158,255,.35);border:1px solid rgba(74,158,255,.7);border-radius:2px;box-sizing:border-box;';
    d.style.left=(gf*FC+gridOffLeft)+'px';d.style.top=((state.layers.length-1-gl)*24+gridOffTop)+'px';
    var fr=state.layers[s.layer].frames[s.frame];
    if(fr&&fr.isKeyframe){
      var dot=document.createElement('div');
      dot.style.cssText='width:7px;height:7px;border-radius:50%;background:'+(fr.strokes.length>0?'var(--key-color)':'transparent')+';border:'+(fr.strokes.length>0?'none':'1.5px solid var(--key-color)')+';position:absolute;top:50%;left:50%;transform:translate(-50%,-50%);';
      d.appendChild(dot);
    }
    _tlDrag.ghost.appendChild(d);
  });

  _tlDrag._toL=toL;_tlDrag._toF=toF;
});

window.addEventListener('mouseup',function(){
  if(!_tlDrag.active)return;
  if(_tlDrag.moved&&_tlDrag.ghost&&_sel.frames.length>0){
    var offL=_tlDrag._toL-_tlDrag.startL;
    var offF=_tlDrag._toF-_tlDrag.startF;
    if(offL!==0||offF!==0){
      var b=selBounds();
      window.SM.moveFrames(_sel.frames.slice(),b.minL+offL,b.minF+offF);
    }
    _tlDrag.ghost.remove();_tlDrag.ghost=null;
  }
  if(_tlDrag.cursorDot){_tlDrag.cursorDot.remove();_tlDrag.cursorDot=null;}
  _tlDrag.active=false;_tlDrag.moved=false;_tlDrag.mode=null;
});

// Multi-selection of layer rows (Cmd/Ctrl+click toggles, Shift+click
// ranges); operations like Delete apply to the whole set.
var _layerSel=[];
// Inline rename — window.prompt() is silently ignored by Tauri's WKWebView,
// so the row's label swaps to a real text input instead.
function startLayerRename(idx){
  var row=document.querySelector('.lrow[data-layer="'+idx+'"]');if(!row)return;
  var nm=row.querySelector('.lnm');if(!nm)return;
  var input=document.createElement('input');input.type='text';input.value=state.layers[idx].name;
  input.style.cssText='width:100%;background:var(--bg);border:1px solid var(--accent);color:var(--text);font-size:11px;border-radius:4px;padding:1px 4px;outline:none;';
  nm.innerHTML='';nm.appendChild(input);input.focus();input.select();
  var done=false;
  function commit(){if(done)return;done=true;var v=input.value.trim();if(v)window.SM.renameLayer(idx,v);else updateUI();}
  input.addEventListener('keydown',function(e){e.stopPropagation();if(e.key==='Enter')commit();else if(e.key==='Escape'){done=true;updateUI();}});
  input.addEventListener('blur',commit);
  input.addEventListener('mousedown',function(e){e.stopPropagation();});
  input.addEventListener('dblclick',function(e){e.stopPropagation();});
}
function renderLayerList(){
  var list=document.getElementById('layer-list');list.innerHTML='';
  for(var i=state.layers.length-1;i>=0;i--){var ld=state.layers[i];var row=document.createElement('div');row.className='lrow'+(ld.symbolId?' is-comp':'');row.dataset.layer=i;if(i===state.activeLayerIdx)row.classList.add('act');
    if(_layerSel.indexOf(i)>=0)row.classList.add('sel');
    var eye=document.createElement('div');eye.className='lico'+(ld.visible?'':' off');eye.title='Show / hide layer';eye.innerHTML='<span class="material-symbols-rounded">'+(ld.visible?'\u{e8f4}':'\u{e8f5}')+'</span>';eye.dataset.layer=i;eye.addEventListener('click',function(e){e.stopPropagation();window.SM.toggleLayerVis(parseInt(this.dataset.layer));});
    // '\u{e898}' (lock_open) isn't in this project's subsetted Material
    // Symbols font (only glyphs actually referenced elsewhere got embedded
    // — confirmed live: it rendered as a totally empty/invisible glyph,
    // which is why the icon looked "not visible when inactive" even after
    // the first fix). Same lock glyph in both states, dimmed via the
    // existing `.off` class instead — matches the eye icon's convention of
    // always showing SOME icon and varying just its color/emphasis.
    var lock=document.createElement('div');lock.className='lico'+(ld.locked?'':' off');lock.title='Lock / unlock layer';lock.innerHTML='<span class="material-symbols-rounded">\u{e899}</span>';lock.dataset.layer=i;lock.addEventListener('click',function(e){e.stopPropagation();window.SM.toggleLayerLock(parseInt(this.dataset.layer));});
    var nm=document.createElement('div');nm.className='lnm';nm.textContent=ld.name;
    row.appendChild(eye);row.appendChild(lock);
    if(ld.symbolId){var cb=document.createElement('div');cb.className='lico comp-badge';cb.title='Component — double-click to edit';cb.innerHTML='<span style="font-size:11px;line-height:1">\u25c8</span>';row.appendChild(cb);}
    if(ld.lfsGroup){var lb=document.createElement('div');lb.className='lico comp-badge';lb.title='Ligne/Plein/Ombre layer';lb.innerHTML='<span style="font-size:11px;line-height:1">LFS</span>';row.appendChild(lb);}
    row.appendChild(nm);
    row.addEventListener('click',function(e){
      // A completed drag-drop still fires a trailing native 'click' on
      // mouseup (browsers do this whenever mousedown/mouseup land on
      // related targets, regardless of movement in between) — by the time
      // it fires, _layerDrag.moved has ALREADY been reset to false by the
      // drag's own mouseup handler below, so this handler had no way to
      // tell "a drag just ended" and unconditionally ran the plain-click
      // branch (_layerSel=[idx]), collapsing a multi-selection back down to
      // one row immediately after every successful batch reorder — the
      // reorder itself worked, but the NEXT drag attempt then looked like
      // "batch reorder doesn't work" because the selection had silently
      // shrunk to one item. Same fix pattern as _secDragJustEnded elsewhere
      // in this file (floating-section drag).
      if(window._layerDragJustEnded){window._layerDragJustEnded=false;return;}
      var idx=parseInt(this.dataset.layer);
      if(e.metaKey||e.ctrlKey){
        if(_layerSel.indexOf(state.activeLayerIdx)<0)_layerSel.push(state.activeLayerIdx);
        var p=_layerSel.indexOf(idx);if(p>=0)_layerSel.splice(p,1);else _layerSel.push(idx);
      }else if(e.shiftKey&&_layerSel.length){
        var anchor=_layerSel[0];_layerSel=[];
        for(var l=Math.min(anchor,idx);l<=Math.max(anchor,idx);l++)_layerSel.push(l);
      }else _layerSel=[idx];
      window.SM.setActiveLayer(idx);
    });
    row.addEventListener('dblclick',function(){var idx3=parseInt(this.dataset.layer);var l2=state.layers[idx3];if(l2.symbolId){window.SM.enterSymbol(l2.symbolId);return;}if(l2.lfsGroup){window.SM.enterSymbol(l2.lfsIds.full);return;}startLayerRename(idx3);});
    row.addEventListener('mousedown',function(e){
      if(e.button!==0||e.target.closest('.lico'))return;
      _layerDrag.active=true;_layerDrag.srcIdx=parseInt(this.dataset.layer);_layerDrag.startY=e.clientY;_layerDrag.moved=false;
    });
    row.addEventListener('contextmenu',function(e){
      e.preventDefault();
      var idx4=parseInt(this.dataset.layer);window.SM.setActiveLayer(idx4);
      var l4=state.layers[idx4];
      window.showContextMenu(e.clientX,e.clientY,[
        {label:'Insérer un calque',action:function(){window.SM.addLayer();}},
        {label:'Dupliquer le calque',action:function(){window.SM.duplicateLayer();}},
        {label:'Supprimer le calque',action:function(){window.SM.deleteLayer();}},
        {sep:true},
        {label:'Renommer',action:function(){startLayerRename(idx4);}},
        {label:'Convertir en composant',disabled:!!l4.symbolId||!!l4.lfsGroup,action:function(){window.SM.convertActiveLayerToComponent();}},
        {label:'Décomposer le composant',disabled:!l4.symbolId,action:function(){window.SM.convertComponentToLayer();}},
        {sep:true},
        {label:'Grouper (Ligne/Plein/Ombre)',disabled:!!l4.symbolId||!!l4.lfsGroup,action:function(){window.SM.convertActiveLayerToLFSGroup();}},
        {label:'Éditer Ligne',disabled:!l4.lfsGroup,action:function(){window.SM.enterSymbol(l4.lfsIds.line);}},
        {label:'Éditer Plein',disabled:!l4.lfsGroup,action:function(){window.SM.enterSymbol(l4.lfsIds.full);}},
        {label:'Éditer Ombre',disabled:!l4.lfsGroup,action:function(){window.SM.enterSymbol(l4.lfsIds.shadow);}},
        {label:'Propager Plein sur les autres images',disabled:!l4.lfsGroup,action:function(){window.SM.propagateLFSFill('full');}},
        {label:'Propager Ombre sur les autres images',disabled:!l4.lfsGroup,action:function(){window.SM.propagateLFSFill('shadow');}},
        {label:'Décomposer le groupe',disabled:!l4.lfsGroup,action:function(){window.SM.convertLFSGroupToLayer();}},
      ]);
    });
    list.appendChild(row);}
}
// Manual mouse-based drag-to-reorder (kept consistent with the frame grid's
// custom drag rather than HTML5 draggable, which behaves inconsistently
// inside the Tauri webview).
var _layerDrag={active:false,srcIdx:-1,startY:0,moved:false};
window.addEventListener('mousemove',function(e){
  if(!_layerDrag.active)return;
  if(!_layerDrag.moved){
    if(Math.abs(e.clientY-_layerDrag.startY)<4)return;
    _layerDrag.moved=true;
    var src=document.querySelector('.lrow[data-layer="'+_layerDrag.srcIdx+'"]');if(src)src.classList.add('dragging');
  }
  document.querySelectorAll('.lrow').forEach(function(r){r.classList.remove('drag-over');});
  var el=document.elementFromPoint(e.clientX,e.clientY);
  var row=el&&el.closest('.lrow');
  if(row)row.classList.add('drag-over');
});
window.addEventListener('mouseup',function(){
  if(!_layerDrag.active)return;
  if(_layerDrag.moved){
    window._layerDragJustEnded=true;
    var overRow=document.querySelector('.lrow.drag-over');
    if(overRow){
      var destIdx=parseInt(overRow.dataset.layer);
      // Dragging one row of an active multi-selection moves the WHOLE
      // selection together, as one contiguous block — dragging a row that
      // ISN'T part of the current selection still moves just that one row
      // (matches how most apps treat a drag starting outside the selection
      // as replacing it with a single-item drag).
      if(_layerSel.length>1&&_layerSel.indexOf(_layerDrag.srcIdx)>=0&&_layerSel.indexOf(destIdx)<0){
        window.SM.reorderLayersBatch(_layerSel,destIdx);
      }else if(destIdx!==_layerDrag.srcIdx){
        window.SM.reorderLayer(_layerDrag.srcIdx,destIdx);
      }
    }
  }
  document.querySelectorAll('.lrow').forEach(function(r){r.classList.remove('dragging','drag-over');});
  _layerDrag.active=false;_layerDrag.moved=false;
});
function renderSymbolTabs(){
  var bar=document.getElementById('symbol-tabs');if(!bar)return;bar.innerHTML='';
  var scene=document.createElement('div');scene.className='sym-tab'+(state.activeSymbolId?'':' act');scene.textContent='Scene';
  scene.addEventListener('click',function(){if(state.activeSymbolId)window.SM.exitToScene();});
  bar.appendChild(scene);
  state.openSymbolTabs.forEach(function(symId){
    var sym=state.symbols[symId];if(!sym)return;
    var tab=document.createElement('div');tab.className='sym-tab'+(state.activeSymbolId===symId?' act':'');
    var label=document.createElement('span');label.textContent=sym.name;
    var x=document.createElement('span');x.className='sym-tab-x';x.textContent='×';
    tab.appendChild(label);tab.appendChild(x);
    tab.addEventListener('click',function(e){if(e.target===x)return;window.SM.enterSymbol(symId);});
    x.addEventListener('click',function(e){e.stopPropagation();window.SM.closeSymbolTab(symId);});
    bar.appendChild(tab);
  });
}
function updateCompInstancePanel(){
  var sec=document.getElementById('comp-instance-sec');
  if(state.activeSymbolId){sec.style.display='none';return;}
  var ld=state.layers[state.activeLayerIdx];
  if(!ld||!ld.symbolId){sec.style.display='none';return;}
  sec.style.display='block';
  document.getElementById('comp-playmode').value=ld.symPlayMode||'loop';
  document.getElementById('comp-singleframe-row').style.display=(ld.symPlayMode==='single')?'flex':'none';
  document.getElementById('comp-singleframe').value=ld.symSingleFrame||0;
  document.getElementById('comp-speed').value=ld.symSpeed||1;
  document.getElementById('comp-offset').value=ld.symPlacedAt||0;
  renderCompFrameStrip(ld);
}
// Frame-strip: one clickable cell per frame inside the component's own
// timeline. Clicking a cell jumps to Single-Frame mode on that frame — the
// closest equivalent to a time-remap pick without a full curve editor. The
// cell matching what's actually resolved right now (via resolveSymbolFrameIdx,
// same math the renderer uses) is highlighted so the strip always shows
// "what's playing" even in loop/ping-pong/once modes.
function renderCompFrameStrip(ld){
  var strip=document.getElementById('comp-frame-strip');strip.innerHTML='';
  var sym=state.symbols[ld.symbolId];if(!sym)return;
  var total=Math.max(1,sym.totalFrames);
  var current=resolveSymbolFrameIdx(sym,ld,state.currentFrame);
  for(var i=0;i<total;i++){
    var cell=document.createElement('div');
    cell.style.cssText='width:14px;height:14px;flex:0 0 auto;border-radius:2px;cursor:pointer;font-size:8px;display:flex;align-items:center;justify-content:center;';
    var isCur=i===current;
    var isSingleSel=ld.symPlayMode==='single'&&i===(ld.symSingleFrame||0);
    cell.style.background=isSingleSel?'var(--accent)':(isCur?'rgba(255,255,255,.25)':'var(--bg)');
    cell.style.border='1px solid '+(isCur?'var(--accent)':'rgba(255,255,255,.15)');
    cell.title='Frame '+i;
    cell.addEventListener('click',function(idx){return function(){window.SM.setSymbolSingleFrame(idx);window.SM.setSymbolPlayMode('single');updateCompInstancePanel();};}(i));
    strip.appendChild(cell);
  }
}
function showToast(m){var el=document.getElementById('toast');el.textContent=m;el.classList.add('show');clearTimeout(window._toastT);window._toastT=setTimeout(function(){el.classList.remove('show');},2500);}

// ---- KEYBOARD ----
function onKeyDown(event){
  if((event.metaKey||event.ctrlKey)&&event.key==='z'){event.preventDefault();if(event.shiftKey)redo();else undo();return;}
  if((event.metaKey||event.ctrlKey)&&event.key==='s'){event.preventDefault();if(event.shiftKey)window.SMProject.saveAs();else window.SMProject.save();return;}
  if((event.metaKey||event.ctrlKey)&&event.key==='c'){event.preventDefault();window.SM.copyFrames();return;}
  if((event.metaKey||event.ctrlKey)&&event.key==='x'){event.preventDefault();window.SM.cutFrames();return;}
  if((event.metaKey||event.ctrlKey)&&event.key==='v'){event.preventDefault();window.SM.pasteFrames();return;}
  if(event.target.tagName==='INPUT'||event.target.tagName==='SELECT')return;
  var k=event.key;
  if(k==='b'||k==='B')window.SM.setTool('draw');
  else if(k==='v'||k==='V')window.SM.setTool('select');
  else if(k==='a'||k==='A')window.SM.setTool('subselect');
  else if(k==='p'||k==='P')window.SM.setTool('pen');
  else if(k==='u'||k==='U')window.SM.setTool('line');
  else if(k==='r'||k==='R')window.SM.setTool('rect');
  else if(k==='l'||k==='L')window.SM.setTool('ellipse');
  else if(k==='e'||k==='E')window.SM.setTool('eraser');
  else if(k==='g'||k==='G')window.SM.setTool('fill');
  else if(k==='k'||k==='K')window.SM.setTool('fillbrush');
  else if(k==='i')window.SM.setTool('eyedropper');
  else if(k==='h'||k==='H')window.SM.setTool('hand');
  else if(k==='z')window.SM.setTool('zoom');
  else if(k==='o'||k==='O')window.SM.toggleOnion();
  else if(k==='t'||k==='T')window.SM.generateTweens();
  else if(k===' '){event.preventDefault();if(!state.spaceDown){state.spaceDown=true;canvasEl.style.cursor='grab';}}
  else if(k==='Enter'){event.preventDefault();if(state.tool==='pen'&&_pen.path)finalizePen();else togglePlay();}
  else if(k==='Escape'){if(state.tool==='pen'&&_pen.path){if(_pen.previewLine){_pen.previewLine.remove();_pen.previewLine=null;}_pen.path.remove();if(state.undoStack.length)state.undoStack.pop();_pen.path=null;_pen.draggingHandle=false;saveActiveLayerFrame();updateUI();}}
  else if(k==='ArrowLeft'){if(state.playing)stopPlay();goToFrame(state.currentFrame-1);}
  else if(k==='ArrowRight'){if(state.playing)stopPlay();goToFrame(state.currentFrame+1);}
  else if(k==='Delete'||k==='Backspace'){
    // Pen tool mid-draw: standard Illustrator/Figma "undo the last placed
    // anchor, keep drawing from the one before it" — previously the only
    // way out of a misplaced point was Escape, which threw the WHOLE
    // in-progress path away.
    if(state.tool==='pen'&&_pen.path){
      event.preventDefault();
      if(_pen.path.segments.length<=1){
        if(_pen.previewLine){_pen.previewLine.remove();_pen.previewLine=null;}
        _pen.path.remove();if(state.undoStack.length)state.undoStack.pop();
        _pen.path=null;_pen.draggingHandle=false;
      }else{
        _pen.path.removeSegment(_pen.path.segments.length-1);
      }
      saveActiveLayerFrame();updateUI();
    }
    else if(_sel.frames.length>0){event.preventDefault();window.SM.deleteSelectedFrames();}
    else if(state.tool==='select'&&selectedPaths.length>0)window.SM.deleteSelStrokes();
    else if(event.shiftKey)removeFrame();
  }
  else if(k==='F5'){event.preventDefault();insertFrame();}
  else if(k==='F6'){event.preventDefault();insertKeyframe();}
  else if(k==='F7'){event.preventDefault();insertBlankKeyframe();}
  else if(k==='d'||k==='D'){window.SM.duplicateKeyframe();}
  else if(k==='f'||k==='F'){if(!event.shiftKey)window.SM.flipPreview();}
  else if(k==='x'||k==='X'){if(event.shiftKey)window.SM.flipVertical();else window.SM.flipHorizontal();}
  else if(k==='+'||k==='='){window.SM.extendExposure(1);}
}
function onKeyUp(event){if(event.key===' '){state.spaceDown=false;state.isPanning=false;var cc={draw:'crosshair',pen:'crosshair',line:'crosshair',rect:'crosshair',ellipse:'crosshair',select:'default',subselect:'default',eraser:'pointer',fill:'crosshair',fillbrush:'crosshair',eyedropper:'crosshair',hand:'grab',zoom:'zoom-in'};canvasEl.style.cursor=cc[state.tool]||'default';}}

document.addEventListener('keydown',onKeyDown);document.addEventListener('keyup',onKeyUp);
document.querySelectorAll('.tool-btn').forEach(function(b){b.addEventListener('click',function(){window.SM.setTool(this.dataset.tool);});});
document.getElementById('color-stroke').addEventListener('input',function(){window.SM.setStrokeColor(this.value);});
document.getElementById('pm-stroke-c').addEventListener('input',function(){window.SM.setStrokeColor(this.value);document.getElementById('color-stroke').value=this.value;});
document.getElementById('p-sw').addEventListener('change',function(){window.SM.setBrushSize(parseInt(this.value));});
// Actively picking a fill color also ENABLES fill (Graphite behavior) —
// without this, the default-off fill state made "I set my fill to red, drew,
// and nothing filled" the reported first-run experience: the only visible
// difference was the small diagonal-line "off" indicator on the swatch, and
// the only way to enable was the (undiscoverable) double-click. These are
// real 'input' events fired only by user interaction (native picker or the
// custom popover writing into the input) — programmatic .value writes on
// load/selection-sync don't dispatch events, so they can't re-enable fill
// behind the user's back.
document.getElementById('color-fill').addEventListener('input',function(){window.SM.setFillColor(this.value);if(!state.fillEnabled)window.SM.setFillEnabled(true);});
document.getElementById('pm-fill-c').addEventListener('input',function(){window.SM.setFillColor(this.value);document.getElementById('color-fill').value=this.value;if(!state.fillEnabled)window.SM.setFillEnabled(true);});
document.getElementById('p-fill-on').addEventListener('change',function(){window.SM.setFillEnabled(this.checked);});
// Paint the panel's fill swatch from the actual starting state.fillColor/
// fillEnabled on load — without this it sits at whatever background the
// static HTML happened to have (transparent) until the user touches it.
window.SM.setFillColor(state.fillColor);
window.SM.setFillEnabled(state.fillEnabled);
if(window.ColorPicker){
  window.ColorPicker.wireColorSwatches([
    {wrap:'stroke-well',input:'color-stroke',onEyedrop:function(){window.SM.setTool('eyedropper');}},
    {wrap:'pm-stroke',input:'pm-stroke-c',onEyedrop:function(){window.SM.setTool('eyedropper');}},
    {wrap:'fill-well',input:'color-fill',onNone:function(){window.SM.setFillEnabled(false);},onEyedrop:function(){window.SM.setTool('eyedropper');}},
    {wrap:'pm-fill',input:'pm-fill-c',onNone:function(){window.SM.setFillEnabled(false);},onEyedrop:function(){window.SM.setTool('eyedropper');}},
  ]);
}
document.getElementById('p-opacity').addEventListener('input',function(){window.SM.setOpacity(this.value);});
document.getElementById('p-smooth').addEventListener('input',function(){window.SM.setSmoothing(parseInt(this.value));});
document.getElementById('p-stab').addEventListener('change',function(){window.SM.setStabilizer(this.value);});
document.getElementById('p-strokestyle').addEventListener('change',function(){window.SM.setStrokeStyle(this.value);});
document.getElementById('p-miterlimit').addEventListener('change',function(){window.SM.setMiterLimit(this.value);});
document.getElementById('p-dashoffset').addEventListener('change',function(){window.SM.setDashOffset(this.value);});
document.getElementById('btn-poststroke-smooth').addEventListener('click',function(){window.SM.smoothSelectedStroke(document.getElementById('p-poststroke-smooth').value);});
// Icon-button groups (Cap/Join/Paint Order) — clicking a button selects it
// (single-choice, like a radio group) and calls the matching SM setter.
// `data-value` on the wrapper always mirrors the currently-selected
// button's value, both for external reads and so re-running this same
// paint logic (e.g. on selection change) is a single lookup.
function wireIconGroup(wrapId,setterName){
  var wrap=document.getElementById(wrapId);
  wrap.addEventListener('click',function(e){
    var btn=e.target.closest('.icon-btn');if(!btn)return;
    wrap.querySelectorAll('.icon-btn').forEach(function(b){b.classList.toggle('sel',b===btn);});
    wrap.dataset.value=btn.dataset.value;
    window.SM[setterName](btn.dataset.value);
  });
}
function paintIconGroup(wrapId,value){
  var wrap=document.getElementById(wrapId);if(!wrap)return;
  wrap.dataset.value=value;
  wrap.querySelectorAll('.icon-btn').forEach(function(b){b.classList.toggle('sel',b.dataset.value===value);});
}
wireIconGroup('p-cap-grp','setStrokeCap');
wireIconGroup('p-join-grp','setStrokeJoin');
wireIconGroup('p-paintorder-grp','setPaintOrder');
wireIconGroup('p-fillbrushmode-grp','setFillBrushMode');
paintIconGroup('p-cap-grp',state.strokeCap);
paintIconGroup('p-join-grp',state.strokeJoin);
paintIconGroup('p-paintorder-grp',state.paintOrder);
paintIconGroup('p-fillbrushmode-grp',state.fillBrushMode);
// Miter Limit only affects sharp corners under Join:Miter — disabled
// (same convention as the old p-miterlimit-adjacent fields elsewhere)
// otherwise, since it's a no-op for Round/Bevel joins.
function syncMiterLimitEnabled(){document.getElementById('p-miterlimit').disabled=(document.getElementById('p-join-grp').dataset.value!=='miter');}
document.getElementById('p-join-grp').addEventListener('click',syncMiterLimitEnabled);
syncMiterLimitEnabled();
document.getElementById('p-vecbrush').addEventListener('change',function(){window.SM.setVectorBrush(this.checked);});
document.getElementById('sp-x').addEventListener('change',function(){var b=xformSelBounds();if(!b)return;selPropsApplyMove((parseFloat(this.value)||0)-b.x,0);});
document.getElementById('sp-y').addEventListener('change',function(){var b=xformSelBounds();if(!b)return;selPropsApplyMove(0,(parseFloat(this.value)||0)-b.y);});
document.getElementById('sp-w').addEventListener('change',function(){var b=xformSelBounds();if(!b||b.width<0.01)return;var nv=Math.max(0.01,parseFloat(this.value)||b.width);selPropsApplyScale(nv/b.width,1,b.topLeft);});
document.getElementById('sp-h').addEventListener('change',function(){var b=xformSelBounds();if(!b||b.height<0.01)return;var nv=Math.max(0.01,parseFloat(this.value)||b.height);selPropsApplyScale(1,nv/b.height,b.topLeft);});
document.getElementById('sp-rot').addEventListener('change',function(){var b=xformSelBounds();if(!b)return;var nv=parseFloat(this.value)||0;var delta=nv-(state.selRotAccum||0);state.selRotAccum=nv;selPropsApplyRotate(delta,b.center);});
document.getElementById('btn-pt-corner').addEventListener('click',function(){window.SM.setPointType('corner');});
document.getElementById('btn-pt-smooth').addEventListener('click',function(){window.SM.setPointType('smooth');});
document.getElementById('btn-pt-symmetric').addEventListener('click',function(){window.SM.setPointType('symmetric');});
document.getElementById('btn-bool-unite').addEventListener('click',function(){window.SM.booleanOp('unite');});
document.getElementById('btn-bool-subtract').addEventListener('click',function(){window.SM.booleanOp('subtract');});
document.getElementById('btn-bool-intersect').addEventListener('click',function(){window.SM.booleanOp('intersect');});
document.getElementById('btn-bool-exclude').addEventListener('click',function(){window.SM.booleanOp('exclude');});
document.getElementById('p-erasersize').addEventListener('input',function(){window.SM.setEraserSize(this.value);});
document.getElementById('p-drawmode').addEventListener('change',function(){window.SM.setDrawMode(this.value);});
document.getElementById('p-pmin').addEventListener('input',function(){window.SM.setPressureMin(this.value);});
document.getElementById('p-pmax').addEventListener('input',function(){window.SM.setPressureMax(this.value);});
document.getElementById('p-pinv').addEventListener('change',function(){window.SM.setPressureInvert(this.checked);});
document.getElementById('p-taper').addEventListener('change',function(){window.SM.setTaperEnds(this.checked);});
document.getElementById('p-cw').addEventListener('change',function(){window.SM.setCanvasSize(parseInt(this.value),state.canvasH);});
document.getElementById('p-ch').addEventListener('change',function(){window.SM.setCanvasSize(state.canvasW,parseInt(this.value));});
document.getElementById('p-cbg').addEventListener('input',function(){window.SM.setCanvasBg(this.value);});
// Same canvas/fps/frame-count controls, duplicated in the Project panel —
// both sets write through the same SM.setCanvasSize/setFps/setTotalFrames
// so either one stays in sync via syncDocFields().
document.getElementById('proj-w').addEventListener('change',function(){window.SM.setCanvasSize(parseInt(this.value),state.canvasH);});
document.getElementById('proj-h').addEventListener('change',function(){window.SM.setCanvasSize(state.canvasW,parseInt(this.value));});
document.getElementById('proj-fps').addEventListener('change',function(){window.SM.setFps(parseInt(this.value));});
document.getElementById('proj-frames').addEventListener('change',function(){window.SM.setTotalFrames(parseInt(this.value));});
document.getElementById('btn-fit').addEventListener('click',function(){window.SM.fitCanvas();});
document.getElementById('btn-resetv').addEventListener('click',function(){window.SM.resetView();});
document.getElementById('p-resamp').addEventListener('input',function(){window.SM.setResamplePts(parseInt(this.value));});
document.getElementById('p-step').addEventListener('change',function(){window.SM.setTweenStep(this.value);});
document.getElementById('p-skipmanual').addEventListener('change',function(){state.tweenSkipManual=this.checked;});
document.getElementById('btn-tw').addEventListener('click',function(){window.SM.generateTweens();});
document.getElementById('btn-os').addEventListener('click',function(){window.SM.toggleOnion();});
document.getElementById('btn-os-outline').addEventListener('click',function(){
  var v=state.onionMode==='outline'?'tinted':'outline';window.SM.setOnionMode(v);
  this.classList.toggle('active',v==='outline');document.getElementById('p-omode').value=v;
});
// Animate's "Modify Onion Markers" menu — presets for the marker span
// around the playhead, plus the anchor/follow toggle (markers travel with
// the playhead by default, exactly like Animate; anchoring pins them).
function setOnionSpan(prev,next){
  var omIn=document.getElementById('om-in'),omOut=document.getElementById('om-out');
  window._omSpan=prev<0?{prev:9999,next:9999}:{prev:prev,next:next};
  var cf=state.currentFrame;
  var inF=prev<0?0:Math.max(0,cf-prev);
  var outF=prev<0?state.totalFrames-1:Math.min(state.totalFrames-1,cf+next);
  omIn.dataset.frame=inF;omOut.dataset.frame=outF;
  window.SM.setOnionRange(inF,outF);
  window.updateOmMarkers(cf,state.totalFrames);
}
document.getElementById('btn-os-range').addEventListener('click',function(e){
  e.stopPropagation();
  var r=this.getBoundingClientRect();
  window.showContextMenu(r.left,r.top-160,[
    {label:'Onion ±1',action:function(){setOnionSpan(1,1);}},
    {label:'Onion ±2',action:function(){setOnionSpan(2,2);}},
    {label:'Onion ±5',action:function(){setOnionSpan(5,5);}},
    {label:'Toutes les images',action:function(){setOnionSpan(-1,-1);}},
    {sep:true},
    {label:(window._omFollow?'✓ ':'')+'Marqueurs suivent le curseur',action:function(){window._omFollow=!window._omFollow;window.updateOmMarkers(state.currentFrame,state.totalFrames);}},
  ]);
});
document.getElementById('btn-loop').addEventListener('click',function(){window.SM.toggleLoopPlayback();});
document.getElementById('p-omode').addEventListener('change',function(){window.SM.setOnionMode(this.value);});
document.getElementById('p-opop').addEventListener('input',function(){window.SM.setOnionPrevOp(parseInt(this.value));});
document.getElementById('p-onop').addEventListener('input',function(){window.SM.setOnionNextOp(parseInt(this.value));});
document.getElementById('btn-ff').addEventListener('click',function(){if(state.playing)stopPlay();goToFrame(state.waIn);});
document.getElementById('btn-pf').addEventListener('click',function(){if(state.playing)stopPlay();goToFrame(state.currentFrame-1);});
document.getElementById('btn-play').addEventListener('click',function(){togglePlay();});
document.getElementById('btn-nf').addEventListener('click',function(){if(state.playing)stopPlay();goToFrame(state.currentFrame+1);});
document.getElementById('btn-lf').addEventListener('click',function(){if(state.playing)stopPlay();goToFrame(state.waOut);});
document.getElementById('tl-fps').addEventListener('change',function(){window.SM.setFps(parseInt(this.value));});
document.getElementById('tl-total').addEventListener('change',function(){window.SM.setTotalFrames(parseInt(this.value));});
document.getElementById('btn-al').addEventListener('click',function(){window.SM.addLayer();});
document.getElementById('btn-dl').addEventListener('click',function(){window.SM.deleteLayer();});
document.getElementById('btn-dupl').addEventListener('click',function(){window.SM.duplicateLayer();});
document.getElementById('btn-comp').addEventListener('click',function(){window.SM.convertActiveLayerToComponent();});
document.getElementById('btn-comp-enter').addEventListener('click',function(){var ld=state.layers[state.activeLayerIdx];if(ld&&ld.symbolId)window.SM.enterSymbol(ld.symbolId);});
document.getElementById('btn-comp-detach').addEventListener('click',function(){window.SM.convertComponentToLayer();});
document.getElementById('comp-playmode').addEventListener('change',function(){window.SM.setSymbolPlayMode(this.value);updateCompInstancePanel();});
document.getElementById('comp-singleframe').addEventListener('change',function(){window.SM.setSymbolSingleFrame(this.value);});
document.getElementById('comp-speed').addEventListener('change',function(){window.SM.setSymbolSpeed(this.value);});
document.getElementById('comp-offset').addEventListener('change',function(){window.SM.setSymbolPlacedAt(this.value);});
// Save/Save As/Open/New buttons and the legacy download/upload fallback
// are wired in project.js, which also owns the start screen and the
// recent-projects list.

// ---- EXPORT MODAL ----
(function(){
  var modal=document.getElementById('export-modal');
  var fmtSel=document.getElementById('exp-format');
  var rangeSel=document.getElementById('exp-range');
  var scaleRow=document.getElementById('exp-scale-row');
  var scaleSel=document.getElementById('exp-scale');
  var progEl=document.getElementById('exp-progress');
  var runBtn=document.getElementById('exp-run');

  function updateScaleVisibility(){
    var v=fmtSel.value;
    scaleRow.style.display=(v==='lottie')?'none':'flex';
  }
  fmtSel.addEventListener('change',updateScaleVisibility);

  document.getElementById('btn-export').addEventListener('click',function(){
    if(!window.SMExport.isAvailable()){
      showToast('Export complet disponible uniquement dans l\'app StrokeMotion (Tauri)');
    }
    updateScaleVisibility();
    progEl.style.display='none';progEl.textContent='';
    modal.style.display='flex';
  });
  document.getElementById('export-close').addEventListener('click',function(){modal.style.display='none';});
  modal.addEventListener('click',function(e){if(e.target===modal)modal.style.display='none';});

  runBtn.addEventListener('click',async function(){
    saveAllLayerFrames();
    var range=(rangeSel.value==='all')?{start:0,end:state.totalFrames-1}:{start:state.waIn,end:state.waOut};
    var scale=parseFloat(scaleSel.value);
    var opts={start:range.start,end:range.end,scale:scale,fps:state.fps,
      onProgress:function(i,n){progEl.style.display='block';progEl.textContent='Rendu image '+i+'/'+n+'…';},
      onFfmpeg:function(line){progEl.style.display='block';progEl.textContent=line.substring(0,80);}};
    runBtn.disabled=true;progEl.style.display='block';progEl.textContent='Préparation…';
    try{
      var fn={svg:'exportSVGSequence',png:'exportPNGSequence',tiff:'exportTIFFSequence',gif:'exportGIF',mp4:'exportMP4',prores:'exportProRes',lottie:'exportLottie'}[fmtSel.value];
      var res=await window.SMExport[fn](opts);
      if(res.cancelled){progEl.textContent='Annulé';}
      else if(res.ok){progEl.textContent='Terminé ✓';showToast('Export terminé');setTimeout(function(){modal.style.display='none';},900);}
      else{progEl.textContent='Erreur: '+(res.error||'inconnue');}
    }catch(err){
      progEl.textContent='Erreur: '+(err&&err.message?err.message:err);
    }finally{
      runBtn.disabled=false;
    }
  });
})();

setInterval(function(){if(!state.playing){saveAllLayerFrames();try{localStorage.setItem('sm-auto',window.SM.exportJSON());}catch(e){}}},30000);
// Restored quietly into memory so it's ready the instant the start screen's
// "Resume Last Session" card is clicked — the toast there was confusing
// alongside the new start screen (state.js decides whether to actually
// show that card, and surfaces its own confirmation once chosen).
try{var saved=localStorage.getItem('sm-auto');if(saved)window.SM.importJSON(saved,true);}catch(e){}
window.SM.setTool('draw');updateUI();renderSymbolTabs();
