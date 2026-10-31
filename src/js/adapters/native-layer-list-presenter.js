/* Native layer-list presenter: the CS5 (#2347) pilot surface mounted through the presenter seam.
 * Owner: Ilya/Claude clean-slate session. API: render(listNode, context) -> {rows}.
 * Builds one row per native read-model layer (pick, visibility and two policy-flag buttons);
 * it commits nothing and owns no state. The caller (native-object-ui.js) supplies the
 * read model, selection, shy mode, policy owner and the click handlers, and replaces the
 * visible rows itself after every surface is built.
 * Fixture: tests/native-layer-list-presenter.test.cjs (pre-seam DOM characterization).
 * #2371 U11: a row also carries the baseline (1065e9d8 renderLayerList) decorations for the facts the native read model
 * stores: folder chevron, colour dot, kind icon (layer-kind.js), component/matte/blend/opacity badges and folder indent.
 * A decoration is DISPLAY ONLY (no native write route exists for it), is marked data-native-layer-decor so the caller can
 * tell it from the four controls, and a fact the row does not carry adds no node (absent differs from a default).
 * Rows stay one per layer in read-model order: the frame grid reserves one row per layer, so no extra header row is added.
 */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeLayerListPresenter=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function text(doc,tag,value){var el=doc.createElement(tag);el.textContent=value;return el;}

  var BADGE='<span style="font-size:9px;line-height:1;font-weight:700">';
  // layer-kind.js keyOf reads layer flags; map the native kind onto the flag it tests so the icon set stays that file's.
  var KIND_FLAGS={component:{symbolId:'x'},effect:{isEffectLayer:true},folder:{isFolderLayer:true},'null':{isNullLayer:true},widget:{isWidgetLayer:true},
    effector:{isEffectorLayer:true},text:{isTextLayer:true},image:{footage:{kind:'image'}},sequence:{footage:{kind:'sequence'}},video:{nativeVideo:true},
    camera:{isCameraLayer:true},guide:{isGuideLayer:true},montage:{montageId:'x'},draw:{}};
  var BADGES={component:['Component — double-click to edit','<span style="font-size:11px;line-height:1;font-weight:700">C</span>'],
    effect:['Calque d’effet — appliqué à tout ce qui est en dessous',BADGE+'FX</span>'],
    'null':['Null layer','<span style="font-size:11px;line-height:1;font-weight:700">✛</span>'],
    text:['Text layer','<span style="font-size:11px;line-height:1;font-weight:700">T</span>'],montage:['Montage layer',BADGE+'MT</span>'],
    guide:['Guide layer','<span style="font-size:11px;line-height:1;font-weight:700">┆</span>']};
  function decor(doc,tag,className,title){var el=text(doc,tag,'');el.className=className;el.dataset.nativeLayerDecor='true';el.style.cursor='default';if(title)el.title=title;return el;}
  function table(info,key,id,field){var rows=info[key];if(!Array.isArray(rows))return null;for(var i=0;i<rows.length;i++)if(rows[i]&&rows[i][field]===id)return rows[i];return null;}
  function nameOf(info,uid){var l=table(info,'layers',uid,'layerUid');return l?(l.name===undefined?l.layerUid:l.name):uid;}
  // Folder depth (baseline folderLayerChildMap): a layer whose parentLayerUid is a kind:"folder" layer is nested one level;
  // a folder layer itself is never absorbed as a child, so nesting is at most one deep.
  function depthOf(info,layer){
    if(layer.kind==='folder'||typeof layer.parentLayerUid!=='string')return 0;
    var parent=table(info,'layers',layer.parentLayerUid,'layerUid');return parent&&parent.kind==='folder'?1:0;
  }
  function percent(v){return String(Math.round(v*100)/100)+'%';}
  // Returns {before:[...nodes ahead of the name], after:[...badges after it]}; rows with no such fact get none.
  function decorate(doc,layer,context,node,slots,i){
    var info=context.info,kinds=context.layerKind,before=[],after=[],depth=depthOf(info,layer),folder=null;
    if(layer.kind==='folder'){
      var arrow=decor(doc,'div','lico larrow',(layer.folderCollapsed?'Folder collapsed':'Folder expanded')+' — display only, native folder collapse is unavailable');
      arrow.textContent=layer.folderCollapsed?'▸':'▾';arrow.dataset.unavailable='true';before.push(arrow);
    }else if(slots.spacer)before.push(decor(doc,'div','lico larrow-spacer'));
    var colour=typeof layer.color==='string'&&/^#[0-9a-fA-F]{3,8}$/.test(layer.color)?layer.color:slots.dots?'#8b8b9e':null;
    if(colour){
      var dot=decor(doc,'div','lico layer-color-dot','Layer colour');
      if(dot.style&&typeof dot.style.setProperty==='function')dot.style.setProperty('--dot-color',colour);else dot.style['--dot-color']=colour;
      before.push(dot);
    }
    if(kinds&&layer.kind&&KIND_FLAGS[layer.kind]&&layer.kind!=='draw'){
      var kind=kinds.of(Object.assign({},KIND_FLAGS[layer.kind],layer.kind==='folder'?{folderCollapsed:layer.folderCollapsed===true}:{}));
      var kb=decor(doc,'div','lkind lkind-'+kind.key,kind.label);kb.innerHTML=kind.icon;before.push(kb);
    }
    var badge=layer.kind&&BADGES[layer.kind];
    if(badge){var cb=decor(doc,'div','lico comp-badge',badge[0]);cb.innerHTML=badge[1];after.push(cb);}
    if(layer.matteMode||layer.isMatteSource===true){
      var label=context.matteLabel?context.matteLabel(layer.matteMode):layer.matteMode,more=Array.isArray(layer.mattesMore)?layer.mattesMore.length:0;
      var mb=decor(doc,'div','lico comp-badge'+(layer.isMatteSource===true&&!layer.matteMode?' off':''),layer.matteMode?
        'Matte: '+label+(layer.matteSourceLayerUid?' — source: '+nameOf(info,layer.matteSourceLayerUid):'')+(more?' (+'+more+')':''):'Source de matte pour un autre calque');
      mb.innerHTML=BADGE+(layer.matteMode?'M':'M▲')+'</span>';after.push(mb);
    }
    if(layer.blendMode&&layer.blendMode!=='normal'){
      var names=context.blendLabels,bb=decor(doc,'div','lico comp-badge native-layer-blend','Blend: '+((names&&names[layer.blendMode])||layer.blendMode));
      bb.innerHTML=BADGE+'Bl</span>';after.push(bb);
    }
    if(typeof layer.opacity==='number'&&(layer.opacity!==100||layer.opacityAnimated===true)||layer.opacityAnimated===true){
      var shown=typeof layer.opacity==='number'?percent(layer.opacity):'%~';
      var ob=decor(doc,'div','lico comp-badge native-layer-opacity','Opacity: '+shown+(layer.opacityAnimated===true?' (animated — static value shown)':''));
      ob.innerHTML=BADGE+shown+'</span>';ob.style.width='auto';after.push(ob);
    }
    // The baseline Parent cell (.lparent) comes from the caller's hook: it owns the Parent A edit, this file commits nothing.
    if(typeof context.parentCell==='function'){var pc=context.parentCell(layer,i,node);if(pc){pc.style.order='1';after.push(pc);}}
    if(layer.kind==='component')node.className+=' is-comp';
    if(depth>0||typeof layer.folderId==='string')node.className+=' in-folder';
    if(typeof layer.folderId==='string'){
      folder=table(info,'layerFolders',layer.folderId,'folderId');node.dataset.folderId=layer.folderId;
      node.title='Folder: '+((folder&&folder.name)||layer.folderId)+(folder&&folder.collapsed?' (collapsed — display only)':'');
    }
    return {before:before,after:after};
  }
  function row(doc,layer,i,context,slots){
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
    // #2256: a double-click opens the component an instance layer instances (the baseline row dblclick); other rows ignore it.
    if(typeof context.onOpen==='function')node.addEventListener('dblclick',function(){return context.onOpen(layer,node);});
    node.hidden=context.shyMode&&layer.shy===true;node.style.display=node.hidden?'none':'flex';
    var extra=decorate(doc,layer,context,node,slots,i);
    // The four controls stay the row's first four children (every consumer addresses them by position); the decorations follow
    // in DOM order and are placed around them with flex `order` (baseline order: arrow, dot, kind, name, badges, eye, lock, shy).
    var controls=[pick,eye,flags[0],flags[1]],decorated=extra.before.length+extra.after.length>0;
    if(decorated){
      extra.before.forEach(function(n,k){n.style.order=String(k-extra.before.length);});pick.style.order='0';
      extra.after.forEach(function(n){n.style.order='1';});[eye,flags[0],flags[1]].forEach(function(n){n.style.order='2';});
    }
    node.replaceChildren.apply(node,controls.concat(extra.before,extra.after));
    return node;
  }
  function render(list,context){
    if(!context||!context.info||!Array.isArray(context.info.layers)||!context.document)throw new TypeError('Layer-list context is incomplete');
    // Slots are shared down the list like the baseline's (icons must not shift by row): the folder-arrow slot once any layer is a
    // folder, and the colour dot (neutral grey where a layer has no label) once any layer carries one.
    var layers=context.info.layers,slots={spacer:layers.some(function(l){return l.kind==='folder';}),dots:layers.some(function(l){return typeof l.color==='string';})};
    return {rows:layers.map(function(layer,i){return row(context.document,layer,i,context,slots);})};
  }
  // Mounts this default presenter unless a replacement registered first, then presents; rows must map one to one
  // onto the read-model layers. A row is an element: its children are a DOM HTMLCollection, not an Array.
  function element(row){
    var kids=row&&typeof row==='object'?row.children:null;
    return !!kids&&typeof kids==='object'&&typeof kids.length==='number';
  }
  function mountRows(root,seam,context){
    if(!seam.isMounted(root,'layer-list'))seam.register(root,'layer-list',api);
    var result=seam.present(root,'layer-list',context),rows=result&&result.rows;
    if(!Array.isArray(rows)||rows.length!==context.info.layers.length||!rows.every(element))
      throw new Error('Native layer-list presenter returned rows that do not match the read model');
    return rows;
  }
  var api=Object.freeze({render:render,mountRows:mountRows});
  return api;
}));
