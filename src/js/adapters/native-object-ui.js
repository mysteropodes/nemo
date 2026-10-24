/* Private native document metadata presenter.
 * Owner: Ilya/A1; independent reviewer: Ilya/O Root.
 * API: render, select, toggleVisibility, restoreInspector, refreshTiming.
 * Rust owns document/revision state. Visibility uses the existing native.object
 * declaration/shared visibility handler through Project's SDK/transport queue.
 * UI state: this adapter's WeakMaps retain DOM bindings, read pins and the prior
 * inspector layout. Injected ports: projectionReady, currentSession (live at click),
 * selectLayer and repaint. No concrete Project/File provider is resolved here.
 * Other presentation ports: DOM, Reference bindings and props-panel layout helper.
 * Lifecycle: owner/revision/metadata/selection/DOM changes invalidate row reuse;
 * restoreInspector clears the cache and restores nodes/styles on non-object routing.
 * Frame-only invariant: unchanged inputs retain row/control DOM and focus.
 * Fixture: tests/native-object-ui.test.cjs (inline independent controls).
 * Timing uses the Project session's pinned SDK reader; installed acceptance stays open.
 */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectUI=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var inspectors=new WeakMap(),views=new WeakMap(),timings=new WeakMap(),policyTasks=new WeakMap(),policyEpochs=new WeakMap(),policyBindings=new WeakMap(),motionBindings=new WeakMap();
  function retireMotion(root){var hooks=motionBindings.get(root);if(hooks)hooks.retire();var prior=inspectors.get(root);if(prior&&prior.motion){var saved=prior.motion;saved.body.replaceChildren.apply(saved.body,saved.children);saved.section.hidden=saved.hidden;saved.section.style.display=saved.display;prior.motion=null;}}
  function bindMotion(root,hooks){
    if(!root||!hooks||typeof hooks.paint!=='function'||typeof hooks.retire!=='function')throw new TypeError('Native Motion UI hooks unavailable');
    if(motionBindings.has(root))throw new Error('Native Motion UI already bound');motionBindings.set(root,hooks);
    if(typeof hooks.setKeyTimelineListener==='function')hooks.setKeyTimelineListener(function(){
      var range=root.NemoNativeLayerRangeDesktop;if(range&&typeof range.refreshKeys==='function')range.refreshKeys(root);
    });
    return Object.freeze({dispose:function(){if(motionBindings.get(root)!==hooks)return;retireMotion(root);
      if(typeof hooks.setKeyTimelineListener==='function')hooks.setKeyTimelineListener(null);motionBindings.delete(root);}});
  }
  function paintMotion(root,ports,prior){
    if(!root.state||root.state.appMode!=='motion'){retireMotion(root);prior.notice.textContent='Native drawing property inspectors are unavailable.';return;}
    var hooks=motionBindings.get(root),section=root.document.getElementById('motion-props-sec'),body=root.document.getElementById('motion-props-body');
    var header=section&&section.querySelector('.phdr');
    prior.notice.textContent='Native Motion property values and edits are unavailable.';
    if(!hooks||!section||!body||!header)return;
    if(prior.motion&&(prior.motion.section!==section||prior.motion.body!==body||prior.motion.header!==header))retireMotion(root);
    if(!prior.motion){prior.motion={section:section,body:body,header:header,children:Array.from(body.childNodes||body.children),hidden:section.hidden,display:section.style.display};body.replaceChildren();}
    section.hidden=false;section.style.display='';
    if(hooks.paint(ports,body,header)!==true){retireMotion(root);prior.notice.textContent='Native Motion property metadata is unavailable.';}
  }
  function routeMotionShortcut(root,key,shift){
    var hooks=motionBindings.get(root);if(!hooks||typeof hooks.shortcut!=='function'||!paintMotionInspector(root))return false;
    Promise.resolve(hooks.shortcut(key,shift)).catch(function(){});return true;
  }
  function readSelectedMotionKeys(root,info){
    try{
      var view=views.get(root),ports=view&&view.ports,session=ports&&ports.currentSession(),hooks=motionBindings.get(root);
      if(!info||!session||!session.isActive()||!sameTimingPin(session.info(),info)||ports.projectionReady(info)!==true||
         !confirmed(root,info,session.readPresentedObjectSession())||!hooks||typeof hooks.captureSelectedKeys!=='function')return null;
      return hooks.captureSelectedKeys(info);
    }catch(_){return null;}
  }
  function readMotionKeyTimeline(root,info){
    try{
      var view=views.get(root),ports=view&&view.ports,session=ports&&ports.currentSession(),hooks=motionBindings.get(root);
      if(!info||!session||!session.isActive()||!sameTimingPin(session.info(),info)||ports.projectionReady(info)!==true||
         !confirmed(root,info,session.readPresentedObjectSession())||!hooks||typeof hooks.readKeyTimeline!=='function')return null;
      return hooks.readKeyTimeline();
    }catch(_){return null;}
  }
  function readMotionKeyTimelines(root,info){
    try{
      var view=views.get(root),ports=view&&view.ports,session=ports&&ports.currentSession(),hooks=motionBindings.get(root);
      if(!info||!session||!session.isActive()||!sameTimingPin(session.info(),info)||ports.projectionReady(info)!==true||
         !confirmed(root,info,session.readPresentedObjectSession())||!hooks||typeof hooks.readKeyTimelines!=='function')return null;
      return hooks.readKeyTimelines();
    }catch(_){return null;}
  }
  function setKeyTimelineRows(root,layout,info){
    var view=views.get(root);
    if(!view||!sameTimingPin(view.pin,info)||!Array.isArray(layout)||!view.rows.every(function(row,i){return row.parentNode===view.list&&row.children.length===4&&view.controls[i].every(function(n,k){return row.children[k]===n;});}))return false;
    var labels=new Map();
    for(var entry of layout){
      if(!entry||typeof entry.layerUid!=='string'||labels.has(entry.layerUid)||!Array.isArray(entry.labels)||!entry.labels.every(function(v){return typeof v==='string';})||!info.layers.some(function(l){return l.layerUid===entry.layerUid;}))return false;
      labels.set(entry.layerUid,entry.labels);
    }
    view.rows.forEach(function(row,i){
      var lines=labels.get(info.layers[i].layerUid)||[],pick=row.children[0],name=info.layers[i].name===undefined?info.layers[i].layerUid:info.layers[i].name;
      row.style.height=((1+lines.length)*22)+'px';row.style.alignItems=lines.length?'flex-start':'';
      pick.style.height=lines.length?'100%':'';pick.style.alignSelf=lines.length?'stretch':'';
      pick.style.paddingTop=lines.length?'0':'';pick.style.paddingBottom=lines.length?'0':'';
      if(!lines.length){pick.textContent=name;return;}
      var spans=[name].concat(lines).map(function(text){var span=createNativeObjectUiText(root,'span',text);Object.assign(span.style,{display:'block',height:'22px',lineHeight:'22px',textAlign:'left'});return span;});
      pick.replaceChildren.apply(pick,spans);
    });
    return true;
  }
  function paintMotionInspector(root){
    var view=views.get(root),prior=inspectors.get(root),ports=view&&view.ports,session=ports&&ports.currentSession(),info=session&&session.info();
    if(!prior||!info||!session.isActive()||!sameTimingPin(view.pin,info)||ports.projectionReady(info)!==true||!confirmed(root,info,session.readPresentedObjectSession()))return false;
    unavailableInspector(root,ports,info);return true;
  }
  function restoreInspector(root){
    if(root.NemoNativeLayerRangeDesktop)root.NemoNativeLayerRangeDesktop.retire(root);
    retireMotion(root);
    retirePolicy(root);
    views.delete(root);
    var timing=timings.get(root);if(timing){timing.latest=null;timing.serial++;timing.job=null;}
    var prior=inspectors.get(root);if(!prior)return;
    if(prior.fonts)prior.fonts.retire();
    if(prior.text)prior.text.retire();
    prior.children.forEach(function(saved){saved.node.hidden=saved.hidden;saved.node.style.display=saved.display;});
    if(prior.trigger&&prior.trigger.node.parentNode===prior.reference){
      var next=prior.trigger.next;
      prior.trigger.parent.insertBefore(prior.trigger.node,next&&next.parentNode===prior.trigger.parent?next:null);
    }
    if(root._lastPropsShow===prior.show){root._lastPropsShow=prior.previousShow;
      if(prior.rail)prior.rail.replaceChildren.apply(prior.rail,prior.railChildren);}
    if(prior.settings){if(root.ColorPicker&&root.ColorPicker.close)root.ColorPicker.close();prior.settings.moves.forEach(function(saved){
      saved.events.forEach(function(entry){if(saved.node.removeEventListener)saved.node.removeEventListener(entry[0],entry[1],entry[2]);});
      if(saved.marker.parentNode===saved.parent){saved.parent.insertBefore(saved.node,saved.marker);saved.parent.removeChild(saved.marker);}
      saved.node.hidden=saved.hidden;saved.node.style.display=saved.display;saved.node.style.pointerEvents=saved.pointer;
    });}
    if(prior.section.parentNode===prior.panel)prior.panel.removeChild(prior.section);
    inspectors.delete(root);
  }
  function settingsInspector(root,prior,ports,info){
    var controller=ports&&typeof ports.projectSettings==='function'&&ports.projectSettings();
    if(!controller||typeof controller.edit!=='function')return;
    if(!prior.settings){
      var box=createNativeObjectUiText(root,'div','');box.id='native-project-settings';
      var heading=createNativeObjectUiText(root,'div','Document');heading.className='phdr';box.appendChild(heading);
      prior.settings={box:box,moves:[],pin:null,bg:null};prior.section.appendChild(box);
      [['p-cw','Width'],['p-ch','Height'],['btn-dims-lock','Proportions'],['proj-fps','FPS'],['p-cbg','Background']].forEach(function(entry){
        var node=root.document.getElementById(entry[0]);if(!node||!node.parentNode)return;
        var parent=node.parentNode,marker=root.document.createComment('native settings position'),row=createNativeObjectUiText(root,'div','');row.className='pr';
        var label=createNativeObjectUiText(root,'span',entry[1]);label.className='pl';row.appendChild(label);
        var saved={node:node,parent:parent,marker:marker,hidden:node.hidden,display:node.style.display,pointer:node.style.pointerEvents,events:[],gesture:null};
        parent.insertBefore(marker,node);row.appendChild(node);node.hidden=false;node.style.display='';box.appendChild(row);prior.settings.moves.push(saved);
        function begin(){saved.gesture=Object.assign({},prior.settings.pin);}
        function guard(event){var dimension=entry[0]==='p-cw'||entry[0]==='p-ch',value=Number(node.value);
          try{var diagnostic=root.NemoNativeObjectProjectDesktop;if(diagnostic&&diagnostic.settingsDiagnostic){
            var session=ports.currentSession();diagnostic.settingsDiagnostic(root,event.type,Object.assign({},session&&session.info(),{
              kind:dimension?'setCanvasSize':entry[0]==='proj-fps'?'setFps':entry[0]==='p-cbg'?'setCanvasBackground':undefined}));}}catch(_){}
          var invalid=dimension&&(!Number.isInteger(value)||value<1||value>4294967295||parseInt(node.value,10)!==value);
          if(invalid||saved.gesture&&!['instanceId','documentId','lifecycleGeneration'].every(function(k){return saved.gesture[k]===prior.settings.pin[k];})){
          if(event.preventDefault)event.preventDefault();if(event.stopImmediatePropagation)event.stopImmediatePropagation();}}
        [['pointerdown',begin,true],['focus',begin,true],['input',guard,true],['change',guard,true]].forEach(function(args){node.addEventListener.apply(node,args);saved.events.push(args);});
        if(entry[0]==='p-cbg'){
          var wrap=createNativeObjectUiText(root,'div','');wrap.id='native-project-background';wrap.className='color-well';row.insertBefore(wrap,node);wrap.appendChild(node);
          wrap.addEventListener('pointerdown',begin,true);prior.settings.bg=wrap;
          if(root.ColorPicker&&root.ColorPicker.wireColorSwatches)root.ColorPicker.wireColorSwatches([{wrap:wrap.id,input:node.id}]);
        }
      });
    }
    var before=prior.settings.pin,changed=before&&!['instanceId','documentId','lifecycleGeneration'].every(function(k){return before[k]===info[k];});
    prior.settings.pin={instanceId:info.instanceId,documentId:info.documentId,lifecycleGeneration:info.lifecycleGeneration};
    if(changed){if(root.ColorPicker&&root.ColorPicker.close)root.ColorPicker.close();prior.settings.moves.forEach(function(saved){if(root.document.activeElement===saved.node&&saved.node.blur)saved.node.blur();});}
    prior.settings.ownerChanged=!!changed;
  }
  function unavailableInspector(root,ports,info){
    var panel=root.document.getElementById('props-panel');if(!panel)return;
    var prior=inspectors.get(root);
    if(prior&&prior.panel!==panel){restoreInspector(root);prior=null;}
    if(!prior){
      var section=createNativeObjectUiText(root,'div','');section.id='native-object-inspector';section.className='psec';
      var heading=createNativeObjectUiText(root,'div','Native document');heading.className='phdr';section.appendChild(heading);
      var notice=createNativeObjectUiText(root,'div','Native drawing property inspectors are unavailable.');notice.setAttribute('role','status');section.appendChild(notice);
      var rail=root.document.getElementById('props-panel-rail'),trigger=root.document.getElementById('p-ref-menu');
      prior={panel:panel,children:Array.from(panel.children).filter(function(child){return !['props-panel-hdr-row','props-panel-rail'].includes(child.id);})
        .map(function(child){return {node:child,hidden:child.hidden,display:child.style.display};}),section:section,
        notice:notice,rail:rail,railChildren:rail?Array.from(rail.children):[],previousShow:root._lastPropsShow,show:{'native-object-inspector':true}};
      if(trigger&&trigger.parentNode){
        var reference=createNativeObjectUiText(root,'div','Reference');reference.className='pr';
        prior.trigger={node:trigger,parent:trigger.parentNode,next:trigger.nextSibling};prior.reference=reference;
        reference.appendChild(trigger);section.appendChild(reference); // Reuse the bound trigger, never clone reference controls.
      }
      panel.appendChild(section);inspectors.set(root,prior);root._lastPropsShow=prior.show;
      if(root.renderPropsPanelRail)root.renderPropsPanelRail(prior.show);
      else if(rail){var expand=createNativeObjectUiText(root,'button','Native document');expand.type='button';expand.className='rail-item';
        expand.addEventListener('click',function(){if(root.togglePropsPanelCollapse)root.togglePropsPanelCollapse(false);else panel.classList.remove('collapsed');});rail.replaceChildren(expand);}
    }
    settingsInspector(root,prior,ports,info);
    if(!prior.fonts&&root.NemoNativeFontPanel&&typeof ports.fontResources==='function')prior.fonts=root.NemoNativeFontPanel.create(prior.section,{
      document:root.document,resources:ports.fontResources,readInfo:function(){var session=ports.currentSession();return session&&session.isActive()?session.info():null;}
    });
    if(prior.fonts)prior.fonts.refresh();
    if(root.NemoNativeTextToolDesktop)prior.text=root.NemoNativeTextToolDesktop.mount(root,prior.section,{currentSession:ports.currentSession,fontResources:ports.fontResources});
    prior.children.forEach(function(saved){saved.node.hidden=true;saved.node.style.display='none';});
    paintMotion(root,ports,prior);
  }
  function nativeObjectUiPortsReady(ports){
    return !!ports&&['projectionReady','currentSession','selectLayer','repaint'].every(function(k){return typeof ports[k]==='function';});
  }
  function checked(root,info,ports){
    if(!nativeObjectUiPortsReady(ports)||ports.projectionReady(info)!==true||
      !Number.isInteger(root.state.currentFrame)||root.state.currentFrame<0||root.state.currentFrame>=info.totalFrames)
      throw new Error('Native document UI metadata is uncorrelated');
    return info;
  }
  function node(root,id){var el=root.document.getElementById(id);if(!el)throw new Error('Native document UI is unavailable: '+id);return el;}
  function createNativeObjectUiText(root,tag,value){var el=root.document.createElement(tag);el.textContent=value;return el;}
  function sameTimingPin(a,b){return !!a&&!!b&&['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'].every(function(k){return a[k]===b[k];});}
  function timingText(value){
    var kind={key:'Key',interpolated:'Sample',held:'Held',unexposed:'Unexposed'}[value.cellKind];
    return kind+' — '+(value.sourceFrame===null?'No source frame':'Source frame '+(value.sourceFrame+1))+
      ' — '+(value.hasContent?'Content':'Blank')+(value.manualEdit?' — Manual edit':'');
  }
  function requestTiming(root,info,ports){
    var view=views.get(root),session=ports.currentSession(),layer=info.layers[root.state.activeLayerIdx];
    var work=timings.get(root);if(!work){work={serial:0,running:false,latest:null,job:null};timings.set(root,work);}
    var selection=(root._layerSel||[]).slice(),frame=root.state.currentFrame;
    var ready=!!(session&&typeof session.readPresentedObjectSession==='function'&&confirmed(root,info,session.readPresentedObjectSession()));
    var old=work.job;
    if(old&&old.view===view&&old.session===session&&old.frame===frame&&old.uid===(layer&&layer.layerUid)&&(work.running||old.ready===ready)&&
      sameTimingPin(old.info,info)&&old.selection.length===selection.length&&old.selection.every(function(v,i){return v===selection[i];}))return;
    var job={serial:++work.serial,view:view,ports:ports,session:session,info:info,frame:frame,uid:layer&&layer.layerUid,selection:selection,ready:ready};
    work.job=job;work.latest=job;view.unavailable.textContent='Native timing is unavailable.';
    function current(value){
      var ports=value.ports,selected=root._layerSel||[],live=ports.currentSession();
      return work.serial===value.serial&&views.get(root)===value.view&&live===value.session&&live&&live.isActive()&&
        ports.projectionReady(value.info)===true&&sameTimingPin(live.info(),value.info)&&root.state.currentFrame===value.frame&&
        value.info.layers[root.state.activeLayerIdx]&&value.info.layers[root.state.activeLayerIdx].layerUid===value.uid&&
        selected.length===value.selection.length&&selected.every(function(v,i){return v===value.selection[i];})&&
        root.document.getElementById('frame-grid')===value.view.grid&&value.view.grid.children[0]===value.view.unavailable&&
        confirmed(root,value.info,live.readPresentedObjectSession());
    }
    if(work.running)return;
    work.running=true;
    // One outstanding read per root, including replaced owners and detached views.
    (async function(){try{
      while(work.latest){var next=work.latest;work.latest=null;var result=null;
        try{if(next.uid!==undefined&&next.session&&typeof next.session.readLayerTiming==='function')
          result=await next.session.readLayerTiming(next.uid,next.frame,next.info);}catch(_){}
        next.ready=!!(next.session&&typeof next.session.readPresentedObjectSession==='function'&&confirmed(root,next.info,next.session.readPresentedObjectSession()));
        if(current(next))next.view.unavailable.textContent=result?timingText(result):'Native timing is unavailable.';
      }
    }finally{work.running=false;}}()).catch(function(){});
  }
  function refreshTiming(root){
    var view=views.get(root),session=view&&view.ports.currentSession(),info=session&&session.info();
    if(view&&session&&session.isActive()&&sameTimingPin(view.pin,info)&&view.ports.projectionReady(info)===true)
      requestTiming(root,info,view.ports);
  }
  function renderNativeObjectUiMetadata(root,info,frameOnly,ports){
    checked(root,info,ports);
    if(info.layers.some(function(l){return ['locked','shy'].some(function(k){return l[k]!==undefined&&typeof l[k]!=='boolean';});}))throw new Error('Invalid native layer flags');
    var shyMode=typeof ports.readShyMode==='function'&&ports.readShyMode()===true,policyOwner=policyPort(ports);
    var list=node(root,'layer-list'),header=node(root,'frame-hdr'),grid=node(root,'frame-grid');
    var previous=views.get(root),selection=root._layerSel||[];
    var reuse=frameOnly===true&&previous&&previous.ports===ports&&previous.list===list&&previous.header===header&&previous.grid===grid&&
      previous.shyMode===shyMode&&previous.inspector===root.document.getElementById('props-panel')&&previous.active===root.state.activeLayerIdx&&
      ['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId','totalFrames'].every(function(k){return previous.pin[k]===info[k];})&&
      selection.length===previous.selection.length&&selection.every(function(value,i){return previous.selection[i]===value;})&&
      info.layers.length===previous.metadata.length&&info.layers.every(function(layer,i){var old=previous.metadata[i];return old[0]===layer.layerUid&&old[1]===layer.name&&old[2]===layer.visible&&old[3]===layer.locked&&old[4]===layer.shy;})&&
      list.children.length===previous.rows.length&&previous.rows.every(function(row,i){return list.children[i]===row&&
        row.children.length===4&&previous.controls[i].every(function(control,k){return row.children[k]===control;});})&&
      header.children.length===1&&header.children[0]===previous.caption&&((root.NemoNativeLayerRangeDesktop&&root.NemoNativeLayerRangeDesktop.owns(root,grid,previous.unavailable))||(grid.children.length===1&&grid.children[0]===previous.unavailable));
    var rows,caption,unavailable;
    if(reuse){rows=previous.rows;caption=previous.caption;unavailable=previous.unavailable;}
    else{
      rows=info.layers.map(function(layer,i){
        var label=layer.name===undefined?layer.layerUid:layer.name;
        var row=createNativeObjectUiText(root,'div',''),pick=createNativeObjectUiText(root,'button',label+' — '+(layer.visible===false?'Hidden':'Visible'));
        row.className='native-object-layer';row.dataset.layerUid=layer.layerUid;row.style.display='flex';
        pick.type='button';pick.style.flex='1';pick.style.textAlign='left';
        pick.setAttribute('aria-pressed',String(selection.indexOf(i)>=0));
        pick.setAttribute('aria-label',label+' ('+layer.layerUid+')');
        pick.addEventListener('click',function(event){
          var view=views.get(root);
          if(!nativeObjectUiPortsReady(ports)||row.parentNode!==list||!view||view.rows[i]!==row)return false;
          if(root._layerDragJustEnded){root._layerDragJustEnded=false;return false;}
          var mode=event&&(event.metaKey||event.ctrlKey)?'toggle':event&&event.shiftKey?'range':null;
          return ports.selectLayer(i,mode?{mode:mode}:false,layer.layerUid,info);
        });
        var eye=createNativeObjectUiText(root,'button',layer.visible===false?'Show':'Hide');eye.type='button';
        eye.setAttribute('aria-label',(layer.visible===false?'Show ':'Hide ')+label+' ('+layer.layerUid+')');
        // Availability is checked at CLICK time: this render can precede preview/ACK.
        eye.addEventListener('click',function(){return toggleVisibility(root,layer.layerUid,info,ports);});
        var flags=['locked','shy'].map(function(flag){
          var on=layer[flag]===true,label=flag==='locked'?(on?'Unlock':'Lock'):(on?'Unmark shy':'Mark shy');
          var button=createNativeObjectUiText(root,'button',label);button.type='button';button.dataset.policyFlag=flag;
          button.setAttribute('aria-pressed',String(on));button.setAttribute('aria-label',label+' '+(layer.name===undefined?layer.layerUid:layer.name));
          button.disabled=!policyOwner;button.addEventListener('click',function(){if(row.parentNode!==list||!views.get(root)||!views.get(root).rows.includes(row))return false;return setLayerPolicy(root,layer.layerUid,flag,!on,info,ports);});return button;
        });
        row.hidden=shyMode&&layer.shy===true;row.style.display=row.hidden?'none':'flex';row.replaceChildren(pick,eye,flags[0],flags[1]);
        return row;
      });
      caption=createNativeObjectUiText(root,'div','');
      unavailable=createNativeObjectUiText(root,'div','Native keyframe, held-frame and tween timing is unavailable.');
      unavailable.setAttribute('role','status');unavailable.className='native-object-timing-unavailable';
      // Build all nodes before replacing any visible legacy rows/cells.
      list.replaceChildren.apply(list,rows);header.replaceChildren(caption);grid.replaceChildren(unavailable);
    }
    if(root.NemoNativeLayerRangeDesktop&&typeof root.NemoNativeLayerRangeDesktop.mount==='function')
      root.NemoNativeLayerRangeDesktop.mount(root,grid,info,Object.assign({},ports,{setKeyTimelineRows:function(layout,expected){return setKeyTimelineRows(root,layout,expected);},readMotionKeyTimelines:function(expected){return readMotionKeyTimelines(root,expected);},readMotionKeyTimeline:function(expected){return readMotionKeyTimeline(root,expected);},readSelectedMotionKeys:function(expected){return readSelectedMotionKeys(root,expected);},readLegacyMotionSelection:function(){var motion=root.SMMotion;return motion&&typeof motion.getKeySelection==='function'?motion.getKeySelection():null;},barUi:root.SMLayerInOut}));
    caption.textContent='Frame '+(root.state.currentFrame+1)+' / '+info.totalFrames;
    unavailableInspector(root,ports,info);
    if(!reuse){var pin={};['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId','totalFrames'].forEach(function(k){pin[k]=info[k];});
      views.set(root,{list:list,header:header,grid:grid,inspector:root.document.getElementById('props-panel'),rows:rows,
        controls:rows.map(function(row){return Array.from(row.children);}),shyMode:shyMode,caption:caption,unavailable:unavailable,
        ports:ports,pin:pin,active:root.state.activeLayerIdx,selection:selection.slice(),metadata:info.layers.map(function(layer){return [layer.layerUid,layer.name,layer.visible,layer.locked,layer.shy];})});}
    var field=root.document.getElementById('tl-cf');
    if(field){if(root.document.activeElement!==field)field.value=root.state.currentFrame+1;field.max=info.totalFrames;}
    ['tl-tf','info-frame'].forEach(function(id){var el=root.document.getElementById(id);if(el)el.textContent=id==='tl-tf'?info.totalFrames:root.state.currentFrame+1;});
    // These are read-side projections only; legacy field event handlers remain
    // guarded and no legacy document/frame reader is called.
    var metadata={'p-cw':info.canvasW,'p-ch':info.canvasH,'tl-fps':info.fps,'tl-total':info.totalFrames,
      'proj-fps':info.fps,'proj-frames':info.totalFrames};
    var settings=inspectors.get(root),ownerChanged=settings&&settings.settings&&settings.settings.ownerChanged;
    Object.keys(metadata).forEach(function(id){var el=root.document.getElementById(id);if(el&&(ownerChanged||root.document.activeElement!==el))el.value=metadata[id];});
    var background=root.document.getElementById('p-cbg'),raw=info.canvasBg===undefined?root.state.canvasBg:info.canvasBg;
    if(background&&(ownerChanged||root.document.activeElement!==background)&&typeof raw==='string'){
      var hex=raw.replace(/^#/,'');if(hex.length===3)hex=hex.split('').map(function(c){return c+c;}).join('');
      background.value='#'+hex.slice(0,6);background.dataset.hex8='#'+(hex.length===8?hex:hex+'ff');
      if(settings&&settings.settings&&settings.settings.bg)settings.settings.bg.style.backgroundColor=background.dataset.hex8;
    }
    if(root.updateWaBar&&root.document.getElementById('wa-bar'))root.updateWaBar();
    ['info-badge','info-strokes','info-sel'].forEach(function(id){var el=root.document.getElementById(id);if(el){el.textContent='Unavailable';if(id==='info-badge')el.style.display='none';}});
    requestTiming(root,info,ports);refreshPolicy(root);
    return true;
  }
  // Ordinary native policy entries bind on paint and dispose when the UI retires.
  function bindLayerPolicy(root,ports){
    if(policyBindings.has(root))throw new Error('Layer policy entry is already bound');
    if(!root.SM||['toggleLayerLock','toggleLayerShy','toggleShyMode'].some(function(k){return typeof root.SM[k]!=='function';}))throw new Error('Ordinary layer policy entries are unavailable');
    var original={},bound={},retired=false;
    ['toggleLayerLock','toggleLayerShy','toggleShyMode'].forEach(function(name){
      original[name]=root.SM[name];bound[name]=function(index){
        if(retired)return false;
        var session=ports.currentSession(),owned=session&&typeof session.blocksLegacy==='function'?session.blocksLegacy():null;
        if(owned===false)return original[name].apply(this,arguments);
        if(owned!==true||!session.isActive())return false;
        var info=session.info();if(!info||ports.projectionReady(info)!==true||!confirmed(root,info,session.readPresentedObjectSession()))return false;
        if(name==='toggleShyMode'){
          if(typeof ports.readShyMode!=='function'||typeof ports.setShyMode!=='function')return false;
          ports.setShyMode(ports.readShyMode()!==true,info);return renderNativeObjectUiMetadata(root,info,true,ports);
        }
        var i=index==null?root.state.activeLayerIdx:index;if(!Number.isInteger(i)||i<0||i>=info.layers.length)return false;
        var layer=info.layers[i],flag=name==='toggleLayerLock'?'locked':'shy';return setLayerPolicy(root,layer.layerUid,flag,layer[flag]!==true,info,ports);
      };root.SM[name]=bound[name];
    });
    var binding=Object.freeze({dispose:function(){if(retired)return;retired=true;retirePolicy(root);Object.keys(bound).forEach(function(name){if(root.SM[name]===bound[name])root.SM[name]=original[name];});policyBindings.delete(root);}});policyBindings.set(root,binding);return binding;
  }
  function createPolicyPorts(root,base){
    var entry=null;
    function current(expected){
      var session=base.currentSession(),info=session&&session.info();
      return session&&session.isActive()&&session.blocksLegacy()===true&&info&&base.projectionReady(info)===true&&(!expected||sameTimingPin(expected,info))&&confirmed(root,info,session.readPresentedObjectSession())?session:null;
    }
    var owner=Object.freeze({edit:async function(payload,pin){
      var session=current(pin),method=session&&session.editLayerPolicy;if(typeof method!=='function')return false;
      var receipt=await method(payload,pin);return base.currentSession()===session&&session.isActive()&&receipt&&receipt.ok===true;
    }});
    return Object.freeze({
      layerPolicy:function(){var session=current();return session&&typeof session.editLayerPolicy==='function'?owner:null;},
      readTransportGeneration:function(){var session=base.currentSession();return session&&typeof session.readSubscriptionGeneration==='function'?session.readSubscriptionGeneration():null;},
      readShyMode:function(){return root.state.shyEnabled===true;},
      setShyMode:function(value,info){if(typeof value!=='boolean'||!current(info))return false;
        root.state.shyEnabled=value;var button=root.document.getElementById('btn-shy');if(button&&button.classList&&typeof button.classList.toggle==='function')button.classList.toggle('active',value);return true;},
      clearDrawingSelection:function(uid,info){var session=current(info);if(!session)return false;
        var live=session.info(),layer=live.layers[root.state.activeLayerIdx];
        if(root.state.appMode!=='drawing'||!layer||layer.layerUid!==uid)return true;
        if(layer.locked!==true)return false;root.selectedPaths=[];root.state.selectedStrokeIndices=[];return true;},
      bindPolicyEntries:function(){
        if(entry)return true;if(!root.SM||['toggleLayerLock','toggleLayerShy','toggleShyMode'].some(function(name){return typeof root.SM[name]!=='function';}))return false;
        entry=bindLayerPolicy(root,base);return true;},
      disposePolicyEntries:function(){if(entry){entry.dispose();entry=null;}}
    });
  }
  function refreshPolicy(root){
    var view=views.get(root);if(!view||view.list!==root.document.getElementById('layer-list')||!view.rows.every(function(row,i){return row.parentNode===view.list&&view.controls[i].every(function(control,k){return row.children[k]===control;});}))return false;
    var ready=false,owner=null;
    try{var ports=view.ports,session=ports.currentSession(),info=session&&session.info();owner=policyPort(ports);
      ready=!!owner&&!policyTasks.has(root)&&!!session&&session.isActive()&&session.blocksLegacy()===true&&sameTimingPin(view.pin,info)&&ports.projectionReady(info)===true&&confirmed(root,info,session.readPresentedObjectSession());
    }catch(_){}
    view.controls.forEach(function(controls){controls.slice(2).forEach(function(button){button.disabled=!ready;});});
    return ready;
  }
  function policyPort(ports){try{var value=typeof ports.layerPolicy==='function'&&ports.layerPolicy();return value&&typeof value.edit==='function'?value:null;}catch(_){return null;}}
  function retirePolicy(root){policyEpochs.set(root,(policyEpochs.get(root)||0)+1);policyTasks.delete(root);views.delete(root);}
  function policySelection(root){return JSON.stringify([root.state.appMode,root.state.activeLayerIdx,root._layerSel||[]]);}
  function setLayerPolicy(root,uid,flag,value,expected,ports){
    function refused(){if(root.showToast)root.showToast('Native layer policy was not confirmed.');return false;}
    try{
      if(!nativeObjectUiPortsReady(ports)||!['locked','shy'].includes(flag)||typeof value!=='boolean'||typeof ports.readTransportGeneration!=='function'||!Number.isSafeInteger(ports.readTransportGeneration())||ports.readTransportGeneration()<0||policyTasks.has(root))return Promise.resolve(false);
      var owner=policyPort(ports),session=ports.currentSession(),info=session&&session.info(),view=views.get(root),epoch=policyEpochs.get(root)||0;
      var layer=info&&info.layers.filter(function(l){return l.layerUid===uid;});
      if(!view||!sameTimingPin(view.pin,info)||!owner||!session||!session.isActive()||!sameTimingPin(expected,info)||!confirmed(root,info,session.readPresentedObjectSession())||!layer||layer.length!==1||ports.projectionReady(info)!==true)return Promise.resolve(false);
      if(['locked','shy'].some(function(k){return layer[0][k]!==undefined&&typeof layer[0][k]!=='boolean';}))return Promise.resolve(false);
      if(value&&(flag==='locked'&&root.state.appMode==='drawing'&&typeof ports.clearDrawingSelection!=='function'||flag==='shy'&&(typeof ports.setShyMode!=='function'||typeof ports.readShyMode!=='function')))return Promise.resolve(false);
      if((layer[0][flag]===true)===value)return Promise.resolve(true); // Absence reads false without authoring false.
      var signature=policySelection(root),transport=ports.readTransportGeneration?ports.readTransportGeneration():null,pin={};
      ['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'].forEach(function(k){pin[k]=info[k];});
      var request=Object.freeze({command:'object.layer.policy.edit',layerUid:uid,flag:flag,value:value}),ticket={};policyTasks.set(root,ticket);refreshPolicy(root);
      return Promise.resolve().then(function(){
        if(policySelection(root)!==signature||ports.currentSession()!==session||!session.isActive()||(policyEpochs.get(root)||0)!==epoch||ports.readTransportGeneration&&ports.readTransportGeneration()!==transport||policyPort(ports)!==owner||!sameTimingPin(pin,session.info())||!confirmed(root,session.info(),session.readPresentedObjectSession()))return false;
        return owner.edit(request,Object.freeze(pin));
      }).then(function(result){
        if(result!==true||policyTasks.get(root)!==ticket||(policyEpochs.get(root)||0)!==epoch||policySelection(root)!==signature||ports.currentSession()!==session||!session.isActive()||!views.get(root)||views.get(root).ports!==ports||views.get(root).list!==view.list||policyPort(ports)!==owner||ports.readTransportGeneration&&ports.readTransportGeneration()!==transport)return refused();
        var next=session.info(),target=next&&next.layers.filter(function(l){return l.layerUid===uid;});
        if(!next||!['instanceId','documentId','lifecycleGeneration'].every(function(k){return next[k]===pin[k];})||next.contentRevision<=pin.contentRevision||next.documentSnapshotId===pin.documentSnapshotId||!target||target.length!==1||target[0][flag]!==value||ports.projectionReady(next)!==true||!confirmed(root,next,session.readPresentedObjectSession()))return refused();
        // Only UI-owned selection/presentation ports run after native confirmation.
        if(flag==='locked'&&value&&root.state.appMode==='drawing'&&typeof ports.clearDrawingSelection==='function')ports.clearDrawingSelection(uid,next);
        if(flag==='shy'&&value&&typeof ports.setShyMode==='function')ports.setShyMode(true,next);
        return renderNativeObjectUiMetadata(root,next,false,ports);
      }).catch(refused).finally(function(){if(policyTasks.get(root)===ticket){policyTasks.delete(root);refreshPolicy(root);}});
    }catch(_){return Promise.resolve(refused());}
  }
  function confirmed(root,info,presented){
    return !!presented&&presented.available===true&&presented.kind==='native-object'&&
      ['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'].every(function(k){return presented[k]===info[k];})&&
      !!presented.presentation&&presented.presentation.owner==='native'&&presented.presentation.status==='presented'&&presented.presentation.frame===root.state.currentFrame&&
      ['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'].every(function(k){return presented.presentation[k]===info[k];});
  }
  function toggleVisibility(root,uid,expected,ports){
    function refused(){try{if(root.showToast)root.showToast('Native layer visibility was not confirmed.');}catch(_){}return false;}
    try{
      if(!nativeObjectUiPortsReady(ports))return Promise.resolve(false);
      var session=ports.currentSession(),info=session&&session.info();
      if(!session||!session.isActive()||!expected||!info||
        !['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'].every(function(k){return expected[k]===info[k];})||
        !confirmed(root,info,session.readPresentedObjectSession()))return Promise.resolve(false);
      checked(root,info,ports);
      var toggle=session.toggleLayerVisibility;
      if(typeof toggle!=='function'||info.layers.filter(function(layer){return layer.layerUid===uid;}).length!==1)return Promise.resolve(false);
      if(ports.repaint(true)!==true)return Promise.resolve(false);
      return Promise.resolve(toggle(uid)).catch(refused);
    }catch(_){return Promise.resolve(refused());}
  }
  function rowSelection(root,count,index,mode){
    function valid(value){return Number.isInteger(value)&&value>=0&&value<count;}
    var prior=Array.isArray(root._layerSel)?root._layerSel:[],selection=prior.filter(function(value,i){return valid(value)&&prior.indexOf(value)===i;});
    var anchor=root._layerSelAnchor,active=root.state.activeLayerIdx;
    if(mode==='toggle'){
      if(valid(active)&&selection.indexOf(active)<0)selection.push(active);
      var position=selection.indexOf(index);
      if(position>=0)selection.splice(position,1);else selection.push(index);
      anchor=index;
    }else{
      var start=valid(anchor)?anchor:selection.length?selection[0]:valid(active)?active:index;
      selection=[];
      for(var i=Math.min(start,index);i<=Math.max(start,index);i++)selection.push(i);
    }
    return {selection:selection,anchor:anchor};
  }
  function select(root,info,index,preserveLayerSel,uid,presented,ports){
    checked(root,info,ports);
    if(!Number.isInteger(index)||index<0||index>=info.layers.length||uid!==undefined&&info.layers[index].layerUid!==uid)return false;
    if(!confirmed(root,info,presented))return false;
    var selection=null;
    if(preserveLayerSel&&typeof preserveLayerSel==='object'){
      if(!['toggle','range'].includes(preserveLayerSel.mode))return false;
      selection=rowSelection(root,info.layers.length,index,preserveLayerSel.mode);
    }
    var old={active:root.state.activeLayerIdx,selection:root._layerSel,anchor:root._layerSelAnchor,paths:root.selectedPaths,
      indices:root.state.selectedStrokeIndices,explicit:root._layerActiveExplicit};
    root.state.activeLayerIdx=index;
    if(selection){root._layerSel=selection.selection;root._layerSelAnchor=selection.anchor;}
    else if(!preserveLayerSel){root._layerSel=[index];root._layerSelAnchor=index;}
    root.selectedPaths=[];root.state.selectedStrokeIndices=[];root._layerActiveExplicit=true;
    try{return renderNativeObjectUiMetadata(root,info,false,ports);}
    catch(error){root.state.activeLayerIdx=old.active;root._layerSel=old.selection;root._layerSelAnchor=old.anchor;
      root.selectedPaths=old.paths;root.state.selectedStrokeIndices=old.indices;root._layerActiveExplicit=old.explicit;throw error;}
  }
  return Object.freeze({render:renderNativeObjectUiMetadata,select:select,toggleVisibility:toggleVisibility,restoreInspector:restoreInspector,refreshTiming:refreshTiming,setLayerPolicy:setLayerPolicy,retirePolicy:retirePolicy,bindLayerPolicy:bindLayerPolicy,refreshPolicy:refreshPolicy,createPolicyPorts:createPolicyPorts,bindMotion:bindMotion,readSelectedMotionKeys:readSelectedMotionKeys,paintMotionInspector:paintMotionInspector,routeMotionShortcut:routeMotionShortcut});
}));
