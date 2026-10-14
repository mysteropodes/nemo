/* Ordinary file UI consumes native pins/digests; never project/media bytes. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProjectFile=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
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
    async function openPath(path){
      if(saving||opening)return false;
      opening=true;
      try{
        var info=await session.openPath(path,{
          confirm:async function(previous){return !previous||baseline===previous.documentDigest||
            !!(ports.confirmReplace&&await ports.confirmReplace());},
          occlude:ports.occlude,reveal:ports.reveal,restore:ports.restore,
          commit:function(current){
            if(ports.opened(path,current)!==true)throw new Error('Native project UI publication failed');
            baseline=current.documentDigest;ports.repaint();
          }
        });
        return info===null?null:!!info;
      }catch(error){
        if(error&&error.details&&error.details.disposition==='notDispatched')ports.refused(error);
        else{baseline=null;ports.failed(error);ports.repaint();}
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
    return Object.freeze({openPath:openPath,saveAs:saveAs,
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
      confirmReplace:function(){return root.__TAURI__.dialog.ask('Unsaved changes will be lost. Open another project?',
        {title:'Unsaved changes',kind:'warning',okLabel:'Discard and open',cancelLabel:'Cancel'});},
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
      commit:commit,repaint:ports.repaint,
      pick:function(){return root.__TAURI__.dialog.save({title:'Save Project As',defaultPath:ports.read().name+'-copy.json',filters:[{name:'Nemo Project',extensions:['json']}]});},
      notice:function(result,written,error){
        ports.notice(result.acknowledged?'Saved: '+ports.read().name:error?'Save failed: '+(error.message||error):
          written&&written.receipt&&written.receipt.published?'File written; current saved status was not confirmed.':'Save cancelled or unavailable. Choose a new file path.');
      },
      refused:function(error){ports.notice('Could not open native project: '+(error.message||error));},
      failed:function(error){ports.show();ports.notice('Could not open native project: '+(error.message||error));}
    },root.NemoNativeObjectSaveAsWorkflow,root.NemoNativeObjectSavedAcknowledgement);
    return Object.freeze({isActive:session.blocksLegacy,canOpen:ui.canOpen,isDirty:ui.isDirty,saveAs:ui.saveAs,openPath:ui.openPath});
  }
  return Object.freeze({create:create,projectView:projectView,bind:bind});
}));
