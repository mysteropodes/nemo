/* Native layer-list presenter: the CS5 (#2347) pilot surface mounted through the presenter seam.
 * Owner: Ilya/Claude clean-slate session. API: render(listNode, context) -> {rows}.
 * Builds one row per native read-model layer (pick, visibility and two policy-flag buttons);
 * it commits nothing and owns no state. The caller (native-object-ui.js) supplies the
 * read model, selection, shy mode, policy owner and the click handlers, and replaces the
 * visible rows itself after every surface is built.
 * Fixture: tests/native-layer-list-presenter.test.cjs (pre-seam DOM characterization).
 */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeLayerListPresenter=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function text(doc,tag,value){var el=doc.createElement(tag);el.textContent=value;return el;}
  function row(doc,layer,i,context){
    var label=layer.name===undefined?layer.layerUid:layer.name;
    var node=text(doc,'div',''),pick=text(doc,'button',label+' — '+(layer.visible===false?'Hidden':'Visible'));
    node.className='native-object-layer';node.dataset.layerUid=layer.layerUid;node.style.display='flex';
    pick.type='button';pick.style.flex='1';pick.style.textAlign='left';
    pick.setAttribute('aria-pressed',String(context.selection.indexOf(i)>=0));
    pick.setAttribute('aria-label',label+' ('+layer.layerUid+')');
    pick.addEventListener('click',function(event){return context.onPick(i,layer,event,node);});
    var eye=text(doc,'button',layer.visible===false?'Show':'Hide');eye.type='button';
    eye.setAttribute('aria-label',(layer.visible===false?'Show ':'Hide ')+label+' ('+layer.layerUid+')');
    // Availability is checked at CLICK time: this render can precede preview/ACK.
    eye.addEventListener('click',function(){return context.onVisibility(layer);});
    var flags=['locked','shy'].map(function(flag){
      var on=layer[flag]===true,caption=flag==='locked'?(on?'Unlock':'Lock'):(on?'Unmark shy':'Mark shy');
      var button=text(doc,'button',caption);button.type='button';button.dataset.policyFlag=flag;
      button.setAttribute('aria-pressed',String(on));button.setAttribute('aria-label',caption+' '+(layer.name===undefined?layer.layerUid:layer.name));
      button.disabled=!context.policyOwner;
      button.addEventListener('click',function(){return context.onPolicy(layer,flag,!on,node);});
      return button;
    });
    node.hidden=context.shyMode&&layer.shy===true;node.style.display=node.hidden?'none':'flex';
    node.replaceChildren(pick,eye,flags[0],flags[1]);
    return node;
  }
  function render(list,context){
    if(!context||!context.info||!Array.isArray(context.info.layers)||!context.document)throw new TypeError('Layer-list context is incomplete');
    return {rows:context.info.layers.map(function(layer,i){return row(context.document,layer,i,context);})};
  }
  // Mounts this default presenter unless a replacement registered first, then presents; rows must map one to one
  // onto the read-model layers.
  function mountRows(root,seam,context){
    if(!seam.isMounted(root,'layer-list'))seam.register(root,'layer-list',api);
    var result=seam.present(root,'layer-list',context),rows=result&&result.rows;
    if(!Array.isArray(rows)||rows.length!==context.info.layers.length||
      !rows.every(function(row){return row&&typeof row==='object'&&Array.isArray(row.children);}))
      throw new Error('Native layer-list presenter returned rows that do not match the read model');
    return rows;
  }
  var api=Object.freeze({render:render,mountRows:mountRows});
  return api;
}));
