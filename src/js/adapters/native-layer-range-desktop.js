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
  function isGestureCurrent(root,capture,info,after,queued,lease){
    try{
      var w=works.get(root),s=session(root);
      if(!w||w.capture!==capture||!capture.valid||s!==capture.session||!s.isActive()||!owner(info,capture.info)||
         info.contentRevision!==capture.info.contentRevision+(after?1:0)||(!after&&!same(info,capture.info))||
         signature(root,info)!==capture.ui||JSON.stringify(bars(root,info))!==capture.bars||root.FC!==capture.fc||
         root.state.activeSymbolId!=null||root.state.activeCompositionId!=null)return false;
      if(!after){
        if(queued){
          var origin=lease&&typeof lease.readPresentedObjectSession==='function'&&lease.readPresentedObjectSession();
          if(!origin||origin.available!==true||origin.kind!=='native-object'||!same(origin,capture.info)||!same(origin.presentation,capture.info)||
            origin.presentation!==capture.presentation||origin.presentation.owner!=='native'||origin.presentation.status!=='presented'||
            origin.presentation.frame!==root.state.currentFrame||capture.keySelectionCurrent(lease)!==true)return false;
        }else{if(capture.keyCurrent()!==true)return false;var keys=selected(root,info);if(!keys||JSON.stringify(keys)!==capture.keySignature)return false;}
      }
      return root.NemoNativeObjectProjectFile.projectionReadyForUI(root,info)===true;
    }catch(_){return false;}
  }
  // A correlated read projection, never another key-selection or document owner.
  function keyView(root,w){
    if(root.state.appMode!=='motion'||!w||!w.ports||typeof w.ports.readMotionKeyTimeline!=='function')return null;
    var value=w.ports.readMotionKeyTimeline(w.info);
    if(value===null)return null;
    if(!value||!same(value.pin,w.info)||value.frame!==root.state.currentFrame||!value.target||
       value.target.contextId!=='scene-root'||typeof value.target.layerUid!=='string'||typeof value.property!=='string'||!value.property||
       !Array.isArray(value.items)||typeof value.isCurrent!=='function'||typeof value.selectFrames!=='function'||value.isCurrent()!==true)throw Error('Native key projection unavailable');
    var previous=-Infinity;
    value.items.forEach(function(k){if(!k||!Number.isFinite(k.frame)||k.frame<=previous||typeof k.selected!=='boolean')throw Error('Invalid native key projection');previous=k.frame;});
    return value;
  }
  function keyViews(root,w){
    var plural=w&&w.ports&&w.ports.readMotionKeyTimelines;
    if(typeof plural!=='function'){
      var one=keyView(root,w);if(!one)return null;
      return {pin:one.pin,frame:one.frame,viewGeneration:one.viewGeneration,tracks:[{target:one.target,property:one.property,items:one.items}],
        isCurrent:one.isCurrent,selectKeys:function(keys){return one.selectFrames(keys.map(function(k){return k.frame;}));}};
    }
    if(root.state.appMode!=='motion')return null;
    var value=plural(w.info);if(value===null)return null;
    if(!value||!same(value.pin,w.info)||value.frame!==root.state.currentFrame||!Array.isArray(value.tracks)||
       typeof value.isCurrent!=='function'||typeof value.selectKeys!=='function'||value.isCurrent()!==true)throw Error('Native key tracks unavailable');
    var seen=new Set();
    value.tracks.forEach(function(track){
      if(!track||!track.target||track.target.contextId!=='scene-root'||typeof track.target.layerUid!=='string'||typeof track.property!=='string'||!track.property||!Array.isArray(track.items))throw Error('Invalid native key track');
      var id=trackIdentity(track),previous=-Infinity;if(seen.has(id))throw Error('Duplicate native key track');seen.add(id);
      track.items.forEach(function(k){if(!k||!Number.isFinite(k.frame)||k.frame<=previous||typeof k.selected!=='boolean')throw Error('Invalid native key frames');previous=k.frame;});
    });
    return value;
  }
  function trackIdentity(track){return JSON.stringify([track.target.contextId,track.target.layerUid,Object.prototype.hasOwnProperty.call(track.target,'strokeId'),track.target.strokeId,track.property]);}
  function keyIdentity(value){return value&&JSON.stringify([pins.map(function(k){return value.pin[k];}),value.frame,value.viewGeneration,
    value.tracks.map(function(track){return [trackIdentity(track),track.items.map(function(k){return k.frame;})];})]);}
  function keyAddress(track,frame){return {target:{kind:'property',target:track.target,property:track.property},frame:frame};}
  function selectedAddresses(value){var keys=[];value.tracks.forEach(function(track){track.items.forEach(function(key){if(key.selected)keys.push(keyAddress(track,key.frame));});});return keys;}
  function clearKeys(w){(w.keyNodes||[]).forEach(function(n){if(n.parentNode)n.parentNode.removeChild(n);});w.keyNodes=[];w.keySig=null;}
  function keyRows(root,w,layout){
    if(layout.length&&(!w.ports||typeof w.ports.setKeyTimelineRows!=='function'||w.ports.setKeyTimelineRows(layout,w.info)!==true))throw Error('Native property label rows unavailable');
    if(!layout.length&&w.ports&&typeof w.ports.setKeyTimelineRows==='function')w.ports.setKeyTimelineRows([],w.info);
    w.rows.forEach(function(row,i){var entry=layout.find(function(v){return v.layerUid===w.info.layers[i].layerUid;}),extra=entry?entry.labels.length:0;
      row.style.height=((1+extra)*22)+'px';var bar=row.querySelector('.layer-inout-bar');if(bar){bar.style.bottom=extra?'auto':'2px';bar.style.height=extra?'18px':'';}
    });
  }
  // T12: a native video placement's Time Remap keys (its placement read) as one inert lane: no atomic move-key op exists.
  function remapTracks(root,w){var f=root.SM&&root.SM.timeRemapKeyFacts;if(typeof f!=='function')return [];
    return w.info.layers.map(function(l,i){var r=f(i),keys=r&&Array.isArray(r.keys)?r.keys:[];
      return keys.length?{target:{contextId:'scene-root',layerUid:l.layerUid},property:'timeRemap',inert:true,items:keys.map(function(k){return {frame:k.frame,selected:false};})}:null;}).filter(Boolean);}
  function resetKeys(root,w){clearKeys(w);keyRows(root,w,[]);}
  // The key buttons depend on the key frames, selection, lanes and zoom, not on which frame is shown: a playback tick whose projection prints the same
  // signature keeps its buttons (#2371) and only swaps the live value their handlers read.
  function keySignature(root,w,fc,layout,positions){
    return JSON.stringify([fc,layout,positions.map(function(p){return [p.index,w.rows[p.index].hidden,trackIdentity(p.track),p.key.frame,p.key.selected,p.left,p.top];})]);
  }
  function refreshKeys(root){
    var w=works.get(root);if(!w)return;
    try{
      var live=w.node&&root.state.appMode==='motion'&&ready(root,session(root),w.info),value=live?keyViews(root,w):null,remap=live?remapTracks(root,w):[],fc=root.FC;
      if((!value&&!remap.length)||!Number.isFinite(fc)||fc<=0){resetKeys(root,w);return;}
      var groups=new Map(),layout=[],positions=[];
      (value?value.tracks:[]).concat(remap).forEach(function(track){
        var index=w.info.layers.findIndex(function(l){return l.layerUid===track.target.layerUid;});if(index<0)throw Error('Foreign native key layer');
        var group=groups.get(index)||[];group.push(track);groups.set(index,group);
      });
      groups.forEach(function(tracks,index){
        if(tracks.length>1)layout.push({layerUid:w.info.layers[index].layerUid,labels:tracks.map(function(t){return (t.target.strokeId===undefined?'':t.target.strokeId+' · ')+t.property;})});
        tracks.forEach(function(track,lane){track.items.forEach(function(key){
          // Preflight ALL coordinates in ALL tracks before publishing labels or nodes.
          var left=(key.frame+0.5)*fc-4.5,top=(tracks.length>1?(lane+1)*22:0)+6;
          if(!Number.isFinite(left)||!Number.isFinite(top))throw Error('Native key coordinate overflow');
          positions.push({track:track,key:key,index:index,left:left,top:top});
        });});
      });
      var signature=keySignature(root,w,fc,layout,positions);
      if(w.keySig===signature&&w.keyNodes.every(function(n){return !!n.parentNode;})){w.keyValue=value;return;}
      resetKeys(root,w);keyRows(root,w,layout);w.keyValue=value;
      positions.forEach(function(position){
        var track=position.track,key=position.key,row=w.rows[position.index];if(row.hidden)return;
        var node=root.document.createElement('button');node.type='button';node.className='motion-key layer-inout-key native-motion-property-key';
        Object.assign(node.style,{position:'absolute',left:position.left+'px',top:position.top+'px',padding:'0',cursor:'pointer',zIndex:'7'});
        node.dataset.frame=String(key.frame);node.dataset.keyTrack=trackIdentity(track);node.classList.toggle('sel',key.selected);
        node.setAttribute('aria-label',track.property+' key at frame '+String(key.frame));node.setAttribute('aria-pressed',String(key.selected));
        if(track.inert){node.disabled=true;node.style.pointerEvents='none';row.appendChild(node);w.keyNodes.push(node);return;}
        function choose(event){
          event.preventDefault();event.stopPropagation();if(event.button!==undefined&&event.button!==0)return;
          try{
            var live=w.keyValue;
            if(root.state.appMode!=='motion'||node.parentNode!==row||w.rows[position.index]!==row||root.FC!==fc||w.down||w.marquee||!ready(root,session(root),w.info)||!live||live.frame!==root.state.currentFrame||!live.isCurrent())throw Error('Stale native key selection');
            var keys=event.ctrlKey||event.metaKey||event.shiftKey?selectedAddresses(live):[],address=keyAddress(track,key.frame),id=JSON.stringify(address);
            var at=keys.findIndex(function(k){return JSON.stringify(k)===id;});if(at>=0&&(event.ctrlKey||event.metaKey))keys.splice(at,1);else if(at<0)keys.push(address);
            if(live.selectKeys(keys)!==true)throw Error('Native key selection refused');refreshKeys(root);
          }catch(_){notice(root,'Native key selection is unavailable or stale.');}
        }
        node.addEventListener('mousedown',choose);node.addEventListener('click',function(event){if(event.detail===0)choose(event);});
        row.appendChild(node);w.keyNodes.push(node);
      });
      w.keySig=signature;
    }catch(_){clearKeys(w);try{keyRows(root,w,[]);}catch(_){}notice(root,'Native key timeline coordinates are unavailable.');}
  }
  function marqueeKeys(root,w,m,x0,y0,x1,y1){
    var value=keyViews(root,w);
    if(keyIdentity(value)!==m.keyIdentity||m.keyView&&m.keyView.isCurrent()!==true)throw Error('Native key view changed');
    if(!value)return;
    var keys=[];
    (w.keyNodes||[]).forEach(function(node){
      if(!node.parentNode||node.parentNode.hidden)return;
      var b=node.getBoundingClientRect(),cx=b.left+b.width/2,cy=b.top+b.height/2;
      if(cx>=x0&&cx<=x1&&cy>=y0&&cy<=y1){var track=value.tracks.find(function(t){return trackIdentity(t)===node.dataset.keyTrack;});if(!track)throw Error('Foreign key node');keys.push(keyAddress(track,Number(node.dataset.frame)));}
    });
    if(value.selectKeys(keys)!==true)throw Error('Native key marquee refused');
    m.keyView=keyViews(root,w);if(keyIdentity(m.keyView)!==m.keyIdentity)throw Error('Native key marquee changed view');refreshKeys(root);
  }
  function cancel(root){var w=works.get(root);if(w&&w.marquee){var m=w.marquee;w.marquee=null;if(m.rect.parentNode)m.rect.parentNode.removeChild(m.rect);}if(w&&w.capture){w.capture.valid=false;w.capture=null;}if(w)w.down=false;}
  function retire(root){var w=works.get(root);if(!w)return;cancel(root);clearKeys(w);try{keyRows(root,w,[]);}catch(_){}w.serial++;w.rows=[];if(w.node&&w.node.parentNode)w.node.parentNode.removeChild(w.node);w.node=null;w.grid=null;}
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
    if(!keys||typeof keys.isSelectionCurrent!=='function'||!barSelection||!row.nativeRange||!same(row.nativeRange.pin,info)||!signature(root,info)){notice(root);return true;}
    if(barSelection.some(function(v){var index=info.layers.findIndex(function(l){return l.layerUid===v.layerUid;});return !w.rows[index]||!w.rows[index].nativeRange||!same(w.rows[index].nativeRange.pin,info);})){notice(root);return true;}
    var origin=s.readPresentedObjectSession();
    if(!origin||!same(origin,info)||!same(origin.presentation,info)||origin.presentation.frame!==root.state.currentFrame){notice(root);return true;}
    var capture={valid:true,session:s,info:info,presentation:origin.presentation,ui:signature(root,info),bars:JSON.stringify(barSelection),keySignature:JSON.stringify(keys),keyCurrent:keys.isCurrent,keySelectionCurrent:keys.isSelectionCurrent,
      fc:root.FC,startX:event.clientX,row:row,li:li,part:part,samples:[],alt:!!event.altKey};
    Object.defineProperty(capture,'isCurrent',{value:function(info,after,queued,lease){return isGestureCurrent(root,capture,info,after,queued,lease);}});
    capture.gesture=freeze({command:'object.layer.range.drag',contextId:'scene-root',layerUid:info.layers[li].layerUid,
      activeLayerUid:info.layers[root.state.activeLayerIdx].layerUid,part:part,barSelection:barSelection,frame:root.state.currentFrame,selectedKeys:keys.keys});
    w.capture=capture;w.down=true;return true;
  }
  function startMarquee(root,event){
    var w=works.get(root),s=session(root),info=s&&s.info();
    if(!w||!w.barUiReady||w.capture||w.marquee||root.state.activeSymbolId!=null||root.state.activeCompositionId!=null||!Number.isFinite(root.FC)||root.FC<=0||!ready(root,s,info)||!same(w.info,info)||!signature(root,info)||
      !Number.isFinite(event.clientX)||!Number.isFinite(event.clientY)||root.NemoNativeFrameEditDesktop.install(root).isPending()){notice(root);return true;}
    var keys;try{keys=keyViews(root,w);}catch(_){notice(root);return true;}
    var rect=root.document.createElement('div');rect.className='layer-inout-marquee-rect';root.document.body.appendChild(rect);
    w.marquee={rect:rect,x:event.clientX,y:event.clientY,info:info,ui:signature(root,info),fc:root.FC,moved:false,keyView:keys,keyIdentity:keyIdentity(keys)};return true;
  }
  function marquee(root,event,end){
    var w=works.get(root),m=w&&w.marquee;if(!m)return false;
    if(root.FC!==m.fc||!ready(root,session(root),m.info)||signature(root,m.info)!==m.ui||!Number.isFinite(event.clientX)||!Number.isFinite(event.clientY)){cancel(root);notice(root);return true;}
    if(!end){
      m.moved=m.moved||Math.abs(event.clientX-m.x)>3||Math.abs(event.clientY-m.y)>3;
      var x0=Math.min(m.x,event.clientX),x1=Math.max(m.x,event.clientX),y0=Math.min(m.y,event.clientY),y1=Math.max(m.y,event.clientY);
      Object.assign(m.rect.style,{left:x0+'px',top:y0+'px',width:(x1-x0)+'px',height:(y1-y0)+'px'});
      if(m.moved){try{marqueeKeys(root,w,m,x0,y0,x1,y1);}catch(_){cancel(root);notice(root,'Native key marquee is unavailable or stale.');return true;}w.ports.barUi.marqueeSelect(x0,y0,x1,y1);}
    }else{
      try{
        var keys=keyViews(root,w);
        if(keyIdentity(keys)!==m.keyIdentity||m.keyView&&m.keyView.isCurrent()!==true)throw Error('Stale native key view');
        if(!m.moved){if(keys&&keys.selectKeys([])!==true)throw Error('Native key selection refused');w.ports.barUi.setBarSelection([]);}
      }catch(_){cancel(root);notice(root);return true;}
      w.marquee=null;if(m.rect.parentNode)m.rect.parentNode.removeChild(m.rect);
    }
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
    // #2371: a frame-only change (same revision pin, same rows) keeps the painted rows and bars; rebuilding them per playback tick
    // reset every bar and re-read every range, which is the timeline flicker. Only a row whose range never arrived asks again.
    if(w.node&&w.node.parentNode===grid&&same(w.info,info)&&w.rows.length===info.layers.length){
      w.ports=ports;w.barUiReady=barUiReady;
      if(w.frame!==frame){w.frame=frame;if(s&&typeof s.readLayerRange==='function')w.rows.forEach(function(row,li){if(!row.nativeRange&&row.rangeAsked!==frame)askRange(root,s,w,row,li,info,frame,w.node);});}
      refreshKeys(root);return;}
    var ownPending=w.capture&&owner(w.capture.info,info)&&info.contentRevision===w.capture.info.contentRevision+1&&!w.down;
    if(!ownPending)cancel(root);
    w.serial++;var ticket=w.serial;w.info=info;w.frame=frame;w.grid=grid;w.ports=ports;w.barUiReady=barUiReady;
    clearKeys(w);if(w.node&&w.node.parentNode)w.node.parentNode.removeChild(w.node);
    var node=root.document.createElement('div');node.className='native-layer-range-rows';w.node=node;
    w.rows=info.layers.map(function(layer,li){var row=root.document.createElement('div');row.className='frow native-layer-range-row';row.dataset.layer=String(li);row.dataset.layerUid=layer.layerUid;
      row.style.position='relative';row.style.height='22px';row.style.width=(info.totalFrames*root.FC)+'px';row.hidden=!!(root.state.hideShy&&layer.shy);
      return row;});
    w.rows.forEach(function(row,li){node.appendChild(row);if(w.barUiReady)w.ports.barUi.buildBar(row,li);});grid.appendChild(node);refreshKeys(root);
    if(!s||typeof s.readLayerRange!=='function')return;
    w.rows.forEach(function(row,li){askRange(root,s,w,row,li,info,frame,node);});
  }
  function askRange(root,s,w,row,li,info,frame,node){
    var ticket=w.serial;row.rangeAsked=frame;
    Promise.resolve(s.readLayerRange(info.layers[li].layerUid,frame,info)).then(function(result){
      if(ticket!==w.serial||w.rows[li]!==row||row.parentNode!==node||!ready(root,s,info)||!result||result.kind!=='layerRange'||
        result.instanceId!==info.instanceId||result.documentId!==info.documentId||result.atRevision!==info.contentRevision||result.documentSnapshotId!==info.documentSnapshotId||
        result.contextId!=='scene-root'||result.layerUid!==info.layers[li].layerUid||result.frame!==frame)return;
      var range=result.range,fields=['rawInPoint','rawOutPoint','inPoint','outPoint','hasCustomRange','hasTimeLink'];
      if(!range||Object.keys(range).length!==6||!fields.every(function(k){return Object.prototype.hasOwnProperty.call(range,k);})||
        !fields.slice(0,4).every(function(k){return Number.isFinite(range[k]);})||typeof range.hasCustomRange!=='boolean'||typeof range.hasTimeLink!=='boolean')return;
      row.nativeRange=freeze({pin:Object.assign({},info),range:JSON.parse(JSON.stringify(range))});paint(root,row,li);
    }).catch(function(){});
  }
  return Object.freeze({active:active,mount:mount,refreshKeys:refreshKeys,retire:retire,owns:owns,buildBar:buildBar,paint:paint,down:down,startMarquee:startMarquee,isGestureCurrent:isGestureCurrent});
}));
