/* UI intent only. Native create, metadata, frame and history stay authoritative. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeDrawingAdd=api;}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var pinKeys=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  function same(a,b){return !!a&&!!b&&pinKeys.every(function(k){return a[k]===b[k];});}
  function presented(session,info,frame){var value=session.readPresentedObjectSession();
    return !!value&&value.available===true&&same(info,value)&&same(info,value.presentation)&&
      value.presentation.owner==='native'&&value.presentation.status==='presented'&&value.presentation.frame===frame;
  }
  function sameSelection(a,b){return Array.isArray(a)&&Array.isArray(b)&&a.length===b.length&&a.every(function(uid,i){return uid===b[i];});}
  function nextName(layers){var names=new Set(layers.map(function(layer){return layer.name;})),n=1;
    while(names.has('Layer '+n))n++;return 'Layer '+n;
  }
  function create(ports){
    var pending=false,generation=0,retired=false;
    async function add(){
      if(retired)return {status:'unavailable'};
      if(pending)return {status:'busy'};
      var session,before,frame,selection,write;
      try{
        session=ports.currentSession();before=session&&session.info();frame=ports.frame();selection=ports.selection();
        if(!session||!before||!Array.isArray(before.layers)||!Number.isSafeInteger(frame)||frame<0||frame>=before.totalFrames||
          !Array.isArray(selection)||ports.available(session,before)!==true||!presented(session,before,frame)||
          typeof (write=session.createLayer)!=='function')return {status:'unavailable'};
      }catch(_){return {status:'unavailable'};}
      selection=selection.slice();var ticket=++generation,name=nextName(before.layers),created=null;pending=true;
      try{
        var receipt=await write.call(session,name,before),result=receipt&&receipt.result;
        if(!receipt||receipt.apiVersion!==2||receipt.ok!==true||receipt.instanceId!==before.instanceId||receipt.documentId!==before.documentId||
          receipt.contentRevision!==before.contentRevision+1||!result||Object.keys(result).length!==3||
          result.applied!==true||result.historyEntriesAdded!==1||typeof result.layerUid!=='string'||!result.layerUid||
          before.layers.some(function(row){return row.layerUid===result.layerUid;}))return {status:'unconfirmed'};
        created=result.layerUid;
        if(ticket!==generation||ports.currentSession()!==session)return {status:'created-unselected',layerUid:created};
        var after=session.info(),rows=after&&after.layers;
        if(!after||after.instanceId!==before.instanceId||after.documentId!==before.documentId||after.lifecycleGeneration!==before.lifecycleGeneration||
          after.contentRevision!==receipt.contentRevision||!Array.isArray(rows)||rows.length!==before.layers.length+1||
          !before.layers.every(function(row,i){return rows[i].layerUid===row.layerUid;}))return {status:'created-unselected',layerUid:created};
        var row=rows[rows.length-1];
        if(row.layerUid!==created||row.name!==name||row.visible!==true||row.locked!==false||row.parentLayerUid!==null||
          typeof row.color!=='string'||!/^#[a-fA-F0-9]{6}$/.test(row.color)||ports.available(session,after)!==true||
          ports.frame()!==frame||!sameSelection(selection,ports.selection())||!presented(session,after,frame))
          return {status:'created-unselected',layerUid:created};
        if(ports.select(created,after,session)!==true||ports.currentSession()!==session||!same(session.info(),after)||
          !presented(session,after,frame)||ports.frame()!==frame||!sameSelection([created],ports.selection()))
          return {status:'created-unselected',layerUid:created};
        return {status:'created-selected',layerUid:created};
      }catch(_){return created?{status:'created-unselected',layerUid:created}:{status:'unconfirmed'};}
      finally{pending=false;}
    }
    return Object.freeze({add:add,dispose:function(){retired=true;generation++;},isPending:function(){return pending;}});
  }
  return Object.freeze({create:create});
}));
