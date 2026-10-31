/* Ordinary file UI consumes native pins/digests; never project/media bytes. */
(function(root,factory){
  var api=factory(typeof module==='object'&&module.exports?require('./native-object-layer-splice.js'):root.NemoNativeObjectLayerSplice);
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProjectFile=api;
}(typeof globalThis==='object'?globalThis:this,function(Splice){
  'use strict';
  var projections=new WeakMap();
  var settingsKeys=['canvasW','canvasH','fps','totalFrames','canvasBg','waIn','waOut'];
  function has(value,key){return Object.prototype.hasOwnProperty.call(value,key);}
  // Parent links are opaque native UIDs. Missing and null both project as no parent.
  function parentValue(layer){return has(layer,'parentLayerUid')?layer.parentLayerUid:null;}
  function validParent(layer){return !has(layer,'parentLayerUid')||layer.parentLayerUid===null||
    typeof layer.parentLayerUid==='string'&&layer.parentLayerUid.length>0;}
  function validateSettings(info){
    if(!info||!['canvasW','canvasH','totalFrames'].every(function(k){return Number.isSafeInteger(info[k])&&info[k]>0;})||
      !Number.isFinite(info.fps)||info.fps<=0||has(info,'canvasBg')&&(typeof info.canvasBg!=='string'||!/^#?(?:[a-fA-F0-9]{3}|[a-fA-F0-9]{6}|[a-fA-F0-9]{8})$/.test(info.canvasBg))||
      ['waIn','waOut'].some(function(k){return has(info,k)&&!Number.isFinite(info[k]);}))throw new Error('Native project settings are invalid');
  }
  function settingsMatch(a,b){return settingsKeys.every(function(k){return has(a,k)===has(b,k)&&a[k]===b[k];});}
  function pinSettings(pin,info){settingsKeys.forEach(function(k){if(has(info,k))pin[k]=info[k];else delete pin[k];});return pin;}
  function projectSettings(root,info,previous){
    var s=root.state,defaults={canvasBg:'#ffffff',waIn:0,waOut:info.totalFrames-1};
    s.canvasW=info.canvasW;s.canvasH=info.canvasH;s.fps=info.fps;s.totalFrames=info.totalFrames;
    ['canvasBg','waIn','waOut'].forEach(function(k){
      if(has(info,k))s[k]=info[k];else if(!previous||has(previous,k))s[k]=defaults[k];
    });
    root._totalF=s.totalFrames;root._waIn=s.waIn;root._waOut=s.waOut;
  }
  // #2256: while a component is presented the rows on screen are ITS rows, so a revision is judged against the entered context
  // (same owner, same revision, layers re-read) and not against the scene projection, which is held aside until the scene returns.
  function enteredView(root){var c=root.NemoNativeComponentContext;return !!c&&typeof c.presented==='function'&&!!c.presented(root);}
  function enteredMatches(root,info){
    var pin=projections.get(root);
    return !!pin&&!!info&&['instanceId','documentId','lifecycleGeneration'].every(function(k){return pin[k]===info[k];})&&root.NemoNativeComponentContext.projectionReady(root,info)===true;
  }
  function projectionMatches(root,info){
    if(enteredView(root))return enteredMatches(root,info);
    var pin=projections.get(root),rows=root.state&&root.state.layers;
    return !!pin&&!!info&&['instanceId','documentId','lifecycleGeneration'].every(function(k){return pin[k]===info[k];})&&
      Array.isArray(rows)&&Array.isArray(info.layers)&&rows.length===info.layers.length&&
      rows.every(function(row,i){var incoming=info.layers[i];return incoming&&validParent(incoming)&&
        row.layerUid===incoming.layerUid&&parentValue(row)===pin.layerParents[i]&&
        (info.contentRevision!==pin.contentRevision||parentValue(incoming)===pin.layerParents[i]);});
  }
  function refreshVisibility(root,info){
    if(!projectionMatches(root,info))return false;
    return refreshLayerTable(root,info);
  }
  function projectionReadyForUI(root,info){
    var pin=projections.get(root),next;
    if(enteredView(root))return enteredMatches(root,info);
    try{validateSettings(info);next=layerTable(info&&info.layers);}catch(_){return false;}
    return projectionMatches(root,info)&&['contentRevision','documentSnapshotId'].every(function(k){return pin[k]===info[k];})&&settingsMatch(pin,info)&&
      ['canvasW','canvasH','fps','totalFrames'].every(function(k){return root.state[k]===info[k];})&&
      ['canvasBg','waIn','waOut'].every(function(k){return !has(info,k)||root.state[k]===info[k];})&&
      next.every(function(layer,i){var row=root.state.layers[i];return row.visible===layer.visible&&
        (layer.name===undefined||row.name===layer.name)&&['locked','shy','color','parentLayerUid'].every(function(k){
          return k==='parentLayerUid'?parentValue(row)===parentValue(layer):
            ['locked','shy'].includes(k)?row[k]===(layer[k]===true):!Object.prototype.hasOwnProperty.call(layer,k)||row[k]===layer[k];
        });});
  }
  // #2371: read-model members the layer table admits (validated by native-object-project-metadata.js; the presenter reads info.layers).
  var MODEL_ROW_KEYS=['kind','folderCollapsed','folderId','componentId','blendMode','matteMode','matteSourceLayerUid','timeLinkUid','followPathUid','mattesMore','isMatteSource','opacity','opacityAnimated','effects','effectsOversize','pathFx','pathFxOversize'];
  function layerTable(layers){
    if(!Array.isArray(layers))throw new Error('Native layer table is invalid');
    var seen=new Set(),next=[];
    for(var i=0;i<layers.length;i++){
      var layer=layers[i];
      if(!layer||typeof layer.layerUid!=='string'||!layer.layerUid||seen.has(layer.layerUid))
        throw new Error('Native layer identity is invalid');
      if(Object.keys(layer).some(function(k){return !['layerUid','name','visible','locked','shy','color','parentLayerUid','widget'].concat(MODEL_ROW_KEYS).includes(k);}))
        throw new Error('Native layer metadata is unsupported');
      if(Object.prototype.hasOwnProperty.call(layer,'name')&&typeof layer.name!=='string')throw new Error('Native layer name is invalid');
      if(Object.prototype.hasOwnProperty.call(layer,'visible')&&typeof layer.visible!=='boolean')throw new Error('Native layer visibility is invalid');
      if(['locked','shy'].some(function(k){return Object.prototype.hasOwnProperty.call(layer,k)&&typeof layer[k]!=='boolean';}))throw new Error('Native layer policy flags are invalid');
      if(Object.prototype.hasOwnProperty.call(layer,'widget')&&layer.widget!==true)throw new Error('Native layer widget flag is invalid');
      if(Object.prototype.hasOwnProperty.call(layer,'color')&&(typeof layer.color!=='string'||!/^#[a-fA-F0-9]{6}$/.test(layer.color)))throw new Error('Native layer color is invalid');
      if(!validParent(layer))throw new Error('Native layer parent is invalid');
      var row={layerUid:layer.layerUid,name:Object.prototype.hasOwnProperty.call(layer,'name')?layer.name:undefined,visible:layer.visible!==false};
      ['locked','shy','color','parentLayerUid','widget'].forEach(function(k){if(Object.prototype.hasOwnProperty.call(layer,k))row[k]=layer[k];});
      seen.add(layer.layerUid);next.push(row);
    }
    return next;
  }
  function projectFields(row,layer){
    // #2255: a widget layer carries the legacy kind flag the layer menus read ("Widget settings…"); the facts stay native reads.
    var widget=layer.widget===true;delete row.widget;if(widget)row.isWidgetLayer=true;else delete row.isWidgetLayer;
    ['locked','shy','color','parentLayerUid'].forEach(function(k){
      if(Object.prototype.hasOwnProperty.call(layer,k))row[k]=layer[k];else if(['locked','shy'].includes(k))row[k]=false;
      else if(k==='parentLayerUid'&&has(row,k))row[k]=null;
    });
  }
  function refreshLayerTable(root,info){
    var pin=projections.get(root),s=root.state,rows=s&&s.layers;
    if(!pin||!info||!['instanceId','documentId','lifecycleGeneration'].every(function(k){return pin[k]===info[k];})||
      !s||s.totalFrames!==pin.totalFrames||
      !Array.isArray(rows)||rows.length!==pin.layerUids.length||!pin.layerUids.every(function(uid,i){return rows[i]&&rows[i].layerUid===uid&&
        parentValue(rows[i])===pin.layerParents[i];}))return false;
    validateSettings(info);
    if(!Number.isSafeInteger(pin.contentRevision)||!Number.isSafeInteger(info.contentRevision)||info.contentRevision<pin.contentRevision||
      typeof info.documentSnapshotId!=='string'||!info.documentSnapshotId||
      info.contentRevision===pin.contentRevision&&(info.documentSnapshotId!==pin.documentSnapshotId||!settingsMatch(pin,info)))return false;
    var next=layerTable(info.layers);
    if(info.contentRevision===pin.contentRevision&&next.some(function(l,i){return rows[i]&&
      (parentValue(l)!==pin.layerParents[i]||['locked','shy'].some(function(k){return rows[i][k]!== (l[k]===true);}));}))return false;
    if(!Splice||typeof Splice.plan!=='function')return false;
    var splice=Splice.plan(pin.layerUids,rows,next,s.activeLayerIdx,root._layerSel,root._layerSelAnchor);
    var changed=!!splice&&(splice.removed>0||splice.addedRows.length>0);
    if(!splice||changed&&info.contentRevision===pin.contentRevision)return false;
    // Prepare the detached UID plan before changing this read-side table.
    if(changed)rows.splice.apply(rows,[splice.at,splice.removed].concat(splice.addedRows));
    next.forEach(function(layer,i){if(layer.name!==undefined)rows[i].name=layer.name;rows[i].visible=layer.visible;projectFields(rows[i],layer);});
    if(changed){s.activeLayerIdx=splice.active;root._layerSel=splice.selection;root._layerSelAnchor=splice.anchor;}
    projectSettings(root,info,pin);
    // Duration edits and their Undo can shorten the timeline. Keep the UI playhead valid
    // before the native session requests the next presentation; drawing bytes stay native.
    if(info.totalFrames!==pin.totalFrames){
      s.currentFrame=Math.max(0,Math.min(info.totalFrames-1,s.currentFrame));root._curFrame=s.currentFrame;
    }
    projections.set(root,pinSettings(Object.assign({},pin,{layerUids:next.map(function(layer){return layer.layerUid;}),
      contentRevision:info.contentRevision,documentSnapshotId:info.documentSnapshotId,layerParents:next.map(parentValue)}),info));
    return true;
  }
  function create(session,ports,workflow,acknowledgement){
    var baseline=null,saving=false,opening=false,baselines=new Map(),initials=new Map(),recoveredSlots=new Map();
    var saved=acknowledgement.create({
      readUISession:ports.readUI,readPresentedObjectSession:session.readPresentedObjectSession,
      commitSavedMetadata:function(plan){
        if(!plan.documentDigest||!session.info()||session.info().documentDigest!==plan.documentDigest)return false;
        if(ports.commit(plan.destinationPath,session.info())!==true)return false;
        baseline=plan.documentDigest;baselines.set(session.info().documentId,baseline);ports.repaint();return true;
      },
      markSavedIndeterminate:function(){baseline=null;ports.repaint();}
    });
    function diagnostic(event,code){try{if(ports.openDiagnostic)ports.openDiagnostic(event,code);}catch(_){}}
    // Crash-recovery cleanup (#2360 R4): the project's slots are dropped once saved or replaced. Fire and forget, so it never
    // fails or delays Save/Open/New; the slots are claimed synchronously inside the port call.
    function discardRecovery(documentId){
      var extra=recoveredSlots.get(documentId);recoveredSlots.delete(documentId);
      try{if(ports.discardRecovery)Promise.resolve(ports.discardRecovery(documentId,extra?[extra]:[])).catch(function(){});}catch(_){}
    }
    async function openPath(path){return enter(path,null);}
    async function newProject(config,isCurrent,onCreationFailure){
      if(!config||typeof config!=='object')return false;
      var frames=config.totalFrames,framed=Object.prototype.hasOwnProperty.call(config,'totalFrames');
      config={w:config.w,h:config.h,fps:config.fps,name:typeof config.name==='string'?config.name:'Untitled'};
      if(framed)config.totalFrames=frames;
      return enter(null,config,isCurrent,onCreationFailure);
    }
    // recovered (#2360 R3): {slotKey, confirmed?, refused?}. The project is Untitled and dirty, never a recents entry, and a
    // not-dispatched refusal is recorded for openRecovered() instead of toasting, so it can try the older generation first.
    async function enter(path,config,isCurrent,onCreationFailure,recovered){
      if(saving||opening)return false;
      opening=true;
      diagnostic(config?'file-new-started':'file-open-started');
      try{
        var creating=!!config,revealing=false,replaced=null;
        if(creating&&typeof session.newProject!=='function')throw {message:'Native New Project is unavailable',details:{disposition:'notDispatched'}};
        var value=creating?{canvasW:config.w,canvasH:config.h,fps:config.fps}:path;
        if(creating&&Object.prototype.hasOwnProperty.call(config,'totalFrames'))value.totalFrames=config.totalFrames;
        var info=await session[creating?'newProject':'openPath'](value,{
          // Reveal hides this form intentionally, but never retires its request
          // generation. Other callers retain their complete currency predicate.
          isCurrent:function(){var current=revealing&&isCurrent&&isCurrent.requestCurrent||isCurrent;
            return !current||current()===true;},
          confirm:async function(previous){
            replaced=previous||null;
            if(!previous||baseline===previous.documentDigest||recovered&&recovered.confirmed)return true;
            var yes=!!(ports.confirmReplace&&await ports.confirmReplace(creating));
            if(yes&&recovered)recovered.confirmed=true;return yes;},
          occlude:ports.occlude,reveal:ports.reveal?async function(current){revealing=true;await ports.reveal(current);}:undefined,restore:ports.restore,
          commit:function(current){
            if((recovered?ports.created('Untitled',current):creating?ports.created(config.name||'Untitled',current):ports.opened(path,current))!==true)
              throw new Error('Native project UI publication failed');
            baseline=creating||recovered?null:current.documentDigest;baselines=new Map([[current.documentId,baseline]]);initials.set(current.documentId,current.documentDigest);
            if(recovered)recoveredSlots.set(current.documentId,recovered.slotKey);ports.repaint();
          }
        });
        // No project and no error: the session found the file unsupported (corrupt or foreign). For a recovery that is a refusal.
        if(recovered&&info===null){diagnostic('file-failed','not-dispatched');recovered.refused={message:'Native recovery file is unsupported',code:'unsupported_format'};return false;}
        if(info&&replaced)discardRecovery(replaced.documentId);
        if(creating&&info&&ports.createdNotice)ports.createdNotice();
        if(recovered&&info&&ports.resumedNotice)ports.resumedNotice();
        diagnostic(info?'file-completed':'file-cancelled');
        return info===null?null:!!info;
      }catch(error){
        try{diagnostic('file-failed',error&&error.details&&error.details.disposition==='notDispatched'?'not-dispatched':'unconfirmed');}catch(_){}
        if(error&&error.details&&error.details.disposition==='notDispatched'){if(recovered)recovered.refused=error;else ports.refused(error);}
        else{baseline=null;ports.failed(error);ports.repaint();if(onCreationFailure)onCreationFailure(error);}
        return false;
      }finally{opening=false;}
    }
    // Resume a crash-recovery slot (#2360 R3) through the same session.openPath admission as Open. Newest generation first; a
    // not-dispatched refusal (unreadable or strict-codec refusal; Open/New stay usable) retries the older one before any toast.
    async function openRecovered(entry){
      var generations=entry&&typeof entry.slotKey==='string'&&Array.isArray(entry.generations)?entry.generations:[];
      var failure=null,recovered={slotKey:entry&&entry.slotKey,confirmed:false,refused:null};
      for(var i=0;i<generations.length;i++){
        recovered.refused=null;
        var result=await enter(generations[i].path,null,undefined,undefined,recovered);
        if(result!==false||!recovered.refused)return result;
        failure=recovered.refused;
      }
      if(failure&&ports.recoveryRefused)ports.recoveryRefused(failure);
      return false;
    }
    async function activateProject(target,transition){
      if(saving||opening||!session.isActive()||typeof session.activateProject!=='function')return {notDispatched:true};
      opening=true;
      var previous=session.info(),previousBaseline=baseline;
      baselines.set(previous.documentId,baseline);
      try{
        var current=await session.activateProject(target,{
          isCurrent:transition.isCurrent,confirm:async function(){return true;},
          occlude:ports.occlude,restore:ports.restore,
          reveal:function(info,view){if(transition.projecting)transition.projecting();return ports.reveal(info,view);},
          commit:async function(info,receipt){
            baseline=baselines.has(info.documentId)?baselines.get(info.documentId):target.digest;
            baselines=new Map([[previous.documentId,previousBaseline],[info.documentId,baseline]]);
            if(await transition.commit(info,receipt)!==true)throw new Error('Native tab publication failed');
            ports.repaint();
          }
        });
        return !!current;
      }catch(error){
        if(error&&error.details&&error.details.disposition==='notDispatched'){ports.refused(error);return {notDispatched:true};}
        baseline=null;ports.failed(error);ports.repaint();return false;
      }finally{opening=false;}
    }
    // destination: an application request's path (#2510) in place of the picker; the same workflow otherwise.
    async function saveAs(destination){
      if(saving||opening)return {status:saving?'save-in-progress':'open-in-progress',acknowledged:false};
      var pick=typeof destination==='string'&&destination?function(){return Promise.resolve(destination);}:ports.pick;
      saving=true;
      try{
        var options={requestId:session.requestId('save'),pinRequestId:session.requestId('save-pin'),project:true};
        var ticket=saved.begin({saveRequestId:options.requestId,serializerRequestId:options.pinRequestId}),serialization;
        options.isCurrent=function(){return saved.isCurrent(ticket);};
        var written=await workflow.run({readPresentedObjectSession:session.captureSaveSession(),
          acquirePin:async function(request){serialization=await session.acquirePin(request);return serialization;},
          pickDestination:pick,savePinned:session.savePinned},options);
        var result=await saved.acknowledge(ticket,{written:written,serialization:serialization},options);
        ports.notice(result,written);
        if(result.acknowledged&&session.info())discardRecovery(session.info().documentId);
        return written&&written.reason&&!result.acknowledged?Object.assign({},result,{reason:written.reason}):result;
      }catch(error){ports.notice({status:'refused'},null,error);return {status:'refused',reason:error&&error.message||'save-failed'};}
      finally{saving=false;}
    }
    // Cached facts for the crash-recovery scheduler (#2360 R2): no IPC, no serialization.
    function recoveryPin(){
      var info=session.info();
      return {active:session.isActive(),info:info,busy:saving||opening,presented:!!session.readPresentedObjectSession(),
        reference:baseline||info&&initials.get(info.documentId)||null,
        // The slot a resumed session replaces: sent as `supersedes` with its first own write (#2360 R3).
        supersedes:info&&recoveredSlots.get(info.documentId)||null};
    }
    return Object.freeze({openPath:openPath,openRecovered:openRecovered,newProject:newProject,activateProject:activateProject,saveAs:saveAs,recoveryPin:recoveryPin,
      canOpen:function(){return !saving&&!opening&&(!session.blocksLegacy()||session.isActive());},
      isDirty:function(){var info=session.info();return !session.isActive()&&session.blocksLegacy()||!baseline||!info||info.documentDigest!==baseline;}});
  }
  function projectView(root,info,view){
    // These rows are read-side UI projections. The native edit guard denies
    // all legacy writes, and no Paper object or serializable document is built.
    var layers=layerTable(info.layers);validateSettings(info);
    var frame=view?view.frame:0,selected=view?view.layerUids:layers.length?[layers[0].layerUid]:[];
    if(!Number.isSafeInteger(frame)||frame<0||frame>=info.totalFrames||!Array.isArray(selected)||
        new Set(selected).size!==selected.length||selected.some(function(uid){return !layers.some(function(l){return l.layerUid===uid;});})||
        view&&[view.activeLayerUid,view.anchorLayerUid].some(function(uid){return uid!==null&&!layers.some(function(l){return l.layerUid===uid;});}))
      throw new Error('Native retained view context is unavailable');
    var indices=selected.map(function(uid){return layers.findIndex(function(l){return l.layerUid===uid;});});
    var s=root.state;
    s.canvasW=info.canvasW;s.canvasH=info.canvasH;s.fps=info.fps;s.totalFrames=info.totalFrames;
    projectSettings(root,info,null);s.currentFrame=frame;s.activeLayerIdx=view&&view.activeLayerUid!==null?layers.findIndex(function(l){return l.layerUid===view.activeLayerUid;}):indices.length?indices[0]:0;
    if(view)root._layerSelAnchor=view.anchorLayerUid===null?-1:layers.findIndex(function(l){return l.layerUid===view.anchorLayerUid;});
    s.layers=layers.map(function(l,i){var row={layerUid:l.layerUid,
      name:l.name===undefined?'Layer '+(i+1):l.name,visible:l.visible,locked:false,color:'#5FA875',parentLayerUid:null,frames:[],motion:{}};
      projectFields(row,l);return row;});
    s.symbols={};s.cameraKeys=[];s.activeSymbolId=null;s.refMedia=null;
    root._curFrame=frame;root._totalF=info.totalFrames;root._waIn=s.waIn;root._waOut=s.waOut;
    root._layerSel=indices;root.selectedPaths=[];
    if(root.userLayers)root.userLayers.forEach(function(layer){layer.visible=false;});
    if(root.SMReference)root.SMReference.refreshNative();
    var pin={layerUids:info.layers.map(function(layer){return layer.layerUid;}),layerParents:layers.map(parentValue),
      contentRevision:info.contentRevision,documentSnapshotId:info.documentSnapshotId};
    ['instanceId','documentId','lifecycleGeneration','canvasW','canvasH','fps','totalFrames'].forEach(function(k){pin[k]=info[k];});
    projections.set(root,pinSettings(pin,info));
  }
  function bind(root,ports){
    var session=root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root);
    if(!session)return null;
    function diagnostic(event,code){try{var d=root.NemoNativeObjectProjectDesktop;if(d&&d.openDiagnostic)d.openDiagnostic(root,event,code);}catch(_){}}
    var startWasHidden=false;
    function commit(path,info){
      var name=root.SMProjectDocument.baseName(path);
      // Persist recents before changing the visible file identity.
      ports.touchRecent(path,name,info,true);ports.commit(path,name);ports.renderRecents();return true;
    }
    var ui=create(session,{
      openDiagnostic:diagnostic,
      readUI:function(){return {handle:ports.read().tabId||'native-project-opening',generation:1};},
      confirmReplace:function(creating){return root.__TAURI__.dialog.ask('Unsaved changes will be lost. '+(creating?'Create a new project?':'Open another project?'),
        {title:'Unsaved changes',kind:'warning',okLabel:creating?'Discard and create':'Discard and open',cancelLabel:'Cancel'});},
      occlude:function(){startWasHidden=root.document.getElementById('start-screen').classList.contains('hid');ports.show();},
      restore:function(previous){if(previous&&startWasHidden)ports.hide();},
      reveal:async function(info,view){
        diagnostic('file-stage','projection');projectView(root,info,view);
        diagnostic('file-stage','ui-refresh');if(root.updateUI)root.updateUI();
        diagnostic('file-stage','hide-start');ports.hide();
        diagnostic('file-stage','layout-wait');
        await new Promise(function(resolve,reject){var done=false,timer=setTimeout(function(){
            if(done)return;done=true;reject(new Error('Native project layout did not settle'));
          },1500);
          function finish(){if(done)return;done=true;clearTimeout(timer);resolve();}
          root.requestAnimationFrame(function(){root.requestAnimationFrame(finish);});});
        diagnostic('file-stage','layout-settled');
      },
      opened:function(path,info){if(commit(path,info)!==true)return false;ports.opened();ports.notice('Opened: '+ports.read().name);return true;},
      created:function(name){ports.commit(null,name);ports.opened();return true;},
      createdNotice:function(){ports.notice('New project created');},
      resumedNotice:function(){ports.notice('Session resumed');},
      recoveryRefused:function(){ports.notice(root.SM&&root.SM.t?root.SM.t('toastCannotResumeSessionCorrupt'):'Cannot resume the session: the recovery is unreadable.');},
      commit:commit,repaint:ports.repaint,discardRecovery:ports.recoveryDiscard,
      pick:function(){return root.__TAURI__.dialog.save({title:'Save Project As',defaultPath:ports.read().name+'-copy.json',filters:[{name:'Nemo Project',extensions:['json']}]});},
      notice:function(result,written,error){
        ports.notice(result.acknowledged?'Saved: '+ports.read().name:error?'Save failed: '+(error.message||error):
          written&&written.receipt&&written.receipt.published?'File written; current saved status was not confirmed.':'Save cancelled or unavailable. Choose a new file path.');
      },
      refused:function(error){ports.notice('Could not open native project: '+(error.message||error));},
      failed:function(error){ports.show();ports.notice('Could not open native project: '+(error.message||error));}
    },root.NemoNativeObjectSaveAsWorkflow,root.NemoNativeObjectSavedAcknowledgement);
    return Object.freeze({isActive:session.blocksLegacy,recoveryPin:ui.recoveryPin,canOpen:ui.canOpen,isDirty:ui.isDirty,saveAs:ui.saveAs,openPath:ui.openPath,openRecovered:ui.openRecovered,newProject:ui.newProject,activateProject:ui.activateProject});
  }
  return Object.freeze({create:create,projectView:projectView,bind:bind,
    projectionReadyForUI:projectionReadyForUI,
    projectionMatches:projectionMatches,refreshVisibility:refreshVisibility,refreshLayerTable:refreshLayerTable,
    hasProjection:function(root){return projections.has(root);}});
}));
