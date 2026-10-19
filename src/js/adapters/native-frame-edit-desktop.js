/* Ordinary desktop frame writers -> the existing native intent/session queue. No document or history writer. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else {root.NemoNativeFrameEditDesktop=api;api.install(root);}
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var installed=new WeakMap();
  var PIN=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  var shared=Object.freeze({insertFrame:'insertFrame',insertKeyframe:'insertKey',insertBlankKeyframe:'insertBlankKey',
    clearKeyframe:'clearKey',convertToKeyframes:'convertToKeyframes',removeFrame:'removeFrame',
    removeFrameSpan:'removeFrameSpan',removeTweenSpan:'removeTween',duplicateSelectedFrames:'duplicateSelection'});
  var globalNames=Object.freeze(Object.keys(shared).concat(['insertKeyframeAt']));
  var timelineNames=Object.freeze(Object.keys(shared).concat(['deleteSelectedFrames','moveKeyframe','duplicateKeyframe',
    'extendExposure','repeatSelection','copyFrames','cutFrames','pasteFrames','moveFrames','shiftLayerFrames']));
  var empty=Object.freeze([]);
  var shortcuts=Object.freeze({cmdInsertFrame:'insertFrame',cmdInsertKey:'insertKey',cmdInsertBlankKey:'insertBlankKey',
    cmdDuplicateKey:'duplicateKey',cmdExtendExposure:'extendExposure'});
  function writerNames(target){return target==='global'?globalNames:target==='timeline'?timelineNames:empty;}
  function equal(a,b){return Array.isArray(a)&&Array.isArray(b)&&a.length===b.length&&a.every(function(v,i){return v===b[i];});}
  function same(a,b){return !!a&&!!b&&PIN.every(function(k){return a[k]===b[k];});}
  function cellsEqual(a,b){return Array.isArray(a)&&Array.isArray(b)&&a.length===b.length&&a.every(function(v,i){
    return !!v&&!!b[i]&&v.layer===b[i].layer&&v.frame===b[i].frame;});}
  function unavailable(code){return Object.freeze({status:'unavailable',code:code});}
  function mapped(target,name,args){
    if(writerNames(target).indexOf(name)<0)return null;
    args=Array.prototype.slice.call(args||[]);
    if(name==='insertKeyframeAt')return {kind:'insertKeyAt',layer:args[0],frame:args[1]};
    if(name==='removeTweenSpan')return {kind:'removeTween',layer:args[0],from:args[1],to:args[2]};
    if(name==='moveKeyframe')return {kind:'moveKey',layer:args[0],from:args[1],to:args[2]};
    if(name==='extendExposure')return {kind:'extendExposure',count:args[0]};
    if(name==='repeatSelection')return {kind:'repeatSelection',times:args[0]};
    if(name==='moveFrames'||name==='shiftLayerFrames')return {unsupported:true};
    return {kind:shared[name]||({deleteSelectedFrames:'deleteFrames',duplicateKeyframe:'duplicateKey'})[name]||name};
  }
  function install(root){
    if(installed.has(root))return installed.get(root);
    if(!root.NemoNativeFrameEditIntent||typeof root.NemoNativeFrameEditIntent.create!=='function')
      throw new Error('Native frame edit intent is unavailable');
    function current(){return root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root);}
    function available(session,info){return !!(session&&session===current()&&session.isActive()&&
      root.NemoNativeObjectProjectFile&&root.NemoNativeObjectProjectFile.projectionReadyForUI(root,info)===true);}
    function nativeMode(){
      try{
        var cutover=root.NemoNativeOpacityCutover;
        if(cutover&&typeof cutover.blocksLegacy==='function'&&cutover.blocksLegacy())return true;
        var session=current();
        return !!(session&&(session.isActive()||typeof session.blocksLegacy==='function'&&session.blocksLegacy()));
      }catch(_){return true;}
    }
    var intent=root.NemoNativeFrameEditIntent.create({currentSession:current,application:root.NemoNativeApplicationAdapter,
      frame:function(){return root.state&&root.state.currentFrame;},
      activeLayer:function(){return root.state&&root.state.activeLayerIdx;},
      layerSelection:function(){return root._layerSel;},
      cellSelection:function(){return root._sel&&root._sel.frames;},available:available});
    function capture(){
      try{
        var session=current(),info=session&&session.info(),state=root.state;
        if(!info||!state||!Array.isArray(root._layerSel)||!root._sel||!Array.isArray(root._sel.frames)||!Array.isArray(info.layers))return null;
        var pin={};PIN.forEach(function(k){pin[k]=info[k];});
        return {session:session,pin:pin,frame:state.currentFrame,active:state.activeLayerIdx,
          layers:root._layerSel.slice(),cells:root._sel.frames.map(function(cell){return cell&&{layer:cell.layer,frame:cell.frame};}),
          uids:info.layers.map(function(row){return row.layerUid;})};
      }catch(_){return null;}
    }
    function mayNavigate(before,result){
      try{
        if(!before||result.status!=='applied-presented'||result.applied!==true||!Number.isSafeInteger(result.playhead))return false;
        var session=current(),info=session&&session.info(),state=root.state;
        if(session!==before.session||!info||!state||!available(session,info)||
          ['instanceId','documentId','lifecycleGeneration'].some(function(k){return info[k]!==before.pin[k];})||
          info.contentRevision!==before.pin.contentRevision+1||!Array.isArray(info.layers)||
          !equal(before.uids,info.layers.map(function(row){return row.layerUid;}))||
          state.currentFrame!==before.frame||state.activeLayerIdx!==before.active||!equal(root._layerSel,before.layers)||
          !cellsEqual(root._sel&&root._sel.frames,before.cells)||result.playhead<0||result.playhead>=info.totalFrames)return false;
        var value=session.readPresentedObjectSession();
        return !!value&&value.available===true&&same(info,value)&&same(info,value.presentation)&&
          value.presentation.owner==='native'&&value.presentation.status==='presented'&&value.presentation.frame===state.currentFrame;
      }catch(_){return false;}
    }
    function notice(result){
      var status=result&&result.status;
      if(status==='applied-presented'||status==='unchanged')return;
      var message=status==='unavailable'?'Native frame editing is unavailable.':
        status==='refused'?'Native frame edit was refused.':
        status==='busy'?'A native frame edit is already pending.':
        'Native frame edit was not confirmed. Wait for synchronization.';
      try{if(typeof root.showToast==='function')root.showToast(message);}catch(_){}
    }
    async function run(action){
      var before=capture(),result=await intent.run(action);
      if(mayNavigate(before,result)&&typeof root.goToFrame==='function'){
        try{await root.goToFrame(result.playhead);}catch(_){notice();}
      }
      notice(result);
      return result;
    }
    function fire(action){Promise.resolve(run(action)).catch(notice);}
    var controller=Object.freeze({run:run,isPending:intent.isPending,dispose:intent.dispose,
      shortcut:function(action,event){
        if(!Object.prototype.hasOwnProperty.call(shortcuts,action)||!nativeMode())return false;
        if(event&&typeof event.preventDefault==='function')event.preventDefault();
        fire(action==='cmdExtendExposure'?{kind:'extendExposure',count:1}:{kind:shortcuts[action]});
        return true;
      },
      routeWriter:function(target,name,args){
        var action=mapped(target,name,args);
        if(!action||!nativeMode())return null;
        if(action.unsupported){
          var result=unavailable('frame_edit_writer_unavailable');notice(result);
          return {handled:true,result:Promise.resolve(result)};
        }
        return {handled:true,result:run(action)};
      }});
    installed.set(root,controller);root.NemoNativeFrameEditController=controller;
    return controller;
  }
  return Object.freeze({install:install,writerNames:writerNames});
}));
