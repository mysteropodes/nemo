/* Native subselect controller. Paper is used only for detached curve hit testing.
 * Selection and SVG overlays are disposable views; every edit and preview goes to the native owner. */
(function(root,factory){
  const api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else {root.NemoNativeSubselect=api;api.install(root);}
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  const pins=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  const same=(a,b)=>!!a&&!!b&&pins.every(k=>a[k]===b[k]);
  const map=(m,p)=>[m[0]*p[0]+m[2]*p[1]+m[4],m[1]*p[0]+m[3]*p[1]+m[5]];
  function inverse(m,p){const d=m[0]*m[3]-m[1]*m[2];if(!Number.isFinite(d)||Math.abs(d)<1e-12)throw new Error('Subselect transform is singular');const x=p[0]-m[4],y=p[1]-m[5];return [(m[3]*x-m[2]*y)/d,(-m[1]*x+m[0]*y)/d];}
  function snap(start,p){const x=p[0]-start[0],y=p[1]-start[1],distance=Math.sqrt(x*x+y*y),angle=Math.round(Math.atan2(y,x)/(Math.PI/4))*(Math.PI/4);return [start[0]+Math.cos(angle)*distance,start[1]+Math.sin(angle)*distance];}
  const consume=e=>{if(e.stopImmediatePropagation)e.stopImmediatePropagation();if(e.preventDefault)e.preventDefault();};
  function create(root){
    let selection=null,drag=null,marquee=null,overlay=null,pending=null,uncertain=null,recovering=false,readToken=0,previewToken=0,previewBusy=false,previewNext=null,retired=false;
    let watching=false;
    let settled=Promise.resolve({status:'idle'});
    const session=()=>root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root);
    const mapping=()=>root.NemoNativeViewportUnderlay&&root.NemoNativeViewportUnderlay.canvasMapping;
    function native(){const s=session();return !!s&&s.blocksLegacy();}
    function refreshUI(){if(root.NemoNativeSubselectUI)root.NemoNativeSubselectUI.refresh(root);}
    function selectionState(){return !pending&&!uncertain&&!drag&&!marquee&&selection&&current(selection)&&selection.indices.length?Object.freeze({count:selection.indices.length}):null;}
    function notice(error){if(root.showToast)root.showToast(error&&error.message||String(error));}
    function layer(pin){const st=root.state,row=st.layers&&st.layers[st.activeLayerIdx],sel=root._layerSel;
      if(!row||!Array.isArray(sel)||sel.length!==1||sel[0]!==st.activeLayerIdx)throw new Error('Select one native Drawing layer.');
      const layers=pin.layers.filter(l=>l.layerUid===row.layerUid);
      if(layers.length!==1||layers[0].locked||layers[0].visible===false)throw new Error('The selected Drawing layer is unavailable.');return row.layerUid;}
    function bounds(pin){const d=root.document,r=d.getElementById('drawing-canvas').getBoundingClientRect(),html=d.documentElement;
      if(!html.classList.contains('nemo-native-underlay')||![r.left,r.top,r.width,r.height,pin.canvasW,pin.canvasH].every(Number.isFinite)||r.width<=0||r.height<=0||
        r.left<0||r.top<0||r.right>html.clientWidth||r.bottom>html.clientHeight||!['left','top','right','bottom'].every((k,i)=>parseFloat(html.style.getPropertyValue('--nemo-native-'+k))===[r.left,r.top,r.right,r.bottom][i]))throw new Error('Native canvas presentation changed.');
      return {left:r.left,top:r.top,width:r.width,height:r.height};}
    function mode(){const s=root.state;return s.appMode==='anim2d'&&!s.playing&&!s.spaceDown&&!s.isPanning&&!s.shadowMode&&!s.activeSymbolId;}
    function capture(){const s=session(),pin=s&&s.info(),shown=s&&s.readPresentedObjectSession(),st=root.state,M=mapping();
      if(retired||!s||!s.isActive()||!pin||!shown||!same(pin,shown)||!same(pin,shown.presentation)||shown.presentation.owner!=='native'||shown.presentation.status!=='presented'||
        !M||!M.matchesView(root,shown.presentation)||shown.presentation.frame!==st.currentFrame||shown.presentation.contextId!=='scene-root'||
        !root.NemoNativeObjectProjectFile||root.NemoNativeObjectProjectFile.projectionReadyForUI(root,pin)!==true||!mode())throw new Error('Native subselect is unavailable in this mode.');
      return {session:s,pin:pin,presentation:shown.presentation,selector:{contextId:'scene-root',layerUid:layer(pin),frame:st.currentFrame},bounds:bounds(pin)};}
    function current(t,lease){try{const s=session(),pin=s&&s.info(),shown=lease?lease.readPresentedObjectSession():s&&s.readPresentedObjectSession();
      return !retired&&!t.cancelled&&s===t.session&&s.isActive()&&s.blocksLegacy()&&same(pin,t.pin)&&shown&&shown.presentation===t.presentation&&same(shown,t.pin)&&
        root.state.tool==='subselect'&&mode()&&root.state.currentFrame===t.selector.frame&&layer(pin)===t.selector.layerUid&&
        (lease||mapping().matchesView(root,t.presentation))&&root.NemoNativeObjectProjectFile.projectionReadyForUI(root,pin)===true&&JSON.stringify(bounds(pin))===JSON.stringify(t.bounds);
    }catch(_){return false;}}
    function point(e,t){const p=mapping().point(e,{bounds:t.bounds,canvasW:t.pin.canvasW,canvasH:t.pin.canvasH,presentation:t.presentation});return [p.x,p.y];}
    function client(p,t){const q=map(mapping().transform({presentation:t.presentation},false),p);return [t.bounds.left+q[0]*t.bounds.width/t.pin.canvasW,t.bounds.top+q[1]*t.bounds.height/t.pin.canvasH];}
    function clear(){if(overlay){overlay.remove();overlay=null;}}
    function release(t){try{if(t&&t.capture&&t.capture.releasePointerCapture)t.capture.releasePointerCapture(t.pointer);}catch(_){}}
    function cancel(clearSelection){if(drag)drag.cancelled=true;if(marquee)marquee.cancelled=true;if(selection)selection.cancelled=true;release(drag);release(marquee);++readToken;++previewToken;previewNext=null;drag=null;marquee=null;clear();if(clearSelection)selection=null;refreshUI();return {status:'cancelled'};}
    function node(tag,attrs){const n=root.document.createElementNS('http://www.w3.org/2000/svg',tag);Object.keys(attrs).forEach(k=>n.setAttribute(k,attrs[k]));return n;}
    function watch(){if(watching||!root.requestAnimationFrame)return;watching=true;root.requestAnimationFrame(()=>{watching=false;if(pending)return;if((selection&&!current(selection))||(marquee&&!current(marquee))){cancel(true);return;}if(selection||marquee)watch();});}
    function draw(t,segments,previewMatrix){clear();if(!current(t))return;watch();
      const d=root.document;overlay=node('svg',{viewBox:'0 0 '+t.pin.canvasW+' '+t.pin.canvasH,preserveAspectRatio:'none','aria-hidden':'true'});
      Object.assign(overlay.style,{position:'fixed',pointerEvents:'none',zIndex:'20',left:t.bounds.left+'px',top:t.bounds.top+'px',width:t.bounds.width+'px',height:t.bounds.height+'px'});d.body.appendChild(overlay);
      const m=previewMatrix||t.item.matrix,V=mapping().transform({presentation:t.presentation},false),world=p=>map(V,map(m,p));
      let text='';segments.forEach((a,i)=>{if(!i){const p=world(a.point);text='M '+p.join(' ');return;}curve(segments[i-1],a);});
      function curve(a,b){text+=' C '+world([a.point[0]+a.handleOut[0],a.point[1]+a.handleOut[1]]).join(' ')+' '+world([b.point[0]+b.handleIn[0],b.point[1]+b.handleIn[1]]).join(' ')+' '+world(b.point).join(' ');}
      if(t.item.closed&&segments.length){curve(segments[segments.length-1],segments[0]);text+=' Z';}
      overlay.appendChild(node('path',{d:text,fill:'none',stroke:'#4b9bff','stroke-width':1,'vector-effect':'non-scaling-stroke'}));
      segments.forEach((a,i)=>{const p=world(a.point),selected=t.indices.includes(i);
        if(selected)for(const key of ['handleIn','handleOut']){const h=a[key];if(!h[0]&&!h[1])continue;const q=world([a.point[0]+h[0],a.point[1]+h[1]]);
          overlay.appendChild(node('line',{x1:p[0],y1:p[1],x2:q[0],y2:q[1],stroke:'#4b9bff','vector-effect':'non-scaling-stroke'}));
          overlay.appendChild(node('circle',{cx:q[0],cy:q[1],r:3*t.pin.canvasW/t.bounds.width,fill:'white',stroke:'#4b9bff','vector-effect':'non-scaling-stroke'}));}
        const r=3*t.pin.canvasW/t.bounds.width;overlay.appendChild(node('rect',{x:p[0]-r,y:p[1]-r,width:2*r,height:2*r,fill:selected?'#4b9bff':'white',stroke:'#4b9bff','vector-effect':'non-scaling-stroke'}));});
      refreshUI();
    }
    function hitAnchor(e,t){let best=null,distance=10;
      t.item.segments.forEach((a,index)=>{for(const key of ['point','handleIn','handleOut']){if(key!=='point'&&(!t.indices.includes(index)||!a[key].some(Boolean)))continue;
        const p=key==='point'?a.point:[a.point[0]+a[key][0],a.point[1]+a[key][1]],q=client(map(t.item.matrix,p),t),dist=Math.hypot(q[0]-e.clientX,q[1]-e.clientY);
        if(dist<distance){distance=dist;best={index:index,key:key};}}});return best;}
    function path(item){const Path=root.Path||(root.paper&&root.paper.Path);if(!Path)throw new Error('Subselect geometry hit testing is unavailable.');
      return new Path({insert:false,closed:item.closed,segments:item.segments.map(a=>[a.point,a.handleIn,a.handleOut])});}
    function rectangle(t){const Rectangle=root.Rectangle||(root.paper&&root.paper.Rectangle),Point=root.Point||(root.paper&&root.paper.Point);return new Rectangle(new Point(t.start),new Point(t.end));}
    function drawMarquee(t){clear();if(!current(t))return;watch();
      if(t.target)draw(t.target,t.target.item.segments);
      if(!overlay){overlay=node('svg',{'aria-hidden':'true'});Object.assign(overlay.style,{position:'fixed',pointerEvents:'none',zIndex:'20',left:t.bounds.left+'px',top:t.bounds.top+'px',width:t.bounds.width+'px',height:t.bounds.height+'px'});root.document.body.appendChild(overlay);}
      // Keep the selection rectangle in the same project coordinate system as the
      // native geometry, then apply only the disposable viewport presentation.
      const V=mapping().transform({presentation:t.presentation},false),r=rectangle(t),corners=[[r.left,r.top],[r.right,r.top],[r.right,r.bottom],[r.left,r.bottom]].map(p=>map(V,p));
      overlay.setAttribute('viewBox','0 0 '+t.pin.canvasW+' '+t.pin.canvasH);overlay.setAttribute('preserveAspectRatio','none');
      overlay.appendChild(node('polygon',{points:corners.map(p=>p.join(',')).join(' '),fill:'rgba(255,184,108,.08)',stroke:'#ffb86c','stroke-width':1,'stroke-dasharray':'4 3','vector-effect':'non-scaling-stroke'}));
    }
    function finishMarquee(t){if(marquee!==t||!current(t)){cancel(true);return {status:'cancelled'};}
      marquee=null;release(t);if(!t.moved){cancel(true);return {status:'cleared'};}
      const r=rectangle(t);let item=t.target&&t.target.item;
      if(!item)for(const candidate of t.items){const curve=path(candidate);try{
        const Matrix=root.Matrix||(root.paper&&root.paper.Matrix);curve.transform(new Matrix(...candidate.matrix));if(curve.bounds.intersects(r))item=candidate;
      }finally{curve.remove();}}
      const Point=root.Point||(root.paper&&root.paper.Point),indices=[];
      if(item)item.segments.forEach((a,i)=>{if(r.contains(new Point(map(item.matrix,a.point))))indices.push(i);});
      if(!indices.length){cancel(true);return {status:'cleared'};}
      selection={session:t.session,pin:t.pin,presentation:t.presentation,selector:t.selector,bounds:t.bounds,item,indices};draw(selection,item.segments);return {status:'selected'};
    }
    function hitItem(e,t,item){const p=inverse(item.matrix,point(e,t)),curve=path(item);try{
      const P=root.Point||(root.paper&&root.paper.Point),local=new P(p[0],p[1]),near=curve.getNearestLocation(local);
      if(near){const q=client(map(item.matrix,[near.point.x,near.point.y]),t);if(Math.hypot(q[0]-e.clientX,q[1]-e.clientY)<10)return {curve:near.index,time:near.curve.getTimeAt(near.curveOffset)};}
      return item.closed&&curve.contains(local)?{}:null;
    }finally{curve.remove();}}
    async function load(t,token){const data=await t.session.readSubselect(t.selector,t.pin);if(token!==readToken||!current(t)||data.presentation!==t.presentation||!same(data.info,t.pin))return null;return data.items;}
    function preview(t,intent){previewNext={t,intent,token:++previewToken};if(previewBusy)return;previewBusy=true;
      (async()=>{while(previewNext){const job=previewNext;previewNext=null;try{const r=await job.t.session.previewSubselect(job.t.selector,job.t.item.stableTarget,job.intent,job.t.pin);
        if(job.token===previewToken&&drag===job.t&&current(job.t)&&r.presentation===job.t.presentation&&same(r.info,job.t.pin))draw(job.t,r.segments,r.matrix);
      }catch(e){if(job.token===previewToken){cancel(false);notice(e);}}}previewBusy=false;})();}
    function admitted(t,lease){if(!recovering)return current(t,lease);const s=session(),p=s&&s.info();return !retired&&s===t.session&&s.isActive()&&p&&['instanceId','documentId','lifecycleGeneration'].every(k=>p[k]===t.pin[k]);}
    function commit(t,intent){if(uncertain)throw new Error('Resolve the previous subselect command before another edit.');if(pending)throw new Error('A native subselect command is still pending.');
      if(!current(t))throw new Error('Subselect target or frame changed.');
      const s=t.session,guarded={info:s.info,isActive:s.isActive,requestId:s.requestId,runResourceCommand:action=>s.runResourceCommand(ctx=>{
        if(!admitted(t,ctx))throw new Error('Subselect intent changed before dispatch.');return action({reconcile:ctx.reconcile,dispatch:q=>{if(!admitted(t,ctx))throw new Error('Subselect intent changed before dispatch.');return ctx.dispatch(q);}});
      })};
      const command=root.NemoNativeObjectVectorCommand.create(guarded,'subselect',root.NemoNativeSubselectContract);
      clear();++previewToken;previewNext=null;
      pending=command.execute(root.NemoNativeSubselectContract.EDIT,pin=>{if(!same(pin,t.pin))throw new Error('Subselect revision changed.');return {command:'object.subselect.edit',stableTarget:t.item.stableTarget,atFrame:t.selector.frame,intent:intent};});
      refreshUI();settled=pending.then(receipt=>{cancel(true);return {status:receipt.applied?'applied':'noop',receipt};},error=>{cancel(true);notice(error);if(error&&error.code==='subselect_command_indeterminate'){uncertain=error;return {status:'indeterminate',error};}return {status:'refused',error};}).finally(()=>{pending=null;refreshUI();});return settled;
    }
    function start(e,t,hit){
      const group=hit.key==='point'&&t.indices.includes(hit.index)&&t.indices.length>1;
      t.dragIndices=group?t.indices.slice():[hit.index];t.pairedPoint=null;
      if(hit.key==='point'&&!group)t.indices=e.shiftKey?Array.from(new Set(t.indices.concat(hit.index))):[hit.index];
      t.hit=hit;t.start=inverse(t.item.matrix,point(e,t));t.clientStart=[e.clientX,e.clientY];t.pointer=e.pointerId;t.alt=e.altKey;t.intent=null;drag=t;t.capture=root.document.getElementById('canvas-area')||root.document.getElementById('drawing-canvas');if(t.capture.setPointerCapture)t.capture.setPointerCapture(t.pointer);draw(t,t.item.segments);
    }
    function down(e){if(pending||drag)return;
      // A fresh gesture supersedes an earlier marquee even when its read has not
      // returned. Retire it before the vertex/Alt-insert fast paths can dispatch.
      if(marquee){const prior=marquee;marquee=null;prior.cancelled=true;++readToken;release(prior);clear();}
      if(selection&&!current(selection))cancel(true);
      const t=capture();if(selection&&current(selection)&&selection.presentation===t.presentation){const hit=hitAnchor(e,selection);if(hit){start(e,selection,hit);return;}
        if(e.altKey){const hit=hitItem(e,selection,selection.item);if(hit&&hit.time!==undefined){commit(selection,{kind:'insert',curve:hit.curve,time:hit.time});return;}}}
      const token=++readToken;const origin={...e,clientX:e.clientX,clientY:e.clientY,pointerId:e.pointerId,altKey:e.altKey,shiftKey:e.shiftKey};
      const box={...t,start:point(e,t),end:point(e,t),pointer:e.pointerId,target:selection&&current(selection)?selection:null,items:null,moved:false,ended:false};marquee=box;
      box.capture=root.document.getElementById('canvas-area')||root.document.getElementById('drawing-canvas');if(box.capture.setPointerCapture)box.capture.setPointerCapture(box.pointer);watch();refreshUI();
      settled=load(t,token).then(items=>{if(!items||marquee!==box)return {status:'cancelled'};let chosen=null;for(const item of items)if(hitItem(origin,t,item))chosen=item;
        if(!chosen){box.items=items;if(box.ended)return finishMarquee(box);if(box.moved)drawMarquee(box);return {status:'marquee'};}
        marquee=null;release(box);selection={...t,item:chosen,indices:[]};draw(selection,chosen.segments);return {status:'selected'};
      }).catch(error=>{if(token===readToken){cancel(true);notice(error);}return {status:'refused',error};});
    }
    function move(e){if(marquee&&marquee.pointer===e.pointerId){const t=marquee;if(!current(t)){cancel(true);return;}t.end=point(e,t);t.moved=true;if(t.items)drawMarquee(t);return;}
      const t=drag;if(!t||t.pointer!==e.pointerId)return;if(!current(t)){cancel(true);return;}
      let p=inverse(t.item.matrix,point(e,t));if(e.shiftKey)p=snap(t.hit.key==='point'?t.start:t.item.segments[t.hit.index].point,p);
      if(t.hit.key==='point')t.intent={kind:'move',indices:t.dragIndices.slice(),delta:[p[0]-t.start[0],p[1]-t.start[1]]};
      else {if(!e.altKey)t.pairedPoint=p;t.intent={kind:'handle',index:t.hit.index,handle:t.hit.key,point:p,breakPair:!!e.altKey};
        if(e.altKey&&t.pairedPoint)t.intent.pairedPoint=t.pairedPoint;}
      preview(t,t.intent);}
    function up(e){if(marquee&&marquee.pointer===e.pointerId){const t=marquee;t.ended=true;if(t.items)settled=Promise.resolve(finishMarquee(t));return;}
      const t=drag;if(!t||t.pointer!==e.pointerId)return;drag=null;release(t);++previewToken;previewNext=null;
      if(!current(t)){cancel(true);return;}const click=Math.hypot(e.clientX-t.clientStart[0],e.clientY-t.clientStart[1])<3;
      if(click&&t.alt&&t.hit.key==='point')commit(t,{kind:'toggle',index:t.hit.index});else if(t.intent&&!click)commit(t,t.intent);
      else if(t.item.stableTarget.frameScope.frame!==t.selector.frame)commit(t,{kind:'move',indices:[t.hit.index],delta:[0,0]});else draw(t,t.item.segments);}
    function action(intent){if(!selection||!current(selection)||!selection.indices.length){notice(new Error('Select one or more native path points.'));return Promise.resolve({status:'refused'});}
      try{return commit(selection,Object.assign({indices:selection.indices.slice()},intent));}catch(error){notice(error);return Promise.resolve({status:'refused',error});}}
    function handle(kind,e){if(!native()||root.state.tool!=='subselect')return false;
      const d=root.document,canvas=e.target===d.getElementById('drawing-canvas')||e.target===d.getElementById('canvas-area');if(!canvas&&!drag&&!marquee)return false;consume(e);
      try{if(kind==='lost'){if(drag||marquee&&!marquee.ended)cancel(false);}else if(kind==='cancel')cancel(false);else if(kind==='down')down(e);else if(kind==='move')move(e);else if(kind==='up')up(e);}catch(error){cancel(true);notice(error);}return true;}
    function deleteFrames(){
      if(pending||uncertain){notice(new Error('Resolve the previous subselect command before another edit.'));return;}
      cancel(true);
      const frames=root.NemoNativeFrameEditController;
      if(!frames||typeof frames.run!=='function'){notice(new Error('Native frame editing is unavailable.'));settled=Promise.resolve({status:'unavailable'});return;}
      // The original timeline gives selected cells priority over path points.
      // Use the native owner directly: releasing this capture must never reach a legacy writer.
      try{pending=Promise.resolve(frames.run({kind:'deleteFrames'}));}
      catch(error){notice(error);settled=Promise.resolve({status:'refused',error});return;}
      settled=pending.catch(error=>{notice(error);return {status:'refused',error};}).finally(()=>{pending=null;refreshUI();});
    }
    function key(e){if(!native()||root.state.tool!=='subselect'||e.target&&(/^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName)||e.target.isContentEditable))return false;
      if(e.key==='Escape'){consume(e);cancel(true);return true;}
      if(e.key==='Delete'||e.key==='Backspace'){consume(e);
        if(root.state.appMode==='anim2d'&&root._sel&&Array.isArray(root._sel.frames)&&root._sel.frames.length)deleteFrames();
        else action({kind:'delete'});return true;}return false;}
    function recover(){if(!uncertain||pending)return Promise.resolve({status:'unavailable'});const prior=uncertain;recovering=true;pending=Promise.resolve().then(()=>prior.replay());settled=pending.then(receipt=>{uncertain=null;return {status:'reconciled',receipt};},error=>{notice(error);return {status:'indeterminate',error};}).finally(()=>{pending=null;recovering=false;});return settled;}
    return Object.freeze({handle,key,recover,selectionState,cancel:()=>cancel(true),setPointType:type=>action({kind:'pointType',pointType:type}),scale:(scale,anchor)=>action({kind:'scale',scale,anchor}),rotate:(degrees,center)=>action({kind:'rotate',degrees,center}),settled:()=>settled,
      dispose:()=>{retired=true;cancel(true);}});
  }
  function install(root){const init=()=>{const controller=create(root);root.NemoNativeSubselectController=controller;const d=root.document,target=d.getElementById('canvas-area')||d.getElementById('drawing-canvas');if(!target)return;
    for(const [event,kind] of [['pointerdown','down'],['pointermove','move'],['pointerup','up'],['pointercancel','cancel'],['lostpointercapture','lost']])target.addEventListener(event,e=>controller.handle(kind,e),{capture:true});
    d.addEventListener('keydown',e=>controller.key(e),{capture:true});root.addEventListener('blur',()=>controller.cancel());
  };if(root.document.readyState==='loading')root.document.addEventListener('DOMContentLoaded',init);else init();}
  return Object.freeze({create,install,inverse,map});
}));
