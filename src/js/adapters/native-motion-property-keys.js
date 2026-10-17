/* Ordinary native authored-key inspector. Ports supply correlated D11 reads and
 * acknowledged D12 inner receipts; this component owns only DOM and selection. */
(function(root,factory){
  var api=factory(typeof module==='object'&&module.exports?require('./native-motion-property-view.js'):root.NemoNativeMotionPropertyView);
  if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeMotionPropertyKeys=api;
}(typeof globalThis==='object'?globalThis:this,function(View){
  'use strict';
  function object(v){return !!v&&typeof v==='object'&&!Array.isArray(v);}
  function exact(v,required,optional){return object(v)&&required.every(function(k){return Object.prototype.hasOwnProperty.call(v,k);})&&Object.keys(v).every(function(k){return required.includes(k)||(optional||[]).includes(k);});}
  function vector(v,n){return Array.isArray(v)&&v.length===n&&Array.from(v).every(Number.isFinite);}
  function uint(v){return Number.isSafeInteger(v)&&v>=0&&v<=4294967295;}
  function freeze(v){if(object(v)||Array.isArray(v)){Object.values(v).forEach(freeze);Object.freeze(v);}return v;}
  function targetKey(t){return JSON.stringify([t.contextId,t.layerUid,Object.prototype.hasOwnProperty.call(t,'strokeId'),t.strokeId]);}
  function authoredKey(v,dimension){
    if(!exact(v,['frame','v'],['hold','curvePoints','hIn','hOut'])||!Number.isFinite(v.frame)||!vector(v.v,dimension))return false;
    if('hold'in v&&typeof v.hold!=='boolean')return false;
    if(['hIn','hOut'].some(function(k){return k in v&&!vector(v[k],2);}))return false;
    if('curvePoints'in v&&v.curvePoints!==null){
      if(!Array.isArray(v.curvePoints))return false;
      var previous=-Infinity;
      for(var point of v.curvePoints){if(!exact(point,['x','y'],['tx','ty'])||!Object.values(point).every(Number.isFinite)||point.x<=previous)return false;previous=point.x;}
      if(v.curvePoints.length>=2&&(v.curvePoints[0].x!==0||previous!==1))return false;
    }
    return true;
  }
  function page(q,v){
    if(!exact(v,['instanceId','documentId','atRevision','documentSnapshotId','kind','target','property','dimension','trackPresent','items','nextOffset']))throw Error('Malformed native key page.');
    if(new TextEncoder().encode(JSON.stringify(v)).length>3072)throw Error('Native key page exceeds its bound.');
    var t=View.normalizeTarget(v.target);
    if(['instanceId','documentId','atRevision','documentSnapshotId'].some(function(k){return v[k]!==q[k];})||v.kind!=='authoredKeys'||!t||targetKey(t)!==targetKey(q.view.target)||v.property!==q.view.property)throw Error('Foreign native key page.');
    if(!Number.isInteger(v.dimension)||v.dimension<1||v.dimension>4||typeof v.trackPresent!=='boolean'||!Array.isArray(v.items)||v.items.length>8||!v.items.every(function(k){return authoredKey(k,v.dimension);}))throw Error('Malformed authored keys.');
    if(!v.trackPresent&&(v.items.length||v.nextOffset!==null))throw Error('Missing native track has keys.');
    if(v.nextOffset!==null&&(!uint(v.nextOffset)||v.items.length!==8||v.nextOffset!==q.view.offset+v.items.length||v.nextOffset<=q.view.offset))throw Error('Invalid native key continuation.');
    return v;
  }
  function create(container,ports){
    if(!View||!container||!ports||!ports.document||typeof ports.readContext!=='function'||typeof ports.readProperty!=='function')throw TypeError('Native key inspector ports unavailable');
    var doc=ports.document,serial=0,retired=false,accepted=null,busy=false,selected=new Set(),bindings=[],rowBindings=[],actionNodes=[];
    var element=make('section',''),title=make('div',''),status=make('div',''),rows=make('div',''),toolbar=make('div','');
    element.className='native-motion-property-keys';element.hidden=true;element.setAttribute('aria-label','Authored property keys');
    status.setAttribute('role','status');toolbar.setAttribute('role','toolbar');
    element.appendChild(title);element.appendChild(status);element.appendChild(rows);element.appendChild(toolbar);container.appendChild(element);
    function make(tag,label){var n=doc.createElement(tag);n.textContent=label;return n;}
    function listen(node,type,fn,list){node.addEventListener(type,fn);(list||bindings).push([node,type,fn]);}
    function unbind(list){list.forEach(function(b){b[0].removeEventListener(b[1],b[2]);});list.length=0;}
    function mounted(){return !retired&&element.parentNode===container&&(!('isConnected'in container)||container.isConnected);}
    function current(value,ticket){try{var c=ports.readContext();return mounted()&&ticket===serial&&View.isCurrentContext(value.token,c)&&c.frame===value.frame;}catch(_){return false;}}
    function ownerCurrent(value,ticket){try{var c=ports.readContext(),p=c&&c.pin;return mounted()&&ticket===serial&&View.captureContext(c)&&c.frame===value.frame&&['instanceId','documentId','lifecycleGeneration'].every(function(k){return p[k]===value.token.pin[k];});}catch(_){return false;}}
    function clearRows(){unbind(rowBindings);rows.replaceChildren();}
    function controls(){var enable=!!accepted&&!busy&&current(accepted,serial)&&typeof ports.editPropertyTrack==='function';actionNodes.forEach(function(n){n.disabled=!enable||!selected.size;});if(percent)percent.disabled=!enable;rowBindings.forEach(function(b){b[0].disabled=!enable;});}
    function reset(message,hide){++serial;accepted=null;selected.clear();busy=false;clearRows();element.hidden=!!hide;status.textContent=message||'';controls();}
    function refuse(message){reset(message,false);return false;}
    function receipt(value){return exact(value,['applied','historyEntriesAdded'])&&typeof value.applied==='boolean'&&value.historyEntriesAdded===(value.applied?1:0);}
    async function apply(action){
      var value=accepted,ticket=serial,writer=ports.editPropertyTrack;
      if(busy||!value||!current(value,ticket)||typeof writer!=='function'||!selected.size)return refuse('Native key action is unavailable or stale.');
      var selections=value.items.filter(function(k){return selected.has(k.frame);}).map(function(k){return {target:value.target,property:value.property,frame:k.frame};});
      if(!selections.length||selections.length>32)return refuse('Select between 1 and 32 native keys.');
      var payload=freeze({command:'object.property.track.edit',selections:selections,action:action});
      busy=true;controls();status.textContent='Waiting for native key edit…';
      try{
        // Queue entry suppresses presentedSession while pending. Binding guards
        // owner/frame/selection there; this predicate owns local UI lifetime.
        var result=await writer(payload,value.token.pin,function(){return mounted()&&ticket===serial&&accepted===value&&selected.size===selections.length&&selections.every(function(k){return selected.has(k.frame);});});
        if(!receipt(result))throw Error('Malformed native key receipt.');
        if(!ownerCurrent(value,ticket)){if(ticket===serial&&!retired)return refuse('Native key view became stale; selections cleared.');return false;}
        reset(result.applied?'Native key edit accepted. Select the property again to read its keys.':'Native key edit made no change. Select the property again to read its keys.',false);
        if(typeof ports.refresh==='function'){try{await ports.refresh();}catch(_){if(!retired&&serial===ticket+1)status.textContent='Native key receipt accepted; native refresh unavailable. Select the property again.';}}
        return true;
      }catch(error){
        if(ticket!==serial||retired)return false;
        return refuse('Native key edit refused: '+String(error&&error.message||error));
      }
    }
    function button(label,action){var n=make('button',label);n.type='button';listen(n,'click',function(){return apply(action);});toolbar.appendChild(n);actionNodes.push(n);return n;}
    [['Linear','linear'],['Ease','ease'],['Ease In','easeIn'],['Ease Out','easeOut'],['Hold','hold']].forEach(function(pair){button(pair[0],{kind:'preset',preset:pair[1]});});
    var percent=make('input','');percent.type='number';percent.step='any';percent.setAttribute('aria-label','Key influence percent');toolbar.appendChild(percent);
    function influence(side,label){var n=make('button',label);n.type='button';listen(n,'click',function(){var raw=String(percent.value),number=Number(raw);if(!raw.trim()||!Number.isFinite(number)){status.textContent='Enter a finite influence percentage.';return false;}return apply({kind:'influence',side:side,percent:number});});toolbar.appendChild(n);actionNodes.push(n);}
    influence('in','Incoming influence');influence('out','Outgoing influence');button('Subdivide',{kind:'subdivide'});
    var cancelButton=make('button','Cancel key read');cancelButton.type='button';listen(cancelButton,'click',cancel);toolbar.appendChild(cancelButton);
    function render(value,ticket){
      clearRows();
      value.items.forEach(function(k){
        var label=make('label',''),check=make('input',''),text=make('span','Frame '+String(k.frame)+' · '+JSON.stringify(k.v)+(k.hold===true?' · Hold':''));
        check.type='checkbox';check.setAttribute('aria-label','Select key at frame '+String(k.frame));
        listen(check,'change',function(){
          if(accepted!==value||!current(value,ticket)||busy||typeof ports.editPropertyTrack!=='function'){check.checked=false;return refuse('Native key selection is unavailable or stale.');}
          if(check.checked){if(selected.size>=32){check.checked=false;status.textContent='Select at most 32 native keys.';return;}selected.add(k.frame);}else selected.delete(k.frame);
          status.textContent=selected.size+' native key(s) selected.';controls();
        },rowBindings);
        label.appendChild(check);label.appendChild(text);rows.appendChild(label);
      });
      status.textContent=!value.trackPresent?'No authored track.':!value.items.length?'Authored track is empty.':typeof ports.editPropertyTrack!=='function'?'Authored keys are read-only; native track editor unavailable.':value.items.length+' authored key(s). Select up to 32.';
      controls();
    }
    async function open(input,property){
      reset('',false);var ticket=serial,target=View.normalizeTarget(input),context,token;
      try{context=ports.readContext();token=View.captureContext(context);}catch(_){}
      if(!target||typeof property!=='string'||!property||!token)return refuse('Native key read is unavailable.');
      var value={token:token,frame:context.frame,target:target,property:property,items:[],trackPresent:null,dimension:null};
      title.textContent=property+' · '+(target.strokeId===undefined?target.layerUid:target.strokeId);status.textContent='Reading native authored keys…';
      var offset=0,previous=-Infinity;
      try{
        for(;;){
          if(!current(value,ticket))throw Error('Stale native key read.');
          var q=freeze({instanceId:token.pin.instanceId,documentId:token.pin.documentId,atRevision:token.pin.contentRevision,documentSnapshotId:token.pin.documentSnapshotId,view:{kind:'authoredKeys',target:target,property:property,offset:offset,limit:8}});
          if(new TextEncoder().encode(JSON.stringify(q)).length>4096)throw Error('Native key selector exceeds its bound.');
          var result=await ports.readProperty(q);
          if(!current(value,ticket))throw Error('Stale native key read.');
          page(q,result);
          if(value.dimension!==null&&(value.dimension!==result.dimension||value.trackPresent!==result.trackPresent))throw Error('Inconsistent native key pages.');
          value.dimension=result.dimension;value.trackPresent=result.trackPresent;
          for(var k of result.items){if(k.frame<=previous)throw Error('Duplicate or unordered authored frame.');previous=k.frame;value.items.push(freeze(JSON.parse(JSON.stringify(k))));}
          if(result.nextOffset===null)break;
          offset=result.nextOffset;
          await new Promise(function(resolve){setTimeout(resolve,0);});
          if(!current(value,ticket))throw Error('Stale native key read.');
        }
        if(!current(value,ticket))throw Error('Stale native key read.');
        accepted=value;render(value,ticket);return true;
      }catch(error){if(ticket!==serial||retired)return false;return refuse('Native key read refused: '+String(error&&error.message||error));}
    }
    function cancel(){reset('Native key read cancelled.',false);return true;}
    controls();
    return Object.freeze({element:element,open:open,clear:function(){reset('',true);},cancel:cancel,dispose:function(){if(retired)return;reset('',true);retired=true;unbind(bindings);if(element.parentNode===container)container.removeChild(element);}});
  }
  return Object.freeze({create:create});
}));
