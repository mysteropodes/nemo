/* Selection-only native header cells. Persistent drawings, timing and history stay native.
 * The existing _sel.frames remains the sole cell-selection owner; captures and
 * unfinished arrays are transient and never feed commands before publication. */
(function(root,factory){
  var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeFrameCellSelection=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var works=new WeakMap(),pins=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'],CHUNK=1024;
  function same(a,b){return !!a&&!!b&&pins.every(function(k){return a[k]===b[k];});}
  function rows(root,info){
    var selected=root._layerSel;
    if(!Array.isArray(selected)||!selected.length||new Set(selected).size!==selected.length||
       !selected.every(function(i){return Number.isSafeInteger(i)&&i>=0&&i<info.layers.length;}))return null;
    return selected.map(function(i){return [i,info.layers[i].layerUid];});
  }
  function presented(root,ports,info){
    try{
      if(typeof ports.frameEditPending!=='function'||ports.frameEditPending()!==false)return null;
      var session=ports.currentSession(),shown=session&&session.readPresentedObjectSession();
      return session&&session.isActive()&&same(session.info(),info)&&ports.projectionReady(info)===true&&
        shown&&shown.available===true&&same(shown,info)&&same(shown.presentation,info)&&
        shown.presentation.owner==='native'&&shown.presentation.status==='presented'&&
        shown.presentation.frame===root.state.currentFrame?{session:session,shown:shown.presentation}:null;
    }catch(_){return null;}
  }
  function connected(root,w){var header=root.document.getElementById('frame-hdr');return works.get(root)===w&&!!header&&
    w.node&&w.node.parentNode===w.caption&&header.contains(w.caption);}
  function cellsEqual(cells,values){return Array.isArray(cells)&&cells.length===values.length&&
    cells.every(function(cell,i){return cell&&cell.layer===values[i][0]&&cell.frame===values[i][1];});}
  function sourceCurrent(root,w,c){
    var p=presented(root,w.ports,c.info),selection=rows(root,c.info);
    return connected(root,w)&&p&&p.session===c.session&&p.shown===c.presentation&&
      same(w.info,c.info)&&root.FC===c.fc&&root.state.currentFrame===c.frame&&root.state.appMode===c.mode&&
      root.state.activeSymbolId==null&&root.state.activeCompositionId==null&&root._sel===c.selection&&
      root._sel.frames===c.frames&&cellsEqual(root._sel.frames,c.cells)&&JSON.stringify(selection)===c.rows&&
      w.node.getBoundingClientRect().left===c.left;
  }
  function current(root,w,c){return w.capture===c&&sourceCurrent(root,w,c);}
  function status(w,text){w.status.textContent=text;w.status.hidden=!text;w.node.setAttribute('aria-busy',text?'true':'false');}
  function pending(root){var w=works.get(root);return !!(w&&(w.capture||w.dialog));}
  function cycle(root,w){
    var button=root.document.getElementById('btn-cycle');
    if(w.cycle&&(!(w.capture||w.dialog)||w.cycle.node!==button)){
      w.cycle.node.disabled=w.cycle.disabled;w.cycle.node.title=w.cycle.title;w.cycle=null;
    }
    if((w.capture||w.dialog)&&button){if(!w.cycle)w.cycle={node:button,disabled:button.disabled,title:button.title};
      button.disabled=true;button.title='Finish or cancel the frame selection first';}
  }
  function mode(w,on){w.enabled=on;if(w.button){w.button.textContent=on?'Done selecting':'Select frames';w.button.setAttribute('aria-pressed',String(on));}}
  function dismiss(root,w){var d=w.dialog;if(!d)return;w.dialog=null;d.done=true;
    if(d.overlay.parentNode)d.overlay.parentNode.removeChild(d.overlay);cycle(root,w);}
  function cancel(root,exit){var w=works.get(root);if(!w)return;w.capture=null;if(exit)mode(w,false);
    dismiss(root,w);
    cycle(root,w);
    if(w.timer!==null){(root.clearTimeout||clearTimeout)(w.timer);w.timer=null;}
    if(w.node){status(w,'');paint(root,w);}}
  function retire(root){var w=works.get(root);if(!w)return;cancel(root,true);
    w.listeners.forEach(function(v){v[0].removeEventListener(v[1],v[2],v[3]);});works.delete(root);}
  function interval(c){return {from:Math.min(c.start,c.end),to:Math.max(c.start,c.end)};}
  function selected(root,w){
    var cells=root._sel&&root._sel.frames;
    if(!Array.isArray(cells)||!cells.length)return null;
    var from=Infinity,to=-1;
    cells.forEach(function(c){if(c&&Number.isSafeInteger(c.frame)&&c.frame>=0&&c.frame<w.info.totalFrames){from=Math.min(from,c.frame);to=Math.max(to,c.frame);}});
    return to<0?null:{from:from,to:to};
  }
  function paint(root,w){
    var range=w.capture?interval(w.capture):selected(root,w),fc=root.FC;
    if(!Number.isFinite(fc)||fc<=0)return;
    w.highlight.hidden=!range;
    if(range){w.highlight.style.left=(range.from*fc)+'px';w.highlight.style.width=((range.to-range.from+1)*fc)+'px';}
    var label=w.enabled?'Frame selection mode: click starts a fresh range; drag selects; Shift-click extends; Escape cancels':
      'Normal ruler mode: click or drag scrubs. Use Select frames for frame selection';
    if(range)label='Selected frames '+(range.from+1)+' through '+(range.to+1)+' for selected layer bounds. '+label;
    w.node.setAttribute('aria-label',label);w.node.title=label;
    w.labels.replaceChildren();
    var wrap=root.document.getElementById('fg-wrap'),first=Math.max(0,Math.floor((wrap&&wrap.scrollLeft||0)/fc));
    var width=wrap&&wrap.clientWidth||w.node.getBoundingClientRect().width;
    var last=Math.min(w.info.totalFrames-1,first+Math.ceil(width/fc));
    for(var frame=first;frame<=last;frame++){
      var cell=root.document.createElement('span');cell.textContent=String(frame+1);cell.dataset.frame=String(frame);
      cell.className='native-frame-selection-cell';
      Object.assign(cell.style,{position:'absolute',left:(frame*fc)+'px',width:fc+'px',height:'20px',
        textAlign:'center',lineHeight:'20px',pointerEvents:'none',fontSize:'10px',overflow:'hidden'});
      w.labels.appendChild(cell);
    }
  }
  function point(c,event,clamp){
    if(!Number.isFinite(event.clientX))return null;
    var frame=Math.floor((event.clientX-c.left)/c.fc);
    return clamp?Math.max(0,Math.min(c.info.totalFrames-1,frame)):
      frame>=0&&frame<c.info.totalFrames?frame:null;
  }
  function down(root,w,event){
    // The ordinary #frame-hdr bubble handler remains the ruler scrub owner.
    if(works.get(root)!==w||event.button!==0||!w.enabled||w.dialog)return;
    event.preventDefault();event.stopPropagation();cancel(root);
    var p=presented(root,w.ports,w.info),chosen=rows(root,w.info),selection=root._sel;
    if(!p||!connected(root,w)||!chosen||!selection||!Array.isArray(selection.frames)||
       !selection.frames.every(function(v){return v&&Number.isSafeInteger(v.layer)&&v.layer>=0&&v.layer<w.info.layers.length&&
         Number.isSafeInteger(v.frame)&&v.frame>=0&&v.frame<w.info.totalFrames;})||
       !Number.isFinite(root.FC)||root.FC<=0||!Number.isFinite(root.FC*w.info.totalFrames)||
       root.state.activeSymbolId!=null||root.state.activeCompositionId!=null)return;
    var c={info:Object.assign({},w.info),session:p.session,presentation:p.shown,fc:root.FC,
      left:w.node.getBoundingClientRect().left,frame:root.state.currentFrame,mode:root.state.appMode,
      selection:selection,frames:selection.frames,cells:selection.frames.map(function(v){return [v.layer,v.frame];}),
      rows:JSON.stringify(chosen),min:chosen[0][0],max:chosen[0][0]};
    chosen.forEach(function(v){c.min=Math.min(c.min,v[0]);c.max=Math.max(c.max,v[0]);});
    var frame=point(c,event,false);if(frame===null)return;
    var anchor=selection.frames.length?selection.frames[selection.frames.length-1].frame:frame;
    c.start=event.shiftKey&&Number.isSafeInteger(anchor)&&anchor>=0&&anchor<w.info.totalFrames?anchor:frame;
    c.end=frame;w.capture=c;cycle(root,w);if(typeof w.node.focus==='function')w.node.focus({preventScroll:true});paint(root,w);
  }
  function move(root,w,event){var c=w.capture;if(works.get(root)!==w||!c||c.build)return;
    if(!current(root,w,c)){cancel(root);return;}
    var frame=point(c,event,true);if(frame===null){cancel(root);return;}c.end=frame;paint(root,w);
  }
  function build(root,w,c){
    if(works.get(root)!==w)return;
    if(!current(root,w,c)){cancel(root);return;}
    var range=interval(c),span=range.to-range.from+1,count=(c.max-c.min+1)*span;
    // This is the intrinsic real-array bound, not a new Repeat/clipboard cap.
    if(!Number.isSafeInteger(count)||count>4294967295){cancel(root);return;}
    try{
      var stop=Math.min(count,c.build.length+CHUNK);
      for(var index=c.build.length;index<stop;index++)c.build.push({layer:c.min+Math.floor(index/span),frame:range.from+index%span});
    }catch(_){cancel(root);return;}
    if(c.build.length<count){
      status(w,'Building frame selection — Escape cancels');
      w.timer=(root.setTimeout||setTimeout)(function(){w.timer=null;build(root,w,c);},0);return;
    }
    if(!current(root,w,c)){cancel(root);return;}
    root._sel.frames=c.build;w.capture=null;cycle(root,w);status(w,'');paint(root,w);
    if(typeof root.updateStatusBarHelp==='function')root.updateStatusBarHelp();
  }
  function up(root,w,event){var c=w.capture;if(works.get(root)!==w||!c||c.build)return;
    if(!current(root,w,c)){cancel(root);return;}
    var frame=point(c,event,true);if(frame===null){cancel(root);return;}c.end=frame;c.build=[];build(root,w,c);
  }
  function mount(root,caption,info,ports){
    var w=works.get(root);
    if(w){cancel(root);if(!same(w.info,info))mode(w,false);}
    else{
      w={capture:null,timer:null,listeners:[],enabled:false};works.set(root,w);
      function listen(target,name,callback,capture){target.addEventListener(name,callback,capture);w.listeners.push([target,name,callback,capture]);}
      listen(root.document,'mousemove',function(e){move(root,w,e);});
      listen(root.document,'mouseup',function(e){up(root,w,e);});
      listen(root.document,'keydown',function(e){if(e.key==='Escape'&&(w.capture||w.enabled||w.dialog)){e.preventDefault();e.stopPropagation();cancel(root,true);}},true);
      listen(root.document,'visibilitychange',function(){if(root.document.hidden)cancel(root,true);});
      if(root.addEventListener){listen(root,'blur',function(){cancel(root,true);});listen(root,'pagehide',function(){retire(root);});}
      var wrap=root.document.getElementById('fg-wrap');if(wrap)listen(wrap,'scroll',function(){if(w.capture)cancel(root);else paint(root,w);});
    }
    w.caption=caption;w.info=info;w.ports=ports;
    var captionText=caption.textContent,fc=root.FC,node=root.document.createElement('div'),labels=root.document.createElement('div');
    var highlight=root.document.createElement('div'),pending=root.document.createElement('span');
    node.className='native-frame-selection-strip';node.setAttribute('role','group');node.tabIndex=0;
    var wrap=root.document.getElementById('fg-wrap'),width=Math.max(Number.isFinite(fc)&&fc>0?info.totalFrames*fc:0,wrap&&wrap.clientWidth||0);
    caption.style.minWidth='100%';
    Object.assign(node.style,{position:'relative',height:'20px',width:width+'px',
      minWidth:'100%',cursor:'crosshair',userSelect:'none',backgroundImage:'repeating-linear-gradient(to right,transparent 0,transparent '+Math.max(0,fc-1)+'px,var(--border) '+Math.max(0,fc-1)+'px,var(--border) '+fc+'px)'});
    Object.assign(highlight.style,{position:'absolute',top:'0',height:'20px',background:'rgba(63,107,245,.35)',pointerEvents:'none'});
    pending.setAttribute('role','status');Object.assign(pending.style,{position:'absolute',left:'0',top:'0',whiteSpace:'nowrap',background:'var(--panel2)',pointerEvents:'none'});
    w.node=node;w.labels=labels;w.highlight=highlight;w.status=pending;
    node.appendChild(highlight);node.appendChild(labels);node.appendChild(pending);caption.replaceChildren(node);
    var frameLabel=root.document.createElement('span');frameLabel.className='native-frame-selection-caption';frameLabel.textContent=captionText;
    Object.assign(frameLabel.style,{position:'sticky',right:'0',float:'right',lineHeight:'20px',background:'var(--panel2)',pointerEvents:'none'});node.appendChild(frameLabel);
    var button=root.document.createElement('button');button.type='button';button.className='native-frame-selection-mode';
    button.title='Select frame cells for the selected layer bounds; normal ruler clicks move the playhead';
    Object.assign(button.style,{position:'sticky',right:'0',float:'right',height:'20px',fontSize:'10px',padding:'0 5px'});node.appendChild(button);w.button=button;mode(w,w.enabled);
    button.addEventListener('mousedown',function(e){if(works.get(root)===w&&w.node===node&&e.button===0){e.preventDefault();e.stopPropagation();}});
    button.addEventListener('click',function(e){if(works.get(root)!==w||w.node!==node)return;e.preventDefault();e.stopPropagation();
      if(!connected(root,w)||!presented(root,w.ports,w.info)||!rows(root,w.info))return;
      cancel(root);mode(w,!w.enabled);paint(root,w);});
    node.addEventListener('mousedown',function(e){if(w.node===node&&node.parentNode===caption)down(root,w,e);});
    node.addEventListener('blur',function(){if(works.get(root)===w&&w.node===node&&w.capture)cancel(root);});
    status(w,'');paint(root,w);return true;
  }
  function openRepeatDialog(root,accept,isCurrent){
    var w=works.get(root);try{
    var p=w&&presented(root,w.ports,w.info),chosen=w&&rows(root,w.info),s=root._sel;
    if(!w||!p||!connected(root,w)||w.capture||w.dialog||!chosen||!s||!Array.isArray(s.frames)||!s.frames.length||typeof accept!=='function'||typeof isCurrent!=='function'||isCurrent()!==true)return false;
    if(!Number.isFinite(root.FC)||root.FC<=0||!s.frames.every(function(v){return v&&Number.isSafeInteger(v.layer)&&v.layer>=0&&v.layer<w.info.layers.length&&
      Number.isSafeInteger(v.frame)&&v.frame>=0&&v.frame<w.info.totalFrames;}))return false;
    var d={info:Object.assign({},w.info),session:p.session,presentation:p.shown,fc:root.FC,left:w.node.getBoundingClientRect().left,
      frame:root.state.currentFrame,mode:root.state.appMode,selection:s,frames:s.frames,
      cells:s.frames.map(function(v){return [v.layer,v.frame];}),rows:JSON.stringify(chosen),done:false};
    var overlay=root.document.createElement('div'),form=root.document.createElement('form'),heading=root.document.createElement('h2');
    var label=root.document.createElement('label'),input=root.document.createElement('input'),repeat=root.document.createElement('button'),close=root.document.createElement('button');
    overlay.className='native-repeat-dialog-overlay';Object.assign(overlay.style,{position:'fixed',inset:'0',display:'flex',alignItems:'center',justifyContent:'center',background:'rgba(0,0,0,.4)',zIndex:'20000'});
    form.setAttribute('role','dialog');form.setAttribute('aria-modal','true');form.setAttribute('aria-label','Repeat selected frames');
    Object.assign(form.style,{background:'var(--panel2)',color:'var(--text)',border:'1px solid var(--border2)',borderRadius:'8px',padding:'18px',display:'flex',gap:'12px',alignItems:'center',flexWrap:'wrap',maxWidth:'360px'});
    heading.textContent='Repeat selected frames';heading.style.width='100%';heading.style.fontSize='15px';
    label.textContent='Repetitions';label.setAttribute('for','native-repeat-count');input.id='native-repeat-count';input.type='text';input.inputMode='numeric';input.value='2';
    input.setAttribute('aria-label','Repetitions');input.style.width='90px';repeat.type='submit';repeat.textContent='Repeat';close.type='button';close.textContent='Cancel';
    d.overlay=overlay;w.dialog=d;cycle(root,w);[heading,label,input,repeat,close].forEach(function(n){form.appendChild(n);});overlay.appendChild(form);root.document.body.appendChild(overlay);
    function commit(e){e.preventDefault();e.stopPropagation();if(d.done||works.get(root)!==w||w.dialog!==d)return;
      var valid=false;try{valid=sourceCurrent(root,w,d)&&!w.capture&&root.document.body.contains(overlay)&&isCurrent()===true;}catch(_){}
      if(!valid){dismiss(root,w);if(root.showToast)root.showToast('Repeat refused: the document or selection changed.');return;}
      var value=input.value;dismiss(root,w);try{Promise.resolve(accept(value)).catch(function(){if(root.showToast)root.showToast('Repeat was not confirmed.');});}
      catch(_){if(root.showToast)root.showToast('Repeat was not confirmed.');}
    }
    form.addEventListener('submit',commit);repeat.addEventListener('click',commit);
    close.addEventListener('click',function(e){e.preventDefault();e.stopPropagation();if(works.get(root)===w&&w.dialog===d)dismiss(root,w);});
    input.addEventListener('keydown',function(e){if(e.key==='Enter')commit(e);else if(e.key==='Escape'){e.preventDefault();e.stopPropagation();if(works.get(root)===w&&w.dialog===d)dismiss(root,w);}});
    overlay.addEventListener('click',function(e){if(e.target===overlay&&works.get(root)===w&&w.dialog===d)dismiss(root,w);});
    if(typeof input.focus==='function')input.focus();if(typeof input.select==='function')input.select();return true;
    }catch(_){if(w&&works.get(root)===w)dismiss(root,w);return false;}
  }
  return Object.freeze({mount:mount,retire:retire,isPending:pending,openRepeatDialog:openRepeatDialog});
}));
