/* Ordinary Text Animator controls over native immutable reads and sealed intentions. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeTextAnimatorDesktop=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var bindings=new WeakMap(),mounts=new WeakMap();
  function element(root,id){var m=mounts.get(root);return m&&!m.retired?m.elements[id]||null:root.document.getElementById(id);}
  function session(root){var m=root.NemoNativeObjectProject;return m&&m.desktop(root);}
  function isNative(root){var s=session(root);return !!s&&s.blocksLegacy();}
  function owner(p){return p&&JSON.stringify([p.instanceId,p.documentId,p.lifecycleGeneration]);}
  function clone(v){return JSON.parse(JSON.stringify(v));}
  function context(root,s){
    var p=s.info(),state=root.state,current=session(root);
    if(current!==s||!p||!s.isActive()||!s.projectionReady||!state||state.playing||state.activeSymbolId||state.shadowMode||state.maskMode||!['anim2d','motion'].includes(state.appMode))return null;
    var layer=Array.isArray(p.layers)&&p.layers[state.activeLayerIdx],frame=state.currentFrame;
    if(!layer||layer.locked||!Number.isInteger(frame)||frame<0||frame>=p.totalFrames)return null;
    return {pin:p,layerUid:layer.layerUid,frame:frame,selectionKey:JSON.stringify([state.activeLayerIdx,root._layerSel||[]])};
  }
  function binding(root){var s=session(root),p=s&&s.info(),b=bindings.get(root);if(!s||!root.NemoNativeTextAnimator)return null;
    if(!b||b.session!==s||b.owner!==owner(p)){b={session:s,owner:owner(p),ticket:0,key:null,data:null,error:null,loading:false,writing:false};b.model=root.NemoNativeTextAnimator.create(s,function(){var m=mounts.get(root);return bindings.get(root)===b&&(!m||!m.retired&&root.document.body.contains(m.section))?context(root,s):null;});bindings.set(root,b);}return b;
  }
  function node(root,tag,text){var n=root.document.createElement(tag);if(text!==undefined)n.textContent=text;return n;}
  function status(root,text){var n=element(root,'p-textanim-empty');if(n){n.style.display='';n.textContent=text;}}
  function send(root,b,action){if(b.writing||b.loading)return;var data=b.data;if(!data)return;b.writing=true;draw(root,b);
    Promise.resolve().then(action).then(function(){b.key=null;b.error=null;},function(e){b.error=e.message||'Text animation unavailable';}).finally(function(){b.writing=false;if(bindings.get(root)===b)render(root);});}
  function live(root,b,data,host){var list=element(root,'p-textanim-list');return bindings.get(root)===b&&b.data===data&&!!list&&root.document.body.contains(list)&&list.contains(host);}
  function edit(root,b,record,fn,data,host){if(!live(root,b,data,host))return;var origin=clone(data.context),draft=clone(record);fn(draft);var delta;
    if(record.enabled!==draft.enabled)delta={kind:'setEnabled',enabled:draft.enabled};
    else for(var group of ['selector','props']){var before=record[group]||{},after=draft[group]||{};var keys=Array.from(new Set(Object.keys(before).concat(Object.keys(after)))).filter(function(k){return JSON.stringify(before[k])!==JSON.stringify(after[k]);});
      if(keys.length){if(keys.length!==1)throw new Error('One field per text animation gesture');delta={kind:group==='selector'?'setSelector':'setProps',field:keys[0],value:Object.prototype.hasOwnProperty.call(after,keys[0])?after[keys[0]]:null};break;}}
    if(delta)send(root,b,function(){return b.model.edit(record.id,delta,origin);});
  }
  function selectorIntent(root,b,record,delta,data,host){if(!live(root,b,data,host))return;var origin=clone(data.context);send(root,b,function(){return b.model.edit(record.id,delta,origin);});}

  function field(root,host,label,value,change,type){var row=node(root,'div'),title=node(root,'div',label),input=node(root,'input');row.className='pr';title.className='pl';input.className='pi';input.type=type||'number';input.value=String(value==null?'':value);if(input.type==='number')input.step='any';input.setAttribute('aria-label',label);input.addEventListener('change',function(){var v=input.type==='number'?Number(input.value):input.value;if(input.type==='number'&&!Number.isFinite(v))return;change(v);});row.appendChild(title);row.appendChild(input);host.appendChild(row);return row;}
  function choice(root,host,label,value,options,change){var row=node(root,'div'),title=node(root,'div',label),select=node(root,'select');row.className='pr';title.className='pl';select.className='psel';select.setAttribute('aria-label',label);options.forEach(function(v){var n=node(root,'option',String(v));n.value=String(v);select.appendChild(n);});select.value=String(value);select.addEventListener('change',function(){change(select.value);});row.appendChild(title);row.appendChild(select);host.appendChild(row);}
  function scalar(root,b,host,record,facts,key,data){
    var row=field(root,host,key,facts[key],function(v){selectorIntent(root,b,record,{kind:'setValue',property:key,frame:data.context.frame,values:[v]},data,host);});
    var p=(record.properties||[]).find(function(p){return p.property===key;}),animated=p&&p.track&&p.track.keys.length;
    var keyButton=node(root,'button',animated?'◆':'◇');keyButton.type='button';keyButton.className='pbtn';keyButton.setAttribute('aria-label','Animate '+key);
    keyButton.addEventListener('click',function(){selectorIntent(root,b,record,{kind:'setAnimated',property:key,frame:data.context.frame,animated:!animated},data,host);});row.appendChild(keyButton);
  }

  function draw(root,b){var doc=root.document,list=element(root,'p-textanim-list'),add=element(root,'p-textanim-add');if(!list)return;list.replaceChildren();var state=b.model.state();if(add)add.disabled=b.loading||b.writing||state.pending||!context(root,b.session);
    status(root,b.error|| (b.loading?'Loading text animation…':b.writing?'Updating text animation…':!b.data||!b.data.animators.length?'Add an animator to animate this layer’s text.':''));
    if(state.pending){var resolve=node(root,'button','Resolve pending animation');resolve.type='button';resolve.addEventListener('click',function(){send(root,b,function(){return b.model.resolve();});});list.appendChild(resolve);return;}
    if(!b.data)return;if(!b.data.hasText){var section=element(root,'p-textanim-sec');if(section)section.style.display='none';return;}
    var data=b.data;data.animators.forEach(function(record,index){var host=node(root,'fieldset'),legend=node(root,'legend','Animator '+(index+1));host.disabled=b.loading||b.writing;host.appendChild(legend);list.appendChild(host);
      var enable=node(root,'input'),label=node(root,'label',' Enabled');enable.type='checkbox';enable.checked=record.enabled!==false;enable.addEventListener('change',function(){edit(root,b,record,function(d){d.enabled=enable.checked;},data,host);});label.prepend(enable);host.appendChild(label);
      var remove=node(root,'button','Remove');remove.type='button';remove.className='pbtn';remove.addEventListener('click',function(){if(!live(root,b,data,host))return;var origin=clone(data.context);send(root,b,function(){return b.model.remove(record.id,origin);});});host.appendChild(remove);
      var evaluated=data.selectors[index];if(!evaluated.available){host.appendChild(node(root,'p','Animation values are unavailable. You can disable or remove this animator.'));return;}var facts=evaluated.value;[['selectorType',['range','wiggly']],['basedOn',['chars','words','lines']],['units',['percent','index']],['shape',[1,2,3,4,5,6]]].forEach(function(spec){choice(root,host,spec[0],facts[spec[0]],spec[1],function(v){edit(root,b,record,function(d){d.selector[spec[0]]=spec[0]==='shape'?Number(v):v;},data,host);});});
      ['start','end','offset','amount','easeHigh','easeLow','smooth','wiggleMin','wiggleMax','wigglesPerSec','correlation'].forEach(function(key){scalar(root,b,host,record,facts,key,data);});
      field(root,host,'seed',facts.seed,function(v){edit(root,b,record,function(d){d.selector.seed=v;},data,host);});
      [['position',[0,0]],['scale',[100,100]],['rotation',0],['opacity',100]].forEach(function(spec){var key=spec[0],value=record.props&&record.props[key];if(value==null)value=spec[1];(Array.isArray(spec[1])?[0,1]:[null]).forEach(function(axis){var fallback=Array.isArray(spec[1])?spec[1][axis]:spec[1];field(root,host,key+(axis===null?'':axis===0?' X':' Y'),axis===null?value:value[axis]==null?fallback:value[axis],function(v){edit(root,b,record,function(d){if(!d.props)d.props={};if(axis===null)d.props[key]=v;else{if(!d.props[key])d.props[key]=spec[1].slice();d.props[key][axis]=v;}},data,host);});});});
      ['fillColor','strokeColor'].forEach(function(key){var v=record.props&&record.props[key];var hex=v?'#'+v.slice(0,3).concat(v.length>3&&v[3]!=null?[v[3]]:[255]).map(function(n){return Math.round(n).toString(16).padStart(2,'0');}).join(''):'';
        field(root,host,key,hex,function(value){if(value!==''&&!/^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(value)){b.error='Enter #rrggbb or #rrggbbaa.';draw(root,b);return;}edit(root,b,record,function(d){if(!d.props)d.props={};if(value==='')delete d.props[key];else d.props[key]=value.slice(1).match(/../g).map(function(h){return parseInt(h,16);});},data,host);},'text');});
    });
  }
  function render(root){if(!isNative(root)){retire(root);return false;}
    var parent=root.document.getElementById('native-object-inspector');if(!mounts.has(root)&&parent){mount(root,parent);return true;}var doc=root.document,section=element(root,'p-textanim-sec');if(!section)return true;section.style.display='';var row=element(root,'p-textanim-add-row');if(row)row.style.display='';var b=binding(root),c=b&&context(root,b.session);
    if(!b||!c){status(root,'Text animation is unavailable for this selection.');var list=element(root,'p-textanim-list');if(list)list.replaceChildren();var add=element(root,'p-textanim-add');if(add)add.disabled=true;return true;}
    var key=JSON.stringify([owner(c.pin),c.pin.contentRevision,c.pin.documentSnapshotId,c.layerUid,c.frame,c.selectionKey]);
    if(b.key!==key&&!b.writing){b.key=key;b.data=null;b.error=null;b.loading=true;var ticket=++b.ticket;
      b.model.load().then(function(data){if(b.ticket===ticket&&bindings.get(root)===b&&context(root,b.session)&&b.key===key){b.data=data;b.error=null;}},function(e){if(b.ticket===ticket)b.error=e.message||'Text animation unavailable';}).finally(function(){if(b.ticket===ticket){b.loading=false;if(bindings.get(root)===b)render(root);}});
    }
    draw(root,b);return true;
  }
  function add(root){var b=binding(root);if(!b||!b.data)return;var data=b.data,origin=clone(data.context),record={id:'ta-'+b.session.requestId('animator'),enabled:true,selector:{start:0,end:40,shape:6},props:{position:[0,-60],scale:[100,100],rotation:0,opacity:0}};send(root,b,function(){return b.model.upsert(record,origin);});}
  function retire(root){var m=mounts.get(root);if(!m)return;m.retired=true;mounts.delete(root);var b=bindings.get(root);if(b)b.ticket++;bindings.delete(root);if(m.section.parentNode)m.section.parentNode.removeChild(m.section);}
  function mount(root,container){var old=mounts.get(root);if(old&&old.container===container&&!old.retired){render(root);return old;}
    if(old)retire(root);var elements={},section=node(root,'section'),heading=node(root,'div','Text animation'),row=node(root,'div'),button=node(root,'button','Add animator'),hint=node(root,'div'),list=node(root,'div');
    section.id='native-text-animators';section.className='psec';heading.className='phdr';row.className='pr';button.type='button';button.className='pbtn';hint.setAttribute('role','status');
    [heading,row,hint,list].forEach(function(n){section.appendChild(n);});row.appendChild(button);container.appendChild(section);
    elements['p-textanim-sec']=section;elements['p-textanim-add-row']=row;elements['p-textanim-add']=button;elements['p-textanim-empty']=hint;elements['p-textanim-list']=list;
    var m={container:container,section:section,elements:elements,retired:false,refresh:function(){if(mounts.get(root)===m&&!m.retired)render(root);},retire:function(){if(mounts.get(root)===m)retire(root);}};
    button.addEventListener('click',function(){if(mounts.get(root)===m&&!m.retired&&root.document.body.contains(section))add(root);});mounts.set(root,m);render(root);return m;
  }
  return Object.freeze({isNative:isNative,render:render,add:add,mount:mount,retire:retire});
}));
