/* Ordinary Text Animator controls over native immutable reads and sealed intentions. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeTextAnimatorDesktop=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var bindings=new WeakMap();
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
    if(!b||b.session!==s||b.owner!==owner(p)){b={session:s,owner:owner(p),ticket:0,key:null,data:null,error:null,loading:false,writing:false};b.model=root.NemoNativeTextAnimator.create(s,function(){return context(root,s);});bindings.set(root,b);}return b;
  }
  function node(root,tag,text){var n=root.document.createElement(tag);if(text!==undefined)n.textContent=text;return n;}
  function status(root,text){var n=root.document.getElementById('p-textanim-empty');if(n){n.style.display='';n.textContent=text;}}
  function send(root,b,action){if(b.writing||b.loading)return;var data=b.data;if(!data)return;b.writing=true;draw(root,b);
    Promise.resolve().then(action).then(function(){b.key=null;b.error=null;},function(e){b.error=e.message||'Text animation unavailable';}).finally(function(){b.writing=false;if(bindings.get(root)===b)render(root);});}
  function live(root,b,data,host){var list=root.document.getElementById('p-textanim-list');return bindings.get(root)===b&&b.data===data&&!!list&&list.contains(host);}
  function edit(root,b,record,fn,data,host){if(!live(root,b,data,host))return;var origin=clone(data.context),draft=clone(record);fn(draft);send(root,b,function(){return b.model.upsert(draft,origin);});}
  function field(root,host,label,value,change,type){var row=node(root,'div'),title=node(root,'div',label),input=node(root,'input');row.className='pr';title.className='pl';input.className='pi';input.type=type||'number';input.value=String(value==null?'':value);if(input.type==='number')input.step='any';input.setAttribute('aria-label',label);input.addEventListener('change',function(){var v=input.type==='number'?Number(input.value):input.value;if(input.type==='number'&&!Number.isFinite(v))return;change(v);});row.appendChild(title);row.appendChild(input);host.appendChild(row);return row;}
  function choice(root,host,label,value,options,change){var row=node(root,'div'),title=node(root,'div',label),select=node(root,'select');row.className='pr';title.className='pl';select.className='psel';select.setAttribute('aria-label',label);options.forEach(function(v){var n=node(root,'option',String(v));n.value=String(v);select.appendChild(n);});select.value=String(value);select.addEventListener('change',function(){change(select.value);});row.appendChild(title);row.appendChild(select);host.appendChild(row);}
  function scalar(root,b,host,record,facts,key,data){var row=field(root,host,key,facts[key],function(v){edit(root,b,record,function(d){var p=(d.properties||[]).find(function(p){return p.property===key;});
      if(p&&p.track&&p.track.keys.length){var k=p.track.keys.find(function(k){return k.frame===data.context.frame;});if(k)k.v=[v];else{p.track.keys.push({frame:data.context.frame,v:[v]});p.track.keys.sort(function(a,b){return a.frame-b.frame;});}}
      else{d.selector[key]=v;if(p)p.static=[v];}},data,host);});
    var p=(record.properties||[]).find(function(p){return p.property===key;}),animated=p&&p.track&&p.track.keys.length;
    var keyButton=node(root,'button',animated?'◆':'◇');keyButton.type='button';keyButton.className='pbtn';keyButton.setAttribute('aria-label','Animate '+key);keyButton.addEventListener('click',function(){edit(root,b,record,function(d){if(!d.properties)d.properties=[];var p=d.properties.find(function(p){return p.property===key;});if(!p){p={property:key};d.properties.push(p);}p.static=[facts[key]];if(animated)delete p.track;else p.track={keys:[{frame:data.context.frame,v:[facts[key]]}]};},data,host);});row.appendChild(keyButton);
  }
  function draw(root,b){var doc=root.document,list=doc.getElementById('p-textanim-list'),add=doc.getElementById('p-textanim-add');if(!list)return;list.replaceChildren();var state=b.model.state();if(add)add.disabled=b.loading||b.writing||state.pending||!context(root,b.session);
    status(root,b.error|| (b.loading?'Loading text animation…':b.writing?'Updating text animation…':!b.data||!b.data.animators.length?'Add an animator to animate this layer’s text.':''));
    if(state.pending){var resolve=node(root,'button','Resolve pending animation');resolve.type='button';resolve.addEventListener('click',function(){send(root,b,function(){return b.model.resolve();});});list.appendChild(resolve);return;}
    if(!b.data)return;if(!b.data.hasText){var section=doc.getElementById('p-textanim-sec');if(section)section.style.display='none';return;}
    var data=b.data;data.animators.forEach(function(record,index){var host=node(root,'fieldset'),legend=node(root,'legend','Animator '+(index+1));host.disabled=b.loading||b.writing;host.appendChild(legend);list.appendChild(host);
      var enable=node(root,'input'),label=node(root,'label',' Enabled');enable.type='checkbox';enable.checked=record.enabled!==false;enable.addEventListener('change',function(){edit(root,b,record,function(d){d.enabled=enable.checked;},data,host);});label.prepend(enable);host.appendChild(label);
      var remove=node(root,'button','Remove');remove.type='button';remove.className='pbtn';remove.addEventListener('click',function(){if(!live(root,b,data,host))return;var origin=clone(data.context);send(root,b,function(){return b.model.remove(record.id,origin);});});host.appendChild(remove);
      var facts=data.selectors[index];[['selectorType',['range','wiggly']],['basedOn',['chars','words','lines']],['units',['percent','index']],['shape',[1,2,3,4,5,6]]].forEach(function(spec){choice(root,host,spec[0],facts[spec[0]],spec[1],function(v){edit(root,b,record,function(d){d.selector[spec[0]]=spec[0]==='shape'?Number(v):v;},data,host);});});
      ['start','end','offset','amount','easeHigh','easeLow','smooth','wiggleMin','wiggleMax','wigglesPerSec','correlation'].forEach(function(key){scalar(root,b,host,record,facts,key,data);});
      field(root,host,'seed',facts.seed,function(v){edit(root,b,record,function(d){d.selector.seed=v;},data,host);});
      [['position',[0,0]],['scale',[100,100]],['rotation',0],['opacity',100]].forEach(function(spec){var key=spec[0],value=record.props&&record.props[key];if(value==null)value=spec[1];(Array.isArray(spec[1])?[0,1]:[null]).forEach(function(axis){var fallback=Array.isArray(spec[1])?spec[1][axis]:spec[1];field(root,host,key+(axis===null?'':axis===0?' X':' Y'),axis===null?value:value[axis]==null?fallback:value[axis],function(v){edit(root,b,record,function(d){if(!d.props)d.props={};if(axis===null)d.props[key]=v;else{if(!d.props[key])d.props[key]=spec[1].slice();d.props[key][axis]=v;}},data,host);});});});
      ['fillColor','strokeColor'].forEach(function(key){var v=record.props&&record.props[key];var hex=v?'#'+v.slice(0,3).concat(v.length>3&&v[3]!=null?[v[3]]:[255]).map(function(n){return Math.round(n).toString(16).padStart(2,'0');}).join(''):'';
        field(root,host,key,hex,function(value){if(value!==''&&!/^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(value)){b.error='Enter #rrggbb or #rrggbbaa.';draw(root,b);return;}edit(root,b,record,function(d){if(!d.props)d.props={};if(value==='')delete d.props[key];else d.props[key]=value.slice(1).match(/../g).map(function(h){return parseInt(h,16);});},data,host);},'text');});
    });
  }
  function render(root){if(!isNative(root))return false;var doc=root.document,section=doc.getElementById('p-textanim-sec');if(!section)return true;section.style.display='';var row=doc.getElementById('p-textanim-add-row');if(row)row.style.display='';var b=binding(root),c=b&&context(root,b.session);
    if(!b||!c){status(root,'Text animation is unavailable for this selection.');var list=doc.getElementById('p-textanim-list');if(list)list.replaceChildren();var add=doc.getElementById('p-textanim-add');if(add)add.disabled=true;return true;}
    var key=JSON.stringify([owner(c.pin),c.pin.contentRevision,c.pin.documentSnapshotId,c.layerUid,c.frame,c.selectionKey]);
    if(b.key!==key&&!b.writing){b.key=key;b.data=null;b.error=null;b.loading=true;var ticket=++b.ticket;
      b.model.load().then(function(data){if(b.ticket===ticket&&bindings.get(root)===b&&context(root,b.session)&&b.key===key){b.data=data;b.error=null;}},function(e){if(b.ticket===ticket)b.error=e.message||'Text animation unavailable';}).finally(function(){if(b.ticket===ticket){b.loading=false;if(bindings.get(root)===b)render(root);}});
    }
    draw(root,b);return true;
  }
  function add(root){var b=binding(root);if(!b||!b.data)return;var data=b.data,origin=clone(data.context),record={id:'ta-'+b.session.requestId('animator'),enabled:true,selector:{start:0,end:40,shape:6},props:{position:[0,-60],scale:[100,100],rotation:0,opacity:0}};send(root,b,function(){return b.model.upsert(record,origin);});}
  return Object.freeze({isNative:isNative,render:render,add:add});
}));
