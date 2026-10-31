/* Existing tab UI composition; native documents stay in their original host. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProjectTabsUI=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function view(root){
    var s=root.state,indices=root._layerSel;
    if(!s||s.activeSymbolId!==null||!Array.isArray(indices)||!Number.isSafeInteger(s.currentFrame)||
        s.currentFrame<0||s.currentFrame>=s.totalFrames)throw new Error('Native tab view context is unavailable');
    function uid(index){return Number.isInteger(index)&&s.layers[index]?s.layers[index].layerUid:null;}
    var uids=indices.map(uid);
    if(uids.some(function(value){return typeof value!=='string'||!value;})||new Set(uids).size!==uids.length)
      throw new Error('Native layer selection context is unavailable');
    return Object.freeze({frame:s.currentFrame,layerUids:Object.freeze(uids),
      activeLayerUid:uid(s.activeLayerIdx),anchorLayerUid:uid(root._layerSelAnchor)});
  }
  function sameView(a,b){return a.frame===b.frame&&a.activeLayerUid===b.activeLayerUid&&
    a.anchorLayerUid===b.anchorLayerUid&&a.layerUids.length===b.layerUids.length&&
    a.layerUids.every(function(uid,index){return uid===b.layerUids[index];});}
  function bind(root,ports){
    return root.NemoNativeObjectInactiveProjectTabs.create(root,{
      focusOpen:function(path){
        var open=findOpen(ports.read(),path);if(!open||!open.tab)return null;
        ports.hideStart();
        if(open.active){ports.notice('Already open: '+root.SMProjectDocument.baseName(path));return {status:'already-open'};}
        return Promise.resolve(ports.switchTab(open.tab.id));
      },
      owner:function(){var s=root.NemoNativeObjectProject.desktop(root);return s&&s.identity();},
      invoke:function(command,args){var response=root.__TAURI__.core.invoke(command,args),gate=root.NemoNativeSettingsValidationGate;
        return gate&&command==='nemo_native_object_inactive_project'?
          gate.hold(root,'inactive-inspect',JSON.parse(args.requestJson),response):response;},
      requestId:function(){return root.crypto.randomUUID();},
      baseName:function(path){return root.SMProjectDocument.baseName(path);},notice:ports.notice,
      commit:function(meta){
        var tabs=ports.read().tabs,t=tabs.find(function(x){return x.nativeInactiveProject&&x.nativeInactiveProject.preparationToken===meta.preparationToken;});
        if(t){t.nativeInactiveProject=meta;t.name=meta.name;t.path=meta.path;ports.render();return true;}
        tabs.push({id:ports.makeId(),name:meta.name,path:meta.path,json:null,nativeInactiveProject:meta});
        ports.render();return true;
      },
      remove:function(meta){
        var tabs=ports.read().tabs,index=tabs.findIndex(function(x){return x.nativeInactiveProject&&x.nativeInactiveProject.preparationToken===meta.preparationToken;});
        if(index<0)return false;tabs.splice(index,1);ports.render();return true;
      },
      capture:async function(meta){
        function refused(reason){var diagnostic=root.NemoNativeObjectProjectDesktop;
          if(diagnostic&&typeof diagnostic.openDiagnostic==='function')diagnostic.openDiagnostic(root,'tab-capture-refused',reason);}
        var current=ports.read(),session=root.NemoNativeObjectProject.desktop(root);
        var incoming=current.tabs.find(function(t){return t.nativeInactiveProject===meta;});
        if(!incoming||incoming.id===current.activeTabId||!current.tabs.some(function(t){return t.id===current.activeTabId;})){
          refused('tab-unavailable');throw new Error('Native tab UI owner is unavailable');}
        var captured=Object.freeze({activeTabId:current.activeTabId,targetTabId:incoming.id,path:current.path,name:current.name,
          dirty:ports.dirty(),view:view(root),targetView:meta.view});
        if(!session||typeof session.capturePresentationSession!=='function'){
          refused('capture-unavailable');throw new Error('Native tab UI owner is unavailable');}
        var pin=await session.capturePresentationSession()(refused),latest=ports.read();
        if(!pin||session!==root.NemoNativeObjectProject.desktop(root)||pin.documentId!==meta.ownerDocumentId||
            pin.lifecycleGeneration!==meta.lifecycleGeneration){
          if(pin)refused('owner-mismatch');throw new Error('Native tab UI owner is unavailable');}
        function sameUI(latest){return latest.activeTabId===captured.activeTabId&&latest.path===captured.path&&latest.name===captured.name&&
            latest.tabs.find(function(t){return t.id===captured.targetTabId;})===incoming&&incoming.nativeInactiveProject===meta&&
            latest.tabs.some(function(t){return t.id===captured.activeTabId;})&&ports.dirty()===captured.dirty&&
            sameView(view(root),captured.view);}
        if(!sameUI(latest)){
          refused('ui-changed');throw new Error('Native tab view changed while waiting');}
        // The caller awaits capture. Recheck synchronously at actual admission,
        // before File/Project set their opening guards; no stale queued intent.
        return Object.freeze(Object.assign({},captured,{admit:function(){
          var current=session.readPresentedObjectSession(refused);
          if(!current||session!==root.NemoNativeObjectProject.desktop(root)||
              !['instanceId','documentId','contentRevision','documentSnapshotId','lifecycleGeneration'].every(function(k){return current[k]===pin[k];})){
            if(current)refused('owner-mismatch');return false;
          }
          if(!sameUI(ports.read())){refused('ui-changed');return false;}return true;
        }}));
      },
      isCurrent:function(capture,committed,projecting){
        var current=ports.read();
        if(current.activeTabId!==(committed?capture.targetTabId:capture.activeTabId))return false;
        if(!committed&&(current.path!==capture.path||current.name!==capture.name))return false;
        return sameView(view(root),projecting?capture.targetView:capture.view);
      },
      canActivate:function(){var file=ports.file();return !!file&&typeof file.activateProject==='function';},
      activate:function(meta,transition,capture){var file=ports.file();return file&&typeof file.activateProject==='function'&&capture.admit()===true?
        file.activateProject(meta,transition):{notDispatched:true};},
      publish:function(incoming,outgoing,capture){
        var current=ports.read(),from=current.tabs.find(function(t){return t.id===capture.activeTabId;});
        var to=current.tabs.find(function(t){return t.id===capture.targetTabId&&t.nativeInactiveProject===incoming;});
        if(!from||!to||from===to||current.activeTabId!==from.id||current.path!==capture.path||current.name!==capture.name)
          return false;
        from.nativeInactiveProject=outgoing;from.path=capture.path;from.name=capture.name;from.dirty=capture.dirty;from.json=null;
        delete to.nativeInactiveProject;to.json=null;
        ports.activateTab(to.id,to.path||null,to.name);ports.render();return true;
      }
    });
  }
  function render(root,ports){
    var add=root.document.getElementById('project-tab-add');
    if(add&&ports.native()){add.setAttribute('data-i18n-title','newProjectHdr');add.title=root.SM.t('newProjectHdr');}
    root.SMProjectEntry.renderTabs(root.document,{
      readRows:function(){var current=ports.read();return current.tabs.map(function(t){
        return {id:t.id,name:t.name,active:t.id===current.activeTabId,dirty:ports.dirty(t)};
      });},switchTab:ports.switchTab,closeTab:ports.closeTab,renameTab:ports.renameTab
    });
  }
  function rename(root,id,ports){
    var el=root.document.querySelector('.project-tab[data-tab="'+id+'"] .pt-name');if(!el)return;
    var t=ports.read().tabs.find(function(t){return t.id===id;});if(!t)return;
    var input=root.document.createElement('input');input.className='pt-rename-input';input.value=t.name;
    el.replaceWith(input);input.focus();input.select();
    function commit(){
      var value=input.value.trim()||t.name;t.name=value;
      if(id===ports.read().activeTabId)ports.renameActive(value);
      ports.render();
    }
    input.addEventListener('blur',commit);
    input.addEventListener('keydown',function(e){e.stopPropagation();if(e.key==='Enter')input.blur();
      else if(e.key==='Escape'){input.value=t.name;input.blur();}});
  }
  function refreshDirty(root,activeTabId,dirty){
    var el=root.document.querySelector('.project-tab[data-tab="'+activeTabId+'"]');
    if(el)el.classList.toggle('dirty',dirty());
  }
  // The tab already holding this file, if any: the active tab by the live path, any other by the path stored on its row.
  // Opening a file that is already open focuses that tab instead of preparing a second copy of it.
  function findOpen(current,path){
    if(typeof path!=='string'||!path)return null;
    if(current.path===path)return {tab:current.tabs.find(function(t){return t.id===current.activeTabId;})||null,active:true};
    var other=current.tabs.find(function(t){return t.id!==current.activeTabId&&t.path===path;});
    return other?{tab:other,active:false}:null;
  }
  return Object.freeze({bind:bind,render:render,rename:rename,refreshDirty:refreshDirty,findOpen:findOpen});
}));
