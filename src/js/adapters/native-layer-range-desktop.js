/* Native read-only bar rows and one ordered gesture. Rust owns range/Motion/content/Arc/history. */
(function(root,factory){
  var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeLayerRangeDesktop=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var works=new WeakMap(),pins=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  function same(a,b){return !!a&&!!b&&pins.every(function(k){return a[k]===b[k];});}
  function owner(a,b){return !!a&&!!b&&pins.slice(0,3).every(function(k){return a[k]===b[k];});}
  function freeze(v){if(v&&typeof v==='object'){Object.values(v).forEach(freeze);Object.freeze(v);}return v;}
  function notice(root,message){try{if(typeof root.showToast==='function')root.showToast(message||'Native layer range editing is unavailable.');}catch(_){}}
  function active(root){try{var s=root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root),c=root.NemoNativeOpacityCutover;
    return !!(s&&(s.isActive()||s.blocksLegacy&&s.blocksLegacy())||c&&c.blocksLegacy&&c.blocksLegacy());}catch(_){return true;}}
  function session(root){return root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root);}
  function ready(root,s,info){try{var p=s&&s.readPresentedObjectSession();return !!s&&s===session(root)&&s.isActive()&&
    root.NemoNativeObjectProjectFile.projectionReadyForUI(root,info)===true&&same(s.info(),info)&&p&&same(p,info)&&same(p.presentation,info)&&
    p.presentation.owner==='native'&&p.presentation.status==='presented'&&p.presentation.frame===root.state.currentFrame;}catch(_){return false;}}
  function signature(root,info){
    if(!root.state||!Array.isArray(root._layerSel)||!root._sel||!Array.isArray(root._sel.frames)||!Array.isArray(info.layers))return null;
    var frame=root.state.currentFrame,active=root.state.activeLayerIdx,count=info.layers.length;
    if(!Number.isInteger(frame)||frame<0||frame>=info.totalFrames||!Number.isInteger(active)||active<0||active>=count)return null;
    if(root._layerSel.some(function(i){return !Number.isInteger(i)||i<0||i>=count;}))return null;
    return JSON.stringify([frame,active,root._layerSel,root._sel.frames,root.state.activeSymbolId,root.state.activeCompositionId]);
  }
  function bars(root,info){
    var w=works.get(root),ui=w&&w.ports.barUi;if(!ui||typeof ui.getBarSelection!=='function')return null;
    var values=ui.getBarSelection(),seen=new Set();
    if(!Array.isArray(values))return null;
    return values.map(function(v){if(!v||!Number.isInteger(v.li)||!info.layers[v.li]||!['in','out','both'].includes(v.part)||seen.has(v.li))throw Error('Invalid bar selection');
      seen.add(v.li);return {layerUid:info.layers[v.li].layerUid,part:v.part};});
  }
  function selected(root,info){
    try{
      var w=works.get(root);if(!w||typeof w.ports.readLegacyMotionSelection!=='function')return null;
      var old=w.ports.readLegacyMotionSelection();if(!Array.isArray(old)||old.length)return null;
      var v=w&&typeof w.ports.readSelectedMotionKeys==='function'&&w.ports.readSelectedMotionKeys(info);
      if(!v||!same(v.pin,info)||v.frame!==root.state.currentFrame||!Array.isArray(v.keys)||typeof v.isCurrent!=='function'||v.isCurrent()!==true)return null;
      return v;
    }catch(_){return null;}
  }
  function isGestureCurrent(root,capture,info,after,queued){
    try{
      var w=works.get(root),s=session(root);
      if(!w||w.capture!==capture||!capture.valid||s!==capture.session||!s.isActive()||!owner(info,capture.info)||
         info.contentRevision!==capture.info.contentRevision+(after?1:0)||(!after&&!same(info,capture.info))||
         signature(root,info)!==capture.ui||JSON.stringify(bars(root,info))!==capture.bars||root.FC!==capture.fc||
         root.state.activeSymbolId!=null||root.state.activeCompositionId!=null)return false;
      if(!after){if(capture.keyCurrent()!==true)return false;if(!queued){var keys=selected(root,info);if(!keys||JSON.stringify(keys)!==capture.keySignature)return false;}}
      return root.NemoNativeObjectProjectFile.projectionReadyForUI(root,info)===true;
    }catch(_){return false;}
  }
  function cancel(root){var w=works.get(root);if(w&&w.marquee){var m=w.marquee;w.marquee=null;if(m.rect.parentNode)m.rect.parentNode.removeChild(m.rect);}if(w&&w.capture){w.capture.valid=false;w.capture=null;}if(w)w.down=false;}
  function retire(root){var w=works.get(root);if(!w)return;cancel(root);w.serial++;w.rows=[];if(w.node&&w.node.parentNode)w.node.parentNode.removeChild(w.node);w.node=null;w.grid=null;}
  function selection(root,w,li,part,event){
    var info=w.info,uid=info.layers[li].layerUid,values=bars(root,info)||[];
    if(event.metaKey||event.ctrlKey){var i=values.findIndex(function(v){return v.layerUid===uid;});if(i>=0)values.splice(i,1);else values.push({layerUid:uid,part:'both'});w.anchor=uid;}
    else if(event.shiftKey){var anchor=info.layers.findIndex(function(v){return v.layerUid===w.anchor;});if(anchor<0)anchor=li;values=[];
      for(var j=Math.min(anchor,li);j<=Math.max(anchor,li);j++)values.push({layerUid:info.layers[j].layerUid,part:'both'});}
    else values=[{layerUid:uid,part:part}];
    w.ports.barUi.setBarSelection(values.map(function(v){return {li:info.layers.findIndex(function(l){return l.layerUid===v.layerUid;}),part:v.part};}),li);
    return values;
  }
  function down(root,li,row,part,event){
    event.preventDefault();event.stopPropagation();
    var w=works.get(root),s=session(root),info=s&&s.info(),controller;
    try{controller=root.NemoNativeFrameEditDesktop.install(root);}catch(_){notice(root);return true;}
    if(!w||!w.barUiReady||w.rows[li]!==row||!ready(root,s,info)||!same(w.info,info)||controller.isPending()||w.down||w.marquee||
       !Number.isFinite(event.clientX)||!Number.isFinite(root.FC)||root.FC<=0||root.state.activeSymbolId!=null||root.state.activeCompositionId!=null){notice(root);return true;}
    if(event.metaKey||event.ctrlKey||event.shiftKey){
      selection(root,w,li,part,event);
      if(w.ports.selectLayer)w.ports.selectLayer(li,{mode:event.metaKey||event.ctrlKey?'toggle':'range'},info.layers[li].layerUid,info);
      return true;
    }
    var keys=selected(root,info),barSelection;
    try{barSelection=bars(root,info);}catch(_){}
    if(!keys||!barSelection||!row.nativeRange||!same(row.nativeRange.pin,info)||!signature(root,info)){notice(root);return true;}
    if(barSelection.some(function(v){var index=info.layers.findIndex(function(l){return l.layerUid===v.layerUid;});return !w.rows[index]||!w.rows[index].nativeRange||!same(w.rows[index].nativeRange.pin,info);})){notice(root);return true;}
    var capture={valid:true,session:s,info:info,ui:signature(root,info),bars:JSON.stringify(barSelection),keySignature:JSON.stringify(keys),keyCurrent:keys.isCurrent,
      fc:root.FC,startX:event.clientX,row:row,li:li,part:part,samples:[],alt:!!event.altKey};
    Object.defineProperty(capture,'isCurrent',{value:function(info,after,queued){return isGestureCurrent(root,capture,info,after,queued);}});
    capture.gesture=freeze({command:'object.layer.range.drag',contextId:'scene-root',layerUid:info.layers[li].layerUid,
      activeLayerUid:info.layers[root.state.activeLayerIdx].layerUid,part:part,barSelection:barSelection,frame:root.state.currentFrame,selectedKeys:keys.keys});
    w.capture=capture;w.down=true;return true;
  }
  function startMarquee(root,event){
    var w=works.get(root),s=session(root),info=s&&s.info();
    if(!w||!w.barUiReady||w.capture||w.marquee||root.state.activeSymbolId!=null||root.state.activeCompositionId!=null||!Number.isFinite(root.FC)||root.FC<=0||!ready(root,s,info)||!same(w.info,info)||!signature(root,info)||
      !Number.isFinite(event.clientX)||!Number.isFinite(event.clientY)||root.NemoNativeFrameEditDesktop.install(root).isPending()){notice(root);return true;}
    var rect=root.document.createElement('div');rect.className='layer-inout-marquee-rect';root.document.body.appendChild(rect);
    w.marquee={rect:rect,x:event.clientX,y:event.clientY,info:info,ui:signature(root,info),fc:root.FC,moved:false};return true;
  }
  function marquee(root,event,end){
    var w=works.get(root),m=w&&w.marquee;if(!m)return false;
    if(root.FC!==m.fc||!ready(root,session(root),m.info)||signature(root,m.info)!==m.ui||!Number.isFinite(event.clientX)||!Number.isFinite(event.clientY)){cancel(root);notice(root);return true;}
    if(!end){
      m.moved=m.moved||Math.abs(event.clientX-m.x)>3||Math.abs(event.clientY-m.y)>3;
      var x0=Math.min(m.x,event.clientX),x1=Math.max(m.x,event.clientX),y0=Math.min(m.y,event.clientY),y1=Math.max(m.y,event.clientY);
      Object.assign(m.rect.style,{left:x0+'px',top:y0+'px',width:(x1-x0)+'px',height:(y1-y0)+'px'});
      if(m.moved)w.ports.barUi.marqueeSelect(x0,y0,x1,y1);
    }else{if(!m.moved)w.ports.barUi.setBarSelection([]);w.marquee=null;if(m.rect.parentNode)m.rect.parentNode.removeChild(m.rect);}
    return true;
  }
  function move(root,event){if(marquee(root,event,false))return;var w=works.get(root),c=w&&w.capture;if(!w||!w.down||!c)return;
    if(!isGestureCurrent(root,c,c.info,false)||!Number.isFinite(event.clientX)){cancel(root);notice(root);return;}
    var dx=Math.round((event.clientX-c.startX)/c.fc);
    if(!Number.isSafeInteger(dx)||c.samples.length>=4096){cancel(root);notice(root,'Native layer gesture exceeds its bound.');return;}
    c.samples.push(dx); // Ordered samples, including zero/repeats, are never coalesced or replaced by the last sample.
  }
  function up(root,event){if(marquee(root,event,true))return;var w=works.get(root),c=w&&w.capture;if(!w||!w.down||!c)return;
    w.down=false;
    if(!isGestureCurrent(root,c,c.info,false)||!event||!Number.isFinite(event.clientX)){cancel(root);notice(root);return;}
    var gesture=Object.assign({},c.gesture,{samples:c.samples.slice(),alt:c.alt||!!event.altKey,
      plainClick:root.state.appMode==='motion'&&Math.abs(event.clientX-c.startX)<3});
    var controller;try{controller=root.NemoNativeFrameEditDesktop.install(root);}catch(_){cancel(root);notice(root);return;}
    Promise.resolve(controller.run({kind:'rangeDrag',gesture:gesture,capture:c})).then(function(result){
      if(w.capture!==c)return;
      if(result.status==='applied-presented'&&isGestureCurrent(root,c,c.session.info(),true)){
        var after=c.session.info();if(gesture.plainClick&&w.ports.selectLayer){selection(root,w,c.li,c.part,{});w.ports.selectLayer(c.li,false,after.layers[c.li].layerUid,after);}
        mount(root,w.grid,after,w.ports);
      }
    }).catch(function(){notice(root);}).finally(function(){if(w.capture===c){c.valid=false;w.capture=null;}});
  }
  function paint(root,row,li){var w=works.get(root),v=row&&row.nativeRange,bar=row&&row.querySelector('.layer-inout-bar');
    if(!w||w.rows[li]!==row||!bar||!v||!same(v.pin,w.info))return false;
    var r=v.range;bar.style.left=(r.rawInPoint*root.FC)+'px';bar.style.width=Math.max(root.FC,(r.rawOutPoint-r.rawInPoint+1)*root.FC)+'px';
    bar.classList.toggle('full-range',!r.hasCustomRange);bar.classList.toggle('has-timelink',r.hasTimeLink);
    bar.setAttribute('aria-disabled','false');return true;
  }
  function buildBar(root,row,li){var w=works.get(root);if(!w||w.rows[li]!==row)return true;
    var bar=root.document.createElement('div');bar.className='layer-inout-bar';bar.setAttribute('aria-disabled','true');
    ['in','out'].forEach(function(part){var handle=root.document.createElement('div');handle.className='layer-inout-handle '+(part==='in'?'left':'right');
      handle.addEventListener('mousedown',function(event){w.ports.barUi.onDown(li,row,part,event,handle);});bar.appendChild(handle);});
    bar.addEventListener('mousedown',function(event){if(event.target===bar)w.ports.barUi.onDown(li,row,'both',event);});
    bar.addEventListener('contextmenu',function(event){event.preventDefault();event.stopPropagation();notice(root,'Native range menus and links are unavailable.');});
    row.appendChild(bar);row.addEventListener('mousedown',function(event){if(event.target===row){event.preventDefault();event.stopPropagation();startMarquee(root,event);}});paint(root,row,li);return true;
  }
  function owns(root,grid,noticeNode){var w=works.get(root);return !!w&&w.grid===grid&&w.node&&w.node.parentNode===grid&&grid.children.length===2&&grid.children[0]===noticeNode&&grid.children[1]===w.node;}
  function mount(root,grid,info,ports){
    var s=session(root),w=works.get(root),frame=root.state&&root.state.currentFrame;
    var ui=ports&&ports.barUi,barUiReady=!!ui&&['getBarSelection','setBarSelection','marqueeSelect','onDown','buildBar'].every(function(k){return typeof ui[k]==='function';});
    if(!w){w={serial:0,rows:[],capture:null,down:false};works.set(root,w);
      root.document.addEventListener('mousemove',function(e){move(root,e);});root.document.addEventListener('mouseup',function(e){up(root,e);});
      if(root.addEventListener){root.addEventListener('blur',function(){cancel(root);});root.addEventListener('pagehide',function(){cancel(root);});}}
    if(w.node&&w.node.parentNode===grid&&same(w.info,info)&&w.frame===frame&&w.rows.length===info.layers.length){w.ports=ports;w.barUiReady=barUiReady;return;}
    var ownPending=w.capture&&owner(w.capture.info,info)&&info.contentRevision===w.capture.info.contentRevision+1&&!w.down;
    if(!ownPending)cancel(root);
    w.serial++;var ticket=w.serial;w.info=info;w.frame=frame;w.grid=grid;w.ports=ports;w.barUiReady=barUiReady;
    if(w.node&&w.node.parentNode)w.node.parentNode.removeChild(w.node);
    var node=root.document.createElement('div');node.className='native-layer-range-rows';w.node=node;
    w.rows=info.layers.map(function(layer,li){var row=root.document.createElement('div');row.className='frow native-layer-range-row';row.dataset.layer=String(li);row.dataset.layerUid=layer.layerUid;
      row.style.position='relative';row.style.height='22px';row.style.width=(info.totalFrames*root.FC)+'px';row.hidden=!!(root.state.hideShy&&layer.shy);
      return row;});
    w.rows.forEach(function(row,li){node.appendChild(row);if(w.barUiReady)w.ports.barUi.buildBar(row,li);});grid.appendChild(node);
    if(!s||typeof s.readLayerRange!=='function')return;
    w.rows.forEach(function(row,li){Promise.resolve(s.readLayerRange(info.layers[li].layerUid,frame,info)).then(function(result){
      if(ticket!==w.serial||w.rows[li]!==row||row.parentNode!==node||!ready(root,s,info)||!result||result.kind!=='layerRange'||
        result.instanceId!==info.instanceId||result.documentId!==info.documentId||result.atRevision!==info.contentRevision||result.documentSnapshotId!==info.documentSnapshotId||
        result.contextId!=='scene-root'||result.layerUid!==info.layers[li].layerUid||result.frame!==frame)return;
      var range=result.range,fields=['rawInPoint','rawOutPoint','inPoint','outPoint','hasCustomRange','hasTimeLink'];
      if(!range||Object.keys(range).length!==6||!fields.every(function(k){return Object.prototype.hasOwnProperty.call(range,k);})||
        !fields.slice(0,4).every(function(k){return Number.isFinite(range[k]);})||typeof range.hasCustomRange!=='boolean'||typeof range.hasTimeLink!=='boolean')return;
      row.nativeRange=freeze({pin:Object.assign({},info),range:JSON.parse(JSON.stringify(range))});paint(root,row,li);
    }).catch(function(){});});
  }
  return Object.freeze({active:active,mount:mount,retire:retire,owns:owns,buildBar:buildBar,paint:paint,down:down,startMarquee:startMarquee,isGestureCurrent:isGestureCurrent});
}));
