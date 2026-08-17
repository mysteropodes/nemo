// ---- Text Animator (2026-08-17) ----
// Cyril: "l'animation de texte lettres, mots, lignes... simple et facile
// à utiliser comme dans Figma Motion". Figma's own text-animation panel
// (and Lottie's/After Effects' text animator presets underneath it) all
// boil down to the SAME idea once you strip the chrome: split the block
// into units (character/word/line), stagger a short in/out transform
// across those units, done. No bespoke animation model needed here either
// — per splitTextIntoCharacters' own header comment (timeline.js), a
// split text block is just N ordinary Motion elements sharing a group id;
// vector text (vector-text-bridge.js) is already N elements from the
// moment it's placed. This file only adds: (a) grouping those elements by
// their word/line index (already stamped at build time — see
// vector-text-bridge.js / splitTextIntoCharacters), and (b) a small
// preset library that writes ordinary staggered elementMotion keyframes
// through SMMotion.ensureElementHolder/setKeyAtFrame — the exact same
// primitives the Motion panel's own stopwatch uses, just called N times
// with an increasing frame offset instead of once at the playhead.
(function(){
  // Overshoot ("pop") curve — a cubic that dips past 1 before settling,
  // the standard spring-flavoured ease every "pop in" preset (Figma,
  // Lottie, Principle) uses for scale-in. Plain DEFAULT_CURVE (ease-out)
  // is used for every other "in" preset.
  var POP_CURVE=[{x:0,y:0},{x:0.34,y:1.56},{x:0.64,y:1},{x:1,y:1}];
  // Ease-IN (slow start, fast finish) — same shape motion.js's own Easy
  // Ease In keyframe menu applies (CURVE_EASE_IN there), duplicated here
  // rather than exported: an exit should accelerate AWAY, the mirror of
  // an entrance's decelerate-INTO-place, so every *Out preset below uses
  // this instead of the entrance presets' default ease-out feel.
  var EASE_IN_CURVE=[{x:0,y:0},{x:0.25,y:0.25},{x:0.5,y:0.5},{x:0.75,y:0.91},{x:1,y:1}];
  // Explicit easing override (2026-08-17) — same 4 named shapes as
  // motion.js's own Easy Ease menu (CURVE_LINEAR/EASE/EASE_IN/EASE_OUT
  // there), duplicated here for the same reason as EASE_IN_CURVE above:
  // no exported hook onto those constants, and re-deriving them from the
  // menu labels would need the same numbers anyway. 'default' (null)
  // keeps each preset's own built-in curve (POP_CURVE for the two
  // rebond presets, EASE_IN_CURVE for every *Out, null==DEFAULT_CURVE
  // for the rest) — only picking a NAMED easing here overrides it.
  var EASING_CURVES={
    'default':null,
    linear:[{x:0,y:0},{x:1,y:1}],
    easeInOut:[{x:0,y:0},{x:0.25,y:0.156},{x:0.5,y:0.5},{x:0.75,y:0.844},{x:1,y:1}],
    easeIn:EASE_IN_CURVE,
    easeOut:[{x:0,y:0},{x:0.25,y:0.09},{x:0.5,y:0.5},{x:0.75,y:0.75},{x:1,y:1}],
  };
  var EASING_LABELS={'default':'Par défaut du style',linear:'Linéaire',easeInOut:'Douce (in/out)',easeIn:'Accélérer',easeOut:'Décélérer'};

  // Every text-carrying stroke dict in this layer belonging to `groupId`
  // (vector: sd.groupId from vector-text-bridge.js; raster split:
  // sd.textGroupId from splitTextIntoCharacters), grouped by the chosen
  // unit's index field and returned in reading order. A stroke missing
  // the requested index field (shouldn't happen for anything stamped by
  // either builder, but legacy text predating this feature has none) is
  // silently skipped rather than crashing the whole block's animation.
  function unitsForGroup(li, ld, groupId, mode){
    var idxField=mode==='word'?'wordIndex':mode==='line'?'lineIndex':'charIndex';
    var byIdx={};
    (window.SMMotion.layerElements(li,ld)||[]).forEach(function(entry){
      var sd=entry.sd;
      var gid=sd.groupId||sd.textGroupId;
      if(gid!==groupId)return;
      var idx=sd[idxField];
      if(idx==null)return;
      if(!byIdx[idx])byIdx[idx]=[];
      byIdx[idx].push(entry.strokeId);
    });
    return Object.keys(byIdx).map(Number).sort(function(a,b){return a-b;}).map(function(idx){
      return {index:idx,strokeIds:byIdx[idx]};
    });
  }
  // Resolves the text group a currently-selected Path/Raster belongs to —
  // vector glyphs (isVectorText) key off data.groupId, split raster
  // characters (isTextChar) off data.textGroupId. Returns null for
  // anything else (including a not-yet-split whole raster text block,
  // which has no per-unit elements to animate yet).
  function groupIdForItem(p){
    if(!p||!p.data)return null;
    if(p.data.isVectorText)return p.data.groupId||null;
    if(p.data.isTextChar)return p.data.textGroupId||null;
    return null;
  }

  var PRESETS={
    fadeIn:{props:['opacity'],from:{opacity:[0]},to:{opacity:[100]},curve:null,exit:false},
    fadeOut:{props:['opacity'],from:{opacity:[100]},to:{opacity:[0]},curve:EASE_IN_CURVE,exit:true},
    slideUp:{props:['opacity','position'],from:{opacity:[0],position:[0,40]},to:{opacity:[100],position:[0,0]},curve:null,exit:false},
    slideUpOut:{props:['opacity','position'],from:{opacity:[100],position:[0,0]},to:{opacity:[0],position:[0,-40]},curve:EASE_IN_CURVE,exit:true},
    slideDown:{props:['opacity','position'],from:{opacity:[0],position:[0,-40]},to:{opacity:[100],position:[0,0]},curve:null,exit:false},
    slideDownOut:{props:['opacity','position'],from:{opacity:[100],position:[0,0]},to:{opacity:[0],position:[0,40]},curve:EASE_IN_CURVE,exit:true},
    slideLeft:{props:['opacity','position'],from:{opacity:[0],position:[40,0]},to:{opacity:[100],position:[0,0]},curve:null,exit:false},
    slideLeftOut:{props:['opacity','position'],from:{opacity:[100],position:[0,0]},to:{opacity:[0],position:[-40,0]},curve:EASE_IN_CURVE,exit:true},
    slideRight:{props:['opacity','position'],from:{opacity:[0],position:[-40,0]},to:{opacity:[100],position:[0,0]},curve:null,exit:false},
    slideRightOut:{props:['opacity','position'],from:{opacity:[100],position:[0,0]},to:{opacity:[0],position:[40,0]},curve:EASE_IN_CURVE,exit:true},
    scaleIn:{props:['opacity','scale'],from:{opacity:[0],scale:[0,0]},to:{opacity:[100],scale:[100,100]},curve:null,exit:false},
    scaleOut:{props:['opacity','scale'],from:{opacity:[100],scale:[100,100]},to:{opacity:[0],scale:[0,0]},curve:EASE_IN_CURVE,exit:true},
    popIn:{props:['opacity','scale'],from:{opacity:[0],scale:[140,140]},to:{opacity:[100],scale:[100,100]},curve:POP_CURVE,exit:false},
    popOut:{props:['opacity','scale'],from:{opacity:[100],scale:[100,100]},to:{opacity:[0],scale:[140,140]},curve:EASE_IN_CURVE,exit:true},
  };
  var PRESET_LABELS={
    fadeIn:'Fondu (apparition)',fadeOut:'Fondu (disparition)',
    slideUp:'Glisser vers le haut (apparition)',slideUpOut:'Glisser vers le haut (disparition)',
    slideDown:'Glisser vers le bas (apparition)',slideDownOut:'Glisser vers le bas (disparition)',
    slideLeft:'Glisser vers la gauche (apparition)',slideLeftOut:'Glisser vers la gauche (disparition)',
    slideRight:'Glisser vers la droite (apparition)',slideRightOut:'Glisser vers la droite (disparition)',
    scaleIn:'Zoom (apparition)',scaleOut:'Zoom (disparition)',
    popIn:'Rebond (apparition)',popOut:'Rebond (disparition)',
  };

  // Every prop any preset can touch — used to clear a stale run before
  // re-writing (live preview re-applies on every slider tweak; without
  // clearing first, shortening the duration would leave the OLD end key
  // behind as an orphan instead of moving it).
  var ALL_PRESET_PROPS=['opacity','position','scale'];
  // Clears every element belonging to `groupId`, regardless of unit mode
  // — a strokeId's elementMotion holder is the same object no matter
  // which grouping (char/word/line) last wrote to it, so clearing must
  // walk ALL group members, not just the current mode's units (switching
  // mode mid-preview would otherwise leave the previous mode's keys
  // behind as orphans on strokeIds the new grouping doesn't touch).
  function clearGroup(li, groupId){
    var ld=state.layers[li]; if(!ld||!ld.elementMotion)return;
    (window.SMMotion.layerElements(li,ld)||[]).forEach(function(entry){
      var sd=entry.sd;
      if((sd.groupId||sd.textGroupId)!==groupId)return;
      var h=ld.elementMotion[entry.strokeId]; if(!h)return;
      ALL_PRESET_PROPS.forEach(function(prop){
        if(h.motion&&h.motion[prop])delete h.motion[prop];
        if(h.motionStatic&&h.motionStatic[prop])delete h.motionStatic[prop];
      });
    });
  }
  // Writes the staggered keys for every unit of `groupId` on layer `li`.
  // opts: {mode, preset, startFrame, unitFrames, staggerFrames, easing,
  // skipUndo, skipClear}. One undo step for the whole INTERACTION, not
  // one per call — the panel below calls this live on every control
  // tweak with skipUndo=true (a single pushUndo happened once at open,
  // see openPanel), so "Annuler" always reverts the whole session in one
  // step regardless of how many times the sliders moved.
  function apply(li, groupId, opts){
    var ld=state.layers[li]; if(!ld)return 0;
    var preset=PRESETS[opts.preset]; if(!preset)return 0;
    var mode=opts.mode||'char';
    if(!opts.skipUndo&&window.pushUndo)pushUndo();
    if(!opts.skipClear)clearGroup(li,groupId);
    var units=unitsForGroup(li,ld,groupId,mode);
    if(!units.length)return 0;
    var unitFrames=Math.max(1,opts.unitFrames||12);
    var staggerFrames=Math.max(0,opts.staggerFrames==null?3:opts.staggerFrames);
    var start=opts.startFrame==null?state.currentFrame:opts.startFrame;
    var easingOverride=opts.easing&&opts.easing!=='default'?EASING_CURVES[opts.easing]:null;
    var curve=easingOverride||preset.curve||null;
    // An exit staggers LAST unit first (AE/Figma convention: a sentence
    // dissolves from the end backwards, mirroring how it typed itself in)
    // — same unit list, just walked in reverse for stagger-offset purposes
    // only; the unit's own strokeIds are untouched.
    var order=preset.exit?units.map(function(_,i){return units.length-1-i;}):units.map(function(_,i){return i;});
    units.forEach(function(u,i){
      var f0=Math.round(start+order[i]*staggerFrames);
      var f1=f0+unitFrames;
      u.strokeIds.forEach(function(strokeId){
        var holder=window.SMMotion.ensureElementHolder(ld,strokeId);
        preset.props.forEach(function(prop){
          window.SMMotion.setKeyAtFrame(holder,prop,f0,preset.from[prop],curve?curve.slice():undefined);
          window.SMMotion.setKeyAtFrame(holder,prop,f1,preset.to[prop],curve?curve.slice():undefined);
        });
      });
    });
    if(window.renderLayerList)renderLayerList();
    if(window.renderTimeline)renderTimeline();
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
    return units.length;
  }

  // Small floating panel (2026-08-17) — deliberately NOT the RiveBar-style
  // "_ui" declarative form (that's a different app/system entirely); built
  // the same way this codebase's own in-place text editor and popovers
  // already are: a plain DOM node appended to <body>, styled inline off
  // the app's existing CSS custom properties (--panel2/--border/--text-dim)
  // so it matches the surrounding chrome without a new stylesheet entry.
  // Live preview (2026-08-17, Cyril: "continue" after being asked for a
  // preview scrub) — the panel writes REAL elementMotion keys on every
  // control tweak (not a separate scratch copy: this codebase has no
  // parallel "preview state" concept anywhere else, and the render path
  // only ever reads ld.elementMotion), gated behind exactly ONE pushUndo
  // taken at open time. Cancel calls undo() once to revert the whole
  // session regardless of how many tweaks happened; Terminé just closes,
  // leaving the already-live result in place — no separate "commit" step.
  var _panel=null, _panelUndoTaken=false;
  function closePanel(){ if(_panel){_panel.remove();_panel=null;} _panelUndoTaken=false; }
  function openPanel(li, groupId){
    closePanel();
    var ld=state.layers[li]; if(!ld)return;
    if(window.pushUndo){pushUndo();_panelUndoTaken=true;}
    var p=document.createElement('div');
    p.id='text-animator-panel';
    p.style.cssText='position:fixed;top:80px;right:280px;z-index:300;width:260px;'+
      'background:var(--panel2);border:1px solid var(--border);border-radius:10px;'+
      'box-shadow:0 8px 24px rgba(0,0,0,.4);padding:12px;font-size:12px;color:var(--text)';
    function row(labelTxt){
      var r=document.createElement('div');
      r.style.cssText='display:flex;align-items:center;justify-content:space-between;gap:8px;margin-bottom:8px';
      var lab=document.createElement('span'); lab.textContent=labelTxt; lab.style.color='var(--text-dim)';
      r.appendChild(lab);
      return r;
    }
    var title=document.createElement('div');
    title.textContent='Animer le texte';
    title.style.cssText='font-weight:600;margin-bottom:10px';
    p.appendChild(title);

    var unitSel=document.createElement('select');
    ['char','word','line'].forEach(function(v){
      var o=document.createElement('option'); o.value=v;
      o.textContent=v==='char'?'Lettres':v==='word'?'Mots':'Lignes';
      unitSel.appendChild(o);
    });
    var rUnit=row('Unité'); rUnit.appendChild(unitSel); p.appendChild(rUnit);

    var presetSel=document.createElement('select');
    var grpIn=document.createElement('optgroup'); grpIn.label='Apparition';
    var grpOut=document.createElement('optgroup'); grpOut.label='Disparition';
    presetSel.appendChild(grpIn); presetSel.appendChild(grpOut);
    Object.keys(PRESETS).forEach(function(k){
      var o=document.createElement('option'); o.value=k; o.textContent=PRESET_LABELS[k]||k;
      (PRESETS[k].exit?grpOut:grpIn).appendChild(o);
    });
    var rPreset=row('Style'); rPreset.appendChild(presetSel); p.appendChild(rPreset);

    var easeSel=document.createElement('select');
    Object.keys(EASING_CURVES).forEach(function(k){
      var o=document.createElement('option'); o.value=k; o.textContent=EASING_LABELS[k];
      easeSel.appendChild(o);
    });
    var rEase=row('Accélération'); rEase.appendChild(easeSel); p.appendChild(rEase);

    function numInput(val,step){
      var inp=document.createElement('input');
      inp.type='number'; inp.value=val; inp.className='pi scrub'; inp.dataset.step=step||1;
      inp.style.width='64px';
      return inp;
    }
    var startInp=numInput(state.currentFrame,1);
    var rStart=row('Frame de départ'); rStart.appendChild(startInp); p.appendChild(rStart);

    var durInp=numInput(12,1);
    var rDur=row('Durée / unité (frames)'); rDur.appendChild(durInp); p.appendChild(rDur);

    var stagInp=numInput(3,1);
    var rStag=row('Décalage entre unités (frames)'); rStag.appendChild(stagInp); p.appendChild(rStag);

    // Scrub slider — previews the animation's own span (start → last
    // unit's end) without touching the app's main timeline/playhead UI,
    // so scrubbing here doesn't fight the transport controls if playback
    // is also on screen. Range/labels are recomputed after every
    // preview() call since the span itself can change (duration/stagger).
    var scrubWrap=document.createElement('div');
    scrubWrap.style.cssText='margin:2px 0 10px';
    var scrubLabelRow=document.createElement('div');
    scrubLabelRow.style.cssText='display:flex;justify-content:space-between;color:var(--text-dim);margin-bottom:4px';
    var scrubLabel=document.createElement('span'); scrubLabel.textContent='Aperçu';
    var scrubFrameLabel=document.createElement('span');
    scrubLabelRow.appendChild(scrubLabel); scrubLabelRow.appendChild(scrubFrameLabel);
    var scrubInp=document.createElement('input');
    scrubInp.type='range'; scrubInp.style.width='100%'; scrubInp.value=state.currentFrame;
    scrubWrap.appendChild(scrubLabelRow); scrubWrap.appendChild(scrubInp);
    p.appendChild(scrubWrap);

    var lastUnitCount=0;
    function currentOpts(){
      return {
        mode:unitSel.value, preset:presetSel.value, easing:easeSel.value,
        startFrame:parseInt(startInp.value,10)||0,
        unitFrames:parseInt(durInp.value,10)||12,
        staggerFrames:parseInt(stagInp.value,10)||0,
      };
    }
    // Live preview: re-applies with skipUndo (the one pushUndo already
    // happened at open) — clearGroup inside apply() wipes the previous
    // pass first, so tweaking a slider back and forth never accumulates
    // orphan keyframes.
    function preview(){
      var opts=currentOpts();
      lastUnitCount=apply(li,groupId,Object.assign({skipUndo:true},opts));
      var span=opts.startFrame+Math.max(0,lastUnitCount-1)*opts.staggerFrames+opts.unitFrames;
      scrubInp.min=Math.max(0,opts.startFrame-2);
      scrubInp.max=Math.min(state.totalFrames-1,span+2);
      if(+scrubInp.value<+scrubInp.min||+scrubInp.value>+scrubInp.max)scrubInp.value=opts.startFrame;
      scrubFrameLabel.textContent='frame '+scrubInp.value;
      scrubToValue();
    }
    function scrubToValue(){
      var f=parseInt(scrubInp.value,10)||0;
      state.currentFrame=f;
      if(window.loadFrame)loadFrame(f);
      if(window.SMEngineBridge)SMEngineBridge.renderNow();
      scrubFrameLabel.textContent='frame '+f;
    }
    [unitSel,presetSel,easeSel].forEach(function(el){el.addEventListener('change',preview);});
    [startInp,durInp,stagInp].forEach(function(el){el.addEventListener('input',preview);el.addEventListener('change',preview);});
    scrubInp.addEventListener('input',scrubToValue);

    var btnRow=document.createElement('div');
    btnRow.style.cssText='display:flex;gap:8px;margin-top:4px';
    var cancelBtn=document.createElement('button');
    cancelBtn.textContent='Annuler'; cancelBtn.className='pbtn'; cancelBtn.style.flex='1';
    cancelBtn.onclick=function(){
      if(_panelUndoTaken&&window.undo)undo();
      closePanel();
    };
    var applyBtn=document.createElement('button');
    applyBtn.textContent='Terminé'; applyBtn.className='pbtn ac'; applyBtn.style.flex='1';
    applyBtn.onclick=function(){
      if(window.showToast)showToast(lastUnitCount?('Texte animé — '+lastUnitCount+' unité'+(lastUnitCount>1?'s':'')):'Aucune unité à animer trouvée');
      closePanel();
    };
    btnRow.appendChild(cancelBtn); btnRow.appendChild(applyBtn);
    p.appendChild(btnRow);

    document.body.appendChild(p);
    _panel=p;
    preview(); // show the default preset live the moment the panel opens
  }

  window.SMTextAnimator={
    PRESETS:PRESETS, PRESET_LABELS:PRESET_LABELS,
    unitsForGroup:unitsForGroup,
    groupIdForItem:groupIdForItem,
    apply:apply,
    openPanel:openPanel,
    closePanel:closePanel,
  };
})();
