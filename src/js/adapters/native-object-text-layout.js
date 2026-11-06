/* Text tool to native text-block commands (lane 6, G38a). A thin client of the native
 * project session: it maps the baseline text-tool arguments to the closed native block
 * record, validates the closed payload and sends ONE command per gesture. It builds no
 * glyphs, no Paper items and no layout; vector-text-bridge.js is not reachable from here.
 * Layout and glyph output come from the retained font provider at evaluation, and its
 * absence is the owner's typed unavailability, surfaced unchanged. */
(function(root,factory){
  var command=typeof module==='object'&&module.exports?require('./native-object-item-command.js'):root.NemoNativeObjectItemCommand;
  var contract=typeof module==='object'&&module.exports?require('./native-object-item-contract.js'):root.NemoNativeObjectItemContract;
  var api=factory(command,contract);
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectTextLayout=api;
}(typeof globalThis==='object'?globalThis:this,function(Command,Contract){
  'use strict';
  function fault(code,reason,message){var e=new Error(message);e.code=code;e.reason=reason;return e;}
  // The panel spells a vector font `vector:<key>`; any other value (a raster point-text face, `Google:...`) has no native
  // block form and is refused here, never mapped to a guessed face.
  function fontFromLegacyKey(key){
    var bare=typeof key==='string'&&key.indexOf('vector:')===0?key.slice(7):key;
    if(typeof bare!=='string'||Contract.FONTS.indexOf(bare)<0)throw fault('invalid_request','unsupported_font','Font has no native text block form: '+String(key));
    return {kind:'bundled',fontKey:bare};
  }
  // The member assembly of buildVectorTextGroup (root.data at the baseline), in canonical order and with its coercions
  // (`||` defaults). The authored font key is kept; the baseline folded bold into the stored key, the owner resolves it at layout.
  function blockFromToolArgs(a){
    if(!a||typeof a!=='object'||!a.topLeft||typeof a.topLeft!=='object')throw fault('invalid_request','local_validation','Text tool arguments are invalid');
    return {text:a.text,font:fontFromLegacyKey(a.fontKey),size:a.size,color:a.color,align:a.align,fixedWidth:a.fixedWidth||null,fixedHeight:a.fixedHeight||null,
      bold:!!a.bold,italic:!!a.italic,underline:!!a.underline,strike:!!a.strike,letterSpacing:a.letterSpacing||0,wordSpacing:a.wordSpacing||0,
      lineHeightMult:a.lineHeightMult||1.25,textCase:a.textCase||'none',anchorTopLeft:{x:a.topLeft.x,y:a.topLeft.y}};
  }
  function blank(text){return typeof text==='string'&&!text.trim();}
  // Baseline guards (commitText, applyTextPropsEdit): a blank text never creates or rebuilds a block.
  function skipped(){return Promise.resolve(Object.freeze({applied:false,skipped:'blank_text'}));}
  function stableTarget(layerUid,strokeId,frame,contextId){return {contextId:contextId||'scene-root',frameScope:{kind:'authored',frame:frame},layerUid:layerUid,strokeId:strokeId};}
  function create(session){
    var scene=Command.create(session,'item'),component=null;
    // #2267: an entered component's text rides the component command route (the presented context must be that component), like the vector writes.
    function route(contextId){
      if(!contextId||contextId==='scene-root')return scene;
      if(typeof session.runComponentCommand!=='function')return null;
      return component||(component=Command.create(Object.freeze({info:session.info,isActive:session.isActive,requestId:session.requestId,runResourceCommand:session.runComponentCommand.bind(session)}),'item'));
    }
    function send(payload,origin,guard,contextId){
      var commands=route(contextId),expected=origin&&Object.assign({},origin);
      if(!commands)return Promise.reject(fault('unavailable','component_route_unavailable','Native component route is unavailable'));
      return commands.execute(Contract.ITEM,function(pin,ctx){
        if(expected&&['instanceId','documentId','lifecycleGeneration'].some(function(k){return pin[k]!==expected[k];}))throw fault('stale_owner','owner_changed','Native text owner changed before execution');
        if(expected&&['contentRevision','documentSnapshotId'].some(function(k){return pin[k]!==expected[k];}))throw fault('stale_revision','snapshot_changed','Native text snapshot changed before execution');
        if(guard&&guard(pin,ctx)!==true)throw fault('stale_revision','intent_changed','Native text intent changed before execution');
        return payload;
      });
    }
    // `opts.copy` asks the owner to key a held cell with a copy of what it showed first (the baseline's ensureKeyframe); `opts.frame` names the cell an edit or delete lands in.
    return Object.freeze({
      createBlock:function(layerUid,frame,block,origin,guard,opts){
        if(block&&blank(block.text))return skipped();
        var payload={command:'object.text.create',target:stableTarget(layerUid,'',frame,opts&&opts.contextId),block:block};
        if(opts&&opts.copy)payload.keyframe='copy';
        return send(payload,origin,guard,opts&&opts.contextId);
      },
      editBlock:function(target,patch,origin,guard,opts){
        if(patch&&blank(patch.text))return skipped();
        var payload={command:'object.text.edit',stableTarget:target,patch:patch};
        if(opts&&opts.copy){payload.frame=opts.frame;payload.keyframe='copy';}
        return send(payload,origin,guard,target&&target.contextId);
      },
      deleteBlock:function(target,origin,guard,opts){
        var payload={command:'object.text.delete',stableTarget:target};
        if(opts&&opts.copy){payload.frame=opts.frame;payload.keyframe='copy';}
        return send(payload,origin,guard,target&&target.contextId);
      },
    });
  }
  var bindings=new WeakMap();
  function desktop(host){
    var module=host.NemoNativeObjectProject,session=module&&module.desktop(host);
    if(!session||!session.blocksLegacy())return null;
    if(!bindings.has(host))bindings.set(host,create(session));
    return bindings.get(host);
  }
  return Object.freeze({create:create,desktop:desktop,blockFromToolArgs:blockFromToolArgs,fontFromLegacyKey:fontFromLegacyKey,stableTarget:stableTarget});
}));
