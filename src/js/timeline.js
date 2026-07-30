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
var playRaf=null;
// One logical frame step, preserving the exact edge semantics the old
// setInterval body had (loop, ping-pong direction flip, audio onLoop,
// stop at the work-area edge). Returns the next frame, or null meaning
// "playback ends here". Mutates state.playDir like before.
// Ping-pong (right-click btn-loop, feedback: "quand on clic sur le
// lecture loop... il faut switché aussi sur une lecture en pingpong")
// bounces back and forth across the work area instead of hard-cutting
// back to waIn every pass — direction only flips at the OUT-of-bounds edge.
function advancePlayFrame(cur){
  var next=cur+state.playDir;
  if(next>state.waOut){
    if(state.loopPlayback&&state.pingPongPlayback){state.playDir=-1;next=cur-1;if(next<state.waIn)next=state.waIn;}
    else if(state.loopPlayback){next=state.waIn;if(window.SMAudio)SMAudio.onLoop(next);}
    else return null;
  }else if(next<state.waIn){
    if(state.loopPlayback&&state.pingPongPlayback){state.playDir=1;next=cur+1;if(next>state.waOut)next=state.waOut;}
    else return null;
  }
  return next;
}
function startPlay(){if(state.playing)return;
  // A brush-menu hover-preview mutates live paths with no pushUndo/save (see
  // brush-menu-bridge.js) — if playback starts while one is active, the next
  // frame change's saveAllLayerFrames() would bake the uncommitted preview
  // into persisted frame data. Close the popover (its close() already
  // reverts any live preview) before playback can advance a frame.
  if(window.BrushMenu&&window.BrushMenu.isOpen())window.BrushMenu.close();
  state.playing=true;state.playDir=1;
  document.getElementById('btn-play').innerHTML='<span class="material-symbols-rounded">\u{e034}</span>';
  document.getElementById('btn-play').classList.add('playing');
  if(window.SMAudio)SMAudio.onPlayStart(state.currentFrame);
  // Wall-clock-driven playback (2026-07 — "à la lecture des accélérations,
  // décélérations"): the old setInterval(1000/fps) advanced exactly ONE
  // frame per tick no matter how late the tick fired. Any main-thread work
  // (engine render, video frame upload, Paper rebuild in loadFrame) delays
  // ticks, then the browser fires them in a catch-up burst — so playback
  // visibly slowed down and sped up in waves, an accordion instead of a
  // steady rate. Standard player fix: a rAF loop with a time accumulator —
  // each animation frame computes how many SOURCE frames have elapsed on
  // the wall clock and advances that many logical steps at once (dropping
  // visual frames when behind, exactly like AE/video players), so the
  // PACE stays true to state.fps even when individual frames are heavy.
  var frameMs=1000/state.fps;
  var playClock=performance.now();
  // Advance the live document + screen to `next` — checks the playback bake
  // cache (playback-cache.js) first: a cached frame just blits a bitmap
  // (cheap 2D drawImage), skipping loadFrame/buildSceneJson/engine.render()
  // entirely for that tick. Falls through to the untouched original path
  // (loadFrame + rust-canvas) on a cache miss, or when the cache module
  // isn't loaded at all — a strict no-op for anyone who never triggers a
  // bake.
  function _advanceToFrame(next){
    saveAllLayerFrames();state.currentFrame=next;window._curFrame=next;
    if(window.SMPlaybackCache&&SMPlaybackCache.blitFrame(next)){
      // cache hit — document intentionally NOT reloaded (see performance
      // monitor below: only a LIVE render should feed the "is real-time
      // failing" signal, and this tick clearly isn't one)
    }else{
      if(window.SMPlaybackCache)SMPlaybackCache.hideBakePreview();
      loadFrame(next);
    }
    updatePlayhead();
  }
  // Real-time-playback monitor (2026-07 — "le preview cache arrivera que si
  // l'app n'arrive pas à lire en temps réel", explicit user direction): the
  // bake cache is a FALLBACK for genuine underperformance, not a step every
  // project pays automatically. Same "rAF interval is the honest number"
  // methodology as CLAUDE.md §5bis — a rolling median over many ticks, not
  // a single sample, so one GC pause or a window-refocus blip can't trigger
  // a bake; only a SUSTAINED shortfall does. Only fed by ticks that did a
  // LIVE (uncached) render — a cache hit is fast by construction and would
  // otherwise constantly reset the signal to "fine" right as playback
  // enters the very range that still needs baking.
  var _perfSamples=[],_perfWindow=30,_lastTickAt=performance.now();
  function _livePerfSample(now){
    var dt=now-_lastTickAt;_lastTickAt=now;
    _perfSamples.push(dt);
    if(_perfSamples.length>_perfWindow)_perfSamples.shift();
    if(_perfSamples.length<_perfWindow)return false;
    var sorted=_perfSamples.slice().sort(function(a,b){return a-b;});
    return sorted[Math.floor(sorted.length/2)]>frameMs*1.5;
  }
  function playStep(now){
    if(!state.playing)return;
    var steps=Math.floor((now-playClock)/frameMs);
    if(steps>0){
      // Cap the catch-up burst (window unfocused, huge stall): jumping
      // hundreds of logical frames in one go would spin through loop
      // wraps invisibly; two seconds' worth is plenty, then re-anchor.
      if(steps>state.fps*2){steps=1;playClock=now;}
      else playClock+=steps*frameMs;
      var next=state.currentFrame;
      for(var k=0;k<steps;k++){
        var n2=advancePlayFrame(next);
        if(n2===null){
          // land exactly on the edge frame before stopping, like before
          if(next!==state.currentFrame)_advanceToFrame(next);
          stopPlay();return;
        }
        next=n2;
      }
      var wasCached=window.SMPlaybackCache&&SMPlaybackCache.hasFrame(next);
      if(next!==state.currentFrame)_advanceToFrame(next);
      if(!wasCached&&window.SMPlaybackCache&&!SMPlaybackCache.isBaking()&&_livePerfSample(now)){
        autoBakeThenResume();
        return;
      }
    }
    playRaf=requestAnimationFrame(playStep);
  }
  playRaf=requestAnimationFrame(playStep);
}
// Auto-triggered by playStep's performance monitor when live playback can't
// keep up with state.fps — stops playback, bakes the current work-area
// range (playback-cache.js), then resumes. Also callable directly from the
// manual "cache de lecture" fallback button (#btn-bake-cache).
function autoBakeThenResume(){
  if(!window.SMPlaybackCache||SMPlaybackCache.isBaking())return;
  var savedDir=state.playDir;
  var from=state.waIn,to=state.waOut;
  if(state.playing)stopPlay();
  if(window.showToast)showToast('Optimisation de la lecture…','info');
  SMPlaybackCache.bakeRange(from,to).then(function(res){
    if(res&&res.started&&window.showToast){
      if(res.budgetHit)showToast('Cache de lecture : '+res.cached+'/'+res.total+' images (limite mémoire atteinte)','warn');
      else if(res.cancelled)showToast('Cache de lecture annulé ('+res.cached+' images)','warn');
      else showToast('Cache de lecture prêt ('+res.cached+' images)','success');
    }
    state.playDir=savedDir;
    startPlay();
  });
}
// Manual fallback (#btn-bake-cache, user-approved safety valve alongside
// the automatic trigger above) — bakes on demand (e.g. before a demo) but
// does NOT auto-start playback afterward, unlike autoBakeThenResume: the
// user asked for the cache to be ready, not for playback to begin.
function manualBakeCache(){
  if(!window.SMPlaybackCache){return;}
  if(SMPlaybackCache.isBaking()){showToast('Cache de lecture déjà en cours…','info');return;}
  if(state.playing)stopPlay();
  showToast('Mise en cache de la lecture…','info');
  SMPlaybackCache.bakeRange(state.waIn,state.waOut).then(function(res){
    if(!res)return;
    if(!res.started){showToast('Cache de lecture indisponible','warn');return;}
    if(res.budgetHit)showToast('Cache de lecture : '+res.cached+'/'+res.total+' images (limite mémoire atteinte)','warn');
    else if(res.cancelled)showToast('Cache de lecture annulé ('+res.cached+' images)','warn');
    else showToast('Cache de lecture prêt ('+res.cached+' images)','success');
  });
}
function stopPlay(){if(!state.playing)return;state.playing=false;
  if(playRaf){cancelAnimationFrame(playRaf);playRaf=null;}
  clearInterval(playInt);playInt=null;
  if(window.SMAudio)SMAudio.onPlayStop();
  document.getElementById('btn-play').innerHTML='<span class="material-symbols-rounded">\u{e037}</span>';
  document.getElementById('btn-play').classList.remove('playing');
  renderOS();renderArcs();updateUI();
}
function togglePlay(){if(state.playing)stopPlay();else startPlay();}

// Keyframe diamonds (.motion-key/.km) land at frame*FC+FC/2 in Motion —
// CSS flex justify-content:center inside `.fc`, no explicit JS positioning
// — while the playhead line used frame*FC everywhere (left-edge convention,
// via border-left on a width:var(--fc) box). Invisible in Animation 2D
// (there the playhead's left edge IS the correct mark — no per-frame
// diamond to line up with), but in Motion the line visibly missed every
// diamond's center by half a cell ("le curseur devrait arriver aligné à la
// keyframe", 2026-07-28) — made obvious there specifically because Motion
// also paints a frame-duration background block (motion.js) spanning that
// same box, contrasting against the centered diamond. One shared helper so
// the three call sites (this function, and renderTimeline()'s Motion/
// Animation-2D branches) can't drift apart again (CLAUDE.md §3).
function playheadLeftPx(frame){return frame*FC+(state.appMode==='motion'?FC/2:0);}
function updatePlayhead(){
  if(window.SMCamera){SMCamera.applyCameraView();if(window.updateCameraPanel)updateCameraPanel();}
  var tlCfEl0=document.getElementById('tl-cf');
  if(document.activeElement!==tlCfEl0)tlCfEl0.value=state.currentFrame+1;
  document.getElementById('info-frame').textContent=state.currentFrame+1;
  document.getElementById('playhead').style.left=playheadLeftPx(state.currentFrame)+'px';
  document.getElementById('playhead-flag').textContent=state.currentFrame+1;
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

// ---- Tween-span retiming (2026-07-17, "pour des keyframes tween, les
// keyframe clé A et B, on doit pouvoir les retimer et cela retime les
// tween et ne laisse des clé ni derrière A, ni devant B") ----
// moveFrames/moveKeyframe only relocate the KEY cells — the generated
// in-between (isInterpolated) frames sat untouched at their old
// positions, so shortening a span left stale tween frames beyond the
// moved key (the reported orphans), and lengthening left a dead gap.
// captureTweenInbetweens snapshots each pair's in-betweens BEFORE any
// frame mutation; retimeTweenSpans then clears every stale in-between
// across each moved pair's old+new union span and re-lays the captured
// sequence onto the new span by nearest-normalized-position resampling —
// the drawings themselves are reused (no re-run of the tween engine, so
// hand-corrected inbetweens keep their content and isManualEdit flag),
// only their timing stretches/squashes.
function captureTweenInbetweens(li,kfs){
  var ld=state.layers[li],out={};if(!ld)return out;
  for(var i=0;i<kfs.length-1;i++){
    var fA=kfs[i],fB=kfs[i+1],list=[];
    for(var f=fA+1;f<fB;f++){
      var fr=ld.frames[f];
      if(fr&&fr.isInterpolated)list.push({frame:f,content:JSON.parse(JSON.stringify(fr))});
    }
    if(list.length)out[fA+':'+fB]=list;
  }
  return out;
}
function retimeTweenSpans(li,pairs,captured){
  var ld=state.layers[li];if(!ld)return;
  var todo=[];
  pairs.forEach(function(pr){
    if(pr.newFA===pr.fA&&pr.newFB===pr.fB)return;
    var list=captured[pr.fA+':'+pr.fB];if(!list||!list.length)return;
    if(pr.newFB-pr.newFA<1)return; // collapsed/reversed span — nothing to lay the sequence onto
    // another keyframe strictly inside the new span (the key was dragged
    // past a neighbor): stretching this pair's tweens through it would be
    // nonsense — leave that pair alone entirely.
    for(var f=pr.newFA+1;f<pr.newFB;f++)if(ld.frames[f]&&ld.frames[f].isKeyframe)return;
    todo.push({pr:pr,list:list});
  });
  // Phase 1 — clear ALL stale in-betweens first, across each pair's
  // old+new union, THEN write (phase 2): with a shared key between two
  // pairs, one pair's union overlaps the neighbor's new span, and a
  // clear running after that neighbor's write would blank frames it
  // just laid down.
  todo.forEach(function(item){
    var lo=Math.min(item.pr.fA,item.pr.newFA),hi=Math.max(item.pr.fB,item.pr.newFB);
    for(var f=Math.max(0,lo+1);f<Math.min(state.totalFrames,hi);f++){
      var fr=ld.frames[f];
      if(fr&&fr.isInterpolated)ld.frames[f]={strokes:[],isKeyframe:false,isInterpolated:false};
    }
  });
  // Phase 2 — each captured in-between lands at its PROPORTIONAL position
  // in the new span (one write per original frame, not one per new-span
  // slot: a sparse pair — say one lone in-between over a long hold —
  // must stay sparse, not densify into a copy on every frame; found live
  // on the first version of this function). Squashing collapses
  // colliding frames onto one slot (later wins); stretching leaves hold
  // gaps between them — same semantics as retiming drawings in Animate.
  todo.forEach(function(item){
    var pr=item.pr,list=item.list;
    list.forEach(function(ib){
      var ot=(ib.frame-pr.fA)/(pr.fB-pr.fA);
      var nf=Math.round(pr.newFA+ot*(pr.newFB-pr.newFA));
      if(nf<=pr.newFA)nf=pr.newFA+1;
      if(nf>=pr.newFB)nf=pr.newFB-1;
      if(nf<0||nf>=state.totalFrames)return;
      if(ld.frames[nf].isKeyframe)return; // never clobber a real key
      ld.frames[nf]=JSON.parse(JSON.stringify(ib.content));
    });
    // Stretching a FULLY-DENSE pair (every original slot was a tween) left
    // HOLES between the proportionally-relaid frames — N in-betweens laid
    // onto a longer span can't cover every slot, and phase 1 had already
    // blanked them. Found live (2026-07, user screenshots: "je bouge la
    // dernière clé entre les tween, des trucs bizarres apparaissent alors
    // que je devrais n'avoir que des tween") — the span alternated tween
    // dots and empty cells, flashing blank at playback. Fill each hole
    // with a copy of the NEAREST captured in-between (frame-hold "on 2s"
    // semantics — content-true, no blank flash; hitting Tween regenerates
    // a smooth interpolation over the new span whenever wanted). Only for
    // dense pairs: a deliberately sparse pair (one lone in-between over a
    // long hold) must stay sparse, per this function's original contract.
    // isManualEdit is stripped from the filled COPIES — the neighbor's
    // hand-edit protection shouldn't shield machine-made duplicates from a
    // future Tween regeneration.
    if(list.length===(pr.fB-pr.fA-1)){
      for(var nf2=pr.newFA+1;nf2<pr.newFB;nf2++){
        if(nf2<0||nf2>=state.totalFrames)continue;
        var fr2=ld.frames[nf2];
        if(fr2.isKeyframe||fr2.isInterpolated)continue;
        var t2=(nf2-pr.newFA)/(pr.newFB-pr.newFA);
        var best=null,bd=Infinity;
        list.forEach(function(ib){
          var ot2=(ib.frame-pr.fA)/(pr.fB-pr.fA);
          var d2=Math.abs(ot2-t2);
          if(d2<bd){bd=d2;best=ib;}
        });
        if(best){
          var cp=JSON.parse(JSON.stringify(best.content));
          delete cp.isManualEdit;
          ld.frames[nf2]=cp;
        }
      }
    }
  });
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
    if(t!=='select'&&t!=='subselect'&&t!=='rig')clearSel();if(t!=='fsselect')fsClearSel();if(t!=='pen'&&_pen.path)finalizePen();if(t!=='rig'&&typeof _rigDraw!=='undefined'&&_rigDraw.path&&window.SMRig)window.SMRig.finalizeRigBone();if(t!=='eraser'&&typeof _eraserCursor!=='undefined'&&_eraserCursor){_eraserCursor.remove();_eraserCursor=null;}if(t!=='select'&&window.SMSelectBridge)window.SMSelectBridge.cancelMarquee();
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
    var cc={draw:'crosshair',pen:'crosshair',line:'crosshair',rect:'crosshair',ellipse:'crosshair',select:'default',subselect:'default',fsselect:'default',comment:'crosshair',camera:'move',text:'text',eraser:'pointer',fill:'crosshair',fillbrush:'crosshair',eyedropper:'crosshair',hand:'grab',zoom:'zoom-in',rotate:'grab',perspective:'crosshair',symmetry:'crosshair',rig:'crosshair'};
    canvasEl.style.cursor=cc[t]||'default';
    updatePropsContext();
    if(window.renderLabsFloatPanel)renderLabsFloatPanel();},
  toggleOnion:function(){state.onionSkin=!state.onionSkin;renderOS();
    // Deux badges à tenir en phase : #os-st (popover d'options) et
    // #os-st-panel (la section Onion Skin du panel droit, mockup 2026-07-17).
    ['os-st','os-st-panel'].forEach(function(id){var el=document.getElementById(id);if(!el)return;el.textContent=state.onionSkin?'ON':'OFF';el.style.color=state.onionSkin?'var(--green)':'var(--text-dim)';});
    var b=document.getElementById('btn-os');if(b)b.classList.toggle('active',state.onionSkin);window.updateOmMarkers(state.currentFrame,state.totalFrames);},
  // Pure display toggle (complements the global/per-pair easing system,
  // never affects generateTweens' output) — mini curve strips under every
  // layer row that has at least one tween span, see renderTweenCurveStrips.
  toggleTweenCurves:function(){
    state.showTweenCurves=!state.showTweenCurves;
    var b=document.getElementById('btn-tween-curves');if(b)b.classList.toggle('active',state.showTweenCurves);
    // A row's HEIGHT (not just the strip drawn inside it) now depends on
    // this flag — that decision is made in renderTimeline()'s own row-
    // building loop (which calls renderTweenCurveStrips() itself at the
    // end), so a bare renderTweenCurveStrips() here left every row at its
    // stale height on toggle (confirmed empirically: strip drew at the new
    // 92px height but the .frow itself stayed 34px, clipping it). Mirror
    // the same height on the layer panel's .lrow (CLAUDE.md §11).
    renderTimeline();
    renderLayerList();
  },
  toggleGhostAll:function(){
    state.ghostAllFrames=!state.ghostAllFrames;
    renderOS();
    var b=document.getElementById('btn-ghost-all');if(b)b.classList.toggle('active',state.ghostAllFrames);
    // btn-ghost-select no longer gets disabled here (2026-07) — it now
    // self-enables Ghost All on demand (selectGhostAll's own comment), so
    // leaving it clickable either way is correct; disabling it made it
    // silently inert with no visual cue at all (no .tb:disabled CSS rule
    // existed), which is exactly what read as "ne marche pas".
    if(!state.ghostAllFrames)clearGhostSelection();
    updateUI();
  },
  // Shadow Brush guide-line visibility (2026-07, shadow-brush-bridge.js) —
  // a single view-only switch (like onionSkin, never persisted to the
  // project file) that hides every data.channelTag==='shadow' item across
  // ALL layers/components/the current frame at once, so guide lines drawn
  // for the Stroke/Fill/Shadow layer-separation workflow can be checked-off
  // before a final look without touching the document itself. Consumed at
  // render time by engine-bridge.js's buildSceneJson (main per-frame scene)
  // and onionLayerItems (onion skin + Ghost-All, which share that reader).
  toggleShadowGuides:function(){
    state.showShadowGuides=!state.showShadowGuides;
    var b=document.getElementById('btn-shadow-guides');if(b)b.classList.toggle('active',state.showShadowGuides);
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
  },
  selectGhostAll:selectGhostAll,
  toggleLoopPlayback:function(){state.loopPlayback=!state.loopPlayback;var b=document.getElementById('btn-loop');if(b)b.classList.toggle('active',state.loopPlayback);},
  // Right-click btn-loop (feedback: "quand on clic sur le lecture loop...
  // il faut switché aussi sur une lecture en pingpong") — a secondary
  // toggle rather than folding into the left-click, so anyone who just
  // wants a plain wrap-to-start loop keeps that as the default single
  // click. Ping-pong only has an effect while loop itself is on (see
  // startPlay's interval) — implied on when picked, doesn't force loop off.
  togglePingPongPlayback:function(){state.pingPongPlayback=!state.pingPongPlayback;if(state.pingPongPlayback)state.loopPlayback=true;var b=document.getElementById('btn-loop');if(b){b.classList.toggle('active',state.loopPlayback);b.classList.toggle('pingpong',state.pingPongPlayback);b.title=SM.t(state.pingPongPlayback?'loopPingPongTitle':'loopWorkAreaTitle');}showToast(SM.t(state.pingPongPlayback?'toastPingPongOn':'toastPingPongOff'));},
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
        }else{
          p.strokeWidth=v;applyStrokeStyle(p);
          // Width unification (2026-07-17, "il ne change pas la taille du
          // trait brush bitmap") : p.strokeWidth above is a NO-OP on a
          // Bitmap Brush anchor — its visible "stroke" is the dab texture,
          // sized by bitmapBrushSpec.size, not by the (camouflaged, null-
          // color) vector stroke. Update the spec and re-bake so Width
          // actually resizes the dabs, same as it now does at draw time.
          if(p.data&&p.data.bitmapBrushSpec&&window.SMBitmapBrush){
            p.data.bitmapBrushSpec.size=v;
            SMBitmapBrush.regenerate(p,userLayers[state.activeLayerIdx]);
          }
        }
      });
      saveActiveLayerFrame();updateUI();
    }},
  setStrokeColor:function(v){state.strokeColor=v;paintStrokeSwatches(v);
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(p.data&&p.data.isVectorBrush)p.fillColor=v;else if(p.strokeColor)p.strokeColor=v;});saveActiveLayerFrame();updateUI();}
    // Fill/Stroke Select tool: recolor ONLY the clicked aspect — a 'stroke'
    // selection here means strokeColor, never touches fillColor even on a
    // combined shape (that's the whole point of this tool vs plain Select).
    else if(state.tool==='fsselect'&&_fsSel.some(function(s){return s.kind==='stroke';})){pushUndo();_fsSel=_fsSel.map(function(sel){if(sel.kind!=='stroke')return sel;var arc=fsRealizeStrokeSegment(sel,userLayers[state.activeLayerIdx]);arc.strokeColor=v;return{path:arc,kind:'stroke',segStart:0,segEnd:arc.length,closed:arc.closed};});saveActiveLayerFrame();updateUI();}},
  setFillColor:function(v){state.fillColor=v;paintFillSwatches(v);
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(p.fillColor)p.fillColor=v;});saveActiveLayerFrame();updateUI();}
    else if(state.tool==='fsselect'&&_fsSel.some(function(s){return s.kind==='fill'||s.kind==='fillregion';})){pushUndo();_fsSel=_fsSel.map(function(sel){if(sel.kind!=='fill'&&sel.kind!=='fillregion')return sel;if(sel.kind==='fillregion')sel=fsRealizeFillRegion(sel,userLayers[state.activeLayerIdx]);sel.path.fillColor=v;return sel;});saveActiveLayerFrame();updateUI();}},
  // Fill/Stroke on-off is mirrored by TWO eye icons — the right-panel one
  // (#fill-enable-toggle / #stroke-enable-toggle) and the left tool-panel one
  // (…-lp). Every place that changes the state must refresh BOTH, plus the
  // open/closed glyph, or the two disagree.
  //
  // Found 2026-07-27: selecting a shape with no stroke ADOPTS its state
  // (state.strokeEnabled=false, see the adopt-from-selection block further
  // down) and only ever greyed the right-panel icon, so the left one kept
  // showing an OPEN eye while the stroke was genuinely off — you then draw a
  // black stroke, nothing appears, and the only visible switch says it is
  // enabled. Same shape as CLAUDE.md §1: one piece of state, several readers,
  // only one updated.
  _syncFillEnabledUI:function(v){
    var fw=document.getElementById('fill-well'),pf=document.getElementById('pm-fill');
    if(fw)fw.classList.toggle('none',!v);
    if(pf)pf.classList.toggle('none',!v);
    var cb=document.getElementById('p-fill-on');if(cb)cb.checked=v;
    ['fill-enable-toggle','fill-enable-toggle-lp'].forEach(function(id){
      var el=document.getElementById(id);
      if(el){el.classList.toggle('off',!v);el.innerHTML=v?ICO_EYE:ICO_EYE_CLOSED;}
    });
  },
  _syncStrokeEnabledUI:function(v){
    ['stroke-enable-toggle','stroke-enable-toggle-lp'].forEach(function(id){
      var el=document.getElementById(id);
      if(el){el.classList.toggle('off',!v);el.innerHTML=v?ICO_EYE:ICO_EYE_CLOSED;}
    });
  },
  setFillEnabled:function(v){state.fillEnabled=v;window.SM._syncFillEnabledUI(v);
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(p.data&&p.data.isVectorBrush)return;p.fillColor=v?state.fillColor:null;});saveActiveLayerFrame();updateUI();}
    else if(state.tool==='fsselect'&&_fsSel.some(function(s){return s.kind==='fill'||s.kind==='fillregion';})){pushUndo();_fsSel=_fsSel.map(function(sel){if(sel.kind!=='fill'&&sel.kind!=='fillregion')return sel;if(sel.kind==='fillregion')sel=fsRealizeFillRegion(sel,userLayers[state.activeLayerIdx]);sel.path.fillColor=v?state.fillColor:null;if(!v){fsUnlinkFillRegen(sel.path);if(!sel.path.strokeColor){sel.path.remove();return null;}}return sel;}).filter(Boolean);saveActiveLayerFrame();updateUI();}},
  // Mirrors setFillEnabled exactly, for the Stroke side — didn't exist
  // before (Stroke had no on/off concept, only a color), added alongside
  // the quick phdr toggle button since disabling stroke without it required
  // opening the color popover and hunting for "None".
  setStrokeEnabled:function(v){state.strokeEnabled=v;window.SM._syncStrokeEnabledUI(v);
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){pushUndo();selectedPaths.forEach(function(p){if(p.data&&p.data.isVectorBrush)return;p.strokeColor=v?state.strokeColor:null;});saveActiveLayerFrame();updateUI();}
    // fsselect's 'stroke' selection can be just a bounded segment (between
    // two crossings), which has no standalone color to null — disabling it
    // means the same real split-and-remove fsApplyDelete's Delete key does.
    // No re-enable path: once a segment is split out there's nothing left
    // to flip back on, and the stroke-sec panel disappears with _fsSel.
    else if(state.tool==='fsselect'&&_fsSel.some(function(s){return s.kind==='stroke';})&&!v){pushUndo();_fsSel=_fsSel.filter(function(sel){if(sel.kind!=='stroke')return true;fsDeleteSegment(sel,userLayers[state.activeLayerIdx]);return false;});renderArcs();updateUI();}},
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
  setBrushPreset:function(v){state.brushPreset=v||'none';
    // Every other Trait field (Width/Color/Cap/Join/Style/Dash…) auto-
    // applies to the current selection the moment it changes — the vector
    // Brush preset (dynamic dab texture, "brush dynamique") was the one
    // exception, silently doing nothing until the separate "Apply to
    // selection" button got clicked (2026-07-17, "les changement de brush
    // dynamique ne s'applique pas au stroke de la selection"). Reuses the
    // exact conversion logic that button already had (strip whatever
    // texture — vector or bitmap — then re-apply the new preset, so
    // switching presets or converting a Bitmap Brush stroke to a vector
    // one both just work), just triggered on every preset pick instead of
    // requiring a second click.
    if((state.tool==='select'||state.tool==='subselect')&&selectedPaths.length){
      var eligible=selectedPaths.filter(function(p){return p instanceof Path&&!(p.data&&(p.data.isVectorBrush||p.data.isFillShape))&&(p.strokeColor||(p.data&&(p.data.brushTexturePreset||p.data.bitmapBrushSpec)));});
      if(eligible.length){
        pushUndo();
        eligible.forEach(function(p){
          stripAnyBrushTexture(p);
          if(state.brushPreset&&state.brushPreset!=='none')applyBrushTexture(p,state.brushPreset);
        });
        saveActiveLayerFrame();updateUI();
      }
    }
  },
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
    // No Apply button anymore (2026-07: "plus besoin du bouton apply pour
    // smooth, il le fait directement quand on change de valeur") — wired to
    // the field's own 'change' event instead, which the scrub mechanism
    // (ui.js) ALSO dispatches repeatedly WHILE dragging (coalesced per
    // frame, not just at release) — its OWN pushUndo() already fires once
    // at the start of that drag (ui.js's pointermove handler), so calling
    // pushUndo() again here on every one of those live ticks would spam
    // the undo stack with one throwaway entry per frame instead of a
    // single "before Smooth" snapshot. Skip ONLY this call's own snapshot
    // while a scrub is live; a plain typed value + Enter/blur (no scrub
    // involved, flag never set) still gets its own real undo step here.
    if(!window._scrubLiveActive)pushUndo();
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
      // Re-stamp any texture (vector-preset dabs OR Bitmap Brush's raster)
      // to follow the just-reshaped geometry — found live (2026-07,
      // "unifie bien les paramètre de smooth aussi présent pour texture
      // bitmap"): this function reshaped the anchor but NEVER called
      // regenerateBrushTexture, so Smooth left a textured stroke's
      // companion stuck at its pre-Smooth shape — a bitmap texture then
      // visibly stopped correlating with the (now re-simplified) path,
      // exactly the reported "la texture ne suit plus le tracé" mismatch.
      // Same shared dispatcher (tools.js) used by subselect's node-drag
      // end and the transform box's onUp, not a hand-rolled bitmap-only
      // check — the vector-preset case had the identical gap.
      regenerateBrushTexture(p,userLayers[state.activeLayerIdx]);
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
  // Picking a formula preset always reverts to it — a custom curve (see
  // ui.js's editPressureCurve, "Éditer la courbe" button) is a deliberate
  // opt-in the user has to open and drag a point in; switching presets
  // afterward should feel like a clean reset, not a value the dropdown
  // silently ignores from then on.
  setPressureCurve:function(v){state.pressureCurve=['linear','sqrt','cbrt','pow2','pow3'].indexOf(v)>=0?v:'linear';state.pressureCurvePoints=null;},
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
  // Null layer (2026-07, Motion) — AE's "Null Object": exists purely as a
  // parenting/pivot target for other layers (SMMotion's existing
  // parentLayerUid/parentChainMats — a Null is just any other layer as far
  // as that mechanism cares), never rendered itself. isNullLayer is the
  // ONLY thing distinguishing it from a normal empty layer; engine-bridge.js's
  // buildSceneJson short-circuits it to items:[] before any of the normal
  // per-layer work, and it's added to the SAME "no real content" guard list
  // as symbolId/nativeVideo/montageId in saveActiveLayerFrame/
  // saveAllLayerFrames/getEffectiveStrokes (app.js) so nothing ever tries
  // to read/write strokes on it.
  addNullLayer:function(){saveAllLayerFrames();pushUndoLayers();var idx=createUserLayer(nextLayerName().replace(/^Layer/,'Null'));state.layers[idx].isNullLayer=true;activateUL(idx);loadFrame(state.currentFrame);updateUI();},
  // Effect (adjustment) layer (2026-07, Motion) — AE's "Adjustment Layer":
  // no painted content of its own (frames/strokes ignored on purpose,
  // same as a Null layer), but its effectType/effectP1/effectP2 apply to
  // EVERYTHING BELOW IT in the stack via engine.rs's composite_scene
  // (color_adjust.wgsl / reused blur.wgsl) — see that function's
  // is_effect_layer branch and engine-bridge.js's buildSceneJson for the
  // full JS<->Rust wire contract. Defaults to a mild blur so placing one
  // has an immediately visible (if subtle) effect rather than looking
  // like a no-op.
  addEffectLayer:function(){saveAllLayerFrames();pushUndoLayers();var idx=createUserLayer(nextLayerName().replace(/^Layer/,'Effet'));state.layers[idx].isEffectLayer=true;state.layers[idx].effects=[];activateUL(idx);loadFrame(state.currentFrame);updateUI();},
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
    // Refusing to delete the last layer is right — a document with no layer
    // has nowhere to draw — but it was silent, so the trash button just
    // appeared broken (2026-07-25 UX audit). Every other refusal in this file
    // says why; this one now does too.
    if(state.layers.length<=1){showToast('Impossible de supprimer le dernier calque');return;}
    saveAllLayerFrames();
    pushUndoLayers();
    var sel=(_layerSel.length?_layerSel.slice():[state.activeLayerIdx]).sort(function(a,b){return b-a;});
    sel.forEach(function(idx){
      if(state.layers.length<=1||idx<0||idx>=state.layers.length)return;
      userLayers[idx].remove();userLayers.splice(idx,1);state.layers.splice(idx,1);
      // Motion mode (motion.js) keeps the "expanded Transform group" as a raw
      // layer index in this global — deleting a lower-indexed layer would
      // otherwise leave it pointing at the wrong layer (or the layer that
      // slid into the deleted slot) without ever crashing, silently editing
      // someone else's keyframes.
      if(typeof window._motionExpandedLayer==='number'){
        if(window._motionExpandedLayer===idx)window._motionExpandedLayer=null;
        else if(window._motionExpandedLayer>idx)window._motionExpandedLayer--;
      }
    });
    _layerSel=[];
    if(state.activeLayerIdx>=state.layers.length)state.activeLayerIdx=state.layers.length-1;
    activateUL(state.activeLayerIdx);loadFrame(state.currentFrame);updateUI();showToast('Calque(s) supprimé(s) — ⌘Z pour annuler');
  },
  // Standing "keyframes follow this edge" lock (Van Dijk 2.2). Stored per
  // layer so it survives the session and needs no modifier at drag time.
  // "Trim Comp to Work Area" (Van Dijk 1.3). Drops everything outside the
  // work area and re-bases frame 0 on its start — layers, keyframes, markers
  // and the camera all shift together, or the trim would silently desync the
  // very things that were timed against it.
  trimToWorkArea:function(){
    var inF=state.waIn||0,outF=(state.waOut!=null?state.waOut:state.totalFrames-1);
    if(outF<=inF){showToast('Zone de travail trop courte');return;}
    if(inF===0&&outF===state.totalFrames-1){showToast('La zone de travail couvre déjà tout');return;}
    saveAllLayerFrames();pushUndoLayers();
    var n=outF-inF+1;
    state.layers.forEach(function(ld,li){
      ld.frames=ld.frames.slice(inF,outF+1);
      while(ld.frames.length<n)ld.frames.push({strokes:[],isKeyframe:false,isInterpolated:false});
      if(ld.inPoint!=null)ld.inPoint=Math.max(0,ld.inPoint-inF);
      if(ld.outPoint!=null)ld.outPoint=Math.max(0,Math.min(n-1,ld.outPoint-inF));
      if(ld.markers)ld.markers=ld.markers.map(function(m){return{frame:m.frame-inF,name:m.name,color:m.color};}).filter(function(m){return m.frame>=0&&m.frame<n;});
      if(window.SMMotion&&SMMotion.shiftLayerMotionKeys)SMMotion.shiftLayerMotionKeys(li,-inF);
    });
    if(state.markers)state.markers=state.markers.map(function(m){return{frame:m.frame-inF,name:m.name,color:m.color};}).filter(function(m){return m.frame>=0&&m.frame<n;});
    if(state.cameraKeys)state.cameraKeys=state.cameraKeys.map(function(k){var c2=JSON.parse(JSON.stringify(k));c2.frame=k.frame-inF;return c2;}).filter(function(k){return k.frame>=0&&k.frame<n;});
    state.totalFrames=n;window._totalF=n;
    state.waIn=0;state.waOut=n-1;window._waIn=0;window._waOut=n-1;
    state.currentFrame=Math.max(0,Math.min(n-1,state.currentFrame-inF));
    loadFrame(state.currentFrame);renderLayerList();renderTimeline();updateUI();
    if(window.updateWaBar)updateWaBar();
    showToast('Composition réduite à la zone de travail ('+n+' frames)');
  },
  setLayerKeyLock:function(li,mode){
    var ld=state.layers[li==null?state.activeLayerIdx:li];if(!ld)return;
    pushUndo();
    if(mode)ld.keyLock=mode;else delete ld.keyLock;
    showToast(mode?('Keyframes verrouillées sur '+(mode==='in'?'le point d\u2019entrée':mode==='out'?'le point de sortie':'le calque')):'Verrou de keyframes retiré');
    renderLayerList();renderTimeline();
  },
  toggleLayerMotionBlur:function(li){
    var ld=state.layers[li==null?state.activeLayerIdx:li];if(!ld)return;
    pushUndo();ld.motionBlur=!ld.motionBlur;
    // buildSceneJson's mbOn gate (engine-bridge.js) is
    // state.motionBlurOn && ld.motionBlur — the toast below has always
    // claimed enabling a layer's own flag turns the comp switch on too,
    // but nothing ever actually flipped state.motionBlurOn: the layer
    // flag flipped, the toast said the comp switch was on too, and
    // #btn-mblur stayed visually off — motion blur silently never
    // rendered until the user found and clicked that separate button.
    // Confirmed live (2026-07). Made the toast true instead of walking it
    // back, matching REAL_FEATURES.symmetry/perspective's own "flip the
    // companion switch + resync its button" precedent (labs-float-panel.js).
    var needsCompOn=ld.motionBlur&&!state.motionBlurOn;
    if(needsCompOn){
      state.motionBlurOn=true;
      var mbBtn=document.getElementById('btn-mblur');if(mbBtn)mbBtn.classList.add('active');
    }
    if(needsCompOn)showToast('Flou de mouvement activé sur le calque — active aussi l\u2019interrupteur de la comp');
    else showToast(ld.motionBlur?'Flou de mouvement activé':'Flou de mouvement désactivé');
    renderLayerList();renderTimeline();
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
  },
  toggleMotionBlurComp:function(){
    state.motionBlurOn=!state.motionBlurOn;
    var n=state.layers.filter(function(l){return l.motionBlur;}).length;
    showToast(state.motionBlurOn?('Flou de mouvement activé sur la comp ('+n+' calque(s))'):'Flou de mouvement désactivé sur la comp');
    var b=document.getElementById('btn-mblur');if(b)b.classList.toggle('active',!!state.motionBlurOn);
    renderLayerList();renderTimeline();
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
  },
  setMotionBlurSettings:function(samples,shutter){
    if(samples!=null)state.motionBlurSamples=Math.max(2,Math.min(16,parseInt(samples,10)||6));
    if(shutter!=null)state.motionBlurShutter=Math.max(0.05,Math.min(2,parseFloat(shutter)||0.5));
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
  },
  toggleLayerShy:function(li){
    var ld=state.layers[li==null?state.activeLayerIdx:li];if(!ld)return;
    pushUndo();ld.shy=!ld.shy;
    // Same over-promising-toast bug as toggleLayerMotionBlur just above
    // (2026-07 fix, same session): claimed marking a layer shy "active
    // l'interrupteur pour le masquer" but never actually flipped
    // state.shyEnabled — #btn-shy stayed off and shy layers stayed
    // visible until the user separately found and clicked that button.
    var needsShyOn=ld.shy&&!state.shyEnabled;
    if(needsShyOn){
      state.shyEnabled=true;
      var shyBtn=document.getElementById('btn-shy');if(shyBtn)shyBtn.classList.add('active');
    }
    if(needsShyOn)showToast('Calque marqué « shy » — active l\u2019interrupteur pour le masquer');
    renderLayerList();renderTimeline();
  },
  toggleShyMode:function(){
    state.shyEnabled=!state.shyEnabled;
    var n=state.layers.filter(function(l){return l.shy;}).length;
    showToast(state.shyEnabled?('Calques shy masqués ('+n+')'):'Tous les calques affichés');
    renderLayerList();renderTimeline();
  },
  // AE's Cmd+Shift+D: cut the layer in TWO at the playhead. Both halves keep
  // the whole content and all their keyframes — it is the in/out window that
  // splits, which is exactly how AE does it (and why the two halves can be
  // retimed independently afterwards without anything being lost).
  splitLayerAtPlayhead:function(li){
    li=(li==null?state.activeLayerIdx:li);
    var ld=state.layers[li];if(!ld){showToast('Aucun calque');return;}
    var f=state.currentFrame;
    var inF=window.layerInPoint?layerInPoint(ld):(ld.inPoint!=null?ld.inPoint:0);
    var outF=window.layerOutPoint?layerOutPoint(ld):(ld.outPoint!=null?ld.outPoint:state.totalFrames-1);
    if(f<=inF||f>outF){showToast('Place la tête de lecture à l\u2019intérieur du calque pour le couper');return;}
    saveAllLayerFrames();pushUndoLayers();
    var ni=createUserLayer(ld.name+' (2)');
    var dst=state.layers[ni];
    dst.frames=JSON.parse(JSON.stringify(ld.frames));
    dst.color=ld.color;
    if(ld.blendMode)dst.blendMode=ld.blendMode;
    if(ld.motion)dst.motion=JSON.parse(JSON.stringify(ld.motion));
    if(ld.motionStatic)dst.motionStatic=JSON.parse(JSON.stringify(ld.motionStatic));
    if(ld.elementMotion)dst.elementMotion=JSON.parse(JSON.stringify(ld.elementMotion));
    if(ld.effects)dst.effects=JSON.parse(JSON.stringify(ld.effects));
    if(ld.markers)dst.markers=JSON.parse(JSON.stringify(ld.markers));
    if(ld.symbolId){dst.symbolId=ld.symbolId;dst.symPlayMode=ld.symPlayMode;dst.symSpeed=ld.symSpeed;dst.symPlacedAt=ld.symPlacedAt;dst.symSingleFrame=ld.symSingleFrame;dst.symMatrix=ld.symMatrix;dst.locked=ld.locked;}
    dst.inPoint=f;dst.outPoint=outF;
    ld.outPoint=f-1;
    // Splitting materialises hard in/out values on both halves, which a
    // time link would then override — so the link is dropped rather than
    // left to silently win over the cut the user just made.
    if(ld.timeLink){delete ld.timeLink;delete dst.timeLink;showToast('Lien temporel retiré : la coupe fixe les points d\u2019entrée/sortie');}
    else delete dst.timeLink;
    // createUserLayer appends to the TOP of the stack, which would drop the
    // second half far from the one it was cut out of. AE leaves the two
    // halves adjacent, and so does this: move it to sit directly above its
    // source. Both arrays are spliced together — userLayers and state.layers
    // are index-parallel everywhere in this file.
    if(ni!==li+1){
      var movedL=state.layers.splice(ni,1)[0];
      var movedU=userLayers.splice(ni,1)[0];
      var at=Math.min(li+1,state.layers.length);
      state.layers.splice(at,0,movedL);
      userLayers.splice(at,0,movedU);
      userLayers.forEach(function(l){l.insertBelow(arcLayer);});
      ni=at;
    }
    activateUL(ni);loadFrame(state.currentFrame);renderLayerList();renderTimeline();updateUI();
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
    showToast('Calque coupé à la frame '+(f+1));
  },
  duplicateLayer:function(){saveAllLayerFrames();pushUndoLayers();var src=state.layers[state.activeLayerIdx];var ni=createUserLayer(src.name+' copy');state.layers[ni].frames=JSON.parse(JSON.stringify(src.frames));if(src.blendMode)state.layers[ni].blendMode=src.blendMode;state.layers[ni].color=src.color;if(src.motion)state.layers[ni].motion=JSON.parse(JSON.stringify(src.motion));if(src.motionStatic)state.layers[ni].motionStatic=JSON.parse(JSON.stringify(src.motionStatic));
    // elementMotion is keyed by strokeId, and duplicateLayer's frames clone
    // above (JSON.stringify) preserves each stroke's strokeId unchanged —
    // so the duplicate's strokes carry the SAME ids the original's element
    // motion data is keyed by, a plain deep-copy stays correctly matched.
    if(src.elementMotion)state.layers[ni].elementMotion=JSON.parse(JSON.stringify(src.elementMotion));
    if(src.duplicator){state.layers[ni].duplicator=JSON.parse(JSON.stringify(src.duplicator));state.layers[ni].locked=true;}
    // ld.rig doesn't force-lock its layer (unlike duplicator) — binds
    // reference strokeId, unchanged by the frame clone above (same comment
    // as elementMotion just above), so relinkRigBinds matches correctly
    // on the duplicate without any extra remapping.
    if(src.rig)state.layers[ni].rig=JSON.parse(JSON.stringify(src.rig));
    // Combine groups (2026-07-29): the frame clone above already preserves
    // each stroke's data.groupId unchanged, so the duplicate's strokes carry
    // the SAME groupId strings as the original — harmless for duplicateLayer
    // ALONE (resolveGroupMembers/membersOf are always scoped to one explicit
    // layer argument, never cross-layer) but see mergeLayersIntoOne's own
    // groupId remap for why merging this duplicate back with its original
    // needs a fresh id per group.
    if(src.groups)state.layers[ni].groups=JSON.parse(JSON.stringify(src.groups));
    // Bugs found live (2026-07-29 QA sweep): symbolId/lfsGroup were missing
    // here entirely — since both redirect real content to state.symbols
    // (getEffectiveStrokes never reads ld.frames for them, and the save
    // guards above correctly never write resolved content back into them),
    // a duplicate with neither flag ended up with genuinely-empty frames —
    // a blank, invisible layer (Component case), or silently demoted to a
    // plain layer that happened to show whatever had already leaked into
    // ld.frames (LFS case, see the leak fix in saveActiveLayerFrame). A
    // Component instance sharing its source symbolId is the existing,
    // correct model for multiple instances of one symbol (each instance
    // already carries its own placement/motion/camera) — same sharing
    // model applies to an LFS group's 3 channel symbols.
    if(src.symbolId)state.layers[ni].symbolId=src.symbolId;
    if(src.lfsGroup){state.layers[ni].lfsGroup=true;state.layers[ni].lfsIds=JSON.parse(JSON.stringify(src.lfsIds));state.layers[ni].lfsSettings=JSON.parse(JSON.stringify(src.lfsSettings));state.layers[ni].locked=true;}
    if(src.inPoint!=null)state.layers[ni].inPoint=src.inPoint;if(src.outPoint!=null)state.layers[ni].outPoint=src.outPoint;
    // 2026-07 fix: markers/shy/keyLock/timeRemap/motionBlur/effects/
    // effectsFrom/isEffectLayer were all missing from this list — every one
    // of them is a real, persisted per-layer field (see exportJSON's own
    // layer serialization a few hundred lines up) that this function simply
    // never copied, so a duplicate silently lost each one even though the
    // source layer visibly had it. motionBlurSamples/motionBlurShutter are
    // NOT here on purpose — those are comp-wide (state.*), not per-layer.
    if(src.markers)state.layers[ni].markers=JSON.parse(JSON.stringify(src.markers));
    if(src.shy)state.layers[ni].shy=true;
    if(src.keyLock)state.layers[ni].keyLock=src.keyLock;
    if(src.timeRemap)state.layers[ni].timeRemap=JSON.parse(JSON.stringify(src.timeRemap));
    if(src.motionBlur)state.layers[ni].motionBlur=true;
    if(src.effects&&src.effects.length)state.layers[ni].effects=JSON.parse(JSON.stringify(src.effects));
    if(src.effectsFrom)state.layers[ni].effectsFrom=src.effectsFrom;
    if(src.isEffectLayer)state.layers[ni].isEffectLayer=true;
    // Same gap, noticed while adding multi-parent (2026-07-30): a parented
    // layer's duplicate silently came out unparented — parentLayerUid
    // (and now parentLayerUidB) were never copied either, despite being
    // real per-layer fields with no comment explaining an intentional
    // omission. Referencing the SAME uid the source points at, not a
    // freshly-generated one — a duplicate following its source's parent
    // is the expected "keep every relationship" behavior every other
    // field on this list already follows.
    if(src.parentLayerUid)state.layers[ni].parentLayerUid=src.parentLayerUid;
    if(src.parentLayerUidB)state.layers[ni].parentLayerUidB=src.parentLayerUidB;
    activateUL(ni);loadFrame(state.currentFrame);updateUI();},
  setActiveLayer:function(idx){if(idx<0||idx>=state.layers.length)return;saveAllLayerFrames();activateUL(idx);clearSel();
    window._layerActiveExplicit=true; // see clearSel()'s own comment — an explicit timeline row click, not a canvas deselect
    // The camera row is a synthetic pseudo-layer (not a real state.layers
    // entry — see camera.js's renderPanelRow) selected by switching TO the
    // camera tool, never by an activeLayerIdx change; picking a real layer
    // here has to explicitly leave it, or its guides/highlight stay stuck
    // on screen with no layer row left looking selected (feedback #5wrip).
    if(state.tool==='camera')window.SM.setTool('select');
    // Motion mode (2026-07-17, "on ne voit pas la box de transformation à
    // la selection d'un calque dans motion") : picking a layer row there
    // almost never happens with the Select tool already active (Motion's
    // own workflow is click-a-layer-row, not pick-a-tool-first) — same
    // camera-row precedent just above, forced here too. Without this, the
    // selection-population block below silently no-op'd (gated on
    // Select/Subselect) AND buildTransformBoxItems (engine-bridge.js)
    // hides the box for any other tool, so Motion's layer click looked
    // completely inert on canvas even though activeLayerIdx did change.
    if(state.appMode==='motion'&&state.tool!=='select'&&state.tool!=='subselect')window.SM.setTool('select');
    // Selecting a layer shows the selection of ALL its elements on canvas
    // (2026-07-17, "quand on select un calque on voit la selection dans le
    // canvas de tous ces éléments") — clearSel() above used to leave the
    // canvas with nothing highlighted, the opposite of what clicking a
    // layer row means in every layer-based tool (Illustrator/AE: picking a
    // layer selects its content). Only with Select/Subselect active —
    // forcing a canvas selection while, say, Draw is the active tool would
    // fight the next stroke. Same isSelectablePathChild filter every other
    // "select everything in this layer" call site already uses (marquee-
    // on-empty-space fallback, component click) — brush-texture dabs/
    // linkedFill companions excluded, they aren't independently selectable.
    if((state.tool==='select'||state.tool==='subselect')&&userLayers[idx]){
      selectedPaths=userLayers[idx].children.filter(function(c){return (c instanceof Path||c instanceof Raster)&&isSelectablePathChild(c);});
      state.selectedStrokeIndices=[];
    }
    renderArcs();updateUI();},
  toggleLayerVis:function(idx){state.layers[idx].visible=!state.layers[idx].visible;loadFrame(state.currentFrame);updateUI();},
  toggleLayerLock:function(idx){
    // A duplicator layer's lock is managed by the duplicator itself
    // (toggleLayerDuplicator/setDuplicatorEditSource, motion.js) — a direct
    // padlock unlock would let edits hit the N-way-expanded live layer and
    // desync locked/_dupEditSource. Route through the panel's own button.
    if(state.layers[idx].duplicator&&!state.layers[idx]._dupEditSource&&state.layers[idx].locked){showToast('Calque duplicateur — « Modifier la forme source » (panneau Duplicator) pour éditer');return;}
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
  // Stroke profiles (van Dijk 6.2). Acts on the current canvas selection;
  // one undo step for the whole batch, like every other selection command.
  applyStrokeProfile:function(kind){
    var paths=(window.selectedPaths||[]).filter(function(p){return p&&p.segments;});
    if(!paths.length){showToast('Sélectionne un ou plusieurs traits');return;}
    pushUndo();
    var done=0,skipped=0;
    paths.forEach(function(p){ if(applyStrokeProfileToPath(p,kind))done++; else skipped++; });
    saveActiveLayerFrame();
    loadFrame(state.currentFrame);updateUI();
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
    showToast(done?(done+' trait(s) profilé(s)'+(skipped?' — '+skipped+' ignoré(s)':'')):'Aucun trait convertible dans la sélection');
  },
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
  splitLayerIntoElementsCore:function(li,opts){return splitLayerIntoElementsCore(li,opts);},
  splitLayerIntoElements:function(li){return splitLayerIntoElements(li);},
  mergeLayersIntoOne:function(indices,opts){return mergeLayersIntoOne(indices,opts);},
  exitToScene:function(){exitToScene();},
  closeSymbolTab:function(symId){closeSymbolTab(symId);},
  enterMontageView:function(montageId){enterMontageView(montageId);},
  exitMontageView:function(){exitMontageView();},
  setSymbolPlayMode:function(v){var ld=state.layers[state.activeLayerIdx];if(!ld||!ld.symbolId)return;ld.symPlayMode=v;loadFrame(state.currentFrame);renderOS();updateUI();},
  setSymbolSpeed:function(v){var ld=state.layers[state.activeLayerIdx];if(!ld||!ld.symbolId)return;ld.symSpeed=Math.max(0.1,parseFloat(v)||1);loadFrame(state.currentFrame);renderOS();updateUI();},
  setSymbolSingleFrame:function(v){
    var ld=state.layers[state.activeLayerIdx];if(!ld||!ld.symbolId)return;
    var picked=Math.max(0,parseInt(v)||0);
    ld.symSingleFrame=picked;
    // When the playhead is on an outer component key, the choice belongs to
    // that key (and is held until the next one). Older projects and frames
    // without an explicit key keep the legacy global single-frame fallback.
    var f=ld.frames&&ld.frames[state.currentFrame];
    if(f&&f.isKeyframe){f.componentFrame=picked;delete f.blankOverride;}
    loadFrame(state.currentFrame);renderOS();updateUI();
  },
  setSymbolPlacedAt:function(v){var ld=state.layers[state.activeLayerIdx];if(!ld||!ld.symbolId)return;ld.symPlacedAt=parseInt(v)||0;loadFrame(state.currentFrame);renderOS();updateUI();},
  moveFrames:function(sel,dLayer,dFrame){
    if(!sel.length)return;
    var b=selBounds();if(!b)return;
    var offsetL=dLayer-b.minL,offsetF=dFrame-b.minF;
    if(offsetL===0&&offsetF===0)return;
    // RIPPLE (2026-07-28 feedback: "bougé l'outpoint d'une clé dans
    // Animation 2D ne pousse pas toutes les clé qui sont après seulement la
    // clé suivante"). Grabbing a single keyframe's own dot in the frame-grid
    // (the mousedown handler above, `grabbedDot` branch) sets `sel` to
    // exactly ONE {layer,frame} entry and lands here — every LATER keyframe
    // on that same layer used to sit at its own original frame number,
    // since rekeyTweenPairData/retimeTweenSpans below only ever rekey the
    // tween PAIR touching a frame that's actually in `sel`. Mirrors
    // moveKeyframe's own ripple fix (timeline.js, Motion's equivalent
    // gesture via layer-inout.js) — SAME proof for why only the upper bound
    // needs guarding: every rippled frame is > the dragged one, so adding
    // the identical offsetF to both can never reorder or collide them,
    // regardless of offsetF's sign.
    //
    // Scoped tightly to this one gesture (single cell, no layer change): a
    // genuine multi-cell rectangle selection, or a cross-layer drag, is a
    // different action the user already explicitly scoped by selecting —
    // rippling THOSE too would silently move frames nobody selected.
    //
    // Added to `sel` itself (not handled separately) so the rest of this
    // function's existing generic capture/blank/write/rekey pipeline
    // carries the rippled frames along for free — they are ordinary
    // keyframes on the same layer, indistinguishable from one the user
    // selected by hand.
    if(sel.length===1&&offsetL===0){
      var li0=sel[0].layer,ld0=state.layers[li0];
      var srcFrame0=ld0&&ld0.frames[sel[0].frame];
      if(ld0&&!ld0.locked&&srcFrame0&&srcFrame0.isKeyframe){
        var allKfs0=ld0.frames.map(function(f,fi){return f.isKeyframe?fi:null;}).filter(function(x){return x!==null;});
        var laterKfs0=allKfs0.filter(function(f){return f>sel[0].frame;});
        if(offsetF>0&&laterKfs0.length){
          // Must clamp here, not rely on the generic out-of-range SKIP a few
          // lines down (`if(tl<0||...||tf>=state.totalFrames)return;`) —
          // that skip silently DROPS the frame (already blanked from its
          // source by then), which for a frame the user never explicitly
          // selected would be data loss introduced by this very fix.
          var lastKf0=laterKfs0[laterKfs0.length-1];
          var maxDelta0=(state.totalFrames-1)-lastKf0;
          if(offsetF>maxDelta0)offsetF=Math.max(0,maxDelta0);
        }
        laterKfs0.forEach(function(f){sel.push({layer:li0,frame:f});});
      }
    }
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
    var beforeKeyframes={},capturedInbetweens={};
    Object.keys(touchedLayers).forEach(function(lk){
      var li=parseInt(lk,10),ld=state.layers[li];if(!ld)return;
      beforeKeyframes[li]=ld.frames.map(function(f,fi){return f.isKeyframe?fi:null;}).filter(function(x){return x!==null;});
      // Snapshot each pair's in-betweens BEFORE the blanking/write below —
      // a moved key can land ON one of them (see retimeTweenSpans).
      capturedInbetweens[li]=captureTweenInbetweens(li,beforeKeyframes[li]);
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
      var pairs=[];
      for(var i=0;i<kfs.length-1;i++){
        var fA=kfs[i],fB=kfs[i+1];
        var newFA=movedFrameMap[li+':'+fA];newFA=(newFA!==undefined)?newFA:fA;
        var newFB=movedFrameMap[li+':'+fB];newFB=(newFB!==undefined)?newFB:fB;
        rekeyTweenPairData(fA,fB,newFA,newFB);
        pairs.push({fA:fA,fB:fB,newFA:newFA,newFB:newFB});
      }
      retimeTweenSpans(li,pairs,capturedInbetweens[li]||{});
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
    if(typeof window!=='undefined')window._lastClipKind='frames';
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
    if(typeof window!=='undefined')window._lastClipKind='frames';
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
      var pasted=JSON.parse(JSON.stringify(d.content));
      // A copied TWEEN frame (generated in-between) pastes as a normal
      // FULL keyframe — explicit request (2026-07-16, "un clé de tween
      // copier et collé ailleurs devient une keyframe pleine normal") :
      // le contenu collé verbatim gardait isInterpolated=true, donc la
      // copie restait affichée/traitée comme un inbetween généré
      // (badge TWEEN, re-écrasable par la prochaine régénération de
      // tween) alors qu'un collage est un choix éditorial délibéré —
      // même principe "manual edit wins" que isManualEdit, en plus fort.
      if(pasted.isInterpolated){pasted.isInterpolated=false;pasted.isKeyframe=true;delete pasted.isManualEdit;}
      state.layers[tl].frames[tf]=pasted;
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
  // Was dead code (defined, never called from any UI path) until Motion
  // mode's in/out bar (layer-inout.js) started drawing per-keyframe tick
  // marks and needed a single-keyframe retime that DOESN'T touch the
  // frame-grid's own _sel/selBounds state (moveFrames below reads that
  // global selection directly — fine from timeline.js's own drag handler,
  // wrong to reach into from a different mode's UI that has nothing
  // selected there). Brought up to the same standard as moveFrames: syncs
  // the live document first (saveAllLayerFrames) and rekeys motion-arc
  // tween data across the move (rekeyTweenPairData), which the original
  // version of this function skipped.
  // RIPPLE (2026-07-28 feedback, with screenshot of the ruler's key
  // diamonds): dragging one keyframe used to leave every LATER keyframe
  // sitting at its own original frame number — only the tween pair
  // immediately touching the dragged key got re-keyed (rekeyTweenPairData
  // matched on `fA===fromFrame` alone), so the gap the drag opened or
  // closed never propagated past the very next key. "quand je bouge
  // l'outpoint d'une clé ça bouge la clé suivante mais pas toutes les
  // clé qui suivent" — every timeline tool with this gesture (AE,
  // Premiere, Harmony's ripple) carries the rest of the sequence along so
  // relative spacing is preserved; this now does that as the default,
  // not an opt-in.
  moveKeyframe:function(layerIdx,fromFrame,toFrame){
    if(fromFrame===toFrame)return false;
    var ld=state.layers[layerIdx];if(!ld||ld.locked)return false;
    if(toFrame<0||toFrame>=state.totalFrames)return false;
    var src=ld.frames[fromFrame];if(!src||!src.isKeyframe)return false;
    pushUndo();saveAllLayerFrames();
    var beforeKfs=ld.frames.map(function(f,fi){return f.isKeyframe?fi:null;}).filter(function(x){return x!==null;});
    // beforeKfs is walked in ascending frame order (map over the frames
    // array), so this filter is already sorted — no separate sort needed.
    var laterKfs=beforeKfs.filter(function(f){return f>fromFrame;});
    var delta=toFrame-fromFrame;
    // Only the UPPER bound needs guarding. laterKfs[i] > fromFrame for
    // every entry, so laterKfs[i]+delta > fromFrame+delta = toFrame always
    // — the dragged key can never end up sitting past (or on) a rippled
    // one, regardless of delta's sign, so relative order/spacing among the
    // moved set survives untouched. Lower bound (>=0) follows from the
    // same inequality once toFrame itself is known >=0 (guarded above).
    if(delta>0&&laterKfs.length){
      var lastKf=laterKfs[laterKfs.length-1];
      var maxDelta=(state.totalFrames-1)-lastKf;
      if(delta>maxDelta){delta=Math.max(0,maxDelta);toFrame=fromFrame+delta;}
    }
    var capturedIb=captureTweenInbetweens(layerIdx,beforeKfs);
    // Every frame actually changing slot, dragged key included. Read ALL
    // source content before writing anything — moves.length writes can't
    // step on each other's reads this way, so write order doesn't matter
    // (unlike the single-key version this replaces, which never had this
    // hazard because it only ever touched two slots).
    var moves=[{from:fromFrame,to:toFrame}];
    laterKfs.forEach(function(f){moves.push({from:f,to:f+delta});});
    var srcData={};moves.forEach(function(m){srcData[m.from]=ld.frames[m.from].strokes;});
    var destSet={};moves.forEach(function(m){destSet[m.to]=true;});
    moves.forEach(function(m){ld.frames[m.to]={strokes:srcData[m.from],isKeyframe:true,isInterpolated:false};});
    // Clear a vacated slot only if nothing else just moved INTO it —
    // otherwise a same-frame no-op segment (from===to, possible once delta
    // is clamped to 0 above) would erase the very content it just wrote.
    moves.forEach(function(m){if(!destSet[m.from])ld.frames[m.from]={strokes:[],isKeyframe:false,isInterpolated:false};});
    var moveMap={};moves.forEach(function(m){moveMap[m.from]=m.to;});
    var mkPairs=[];
    for(var i=0;i<beforeKfs.length-1;i++){
      var fA=beforeKfs[i],fB=beforeKfs[i+1];
      var newFA=moveMap[fA]!==undefined?moveMap[fA]:fA;
      var newFB=moveMap[fB]!==undefined?moveMap[fB]:fB;
      rekeyTweenPairData(fA,fB,newFA,newFB);
      mkPairs.push({fA:fA,fB:fB,newFA:newFA,newFB:newFB});
    }
    retimeTweenSpans(layerIdx,mkPairs,capturedIb);
    loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast('Keyframe déplacée → '+(toFrame+1));
    return true;
  },
  // Shifts EVERY keyframe/content frame of a layer by dx frames at once —
  // used by layer-inout.js when dragging a bar's BODY (whole-range move,
  // not a pure in/out trim): "il faudrait pouvoir select des in et/out
  // point de calque avec keyframe pour les déplacer ensemble". Moving the
  // visibility window alone (ld.inPoint/outPoint) left the actual drawn
  // content behind — this keeps them in sync by retiming the whole clip's
  // source frames together with the window, exactly like dragging a clip
  // in After Effects/Premiere moves both.
  //
  // Deliberately NO pushUndo/saveAllLayerFrames-then-render here: the
  // caller (layer-inout.js) already took ONE undo snapshot at drag START
  // (not per mousemove) and may call this once per member of a group
  // drag — an extra pushUndo here would split one user drag into several
  // undo steps. Caller is responsible for the final loadFrame/render pass
  // too, same reasoning.
  //
  // Bug found live 2026-07-17 ("ça ne retime pas la forme, ça n'agit pas
  // vraiment"): this used to call saveAllLayerFrames() here too, AFTER the
  // drag had already committed ld.inPoint/outPoint (mousemove sets those
  // live, per-frame, so the visible range shrinks WHILE dragging — see
  // layer-inout.js's own "must reflect the new range live" comment). By
  // the time THIS function ran (at drop), getEffectiveStrokes' in/out gate
  // (app.js) had already hidden the source frame's content from the LIVE
  // Paper canvas, so saveAllLayerFrames() re-collected an now-EMPTY canvas
  // straight into ld.frames — permanently wiping the very content this
  // function is supposed to carry forward, before the shift loop below
  // ever got to read it. The caller already saved everything it needs to
  // BEFORE starting the drag (see onDown, layer-inout.js) — no reason to
  // re-derive from a canvas the drag itself has since made unreliable.
  shiftLayerFrames:function(layerIdx,dx){
    var ld=state.layers[layerIdx];if(!ld||ld.locked||!dx)return false;
    var total=state.totalFrames;
    var beforeKfs=ld.frames.map(function(f,fi){return f.isKeyframe?fi:null;}).filter(function(x){return x!==null;});
    var newFrames=[];
    for(var i=0;i<total;i++)newFrames.push({strokes:[],isKeyframe:false,isInterpolated:false});
    for(var j=0;j<ld.frames.length;j++){
      var src=ld.frames[j];
      if(!src||!src.strokes||!src.strokes.length)continue; // truly empty — nothing to carry
      var ni=j+dx;
      if(ni<0||ni>=total)continue; // falls off the timeline edge — dropped, matches moveFrames' own bounds behavior
      newFrames[ni]=src;
    }
    ld.frames=newFrames;
    for(var k=0;k<beforeKfs.length-1;k++){
      var fA=beforeKfs[k],fB=beforeKfs[k+1];
      rekeyTweenPairData(fA,fB,fA+dx,fB+dx);
    }
    return true;
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
    //
    // Same problem, same fix, for a StoryBoard montage view (2026-07,
    // enterMontageView/app.js): while active, state.layers is a SYNTHETIC
    // per-segment scene built for editing, not the real top-level document
    // — saving it wholesale would silently overwrite the real scene's own
    // layer arrangement with the montage's segments the moment autosave's
    // 30s tick fires while a montage is open. Checked BEFORE activeSymbolId
    // deliberately: entering a component FROM WITHIN a montage view (a
    // supported nested case) leaves both flags set, and _sceneSnapshot in
    // that case only unwinds one level (back to the montage view's own
    // synthetic scene) — _montageViewSnapshot is the one that actually
    // holds the real document.
    saveAllLayerFrames();
    var inMontage=!!(state.activeMontageViewId&&_montageViewSnapshot);
    var inSym=!inMontage&&!!(state.activeSymbolId&&_sceneSnapshot);
    var srcSnap=inMontage?_montageViewSnapshot:(inSym?_sceneSnapshot:null);
    var sceneLayers=srcSnap?srcSnap.layers:state.layers;
    var sceneTotal=srcSnap?srcSnap.totalFrames:state.totalFrames;
    var sceneFps=srcSnap?srcSnap.fps:state.fps;
    var sceneWaIn=srcSnap?srcSnap.waIn:state.waIn;
    var sceneWaOut=srcSnap?srcSnap.waOut:state.waOut;
    return JSON.stringify({version:13,totalFrames:sceneTotal,fps:sceneFps,canvasW:state.canvasW,canvasH:state.canvasH,canvasBg:state.canvasBg,waIn:sceneWaIn,waOut:sceneWaOut,
      layers:sceneLayers.map(function(l){return{name:l.name,visible:l.visible,locked:l.locked,frames:l.frames,symbolId:l.symbolId,symPlayMode:l.symPlayMode,symSpeed:l.symSpeed,symPlacedAt:l.symPlacedAt,symSingleFrame:l.symSingleFrame,symMatrix:l.symMatrix,lfsGroup:l.lfsGroup,lfsIds:l.lfsIds,lfsSettings:l.lfsSettings,blendMode:l.blendMode,folderId:l.folderId,channel:l.channel,linkGroupId:l.linkGroupId,color:l.color,motion:l.motion,motionStatic:l.motionStatic,elementMotion:l.elementMotion,inPoint:l.inPoint,outPoint:l.outPoint,nativeVideo:l.nativeVideo,matteMode:l.matteMode,montageId:l.montageId,expressions:l.expressions,isTextLayer:l.isTextLayer,isNullLayer:l.isNullLayer,isEffectLayer:l.isEffectLayer,effects:l.effects,footage:l.footage,
        // Layer parenting (2026-07-25). BOTH of these were missing from this
        // list, so every parent link was silently dropped on save — a rig
        // survived the session and nothing more. `uid` is the stable identity
        // parentLayerUid points at, so persisting one without the other would
        // be just as useless. Note isNullLayer above was already persisted,
        // and its own tooltip calls a null layer a "pivot/parent pour d'autres
        // calques" — the pivot came back, everything hung off it did not.
        layerUid:l.layerUid,parentLayerUid:l.parentLayerUid,parentLayerUidB:l.parentLayerUidB,markers:l.markers,shy:l.shy,keyLock:l.keyLock,timeRemap:l.timeRemap,motionBlur:l.motionBlur,effectsFrom:l.effectsFrom,timeLink:l.timeLink,
        // 3D layer toggle (2026-07-28) — see motion.js's compute3DCorners.
        threeD:l.threeD,
        // Mograph duplicator (2026-07-29) — copied wholesale like
        // symMatrix/lfsSettings, no per-field whitelist for its innards.
        // (_dupEditSource is transient and deliberately NOT persisted.)
        duplicator:l.duplicator||undefined,
        // Rig tool (2026-07-29) — bones/binds/ikChains, plain JSON by
        // construction (see app.js's ensureLayerRig), copied wholesale like
        // duplicator above. Binds reference strokeId, relinked on load by
        // relinkRigBinds (app.js, called from loadFrame) — never a live
        // Path reference, so a straight JSON round-trip is safe.
        rig:l.rig||undefined,
        // Non-destructive combine groups (2026-07-29) — copied wholesale
        // like duplicator/rig above. Membership itself is the plain
        // data.groupId tag on each stroke (already round-trips via serP/
        // desP unmodified); ld.groups is just the group-level combineMode/
        // order metadata group-bridge.js has nowhere else to hang.
        groups:l.groups||undefined};}),
      layerFolders:state.layerFolders,layerLinkGroups:state.layerLinkGroups,
      // StoryBoard node space (2026-07) — plain data by construction (no
      // runtime-only fields live in state.storyboard, see storyboard.js's
      // own data-model comment), so a wholesale copy is safe.
      storyboard:state.storyboard||null,
      symbols:state.symbols,palettes:state.palettes,activePaletteIdx:state.activePaletteIdx,customBrushPresets:state.customBrushPresets,
      shadowPalette:state.shadowPalette,shadowActiveId:state.shadowActiveId,
      // Custom WGSL effects (2026-07, feedback: "la possibilité d'ajouter
      // ses propres effets wgsl") — project-wide like symbols/palettes
      // above (not per-layer), since one definition can be applied to any
      // number of layers' effects stacks by referencing its id.
      customEffects:state.customEffects,
      // audio: only the persistable fields — _buffer/_peaksCanvas/_srcNode
      // are live runtime objects that must never hit JSON
      audioTracks:(state.audioTracks||[]).map(function(t){return{name:t.name,dataB64:t.dataB64,offsetFrames:t.offsetFrames||0,volume:t.volume!==undefined?t.volume:1,muted:!!t.muted};}),
      refMedia:state.refMedia?{type:state.refMedia.type,name:state.refMedia.name,src:state.refMedia.src,frames:state.refMedia.frames,opacity:state.refMedia.opacity,visible:state.refMedia.visible,offsetFrames:state.refMedia.offsetFrames||0}:null,
      mediaLibrary:(state.mediaLibrary||[]).map(function(m){return{id:m.id,name:m.name,kind:m.kind,thumb:m.thumb,layerName:m.layerName};}),
      perspectiveEnabled:state.perspectiveEnabled,perspectiveMode:state.perspectiveMode,perspectiveDensity:state.perspectiveDensity,perspectiveVPs:state.perspectiveVPs,
      symmetryEnabled:state.symmetryEnabled,symmetryMode:state.symmetryMode,symmetryAxis:state.symmetryAxis,symmetryRadialCenter:state.symmetryRadialCenter,symmetryRadialSectors:state.symmetryRadialSectors,symmetryExtend:state.symmetryExtend,
      motionArcs:state.motionArcs,easingCurve:state.easingCurve,resamplePts:state.resamplePts,tweenStep:state.tweenStep,
      tweenOverrides:state.tweenOverrides,tweenEasing:state.tweenEasing||{},comments:state.comments||[],
      cameraKeys:state.cameraKeys||[],cameraLayerOn:!!state.cameraLayerOn,
      // Comp markers (markers.js) — pure annotation, but losing them on save
      // would make the feature pointless.
      markers:state.markers||[],shyEnabled:!!state.shyEnabled,
      bpm:state.bpm,bpmOffset:state.bpmOffset,bpmShow:!!state.bpmShow,
      motionBlurOn:!!state.motionBlurOn,motionBlurSamples:state.motionBlurSamples,motionBlurShutter:state.motionBlurShutter,
      exprGlobals:state.exprGlobals||''});
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
    var fsPrimPCA=fsPrimarySel();
    if(state.tool==='fsselect'&&fsPrimPCA&&fsPrimPCA.path&&fsPrimPCA.path.data){
      sid=fsPrimPCA.path.data.strokeId;
      if(fsPrimPCA.kind==='stroke'&&fsPrimPCA.path.strokeColor)strokeHex=colorHex8(fsPrimPCA.path.strokeColor);
      else if(fsPrimPCA.path.fillColor)fillHex=colorHex8(fsPrimPCA.path.fillColor);
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
    // Every stored stroke dict is about to be replaced, so the engine's
    // retained path store (keyed on dict identity) is entirely stale.
    if(window.SMEngineBridge&&SMEngineBridge.clearRetainedPaths)SMEngineBridge.clearRetainedPaths();
    try{var d=JSON.parse(json);if(!d.layers&&!d.frames)throw new Error('Invalid');
    // exportJSON stamps version:13 but nothing ever CHECKED it — a file
    // written by a future format (version bumped for a breaking change)
    // half-loaded silently, dropping whatever the old code didn't know
    // about, and the user's next Save destroyed the original. Warn loudly
    // instead; loading still proceeds (fields degrade to defaults) but the
    // user knows an app update is needed and not to overwrite the file.
    // NOT gated on `silent` — openPath passes silent=true (it shows its own
    // "Opened" toast), and a data-integrity warning must never be muted.
    if(d.version&&d.version>13)showToast('⚠ Fichier créé par une version plus récente de Nemo (format v'+d.version+') — mettre à jour l’app avant de re-sauvegarder ce fichier');
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
    state.customEffects=d.customEffects||[];
    if(window.registerAllCustomEffects)window.registerAllCustomEffects();
    d.layers.forEach(function(ld){var idx=createUserLayer(ld.name);state.layers[idx].visible=ld.visible!==false;state.layers[idx].locked=ld.locked||false;state.layers[idx].frames=ld.frames;
      if(ld.blendMode)state.layers[idx].blendMode=ld.blendMode;
      if(ld.matteMode)state.layers[idx].matteMode=ld.matteMode;
      if(ld.expressions)state.layers[idx].expressions=ld.expressions;
      if(ld.isTextLayer)state.layers[idx].isTextLayer=true;
      if(ld.isNullLayer)state.layers[idx].isNullLayer=true;
      if(ld.isEffectLayer){state.layers[idx].isEffectLayer=true;}
      state.layers[idx].effects=ld.effects||[];
      if(ld.symbolId){state.layers[idx].symbolId=ld.symbolId;state.layers[idx].symPlayMode=ld.symPlayMode||'loop';state.layers[idx].symSpeed=ld.symSpeed||1;state.layers[idx].symPlacedAt=ld.symPlacedAt||0;state.layers[idx].symSingleFrame=ld.symSingleFrame||0;if(ld.symMatrix)state.layers[idx].symMatrix=ld.symMatrix;}
      if(ld.lfsGroup){state.layers[idx].lfsGroup=true;state.layers[idx].lfsIds=ld.lfsIds;state.layers[idx].lfsSettings=ld.lfsSettings;}
      if(ld.folderId)state.layers[idx].folderId=ld.folderId;
      if(ld.channel)state.layers[idx].channel=ld.channel;
      if(ld.linkGroupId)state.layers[idx].linkGroupId=ld.linkGroupId;
      // Restore the layer's own identity BEFORE anything resolves a parent
      // against it — parentLayerUid is matched by uid, so a layer whose uid
      // was regenerated on load would orphan every child pointing at it.
      if(ld.layerUid)state.layers[idx].layerUid=ld.layerUid;
      if(ld.parentLayerUid)state.layers[idx].parentLayerUid=ld.parentLayerUid;
      if(ld.parentLayerUidB)state.layers[idx].parentLayerUidB=ld.parentLayerUidB;
      if(ld.motion)state.layers[idx].motion=ld.motion;
      if(ld.elementMotion)state.layers[idx].elementMotion=ld.elementMotion;
      if(ld.motionStatic)state.layers[idx].motionStatic=ld.motionStatic;
      // `!=null`, not truthy: a project saved with an explicit inPoint of 0
      // silently lost that override on reload, because 0 is falsy. Its
      // sibling one line down already used !=null. Pre-existing, found
      // 2026-07-26 while auditing this whitelist.
      if(ld.inPoint!=null)state.layers[idx].inPoint=ld.inPoint;
      if(ld.outPoint!=null)state.layers[idx].outPoint=ld.outPoint;
      // EXPERIMENTAL (native-video-decode): decoder session is runtime-only
      // (_nvSessionId) — native-video-bridge reopens it lazily from
      // nativeVideo.path on the first frame sync after this load.
      if(ld.nativeVideo)state.layers[idx].nativeVideo=ld.nativeVideo;
      if(ld.montageId)state.layers[idx].montageId=ld.montageId;
      // Footage tag (2026-07-27) — descriptive only (it changes no
      // stroke), but exportJSON persists it, so the import side has to
      // restore it or a reopened project silently falls back to the
      // heuristic and can re-label a sequence as a still.
      if(ld.footage)state.layers[idx].footage=ld.footage;
      // Migration: every component created before 2026-07-28 carries a
      // componentFrame:0 stamp on its frame 0, written by the conversion
      // itself. resolveSymbolFrameIdx reads that as a deliberate "hold
      // internal frame 0", so those components never play — and show nothing
      // at all when their content starts after frame 0. A DELIBERATE pick is
      // distinguishable: the frame strip sets play mode to 'single' at the
      // same time (setSymbolSingleFrame + setSymbolPlayMode, timeline.js), so
      // only clear the stamp when the layer is in a PLAYING mode, where an
      // explicit hold on frame 0 is exactly the bug and never a choice.
      var _mig=state.layers[idx];
      if(_mig.symbolId&&_mig.symPlayMode!=='single'&&_mig.frames&&_mig.frames[0]&&_mig.frames[0].componentFrame===0){
        var _other=false;
        for(var _fi=1;_fi<_mig.frames.length;_fi++){if(_mig.frames[_fi]&&_mig.frames[_fi].componentFrame!=null){_other=true;break;}}
        if(!_other)delete _mig.frames[0].componentFrame;
      }
      // Per-layer fields added 2026-07-25/26. This restore list is a
      // WHITELIST — a field written by exportJSON but absent here is
      // silently dropped on load, which is the same "writer updated, reader
      // forgotten" shape as CLAUDE.md §1's warning about
      // saveAllLayerFrames. Every one of these was in the file and gone
      // after a round-trip until this block existed; caught by testing the
      // round-trip rather than the save.
      if(ld.markers)state.layers[idx].markers=ld.markers;                 // repères de calque
      if(ld.shy)state.layers[idx].shy=true;                               // interrupteur shy
      if(ld.keyLock)state.layers[idx].keyLock=ld.keyLock;                 // verrou clés -> in/out
      if(ld.timeRemap)state.layers[idx].timeRemap=ld.timeRemap;           // remappage temporel
      if(ld.motionBlur)state.layers[idx].motionBlur=true;                 // flou de mouvement
      if(ld.effectsFrom)state.layers[idx].effectsFrom=ld.effectsFrom;     // Instance Effect
      if(ld.timeLink)state.layers[idx].timeLink=ld.timeLink;              // Parent in Time
      if(ld.threeD)state.layers[idx].threeD=true;                         // calque 3D
      if(ld.duplicator){state.layers[idx].duplicator=ld.duplicator;state.layers[idx].locked=true;} // duplicateur mograph (relock: _dupEditSource n'est jamais persisté)
      if(ld.rig)state.layers[idx].rig=ld.rig;                             // rig (os/binds/IK) — relinkRigBinds fait le reste au premier loadFrame
      if(ld.groups)state.layers[idx].groups=ld.groups;                    // groupes de combinaison non-destructifs — data.groupId sur chaque stroke fait déjà le tour via serP/desP
      state.layers[idx].color=ld.color||nextLayerColor();
      ld.frames.forEach(function(f){if(!f.isInterpolated)f.isInterpolated=false;});while(state.layers[idx].frames.length<state.totalFrames)state.layers[idx].frames.push({strokes:[],isKeyframe:false,isInterpolated:false});});
    state.layerFolders=d.layerFolders||{};state.layerLinkGroups=d.layerLinkGroups||{};
    state.motionArcs=d.motionArcs||{};state.tweenOverrides=d.tweenOverrides||{};state.tweenEasing=d.tweenEasing||{};
    // Migration (2026-07): the old shipped DEFAULT easing points
    // ({.42,0}/{.58,1} — CSS control values misread as on-curve knots, a
    // park/teleport/park cliff, see app.js's easingCurve comment) were
    // never a deliberate user choice — any project still carrying exactly
    // that default gets the corrected one. A curve the user actually
    // edited (any other point set) is left untouched.
    if(d.easingCurve){
      var _ec=d.easingCurve,_ep=_ec.points;
      var _isOldDefault=_ep&&_ep.length===4&&_ep.every(function(p){return typeof p.tx!=='number';})&&
        Math.abs(_ep[0].x)<1e-6&&Math.abs(_ep[0].y)<1e-6&&Math.abs(_ep[1].x-0.42)<1e-6&&Math.abs(_ep[1].y)<1e-6&&
        Math.abs(_ep[2].x-0.58)<1e-6&&Math.abs(_ep[2].y-1)<1e-6&&Math.abs(_ep[3].x-1)<1e-6&&Math.abs(_ep[3].y-1)<1e-6;
      if(_isOldDefault)_ec={points:[{x:0,y:0},{x:.3,y:.05},{x:.7,y:.95},{x:1,y:1}]};
      state.easingCurve=_ec;if(window._curveEditor)window._curveEditor.setState(_ec);
    }
    state.comments=d.comments||[];
    state.markers=d.markers||[];
    state.shyEnabled=!!d.shyEnabled;
    state.bpm=d.bpm!=null?d.bpm:120;state.bpmOffset=d.bpmOffset||0;state.bpmShow=!!d.bpmShow;
    state.exprGlobals=d.exprGlobals||'';
    state.motionBlurOn=!!d.motionBlurOn;state.motionBlurSamples=d.motionBlurSamples||6;state.motionBlurShutter=d.motionBlurShutter!=null?d.motionBlurShutter:0.5;
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
    state.shadowPalette=(d.shadowPalette&&d.shadowPalette.length)?d.shadowPalette:[{id:'sh1',color:'#ff3355'},{id:'sh2',color:'#ff8800'},{id:'sh3',color:'#ffdd00'},{id:'sh4',color:'#22cc55'},{id:'sh5',color:'#2288ff'},{id:'sh6',color:'#aa33ff'}];
    state.shadowActiveId=d.shadowActiveId||state.shadowPalette[0].id;
    state.customBrushPresets=d.customBrushPresets||{};
    state.audioTracks=d.audioTracks||[];
    if(window.SMAudio)SMAudio.reload();
    state.refMedia=d.refMedia||null;
    state.storyboard=d.storyboard||null;
    if(window.SMReference)SMReference.reload();
    state.mediaLibrary=d.mediaLibrary||[];
    if(window.SMMediaLibrary)SMMediaLibrary.reload();
    state.perspectiveEnabled=d.perspectiveEnabled||false;state.perspectiveMode=d.perspectiveMode||'2pt';state.perspectiveDensity=d.perspectiveDensity||24;state.perspectiveVPs=d.perspectiveVPs||null;
    state.symmetryEnabled=d.symmetryEnabled||false;state.symmetryMode=d.symmetryMode||'y';state.symmetryAxis=d.symmetryAxis||null;state.symmetryRadialCenter=d.symmetryRadialCenter||null;state.symmetryRadialSectors=d.symmetryRadialSectors||6;state.symmetryExtend=d.symmetryExtend!==undefined?d.symmetryExtend:true;
    if(window.renderPaletteGrid)window.renderPaletteGrid();
    if(d.resamplePts)state.resamplePts=d.resamplePts;if(d.tweenStep)state.tweenStep=d.tweenStep;
    // Repair motion keyframes saved with the step-shaped default ease before
    // 2026-07-25 — see DEFAULT_CURVE in motion.js. Only keys still carrying
    // that exact array are touched, so a hand-tuned curve is never rewritten.
    if(window.SMMotion&&SMMotion.migrateLegacyCurves)SMMotion.migrateLegacyCurves();
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
    'proj-fps':state.fps,'proj-frames':state.totalFrames};
  Object.keys(els).forEach(function(id){var el=document.getElementById(id);if(el&&document.activeElement!==el)el.value=els[id];});
  // Clip to canvas / Safety zones icon-toggle buttons (2026-07, replaced
  // checkboxes — see index.html's own comment) — synced here alongside
  // every other Document field so a project load/undo restores their
  // visible state too, not just the click handler's own toggle.
  var clipBtn=document.getElementById('btn-clip');if(clipBtn)clipBtn.classList.toggle('active',!!state.canvasClip);
  var safetyBtn=document.getElementById('btn-safety');if(safetyBtn)safetyBtn.classList.toggle('active',!!state.safetyZones);
}
// `frameOnly` — the caller guarantees that NOTHING but state.currentFrame
// changed. renderTimeline() rebuilds every ruler cell and every grid row from
// scratch (measured 9.9ms at 10 layers, 27.7ms at 40 — 4800 DOM nodes), and a
// scrub calls this once per displayed frame, so at 40 layers a drag was
// capped at 14fps. Its ONLY frame-dependent output is the '.cur' class on the
// header cell plus the playhead's position and flag text — verified by
// reading every state.currentFrame reference inside it — and updatePlayhead()
// already produces exactly those three, in 0.1ms.
//
// This is the same split playback has always used (startPlay's loop calls
// updatePlayhead alone and stopPlay then does a full updateUI), so it is not
// a new invariant, just the scrub path adopting the proven one. Any caller
// that CHANGED CONTENT must not pass frameOnly — the panel refreshes below
// still run either way, only the wholesale timeline rebuild is skipped.
function updateUI(frameOnly){
  syncDocFields();
  // countOnly: this is only ever read for .length just below.
  var strokes=getEffectiveStrokes(state.activeLayerIdx,state.currentFrame,true);
  document.getElementById('info-frame').textContent=state.currentFrame+1;
  document.getElementById('info-strokes').textContent=(window.SM&&SM.t?SM.t(strokes.length===1?'strokeCountOne':'strokeCountOther'):(strokes.length+' trait'+(strokes.length!==1?'s':''))).replace('{n}',strokes.length);
  var tlCfEl=document.getElementById('tl-cf');
  if(document.activeElement!==tlCfEl)tlCfEl.value=state.currentFrame+1;
  tlCfEl.max=state.totalFrames;
  document.getElementById('tl-tf').textContent=state.totalFrames;
  var f=state.layers[state.activeLayerIdx].frames[state.currentFrame];
  var badge=document.getElementById('info-badge');
  if(f&&f.isKeyframe){badge.style.display='inline-block';badge.className='badge key';badge.textContent='KEY';}
  else if(f&&f.isInterpolated){badge.style.display='inline-block';badge.className='badge tw';badge.textContent='TWEEN';}
  else badge.style.display='none';
  document.getElementById('info-sel').textContent=state.tool==='select'&&selectedPaths.length>0?selectedPaths.length+' '+SM.t('selCountSuffix'):'';
  window._totalF=state.totalFrames;window._waIn=state.waIn;window._waOut=state.waOut;window._curFrame=state.currentFrame;
  window.updateWaBar();window.updateOmMarkers(state.currentFrame,state.totalFrames);
  if(frameOnly)updatePlayhead();else renderTimeline();
  renderLayerList(frameOnly);updateCompInstancePanel();updateDuplicatorPanel();updateFootagePanel();updateSelPropsPanel();updateFsSelPanel();updateRevisionPanel();updateTextActionsPanel();if(window.updateEffectsPanel)window.updateEffectsPanel();updatePropsContext();
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
function getToolHelp(tool){
  var tt=(window.SM&&SM.t)?SM.t:function(k){return k;};
  var TOOL_HELP={
    // UI/UX audit (2026-07-30): Ctrl+glisser un coin (distort) had NO
    // affordance anywhere — not here, not on the handle itself before
    // Ctrl was already held. Alt+glisser also had two DIFFERENT meanings
    // depending on where the drag starts (pivot vs empty canvas) collapsed
    // into one label that only documented the first — split into two rows.
    select:{desc:tt('thSelectDesc'),sc:[['V',tt('thTool')],['Shift+clic',tt('thAdd')],['Alt+glisser (pivot)',tt('thMoveAnchor')],['Alt+glisser (zone vide)',tt('thLassoEmpty')],['Ctrl+glisser un coin',tt('thDistort')],['Suppr',tt('thErase')],['Shift+X',tt('thFlipH')],['Shift+Alt+X',tt('thFlipV')]]},
    subselect:{desc:tt('thSubselectDesc'),sc:[['A',tt('thTool')],['Alt+clic',tt('thBreakTangent')]]},
    fsselect:{desc:tt('thFsselectDesc'),sc:[['Shift+clic',tt('thAdd')]]},
    draw:{desc:tt('thDrawDesc'),sc:[['B',tt('thTool')],['Alt+glisser',tt('thSize')],['[ ]',tt('thSizePlusMinus')]]},
    // Shift (angle 45°) and Alt (break the handle being dragged into a
    // corner) were undocumented for Pen — the ONLY place a user would
    // learn them was Subselect's own Alt+clic row above, a different tool.
    pen:{desc:tt('thPenDesc'),sc:[['P',tt('thTool')],['Clic',tt('thAnchor')],['Shift',tt('thAngle45')],['Alt+glisser une poignée',tt('thBreakTangent')],['Échap',tt('thFinish')]]},
    line:{desc:tt('thLineDesc'),sc:[['Shift',tt('thAngle45')]]},
    rect:{desc:tt('thRectDesc'),sc:[['Shift',tt('thSquare')]]},
    ellipse:{desc:tt('thEllipseDesc'),sc:[['Shift',tt('thCircle')]]},
    fill:{desc:tt('thFillDesc'),sc:[['Alt+glisser',tt('thClosingStroke')],['Shift+clic',tt('thRemoveFill')],['Échap',tt('thCancelStroke')]]},
    fillbrush:{desc:tt('thFillbrushDesc'),sc:[]},
    // Alt+glisser live-resizes the brush here exactly like it does on Draw
    // (above) — was undocumented on this sibling tool specifically, even
    // though the gesture and the code path are the same.
    eraser:{desc:tt('thEraserDesc'),sc:[['E',tt('thTool')],['Alt+glisser',tt('thSize')],['[ ]',tt('thSizePlusMinus')]]},
    eyedropper:{desc:tt('thEyedropperDesc'),sc:[['I',tt('thTool')]]},
    hand:{desc:tt('thHandDesc'),sc:[['Espace',tt('thHoldTemp')],['Espace',tt('thTapPlay')]]},
    zoom:{desc:tt('thZoomDesc'),sc:[['Z',tt('thTool')]]},
    rotate:{desc:tt('thRotateDesc'),sc:[['Alt+glisser',tt('thOnOtherTool')]]},
    camera:{desc:tt('thCameraDesc'),sc:[['Clic-droit',tt('thEasingCurve')]]},
    comment:{desc:tt('thCommentDesc'),sc:[]},
    text:{desc:tt('thTextDesc'),sc:[]},
    perspective:{desc:tt('thPerspectiveDesc'),sc:[]},
    // Rig's Tracer step is Pen-style bone drawing (rig-bridge.js) — same
    // Shift/Alt semantics as Pen, undocumented here (only Pen's own row
    // had them, a different tool).
    rig:{desc:tt('thRigDesc'),sc:[['S',tt('thTool')],['Clic',tt('thAnchor')],['Shift',tt('thAngle45')],['Alt+glisser une poignée',tt('thBreakTangent')],['Glisser une ancre existante',tt('thRigPose')],['Double-clic',tt('thFinish')]]},
  };
  return TOOL_HELP[tool];
}
function statusbarHelpRender(desc,sc){
  var el=document.getElementById('statusbar-help');
  if(!el)return;
  var html=desc?desc+' ':'';
  html+=sc.map(function(pair){return '<span class="sc">'+pair[0]+'</span>'+pair[1];}).join(' ');
  el.innerHTML=html;
}
function getStatusbarDefaultSc(){
  var tt=(window.SM&&SM.t)?SM.t:function(k){return k;};
  return [['F5',tt('scFrame')],['F6',tt('scKey')],['F7',tt('scBlank')],['T',tt('scTween')],['D',tt('scDupli')],['F',tt('scFlip')],['X',tt('scMirror')],['Enter',tt('scPlay')]];
}
function updateStatusBarHelp(){
  var tt=(window.SM&&SM.t)?SM.t:function(k){return k;};
  // 1. A canvas element is selected (select/subselect) — most specific.
  if((state.tool==='select'||state.tool==='subselect')&&typeof selectedPaths!=='undefined'&&selectedPaths.length>0){
    var n=selectedPaths.length;
    statusbarHelpRender(n+' '+tt(n>1?'thElementsSelected':'thElementSelected')+' —',[['Suppr',tt('thErase')],['⌘/Ctrl+D',tt('thDuplicate')],['↑↓←→',tt('thMove')],['Échap',tt('thDeselect')]]);
    return;
  }
  // 2. fsselect has an active fill/stroke pick.
  if(state.tool==='fsselect'&&typeof _fsSel!=='undefined'&&_fsSel.length){
    var fsPrimSB=fsPrimarySel();
    var fsCountSB=_fsSel.length>1?' ('+_fsSel.length+')':'';
    statusbarHelpRender(tt(fsPrimSB.kind==='stroke'?'hdrStroke':'hdrFill')+fsCountSB+' '+tt('thSelected')+' —',[['Suppr',tt('thErase')],['Échap',tt('thDeselect')]]);
    return;
  }
  // 3. A timeline keyframe cell (or span) is selected.
  if(typeof _sel!=='undefined'&&_sel.frames&&_sel.frames.length>0){
    var kn=_sel.frames.length;
    statusbarHelpRender(kn+' '+tt(kn>1?'thKeyframesSelected':'thKeyframeSelected')+' —',[['F6',tt('scKey')],['F7',tt('scBlank')],['T',tt('scTween')],['D',tt('scDupli')],['F',tt('scFlip')],['X',tt('scMirror')],['Suppr',tt('thErase')]]);
    return;
  }
  // 4. The active tool's own help.
  var help=getToolHelp(state.tool);
  if(help){statusbarHelpRender(help.desc,help.sc);return;}
  // 5. Nothing more specific — original generic frame cheat-sheet.
  statusbarHelpRender('',getStatusbarDefaultSc());
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
  var show={'sel-props-sec':false,'fill-sec':false,'stroke-sec':false,'tool-opts-sec':false,'canvas-sec':false,'layer-sec':false,'rig-opts-sec':false,'combine-opts-sec':false};
  if(state.tool==='rig'){
    ctx='rig';
    show['rig-opts-sec']=true;
    show['layer-sec']=!!(state.layers[state.activeLayerIdx]);
    if(window.renderRigModeUI)renderRigModeUI();
    hdrText=(window.SM&&SM.t?SM.t('toolRig'):'Rig')+' — Options';
  }else if(state.tool==='fsselect'&&_fsSel.length){
    // Multi-select (2026-07): show BOTH sections if the selection mixes
    // fill and stroke picks — no Position/Size (this tool doesn't offer
    // transform, Select already owns that) and no Effects (blend mode
    // lives on the layer, not a fill/stroke aspect).
    ctx='fsselect';
    show['fill-sec']=_fsSel.some(function(s){return s.kind==='fill'||s.kind==='fillregion';});
    show['stroke-sec']=_fsSel.some(function(s){return s.kind==='stroke';});
    var fsPrimPC=fsPrimarySel();
    var fsSegLabel=fsPrimPC.kind==='stroke'&&!(fsPrimPC.segStart===0&&fsPrimPC.segEnd===fsPrimPC.path.length)?' (segment)':'';
    var fsFillLabel=fsPrimPC.kind==='fillregion'?' (région)':'';
    var fsCountPC=_fsSel.length>1?' ('+_fsSel.length+')':'';
    hdrText=(fsPrimPC.kind==='stroke'?'Stroke'+fsSegLabel:'Fill'+fsFillLabel)+fsCountPC+' sélectionné(e)';
  }else if(hasSel){
    ctx='selection';
    show['sel-props-sec']=show['fill-sec']=show['stroke-sec']=true;
    // Mockup 2026-07-17 (réordonnancement du panel) : Layer (Blend) et
    // Document restent visibles pendant une sélection, juste sous le bloc
    // transform — avant, sélectionner un trait les faisait disparaître.
    // EXCEPT the Subselect tool (2026-07, "avec subselection le panneau
    // document n'a pas besoin d'être ouvert") — editing individual vertices
    // has no use for canvas W/H/FPS/BG, and the panel real estate is more
    // useful given over to the Position/Size/Rotation-of-selected-vertices
    // fields (updateSelPropsPanel) that section sits right above.
    show['canvas-sec']=state.tool!=='subselect';
    show['layer-sec']=!!(state.layers[state.activeLayerIdx]);
    // Rig bind (2026-07-29 fix, "on ne sait pas comment select l'élément qui
    // doit y être associé"): #rig-opts-sec (with the "Lier la sélection"
    // button) used to be shown ONLY while state.tool==='rig' — but binding a
    // shape needs a REAL canvas selection, which only the Select/Subselect
    // tools can make (the Rig tool's own onDown intercepts every click for
    // bone-drawing/posing instead). So switching to Select to actually pick
    // something hid the one button that finishes the job — there was no
    // sequence of clicks that showed both a selection AND the Bind button at
    // once. Surfacing it here too (only when there's something to bind TO —
    // the active layer already has 1+ bones) closes that gap without
    // touching the Rig-tool-active branch above.
    var rigLd=state.layers[state.activeLayerIdx];
    if(rigLd&&rigLd.rig&&rigLd.rig.bones&&Object.keys(rigLd.rig.bones).length){show['rig-opts-sec']=true;if(window.renderRigModeUI)renderRigModeUI();}
    // Combine-group panel (2026-07-29 UX fix) — visible for ANY selection so
    // "Combiner" is discoverable the moment 2+ shapes are selected, not only
    // after already knowing the Alt+click/context-menu shortcuts exist.
    show['combine-opts-sec']=true;
    if(window.updateCombinePanel)updateCombinePanel();
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
    // Nothing selected on canvas — the right panel falls back to Document.
    // Layer-sec (Blend Mode etc.) only joins it if the CURRENT activeLayerIdx
    // was reached by an explicit click on a layer row in the timeline
    // (window._layerActiveExplicit, set by setActiveLayer/cleared by
    // clearSel() — tools.js) — 2026-07: "si on clic dans le canvas sans
    // rien sélectionner il ne faut pas afficher les options de calque, ce
    // n'est que si on sélectionne des calques dans la timeline". Before
    // this flag, activeLayerIdx being ALWAYS a valid index (never "none")
    // meant this branch showed the last-active layer's properties even
    // right after deselecting everything on canvas.
    show['layer-sec']=!!window._layerActiveExplicit&&!!(state.layers[state.activeLayerIdx]);
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
  var matteSel=document.getElementById('p-mattemode');
  if(matteSel&&state.layers[state.activeLayerIdx]){
    var mv=state.layers[state.activeLayerIdx].matteMode||'none';
    matteSel.dataset.value=mv;
    matteSel.textContent=(typeof MATTE_MODE_LABELS!=='undefined'&&MATTE_MODE_LABELS[mv])||mv;
  }
  // Flou/Ombre au sol sync moved into effects-panel.js's unified Effects
  // stack (2026-07 rewrite) — see updateEffectsPanel, hooked via updateUI.
  if(window.renderGradientPanel)window.renderGradientPanel();
  var hdrEl=document.getElementById('props-context-hdr');if(hdrEl)hdrEl.textContent=hdrText;
  // Motion mode: none of these 2D-drawing-tool sections apply (no active
  // Fill/Stroke/Draw tool, no canvas marquee selection in the Animation 2D
  // sense) — they were rendering unconditionally via this same inline
  // display:block regardless of state.appMode, which is what buried
  // #motion-props-sec (the ONLY section actually relevant in Motion mode,
  // and the one containing the ƒx expression buttons) ~1375px down the
  // right panel behind Fill/Stroke/Tool Options/etc. Reported as "toujours
  // pas d'endroit où mettre les expressions" even after the local
  // scroll-into-view fix (PR #48) — that fix only handled scrolling WITHIN
  // an already-visible section, not this much bigger burial problem.
  // layer-sec (Blend/Matte/Flou) is spared: those are genuine per-layer
  // properties still meaningful while animating, not a drawing-tool panel.
  if(state.appMode==='motion'){
    show['sel-props-sec']=show['fill-sec']=show['stroke-sec']=show['tool-opts-sec']=show['canvas-sec']=show['rig-opts-sec']=show['combine-opts-sec']=false;
  }
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
  // 2026-07 feedback ("une fois une brush selectionné/appliqué et modifié
  // dans le panel de droit > stroke, le menu déroulant... disparaît"):
  // applying a brush texture to a filled anchor deliberately nulls its
  // strokeColor (applyBrushTexture, tools.js — data.preTextureStroke keeps
  // the original for restoration). Requiring p.strokeColor here made this
  // row hide itself the moment ANY Stroke-panel edit re-ran updateUI(),
  // even though the path is still a legitimate, already-textured brush
  // stroke — check data.preTextureStroke as an alternate signal that a
  // stroke used to live here (or still does, for the no-fill invisible-
  // anchor case via data.preTextureOpacity).
  var eligibleSel=hasSel&&selectedPaths.every(function(p){return p instanceof Path&&(p.strokeColor||(p.data&&(p.data.preTextureStroke!==undefined||p.data.preTextureOpacity!==undefined)))&&!(p.data&&(p.data.isVectorBrush||p.data.isFillShape));});
  var brushPresetRow=document.getElementById('p-brushpreset-row');
  if(brushPresetRow)brushPresetRow.style.display=(state.tool==='draw'||eligibleSel)?'flex':'none';
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
  // Subselect + vertices picked: show/drive THEIR bounds, not the whole
  // path's (activeXformBounds/Vertex mode, wired further down alongside
  // wireLiveXformField) — 2026-07, "la position du panneau doivent driver
  // les vertices".
  var b=activeXformBounds();if(!b)return;
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
      // Restore the rotate/scale anchor from the (re)selected stroke's OWN
      // data (2026-07, "la position du point d'ancrage n'est pas mise en
      // mémoire si je désélectionne et resélectionne l'élément") —
      // state.xformAnchorKey/Custom is otherwise just session UI state,
      // reset to center by clearSel() on every new selection. Only applied
      // when every selected stroke agrees on the SAME saved anchor (same
      // "mixed selection falls back to the default" convention as
      // selBoxAngleOf/orientedSelBox above) — a multi-selection with
      // differing per-stroke anchors has no single honest pivot to show.
      var refAnchorKey=ref.data&&ref.data.xformAnchorKey,refAnchorCustom=ref.data&&ref.data.xformAnchorCustom;
      var anchorsAgree=selectedPaths.every(function(p){
        if(refAnchorCustom)return p.data&&p.data.xformAnchorCustom&&p.data.xformAnchorCustom[0]===refAnchorCustom[0]&&p.data.xformAnchorCustom[1]===refAnchorCustom[1];
        return (p.data&&p.data.xformAnchorKey)===refAnchorKey;
      });
      if(anchorsAgree&&refAnchorCustom){state.xformAnchorCustom=refAnchorCustom.slice();state.xformAnchorKey='mc';}
      else if(anchorsAgree&&refAnchorKey){state.xformAnchorKey=refAnchorKey;state.xformAnchorCustom=null;}
      else{state.xformAnchorKey='mc';state.xformAnchorCustom=null;}
      if(window.renderXformAnchorGrid)renderXformAnchorGrid();
      // Pressure-brush ribbons and fill-brush shapes are ALWAYS
      // fillColor:<ink color>/strokeColor:null by construction (they paint
      // via fillColor only — see draw-bridge.js's commitStroke): ref.fillColor
      // is truthy and ref.strokeColor is null on EVERY one of these
      // regardless of what the Fill/Stroke eyes were actually set to at
      // draw time. Reading that as "this path has fill enabled / stroke
      // disabled" and writing it into the GLOBAL state.fillEnabled/
      // strokeEnabled silently flipped BOTH channels for every FUTURE new
      // stroke the moment one of these got selected (e.g. by switching
      // tools/selecting while drawing) — reported as "quand je change
      // d'outil ça change en fill ou stroke non visible" (feedback #6yqij).
      // The strokeEnabled half was fixed before; the fillEnabled write was
      // left unguarded (ref.fillColor is truthy for EVERY brush-shape path,
      // so selecting any of them force-set state.fillEnabled=true) — same
      // bug, same root cause, re-reported. Both channels now skip this
      // sync entirely for these two path kinds, matching the isVectorBrush
      // exclusion already used for Cap/Join/Dash sync just below.
      // Texture anchors too (third occurrence of this same bug, 2026-07,
      // "si j'essaye de redessiner avec le trait brush bitmap il n'apparaît
      // plus"): a Bitmap Brush or vector-preset anchor carries
      // strokeColor:null as CAMOUFLAGE (applyBrushTexture/
      // applyBitmapBrushTexture null it on purpose, remembering the real
      // color in preTextureStroke) — reading that as "user disabled the
      // stroke channel" silently turned state.strokeEnabled off the moment
      // one got selected (e.g. by a subselect node edit), making every
      // FUTURE stroke invisible and skipping the bitmap-brush branch
      // entirely.
      var isBrushShape=ref.data&&(ref.data.isVectorBrush||ref.data.isFillShape||ref.data.bitmapBrushSpec||ref.data.brushTexturePreset||ref.data.isBrushTextureCopy);
      if(!isBrushShape){
        var hasFill=!!ref.fillColor;
        // colorHex8(), not .toCSS(true) — the latter always forces alpha to
        // 1 (Paper.js quirk, see colorHex8's own comment in app.js); .dataset
        // .hex8 alongside .value for the same native-<input>-truncates-alpha
        // reason as every other color-input writer in this codebase.
        var css=hasFill?colorHex8(ref.fillColor):state.fillColor;
        state.fillColor=css;state.fillEnabled=hasFill;
        // Through the shared writer, not a hand-picked subset of the DOM —
        // this used to paint pm-fill and pm-fill-c only, leaving p-fill-hex
        // and the whole LEFT panel showing the previous colour. See
        // paintFillSwatches for the measurement.
        paintFillSwatches(css);
        window.SM._syncFillEnabledUI(hasFill);
        var hasStroke=!!ref.strokeColor;state.strokeEnabled=hasStroke;
        // The stroke side adopted NOTHING before: selecting a #e91e63 stroke
        // left every stroke surface on the previous colour, so the panels
        // described a shape that wasn't selected.
        if(hasStroke){var scss=colorHex8(ref.strokeColor);state.strokeColor=scss;paintStrokeSwatches(scss);}
        window.SM._syncStrokeEnabledUI(hasStroke);
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
      // Bitmap Brush panel staleness fix — same "reflect the selection,
      // don't leave the tool-default stale" convention as Fill/Stroke/Cap
      // above, applied to the checkbox+fields the user reported as wrong
      // (2026-07-17, "il ne reconnait pas que c'est une texture bitmap...
      // la case n'est pas cochée"): p-bitmapbrush-on is a DRAW-TOOL default
      // (governs future strokes), but it never mirrored what was actually
      // SELECTED, so a bitmap-textured stroke read as "not bitmap" the
      // moment you clicked it, and switching it back to vector via
      // Apply/Remove-to-selection (btn-bitmap-apply/-remove, already wired
      // for exactly this — see bitmap-brush.js) looked broken because the
      // checkbox never confirmed which state you were in or landed in.
      // Bitmap Brush controls moved OUT of the Stroke panel entirely
      // (2026-07 harmonization — see index.html's own comment where that
      // section used to live): the state.* writes below no longer have a
      // Stroke-panel checkbox/fields to also mirror into, but they still
      // need to run unconditionally — the floating Brush panel
      // (brush-menu-bridge.js) reads these same state fields fresh every
      // time it's opened, so keeping them in sync with the actual selection
      // is still exactly as necessary as before, just with no DOM element
      // here to gate on.
      var bmSpec=ref.data&&ref.data.bitmapBrushSpec;
      state.bitmapBrushOn=!!bmSpec;
      if(bmSpec){
        state.bitmapTip=bmSpec.tip;state.bitmapSpacing=bmSpec.spacing;
        state.bitmapScatter=bmSpec.scatter;state.bitmapOpacity=Math.round(bmSpec.opacity*100);
        state.bitmapPressure=!!bmSpec.pressure;
      }
    }
  }
  document.getElementById('sel-count').textContent=selectedPaths.length+' '+SM.t(selectedPaths.length>1?'selCountSuffix':'selCountItemSuffix');
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
  if(state.tool!=='fsselect'||!_fsSel.length)return;
  var _fsSel0=fsPrimarySel();
  var p=_fsSel0.path;
  var ftog=document.getElementById('fill-enable-toggle'),stog=document.getElementById('stroke-enable-toggle');
  // Pressure-brush ribbons and fill-brush shapes are ALWAYS fillColor:<ink>/
  // strokeColor:null by construction (draw-bridge.js commitStroke) — same
  // exclusion as updateSelPropsPanel's isBrushShape guard just above, same
  // reason: their fillColor being set is NOT a signal that the Fill/Stroke
  // eyes were actually on at draw time.
  var isBrushShape=p.data&&(p.data.isVectorBrush||p.data.isFillShape);
  if((_fsSel0.kind==='fill'||_fsSel0.kind==='fillregion')&&p.fillColor&&!isBrushShape){
    var fc=colorHex8(p.fillColor);
    document.getElementById('fill-well').style.background=fc;document.getElementById('pm-fill').style.background=fc;
    ['color-fill','pm-fill-c'].forEach(function(id){var el=document.getElementById(id);if(el){el.value=fc;el.dataset.hex8=fc;}});
    // Was only removing the toggle's 'off' CSS class here (a display-only
    // fix, per this function's own header comment) — leaving the actual
    // state.fillEnabled untouched meant the icon could show "on" while the
    // NEXT drawn stroke still used a stale, possibly-off state.fillEnabled
    // underneath it — icon and reality visibly disagreeing (reported: "des
    // conflits" on Fill/Stroke enabled state). Now both move together.
    state.fillEnabled=true;
    if(ftog)ftog.classList.remove('off');
  }else if(_fsSel0.kind==='stroke'&&p.strokeColor){
    var sc=colorHex8(p.strokeColor);
    document.getElementById('stroke-well').style.background=sc;document.getElementById('pm-stroke').style.background=sc;
    ['color-stroke','pm-stroke-c'].forEach(function(id){var el=document.getElementById(id);if(el){el.value=sc;el.dataset.hex8=sc;}});
    state.strokeEnabled=true;
    window.SM._syncStrokeEnabledUI(true);
  }
}
// ---- GHOST ALL: turn the visual-only ghosts into real, editable, jointly-
// selected proxy objects (see app.js's _writeBackGhostProxies for how their
// edits get routed back to each proxy's own frame instead of the current one).
var _ghostProxyActive=false;
function selectGhostAll(){
  // Used to silently no-op unless Ghost All (btn-ghost-all) had already
  // been turned on first — an invisible prerequisite (the button looked
  // identically clickable either way, `disabled` had no CSS styling of its
  // own) that read as "le bouton n'a pas l'air de marcher" (2026-07). Now
  // self-enables Ghost All instead of requiring that separate step —
  // same side effects toggleGhostAll's own "turning on" branch has.
  var li=state.activeLayerIdx,cf=state.currentFrame;
  var ld=state.layers[li];if(!ld||ld.symbolId){showToast('Ghost All ne fonctionne pas sur un composant');return;}
  if(!state.ghostAllFrames){
    state.ghostAllFrames=true;
    renderOS();
    var gb=document.getElementById('btn-ghost-all');if(gb)gb.classList.add('active');
  }
  window.SM.setTool('select');
  var layer=userLayers[li];
  pushUndo();
  var count=0,frameCount=0;
  for(var fi=0;fi<ld.frames.length;fi++){
    if(fi===cf)continue;
    var fr=ld.frames[fi];if(!fr.isKeyframe||!fr.strokes.length)continue;
    frameCount++;
    fr.strokes.forEach(function(sd){
      if(sd.isRaster)return;
      // A frozen review ghost (revision-bridge.js) is a static "before"
      // record, never meant to move — without this it became a live,
      // draggable proxy right alongside real content (2026-07 audit),
      // unlike select-bridge.js's own ghost-vs-active-revision handling
      // elsewhere, which always treats ghosts as non-interactive.
      if(sd.isRevisionGhost)return;
      var p=desP(sd,layer);
      p.data.ghostFrame=fi;
      count++;
    });
  }
  _ghostProxyActive=count>0;
  selectedPaths=layer.children.filter(function(c){return c instanceof Path&&isSelectablePathChild(c);});
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
    var d=new Point(dx,dy);
    p.translate(d);
    transformFillGradient(p,function(pt){return pt.add(d);});
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
    transformFillGradient(p,function(pt){return new Point(anchor.x+(pt.x-anchor.x)*sx,anchor.y+(pt.y-anchor.y)*sy);});
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
    transformFillGradient(p,function(pt){return pt.rotate(deltaDeg,center);});
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

// Every render below empties #frame-grid / #layer-list with innerHTML='',
// which drops both containers' scroll position to 0 — and since a render
// fires after almost every edit (a bar drag, a keyframe move, a property
// change), the timeline kept snapping back to the top mid-gesture.
// Reported 2026-07-25 with before/after screenshots: "je déplace les out
// point de calque et là tout descend d'un coup, il faut laisser en place".
// Horizontal scroll had the same problem, just less visible because the
// jump only shows once you've scrolled along the timeline.
//
// Captured before the wipe, restored after the content is back — the
// restore has to run while the new rows already give the container its
// full scrollHeight, or the assignment is clamped to a stale (smaller)
// maximum and quietly lands short.
// #playhead is position:absolute inside #fg-wrap, so it scrolls WITH the
// content: as soon as you scrolled down, its top edge — and with it the
// numbered flag that is the whole grab affordance — slid up out of the
// viewport, and the line looked cut short (2026-07-25: "le timecursor
// disparaît quand on scroll", with a before/after screenshot).
// Pinning it to the viewport instead: top follows scrollTop, height is the
// visible height, so the flag always sits at the ruler and the line always
// spans exactly what you can see. Only the VERTICAL axis — `left` stays
// content-relative (currentFrame*FC), which is what makes it scroll
// correctly sideways with the frames it points at.
function syncPlayheadToViewport(){
  var wrap=document.getElementById('fg-wrap'),ph=document.getElementById('playhead');
  if(!wrap||!ph)return;
  ph.style.top=wrap.scrollTop+'px';
  ph.style.height=Math.max(0,wrap.clientHeight)+'px';
}
(function bindPlayheadScrollSync(){
  function bind(){
    var wrap=document.getElementById('fg-wrap');
    if(!wrap)return;
    wrap.addEventListener('scroll',syncPlayheadToViewport);
    syncPlayheadToViewport();
  }
  if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',bind);else bind();
})();
function _tlScrollSnapshot(){
  var wrap=document.getElementById('fg-wrap'),panel=document.getElementById('layer-list');
  return {wrap:wrap,panel:panel,
    top:wrap?wrap.scrollTop:0,left:wrap?wrap.scrollLeft:0,
    panelTop:panel?panel.scrollTop:0};
}
function _tlScrollRestore(s){
  if(!s)return;
  if(s.wrap){s.wrap.scrollTop=s.top;s.wrap.scrollLeft=s.left;}
  if(s.panel)s.panel.scrollTop=s.panelTop;
}
function renderTimeline(){
  var _scroll=_tlScrollSnapshot();
  var hdr=document.getElementById('frame-hdr'),grid=document.getElementById('frame-grid');hdr.innerHTML='';grid.innerHTML='';
  var fps=Math.max(1,state.fps);
  for(var i=0;i<state.totalFrames;i++){
    var c=document.createElement('div');c.className='fhc';if(i===state.currentFrame)c.classList.add('cur');
    // Second-boundary ticks (Animate-style ruler: "1s"/"2s" labels take
    // priority over the plain every-5-frames tick when they land on the
    // same cell) — gives a time-based read of the timeline, not just frames.
    // `i` is the 0-BASED frame index, and frame index 0 is time zero — so one
    // second is at index `fps`, not `fps-1` (2026-07-25). The old
    // `(i+1)%fps===0` marked one frame early at every boundary: at 24fps "1s"
    // sat on index 23, which is 23/24 = 0.958s.
    //
    // It was the only place in the app that read time that way. Every other
    // frame-to-seconds conversion — the camera export, video import, and
    // reference-bridge's own video seek — uses frame/fps, so the ruler was
    // announcing "1s" at the exact frame where the reference video would be
    // parked at 0.958s. Anyone timing to that reference was off by a frame.
    //
    // The plain every-5 tick keeps showing the 1-based frame NUMBER (i+1),
    // which is what the rest of the UI displays — that part was never wrong.
    if(i>0&&i%fps===0){c.classList.add('sec');c.textContent=Math.round(i/fps)+'s';}
    else if((i+1)%5===0)c.textContent=i+1;
    hdr.appendChild(c);
  }
  // Bug found 2026-07 ("pourquoi visuellement y a cette séparation du noir
  // au gris, il faut prolongé le noir jusqu'au bout"): #bars-row (the
  // onion-skin/work-area strip above the ruler) has no normal-flow
  // children of its own — #onion-bar/.onion-marker/#wa-bar are all
  // absolutely positioned, so they don't contribute to its intrinsic
  // width. #frame-hdr's own width is established naturally by its real
  // .fhc children (one per frame, just appended above), which stretch the
  // whole scrollable area out to state.totalFrames*FC — #bars-row never
  // got that same width, so its dark background stopped wherever it
  // happened to shrink-to-fit while the ruler (and whatever's behind
  // #bars-row) kept going, showing through as a lighter seam partway
  // across. Sizing it explicitly to match fixes the seam.
  var barsRow=document.getElementById('bars-row');
  if(barsRow)barsRow.style.width=(state.totalFrames*FC)+'px';
  // #frame-hdr needs the SAME explicit width, for the same reason — the
  // comment above claimed flexbox sized it from its .fhc children, but it
  // is a flex-direction:column CHILD of #fg-wrap, so its width is its CROSS
  // axis and follows the container, not the content. Measured: 779px box
  // around 844px of cells. The ruler's own background therefore stopped
  // partway across, and whatever sits behind it showed through at ruler
  // height once scrolled — reported 2026-07-25 as "des layers qui
  // apparaissent au niveau des rulers de timing en haut à droite". Exactly
  // the #bars-row and #frame-grid bug, in the one place it was assumed not
  // to apply.
  hdr.style.width=(state.totalFrames*FC)+'px';
  // Bug found 2026-07 ("le highlight d'un layer selectionné ne va pas
  // jusqu'au bout de la timeline") — same root cause family as #bars-row
  // above, one level down: #frame-grid is `flex-direction:column`, so
  // WIDTH is its CROSS axis, not its main axis. A column flex container's
  // auto cross-size follows normal BLOCK sizing (fill the containing
  // block, i.e. #fg-wrap's ~viewport width) — unlike #frame-hdr
  // (flex-direction:row, width IS the main axis, where flexbox's own
  // auto-sizing naturally fits the summed .fhc children, no JS needed).
  // Every `.frow` row then gets stretched (align-items:stretch, the flex
  // default) to that SAME too-narrow width, even though its own .fc
  // children total state.totalFrames*FC — the cells still render past the
  // row's box edge (visible overflow, so the grid itself LOOKS complete),
  // but a `.frow`'s own background (.frow.act's selected-layer tint,
  // .frow.camrow, etc.) only paints within that box, so it visibly cuts
  // off wherever the viewport happened to end at last layout, not at the
  // timeline's real end. Confirmed live: `.frow.act` measured exactly
  // #fg-wrap's clientWidth instead of state.totalFrames*FC before this
  // fix. Sized explicitly here, exactly like #bars-row above it.
  grid.style.width=(state.totalFrames*FC)+'px';
  // Bug found 2026-07 ("la barre de scroll doit pouvoir aller d'un bout à
  // l'autre... pas d'offset derrière"): the custom zoom scrollbar
  // (timeline-zoom.js) sizes its thumb off state.totalFrames but only
  // ever resynced itself from its OWN zoom/pan/resize/scroll handlers —
  // any totalFrames change coming from elsewhere (frame insert/delete,
  // project load) left it stale against the real content width.
  // renderTimeline() already runs after every such change, so resync it
  // here once instead of chasing every individual call site.
  if(window.SMTimelineZoom)SMTimelineZoom.redraw();
  window.updateWaBar&&window.updateWaBar();
  // Motion mode has its own row structure entirely (property tracks, not
  // per-layer frame cells) — same ruler above, different grid content
  // below. Early return, same pattern camera.js's own row already used.
  // Bug found 2026-07 ("le curseur de temps ne fonctionne pas" in Motion):
  // this early return skipped the #playhead left/height update below
  // (line ~1338) entirely, so click/drag-to-scrub on #frame-hdr (ui.js)
  // still moved state.currentFrame and the canvas correctly, but the
  // visible playhead line in the timeline stayed frozen at its old spot.
  // Motion's row structure varies (filtered properties, variable track
  // counts) so there's no fixed rowCount formula to reuse — grid.scrollHeight
  // reads the ACTUAL rendered content height after renderTimelineMotion
  // populates it, which stays correct regardless of how many tracks render.
  if(state.appMode==='motion'){
    if(window.SMMotion)SMMotion.renderTimelineMotion(grid);
    var mph=document.getElementById('playhead');
    mph.style.left=playheadLeftPx(state.currentFrame)+'px';
    // Bug found 2026-07 ("la valeur de frame ne change pas dans motion"):
    // this branch positioned the playhead LINE but never touched
    // #playhead-flag's own text — only the Animation 2D branch further
    // down (unreachable here, this branch `return`s first) sets
    // `playhead-flag.textContent`. The line moved correctly on scrub, but
    // the little frame-number pill on it stayed frozen at whatever it
    // last showed in Animation 2D.
    document.getElementById('playhead-flag').textContent=state.currentFrame+1;
    // Bug found 2026-07 ("le curseur de temps devrait descendre jusqu'au
    // niveau de la scroll bar en bas"): grid.scrollHeight only covers the
    // RENDERED content — with few tracks that's shorter than #fg-wrap's
    // own visible box (flex:1, fills the panel), so the line stopped short
    // of the actual bottom edge/scrollbar with dead space below it. Extend
    // to whichever is taller: content height (so a tall/scrolled track
    // list still gets a fully-covering line) or the wrap's own visible
    // height (so a short list still reaches the panel's bottom).
    var mwrap=document.getElementById('fg-wrap');
    mph.style.height=Math.max(grid.scrollHeight,mwrap?mwrap.clientHeight:0)+'px';
    return;
  }
  if(window.SMCamera)SMCamera.renderGridRow(grid);
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
    // data-layer was Motion-only, so anything addressing "the grid row for
    // layer N" silently found nothing in Animation 2D (2026-07-25: layer
    // markers rendered in Motion and vanished here). Same attribute, same
    // meaning, both modes — the row already knows its index.
    row.dataset.layer=li;
    // Tween curve strips (below) need this row taller than the default
    // ROW_H when it has a tween span to show — plain document flow handles
    // the actual reflow of rows below it for free, no cumulative-height
    // bookkeeping needed HERE. frameGridRowHeight/layerCurveRowExtraHeight
    // are the single source of truth other pixel-math consumers (the
    // cross-layer keyframe-drag target detection below) must also read from.
    var extraH=layerCurveRowExtraHeight(li);
    if(extraH>0)row.style.height=(ROW_H+extraH)+'px';
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
    // The in/out point bar (layer-inout.js) is Motion-mode-only visually —
    // motion.js's own renderTimelineMotion builds it on its own collapsed
    // layer row separately. Animation 2D keeps its classic keyframe-circles
    // look untouched (explicit feedback: the two modes must stay visually
    // distinct, not share overlay chrome) — the in/out DATA and render gate
    // (getEffectiveStrokes, app.js) still fully apply either way, only the
    // bar's on-screen presence here is gone.
    renderKeyframeCellsInto(row,li,contentIdxs);
    grid.appendChild(row);
  });
  // rowCount only counts state.layers rows — the camera row (prepended
  // above, not part of state.layers) never added its own height here, so
  // the playhead line always stopped short by one row whenever a camera
  // track existed.
  // Same "reach the actual bottom/scrollbar" fix as Motion mode above:
  // with few layers the row-count formula is shorter than #fg-wrap's own
  // visible height, leaving dead space below the line before the
  // scrollbar. Math.max keeps the line covering a tall/scrolled layer
  // list too (that case was already correct).
  var awrap=document.getElementById('fg-wrap');
  document.getElementById('playhead').style.left=playheadLeftPx(state.currentFrame)+'px';
  syncPlayheadToViewport();
  document.getElementById('playhead-flag').textContent=state.currentFrame+1;
  // Markers are overlays on rows this function just rebuilt — re-attach.
  if(window.SMMarkers)SMMarkers.render();
  if(window.SMBpm)SMBpm.render();
  if(window.SMAudio)SMAudio.renderStrip();
  renderTweenCurveStrips();
  // renderTimeline() wipes #frame-grid, so the graph editor — which hides that
  // grid and draws over the same box — has to be re-applied after every
  // rebuild, or toggling zoom/frames silently drops it.
  if(window.SMMotionGraph&&SMMotionGraph.isOn()){
    document.getElementById('frame-grid').style.visibility='hidden';
    SMMotionGraph.render();
  }
  _tlScrollRestore(_scroll);
}
// ---- TWEEN EASING CURVE STRIPS (toggle: btn-tween-curves) ----
// Purely additive display, complements the global/per-pair easing system —
// never touched by generateTweens(). Was originally a thin 8px sparkline
// per tween span; per explicit request ("j'aimerais que ça soit
// visuellement dans le genre ça ouvre le panneau avec les courbes mais pour
// toutes les clés de la timeline") it's now the SAME rich draggable-point
// editor as the floating popup (openTweenCurveInset, below), but persistent
// and inline — every tween span on a qualifying layer's row, all at once,
// no click needed to open anything. Only layers that actually have a tween
// span grow taller to fit it (TWEEN_ROW_EXTRA_H); a layer with no tween
// (e.g. no keyframes generated between its keys) stays at the normal
// ROW_H, non-overlapping, per the reference screenshot.
function layerKeyframeList(li){
  var ld=state.layers[li];if(!ld)return[];
  var keys=[];
  for(var i=0;i<state.totalFrames;i++)if(ld.frames[i]&&ld.frames[i].isKeyframe)keys.push(i);
  return keys;
}
// [fA,fB] pairs of consecutive keyframes on layer li with at least one
// generated isInterpolated frame between them — the single definition of
// "this layer has a tween to show", shared by the row-height decision below
// and the strip renderer, so the two can never drift out of phase.
function layerTweenSpans(li){
  var ld=state.layers[li];if(!ld)return[];
  var keys=layerKeyframeList(li);
  var spans=[];
  for(var k=0;k<keys.length-1;k++){
    var fA=keys[k],fB=keys[k+1];
    var hasTween=false;
    for(var fi=fA+1;fi<fB;fi++){if(ld.frames[fi]&&ld.frames[fi].isInterpolated){hasTween=true;break;}}
    if(hasTween)spans.push([fA,fB]);
  }
  return spans;
}
function layerHasTweenSpans(li){return layerTweenSpans(li).length>0;}
var TWEEN_ROW_EXTRA_H=92;
// Extra pixel height a layer's .frow/.lrow reserves for its curve strip —
// 0 unless the toggle is on AND this layer actually has a tween to show.
function layerCurveRowExtraHeight(li){
  if(!state.showTweenCurves)return 0;
  return layerHasTweenSpans(li)?TWEEN_ROW_EXTRA_H:0;
}
// A layer's CURRENT total row height in #frame-grid — the single source of
// truth every pixel-math consumer below (and renderTimeline's own row
// height, and renderLayerList's mirrored .lrow) must read from. Reduces to
// the plain ROW_H constant whenever no row is taller, so anything built on
// top of it is byte-for-byte identical to before this feature existed in
// that (default) case.
function frameGridRowHeight(li){return ROW_H+layerCurveRowExtraHeight(li);}
// Cumulative pixel offset from the grid's top to the TOP of layer li's own
// row. Rows render top-to-bottom from the HIGHEST layer index (see
// computeLayerRenderOrder/renderTimeline's own comment), and a row can now
// be taller than ROW_H, so this can no longer be a flat li*ROW_H
// multiplication — this + layerIndexAtGridY below replace that formula
// everywhere a pixel position needs to know which/where a layer's row is,
// WITHOUT touching the rows' own rendering (that's plain document flow,
// the browser reflows it for free when a .frow's height changes).
function visualTopOfLayer(li){
  var n=state.layers.length,acc=0;
  for(var li2=n-1;li2>li;li2--)acc+=frameGridRowHeight(li2);
  return acc;
}
function frameGridTotalRowsHeight(){
  var n=state.layers.length,total=0;
  for(var i=0;i<n;i++)total+=frameGridRowHeight(i);
  return total;
}
// Reverse of visualTopOfLayer: which layer's row a given yRel (pixels from
// the grid top, past the camera row) falls in. Replaces
// `state.layers.length-1-Math.floor(yRel/ROW_H)` — identical result when
// every row is still ROW_H tall.
function layerIndexAtGridY(yRel){
  var n=state.layers.length;
  if(n===0)return 0;
  var y=Math.max(0,yRel),acc=0;
  for(var p=0;p<n;p++){
    var li=n-1-p,h=frameGridRowHeight(li);
    if(y<acc+h)return li;
    acc+=h;
  }
  return 0;
}
function renderTweenCurveStrips(){
  var grid=document.getElementById('frame-grid');
  if(!grid)return;
  var old=grid.querySelectorAll('.tw-curve-strip');
  for(var oi=0;oi<old.length;oi++)old[oi].remove();
  if(!state.showTweenCurves)return;
  var rows=grid.querySelectorAll('.frow');
  rows.forEach(function(row){
    var firstCell=row.querySelector('.fc');
    if(!firstCell||firstCell.dataset.layer===undefined)return;
    var li=parseInt(firstCell.dataset.layer);
    if(isNaN(li)||!state.layers[li])return;
    var spans=layerTweenSpans(li);
    if(!spans.length)return;
    var wrap=document.createElement('div');
    wrap.className='tw-curve-strip';
    wrap.style.cssText='position:absolute;left:0;bottom:0;width:'+(state.totalFrames*FC)+'px;height:'+TWEEN_ROW_EXTRA_H+'px;background:rgba(0,0,0,.18);border-top:1px solid rgba(255,255,255,.08);';
    row.style.position='relative';
    row.appendChild(wrap);
    spans.forEach(function(span){
      var fA=span[0],fB=span[1];
      var x0=fA*FC,w=(fB-fA)*FC;
      var box=document.createElement('div');
      box.className='tw-curve-strip-box';
      box.style.cssText='position:absolute;left:'+x0+'px;top:0;width:'+w+'px;height:100%;box-sizing:border-box;border-right:1px solid rgba(255,255,255,.06);';
      var pad=6;
      // Same builder as the floating popup (openTweenCurveInset) — one
      // implementation of the drag/redraw/regen logic, see its own header
      // comment.
      var built=buildTweenCurveSVG(li,fA,fB,Math.max(20,w-2*pad),TWEEN_ROW_EXTRA_H-2*pad);
      built.svg.style.position='absolute';built.svg.style.left=pad+'px';built.svg.style.top=pad+'px';
      box.appendChild(built.svg);
      wrap.appendChild(box);
    });
  });
}
window.renderTweenCurveStrips=renderTweenCurveStrips;
// ---- SHARED CURVE-EDITOR SVG (draggable control points) ----
// Builds the actual draggable-point curve editor surface — used by BOTH the
// persistent inline strips above and the floating popup below, so there is
// exactly one implementation of the drag/redraw/regen logic (CLAUDE.md §3:
// duplicated logic drifts out of phase). Reads/writes the same
// state.tweenEasing[li+':'+fA+'-'+fB] entry either way, so editing a span
// inline and via the popup (right-click a cell → "Éditer la courbe de ce
// tween…", still available when the toggle is off) always agree.
function buildTweenCurveSVG(li,fA,fB,svgW,svgH){
  var key=li+':'+fA+'-'+fB;
  if(!state.tweenEasing)state.tweenEasing={};
  var seg=state.tweenEasing[key]=state.tweenEasing[key]||{};
  if(!seg.points||!seg.points.length){
    // Starts from whatever curve is CURRENTLY effective for this pair
    // (global fallback), not a hardcoded default.
    var base=(window._curveEditor&&window._curveEditor.getState().points)||[{x:0,y:0},{x:.42,y:0},{x:.58,y:1},{x:1,y:1}];
    seg.points=base.map(function(p){return{x:p.x,y:p.y};});
  }
  var svgNS='http://www.w3.org/2000/svg';
  var svg=document.createElementNS(svgNS,'svg');
  svg.setAttribute('width',svgW);svg.setAttribute('height',svgH);
  svg.style.cssText='display:block;cursor:crosshair;touch-action:none;';

  var ipad=6;
  function toSX(px){return ipad+px*(svgW-2*ipad);}
  function toSY(py){return svgH-ipad-py*(svgH-2*ipad);}
  function fromSX(sx){return Math.max(0,Math.min(1,(sx-ipad)/(svgW-2*ipad)));}
  function fromSY(sy){return Math.max(-0.3,Math.min(1.3,(svgH-ipad-sy)/(svgH-2*ipad)));}

  function redraw(){
    while(svg.firstChild)svg.removeChild(svg.firstChild);
    [0,1].forEach(function(v){
      var l=document.createElementNS(svgNS,'line');
      l.setAttribute('x1',ipad);l.setAttribute('x2',svgW-ipad);l.setAttribute('y1',toSY(v));l.setAttribute('y2',toSY(v));
      l.setAttribute('stroke','#334155');l.setAttribute('stroke-dasharray','3,3');
      svg.appendChild(l);
    });
    var pts=seg.points;
    var poly=document.createElementNS(svgNS,'polyline');
    var N=48,ptsStr=[];
    for(var s=0;s<=N;s++){var t=s/N;var yv=window._curveEditor?window._curveEditor.evalPointsCurve(pts,t):t;ptsStr.push(toSX(t)+','+toSY(yv));}
    poly.setAttribute('points',ptsStr.join(' '));
    poly.setAttribute('fill','none');poly.setAttribute('stroke','#4a9eff');poly.setAttribute('stroke-width','2');
    svg.appendChild(poly);
    pts.forEach(function(p,i){
      var c=document.createElementNS(svgNS,'circle');
      c.setAttribute('cx',toSX(p.x));c.setAttribute('cy',toSY(p.y));c.setAttribute('r',5.5);
      c.setAttribute('fill','#fff');c.setAttribute('stroke','#4a9eff');c.setAttribute('stroke-width','2');
      c.style.cursor='grab';
      c.addEventListener('pointerdown',function(e){
        e.stopPropagation();e.preventDefault();
        c.setPointerCapture(e.pointerId);
        function move(ev){
          var r=svg.getBoundingClientRect();
          var nx=fromSX(ev.clientX-r.left),ny=fromSY(ev.clientY-r.top);
          if(i===0)nx=0;else if(i===pts.length-1)nx=1;
          else{nx=Math.max(pts[i-1].x+0.001,Math.min(pts[i+1].x-0.001,nx));}
          p.x=nx;p.y=ny;
          redraw();
          scheduleRegen();
        }
        function up(ev){c.releasePointerCapture(ev.pointerId);svg.removeEventListener('pointermove',move);svg.removeEventListener('pointerup',up);regenNow();}
        svg.addEventListener('pointermove',move);svg.addEventListener('pointerup',up);
      });
      c.addEventListener('dblclick',function(e){
        e.stopPropagation();
        if(i>0&&i<pts.length-1&&pts.length>2){pts.splice(i,1);redraw();regenNow();}
      });
      svg.appendChild(c);
    });
  }
  // Regenerating the tween on every single pointermove tick during a drag
  // would re-run autoMatch/interpStroke dozens of times per second for no
  // visible benefit (the drag preview only needs the CURVE redrawn live —
  // regen only needs to happen once the point settles). Debounced instead
  // of per-move, then a final regenNow() on pointerup guarantees the last
  // position is always applied even if the debounce hasn't fired yet.
  var regenTimer=null;
  function scheduleRegen(){clearTimeout(regenTimer);regenTimer=setTimeout(regenNow,120);}
  function regenNow(){clearTimeout(regenTimer);onTweenPairCurveChanged(li,fA);}
  svg.addEventListener('click',function(e){
    if(e.target!==svg)return;
    var r=svg.getBoundingClientRect();
    var nx=fromSX(e.clientX-r.left),ny=fromSY(e.clientY-r.top);
    var pts=seg.points,idx=pts.length;
    for(var i2=0;i2<pts.length;i2++)if(pts[i2].x>nx){idx=i2;break;}
    pts.splice(idx,0,{x:nx,y:ny});
    redraw();regenNow();
  });
  redraw();
  return {svg:svg,redraw:redraw,seg:seg};
}
// ---- FLOATING INLINE CURVE EDITOR ("encart") for one tween span ----
// A self-contained draggable-point curve editor (independent of the
// shared #curve-canvas singleton in ui.js, which can only show ONE curve
// at a time and lives in a fixed right-panel spot) — appended to
// document.body so it truly floats ON TOP of the timeline regardless of
// #frame-grid's own scroll/overflow clipping, anchored right where the
// clicked tween segment is. Same underlying data (state.tweenEasing) and
// evaluator (evalPointsCurve) as the main widget, so both stay in sync;
// this is just a second, quicker editing surface for in-place tweaks.
var _tweenCurveInset=null;
function closeTweenCurveInset(){
  if(!_tweenCurveInset)return;
  document.removeEventListener('pointerdown',_tweenCurveInset.outsideHandler,true);
  _tweenCurveInset.el.remove();
  _tweenCurveInset=null;
}
// Finds the DOM row for layer `li` in the frame-grid — used to align the
// inset to the tween span's actual on-screen position (not a fixed popup
// size dropped near a click point, per explicit follow-up: "il faut
// vraiment que ça soit une partie distincte qui s'ouvre dessous... par un
// encart", with the reference screenshot showing the curve box spanning
// EXACTLY the keyframe-to-keyframe width, right under that layer's row).
function findLayerRow(li){
  var rows=document.querySelectorAll('#frame-grid .frow');
  for(var i=0;i<rows.length;i++){
    var fc=rows[i].querySelector('.fc');
    if(fc&&parseInt(fc.dataset.layer,10)===li)return rows[i];
  }
  return null;
}
function openTweenCurveInset(li,fA,fB){
  closeTweenCurveInset();
  var row=findLayerRow(li);
  if(!row)return;
  var key=li+':'+fA+'-'+fB;
  var rowRect=row.getBoundingClientRect();
  var HDR=20,pad=10;
  var W=Math.max(140,(fB-fA)*FC);
  var H=110;
  var left=Math.max(4,Math.min(window.innerWidth-W-4,rowRect.left+fA*FC));
  var top=rowRect.bottom+2;
  var svgW=W-2*pad,svgH=H-HDR-pad;
  var box=document.createElement('div');
  box.className='tw-curve-inset';
  box.style.cssText='position:fixed;left:'+Math.round(left)+'px;top:'+Math.round(top)+'px;width:'+W+'px;height:'+H+'px;background:rgba(13,17,23,.85);border:2px solid #4a9eff;border-radius:3px;box-shadow:0 6px 24px rgba(0,0,0,.55);z-index:5000;padding:'+pad+'px;box-sizing:border-box;font-family:inherit;';
  box.innerHTML='<div style="display:flex;justify-content:space-between;align-items:center;height:'+HDR+'px">'
    +'<span style="font-size:10px;color:#9ca3af">Tween '+(fA+1)+' → '+(fB+1)+'</span>'
    +'<div style="display:flex;gap:8px;align-items:center">'
    +'<button class="tw-ci-reset" style="background:none;border:none;color:#9ca3af;cursor:pointer;font-size:9px;text-decoration:underline;padding:0" title="Revenir à la courbe globale pour cette paire">réinitialiser</button>'
    +'<button class="tw-ci-close" style="background:none;border:none;color:#9ca3af;cursor:pointer;font-size:14px;line-height:1;padding:0" title="Fermer">✕</button>'
    +'</div></div>';
  box.querySelector('.tw-ci-close').addEventListener('click',function(e){e.stopPropagation();closeTweenCurveInset();});
  box.querySelector('.tw-ci-reset').addEventListener('click',function(e){
    e.stopPropagation();
    delete state.tweenEasing[key];
    closeTweenCurveInset();
    onTweenPairCurveChanged(li,fA);
  });
  // Same builder as the persistent inline strips (renderTweenCurveStrips) —
  // see its header comment for why this is shared rather than duplicated.
  var built=buildTweenCurveSVG(li,fA,fB,svgW,svgH);
  box.appendChild(built.svg);
  document.body.appendChild(box);
  var outsideHandler=function(e){if(!box.contains(e.target))closeTweenCurveInset();};
  setTimeout(function(){document.addEventListener('pointerdown',outsideHandler,true);},0);
  _tweenCurveInset={el:box,outsideHandler:outsideHandler};
}
// Shared by the inset above and the main widget's tween-pair mode (see
// editTweenSegForCell) — regenerate just this span and refresh every
// display that could be showing it (strips, onion, arcs).
function onTweenPairCurveChanged(li,fA){
  selClear();selAdd(li,fA);
  generateTweens();
  selClear();
  if(state.activeLayerIdx===li)renderOS();
  updateUI();
  renderTweenCurveStrips();
}
// Which keyframe pair (of layer li's own keyframe list) frame `fi` falls
// inside — same lookup used by both the frame-grid click handler and its
// context menu, kept as one function so they can't disagree about which
// span a given cell belongs to.
function tweenPairForCell(li,fi){
  var keys=layerKeyframeList(li);
  for(var i=0;i<keys.length-1;i++){if(fi>=keys[i]&&fi<=keys[i+1])return{a:keys[i],b:keys[i+1]};}
  return null;
}
// Puts the shared Easing Curve widget (ui.js's #curve-canvas) into
// tween-pair mode scoped to whichever span this cell belongs to, instead
// of leaving it on the global default curve every span without its own
// override shares (the actual bug this whole helper exists to close — see
// this function's call site). No-op (falls back to exitTweenPair via the
// caller) when the cell isn't inside a tween span.
function editTweenSegForCell(li,fi,fr0){
  openPropsSection('tween-sec');openPropsSection('easing-sec');
  var pair=tweenPairForCell(li,fi);
  if(!pair||!window._curveEditor||!window._curveEditor.editTweenPair)return;
  var key=li+':'+pair.a+'-'+pair.b;
  if(!state.tweenEasing)state.tweenEasing={};
  var seg=state.tweenEasing[key]=state.tweenEasing[key]||{};
  window._curveEditor.editTweenPair(seg,'Tween '+(pair.a+1)+' → '+(pair.b+1),function(){onTweenPairCurveChanged(li,pair.a);});
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
  // A component instance is one object on the parent Animation 2D timeline.
  // Its row therefore shows the OUTER placement/blank keys in ld.frames,
  // never the symbol's internal drawing keys. Internal keys belong to the
  // component editor; mirroring them here made one instance look like a
  // collection of unrelated parent keys and made selecting/moving the
  // instance timing ambiguous.
  var sym=ld.symbolId?state.symbols[ld.symbolId]:null;
  function frOf(fi){return ld.frames[fi];}
  // Normally just this layer's own strokes; when contentLayerIdxs is given
  // (collapsed Stroke/Fill/Shadow head row — see renderTimeline's call
  // site), "full" means ANY sibling channel has content at that frame.
  function hasContentAt(fi){
    if(sym){
      var own=ld.frames[fi];
      return !!(own&&own.isKeyframe&&!own.blankOverride);
    }
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
    var fr=frOf(fi);
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
      for(var pi=fi;pi>=0;pi--){var pfr=frOf(pi);if(pfr.isKeyframe){hc=hasContentAt(pi);srcFound=true;break;}}
      if(srcFound){
        cell.classList.add(hc?'span-full':'span-empty');
        var nextFr=(fi+1<state.totalFrames)?frOf(fi+1):null;
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
  // RIPPLE (2026-07-28 feedback, confirmed as a third occurrence of the
  // same bug already fixed for Motion's layer-bar drag and Animation 2D's
  // keyframe-dot drag): committing this trim used to relocate ONLY
  // nextKeyFi, leaving every keyframe after it sitting at its own original
  // frame number — the WHOLE tail should shift by the same amount the
  // handle moved, exactly like dragging a clip edge in a video editor
  // ripples everything downstream of the cut. See the mouseup handler
  // below for the actual move; the bound here just needs to stop being
  // MORE conservative than that new behavior requires — the old cap
  // (stop before the next-next keyframe) existed only because that
  // keyframe used to stay put and could be collided with. Now it moves
  // too, so the true limit is whichever keyframe ends up LAST in the
  // chain, capped by the timeline itself — same proof as moveFrames' own
  // ripple: every rippled frame's ORIGINAL position is > nextKeyFi-1, so
  // adding the identical delta to all of them can never reorder or
  // collide them with each other.
  var dragMax=state.totalFrames-1;
  if(nextKeyFi>=0){
    var lastKeyInChain=nextKeyFi;
    for(var ni=nextKeyFi+1;ni<ld.frames.length;ni++){if(ld.frames[ni].isKeyframe)lastKeyInChain=ni;}
    // Bound is relative to nextKeyFi (what actually moves), NOT the
    // span-end cell's own `fi` (always nextKeyFi-1, the last frame of the
    // CURRENT span) — got this backwards on the first pass, caught before
    // driving it: verified below against a concrete totalFrames example.
    dragMax=nextKeyFi+((state.totalFrames-1)-lastKeyInChain);
  }
  else dragMax=fi; // no bordering keyframe — old shrink-only behavior, capped at the current end
  // Snapshot every keyframe AFTER nextKeyFi ONCE here — not on every
  // mousemove tick — so the live preview (below) can show them riding
  // along with the same delta, matching the ripple this handler's mouseup
  // now actually commits (2026-07-28, "on peut les voir bougé pendant le
  // drag").
  var laterKeys=[];
  if(nextKeyFi>=0)for(var lki=nextKeyFi+1;lki<ld.frames.length;lki++){if(ld.frames[lki].isKeyframe)laterKeys.push(lki);}
  _spanShrink={active:true,li:li,srcFi:srcFi,maxFi:fi,nextKeyFi:nextKeyFi,dragMax:dragMax,laterKeys:laterKeys};
  // Dimmed (not hidden — feedback: "laisser la barre bleue d'outpoint et la
  // clé d'après visuellement pendant le drag") so the original position
  // stays visible as a reference next to the live span-drag-preview.
  cell.classList.add('tl-outdrag-source-end');
  // Trim path (a REAL bordering keyframe is being relocated, not just an
  // abstract span end) — dim ITS dot too for the drag's duration, same
  // reasoning as the square above.
  if(nextKeyFi>=0){
    var nextCell=document.querySelector('.fc[data-layer="'+li+'"][data-frame="'+nextKeyFi+'"]');
    if(nextCell)nextCell.classList.add('tl-outdrag-source-key');
  }
},true);
function _spanShrinkFrameAt(e){
  var wrap=document.getElementById('fg-wrap');var rect=wrap.getBoundingClientRect();
  var x=e.clientX-rect.left+wrap.scrollLeft;
  // +HIT_TOLERANCE_PX before flooring: Math.floor(x/FC) alone puts the
  // ambiguous boundary exactly at each cell's LEFT edge — that's right
  // where a user aiming to "drop on frame N" naturally releases (the left
  // edge is where the cell visually begins/where its own outpoint bar from
  // the PREVIOUS cell sits, at right:2px of it), so a release even 1-2px
  // short of that edge silently landed one frame back (feedback: "je
  // relâche sur la frame 20... il va me le positionné en frame 19"). A
  // few pixels of forward tolerance make reaching the intended cell
  // reliable without meaningfully changing where a deliberate drop several
  // cells earlier lands (FC is only ~16px at the default zoom — an
  // untolerant 1:1 pixel target is unreasonably precise for a mouse drag).
  var HIT_TOLERANCE_PX=4;
  return Math.max(_spanShrink.srcFi+1,Math.min(_spanShrink.dragMax,Math.floor((x+HIT_TOLERANCE_PX)/FC)));
}
window.addEventListener('mousemove',function(e){
  if(!_spanShrink.active)return;
  var fi=_spanShrinkFrameAt(e);
  document.querySelectorAll('.fc.span-drag-band').forEach(function(c){c.classList.remove('span-drag-band');});
  document.querySelectorAll('.fc.span-drag-preview').forEach(function(c){c.classList.remove('span-drag-preview');});
  document.querySelectorAll('.fc.span-drag-dot-cell').forEach(function(c){c.classList.remove('span-drag-dot-cell');});
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
  if(prevCell)prevCell.classList.add('span-drag-preview');
  // Trim path: show the relocating keyframe's OWN dot (full/hollow, matching
  // whatever it'll actually look like once dropped) riding one cell to the
  // RIGHT of the bar — the same adjacency a committed span-end bar + its
  // bordering keyframe dot already have when nothing is being dragged
  // (feedback: "le rond de la keyframe doit être juste à côté à droite").
  // A previous version appended this dot into the SAME cell as the bar
  // (prevCell) — visually overlapping instead of adjacent.
  if(_spanShrink.nextKeyFi>=0){
    // FIX (2026-07-28, found while adding the ripple preview below): this
    // used to show the dot at `fi+1`, one frame LATER than where mouseup
    // actually writes it (`ld.frames[fi]=moved` — confirmed empirically,
    // not just by reading: dragging to a cell dead-center on frame 20
    // showed the preview dot at 20 but committed to 19). The preview must
    // show the SAME frame the drop will actually use, or the ripple
    // preview added below would inherit the identical one-frame lie.
    var ld=state.layers[_spanShrink.li];
    var dotCell=document.querySelector('.fc[data-layer="'+_spanShrink.li+'"][data-frame="'+fi+'"]');
    if(dotCell){
      dotCell.classList.add('span-drag-dot-cell');
      // Still sitting at the keyframe's ORIGINAL spot (fi===nextKeyFi) —
      // its real .km dot is already there (dimmed via tl-outdrag-source-key
      // on mousedown); the opacity override in style.css brings it back to
      // full instead of stacking a redundant second dot on top of it.
      if(fi!==_spanShrink.nextKeyFi){
        var movedFr=ld.frames[_spanShrink.nextKeyFi];
        var full=movedFr&&movedFr.strokes.length>0;
        var dot=document.createElement('div');
        dot.className='km drag-key-preview '+(full?'fl':'hl');
        dotCell.appendChild(dot);
      }
    }
    // Ripple preview: every keyframe AFTER nextKeyFi rides along with the
    // SAME delta (fi-nextKeyFi) this drag would actually apply — without
    // this, only the bordering keyframe appeared to move while the rest of
    // the tail silently teleported the instant the mouse released.
    var delta=fi-_spanShrink.nextKeyFi;
    if(delta!==0){
      _spanShrink.laterKeys.forEach(function(origFi){
        var newFi=origFi+delta;
        if(newFi<0||newFi>=state.totalFrames)return; // would overflow — dragMax already prevents this in practice, defensive only
        var lCell=document.querySelector('.fc[data-layer="'+_spanShrink.li+'"][data-frame="'+newFi+'"]');
        if(!lCell)return;
        lCell.classList.add('span-drag-dot-cell');
        var lFr=ld.frames[origFi];
        var lFull=lFr&&lFr.strokes.length>0;
        var lDot=document.createElement('div');
        lDot.className='km drag-key-preview '+(lFull?'fl':'hl');
        lCell.appendChild(lDot);
      });
    }
  }
});
window.addEventListener('mouseup',function(e){
  if(!_spanShrink.active)return;
  document.querySelectorAll('.fc.span-drag-band').forEach(function(c){c.classList.remove('span-drag-band');});
  document.querySelectorAll('.fc.span-drag-preview').forEach(function(c){c.classList.remove('span-drag-preview');});
  document.querySelectorAll('.fc.span-drag-dot-cell').forEach(function(c){c.classList.remove('span-drag-dot-cell');});
  document.querySelectorAll('.km.drag-key-preview').forEach(function(k){k.remove();});
  // Explicit cleanup, not just "a re-render will replace it" — several
  // branches below return early on a no-op drop (dropped back on the
  // source, or back on the already-existing end) with no re-render at all,
  // which would otherwise leave the original marker permanently dimmed.
  document.querySelectorAll('.fc.tl-outdrag-source-end').forEach(function(c){c.classList.remove('tl-outdrag-source-end');});
  document.querySelectorAll('.fc.tl-outdrag-source-key').forEach(function(c){c.classList.remove('tl-outdrag-source-key');});
  var fi=_spanShrinkFrameAt(e);
  var li=_spanShrink.li,nextKeyFi=_spanShrink.nextKeyFi;
  _spanShrink.active=false;
  if(fi<=_spanShrink.srcFi)return; // dropped back on the source key — no-op
  if(nextKeyFi>=0){
    if(fi===nextKeyFi)return; // dropped right back where it already was
    pushUndo();saveAllLayerFrames();
    var ld=state.layers[li];
    var delta=fi-nextKeyFi;
    // RIPPLE (2026-07-28 feedback — third occurrence of the bug already
    // fixed for Motion's layer-bar drag and Animation 2D's keyframe-dot
    // drag): every keyframe AFTER nextKeyFi moves by the SAME delta, not
    // just nextKeyFi itself — otherwise the tail stays put while the span
    // in front of it changes length, which is what read as "only the next
    // key moves, not the ones after it". Every rippled frame's ORIGINAL
    // position is > nextKeyFi (checked at mousedown, before this delta was
    // known), so adding the identical delta to all of them can't reorder
    // or collide them with each other — same proof as moveFrames' ripple.
    // Capture-blank-write in three separate passes (not one combined loop)
    // for the same reason moveFrames does it that way: with delta>0 a
    // naive in-order write could stomp a not-yet-read LATER frame whose
    // target position coincides with an EARLIER frame's source position.
    var toMove=[nextKeyFi];
    for(var ri=nextKeyFi+1;ri<ld.frames.length;ri++){if(ld.frames[ri].isKeyframe)toMove.push(ri);}
    var captured=toMove.map(function(f){return {from:f,to:f+delta,content:ld.frames[f]};});
    captured.forEach(function(c){ld.frames[c.from]={strokes:[],isKeyframe:false,isInterpolated:false};});
    captured.forEach(function(c){if(c.to>=0&&c.to<ld.frames.length)ld.frames[c.to]=c.content;});
    // Tween/arc rekeying is NOT attempted here — the ORIGINAL (pre-fix)
    // handler never called captureTweenInbetweens/rekeyTweenPairData for
    // this specific gesture either (unlike moveFrames, which already did),
    // so any interpolated spans crossing a rippled keyframe were already
    // silently orphaned before this change. Out of scope for "fix the
    // ripple" — flagged here rather than silently left unfixed AND
    // undocumented.
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
  // Deselecting every layer (clicking the empty background below the rows,
  // either in the left layer-panel list or the timeline grid area) — was
  // previously impossible, a plain row click always left exactly one layer
  // in _layerSel and there was no gesture to clear it back to empty. Needed
  // for F5/insertFrame (and F6/insertKeyframe) to be able to tell "the user
  // deliberately picked no layer, apply to ALL of them" apart from "some
  // specific layer(s) are picked, apply to only those" — previously
  // insertFrame() ignored _layerSel entirely and always hit every layer
  // regardless of selection (reported: "peu importe le layer sélectionné
  // ça ajoute des keyframes à tous"). Excludes actual rows/folder headers
  // and the layer-panel's own buttons (new/delete/duplicate/camera/audio…)
  // so this never fires from clicking something that already has its own
  // meaning. state.activeLayerIdx (which layer NEW STROKES draw onto) is
  // deliberately left untouched — that must always stay a valid layer for
  // drawing tools to work; only the batch-operation selection (_layerSel,
  // the .sel highlight) clears here, not the .act "current drawing target"
  // highlight, so a plain click straight back onto a row still un-ambiguously
  // resumes drawing on whatever was active before.
  if(e.target.closest('.lrow,.frow,#layer-ctrls,button'))return;
  if(_layerSel.length){_layerSel=[];renderLayerList();}
});

// Keeps _layerSel (the panel's batch-operation selection — now also what
// F5/F6 target, see app.js) in lockstep with whichever layer(s) actually
// have a selected frame cell right now. Without this, clicking a keyframe
// in Layer 2 right after Layer 1 left Layer 1's row STILL showing selected
// (reported: "si je select une keyframe dans layer 1 et après select une
// autre keyframe dans layer 2 sans shift alors le layer 1 reste select") —
// _layerSel was only ever written by an explicit layer-PANEL row click,
// never by clicking a frame cell, so a stale panel selection from earlier
// silently outlived it and (now that F5/F6 read _layerSel) could target
// the wrong layer entirely. Recomputed as the exact set of layers spanned
// by _sel.frames, so a plain click collapses to just that one layer, a
// shift-range or ctrl-toggle spanning several layers targets exactly those.
function syncLayerSelFromFrameSel(){
  var layers=[];
  _sel.frames.forEach(function(s){if(layers.indexOf(s.layer)<0)layers.push(s.layer);});
  layers.sort(function(a,b){return a-b;});
  _layerSel=layers;
  renderLayerList();
}
document.getElementById('frame-grid').addEventListener('mousedown',function(e){
  if(e.button!==0)return;
  var cell=e.target.closest('.fc');if(!cell)return;
  var fi=parseInt(cell.dataset.frame),li=parseInt(cell.dataset.layer);
  if(state.playing)stopPlay();

  if(e.shiftKey){
    selRange(li,fi);selApplyCSS();syncLayerSelFromFrameSel();
    if(li!==state.activeLayerIdx)window.SM.setActiveLayer(li);
    goToFrame(fi);return;
  }
  if(e.metaKey||e.ctrlKey){
    selToggle(li,fi);selApplyCSS();syncLayerSelFromFrameSel();
    if(li!==state.activeLayerIdx)window.SM.setActiveLayer(li);
    goToFrame(fi);return;
  }

  // Grabbing a keyframe's own dot directly (2026-07, "petit carré
  // draggable") starts a MOVE drag on the very first press — no separate
  // prior click-to-select needed first, matching Animate/AE's keyframe-
  // diamond drag convention (reuses the exact same _tlDrag/moveFrames
  // machinery the "drag an already-selected cell" path below already had;
  // this just skips straight to it when the grab target is the dot).
  // Scoped to real keyframes (kf-full/kf-empty — not the .td tween tick)
  // on a NON-Component layer: a Component layer's own ld.frames is just a
  // placement stub (see renderKeyframeCellsInto's symbolId branch), so
  // moving it wouldn't relocate any real content — those markers stay
  // visual-only here, moving the symbol's OWN inner keyframes would need a
  // dedicated symbol-timeline UI that doesn't exist yet.
  var grabbedDot=e.target.closest('.km')&&(cell.classList.contains('kf-full')||cell.classList.contains('kf-empty'));
  var ldForDot=state.layers[li];
  if(grabbedDot&&ldForDot&&!ldForDot.symbolId){
    if(!selHas(li,fi)){selClear();selAdd(li,fi);selApplyCSS();syncLayerSelFromFrameSel();}
    if(li!==state.activeLayerIdx)window.SM.setActiveLayer(li);
    goToFrame(fi);
    _tlDrag.active=true;_tlDrag.startL=li;_tlDrag.startF=fi;_tlDrag.moved=false;_tlDrag.ghost=null;_tlDrag.mode='move';
    return;
  }

  // One predictable rule (Animate's): dragging from a cell that was
  // ALREADY selected before this press MOVES the selection; dragging from
  // anything else range-SELECTS. The old "any keyframe drag moves" variant
  // made range-selecting a run of keyframes impossible and moved content
  // when the user only meant to select it.
  var wasSelected=selHas(li,fi);

  if(!wasSelected){selClear();selAdd(li,fi);selApplyCSS();syncLayerSelFromFrameSel();}

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
      selectedPaths=userLayers[li].children.filter(function(c){return c instanceof Path&&isSelectablePathChild(c);});
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
    // 2026-07 fix ("le easing s'applique à tous les tween du calque"): also
    // put the Easing Curve widget into tween-pair mode (editTweenSegForCell)
    // scoped to THIS span, instead of leaving it on its default global
    // curve — which every OTHER span without its own override shares, so
    // editing it here used to visibly reshape every tween on the project.
    if(fr0&&fr0.isInterpolated)editTweenSegForCell(li,fi,fr0);
    else if(window._curveEditor)window._curveEditor.exitTweenPair();
  }

  // Relocating a SINGLE keyframe that has its own outpoint (a bordering
  // NEXT keyframe defining how long it holds) left that hold length behind
  // — moveFrames only relocates the cells actually in _sel.frames, and a
  // hold has no stored length of its own (purely derived by scanning
  // forward to the next real keyframe, timeline.js convention), so only
  // the dragged keyframe moved while its outpoint/next-keyframe stayed put,
  // silently changing (or inverting) the gap between them — feedback: "le
  // drag de la keyframe avec un outpoint derrière elle n'entraine pas
  // l'outpoint avec elle". Fix: when this move-drag starts from EXACTLY one
  // selected keyframe (not a deliberate multi-select/range — same
  // `!wasSelected`-style single-cell precedent used elsewhere in this
  // handler) and it borders a real next keyframe on the same layer, add
  // that bordering keyframe to the selection too — moveFrames' single
  // uniform offset then carries both, preserving the gap between them.
  if(wasSelected&&_sel.frames.length===1&&_sel.frames[0].layer===li&&_sel.frames[0].frame===fi){
    var ldBorder=state.layers[li];
    if(ldBorder&&ldBorder.frames[fi]&&ldBorder.frames[fi].isKeyframe){
      for(var nb=fi+1;nb<ldBorder.frames.length;nb++){
        if(ldBorder.frames[nb].isKeyframe){selAdd(li,nb);selApplyCSS();break;}
      }
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
  // If this cell sits inside (or starts) a tween span, offer to edit JUST
  // that pair's easing curve (openTweenCurveInset) — same idea as the
  // tween-curve-strip's own click-to-edit, reachable even with
  // state.showTweenCurves off.
  var twPair=tweenPairForCell(li,fi);
  var twMenuItems=twPair?[{label:'Éditer la courbe de ce tween…',action:function(){openTweenCurveInset(li,twPair.a,twPair.b);}},{sep:true}]:[];
  window.showContextMenu(e.clientX,e.clientY,twMenuItems.concat([
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
  ]));
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
  var toL=layerIndexAtGridY(yRel);

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
    // Drag-rectangle range-select spanning several layer ROWS (not just the
    // single-cell shift-click case the mousedown handler already covers) —
    // every layer touched by the drag needs to show selected in the panel
    // too, same reasoning as syncLayerSelFromFrameSel's own comment above:
    // F5/F6 now read _layerSel, so a multi-layer block selection that only
    // highlighted ONE row understated which layers were actually about to
    // be affected (reported with a screenshot: dragging a selection across
    // Layer 1 and Layer 2 only showed Layer 2 as selected in the panel).
    syncLayerSelFromFrameSel();
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
  var clampedY=Math.max(12,Math.min(frameGridTotalRowsHeight()-12,yRel));
  _tlDrag.cursorDot.style.left=(clampedX+gridOffLeft)+'px';
  _tlDrag.cursorDot.style.top=(clampedY+camOff+gridOffTop)+'px';

  _tlDrag.ghost.innerHTML='';
  _sel.frames.forEach(function(s){
    var gl=s.layer+offL,gf=s.frame+offF;
    if(gl<0||gl>=state.layers.length||gf<0||gf>=state.totalFrames)return;
    var d=document.createElement('div');
    d.style.cssText='position:absolute;width:'+FC+'px;height:'+frameGridRowHeight(gl)+'px;background:rgba(74,158,255,.35);border:1px solid rgba(74,158,255,.7);border-radius:2px;box-sizing:border-box;';
    d.style.left=(gf*FC+gridOffLeft)+'px';d.style.top=(visualTopOfLayer(gl)+camOff+gridOffTop)+'px';
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
  // Shy (AE's own switch, 2026-07-25): a per-layer flag plus one global
  // toggle. Marked hidden here rather than filtered out, so it rides the
  // SAME mechanism folders and link-groups already use — every consumer
  // that already honours `hidden` gets shy for free, and nothing downstream
  // has to learn a second way for a row to be absent.
  if(state.shyEnabled)order.forEach(function(e){
    if(e.type==='layer'&&state.layers[e.idx]&&state.layers[e.idx].shy)e.hidden=true;
  });
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
// 3D layer toggle (2026-07-28, After-Effects-style) — isometric cube
// wireframe, plain inline SVG like every other layer-row icon (not an
// icon-font glyph — this project's embedded font is a subset containing
// only already-referenced codepoints, see this project's own CLAUDE.md).
var ICO_3D='<svg viewBox="0 0 24 24"><path d="M12 3 L20 7.5 L20 16.5 L12 21 L4 16.5 L4 7.5 Z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/><path d="M12 3 L12 12 M20 7.5 L12 12 M4 7.5 L12 12 M12 12 L12 21" fill="none" stroke="currentColor" stroke-width="1.6"/></svg>';
// Mograph duplicator toggle (2026-07-29) — 2x2 grid of squares, one filled
// (the seed) and three outlined (the copies). Same inline-SVG convention as
// ICO_3D above.
var ICO_DUP='<svg viewBox="0 0 24 24"><rect x="4" y="4" width="7" height="7" rx="1.2" fill="currentColor"/><rect x="13.5" y="4" width="7" height="7" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.6"/><rect x="4" y="13.5" width="7" height="7" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.6"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.6"/></svg>';
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
// AE's "Parent & Link" column, on the layer rows themselves (2026-07-25,
// "il manque le système de parentage directement sur les calques comme dans
// after"). The data model (ld.parentLayerUid), the cycle refusal
// (SMMotion.setLayerParent) and the chain composition (parentChainMats, wired
// into engine-bridge/export/native-video) all already existed — but the only
// way to REACH them was a dropdown inside Motion mode's property panel, on one
// layer at a time, and only while that layer was expanded. motion.js's own
// comment calls that row "AE's Parent & Link column reimagined", which is
// exactly the thing this restores: in AE the column is always on screen for
// every layer at once, which is what makes a rig readable at a glance.
//
// A menu rather than a <select>: the layer panel is narrow and user-resizable,
// so a native select would either clip its own label or force the column wider
// than the panel. showContextMenu already gives dimmed disabled entries,
// outside-click dismissal and Escape for free.
function parentDescendants(li){
  // Every layer that has `li` somewhere up its own parent chain — these can't
  // become its parent without forming a cycle. setLayerParent refuses those
  // anyway; listing them as disabled explains WHY before the click instead of
  // after it.
  var M=window.SMMotion; if(!M||!M.findLayerIndexByUid)return {};
  var out={};
  // Walks BOTH parent slots (2026-07-30, multi-parent) — a layer reachable
  // only through someone's Parent B is just as much a descendant as one
  // reachable through Parent A; missing this half wouldn't let an actual
  // cycle through, since setLayerParent/setLayerParentB's own
  // wouldCreateParentCycle guard is the real backstop, but it WOULD leave
  // that option un-greyed-out here, looking valid until the click refused
  // it.
  state.layers.forEach(function(other,oi){
    var queue=[other.parentLayerUid,other.parentLayerUidB].filter(Boolean),guard=0,seen={};
    while(queue.length&&guard++<256){
      var cur=queue.shift();
      if(seen[cur])continue;
      seen[cur]=true;
      var pi=M.findLayerIndexByUid(cur);
      if(pi===li){out[oi]=true;break;}
      if(pi>=0){
        if(state.layers[pi].parentLayerUid)queue.push(state.layers[pi].parentLayerUid);
        if(state.layers[pi].parentLayerUidB)queue.push(state.layers[pi].parentLayerUidB);
      }
    }
  });
  return out;
}
// Resolved locally rather than through SMMotion.findLayerIndexByUid, because
// this runs at RENDER time and renderLayerList fires during boot BEFORE
// motion.js has defined SMMotion. An early return on a missing SMMotion made
// the whole column vanish on first paint and never come back until something
// else happened to re-render the list — silently, which is how it survived
// testing (every test called renderLayerList itself, long after load). The
// lookup is two lines; the dependency was not worth it.
function _layerIndexByUid(uid){
  if(!uid)return -1;
  for(var i=0;i<state.layers.length;i++)if(state.layers[i].layerUid===uid)return i;
  return -1;
}
// AE's parent pickwhip, shared by BOTH timelines (2026-07-25). Drag the dot
// onto any layer row to parent to it; drop outside / on itself / on a
// descendant cancels. Deliberately the same code as the dropdown path below
// — both call SMMotion.setLayerParent, which owns the cycle refusal, so
// there is still exactly ONE writer of ld.parentLayerUid.
function startParentPickwhip(li,fromEl,ev){
  ev.stopPropagation(); ev.preventDefault();
  var M=window.SMMotion; if(!M||!M.setLayerParent){showToast('Parentage indisponible');return;}
  var bad=parentDescendants(li);
  var r0=fromEl.getBoundingClientRect();
  var ox=r0.left+r0.width/2, oy=r0.top+r0.height/2;
  var line=document.createElement('div'); line.className='lpick-line'; document.body.appendChild(line);
  var hover=null;
  function paint(x,y){
    var dx=x-ox, dy=y-oy;
    line.style.left=ox+'px'; line.style.top=oy+'px';
    line.style.width=Math.sqrt(dx*dx+dy*dy)+'px';
    line.style.transform='rotate('+Math.atan2(dy,dx)+'rad)';
  }
  function rowUnder(x,y){
    var el=document.elementFromPoint(x,y); if(!el)return null;
    var row=el.closest?el.closest('.lrow[data-layer]'):null; if(!row)return null;
    var idx=parseInt(row.dataset.layer,10);
    if(isNaN(idx)||idx===li||bad[idx])return null;
    return {row:row,idx:idx};
  }
  function onMove(e){
    paint(e.clientX,e.clientY);
    var t=rowUnder(e.clientX,e.clientY);
    if(hover&&(!t||t.row!==hover.row))hover.row.classList.remove('pick-target');
    if(t&&(!hover||t.row!==hover.row))t.row.classList.add('pick-target');
    hover=t;
  }
  function cleanup(){
    document.removeEventListener('mousemove',onMove,true);
    document.removeEventListener('mouseup',onUp,true);
    document.removeEventListener('keydown',onKey,true);
    if(hover)hover.row.classList.remove('pick-target');
    line.remove();
  }
  function onUp(e){
    var t=rowUnder(e.clientX,e.clientY);
    cleanup();
    if(!t)return; // dropped on nothing / itself / a descendant — no-op, no undo entry
    pushUndo();
    M.setLayerParent(li,M.ensureLayerUid(state.layers[t.idx]));
    renderLayerList(); renderTimeline();
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
  }
  function onKey(e){ if(e.key==='Escape'){cleanup();} }
  document.addEventListener('mousemove',onMove,true);
  document.addEventListener('mouseup',onUp,true);
  document.addEventListener('keydown',onKey,true);
  paint(ev.clientX,ev.clientY);
}
// Shared by buildParentCell (layer-list row) AND motion.js's renderParentRow
// (Layer Properties panel) — ONE implementation of "what does the Parent A /
// Parent B picker menu contain," so the two surfaces can never drift apart
// (CLAUDE.md §3). `onChanged()` fires after every write, before the caller's
// own re-render — each surface refreshes only what it actually shows.
function buildParentMenuItems(li,ld,onChanged){
  var M=window.SMMotion;
  if(!M||!M.setLayerParent)return [{label:'Parentage indisponible',disabled:true}];
  var bad=parentDescendants(li);
  var items=[{label:'Parent A : Aucun (parentage libre)',disabled:!ld.parentLayerUid,action:function(){
    pushUndo(); M.setLayerParent(li,null); onChanged();
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
  }}];
  state.layers.forEach(function(other,oi){
    if(oi===li)return; // a layer can't parent itself
    var uid=M.ensureLayerUid(other);
    items.push({
      label:'Parent A : '+(other.name||('Layer '+(oi+1)))+(bad[oi]?'  (descendant)':'')+(ld.parentLayerUidB===uid?'  (déjà Parent B)':''),
      disabled:!!bad[oi]||ld.parentLayerUid===uid||ld.parentLayerUidB===uid,
      action:function(){
        pushUndo(); M.setLayerParent(li,uid); onChanged();
        if(window.SMEngineBridge)SMEngineBridge.renderNow();
      }
    });
  });
  // Parent B section — only reachable once Parent A exists (crossfading
  // needs two endpoints; a lone Parent B with no A would just silently
  // do nothing, per blendedAncestorMat's own guard, motion.js).
  items.push({sep:true});
  if(!ld.parentLayerUid){
    items.push({label:'Parent B (choisir d’abord un Parent A)',disabled:true});
  }else{
    items.push({label:'Parent B : Aucun',disabled:!ld.parentLayerUidB,action:function(){
      pushUndo(); M.setLayerParentB(li,null); onChanged();
      if(window.SMEngineBridge)SMEngineBridge.renderNow();
    }});
    state.layers.forEach(function(other,oi){
      if(oi===li)return;
      var uid=M.ensureLayerUid(other);
      items.push({
        label:'Parent B : '+(other.name||('Layer '+(oi+1)))+(bad[oi]?'  (descendant)':'')+(ld.parentLayerUid===uid?'  (déjà Parent A)':''),
        disabled:!!bad[oi]||ld.parentLayerUid===uid||ld.parentLayerUidB===uid,
        action:function(){
          pushUndo(); M.setLayerParentB(li,uid); onChanged();
          if(window.SMEngineBridge)SMEngineBridge.renderNow();
        }
      });
    });
  }
  return items;
}
window.buildParentMenuItems=buildParentMenuItems;
function buildParentCell(row,ld,li){
  var cell=document.createElement('div');
  cell.className='lparent';
  var pIdx=_layerIndexByUid(ld.parentLayerUid);
  var pName=(pIdx>=0&&state.layers[pIdx])?(state.layers[pIdx].name||('Layer '+(pIdx+1))):null;
  // Multi-parent crossfade (2026-07-30, "plusieurs parent... jouer comme
  // une opacité les parents entre eux") — Parent B is picked from the SAME
  // cell's context menu (below) rather than a second pick-whip/column: the
  // layer-list row is already tight, and a second parent is a rare/
  // advanced case, not something every row needs a permanent affordance
  // for. The label grows to show both once B is set, so it's discoverable
  // rather than hidden.
  var pbIdx=_layerIndexByUid(ld.parentLayerUidB);
  var pbName=(pbIdx>=0&&state.layers[pbIdx])?(state.layers[pbIdx].name||('Layer '+(pbIdx+1))):null;
  var pick=document.createElement('span');
  pick.className='lpick';
  pick.title='Glisser sur un calque pour le définir comme parent (A)';
  pick.addEventListener('mousedown',function(e){startParentPickwhip(li,pick,e);});
  cell.appendChild(pick);
  var lbl=document.createElement('span');
  lbl.textContent=pbName?(pName||'—')+' + '+pbName:(pName||'—');
  cell.appendChild(lbl);
  cell.classList.toggle('none',!pName);
  cell.title=pbName?('Parent A : '+pName+' + Parent B : '+pbName+' (blend animable) — cliquer pour changer')
    :(pName?('Parent : '+pName+' — cliquer pour changer'):'Aucun parent — cliquer pour en choisir un');
  function open(e){
    e.stopPropagation(); e.preventDefault();
    var items=buildParentMenuItems(li,ld,function(){renderLayerList();renderTimeline();});
    var r=cell.getBoundingClientRect();
    showContextMenu(r.left,r.bottom+2,items);
  }
  cell.addEventListener('click',open);
  cell.addEventListener('mousedown',function(e){e.stopPropagation();});
  row.appendChild(cell);
}
// THE single writer for every surface that DISPLAYS the current stroke/fill
// colour (2026-07-25, "les couleurs j'ai l'impression que c'est pas synchro
// entre panneau gauche et droit").
//
// There were two writers for the same value and they disagreed. setStrokeColor
// /setFillColor painted all seven surfaces; the selection handler in updateUI
// hand-wrote only four of them. Selecting a shape therefore left the app
// showing two different colours at once — measured on a #8bc34a fill:
//
//   pm-fill (right swatch)   #8bc34a   <- adopted the selection
//   pm-fill-c (right input)  #8bc34a   <- adopted
//   p-fill-hex (right hex)   0066FF    <- still the previous global colour
//   color-fill / fill-well   #0066ff   <- still the previous, LEFT panel
//
// Two widgets in the same panel contradicting each other. Split out here so
// both callers paint the same set: setStrokeColor/setFillColor also APPLY the
// colour to the selection, which is why the selection handler can't simply
// call them (it would rewrite the object it just read, and push undo doing it).
//
// Keeps the existing 8-digit-hex convention untouched: `.value` is assigned
// the full string and the native input truncates it to 6 digits by design,
// with `.dataset.hex8` carrying the alpha alongside (CLAUDE.md §2).
function paintStrokeSwatches(v){
  var sw=document.getElementById('stroke-well'); if(sw)sw.style.background=v;
  var pm=document.getElementById('pm-stroke');   if(pm)pm.style.background=v;
  ['color-stroke','pm-stroke-c'].forEach(function(id){var el=document.getElementById(id);if(el){el.value=v;el.dataset.hex8=v;}});
  var shex=document.getElementById('p-stroke-hex');if(shex&&document.activeElement!==shex)shex.value=hexDisplayValue(v);
  var salpha=document.getElementById('p-stroke-alpha');if(salpha&&document.activeElement!==salpha)salpha.value=alphaPctFromHex(v);
}
function paintFillSwatches(v){
  // Background written unconditionally (even while fill is disabled) so the
  // swatch shows the last-picked colour the moment fill is re-enabled — the
  // .none overlay is what communicates "off".
  var fw=document.getElementById('fill-well'); if(fw)fw.style.background=v;
  var pf=document.getElementById('pm-fill');   if(pf)pf.style.background=v;
  ['color-fill','pm-fill-c'].forEach(function(id){var el=document.getElementById(id);if(el){el.value=v;el.dataset.hex8=v;}});
  // No fill-side alpha field exists (only the stroke has one) — the fill's
  // alpha rides in dataset.hex8 above.
  var fhex=document.getElementById('p-fill-hex');if(fhex&&document.activeElement!==fhex)fhex.value=hexDisplayValue(v);
}
// `frameOnly` — same contract as updateUI's: nothing but state.currentFrame
// changed. Animation 2D's rows are frame-INDEPENDENT (name, colour dot,
// visibility, lock, solo — not one state.currentFrame reference in this
// function's own 240 lines), so a scrub was rebuilding every row for
// nothing: 4.2ms per tick at 40 layers. Motion's rows are NOT — each
// property track prints its VALUE at the playhead — so the decision lives
// here, next to the mode branch that already exists, rather than in the
// caller.
function renderLayerList(frameOnly){
  if(frameOnly&&state.appMode!=='motion')return;
  var _scroll=_tlScrollSnapshot(); // see _tlScrollSnapshot — same wipe, same jump
  var list=document.getElementById('layer-list');list.innerHTML='';
  // Motion mode: expandable Transform property rows instead of the plain
  // per-layer row list — see motion.js's own header comment for why this
  // is a full early return rather than a branch woven through the rest of
  // this function (folders/link-groups/components have no meaning yet in
  // Motion mode's v1 scope).
  if(state.appMode==='motion'){if(window.SMMotion)SMMotion.renderLayerListMotion(list);_tlScrollRestore(_scroll);return;}
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
    // Mirror renderTimeline's per-row tween-curve extra height exactly (same
    // layerCurveRowExtraHeight source) — the #layer-list/#frame-grid
    // alignment invariant (CLAUDE.md §11) requires both sides reserve the
    // same height for any row. align-items:center (the row's own CSS)
    // would otherwise vertically center the icons in the middle of the
    // taller box; pin them to the top instead so they still line up with
    // the frame-grid's own (unchanged, still ROW_H-tall) keyframe cells.
    var lrowExtraH=layerCurveRowExtraHeight(i);
    if(lrowExtraH>0){row.style.height=(ROW_H+lrowExtraH)+'px';row.style.alignItems='flex-start';row.style.paddingTop='8px';}
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
    // 3D layer toggle (2026-07-28, After-Effects-style) — same .lico
    // convention as eye/lock above, delegating to motion.js's
    // toggleLayer3D (the state mutation + re-render live there, not
    // inline here, matching every other icon button's own convention).
    var d3=document.createElement('div');d3.className='lico'+(ld.threeD?'':' off');d3.title='3D Layer';d3.innerHTML=ICO_3D;d3.dataset.layer=i;d3.addEventListener('click',function(e){e.stopPropagation();if(window.SMMotion)SMMotion.toggleLayer3D(parseInt(this.dataset.layer));});
    // Mograph duplicator toggle — same .lico convention, delegates to
    // motion.js's toggleLayerDuplicator (lock + config init live there).
    var ddup=document.createElement('div');ddup.className='lico'+(ld.duplicator?'':' off');ddup.title='Duplicator — répète cette forme en grille, en cercle (radial) ou le long d’un chemin, avec un décalage animable entre copies (comme le Cloner de Cinema 4D / MoGraph)';ddup.innerHTML=ICO_DUP;ddup.dataset.layer=i;ddup.addEventListener('click',function(e){e.stopPropagation();if(window.SMMotion)SMMotion.toggleLayerDuplicator(parseInt(this.dataset.layer));});
    // Type badge — a video, an imported sequence and a hand-drawn layer were
    // three identical rows before this (layer-kind.js decides which). Sits
    // just before the name so the eye reads "what" then "which", and carries
    // the kind as a class so CSS can tint it per type.
    var kind=window.SMLayerKind?SMLayerKind.of(ld):null;
    if(kind&&kind.key!=='draw'){
      var kb=document.createElement('div');kb.className='lkind lkind-'+kind.key;
      kb.title=kind.label;kb.innerHTML=kind.icon;
      row.appendChild(kb);
    }
    var nm=document.createElement('div');nm.className='lnm';nm.textContent=ld.name;
    row.appendChild(eye);row.appendChild(lock);row.appendChild(solo);row.appendChild(d3);row.appendChild(ddup);
    // Text badge, not the ◈ glyph it used to be (2026-07-29 fix, "l'icon
    // component du calque est le meme que celui pour le 3D layer") -- the
    // diamond-shaped glyph read as visually identical to ICO_3D's hexagon
    // wireframe at 14px next to each other in the row. 'C' matches this same
    // file's own text-badge convention already used for every OTHER layer
    // kind below (T/MT/FX/LFS/Tr/Pl/Om) instead of being the one symbolic
    // outlier among them.
    if(ld.symbolId){var cb=document.createElement('div');cb.className='lico comp-badge';cb.title='Component — double-click to edit';cb.innerHTML='<span style="font-size:11px;line-height:1;font-weight:700">C</span>';row.appendChild(cb);}
    if(ld.lfsGroup){var lb=document.createElement('div');lb.className='lico comp-badge';lb.title='Ligne/Plein/Ombre layer';lb.innerHTML='<span style="font-size:11px;line-height:1">LFS</span>';row.appendChild(lb);}
    // Stroke/Fill/Shadow channel badge — a fully normal layer row (own
    // working eye/lock/solo above, no folder wrapper), just visually
    // labeled so its role in the split is obvious at a glance. Letter
    // matches the channel initial (S/F/O for Ombre, avoiding a clash with
    // the Solo 'S' badge's own single-letter convention would need 2
    // letters here anyway since Fill/Shadow both start differently in FR).
    if(ld.montageId){var mtb=document.createElement('div');mtb.className='lico comp-badge';mtb.title='Calque montage (StoryBoard) — contenu piloté par le montage \u00ab '+ld.name+' \u00bb, s\u2019édite dans l\u2019onglet StoryBoard';mtb.innerHTML='<span style="font-size:9px;line-height:1;font-weight:700">MT</span>';row.appendChild(mtb);}
    // Text layer badge (2026-07) -- ld.isTextLayer, same "layer-level flag,
    // not a new item type" precedent as symbolId/lfsGroup/montageId above:
    // it distinguishes the layer visually/structurally without touching
    // any layer.children consumer (CLAUDE.md family of bug n1 never
    // applies here -- a text layer's items are still ordinary Rasters).
    // Auto-set the first time text lands on an empty layer (commitText,
    // "Outil texte" section) or explicitly via the layer row's context menu.
    if(ld.isTextLayer){var txb=document.createElement('div');txb.className='lico comp-badge';txb.title='Calque de texte';txb.innerHTML='<span style="font-size:11px;line-height:1;font-weight:700">T</span>';row.appendChild(txb);}
    // Null / Effect layer badges (2026-07, Motion) — same "lico comp-badge"
    // text-glyph convention as every badge above (this project's embedded
    // icon font is subsetted and silently renders blank for un-included
    // codepoints, see the lock-icon fix comment further up this function).
    if(ld.isNullLayer){var nlb=document.createElement('div');nlb.className='lico comp-badge';nlb.title='Calque Null — jamais rendu, sert de pivot/parent pour d’autres calques';nlb.innerHTML='<span style="font-size:11px;line-height:1;font-weight:700">⊘</span>';row.appendChild(nlb);}
    if(ld.isEffectLayer){
      var fxLabels=window.EFFECT_LABELS||{};
      var enabledFx=(ld.effects||[]).filter(function(e){return e.enabled;});
      var fxDesc=enabledFx.length?enabledFx.map(function(e){return fxLabels[e.type]||e.type;}).join(', '):'aucun effet';
      var fxb=document.createElement('div');fxb.className='lico comp-badge';fxb.title='Calque d’effet — '+fxDesc+' — appliqué à tout ce qui est en dessous';fxb.innerHTML='<span style="font-size:9px;line-height:1;font-weight:700">FX</span>';row.appendChild(fxb);
    }
    if(ld.channel){var chLabel=ld.channel==='stroke'?'Tr':ld.channel==='fill'?'Pl':'Om';var chb=document.createElement('div');chb.className='lico comp-badge';chb.title='Calque '+(ld.channel==='stroke'?'Trait':ld.channel==='fill'?'Plein':'Ombre')+' (Stroke/Fill/Shadow) — calque normal, keyframes liées';chb.innerHTML='<span style="font-size:9px;line-height:1;font-weight:700">'+chLabel+'</span>';row.appendChild(chb);}
    // Track matte badge (2026-07) — the Blend/Matte dropdowns only surface
    // in the right panel's Document fallback context (nothing selected,
    // no draw tool active — see updatePropsContext), which turned out to
    // be too easy to miss entirely ("je vois pas où appliqué les track
    // matte"). This badge is visible on the layer row REGARDLESS of tool/
    // selection state — the always-reachable entry point — and click opens
    // the exact same picker the right-panel dropdown does, just anchored
    // here instead. A masked layer shows 'M' (its own matte); the layer
    // directly above a masked one — its IMPLICIT source, AE convention —
    // shows a dimmed 'M▲' so its role is visible too, even though nothing
    // is actually SET on that layer's own data.
    // A layer i is a matte SOURCE when the layer BELOW it (i-1) has its
    // matteMode set — matteMode on layer X means "X's alpha comes from
    // X+1", so X+1 is the source. Row i is that source when i-1's mode is
    // set, not i+1's (caught by the badge simply never appearing on the
    // source layer in a quick browser check — the array direction was
    // backwards on the first pass).
    var isMatteSource=(i-1>=0)&&state.layers[i-1]&&state.layers[i-1].matteMode;
    if(ld.matteMode||isMatteSource){
      var mb=document.createElement('div');mb.className='lico comp-badge'+(isMatteSource&&!ld.matteMode?' off':'');
      mb.title=ld.matteMode?('Matte: '+(typeof MATTE_MODE_LABELS!=='undefined'?MATTE_MODE_LABELS[ld.matteMode]:ld.matteMode)+' — clic pour changer'):'Source de matte pour le calque du dessous';
      mb.innerHTML='<span style="font-size:9px;line-height:1;font-weight:700">'+(ld.matteMode?'M':'M▲')+'</span>';
      if(ld.matteMode){mb.style.cursor='pointer';mb.addEventListener('click',function(e){e.stopPropagation();state.activeLayerIdx=i;activateUL(i);updatePropsContext();openMatteDropdownAt(mb);});}
      row.appendChild(mb);
    }
    row.appendChild(nm);
    buildParentCell(row,ld,i);
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
        {label:'Insérer un calque Null',action:function(){window.SM.addNullLayer();}},
        {label:'Insérer un calque d’effet',action:function(){window.SM.addEffectLayer();}},
        {label:'Dupliquer le calque',action:function(){window.SM.duplicateLayer();}},
        {label:'Supprimer le calque',action:function(){window.SM.deleteLayer();}},
        {sep:true},
        {label:'Renommer',action:function(){startLayerRename(idx4);}},
        {label:l4.isTextLayer?'Retirer le marquage « calque de texte »':'Marquer comme calque de texte',action:function(){l4.isTextLayer=!l4.isTextLayer;renderLayerList();}},
        {label:'Grouper en dossier',disabled:_layerSel.length<2,action:function(){groupSelectionIntoFolder();}},
        // Split / merge as a reversible PAIR (2026-07-25). "Éclater" existed
        // only as a Motion double-click with no visible entry point and no
        // inverse; both directions now sit next to each other, in both
        // timelines, so the round-trip is discoverable from either end.
        {label:'Éclater en calques (une forme par calque)',disabled:!!l4.symbolId||!!l4.lfsGroup,action:function(){window.SM.splitLayerIntoElements(idx4);}},
        {label:'Couper au niveau de la tête de lecture  (⌘⇧D)',action:function(){window.SM.splitLayerAtPlayhead(idx4);}},
        {label:l4.shy?'Retirer le marquage « shy »':'Marquer comme « shy »',action:function(){window.SM.toggleLayerShy(idx4);}},
        {label:'Fusionner les calques sélectionnés',disabled:_layerSel.length<2,action:function(){window.SM.mergeLayersIntoOne(_layerSel.slice());}},
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
        {sep:true},
        // Track matte (2026-07) — the discoverable entry point: works
        // whether or not this layer already has a matte, unlike the badge
        // (which only shows once one exists) or the right-panel dropdown
        // (buried in a fallback context — see updatePropsContext). Needs a
        // layer above to draw the mask FROM, AE convention.
        {label:idx4>=state.layers.length-1?'Matte (aucun calque au-dessus)':(l4.matteMode?'Changer la matte…':'Appliquer une matte…'),disabled:idx4>=state.layers.length-1,action:function(){
          window.SM.setActiveLayer(idx4);updatePropsContext();
          // The right-panel dropdown only exists in the Document-fallback
          // context (see updatePropsContext) — if a draw tool is active it
          // stays display:none and its getBoundingClientRect() would be
          // all-zero, opening the popup pinned to the top-left corner.
          // Fall back to this very row, always visible by construction
          // (the user just right-clicked it).
          var panelAnchor=document.getElementById('p-mattemode');
          var anchor=(panelAnchor&&panelAnchor.offsetParent)?panelAnchor:row;
          if(window.openMatteDropdownAt)openMatteDropdownAt(anchor);
        }},
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
  _tlScrollRestore(_scroll);
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
  var scene=document.createElement('div');scene.className='sym-tab'+((state.activeSymbolId||state.activeMontageViewId)?'':' act');scene.textContent='Scene';
  scene.addEventListener('click',function(){
    if(state.activeSymbolId)window.SM.exitToScene();
    else if(state.activeMontageViewId)window.SM.exitMontageView();
  });
  bar.appendChild(scene);
  // StoryBoard "entrer dans le montage" (2026-07) — a montage view sits
  // BETWEEN Scene and any real component entered from within it (breadcrumb:
  // Scene > Montage > Composant). Not part of openSymbolTabs — at most one
  // montage view is active at a time, no multi-tab list needed.
  if(state.activeMontageViewId){
    var mtg=window.SMStoryboard&&window.SMStoryboard.montageById(state.activeMontageViewId);
    var mtab=document.createElement('div');mtab.className='sym-tab'+(state.activeSymbolId?'':' act');
    var mlabel=document.createElement('span');mlabel.textContent=(mtg?mtg.name:'Montage')+' (montage)';
    var mx=document.createElement('span');mx.className='sym-tab-x';mx.textContent='×';
    mtab.appendChild(mlabel);mtab.appendChild(mx);
    mtab.addEventListener('click',function(e){if(e.target===mx)return;if(state.activeSymbolId)window.SM.exitToScene();});
    mx.addEventListener('click',function(e){e.stopPropagation();if(state.activeSymbolId)window.SM.exitToScene();window.SM.exitMontageView();});
    bar.appendChild(mtab);
  }
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
// Footage panel — mirrors updateCompInstancePanel's shape (show/hide from the
// active layer, populate, no state of its own). Reads the layer's kind from
// layer-kind.js rather than re-testing flags, so it can never disagree with
// the badge on the row.
function updateFootagePanel(){
  var sec=document.getElementById('footage-sec');
  if(!sec)return;
  var ld=state.layers[state.activeLayerIdx];
  var kind=(ld&&window.SMLayerKind)?SMLayerKind.of(ld):null;
  var isFootage=kind&&(kind.key==='image'||kind.key==='sequence'||kind.key==='video');
  if(!isFootage||!window._layerActiveExplicit){sec.style.display='none';return;}
  sec.style.display='block';
  document.getElementById('footage-kind').textContent=kind.label;
  var meta=ld.footage||{};
  // The name is whatever we were told at import; fall back to the layer's
  // own name rather than showing an empty row for pre-tag projects.
  document.getElementById('footage-name').textContent=meta.name||ld.name||'—';
  document.getElementById('footage-name').title=meta.name||ld.name||'';
  // Dimensions come from the raster actually on the frame, not from the
  // import-time metadata: a scaled footage layer should report what it IS
  // now, and pre-tag projects have no metadata at all.
  var st=(ld.frames&&ld.frames[state.currentFrame]&&ld.frames[state.currentFrame].strokes)||[];
  var ras=null;
  for(var i=0;i<st.length;i++){if(st[i]&&st[i].isRaster){ras=st[i];break;}}
  document.getElementById('footage-w').value=ras?Math.round(ras.width):0;
  document.getElementById('footage-h').value=ras?Math.round(ras.height):0;
  var countRow=document.getElementById('footage-count-row');
  var isSeq=kind.key==='sequence';
  countRow.style.display=isSeq?'flex':'none';
  if(isSeq){
    var n=meta.count;
    if(n==null){n=0;(ld.frames||[]).forEach(function(f){if(f&&f.strokes&&f.strokes.some(function(x){return x&&x.isRaster;}))n++;});}
    document.getElementById('footage-count').textContent=n;
  }
  // A video's source lives in ld.nativeVideo and is swapped by its own
  // importer, so only the raster kinds get the in-place replace.
  document.getElementById('btn-footage-replace').style.display=(kind.key==='video')?'none':'';
}
function updateCompInstancePanel(){
  var sec=document.getElementById('comp-instance-sec');
  if(state.activeSymbolId){sec.style.display='none';return;}
  var ld=state.layers[state.activeLayerIdx];
  if(!ld||!ld.symbolId){sec.style.display='none';return;}
  sec.style.display='block';
  document.getElementById('comp-playmode').value=ld.symPlayMode||'loop';
  document.getElementById('comp-singleframe-row').style.display=(ld.symPlayMode==='single')?'flex':'none';
  var sym=state.symbols[ld.symbolId];
  var resolved=sym?resolveSymbolFrameIdx(sym,ld,state.currentFrame):(ld.symSingleFrame||0);
  document.getElementById('comp-singleframe').value=resolved;
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
    var own=ld.frames&&ld.frames[state.currentFrame];
    var isExplicit=!!(own&&own.isKeyframe&&own.componentFrame!=null);
    var isSingleSel=(isExplicit||ld.symPlayMode==='single')&&i===current;
    cell.style.background=isSingleSel?'var(--accent)':(isCur?'rgba(255,255,255,.25)':'var(--bg)');
    cell.style.border='1px solid '+(isCur?'var(--accent)':'rgba(255,255,255,.15)');
    cell.title='Frame '+i;
    cell.addEventListener('click',function(idx){return function(){window.SM.setSymbolSingleFrame(idx);window.SM.setSymbolPlayMode('single');updateCompInstancePanel();};}(i));
    strip.appendChild(cell);
  }
}
// Mograph duplicator config panel (2026-07-29) — same show/hide-from-the-
// active-layer shape as updateCompInstancePanel above. Only the STATIC
// config lives here (mode/counts/seed/random toggles); the animatable
// per-copy deltas are ordinary Motion properties (PROPS_DUP_EXTRA,
// motion.js). Field listeners are wired once at startup (bottom of this
// file, next to #comp-playmode's own) and write into ld.duplicator.* then
// reload the frame.
function updateDuplicatorPanel(){
  var sec=document.getElementById('duplicator-sec');
  if(!sec)return;
  var ld=state.layers[state.activeLayerIdx];
  if(!ld||!ld.duplicator){sec.style.display='none';return;}
  sec.style.display='block';
  var dup=ld.duplicator;
  var mode=dup.mode||'grid';
  document.getElementById('dup-mode').value=mode;
  document.getElementById('dup-grid-row').style.display=mode==='grid'?'flex':'none';
  document.getElementById('dup-grid-spacing-row').style.display=mode==='grid'?'flex':'none';
  document.getElementById('dup-count-row').style.display=mode!=='grid'?'flex':'none';
  document.getElementById('dup-radial-row').style.display=mode==='radial'?'flex':'none';
  document.getElementById('dup-radial-orient-row').style.display=mode==='radial'?'flex':'none';
  document.getElementById('dup-path-row').style.display=mode==='path'?'flex':'none';
  document.getElementById('dup-path-align-row').style.display=mode==='path'?'flex':'none';
  document.getElementById('dup-rows').value=dup.rows||1;
  document.getElementById('dup-cols').value=dup.cols||1;
  document.getElementById('dup-spacingx').value=dup.spacingX||0;
  document.getElementById('dup-spacingy').value=dup.spacingY||0;
  document.getElementById('dup-count').value=dup.count||1;
  document.getElementById('dup-radius').value=dup.radius||0;
  document.getElementById('dup-startangle').value=dup.startAngle||0;
  document.getElementById('dup-radial-orient').checked=!!dup.radialOrient;
  document.getElementById('dup-path-align').checked=!!dup.pathAlignTangent;
  document.getElementById('dup-seed').value=dup.seed||0;
  var sr=dup.staggerRandom||{};
  document.getElementById('dup-rand-pos').checked=!!sr.position;
  document.getElementById('dup-rand-rot').checked=!!sr.rotation;
  document.getElementById('dup-rand-scale').checked=!!sr.scale;
  document.getElementById('dup-rand-op').checked=!!sr.opacity;
  // Path-layer picker: every OTHER layer that isn't itself a duplicator
  // (no chained duplicators in v1 — same refusal as _resolveDuplicatorPath's
  // own runtime guard, this is just the UI half).
  var tOff=dup.timeOffset||{};
  document.getElementById('dup-anim-enabled').checked=!!tOff.enabled;
  document.getElementById('dup-anim-row').style.display=tOff.enabled?'flex':'none';
  document.getElementById('dup-anim-offset').value=tOff.offsetFrames!=null?tOff.offsetFrames:1;
  document.getElementById('dup-anim-direction').value=tOff.direction||'forward';
  var sel=document.getElementById('dup-path-layer');
  sel.innerHTML='';
  var none=document.createElement('option');none.value='';none.textContent='—';sel.appendChild(none);
  state.layers.forEach(function(l,i){
    if(i===state.activeLayerIdx||l.duplicator)return;
    var o=document.createElement('option');o.value=l.layerUid||'';o.textContent=l.name||('Layer '+(i+1));sel.appendChild(o);
  });
  sel.value=dup.pathLayerUid||'';
  var editBtn=document.getElementById('btn-dup-edit-source');
  editBtn.textContent=ld._dupEditSource?(window.SM&&SM.t?SM.t('dupEditSourceDone'):'Terminé — réactiver la duplication'):(window.SM&&SM.t?SM.t('dupEditSource'):'Modifier la forme source…');
  editBtn.classList.toggle('ac',!ld._dupEditSource);
  renderDuplicatorEffectors(dup);
}
// Effector rows (2026-07-29) — rebuilt from scratch every call, same
// "variable-length list, no static markup" pattern as renderCompFrameStrip
// above. Each field writes straight into ld.duplicator.effectors[i].* then
// reloads the frame — the shipped renderNow() duplicator guard already
// covers real-time correctness for any mutation path, this one included.
function renderDuplicatorEffectors(dup){
  var list=document.getElementById('dup-effectors-list');
  if(!list)return;
  list.innerHTML='';
  (dup.effectors||[]).forEach(function(eff,i){
    var row=document.createElement('div');
    row.style.cssText='display:flex;flex-direction:column;gap:4px;padding:6px 8px;margin:2px 0;border-radius:6px;background:rgba(255,255,255,.03)';
    function line(){var l=document.createElement('div');l.className='pr';row.appendChild(l);return l;}
    function num(l,id,val,step){var s=document.createElement('span');s.className='pl';s.textContent=l;var inp=document.createElement('input');inp.type='number';inp.className='pi scrub';inp.value=val;inp.dataset.step=step||1;inp.addEventListener('change',function(){id(parseFloat(inp.value)||0);dupRefreshFromPanel();});return[s,inp];}
    // Header: falloff mode, radius, strength, delete.
    var hdr=line();
    var modeSel=document.createElement('select');modeSel.className='psel';
    ['radial','linear'].forEach(function(v){var o=document.createElement('option');o.value=v;o.textContent=v==='radial'?'Radial':'Linear';if(eff.falloff===v||(!eff.falloff&&v==='radial'))o.selected=true;modeSel.appendChild(o);});
    modeSel.addEventListener('change',function(){eff.falloff=modeSel.value;renderDuplicatorEffectors(dup);dupRefreshFromPanel();});
    hdr.appendChild(modeSel);
    var rad=num('R',function(v){eff.radius=v;},eff.radius||200,1);hdr.appendChild(rad[0]);hdr.appendChild(rad[1]);
    var str=num('%',function(v){eff.strength=v;},eff.strength!=null?eff.strength:100,1);hdr.appendChild(str[0]);hdr.appendChild(str[1]);
    var delBtn=document.createElement('button');delBtn.className='pbtn';delBtn.textContent='✕';delBtn.style.marginLeft='auto';
    delBtn.addEventListener('click',function(){dup.effectors.splice(i,1);renderDuplicatorEffectors(dup);dupRefreshFromPanel();});
    hdr.appendChild(delBtn);
    if((eff.falloff||'radial')==='linear'){
      var angRow=line();
      var ang=num('°',function(v){eff.angle=v;},eff.angle||0,1);angRow.appendChild(ang[0]);angRow.appendChild(ang[1]);
    }
    // Offsets: "n'importe quel property" (2026-07-30) — was 4 hardcoded
    // Pos/Rot/Scale/Opacity rows; now an arbitrary per-effector channel
    // list (eff.channels, app.js's effectorChannels — migrates a
    // pre-2026-07-30 effector's legacy 4 fields into this same shape once,
    // in place, the first time the duplicator math or this panel touches
    // it), any of SMMotion.DUP_TARGET_PROPS (adds positionZ/rotationX/
    // rotationY alongside the original 4 — the "3D aussi" half). One
    // shared row-builder handles both the 2-value (X/Y-style) and
    // 1-value properties via SMMotion.propDim/propDimLabels, so a future
    // addition to DUP_TARGET_PROPS needs no new UI code here.
    var M2=window.SMMotion;
    var targetProps=(M2&&M2.DUP_TARGET_PROPS)||['position','rotation','scale','opacity'];
    var channels=effectorChannels(eff);
    channels.forEach(function(ch,ci){
      var dim=M2?M2.propDim(ch.prop):(ch.value?ch.value.length:1);
      var dimLabels=(M2&&M2.propDimLabels(ch.prop))||null;
      var propLabel=M2?M2.propLabel(ch.prop):ch.prop;
      // Two lines per channel, not one (2026-07-30 fix — "alignement comme
      // les autres et value box trop petite"): a single line with the
      // property name PLUS 2 value fields meant 3 separate .pl labels
      // (each a fixed 68px per style.css) fighting the row's own .pi
      // flex:1 inputs for space, so a 2D property's boxes ended up tiny.
      // Name+delete get their OWN line; the value(s) go on a SEPARATE
      // `dims-row`-classed line — the exact class the Rows/Cols and X/Y
      // spacing fields above already use (style.css: `.dims-row .pl`
      // shrinks to its label's own content width instead of the fixed
      // 68px), so X/Y get to split the full row width like every other
      // multi-value field in this same panel, not a bespoke narrower one.
      var nameRow=line();
      var nameLbl=document.createElement('span');nameLbl.textContent=propLabel;nameRow.appendChild(nameLbl);
      var rmCh=document.createElement('button');rmCh.className='pbtn';rmCh.textContent='✕';rmCh.style.marginLeft='auto';
      rmCh.addEventListener('click',function(){channels.splice(ci,1);renderDuplicatorEffectors(dup);dupRefreshFromPanel();});
      nameRow.appendChild(rmCh);
      var valRow=line();valRow.classList.add('dims-row');
      if(!ch.value)ch.value=[];
      for(var vd=0;vd<dim;vd++){
        (function(vd){
          var f=num(dimLabels?dimLabels[vd]:(dim>1?String(vd+1):''),function(v){ch.value[vd]=v;},ch.value[vd]||0,1);
          valRow.appendChild(f[0]);valRow.appendChild(f[1]);
        })(vd);
      }
    });
    var addRow=line();
    var addSel=document.createElement('select');addSel.className='psel';
    var placeholder=document.createElement('option');placeholder.textContent='+ Propriété…';placeholder.value='';addSel.appendChild(placeholder);
    var already={};channels.forEach(function(ch){already[ch.prop]=true;});
    targetProps.forEach(function(p){
      if(already[p])return; // one entry per property — edit the existing row instead of stacking duplicates
      var o=document.createElement('option');o.value=p;o.textContent=M2?M2.propLabel(p):p;addSel.appendChild(o);
    });
    addSel.addEventListener('change',function(){
      if(!addSel.value)return;
      var dim=M2?M2.propDim(addSel.value):1;
      channels.push({prop:addSel.value,value:new Array(dim).fill(0)});
      renderDuplicatorEffectors(dup);dupRefreshFromPanel();
    });
    addRow.appendChild(addSel);
    list.appendChild(row);
  });
}
// Shared refresh for every dynamically-built effector field above — same
// reload+render sequence the static duplicator fields already use
// (dupRefresh, in the wiring IIFE further down), factored out here since
// this function lives above that closure.
function dupRefreshFromPanel(){loadFrame(state.currentFrame);if(window.SMEngineBridge)SMEngineBridge.renderNow();}
window.updateDuplicatorPanel=updateDuplicatorPanel;
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
  // UI/UX audit (2026-07): these tools had NO letter shortcut at all —
  // every other tool button does, so their absence read as an
  // inconsistency rather than a deliberate omission. The alphabet is
  // nearly exhausted by the bindings above (only q/s/w/y were free); no
  // mnemonic reads as cleanly as the existing ones (v=select, b=brush,
  // p=pen...) so these are arbitrary placeholders, not a claimed "right"
  // answer — rebindable via the existing Réglages > Raccourcis UI
  // (shortcutOverrides/localStorage) like any other entry here.
  {action:'text',key:'y',label:'Texte'},
  {action:'rotate',key:'w',label:'Rotation du canevas'},
  {action:'rig',key:'s',label:'Rig (Skeleton)'},
  // Deliberately NOT bound to 'q' (or anything): the Perspective rail
  // button was removed on purpose (see the comment above the button
  // markup in index.html) — perspective is reachable ONLY via the Labs
  // floating panel now. A live 'q' binding with no matching rail button
  // used to switch state.tool to 'perspective' silently: every .tool-btn
  // lost its .active class (none has data-tool="perspective" to match),
  // so the whole rail went dark with zero explanation while the cursor
  // quietly became a crosshair — found by the same audit, fixed by
  // deleting the binding rather than re-adding a button the UI review
  // that removed it explicitly didn't want back.
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
  syncToolButtonShortcutBadge(action);
}
// UI/UX audit (2026-07): rebinding a shortcut in Réglages > Raccourcis
// updated shortcutKeyFor()/localStorage correctly, but the left rail's own
// <span class="sk"> letter badge is static markup — nothing ever told it a
// rebind happened, so the toolbar kept showing the OLD default letter
// forever after a rebind (and even on a fresh load with a pre-existing
// override already in localStorage). Only touches the badge text, never
// the prose title= attribute — those aren't all "(X)"-suffixed the same
// way (e.g. Hand's is "(H or Space)"), so rewriting them generically here
// would be more likely to mangle one than to fix it.
function syncToolButtonShortcutBadge(action){
  var btn=document.querySelector('.tool-btn[data-tool="'+action+'"]');
  if(!btn)return;
  var sk=btn.querySelector('.sk');
  if(!sk)return;
  var key=shortcutKeyFor(action);
  sk.textContent=key?key.toUpperCase():'';
}
function syncAllToolButtonShortcutBadges(){
  TOOL_SHORTCUTS.forEach(function(s){syncToolButtonShortcutBadge(s.action);});
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',syncAllToolButtonShortcutBadges);else syncAllToolButtonShortcutBadges();
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
    keyBtn.title=SM.t('shortcutClickThenPressTitle');
    keyBtn.addEventListener('click',function(){
      keyBtn.textContent='…';keyBtn.classList.add('ac');
      function capture(ev){
        ev.preventDefault();ev.stopPropagation();
        if(ev.key==='Escape'){keyBtn.textContent=shortcutKeyFor(s.action);}
        else{
          // reject a key already bound to a different tool action
          var clash=TOOL_SHORTCUTS.find(function(o){return o.action!==s.action&&shortcutKeyFor(o.action)===ev.key.toLowerCase();});
          if(clash){showToast(SM.t('shortcutKeyClashToast').replace('{label}',clash.label));keyBtn.textContent=shortcutKeyFor(s.action);}
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
    btn.textContent=SM.t('commentStopRecordBtn');
    btn.classList.add('ac');
    if(status){status.style.display='block';status.textContent='🔴 Enregistrement en cours — change d\'outil et reproduis le problème, puis reviens ici cliquer Stop.';}
  }else{
    btn.textContent=SM.t('commentRecordBtn');
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
// ---- Outil texte (v20, 2026-07 rework) ----
// Le moteur Rust ne rasterise pas de glyphes : le texte est rendu une fois
// dans un canvas offscreen (2x pour la nettete) puis insere comme Raster —
// le pipeline image existant (serR/desR, registerImagePixels) fait tout le
// reste (persistance, rendu, selection/deplacement via l'outil V). Ce qui a
// change (Figma/Illustrator-style) : (1) un simple clic pose du "point text"
// (largeur auto, inchange), un DRAG pose une boite de largeur fixe ("area
// text") avec retour a la ligne automatique ; (2) double-clic sur un bloc de
// texte existant rouvre le popover pre-rempli et RE-BAKE en place (source du
// Raster remplacee via .source, meme identite d'item, meme position
// d'ancrage haut-gauche) au lieu de forcer supprimer/retaper ; (3) couleur et
// alignement sont maintenant des champs du popover, pas figes sur
// state.strokeColor au moment du clic ; (4) text/font/size/color/align/
// fixedWidth persistent sur r.data et via serR/desR (app.js) — avant cette
// session, isText lui-meme ne survivait meme pas a un save/reload.
var _textPendingPt=null,_textPendingBox=null,_textEditingRaster=null;
function positionTextPopover(worldPt){
  var pop=document.getElementById('text-popover');if(!pop)return;
  var v=view.projectToView(worldPt);
  var cr=document.getElementById('drawing-canvas').getBoundingClientRect();
  pop.style.display='block';
  pop.style.left=Math.min(window.innerWidth-240,Math.max(4,cr.left+v.x+10))+'px';
  pop.style.top=Math.min(window.innerHeight-220,Math.max(4,cr.top+v.y+10))+'px';
}
function resetTextPopoverFields(text,size,font,color,align){
  document.getElementById('text-input').value=text||'';
  document.getElementById('text-size').value=size||48;
  document.getElementById('text-font').value=font||'sans-serif';
  document.getElementById('text-align').value=align||'left';
  var colorInp=document.getElementById('text-color');if(colorInp)colorInp.value=color||state.strokeColor||'#000000';
}
function openTextPopover(worldPt){
  _textPendingPt=worldPt.clone();_textPendingBox=null;_textEditingRaster=null;
  resetTextPopoverFields();
  positionTextPopover(worldPt);
  document.getElementById('text-input').focus();
}
// Drag-to-place area text (Illustrator "type in a box") — widthWorld is in
// world units (same space as x/y/width/height everywhere else in this
// codebase), wrapping happens in commitText once the actual font/size are
// known (they aren't yet at drag time).
function openTextPopoverForBox(topLeft,widthWorld){
  _textPendingPt=null;_textPendingBox={topLeft:topLeft.clone(),width:widthWorld};_textEditingRaster=null;
  resetTextPopoverFields();
  positionTextPopover(topLeft);
  document.getElementById('text-input').focus();
}
// Re-edit an existing text raster (double-click, onViewDoubleClick in
// tools.js) — prefills every field from what was persisted on r.data and
// keeps the box's fixedWidth (if any) so re-wrapping behaves the same way.
function openTextPopoverForEdit(raster){
  _textPendingPt=null;_textEditingRaster=raster;
  // A vector-text root's group can span multiple glyph Paths whose combined
  // bounds is wider than any single one — use the whole group's bounds
  // (vectorTextGroupMembers), not just the root Path's own, so a re-edit
  // popover positions/wraps against the block's REAL current footprint.
  var isVectorRoot=!!(raster.data&&raster.data.isVectorText&&raster.data.isTextRoot);
  var groupBounds=isVectorRoot?window.SMVectorText.vectorTextGroupMembers(raster).reduce(function(b,p){return b?b.unite(p.bounds):p.bounds.clone();},null):raster.bounds;
  _textPendingBox=raster.data.fixedWidth?{topLeft:groupBounds.topLeft.clone(),width:raster.data.fixedWidth}:null;
  var fontValue=isVectorRoot?('vector:'+raster.data.vectorFont):raster.data.font;
  resetTextPopoverFields(raster.data.text,raster.data.size,fontValue,raster.data.color,raster.data.align);
  positionTextPopover(groupBounds.topLeft);
  var ta=document.getElementById('text-input');ta.focus();ta.select();
}
function closeTextPopover(){var pop=document.getElementById('text-popover');if(pop)pop.style.display='none';_textPendingPt=null;_textPendingBox=null;_textEditingRaster=null;}
// Shared layout pass (2026-07) — used by BOTH commitText's flattened bake
// AND splitTextIntoCharacters' per-character split, so a split always
// matches the flattened text pixel-for-pixel (same wrap decisions, same
// per-line x for the chosen alignment) instead of drifting from a second,
// slightly-different implementation of the same word-wrap.
function computeTextLayout(text,font,size,fixedWidthWorld){
  // Supersample factor for the offscreen bake — bumped from 2 to 3 (2026-07):
  // since this text is still fundamentally a raster (real vector glyph
  // rendering would need font-shaping work in the Rust/vello engine, out of
  // scope this session), the only lever available for "doesn't look
  // blurry when the user zooms in or exports large" is baking at a higher
  // fixed resolution up front — cheap (a few more KB per text PNG) for a
  // real crispness win at anything up to ~3x view zoom.
  var SS=3;
  var lines=text.split('\n');
  var off=document.createElement('canvas');
  var octx=off.getContext('2d');
  octx.font=size*SS+'px '+font;
  var lineH=size*SS*1.25;
  var fixedWidthPx=fixedWidthWorld?Math.max(20,fixedWidthWorld*SS):null;
  var wrapped=[];
  if(fixedWidthPx){
    // Greedy word-wrap at the fixed pixel width (minus padding) — Figma/
    // Illustrator area-text behavior. Point text (fixedWidthPx null) keeps
    // the original one-line-per-input-line, auto-width behavior untouched.
    lines.forEach(function(l){
      if(l===''){wrapped.push('');return;}
      var words=l.split(' ');var cur='';
      words.forEach(function(w){
        var test=cur?cur+' '+w:w;
        if(cur&&octx.measureText(test).width>fixedWidthPx-16){wrapped.push(cur);cur=w;}
        else cur=test;
      });
      wrapped.push(cur);
    });
  }else{
    wrapped=lines;
  }
  var wMax=1;
  if(fixedWidthPx)wMax=fixedWidthPx;
  else wrapped.forEach(function(l){wMax=Math.max(wMax,octx.measureText(l).width);});
  off.width=Math.ceil(wMax)+16;off.height=Math.ceil(wrapped.length*lineH)+16;
  octx=off.getContext('2d'); // resizing the canvas resets its context/state
  octx.font=size*SS+'px '+font;octx.textBaseline='top';
  return {off:off,octx:octx,wrapped:wrapped,lineH:lineH,SS:SS};
}
function lineDrawX(off,octx,line,align){
  var tw=octx.measureText(line).width;
  if(align==='center')return(off.width-tw)/2;
  if(align==='right')return off.width-8-tw;
  return 8;
}
// Vector text (2026-07) — "vector:<fontKey>" is the #text-font select's
// escape hatch into real glyph outlines (vector-text-bridge.js) instead of
// a Canvas2D bake. Kept as its own function (not woven into commitText's
// raster branch) since it's async (font fetch) and its "editing" case is
// remove-all-members-then-rebuild rather than reassign-one-Raster's-source.
function commitVectorText(txt,fontKey,size,align,color,fixedWidthWorld){
  var editingRoot=(_textEditingRaster&&_textEditingRaster.parent&&_textEditingRaster.data&&_textEditingRaster.data.isTextRoot)?_textEditingRaster:null;
  var topLeft;
  if(editingRoot)topLeft=window.SMVectorText.vectorTextGroupMembers(editingRoot).reduce(function(b,p){return b?b.unite(p.bounds):p.bounds.clone();},null).topLeft.clone();
  else if(_textPendingBox)topLeft=_textPendingBox.topLeft.clone();
  // Point-click placement anchors its TOP-LEFT at the click point (unlike
  // raster point-text, which centers there) — matches Illustrator's own
  // point-text tool, and there's no pre-existing raster-mode convention to
  // stay consistent with since this is a separate placement mode.
  else topLeft=_textPendingPt.clone();
  var layer=userLayers[state.activeLayerIdx];
  var ldForTag=state.layers[state.activeLayerIdx];
  var wasEmpty=layer.children.length===0;
  pushUndo();
  if(editingRoot)window.SMVectorText.vectorTextGroupMembers(editingRoot).forEach(function(p){p.remove();});
  else if(wasEmpty&&!ldForTag.isTextLayer)ldForTag.isTextLayer=true;
  closeTextPopover();
  window.SMVectorText.buildVectorTextGroup(txt,fontKey,size,color,align,fixedWidthWorld,topLeft,layer).then(function(){
    saveActiveLayerFrame();updateUI();
    if(window.SMEngineBridge)window.SMEngineBridge.renderNow();
  }).catch(function(e){
    console.warn('[vector-text] build failed',e);
    showToast('Texte vectoriel : échec du chargement de la police');
  });
}
function commitText(){
  var txt=document.getElementById('text-input').value;
  if(!txt.trim()||(!_textPendingPt&&!_textPendingBox&&!_textEditingRaster)){closeTextPopover();return;}
  var size=parseInt(document.getElementById('text-size').value,10)||48;
  var font=document.getElementById('text-font').value||'sans-serif';
  var align=document.getElementById('text-align').value||'left';
  var colorInp=document.getElementById('text-color');
  var color=(colorInp&&colorInp.value)||state.strokeColor||'#000000';
  var fixedWidthWorld=_textPendingBox?_textPendingBox.width:null;
  if(font.indexOf('vector:')===0){
    commitVectorText(txt,font.slice(7),size,align,color,fixedWidthWorld);
    return;
  }
  var L=computeTextLayout(txt,font,size,fixedWidthWorld);
  var off=L.off,octx=L.octx,wrapped=L.wrapped,lineH=L.lineH,SS=L.SS;
  octx.fillStyle=color;
  wrapped.forEach(function(l,i){
    octx.fillText(l,lineDrawX(off,octx,l,align),8+i*lineH);
  });
  var url=off.toDataURL('image/png');
  pushUndo();
  var textMeta={text:txt,font:font,size:size,color:color,align:align,fixedWidth:fixedWidthWorld||null};
  if(_textEditingRaster&&_textEditingRaster.parent){
    var r=_textEditingRaster;
    var topLeft=r.bounds.topLeft.clone(); // re-bake keeps its top-left anchor fixed, doesn't re-center on the old click point
    r.data.isText=true;r.data.src=url;
    r.data.text=textMeta.text;r.data.font=textMeta.font;r.data.size=textMeta.size;r.data.color=textMeta.color;r.data.align=textMeta.align;r.data.fixedWidth=textMeta.fixedWidth;
    r.onLoad=function(){r.size=new Size(off.width/SS,off.height/SS);r.position=topLeft.add(new Point(r.size.width/2,r.size.height/2));saveActiveLayerFrame();updateUI();if(window.SMEngineBridge)window.SMEngineBridge.renderNow();};
    r.source=url; // reassigning .source keeps the same item identity (index/parent/references) — just reloads the bitmap, same pattern as any other Raster edit-in-place in this codebase
  }else{
    var layer=userLayers[state.activeLayerIdx];
    // Auto-tag as a text layer (2026-07) — same "auto-convert on first
    // qualifying action" precedent as Motion's own layer->Component
    // conversion (CLAUDE.md §8): the FIRST text placed onto an otherwise
    // empty layer marks it, never overriding a layer the user already
    // filled with other content (that layer just gets a normal text item
    // mixed in, no auto-tag — matches "a text layer's items are still
    // ordinary Rasters", nothing here restricts what else can go on one).
    var ldForTag=state.layers[state.activeLayerIdx];
    if(layer.children.length===0&&!ldForTag.isTextLayer)ldForTag.isTextLayer=true;
    var prev=project.activeLayer;layer.activate();
    var r2=new Raster(url);
    r2.data.src=url;r2.data.isText=true;
    r2.data.text=textMeta.text;r2.data.font=textMeta.font;r2.data.size=textMeta.size;r2.data.color=textMeta.color;r2.data.align=textMeta.align;r2.data.fixedWidth=textMeta.fixedWidth;
    var pt=_textPendingBox?_textPendingBox.topLeft.clone():_textPendingPt;
    var isBox=!!_textPendingBox;
    r2.onLoad=function(){
      r2.size=new Size(off.width/SS,off.height/SS);
      r2.position=isBox?pt.add(new Point(r2.size.width/2,r2.size.height/2)):pt;
      saveActiveLayerFrame();updateUI();if(window.SMEngineBridge)window.SMEngineBridge.renderNow();
    };
    prev.activate();
  }
  closeTextPopover();
}
function initTextPopover(){
  var save=document.getElementById('text-apply');if(!save)return;
  save.addEventListener('click',commitText);
  document.getElementById('text-cancel').addEventListener('click',closeTextPopover);
  // UI/UX audit (2026-07): the popover was mouse-only — no keyboard path
  // at all (the textarea has no keydown listener, and the app's global
  // shortcut handler deliberately ignores every TEXTAREA target). Escape
  // to cancel and Ctrl/Cmd+Enter to place match this app's OWN existing
  // convention elsewhere (frame/layer rename fields: Enter commits,
  // Escape cancels, timeline.js). Plain Enter is left alone — this
  // textarea is explicitly multi-line (commitText splits on '\n'), so
  // Enter must keep inserting a newline, not submit.
  document.getElementById('text-input').addEventListener('keydown',function(e){
    if(e.key==='Escape'){e.stopPropagation();closeTextPopover();}
    else if(e.key==='Enter'&&(e.metaKey||e.ctrlKey)){e.stopPropagation();e.preventDefault();commitText();}
  });
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initTextPopover);else initTextPopover();
window.openTextPopover=openTextPopover;
window.openTextPopoverForBox=openTextPopoverForBox;
window.openTextPopoverForEdit=openTextPopoverForEdit;
// ---- Per-character split ("Découper par caractère", 2026-07) ----
// Modern text animation, Nemo-style: rather than inventing a parallel
// "text animator" system (AE's per-property character-range groups), a
// split text block becomes ORDINARY per-character Raster items sharing a
// textGroupId (same "stable id groups members, not a new item type/class"
// pattern already established for Cmd+G groups, group-bridge.js) — each
// character is then a normal Motion Element (layerElements/elementMotion,
// already generic over any stroke) and can carry its own keys AND its own
// expression (motion.js's holder.expressions), so "stagger a wiggle across
// every letter" needs zero new animation machinery, just N ordinary
// elements + N expressions. Deliberately irreversible once split (same
// "Release to Layers" precedent as splitLayerIntoElements, CLAUDE.md §8) —
// re-editing the ORIGINAL sentence after a split isn't supported; that's
// a one-way "commit to per-character" action, not a live toggle.
function splitTextIntoCharacters(raster){
  var d=raster.data;
  if(!d||!d.isText||d.isTextChar||!raster.parent)return;
  var layer=raster.parent;
  var topLeft=raster.bounds.topLeft.clone();
  var insertIdx=layer.children.indexOf(raster);
  var L=computeTextLayout(d.text||'',d.font||'sans-serif',d.size||48,d.fixedWidth||null);
  var off=L.off,octx=L.octx,wrapped=L.wrapped,lineH=L.lineH,SS=L.SS;
  var color=d.color||'#000000';
  var textGroupId='txt'+Date.now().toString(36)+'_'+Math.floor(Math.random()*1e6);
  // Count non-whitespace chars up front so the async onLoad callbacks know
  // when EVERY character has actually finished decoding before the single
  // shared saveActiveLayerFrame()/renderNow() at the end — saving right
  // after the synchronous creation loop would persist each Raster's
  // pre-onLoad placeholder geometry (0×0 at the origin) instead of its
  // real position, same race serR's own _pendingGeom comment (app.js)
  // warns about for a single raster, multiplied by every character here.
  var totalChars=0;
  wrapped.forEach(function(line){line.split('').forEach(function(ch){if(ch.trim()!=='')totalChars++;});});
  if(!totalChars)return;
  pushUndo();
  var loaded=0;
  var newRasters=[];
  function onOneLoaded(){
    loaded++;
    if(loaded>=totalChars){
      newRasters.forEach(function(cr){tagOwner(cr);});
      saveActiveLayerFrame();updateUI();
      if(window.SMEngineBridge)window.SMEngineBridge.renderNow();
    }
  }
  var prev=project.activeLayer;layer.activate();
  wrapped.forEach(function(line,li){
    if(!line)return;
    var startX=lineDrawX(off,octx,line,d.align||'left');
    var cursor=startX;
    line.split('').forEach(function(ch){
      var chW=octx.measureText(ch).width;
      if(ch.trim()===''){cursor+=chW;return;}
      var co=document.createElement('canvas');
      co.width=Math.ceil(chW)+8;co.height=Math.ceil(lineH);
      var cctx=co.getContext('2d');
      cctx.font=(d.size||48)*SS+'px '+(d.font||'sans-serif');cctx.fillStyle=color;cctx.textBaseline='top';
      cctx.fillText(ch,4,0);
      var curl=co.toDataURL('image/png');
      var cr=new Raster(curl);
      cr.data.src=curl;cr.data.isText=true;cr.data.isTextChar=true;cr.data.textGroupId=textGroupId;
      cr.data.text=ch;cr.data.font=d.font||'sans-serif';cr.data.size=d.size||48;cr.data.color=color;
      cr.insertAbove(raster);
      (function(cr,cursorX,lineIdx,coW,coH){
        cr.onLoad=function(){
          cr.size=new Size(coW/SS,coH/SS);
          cr.position=topLeft.add(new Point(cursorX/SS+cr.size.width/2,lineIdx*lineH/SS+cr.size.height/2));
          onOneLoaded();
        };
      })(cr,cursor,li,co.width,co.height);
      newRasters.push(cr);
      cursor+=chW;
    });
  });
  prev.activate();
  raster.remove();
}
window.splitTextIntoCharacters=splitTextIntoCharacters;
// Contextual panel (mirrors updateRevisionPanel's exact shape/precedent) —
// shown only when exactly one selected item is a whole (not already-split)
// text block.
function updateTextActionsPanel(){
  var sec=document.getElementById('text-actions-sec');
  if(!sec)return;
  var p=(state.tool==='select'&&selectedPaths.length===1)?selectedPaths[0]:null;
  // Vector text is excluded: every glyph is already its own Path/element —
  // there's nothing to "split", it's per-character-animatable from the
  // moment it's placed (see vector-text-bridge.js's own header comment).
  var isWholeText=!!(p&&p.data&&p.data.isText&&!p.data.isTextChar&&!p.data.isVectorText);
  sec.style.display=isWholeText?'':'none';
  if(isWholeText){
    document.getElementById('btn-text-split-chars').onclick=function(){splitTextIntoCharacters(p);};
  }
}
// Effects stack panel (2026-07 rewrite) — see effects-panel.js; the
// separate #effect-layer-sec-specific rendering that used to live here
// has been replaced by that file's unified updateEffectsPanel(), shared
// with ordinary layers.
// Gradient toggle on the fill row (2026-07-25, "les gradient et blur ont été
// travaillé mais non implémenté dans l'ui"). The feature was fully working —
// verified applying, editing and surviving save/load — but its only entry
// point was a checkbox inside a section headed "Effects", which is a
// mislabelled neighbour of the real effects stack. Unfindable unless you
// already knew where to look.
//
// This drives that SAME checkbox rather than duplicating its logic: dispatching
// a real 'change' means gradient-bridge.js's own handler does all the work, so
// there is one implementation and no second path to keep in sync.
function initFillGradientButton(){
  var btn=document.getElementById('p-fill-grad-btn');
  var cb=document.getElementById('p-grad-on');
  var editor=document.getElementById('p-fill-gradient-editor');
  if(!btn||!cb)return;
  btn.addEventListener('click',function(e){
    e.preventDefault();e.stopPropagation();
    if(cb.disabled){
      // The checkbox disables itself when there is no single shape selected —
      // say why instead of looking broken (same rule as the rest of this file).
      showToast('Sélectionne une seule forme avec l\'outil Sélection pour lui appliquer un dégradé');
      return;
    }
    if(editor)editor.style.display='block';
    cb.checked=!cb.checked;
    cb.dispatchEvent(new Event('change',{bubbles:true}));
    syncFillGradientButton();
  });
  syncFillGradientButton();
}
// Reflects the selection's real state, so the button reads as ON when the
// selected shape actually carries a gradient.
function syncFillGradientButton(){
  var btn=document.getElementById('p-fill-grad-btn');
  var cb=document.getElementById('p-grad-on');
  var editor=document.getElementById('p-fill-gradient-editor');
  if(!btn||!cb)return;
  btn.classList.toggle('on',!!cb.checked);
  btn.classList.toggle('off',!!cb.disabled);
  if(editor)editor.style.display=(!cb.disabled&&cb.checked)?'block':'none';
  btn.title=cb.disabled
    ? 'Dégradé de fill — sélectionne une seule forme avec l\'outil Sélection'
    : (cb.checked?'Dégradé de fill actif — cliquer pour revenir en aplat'
                 :'Appliquer un dégradé au fill de la forme sélectionnée');
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initFillGradientButton);
else initFillGradientButton();
function initCycleAndPropagate(){
  var cyc=document.getElementById('btn-cycle');
  if(cyc)cyc.addEventListener('click',function(){
    // Check the precondition BEFORE asking the question (2026-07-25 UX audit).
    // repeatSelection already refuses with a clear message when no frame range
    // is selected — but the prompt ran first, so the user typed a count, hit
    // OK, and only then learned they needed a selection. Worse, cancelling the
    // prompt returned before repeatSelection ever ran, so clicking this button
    // with nothing selected was completely silent. Same message, raised to the
    // point where it's actually useful.
    if(typeof selBounds==='function'&&!selBounds()){
      showToast('Sélectionne d\'abord une plage de frames dans la timeline');
      return;
    }
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
  btn.addEventListener('click',function(){renderShortcutsList();syncProfileFields();syncFolderFields();refreshFeedbackList();if(window.renderLabsPanel)renderLabsPanel();modal.style.display='flex';});
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

// Burger menu (#app-topbar, 2026-07 redesign) — Rnote-inspired File menu,
// adapted to Nemo's actual entry points: reuses window.SMProject for
// new/open/save, the existing import/export buttons (clicked
// programmatically so their own listeners run unchanged), and the
// settings modal (general pane, or jump straight to its shortcuts tab).
function initAppMenu(){
  (function bindMotionBlur(){
    var b=document.getElementById('btn-mblur');
    if(!b)return;
    b.addEventListener('click',function(){window.SM.toggleMotionBlurComp();});
    b.addEventListener('contextmenu',function(e){
      e.preventDefault();
      var s=prompt('Échantillons de flou (2-16)',String(state.motionBlurSamples||6));
      if(s===null)return;
      var sh=prompt('Ouverture d\u2019obturateur, en frames (0.05-2)',String(state.motionBlurShutter!=null?state.motionBlurShutter:0.5));
      if(sh===null)return;
      window.SM.setMotionBlurSettings(s,sh);
      showToast('Flou : '+state.motionBlurSamples+' échantillons · obturateur '+state.motionBlurShutter+' f');
    });
  })();
  (function bindShy(){
    var b=document.getElementById('btn-shy');
    if(!b)return;
    b.addEventListener('click',function(){
      window.SM.toggleShyMode();
      b.classList.toggle('active',!!state.shyEnabled);
    });
  })();
  var btn=document.getElementById('app-menu-btn');if(!btn||!window.showContextMenu)return;
  // Reads an .obj and hands it to the 3D reference viewer (labs/reference-3d.js).
// Tauri gets a native dialog, the browser preview a hidden input — same split
// every other importer here uses.
function openObjReference(){
  function feed(text){
    if(!window.SMLabs||!SMLabs.open3DReference){showToast('Visionneuse 3D indisponible');return;}
    SMLabs.open3DReference(text);
  }
  if(typeof window.__TAURI__!=='undefined'){
    window.__TAURI__.dialog.open({title:'Importer une référence 3D (.obj)',multiple:false,
      filters:[{name:'OBJ',extensions:['obj']}]}).then(function(path){
      if(!path)return;
      return window.__TAURI__.fs.readTextFile(path).then(feed);
    }).catch(function(e){showToast('Import OBJ échoué : '+e.message);});
    return;
  }
  var inp=document.createElement('input');inp.type='file';inp.accept='.obj';inp.style.display='none';
  document.body.appendChild(inp);
  inp.addEventListener('change',function(e){
    var f=e.target.files&&e.target.files[0];inp.remove();
    if(!f)return;
    var r=new FileReader();r.onload=function(){feed(r.result);};r.readAsText(f);
  });
  inp.click();
}
function clickEl(id){var el=document.getElementById(id);if(el)el.click();}
  btn.addEventListener('click',function(e){
    e.stopPropagation();
    var tt=(window.SM&&SM.t)?SM.t:function(k){return k;};
    var r=btn.getBoundingClientRect();
    var items=[
      // Live current-project label (feedback 2026-07: the right-panel
      // "Projet" section was pure duplication of this menu — everything
      // in it either already had an entry here or is added below. This
      // disabled row replaces its old #proj-current text readout, the one
      // thing in that section with no action of its own.
      {label:(window.SMProject&&window.SMProject.getCurrentLabel)?window.SMProject.getCurrentLabel():tt('menuUntitledNotSaved'),disabled:true},
      {sep:true},
      {label:tt('menuNewProject'),shortcut:'⌘N',action:function(){clickEl('project-tab-add');}},
      {label:tt('menuOpen'),shortcut:'⌘O',action:function(){if(window.SMProject)window.SMProject.open();}},
      {label:tt('menuSave'),shortcut:'⌘S',action:function(){if(window.SMProject)window.SMProject.save();}},
      {label:tt('menuSaveAs'),shortcut:'⇧⌘S',action:function(){if(window.SMProject)window.SMProject.saveAs();}},
      {label:tt('menuFromKitsu'),id:'ctx-kitsu-open',action:function(){clickEl('btn-kitsu-open');}},
      // Nemo's own extensibility (nemo-script.js / nemo-plugin.js) — this
      // app's model in this app's vocabulary, so it ships.
      {label:'Ouvrir un script Nemo (.js)…',id:'ctx-nemo-script',
        action:function(){if(window.SMScript)SMScript.openFile();}},
      {label:'Ouvrir un plugin Nemo (.zip)…',id:'ctx-nemo-plugin',
        action:function(){if(window.SMPlugin)SMPlugin.openFile();}},
      {label:tt('menuVersionHistory'),id:'ctx-history',action:function(){clickEl('btn-history');}},
      {sep:true},
      {label:tt('menuImportImg'),action:function(){clickEl('btn-import-img');}},
      {label:tt('menuImportVideo'),action:function(){clickEl('btn-import-video');}},
      {label:tt('menuImportPsd'),id:'ctx-import-psd',action:function(){clickEl('btn-import-psd');}},
      // SVG sits with the other importers now, not behind a Labs flag and a
      // floating button (2026-07-27): it is the only import path that yields
      // EDITABLE geometry instead of a flat Raster, so it is the natural
      // front door for a logo or a turnaround.
      {label:tt('menuImportSvg'),id:'ctx-import-svg',
        action:function(){if(window.SMSvgImport)SMSvgImport.openFile();}},
      // The OBJ reference viewer's loader existed and worked, with nothing
      // anywhere calling it — SMLabs.open3DReference had zero call sites, so
      // only the two bundled CC0 models were ever reachable. This is that
      // missing entry point. It stays a REFERENCE (an overlay you draw from,
      // never exported, never baked), which is what the viewer is today.
      {label:tt('menuImport3D'),id:'ctx-import-3d',action:openObjReference},
      {label:tt('menuExport'),id:'ctx-export',action:function(){clickEl('btn-export');}},
      {sep:true},
      {label:tt('menuSettings'),action:function(){clickEl('btn-settings');}},
      {label:tt('menuKeyboardShortcuts'),action:function(){
        clickEl('btn-settings');
        var t=document.querySelector('#settings-tabs .settings-tab[data-tab="shortcuts"]');
        if(t)t.click();
      }},
      {label:tt('menuAboutNemo'),action:function(){
        clickEl('btn-settings');
        var t=document.querySelector('#settings-tabs .settings-tab[data-tab="updates"]');
        if(t)t.click();
      }}
    ];
    // Kitsu publish only makes sense once a shot is actually open — mirrors
    // the old kitsu-shot-row/kitsu-publish-row's own show/hide condition
    // (kitsu.js's updateKitsuShotUI), just decided fresh at menu-open time
    // instead of a persistent DOM row kept in sync.
    var ks=window.SMKitsu&&window.SMKitsu.getCurrentShot&&window.SMKitsu.getCurrentShot();
    if(ks){
      items.splice(1,0,{label:'Kitsu: '+ks.projectName+(ks.sequenceName?' / '+ks.sequenceName:'')+' / '+ks.shotName+(ks.taskName?' ('+ks.taskName+')':''),disabled:true});
      items.splice(2,0,{label:tt('menuPublishToKitsu'),id:'ctx-kitsu-publish',action:function(){clickEl('btn-kitsu-publish');}});
    }
    window.showContextMenu(r.left,r.bottom+4,items);
  });
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initAppMenu);else initAppMenu();

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

// Design audit 2026-07 (feedback: "ajoute les raccourcis b et n comme
// after pour ajuster le in et out point du work area"): B/N are ALREADY
// TOOL_SHORTCUTS ('draw'/'fillbrush') here, unlike real AE where those
// letters are free — so the work-area override below only fires while the
// pointer is actually over the timeline (this flag), same "contextual
// override, tool shortcut untouched everywhere else" idea as the
// Motion-only P/A/R/S/T/U overrides in onKeyDown, just hover-gated instead
// of mode-gated since work area is a timeline-wide concept (Animation 2D
// AND Motion), not specific to one mode/layer-expanded state.
var _pointerOverTimeline=false;
(function(){
  var wrap=document.getElementById('fg-wrap');
  if(!wrap)return;
  wrap.addEventListener('pointerenter',function(){_pointerOverTimeline=true;});
  wrap.addEventListener('pointerleave',function(){_pointerOverTimeline=false;});
})();

// Walks the layer list AS DISPLAYED, not state.layers order: the panel renders
// highest index first (computeLayerRenderOrder counts down), and members of a
// collapsed folder carry hidden:true and must be stepped over — otherwise
// PageDown would appear to do nothing while it moved onto a row that isn't on
// screen. dir -1 goes up the panel, +1 goes down.
function stepActiveLayer(dir){
  var idxs;
  if(typeof computeLayerRenderOrder==='function'){
    idxs=computeLayerRenderOrder()
      .filter(function(e){return e.type==='layer'&&!e.hidden;})
      .map(function(e){return e.idx;});
  }else{
    idxs=[];for(var i=state.layers.length-1;i>=0;i--)idxs.push(i);
  }
  if(idxs.length<2)return false;
  var pos=idxs.indexOf(state.activeLayerIdx);
  if(pos<0)pos=0;
  var np=pos+dir;
  if(np<0||np>=idxs.length)return false; // already at the end — stop, don't wrap
  // Goes through the app's own layer-activation path rather than assigning
  // activeLayerIdx directly: that one also saves the outgoing layer's frame,
  // clears the selection, and unsticks the camera row (see setActiveLayer).
  window.SM.setActiveLayer(idxs[np]);
  return true;
}
function onKeyDown(event){
  // Editing a text/number field: leave Ctrl+Z/X/C/V to the BROWSER's own
  // in-field behavior (text undo, text cut/copy/paste) instead of
  // hijacking them app-wide — found live (2026-07-17, "les paramètres de
  // valeurs de properties doivent aussi pouvoir être ctrl+z"): these
  // handlers ran unconditionally (the INPUT guard further down only
  // protects the single-letter shortcuts BELOW it), so typing a value in
  // a Motion scrub field then hitting Ctrl+Z fired the APP undo against
  // some earlier action while the field kept the typed-but-uncommitted
  // text — which then committed anyway on blur. Felt exactly like "undo
  // doesn't work on property values". Standard pro-app convention (AE,
  // Figma): field focused → Ctrl+Z is text-level undo; blur/Esc first
  // for app-level undo. Ctrl+S stays global — native "save page" is
  // never what anyone wants here.
  var inField=event.target.tagName==='INPUT'||event.target.tagName==='SELECT'||event.target.tagName==='TEXTAREA'||event.target.isContentEditable;
  // Enter in a (single-line) field confirms AND releases focus — same
  // convention as AE/Figma, and the necessary complement to the in-field
  // Ctrl+Z guard below: without the blur, focus stayed in the field after
  // Enter, so the very next Ctrl+Z was still treated as text-level undo
  // instead of undoing the just-committed value.
  if(event.key==='Enter'&&event.target.tagName==='INPUT'){event.target.blur();return;}
  if((event.metaKey||event.ctrlKey)&&event.key==='z'){if(inField)return;event.preventDefault();if(event.shiftKey)redo();else undo();return;}
  if((event.metaKey||event.ctrlKey)&&event.key==='s'){event.preventDefault();if(event.shiftKey)window.SMProject.saveAs();else window.SMProject.save();return;}
  // ⌘C/⌘X/⌘V routes to whichever clipboard is relevant: a live canvas shape
  // selection (selectedPaths, Select/Subselect tool) takes priority over a
  // timeline keyframe-cell selection (_sel.frames) when BOTH happen to be
  // non-empty — matches how ⌘D (duplicate, below) and Delete already treat
  // a canvas selection as the more specific/intentional one. Paste has no
  // "current selection" to disambiguate from, so it follows whichever
  // clipboard was filled most recently (2026-07, "vérifie que copier/
  // couper/coller existe pour tous les éléments... les keyframes" — canvas
  // shapes had NO copy/cut/paste at all before this, only keyframes did).
  // A Motion keyframe selection is the most specific thing ⌘C can mean, so it
  // wins over shapes and frame cells — same "more specific selection wins"
  // rule the shape-vs-frame split below already follows. Gated on there BEING
  // a keyframe selection, so ⌘C is unchanged everywhere else.
  if((event.metaKey||event.ctrlKey)&&event.key==='c'){if(inField)return;event.preventDefault();
    if(state.appMode==='motion'&&window.SMMotion&&SMMotion.hasKeySelection&&SMMotion.hasKeySelection()){SMMotion.copySelectedKeys();return;}
    if(selectedPaths.length)copySelection();else window.SM.copyFrames();return;}
  if((event.metaKey||event.ctrlKey)&&event.key==='x'){if(inField)return;event.preventDefault();if(selectedPaths.length)cutSelection();else window.SM.cutFrames();return;}
  if((event.metaKey||event.ctrlKey)&&event.key==='v'){if(inField)return;event.preventDefault();
    if(state.appMode==='motion'&&window.SMMotion&&SMMotion.hasKeyClipboard&&SMMotion.hasKeyClipboard()){SMMotion.pasteKeys();return;}
    if(window._lastClipKind==='canvas'&&_canvasClip&&_canvasClip.snaps.length)pasteSelection();
    else if(window._lastClipKind==='frames'&&_sel.clipboard&&_sel.clipboard.length)window.SM.pasteFrames();
    else if(_canvasClip&&_canvasClip.snaps.length)pasteSelection();
    else window.SM.pasteFrames();
    return;
  }
  // UI/UX audit (2026-07): Ctrl/Cmd+A "select all" existed nowhere in the
  // app — a near-universal convention in every creative tool. Selects
  // every path/raster on the ACTIVE layer at the current frame, mirroring
  // the exact filter select-bridge.js already uses for its own "click on
  // empty layer space selects everything in it" fallback (isLinkedFill
  // companions and brush-texture dab copies excluded — they're rendering
  // artifacts of their anchor stroke, not independently selectable
  // drawings). Scoped to the Select/Subselect tools only: select-all while
  // Brush/Pen/etc are active would be a silent, confusing side effect with
  // no visible feedback for what tool is active.
  if((event.metaKey||event.ctrlKey)&&(event.key==='a'||event.key==='A')&&(state.tool==='select'||state.tool==='subselect')&&userLayers[state.activeLayerIdx]){
    event.preventDefault();
    clearSel();
    selectedPaths=userLayers[state.activeLayerIdx].children.filter(function(c){return(c instanceof Path||c instanceof Raster)&&isSelectablePathChild(c);});
    state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
    renderArcs();updateUI();
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
    return;
  }
  // Ctrl/Cmd+D "duplicate the selected object(s)" — see duplicateSelection's
  // own comment (tools.js) for why this was needed: the footer hint has
  // claimed this combo for object duplication for a long time, but nothing
  // was ever wired to it. Plain 'd' (no modifier, handled further down)
  // stays bound to duplicateKeyframe — a real, distinct, still-useful
  // action (clones the whole current frame) — untouched.
  // Cmd/Ctrl+Shift+D — AE's "split layer at the playhead". Checked BEFORE
  // the plain Cmd+D below, which would otherwise swallow it whenever canvas
  // objects happen to be selected.
  if((event.metaKey||event.ctrlKey)&&event.shiftKey&&(event.key==='d'||event.key==='D')){
    event.preventDefault();
    window.SM.splitLayerAtPlayhead();
    return;
  }
  if((event.metaKey||event.ctrlKey)&&(event.key==='d'||event.key==='D')&&selectedPaths.length){
    event.preventDefault();
    duplicateSelection();
    return;
  }
  // Ctrl/Cmd+G "group selected CANVAS objects" (2026-07, feedback: "mettre
  // en place les groupes, avec command+g, select les éléments et ça permet
  // d'avoir... une seule boite de transformation") — takes priority over
  // the PRE-EXISTING layer-panel "group into folder" binding just below
  // when there's an actual canvas selection, since Select-tool grouping is
  // the more central/expected meaning of Cmd+G. Falls through to the
  // folder-grouping behavior unchanged when there's no canvas selection
  // (e.g. 2+ layers selected in the layer list instead) — group-bridge.js's
  // own groupSelection() already no-ops+toasts on <2 selectedPaths, so this
  // guard just decides which of the two "group" actions gets first refusal.
  if((event.metaKey||event.ctrlKey)&&(event.key==='g'||event.key==='G')&&!event.shiftKey&&window.selectedPaths&&selectedPaths.length>=2){
    event.preventDefault();
    if(window.SMGroup)SMGroup.groupSelection();
    return;
  }
  // Ctrl/Cmd+Shift+G "ungroup" — free shortcut (unused elsewhere), only
  // acts when the current canvas selection actually has grouped members;
  // otherwise falls through (currently nothing else claims this combo).
  if((event.metaKey||event.ctrlKey)&&(event.key==='g'||event.key==='G')&&event.shiftKey&&window.selectedPaths&&selectedPaths.length){
    event.preventDefault();
    if(window.SMGroup)SMGroup.ungroupSelection();
    return;
  }
  // Ctrl/Cmd+G "group into folder" — the action already existed
  // (groupSelectionIntoFolder, right-click menu only) but every other
  // grouping-adjacent action in this app has a keyboard path; this one
  // didn't. Same guard as the menu entry (2+ consecutive, ungrouped
  // layers) so the shortcut can't silently no-op without the SAME
  // showToast explanation the menu item already gives on failure.
  if((event.metaKey||event.ctrlKey)&&(event.key==='g'||event.key==='G')&&!event.shiftKey){
    event.preventDefault();
    groupSelectionIntoFolder();
    return;
  }
  // Ctrl/Cmd+Alt +/-/0 canvas zoom — mouse wheel already zoomed the canvas
  // (tools.js), but had no keyboard equivalent. Three rounds of live
  // feedback, each collision a different flavor of the same problem:
  // plain Ctrl+=/-/0 is the base OS/browser page-zoom accelerator;
  // Ctrl+Shift+=/-/0 (round 2's fix) turned out to be Safari's own
  // documented ALTERNATE zoom-in accelerator (Shift is how "+" is reached
  // without a numpad on many layouts, so Shift+= often just IS "+" as far
  // as the OS/browser's own zoom listener is concerned) — round 2's
  // report ("13: ça bloque juste le zoom") was the WebView itself
  // zooming, not this app. Bare +/-/0 (no modifier at all) was
  // considered and rejected: "+"/"=" is ALREADY bound, unmodified, to
  // extendExposure(1) a few lines below (real, pre-existing Animate-style
  // "extend this frame" shortcut) — dropping the modifier would have
  // silently broken that instead of fixing zoom. Ctrl+Alt is the
  // remaining safe choice: essentially never reserved by any OS or
  // browser (unlike Ctrl and Ctrl+Shift, which both are, in different
  // ways, on different platforms). event.code (Equal/Minus/Digit0 +
  // Numpad variants), not event.key — still layout-proof against AZERTY,
  // same reasoning as round 2.
  if((event.metaKey||event.ctrlKey)&&event.altKey&&(event.code==='Equal'||event.code==='NumpadAdd')){event.preventDefault();view.zoom=Math.min(20,view.zoom*1.1);updZoom();renderArcs();if(window.SMEngineBridge)SMEngineBridge.renderNow();return;}
  if((event.metaKey||event.ctrlKey)&&event.altKey&&(event.code==='Minus'||event.code==='NumpadSubtract')){event.preventDefault();view.zoom=Math.max(.05,view.zoom/1.1);updZoom();renderArcs();if(window.SMEngineBridge)SMEngineBridge.renderNow();return;}
  if((event.metaKey||event.ctrlKey)&&event.altKey&&(event.code==='Digit0'||event.code==='Numpad0')){event.preventDefault();window.SM.fitCanvas();return;}
  if(event.target.tagName==='INPUT'||event.target.tagName==='SELECT'||event.target.tagName==='TEXTAREA'||event.target.isContentEditable)return;
  var k=event.key;
  // Motion mode's P/A/R/S/T property-reveal shortcuts (After Effects
  // convention, explicitly requested) DELIBERATELY take priority over the
  // normal tool shortcuts below — p/r/a are otherwise bound to Pen/
  // Rectangle/Subselect and s to Rig (TOOL_SHORTCUTS above; found missing
  // from this comment by a UI/UX audit — the collision itself was already
  // correct in code, only the explanation was incomplete) — and t is
  // hardcoded to generateTweens a few lines down. Scoped tightly (Motion
  // mode active AND
  // a layer's Transform group actually expanded) so this contextual
  // override never fires outside Motion mode — the toolbar buttons for
  // those tools still work unaffected everywhere, only the KEYBOARD letter
  // is reassigned while a layer is expanded here, same as AE's own P/A/R/S/T
  // only means "reveal this property" when a layer is selected in its
  // Timeline panel.
  // The "a layer's Transform group actually expanded" half of that scoping is
  // gone (2026-07-27: "si je fais 'p' alors ça affiche seulement toute les
  // prop position de tous les calques ou ceux de la sélection"): in AE you
  // press P to OPEN Position, you don't open a layer first and then filter
  // it. handlePropShortcut reveals its own targets now, so requiring one to
  // be open beforehand meant the key did nothing from a clean timeline —
  // which is exactly when you reach for it. Motion mode alone is scope
  // enough; the toolbar buttons for Pen/Rect/Subselect are untouched.
  if(state.appMode==='motion'&&window.SMMotion&&SMMotion.handlePropShortcut(k,event.shiftKey)){event.preventDefault();return;}
  // AE's "U" — reveal animated properties on the selected layer(s) (or
  // every layer if none selected), explicit request. Takes priority over
  // the normal tool shortcuts below for the same reason P/A/R/S/T does
  // ('u' is otherwise bound to the Line tool, TOOL_SHORTCUTS) — only inside
  // Motion mode, the Line tool shortcut is untouched everywhere else.
  if((k==='u'||k==='U')&&state.appMode==='motion'&&window.SMMotion&&SMMotion.revealAnimated()){event.preventDefault();return;}
  // E / M — Van Dijk 5.1's siblings of U. Guarded on Motion mode like U, and
  // checked AFTER the tool shortcuts they'd otherwise shadow: `e` is only a
  // reveal here, never in Animation 2D where it may be a tool letter.
  if((k==='e'||k==='E')&&!event.metaKey&&!event.ctrlKey&&!event.altKey&&state.appMode==='motion'&&window.SMMotion&&SMMotion.revealEffects()){event.preventDefault();return;}
  if((k==='m'||k==='M')&&!event.metaKey&&!event.ctrlKey&&!event.altKey&&state.appMode==='motion'&&window.SMMotion&&SMMotion.revealMattes()){event.preventDefault();return;}
  // Shift+F3 — AE's own Graph Editor toggle. Motion-only, like every other
  // binding in this block.
  if(k==='F3'&&event.shiftKey&&state.appMode==='motion'&&window.SMMotionGraph){event.preventDefault();SMMotionGraph.toggle();return;}
  // ---- Motion keyframe keyboard layer (2026-07-25) ----
  // The Motion timeline already had drag-retime, marquee multi-select, hold,
  // distribute/flip/select-every-2nd and a per-segment ease editor — but every
  // one of them was reachable ONLY by right-click. After Effects is
  // keyboard-first for exactly these operations, which is most of what "a real
  // AE timeline" means in daily use, so they get the AE bindings here.
  //
  // Every branch is gated on an actual keyframe selection, so with nothing
  // selected these keys keep their existing meanings untouched — Delete still
  // deletes canvas objects, arrows still step the playhead, Cmd+C/V still route
  // to shapes or frame cells.
  if(state.appMode==='motion'&&window.SMMotion&&SMMotion.hasKeySelection&&SMMotion.hasKeySelection()){
    // F9 / Shift+F9 / Cmd+Shift+F9 — Easy Ease, Easy Ease In, Easy Ease Out.
    if(k==='F9'){event.preventDefault();SMMotion.applyEasyEase(event.shiftKey?((event.metaKey||event.ctrlKey)?'easeOut':'easeIn'):'ease');return;}
    // Ctrl/Cmd+Alt+K — linear (AE has no single default binding for this; K is
    // free here and reads as "keyframe interpolation").
    if((event.metaKey||event.ctrlKey)&&event.altKey&&(k==='k'||k==='K')){event.preventDefault();SMMotion.setKeyInterp('linear');return;}
    // Ctrl/Cmd+Alt+H — toggle hold, AE's own binding.
    if((event.metaKey||event.ctrlKey)&&event.altKey&&(k==='h'||k==='H')){event.preventDefault();SMMotion.setKeyInterp('hold');return;}
    if(k==='Delete'||k==='Backspace'){event.preventDefault();SMMotion.deleteSelectedKeys();return;}
    // Alt+Left/Right nudges the SELECTED KEYS in time; bare arrows keep moving
    // the playhead, which is the distinction AE draws too.
    if(event.altKey&&(k==='ArrowLeft'||k==='ArrowRight')){event.preventDefault();SMMotion.nudgeSelectedKeys(k==='ArrowRight'?1:-1);return;}
    // ⌘C/⌘V are claimed much earlier in this handler (they must be, to sit
    // alongside the shape and frame-cell clipboards) — the Motion branch lives
    // there, not here.
  }
  // AE's B/N — "Set Work Area Start/End" at the current frame. See the
  // _pointerOverTimeline comment above this function for why this is
  // hover-scoped rather than global (unlike real AE, these letters are
  // already real tool shortcuts here). Same clamp rules as dragging
  // #wa-bar's own handles (ui.js initWaDrag) — in can't cross past
  // out-1 and vice versa.
  if((k==='b'||k==='B')&&_pointerOverTimeline){
    event.preventDefault();
    var curOut=(state.waOut!=null)?state.waOut:state.totalFrames-1;
    state.waIn=Math.max(0,Math.min(state.currentFrame,curOut-1));
    if(window.updateWaBar)updateWaBar();
    return;
  }
  if((k==='n'||k==='N')&&_pointerOverTimeline){
    event.preventDefault();
    var curIn=(state.waIn!=null)?state.waIn:0;
    state.waOut=Math.min(state.totalFrames-1,Math.max(state.currentFrame,curIn+1));
    if(window.updateWaBar)updateWaBar();
    return;
  }
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
  // UI/UX audit (2026-07-30): X was bound twice in this same chain — once
  // here (fires first, always wins) and once further down to flipHorizontal/
  // flipVertical (shiftKey-gated), making the second binding permanently
  // unreachable dead code even though both functions are fully implemented
  // and useful. Merged into one branch: X alone keeps its existing meaning
  // (swap stroke/fill), Shift+X/Shift+Alt+X reach the flip functions that
  // were otherwise unreachable from any key, menu, or button.
  else if(k==='x'||k==='X'){
    if(event.shiftKey){if(event.altKey)window.SM.flipVertical();else window.SM.flipHorizontal();}
    else window.SM.swapStrokeFill();
  }
  else if(k==='t'||k==='T')window.SM.generateTweens();
  // [ ] brush/eraser size — Photoshop/Procreate/Clip Studio convention,
  // absent here entirely before this (grepped: no bracket-key handler
  // anywhere). 1px per press, 5px with Shift; eraser gets its own
  // state.eraserSize (setEraserSize already clamps at a 2px floor) rather
  // than state.brushSize so this doesn't cross-affect the two. When the
  // Select/Subselect tool has a live selection, setBrushSize already
  // restyles it in place (see its own comment, timeline.js) — bracket
  // keys inherit that for free.
  else if((k==='['||k===']')&&!event.altKey){
    var sizeStep=(event.shiftKey?5:1)*(k===']'?1:-1);
    if(state.tool==='eraser')window.SM.setEraserSize(state.eraserSize+sizeStep);
    else window.SM.setBrushSize(Math.max(1,Math.min(80,state.brushSize+sizeStep)));
    updateUI();
  }
  // SPACE — tap plays, hold pans (2026-07-25, "les choses basiques qui
  // manquent... la barre espace qui fait play par exemple"). Nemo is both a
  // drawing app and an animation app, and the two conventions collide head-on:
  // Space is the temporary Hand in Photoshop/Procreate/Krita/Clip Studio, and
  // play/pause in After Effects/Premiere/TVPaint/Blender. After Effects
  // resolves it by gesture rather than by picking a side — hold and drag to
  // pan the comp, tap and release to play — so both live on the same key with
  // no mode and nothing to learn. Same resolution here: this handler only ever
  // ARMS the pan (unchanged behaviour); onKeyUp decides, from whether a drag
  // actually happened and how long the key was held, whether it was a tap.
  else if(k===' '){
    event.preventDefault();
    if(!state.spaceDown){
      state.spaceDown=true;
      state._spaceAt=(window.performance&&performance.now)?performance.now():Date.now();
      state.spaceUsedForPan=false;
      canvasEl.style.cursor='grab';
    }
  }
  else if(k==='Alt'){state.altDown=true;}
  // Rig: Enter-to-finish / Escape-to-cancel an in-progress bone (2026-07-29
  // fix, QA-confirmed "on ne sait pas comment finir les traits de bones") —
  // rig-bridge.js's bone drawing otherwise ONLY finished via double-click or
  // clicking back near the first anchor (Pen-tool convention), but Pen ITSELF
  // already has this exact Enter/Escape pair a few lines below — Rig
  // silently never got the same pair when it was built, even though it
  // mirrors Pen's drawing interaction everywhere else.
  else if(k==='Enter'&&state.tool==='rig'&&typeof _rigDraw!=='undefined'&&_rigDraw.path){event.preventDefault();if(window.SMRig)SMRig.finalizeRigBone();updateUI();}
  else if(k==='Enter'){event.preventDefault();if(state.tool==='pen'&&_pen.path)finalizePen();else togglePlay();}
  else if(k==='Escape'){
    if(state.tool==='rig'&&typeof _rigDraw!=='undefined'&&_rigDraw.path){
      if(window.SMEngineBridge&&window.SMEngineBridge.setRigPreview)window.SMEngineBridge.setRigPreview(null);
      _rigDraw.path.remove();
      if(state.undoStack.length)state.undoStack.pop();
      _rigDraw.path=null;_rigDraw.boneId=null;_rigDraw.ld=null;_rigDraw.draggingHandle=false;
      updateUI();
    }
    if(state.tool==='pen'&&_pen.path){if(_pen.previewLine){_pen.previewLine.remove();_pen.previewLine=null;}_pen.path.remove();if(state.undoStack.length)state.undoStack.pop();_pen.path=null;_pen.draggingHandle=false;saveActiveLayerFrame();updateUI();}
    // UI/UX audit (2026-07): the footer hint has claimed "Échap
    // Désélectionner" for an object selection, but this handler only ever
    // covered cancelling a pen tool draw-in-progress — deselecting a
    // plain Select/Subselect selection had no Escape path at all,
    // confirmed live via a real dispatched keydown (selection count
    // unchanged before/after). clearSel() also resets any in-progress
    // node marquee/xform anchor override, matching what clicking empty
    // canvas is supposed to do.
    else if(selectedPaths.length){clearSel();renderArcs();updateUI();}
  }
  // Arrow-key nudge for the current selection — the statusbar hint has
  // claimed "↑↓←→ Déplacer" whenever something's selected (see
  // statusbarHelpRender below) since that hint was written, but no code
  // path ever actually moved anything: ArrowLeft/Right unconditionally
  // scrubbed the timeline frame instead (below), and ArrowUp/Down did
  // nothing at all. Live UI/UX audit, 2026-07: confirmed via
  // document.activeElement + a real drag/nudge test that the hint was
  // simply false. selectedPaths is guaranteed empty while any tool other
  // than Select/Subselect is active (timeline.js's own tool-switch
  // handler clears it), so gating on its length here can't steal arrow
  // keys from Brush/Pen/etc — only true when the Select tool actually has
  // something selected, exactly matching when the hint is shown. 1px per
  // press, 10px with Shift (standard Illustrator/Figma convention).
  else if(selectedPaths.length&&(k==='ArrowLeft'||k==='ArrowRight'||k==='ArrowUp'||k==='ArrowDown')){
    event.preventDefault();
    var nudgeStep=event.shiftKey?10:1;
    var ndx=k==='ArrowLeft'?-nudgeStep:k==='ArrowRight'?nudgeStep:0;
    var ndy=k==='ArrowUp'?-nudgeStep:k==='ArrowDown'?nudgeStep:0;
    selPropsApplyMove(ndx,ndy);
  }
  else if(k==='ArrowLeft'){if(state.playing)stopPlay();goToFrame(state.currentFrame-1);}
  else if(k==='ArrowRight'){if(state.playing)stopPlay();goToFrame(state.currentFrame+1);}
  // Home/End "go to first/last frame" — a near-universal NLE/AE convention
  // (Home = start of composition, End = end) that had no binding here at
  // all (grepped: zero 'Home'/'End' handlers before this). AE audit, 2026-07.
  // PageUp / PageDown — step the active layer up/down the panel (2026-07-25).
  // Changing the active layer is one of the most frequent actions in any
  // multi-layer drawing app (rough here, clean there, background below) and
  // had NO keyboard path at all: Alt+arrows, Ctrl+arrows, PageUp/PageDown and
  // Alt+[ ] were all verified inert for it. Alt+[ / Alt+] are taken (AE
  // in/out-point trim) and plain [ ] are brush size, so PageUp/PageDown —
  // entirely unclaimed here, and Krita's own binding for exactly this.
  else if(k==='PageUp'||k==='PageDown'){event.preventDefault();stepActiveLayer(k==='PageUp'?-1:1);}
  else if(k==='Home'){if(state.playing)stopPlay();goToFrame(0);}
  else if(k==='End'){if(state.playing)stopPlay();goToFrame(state.totalFrames-1);}
  // Alt+[ / Alt+] "trim layer in/out point to current time" (AE convention)
  // — layer-inout.js already has the full in/out-point system (draggable
  // bar/handles, layerInPoint/layerOutPoint, app.js) but only via mouse
  // drag; AE users reach for this shortcut constantly and it was entirely
  // unbound. Mirrors the exact set/clamp logic layer-inout.js's own drag
  // handler uses (ld.inPoint clamped below outPoint, and vice versa) so
  // this can never invert the range. Plain [ ] stay bound to brush/eraser
  // size (established earlier this session) — Alt is free and unclaimed.
  else if(event.altKey&&k==='['){
    var ldIn=state.layers[state.activeLayerIdx];
    if(ldIn&&!ldIn.symbolId){
      pushUndo();
      var curOut=window.layerOutPoint?layerOutPoint(ldIn):(ldIn.outPoint!=null?ldIn.outPoint:state.totalFrames-1);
      ldIn.inPoint=Math.max(0,Math.min(state.currentFrame,curOut-1));
      renderTimeline();loadFrame(state.currentFrame);
      if(window.SMEngineBridge)SMEngineBridge.renderNow();
    }
  }
  else if(event.altKey&&k===']'){
    var ldOut=state.layers[state.activeLayerIdx];
    if(ldOut&&!ldOut.symbolId){
      pushUndo();
      var curIn=window.layerInPoint?layerInPoint(ldOut):(ldOut.inPoint!=null?ldOut.inPoint:0);
      ldOut.outPoint=Math.max(curIn+1,Math.min(state.currentFrame,state.totalFrames-1));
      renderTimeline();loadFrame(state.currentFrame);
      if(window.SMEngineBridge)SMEngineBridge.renderNow();
    }
  }
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
    else if(state.tool==='fsselect'&&_fsSel.length){event.preventDefault();fsApplyDelete();}
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
  else if(k==='+'||k==='='){window.SM.extendExposure(1);}
}
// Longest hold still read as a tap. Generous enough to survive a slow
// keypress, short enough that parking a thumb on Space to pan and thinking
// better of it never starts playback.
var SPACE_TAP_MS=250;
function onKeyUp(event){if(event.key===' '){
  // wasDown gates the whole thing: onKeyDown returns before its Space branch
  // whenever a field has focus, so without this a Space typed into a text
  // input would still reach here and start playback on release.
  var wasDown=state.spaceDown;
  var held=((window.performance&&performance.now)?performance.now():Date.now())-(state._spaceAt||0);
  var tapped=wasDown&&!state.spaceUsedForPan&&held<SPACE_TAP_MS;
  state.spaceDown=false;state.isPanning=false;state.spaceUsedForPan=false;
  var cc={draw:'crosshair',pen:'crosshair',line:'crosshair',rect:'crosshair',ellipse:'crosshair',select:'default',subselect:'default',eraser:'pointer',fill:'crosshair',fillbrush:'crosshair',eyedropper:'crosshair',hand:'grab',zoom:'zoom-in',rotate:'grab',perspective:'crosshair'};canvasEl.style.cursor=cc[state.tool]||'default';
  // Mid-path the Pen tool owns Enter/Escape to finish or cancel; starting
  // playback under it would strand a half-drawn path, so Space stays inert
  // there — the one place the tap gesture is suppressed.
  if(tapped&&!(state.tool==='pen'&&_pen.path))togglePlay();
}else if(event.key==='Alt'){state.altDown=false;}}
// One capture-phase listener marks "the Space hold was actually used", rather
// than touching either pan implementation: panning lives in BOTH tools.js
// (Paper.js path) and viewtools-bridge.js (Rust engine path), and any pointer
// press while Space is held belongs to the pan gesture whether or not it ends
// up moving far enough to shift the view.
document.addEventListener('pointerdown',function(){if(state.spaceDown)state.spaceUsedForPan=true;},true);

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
// Figma-style color row (2026-07, "les couleurs ça peut être un système
// comme [Figma]") — inline eye toggles + editable hex/alpha text fields
// right in the Fill/Stroke rows, alongside the existing swatch popover.
document.getElementById('fill-enable-toggle').addEventListener('click',function(){window.SM.setFillEnabled(!state.fillEnabled);});
document.getElementById('stroke-enable-toggle').addEventListener('click',function(){window.SM.setStrokeEnabled(!state.strokeEnabled);});
// Formats a color for the inline hex text field: no '#', uppercase, and
// the alpha byte dropped when it's just FF (fully opaque) — showing
// "FF0000FF" for a plain opaque red would read as broken/unexpected to
// anyone not already thinking in hex8.
function hexDisplayValue(hex){
  var h=(hex||'#000000').replace('#','');
  if(h.length===8&&h.slice(6).toUpperCase()==='FF')h=h.slice(0,6);
  return h.toUpperCase();
}
// Reads the alpha byte (if present) as a 0-100 percentage, same "#rrggbb
// == fully opaque" convention color-picker.js's own hexToAlpha uses.
function alphaPctFromHex(hex){
  var h=(hex||'').replace('#','');
  if(h.length!==8)return 100;
  return Math.round((parseInt(h.slice(6,8),16)||0)/255*100);
}
// Parses a typed hex string (with or without '#', 3/6/8 digits) into a
// normalized '#rrggbb'/'#rrggbbaa' string, or null if it's not valid yet
// (mid-typing) — shared by both hex fields below.
function parseHexInput(raw){
  var h=(raw||'').trim().replace(/^#/,'');
  if(/^[0-9a-fA-F]{3}$/.test(h))return '#'+h.split('').map(function(c){return c+c;}).join('').toUpperCase();
  if(/^[0-9a-fA-F]{6}$/.test(h)||/^[0-9a-fA-F]{8}$/.test(h))return '#'+h.toUpperCase();
  return null;
}
document.getElementById('p-fill-hex').addEventListener('change',function(){
  var hex=parseHexInput(this.value);
  if(!hex){this.value=hexDisplayValue(state.fillColor);return;}
  window.SM.setFillColor(hex);
  if(!state.fillEnabled)window.SM.setFillEnabled(true);
});
document.getElementById('p-stroke-hex').addEventListener('change',function(){
  var hex=parseHexInput(this.value);
  if(!hex){this.value=hexDisplayValue(state.strokeColor);return;}
  window.SM.setStrokeColor(hex);
  if(!state.strokeEnabled)window.SM.setStrokeEnabled(true);
});
// Stroke's own alpha (2026-07) — Stroke has no separate per-object
// "opacity" field the way Fill does (#p-opacity), so its Figma-style
// opacity% box writes straight into the stroke COLOR's own hex8 alpha
// byte instead, keeping RGB untouched.
document.getElementById('p-stroke-alpha').addEventListener('input',function(){
  var pct=Math.max(0,Math.min(100,parseInt(this.value)||0));
  var rgb=(state.strokeColor||'#000000').replace('#','').slice(0,6);
  var a=Math.round(pct/100*255).toString(16).padStart(2,'0').toUpperCase();
  window.SM.setStrokeColor('#'+rgb+(pct<100?a:''));
  if(!state.strokeEnabled)window.SM.setStrokeEnabled(true);
});
['p-fill-hex','p-stroke-hex'].forEach(function(id){
  var el=document.getElementById(id);
  el.addEventListener('keydown',function(e){if(e.key==='Enter')this.blur();});
});
// Document Dimensions proportion-lock (2026-07, "des tailles ou position
// sur une seule ligne") — remembers the W/H ratio at the moment it's
// switched ON; while locked, editing either field scales the other to
// preserve that ratio. Off by default (matches the pre-existing behavior
// of W/H being fully independent).
var _dimsLockRatio=null;
document.getElementById('btn-dims-lock').addEventListener('click',function(){
  var on=!this.classList.contains('on');
  this.classList.toggle('on',on);
  _dimsLockRatio=on?(state.canvasW/state.canvasH):null;
});
document.getElementById('p-cw').addEventListener('input',function(){
  if(_dimsLockRatio){
    var h=Math.max(1,Math.round(parseFloat(this.value)/_dimsLockRatio));
    document.getElementById('p-ch').value=h;
    window.SM.setCanvasSize(parseInt(this.value)||1,h);
  }
});
document.getElementById('p-ch').addEventListener('input',function(){
  if(_dimsLockRatio){
    var w=Math.max(1,Math.round(parseFloat(this.value)*_dimsLockRatio));
    document.getElementById('p-cw').value=w;
    window.SM.setCanvasSize(w,parseInt(this.value)||1);
  }
});
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
// "Apply" button removed (2026-07: "plus besoin du bouton apply pour
// smooth, il le fait directement quand on change de valeur") — the field
// applies live on its own 'change' (fires on type+Enter/blur, and on the
// scrub drag's own coalesced/final dispatches — see smoothSelectedStroke's
// own comment for why its internal pushUndo() is skipped during a live
// scrub instead of double-snapshotting).
document.getElementById('p-poststroke-smooth').addEventListener('change',function(){window.SM.smoothSelectedStroke(this.value);});
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
// Subselect tool + at least one vertex picked (_nodeSel) drives THOSE
// vertices' bounds/move/scale/rotate instead of the whole path's — 2026-07,
// "la position du panneau doivent driver les vertices... pareil pour size
// ... et rotation aussi". Falls back to the existing whole-selection
// behavior for every other case (Select tool, or Subselect with nothing
// vertex-picked yet — same as before this change).
function activeXformVertexMode(){return state.tool==='subselect'&&_nodeSel.length>0;}
function activeXformBounds(){return activeXformVertexMode()?nodeSelBounds():xformSelBounds();}
function activeXformApplyMove(dx,dy,skipUndo){if(activeXformVertexMode())nodeSelApplyMove(dx,dy,skipUndo);else selPropsApplyMove(dx,dy,skipUndo);}
function activeXformApplyScale(sx,sy,anchor,skipUndo){if(activeXformVertexMode())nodeSelApplyScale(sx,sy,anchor,skipUndo);else selPropsApplyScale(sx,sy,anchor,skipUndo);}
function activeXformApplyRotate(deltaDeg,center,skipUndo){if(activeXformVertexMode())nodeSelApplyRotate(deltaDeg,center,skipUndo);else selPropsApplyRotate(deltaDeg,center,skipUndo);}
wireLiveXformField('sp-x',function(el,started){var b=activeXformBounds();if(!b)return;activeXformApplyMove((parseFloat(el.value)||0)-b.x,0,started);});
wireLiveXformField('sp-y',function(el,started){var b=activeXformBounds();if(!b)return;activeXformApplyMove(0,(parseFloat(el.value)||0)-b.y,started);});
// Proportion lock (2026-07, "il manque le cadenas sur le size pour lier
// les 2") — same on/off toggle convention as btn-dims-lock (Document
// panel's W/H), but applies the SAME scale factor to both axes in one
// activeXformApplyScale call (a true uniform transform) rather than
// deriving one field from a remembered ratio like the canvas-size lock
// does — simpler here since both fields already read live bounds.
var _spSizeLockOn=false;
document.getElementById('btn-sp-size-lock').addEventListener('click',function(){
  _spSizeLockOn=!this.classList.contains('on');
  this.classList.toggle('on',_spSizeLockOn);
});
wireLiveXformField('sp-w',function(el,started){
  var b=activeXformBounds();if(!b||b.width<0.01)return;
  var nv=Math.max(0.01,parseFloat(el.value)||b.width);
  var sx=nv/b.width;
  if(_spSizeLockOn&&b.height>=0.01){
    document.getElementById('sp-h').value=Math.round(b.height*sx);
    activeXformApplyScale(sx,sx,b.topLeft,started);
  }else{
    activeXformApplyScale(sx,1,b.topLeft,started);
  }
});
wireLiveXformField('sp-h',function(el,started){
  var b=activeXformBounds();if(!b||b.height<0.01)return;
  var nv=Math.max(0.01,parseFloat(el.value)||b.height);
  var sy=nv/b.height;
  if(_spSizeLockOn&&b.width>=0.01){
    document.getElementById('sp-w').value=Math.round(b.width*sy);
    activeXformApplyScale(sy,sy,b.topLeft,started);
  }else{
    activeXformApplyScale(1,sy,b.topLeft,started);
  }
});
wireLiveXformField('sp-rot',function(el,started){var b=activeXformBounds();if(!b)return;var nv=parseFloat(el.value)||0;var delta=nv-(state.selRotAccum||0);state.selRotAccum=nv;activeXformApplyRotate(delta,xformAnchorPoint(b),started);});
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
    // Persist per-stroke (2026-07, same fix as select-bridge.js's canvas
    // Alt-drag path) so picking a preset here ALSO survives a deselect+
    // reselect, not just the canvas gesture.
    selectedPaths.forEach(function(p){if(p&&p.data){p.data.xformAnchorKey=btn.dataset.key;delete p.data.xformAnchorCustom;}});
    if(selectedPaths.length)saveActiveLayerFrame();
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
// Alt/Option+click = non-destructive combine-group (2026-07-29, mirrors
// Illustrator's own Alt+Pathfinder "Compound Shape" convention) — plain
// click keeps the existing destructive booleanOp byte-for-byte unchanged.
function wireBoolBtn(id,mode){
  document.getElementById(id).addEventListener('click',function(e){
    if(e.altKey&&window.SMGroup)SMGroup.combineSelection(mode);
    else window.SM.booleanOp(mode);
  });
}
wireBoolBtn('btn-bool-unite','unite');
wireBoolBtn('btn-bool-subtract','subtract');
wireBoolBtn('btn-bool-intersect','intersect');
wireBoolBtn('btn-bool-exclude','exclude');
document.getElementById('p-erasersize').addEventListener('input',function(){window.SM.setEraserSize(this.value);});
document.getElementById('p-fillbrushsize').addEventListener('input',function(){window.SM.setFillBrushSize(this.value);});
document.getElementById('p-brushpreset').addEventListener('change',function(){window.SM.setBrushPreset(this.value);if(window.BrushPresetPicker)window.BrushPresetPicker.paintButton(this.value);});
// Applies a vector brush preset to the current selection — was wired to a
// dedicated "Apply to selection" button in the Stroke panel; that button
// (and its Bitmap Brush counterpart) is gone (2026-07 harmonization, moved
// out to the floating Brush panel), so this is now a plain function the
// floating panel (brush-menu-bridge.js) calls directly the moment a preset
// swatch is clicked — same eligibility/strip/apply logic as before, just
// callable from more than one caller.
function applyVectorBrushToSelection(preset){
  // `p.strokeColor` used to gate eligibility — excluded every fill-
  // camouflaged anchor (a Bitmap Brush stroke with a fill, or any already-
  // textured anchor with strokeColor nulled by the SAME camouflage
  // convention) from ever being eligible, silently no-op'ing this button
  // on exactly the strokes someone would want to convert FROM. Eligible
  // now: a real stroke (strokeColor truthy) OR an already-textured anchor
  // (recognizable by its own texture tag, regardless of which kind) — a
  // plain fill-only shape with neither is still correctly excluded, same
  // as before.
  var eligible=selectedPaths.filter(function(p){return p instanceof Path&&!(p.data&&(p.data.isVectorBrush||p.data.isFillShape))&&(p.strokeColor||(p.data&&(p.data.brushTexturePreset||p.data.bitmapBrushSpec)));});
  if(!eligible.length)return;
  pushUndo();
  eligible.forEach(function(p){
    // Re-applying / converting: strip whatever texture (vector OR bitmap)
    // this anchor already carries first — stripAnyBrushTexture (app.js)
    // handles both kinds identically (same camouflage convention), so
    // switching FROM a Bitmap Brush stroke TO a vector preset here is a
    // real conversion, not a silent no-op.
    stripAnyBrushTexture(p);
    if(preset&&preset!=='none')applyBrushTexture(p,preset);
  });
  saveActiveLayerFrame();updateUI();showToast('Brush appliqué à la sélection');
}
window.SM.applyVectorBrushToSelection=applyVectorBrushToSelection;
if(window.BrushPresetPicker)window.BrushPresetPicker.paintButton(state.brushPreset);
document.getElementById('p-drawmode').addEventListener('change',function(){window.SM.setDrawMode(this.value);});
document.getElementById('p-pmin').addEventListener('input',function(){window.SM.setPressureMin(this.value);});
document.getElementById('p-pmax').addEventListener('input',function(){window.SM.setPressureMax(this.value);});
var _pcurveSel=document.getElementById('p-pcurve');if(_pcurveSel)_pcurveSel.addEventListener('change',function(){window.SM.setPressureCurve(this.value);});
var _pcurveEditBtn=document.getElementById('btn-edit-pcurve');if(_pcurveEditBtn)_pcurveEditBtn.addEventListener('click',function(){if(window._curveEditor)window._curveEditor.editPressureCurve();});
document.getElementById('p-pinv').addEventListener('change',function(){window.SM.setPressureInvert(this.checked);});
document.getElementById('p-taper').addEventListener('change',function(){window.SM.setTaperEnds(this.checked);});
document.getElementById('p-shadowmode').addEventListener('change',function(){window.SM.setShadowMode(this.checked);});
document.getElementById('p-cw').addEventListener('change',function(){window.SM.setCanvasSize(parseInt(this.value),state.canvasH);});
document.getElementById('p-ch').addEventListener('change',function(){window.SM.setCanvasSize(state.canvasW,parseInt(this.value));});
document.getElementById('p-cbg').addEventListener('input',function(){window.SM.setCanvasBg(this.value);});
document.getElementById('btn-clip').addEventListener('click',function(){window.SM.setCanvasClip(!state.canvasClip);this.classList.toggle('active',state.canvasClip);});
document.getElementById('btn-safety').addEventListener('click',function(){window.SM.setSafetyZones(!state.safetyZones);this.classList.toggle('active',state.safetyZones);});
// Flou/Ombre au sol/effect-layer type/param wiring all moved into
// effects-panel.js's unified Effects stack (2026-07 rewrite).
// Perspective/Symmetry Guide (feedback 2026-07: "les onglet guide symétrie
// et perspective n'ont plus lieu d'être, les options doivent être gérées au
// niveau du panneau flottant") — the right-panel section these ids used to
// live in is gone; every control (on/off, mode, density/sectors, lock/
// extend, reset) now lives in the Labs floating panel (labs-float-panel.js)
// instead, reading/writing the same state.* fields directly. Kept as a
// no-op stub (rather than deleted outright) since labs-float-panel.js's
// cycleMode still calls it after a Symmetry mode change.
window.syncSymmetryPanelVisibility=function(){};
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

// Track matte (2026-07, scouted from Caddis's Layer.matteMode) — dropdown
// wired IDENTICALLY to initBlendDropdown above (same hover-preview /
// click-commit / Escape-reverts UX), deliberately not factored into a
// shared helper: the two are independent small pieces of UI logic that
// happen to look alike today but read/write different layer fields and
// have already diverged once (Blend has no "which OTHER layer" concept;
// Matte's whole point is the layer above). A shared abstraction would be
// solving a duplication that isn't really there yet.
var MATTE_MODE_LABELS={none:'Aucun',alpha:'Alpha',alphaInverted:'Alpha (inversé)',luma:'Luminance',lumaInverted:'Luminance (inversée)'};
(function initMatteDropdown(){
  var dd=document.getElementById('p-mattemode');if(!dd)return;
  var pop=document.createElement('div');pop.id='matte-pop';document.body.appendChild(pop);
  var origMode=null;
  function currentLd(){return state.layers[state.activeLayerIdx];}
  function applyPreview(v){
    var ld=currentLd();if(!ld)return;
    ld.matteMode=v==='none'?undefined:v;
    window._sceneVersion=(window._sceneVersion||0)+1;
    if(window.SMEngineBridge&&window.SMEngineBridge.renderNow)window.SMEngineBridge.renderNow();
  }
  function setLabel(v){dd.dataset.value=v;dd.textContent=MATTE_MODE_LABELS[v]||v;}
  function close(revert){
    pop.style.display='none';
    if(revert&&origMode!==null)applyPreview(origMode);
    origMode=null;
  }
  // `anchorEl` defaults to the right-panel dropdown itself, but the layer-
  // row badge and the row's own context menu (both added because the
  // right panel's Document-fallback visibility turned out too easy to
  // miss — "je vois pas où appliqué les track matte") pass THEIR element
  // instead, so the popup opens next to whatever the user actually clicked
  // rather than off in the (possibly hidden) right panel.
  function open(anchorEl){
    var anchor=anchorEl||dd;
    var ld=currentLd();if(!ld)return;
    // A matte needs a layer ABOVE this one to draw from (AE convention —
    // the source is implicit, never picked). Nothing to offer on the
    // topmost layer; refusing to open beats a picker that visibly does
    // nothing once a mode is chosen.
    if(state.activeLayerIdx>=state.layers.length-1){
      if(window.showToast)showToast('Aucun calque au-dessus pour servir de matte');
      return;
    }
    origMode=ld.matteMode||'none';
    pop.innerHTML='';
    Object.keys(MATTE_MODE_LABELS).forEach(function(v){
      var it=document.createElement('div');
      it.className='blend-opt'+(v===origMode?' sel':'');
      it.textContent=MATTE_MODE_LABELS[v];
      it.addEventListener('mouseenter',function(){applyPreview(v);});
      it.addEventListener('click',function(e){
        e.stopPropagation();
        var ld2=currentLd();if(ld2)ld2.matteMode=origMode==='none'?undefined:origMode;
        pushUndo();
        applyPreview(v);
        setLabel(v);
        origMode=null;
        close(false);
        renderLayerList(); // badge (added/removed) must reflect the new mode immediately
      });
      pop.appendChild(it);
    });
    var r=anchor.getBoundingClientRect();
    pop.style.display='block';
    pop.style.left=Math.max(8,Math.min(window.innerWidth-pop.offsetWidth-8,r.left))+'px';
    var top=r.bottom+4;
    if(top+pop.offsetHeight>window.innerHeight-8)top=Math.max(8,r.top-pop.offsetHeight-4);
    pop.style.top=top+'px';
  }
  pop.addEventListener('mouseleave',function(){if(origMode!==null)applyPreview(origMode);});
  dd.addEventListener('click',function(e){e.stopPropagation();if(pop.style.display==='block')close(true);else open();});
  document.addEventListener('pointerdown',function(e){if(pop.style.display==='block'&&!pop.contains(e.target)&&e.target!==dd)close(true);});
  window.addEventListener('keydown',function(e){if(e.key==='Escape'&&pop.style.display==='block')close(true);});
  // Exposed for the layer-row badge and context-menu entry points (see
  // renderLayerList) — same picker, different trigger, so behavior (hover-
  // preview, click-commit, Escape-revert) is identical everywhere it opens.
  window.openMatteDropdownAt=open;
})();
// FPS/Frames now live in the Document section (canvas-sec) alongside
// Width/Height (p-cw/p-ch below) — the old separate proj-w/proj-h duplicate
// pair was removed (audit 2026-07-17); syncDocFields() still keeps these
// two ids in sync with state.
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
document.getElementById('btn-bake-cache').addEventListener('click',function(){manualBakeCache();});
// Persistent toolbar customization. The overflow button is intentionally
// never hideable, so hidden controls always remain recoverable.
(function initToolbarCustomization(){
  var toolbar=document.getElementById('tl-toolbar');
  var trigger=document.getElementById('btn-toolbar-customize');
  if(!toolbar||!trigger)return;
  var KEY='nemo-timeline-toolbar-hidden';
  var pop=null;
  // Transport is never hideable — same rule as the overflow trigger itself.
  // Go to first / Previous / Play-Stop / Next / Go to last / Loop are how you
  // move through time at all; a timeline without them is not a timeline, and
  // hiding one would be an easy irreversible-looking mistake to make from a
  // checkbox list (2026-07-27: "tout ça ne doit pas y apparaître, il reste
  // quoi qu'il arrive"). They are excluded from the list rather than shown
  // disabled — an entry you can never act on is just noise.
  var ALWAYS_VISIBLE=['btn-ff','btn-pf','btn-play','btn-nf','btn-lf','btn-loop'];
  function readHidden(){try{return JSON.parse(localStorage.getItem(KEY)||'[]');}catch(e){return[];}}
  function candidates(){return Array.prototype.slice.call(toolbar.querySelectorAll(':scope > button.tb[id]')).filter(function(b){return b!==trigger&&ALWAYS_VISIBLE.indexOf(b.id)<0;});}
  function apply(){
    var hidden=readHidden();
    // Drop any stored id that is now protected, so a transport button hidden
    // by an earlier build (or by hand in localStorage) comes back instead of
    // staying invisible with no entry left to re-enable it.
    var cleaned=hidden.filter(function(id){return ALWAYS_VISIBLE.indexOf(id)<0;});
    if(cleaned.length!==hidden.length){hidden=cleaned;localStorage.setItem(KEY,JSON.stringify(hidden));}
    ALWAYS_VISIBLE.forEach(function(id){
      var b=document.getElementById(id);
      if(b)b.classList.remove('toolbar-user-hidden');
    });
    candidates().forEach(function(b){b.classList.toggle('toolbar-user-hidden',hidden.indexOf(b.id)>=0);});
  }
  function close(){if(pop){pop.remove();pop=null;}document.removeEventListener('pointerdown',outside,true);}
  function outside(e){if(pop&&!pop.contains(e.target)&&e.target!==trigger)close();}
  trigger.addEventListener('click',function(e){
    e.stopPropagation();
    if(pop){close();return;}
    pop=document.createElement('div');pop.className='ctx-menu toolbar-custom-pop';
    var hidden=readHidden();
    candidates().forEach(function(b){
      var row=document.createElement('label');row.className='toolbar-custom-row';
      var cb=document.createElement('input');cb.type='checkbox';cb.checked=hidden.indexOf(b.id)<0;
      // The row shows the button's OWN icon plus a SHORT name. Using the raw
      // title made every row a full tooltip sentence (up to 90 chars, e.g.
      // "Loop playback (work area) — right-click for ping-pong"), ellipsised
      // to nothing useful in a 250px popover — reported 2026-07-27 ("icon
      // texte moins long mieux mis en page"). A tooltip explains what a
      // control DOES; this list only has to say WHICH control it is, and the
      // icon carries most of that.
      var ico=document.createElement('span');ico.className='toolbar-custom-ico';
      // Clone the button's WHOLE icon content, not just an <svg>: this
      // toolbar mixes inline SVG, material-symbols spans and bare text
      // glyphs (↻ ▭ ☷ ✥) and one text label ("All"). Matching only svg left
      // 11 of 18 rows with an empty icon slot.
      ico.innerHTML=b.innerHTML;
      // Cut at the first tooltip separator: everything before the em dash,
      // middle dot or shortcut parenthesis is the control's name, the rest is
      // explanation. Falls back to the whole title when there is no separator.
      var full=b.title||b.getAttribute('aria-label')||b.id;
      var short=full.split(/\s+[—·]\s+|\s*\(|\s*,\s*/)[0].trim()||full;
      // A few titles are one long clause with no separator at all
      // ("Show/hide Shadow Brush guide lines across all layers and
      // components"). Clip those at a word boundary rather than letting CSS
      // ellipsis eat an arbitrary character — the full text stays on hover.
      if(short.length>30){
        var cut=short.slice(0,30);
        var sp=cut.lastIndexOf(' ');
        short=(sp>12?cut.slice(0,sp):cut).replace(/[\s'’,:;-]+$/,'')+'…';
      }
      var label=document.createElement('span');label.className='toolbar-custom-name';
      label.textContent=short;
      row.title=full; // the long explanation stays reachable on hover
      cb.addEventListener('change',function(){
        var h=readHidden(),at=h.indexOf(b.id);
        if(cb.checked&&at>=0)h.splice(at,1);
        if(!cb.checked&&at<0)h.push(b.id);
        localStorage.setItem(KEY,JSON.stringify(h));apply();
      });
      row.appendChild(cb);row.appendChild(ico);row.appendChild(label);pop.appendChild(row);
    });
    document.body.appendChild(pop);
    var r=trigger.getBoundingClientRect();
    pop.style.left=Math.max(6,Math.min(window.innerWidth-pop.offsetWidth-6,r.right-pop.offsetWidth))+'px';
    pop.style.top=Math.min(window.innerHeight-pop.offsetHeight-6,r.bottom+5)+'px';
    setTimeout(function(){document.addEventListener('pointerdown',outside,true);},0);
  });
  apply();
})();
document.getElementById('btn-tween-curves').addEventListener('click',function(){window.SM.toggleTweenCurves();});
document.getElementById('btn-ghost-select').addEventListener('click',function(){selectGhostAll();});
document.getElementById('btn-shadow-guides').addEventListener('click',function(){window.SM.toggleShadowGuides();});
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
var currentOutlineChk=document.getElementById('p-current-outline');
if(currentOutlineChk){
  currentOutlineChk.checked=!!state.currentFrameOutline;
  currentOutlineChk.addEventListener('change',function(){
    state.currentFrameOutline=this.checked;
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
  });
}
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
// Marker-range presets — folded into the onion-skin right-click popover
// (#onion-pop) from the now-removed separate "Modify onion markers"
// (#btn-os-range) toolbar button/dropdown (2026-07, "les options de ce
// bouton là doivent aller dans le clic droit du bouton onion skin et donc
// plus besoin de celui-ci"). Same setOnionSpan()/_omFollow logic as
// before, just plain buttons/checkbox inside the popover instead of a
// second showContextMenu dropdown.
document.getElementById('om-span-1').addEventListener('click',function(){setOnionSpan(1,1);});
document.getElementById('om-span-2').addEventListener('click',function(){setOnionSpan(2,2);});
document.getElementById('om-span-5').addEventListener('click',function(){setOnionSpan(5,5);});
document.getElementById('om-span-all').addEventListener('click',function(){setOnionSpan(-1,-1);});
var omFollowChk=document.getElementById('p-om-follow');
if(omFollowChk){
  omFollowChk.checked=!!window._omFollow;
  omFollowChk.addEventListener('change',function(){
    window._omFollow=this.checked;
    window.updateOmMarkers(state.currentFrame,state.totalFrames);
  });
}
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
  // Section Onion Skin du panel droit (mockup 2026-07-17) : clic sur le
  // header = même toggle que #btn-os ; le badge #os-st-panel est mis à
  // jour par toggleOnion. État initial synchronisé ici.
  var osSec=document.getElementById('onion-sec');
  if(osSec){
    osSec.querySelector('.phdr').addEventListener('click',function(){window.SM.toggleOnion();});
    var badge=document.getElementById('os-st-panel');
    if(badge){badge.textContent=state.onionSkin?'ON':'OFF';badge.style.color=state.onionSkin?'var(--green)':'var(--text-dim)';}
  }
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
// tl-cf drag/type-to-scrub through frames (2026-07, "rendre la frame 1/120
// draggable") — goToFrame() itself no-ops silently on an out-of-range
// index (app.js) rather than clamping, so clamp here first or a
// drag-past-the-end would leave the input showing a stale/invalid number
// nothing ever applied.
document.getElementById('tl-cf').addEventListener('change',function(){
  var v=Math.max(1,Math.min(state.totalFrames,parseInt(this.value)||1));
  this.value=v;
  if(state.playing)stopPlay();
  goToFrame(v-1);
});
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
// ---- Mograph duplicator panel (2026-07-29) ----
// Every field writes into ld.duplicator.* then reloads the frame — the
// multiplication itself lives in applyLayerDuplicator (app.js). `change`
// (not `input`) so a scrub gesture commits once per settle, same as the
// component-instance fields above.
(function(){
  function dupOf(){var ld=state.layers[state.activeLayerIdx];return(ld&&ld.duplicator)?ld.duplicator:null;}
  // Union-bounds cache invalidation (2026-07-29, same fix as
  // symbolUnionBounds's getEffectiveStrokesRendered switch, motion.js): a
  // duplicator field changes what a COMPONENT instance's union bounds should
  // be, but that cache's key is only symbolId|inPoint|outPoint — it has no
  // way to know the duplicator config itself just changed, so it would keep
  // serving the pre-edit bounds (gizmo drifts again after the very next
  // rows/cols/spacing tweak) without this.
  function dupRefresh(){loadFrame(state.currentFrame);if(window.SMMotion&&SMMotion.invalidateSymbolUnionBounds)SMMotion.invalidateSymbolUnionBounds();if(window.SMEngineBridge)SMEngineBridge.renderNow();updateDuplicatorPanel();}
  // pushUndo() BEFORE the mutation in every listener below (2026-07-29 fix,
  // QA-confirmed: "aucun champ du panneau Duplicator ne pousse d'undo — un
  // Cmd+Z après avoir juste coché/tapé un réglage saute silencieusement à
  // l'action réelle précédente (ex. un dessin), perdant les deux à la fois").
  // Only the scrub-drag path on these same <input class="scrub"> fields ever
  // got a checkpoint for free, via ui.js's generic drag handler — a plain
  // click/type/select change on any of them (checkbox, <select>, or a typed
  // number + Tab, no drag) bypassed pushUndo entirely. Matches every other
  // panel's own convention (group-bridge.js etc.: pushUndo() first, mutate
  // second).
  function wireNum(id,key,min,max){document.getElementById(id).addEventListener('change',function(){var d=dupOf();if(!d)return;var v=parseFloat(this.value);if(!isFinite(v))return;if(min!==undefined)v=Math.max(min,v);if(max!==undefined)v=Math.min(max,v);pushUndo();d[key]=v;dupRefresh();});}
  document.getElementById('dup-mode').addEventListener('change',function(){var d=dupOf();if(!d)return;pushUndo();d.mode=this.value;dupRefresh();});
  wireNum('dup-rows','rows',1,30);wireNum('dup-cols','cols',1,30);
  wireNum('dup-spacingx','spacingX');wireNum('dup-spacingy','spacingY');
  wireNum('dup-count','count',1,500);
  wireNum('dup-radius','radius');wireNum('dup-startangle','startAngle');
  wireNum('dup-seed','seed');
  document.getElementById('dup-radial-orient').addEventListener('change',function(){var d=dupOf();if(!d)return;pushUndo();d.radialOrient=this.checked;dupRefresh();});
  document.getElementById('dup-path-align').addEventListener('change',function(){var d=dupOf();if(!d)return;pushUndo();d.pathAlignTangent=this.checked;dupRefresh();});
  document.getElementById('dup-path-layer').addEventListener('change',function(){var d=dupOf();if(!d)return;pushUndo();d.pathLayerUid=this.value||null;dupRefresh();});
  document.getElementById('btn-dup-reseed').addEventListener('click',function(){var d=dupOf();if(!d)return;pushUndo();d.seed=Math.floor(Math.random()*1e6);dupRefresh();});
  // Temporal stagger (2026-07-29) — same pattern as the fields above.
  function tOffOf(){var d=dupOf();if(!d)return null;return d.timeOffset||(d.timeOffset={enabled:false,offsetFrames:1,direction:'forward'});}
  document.getElementById('dup-anim-enabled').addEventListener('change',function(){var t=tOffOf();if(!t)return;pushUndo();t.enabled=this.checked;dupRefresh();});
  document.getElementById('dup-anim-offset').addEventListener('change',function(){var t=tOffOf();if(!t)return;var v=parseFloat(this.value);if(!isFinite(v))return;pushUndo();t.offsetFrames=v;dupRefresh();});
  document.getElementById('dup-anim-direction').addEventListener('change',function(){var t=tOffOf();if(!t)return;pushUndo();t.direction=this.value;dupRefresh();});
  // Effectors (2026-07-29) — new one starts centered on the layer's own
  // seed content so it's immediately visible/draggable rather than sitting
  // off-canvas at the space origin.
  document.getElementById('btn-dup-add-effector').addEventListener('click',function(){
    var d=dupOf();if(!d)return;
    var ld=state.layers[state.activeLayerIdx];
    var strokes=getEffectiveStrokes(state.activeLayerIdx,state.currentFrame);
    var pivot=_boundsCenterOfStrokes(strokes);
    pushUndo();
    if(!d.effectors)d.effectors=[];
    // channels (2026-07-30, "n'importe quel property"): a fresh effector
    // starts pre-seeded with a Position channel (the single most common
    // starting point, matching this button's old always-4-fields default)
    // rather than empty — the user adds more via the row's own "+
    // Propriété…" picker (renderDuplicatorEffectors). No legacy
    // offsetPos/offsetRot/offsetScale/offsetOpacity fields on a NEW
    // effector — those only exist for effectors created before this
    // change, migrated into this same channels shape on first read
    // (effectorChannels, app.js).
    d.effectors.push({pos:{x:pivot.x,y:pivot.y},radius:200,falloff:'radial',angle:0,strength:100,channels:[{prop:'position',value:[0,0]}]});
    renderDuplicatorEffectors(d);dupRefresh();
  });
  function wireRand(id,key){document.getElementById(id).addEventListener('change',function(){var d=dupOf();if(!d)return;pushUndo();(d.staggerRandom||(d.staggerRandom={}))[key]=this.checked;dupRefresh();});}
  wireRand('dup-rand-pos','position');wireRand('dup-rand-rot','rotation');wireRand('dup-rand-scale','scale');wireRand('dup-rand-op','opacity');
  document.getElementById('btn-dup-edit-source').addEventListener('click',function(){
    var ld=state.layers[state.activeLayerIdx];
    if(!ld||!ld.duplicator||!window.SMMotion)return;
    SMMotion.setDuplicatorEditSource(state.activeLayerIdx,!ld._dupEditSource);
  });
})();
// ---- Rig tool panel (2026-07-29) — 3-step: Tracer / Assigner / Déplacer ----
// Weight radius / rotate mode are read directly off the panel at BIND time
// (rigBindStroke's own radius/rotate params) rather than mirrored into a
// state field — they're bind-time parameters, not a persisted per-layer
// setting, so there's nothing to keep in sync when the panel isn't visible.
// state.rigSubMode is transient (not persisted, resets to 'draw' on reload),
// read directly by rig-bridge.js's onDown to decide what a click means.
state.rigSubMode=state.rigSubMode||'draw';
var RIG_MODE_BTN_IDS={draw:'btn-rig-mode-draw',assign:'btn-rig-mode-assign',move:'btn-rig-mode-move'};
var RIG_HINTS={
  draw:'rigHint',
  assign:'rigHintAssign',
  move:'rigHintMove',
};
function renderRigModeUI(){
  var btns=Object.keys(RIG_MODE_BTN_IDS).map(function(m){return document.getElementById(RIG_MODE_BTN_IDS[m]);});
  var assignRow=document.getElementById('rig-assign-row');
  var hintEl=document.getElementById('rig-hint');
  if(btns.some(function(b){return !b;})||!assignRow||!hintEl)return;
  var mode=state.rigSubMode||'draw';
  Object.keys(RIG_MODE_BTN_IDS).forEach(function(m){document.getElementById(RIG_MODE_BTN_IDS[m]).classList.toggle('ac',m===mode);});
  assignRow.style.display=mode==='assign'?'flex':'none';
  hintEl.textContent=window.SM&&SM.t?SM.t(RIG_HINTS[mode]):hintEl.textContent;
}
window.renderRigModeUI=renderRigModeUI;
(function(){
  Object.keys(RIG_MODE_BTN_IDS).forEach(function(mode){
    document.getElementById(RIG_MODE_BTN_IDS[mode]).addEventListener('click',function(){
      state.rigSubMode=mode;
      renderRigModeUI();
      if(window.SMEngineBridge)SMEngineBridge.renderNow(); // influence circles only draw in Assigner — must repaint on mode switch
    });
  });
  document.getElementById('btn-rig-auto-assign').addEventListener('click',function(){
    var ld=state.layers[state.activeLayerIdx];
    if(!ld)return;
    if(!canEditActiveLayer())return;
    var rig=ld.rig;
    if(!rig||!Object.keys(rig.bones).length){showToast(window.SM&&SM.t?SM.t('rigNeedBoneToast'):'Dessine au moins un os avant d\'assigner');return;}
    var radius=parseFloat(document.getElementById('rig-weight-radius').value)||200;
    var rotate=document.getElementById('rig-rotate-mode').checked;
    pushUndo();
    var res=rigAutoAssignLayer(ld,userLayers[state.activeLayerIdx],radius,rotate);
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
    // 0 assigned but candidates existed (2026-07-29 fix, QA-confirmed live:
    // "l'autoassign marche pas" on a shape drawn with the default Brush
    // tool) — rigBindStroke used to silently reject vector-brush strokes;
    // it now supports them like any other Path (see its own comment) —
    // only CompoundPaths (boolean results with holes/islands) still hit
    // this gap, with only a console.warn, so a plain "0 forme(s)
    // assignée(s)" toast read exactly like a bug instead of an honest
    // "not supported yet" — surfaced explicitly instead of guessing.
    if(res.n===0&&res.skippedUnsupported>0)showToast(window.SM&&SM.t?SM.t('rigAutoAssignBrushUnsupportedToast'):'Les formes avec des trous (résultats booléens) ne sont pas encore supportées par le Rig');
    else showToast(res.n+(window.SM&&SM.t?SM.t('rigAutoAssignedToast'):' forme(s) assignée(s) automatiquement'));
  });
  document.getElementById('btn-rig-commit').addEventListener('click',function(){
    var ld=state.layers[state.activeLayerIdx];
    if(ld)rigCommitFrame(ld);
  });
  document.getElementById('btn-rig-reset').addEventListener('click',function(){
    var ld=state.layers[state.activeLayerIdx];
    if(!ld||!ld.rig)return;
    if(!canEditActiveLayer())return;
    pushUndo();
    rigResetPose(ld);
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
    showToast('Pose du rig réinitialisée');
  });
})();
// ---- Combine-group panel (2026-07-29 UX fix) ----
// Resolves whether the CURRENT canvas selection already IS one existing
// combine-group (single member selected, or the full matching set selected)
// — mirrors group-bridge.js's own combineSelection "existingGid" check so
// this panel and Alt+click never disagree about what counts as "already
// grouped". Exposed on window so updatePropsContext (above) can call it
// every time the selection changes.
var COMBINE_MODE_BTN_IDS={unite:'btn-combine-unite',subtract:'btn-combine-subtract',intersect:'btn-combine-intersect',exclude:'btn-combine-exclude'};
function updateCombinePanel(){
  var sel=window.selectedPaths;
  var existingRow=document.getElementById('combine-existing-row');
  var btns=Object.keys(COMBINE_MODE_BTN_IDS).map(function(m){return document.getElementById(COMBINE_MODE_BTN_IDS[m]);});
  if(!existingRow||btns.some(function(b){return !b;}))return;
  var li=state.activeLayerIdx,ld=state.layers[li],layer=userLayers[li];
  var gid=null;
  if(ld&&ld.groups&&sel&&sel.length){
    var firstGid=sel[0].data&&sel[0].data.groupId;
    if(firstGid&&ld.groups[firstGid]&&ld.groups[firstGid].combineMode!=='none'){
      if(sel.length===1)gid=firstGid;
      else if(window.SMGroup){
        var members=SMGroup.resolveGroupMembers(firstGid,ld,layer);
        if(members.length===sel.length&&members.every(function(m){return sel.indexOf(m)!==-1;}))gid=firstGid;
      }
    }
  }
  var activeMode=gid?ld.groups[gid].combineMode:'unite';
  Object.keys(COMBINE_MODE_BTN_IDS).forEach(function(m){document.getElementById(COMBINE_MODE_BTN_IDS[m]).classList.toggle('ac',m===activeMode);});
  existingRow.style.display=gid?'flex':'none';
  // Disabled (not hidden) when there's nothing to combine yet — 2+ shapes
  // needed to CREATE a group, but changing an EXISTING group's mode only
  // needs that group to be the current selection (gid set), even at 1 member.
  var canAct=gid||(sel&&sel.length>=2);
  btns.forEach(function(b){b.disabled=!canAct;});
}
window.updateCombinePanel=updateCombinePanel;
(function(){
  function activeCombineGid(){
    var sel=window.selectedPaths;
    if(!sel||!sel.length)return null;
    var firstGid=sel[0].data&&sel[0].data.groupId;
    var ld=state.layers[state.activeLayerIdx];
    if(!firstGid||!ld||!ld.groups||!ld.groups[firstGid]||ld.groups[firstGid].combineMode==='none')return null;
    return firstGid;
  }
  // One click = combine (first time) or change mode (already a group) —
  // collapses the old select+separate-button two-step into the actual
  // one decision the user is making (2026-07-29 UX fix).
  Object.keys(COMBINE_MODE_BTN_IDS).forEach(function(mode){
    document.getElementById(COMBINE_MODE_BTN_IDS[mode]).addEventListener('click',function(){
      if(!window.SMGroup)return;
      var gid=activeCombineGid();
      if(gid)SMGroup.setGroupCombineMode(gid,state.layers[state.activeLayerIdx],mode);
      else SMGroup.combineSelection(mode);
      updateCombinePanel();
    });
  });
  document.getElementById('btn-combine-remove').addEventListener('click',function(){
    var sel=window.selectedPaths;
    if(!sel||sel.length!==1||!window.SMGroup)return;
    var li=state.activeLayerIdx;
    SMGroup.removeMemberFromGroup(sel[0],state.layers[li],userLayers[li]);
    updateCombinePanel();
  });
  document.getElementById('btn-combine-flatten').addEventListener('click',function(){
    var gid=activeCombineGid();
    if(!gid||!window.SMGroup)return;
    var li=state.activeLayerIdx;
    SMGroup.flattenGroup(gid,state.layers[li],userLayers[li]);
    updateCombinePanel();
  });
})();
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
      showToast(SM.t('exportTauriOnly'));
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
    // Read straight into state (not passed through opts like alpha/scale)
    // since export.js's exportBuildFrame/Lottie stroke filters both read
    // state.exportIncludeShadowGuides directly — simplest way to reach
    // both without threading a new param through every exportXxx function.
    state.exportIncludeShadowGuides=document.getElementById('exp-include-shadow').checked;
    var opts={start:range.start,end:range.end,scale:scale,alpha:alpha,fps:state.fps,
      onProgress:function(i,n){progEl.style.display='block';progEl.textContent=SM.t('exportRenderingFrame').replace('{i}',i).replace('{n}',n);},
      onFfmpeg:function(line){progEl.style.display='block';progEl.textContent=line.substring(0,80);},
      onRiveProgress:function(msg){progEl.style.display='block';progEl.textContent=msg;}};
    runBtn.disabled=true;progEl.style.display='block';progEl.textContent=SM.t('exportPreparing');
    try{
      var fn={svg:'exportSVGSequence',png:'exportPNGSequence',tiff:'exportTIFFSequence',gif:'exportGIF',mp4:'exportMP4',prores:'exportProRes',lottie:'exportLottie',rive:'exportRive','ae-camera':'exportAECamera'}[fmtSel.value];
      var res=await window.SMExport[fn](opts);
      if(res.cancelled){progEl.textContent=SM.t('exportCancelled');}
      else if(res.ok){
        progEl.textContent=SM.t('exportDone');
        var aeCamMsg=SM.t('exportAeCamMsg').replace('{n}',res.keyCount)+
          (res.kitsu?(res.kitsu.ok?SM.t('exportAeCamKitsuOk'):SM.t('exportAeCamKitsuFail').replace('{e}',res.kitsu.error)):'');
        var doneMsg=fmtSel.value==='rive'?SM.t('exportRiveDone').replace('{name}',res.artboardName)
          :fmtSel.value==='ae-camera'?aeCamMsg
          :SM.t('exportGenericDone');
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
      else{progEl.textContent=SM.t('exportError').replace('{e}',res.error||SM.t('exportErrorUnknown'));}
    }catch(err){
      progEl.textContent=SM.t('exportError').replace('{e}',err&&err.message?err.message:err);
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
