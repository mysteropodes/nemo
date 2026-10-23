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
    var doc=ports.document,serial=0,retired=false,accepted=null,busy=false,inputs=[],actions=[],bindings=[];
    var element=make('div',''),fields=make('div',''),status=make('span',''),toolbar=make('div','');
    element.className='native-motion-property-value';element.setAttribute('aria-label',descriptor.property+' value');
    fields.className='motion-fields';status.setAttribute('role','status');toolbar.setAttribute('role','toolbar');
    if(descriptor.authored&&descriptor.authored.expressionEnabled)element.appendChild(make('span','Base value before expression'));
    element.appendChild(fields);element.appendChild(toolbar);element.appendChild(status);container.appendChild(element);
    function make(tag,label){var n=doc.createElement(tag);n.textContent=label;return n;}
    function listen(node,type,fn){node.addEventListener(type,fn);bindings.push([node,type,fn]);}
    function mounted(){return !retired&&element.parentNode===container&&(!('isConnected'in container)||container.isConnected);}
    function current(value,ticket){try{var context=ports.readContext();return mounted()&&ticket===serial&&View.isCurrentContext(value.token,context)&&value.frame===context.frame;}catch(_){return false;}}
    function ownerCurrent(value,ticket){try{var context=ports.readContext(),pin=context&&context.pin;return mounted()&&ticket===serial&&View.captureContext(context)&&value.frame===context.frame&&['instanceId','documentId','lifecycleGeneration'].every(function(k){return pin[k]===value.token.pin[k];});}catch(_){return false;}}
    function controls(){
      var enabled=!!accepted&&!busy&&current(accepted,serial)&&typeof ports.editProperty==='function';
      inputs.forEach(function(n){n.disabled=!enabled;n.readOnly=!enabled;});
      actions.forEach(function(item){item.node.disabled=!enabled||item.animation&&descriptor.animatable!==true;});
    }
    function clear(message){serial++;accepted=null;busy=false;inputs.forEach(function(n){n.value='';});status.textContent=message;controls();return false;}
    function values(){return inputs.map(function(input){var text=String(input.value);if(!text.trim()||!Number.isFinite(Number(text)))throw Error('Enter finite numeric values.');return Number(text);});}
    function receipt(value){return exact(value,['applied','historyEntriesAdded'])&&typeof value.applied==='boolean'&&value.historyEntriesAdded===(value.applied?1:0);}
    async function apply(kind,animated){
      if(busy)return false;
      var value=accepted,ticket=serial,writer=ports.editProperty;
      if(!value||!current(value,ticket)||typeof writer!=='function')return clear('Native property edit is unavailable or stale.');
      if(kind!=='setValue'&&descriptor.animatable!==true)return false;
      var action={kind:kind,frame:value.frame};
      try{if(kind==='setValue'||kind==='setKey')action.values=values();else if(kind==='setAnimated')action.animated=animated;}
      catch(error){status.textContent=error.message;return false;}
      var payload=frozen({command:'object.property.edit',target:JSON.parse(JSON.stringify(target)),property:descriptor.property,action:action});
      busy=true;controls();status.textContent='Waiting for native property edit…';
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
      var input=make('input','');input.type='number';input.step='any';input.className='motion-val';
      input.setAttribute('aria-label',descriptor.property+' component '+(i+1));
      listen(input,'change',function(){return apply('setValue');});fields.appendChild(input);inputs.push(input);
    }
    function button(label,kind,animated){var node=make('button',label);node.type='button';listen(node,'click',function(){return apply(kind,animated);});toolbar.appendChild(node);actions.push({node:node,animation:true});}
    button('Add key','setKey');button('Remove key','removeKey');button('Enable animation','setAnimated',true);button('Disable animation','setAnimated',false);
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
        accepted=value;inputs.forEach(function(input,i){input.value=String(result.value[i]);});
        status.textContent=typeof ports.editProperty==='function'?'':'Native value is read-only.';controls();return true;
      }catch(error){if(ticket!==serial||!mounted())return false;return clear('Native property value unavailable: '+String(error&&error.message||error));}
    }
    controls();
    return Object.freeze({element:element,refresh:refresh,cancel:function(){return clear('Property read cancelled.');},dispose:function(){if(retired)return;retired=true;serial++;accepted=null;bindings.forEach(function(b){b[0].removeEventListener(b[1],b[2]);});if(element.parentNode===container)container.removeChild(element);}});
  }
  return Object.freeze({create:create});
}));
