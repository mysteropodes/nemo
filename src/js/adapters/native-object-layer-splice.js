/* Contiguous native layer insertion/removal plan; the existing projection applies it. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectLayerSplice=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function uniqueIds(values){
    return Array.isArray(values)&&values.every(function(uid){return typeof uid==='string'&&uid.length>0;})&&new Set(values).size===values.length;
  }
  function plan(previous,rows,next,active,selection,anchor){
    if(!uniqueIds(previous)||!Array.isArray(rows)||rows.length!==previous.length||
      !rows.every(function(row,i){return row&&row.layerUid===previous[i];})||!Array.isArray(next)||
      !next.every(function(row){return row&&typeof row==='object';}))return null;
    var incoming=next.map(function(row){return row.layerUid;}),delta=incoming.length-previous.length;
    if(!uniqueIds(incoming))return null;
    var at=0;
    while(at<Math.min(previous.length,incoming.length)&&previous[at]===incoming[at])at++;
    if(delta===0){
      if(at!==previous.length)return null;
      return Object.freeze({delta:0,at:at,added:null,active:active,selection:selection,anchor:anchor});
    }
    for(var i=at;i<Math.min(previous.length,incoming.length);i++){
      if(delta>0?previous[i]!==incoming[i+delta]:previous[i-delta]!==incoming[i])return null;
    }
    var chosen=Array.isArray(selection)?selection:[],indices=[];
    chosen.forEach(function(index){
      if(!Number.isInteger(index)||index<0||index>=previous.length)return;
      var mapped=incoming.indexOf(previous[index]);
      if(mapped>=0&&indices.indexOf(mapped)<0)indices.push(mapped);
    });
    var mappedActive=Number.isInteger(active)&&active>=0&&active<previous.length?incoming.indexOf(previous[active]):-1;
    if(mappedActive<0)mappedActive=indices.length?indices[0]:Math.max(0,Math.min(at-1,incoming.length-1));
    if(!indices.length&&chosen.length&&incoming.length)indices=[mappedActive];
    var unchanged=Array.isArray(selection)&&indices.length===selection.length&&indices.every(function(index,j){return index===selection[j];});
    var mappedAnchor=anchor;
    if(anchor!==null&&anchor!==undefined&&anchor!==-1){
      mappedAnchor=Number.isInteger(anchor)&&anchor>=0&&anchor<previous.length?incoming.indexOf(previous[anchor]):-1;
      if(mappedAnchor<0)mappedAnchor=mappedActive;
    }
    var addedRows=delta>0?next.slice(at,at+delta).map(function(layer,j){
      return {layerUid:layer.layerUid,name:layer.name===undefined?'Layer '+(at+j+1):layer.name,visible:layer.visible!==false};
    }):[];
    return Object.freeze({delta:delta,at:at,added:addedRows[0]||null,addedRows:addedRows,active:mappedActive,
      selection:unchanged?selection:indices,anchor:mappedAnchor});
  }
  return Object.freeze({plan:plan});
}));
