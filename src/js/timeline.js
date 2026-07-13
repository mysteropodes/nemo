// Plain window.confirm() inside the Tauri webview is intercepted and
// routed to a deprecated/missing plugin command ("dialog.confirm not
// allowed. Command not found") — the dialog never shows, and since the
// call returns a Promise (not a synchronous boolean like a real browser),
// `if (!confirm(msg))` is always false regardless of what got clicked (a
// Promise object is truthy). Same root cause as the updater's confirm bug
// (updater-bridge.js) — this is the shared, correct replacement: the real
// Promise<boolean> API when running in Tauri, plain confirm() otherwise
// (browser preview has no window.__TAURI__ at all).
async function smConfirm(msg, title) {
  if (typeof window.__TAURI__ !== 'undefined' && window.__TAURI__.dialog) {
    return window.__TAURI__.dialog.confirm(msg, { title: title || 'Confirmer' });
  }
  return confirm(msg);
}

// ---- PLAYBACK (optimized: no DOM rebuild during play) ----
var playInt=null;
function startPlay(){if(state.playing)return;state.playing=true;state.playDir=1;
  document.getElementById('btn-play').innerHTML='<span class="material-symbols-rounded">\u{e034}</span>';
  document.getElementById('btn-play').classList.add('playing');
  if(window.SMAudio)SMAudio.onPlayStart(state.currentFrame);
  playInt=setInterval(function(){
    // Ping-pong (right-click btn-loop, feedback: "quand on clic sur le
    // lecture loop... il faut switché aussi sur une lecture en pingpong")
    // bounces back and forth across the work area instead of hard-cutting
    // back to waIn every pass — direction only flips at the OUT-of-bounds
    // edge, one frame at a time, same 1000/fps cadence as forward playback.
    var next=state.currentFrame+state.playDir;
    if(next>state.waOut){
      if(state.loopPlayback&&state.pingPongPlayback){state.playDir=-1;next=state.currentFrame-1;if(next<state.waIn)next=state.waIn;}
      else if(state.loopPlayback){next=state.waIn;if(window.SMAudio)SMAudio.onLoop(next);}
      else{stopPlay();return;}
    }else if(next<state.waIn){
      if(state.loopPlayback&&state.pingPongPlayback){state.playDir=1;next=state.currentFrame+1;if(next>state.waOut)next=state.waOut;}
      else{stopPlay();return;}
    }
    saveAllLayerFrames();state.currentFrame=next;window._curFrame=next;loadFrame(next);updatePlayhead();
  },1000/state.fps);
}
function stopPlay(){if(!state.playing)return;state.playing=false;clearInterval(playInt);playInt=null;
  if(window.SMAudio)SMAudio.onPlayStop();
  document.getElementById('btn-play').innerHTML='<span class="material-symbols-rounded">\u{e037}</span>';
  document.getElementById('btn-play').classList.remove('playing');
  renderOS();renderArcs();updateUI();
}
function togglePlay(){if(state.playing)stopPlay();else startPlay();}

function updatePlayhead(){
  if(window.SMCamera){SMCamera.applyCameraView();if(window.updateCameraPanel)updateCameraPanel();}
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
function selClear(){_sel.frames=[];document.querySelectorAll('.fc.sel').forEach(function(el){el.classList.remove('sel');});if(typeof updateStatusBarHelp==='function')updateStatusBarHelp();}
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
  if(typeof updateStatusBarHelp==='function')updateStatusBarHelp();
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
var PRODUCER_ALLOWED_TOOLS=['hand','zoom','rotate','comment'];
window.SM={
  goToFrame:function(idx){goToFrame(idx);},togglePlay:togglePlay,stopPlay:stopPlay,undo:undo,redo:redo,
  setTool:function(t){
    // RBAC (Phase 3): "producteur" is read-only + comments/validation — see
    // the roadmap plan. Gated here rather than at every individual mutation
    // site since virtually every edit path requires picking a tool first;
    // honor-system only (no server-side enforcement in v1).
    if(state.userProfile&&state.userProfile.role==='producer'&&PRODUCER_ALLOWED_TOOLS.indexOf(t)<0){
      showToast('Profil "Producteur" : lecture seule + commentaires');
      return;
    }
    if(t!=='select'&&t!=='subselect')clearSel();if(t!=='fsselect')fsClearSel();if(t!=='pen'&&_pen.path)finalizePen();if(t!=='eraser'&&typeof _eraserCursor!=='undefined'&&_eraserCursor){_eraserCursor.remove();_eraserCursor=null;}
    // Picking a tool always means "I'm done with the timeline frame
    // selection" — leaving it selected made the status bar keep showing
    // keyframe shortcuts instead of the newly-picked tool's help.
    if(_sel.frames.length)selClear();
    // Leaving the fill tool discards any queued/in-progress Alt-drawn
    // closing stroke (reported: it should persist until either a fill
    // click consumes it or the tool changes — this is the "tool change"
    // half of that). Also the recovery half of the pointer-capture fix:
    // if a drag was somehow left stuck active (_fillCloseDrag non-null),
    // the engine would still be suspended — resume it here too, not just
    // on Escape, so switching tools always leaves rendering in a sane state.
    if(t!=='fill'&&typeof _fillCloseStrokes!=='undefined'){
      if(_fillCloseDrag){_fillCloseDrag=null;if(window.SMEngineBridge)window.SMEngineBridge.resume();}
      _fillCloseStrokes=[];
    }
    // Camera row's frame-grid twin (SMCamera.renderGridRow) sizes itself
    // (compact vs full height + speed-curve SVG) off state.tool==='camera',
    // same condition as the layer-panel row (SMCamera.renderPanelRow) — but
    // unlike that panel row, nothing here used to rebuild #frame-grid on a
    // tool switch, so entering/leaving the camera tool expanded/collapsed
    // the layer row while the keyframes/easing row below it stayed stuck at
    // its old (stale) height, easing curve included (feedback: "il faut ça
    // aussi avec la partie keyframes... afficher les easing comme avant").
    var _camToolChanged=(t==='camera')!==(state.tool==='camera');
    state.tool=t;renderArcs();
    if(_camToolChanged)renderTimeline();
    document.querySelectorAll('.tool-btn').forEach(function(b){b.classList.toggle('active',b.dataset.tool===t);});
    var cc={draw:'crosshair',pen:'crosshair',line:'crosshair',rect:'crosshair',ellipse:'crosshair',select:'default',subselect:'default',fsselect:'default',comment:'crosshair',camera:'move',text:'text',eraser:'pointer',fill:'crosshair',fillbrush:'crosshair',eyedropper:'crosshair',hand:'grab',zoom:'zoom-in',rotate:'grab',perspective:'crosshair'};
    canvasEl.style.cursor=cc[t]||'default';
    updatePropsContext();},
  toggleOnion:function(){state.onionSkin=!state.onionSkin;renderOS();var el=document.getElementById('os-st');el.textContent=state.onionSkin?'ON':'OFF';el.style.color=state.onionSkin?'var(--green)':'var(--text-dim)';var b=document.getElementById('btn-os');if(b)b.classList.toggle('active',state.onionSkin);},
  toggleGhostAll:function(){
    state.ghostAllFrames=!state.ghostAllFrames;
    renderOS();
    var b=document.getElementById('btn-ghost-all');if(b)b.classList.toggle('active',state.ghostAllFrames);
    var sb=document.getElementById('btn-ghost-select');if(sb)sb.disabled=!state.ghostAllFrames;
    if(!state.ghostAllFrames)clearGhostSelection();
    updateUI();
  },
  selectGhostAll:selectGhostAll,
  toggleLoopPlayback:function(){state.loopPlayback=!state.loopPlayback;var b=document.getElementById('btn-loop');if(b)b.classList.toggle('active',state.loopPlayback);},
  // Right-click btn-loop (feedback: "quand on clic sur le lecture loop...
  // il faut switché aussi sur une lecture en pingpong") — a secondary
  // toggle rather than folding into the left-click, so anyone who just
  // wants a plain wrap-to-start loop keeps that as the default single
  // click. Ping-pong only has an effect while loop itself is on (see
  // startPlay's interval) — implied on when picked, doesn't force loop off.
  togglePingPongPlayback:function(){state.pingPongPlayback=!state.pingPongPlayback;if(state.pingPongPlayback)state.loopPlayback=true;var b=document.getElementById('btn-loop');if(b){b.classList.toggle('active',state.loopPlayback);b.classList.toggle('pingpong',state.pingPongPlayback);b.title=state.pingPongPlayback?'Loop playback — ping-pong (clic droit pour désactiver)':'Loop playback (work area) — clic droit pour ping-pong';}showToast(state.pingPongPlayback?'Lecture ping-pong activée':'Lecture ping-pong désactivée');},
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
    ['color-stroke','pm-stroke-c'].forEach(function(id){var el=document.getElementById(id);el.value=v;el.dataset.hex8=v;});
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(p.data&&p.data.isVectorBrush)p.fillColor=v;else if(p.strokeColor)p.strokeColor=v;});saveActiveLayerFrame();updateUI();}
    // Fill/Stroke Select tool: recolor ONLY the clicked aspect — a 'stroke'
    // selection here means strokeColor, never touches fillColor even on a
    // combined shape (that's the whole point of this tool vs plain Select).
    else if(state.tool==='fsselect'&&_fsSel&&_fsSel.kind==='stroke'){pushUndo();var arc=fsRealizeStrokeSegment(_fsSel,userLayers[state.activeLayerIdx]);_fsSel={path:arc,kind:'stroke',segStart:0,segEnd:arc.length,closed:arc.closed};arc.strokeColor=v;saveActiveLayerFrame();updateUI();}},
  setFillColor:function(v){state.fillColor=v;
    // Background is written unconditionally (even while fill is disabled) so
    // the swatch shows the last-picked color as soon as fill is re-enabled —
    // the .none overlay (red diagonal) is what actually communicates "off".
    document.getElementById('fill-well').style.background=v;document.getElementById('pm-fill').style.background=v;
    ['color-fill','pm-fill-c'].forEach(function(id){var el=document.getElementById(id);el.value=v;el.dataset.hex8=v;});
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(p.fillColor)p.fillColor=v;});saveActiveLayerFrame();updateUI();}
    else if(state.tool==='fsselect'&&_fsSel&&(_fsSel.kind==='fill'||_fsSel.kind==='fillregion')){pushUndo();if(_fsSel.kind==='fillregion')_fsSel=fsRealizeFillRegion(_fsSel,userLayers[state.activeLayerIdx]);_fsSel.path.fillColor=v;saveActiveLayerFrame();updateUI();}},
  setFillEnabled:function(v){state.fillEnabled=v;var fw=document.getElementById('fill-well'),pf=document.getElementById('pm-fill');fw.classList.toggle('none',!v);pf.classList.toggle('none',!v);document.getElementById('p-fill-on').checked=v;
    var ft=document.getElementById('fill-enable-toggle');if(ft)ft.classList.toggle('off',!v);
    var ftlp=document.getElementById('fill-enable-toggle-lp');if(ftlp){ftlp.classList.toggle('off',!v);ftlp.innerHTML=v?ICO_EYE:ICO_EYE_CLOSED;}
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(p.data&&p.data.isVectorBrush)return;p.fillColor=v?state.fillColor:null;});saveActiveLayerFrame();updateUI();}
    else if(state.tool==='fsselect'&&_fsSel&&(_fsSel.kind==='fill'||_fsSel.kind==='fillregion')){pushUndo();if(_fsSel.kind==='fillregion')_fsSel=fsRealizeFillRegion(_fsSel,userLayers[state.activeLayerIdx]);_fsSel.path.fillColor=v?state.fillColor:null;if(!v){fsUnlinkFillRegen(_fsSel.path);if(!_fsSel.path.strokeColor){_fsSel.path.remove();fsClearSel();}}saveActiveLayerFrame();updateUI();}},
  // Mirrors setFillEnabled exactly, for the Stroke side — didn't exist
  // before (Stroke had no on/off concept, only a color), added alongside
  // the quick phdr toggle button since disabling stroke without it required
  // opening the color popover and hunting for "None".
  setStrokeEnabled:function(v){state.strokeEnabled=v;
    var st=document.getElementById('stroke-enable-toggle');if(st)st.classList.toggle('off',!v);
    var stlp=document.getElementById('stroke-enable-toggle-lp');if(stlp){stlp.classList.toggle('off',!v);stlp.innerHTML=v?ICO_EYE:ICO_EYE_CLOSED;}
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(p.data&&p.data.isVectorBrush)return;p.strokeColor=v?state.strokeColor:null;});saveActiveLayerFrame();updateUI();}
    // fsselect's 'stroke' selection can be just a bounded segment (between
    // two crossings), which has no standalone color to null — disabling it
    // means the same real split-and-remove fsApplyDelete's Delete key does.
    // No re-enable path: once a segment is split out there's nothing left
    // to flip back on, and the stroke-sec panel disappears with _fsSel.
    else if(state.tool==='fsselect'&&_fsSel&&_fsSel.kind==='stroke'&&!v){pushUndo();fsDeleteSegment(_fsSel,userLayers[state.activeLayerIdx]);fsClearSel();renderArcs();updateUI();}},
  // Illustrator's "X" swap — exchanges the stroke and fill colors (tool
  // defaults, and the current selection's actual colors if select/subselect
  // is active, since setStrokeColor/setFillColor already apply to
  // selectedPaths on their own). Fill's enabled/disabled state is left
  // alone; only the color VALUES swap.
  swapStrokeFill:function(){
    var s=state.strokeColor,f=state.fillColor;
    window.SM.setFillColor(s);
    window.SM.setStrokeColor(f);
  },
  setFillBrushSize:function(v){state.fillBrushSize=Math.max(1,parseInt(v)||40);},
  setBrushPreset:function(v){state.brushPreset=v||'none';},
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
  setVectorBrush:function(v){state.vectorBrush=v;},setTaperEnds:function(v){state.taperEnds=v;},setShadowMode:function(v){state.shadowMode=v;},
  setDrawMode:function(v){state.drawMode=v;},
  setFillBrushMode:function(v){state.fillBrushMode=v;},
  setEraserSize:function(v){state.eraserSize=Math.max(2,parseInt(v)||24);},
  setPressureMin:function(v){state.pressureMin=Math.max(0,Math.min(100,parseInt(v)||0));},
  setPressureMax:function(v){state.pressureMax=Math.max(50,Math.min(300,parseInt(v)||170));},
  setPressureInvert:function(v){state.pressureInvert=!!v;},
  setOpacity:function(v){state.opacity=parseInt(v);
    // Unlike setFillColor/setStrokeColor right above, this never applied to
    // the current selection — only ever wrote the tool-default opacity for
    // the NEXT stroke drawn, so editing the Opacity field with something
    // already selected (a normal shape, or an imported image/video frame,
    // which has no fillColor/strokeColor to fall back through) silently did
    // nothing to what was on screen.
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){
      pushUndo();
      var op=Math.max(0,Math.min(100,parseInt(v)||0))/100;
      selectedPaths.forEach(function(p){p.opacity=op;});
      saveActiveLayerFrame();updateUI();
    }
  },
  setFps:function(v){state.fps=Math.max(1,Math.min(60,v));if(state.playing){stopPlay();startPlay();}if(window.SMAudio)SMAudio.invalidateWaveforms();syncDocFields();},
  setCurve:function(points){state.easingCurve={points:points};},
  setResamplePts:function(v){state.resamplePts=v;},setTweenStep:function(v){state.tweenStep=parseInt(v);},
  setCanvasSize:function(w,h){state.canvasW=w;state.canvasH=h;drawStage();syncDocFields();},setCanvasBg:function(c){state.canvasBg=c;drawStage();syncDocFields();},
  setCanvasClip:function(v){state.canvasClip=!!v;if(window.SMEngineBridge)window.SMEngineBridge.renderNow();},
  setSafetyZones:function(v){state.safetyZones=!!v;if(window.SMEngineBridge)window.SMEngineBridge.renderNow();},
  fitCanvas:fitCanvas,resetView:resetView,
  setOnionRange:function(inF,outF){state.onionIn=inF;state.onionOut=outF;renderOS();},
  setOnionPrevOp:function(v){state.onionPrevOpacity=v;renderOS();},setOnionNextOp:function(v){state.onionNextOpacity=v;renderOS();},
  setOnionMode:function(v){state.onionMode=v;renderOS();},
  setWorkArea:function(inF,outF){state.waIn=inF;state.waOut=outF;},
  setTotalFrames:function(v){v=Math.max(1,Math.min(999,v));saveAllLayerFrames();for(var i=0;i<state.layers.length;i++){while(state.layers[i].frames.length<v)state.layers[i].frames.push({strokes:[],isKeyframe:false,isInterpolated:false});}state.totalFrames=v;window._totalF=v;if(state.waOut>=v)state.waOut=v-1;window._waOut=state.waOut;if(state.currentFrame>=v)goToFrame(v-1);updateUI();},
  addLayer:function(){saveAllLayerFrames();pushUndoLayers();var idx=createUserLayer(nextLayerName());activateUL(idx);loadFrame(state.currentFrame);updateUI();},
  deleteLayer:function(){
    // The camera row isn't in state.layers (synthetic pseudo-layer, see
    // camera.js) — the generic layer-panel trash button silently did
    // nothing while it was "active" (feedback #5xtsn) since every branch
    // below only ever touches real layers. Delete it here first and return.
    if(state.tool==='camera'){
      state.cameraLayerOn=false;state.cameraKeys=[];state.cameraView=false;
      window.SM.setTool('select');renderLayerList();renderTimeline();updateUI();
      if(window.updateCameraPanel)window.updateCameraPanel();
      if(window.SMEngineBridge)window.SMEngineBridge.renderNow();
      showToast('Calque caméra supprimé');
      return;
    }
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
  duplicateLayer:function(){saveAllLayerFrames();pushUndoLayers();var src=state.layers[state.activeLayerIdx];var ni=createUserLayer(src.name+' copy');state.layers[ni].frames=JSON.parse(JSON.stringify(src.frames));if(src.blendMode)state.layers[ni].blendMode=src.blendMode;state.layers[ni].color=src.color;activateUL(ni);loadFrame(state.currentFrame);updateUI();},
  setActiveLayer:function(idx){if(idx<0||idx>=state.layers.length)return;saveAllLayerFrames();activateUL(idx);clearSel();
    // The camera row is a synthetic pseudo-layer (not a real state.layers
    // entry — see camera.js's renderPanelRow) selected by switching TO the
    // camera tool, never by an activeLayerIdx change; picking a real layer
    // here has to explicitly leave it, or its guides/highlight stay stuck
    // on screen with no layer row left looking selected (feedback #5wrip).
    if(state.tool==='camera')window.SM.setTool('select');
    renderArcs();updateUI();},
  toggleLayerVis:function(idx){state.layers[idx].visible=!state.layers[idx].visible;loadFrame(state.currentFrame);updateUI();},
  toggleLayerLock:function(idx){
    state.layers[idx].locked=!state.layers[idx].locked;
    // Locking a layer that already has content selected (selected before the
    // lock, or the lock toggled while it's the active layer) must drop that
    // selection immediately — otherwise the resize/rotate handles stay live
    // on now-locked geometry until the next unrelated selection change,
    // same "lock isn't total" gap as the plain click/marquee case.
    if(state.layers[idx].locked&&userLayers[idx]){
      selectedPaths=selectedPaths.filter(function(p){return p.layer!==userLayers[idx];});
      state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
      renderArcs();
    }
    updateUI();
  },
  toggleLayerSolo:function(idx){state.layers[idx].solo=!state.layers[idx].solo;loadFrame(state.currentFrame);updateUI();},
  renameLayer:function(idx,n){state.layers[idx].name=n;updateUI();},
  reorderLayer:function(fromIdx,toIdx){reorderLayer(fromIdx,toIdx);},
  reorderLayersBatch:function(fromIndices,toIdx){reorderLayersBatch(fromIndices,toIdx);},
  convertActiveLayerToComponent:function(){
    if(_layerSel.length>1)convertLayersToComponent(_layerSel);
    else convertLayerToComponent(state.activeLayerIdx);
  },
  convertComponentToLayer:function(){convertComponentToLayer(state.activeLayerIdx);},
  convertActiveLayerToLFSGroup:function(){convertLayerToLFSGroup(state.activeLayerIdx);},
  convertActiveLayerToStrokeFillShadow:function(){convertLayerToStrokeFillShadowFolder(state.activeLayerIdx);},
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
    var b=selBounds();if(!b)return;
    var offsetL=dLayer-b.minL,offsetF=dFrame-b.minF;
    if(offsetL===0&&offsetF===0)return;
    // Locked layers are untouchable on BOTH ends of a move: a locked source
    // must not be blanked out, and a locked target must not be overwritten
    // (feedback #18 — dragging keyframes in the grid bypassed the lock that
    // every other edit path already respects).
    sel=sel.filter(function(s){
      var srcLd=state.layers[s.layer];if(!srcLd||srcLd.locked)return false;
      var tl=s.layer+offsetL;
      var tgtLd=state.layers[tl];
      return !(tgtLd&&tgtLd.locked);
    });
    if(!sel.length){showToast('Calque verrouillé');return;}
    pushUndo();saveAllLayerFrames();
    var data=[];
    sel.forEach(function(s){
      var ld=state.layers[s.layer];if(!ld)return;
      data.push({layer:s.layer,frame:s.frame,content:JSON.parse(JSON.stringify(ld.frames[s.frame]))});
    });
    // v16: snapshot each touched layer's keyframe list BEFORE the move so a
    // retimed keyframe's motion-arc data (tweens.js rekeyTweenPairData) can
    // follow it to the new frame index instead of orphaning — see that
    // function's comment for why the arc data itself is still valid, only
    // its lookup key needs to move.
    var movedFrameMap={};
    sel.forEach(function(s){
      var tl=s.layer+offsetL,tf=s.frame+offsetF;
      if(tl<0||tl>=state.layers.length||tf<0||tf>=state.totalFrames)return;
      movedFrameMap[s.layer+':'+s.frame]=tf;
    });
    var touchedLayers={};sel.forEach(function(s){touchedLayers[s.layer]=true;});
    var beforeKeyframes={};
    Object.keys(touchedLayers).forEach(function(lk){
      var li=parseInt(lk,10),ld=state.layers[li];if(!ld)return;
      beforeKeyframes[li]=ld.frames.map(function(f,fi){return f.isKeyframe?fi:null;}).filter(function(x){return x!==null;});
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
    Object.keys(beforeKeyframes).forEach(function(lk){
      var li=parseInt(lk,10),kfs=beforeKeyframes[li];
      for(var i=0;i<kfs.length-1;i++){
        var fA=kfs[i],fB=kfs[i+1];
        var newFA=movedFrameMap[li+':'+fA];newFA=(newFA!==undefined)?newFA:fA;
        var newFB=movedFrameMap[li+':'+fB];newFB=(newFB!==undefined)?newFB:fB;
        rekeyTweenPairData(fA,fB,newFA,newFB);
      }
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
      if(ld.locked)return; // keep it in the clipboard (paste elsewhere still works), just don't blank out a locked layer's own content
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
      if(state.layers[tl].locked)return; // pasting into a locked layer must no-op for that layer, same as any other edit
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
      var ld=state.layers[s.layer];if(!ld||ld.locked)return;
      ld.frames[s.frame]={strokes:[],isKeyframe:false,isInterpolated:false};
    });
    selClear();loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
    showToast('Frames supprimées');
  },
  moveKeyframe:function(layerIdx,fromFrame,toFrame){
    if(fromFrame===toFrame)return;var ld=state.layers[layerIdx];if(!ld||ld.locked)return;
    var src=ld.frames[fromFrame];if(!src||!src.isKeyframe)return;
    var dst=ld.frames[toFrame];if(!dst)return;
    pushUndo();
    ld.frames[toFrame]={strokes:src.strokes,isKeyframe:true,isInterpolated:false};
    ld.frames[fromFrame]={strokes:[],isKeyframe:false,isInterpolated:false};
    loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast('Keyframe déplacée → '+(toFrame+1));
  },
  deleteSelStrokes:function(){if(selectedPaths.length>0){pushUndo();selectedPaths.forEach(function(p){
    // Team review: deleting someone else's stroke ghosts it instead of
    // removing it outright — see markDeleteAsRevision's own comment.
    if(markDeleteAsRevision(p))return;
    // Companions (linkedFill backdrop, brushCompanions texture copies)
    // are deliberately excluded from selectedPaths (see their own "why" —
    // they're not independently selectable) so deleting the primary has to
    // explicitly take them with it too, or they're left behind as
    // orphaned, invisible-in-the-panel geometry nobody can select to clean up.
    if(p.data&&p.data.linkedFill&&!p.data.linkedFill.removed)p.data.linkedFill.remove();
    if(p.data&&p.data.brushCompanions)p.data.brushCompanions.forEach(function(c){if(!c.removed)c.remove();});
    p.remove();
  });clearSel();saveActiveLayerFrame();updateUI();}},
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
    if(ld.locked){showToast('Calque verrouillé');return;}
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
      layers:sceneLayers.map(function(l){return{name:l.name,visible:l.visible,locked:l.locked,frames:l.frames,symbolId:l.symbolId,symPlayMode:l.symPlayMode,symSpeed:l.symSpeed,symPlacedAt:l.symPlacedAt,symSingleFrame:l.symSingleFrame,symMatrix:l.symMatrix,lfsGroup:l.lfsGroup,lfsIds:l.lfsIds,lfsSettings:l.lfsSettings,blendMode:l.blendMode,folderId:l.folderId,channel:l.channel,linkGroupId:l.linkGroupId,color:l.color};}),
      layerFolders:state.layerFolders,layerLinkGroups:state.layerLinkGroups,
      symbols:state.symbols,palettes:state.palettes,activePaletteIdx:state.activePaletteIdx,customBrushPresets:state.customBrushPresets,
      // audio: only the persistable fields — _buffer/_peaksCanvas/_srcNode
      // are live runtime objects that must never hit JSON
      audioTracks:(state.audioTracks||[]).map(function(t){return{name:t.name,dataB64:t.dataB64,offsetFrames:t.offsetFrames||0,volume:t.volume!==undefined?t.volume:1,muted:!!t.muted};}),
      refMedia:state.refMedia?{type:state.refMedia.type,name:state.refMedia.name,src:state.refMedia.src,frames:state.refMedia.frames,opacity:state.refMedia.opacity,visible:state.refMedia.visible,offsetFrames:state.refMedia.offsetFrames||0}:null,
      perspectiveEnabled:state.perspectiveEnabled,perspectiveMode:state.perspectiveMode,perspectiveDensity:state.perspectiveDensity,perspectiveVPs:state.perspectiveVPs,
      motionArcs:state.motionArcs,easingCurve:state.easingCurve,resamplePts:state.resamplePts,tweenStep:state.tweenStep,
      tweenOverrides:state.tweenOverrides,comments:state.comments||[],
      cameraKeys:state.cameraKeys||[],cameraLayerOn:!!state.cameraLayerOn});
  },
  mergeRemoteSnapshot:function(remoteData,remoteProfile){return mergeRemoteSnapshot(remoteData,remoteProfile);},
  // Cycles (v19) : repete N fois la plage de frames selectionnee (walk
  // cycles etc. — TVPaint instances / Animate graphic loop, version copie).
  // Copie profonde des strokes, frames marquees keyframe, timeline etendue
  // si besoin. strokeIds conserves : c'est le MEME dessin qui revient, le
  // matching de tween inter-cles continue de fonctionner.
  repeatSelection:function(times){
    var b=selBounds();
    if(!b){showToast('Selectionne d\'abord une plage de frames dans la timeline');return;}
    times=Math.max(1,Math.min(50,parseInt(times,10)||1));
    pushUndoLayers();
    saveAllLayerFrames();
    var span=b.maxF-b.minF+1;
    var needed=b.maxF+1+span*times;
    if(needed>state.totalFrames){
      var add=needed-state.totalFrames;
      for(var li=0;li<state.layers.length;li++){for(var a=0;a<add;a++)state.layers[li].frames.push({strokes:[],isKeyframe:false,isInterpolated:false});}
      state.totalFrames=needed;window._totalF=needed;
      if(state.waOut<needed-1){state.waOut=needed-1;window._waOut=state.waOut;}
    }
    for(var l=b.minL;l<=b.maxL;l++){
      if(!state.layers[l]||state.layers[l].symbolId)continue;
      for(var r=1;r<=times;r++){
        for(var f=b.minF;f<=b.maxF;f++){
          var src=state.layers[l].frames[f];
          var dst=b.maxF+ (r-1)*span + (f-b.minF) + 1;
          state.layers[l].frames[dst]={strokes:JSON.parse(JSON.stringify(src.strokes||[])),isKeyframe:!!src.isKeyframe,isInterpolated:!!src.isInterpolated};
        }
      }
    }
    loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
    showToast('Cycle : plage repetee '+times+' fois');
  },
  // Propagation de couleur (v19) : applique la couleur du fill/trait
  // selectionne a TOUTES les occurrences du meme strokeId sur toutes les
  // frames de tous les calques (ink & paint : recolorer un aplat sur toute
  // la sequence en un clic, facon CTG TVPaint).
  propagateColorAllFrames:function(){
    var sid=null,fillHex=null,strokeHex=null;
    if(state.tool==='fsselect'&&_fsSel&&_fsSel.path&&_fsSel.path.data){
      sid=_fsSel.path.data.strokeId;
      if(_fsSel.kind==='stroke'&&_fsSel.path.strokeColor)strokeHex=colorHex8(_fsSel.path.strokeColor);
      else if(_fsSel.path.fillColor)fillHex=colorHex8(_fsSel.path.fillColor);
    }else if(selectedPaths.length===1&&selectedPaths[0].data){
      var p0=selectedPaths[0];sid=p0.data.strokeId;
      if(p0.fillColor)fillHex=colorHex8(p0.fillColor);
      if(p0.strokeColor)strokeHex=colorHex8(p0.strokeColor);
    }
    if(!sid){showToast('Selectionne un trait/fill (outil V ou M) a propager');return;}
    pushUndoLayers();saveAllLayerFrames();
    var count=0;
    for(var li=0;li<state.layers.length;li++){
      var frames=state.layers[li].frames;
      for(var fi=0;fi<frames.length;fi++){
        var strokes=frames[fi]&&frames[fi].strokes;if(!strokes)continue;
        for(var si=0;si<strokes.length;si++){
          if(strokes[si].strokeId!==sid)continue;
          if(fillHex&&strokes[si].fillColor)strokes[si].fillColor=fillHex;
          if(strokeHex&&strokes[si].strokeColor)strokes[si].strokeColor=strokeHex;
          count++;
        }
      }
    }
    loadFrame(state.currentFrame);updateUI();
    showToast('Couleur propagee sur '+count+' frame(s)');
  },
  importJSON:function(json,silent){
    try{var d=JSON.parse(json);if(!d.layers&&!d.frames)throw new Error('Invalid');
    if(!d.layers)d.layers=[{name:'Layer 1',visible:true,locked:false,frames:d.frames}];
    // Validate the FULL layer/frame structure BEFORE the teardown below —
    // the old shallow `!d.layers` check let a file that parses but is
    // structurally broken deeper (layers not an array, a layer whose
    // frames is null, a frame without a strokes array) destroy the
    // in-memory document first and THEN throw partway through rebuild,
    // stranding the app on a half-imported state with only the 30s
    // autosave to fall back on. Now a bad file is rejected while the
    // current document is still fully intact.
    if(!Array.isArray(d.layers)||!d.layers.length)throw new Error('Fichier invalide (layers)');
    d.layers.forEach(function(ld,li){
      if(!ld||!Array.isArray(ld.frames))throw new Error('Fichier invalide (calque '+(li+1)+')');
      ld.frames.forEach(function(f,fi){if(!f||!Array.isArray(f.strokes))throw new Error('Fichier invalide (calque '+(li+1)+', frame '+(fi+1)+')');});
    });
    if(state.activeSymbolId)exitToScene();
    state.totalFrames=d.totalFrames||d.layers[0].frames.length;state.fps=d.fps||12;state.canvasW=d.canvasW||1920;state.canvasH=d.canvasH||1080;state.canvasBg=d.canvasBg||'#ffffff';
    state.waIn=d.waIn||0;state.waOut=d.waOut!==undefined?d.waOut:state.totalFrames-1;
    window._waIn=state.waIn;window._waOut=state.waOut;window._totalF=state.totalFrames;
    while(userLayers.length>0)userLayers.pop().remove();state.layers=[];
    Object.keys(_symbolPaperLayers).forEach(function(k){_symbolPaperLayers[k].forEach(function(l){l.remove();});});_symbolPaperLayers={};
    state.symbols=d.symbols||{};state.openSymbolTabs=[];state.activeSymbolId=null;
    d.layers.forEach(function(ld){var idx=createUserLayer(ld.name);state.layers[idx].visible=ld.visible!==false;state.layers[idx].locked=ld.locked||false;state.layers[idx].frames=ld.frames;
      if(ld.blendMode)state.layers[idx].blendMode=ld.blendMode;
      if(ld.symbolId){state.layers[idx].symbolId=ld.symbolId;state.layers[idx].symPlayMode=ld.symPlayMode||'loop';state.layers[idx].symSpeed=ld.symSpeed||1;state.layers[idx].symPlacedAt=ld.symPlacedAt||0;state.layers[idx].symSingleFrame=ld.symSingleFrame||0;if(ld.symMatrix)state.layers[idx].symMatrix=ld.symMatrix;}
      if(ld.lfsGroup){state.layers[idx].lfsGroup=true;state.layers[idx].lfsIds=ld.lfsIds;state.layers[idx].lfsSettings=ld.lfsSettings;}
      if(ld.folderId)state.layers[idx].folderId=ld.folderId;
      if(ld.channel)state.layers[idx].channel=ld.channel;
      if(ld.linkGroupId)state.layers[idx].linkGroupId=ld.linkGroupId;
      state.layers[idx].color=ld.color||nextLayerColor();
      ld.frames.forEach(function(f){if(!f.isInterpolated)f.isInterpolated=false;});while(state.layers[idx].frames.length<state.totalFrames)state.layers[idx].frames.push({strokes:[],isKeyframe:false,isInterpolated:false});});
    state.layerFolders=d.layerFolders||{};state.layerLinkGroups=d.layerLinkGroups||{};
    state.motionArcs=d.motionArcs||{};state.tweenOverrides=d.tweenOverrides||{};if(d.easingCurve){state.easingCurve=d.easingCurve;if(window._curveEditor)window._curveEditor.setState(d.easingCurve);}
    state.comments=d.comments||[];
    if(typeof refreshFbAvatars==='function')refreshFbAvatars(); // avatar stack mirrors state.comments — resync on project import
    state.cameraKeys=d.cameraKeys||[];state.cameraLayerOn=!!d.cameraLayerOn;state.cameraView=false;
    // Explicit fallback to the app default, not just "leave whatever was
    // there" — opening an old-format project right after working on a
    // DIFFERENT project would otherwise silently carry that other
    // project's palette over, since importJSON can be called directly from
    // the Open dialog without newProject() resetting state first.
    // Back-compat: a pre-multi-palette file only has the flat
    // `colorPalette` array — wrap it into a single palette instead of
    // losing the artist's saved colors.
    if(d.palettes&&d.palettes.length)state.palettes=d.palettes;
    else if(d.colorPalette)state.palettes=[{id:'p0',name:'Palette 1',colors:d.colorPalette}];
    else state.palettes=[{id:'p0',name:'Palette 1',colors:['#000000','#ffffff','#ff0000','#ff8800','#ffee00','#00cc44','#0088ff','#8833ff']}];
    state.activePaletteIdx=d.activePaletteIdx||0;
    state.customBrushPresets=d.customBrushPresets||{};
    state.audioTracks=d.audioTracks||[];
    if(window.SMAudio)SMAudio.reload();
    state.refMedia=d.refMedia||null;
    if(window.SMReference)SMReference.reload();
    state.perspectiveEnabled=d.perspectiveEnabled||false;state.perspectiveMode=d.perspectiveMode||'2pt';state.perspectiveDensity=d.perspectiveDensity||24;state.perspectiveVPs=d.perspectiveVPs||null;
    var perspOnCb=document.getElementById('p-persp-on');if(perspOnCb)perspOnCb.checked=state.perspectiveEnabled;
    var perspModeSel=document.getElementById('p-persp-mode');if(perspModeSel)perspModeSel.value=state.perspectiveMode;
    var perspDensityInp=document.getElementById('p-persp-density');if(perspDensityInp)perspDensityInp.value=state.perspectiveDensity;
    if(window.renderPaletteGrid)window.renderPaletteGrid();
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
  renderTimeline();renderLayerList();updateCompInstancePanel();updateSelPropsPanel();updateFsSelPanel();updateRevisionPanel();updatePropsContext();
}
// Team review Accept/Reject panel — shown when exactly one selected item is
// either an active (non-ghost) revision (data.revisionParentId) or a
// delete-revision ghost (data.isRevisionGhost && revisionAction==='delete').
// A reshape/move ghost is NOT independently actionable here — accepting or
// rejecting it always happens through its PAIRED active item instead (there
// would be nothing to "keep" if you accepted the ghost directly), so ghosts
// only surface this panel for the delete case, which has no pair.
function updateRevisionPanel(){
  var sec=document.getElementById('revision-sec');
  if(!sec)return;
  var p=(state.tool==='select'&&selectedPaths.length===1)?selectedPaths[0]:null;
  var isActiveRevision=!!(p&&p.data&&p.data.revisionParentId&&!p.data.isRevisionGhost);
  var isDeleteGhost=!!(p&&p.data&&p.data.isRevisionGhost&&p.data.revisionAction==='delete');
  if(!isActiveRevision&&!isDeleteGhost){sec.style.display='none';return;}
  sec.style.display='';
  var row=document.getElementById('revision-author-row');
  if(row)row.textContent=isDeleteGhost
    ?'Suppression proposée sur le trait de '+(p.data.ownerName||'un autre profil')
    :'Correction par '+(p.data.ownerName||'un autre profil');
  document.getElementById('btn-revision-accept').onclick=function(){
    pushUndo();
    var layer=userLayers[state.activeLayerIdx];
    if(isDeleteGhost)acceptDeleteRevision(p);else acceptRevision(p,layer);
    clearSel();saveActiveLayerFrame();updateUI();
  };
  document.getElementById('btn-revision-reject').onclick=function(){
    pushUndo();
    var layer=userLayers[state.activeLayerIdx];
    if(isDeleteGhost)rejectDeleteRevision(p);else rejectRevision(p,layer);
    clearSel();saveActiveLayerFrame();updateUI();
  };
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
// Force-expands a right-panel section by id (Tween/Easing Curve, etc.) and
// scrolls it into view — used when a UI action (clicking a tween cell)
// implies the user wants to see that section NOW, distinct from the normal
// user-toggle-only .phdr click handler (ui.js) which never auto-opens
// anything the user didn't click directly.
function openPropsSection(id){
  var sec=document.getElementById(id);if(!sec)return;
  var h=sec.querySelector('.phdr'),b=sec.querySelector('.pbdy');
  if(h&&b){b.classList.remove('hid');h.classList.remove('closed');}
  sec.scrollIntoView({block:'nearest',behavior:'smooth'});
}
// ---- Contextual help in the bottom status bar ----
// Feature request: the bar's shortcut list used to be one hardcoded frame/
// keyframe cheat-sheet regardless of what's active — user wanted it to
// explain whatever tool/selection is CURRENT instead ("si je prends le pot
// de peinture ça explique en une phrase courte ce que ça fait et les
// raccourcis... si je select une clé pareil"). Reuses .sc styling so it
// reads identically to the original static bar. Priority, most specific
// first: a canvas selection (shape/point) > a timeline keyframe selection
// > the active tool's own help > the original generic frame cheat-sheet
// as the fallback with nothing more specific going on.
var TOOL_HELP={
  select:{desc:'Sélection — clique ou entoure pour sélectionner, glisse les poignées pour transformer.',sc:[['V','Outil'],['Shift+clic','Ajouter'],['Suppr','Effacer']]},
  subselect:{desc:'Sous-sélection — édite les points d\'ancrage et leurs tangentes.',sc:[['A','Outil'],['Alt+clic','Casser tangente']]},
  fsselect:{desc:'Sélection fill/stroke — clique une zone remplie ou un segment de trait précis.',sc:[['Shift+clic','Ajouter']]},
  draw:{desc:'Pinceau — dessine un trait.',sc:[['B','Outil'],['Alt+glisser','Taille'],['[ ]','Taille ±']]},
  pen:{desc:'Plume — place des ancres, glisse pour une poignée de courbe.',sc:[['P','Outil'],['Clic','Ancre'],['Échap','Terminer']]},
  line:{desc:'Ligne — glisse pour tracer un segment droit.',sc:[['Shift','Angle 45°']]},
  rect:{desc:'Rectangle — glisse pour dessiner.',sc:[['Shift','Carré']]},
  ellipse:{desc:'Ellipse — glisse pour dessiner.',sc:[['Shift','Cercle']]},
  fill:{desc:'Pot de peinture — clique une zone fermée pour la remplir.',sc:[['Alt+glisser','Trait de fermeture'],['Shift+clic','Retirer le fill'],['Échap','Annuler le trait']]},
  fillbrush:{desc:'Pinceau de remplissage — peint directement une forme pleine, sans contour.',sc:[]},
  eraser:{desc:'Gomme — glisse sur un trait ou un fill pour effacer une zone.',sc:[['E','Outil'],['[ ]','Taille ±']]},
  eyedropper:{desc:'Pipette — clique une couleur du canvas pour la prélever.',sc:[['I','Outil']]},
  hand:{desc:'Main — glisse pour déplacer la vue.',sc:[['Espace','Maintenir temporairement']]},
  zoom:{desc:'Zoom — clique pour zoomer, Alt+clic pour dézoomer.',sc:[['Z','Outil']]},
  rotate:{desc:'Rotation de la vue — glisse pour tourner le canvas (pas le contenu).',sc:[['Alt+glisser','Sur un autre outil']]},
  camera:{desc:'Caméra — glisse le cadre pour cadrer, clic-droit pour la courbe d\'accélération.',sc:[['Clic-droit','Courbe d\'easing']]},
  comment:{desc:'Commentaire — clique pour laisser une note à cet endroit.',sc:[]},
  text:{desc:'Texte — clique pour placer un champ de texte.',sc:[]},
  perspective:{desc:'Guide de perspective.',sc:[]},
};
function statusbarHelpRender(desc,sc){
  var el=document.getElementById('statusbar-help');
  if(!el)return;
  var html=desc?desc+' ':'';
  html+=sc.map(function(pair){return '<span class="sc">'+pair[0]+'</span>'+pair[1];}).join(' ');
  el.innerHTML=html;
}
var STATUSBAR_DEFAULT_SC=[['F5','Frame'],['F6','Key'],['F7','Blank'],['T','Tween'],['D','Dupli'],['F','Flip'],['X','Mirror'],['Enter','Play']];
function updateStatusBarHelp(){
  // 1. A canvas element is selected (select/subselect) — most specific.
  if((state.tool==='select'||state.tool==='subselect')&&typeof selectedPaths!=='undefined'&&selectedPaths.length>0){
    var n=selectedPaths.length;
    statusbarHelpRender(n+(n>1?' éléments sélectionnés':' élément sélectionné')+' —',[['Suppr','Effacer'],['⌘/Ctrl+D','Dupliquer'],['↑↓←→','Déplacer'],['Échap','Désélectionner']]);
    return;
  }
  // 2. fsselect has an active fill/stroke pick.
  if(state.tool==='fsselect'&&typeof _fsSel!=='undefined'&&_fsSel){
    statusbarHelpRender((_fsSel.kind==='stroke'?'Stroke':'Fill')+' sélectionné(e) —',[['Suppr','Effacer'],['Échap','Désélectionner']]);
    return;
  }
  // 3. A timeline keyframe cell (or span) is selected.
  if(typeof _sel!=='undefined'&&_sel.frames&&_sel.frames.length>0){
    var kn=_sel.frames.length;
    statusbarHelpRender(kn+(kn>1?' images-clés sélectionnées':' image-clé sélectionnée')+' —',[['F6','Key'],['F7','Blank'],['T','Tween'],['D','Dupli'],['F','Flip'],['X','Mirror'],['Suppr','Effacer']]);
    return;
  }
  // 4. The active tool's own help.
  var help=TOOL_HELP[state.tool];
  if(help){statusbarHelpRender(help.desc,help.sc);return;}
  // 5. Nothing more specific — original generic frame cheat-sheet.
  statusbarHelpRender('',STATUSBAR_DEFAULT_SC);
}
// Hovering a right-panel control (or timeline/layer toolbar button) shows
// its native `title` in the status bar too — those tooltips already exist
// on nearly every control (added over many sessions) but only surface after
// the OS's slow hover delay and easy to miss; mirroring them here makes the
// same text show up instantly. Delegated (not per-element) so it stays in
// sync automatically as controls are added/removed, and reuses `title`
// instead of a parallel data-help attribute that would inevitably drift out
// of sync with the real tooltip text.
(function(){
  var hoverEl=null;
  // ui.js's own delegated tooltip listener (also on document, 'mouseover')
  // rewrites `title` -> `data-tip` on an element's FIRST hover (to swap in
  // its own styled instant tooltip instead of the slow native one) — so by
  // the time we look, the attribute we want may be under either name
  // depending on whether that listener already ran on this element before.
  function tipText(el){return el.getAttribute('title')||el.dataset.tip||'';}
  function showTitle(e){
    var el=e.target.closest('[title],[data-tip]');
    var t=el&&tipText(el);
    if(!el||!t||el===hoverEl)return;
    hoverEl=el;
    statusbarHelpRender(t,[]);
  }
  function clearTitle(e){
    if(!hoverEl)return;
    var el=e.target.closest('[title],[data-tip]');
    if(el!==hoverEl)return;
    if(e.relatedTarget&&el.contains(e.relatedTarget))return;
    hoverEl=null;
    updateStatusBarHelp();
  }
  ['props-panel','tl-toolbar','layer-panel','layer-ctrls'].forEach(function(id){
    var root=document.getElementById(id);
    if(!root)return;
    root.addEventListener('mouseover',showTitle);
    root.addEventListener('mouseout',clearTitle);
  });
})();
function updatePropsContext(){
  var hasSel=(state.tool==='select'||state.tool==='subselect')&&selectedPaths.length>0;
  var ctx,hdrText;
  var show={'sel-props-sec':false,'fill-sec':false,'stroke-sec':false,'tool-opts-sec':false,'effects-sec':false,'canvas-sec':false,'layer-sec':false};
  if(state.tool==='fsselect'&&_fsSel){
    // Only the ONE clicked aspect's panel shows — no Position/Size (this
    // tool doesn't offer transform, Select already owns that) and no
    // Effects (blend mode lives on the layer, not a fill/stroke aspect).
    ctx='fsselect';
    show['fill-sec']=_fsSel.kind==='fill'||_fsSel.kind==='fillregion';
    show['stroke-sec']=_fsSel.kind==='stroke';
    var fsSegLabel=_fsSel.kind==='stroke'&&!(_fsSel.segStart===0&&_fsSel.segEnd===_fsSel.path.length)?' (segment)':'';
    var fsFillLabel=_fsSel.kind==='fillregion'?' (région)':'';
    hdrText=(_fsSel.kind==='stroke'?'Stroke'+fsSegLabel:'Fill'+fsFillLabel)+' sélectionné(e)';
  }else if(hasSel){
    ctx='selection';
    show['sel-props-sec']=show['fill-sec']=show['stroke-sec']=show['effects-sec']=true;
    hdrText=selectedPaths.length+(selectedPaths.length>1?' éléments sélectionnés':' élément sélectionné');
  }else if(FILL_STROKE_TOOLS.indexOf(state.tool)>=0){
    ctx='tool:'+state.tool;
    // Fill Brush never touches strokeColor at all (it paints a genuine
    // filled shape, no outline — see draw-bridge.js commitStroke's
    // isFillBrush() branch: strokeColor is always null). Showing the Stroke
    // section while it's the active tool implied it was some kind of
    // stroke/line tool, which is exactly the "fill brush est une stroke pas
    // un fill" confusion reported — it only ever needs Fill + its own Tool
    // Options (Above/Below/Merge placement, pressure range, eraser size).
    show['fill-sec']=true;
    show['stroke-sec']=state.tool!=='fillbrush';
    if(TOOL_OPTS_TOOLS.indexOf(state.tool)>=0)show['tool-opts-sec']=true;
    hdrText=(TOOL_LABELS[state.tool]||state.tool)+' — Options';
  }else{
    ctx='document';
    show['canvas-sec']=true;
    // Nothing selected on canvas — the right panel falls back to Document,
    // which is also the natural place to surface the clicked layer's own
    // properties (Blend Mode). state.activeLayerIdx is already whatever
    // layer was last clicked in the layer list (setActiveLayer runs on
    // every row click), so this needs no separate "layer panel selection"
    // tracking of its own.
    show['layer-sec']=!!(state.layers[state.activeLayerIdx]);
    hdrText='Document';
  }
  // p-blendmode sync moved OUT of the 'document' branch above: it only ran
  // when the props panel happened to be showing that fallback context (no
  // selection, non-fill/stroke tool) — returning from editing a component
  // with, say, the Draw tool still active left the dropdown showing stale
  // data (label/value from whatever it last displayed) even though the
  // layer's actual blendMode — and the real render, buildSceneJson reads
  // it directly — were both already correct. Reported as "on revient dans
  // la scène, on ne voit pas le blend" (component blend mode investigation).
  // Now runs every updatePropsContext() call, regardless of which section
  // ends up visible, so the panel is never stale by the time the user
  // actually looks at it.
  var blendSel=document.getElementById('p-blendmode');
  if(blendSel&&state.layers[state.activeLayerIdx]){
    var bv=state.layers[state.activeLayerIdx].blendMode||'normal';
    blendSel.dataset.value=bv;
    blendSel.textContent=(typeof BLEND_MODE_LABELS!=='undefined'&&BLEND_MODE_LABELS[bv])||bv;
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
  // Fill Brush is UNCONDITIONALLY pressure/centerline-based (draw-bridge.js
  // checks isFillBrush() before ever looking at state.vectorBrush) and never
  // tapers its ends (a fill patch has no "line ends") — the checkbox and the
  // taper toggle only mean anything for the Draw tool, so hide both rather
  // than show controls that silently do nothing while this tool is active.
  var vbRow=document.getElementById('p-vecbrush-row'),taperRow=document.getElementById('p-taper-row');
  if(vbRow)vbRow.style.display=isFillBrush?'none':'flex';
  if(taperRow)taperRow.style.display=isFillBrush?'none':'flex';
  var fbSizeRow=document.getElementById('p-fillbrushsize-row');
  if(fbSizeRow)fbSizeRow.style.display=isFillBrush?'flex':'none';
  // Brush presets apply to Draw's plain constant-width commit path (see
  // draw-bridge.js's commitStroke) AND, now, retroactively to an already-
  // drawn plain stroke via "Apply to selection" below — but not to a
  // vector-brush ribbon or a fill-shape (companion-stacking technique
  // doesn't coexist with either, see applyBrushTexture's own comment), so a
  // selection of ONLY plain strokes is required for the row to make sense
  // during selection; it's unconditionally shown for the Draw tool itself
  // since it just sets the default for the NEXT stroke.
  var eligibleSel=hasSel&&selectedPaths.every(function(p){return p instanceof Path&&p.strokeColor&&!(p.data&&(p.data.isVectorBrush||p.data.isFillShape));});
  var brushPresetRow=document.getElementById('p-brushpreset-row');
  if(brushPresetRow)brushPresetRow.style.display=(state.tool==='draw'||eligibleSel)?'flex':'none';
  var brushApplyRow=document.getElementById('p-brushpreset-apply-row');
  if(brushApplyRow)brushApplyRow.style.display=eligibleSel?'flex':'none';
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
  updateStatusBarHelp();
}
// Populates the Transform section's fields (position/size/rotation/point-
// type/boolean-op rows) — visibility itself is now owned by
// updatePropsContext(), called separately.
function updateSelPropsPanel(){
  if((state.tool!=='select'&&state.tool!=='subselect')||!selectedPaths.length){_selPropsSig='';return;}
  var b=xformSelBounds();if(!b)return;
  // Align toolbar only makes sense with 2+ objects (nothing to align a
  // single selection AGAINST) — toggled every call, not gated behind the
  // signature-change check below, since it only depends on count.
  var alignBar=document.getElementById('align-toolbar');
  if(alignBar)alignBar.style.display=selectedPaths.length>=2?'flex':'none';
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
      // colorHex8(), not .toCSS(true) — the latter always forces alpha to
      // 1 (Paper.js quirk, see colorHex8's own comment in app.js); .dataset
      // .hex8 alongside .value for the same native-<input>-truncates-alpha
      // reason as every other color-input writer in this codebase.
      var css=hasFill?colorHex8(ref.fillColor):state.fillColor;
      state.fillColor=css;state.fillEnabled=hasFill;
      document.getElementById('pm-fill').style.background=css;
      document.getElementById('pm-fill').classList.toggle('none',!hasFill);
      document.getElementById('pm-fill-c').value=css;
      document.getElementById('pm-fill-c').dataset.hex8=css;
      document.getElementById('p-fill-on').checked=hasFill;
      var ftog=document.getElementById('fill-enable-toggle');if(ftog)ftog.classList.toggle('off',!hasFill);
      // Pressure-brush ribbons and fill-brush shapes are ALWAYS
      // strokeColor:null by construction (they paint via fillColor only —
      // see draw-bridge.js's commitStroke) — reading that as "this path
      // has stroke disabled" and writing it into the GLOBAL state.
      // strokeEnabled silently turned the Stroke channel off for every
      // FUTURE new stroke the moment one of these got selected (e.g. by
      // switching tools/selecting while drawing), well after the user had
      // moved on — reported as "quand je change d'outil ça change en fill
      // ou stroke non visible" (feedback #6yqij). Skipped for these two
      // path kinds, matching the same isVectorBrush exclusion already used
      // for Cap/Join/Dash sync just below.
      var isBrushShape=ref.data&&(ref.data.isVectorBrush||ref.data.isFillShape);
      if(!isBrushShape){
        var hasStroke=!!ref.strokeColor;state.strokeEnabled=hasStroke;
        var stog=document.getElementById('stroke-enable-toggle');if(stog)stog.classList.toggle('off',!hasStroke);
      }
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
// Fill/Stroke Select tool: reflect the clicked aspect's ACTUAL current
// color into the Fill/Stroke panel swatches — passive display only (does
// NOT go through setFillColor/setStrokeColor, which would re-apply back
// onto the selection and turn a read into a write).
function updateFsSelPanel(){
  if(state.tool!=='fsselect'||!_fsSel)return;
  var p=_fsSel.path;
  var ftog=document.getElementById('fill-enable-toggle'),stog=document.getElementById('stroke-enable-toggle');
  if((_fsSel.kind==='fill'||_fsSel.kind==='fillregion')&&p.fillColor){
    var fc=colorHex8(p.fillColor);
    document.getElementById('fill-well').style.background=fc;document.getElementById('pm-fill').style.background=fc;
    ['color-fill','pm-fill-c'].forEach(function(id){var el=document.getElementById(id);if(el){el.value=fc;el.dataset.hex8=fc;}});
    if(ftog)ftog.classList.remove('off');
  }else if(_fsSel.kind==='stroke'&&p.strokeColor){
    var sc=colorHex8(p.strokeColor);
    document.getElementById('stroke-well').style.background=sc;document.getElementById('pm-stroke').style.background=sc;
    ['color-stroke','pm-stroke-c'].forEach(function(id){var el=document.getElementById(id);if(el){el.value=sc;el.dataset.hex8=sc;}});
    if(stog)stog.classList.remove('off');
  }
}
// ---- GHOST ALL: turn the visual-only ghosts into real, editable, jointly-
// selected proxy objects (see app.js's _writeBackGhostProxies for how their
// edits get routed back to each proxy's own frame instead of the current one).
var _ghostProxyActive=false;
function selectGhostAll(){
  if(!state.ghostAllFrames)return;
  window.SM.setTool('select');
  var li=state.activeLayerIdx,cf=state.currentFrame;
  var ld=state.layers[li];if(!ld||ld.symbolId){showToast('Ghost All ne fonctionne pas sur un composant');return;}
  var layer=userLayers[li];
  pushUndo();
  var count=0,frameCount=0;
  for(var fi=0;fi<ld.frames.length;fi++){
    if(fi===cf)continue;
    var fr=ld.frames[fi];if(!fr.isKeyframe||!fr.strokes.length)continue;
    frameCount++;
    fr.strokes.forEach(function(sd){
      if(sd.isRaster)return;
      var p=desP(sd,layer);
      p.data.ghostFrame=fi;
      count++;
    });
  }
  _ghostProxyActive=count>0;
  selectedPaths=layer.children.filter(function(c){return c instanceof Path&&!(c.data&&(c.data.isLinkedFillCompanion||c.data.isBrushTextureCopy));});
  state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
  renderArcs();updateUI();
  showToast(count+' élément(s) sur '+frameCount+' image(s) clé — déplacez/transformez ensemble');
}
function clearGhostSelection(){
  if(!_ghostProxyActive)return;
  saveActiveLayerFrame(); // flush any move/scale/rotate on the proxies back to their own frames first
  var layer=userLayers[state.activeLayerIdx];
  if(layer)layer.children.slice().forEach(function(c){if(c.data&&c.data.ghostFrame!==undefined)c.remove();});
  _ghostProxyActive=false;
  clearSel();loadFrame(state.currentFrame);updateUI();
}
function selPropsApplyMove(dx,dy,skipUndo){
  if((!dx&&!dy)||!selectedPaths.length)return;
  if(!skipUndo)pushUndo();
  // translate(), not position=position.add() — see select-bridge.js's live
  // canvas-drag move handler for the full explanation (same fix, same
  // reason): .position round-trips through a bounds computation that the
  // stroke ribbon and its linkedFill backdrop (two different objects,
  // different geometry) don't round the same way, drifting apart a little
  // more on every tick — this field now fires on every 'input' tick of a
  // scrub-drag (many times per gesture), same accumulation risk.
  selectedPaths.forEach(function(p){
    p.translate(new Point(dx,dy));
    if(p.data&&p.data.isVectorBrush&&p.data.centerSegments)p.data.centerSegments.forEach(function(s){s.point=[s.point[0]+dx,s.point[1]+dy];});
    if(p.data&&p.data.linkedFill&&!p.data.linkedFill.removed)p.data.linkedFill.translate(new Point(dx,dy));
    if(p.data&&p.data.brushCompanions)p.data.brushCompanions.forEach(function(c){if(!c.removed)c.translate(new Point(dx,dy));});
  });
  fillRegenerateLinked(userLayers[state.activeLayerIdx],null);
  saveActiveLayerFrame();renderArcs();updateUI();
}
function selPropsApplyScale(sx,sy,anchor,skipUndo){
  if((sx===1&&sy===1)||!selectedPaths.length)return;
  if(!skipUndo)pushUndo();
  selectedPaths.forEach(function(p){
    p.scale(sx,sy,anchor);
    if(p.data&&p.data.isVectorBrush&&p.data.centerSegments){scaleCenterSegments(p.data.centerSegments,sx,sy,anchor.x,anchor.y);rebuildVectorBrushOutline(p);}
  });
  fillRegenerateLinked(userLayers[state.activeLayerIdx],null);
  saveActiveLayerFrame();renderArcs();updateUI();
}
function selPropsApplyRotate(deltaDeg,center,skipUndo){
  if(!deltaDeg||!selectedPaths.length)return;
  if(!skipUndo)pushUndo();
  selectedPaths.forEach(function(p){
    p.rotate(deltaDeg,center);
    if(p.data&&p.data.isVectorBrush&&p.data.centerSegments){rotateCenterSegments(p.data.centerSegments,deltaDeg,center.x,center.y);rebuildVectorBrushOutline(p);}
  });
  fillRegenerateLinked(userLayers[state.activeLayerIdx],null);
  saveActiveLayerFrame();renderArcs();updateUI();
}

// ---- TIMELINE DRAG STATE ----
var _tlDrag={active:false,startL:-1,startF:-1,ghost:null,moved:false,mode:null};
// The camera row (SMCamera.renderGridRow) is always PREPENDED to #frame-grid
// before any real layer row (renderTimeline, below) — every row-index<->pixel
// conversion below assumed rows start flush at the grid's own top, off by
// this row's height whenever a camera track exists. Read live via the DOM
// rather than a constant, since the row's own height varies (16px compact /
// 34px active, camera.js) depending on whether the camera tool is selected.
function camGridRowOffset(){
  var r=document.querySelector('#frame-grid .frow.camrow');
  return r?r.getBoundingClientRect().height:0;
}

function renderTimeline(){
  var hdr=document.getElementById('frame-hdr'),grid=document.getElementById('frame-grid');hdr.innerHTML='';grid.innerHTML='';
  if(window.SMCamera)SMCamera.renderGridRow(grid);
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
  // Same folder-aware order as renderLayerList() (computeLayerRenderOrder)
  // so the two panels' rows always line up — a folder header gets a blank
  // spacer row here (frame data is per-LAYER, a folder has none of its own),
  // and a collapsed folder's member layers are skipped entirely, matching
  // renderLayerList() hiding those same rows.
  var order=computeLayerRenderOrder();
  var rowCount=0;
  order.forEach(function(entry){
    if(entry.type==='folder'){
      // Collapsed: show the representative member's actual keyframes instead
      // of a blank spacer — a collapsed Stroke/Fill/Shadow folder (synced
      // timing via syncLinkedKeyframeFolder) previously hid its keyframes
      // entirely, which read as "the keys disappeared". Cells point at
      // keyLayerIdx via dataset.layer, so the existing insert/clear/drag
      // handlers keep working unmodified — for a linked folder that already
      // propagates the change to every member; for a plain (non-linked)
      // folder it only affects that one representative layer, same as before.
      var collapsed=!!state.layerFolders[entry.id].collapsed;
      var frow=document.createElement('div');frow.className='frow ffolder'+(collapsed?' ffolder-collapsed':'');
      if(collapsed&&entry.keyLayerIdx!==undefined){
        renderKeyframeCellsInto(frow,entry.keyLayerIdx);
      }
      grid.appendChild(frow);rowCount++;
      return;
    }
    if(entry.hidden)return;
    rowCount++;
    var li=entry.idx;var row=document.createElement('div');row.className='frow'+(li===state.activeLayerIdx?' act':'');
    // Collapsed Stroke/Fill/Shadow head row: its OWN strokes are what the
    // 'fl'/'hl' (full/hollow) keyframe dot would normally reflect, but the
    // head is whichever member happens to render topmost (often Shadow,
    // which starts empty on every frame by design — no shadow-generation
    // algorithm exists yet) — showing only ITS content read as "the
    // keyframes went empty" the moment you collapsed, even though Stroke/
    // Fill right underneath are fully drawn. Pass every sibling's layer
    // index so the collapsed row's dots reflect "does ANY channel have
    // content here", matching what collapsing is supposed to summarize.
    var contentIdxs=null;
    if(entry.linkGroupHead&&state.layerLinkGroups[entry.linkGroupId]&&state.layerLinkGroups[entry.linkGroupId].collapsed){
      contentIdxs=[];
      state.layers.forEach(function(l,idx){if(l.linkGroupId===entry.linkGroupId)contentIdxs.push(idx);});
    }
    renderKeyframeCellsInto(row,li,contentIdxs);
    grid.appendChild(row);
  });
  // rowCount only counts state.layers rows — the camera row (prepended
  // above, not part of state.layers) never added its own height here, so
  // the playhead line always stopped short by one row whenever a camera
  // track existed.
  document.getElementById('playhead').style.left=(state.currentFrame*FC)+'px';document.getElementById('playhead').style.height=(30+rowCount*ROW_H+camGridRowOffset())+'px';
  if(window.SMAudio)SMAudio.renderStrip();
}
// Builds one row's worth of frame cells for layer `li` into `rowEl` — shared
// by the normal per-layer row and a collapsed folder's representative row
// (see renderTimeline() above) so both stay pixel-identical.
function hexToRgbTriplet(hex){
  hex=(hex||'').replace('#','');
  if(hex.length===3)hex=hex.split('').map(function(c){return c+c;}).join('');
  var r=parseInt(hex.substr(0,2),16)||0,g=parseInt(hex.substr(2,2),16)||0,b=parseInt(hex.substr(4,2),16)||0;
  return r+','+g+','+b;
}
function renderKeyframeCellsInto(rowEl,li,contentLayerIdxs){
  var ld=state.layers[li];
  // Normally just this layer's own strokes; when contentLayerIdxs is given
  // (collapsed Stroke/Fill/Shadow head row — see renderTimeline's call
  // site), "full" means ANY sibling channel has content at that frame.
  function hasContentAt(fi){
    if(!contentLayerIdxs)return ld.frames[fi].strokes.length>0;
    return contentLayerIdxs.some(function(idx){var f=state.layers[idx]&&state.layers[idx].frames[fi];return f&&f.strokes.length>0;});
  }
  for(var fi=0;fi<state.totalFrames;fi++){var cell=document.createElement('div');cell.className='fc';cell.dataset.frame=fi;cell.dataset.layer=li;
    if(fi===state.currentFrame)cell.classList.add('cur');if(fi<state.waIn||fi>state.waOut)cell.classList.add('outside-wa');
    if(selHas(li,fi))cell.classList.add('sel');
    // Set once per cell (not just on the .km dot) so the CSS-inherited
    // --dot-color/--dot-rgb custom props also reach the cell's OWN
    // kf-full/span-full/span-end backgrounds (see style.css) — the whole
    // span reads in the layer's own color, matching the mockup, not a
    // generic gray.
    if(ld.color){cell.style.setProperty('--dot-color',ld.color);cell.style.setProperty('--dot-rgb',hexToRgbTriplet(ld.color));}
    var fr=ld.frames[fi];
    if(fr.isKeyframe){
      var full=hasContentAt(fi);
      var mk=document.createElement('div');mk.className='km '+(full?'fl':'hl');
      cell.appendChild(mk);
      // the keyframe cell itself carries the span tint so the band reads
      // as starting AT the key, not one cell after it (the hollow
      // end-rectangle only ever sits on held cells — a lone keyframe
      // shows just its dot, like Animate)
      cell.classList.add(full?'kf-full':'kf-empty');
    }
    else if(fr.isInterpolated){cell.classList.add(fr.isManualEdit?'tw-manual':'tw');var td=document.createElement('div');td.className='km td'+(fr.isManualEdit?' manual':'');cell.appendChild(td);}
    else{
      // Extended ("held") frames read differently depending on whether
      // they trace back to an empty or a drawn keyframe, and the last
      // extended cell before the next keyframe gets an end-of-span tick
      // — both distinctions Animate shows and this app previously didn't.
      var hc=false,srcFound=false;
      for(var pi=fi;pi>=0;pi--){if(ld.frames[pi].isKeyframe){hc=hasContentAt(pi);srcFound=true;break;}}
      if(srcFound){
        cell.classList.add(hc?'span-full':'span-empty');
        var nextFr=ld.frames[fi+1];
        if(!nextFr||nextFr.isKeyframe||nextFr.isInterpolated)cell.classList.add('span-end');
      }
    }
    rowEl.appendChild(cell);}
}

// ---- HELD-KEYFRAME SPAN SHRINK/TRIM HANDLE ----
// A held/extended span's end (the small bracket drawn by .fc.span-end::after
// in CSS) has no stored length of its own — it's always wherever the next
// real keyframe happens to sit (see renderTimeline() above). Capture-phase
// so it runs before the existing mousedown handler below — but
// stopPropagation() alone did NOT stop that handler: both listeners sit on
// the SAME element (#frame-grid), and per spec, listeners on one element
// all fire "at target" in registration order regardless of the capture
// flag — only stopImmediatePropagation() skips a later sibling listener on
// that same element. The old stopPropagation()-only version let the plain
// cell mousedown handler ALSO run on every handle grab, starting its own
// range-select drag (_tlDrag) concurrently with this one — that's what
// read as "the drag jumps rows" and "fights with selection": two unrelated
// drag state machines were both live off one mousedown. The last ~7px of a
// .span-end cell counts as grabbing the handle (cell is only 14px wide
// total — see --fc — so this is deliberately close to half the cell).
//
// Trim, not insert: if a REAL keyframe borders the span right after it
// (nextKeyFi), dragging is a video-editor-style trim — that keyframe's own
// content MOVES to the drop frame, exactly like dragging a clip's edge in
// Premiere/After Effects moves the cut point without leaving a duplicate
// behind. The first version always called insertKeyframeAt() at the drop
// frame regardless, which left the ORIGINAL next keyframe untouched in
// place and just wedged an extra one in front of it — visually looked like
// "shrinking" only because the new key's content happened to match, but the
// real next keyframe never actually moved, and dragging couldn't extend the
// span later (past nextKeyFi) at all. If there's no bordering keyframe (the
// hold just runs to the end of the timeline, or of an unbroken empty tail),
// there's nothing to move, so it falls back to the original insert-a-new-
// keyframe behavior — the only way to "shrink" an open-ended hold.
var _spanShrink={active:false,li:-1,srcFi:-1,maxFi:-1,nextKeyFi:-1,dragMax:-1};
document.getElementById('frame-grid').addEventListener('mousedown',function(e){
  if(e.button!==0)return;
  var cell=e.target.closest('.fc.span-end');if(!cell)return;
  // v8: the whole span-end cell is now the grab zone (was progressively
  // widened from 7px → 65% of the cell across earlier passes and still
  // read as fiddly/hard to grab per feedback) — a span-end cell's ONLY
  // useful single-cell action was ever this resize anyway, so there's no
  // competing "just select this cell" gesture being lost by claiming the
  // whole thing. The visible grab-bar (.fc.span-end::after, style.css) is
  // sized to match this generous hit area now instead of a small fraction
  // of it.
  e.preventDefault();e.stopPropagation();e.stopImmediatePropagation();
  var fi=parseInt(cell.dataset.frame),li=parseInt(cell.dataset.layer);
  var ld=state.layers[li];
  if(ld.locked){showToast('Calque verrouillé');return;} // span-end drag is a keyframe edit like any other (feedback #18)
  var srcFi=fi;for(var pi=fi;pi>=0;pi--){if(ld.frames[pi].isKeyframe){srcFi=pi;break;}}
  var nextKeyFi=-1;
  if(ld.frames[fi+1]&&ld.frames[fi+1].isKeyframe)nextKeyFi=fi+1;
  // How far right the handle can be dragged: up to (but not onto) whichever
  // keyframe comes after nextKeyFi — can't trim past the FOLLOWING span's
  // own boundary — or the last frame of the timeline if there's no such
  // keyframe (nothing else to collide with).
  var dragMax=state.totalFrames-1;
  if(nextKeyFi>=0){for(var ni=nextKeyFi+1;ni<state.layers[li].frames.length;ni++){if(ld.frames[ni].isKeyframe){dragMax=ni-1;break;}}}
  else dragMax=fi; // no bordering keyframe — old shrink-only behavior, capped at the current end
  _spanShrink={active:true,li:li,srcFi:srcFi,maxFi:fi,nextKeyFi:nextKeyFi,dragMax:dragMax};
  cell.classList.add('tl-outdrag-hide-end');
  // Trim path (a REAL bordering keyframe is being relocated, not just an
  // abstract span end) — hide ITS dot too for the drag's duration, same
  // reasoning as hiding the old square: otherwise the keyframe you're
  // dragging looks like it never moves until you let go.
  if(nextKeyFi>=0){
    var nextCell=document.querySelector('.fc[data-layer="'+li+'"][data-frame="'+nextKeyFi+'"]');
    if(nextCell)nextCell.classList.add('tl-outdrag-hide-key');
  }
},true);
function _spanShrinkFrameAt(e){
  var wrap=document.getElementById('fg-wrap');var rect=wrap.getBoundingClientRect();
  var x=e.clientX-rect.left+wrap.scrollLeft;
  return Math.max(_spanShrink.srcFi+1,Math.min(_spanShrink.dragMax,Math.floor(x/FC)));
}
window.addEventListener('mousemove',function(e){
  if(!_spanShrink.active)return;
  var fi=_spanShrinkFrameAt(e);
  document.querySelectorAll('.fc.span-drag-band').forEach(function(c){c.classList.remove('span-drag-band');});
  document.querySelectorAll('.fc.span-drag-preview').forEach(function(c){c.classList.remove('span-drag-preview');});
  document.querySelectorAll('.km.drag-key-preview').forEach(function(k){k.remove();});
  // v8: tint the WHOLE prospective span (source key's next frame through
  // the current drag position), not just the single cell under the cursor
  // — reads as the held region actually growing/shrinking as you drag,
  // instead of one small marker silently teleporting between cells with no
  // sense of the span's new extent until you let go.
  for(var bf=_spanShrink.srcFi+1;bf<=fi;bf++){
    var bandCell=document.querySelector('.fc[data-layer="'+_spanShrink.li+'"][data-frame="'+bf+'"]');
    if(bandCell)bandCell.classList.add('span-drag-band');
  }
  var prevCell=document.querySelector('.fc[data-layer="'+_spanShrink.li+'"][data-frame="'+fi+'"]');
  if(prevCell){
    prevCell.classList.add('span-drag-preview');
    // Trim path: show the relocating keyframe's OWN dot (full/hollow,
    // matching whatever it'll actually look like once dropped) riding
    // along with the preview cell, instead of only the abstract square —
    // same visual language as a real committed keyframe.
    if(_spanShrink.nextKeyFi>=0){
      var ld=state.layers[_spanShrink.li];
      var movedFr=ld.frames[_spanShrink.nextKeyFi];
      var full=movedFr&&movedFr.strokes.length>0;
      var dot=document.createElement('div');
      dot.className='km drag-key-preview '+(full?'fl':'hl');
      prevCell.appendChild(dot);
      prevCell.classList.add('span-drag-preview-key');
    }
  }
});
window.addEventListener('mouseup',function(e){
  if(!_spanShrink.active)return;
  document.querySelectorAll('.fc.span-drag-band').forEach(function(c){c.classList.remove('span-drag-band');});
  document.querySelectorAll('.fc.span-drag-preview').forEach(function(c){c.classList.remove('span-drag-preview','span-drag-preview-key');});
  document.querySelectorAll('.km.drag-key-preview').forEach(function(k){k.remove();});
  // Explicit cleanup, not just "a re-render will replace it" — several
  // branches below return early on a no-op drop (dropped back on the
  // source, or back on the already-existing end) with no re-render at all,
  // which would otherwise leave the original marker permanently hidden.
  document.querySelectorAll('.fc.tl-outdrag-hide-end').forEach(function(c){c.classList.remove('tl-outdrag-hide-end');});
  document.querySelectorAll('.fc.tl-outdrag-hide-key').forEach(function(c){c.classList.remove('tl-outdrag-hide-key');});
  var fi=_spanShrinkFrameAt(e);
  var li=_spanShrink.li,nextKeyFi=_spanShrink.nextKeyFi;
  _spanShrink.active=false;
  if(fi<=_spanShrink.srcFi)return; // dropped back on the source key — no-op
  if(nextKeyFi>=0){
    if(fi===nextKeyFi)return; // dropped right back where it already was
    pushUndo();saveAllLayerFrames();
    var ld=state.layers[li];
    var moved=ld.frames[nextKeyFi];
    ld.frames[nextKeyFi]={strokes:[],isKeyframe:false,isInterpolated:false};
    ld.frames[fi]=moved;
    loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
  }else{
    if(fi>=_spanShrink.maxFi)return; // dropped on the existing (open-ended) end — no-op
    insertKeyframeAt(li,fi);
  }
});

// ---- FRAME GRID MOUSE HANDLERS (multi-select + drag) ----
// Matches Animate's convention: dragging from an *empty*/not-yet-selected
// frame extends a range SELECTION (marquee-style), while dragging from a
// cell that already holds a keyframe (or is already part of the current
// multi-selection) RELOCATES it. Without this split, every drag tried to
// move frames, which made simple "select a range" gestures relocate content.
// Clicking anywhere in the timeline that isn't a frame cell (ruler, layer
// panel, transport bar, empty grid margin...) drops the frame selection —
// otherwise it stayed highlighted (and the status-bar help kept showing
// keyframe shortcuts) even after the user had clearly moved on.
document.getElementById('tl-content').addEventListener('mousedown',function(e){
  if(e.button!==0)return;
  if(e.target.closest('.fc'))return;
  if(_sel.frames.length)selClear();
});

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
      selectedPaths=userLayers[li].children.filter(function(c){return c instanceof Path&&!(c.data&&(c.data.isLinkedFillCompanion||c.data.isBrushTextureCopy));});
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
    // Clicking a tween (interpolated) cell jumps straight to the Tween +
    // Easing Curve sections on the right — those are exactly the two panels
    // relevant to an inbetween frame, and they previously stayed collapsed
    // (or whatever section the user last had open) until manually expanded.
    if(fr0&&fr0.isInterpolated){openPropsSection('tween-sec');openPropsSection('easing-sec');}
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
    // generateTweens() already scopes itself to the current frame selection
    // (re-tweens just that span) or the whole layer if nothing's selected —
    // same behavior as the Tween panel's own button/T shortcut, just
    // reachable without opening that panel first (feedback #5qmww). Works
    // equally to CREATE a tween between two keys or to REGENERATE one that
    // already exists (e.g. after editing an endpoint's content).
    {label:'Générer / refaire le tween',shortcut:'T',action:function(){window.SM.generateTweens();}},
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
  var camOff=camGridRowOffset();
  var yRel=e.clientY-gridRect.top-camOff;
  var toF=Math.max(0,Math.min(state.totalFrames-1,Math.floor(xRel/FC)));
  // grid rows run top-to-bottom from the highest layer index (see
  // renderTimeline), so the row under the cursor maps to a flipped index.
  // yRel is measured past the camera row (camOff), which is always
  // prepended before any real layer row and isn't part of state.layers.
  var toL=Math.max(0,Math.min(state.layers.length-1,state.layers.length-1-Math.floor(yRel/ROW_H)));

  if(!_tlDrag.moved){
    var dist=Math.abs(toF-_tlDrag.startF)+Math.abs(toL-_tlDrag.startL);
    if(dist<1)return;
    _tlDrag.moved=true;
    if(_tlDrag.mode==='move'&&!_sel.frames.length)selAdd(_tlDrag.startL,_tlDrag.startF);
    // Fade the ORIGINAL held span each dragged keyframe belongs to, not just
    // the single keyframe cell — otherwise only a small ghost box tracks the
    // cursor while the full colored band it's being pulled out of stays
    // solid and unchanged-looking until drop, reading as "the color stays
    // behind" (real feedback: it wasn't obvious anything was actually being
    // moved out of its old span until you let go).
    if(_tlDrag.mode==='move')fadeDragSourceSpans(true);
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
  var clampedY=Math.max(12,Math.min(state.layers.length*ROW_H-12,yRel));
  _tlDrag.cursorDot.style.left=(clampedX+gridOffLeft)+'px';
  _tlDrag.cursorDot.style.top=(clampedY+camOff+gridOffTop)+'px';

  _tlDrag.ghost.innerHTML='';
  _sel.frames.forEach(function(s){
    var gl=s.layer+offL,gf=s.frame+offF;
    if(gl<0||gl>=state.layers.length||gf<0||gf>=state.totalFrames)return;
    var d=document.createElement('div');
    d.style.cssText='position:absolute;width:'+FC+'px;height:'+ROW_H+'px;background:rgba(74,158,255,.35);border:1px solid rgba(74,158,255,.7);border-radius:2px;box-sizing:border-box;';
    d.style.left=(gf*FC+gridOffLeft)+'px';d.style.top=((state.layers.length-1-gl)*ROW_H+camOff+gridOffTop)+'px';
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

// Held-span fade for an in-progress keyframe MOVE drag (see the mousemove
// handler above) — finds the full run of frames each selected keyframe
// belongs to (its own key up through whatever comes right before the next
// one) and dims those actual .fc cells for the duration of the drag, so the
// span you're pulling a keyframe out of visibly recedes instead of staying
// solid while a separate ghost tracks the cursor elsewhere. Purely a CSS
// class toggle on existing DOM cells — no re-render, so it's cheap enough
// to call once per drag-start.
function fadeDragSourceSpans(on){
  document.querySelectorAll('.fc.tl-drag-fading').forEach(function(c){c.classList.remove('tl-drag-fading');});
  if(!on)return;
  var seen={};
  _sel.frames.forEach(function(s){
    var key=s.layer+':'+s.frame;if(seen[key])return;seen[key]=true;
    var ld=state.layers[s.layer];if(!ld)return;
    var start=s.frame;while(start>0&&!ld.frames[start].isKeyframe)start--;
    var end=s.frame;while(end+1<state.totalFrames&&!ld.frames[end+1].isKeyframe)end++;
    for(var f=start;f<=end;f++){
      var c=document.querySelector('.fc[data-layer="'+s.layer+'"][data-frame="'+f+'"]');
      if(c)c.classList.add('tl-drag-fading');
    }
  });
}
window.addEventListener('mouseup',function(){
  if(!_tlDrag.active)return;
  fadeDragSourceSpans(false);
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
// Shared by renderLayerList() and renderTimeline() so the layer list and
// the frame grid always agree on which rows are visible — a folder header
// entry appears once, right before the first of its (consecutive) member
// layers in display order; a collapsed folder's members are marked
// hidden:true instead of omitted outright, so callers that need the real
// layer index (frame grid columns) can still find it if they need to.
function computeLayerRenderOrder(){
  var order=[],seenFolder={},folderEntry={},seenLinkGroup={};
  for(var i=state.layers.length-1;i>=0;i--){
    var ld=state.layers[i];
    var fid=ld.folderId;
    if(fid&&state.layerFolders[fid]){
      if(!seenFolder[fid]){
        seenFolder[fid]=true;
        var fe={type:'folder',id:fid,keyLayerIdx:i};
        folderEntry[fid]=fe;
        order.push(fe);
      }
      // A collapsed folder still needs ONE layer's frame data to represent the
      // group in renderTimeline() (see its comment) — the LOWEST member index
      // wins simply because this loop runs top-down (highest index first), so
      // the last member visited (lowest index) is whichever one sticks.
      folderEntry[fid].keyLayerIdx=i;
      order.push({type:'layer',idx:i,hidden:!!state.layerFolders[fid].collapsed});
      continue;
    }
    var gid=ld.linkGroupId;
    if(gid&&state.layerLinkGroups[gid]){
      // Stroke/Fill/Shadow link groups (app.js convertLayerToStrokeFillShadowFolder)
      // are NOT layerFolders — deliberately, so each member stays a fully
      // normal, independently-controlled layer row (see that function's own
      // comment). This is only the collapse/expand SPACE-SAVING affordance:
      // the first member encountered (topmost, since this loop runs high-
      // index-first) is the permanent "head" row — it always shows ITS OWN
      // real eye/lock/solo (never an aggregate), and while the group is
      // collapsed the other members are just marked hidden, same technique
      // the folder path above already uses, so renderTimeline's frame-grid
      // needs zero special-casing (each hidden/visible real layer carries
      // its own real frame data, unlike a synthetic folder header row).
      var isHead=!seenLinkGroup[gid];
      if(isHead)seenLinkGroup[gid]=true;
      var collapsed=!!state.layerLinkGroups[gid].collapsed;
      order.push({type:'layer',idx:i,hidden:collapsed&&!isHead,linkGroupId:gid,linkGroupHead:isHead});
    }else{
      order.push({type:'layer',idx:i,hidden:false});
    }
  }
  return order;
}
// Groups the current multi-selection (_layerSel) into a new folder — only
// when they're already CONSECUTIVE in layer order (computeLayerRenderOrder
// derives a folder's membership purely from adjacency, so a non-contiguous
// selection would render wrong: reorder first, like Animate requires too).
function groupSelectionIntoFolder(){
  if(_layerSel.length<2){showToast('Sélectionnez au moins 2 calques');return;}
  var sorted=_layerSel.slice().sort(function(a,b){return a-b;});
  for(var k=1;k<sorted.length;k++)if(sorted[k]!==sorted[k-1]+1){showToast('Les calques doivent être consécutifs — réordonnez-les d\'abord');return;}
  if(sorted.some(function(i){return state.layers[i].folderId;})){showToast('Un calque sélectionné est déjà dans un dossier');return;}
  var fid='folder-'+Date.now()+'-'+Math.floor(Math.random()*1000);
  state.layerFolders[fid]={name:'Dossier',collapsed:false};
  sorted.forEach(function(i){state.layers[i].folderId=fid;});
  renderLayerList();renderTimeline();
}
function folderMemberIndices(fid){
  var idxs=[];
  state.layers.forEach(function(ld,i){if(ld.folderId===fid)idxs.push(i);});
  return idxs;
}
function ungroupFolder(fid){
  state.layers.forEach(function(ld){if(ld.folderId===fid)delete ld.folderId;});
  delete state.layerFolders[fid];
  renderLayerList();renderTimeline();
}
function startFolderRename(fid){
  var fmeta=state.layerFolders[fid];if(!fmeta)return;
  var row=document.querySelector('.lrow.lfolder');
  var frows=[...document.querySelectorAll('.lrow.lfolder')].filter(function(r){return r.querySelector('.lnm').textContent===fmeta.name;});
  var target=frows[0]||row;if(!target)return;
  var nm=target.querySelector('.lnm');if(!nm)return;
  var input=document.createElement('input');input.type='text';input.value=fmeta.name;
  input.style.cssText='flex:1;background:var(--bg);border:1px solid var(--accent);color:var(--text);font-size:11px;padding:0 4px;font-weight:700;';
  nm.replaceWith(input);input.focus();input.select();
  function commit(){fmeta.name=input.value.trim()||fmeta.name;renderLayerList();}
  input.addEventListener('blur',commit);
  input.addEventListener('keydown',function(e){e.stopPropagation();if(e.key==='Enter')input.blur();else if(e.key==='Escape'){input.value=fmeta.name;input.blur();}});
}
// Real icon-handoff SVGs (v5) instead of Material Symbols codepoints — no
// risk of a codepoint missing from the subsetted embedded font (see the
// lock-icon comment below, a bug that class of mistake caused before).
var ICO_EYE='<svg viewBox="0 0 24 24"><path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7-10-7-10-7z" fill="none" stroke="currentColor" stroke-width="1.8"/><circle cx="12" cy="12" r="3" fill="currentColor"/></svg>';
// v11: distinct closed-eye icon for the hidden state (icon-handoff SVG)
// instead of the same open-eye glyph just dimmed via .off — reads as
// "hidden" at a glance instead of "same icon, slightly grayed out".
// v14: explicit fill="currentColor" — SVG's own default fill (black, per
// spec, when unspecified) was silently overriding the CSS `color` this
// icon is supposed to follow (.lico.off{color:var(--text-dark)}), which is
// exactly why the closed-eye icon rendered solid black instead of the same
// dim gray every other "off" icon in this list uses.
var ICO_EYE_CLOSED='<svg viewBox="0 0 24 24"><path fill="currentColor" d="m9.342 18.781-1.931-.518.787-2.939a10.99 10.99 0 0 1-3.237-1.872l-2.153 2.154-1.415-1.415 2.154-2.153a10.957 10.957 0 0 1-2.371-5.07l1.968-.359C3.903 10.811 7.579 14 12 14c4.42 0 8.097-3.188 8.856-7.39l1.968.358a10.958 10.958 0 0 1-2.37 5.071l2.153 2.153-1.415 1.415-2.153-2.154a10.99 10.99 0 0 1-3.237 1.872l.787 2.94-1.931.517-.788-2.94a11.07 11.07 0 0 1-3.74 0l-.788 2.94Z"/></svg>';
var ICO_LOCK='<svg viewBox="0 0 24 24"><rect x="5" y="11" width="14" height="9" rx="1.8" fill="currentColor"/><path d="M8 11V8a4 4 0 018 0v3" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>';
var ICO_UNLOCK='<svg viewBox="0 0 24 24"><rect x="5" y="11" width="14" height="9" rx="1.8" fill="currentColor"/><path d="M8 11V8a4 4 0 017.6-1.8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>';
// Layer color label picker (v5) — a small predefined-swatch "nuancier"
// instead of jumping straight to the full SV/hue/hex ColorPicker. Reuses
// LAYER_COLOR_PALETTE (app.js) so the choices match the auto-assigned
// colors new layers already get; a "+" cell falls through to the full
// picker for anyone who wants an arbitrary custom color.
function openLayerColorSwatches(anchorEl,currentHex,onPick){
  var old=document.getElementById('layer-color-swatches');if(old)old.remove();
  var pop=document.createElement('div');pop.id='layer-color-swatches';pop.className='lcs-pop';
  LAYER_COLOR_PALETTE.forEach(function(hex){
    var sw=document.createElement('button');sw.className='lcs-swatch'+(hex.toLowerCase()===(currentHex||'').toLowerCase()?' sel':'');
    sw.style.background=hex;sw.title=hex;
    sw.addEventListener('click',function(e){e.stopPropagation();onPick(hex);pop.remove();});
    pop.appendChild(sw);
  });
  var custom=document.createElement('button');custom.className='lcs-swatch lcs-custom';custom.title='Couleur personnalisée…';custom.textContent='+';
  custom.addEventListener('click',function(e){e.stopPropagation();pop.remove();window.ColorPicker.open(anchorEl,currentHex,onPick);});
  pop.appendChild(custom);
  document.body.appendChild(pop);
  var r=anchorEl.getBoundingClientRect();
  pop.style.left=Math.min(r.left,window.innerWidth-pop.offsetWidth-8)+'px';
  pop.style.top=(r.bottom+4)+'px';
  setTimeout(function(){
    document.addEventListener('mousedown',function closeOnce(e){
      if(!pop.contains(e.target)){pop.remove();document.removeEventListener('mousedown',closeOnce);}
    });
  },0);
}
function renderLayerList(){
  var list=document.getElementById('layer-list');list.innerHTML='';
  if(window.SMCamera)SMCamera.renderPanelRow(list);
  var order=computeLayerRenderOrder();
  order.forEach(function(entry){
    if(entry.type==='folder'){
      var fid=entry.id,fmeta=state.layerFolders[fid];if(!fmeta)return;
      var members=folderMemberIndices(fid);
      var frow=document.createElement('div');frow.className='lrow lfolder'+(fmeta.collapsed?' collapsed':'');
      var arrow=document.createElement('div');arrow.className='lico larrow';arrow.style.cursor='pointer';arrow.textContent=fmeta.collapsed?'▸':'▾';
      arrow.addEventListener('click',function(e){e.stopPropagation();fmeta.collapsed=!fmeta.collapsed;renderLayerList();renderTimeline();});
      // A collapsed Stroke/Fill/Shadow folder should look and behave like
      // ONE ordinary layer — same eye/lock/solo icon set as a normal row,
      // each toggling that property on EVERY member at once (set to the
      // opposite of the current state, not each one individually inverted,
      // so a mixed on/off state always lands on a single clean result
      // rather than flipping each member independently).
      var allVisible=members.every(function(mi){return state.layers[mi].visible;});
      var allLocked=members.length>0&&members.every(function(mi){return state.layers[mi].locked;});
      var allSolo=members.length>0&&members.every(function(mi){return state.layers[mi].solo;});
      var feye=document.createElement('div');feye.className='lico'+(allVisible?'':' off');feye.title='Show / hide all channels';feye.innerHTML=allVisible?ICO_EYE:ICO_EYE_CLOSED;
      feye.addEventListener('click',function(e){e.stopPropagation();var target=!allVisible;members.forEach(function(mi){if(state.layers[mi].visible!==target)window.SM.toggleLayerVis(mi);});});
      var flock=document.createElement('div');flock.className='lico'+(allLocked?'':' off');flock.title='Lock / unlock all channels';flock.innerHTML=allLocked?ICO_LOCK:ICO_UNLOCK;
      flock.addEventListener('click',function(e){e.stopPropagation();var target=!allLocked;members.forEach(function(mi){if(state.layers[mi].locked!==target)window.SM.toggleLayerLock(mi);});});
      var fsolo=document.createElement('div');fsolo.className='lico solo-btn'+(allSolo?' on':' off');fsolo.title='Solo all channels';fsolo.textContent='S';
      fsolo.addEventListener('click',function(e){e.stopPropagation();var target=!allSolo;members.forEach(function(mi){if(state.layers[mi].solo!==target)window.SM.toggleLayerSolo(mi);});});
      var nm=document.createElement('div');nm.className='lnm';nm.textContent=fmeta.name;nm.style.fontWeight='700';
      frow.appendChild(arrow);frow.appendChild(feye);frow.appendChild(flock);frow.appendChild(fsolo);frow.appendChild(nm);
      frow.addEventListener('dblclick',function(){startFolderRename(fid);});
      frow.addEventListener('contextmenu',function(e){
        e.preventDefault();
        window.showContextMenu(e.clientX,e.clientY,[
          {label:'Renommer le dossier',action:function(){startFolderRename(fid);}},
          {label:'Dissoudre le dossier',action:function(){ungroupFolder(fid);}},
        ]);
      });
      list.appendChild(frow);
      return;
    }
    if(entry.hidden)return;
    var i=entry.idx;
    var ld=state.layers[i];var row=document.createElement('div');row.className='lrow'+(ld.symbolId?' is-comp':'')+(ld.folderId?' in-folder':'')+(ld.linkGroupId?' in-linkgroup':'');row.dataset.layer=i;if(i===state.activeLayerIdx)row.classList.add('act');
    if(_layerSel.indexOf(i)>=0)row.classList.add('sel');
    // Every row reserves the SAME arrow slot a folder header uses, even
    // when it does nothing here — otherwise every other icon shifts left
    // by one slot's width depending on whether the row above happens to be
    // a folder, which reads as visually broken alignment down the list.
    // The head row of a Stroke/Fill/Shadow link group gets a REAL collapse
    // arrow here (toggles state.layerLinkGroups[gid].collapsed) instead of
    // the inert spacer — this is the "open/close" affordance for the group,
    // deliberately NOT a folder header: the row underneath it is still this
    // same real layer's own normal row (own eye/lock/solo/name), just with
    // an arrow prepended.
    if(entry.linkGroupHead){
      var garr=document.createElement('div');garr.className='lico larrow';garr.style.cursor='pointer';
      var gmeta=state.layerLinkGroups[entry.linkGroupId];
      garr.textContent=gmeta.collapsed?'▸':'▾';
      garr.title=gmeta.collapsed?'Afficher les calques Stroke/Fill/Shadow liés':'Masquer les calques Stroke/Fill/Shadow liés';
      garr.addEventListener('click',function(e){e.stopPropagation();gmeta.collapsed=!gmeta.collapsed;renderLayerList();renderTimeline();});
      row.appendChild(garr);
    }else if(!ld.folderId){var spacer=document.createElement('div');spacer.className='lico larrow-spacer';row.appendChild(spacer);}
    // Layer color label (redesign 2026-07-09) — click opens the same
    // color-picker popover used for stroke/fill swatches; every layer
    // already has SOME color (assigned at creation, app.js nextLayerColor),
    // this only ever changes which one, never turns it "off".
    var cdot=document.createElement('div');cdot.className='lico layer-color-dot';cdot.title='Couleur du calque';cdot.style.setProperty('--dot-color',ld.color||'#8b8b9e');
    cdot.addEventListener('click',function(e){
      e.stopPropagation();
      openLayerColorSwatches(cdot,ld.color||'#8b8b9e',function(hex){ld.color=hex;cdot.style.setProperty('--dot-color',hex);renderTimeline();});
    });
    row.appendChild(cdot);
    var eye=document.createElement('div');eye.className='lico'+(ld.visible?'':' off');eye.title='Show / hide layer';eye.innerHTML=ld.visible?ICO_EYE:ICO_EYE_CLOSED;eye.dataset.layer=i;eye.addEventListener('click',function(e){e.stopPropagation();window.SM.toggleLayerVis(parseInt(this.dataset.layer));});
    // Real lock/unlock icon pair (icon-handoff SVGs, v5) — the old single-
    // glyph-dimmed-via-.off workaround was because Material Symbols'
    // lock_open codepoint wasn't in this project's subsetted embedded font
    // (rendered totally blank). Actual distinct shapes now, no font risk.
    var lock=document.createElement('div');lock.className='lico'+(ld.locked?'':' off');lock.title='Lock / unlock layer';lock.innerHTML=ld.locked?ICO_LOCK:ICO_UNLOCK;lock.dataset.layer=i;lock.addEventListener('click',function(e){e.stopPropagation();window.SM.toggleLayerLock(parseInt(this.dataset.layer));});
    // Plain text badge, not a Material Symbols glyph — this project's font
    // is a subsetted embed containing ONLY codepoints already referenced
    // elsewhere (see the lock-icon fix above); inventing a new one renders
    // silently blank. 'S' matches the existing text-badge convention (LFS,
    // the ◈ component badge) already used for icons outside that font.
    var solo=document.createElement('div');solo.className='lico solo-btn'+(ld.solo?' on':' off');solo.title='Solo layer (hide all others)';solo.textContent='S';solo.dataset.layer=i;solo.addEventListener('click',function(e){e.stopPropagation();window.SM.toggleLayerSolo(parseInt(this.dataset.layer));});
    var nm=document.createElement('div');nm.className='lnm';nm.textContent=ld.name;
    row.appendChild(eye);row.appendChild(lock);row.appendChild(solo);
    if(ld.symbolId){var cb=document.createElement('div');cb.className='lico comp-badge';cb.title='Component — double-click to edit';cb.innerHTML='<span style="font-size:11px;line-height:1">\u25c8</span>';row.appendChild(cb);}
    if(ld.lfsGroup){var lb=document.createElement('div');lb.className='lico comp-badge';lb.title='Ligne/Plein/Ombre layer';lb.innerHTML='<span style="font-size:11px;line-height:1">LFS</span>';row.appendChild(lb);}
    // Stroke/Fill/Shadow channel badge — a fully normal layer row (own
    // working eye/lock/solo above, no folder wrapper), just visually
    // labeled so its role in the split is obvious at a glance. Letter
    // matches the channel initial (S/F/O for Ombre, avoiding a clash with
    // the Solo 'S' badge's own single-letter convention would need 2
    // letters here anyway since Fill/Shadow both start differently in FR).
    if(ld.channel){var chLabel=ld.channel==='stroke'?'Tr':ld.channel==='fill'?'Pl':'Om';var chb=document.createElement('div');chb.className='lico comp-badge';chb.title='Calque '+(ld.channel==='stroke'?'Trait':ld.channel==='fill'?'Plein':'Ombre')+' (Stroke/Fill/Shadow) — calque normal, keyframes liées';chb.innerHTML='<span style="font-size:9px;line-height:1;font-weight:700">'+chLabel+'</span>';row.appendChild(chb);}
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
        {label:'Grouper en dossier',disabled:_layerSel.length<2,action:function(){groupSelectionIntoFolder();}},
        {label:'Retirer du dossier',disabled:!l4.folderId,action:function(){delete l4.folderId;renderLayerList();renderTimeline();}},
        {label:'Convertir en composant',disabled:!!l4.symbolId||!!l4.lfsGroup,action:function(){window.SM.convertActiveLayerToComponent();}},
        {label:'Décomposer le composant',disabled:!l4.symbolId,action:function(){window.SM.convertComponentToLayer();}},
        {sep:true},
        {label:'Séparer Stroke/Fill/Shadow (3 calques liés, keyframes partagées)',disabled:!!l4.symbolId||!!l4.lfsGroup||!!l4.linkGroupId,action:function(){window.SM.convertActiveLayerToStrokeFillShadow();}},
        {label:'Dissocier ce calque du groupe Stroke/Fill/Shadow',disabled:!l4.linkGroupId,action:function(){delete l4.channel;delete l4.linkGroupId;renderLayerList();renderTimeline();showToast('Calque dissocié — reste un calque normal, keyframes plus liées');}},
        {label:'Grouper (Ligne/Plein/Ombre)',disabled:!!l4.symbolId||!!l4.lfsGroup,action:function(){window.SM.convertActiveLayerToLFSGroup();}},
        {label:'Éditer Ligne',disabled:!l4.lfsGroup,action:function(){window.SM.enterSymbol(l4.lfsIds.line);}},
        {label:'Éditer Plein',disabled:!l4.lfsGroup,action:function(){window.SM.enterSymbol(l4.lfsIds.full);}},
        {label:'Éditer Ombre',disabled:!l4.lfsGroup,action:function(){window.SM.enterSymbol(l4.lfsIds.shadow);}},
        {label:'Propager Plein sur les autres images',disabled:!l4.lfsGroup,action:function(){window.SM.propagateLFSFill('full');}},
        {label:'Propager Ombre sur les autres images',disabled:!l4.lfsGroup,action:function(){window.SM.propagateLFSFill('shadow');}},
        {label:'Décomposer le groupe',disabled:!l4.lfsGroup,action:function(){window.SM.convertLFSGroupToLayer();}},
      ]);
    });
    list.appendChild(row);
  });
  // v14: audio tracks get their own rows appended after the real layers —
  // synthetic (not part of state.layers, so none of the layer.children
  // consumers CLAUDE.md warns about need to know they exist), but visually
  // and interactively a layer row: name + mute + volume live here now
  // instead of overlapping the waveform strip in the frame grid.
  if(window.SMAudio)window.SMAudio.renderStrip();
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
// ---- Remappable tool shortcuts (v15) ----
// Only the single-key tool-switch bindings are table-driven — the rest of
// onKeyDown below (transport, modifiers, pen-editing Delete/Escape) has
// conditional logic beyond a flat key->action map and stays hardcoded.
// Overrides persist to localStorage so a rebind survives restarts; nothing
// here is imported from Animate/Blender — that's a larger, separate effort
// (see the Settings panel's own note).
var TOOL_SHORTCUTS=[
  {action:'draw',key:'b',label:'Draw'},
  {action:'select',key:'v',label:'Select'},
  {action:'subselect',key:'a',label:'Subselect (node edit)'},
  {action:'fsselect',key:'m',label:'Fill/Stroke Select'},
  {action:'comment',key:'c',label:'Comment'},
  {action:'pen',key:'p',label:'Pen'},
  {action:'line',key:'u',label:'Line'},
  {action:'rect',key:'r',label:'Rectangle'},
  {action:'ellipse',key:'l',label:'Ellipse'},
  {action:'eraser',key:'e',label:'Eraser'},
  {action:'fill',key:'g',label:'Fill'},
  {action:'fillbrush',key:'n',label:'Fill Brush'},
  {action:'eyedropper',key:'i',label:'Eyedropper'},
  {action:'hand',key:'h',label:'Hand (pan)'},
  {action:'zoom',key:'z',label:'Zoom'},
  {action:'toggleOnion',key:'o',label:'Toggle Onion Skin'},
];
var _shortcutOverrides=null;
function shortcutOverrides(){
  if(_shortcutOverrides)return _shortcutOverrides;
  try{_shortcutOverrides=JSON.parse(localStorage.getItem('nemo-shortcuts')||'{}');}catch(e){_shortcutOverrides={};}
  return _shortcutOverrides;
}
function shortcutKeyFor(action){
  var ov=shortcutOverrides();if(ov[action])return ov[action];
  var d=TOOL_SHORTCUTS.find(function(s){return s.action===action;});
  return d?d.key:null;
}
function setShortcutKey(action,key){
  var ov=shortcutOverrides();
  if(key)ov[action]=key.toLowerCase();else delete ov[action];
  try{localStorage.setItem('nemo-shortcuts',JSON.stringify(ov));}catch(e){}
}
function runToolShortcut(k){
  var lk=(k||'').toLowerCase();
  for(var i=0;i<TOOL_SHORTCUTS.length;i++){
    if(shortcutKeyFor(TOOL_SHORTCUTS[i].action)===lk){
      if(TOOL_SHORTCUTS[i].action==='toggleOnion')window.SM.toggleOnion();
      else window.SM.setTool(TOOL_SHORTCUTS[i].action);
      return true;
    }
  }
  return false;
}
// ---- Settings modal: shortcut rebinding UI ----
function renderShortcutsList(){
  var list=document.getElementById('shortcuts-list');if(!list)return;
  list.innerHTML='';
  TOOL_SHORTCUTS.forEach(function(s){
    var row=document.createElement('div');
    row.style.cssText='display:flex;align-items:center;justify-content:space-between;padding:5px 8px;border-radius:4px;font-size:11px;';
    var lbl=document.createElement('span');lbl.textContent=s.label;
    row.appendChild(lbl);
    var keyBtn=document.createElement('button');
    keyBtn.className='pbtn';keyBtn.style.cssText='min-width:60px;font-family:monospace;text-transform:uppercase;';
    keyBtn.textContent=shortcutKeyFor(s.action);
    keyBtn.title='Cliquer puis appuyer sur une touche';
    keyBtn.addEventListener('click',function(){
      keyBtn.textContent='…';keyBtn.classList.add('ac');
      function capture(ev){
        ev.preventDefault();ev.stopPropagation();
        if(ev.key==='Escape'){keyBtn.textContent=shortcutKeyFor(s.action);}
        else{
          // reject a key already bound to a different tool action
          var clash=TOOL_SHORTCUTS.find(function(o){return o.action!==s.action&&shortcutKeyFor(o.action)===ev.key.toLowerCase();});
          if(clash){showToast('Touche déjà utilisée par « '+clash.label+' »');keyBtn.textContent=shortcutKeyFor(s.action);}
          else{setShortcutKey(s.action,ev.key);keyBtn.textContent=ev.key.toLowerCase();}
        }
        keyBtn.classList.remove('ac');
        document.removeEventListener('keydown',capture,true);
      }
      document.addEventListener('keydown',capture,true);
    });
    row.appendChild(keyBtn);
    list.appendChild(row);
  });
}
// Team review: anchored comment pins. `_activeComment` is either an
// existing entry from state.comments (edit-in-place) or a fresh draft
// object not yet pushed to the array — Save decides which by checking
// whether it's already in state.comments, so a cancelled/blurred-away new
// comment never litters the project with an empty pin.
var _activeComment=null;
// Debug-feedback-only state for the popover's tag chips (see comment-save-
// feedback below) — reset on every open, unrelated to _activeComment/
// state.comments since a feedback submission never becomes a team comment.
var _activeFbTags=[];
// Feedback screenshot attachment — a data URL for the thumbnail preview
// (comment-shot-img) held here for the life of the popover; the actual
// GitHub upload (base64 → Contents API commit) happens in
// feedback-bridge.js's uploadScreenshotIfAny() at submit time, not here.
var _activeShotDataUrl=null;
function setCommentShot(dataUrl){
  _activeShotDataUrl=dataUrl;
  var drop=document.getElementById('comment-shot-drop'),prev=document.getElementById('comment-shot-preview'),img=document.getElementById('comment-shot-img');
  if(dataUrl){
    img.src=dataUrl;
    prev.style.display='block';
    drop.style.display='none';
  }else{
    img.src='';
    prev.style.display='none';
    drop.style.display='block';
  }
}
function loadCommentShotFile(file){
  if(!file||file.type.indexOf('image/')!==0)return;
  var reader=new FileReader();
  reader.onload=function(){setCommentShot(reader.result);};
  reader.readAsDataURL(file);
}
// Precise start/stop action recording (comment-record button) — bracketing
// an explicit Record/Stop around the actual repro gesture captures exactly
// what happened for THIS comment, instead of feedback-bridge.js's own
// generic "last 20 actions" guess at submit time. While recording, the
// popover must survive both outside clicks AND switching tools (you need
// the Draw/Eraser/whatever tool active to actually reproduce something) —
// see the outside-click listener below and closeCommentPopover's guard.
var _recording=false,_recActionMark=0,_recClickMark=0,_recStartTime=0;
var _recordedActionTrail=null,_recordedClickTrail=null;
function updateRecordUI(){
  var btn=document.getElementById('comment-record'),status=document.getElementById('comment-record-status');
  if(!btn)return;
  if(_recording){
    btn.textContent='⏹ Arrêter l\'enregistrement';
    btn.classList.add('ac');
    if(status){status.style.display='block';status.textContent='🔴 Enregistrement en cours — change d\'outil et reproduis le problème, puis reviens ici cliquer Stop.';}
  }else{
    btn.textContent='⏺ Enregistrer les actions';
    btn.classList.remove('ac');
    if(status){
      if(_recordedActionTrail||_recordedClickTrail){
        var n=(_recordedActionTrail?_recordedActionTrail.length:0)+(_recordedClickTrail?_recordedClickTrail.length:0);
        status.style.display='block';status.textContent='✓ '+n+' action(s)/clic(s) enregistré(s) pour ce commentaire.';
      }else{
        status.style.display='none';status.textContent='';
      }
    }
  }
}
function startRecording(){
  if(!window.SMFeedback)return;
  _recording=true;
  _recActionMark=window.SMFeedback.actionLogMark();
  _recClickMark=window.SMFeedback.clickLogMark();
  _recStartTime=Date.now();
  _recordedActionTrail=null;_recordedClickTrail=null;
  updateRecordUI();
}
function stopRecording(){
  if(!_recording||!window.SMFeedback)return;
  _recording=false;
  _recordedActionTrail=window.SMFeedback.actionTrailSince(_recActionMark);
  _recordedClickTrail=window.SMFeedback.clickTrailSince(_recClickMark);
  updateRecordUI();
}
function openCommentPopover(worldPt,existing){
  if(!document.getElementById('comment-popover'))return;
  _activeComment=existing||{
    id:'c_'+Date.now().toString(36)+'_'+Math.floor(Math.random()*1e6),
    x:worldPt.x,y:worldPt.y,frame:state.currentFrame,
    authorId:state.userProfile&&state.userProfile.id,
    authorName:state.userProfile&&state.userProfile.name,
    authorColor:state.userProfile&&state.userProfile.color,
    text:'',createdAt:Date.now(),resolved:false,
  };
  var pop=document.getElementById('comment-popover');
  var ca=document.getElementById('canvas-area');
  var rect=ca.getBoundingClientRect();
  var vp=view.projectToView(worldPt);
  pop.style.left=Math.round(rect.left+vp.x+12)+'px';
  pop.style.top=Math.round(rect.top+vp.y-8)+'px';
  document.getElementById('comment-author-row').textContent=
    (existing?'Par ':'Nouveau — ')+(_activeComment.authorName||'Anonyme')+' · frame '+(_activeComment.frame+1);
  document.getElementById('comment-text').value=_activeComment.text||'';
  document.getElementById('comment-resolved').checked=!!_activeComment.resolved;
  document.getElementById('comment-delete').style.display=existing?'':'none';
  _activeFbTags=[];
  document.querySelectorAll('#comment-fb-tags .fb-tag').forEach(function(b){b.classList.remove('active');});
  var fbBlocking=document.getElementById('comment-fb-blocking');if(fbBlocking)fbBlocking.checked=false;
  _recording=false;_recordedActionTrail=null;_recordedClickTrail=null;
  updateRecordUI();
  setCommentShot(existing&&existing.screenshotDataUrl?existing.screenshotDataUrl:null);
  pop.style.display='block';
  document.getElementById('comment-text').focus();
}
function closeCommentPopover(){
  var pop=document.getElementById('comment-popover');
  if(pop)pop.style.display='none';
  _activeComment=null;
  _recording=false;_recordedActionTrail=null;_recordedClickTrail=null;
  setCommentShot(null);
}
function initCommentPopover(){
  var saveBtn=document.getElementById('comment-save'),delBtn=document.getElementById('comment-delete'),pop=document.getElementById('comment-popover');
  if(!saveBtn||!pop)return;
  document.querySelectorAll('#comment-fb-tags .fb-tag').forEach(function(btn){
    btn.addEventListener('click',function(){
      var tag=this.dataset.tag;
      var idx=_activeFbTags.indexOf(tag);
      if(idx>=0){_activeFbTags.splice(idx,1);this.classList.remove('active');}
      else{_activeFbTags.push(tag);this.classList.add('active');}
    });
  });
  var recordBtn=document.getElementById('comment-record');
  if(recordBtn)recordBtn.addEventListener('click',function(){
    if(_recording)stopRecording();else startRecording();
  });
  // Screenshot attachment: drag&drop, click-to-browse, or Cmd+V paste.
  var shotDrop=document.getElementById('comment-shot-drop'),shotInput=document.getElementById('comment-shot-input'),shotRemove=document.getElementById('comment-shot-remove');
  if(shotDrop){
    shotDrop.addEventListener('click',function(){shotInput.click();});
    shotDrop.addEventListener('dragover',function(e){e.preventDefault();shotDrop.style.borderColor='var(--accent)';});
    shotDrop.addEventListener('dragleave',function(){shotDrop.style.borderColor='';});
    shotDrop.addEventListener('drop',function(e){
      e.preventDefault();shotDrop.style.borderColor='';
      if(e.dataTransfer.files&&e.dataTransfer.files[0])loadCommentShotFile(e.dataTransfer.files[0]);
    });
  }
  if(shotInput)shotInput.addEventListener('change',function(){
    if(shotInput.files&&shotInput.files[0])loadCommentShotFile(shotInput.files[0]);
    shotInput.value='';
  });
  if(shotRemove)shotRemove.addEventListener('click',function(e){e.stopPropagation();setCommentShot(null);});
  pop.addEventListener('paste',function(e){
    var items=e.clipboardData&&e.clipboardData.items;
    if(!items)return;
    for(var i=0;i<items.length;i++){
      if(items[i].type.indexOf('image/')===0){loadCommentShotFile(items[i].getAsFile());break;}
    }
  });
  var saveFbBtn=document.getElementById('comment-save-feedback');
  if(saveFbBtn)saveFbBtn.addEventListener('click',function(){
    if(!_activeComment||!window.SMFeedback)return;
    // Forgot to hit Stop — capture up to right now instead of silently
    // discarding whatever was being recorded.
    if(_recording)stopRecording();
    var note=document.getElementById('comment-text').value;
    if(!note.trim()){showToast('Écris une note avant d\'enregistrer le feedback');return;}
    var blocking=document.getElementById('comment-fb-blocking').checked;
    window.SMFeedback.submitFeedback({
      note:note,tags:_activeFbTags.slice(),blocking:blocking,
      pos:new Point(_activeComment.x,_activeComment.y),
      actionTrail:_recordedActionTrail,clickTrail:_recordedClickTrail,
      screenshotDataUrl:_activeShotDataUrl,
    }).then(function(){showToast(_activeShotDataUrl?'Feedback + capture envoyés':'Feedback enregistré (hors projet)');})
      .catch(function(e){console.warn('[feedback] submit failed',e);showToast('Échec de l\'enregistrement du feedback');});
    closeCommentPopover();
  });
  saveBtn.addEventListener('click',function(){
    if(!_activeComment)return;
    if(_recording)stopRecording();
    _activeComment.text=document.getElementById('comment-text').value;
    _activeComment.resolved=document.getElementById('comment-resolved').checked;
    if(!state.comments)state.comments=[];
    if(state.comments.indexOf(_activeComment)<0){
      // A blank draft (never typed into) isn't worth keeping as a pin.
      if(!_activeComment.text.trim()){closeCommentPopover();return;}
      state.comments.push(_activeComment);
    }
    saveActiveLayerFrame();closeCommentPopover();updateUI();refreshFbAvatars();
    if(window.SMEngineBridge&&window.SMEngineBridge.renderNow)window.SMEngineBridge.renderNow();
  });
  delBtn.addEventListener('click',function(){
    if(!_activeComment||!state.comments)return;
    var idx=state.comments.indexOf(_activeComment);
    if(idx>=0)state.comments.splice(idx,1);
    saveActiveLayerFrame();closeCommentPopover();updateUI();refreshFbAvatars();
    if(window.SMEngineBridge&&window.SMEngineBridge.renderNow)window.SMEngineBridge.renderNow();
  });
  // Clicking anywhere outside the popover while it's open just discards an
  // unsaved draft (or leaves an existing comment's edits unsaved) rather
  // than forcing an explicit Cancel button — matches the color-picker
  // popover's own outside-click-closes convention elsewhere in this file.
  // Suppressed entirely while recording: reproducing a bug requires
  // switching to the ACTUAL tool involved (Draw, Eraser…), which is itself
  // a click outside this popover — the whole point of Record is that this
  // must NOT close the window mid-repro.
  document.addEventListener('mousedown',function(e){
    if(pop.style.display==='none')return;
    if(_recording)return;
    if(!pop.contains(e.target))closeCommentPopover();
  },true);
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initCommentPopover);else initCommentPopover();
// ---- Outil texte (v19) ----
// Le moteur Rust ne rasterise pas de glyphes : le texte est rendu une fois
// dans un canvas offscreen (2x pour la nettete) puis insere comme Raster —
// le pipeline image existant (serR/desR, registerImagePixels) fait tout le
// reste (persistance, rendu, selection/deplacement via l'outil V).
var _textPendingPt=null;
function openTextPopover(worldPt){
  _textPendingPt=worldPt.clone();
  var pop=document.getElementById('text-popover');if(!pop)return;
  var v=view.projectToView(worldPt);
  var cr=document.getElementById('drawing-canvas').getBoundingClientRect();
  pop.style.display='block';
  pop.style.left=Math.min(window.innerWidth-240,Math.max(4,cr.left+v.x+10))+'px';
  pop.style.top=Math.min(window.innerHeight-180,Math.max(4,cr.top+v.y+10))+'px';
  var ta=document.getElementById('text-input');ta.value='';ta.focus();
}
function closeTextPopover(){var pop=document.getElementById('text-popover');if(pop)pop.style.display='none';_textPendingPt=null;}
function commitText(){
  var txt=document.getElementById('text-input').value;
  if(!txt.trim()||!_textPendingPt){closeTextPopover();return;}
  var size=parseInt(document.getElementById('text-size').value,10)||48;
  var font=document.getElementById('text-font').value||'sans-serif';
  var color=state.strokeColor||'#000';
  var lines=txt.split('\n');
  var off=document.createElement('canvas');
  var octx=off.getContext('2d');
  octx.font=size*2+'px '+font;
  var wMax=1;lines.forEach(function(l){wMax=Math.max(wMax,octx.measureText(l).width);});
  off.width=Math.ceil(wMax)+8;off.height=lines.length*size*2*1.25+8;
  octx=off.getContext('2d');
  octx.font=size*2+'px '+font;octx.fillStyle=color;octx.textBaseline='top';
  lines.forEach(function(l,i){octx.fillText(l,4,4+i*size*2*1.25);});
  var url=off.toDataURL('image/png');
  pushUndo();
  var layer=userLayers[state.activeLayerIdx];
  var prev=project.activeLayer;layer.activate();
  var r=new Raster(url);
  r.data.src=url;r.data.isText=true;
  var pt=_textPendingPt;
  r.onLoad=function(){r.size=new Size(off.width/2,off.height/2);r.position=pt;saveActiveLayerFrame();updateUI();if(window.SMEngineBridge){if(r.canvas)SMEngineBridge.registerImagePixels&&0;SMEngineBridge.renderNow();}};
  prev.activate();
  closeTextPopover();
}
function initTextPopover(){
  var save=document.getElementById('text-apply');if(!save)return;
  save.addEventListener('click',commitText);
  document.getElementById('text-cancel').addEventListener('click',closeTextPopover);
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initTextPopover);else initTextPopover();
window.openTextPopover=openTextPopover;
function initCycleAndPropagate(){
  var cyc=document.getElementById('btn-cycle');
  if(cyc)cyc.addEventListener('click',function(){
    var n=prompt('Repeter la plage selectionnee combien de fois ?','2');
    if(n===null)return;
    window.SM.repeatSelection(n);
  });
  var prop=document.getElementById('btn-propagate-color');
  if(prop)prop.addEventListener('click',function(){window.SM.propagateColorAllFrames();});
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initCycleAndPropagate);else initCycleAndPropagate();
// Settings tabs (feedback: "le panel réglages encore mal rangé, tu peux
// pas faire des onglets en haut avec différentes sections ?") — 6 stacked
// sections regrouped under 5 tabs; every pane keeps its original ids
// unchanged, so every existing listener in this file still resolves them.
function initSettingsTabs(){
  var bar=document.getElementById('settings-tabs');if(!bar)return;
  bar.querySelectorAll('.settings-tab').forEach(function(tab){
    tab.addEventListener('click',function(){
      bar.querySelectorAll('.settings-tab').forEach(function(t){t.classList.remove('active');});
      tab.classList.add('active');
      document.querySelectorAll('#settings-modal .settings-pane').forEach(function(p){
        p.style.display=(p.dataset.pane===tab.dataset.tab)?'':'none';
      });
    });
  });
}
function initSettingsModal(){
  var btn=document.getElementById('btn-settings'),modal=document.getElementById('settings-modal');
  if(!btn||!modal)return;
  initSettingsTabs();
  btn.addEventListener('click',function(){renderShortcutsList();syncProfileFields();syncFolderFields();refreshFeedbackList();modal.style.display='flex';});
  var topBtn=document.getElementById('project-tabs-settings');
  if(topBtn)topBtn.addEventListener('click',function(){btn.click();});
  var closeBtn=document.getElementById('settings-close');
  if(closeBtn)closeBtn.addEventListener('click',function(){modal.style.display='none';});
  modal.addEventListener('click',function(e){if(e.target===modal)modal.style.display='none';});
  var resetBtn=document.getElementById('shortcuts-reset');
  if(resetBtn)resetBtn.addEventListener('click',function(){
    _shortcutOverrides={};try{localStorage.removeItem('nemo-shortcuts');}catch(e){}
    renderShortcutsList();showToast('Raccourcis réinitialisés');
  });
}
var ROLE_HINTS={
  animator:'Édite ses propres traits. Éditer le trait d\'un autre profil crée une correction à Accepter/Rejeter.',
  supervisor:'Édite n\'importe quel calque directement, sans créer de correction — autorité de révision.',
  producer:'Lecture seule + commentaires. Les outils de dessin/édition sont désactivés.'
};
function syncProfileFields(){
  var nameEl=document.getElementById('profile-name'),colorEl=document.getElementById('profile-color'),wellEl=document.getElementById('profile-color-well'),roleEl=document.getElementById('profile-role'),hintEl=document.getElementById('profile-role-hint');
  if(!nameEl||!state.userProfile)return;
  nameEl.value=state.userProfile.name;
  var c=state.userProfile.color;
  if(colorEl){colorEl.value=c;colorEl.dataset.hex8=c;}
  if(wellEl)wellEl.style.background=c;
  var role=state.userProfile.role||'animator';
  if(roleEl)roleEl.value=role;
  if(hintEl)hintEl.textContent=ROLE_HINTS[role]||'';
}
function initProfileFields(){
  var nameEl=document.getElementById('profile-name'),colorEl=document.getElementById('profile-color'),roleEl=document.getElementById('profile-role');
  if(!nameEl||!colorEl)return;
  syncProfileFields();
  nameEl.addEventListener('input',function(){
    state.userProfile.name=this.value||'Animateur';
    saveUserProfile();
  });
  colorEl.addEventListener('input',function(){
    var v=this.dataset.hex8||this.value;
    state.userProfile.color=v;
    document.getElementById('profile-color-well').style.background=v;
    saveUserProfile();
  });
  if(roleEl)roleEl.addEventListener('change',function(){
    state.userProfile.role=this.value;
    saveUserProfile();
    var hintEl=document.getElementById('profile-role-hint');
    if(hintEl)hintEl.textContent=ROLE_HINTS[this.value]||'';
    // Switching TO producer while mid-edit with a non-view tool active would
    // leave a blocked tool selected — force back to a safe one immediately,
    // same as the boot-time role check.
    if(this.value==='producer'&&PRODUCER_ALLOWED_TOOLS.indexOf(state.tool)<0)window.SM.setTool('hand');
  });
  if(window.ColorPicker)window.ColorPicker.wireColorSwatches([{wrap:'profile-color-well',input:'profile-color'}]);
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initProfileFields);else initProfileFields();
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initSettingsModal);else initSettingsModal();

// ---- Team sync UI (v16, Phase 2) ----
function syncRelTime(ts){
  var s=Math.max(0,Math.round((Date.now()-ts)/1000));
  if(s<60)return 'il y a '+s+'s';
  var m=Math.round(s/60);
  if(m<60)return 'il y a '+m+' min';
  var h=Math.round(m/60);
  return 'il y a '+h+'h'+(m%60?Math.round(m%60)+'min':'');
}
function syncFolderFields(){
  var pathEl=document.getElementById('sync-folder-path');
  if(!pathEl)return;
  pathEl.value=(window.SMProject&&window.SMProject.getSyncFolder())||'';
  var listEl=document.getElementById('sync-updates-list');
  if(listEl)listEl.innerHTML='';
}
function renderSyncUpdates(entries){
  var listEl=document.getElementById('sync-updates-list');if(!listEl)return;
  listEl.innerHTML='';
  if(!entries||!entries.length){
    listEl.innerHTML='<div style="font-size:10px;color:var(--text-dim)">Aucune mise à jour d\'un autre profil.</div>';
    return;
  }
  entries.forEach(function(entry){
    var row=document.createElement('div');
    row.style.cssText='display:flex;align-items:center;justify-content:space-between;gap:6px;padding:5px 6px;border-radius:4px;background:var(--panel3);font-size:10px';
    var label=document.createElement('span');
    var dot=document.createElement('span');dot.style.cssText='display:inline-block;width:8px;height:8px;border-radius:50%;margin-right:5px;background:'+(entry.profileColor||'#888');
    label.appendChild(dot);
    label.appendChild(document.createTextNode(entry.profileName+' — '+syncRelTime(entry.ts)));
    var btn=document.createElement('button');
    btn.className='pbtn';btn.textContent='Fusionner';btn.style.cssText='font-size:10px;padding:3px 8px';
    btn.addEventListener('click',function(){
      window.SMProject.pullAndMerge(entry).then(function(){
        btn.disabled=true;btn.textContent='Fusionné';
      });
    });
    row.appendChild(label);row.appendChild(btn);
    listEl.appendChild(row);
  });
}
function initSyncUI(){
  var chooseBtn=document.getElementById('sync-choose-folder');
  var publishBtn=document.getElementById('sync-publish');
  var checkBtn=document.getElementById('sync-check');
  if(!chooseBtn||!publishBtn||!checkBtn)return;
  chooseBtn.addEventListener('click',function(){
    window.SMProject.chooseSyncFolder().then(function(){syncFolderFields();});
  });
  publishBtn.addEventListener('click',function(){window.SMProject.publishToShared();});
  checkBtn.addEventListener('click',function(){
    var listEl=document.getElementById('sync-updates-list');
    if(listEl)listEl.innerHTML='<div style="font-size:10px;color:var(--text-dim)">Recherche…</div>';
    window.SMProject.checkSharedUpdates().then(renderSyncUpdates);
  });
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initSyncUI);else initSyncUI();

// ---- Feedback inbox (feedback-bridge.js) ----
// Pending vs approved mirrors the Sync updates list right above it in the
// same panel: your own entries are already 'approved' (written straight to
// disk by submitFeedback), anything pulled from a teammate's shared folder
// lands 'pending' until you click Approuver here — see feedback-bridge.js's
// own doc comment for why that trust boundary exists.
function fbTagLabel(t){return {bug:'🐞',perf:'⚡',idea:'💡',polish:'✨'}[t]||t;}
function renderFeedbackList(entries){
  var listEl=document.getElementById('fb-list');if(!listEl)return;
  listEl.innerHTML='';
  if(!entries||!entries.length){
    listEl.innerHTML='<div style="font-size:10px;color:var(--text-dim)">Aucun feedback pour ce projet.</div>';
    return;
  }
  entries.forEach(function(entry){
    var row=document.createElement('div');
    row.style.cssText='display:flex;flex-direction:column;gap:3px;padding:6px 7px;border-radius:4px;background:var(--panel3);font-size:10px';
    var head=document.createElement('div');
    head.style.cssText='display:flex;align-items:center;gap:5px;color:var(--text-dim)';
    var dot=document.createElement('span');dot.style.cssText='display:inline-block;width:7px;height:7px;border-radius:50%;background:'+((entry.author&&entry.author.color)||'#888');
    head.appendChild(dot);
    var tagsTxt=(entry.tags||[]).map(fbTagLabel).join(' ');
    var statusTxt=entry.status==='pending'?'⏳ en attente':(entry.status==='resolved'?'✓ résolu':'');
    head.appendChild(document.createTextNode(((entry.author&&entry.author.name)||'?')+' · frame '+(entry.frame+1)+(tagsTxt?' · '+tagsTxt:'')+(entry.blocking?' · 🚫 bloquant':'')+(statusTxt?' · '+statusTxt:'')));
    row.appendChild(head);
    var note=document.createElement('div');note.style.cssText='color:var(--text)';note.textContent=entry.note;
    row.appendChild(note);
    if(entry.status==='pending'){
      var actions=document.createElement('div');actions.style.cssText='display:flex;gap:5px;margin-top:2px';
      var appBtn=document.createElement('button');appBtn.className='pbtn';appBtn.textContent='Approuver';appBtn.style.cssText='font-size:9px;padding:3px 7px';
      appBtn.addEventListener('click',function(){
        window.SMFeedback.approveFeedback(entry.id).then(refreshFeedbackList);
      });
      actions.appendChild(appBtn);
      row.appendChild(actions);
    }
    listEl.appendChild(row);
  });
}
function refreshFeedbackList(){
  if(!window.SMFeedback)return;
  var listEl=document.getElementById('fb-list');
  if(listEl)listEl.innerHTML='<div style="font-size:10px;color:var(--text-dim)">Chargement…</div>';
  window.SMFeedback.readAllLocal().then(renderFeedbackList);
}
function initFeedbackUI(){
  var pullBtn=document.getElementById('fb-pull');
  if(!pullBtn||!window.SMFeedback)return;
  pullBtn.addEventListener('click',function(){
    pullBtn.disabled=true;var orig=pullBtn.textContent;pullBtn.textContent='Recherche…';
    window.SMFeedback.pullAllIncoming().then(function(imported){
      pullBtn.disabled=false;pullBtn.textContent=orig;
      showToast(imported.length?imported.length+' feedback récupéré(s), en attente d\'approbation':'Rien de nouveau');
      refreshFeedbackList();
    }).catch(function(e){pullBtn.disabled=false;pullBtn.textContent=orig;console.warn('[feedback] pull failed',e);showToast('Échec de la récupération');});
  });
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initFeedbackUI);else initFeedbackUI();

// ---- Collaborator avatar stack (top bar, left of the settings gear) ----
// Shows the current profile plus every author of a TEAM COMMENT on this
// project (state.comments — the in-project "Enregistrer" pins shared via
// Sync équipe), NOT the debug-feedback entries (SMFeedback), which are
// out-of-project tooling for the dev loop. Click opens a compact list of
// those comments; clicking one jumps to its frame. Read-only glance —
// editing/resolving still happens on the pin itself via the Comment tool.
function fbInitial(name){return (name||'?').trim().charAt(0).toUpperCase()||'?';}
function refreshFbAvatars(){
  var el=document.getElementById('fb-avatars');
  if(!el)return;
  var comments=state.comments||[];
  var seen={},people=[];
  if(state.userProfile){people.push({id:state.userProfile.id,name:state.userProfile.name,color:state.userProfile.color,me:true});seen[state.userProfile.id]=true;}
  comments.forEach(function(c){
    if(c.authorId&&!seen[c.authorId]){seen[c.authorId]=true;people.push({id:c.authorId,name:c.authorName,color:c.authorColor});}
  });
  el.innerHTML='';
  var maxShown=4;
  people.slice(0,maxShown).forEach(function(p){
    var av=document.createElement('div');
    av.className='fb-av';av.style.background=p.color||'#888';
    av.textContent=fbInitial(p.name);
    av.title=(p.name||'?')+(p.me?' (toi)':'');
    el.appendChild(av);
  });
  if(people.length>maxShown){
    var more=document.createElement('div');
    more.className='fb-av fb-av-more';more.textContent='+'+(people.length-maxShown);
    el.appendChild(more);
  }
}
function renderFbAvatarPopover(){
  var pop=document.getElementById('fb-avatars-pop');
  if(!pop)return;
  pop.innerHTML='';
  var comments=(state.comments||[]).slice().sort(function(a,b){return (b.createdAt||0)-(a.createdAt||0);});
  if(!comments.length){
    pop.innerHTML='<div class="fb-pop-empty">Aucun commentaire d\'équipe sur ce projet.</div>';
    return;
  }
  comments.forEach(function(cm){
    var item=document.createElement('div');item.className='fb-pop-item';
    var av=document.createElement('div');av.className='fb-pop-av';
    av.style.background=cm.authorColor||'#888';
    av.textContent=fbInitial(cm.authorName);
    item.appendChild(av);
    var body=document.createElement('div');body.className='fb-pop-body';
    var note=document.createElement('div');note.className='fb-pop-note';note.textContent=cm.text||'';note.title=cm.text||'';
    body.appendChild(note);
    var meta=document.createElement('div');meta.className='fb-pop-meta';
    meta.appendChild(document.createTextNode((cm.authorName||'?')+' · frame '+((cm.frame||0)+1)));
    var st=document.createElement('span');
    st.className='fb-pop-status'+(cm.resolved?' resolved':'');
    st.textContent=cm.resolved?'résolu':'à traiter';
    meta.appendChild(st);
    body.appendChild(meta);
    item.appendChild(body);
    item.addEventListener('click',function(){
      if(typeof cm.frame==='number')goToFrame(cm.frame);
      document.getElementById('fb-avatars-pop').classList.remove('open');
    });
    pop.appendChild(item);
  });
}
function toggleFbAvatarsPopover(){
  var pop=document.getElementById('fb-avatars-pop');
  if(!pop)return;
  if(pop.classList.contains('open')){pop.classList.remove('open');return;}
  renderFbAvatarPopover();
  var r=document.getElementById('fb-avatars').getBoundingClientRect();
  pop.style.top=(r.bottom+6)+'px';
  pop.style.right=(window.innerWidth-r.right)+'px';
  pop.classList.add('open');
}
function initFbAvatars(){
  var el=document.getElementById('fb-avatars');
  if(!el)return;
  el.addEventListener('click',function(e){e.stopPropagation();toggleFbAvatarsPopover();});
  document.addEventListener('click',function(e){
    var pop=document.getElementById('fb-avatars-pop');
    if(pop&&pop.classList.contains('open')&&!pop.contains(e.target))pop.classList.remove('open');
  });
  refreshFbAvatars();
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initFbAvatars);else initFbAvatars();

// ---- GitHub feedback triage dashboard (beta testers → mysteropodes/
// strokemotion-feedback Issues) — a dedicated wide modal (#fb-dashboard-
// modal), not crammed into the 340px Réglages sidebar: status tabs + tag
// chips filter the already-fetched list client-side (no refetch per
// click), and Résoudre/Éditer expand an inline textarea in the card
// instead of the browser's prompt()/confirm() (single-line, blocks the
// whole UI, no way to see what you're editing while typing).
// Reading needs no auth (public repo); labeling/closing/commenting/editing
// needs Cyril's own token, entered in Réglages and kept in localStorage on
// his machine only (see feedback-bridge.js's githubTriageToken comment).
var _fbDashIssues=[];
var _fbDashStatus='all'; // all | pending | approved | resolved
var _fbDashTags=[]; // active tag filters, empty = no filter
var _fbDashOpenForms={}; // issueNumber -> 'resolve'|'edit'|null
function ghFbLabelEmoji(l){return {bug:'🐞',perf:'⚡','idée':'💡',polish:'✨',blocking:'🚫',pending:'⏳',resolved:'✓'}[l]||l;}
function fbDashIssueStatus(issue){
  if(issue.labels.indexOf('pending')>=0)return 'pending';
  if(issue.state==='closed'||issue.labels.indexOf('resolved')>=0)return 'resolved';
  return 'approved';
}
function fbDashAllTags(){
  var set={};
  _fbDashIssues.forEach(function(i){i.labels.forEach(function(l){if(l!=='pending'&&l!=='resolved')set[l]=true;});});
  return Object.keys(set);
}
function renderFbDashFilters(){
  var tabsEl=document.getElementById('fb-dash-status-tabs');
  var chipsEl=document.getElementById('fb-dash-tag-chips');
  if(!tabsEl||!chipsEl)return;
  var statuses=[['all','Tous'],['pending','⏳ En attente'],['approved','Approuvé'],['resolved','✓ Résolu']];
  tabsEl.innerHTML='';
  statuses.forEach(function(s){
    var b=document.createElement('button');
    b.className='fb-chip'+(_fbDashStatus===s[0]?' active':'');
    b.textContent=s[1];
    b.addEventListener('click',function(){_fbDashStatus=s[0];renderFbDashFilters();renderFbDashList();});
    tabsEl.appendChild(b);
  });
  chipsEl.innerHTML='';
  fbDashAllTags().forEach(function(tag){
    var b=document.createElement('button');
    var active=_fbDashTags.indexOf(tag)>=0;
    b.className='fb-chip label-'+tag+(active?' active':'');
    b.textContent=ghFbLabelEmoji(tag)+' '+tag;
    b.addEventListener('click',function(){
      var idx=_fbDashTags.indexOf(tag);
      if(idx>=0)_fbDashTags.splice(idx,1);else _fbDashTags.push(tag);
      renderFbDashFilters();renderFbDashList();
    });
    chipsEl.appendChild(b);
  });
}
function fbDashFiltered(){
  return _fbDashIssues.filter(function(issue){
    if(_fbDashStatus!=='all'&&fbDashIssueStatus(issue)!==_fbDashStatus)return false;
    if(_fbDashTags.length&&!_fbDashTags.every(function(t){return issue.labels.indexOf(t)>=0;}))return false;
    return true;
  });
}
function fbDashCardEl(issue){
  var card=document.createElement('div');card.className='fb-card';
  var head=document.createElement('div');head.className='fb-card-head';
  var link=document.createElement('a');link.href=issue.url;link.target='_blank';link.style.color='var(--accent)';link.textContent='#'+issue.number;
  head.appendChild(link);
  issue.labels.forEach(function(l){
    var chip=document.createElement('span');chip.className='fb-chip label-'+l;chip.textContent=ghFbLabelEmoji(l)+' '+l;
    head.appendChild(chip);
  });
  if(issue.state==='closed'){var c=document.createElement('span');c.className='fb-chip';c.textContent='fermé';head.appendChild(c);}
  head.appendChild(document.createTextNode(new Date(issue.createdAt).toLocaleDateString()));
  card.appendChild(head);
  var title=document.createElement('div');title.className='fb-card-title';title.textContent=issue.title;
  card.appendChild(title);
  var noteMatch=/\*\*Note\*\*\n([\s\S]*?)\n\n\*\*Contexte\*\*/.exec(issue.body||'');
  if(noteMatch&&noteMatch[1].trim()!==issue.title){
    var note=document.createElement('div');note.style.cssText='color:var(--text-dim);white-space:pre-wrap';note.textContent=noteMatch[1].trim();
    card.appendChild(note);
  }
  var actions=document.createElement('div');actions.className='fb-card-actions';
  if(issue.labels.indexOf('pending')>=0){
    var appBtn=document.createElement('button');appBtn.className='pbtn';appBtn.textContent='Approuver';appBtn.style.cssText='font-size:9px;padding:3px 7px';
    appBtn.addEventListener('click',function(){
      window.SMFeedback.approveGithubIssue(issue.number,issue.labels).then(refreshFbDashboard).catch(function(e){showToast(e.message);});
    });
    actions.appendChild(appBtn);
  }
  if(issue.state!=='closed'){
    var resBtn=document.createElement('button');resBtn.className='pbtn';resBtn.textContent='Résoudre…';resBtn.style.cssText='font-size:9px;padding:3px 7px';
    resBtn.addEventListener('click',function(){
      _fbDashOpenForms[issue.number]=_fbDashOpenForms[issue.number]==='resolve'?null:'resolve';
      renderFbDashList();
    });
    actions.appendChild(resBtn);
  }
  var editBtn=document.createElement('button');editBtn.className='pbtn';editBtn.textContent='Éditer';editBtn.style.cssText='font-size:9px;padding:3px 7px';
  editBtn.addEventListener('click',function(){
    _fbDashOpenForms[issue.number]=_fbDashOpenForms[issue.number]==='edit'?null:'edit';
    renderFbDashList();
  });
  actions.appendChild(editBtn);
  var delBtn=document.createElement('button');delBtn.className='pbtn';delBtn.textContent='Supprimer';delBtn.style.cssText='font-size:9px;padding:3px 7px;color:#ff8a8a';
  delBtn.addEventListener('click',function(){
    smConfirm('Supprimer définitivement l\'issue #'+issue.number+' ("'+issue.title+'") ? Action irréversible.','Supprimer le feedback').then(function(ok){
      if(!ok)return;
      window.SMFeedback.deleteGithubIssue(issue.nodeId).then(refreshFbDashboard).catch(function(e){showToast(e.message);});
    });
  });
  actions.appendChild(delBtn);
  card.appendChild(actions);
  var openForm=_fbDashOpenForms[issue.number];
  if(openForm==='resolve'){
    var rform=document.createElement('div');rform.className='fb-card-inline-form';
    var rta=document.createElement('textarea');rta.placeholder='Résolution (postée en commentaire sur l\'issue, puis ferme et tague resolved)…';
    var rbtn=document.createElement('button');rbtn.className='pbtn';rbtn.style.cssText='font-size:9px;padding:4px 8px;align-self:flex-start';rbtn.textContent='Confirmer la résolution';
    rbtn.addEventListener('click',function(){
      window.SMFeedback.resolveGithubIssue(issue.number,issue.labels,rta.value).then(function(){
        _fbDashOpenForms[issue.number]=null;refreshFbDashboard();
      }).catch(function(e){showToast(e.message);});
    });
    rform.appendChild(rta);rform.appendChild(rbtn);card.appendChild(rform);
  }else if(openForm==='edit'){
    var eform=document.createElement('div');eform.className='fb-card-inline-form';
    var eta=document.createElement('textarea');eta.value=issue.body||'';eta.style.minHeight='140px';
    var ebtn=document.createElement('button');ebtn.className='pbtn';ebtn.style.cssText='font-size:9px;padding:4px 8px;align-self:flex-start';ebtn.textContent='Enregistrer';
    ebtn.addEventListener('click',function(){
      window.SMFeedback.editGithubIssueBody(issue.number,eta.value).then(function(){
        _fbDashOpenForms[issue.number]=null;refreshFbDashboard();
      }).catch(function(e){showToast(e.message);});
    });
    eform.appendChild(eta);eform.appendChild(ebtn);card.appendChild(eform);
  }
  if(issue.body){
    var det=document.createElement('details');
    var sum=document.createElement('summary');sum.textContent='Voir le trail complet';
    var pre=document.createElement('pre');pre.textContent=issue.body;
    det.appendChild(sum);det.appendChild(pre);card.appendChild(det);
  }
  return card;
}
function renderFbDashList(){
  var listEl=document.getElementById('fb-dash-list');if(!listEl)return;
  var filtered=fbDashFiltered();
  listEl.innerHTML='';
  if(!filtered.length){listEl.innerHTML='<div style="font-size:10px;color:var(--text-dim)">Aucune issue pour ce filtre.</div>';return;}
  filtered.forEach(function(issue){listEl.appendChild(fbDashCardEl(issue));});
}
function refreshFbDashboard(){
  if(!window.SMFeedback)return;
  var listEl=document.getElementById('fb-dash-list');
  if(listEl)listEl.innerHTML='<div style="font-size:10px;color:var(--text-dim)">Chargement…</div>';
  window.SMFeedback.fetchGithubIssues().then(function(issues){
    _fbDashIssues=issues;renderFbDashFilters();renderFbDashList();
  }).catch(function(e){
    if(listEl)listEl.innerHTML='<div style="font-size:10px;color:var(--text-dim)">Échec du chargement — '+e.message+'</div>';
  });
}
function openFbDashboard(){
  document.getElementById('fb-dashboard-modal').style.display='flex';
  refreshFbDashboard();
}
function closeFbDashboard(){document.getElementById('fb-dashboard-modal').style.display='none';}
function initGithubFeedbackUI(){
  var tokenInput=document.getElementById('gh-token'),saveBtn=document.getElementById('gh-token-save');
  var openBtn=document.getElementById('fb-dashboard-open'),closeBtn=document.getElementById('fb-dashboard-close'),refreshBtn=document.getElementById('fb-dash-refresh');
  if(!tokenInput||!window.SMFeedback)return;
  tokenInput.value=window.SMFeedback.githubTriageToken();
  saveBtn.addEventListener('click',function(){
    window.SMFeedback.setGithubTriageToken(tokenInput.value.trim());
    showToast('Token enregistré (local uniquement)');
  });
  if(openBtn)openBtn.addEventListener('click',openFbDashboard);
  if(closeBtn)closeBtn.addEventListener('click',closeFbDashboard);
  if(refreshBtn)refreshBtn.addEventListener('click',refreshFbDashboard);
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initGithubFeedbackUI);else initGithubFeedbackUI();

function onKeyDown(event){
  if((event.metaKey||event.ctrlKey)&&event.key==='z'){event.preventDefault();if(event.shiftKey)redo();else undo();return;}
  if((event.metaKey||event.ctrlKey)&&event.key==='s'){event.preventDefault();if(event.shiftKey)window.SMProject.saveAs();else window.SMProject.save();return;}
  if((event.metaKey||event.ctrlKey)&&event.key==='c'){event.preventDefault();window.SM.copyFrames();return;}
  if((event.metaKey||event.ctrlKey)&&event.key==='x'){event.preventDefault();window.SM.cutFrames();return;}
  if((event.metaKey||event.ctrlKey)&&event.key==='v'){event.preventDefault();window.SM.pasteFrames();return;}
  if(event.target.tagName==='INPUT'||event.target.tagName==='SELECT'||event.target.tagName==='TEXTAREA'||event.target.isContentEditable)return;
  var k=event.key;
  if(runToolShortcut(k)){}
  // NLE-style transport: J/K jump to the previous/next real keyframe on the
  // active layer (Premiere/Final Cut convention); ','/'.' step exactly one
  // frame at a time regardless of keyframes. K was freed up from Fill Brush
  // (moved to N) specifically to make room for this — J/K next-key/prev-key
  // navigation was an explicit, named request.
  else if(k==='j'||k==='J'){if(state.playing)stopPlay();goToFrame(prevKeyframeFrame(state.activeLayerIdx,state.currentFrame));}
  else if(k==='k'||k==='K'){if(state.playing)stopPlay();goToFrame(nextKeyframeFrame(state.activeLayerIdx,state.currentFrame));}
  else if(k===','){if(state.playing)stopPlay();goToFrame(state.currentFrame-1);}
  else if(k==='.'||k===';'){if(state.playing)stopPlay();goToFrame(state.currentFrame+1);}
  else if(k==='x'||k==='X')window.SM.swapStrokeFill();
  else if(k==='t'||k==='T')window.SM.generateTweens();
  else if(k===' '){event.preventDefault();if(!state.spaceDown){state.spaceDown=true;canvasEl.style.cursor='grab';}}
  else if(k==='Alt'){state.altDown=true;}
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
    else if(state.tool==='fsselect'&&_fsSel){event.preventDefault();fsApplyDelete();}
    // Subselect: Delete removes just the selected anchor(s) from the path
    // and lets the curve reflow through the remaining points (Illustrator/
    // Figma convention) — previously had NO handler at all here, so Delete
    // silently did nothing while a node was selected with the subselect
    // tool (reported as "impossible d'enlever le point de la ligne").
    else if(state.tool==='subselect'&&_nodeSel.length>0){
      event.preventDefault();
      var ntp=nodeEditTargetPath();
      if(ntp){
        var isCenter=!!(ntp.data&&ntp.data.isVectorBrush&&ntp.data.centerSegments);
        var curLen=isCenter?ntp.data.centerSegments.length:ntp.segments.length;
        var minPts=isCenter?2:(ntp.closed?3:2);
        if(curLen-_nodeSel.length<minPts){showToast('Pas assez de points restants');}
        else{
          pushUndo();
          var idxsDesc=_nodeSel.slice().sort(function(a,b){return b-a;});
          idxsDesc.forEach(function(i){
            if(isCenter)ntp.data.centerSegments.splice(i,1);
            else ntp.removeSegment(i);
          });
          if(isCenter)rebuildVectorBrushOutline(ntp);
          _nodeSel=[];
          renderNodeHandles();
          fillRegenerateLinked(userLayers[state.activeLayerIdx],ntp);
          saveActiveLayerFrame();renderArcs();updateUI();
          if(window.SMEngineBridge)window.SMEngineBridge.renderNow();
        }
      }
    }
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
function onKeyUp(event){if(event.key===' '){state.spaceDown=false;state.isPanning=false;var cc={draw:'crosshair',pen:'crosshair',line:'crosshair',rect:'crosshair',ellipse:'crosshair',select:'default',subselect:'default',eraser:'pointer',fill:'crosshair',fillbrush:'crosshair',eyedropper:'crosshair',hand:'grab',zoom:'zoom-in',rotate:'grab',perspective:'crosshair'};canvasEl.style.cursor=cc[state.tool]||'default';}else if(event.key==='Alt'){state.altDown=false;}}

document.addEventListener('keydown',onKeyDown);document.addEventListener('keyup',onKeyUp);
document.querySelectorAll('.tool-btn').forEach(function(b){b.addEventListener('click',function(){window.SM.setTool(this.dataset.tool);});});
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
document.getElementById('color-fill').addEventListener('input',function(){window.SM.setFillColor(this.dataset.hex8||this.value);if(!state.fillEnabled)window.SM.setFillEnabled(true);});
document.getElementById('pm-fill-c').addEventListener('input',function(){var v=this.dataset.hex8||this.value;window.SM.setFillColor(v);document.getElementById('color-fill').value=v;document.getElementById('color-fill').dataset.hex8=v;if(!state.fillEnabled)window.SM.setFillEnabled(true);});
document.getElementById('p-fill-on').addEventListener('change',function(){window.SM.setFillEnabled(this.checked);});
// (fill-enable-toggle / stroke-enable-toggle section-header buttons removed
// per redesign — the left panel's cw-eye badges are the one on/off switch
// now; every other reference to those old IDs is null-guarded.)
document.getElementById('fill-enable-toggle-lp').addEventListener('click',function(){window.SM.setFillEnabled(!state.fillEnabled);});
document.getElementById('stroke-enable-toggle-lp').addEventListener('click',function(){window.SM.setStrokeEnabled(!state.strokeEnabled);});
document.getElementById('color-stroke').addEventListener('input',function(){window.SM.setStrokeColor(this.dataset.hex8||this.value);if(!state.strokeEnabled)window.SM.setStrokeEnabled(true);});
document.getElementById('pm-stroke-c').addEventListener('input',function(){var v=this.dataset.hex8||this.value;window.SM.setStrokeColor(v);document.getElementById('color-stroke').value=v;document.getElementById('color-stroke').dataset.hex8=v;if(!state.strokeEnabled)window.SM.setStrokeEnabled(true);});
// Paint the panel's fill swatch from the actual starting state.fillColor/
// fillEnabled on load — without this it sits at whatever background the
// static HTML happened to have (transparent) until the user touches it.
window.SM.setFillColor(state.fillColor);
window.SM.setFillEnabled(state.fillEnabled);
window.SM.setStrokeEnabled(state.strokeEnabled);
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
// Transform panel fields (position/size/rotation) used to only apply on
// 'change' — the native event that fires once, at the END of a drag-scrub
// or on blur after typing — so dragging one of these values showed no
// effect on the canvas (or the panel's OWN other fields, e.g. dragging
// Width while Height stays stale) until you released the mouse. Every
// scrub-capable field elsewhere in the app (ui.js's pointer-based scrub
// handler) already dispatches a real 'input' event on every drag tick
// specifically so listeners can react live — these five just never listened
// for it. Switched to 'input' for live application, with a per-field
// "gesture already has an undo entry" flag (reset on the drag-ending
// 'change') so a single scrub still pushes exactly one undo step instead of
// one per tick.
function wireLiveXformField(id,apply){
  var el=document.getElementById(id),started=false;
  el.addEventListener('input',function(){apply(this,started);started=true;});
  el.addEventListener('change',function(){started=false;});
}
wireLiveXformField('sp-x',function(el,started){var b=xformSelBounds();if(!b)return;selPropsApplyMove((parseFloat(el.value)||0)-b.x,0,started);});
wireLiveXformField('sp-y',function(el,started){var b=xformSelBounds();if(!b)return;selPropsApplyMove(0,(parseFloat(el.value)||0)-b.y,started);});
wireLiveXformField('sp-w',function(el,started){var b=xformSelBounds();if(!b||b.width<0.01)return;var nv=Math.max(0.01,parseFloat(el.value)||b.width);selPropsApplyScale(nv/b.width,1,b.topLeft,started);});
wireLiveXformField('sp-h',function(el,started){var b=xformSelBounds();if(!b||b.height<0.01)return;var nv=Math.max(0.01,parseFloat(el.value)||b.height);selPropsApplyScale(1,nv/b.height,b.topLeft,started);});
wireLiveXformField('sp-rot',function(el,started){var b=xformSelBounds();if(!b)return;var nv=parseFloat(el.value)||0;var delta=nv-(state.selRotAccum||0);state.selRotAccum=nv;selPropsApplyRotate(delta,xformAnchorPoint(b),started);});
// Anchor-point (pivot) picker — see tools.js xformAnchorPoint's own comment.
// Clicking a dot just changes WHICH point future rotations pivot around
// (state.xformAnchorKey); it doesn't move anything on its own, so no
// undo/render is needed here beyond repainting the widget's active dot.
function renderXformAnchorGrid(){
  // A custom Alt+click anchor (select-bridge.js) overrides every preset —
  // none of the 9 dots is "active" while it's in effect, otherwise the
  // widget would misleadingly keep showing e.g. "center" highlighted while
  // the pivot actually sits wherever the artist Alt+clicked.
  document.querySelectorAll('#xform-anchor-grid .xa-dot').forEach(function(btn){
    btn.classList.toggle('xa-active',!state.xformAnchorCustom&&btn.dataset.key===state.xformAnchorKey);
  });
}
document.querySelectorAll('#xform-anchor-grid .xa-dot').forEach(function(btn){
  btn.addEventListener('click',function(e){
    e.stopPropagation();
    state.xformAnchorKey=btn.dataset.key;
    state.xformAnchorCustom=null; // explicit preset pick always wins back over a custom point
    renderXformAnchorGrid();
    if(window.renderTransformHandles)renderTransformHandles();
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
  });
});
renderXformAnchorGrid();
document.querySelectorAll('#align-toolbar .align-btn').forEach(function(btn){
  btn.addEventListener('click',function(){alignSelection(btn.dataset.align);});
});
document.getElementById('btn-pt-corner').addEventListener('click',function(){window.SM.setPointType('corner');});
document.getElementById('btn-pt-smooth').addEventListener('click',function(){window.SM.setPointType('smooth');});
document.getElementById('btn-pt-symmetric').addEventListener('click',function(){window.SM.setPointType('symmetric');});
document.getElementById('btn-bool-unite').addEventListener('click',function(){window.SM.booleanOp('unite');});
document.getElementById('btn-bool-subtract').addEventListener('click',function(){window.SM.booleanOp('subtract');});
document.getElementById('btn-bool-intersect').addEventListener('click',function(){window.SM.booleanOp('intersect');});
document.getElementById('btn-bool-exclude').addEventListener('click',function(){window.SM.booleanOp('exclude');});
document.getElementById('p-erasersize').addEventListener('input',function(){window.SM.setEraserSize(this.value);});
document.getElementById('p-fillbrushsize').addEventListener('input',function(){window.SM.setFillBrushSize(this.value);});
document.getElementById('p-brushpreset').addEventListener('change',function(){window.SM.setBrushPreset(this.value);if(window.BrushPresetPicker)window.BrushPresetPicker.paintButton(this.value);});
document.getElementById('btn-brushpreset-apply').addEventListener('click',function(){
  var preset=state.brushPreset;
  var eligible=selectedPaths.filter(function(p){return p instanceof Path&&p.strokeColor&&!(p.data&&(p.data.isVectorBrush||p.data.isFillShape));});
  if(!eligible.length)return;
  pushUndo();
  eligible.forEach(function(p){
    // Re-applying: drop the OLD companions first (a fresh applyBrushTexture
    // call only touches `p` itself + inserts new ones — it doesn't know
    // about or clean up a previous texture's copies, so switching presets
    // on the same stroke would otherwise leave the old jittered clones
    // behind underneath the new ones).
    if(p.data&&p.data.brushCompanions){
      p.data.brushCompanions.forEach(function(c){c.remove();});
      p.data.brushCompanions=null;
      // applyBrushTexture hid the primary (opacity 0 for stroke-only paths,
      // strokeColor nulled for filled ones — see its own comment) — restore
      // both remembered values, otherwise switching back to "None" leaves
      // the stroke permanently invisible / permanently fill-only.
      if(p.data.preTextureOpacity!==undefined){p.opacity=p.data.preTextureOpacity;delete p.data.preTextureOpacity;}
      if(p.data.preTextureStroke!==undefined){p.strokeColor=p.data.preTextureStroke;delete p.data.preTextureStroke;}
    }
    if(preset&&preset!=='none')applyBrushTexture(p,preset);
  });
  saveActiveLayerFrame();updateUI();showToast('Brush appliqué à la sélection');
});
if(window.BrushPresetPicker)window.BrushPresetPicker.paintButton(state.brushPreset);
document.getElementById('p-drawmode').addEventListener('change',function(){window.SM.setDrawMode(this.value);});
document.getElementById('p-pmin').addEventListener('input',function(){window.SM.setPressureMin(this.value);});
document.getElementById('p-pmax').addEventListener('input',function(){window.SM.setPressureMax(this.value);});
document.getElementById('p-pinv').addEventListener('change',function(){window.SM.setPressureInvert(this.checked);});
document.getElementById('p-taper').addEventListener('change',function(){window.SM.setTaperEnds(this.checked);});
document.getElementById('p-shadowmode').addEventListener('change',function(){window.SM.setShadowMode(this.checked);});
document.getElementById('p-cw').addEventListener('change',function(){window.SM.setCanvasSize(parseInt(this.value),state.canvasH);});
document.getElementById('p-ch').addEventListener('change',function(){window.SM.setCanvasSize(state.canvasW,parseInt(this.value));});
document.getElementById('p-cbg').addEventListener('input',function(){window.SM.setCanvasBg(this.value);});
document.getElementById('p-clip').addEventListener('change',function(){window.SM.setCanvasClip(this.checked);});
document.getElementById('p-safety').addEventListener('change',function(){window.SM.setSafetyZones(this.checked);});
document.getElementById('p-persp-on').addEventListener('change',function(){state.perspectiveEnabled=this.checked;if(this.checked&&window.ensurePerspectiveVPs)window.ensurePerspectiveVPs();if(window.SMEngineBridge)window.SMEngineBridge.renderNow();});
document.getElementById('p-persp-mode').addEventListener('change',function(){if(window.setPerspectiveMode)window.setPerspectiveMode(this.value);});
document.getElementById('p-persp-density').addEventListener('input',function(){state.perspectiveDensity=parseInt(this.value)||24;if(window.SMEngineBridge)window.SMEngineBridge.renderNow();});
document.getElementById('p-persp-lock').addEventListener('change',function(){var locked=this.checked;(window.ensurePerspectiveVPs?window.ensurePerspectiveVPs():[]).forEach(function(vp){vp.locked=locked;});if(window.SMEngineBridge)window.SMEngineBridge.renderNow();});
document.getElementById('btn-persp-reset').addEventListener('click',function(){if(window.resetPerspectiveVPs)window.resetPerspectiveVPs();});
// Custom blend-mode dropdown (feedback #17): hovering an option in the open
// list applies that blend mode to the active layer IMMEDIATELY as a live
// canvas preview; clicking commits it (with undo), while closing any other
// way (outside click, Escape, re-click on the field) reverts to the mode
// the layer had when the list opened. A native <select> can't do this: its
// open popup is OS-rendered and exposes no per-option hover events.
var BLEND_MODE_LABELS={normal:'Normal',multiply:'Multiply',screen:'Screen',overlay:'Overlay',darken:'Darken',lighten:'Lighten',colorDodge:'Color Dodge',colorBurn:'Color Burn',hardLight:'Hard Light',softLight:'Soft Light',difference:'Difference',exclusion:'Exclusion',hue:'Hue',saturation:'Saturation',color:'Color',luminosity:'Luminosity'};
(function initBlendDropdown(){
  var dd=document.getElementById('p-blendmode');if(!dd)return;
  var pop=document.createElement('div');pop.id='blend-pop';document.body.appendChild(pop);
  var origMode=null; // layer's mode when the list opened — the revert target while previewing
  function currentLd(){return state.layers[state.activeLayerIdx];}
  function applyPreview(v){
    var ld=currentLd();if(!ld)return;
    ld.blendMode=v==='normal'?undefined:v;
    window._sceneVersion=(window._sceneVersion||0)+1;
    if(window.SMEngineBridge&&window.SMEngineBridge.renderNow)window.SMEngineBridge.renderNow();
  }
  function setLabel(v){dd.dataset.value=v;dd.textContent=BLEND_MODE_LABELS[v]||v;}
  function close(revert){
    pop.style.display='none';
    if(revert&&origMode!==null)applyPreview(origMode);
    origMode=null;
  }
  function open(){
    var ld=currentLd();if(!ld)return;
    origMode=ld.blendMode||'normal';
    pop.innerHTML='';
    Object.keys(BLEND_MODE_LABELS).forEach(function(v){
      var it=document.createElement('div');
      it.className='blend-opt'+(v===origMode?' sel':'');
      it.textContent=BLEND_MODE_LABELS[v];
      it.addEventListener('mouseenter',function(){applyPreview(v);});
      it.addEventListener('click',function(e){
        e.stopPropagation();
        // Restore the original first so pushUndo snapshots the true
        // pre-preview state, then commit the pick on top of it.
        var ld2=currentLd();if(ld2)ld2.blendMode=origMode==='normal'?undefined:origMode;
        pushUndo();
        applyPreview(v);
        setLabel(v);
        origMode=null;
        close(false);
      });
      pop.appendChild(it);
    });
    var r=dd.getBoundingClientRect();
    pop.style.display='block';
    pop.style.left=Math.max(8,Math.min(window.innerWidth-pop.offsetWidth-8,r.left))+'px';
    var top=r.bottom+4;
    if(top+pop.offsetHeight>window.innerHeight-8)top=Math.max(8,r.top-pop.offsetHeight-4);
    pop.style.top=top+'px';
  }
  // Leaving the whole list without picking: preview back to the original so
  // the canvas never lingers on a mode the user merely passed over.
  pop.addEventListener('mouseleave',function(){if(origMode!==null)applyPreview(origMode);});
  dd.addEventListener('click',function(e){e.stopPropagation();if(pop.style.display==='block')close(true);else open();});
  document.addEventListener('pointerdown',function(e){if(pop.style.display==='block'&&!pop.contains(e.target)&&e.target!==dd)close(true);});
  window.addEventListener('keydown',function(e){if(e.key==='Escape'&&pop.style.display==='block')close(true);});
})();
// Same canvas/fps/frame-count controls, duplicated in the Project panel —
// both sets write through the same SM.setCanvasSize/setFps/setTotalFrames
// so either one stays in sync via syncDocFields().
document.getElementById('proj-w').addEventListener('change',function(){window.SM.setCanvasSize(parseInt(this.value),state.canvasH);});
document.getElementById('proj-h').addEventListener('change',function(){window.SM.setCanvasSize(state.canvasW,parseInt(this.value));});
document.getElementById('proj-fps').addEventListener('change',function(){window.SM.setFps(parseInt(this.value));});
document.getElementById('proj-frames').addEventListener('change',function(){window.SM.setTotalFrames(parseInt(this.value));});
document.getElementById('btn-fit').addEventListener('click',function(){window.SM.fitCanvas();});
document.getElementById('btn-resetv').addEventListener('click',function(){window.SM.resetView();});
// v10: canvas-viewport zoom/fit pills (mockup) — #zoom-scrub picks up ui.js's
// generic pointer-scrub handler automatically (class="scrub" is all that
// takes), this just applies the resulting value to the live view on every
// drag tick ('input', not 'change') so the canvas visibly zooms while
// scrubbing instead of only on release.
document.getElementById('zoom-scrub').addEventListener('input',function(){
  var pct=parseFloat(this.value);if(!pct||pct<=0)return;
  view.zoom=pct/100;renderArcs();if(window.SMEngineBridge)window.SMEngineBridge.renderNow();
});
document.getElementById('canvas-fit-btn').addEventListener('click',function(e){
  var r=this.getBoundingClientRect();
  window.showContextMenu(r.left,r.top-70,[
    {label:'Fit',action:function(){window.SM.fitCanvas();}},
    {label:'Reset View (100%)',action:function(){window.SM.resetView();}},
  ]);
});
document.getElementById('p-resamp').addEventListener('input',function(){window.SM.setResamplePts(parseInt(this.value));});
document.getElementById('p-step').addEventListener('change',function(){window.SM.setTweenStep(this.value);});
document.getElementById('p-skipmanual').addEventListener('change',function(){state.tweenSkipManual=this.checked;});
document.getElementById('btn-tw').addEventListener('click',function(){window.SM.generateTweens();});
document.getElementById('btn-os').addEventListener('click',function(){window.SM.toggleOnion();});
document.getElementById('btn-ghost-all').addEventListener('click',function(){window.SM.toggleGhostAll();});
document.getElementById('btn-ghost-select').addEventListener('click',function(){selectGhostAll();});
// Team review view filter — cycles All -> Mine -> Corrections, purely a
// render-time filter in engine-bridge.js's buildSceneJson (never touches
// the document), so switching is instant and always reversible.
var REVISION_VIEW_LABELS={all:'Tout',mine:'Mes traits',revisions:'Corrections'};
var REVISION_VIEW_ORDER=['all','mine','revisions'];
document.getElementById('btn-revision-view').addEventListener('click',function(){
  var idx=REVISION_VIEW_ORDER.indexOf(state.revisionView);
  state.revisionView=REVISION_VIEW_ORDER[(idx+1)%REVISION_VIEW_ORDER.length];
  this.textContent=REVISION_VIEW_LABELS[state.revisionView];
  this.classList.toggle('active',state.revisionView!=='all');
  window.SMEngineBridge.renderNow();
});
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
document.getElementById('btn-loop').addEventListener('contextmenu',function(e){e.preventDefault();window.SM.togglePingPongPlayback();});
// Onion Skin options popover (feedback #23): right-click on the timeline's
// onion toggle opens the options that used to live in the right panel;
// left-click keeps toggling on/off as before.
(function initOnionPopover(){
  var btn=document.getElementById('btn-os'),pop=document.getElementById('onion-pop');
  if(!btn||!pop)return;
  btn.addEventListener('contextmenu',function(e){
    e.preventDefault();e.stopPropagation();
    var r=btn.getBoundingClientRect();
    pop.style.display='block';
    pop.style.left=Math.max(8,Math.min(window.innerWidth-228,r.left))+'px';
    // Above the button (the timeline sits at the bottom of the window) —
    // measured AFTER display:block so offsetHeight is real.
    pop.style.top=Math.max(8,r.top-pop.offsetHeight-6)+'px';
  });
  document.addEventListener('pointerdown',function(e){
    if(pop.style.display!=='none'&&!pop.contains(e.target)&&e.target!==btn)pop.style.display='none';
  });
})();
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
  var sizeRow=document.getElementById('exp-size-row');
  var wInput=document.getElementById('exp-w'),hInput=document.getElementById('exp-h');
  var alphaRow=document.getElementById('exp-alpha-row');
  var progEl=document.getElementById('exp-progress');
  var runBtn=document.getElementById('exp-run');
  // Alpha only makes sense for formats that actually HAVE an alpha channel:
  // a PNG sequence trivially does, and ProRes 4444 (.mov) supports it too
  // (switches ffmpeg profile — see export.js) — MP4/H.264 and GIF have no
  // alpha channel at the codec level at all, and TIFF/SVG/Lottie weren't
  // asked for, so kept out of scope rather than guessing.
  var ALPHA_FORMATS=['png','prores'];
  var NO_SCALE_FORMATS=['lottie','rive','ae-camera'];
  function updateScaleVisibility(){
    var v=fmtSel.value;
    scaleRow.style.display=NO_SCALE_FORMATS.indexOf(v)>=0?'none':'flex';
    sizeRow.style.display=(NO_SCALE_FORMATS.indexOf(v)<0&&scaleSel.value==='custom')?'flex':'none';
    alphaRow.style.display=ALPHA_FORMATS.indexOf(v)>=0?'flex':'none';
    var riveHint=document.getElementById('exp-rive-hint');
    if(riveHint)riveHint.style.display=(v==='rive')?'flex':'none';
    var aeCamHint=document.getElementById('exp-ae-camera-hint');
    if(aeCamHint)aeCamHint.style.display=(v==='ae-camera')?'flex':'none';
  }
  fmtSel.addEventListener('change',updateScaleVisibility);
  scaleSel.addEventListener('change',function(){
    if(scaleSel.value==='custom'){wInput.value=state.canvasW;hInput.value=state.canvasH;}
    updateScaleVisibility();
  });
  // Keep W/H locked to the canvas' own aspect ratio — this is a uniform
  // render-resolution multiplier, not a crop/stretch/reflow control.
  wInput.addEventListener('input',function(){hInput.value=Math.round(parseFloat(wInput.value||state.canvasW)*state.canvasH/state.canvasW);});
  hInput.addEventListener('input',function(){wInput.value=Math.round(parseFloat(hInput.value||state.canvasH)*state.canvasW/state.canvasH);});
  function currentExportScale(){
    if(scaleSel.value==='custom'){
      var w=parseFloat(wInput.value)||state.canvasW;
      return w/state.canvasW;
    }
    return parseFloat(scaleSel.value);
  }

  document.getElementById('btn-export').addEventListener('click',function(){
    if(!window.SMExport.isAvailable()){
      showToast('Export complet disponible uniquement dans l\'app Nemo (Tauri)');
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
    var scale=currentExportScale();
    var alpha=ALPHA_FORMATS.indexOf(fmtSel.value)>=0&&document.getElementById('exp-alpha').checked;
    var opts={start:range.start,end:range.end,scale:scale,alpha:alpha,fps:state.fps,
      onProgress:function(i,n){progEl.style.display='block';progEl.textContent='Rendu image '+i+'/'+n+'…';},
      onFfmpeg:function(line){progEl.style.display='block';progEl.textContent=line.substring(0,80);},
      onRiveProgress:function(msg){progEl.style.display='block';progEl.textContent=msg;}};
    runBtn.disabled=true;progEl.style.display='block';progEl.textContent='Préparation…';
    try{
      var fn={svg:'exportSVGSequence',png:'exportPNGSequence',tiff:'exportTIFFSequence',gif:'exportGIF',mp4:'exportMP4',prores:'exportProRes',lottie:'exportLottie',rive:'exportRive','ae-camera':'exportAECamera'}[fmtSel.value];
      var res=await window.SMExport[fn](opts);
      if(res.cancelled){progEl.textContent='Annulé';}
      else if(res.ok){
        progEl.textContent='Terminé ✓';
        var aeCamMsg='Script caméra exporté ('+res.keyCount+' clé(s))'+
          (res.kitsu?(res.kitsu.ok?' — envoyé sur Kitsu':' — échec envoi Kitsu: '+res.kitsu.error):'');
        var doneMsg=fmtSel.value==='rive'?('Exporté vers l\'artboard Rive "'+res.artboardName+'"')
          :fmtSel.value==='ae-camera'?aeCamMsg
          :'Export terminé';
        showToast(doneMsg);
        // Lottie JSON gets its own preview instead of auto-closing straight
        // away — a bad export (empty/misplaced shapes) is otherwise silent
        // until opened in some other player.
        if(fmtSel.value==='lottie'&&res.json&&window.SMLottiePreview){
          modal.style.display='none';
          window.SMLottiePreview.open(res.json);
        }else{
          setTimeout(function(){modal.style.display='none';},900);
        }
      }
      else{progEl.textContent='Erreur: '+(res.error||'inconnue');}
    }catch(err){
      progEl.textContent='Erreur: '+(err&&err.message?err.message:err);
    }finally{
      runBtn.disabled=false;
    }
  });
})();

setInterval(function(){
  if(state.playing)return;
  saveAllLayerFrames();
  var json=window.SM.exportJSON();
  try{localStorage.setItem('nemo-auto',json);}catch(e){}
  // v15: dense on-disk version history (Tauri only) alongside the single-
  // slot localStorage fallback above — see project.js pushVersionSnapshot.
  if(window.SMProject&&window.SMProject.pushVersionSnapshot)window.SMProject.pushVersionSnapshot(json);
},30000);
// Restored quietly into memory so it's ready the instant the start screen's
// "Resume Last Session" card is clicked — the toast there was confusing
// alongside the new start screen (state.js decides whether to actually
// show that card, and surfaces its own confirmation once chosen).
try{var saved=localStorage.getItem('nemo-auto');if(saved)window.SM.importJSON(saved,true);}catch(e){}
window.SM.setTool(state.userProfile&&state.userProfile.role==='producer'?'hand':'draw');updateUI();renderSymbolTabs();
