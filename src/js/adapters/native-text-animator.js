/* Native animator intentions. Drafts are detached; only the sealed item owner writes records. */
(function(root,factory){var C=typeof module==='object'&&module.exports?require('./native-object-item-command.js'):root.NemoNativeObjectItemCommand;
  var I=typeof module==='object'&&module.exports?require('./native-object-item-contract.js'):root.NemoNativeObjectItemContract;
  var R=typeof module==='object'&&module.exports?require('./native-text-read.js'):root.NemoNativeTextRead;
  var api=factory(C,I,R);if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeTextAnimator=api;
}(typeof globalThis==='object'?globalThis:this,function(Command,Item,Read){
  'use strict';
  function fault(code,message){var e=new Error(message);e.code=code;return e;}
  function clone(v){return JSON.parse(JSON.stringify(v));}
  function same(a,b){return Read.samePin(a,b);}
  function create(session,context){
    var reader=Read.create(session),command=Command.create(session,'item'),pending=null,busy=false;
    function current(expected){var c=context(),p=session.info();if(!session.isActive()||!c||!c.layerUid||!same(c.pin,p)||expected&&(!same(expected.pin,p)||expected.layerUid!==c.layerUid||expected.frame!==c.frame||expected.selectionKey!==c.selectionKey))throw fault('stale_revision','Text animator intent is stale');return clone(c);}
    async function load(){var c=current(),value=await reader.readAnimators(c.layerUid,c.frame,c.pin);current(c);var ids=new Set();value.animators.forEach(function(r){Item.animator(r);if(ids.has(r.id))throw fault("uncorrelated","Duplicate native animator identity");ids.add(r.id);});value.selectors.forEach(function(s){Item.animator({id:"native-view",selector:s});});return {context:c,animators:clone(value.animators),selectors:clone(value.selectors),hasText:value.hasText};}
    async function write(payload,origin){
      if(busy||pending)throw fault('unavailable','Resolve the previous animator command first');current(origin);busy=true;
      try{return await command.execute(Item.ITEM,function(){current(origin);return clone(payload);});}
      catch(e){if(e&&e.code==='item_command_indeterminate')pending=e;throw e;}finally{busy=false;}
    }
    return Object.freeze({load:load,
      upsert:function(record,origin){Item.animator(record);return write({command:'object.text.animator.upsert',layerUid:origin.layerUid,animator:clone(record)},origin);},
      remove:function(id,origin){return write({command:'object.text.animator.remove',layerUid:origin.layerUid,animatorId:id},origin);},
      resolve:async function(){if(!pending||busy)throw fault('unavailable','No pending animator receipt');busy=true;try{var receipt=await pending.replay();pending=null;return receipt;}finally{busy=false;}},
      state:function(){return {busy:busy,pending:!!pending};}});
  }
  return Object.freeze({create:create});
}));
