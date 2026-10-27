/* Presenter seam (CS5 #2347): read-model revision notifications, pinned queries and
 * per-surface mount points, so a timeline/Motion/layer panel surface is replaced by a
 * presenter over native read models without editing the monolith bodies.
 * Owner: Ilya/Claude clean-slate session. Fixture: tests/presenter-seam.test.cjs.
 * State: per-root WeakMaps only (the last published pin and the mounted presenters);
 * the read model itself stays owned by Rust, published here by the presenting adapter.
 * A presenter is {render(mountNode, context)}; it builds nodes, its caller commits them.
 * Delivery is at most once per revision identity: the pin advances before listeners run,
 * so a failed listener is reported (rethrown) but the same revision is not re-delivered.
 * Only the layer list has a presenter today (the CS5 pilot); the other declared surfaces
 * are mount points for later presenters and refuse to present until one registers.
 */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoPresenterSeam=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var PIN=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  // Surface id -> existing panel element id (the mount points).
  var SURFACES=Object.freeze({'layer-list':'layer-list','frame-header':'frame-hdr','frame-grid':'frame-grid',
    'inspector':'props-panel','motion-properties':'motion-props-body'});
  var hubs=new WeakMap(),registries=new WeakMap();
  function own(object,key){return Object.prototype.hasOwnProperty.call(object,key);}
  function pinOf(info){
    if(!info||typeof info!=='object')throw new TypeError('Read model is required');
    var pin={};
    PIN.forEach(function(k){if(info[k]===undefined||info[k]===null)throw new TypeError('Read model pin is incomplete: '+k);pin[k]=info[k];});
    return Object.freeze(pin);
  }
  function samePin(a,b){return !!a&&!!b&&PIN.every(function(k){return a[k]===b[k];});}
  function hub(root){var h=hubs.get(root);if(!h){h={pin:null,info:null,listeners:[]};hubs.set(root,h);}return h;}
  // Notifies subscribers once per revision identity; a later publish of the same pin only
  // refreshes the retained read model. Every listener runs before the first failure is rethrown.
  function publish(root,info){
    var h=hub(root),pin=pinOf(info);h.info=info;
    if(samePin(h.pin,pin))return false;
    h.pin=pin;
    var failures=[];
    h.listeners.slice().forEach(function(listener){try{listener(pin,info);}catch(error){failures.push(error);}});
    if(failures.length)throw failures[0];
    return true;
  }
  function subscribe(root,listener){
    if(typeof listener!=='function')throw new TypeError('Read-model listener must be a function');
    var h=hub(root),live=true;h.listeners.push(listener);
    return function unsubscribe(){
      if(!live)return false;live=false;
      var i=h.listeners.indexOf(listener);if(i>=0)h.listeners.splice(i,1);return true;
    };
  }
  function current(root){var h=hubs.get(root);return h&&h.pin?Object.freeze({pin:h.pin,info:h.info}):null;}
  // Pinned query: reads only the read model published for exactly the expected revision.
  // The read model passed to `read` is the retained object: treat it as read-only.
  function query(root,expected,read){
    var now=current(root),want=null;
    try{want=pinOf(expected);}catch(_){want=null;}
    if(!now||!want||!samePin(now.pin,want))throw new Error('Read-model pin is stale');
    return read(now.info);
  }
  function surfaceId(surface){if(!own(SURFACES,surface))throw new Error('Unknown presenter surface: '+surface);return SURFACES[surface];}
  function registry(root){var r=registries.get(root);if(!r){r=new Map();registries.set(root,r);}return r;}
  function register(root,surface,presenter){
    surfaceId(surface);
    if(!presenter||typeof presenter.render!=='function')throw new TypeError('Presenter must implement render');
    var r=registry(root);
    if(r.has(surface))throw new Error('Presenter surface is already mounted: '+surface);
    r.set(surface,presenter);
    var live=true;
    return function unregister(){if(!live||r.get(surface)!==presenter)return false;live=false;r.delete(surface);return true;};
  }
  function isMounted(root,surface){surfaceId(surface);return registry(root).has(surface);}
  function mountPoint(root,surface){
    var id=surfaceId(surface),node=root&&root.document&&root.document.getElementById(id);
    if(!node)throw new Error('Presenter mount point is unavailable: '+surface);
    return node;
  }
  // Returns the mounted presenter's result; refuses (never falls back) when none is mounted.
  function present(root,surface,context){
    var presenter=registry(root).get(surface);
    if(!presenter){surfaceId(surface);throw new Error('No presenter is mounted for surface: '+surface);}
    return presenter.render(mountPoint(root,surface),context);
  }
  return Object.freeze({SURFACES:SURFACES,publish:publish,subscribe:subscribe,current:current,query:query,
    register:register,isMounted:isMounted,mountPoint:mountPoint,present:present});
}));
