/* Ordinary file UI consumes native pins/digests; never project/media bytes. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProjectFile=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function create(session,ports,workflow,acknowledgement){
    var baseline=null,saving=false;
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
      try{
        var info=await session.openPath(path);if(!info)return null;
        await ports.reveal(info);
        await session.presentPreview(0);
        ports.opened(path,info);baseline=info.documentDigest;ports.repaint();return true;
      }catch(error){ports.failed(error);return false;}
    }
    async function saveAs(){
      if(saving)return {status:'save-in-progress'};
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
    return Object.freeze({openPath:openPath,saveAs:saveAs,isDirty:function(){var info=session.info();return !baseline||!info||info.documentDigest!==baseline;}});
  }
  function projectView(root,info){
    // These rows are read-side UI projections. The native edit guard denies
    // all legacy writes, and no Paper object or serializable document is built.
    var s=root.state;
    s.canvasW=info.canvasW;s.canvasH=info.canvasH;s.fps=info.fps;s.totalFrames=info.totalFrames;
    s.canvasBg='#ffffff';s.waIn=0;s.waOut=info.totalFrames-1;s.currentFrame=0;s.activeLayerIdx=0;
    s.layers=info.layers.map(function(l,i){return {layerUid:l.layerUid,name:'Layer '+(i+1),visible:true,locked:false,
      frames:[],color:'#5FA875',motion:{},parentLayerUid:null};});
    s.symbols={};s.cameraKeys=[];s.activeSymbolId=null;s.refMedia=null;
    root._curFrame=0;root._totalF=info.totalFrames;root._waIn=0;root._waOut=info.totalFrames-1;
    root._layerSel=info.layers.length?[0]:[];root.selectedPaths=[];
    if(root.userLayers)root.userLayers.forEach(function(layer){layer.visible=false;});
    if(root.SMReference)root.SMReference.refreshNative();
  }
  return Object.freeze({create:create,projectView:projectView});
}));
