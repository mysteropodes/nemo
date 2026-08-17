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

  // Writes the staggered keys for every unit of `groupId` on layer `li`.
  // opts: {mode:'char'|'word'|'line', preset, startFrame, unitFrames,
  // staggerFrames}. One undo step for the whole operation (pushUndo is
  // called ONCE up front, matching every other multi-mutation batch
  // action in this codebase — Cycle, Easy Ease on a multi-key selection,
  // etc. — never once per unit).
  function apply(li, groupId, opts){
    var ld=state.layers[li]; if(!ld)return 0;
    var preset=PRESETS[opts.preset]; if(!preset)return 0;
    var units=unitsForGroup(li,ld,groupId,opts.mode||'char');
    if(!units.length)return 0;
    if(window.pushUndo)pushUndo();
    var unitFrames=Math.max(1,opts.unitFrames||12);
    var staggerFrames=Math.max(0,opts.staggerFrames==null?3:opts.staggerFrames);
    var start=opts.startFrame==null?state.currentFrame:opts.startFrame;
    var curve=preset.curve||null;
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
  var _panel=null;
  function closePanel(){ if(_panel){_panel.remove();_panel=null;} }
  function openPanel(li, groupId){
    closePanel();
    var ld=state.layers[li]; if(!ld)return;
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

    var btnRow=document.createElement('div');
    btnRow.style.cssText='display:flex;gap:8px;margin-top:4px';
    var cancelBtn=document.createElement('button');
    cancelBtn.textContent='Annuler'; cancelBtn.className='pbtn'; cancelBtn.style.flex='1';
    cancelBtn.onclick=closePanel;
    var applyBtn=document.createElement('button');
    applyBtn.textContent='Appliquer'; applyBtn.className='pbtn ac'; applyBtn.style.flex='1';
    applyBtn.onclick=function(){
      var n=apply(li,groupId,{
        mode:unitSel.value, preset:presetSel.value,
        startFrame:parseInt(startInp.value,10)||0,
        unitFrames:parseInt(durInp.value,10)||12,
        staggerFrames:parseInt(stagInp.value,10)||0,
      });
      if(window.showToast)showToast(n?('Texte animé — '+n+' unité'+(n>1?'s':'')):'Aucune unité à animer trouvée');
      closePanel();
    };
    btnRow.appendChild(cancelBtn); btnRow.appendChild(applyBtn);
    p.appendChild(btnRow);

    document.body.appendChild(p);
    _panel=p;
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
