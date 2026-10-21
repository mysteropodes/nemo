/* Composition of the ordinary Text canvas/editor controls with injected current
 * native sessions. No provider lookup, Paper hit inference or persistent mirror. */
(function(root,factory){
  var Tool=typeof module==='object'&&module.exports?require('./native-text-tool.js'):root.NemoNativeTextTool;
  var Item=typeof module==='object'&&module.exports?require('./native-object-item-contract.js'):root.NemoNativeObjectItemContract;
  var Reader=typeof module==='object'&&module.exports?require('./native-text-read.js'):root.NemoNativeTextRead;
  var Writer=typeof module==='object'&&module.exports?require('./native-object-text-layout.js'):root.NemoNativeObjectTextLayout;
  var api=factory(Tool,Item,Reader,Writer);if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeTextToolDesktop=api;
}(typeof globalThis==='object'?globalThis:this,function(Tool,Item,Reader,Writer){
  'use strict';var bindings=new WeakMap(),traceBudget=64;
  // Temporary local-candidate diagnostic: eager scalars, no Event retention or content.
  function trace(root,decision,method,sampled){
    try{
      if(traceBudget<=0||!root.console||typeof root.console.info!=='function')return;
      traceBudget--;
      var row={decision:decision,method:method,observedAt:Date.now()};
      if(sampled)Object.keys(sampled).forEach(function(k){var v=sampled[k];if(v===null||['string','number','boolean'].includes(typeof v))row[k]=v;});
      root.console.info('[nemo-native-text-input] '+JSON.stringify(row));
    }catch(_){} // No input reads, formatting or sink failure may escape diagnostics.
  }
  function consume(e){if(e.stopImmediatePropagation)e.stopImmediatePropagation();if(e.preventDefault)e.preventDefault();}
  function bounds(root,pin){var doc=root.document,el=doc.getElementById('drawing-canvas'),r=el&&el.getBoundingClientRect(),html=doc.documentElement;
      if(!r||![r.left,r.top,r.width,r.height,pin.canvasW,pin.canvasH].every(Number.isFinite)||r.width<=0||r.height<=0||pin.canvasW<=0||pin.canvasH<=0||r.left<0||r.top<0||r.left+r.width>html.clientWidth||r.top+r.height>html.clientHeight||!html.classList.contains('nemo-native-underlay'))return null;
      var edges=[r.left,r.top,r.left+r.width,r.top+r.height];
      if(typeof html.style.getPropertyValue!=='function'||!['left','top','right','bottom'].every(function(k,i){return parseFloat(html.style.getPropertyValue('--nemo-native-'+k))===edges[i];}))return null;
      return {left:r.left,top:r.top,width:r.width,height:r.height};}
    function point(e,c){return {x:(e.clientX-c.bounds.left)*c.canvasW/c.bounds.width,y:(e.clientY-c.bounds.top)*c.canvasH/c.bounds.height};}
    function screen(p,c){return {x:c.bounds.left+p.x*c.bounds.width/c.canvasW,y:c.bounds.top+p.y*c.bounds.height/c.canvasH};}
  function mount(root,container,ports){
    var old=bindings.get(root);if(old&&old.container===container&&!old.retired){old.ports=ports;old.refresh();return old;}
    if(old)old.retire();
    var doc=root.document,session=ports.currentSession();if(!session||!Tool||!Item||!Reader||typeof Reader.create!=='function'||!Writer||typeof Writer.create!=='function')return null;
    var b={container:container,ports:ports,retired:false},drag=null,discovery=null,lastTool=root.state&&root.state.tool,area=null,fontChoices=[],fontTicket=0,readyPin=null,shown=null,dirty=new Set(),modelState={busy:false,unknown:false},page=null,refreshing=false;
    var perspectiveRefreshNeeded=false,perspectiveRefreshScheduled=false,removePerspectiveRead=null;
    function flushPerspectiveRefresh(){
      if(!ownsUI()||!perspectiveRefreshNeeded||perspectiveRefreshScheduled||modelState.busy||modelState.unknown||refreshing)return;
      perspectiveRefreshScheduled=true;
      Promise.resolve().then(function(){
        perspectiveRefreshScheduled=false;
        if(!ownsUI()||!perspectiveRefreshNeeded||modelState.busy||modelState.unknown||refreshing)return;
        perspectiveRefreshNeeded=false;b.refresh();
      });
    }
    function perspectiveReadChanged(){perspectiveRefreshNeeded=true;flushPerspectiveRefresh();}
    function node(tag,text){var n=doc.createElement(tag);if(text!==undefined)n.textContent=text;return n;}
    var section=node('section'),title=node('div','Text'),status=node('div','Refresh to select native text in the current authored frame.'),select=node('select'),refresh=node('button','Refresh text'),prev=node('button','Previous'),next=node('button','Next'),resolve=node('button','Resolve pending text');
    section.id='native-text-controls';title.className='phdr';select.setAttribute('aria-label','Native text block');status.setAttribute('role','status');
    [title,status,select,refresh,prev,next,resolve].forEach(function(n){section.appendChild(n);});container.appendChild(section);
    [refresh,prev,next,resolve].forEach(function(n){n.type='button';});resolve.hidden=true;
    var pop=doc.getElementById('text-popover'),input=doc.getElementById('text-input'),font=doc.getElementById('text-font'),apply=doc.getElementById('text-apply'),cancel=doc.getElementById('text-cancel');
    var fields={text:input,size:doc.getElementById('text-size'),font:font,color:doc.getElementById('text-color'),align:doc.getElementById('text-align')};
    var colorHex=node('input'),colorHexLabel=node('label','Hex'),invalidHex=false,colorParent=fields.color&&fields.color.parentNode,colorParentWrap=colorParent&&colorParent.style.flexWrap;
    colorHex.id='native-text-color-hex';colorHex.type='text';colorHex.className='pi';colorHex.maxLength=7;colorHex.placeholder='#rrggbb';colorHex.autocomplete='off';colorHex.spellcheck=false;
    colorHex.setAttribute('aria-label','Text color hex');colorHex.setAttribute('pattern','#[0-9a-fA-F]{6}');colorHex.setAttribute('aria-invalid','false');
    Object.assign(colorHex.style,{width:'74px',minWidth:'0',fontFamily:'monospace'});Object.assign(colorHexLabel.style,{display:'inline-flex',alignItems:'center',gap:'4px',flexBasis:'100%'});
    colorHexLabel.appendChild(colorHex);if(pop&&colorParent){colorParent.style.flexWrap='wrap';colorParent.appendChild(colorHexLabel);}
    var savedFonts=font?Array.from(font.children):[],savedFontValue=font&&font.value,events=[];
    function on(n,name,fn,capture){if(!n)return;n.addEventListener(name,fn,!!capture);events.push([n,name,fn,!!capture]);}
    function ownsUI(){return !b.retired&&bindings.get(root)===b;}
    function notify(message){if(ownsUI())status.textContent=message;}
    var perspectiveProof=null;
    function context(presented){
      var current=b.ports.currentSession(),s=root.state,info=current&&current.info();
      var port=root.NemoNativePerspectiveController,perspective=port&&(presented?port.read():typeof port.readForCommand==='function'&&port.readForCommand(perspectiveProof));
      if(b.retired||current!==session||!current.isActive()||!info||!s||s.appMode!=='anim2d'||s.playing||s.spaceDown||s.isPanning||s.activeSymbolId||s.shadowMode||s.maskMode||s.symmetryEnabled||!perspective||!Tool.samePin(perspective.info,info)||perspective.perspective.settings.perspectiveEnabled)return null;
      if(presented)perspectiveProof=perspective;
      var layer=info.layers[s.activeLayerIdx],frame=s.currentFrame,rect=bounds(root,info);
      if(!rect||!layer||layer.locked||!Number.isInteger(frame)||frame<0||frame>=info.totalFrames)return null;
      if(presented){var p=current.readPresentedObjectSession();if(!p||!Tool.samePin(p,info)||!p.presentation||p.presentation.frame!==frame)return null;}
      return {pin:{instanceId:info.instanceId,documentId:info.documentId,lifecycleGeneration:info.lifecycleGeneration,contentRevision:info.contentRevision,documentSnapshotId:info.documentSnapshotId},layerUid:layer.layerUid,frame:frame,
        selectionKey:JSON.stringify([s.activeLayerIdx,root._layerSel||[],s.tool]),viewKey:JSON.stringify(rect),bounds:rect,canvasW:info.canvasW,canvasH:info.canvasH};
    }
    function controls(){if(!ownsUI())return;var c=context(true),blocked=modelState.busy||modelState.unknown;
      if(apply)apply.disabled=blocked||!shown||!c||invalidHex;colorHex.disabled=blocked||!shown||!c;select.disabled=blocked||!page;refresh.disabled=b.retired||refreshing||modelState.busy;
      prev.disabled=blocked||!page||page.page.offset===0;next.disabled=blocked||!page||page.page.nextOffset===null;resolve.hidden=!modelState.unknown;resolve.disabled=modelState.busy;}
    function validHex(value){return /^#[0-9a-fA-F]{6}$/.test(value);}
    function hexValidity(){invalidHex=!validHex(colorHex.value);colorHex.setAttribute('aria-invalid',String(invalidHex));return !invalidHex;}
    function syncHex(){colorHex.value=fields.color.value;hexValidity();}
    function hide(){fontTicket++;shown=null;dirty.clear();invalidHex=false;colorHex.setAttribute('aria-invalid','false');if(!ownsUI())return;
      if(area&&area.parentNode)area.parentNode.removeChild(area);area=null;if(pop)pop.style.display='none';controls();}
    function chosenFont(value){var i=Number(value);return Number.isInteger(i)&&fontChoices[i]?fontChoices[i].font:null;}
    function fillFonts(rows,d,ticket){
      if(!ownsUI()||shown!==d||ticket!==fontTicket)return;
      var wanted=dirty.has('font')?chosenFont(font.value):d.block.font;wanted=wanted||d.block.font;
      var choices=Item.FONTS.map(function(key){return {name:key,font:{kind:'bundled',fontKey:key}};});
      rows.filter(function(r){return r&&r.available===true&&typeof r.assetId==='string';}).forEach(function(r){choices.push({name:r.name+' — '+r.assetId,font:{kind:'asset',assetId:r.assetId}});});
      var selected=choices.findIndex(function(c){return c.font.kind===wanted.kind&&(c.font.kind==='asset'?c.font.assetId===wanted.assetId:c.font.fontKey===wanted.fontKey);});
      if(selected<0){choices.push({name:'Unavailable font: '+(wanted.assetId||wanted.fontKey),font:wanted,unavailable:true});selected=choices.length-1;}
      fontChoices=choices;font.replaceChildren();choices.forEach(function(c,i){var o=node('option',c.name);o.value=String(i);o.disabled=!!c.unavailable;font.appendChild(o);});font.value=String(selected);controls();
    }
    function show(d){
      if(!ownsUI()){trace(root,'show.reject.owner',null,null);return;}
      if(!pop||Object.values(fields).some(function(n){return !n;})){trace(root,'show.reject.controls',null,null);notify('Text editor controls are unavailable.');return;}
      shown=d;dirty.clear();fields.text.value=d.block.text;fields.size.value=String(d.block.size);fields.color.value=d.block.color.slice(0,7);syncHex();fields.align.value=d.block.align;
      var c=context(true);if(!c){trace(root,'show.reject.context',null,null);return;}var at=screen(d.block.anchorTopLeft,c);
      pop.style.display='block';pop.style.left=Math.max(4,Math.min(doc.documentElement.clientWidth-240,at.x+10))+'px';pop.style.top=Math.max(4,Math.min(doc.documentElement.clientHeight-220,at.y+10))+'px';
      if(area&&area.parentNode)area.parentNode.removeChild(area);area=null;
      if(!d.target&&d.placement.fixedWidth!==null){
        area=node('textarea');area.id='native-text-area-draft';area.setAttribute('aria-label','Text');area.value=d.block.text;
        Object.assign(area.style,{position:'fixed',zIndex:'201',left:at.x+'px',top:at.y+'px',width:(d.placement.fixedWidth*c.bounds.width/c.canvasW)+'px',minHeight:Math.max(24,d.block.size*1.25*c.bounds.height/c.canvasH)+'px',fontSize:(d.block.size*c.bounds.height/c.canvasH)+'px',color:d.block.color,background:'transparent'});
        area.addEventListener('input',function(){dirty.add('text');input.value=area.value;});area.addEventListener('keydown',key);
        area.addEventListener('blur',function(e){if(!e.relatedTarget||!pop.contains(e.relatedTarget))commit();});doc.body.appendChild(area);area.focus();
      }else input.focus();
      var ticket=++fontTicket;fillFonts([],d,ticket);
      var resources=b.ports.fontResources&&b.ports.fontResources();
      if(resources&&typeof resources.list==='function')Promise.resolve(resources.list()).then(function(rows){var now=context(true);if(now&&Tool.samePin(now.pin,d.context.pin))fillFonts(rows,d,ticket);}).catch(function(){if(shown===d)notify('Document font resources are unavailable.');});
      controls();trace(root,'show.visible',null,{area:!!area});
    }
    var core=Tool.create({session:session,reader:Reader,writer:Writer,context:context,defaultColor:function(){var color=root.state.strokeColor||'#000000';return /^#[0-9a-fA-F]{3}$/.test(color)?'#'+color.slice(1).split('').map(function(x){return x+x;}).join(''):color;},
      draft:show,state:function(s){modelState=s;controls();flushPerspectiveRefresh();},cancelled:hide,committed:function(){hide();notify('Text updated.');b.refresh();}});
    async function commit(){if(!ownsUI()||!shown||modelState.busy||modelState.unknown)return;
      if(!hexValidity()){notify('Enter a color as #rrggbb.');controls();return;}var changes={};
      dirty.forEach(function(k){changes[k]=k==='font'?chosenFont(font.value):k==='size'?Number(fields.size.value):k==='text'&&area?area.value:fields[k].value;
        if(k==='color'&&shown.block.color.length===9)changes[k]+=shown.block.color.slice(7);});
      if(!shown.target)changes.text=area?area.value:input.value;
      try{await core.commit(changes);}catch(error){notify(error.message||'Text command failed.');}controls();}
    function key(e){if(e.key==='Escape'){consume(e);core.cancel();}else if(e.key==='Enter'&&(e.metaKey||e.ctrlKey)){consume(e);commit();}}
    Object.keys(fields).forEach(function(k){on(fields[k],'input',function(){if(shown){dirty.add(k);if(k==='color'){syncHex();controls();}if(k==='text'&&area)area.value=input.value;}},true);on(fields[k],'change',function(){if(shown){dirty.add(k);if(k==='color'){syncHex();controls();}}},true);});
    function hexInput(){if(!ownsUI()||!shown||modelState.busy||modelState.unknown)return;
      dirty.add('color');if(hexValidity())fields.color.value=colorHex.value.toLowerCase();controls();}
    on(colorHex,'input',hexInput,true);on(colorHex,'change',hexInput,true);on(colorHex,'keydown',key,true);
    on(input,'keydown',key,true);on(apply,'click',function(e){consume(e);if(shown)commit();},true);on(cancel,'click',function(e){consume(e);core.cancel();},true);
    function cancelIntent(){trace(root,'cancel.intent',null);drag=null;discovery=null;core.cancel();}
    function syncTool(){var tool=root.state&&root.state.tool;if(tool!==lastTool){lastTool=tool;cancelIntent();}}
    on(root,'blur',cancelIntent,false);
    on(doc,'keydown',function(e){if(e.key==='Escape'&&(drag||shown||core.snapshot().discovering)){consume(e);cancelIntent();}},true);
    on(select,'change',function(){if(select.value==='')return;var index=Number(select.value),row=page&&page.page.items[index];if(row)core.openExisting(row.target).catch(function(e){notify(e.message);});});
    on(refresh,'click',function(){b.refresh(undefined,true);});on(prev,'click',function(){b.refresh(Math.max(0,page.page.offset-8));});on(next,'click',function(){b.refresh(page.page.nextOffset);});
    on(resolve,'click',function(){core.resolve().catch(function(e){notify(e.message);});});
    b.refresh=async function(offset,force){if(!ownsUI())return;
      if(root.NemoNativeTextAnimatorDesktop){if(!b.animators)b.animators=root.NemoNativeTextAnimatorDesktop.mount(root,container);else b.animators.refresh();}syncTool();if(refreshing||modelState.busy)return;var state=core.snapshot();modelState={busy:state.busy,unknown:state.unknown};if(!state.draft&&shown)hide();var c=context(true);if(!c){readyPin=null;controls();return;}
      if(!force&&offset===undefined&&readyPin&&Tool.samePin(readyPin,c.pin)&&page&&page.context.layerUid===c.layerUid&&page.context.frame===c.frame&&page.context.selectionKey===c.selectionKey){controls();return;}
      if(shown&&!Tool.samePin(shown.context.pin,c.pin)&&!modelState.unknown)core.cancel();
      refreshing=true;controls();try{var result=await core.page(offset||0,8);if(!ownsUI())return;page=result;readyPin=result.context.pin;select.replaceChildren();
        var placeholder=node('option','Select text in this authored frame');placeholder.value='';select.appendChild(placeholder);
        result.page.items.forEach(function(row,i){var option=node('option',row.target.strokeId);option.value=String(i);select.appendChild(option);});select.value='';
        notify(result.page.items.length?'Select a text block to edit.':'No authored text at this frame. Use the Text tool to place text.');
      }catch(error){readyPin=null;page=null;notify(error.message||'Native text is unavailable.');}finally{refreshing=false;controls();flushPerspectiveRefresh();}};
    b.handle=function(method,e){
      syncTool();if(b.retired||!root.state||root.state.tool!=='text')return false;
      var sampled={};function read(k){return sampled[k]=e[k];}
      if(method!=='move')trace(root,'handle.enter',method);
      if(method==='cancelEvent'){
        if(read('type')==='lostpointercapture'&&!drag)return true;
        var pending=drag||discovery;if(pending&&read('pointerId')!==undefined&&read('pointerId')!==pending.id)return true;
        consume(e);cancelIntent();return true;
      }
      var stage=e.target===doc.getElementById('drawing-canvas')||e.target===doc.getElementById('canvas-area');if(method==='down'&&!stage){trace(root,'reject.stage',method,{stage:false});cancelIntent();return true;}
      consume(e);if(!Number.isSafeInteger(read('pointerId'))||read('isPrimary')!==true){trace(root,'reject.primary',method,sampled);return true;}
      if(drag&&read('pointerId')!==drag.id){trace(root,'reject.pointer',method,sampled);return true;}
      if(read('altKey')||read('ctrlKey')||read('metaKey')||read('shiftKey')||read('button')!==0&&(method==='down'||method==='up')){trace(root,'reject.modifierOrButton',method,sampled);cancelIntent();return true;}
      if(modelState.busy||modelState.unknown){trace(root,'reject.pending',method,sampled);return true;}if(!Number.isFinite(read('clientX'))||!Number.isFinite(read('clientY'))){trace(root,'reject.coordinates',method,sampled);cancelIntent();return true;}var c=context(true);
      if(!c){trace(root,'reject.context',method,sampled);cancelIntent();notify('Native Text is unavailable.');return true;}
      if(method==='down'){var mask=read('buttons');if(mask!==0&&mask!==1){trace(root,'reject.buttons.down',method,sampled);cancelIntent();return true;}
        // A zero-mask primary edge is only a paired point click, never evidence of a held drag.
        trace(root,mask===0?'down.clickOnly':'down.accepted',method,sampled);drag={id:read('pointerId'),context:c,point:point(e,c),x:read('clientX'),y:read('clientY'),clickOnly:mask===0};}
      else if(method==='move'&&drag&&(read('buttons')!==1||drag.clickOnly)){trace(root,drag.clickOnly?'reject.clickOnly.move':'reject.buttons.move',method,sampled);cancelIntent();}
      else if(method==='up'&&drag){var held=drag;drag=null;if(read('buttons')!==0||!Tool.samePin(held.context.pin,c.pin)||held.context.selectionKey!==c.selectionKey||held.context.viewKey!==c.viewKey){trace(root,'reject.releaseOrPin',method,sampled);return true;}
        var end=point(e,c),wide=Math.abs(read('clientX')-held.x)>20,token={id:held.id};
        if(held.clickOnly&&(!stage||wide||Math.hypot(sampled.clientX-held.x,read('clientY')-held.y)>20||held.context.frame!==c.frame||held.context.layerUid!==c.layerUid)){trace(root,'reject.clickOnly.release',method,sampled);return true;}
        discovery=token;trace(root,'openNew.start',method,sampled);
        core.openNew({anchorTopLeft:wide?{x:Math.min(held.point.x,end.x),y:Math.min(held.point.y,end.y)}:held.point,fixedWidth:wide?Math.abs(end.x-held.point.x):null})
          .catch(function(error){trace(root,'openNew.error',null);if(discovery===token)notify(error.message);}).finally(function(){trace(root,'openNew.finished',null,{current:discovery===token,shown:!!shown});if(discovery===token)discovery=null;});
      }return true;};
    b.retire=function(){if(b.retired)return;perspectiveRefreshNeeded=false;if(removePerspectiveRead){removePerspectiveRead();removePerspectiveRead=null;}if(b.animators)b.animators.retire();var owned=ownsUI();hide();b.retired=true;core.retire();events.forEach(function(a){a[0].removeEventListener(a[1],a[2],a[3]);});
      if(owned&&font){font.replaceChildren.apply(font,savedFonts);font.value=savedFontValue;}if(colorHexLabel.parentNode)colorHexLabel.parentNode.removeChild(colorHexLabel);if(owned&&colorParent)colorParent.style.flexWrap=colorParentWrap;if(section.parentNode)section.parentNode.removeChild(section);};
    b.ready=function(){var c=context(true);return !!c&&!!readyPin&&Tool.samePin(c.pin,readyPin);};b.core=core;bindings.set(root,b);
    var perspectiveController=root.NemoNativePerspectiveController;
    if(perspectiveController&&typeof perspectiveController.onRead==='function')removePerspectiveRead=perspectiveController.onRead(perspectiveReadChanged);
    b.refresh();return b;
  }
  function handle(root,method,e,cancelOtherIntent){
    var b=bindings.get(root);if(method!=='move')trace(root,'route',method,{binding:!!b});
    var handled=!!b&&b.handle(method,e);
    if(handled&&typeof cancelOtherIntent==='function')cancelOtherIntent();
    return handled;
  }
  return Object.freeze({mount:mount,handle:handle,canvasMapping:Object.freeze({bounds:bounds,point:point,screen:screen})});
}));
