/* Image import and removal as native asset-catalog commands (lane 6, G01/G02/G27).
 * A thin client of the native project session: it derives the baseline display name
 * and declared media type from a picked path, validates the closed payload, and sends
 * ONE command whose bytes the host reads itself. It owns no catalog, no mirror of the
 * legacy media library and no document writer; that legacy writer is not reachable. */
(function(root,factory){
  var command=typeof module==='object'&&module.exports?require('./native-object-item-command.js'):root.NemoNativeObjectItemCommand;
  var contract=typeof module==='object'&&module.exports?require('./native-object-item-contract.js'):root.NemoNativeObjectItemContract;
  var api=factory(command,contract);
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectRasterAsset=api;
}(typeof globalThis==='object'?globalThis:this,function(Command,Contract){
  'use strict';
  // Rules of src/js/images.js at the baseline: extOf and baseName verbatim, mimeOf with an own-property lookup (the baseline object
  // literal also answers inherited names such as `constructor`), and the name expression of the import paths.
  var MIME={png:'image/png',jpg:'image/jpeg',jpeg:'image/jpeg',gif:'image/gif',webp:'image/webp',bmp:'image/bmp',svg:'image/svg+xml',avif:'image/avif'};
  var IMAGE_EXTENSIONS=Object.freeze(['png','jpg','jpeg','gif','webp','bmp','svg','avif']);
  function extOf(name){var m=/\.([^.]+)$/.exec(name);return m?m[1].toLowerCase():'';}
  function mimeOf(ext){return Object.prototype.hasOwnProperty.call(MIME,ext)?MIME[ext]:'image/png';}
  function baseName(path){var parts=path.split(/[\\/]/);return parts[parts.length-1];}
  function fault(code,reason,message){var e=new Error(message);e.code=code;e.reason=reason;return e;}
  function describePath(path){
    if(typeof path!=='string'||!path.length||path.includes('\0'))throw fault('invalid_request','local_validation','An image path must be a non-empty string without NUL');
    return Object.freeze({name:baseName(path).replace(/\.[^.]+$/,''),mediaType:mimeOf(extOf(path))});
  }
  // L06-D rule: the owner mints `asset-<requestId>` for a NEW asset. The receipt does not echo it yet, so the adapter derives it;
  // a duplicate (applied false) names an already held asset and has no new id to derive.
  function mintedAssetId(requestId){return 'asset-'+requestId;}
  function create(session,ports){
    ports=ports||{};
    var commands=Command.create(session,'asset');
    function routed(){var route=ports.assetRoute;return typeof route==='function'?route()===true:route===true;}
    function gate(){
      // The host byte-source route is Main's. Until it is declared available the answer is typed unavailability, never a JS fallback.
      return routed()?null:Promise.reject(fault('unavailable','asset_host_route_unavailable','Native asset import route is unavailable'));
    }
    function attachPath(path){
      var blocked=gate();if(blocked)return blocked;
      var described;try{described=describePath(path);}catch(error){return Promise.reject(error);}
      return commands.execute(Contract.ATTACH,function(pin){
        return {documentSnapshotId:pin.documentSnapshotId,name:described.name,mediaType:described.mediaType,source:{kind:'file',path:path}};
      },function(receipt){
        // Also applied to the receipt of a replay after an indeterminate outcome.
        return Object.freeze(Object.assign({},receipt,{assetId:receipt.applied?mintedAssetId(receipt.requestId):null}));
      });
    }
    function imageRouted(){var route=ports.imageRoute;return (typeof route==='function'?route():route)===true;}
    function imageGate(){return imageRouted()?null:Promise.reject(fault('unavailable','image_host_route_unavailable','Image import is unavailable'));}
    function originPin(){
      var value=session.info();
      if(!session.isActive()||!value)throw fault('unavailable','no_native_project','Image import is unavailable');
      return ['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'].reduce(function(out,key){out[key]=value[key];return out;},{});
    }
    function sameOrigin(a,b){return !!a&&!!b&&['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'].every(function(key){return a[key]===b[key];});}
    function importStillPath(path,placement,options){
      var blocked=imageGate();if(blocked)return blocked;
      options=options||{};
      var before,disposition;
      try{
        describePath(path);
        if(extOf(path)!=='png')throw fault('unavailable','image_codec_unavailable','This image format is unavailable');
        before=options.origin||originPin();disposition=JSON.parse(JSON.stringify(placement));
      }catch(error){return Promise.reject(error);}
      // Admit the picker origin before entering a queue that deliberately hides the global presentation.
      if(options.current&&options.current()!==true)return Promise.reject(fault('stale_revision','image_origin_changed','Image import context changed'));
      return commands.execute(Contract.ATTACH,function(pin,queueScope){
        if(!imageRouted())throw fault('unavailable','image_host_route_unavailable','Image import is unavailable');
        if(!sameOrigin(pin,before)||(options.current&&options.current(queueScope)!==true))throw fault('stale_revision','image_origin_changed','Image import context changed');
        return {documentSnapshotId:pin.documentSnapshotId,lifecycleGeneration:pin.lifecycleGeneration,name:baseName(path),
          mediaType:'image/png',source:{kind:'file',path:path},placement:disposition};
      });
    }
    async function importStillPicked(placement,options){
      var blocked=imageGate();if(blocked)return blocked;
      options=Object.assign({},options||{});options.origin=options.origin||originPin();
      if(typeof ports.pickImage!=='function')throw fault('unavailable','picker_unavailable','Image import is unavailable');
      var path=await ports.pickImage();
      return path==null?null:importStillPath(path,placement,options);
    }
    async function importPicked(){
      if(typeof ports.pick!=='function')throw fault('unavailable','picker_unavailable','No image picker is bound');
      var path=await ports.pick();
      return path==null?null:attachPath(path);
    }
    function remove(assetId){
      var blocked=gate();if(blocked)return blocked;
      return commands.execute(Contract.REMOVE,function(pin){return {documentSnapshotId:pin.documentSnapshotId,assetId:assetId};});
    }
    // Raster ITEM edits (L06-L). Same route and receipt rules; the owner allocates the id of a created item.
    function item(build){return commands.execute(Contract.ITEM,build);}
    function createItem(layerUid,frame,raster){
      return item(function(){return {command:'object.raster.create',target:{contextId:'scene-root',frameScope:{kind:'authored',frame:frame},layerUid:layerUid,strokeId:''},item:raster};});
    }
    function edit(command,members){return function(target,a,b){return item(function(){return Object.assign({command:command,stableTarget:target},members(a,b));});};}
    return Object.freeze({describePath:describePath,attachPath:attachPath,importPicked:importPicked,
      importStillPath:importStillPath,importStillPicked:importStillPicked,remove:remove,createItem:createItem,
      moveItem:edit('object.raster.move',function(x,y){return {x:x,y:y};}),resizeItem:edit('object.raster.resize',function(width,height){return {width:width,height:height};}),
      rotateItem:edit('object.raster.rotate',function(rotation){return {rotation:rotation};}),opacityItem:edit('object.raster.opacity',function(opacity){return {opacity:opacity};}),
      deleteItem:edit('object.raster.delete',function(){return {};})});
  }
  var bindings=new WeakMap();
  // One binding per host, but the route flag is read at CALL time from the latest options that named it: a first caller without
  // options must not pin an unrouted binding once Main registers the host route.
  function desktop(host,options){
    var module=host.NemoNativeObjectProject,session=module&&module.desktop(host);
    if(!session||!session.blocksLegacy())return null;
    var pin=session.info(),key=JSON.stringify([pin.instanceId,pin.documentId,pin.lifecycleGeneration]);
    var entry=bindings.get(host);
    if(!entry||entry.ownerKey!==key){
      entry={ownerKey:key,route:false,imageRoute:false};
      entry.binding=create(session,{assetRoute:function(){var route=entry.route;return (typeof route==='function'?route():route)===true;},
        imageRoute:function(){var route=entry.imageRoute;return (typeof route==='function'?route():route)===true;},
        pickImage:function(){return host.__TAURI__.dialog.open({title:'Import Image',multiple:false,filters:[{name:'PNG Image',extensions:['png']}]});},pick:function(){
        return host.__TAURI__.dialog.open({title:'Import Image',multiple:false,filters:[{name:'Images',extensions:IMAGE_EXTENSIONS.slice()},{name:'All files',extensions:['*']}]});
      }});
      bindings.set(host,entry);
    }
    if(options&&Object.prototype.hasOwnProperty.call(options,'assetRoute'))entry.route=options.assetRoute;
    if(options&&Object.prototype.hasOwnProperty.call(options,'imageRoute'))entry.imageRoute=options.imageRoute;
    return entry.binding;
  }
  return Object.freeze({create:create,desktop:desktop,describePath:describePath,mintedAssetId:mintedAssetId,IMAGE_EXTENSIONS:IMAGE_EXTENSIONS});
}));
