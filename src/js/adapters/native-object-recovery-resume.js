/* Start-screen Resume card for native crash recovery (#2360 R3). Lists the Rust recovery store
   (paths and counts only, never project bytes) and resumes the newest slot through the file UI's
   openRecovered, which uses the ordinary session.openPath admission. The list never offers a slot the running
   session owns (ports.ownSlots), and an empty store reads as "nothing to resume", not as a corrupt session. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectRecoveryResume=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var LIST='nemo_native_object_recovery_list',STATUS='nemo_native_object_client_status';
  var SLOT=/^[a-z0-9-]{1,64}$/,ID=/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  function integer(v){return Number.isSafeInteger(v)&&v>=0;}
  function generation(g){
    return !!g&&typeof g==='object'&&typeof g.path==='string'&&g.path.charAt(0)==='/'&&g.path.length<=1024&&g.path.indexOf('\0')<0&&
      integer(g.savedAtMs)&&integer(g.contentRevision)&&Number.isSafeInteger(g.byteCount)&&g.byteCount>0;
  }
  // Keep only well-formed slots with at least one usable generation, newest slot first and newest generation first.
  function normalize(receipt){
    if(!receipt||!Array.isArray(receipt.entries))return [];
    var out=[];
    receipt.entries.forEach(function(e){
      if(!e||typeof e.slotKey!=='string'||!SLOT.test(e.slotKey)||!Array.isArray(e.generations))return;
      var gens=e.generations.filter(generation).sort(function(a,b){return b.savedAtMs-a.savedAtMs;});
      if(gens.length)out.push({slotKey:e.slotKey,displayName:typeof e.displayName==='string'?e.displayName:'Untitled',
        originPath:typeof e.originPath==='string'?e.originPath:null,generations:gens});
    });
    return out.sort(function(a,b){return b.generations[0].savedAtMs-a.generations[0].savedAtMs;});
  }
  // ports: invoke(command,args), requestId(kind), openRecovered(entry), allowOpen(), notice(text), failureText(), emptyText(),
  // ownSlots() (slot keys the live session may have published; optional)
  function create(ports){
    var busy=false;
    async function list(){
      var status=await ports.invoke(STATUS);
      if(!status||status.apiVersion!==2||typeof status.instanceId!=='string'||!ID.test(status.instanceId))return [];
      var request={apiVersion:2,requestId:ports.requestId('recovery-list'),instanceId:status.instanceId};
      var receipt=await ports.invoke(LIST,{requestJson:JSON.stringify(request)});
      if(!receipt||receipt.apiVersion!==2||receipt.requestId!==request.requestId||receipt.instanceId!==request.instanceId)return [];
      var own=[];
      try{own=ports.ownSlots?ports.ownSlots()||[]:[];}catch(_){own=[];}
      return normalize(receipt).filter(function(e){return own.indexOf(e.slotKey)<0;});
    }
    // The card is shown only when a valid recovery exists. Any failure reads as "none" (published, so a card shown earlier
    // is hidden again): Open and New are unaffected.
    function presence(publish){
      return list().then(function(entries){publish(entries.length>0);return entries.length>0;},function(){publish(false);return false;});
    }
    async function resume(){
      if(busy)return false;
      busy=true;
      try{
        if(ports.allowOpen&&ports.allowOpen()!==true)return false;
        var entries=await list();
        if(!entries.length){ports.notice(ports.emptyText?ports.emptyText():ports.failureText());return false;}
        return await ports.openRecovered(entries[0]);
      }catch(_){
        try{ports.notice(ports.failureText());}catch(__){}
        return false;
      }finally{busy=false;}
    }
    return Object.freeze({list:list,presence:presence,resume:resume});
  }
  // Binds to the desktop (Tauri) native object project. Returns null where there is none, so browser and
  // legacy projects keep the nemo-auto card unchanged.
  function bind(root,ports){
    var Project=root.NemoNativeObjectProject,session=Project&&Project.desktop&&Project.desktop(root);
    if(!session||!root.__TAURI__||!root.__TAURI__.core||typeof ports.openRecovered!=='function')return null;
    return create({
      invoke:function(command,args){return arguments.length>1?root.__TAURI__.core.invoke(command,args):root.__TAURI__.core.invoke(command);},
      requestId:function(kind){return session.requestId(kind);},
      openRecovered:ports.openRecovered,allowOpen:ports.allowOpen,notice:ports.notice,ownSlots:ports.ownSlots,
      emptyText:function(){return 'Nothing to resume.';},
      failureText:function(){return root.SM&&root.SM.t?root.SM.t('toastCannotResumeSessionCorrupt'):'Cannot resume the session.';}
    });
  }
  return Object.freeze({create:create,bind:bind,normalize:normalize,LIST:LIST,STATUS:STATUS});
}));
