/* Linked-video import and source replacement as ONE native command (T12 point 8).
 * A thin client of the native project session: it takes a picked host path, validates the
 * closed payload and sends `command.document.object.asset.video.link`; the host probes the
 * file and the engine links it, retaining only its canonical locator. It owns no catalog,
 * no decode session and no document writer, and the legacy relink writer is not reachable. */
(function(root,factory){
  var command=typeof module==='object'&&module.exports?require('./native-object-item-command.js'):root.NemoNativeObjectItemCommand;
  var contract=typeof module==='object'&&module.exports?require('./native-video-asset-contract.js'):root.NemoNativeVideoAssetContract;
  var api=factory(command,contract);
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeVideoAsset=api;
}(typeof globalThis==='object'?globalThis:this,function(Command,Contract){
  'use strict';
  // The picker filter of the baseline relink flow (native-video-bridge.js replaceNativeVideoSource).
  var VIDEO_EXTENSIONS=Object.freeze(['mp4','mov','mkv','webm','avi']);
  var PIN_KEYS=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  function fault(code,reason,message){var e=new Error(message);e.code=code;e.reason=reason;return e;}
  function sameOrigin(a,b){return !!a&&!!b&&PIN_KEYS.every(function(key){return a[key]===b[key];});}
  function create(session,ports){
    if(!Command||!Contract)throw new Error('Native video link modules are unavailable');
    ports=ports||{};
    var commands=Command.create(session,'video',Contract);
    function routed(){var route=ports.videoRoute;return (typeof route==='function'?route():route)===true;}
    function gate(){return routed()?null:Promise.reject(fault('unavailable','video_host_route_unavailable','Video import is unavailable'));}
    function originPin(){
      var value=session.info();
      if(!session.isActive()||!value)throw fault('unavailable','no_native_project','Video import is unavailable');
      return PIN_KEYS.reduce(function(out,key){out[key]=value[key];return out;},{});
    }
    // intent: {kind:'newLayer'} or {kind:'replace',placementId}. options: origin (the picker's five pins) and current(queueScope).
    function link(path,intent,options){
      var blocked=gate();if(blocked)return blocked;
      options=options||{};
      var before,disposition;
      try{
        if(typeof path!=='string'||!path.length||path.includes('\0'))throw fault('invalid_request','local_validation','A video path must be a non-empty string without NUL');
        before=options.origin||originPin();disposition=JSON.parse(JSON.stringify(intent));
      }catch(error){return Promise.reject(error);}
      // Admit the picker origin before entering a queue that deliberately hides the global presentation.
      if(options.current&&options.current()!==true)return Promise.reject(fault('stale_revision','video_origin_changed','Video import context changed'));
      return commands.execute(Contract.LINK,function(pin,queueScope){
        if(!routed())throw fault('unavailable','video_host_route_unavailable','Video import is unavailable');
        if(!sameOrigin(pin,before)||(options.current&&options.current(queueScope)!==true))throw fault('stale_revision','video_origin_changed','Video import context changed');
        return {documentSnapshotId:pin.documentSnapshotId,lifecycleGeneration:pin.lifecycleGeneration,source:{kind:'file',path:path},intent:disposition};
      });
    }
    function importPath(path,options){return link(path,{kind:'newLayer'},options);}
    function replacePath(placementId,path,options){return link(path,{kind:'replace',placementId:placementId},options);}
    async function picked(pick,options,send){
      var blocked=gate();if(blocked)return blocked;
      options=Object.assign({},options||{});options.origin=options.origin||originPin();
      if(typeof pick!=='function')throw fault('unavailable','picker_unavailable','Video import is unavailable');
      // A picker failure dispatched nothing: it is an ordinary unavailable outcome, never an unknown commit.
      var path;try{path=await pick();}catch(error){throw fault('unavailable','picker_failed','Video picker failed: '+(error&&error.message||error));}
      return path==null?null:send(path,options);
    }
    // null when the picker was cancelled; a receipt otherwise.
    function importPicked(options){return picked(ports.pickVideo,options,importPath);}
    function replacePicked(placementId,options){
      if(typeof placementId!=='string'||!placementId.trim())return Promise.reject(fault('invalid_request','local_validation','A placement id is required'));
      return picked(ports.pickReplacement||ports.pickVideo,options,function(path,o){return replacePath(placementId,path,o);});
    }
    return Object.freeze({importPath:importPath,replacePath:replacePath,importPicked:importPicked,replacePicked:replacePicked});
  }
  var bindings=new WeakMap();
  // One binding per host and native owner; the route flag is read at CALL time from the latest options that named it.
  function desktop(host,options){
    var module=host.NemoNativeObjectProject,session=module&&module.desktop(host);
    if(!session||!session.blocksLegacy())return null;
    var pin=session.info(),key=JSON.stringify([pin.instanceId,pin.documentId,pin.lifecycleGeneration]);
    var entry=bindings.get(host);
    if(!entry||entry.ownerKey!==key){
      entry={ownerKey:key,videoRoute:false};
      var dialog=function(title){
        if(!host.__TAURI__||!host.__TAURI__.dialog||typeof host.__TAURI__.dialog.open!=='function')
          return Promise.reject(fault('unavailable','picker_unavailable','Video import is unavailable'));
        return host.__TAURI__.dialog.open({title:title,multiple:false,filters:[{name:'Videos',extensions:VIDEO_EXTENSIONS.slice()}]});
      };
      entry.binding=create(session,{videoRoute:function(){var route=entry.videoRoute;return (typeof route==='function'?route():route)===true;},
        pickVideo:function(){return dialog('Import Video');},pickReplacement:function(){return dialog('Relink / replace video file');}});
      bindings.set(host,entry);
    }
    if(options&&Object.prototype.hasOwnProperty.call(options,'videoRoute'))entry.videoRoute=options.videoRoute;
    return entry.binding;
  }
  return Object.freeze({create:create,desktop:desktop,VIDEO_EXTENSIONS:VIDEO_EXTENSIONS});
}));
