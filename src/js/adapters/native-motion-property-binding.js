/* Native Motion UI binding; no document or revision writes.
 * Reader receives only the five-field selector and returns a correlated inner
 * reply. Unique context is proved with native targets, never assumed from its
 * name/order. Multiple contexts require the explicit active-context owner port. */
(function(root,factory){
  var api=factory(typeof module==='object'&&module.exports?require('./native-motion-property-panel.js'):root.NemoNativeMotionPropertyPanel,
    typeof module==='object'&&module.exports?require('./native-motion-property-view.js'):root.NemoNativeMotionPropertyView);
  if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeMotionBinding=api;
}(typeof globalThis==='object'?globalThis:this,function(Panel,View){
  'use strict';
  var pins=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  function same(a,b){return !!a&&!!b&&pins.every(function(k){return a[k]===b[k];});}
  function clonePin(v){var p={};pins.forEach(function(k){p[k]=v[k];});return Object.freeze(p);}
  function key(t){return JSON.stringify([t.contextId,t.layerUid,t.strokeId===undefined?null:t.strokeId]);}
  function create(root,ui,reader,options){
    options=options||{};
    if(!root||!root.state||!root.document||!ui||typeof ui.currentSession!=='function'||typeof ui.projectionReady!=='function'||typeof reader!=='function'||!options.container)throw new TypeError('Native Motion binding ports are unavailable');
    var retired=false,generation=0,lastSession=null,lastSignature=null,contextCache=null,pending=null,resolveSerial=0,updateSerial=0,waiting=null;
    function invalidate(){if(lastSession!==null||lastSignature!==null)generation++;lastSession=null;lastSignature=null;return null;}
    function baseScope(){try{
      if(retired||root.state.appMode!=='motion')return invalidate();
      var session=ui.currentSession();if(!session||!session.isActive||!session.isActive()||!session.info||!session.readPresentedObjectSession)return invalidate();
      var info=session.info(),presented=session.readPresentedObjectSession();if(!info||!presented||presented.kind!=='native-object'||ui.projectionReady(info)!==true||!Array.isArray(info.layers))return invalidate();
      if(!Number.isInteger(info.totalFrames)||info.totalFrames<1||!Number.isInteger(root.state.currentFrame)||root.state.currentFrame<0||root.state.currentFrame>=info.totalFrames)return invalidate();
      var identity=clonePin(info),seen=new Set();info.layers.forEach(function(l){if(!l||typeof l.layerUid!=='string'||!l.layerUid||seen.has(l.layerUid))throw new Error('Invalid native layer table');seen.add(l.layerUid);});
      var indices=root._layerSel===undefined?[]:root._layerSel;if(!Array.isArray(indices))return invalidate();indices=indices.slice();var unique=new Set();
      indices.forEach(function(i){if(!Number.isInteger(i)||i<0||i>=info.layers.length||unique.has(i))throw new Error('Invalid UI layer index');unique.add(i);});
      var active=root.state.activeLayerIdx;if(info.layers.length&&(!Number.isInteger(active)||active<0||active>=info.layers.length))return invalidate();
      var override=null;if(options.readTargetSelection){var raw=options.readTargetSelection(identity);if(!raw||!Array.isArray(raw.selected))return invalidate();
        override={primary:raw.primary==null?null:View.normalizeTarget(raw.primary),selected:raw.selected.map(function(t){var v=View.normalizeTarget(t);if(!v)throw new Error('Invalid native UI target');return v;})};if(raw.primary!=null&&!override.primary)return invalidate();}
      var explicit=options.readContextId?options.readContextId(info,presented):null;if(options.readContextId&&(typeof explicit!=='string'||!explicit))return invalidate();
      var transport=options.readTransportGeneration?options.readTransportGeneration():null;
      var signature=JSON.stringify([pins.map(function(k){return identity[k];}),indices,active,root.state.activeSymbolId,override,explicit,transport]);
      if(lastSession!==session||lastSignature!==signature){generation++;lastSession=session;lastSignature=signature;}
      var context={pin:identity,presented:presented,frame:root.state.currentFrame,viewGeneration:generation},token=View.captureContext(context);if(!token)return invalidate();
      return Object.freeze({session:session,info:info,identity:identity,context:context,token:token,indices:indices,active:active,override:override,explicit:explicit,uiSignature:signature});
    }catch(_){return invalidate();}}
    function sameBase(before,after){return !!after&&!retired&&before.session===after.session&&same(before.identity,after.identity)&&before.context.frame===after.context.frame&&View.isCurrentContext(before.token,after.context);}
    function cacheMatches(base){return contextCache&&contextCache.session===base.session&&same(contextCache.identity,base.identity)&&(contextCache.available||contextCache.baseGeneration===base.context.viewGeneration);}
    function uniqueContextAllowed(){return root.state.activeSymbolId===null&&options.singleContextContract!==false&&!options.readTargetSelection;}
    function scope(){var base=baseScope();if(!base)return null;
      var contextId=base.explicit;if(!contextId&&uniqueContextAllowed()&&cacheMatches(base)&&contextCache.available)contextId=contextCache.contextId;if(!contextId)return null;
      var primary,selected;if(base.override){primary=base.override.primary;selected=base.override.selected;}
      else{selected=base.indices.map(function(i){return Object.freeze({contextId:contextId,layerUid:base.info.layers[i].layerUid});});primary=base.info.layers.length?Object.freeze({contextId:contextId,layerUid:base.info.layers[base.active].layerUid}):null;}
      var seen=new Set();if(primary&&primary.contextId!==contextId)return null;
      for(var t of selected){if(t.contextId!==contextId||seen.has(key(t)))return null;seen.add(key(t));}
      return Object.freeze({session:base.session,info:base.info,identity:base.identity,context:base.context,contextId:contextId,selected:Object.freeze(selected),primary:primary});
    }
    function clearWait(){if(waiting){waiting.button.removeEventListener('click',cancel);if(waiting.node.parentNode===options.container)options.container.removeChild(waiting.node);waiting=null;}}
    function showWait(message,cancellable){if(waiting&&waiting.node.parentNode===options.container&&waiting.cancellable===cancellable){waiting.label.textContent=message;return;}clearWait();var node=root.document.createElement('div'),label=root.document.createElement('span'),button=root.document.createElement('button');node.className='native-motion-context-read';label.setAttribute('role','status');label.textContent=message;node.appendChild(label);
      if(cancellable){button.type='button';button.textContent='Cancel';button.addEventListener('click',cancel);node.appendChild(button);}options.container.appendChild(node);waiting={node:node,button:button,label:label,cancellable:cancellable};}
    async function yieldControl(){if(options.yieldControl)await options.yieldControl();else await new Promise(function(resolve){setTimeout(resolve,0);});}
    async function resolveContext(base,force){
      if(base.explicit)return true;if(!uniqueContextAllowed())return false;
      if(!force&&cacheMatches(base))return contextCache.available;
      if(pending&&sameBase(pending.base,base))return pending.promise;
      var ticket=++resolveSerial,entry={base:base,promise:null};pending=entry;
      entry.promise=(async function(){var view={kind:'targets',offset:0,limit:8},contexts=new Set(),ids=new Set();
        try{for(;;){if(ticket!==resolveSerial||!sameBase(base,baseScope()))throw new Error('Native context read retired');
          var selector=Object.freeze({instanceId:base.identity.instanceId,documentId:base.identity.documentId,atRevision:base.identity.contentRevision,documentSnapshotId:base.identity.documentSnapshotId,view:Object.freeze(view)});
          var value=await reader(selector),after=baseScope();if(ticket!==resolveSerial||!sameBase(base,after))throw new Error('Native context became stale');
          var page=View.decodePage(selector,value,after.context);if(!page.available)throw new Error('Native context unavailable');
          page.items.forEach(function(item){var id=item.kind==='context'?'context:'+item.contextId:item.kind+':'+key(item.target);if(ids.has(id))throw new Error('Duplicate native target');ids.add(id);if(item.kind==='context')contexts.add(item.contextId);});
          if(contexts.size>1){contextCache={session:base.session,identity:base.identity,available:false,baseGeneration:base.context.viewGeneration,reason:'active-context-port-required'};return false;}
          var next=View.continuation(page);if(!next){if(contexts.size!==1)throw new Error('Native context identity missing');contextCache={session:base.session,identity:base.identity,available:true,contextId:Array.from(contexts)[0]};generation++;return true;}view=next;await yieldControl();
        }}catch(_){if(ticket===resolveSerial&&sameBase(base,baseScope()))contextCache={session:base.session,identity:base.identity,available:false,baseGeneration:base.context.viewGeneration,reason:'native-read-unavailable'};return false;}
        finally{if(pending===entry)pending=null;}
      }());return entry.promise;
    }
    function readContext(){var v=scope();return v?v.context:null;}
    function readSelection(){var v=scope();return v?v.primary:null;}
    function readSelections(){var v=scope();return v?v.selected:null;}
    function readContextId(){var v=scope();return v?v.contextId:null;}
    async function query(selector){var before=scope();if(!before||!selector||selector.instanceId!==before.identity.instanceId||selector.documentId!==before.identity.documentId||selector.atRevision!==before.identity.contentRevision||selector.documentSnapshotId!==before.identity.documentSnapshotId)throw new Error('Native metadata read unavailable');
      var value=await reader(selector),after=scope();if(!after||before.session!==after.session||!same(before.identity,after.identity)||before.context.viewGeneration!==after.context.viewGeneration||before.context.frame!==after.context.frame)throw new Error('Native metadata read became stale');return value;}
    async function selectTarget(target,pin,choice){var before=scope(),t=View.normalizeTarget(target);if(!before||!t||!same(before.identity,pin)||t.contextId!==before.contextId)return false;
      var preserve=!!(choice&&choice.preserveSelection),result;
      if(t.strokeId!==undefined){if(!options.selectElementTarget||!options.readTargetSelection)return false;result=await options.selectElementTarget(t,pin,Object.freeze({preserveSelection:preserve}));}
      else if(options.selectLayerTargets)result=await options.selectLayerTargets(t,pin,Object.freeze({preserveSelection:preserve}));
      else{if(typeof ui.selectLayer!=='function')return false;var index=before.info.layers.findIndex(function(l){return l.layerUid===t.layerUid;});if(index<0||preserve&&!before.selected.some(function(item){return key(item)===key(t);}))return false;result=ui.selectLayer(index,preserve,t.layerUid,before.info);}
      if(result!==true)return false;var after=scope();return !!after&&before.session===after.session&&same(before.identity,after.identity)&&after.primary&&key(after.primary)===key(t)&&after.selected.some(function(item){return key(item)===key(t);});
    }
    function labelTarget(t,pin){var v=scope();if(!v||!same(v.identity,pin)||t.contextId!==v.contextId)throw new Error('Stale native label');if(t.strokeId!==undefined)return t.strokeId;var matches=v.info.layers.filter(function(l){return l.layerUid===t.layerUid;});return matches.length===1&&typeof matches[0].name==='string'?matches[0].name:t.layerUid;}
    var caller=Panel.createCaller({document:root.document,container:options.container,pageBudget:options.pageBudget,yieldControl:options.yieldControl,
      isMotion:function(){return !retired&&root.state.appMode==='motion';},readContext:readContext,readSelection:readSelection,readSelections:readSelections,readContextId:readContextId,
      query:query,selectTarget:selectTarget,labelTarget:labelTarget,labelProperty:options.labelProperty,readPreference:options.readPreference,writePreference:options.writePreference});
    async function update(force){var ticket=++updateSerial,base=baseScope();if(base&&!base.explicit&&!cacheMatches(base)&&uniqueContextAllowed())showWait('Loading properties…',true);
      if(base)await resolveContext(base,!!force);if(retired||ticket!==updateSerial)return false;clearWait();return caller.update(force);}
    function cancel(){var wasPending=!!pending;updateSerial++;resolveSerial++;pending=null;clearWait();var stopped=caller.cancel();if(wasPending&&!retired)showWait('Property read cancelled.',false);return wasPending||stopped;}
    function dispose(){if(retired)return;retired=true;updateSerial++;resolveSerial++;pending=null;clearWait();invalidate();caller.dispose();}
    async function shortcut(k,shift){var before=baseScope();if(!before||!await update(false))return false;var after=baseScope();if(!after||before.session!==after.session||before.uiSignature!==after.uiSignature||before.context.frame!==after.context.frame)return false;return caller.shortcut(k,shift);}
    var actionScopes=new WeakMap();
    function captureShortcutScope(){var value=baseScope();if(!value)return null;var token=Object.freeze({});actionScopes.set(token,{session:value.session,signature:value.uiSignature,identity:value.identity,frame:value.context.frame});return token;}
    function isShortcutScopeCurrent(token){var before=token&&actionScopes.get(token),after=baseScope();return !!before&&!!after&&before.session===after.session&&before.signature===after.uiSignature&&same(before.identity,after.identity)&&before.frame===after.context.frame;}
    return Object.freeze({update:update,shortcut:shortcut,captureShortcutScope:captureShortcutScope,isShortcutScopeCurrent:isShortcutScopeCurrent,isMounted:caller.isMounted,setFilter:function(mode){return caller.setFilter(mode);},cancel:cancel,dispose:dispose,readContext:readContext,readSelection:readSelection,readSelections:readSelections,readContextId:readContextId,query:query,selectTarget:selectTarget});
  }
  var owners=new WeakMap();
  var revealKeys=['p','a','r','s','t','u','e','m'];
  function editable(node){for(;node;node=node.parentNode)if(['INPUT','SELECT','TEXTAREA'].includes(node.tagName)||node.isContentEditable)return true;return false;}
  function install(document,ports){
    if(!document||!document.addEventListener||!document.removeEventListener||!ports||typeof ports.ownsNativeMotion!=='function'||typeof ports.route!=='function'||typeof ports.captureScope!=='function'||typeof ports.isCurrentScope!=='function')throw new TypeError('Native Motion shortcut ports unavailable');
    if(owners.has(document))throw new Error('Native Motion shortcuts already owned');
    var retired=false,serial=0,api;
    function stop(event){event.preventDefault();if(event.stopImmediatePropagation)event.stopImmediatePropagation();else if(event.stopPropagation)event.stopPropagation();}
    function unavailable(){try{if(!retired&&ports.unavailable)ports.unavailable();}catch(_){}}
    function onKey(event){
      if(retired||event.altKey||event.ctrlKey||event.metaKey||editable(event.target))return;
      var key=String(event.key||'').toLowerCase();if(!revealKeys.includes(key))return;
      var owned;try{owned=ports.ownsNativeMotion();}catch(_){owned=null;}if(owned===false)return;if(owned!==true){stop(event);unavailable();return;}
      // Modal capture listeners on this document retain the event; ancestor
      // propagation is stopped so the legacy document bubble route cannot run.
      try{if(ports.modalOpen&&ports.modalOpen()){if(event.stopPropagation)event.stopPropagation();return;}}catch(_){stop(event);unavailable();return;}
      if(event.defaultPrevented){stop(event);return;}
      stop(event);var ticket=++serial,scope;try{scope=ports.captureScope();}catch(_){scope=null;}if(!scope){unavailable();return;}
      function current(){try{return !retired&&ticket===serial&&ports.ownsNativeMotion()===true&&ports.isCurrentScope(scope)===true;}catch(_){return false;}}
      try{if(!ports.acceptsTarget||ports.acceptsTarget(event)!==true){unavailable();return;}}catch(_){unavailable();return;}
      try{Promise.resolve(ports.route(key,!!event.shiftKey)).then(function(ok){if(current()&&ok!==true)unavailable();},function(){if(current())unavailable();});}catch(_){unavailable();}
    }
    document.addEventListener('keydown',onKey,true);
    api=Object.freeze({dispose:function(){if(retired)return;retired=true;serial++;document.removeEventListener('keydown',onKey,true);if(owners.get(document)===api)owners.delete(document);}});owners.set(document,api);return api;
  }
  function installForBinding(document,root,ui,binding,options){options=options||{};return install(document,{
    ownsNativeMotion:function(){if(root.state.appMode!=='motion')return false;var session=ui.currentSession();return session?typeof session.blocksLegacy==='function'?session.blocksLegacy():null:false;},
    captureScope:binding.captureShortcutScope,isCurrentScope:binding.isShortcutScopeCurrent,route:binding.shortcut,
    acceptsTarget:options.acceptsTarget,modalOpen:options.modalOpen,unavailable:options.unavailable});}
  // One ordinary inspector binding per installed Motion renderer. Ports and
  // final presentation remain owned by Project; this object owns UI disposal.
  function createOrdinary(root,options){
    options=options||{};var active=null,retired=false;
    function retire(){var old=active;active=null;if(!old)return;old.keyboard.dispose();old.header.removeEventListener('contextmenu',old.menu);old.binding.dispose();}
    function unavailable(){if(active&&options.unavailable)options.unavailable();}
    function paint(ports,container,header){
      if(retired||!root.state||root.state.appMode!=='motion'){retire();return false;}
      if(!ports||typeof ports.readPropertyMetadata!=='function'||!container||!header){retire();return false;}
      if(active&&(active.ports!==ports||active.container!==container||active.header!==header||!active.binding.isMounted()))retire();
      if(!active){
        var binding=create(root,ports,function(selector){return ports.readPropertyMetadata(selector);},Object.assign({},options,{container:container,readTransportGeneration:ports.readTransportGeneration}));
        var menu=function(event){event.preventDefault();event.stopPropagation();var scope=binding.captureShortcutScope();if(active&&active.binding===binding&&scope&&options.openFilterMenu)options.openFilterMenu(event,function(mode){if(active&&active.binding===binding&&binding.isShortcutScopeCurrent(scope))return binding.setFilter(mode);return false;});};
        header.addEventListener('contextmenu',menu);
        var keyboard=installForBinding(root.document,root,ports,binding,{acceptsTarget:options.acceptsTarget,modalOpen:options.modalOpen,unavailable:unavailable});
        active={ports:ports,container:container,header:header,binding:binding,menu:menu,keyboard:keyboard};
      }
      Promise.resolve(active.binding.update(false)).catch(unavailable);return true;
    }
    return Object.freeze({paint:paint,retire:retire,shortcut:function(key,shift){return active?active.binding.shortcut(key,shift):Promise.resolve(false);},dispose:function(){if(retired)return;retire();retired=true;}});
  }
  return Object.freeze({create:create,createOrdinary:createOrdinary});
}));
