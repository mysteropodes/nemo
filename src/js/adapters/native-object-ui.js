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
  var inspectors=new WeakMap(),views=new WeakMap(),timings=new WeakMap();
  function restoreInspector(root){
    views.delete(root);
    var timing=timings.get(root);if(timing){timing.latest=null;timing.serial++;timing.job=null;}
    var prior=inspectors.get(root);if(!prior)return;
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
        rail:rail,railChildren:rail?Array.from(rail.children):[],previousShow:root._lastPropsShow,show:{'native-object-inspector':true}};
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
    prior.children.forEach(function(saved){saved.node.hidden=true;saved.node.style.display='none';});
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
    var list=node(root,'layer-list'),header=node(root,'frame-hdr'),grid=node(root,'frame-grid');
    var previous=views.get(root),selection=root._layerSel||[];
    var reuse=frameOnly===true&&previous&&previous.ports===ports&&previous.list===list&&previous.header===header&&previous.grid===grid&&
      previous.inspector===root.document.getElementById('props-panel')&&previous.active===root.state.activeLayerIdx&&
      ['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId','totalFrames'].every(function(k){return previous.pin[k]===info[k];})&&
      selection.length===previous.selection.length&&selection.every(function(value,i){return previous.selection[i]===value;})&&
      info.layers.length===previous.metadata.length&&info.layers.every(function(layer,i){var old=previous.metadata[i];return old[0]===layer.layerUid&&old[1]===layer.name&&old[2]===layer.visible;})&&
      list.children.length===previous.rows.length&&previous.rows.every(function(row,i){return list.children[i]===row&&
        row.children.length===2&&row.children[0]===previous.controls[i][0]&&row.children[1]===previous.controls[i][1];})&&
      header.children.length===1&&header.children[0]===previous.caption&&grid.children.length===1&&grid.children[0]===previous.unavailable;
    var rows,caption,unavailable;
    if(reuse){rows=previous.rows;caption=previous.caption;unavailable=previous.unavailable;}
    else{
      rows=info.layers.map(function(layer,i){
        var label=layer.name===undefined?layer.layerUid:layer.name;
        var row=createNativeObjectUiText(root,'div',''),pick=createNativeObjectUiText(root,'button',label+' — '+(layer.visible===false?'Hidden':'Visible'));
        row.className='native-object-layer';row.dataset.layerUid=layer.layerUid;row.style.display='flex';
        pick.type='button';pick.style.flex='1';pick.style.textAlign='left';
        pick.setAttribute('aria-pressed',String(i===root.state.activeLayerIdx));
        pick.setAttribute('aria-label',label+' ('+layer.layerUid+')');
        pick.addEventListener('click',function(){if(!nativeObjectUiPortsReady(ports))return false;return ports.selectLayer(i,false,layer.layerUid,info);});
        var eye=createNativeObjectUiText(root,'button',layer.visible===false?'Show':'Hide');eye.type='button';
        eye.setAttribute('aria-label',(layer.visible===false?'Show ':'Hide ')+label+' ('+layer.layerUid+')');
        // Availability is checked at CLICK time: this render can precede preview/ACK.
        eye.addEventListener('click',function(){return toggleVisibility(root,layer.layerUid,info,ports);});
        row.replaceChildren(pick,eye);
        return row;
      });
      caption=createNativeObjectUiText(root,'div','');
      unavailable=createNativeObjectUiText(root,'div','Native keyframe, held-frame and tween timing is unavailable.');
      unavailable.setAttribute('role','status');unavailable.className='native-object-timing-unavailable';
      // Build all nodes before replacing any visible legacy rows/cells.
      list.replaceChildren.apply(list,rows);header.replaceChildren(caption);grid.replaceChildren(unavailable);
    }
    caption.textContent='Frame '+(root.state.currentFrame+1)+' / '+info.totalFrames;
    unavailableInspector(root,ports,info);
    if(!reuse){var pin={};['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId','totalFrames'].forEach(function(k){pin[k]=info[k];});
      views.set(root,{list:list,header:header,grid:grid,inspector:root.document.getElementById('props-panel'),rows:rows,
        controls:rows.map(function(row){return [row.children[0],row.children[1]];}),caption:caption,unavailable:unavailable,
        ports:ports,pin:pin,active:root.state.activeLayerIdx,selection:selection.slice(),metadata:info.layers.map(function(layer){return [layer.layerUid,layer.name,layer.visible];})});}
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
    requestTiming(root,info,ports);
    return true;
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
  function select(root,info,index,preserveLayerSel,uid,presented,ports){
    checked(root,info,ports);
    if(!Number.isInteger(index)||index<0||index>=info.layers.length||uid!==undefined&&info.layers[index].layerUid!==uid)return false;
    if(!confirmed(root,info,presented))return false;
    var old={active:root.state.activeLayerIdx,selection:root._layerSel,anchor:root._layerSelAnchor,paths:root.selectedPaths,
      indices:root.state.selectedStrokeIndices,explicit:root._layerActiveExplicit};
    root.state.activeLayerIdx=index;
    if(!preserveLayerSel){root._layerSel=[index];root._layerSelAnchor=index;}
    root.selectedPaths=[];root.state.selectedStrokeIndices=[];root._layerActiveExplicit=true;
    try{return renderNativeObjectUiMetadata(root,info,false,ports);}
    catch(error){root.state.activeLayerIdx=old.active;root._layerSel=old.selection;root._layerSelAnchor=old.anchor;
      root.selectedPaths=old.paths;root.state.selectedStrokeIndices=old.indices;root._layerActiveExplicit=old.explicit;throw error;}
  }
  return Object.freeze({render:renderNativeObjectUiMetadata,select:select,toggleVisibility:toggleVisibility,restoreInspector:restoreInspector,refreshTiming:refreshTiming});
}));
