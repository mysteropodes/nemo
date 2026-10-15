/* Ordinary file UI consumes native pins/digests; never project/media bytes. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProjectFile=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var projections=new WeakMap();
  function projectionMatches(root,info){
    var pin=projections.get(root),rows=root.state&&root.state.layers;
    return !!pin&&!!info&&['instanceId','documentId','lifecycleGeneration'].every(function(k){return pin[k]===info[k];})&&
      Array.isArray(rows)&&Array.isArray(info.layers)&&rows.length===info.layers.length&&
      rows.every(function(row,i){return row.layerUid===info.layers[i].layerUid;});
  }
  function refreshVisibility(root,info){
    if(!projectionMatches(root,info))return false;
    var values=info.layers.map(function(layer){
      if(Object.prototype.hasOwnProperty.call(layer,'visible')&&typeof layer.visible!=='boolean')
        throw new Error('Native layer visibility is invalid');
      return layer.visible!==false;
    });
    root.state.layers.forEach(function(row,i){row.visible=values[i];});
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
          // The form intentionally becomes hidden during native reveal. Its
          // currency predicate applies through dispatch and first presentation.
          isCurrent:function(){return revealing||!isCurrent||isCurrent()===true;},
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
    if(!info.layers.every(function(l){return !Object.prototype.hasOwnProperty.call(l,'name')||typeof l.name==='string';}))
      throw new Error('Native layer name is invalid');
    if(!info.layers.every(function(l){return !Object.prototype.hasOwnProperty.call(l,'visible')||typeof l.visible==='boolean';}))
      throw new Error('Native layer visibility is invalid');
    var s=root.state;
    s.canvasW=info.canvasW;s.canvasH=info.canvasH;s.fps=info.fps;s.totalFrames=info.totalFrames;
    s.canvasBg='#ffffff';s.waIn=0;s.waOut=info.totalFrames-1;s.currentFrame=0;s.activeLayerIdx=0;
    s.layers=info.layers.map(function(l,i){return {layerUid:l.layerUid,
      name:Object.prototype.hasOwnProperty.call(l,'name')?l.name:'Layer '+(i+1),visible:l.visible!==false,locked:false,
      frames:[],color:'#5FA875',motion:{},parentLayerUid:null};});
    s.symbols={};s.cameraKeys=[];s.activeSymbolId=null;s.refMedia=null;
    root._curFrame=0;root._totalF=info.totalFrames;root._waIn=0;root._waOut=info.totalFrames-1;
    root._layerSel=info.layers.length?[0]:[];root.selectedPaths=[];
    if(root.userLayers)root.userLayers.forEach(function(layer){layer.visible=false;});
    if(root.SMReference)root.SMReference.refreshNative();
    projections.set(root,{instanceId:info.instanceId,documentId:info.documentId,lifecycleGeneration:info.lifecycleGeneration});
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
        projectView(root,info);ports.hide();
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
    projectionMatches:projectionMatches,refreshVisibility:refreshVisibility});
}));
