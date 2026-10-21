/* Provisional Open only: exposure hints wake retained native work, never publish it. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectOpenExposure=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  // Inspect each returned receipt once; no later guard reads hostile accessors again.
  function viewMode(read){var mode=read?read():'canvas';if(!['canvas','stored-camera'].includes(mode))throw new Error('Native preview view is unavailable');return mode;}
  function matchesView(value,read){try{return !!value&&!!value.view&&value.view.mode===viewMode(read);}catch(_){return false;}}
  function nav(value){
    if(value===undefined)return {zoom:1,pan:[0,0]};
    if(!value||Object.keys(value).sort().join(',')!=='pan,zoom'||!Number.isFinite(value.zoom)||value.zoom<=0||!Array.isArray(value.pan)||value.pan.length!==2||!value.pan.every(Number.isFinite))throw new Error('Native viewport navigation is unavailable');
    return {zoom:value.zoom,pan:value.pan.map(function(n){return n===0?0:n;})};
  }
  function equalNav(a,b){return a.zoom===b.zoom&&a.pan.every(function(n,i){return n===b.pan[i];});}
  function projection(value,mode,navigation){
    if(!['canvas','stored-camera'].includes(mode))return null;
    var wanted;try{wanted=nav(navigation);}catch(_){return null;}
    if(value===undefined&&mode==='canvas'&&equalNav(wanted,nav()))return {mode:'canvas',documentToComposition:[1,0,0,1,0,0],compositionToDocument:[1,0,0,1,0,0]};
    if(!value||!['cameraAvailable,compositionToDocument,documentToComposition,mode','cameraAvailable,compositionToDocument,documentToComposition,mode,navigation'].includes(Object.keys(value).sort().join(',')))return null;
    var actualMode=value.mode,available=value.cameraAvailable,forward=value.documentToComposition,inverse=value.compositionToDocument,navigationValue=value.navigation,actual;
    try{if(Object.prototype.hasOwnProperty.call(value,'navigation')&&navigationValue===undefined)return null;actual=nav(navigationValue);}catch(_){return null;}
    if(actualMode!==mode||!equalNav(actual,wanted)||typeof available!=='boolean'||![forward,inverse].every(function(m){return Array.isArray(m)&&m.length===6;}))return null;
    forward=forward.slice();inverse=inverse.slice();
    if(![forward,inverse].every(function(m){return m.every(Number.isFinite);}))return null;
    var a=forward[0],b=forward[1],c=forward[2],d=forward[3],e=forward[4],f=forward[5],det=a*d-b*c;
    if(!Number.isFinite(det)||det===0)return null;
    var expected=[d/det,-b/det,-c/det,a/det,(c*f-d*e)/det,(b*e-a*f)/det];
    if(!inverse.every(function(n,i){return n===expected[i];})||equalNav(actual,nav())&&(mode==='canvas'||!available)&&!forward.every(function(n,i){return n===[1,0,0,1,0,0][i];}))return null;
    var result={mode:mode,cameraAvailable:available,documentToComposition:forward,compositionToDocument:inverse};if(navigationValue!==undefined)result.navigation=actual;return result;
  }
  // The existing session owns desired presentation only. No geometry/camera keys live here.
  function navigation(readMode){
    var owner=null,mode=null,desired=nav(),gesture=null;
    function identity(info){return JSON.stringify([info.instanceId,info.documentId,info.lifecycleGeneration]);}
    function read(info){var next=viewMode(readMode),key=identity(info);
      if(owner!==key||mode!==next){owner=key;mode=next;desired=nav();gesture=null;}
      return {mode:mode,navigation:nav(desired)};
    }
    function matches(p,info){try{var value=read(info);return !!p&&!!p.view&&p.view.mode===value.mode&&equalNav(nav(p.view.navigation),value.navigation);}catch(_){return false;}}
    function begin(p,info,frame,mapping,shownBounds,kind,point){
      var selected=read(info),b=mapping&&mapping.cssBounds;
      if(!p||p.frame!==frame||p.view.mode!==selected.mode||!b||!shownBounds||!['x','y','width','height'].every(function(k){return Number.isFinite(b[k])&&b[k]===shownBounds[k];})||b.width<=0||b.height<=0||!['pan','zoom'].includes(kind))return null;
      var actual=nav(p.view.navigation),inverse=p.view.compositionToDocument,forward=p.view.documentToComposition;
      if(![inverse,forward].every(function(m){return Array.isArray(m)&&m.length===6&&m.every(Number.isFinite);}))return null;
      var cx=info.canvasW/2,cy=info.canvasH/2,r=actual.zoom/desired.zoom,shift=[desired.pan[0]-actual.pan[0],desired.pan[1]-actual.pan[1]];
      var m=[inverse[0]*r,inverse[1]*r,inverse[2]*r,inverse[3]*r,
        inverse[4]+(inverse[0]*cx+inverse[2]*cy)*(1-r)+shift[0],inverse[5]+(inverse[1]*cx+inverse[3]*cy)*(1-r)+shift[1]];
      var scale=Math.hypot(forward[0]*b.width/info.canvasW,forward[1]*b.height/info.canvasH)/actual.zoom;
      if(!Number.isFinite(scale)||scale<=0||!m.every(Number.isFinite))return null;
      point=point||{x:b.x+b.width/2,y:b.y+b.height/2};
      if(![point.x,point.y].every(Number.isFinite))return null;
      gesture={source:[info.instanceId,info.documentId,info.contentRevision,info.documentSnapshotId,info.lifecycleGeneration],frame:frame,mode:mode,
        kind:kind,bounds:{x:b.x,y:b.y,width:b.width,height:b.height},canvas:[info.canvasW,info.canvasH],inverse:m,start:{x:point.x,y:point.y},base:nav(desired),scale:scale};
      [gesture.source,gesture.bounds,gesture.canvas,gesture.inverse,gesture.start,gesture.base.pan,gesture.base,gesture].forEach(Object.freeze);return gesture;
    }
    function update(token,info,frame,mapping,point,factor,target){
      read(info);
      if(token!==gesture||token.frame!==frame||token.mode!==mode||![info.instanceId,info.documentId,info.contentRevision,info.documentSnapshotId,info.lifecycleGeneration].every(function(v,i){return v===token.source[i];}))throw new Error('Native navigation source changed');
      if(!mapping||!mapping.cssBounds||!['x','y','width','height'].every(function(k){return mapping.cssBounds[k]===token.bounds[k];}))throw new Error('Native navigation bounds changed');
      var b=token.bounds,m=token.inverse,cx=token.canvas[0]/2,cy=token.canvas[1]/2;
      function world(p){var x=(p.x-b.x)*token.canvas[0]/b.width,y=(p.y-b.y)*token.canvas[1]/b.height;return [m[0]*x+m[2]*y+m[4],m[1]*x+m[3]*y+m[5]];}
      point=point||token.start;if(![point.x,point.y].every(Number.isFinite))throw new Error('Native navigation point is unavailable');
      var z=token.base.zoom,pan=token.base.pan.slice(),anchor=world(token.start),destination=world(point),centre=[m[0]*cx+m[2]*cy+m[4],m[1]*cx+m[3]*cy+m[5]];
      if(token.kind==='pan'){pan[0]+=anchor[0]-destination[0];pan[1]+=anchor[1]-destination[1];}
      else {
        var absolute=token.scale*z;
        if(target==='fit')absolute=Math.min(b.width*.85/token.canvas[0],b.height*.85/token.canvas[1],3);
        else if(target==='reset')absolute=1;
        else if(typeof target==='number')absolute=target;
        else {if(!Number.isFinite(factor)||factor<=0)throw new Error('Native navigation zoom is unavailable');absolute=Math.max(.05,Math.min(20,absolute*factor));}
        var next=absolute/token.scale;
        if(target==='fit'||target==='reset'){pan=[token.base.pan[0]+cx-centre[0],token.base.pan[1]+cy-centre[1]];}
        else {var ratio=z/next;pan[0]+=anchor[0]-(centre[0]+(destination[0]-centre[0])*ratio);pan[1]+=anchor[1]-(centre[1]+(destination[1]-centre[1])*ratio);}
        z=next;
      }
      desired=nav({zoom:z,pan:pan});return nav(desired);
    }
    return Object.freeze({read:read,matches:matches,begin:begin,update:update,identity:function(value){return equalNav(value,nav());}});
  }
  function inspectReceipt(receipt,ownerMatches,allowDeferred,viewMode,navigation){
    if(!ownerMatches)return {failure:'owner'};
    if(!receipt)return {failure:'receipt'};
    var status=receipt.status;
    if(status!=='presented'&&!(allowDeferred&&status==='deferred-occluded'))return {failure:
      ['stale-discarded','deferred-timeout','deferred-occluded','failed-validation','failed-device-lost',
        'failed-retry-lost','failed-retry-outdated','failed-recreate','failed-reconfigure'].includes(status)?'status-'+status:'status'};
    var workId=receipt.workId;
    if(typeof workId!=='string'||!/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(workId))return {failure:'work-id'};
    var viewGeneration=receipt.viewGeneration;
    if(!Number.isSafeInteger(viewGeneration)||viewGeneration<1)return {failure:'view-generation'};
    var view=projection(receipt.view,viewMode||'canvas',navigation);if(!view)return {failure:'view'};
    return {failure:null,status:status,workId:workId,viewGeneration:viewGeneration,view:view};
  }
  function presentedFrame(receipt,request,frame){
    return {owner:'native',status:'presented',contextId:'scene-root',frame:frame,workId:receipt.workId,
      viewGeneration:receipt.viewGeneration,view:receipt.view,instanceId:request.instanceId,documentId:request.documentId,
      contentRevision:request.contentRevision,documentSnapshotId:request.documentSnapshotId,lifecycleGeneration:request.lifecycleGeneration};
  }
  function create(subscribe,guard){
    var active=true,connected=false,epoch=0,visible=null,binding=null,detached=false,waiter=null,reason=null,rejectStop;
    var stopped=new Promise(function(_,reject){rejectStop=reject;});stopped.catch(function(){});
    function assertActive(){if(!active)throw reason;}
    function check(){assertActive();guard();}
    function cancel(error){
      if(!active)return;
      reason=error instanceof Error?error:new Error('Native project Open was cancelled');active=false;
      if(waiter){var pending=waiter;waiter=null;pending.reject(reason);}
      rejectStop(reason);
    }
    function hint(value){
      if(!active||detached||!value||typeof value.visible!=='boolean'||Object.keys(value).length!==1)return;
      try{check();}catch(error){cancel(error);return;}
      if(visible===value.visible)return;
      visible=value.visible;if(visible){epoch++;if(waiter){var pending=waiter;waiter=null;pending.resolve();}}
    }
    async function join(action){
      check();var work=Promise.resolve().then(function(){check();return action();});
      var result=await Promise.race([work,stopped]);check();return result;
    }
    async function connect(){
      check();if(typeof subscribe!=='function')return false;
      var registration=Promise.resolve().then(function(){check();return subscribe(hint,cancel);}).then(function(value){
        if(!value||Object.keys(value).length!==2||typeof value.detach!=='function'||typeof value.close!=='function')
          throw new Error('Native window exposure subscription is unavailable');
        if(!active){value.close();return;}
        binding=value;connected=true;
      });
      await Promise.race([registration,stopped]);check();return true;
    }
    function wait(seen){
      check();if(epoch!==seen)return Promise.resolve();
      return new Promise(function(resolve,reject){
        waiter={resolve:resolve,reject:reject};
        try{check();if(epoch!==seen){waiter=null;resolve();}}catch(error){waiter=null;reject(error);}
      });
    }
    async function run(invoke,verifyOwner,inspect){
      var retained=null;
      for(;;){
        check();var seen=epoch,receipt=inspect(await join(invoke),connected);
        if(receipt.failure)return receipt;
        if(retained&&(receipt.workId!==retained.workId||receipt.viewGeneration!==retained.viewGeneration||JSON.stringify(receipt.view)!==JSON.stringify(retained.view)))
          throw new Error('Native project deferred preview work changed');
        if(receipt.status!=='deferred-occluded'||!connected)return receipt;
        retained={workId:receipt.workId,viewGeneration:receipt.viewGeneration,view:receipt.view};
        await join(verifyOwner);
        // Only a new actual visible exposure can wake this wait. No timer retries.
        await wait(seen);check();await join(verifyOwner);
      }
    }
    function detach(){check();detached=true;if(binding)binding.detach();}
    function close(){
      cancel(new Error('Native project Open exposure scope ended'));
      var previous=binding;binding=null;if(previous)previous.close();
    }
    return Object.freeze({connect:connect,join:join,run:run,cancel:cancel,detach:detach,close:close,
      assertActive:assertActive,isStopped:function(){return !active;}});
  }
  return Object.freeze({create:create,inspectReceipt:inspectReceipt,presentedFrame:presentedFrame,viewMode:viewMode,matchesView:matchesView,navigation:navigation});
}));
