/* Read-only native metadata presentation. Timing content requires its own native reader. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectUI=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function checked(root,info){
    if(!root.NemoNativeObjectProjectFile||!root.NemoNativeObjectProjectFile.projectionReadyForUI(root,info)||
      !Number.isInteger(root.state.currentFrame)||root.state.currentFrame<0||root.state.currentFrame>=info.totalFrames)
      throw new Error('Native document UI metadata is uncorrelated');
    return info;
  }
  function node(root,id){var el=root.document.getElementById(id);if(!el)throw new Error('Native document UI is unavailable: '+id);return el;}
  function text(root,tag,value){var el=root.document.createElement(tag);el.textContent=value;return el;}
  function render(root,info){
    checked(root,info);
    var list=node(root,'layer-list'),header=node(root,'frame-hdr'),grid=node(root,'frame-grid');
    var rows=info.layers.map(function(layer,i){
      var label=layer.name===undefined?layer.layerUid:layer.name;
      var row=text(root,'button',label+' — '+(layer.visible===false?'Hidden':'Visible'));
      row.type='button';row.className='native-object-layer';row.dataset.layerUid=layer.layerUid;
      row.style.display='block';row.style.width='100%';row.style.textAlign='left';
      row.setAttribute('aria-pressed',String(i===root.state.activeLayerIdx));
      row.setAttribute('aria-label',label+' ('+layer.layerUid+')');
      row.addEventListener('click',function(){root.NemoNativeObjectProject.selectLayerUI(root,i,false,layer.layerUid,info);});
      return row;
    });
    var caption=text(root,'div','Frame '+(root.state.currentFrame+1)+' / '+info.totalFrames);
    var unavailable=text(root,'div','Native keyframe, held-frame and tween timing is unavailable.');
    unavailable.setAttribute('role','status');unavailable.className='native-object-timing-unavailable';
    // Build all nodes before replacing any visible legacy rows/cells.
    list.replaceChildren.apply(list,rows);header.replaceChildren(caption);grid.replaceChildren(unavailable);
    var field=root.document.getElementById('tl-cf');
    if(field){if(root.document.activeElement!==field)field.value=root.state.currentFrame+1;field.max=info.totalFrames;}
    ['tl-tf','info-frame'].forEach(function(id){var el=root.document.getElementById(id);if(el)el.textContent=id==='tl-tf'?info.totalFrames:root.state.currentFrame+1;});
    ['info-badge','info-strokes','info-sel'].forEach(function(id){var el=root.document.getElementById(id);if(el){el.textContent='Unavailable';if(id==='info-badge')el.style.display='none';}});
    return true;
  }
  function select(root,info,index,preserveLayerSel,uid,presented){
    checked(root,info);
    if(!Number.isInteger(index)||index<0||index>=info.layers.length||uid!==undefined&&info.layers[index].layerUid!==uid)return false;
    if(!presented||presented.available!==true||presented.kind!=='native-object'||
      !['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'].every(function(k){return presented[k]===info[k];})||
      !presented.presentation||presented.presentation.owner!=='native'||presented.presentation.status!=='presented'||presented.presentation.frame!==root.state.currentFrame||
      !['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'].every(function(k){return presented.presentation[k]===info[k];}))return false;
    var old={active:root.state.activeLayerIdx,selection:root._layerSel,anchor:root._layerSelAnchor,paths:root.selectedPaths,
      indices:root.state.selectedStrokeIndices,explicit:root._layerActiveExplicit};
    root.state.activeLayerIdx=index;
    if(!preserveLayerSel){root._layerSel=[index];root._layerSelAnchor=index;}
    root.selectedPaths=[];root.state.selectedStrokeIndices=[];root._layerActiveExplicit=true;
    try{return render(root,info);}
    catch(error){root.state.activeLayerIdx=old.active;root._layerSel=old.selection;root._layerSelAnchor=old.anchor;
      root.selectedPaths=old.paths;root.state.selectedStrokeIndices=old.indices;root._layerActiveExplicit=old.explicit;throw error;}
  }
  return Object.freeze({render:render,select:select});
}));
