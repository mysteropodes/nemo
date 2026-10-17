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
    return Object.freeze({describePath:describePath,attachPath:attachPath,importPicked:importPicked,remove:remove,createItem:createItem,
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
    var entry=bindings.get(host);
    if(!entry){
      entry={route:false};
      entry.binding=create(session,{assetRoute:function(){var route=entry.route;return (typeof route==='function'?route():route)===true;},pick:function(){
        return host.__TAURI__.dialog.open({title:'Import Image',multiple:false,filters:[{name:'Images',extensions:IMAGE_EXTENSIONS.slice()},{name:'All files',extensions:['*']}]});
      }});
      bindings.set(host,entry);
    }
    if(options&&Object.prototype.hasOwnProperty.call(options,'assetRoute'))entry.route=options.assetRoute;
    return entry.binding;
  }
  return Object.freeze({create:create,desktop:desktop,describePath:describePath,mintedAssetId:mintedAssetId,IMAGE_EXTENSIONS:IMAGE_EXTENSIONS});
}));
