/* UI intent only; native settings, revision, history and presentation stay authoritative. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeProjectSettingsIntent=api;}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var pinKeys=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  function same(a,b){return !!a&&!!b&&pinKeys.every(function(k){return a[k]===b[k];});}
  function presented(session,info,frame){var value=session.readPresentedObjectSession();return !!value&&value.available===true&&
    same(info,value)&&same(info,value.presentation)&&value.presentation.owner==='native'&&value.presentation.status==='presented'&&value.presentation.frame===frame;}
  function equal(a,b){return Array.isArray(a)&&Array.isArray(b)&&a.length===b.length&&a.every(function(v,i){return v===b[i];});}
  function action(value){
    if(!value||typeof value!=='object'||Array.isArray(value))return null;
    var fields={setCanvasSize:['width','height'],setFps:['fps'],setCanvasBackground:['color'],setWorkArea:['inFrame','outFrame']}[value.kind];
    if(!fields||Object.keys(value).length!==fields.length+1||!fields.every(function(k){return Object.prototype.hasOwnProperty.call(value,k);}))return null;
    var valid=value.kind==='setCanvasSize'?fields.every(function(k){return Number.isInteger(value[k])&&value[k]>0&&value[k]<=4294967295;}):
      value.kind==='setCanvasBackground'?typeof value.color==='string'&&/^#?(?:[a-fA-F0-9]{3}|[a-fA-F0-9]{6}|[a-fA-F0-9]{8})$/.test(value.color):
      fields.every(function(k){return Number.isFinite(value[k]);});
    return valid?Object.freeze(Object.assign({},value)):null;
  }
  function effect(info,value){return value.kind==='setCanvasSize'?info.canvasW===value.width&&info.canvasH===value.height:
    value.kind==='setFps'?info.fps===Math.max(1,Math.min(60,value.fps)):
    value.kind==='setCanvasBackground'?info.canvasBg===value.color:info.waIn===value.inFrame&&info.waOut===value.outFrame;}
  function create(ports){var pending=false,retired=false,generation=0,running=null,queued=[];
    async function execute(value){
      if(retired)return {status:'unavailable'};if(pending)return {status:'busy'};
      var session,before,frame,selection,write,intent;
      try{intent=action(value);session=ports.currentSession();before=session&&session.info();frame=ports.frame();selection=ports.selection();
        if(!intent||!session||!before||!Array.isArray(before.layers)||!Number.isSafeInteger(frame)||frame<0||frame>=before.totalFrames||!Array.isArray(selection)||
          ports.available(session,before)!==true||!presented(session,before,frame)||typeof(write=session.editProjectSettings)!=='function')return {status:'unavailable'};
      }catch(_){return {status:'unavailable'};}
      selection=selection.slice();pending=true;var ticket=++generation,applied=null;
      try{var receipt=await write.call(session,intent,before),result=receipt&&receipt.result;
        if(!receipt||receipt.apiVersion!==2||receipt.ok!==true||receipt.instanceId!==before.instanceId||receipt.documentId!==before.documentId||
          !result||Object.keys(result).length!==2||typeof result.applied!=='boolean'||result.historyEntriesAdded!==(result.applied?1:0)||
          receipt.contentRevision!==before.contentRevision+(result.applied?1:0))return {status:'unconfirmed'};
        applied=result.applied;
        if(ticket!==generation||ports.currentSession()!==session)return {status:'applied-unconfirmed',applied:applied};
        var after=session.info();
        if(!after||after.instanceId!==before.instanceId||after.documentId!==before.documentId||after.lifecycleGeneration!==before.lifecycleGeneration||
          after.contentRevision!==receipt.contentRevision||after.totalFrames!==before.totalFrames||!Array.isArray(after.layers)||
          !equal(before.layers.map(function(row){return row.layerUid;}),after.layers.map(function(row){return row.layerUid;}))||
          !effect(after,intent)||ports.available(session,after)!==true||ports.frame()!==frame||!equal(selection,ports.selection())||!presented(session,after,frame))
          return {status:'applied-unconfirmed',applied:applied};
        return {status:applied?'applied-presented':'unchanged',applied:applied};
      }catch(_){return applied===null?{status:'unconfirmed'}:{status:'applied-unconfirmed',applied:applied};}
      finally{pending=false;}
    }
    function context(){var session=ports.currentSession(),info=session&&session.info(),selection=ports.selection();
      if(!info||!Array.isArray(info.layers)||!Array.isArray(selection))throw new Error('unavailable');
      return {session:session,instanceId:info.instanceId,documentId:info.documentId,lifecycleGeneration:info.lifecycleGeneration,
        frame:ports.frame(),selection:selection.slice(),rows:info.layers.map(function(row){return row.layerUid;})};}
    function compatible(a,b){return a&&b&&a.session===b.session&&['instanceId','documentId','lifecycleGeneration','frame'].every(function(k){return a[k]===b[k];})&&equal(a.selection,b.selection)&&equal(a.rows,b.rows);}
    function unavailable(){return {status:'unavailable'};}
    function launch(value,stamp){var active={context:stamp};running=active;var result=execute(value);
      result.then(function(outcome){if(running!==active)return;running=null;
        if(!['applied-presented','unchanged'].includes(outcome.status)){queued.splice(0).forEach(function(item){item.resolve(unavailable());});return;}
        var next=queued.shift();if(!next)return;
        try{if(!retired&&compatible(next.context,context())){next.resolve(launch(next.action,next.context));return;}}catch(_){}
        next.resolve(unavailable());queued.splice(0).forEach(function(item){item.resolve(unavailable());});
      });return result;}
    function edit(value){var intent,stamp;try{intent=action(value);if(retired||!intent)return Promise.resolve(unavailable());stamp=context();}catch(_){return Promise.resolve(unavailable());}
      if(!running)return launch(intent,stamp);
      if(!compatible(stamp,running.context))return Promise.resolve(unavailable());
      return new Promise(function(resolve){var old=queued.findIndex(function(item){return item.action.kind===intent.kind;}),item={action:intent,context:stamp,resolve:resolve};
        if(old<0)queued.push(item);else{queued[old].resolve({status:'superseded'});queued[old]=item;}});
    }
    return Object.freeze({edit:edit,isPending:function(){return pending||!!running||queued.length>0;},dispose:function(){retired=true;generation++;queued.splice(0).forEach(function(item){item.resolve(unavailable());});}});
  }
  return Object.freeze({create:create});
}));
