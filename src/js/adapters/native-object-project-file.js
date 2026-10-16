/* Ordinary file UI consumes native pins/digests; never project/media bytes. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProjectFile=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var projections=new WeakMap();
  var settingsKeys=['canvasW','canvasH','fps','totalFrames','canvasBg','waIn','waOut'];
  function has(value,key){return Object.prototype.hasOwnProperty.call(value,key);}
  function validateSettings(info){
    if(!info||!['canvasW','canvasH','totalFrames'].every(function(k){return Number.isSafeInteger(info[k])&&info[k]>0;})||
      !Number.isFinite(info.fps)||info.fps<=0||has(info,'canvasBg')&&(typeof info.canvasBg!=='string'||!/^#?(?:[a-fA-F0-9]{3}|[a-fA-F0-9]{6}|[a-fA-F0-9]{8})$/.test(info.canvasBg))||
      ['waIn','waOut'].some(function(k){return has(info,k)&&!Number.isFinite(info[k]);}))throw new Error('Native project settings are invalid');
  }
  function settingsMatch(a,b){return settingsKeys.every(function(k){return has(a,k)===has(b,k)&&a[k]===b[k];});}
  function pinSettings(pin,info){settingsKeys.forEach(function(k){if(has(info,k))pin[k]=info[k];else delete pin[k];});return pin;}
  function projectSettings(root,info,previous){
    var s=root.state,defaults={canvasBg:'#ffffff',waIn:0,waOut:info.totalFrames-1};
    s.canvasW=info.canvasW;s.canvasH=info.canvasH;s.fps=info.fps;
    ['canvasBg','waIn','waOut'].forEach(function(k){
      if(has(info,k))s[k]=info[k];else if(!previous||has(previous,k))s[k]=defaults[k];
    });
    root._waIn=s.waIn;root._waOut=s.waOut;
  }
  function projectionMatches(root,info){
    var pin=projections.get(root),rows=root.state&&root.state.layers;
    return !!pin&&!!info&&['instanceId','documentId','lifecycleGeneration'].every(function(k){return pin[k]===info[k];})&&
      Array.isArray(rows)&&Array.isArray(info.layers)&&rows.length===info.layers.length&&
      rows.every(function(row,i){return row.layerUid===info.layers[i].layerUid;});
  }
  function refreshVisibility(root,info){
    if(!projectionMatches(root,info))return false;
    var values=layerTable(info.layers);
    root.state.layers.forEach(function(row,i){row.visible=values[i].visible;projectFields(row,values[i]);});
    return true;
  }
  function projectionReadyForUI(root,info){
    var pin=projections.get(root),next;
    try{validateSettings(info);next=layerTable(info&&info.layers);}catch(_){return false;}
    return projectionMatches(root,info)&&['contentRevision','documentSnapshotId'].every(function(k){return pin[k]===info[k];})&&settingsMatch(pin,info)&&
      ['canvasW','canvasH','fps','totalFrames'].every(function(k){return root.state[k]===info[k];})&&
      ['canvasBg','waIn','waOut'].every(function(k){return !has(info,k)||root.state[k]===info[k];})&&
      next.every(function(layer,i){var row=root.state.layers[i];return row.visible===layer.visible&&
        (layer.name===undefined||row.name===layer.name)&&['locked','color','parentLayerUid'].every(function(k){
          return !Object.prototype.hasOwnProperty.call(layer,k)||row[k]===layer[k];
        });});
  }
  function layerTable(layers){
    if(!Array.isArray(layers))throw new Error('Native layer table is invalid');
    var seen=new Set(),next=[];
    for(var i=0;i<layers.length;i++){
      var layer=layers[i];
      if(!layer||typeof layer.layerUid!=='string'||!layer.layerUid||seen.has(layer.layerUid))
        throw new Error('Native layer identity is invalid');
      if(Object.keys(layer).some(function(k){return !['layerUid','name','visible','locked','color','parentLayerUid'].includes(k);}))
        throw new Error('Native layer metadata is unsupported');
      if(Object.prototype.hasOwnProperty.call(layer,'name')&&typeof layer.name!=='string')throw new Error('Native layer name is invalid');
      if(Object.prototype.hasOwnProperty.call(layer,'visible')&&typeof layer.visible!=='boolean')throw new Error('Native layer visibility is invalid');
      if(Object.prototype.hasOwnProperty.call(layer,'locked')&&layer.locked!==false)throw new Error('Native layer lock is unsupported');
      if(Object.prototype.hasOwnProperty.call(layer,'color')&&(typeof layer.color!=='string'||!/^#[a-fA-F0-9]{6}$/.test(layer.color)))throw new Error('Native layer color is invalid');
      if(Object.prototype.hasOwnProperty.call(layer,'parentLayerUid')&&layer.parentLayerUid!==null)throw new Error('Native layer parent is unsupported');
      var row={layerUid:layer.layerUid,name:Object.prototype.hasOwnProperty.call(layer,'name')?layer.name:undefined,visible:layer.visible!==false};
      ['locked','color','parentLayerUid'].forEach(function(k){if(Object.prototype.hasOwnProperty.call(layer,k))row[k]=layer[k];});
      seen.add(layer.layerUid);next.push(row);
    }
    return next;
  }
  function projectFields(row,layer){
    ['locked','color','parentLayerUid'].forEach(function(k){
      if(Object.prototype.hasOwnProperty.call(layer,k))row[k]=layer[k];
    });
  }
  function refreshLayerTable(root,info){
    var pin=projections.get(root),s=root.state,rows=s&&s.layers;
    if(!pin||!info||!['instanceId','documentId','lifecycleGeneration','totalFrames'].every(function(k){return pin[k]===info[k];})||
      !Array.isArray(rows)||rows.length!==pin.layerUids.length||!pin.layerUids.every(function(uid,i){return rows[i]&&rows[i].layerUid===uid;}))return false;
    validateSettings(info);
    if(!Number.isSafeInteger(pin.contentRevision)||!Number.isSafeInteger(info.contentRevision)||info.contentRevision<pin.contentRevision||
      typeof info.documentSnapshotId!=='string'||!info.documentSnapshotId||
      info.contentRevision===pin.contentRevision&&(info.documentSnapshotId!==pin.documentSnapshotId||!settingsMatch(pin,info)))return false;
    var next=layerTable(info.layers),delta=next.length-rows.length;
    if(Math.abs(delta)>1||delta!==0&&info.contentRevision===pin.contentRevision||
      !next.slice(0,Math.min(next.length,rows.length)).every(function(row,i){return row.layerUid===pin.layerUids[i];}))return false;
    // Prepare all metadata and selection before changing this read-side table.
    var active=s.activeLayerIdx,selection=root._layerSel;
    if(delta<0){
      var previousSelection=selection;
      selection=Array.isArray(selection)?selection.filter(function(i){return Number.isInteger(i)&&i>=0&&i<next.length;}):[];
      if(!Number.isInteger(active)||active<0||active>=next.length)active=selection.length?selection[0]:Math.max(0,next.length-1);
      if(!selection.length&&next.length&&previousSelection&&previousSelection.length)selection=[active];
      else if(previousSelection&&selection.length===previousSelection.length)selection=previousSelection;
    }
    if(delta>0){var added=next[next.length-1];rows.push({layerUid:added.layerUid,
      name:added.name===undefined?'Layer '+next.length:added.name,visible:added.visible});}
    else if(delta<0)rows.pop();
    next.forEach(function(layer,i){if(layer.name!==undefined)rows[i].name=layer.name;rows[i].visible=layer.visible;projectFields(rows[i],layer);});
    if(delta<0){s.activeLayerIdx=active;root._layerSel=selection;}
    projectSettings(root,info,pin);
    projections.set(root,pinSettings(Object.assign({},pin,{layerUids:next.map(function(layer){return layer.layerUid;}),
      contentRevision:info.contentRevision,documentSnapshotId:info.documentSnapshotId}),info));
    return true;
  }
  function create(session,ports,workflow,acknowledgement){
    var baseline=null,saving=false,opening=false;
    var saved=acknowledgement.create({
      readUISession:ports.readUI,readPresentedObjectSession:session.readPresentedObjectSession,
      commitSavedMetadata:function(plan){
        if(!plan.documentDigest||!session.info()||session.info().documentDigest!==plan.documentDigest)return false;
        if(ports.commit(plan.destinationPath,session.info())!==true)return false;
        baseline=plan.documentDigest;ports.repaint();return true;
      },
      markSavedIndeterminate:function(){baseline=null;ports.repaint();}
    });
    async function openPath(path){return enter(path,null);}
    async function newProject(config,isCurrent,onCreationFailure){
      if(!config||typeof config!=='object')return false;
      config={w:config.w,h:config.h,fps:config.fps,name:typeof config.name==='string'?config.name:'Untitled'};
      return enter(null,config,isCurrent,onCreationFailure);
    }
    async function enter(path,config,isCurrent,onCreationFailure){
      if(saving||opening)return false;
      opening=true;
      try{
        var creating=!!config,revealing=false;
        if(creating&&typeof session.newProject!=='function')throw {message:'Native New Project is unavailable',details:{disposition:'notDispatched'}};
        var value=creating?{canvasW:config.w,canvasH:config.h,fps:config.fps}:path;
        var info=await session[creating?'newProject':'openPath'](value,{
          // Reveal hides this form intentionally, but never retires its request
          // generation. Other callers retain their complete currency predicate.
          isCurrent:function(){var current=revealing&&isCurrent&&isCurrent.requestCurrent||isCurrent;
            return !current||current()===true;},
          confirm:async function(previous){return !previous||baseline===previous.documentDigest||
            !!(ports.confirmReplace&&await ports.confirmReplace(creating));},
          occlude:ports.occlude,reveal:ports.reveal?async function(current){revealing=true;await ports.reveal(current);}:undefined,restore:ports.restore,
          commit:function(current){
            if((creating?ports.created(config.name||'Untitled',current):ports.opened(path,current))!==true)
              throw new Error('Native project UI publication failed');
            baseline=creating?null:current.documentDigest;ports.repaint();
          }
        });
        if(creating&&info&&ports.createdNotice)ports.createdNotice();
        return info===null?null:!!info;
      }catch(error){
        if(error&&error.details&&error.details.disposition==='notDispatched')ports.refused(error);
        else{baseline=null;ports.failed(error);ports.repaint();if(onCreationFailure)onCreationFailure(error);}
        return false;
      }finally{opening=false;}
    }
    async function saveAs(){
      if(saving||opening)return {status:saving?'save-in-progress':'open-in-progress',acknowledged:false};
      saving=true;
      try{
        var options={requestId:session.requestId('save'),pinRequestId:session.requestId('save-pin'),project:true};
        var ticket=saved.begin({saveRequestId:options.requestId,serializerRequestId:options.pinRequestId}),serialization;
        options.isCurrent=function(){return saved.isCurrent(ticket);};
        var written=await workflow.run({readPresentedObjectSession:session.readPresentedObjectSession,
          acquirePin:async function(request){serialization=await session.acquirePin(request);return serialization;},
          pickDestination:ports.pick,savePinned:session.savePinned},options);
        var result=await saved.acknowledge(ticket,{written:written,serialization:serialization},options);
        ports.notice(result,written);return result;
      }catch(error){ports.notice({status:'refused'},null,error);return {status:'refused'};}
      finally{saving=false;}
    }
    return Object.freeze({openPath:openPath,newProject:newProject,saveAs:saveAs,
      canOpen:function(){return !saving&&!opening&&(!session.blocksLegacy()||session.isActive());},
      isDirty:function(){var info=session.info();return !session.isActive()&&session.blocksLegacy()||!baseline||!info||info.documentDigest!==baseline;}});
  }
  function projectView(root,info){
    // These rows are read-side UI projections. The native edit guard denies
    // all legacy writes, and no Paper object or serializable document is built.
    var layers=layerTable(info.layers);validateSettings(info);
    var s=root.state;
    s.canvasW=info.canvasW;s.canvasH=info.canvasH;s.fps=info.fps;s.totalFrames=info.totalFrames;
    projectSettings(root,info,null);s.currentFrame=0;s.activeLayerIdx=0;
    s.layers=layers.map(function(l,i){var row={layerUid:l.layerUid,
      name:l.name===undefined?'Layer '+(i+1):l.name,visible:l.visible,locked:false,color:'#5FA875',parentLayerUid:null,frames:[],motion:{}};
      projectFields(row,l);return row;});
    s.symbols={};s.cameraKeys=[];s.activeSymbolId=null;s.refMedia=null;
    root._curFrame=0;root._totalF=info.totalFrames;root._waIn=s.waIn;root._waOut=s.waOut;
    root._layerSel=info.layers.length?[0]:[];root.selectedPaths=[];
    if(root.userLayers)root.userLayers.forEach(function(layer){layer.visible=false;});
    if(root.SMReference)root.SMReference.refreshNative();
    var pin={layerUids:info.layers.map(function(layer){return layer.layerUid;}),contentRevision:info.contentRevision,documentSnapshotId:info.documentSnapshotId};
    ['instanceId','documentId','lifecycleGeneration','canvasW','canvasH','fps','totalFrames'].forEach(function(k){pin[k]=info[k];});
    projections.set(root,pinSettings(pin,info));
  }
  function bind(root,ports){
    var session=root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root);
    if(!session)return null;
    var startWasHidden=false;
    function commit(path,info){
      var name=root.SMProjectDocument.baseName(path);
      // Persist recents before changing the visible file identity.
      ports.touchRecent(path,name,info,true);ports.commit(path,name);ports.renderRecents();return true;
    }
    var ui=create(session,{
      readUI:function(){return {handle:ports.read().tabId||'native-project-opening',generation:1};},
      confirmReplace:function(creating){return root.__TAURI__.dialog.ask('Unsaved changes will be lost. '+(creating?'Create a new project?':'Open another project?'),
        {title:'Unsaved changes',kind:'warning',okLabel:creating?'Discard and create':'Discard and open',cancelLabel:'Cancel'});},
      occlude:function(){startWasHidden=root.document.getElementById('start-screen').classList.contains('hid');ports.show();},
      restore:function(previous){if(previous&&startWasHidden)ports.hide();},
      reveal:async function(info){
        projectView(root,info);if(root.updateUI)root.updateUI();ports.hide();
        await new Promise(function(resolve,reject){var done=false,timer=setTimeout(function(){
            if(done)return;done=true;reject(new Error('Native project layout did not settle'));
          },1500);
          function finish(){if(done)return;done=true;clearTimeout(timer);resolve();}
          root.requestAnimationFrame(function(){root.requestAnimationFrame(finish);});});
      },
      opened:function(path,info){if(commit(path,info)!==true)return false;ports.opened();ports.notice('Opened: '+ports.read().name);return true;},
      created:function(name){ports.commit(null,name);ports.opened();return true;},
      createdNotice:function(){ports.notice('New project created');},
      commit:commit,repaint:ports.repaint,
      pick:function(){return root.__TAURI__.dialog.save({title:'Save Project As',defaultPath:ports.read().name+'-copy.json',filters:[{name:'Nemo Project',extensions:['json']}]});},
      notice:function(result,written,error){
        ports.notice(result.acknowledged?'Saved: '+ports.read().name:error?'Save failed: '+(error.message||error):
          written&&written.receipt&&written.receipt.published?'File written; current saved status was not confirmed.':'Save cancelled or unavailable. Choose a new file path.');
      },
      refused:function(error){ports.notice('Could not open native project: '+(error.message||error));},
      failed:function(error){ports.show();ports.notice('Could not open native project: '+(error.message||error));}
    },root.NemoNativeObjectSaveAsWorkflow,root.NemoNativeObjectSavedAcknowledgement);
    return Object.freeze({isActive:session.blocksLegacy,canOpen:ui.canOpen,isDirty:ui.isDirty,saveAs:ui.saveAs,openPath:ui.openPath,newProject:ui.newProject});
  }
  return Object.freeze({create:create,projectView:projectView,bind:bind,
    projectionReadyForUI:projectionReadyForUI,
    projectionMatches:projectionMatches,refreshVisibility:refreshVisibility,refreshLayerTable:refreshLayerTable,
    hasProjection:function(root){return projections.has(root);}});
}));
