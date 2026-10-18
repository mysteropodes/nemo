/* Ordinary Text intent orchestration. Only selection/draft/request state lives
 * here; immutable native text reads and sealed native item commands own facts. */
(function(root,factory){
  var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeTextTool=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function fault(code,message){var e=new Error(message);e.code=code;return e;}
  function clone(v){return JSON.parse(JSON.stringify(v));}
  function freeze(v){if(v&&typeof v==='object'){Object.keys(v).forEach(function(k){freeze(v[k]);});Object.freeze(v);}return v;}
  function owner(a,b){return a&&b&&['instanceId','documentId','lifecycleGeneration'].every(function(k){return a[k]===b[k];});}
  function same(a,b){return owner(a,b)&&a.contentRevision===b.contentRevision&&a.documentSnapshotId===b.documentSnapshotId;}
  function equal(a,b){return JSON.stringify(a)===JSON.stringify(b);}
  function create(ports){
    if(!ports||!ports.session||typeof ports.context!=='function'||!ports.reader||typeof ports.reader.create!=='function'||!ports.writer||typeof ports.writer.create!=='function')throw new Error('Native Text ports unavailable');
    var Reader=ports.reader,Writer=ports.writer;
    var session=ports.session,reader=Reader.create(session),draft=null,busy=false,discovering=false,unknown=null,serial=0,retired=false;
    function context(presented){var c=ports.context(presented!==false);
      if(retired||!c||!c.pin||!session.isActive()||!same(c.pin,session.info())||typeof c.layerUid!=='string'||!c.layerUid||!Number.isInteger(c.frame)||c.frame<0)throw fault('unavailable','Native Text context unavailable');return c;}
    function current(c,revision,presented){try{var n=context(presented);return owner(c.pin,n.pin)&&(!revision||same(c.pin,n.pin))&&c.layerUid===n.layerUid&&c.frame===n.frame&&c.selectionKey===n.selectionKey&&c.viewKey===n.viewKey;}catch(_){return false;}}
    function notify(){if(ports.state)ports.state({busy:busy,discovering:discovering,unknown:!!unknown,draft:!!draft});}
    function resetOwner(){if(unknown){var p=session.info();if(!owner(unknown.pin,p)){unknown=null;draft=null;serial++;}}}
    function requireWrite(){resetOwner();if(retired||busy||unknown)throw fault(unknown?'item_command_indeterminate':'unavailable',unknown?'Resolve the previous text command or reopen the document.':'Text command is busy or retired.');}
    function scope(c){return {contextId:'scene-root',layerUid:c.layerUid,frameScope:{kind:'authored',frame:c.frame}};}
    async function open(target,placement){
      requireWrite();var c=clone(context(true)),ticket=++serial;busy=true;discovering=true;notify();
      try{
        var value;
        if(target){value=await reader.readBlock(clone(target),c.pin);
          if(value.target.contextId!=='scene-root'||value.target.layerUid!==c.layerUid||value.target.frameScope.kind!=='authored'||value.target.frameScope.frame!==c.frame)throw fault('unavailable','Select text in the current authored frame.');
        }else{
          // A valid empty native target page proves the actual read provider exists.
          await reader.listPage(scope(c),0,1,c.pin);
          if(!placement||!placement.anchorTopLeft||![placement.anchorTopLeft.x,placement.anchorTopLeft.y].every(Number.isFinite)||placement.fixedWidth!==null&&!(Number.isFinite(placement.fixedWidth)&&placement.fixedWidth>0))throw fault('invalid_request','Text placement invalid');
          value={target:null,block:{text:'',font:{kind:'bundled',fontKey:'Roboto-Regular'},size:48,color:ports.defaultColor?ports.defaultColor():'#000000',align:'left',anchorTopLeft:clone(placement.anchorTopLeft),fixedWidth:placement.fixedWidth}};
        }
        if(ticket!==serial||!current(c,true,true))throw fault('stale_revision','Text selection changed during discovery');
        draft={ticket:ticket,context:c,target:value.target,block:value.block,placement:placement&&clone(placement)};
        if(ports.draft)ports.draft(freeze(clone(draft)));return freeze(clone(draft));
      }finally{if(ticket===serial){discovering=false;busy=false;notify();}}
    }
    async function page(offset,limit){var c=clone(context(true)),value=await reader.listPage(scope(c),offset,limit,c.pin);
      if(!current(c,true,true))throw fault('stale_revision','Text target page is stale');return freeze({context:c,page:value});}
    async function commit(changes){
      requireWrite();if(!draft)throw fault('unavailable','No native text draft');
      var d=draft;if(!current(d.context,true,true))throw fault('stale_revision','Text draft is stale');
      if(!changes||typeof changes!=='object'||Array.isArray(changes))throw fault('invalid_request','Text changes invalid');
      var keys=['text','font','size','color','align','fixedWidth','fixedHeight','bold','italic','underline','strike','letterSpacing','wordSpacing','lineHeightMult','textCase'];
      if(!Object.keys(changes).every(function(k){return keys.includes(k);}))throw fault('invalid_request','Text change fields invalid');
      var patch=Object.create(null);Object.keys(changes).forEach(function(k){if(!equal(changes[k],d.block[k]))patch[k]=clone(changes[k]);});
      if(d.target&&!Object.keys(patch).length){cancel();return {applied:false,skipped:'unchanged'};}
      var block=Object.assign({},d.block,patch);if(typeof block.text!=='string'||!block.text.trim()){cancel();return {applied:false,skipped:'blank_text'};}
      busy=true;notify();
      try{
        var writer=Writer.create(session),guard=function(){return current(d.context,true,false);},receipt=d.target?await writer.editBlock(d.target,patch,d.context.pin,guard):await writer.createBlock(d.context.layerUid,d.context.frame,block,d.context.pin,guard);
        if(current(d.context,false,true)){draft=null;serial++;if(ports.committed)ports.committed(receipt);}
        return receipt;
      }catch(error){if(error&&error.code==='item_command_indeterminate'){unknown={pin:clone(d.context.pin),replay:error.replay};}throw error;}
      finally{busy=false;notify();}
    }
    async function resolve(){resetOwner();if(!unknown||busy||typeof unknown.replay!=='function')throw fault('unavailable','No recoverable pending text receipt');
      var held=unknown;if(!owner(held.pin,session.info()))throw fault('stale_owner','Text owner changed');busy=true;notify();
      try{var receipt=await held.replay();if(owner(held.pin,session.info())){unknown=null;draft=null;serial++;if(ports.committed)ports.committed(receipt);}return receipt;}
      finally{busy=false;notify();}}
    function cancel(){
      if(unknown||busy&&!discovering)return false;
      draft=null;serial++;if(discovering){discovering=false;busy=false;}
      if(ports.cancelled)ports.cancelled();notify();return true;
    }
    return Object.freeze({page:page,openNew:function(p){return open(null,p);},openExisting:function(t){return open(t,null);},commit:commit,resolve:resolve,cancel:cancel,
      snapshot:function(){resetOwner();return freeze({busy:busy,discovering:discovering,unknown:!!unknown,draft:draft&&clone(draft)});},retire:function(){retired=true;draft=null;serial++;if(discovering){discovering=false;busy=false;}notify();}});
  }
  return Object.freeze({create:create,samePin:same});
}));
