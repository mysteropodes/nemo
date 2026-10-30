/* Ordinary property values and key intents. Only the native owner samples,
 * changes animation mode, edits values and publishes history. */
(function(root,factory){
  var api=factory(typeof module==='object'&&module.exports?require('./native-motion-property-view.js'):root.NemoNativeMotionPropertyView);
  if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeMotionPropertyValue=api;
}(typeof globalThis==='object'?globalThis:this,function(View){
  'use strict';
  function exact(value,names){return !!value&&typeof value==='object'&&!Array.isArray(value)&&Object.keys(value).length===names.length&&names.every(function(k){return Object.prototype.hasOwnProperty.call(value,k);});}
  function targetKey(value){return JSON.stringify([value.contextId,value.layerUid,Object.prototype.hasOwnProperty.call(value,'strokeId'),value.strokeId]);}
  function frozen(value){if(value&&typeof value==='object'){Object.values(value).forEach(frozen);Object.freeze(value);}return value;}
  function create(container,ports,target,descriptor){
    target=View.normalizeTarget(target);
    if(!target||!descriptor||typeof descriptor.property!=='string'||!descriptor.property||
        !Number.isInteger(descriptor.dimension)||descriptor.dimension<1||descriptor.dimension>4||
        !ports||!ports.document||typeof ports.readContext!=='function'||typeof ports.readProperty!=='function')throw TypeError('Native property value ports are unavailable');
    var doc=ports.document,serial=0,retired=false,accepted=null,busy=false,inputs=[],bindings=[],pending=null,armed=false,shown=null,keyHere=null,swOk=false;
    var element=make('div',''),fields=element,status=make('span',''),sw=make('div',''),unit=unitText(descriptor.unit),dims=dimLabels();
    element.className='native-motion-property-value motion-fields';element.setAttribute('aria-label',descriptor.property+' value');
    status.setAttribute('role','status');status.className='native-motion-property-status';sw.setAttribute('role','button');sw.setAttribute('tabindex','0');
    if(descriptor.authored&&descriptor.authored.expressionEnabled){var note=make('span','Base value before expression');note.className='native-motion-sr-only';element.appendChild(note);}
    container.appendChild(element);
    function unitText(u){return u==='deg'?'\u00b0':typeof u==='string'?u:'';}
    function dimLabels(){var named;try{named=ports.dimLabels?ports.dimLabels(descriptor.property):null;}catch(_){named=null;}
      if(Array.isArray(named)&&named.length===descriptor.dimension&&named.every(function(x){return typeof x==='string'&&x;}))return named.slice();
      return descriptor.dimension===2?['X','Y']:null;}
    function say(message){status.textContent=message;status.className=/^(Property edit refused|Native property|Enter finite)/.test(message)?'native-motion-property-status':'native-motion-property-status native-motion-sr-only';}
    function make(tag,label){var n=doc.createElement(tag);n.textContent=label;return n;}
    function listen(node,type,fn){node.addEventListener(type,fn);bindings.push([node,type,fn]);}
    function mounted(){return !retired&&element.parentNode===container&&(!('isConnected'in container)||container.isConnected);}
    function current(value,ticket){try{var context=ports.readContext();return mounted()&&ticket===serial&&View.isCurrentContext(value.token,context)&&value.frame===context.frame;}catch(_){return false;}}
    function ownerCurrent(value,ticket){try{var context=ports.readContext(),pin=context&&context.pin;return mounted()&&ticket===serial&&View.captureContext(context)&&value.frame===context.frame&&['instanceId','documentId','lifecycleGeneration'].every(function(k){return pin[k]===value.token.pin[k];});}catch(_){return false;}}
    // Baseline stopwatch diamond (motion.js): off = not animated, outline = animated with no key at
    // the playhead, filled = a key sits here. One click maps onto the existing owner actions only.
    function tr(name,fallback){var v;try{v=typeof ports.translate==='function'?ports.translate(name):undefined;}catch(_){v=undefined;}return typeof v==='string'&&v&&v!==name?v:fallback;}
    function animated(){return !!descriptor.authored&&descriptor.authored.keyCount>0;}
    function swIntent(){
      if(!animated())return {kind:'setAnimated',animated:true};
      if(keyHere===true)return descriptor.authored.keyCount>1?{kind:'removeKey'}:{kind:'setAnimated',animated:false};
      return {kind:'setKey'};
    }
    function swEnabled(enabled){
      return enabled&&descriptor.animatable===true&&!(descriptor.authored&&descriptor.authored.expressionEnabled)&&(!animated()||keyHere!==null);
    }
    function paintSw(enabled){
      var on=animated(),here=on&&keyHere===true,off=!swEnabled(enabled);
      sw.className='lico motion-stopwatch'+(on?' on':'')+(off?' disabled':'');
      var label=!on?tr('motionAnimateProp','Enable animation for this property'):here?tr('motionKeyRemove','Remove the key at the current frame'):tr('motionKeyAdd','Add a key at the current frame');
      sw.title=label;sw.setAttribute('aria-label',label);sw.setAttribute('aria-disabled',off?'true':'false');
      sw.innerHTML='<svg viewBox="0 0 24 24" width="12" height="12"><path d="M12 3l9 9-9 9-9-9z" fill="'+(here?'currentColor':'none')+'" stroke="currentColor" stroke-width="2"/></svg>';
    }
    function controls(){
      var enabled=!!accepted&&!busy&&current(accepted,serial)&&typeof ports.editProperty==='function';
      inputs.forEach(function(n){n.disabled=!enabled;n.readOnly=!enabled;n.title=typeof ports.editProperty==='function'?'':'Read-only';});
      swOk=swEnabled(enabled);paintSw(enabled);
    }
    // On-demand keys drawer: Alt/Option-click or double-click opens it for this property (no per-row Keys button).
    function openKeys(){var open=ports.openKeys;if(typeof open!=='function')return false;try{return open();}catch(_){return false;}}
    function onSw(event){
      if(event&&event.stopPropagation)event.stopPropagation();
      if(event&&event.altKey)return openKeys();
      if(event&&event.detail>1)return false;
      if(!swOk)return false;var i=swIntent();return apply(i.kind,i.animated);
    }
    function onSwDouble(event){if(event&&event.stopPropagation)event.stopPropagation();return openKeys();}
    function onSwKey(event){if(event&&(event.key==='Enter'||event.key===' ')){if(event.preventDefault)event.preventDefault();return onSw(event);}}
    function clear(message){serial++;accepted=null;busy=false;shown=null;keyHere=null;drop();inputs.forEach(function(n){n.value='';});say(message);controls();return false;}
    // ui.js drives a scrub by dispatching change every frame while the document-level
    // window._scrubLiveActive flag is up, and one last change before it drops the flag
    // at pointerup. Only the final value of such a drag reaches the native owner, so one
    // drag is one history entry; typed edits (flag down) commit at once.
    function scrubbing(){var w=doc.defaultView;return !!(w&&w._scrubLiveActive);}
    function disarm(){if(armed){armed=false;doc.removeEventListener('pointerup',release);doc.removeEventListener('pointercancel',release);}}
    function drop(){pending=null;disarm();}
    function settle(force){if(!pending||!force&&scrubbing())return false;var held=pending;drop();return apply('setValue',undefined,held);}
    function release(){if(scrubbing()){setTimeout(function(){settle(true);},0);return false;}return settle(false);}
    function onChange(){
      if(!scrubbing()){drop();return apply('setValue');}
      try{pending=values();}catch(error){say(error.message);return false;}
      if(!armed){armed=true;doc.addEventListener('pointerup',release);doc.addEventListener('pointercancel',release);}
      Promise.resolve().then(function(){return settle(false);});return false;
    }
    function onKey(event){
      if(!event||event.key!=='Escape')return;
      drop();if(shown)inputs.forEach(function(n,i){n.value=String(shown[i]);});
      var node=event.target;if(node&&typeof node.blur==='function')node.blur();
    }
    function values(){return inputs.map(function(input){var text=String(input.value);if(!text.trim()||!Number.isFinite(Number(text)))throw Error('Enter finite numeric values.');return Number(text);});}
    function receipt(value){return exact(value,['applied','historyEntriesAdded'])&&typeof value.applied==='boolean'&&value.historyEntriesAdded===(value.applied?1:0);}
    async function apply(kind,animated,held){
      if(busy)return false;
      var value=accepted,ticket=serial,writer=ports.editProperty;
      if(!value||!current(value,ticket)||typeof writer!=='function')return clear('Native property edit is unavailable or stale.');
      if(kind!=='setValue'&&descriptor.animatable!==true)return false;
      var action={kind:kind,frame:value.frame};
      try{if(kind==='setValue'||kind==='setKey')action.values=held||values();else if(kind==='setAnimated')action.animated=animated;}
      catch(error){say(error.message);return false;}
      var payload=frozen({command:'object.property.edit',target:JSON.parse(JSON.stringify(target)),property:descriptor.property,action:action});
      busy=true;controls();say('Waiting for native property edit…');
      try{
        // Binding fences the actual session, frame and selection at queue entry.
        // Do not demand a presented session while that same queue is busy.
        var result=await writer(payload,value.token.pin,function(){return mounted()&&serial===ticket&&accepted===value;});
        if(!receipt(result))throw Error('Malformed native property receipt.');
        if(ticket!==serial||!mounted())return false;
        if(!ownerCurrent(value,ticket))return clear('Native property view changed; refresh its values.');
        clear(result.applied?'Property updated.':'No property change.');
        // The binding refreshes after the queue settles, even when native
        // publication retires this row. No optimistic value or auto-retry.
        return true;
      }catch(error){if(ticket!==serial||!mounted())return false;return clear('Property edit refused: '+String(error&&error.message||error));}
    }
    for(var i=0;i<descriptor.dimension;i++){
      if(dims){var dl=make('span',dims[i]);dl.className='motion-dim-label';fields.appendChild(dl);}
      var input=make('input','');input.type='number';input.step='1';input.className='pi scrub motion-val';input.dataset.step='1';
      // Mark the field native so the document scrub (ui.js) skips the legacy undo checkpoint and sends one final change on release (#2377).
      input.dataset.nativeOpacityScrub='static';
      input.setAttribute('aria-label',descriptor.property+' component '+(i+1));
      listen(input,'change',onChange);listen(input,'keydown',onKey);fields.appendChild(input);inputs.push(input);
    }
    if(unit){var ut=make('span',unit);ut.className='motion-unit';fields.appendChild(ut);}
    fields.appendChild(sw);fields.appendChild(status);listen(sw,'click',onSw);listen(sw,'dblclick',onSwDouble);listen(sw,'keydown',onSwKey);
    // Authored keys are read only for animated rows, to learn whether a key sits at the playhead.
    async function readKeyHere(value,ticket){
      var offset=0,found=false,pages=Math.ceil(descriptor.authored.keyCount/8)+1;
      for(var n=0;n<pages;n++){
        var q=frozen({instanceId:value.token.pin.instanceId,documentId:value.token.pin.documentId,atRevision:value.token.pin.contentRevision,documentSnapshotId:value.token.pin.documentSnapshotId,
          view:{kind:'authoredKeys',target:JSON.parse(JSON.stringify(target)),property:descriptor.property,offset:offset,limit:8}});
        var r=await ports.readProperty(q),t=r&&View.normalizeTarget(r.target);
        if(!current(value,ticket))return null;
        if(!r||r.kind!=='authoredKeys'||!t||targetKey(t)!==targetKey(target)||r.property!==descriptor.property||!Array.isArray(r.items)||
            ['instanceId','documentId','atRevision','documentSnapshotId'].some(function(k){return r[k]!==q[k];}))throw Error('Native key read is uncorrelated.');
        if(r.items.some(function(k){return k&&k.frame===value.frame;}))found=true;
        if(r.nextOffset===null||r.nextOffset===undefined)return found;
        if(!Number.isInteger(r.nextOffset)||r.nextOffset<=offset)throw Error('Native key continuation is invalid.');
        offset=r.nextOffset;
      }
      throw Error('Native key read exceeded its bound.');
    }
    async function refresh(){
      clear('Reading property value…');var ticket=serial,context,token;
      try{context=ports.readContext();token=View.captureContext(context);}catch(_){}
      if(!token)return clear('Native property value is unavailable.');
      var value={token:token,frame:context.frame};
      var selector=frozen({instanceId:token.pin.instanceId,documentId:token.pin.documentId,atRevision:token.pin.contentRevision,documentSnapshotId:token.pin.documentSnapshotId,
        view:{kind:'rawSample',target:JSON.parse(JSON.stringify(target)),property:descriptor.property,frame:value.frame}});
      try{
        if(!current(value,ticket))throw Error('Native property read is stale.');
        var result=await ports.readProperty(selector),actual=result&&View.normalizeTarget(result.target);
        if(!current(value,ticket))throw Error('Native property read became stale.');
        if(!exact(result,['instanceId','documentId','atRevision','documentSnapshotId','kind','target','property','dimension','frame','value'])||
            new TextEncoder().encode(JSON.stringify(result)).length>3072||!actual||targetKey(actual)!==targetKey(target)||
            ['instanceId','documentId','atRevision','documentSnapshotId'].some(function(k){return result[k]!==selector[k];})||
            result.kind!=='rawSample'||result.property!==descriptor.property||result.frame!==value.frame||result.dimension!==descriptor.dimension||
            !Array.isArray(result.value)||result.value.length!==descriptor.dimension||!Array.from(result.value).every(Number.isFinite))throw Error('Native property read is uncorrelated or malformed.');
        accepted=value;shown=result.value.slice();inputs.forEach(function(input,i){input.value=String(result.value[i]);});
        say('');controls();
        if(animated()){try{var here=await readKeyHere(value,ticket);if(here!==null&&ticket===serial&&mounted()){keyHere=here;controls();}}catch(_){if(ticket===serial&&mounted()){keyHere=null;controls();}}}
        return ticket===serial;
      }catch(error){if(ticket!==serial||!mounted())return false;return clear('Native property value unavailable: '+String(error&&error.message||error));}
    }
    controls();
    return Object.freeze({element:element,refresh:refresh,cancel:function(){return clear('Property read cancelled.');},dispose:function(){if(retired)return;retired=true;serial++;accepted=null;drop();bindings.forEach(function(b){b[0].removeEventListener(b[1],b[2]);});if(element.parentNode===container)container.removeChild(element);}});
  }
  return Object.freeze({create:create});
}));
