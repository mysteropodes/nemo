/* Multi-target native Motion presentation with public native key actions. No local document writes.
 * Native queries retain the exact five-field selector. readSelections and active
 * context are UI selection ports; lifecycle/frame/view generations never enter it.
 * Every cursor is consumed as returned. Reads yield/cancel between whole pages. */
(function(root,factory){
  var api=factory(typeof module==='object'&&module.exports?require('./native-motion-property-view.js'):root.NemoNativeMotionPropertyView,
    typeof module==='object'&&module.exports?require('./native-motion-property-keys.js'):root.NemoNativeMotionPropertyKeys,
    typeof module==='object'&&module.exports?require('./native-motion-property-value.js'):root.NemoNativeMotionPropertyValue);
  if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeMotionPropertyPanel=api;
}(typeof globalThis==='object'?globalThis:this,function(View,Keys,Value){
  'use strict';
  var shortcuts={p:'position',a:'anchor',r:'rotation',s:'scale',t:'opacity'};
  var modes=['all','animated','modified','expressions','errors','effects','mattes'];
  function key(t){return JSON.stringify([t.contextId,t.layerUid,t.strokeId===undefined?null:t.strokeId]);}
  function create(container,ports){
    if(!View||!container||!ports||!ports.document||typeof ports.query!=='function'||typeof ports.readContext!=='function'||typeof ports.readSelection!=='function')throw new TypeError('Native Motion panel ports are unavailable');
    var doc=ports.document,retired=false,serial=0,client=View.createConsumer(ports),accepted=null;
    var propertySet=null,mode='all',contentOnly=false,collapsed=false,search='',scope='inspector',rowBindings=[],valueEditors=[],loading=false;
    var maxPages=ports.pageBudget===undefined?Infinity:ports.pageBudget;
    if(maxPages!==Infinity&&(!Number.isSafeInteger(maxPages)||maxPages<1))throw new TypeError('Invalid explicit native Motion read budget');
    var section=make('section',''),toolbar=make('div',''),status=make('div',''),body=make('div',''),controls=[];
    section.className='native-motion-property-panel';section.setAttribute('aria-label','Motion properties');
    toolbar.setAttribute('role','toolbar');status.setAttribute('role','status');body.className='native-motion-property-rows';
    section.appendChild(toolbar);section.appendChild(status);section.appendChild(body);container.appendChild(section);
    var keyEditor=null,keyEditors=[],selectionBatch=null,suppressKeyNotice=false,notifyingKeys=false;
    function keyNotice(reason){
      if(suppressKeyNotice||notifyingKeys||typeof ports.keyTimelineChanged!=='function')return;
      notifyingKeys=true;try{ports.keyTimelineChanged(reason);}catch(_){}finally{notifyingKeys=false;}
    }
    function sameView(a,b){return a.frame===b.frame&&a.viewGeneration===b.viewGeneration&&JSON.stringify(a.pin)===JSON.stringify(b.pin);}
    function acceptKeySelection(entry,frames){
      if(selectionBatch)return selectionBatch.get(entry)===frames||JSON.stringify(selectionBatch.get(entry))===JSON.stringify(frames);
      if(!validAccepted())return false;
      var count=frames.length,base=null;
      for(var item of keyEditors){
        if(item===entry||item.id===null)continue;
        var view=item.editor.readKeyTimeline();if(!view)return false;
        if(base&&!sameView(base,view))return false;base=view;count+=view.items.filter(function(k){return k.selected;}).length;
      }
      var own=entry.editor.readKeyTimeline();
      return !!own&&(!base||sameView(base,own))&&count<=32;
    }
    function makeKeyEditor(){
      if(!Keys||typeof Keys.create!=='function'||typeof ports.readProperty!=='function')return null;
      var entry={id:null,property:null,editor:null};
      entry.editor=Keys.create(section,{document:doc,readContext:ports.readContext,readProperty:ports.readProperty,
        keyTimelineChanged:keyNotice,acceptKeySelection:function(frames){return acceptKeySelection(entry,frames);},
        get editPropertyTrack(){return entry.property==='order'?null:ports.editPropertyTrack;},refresh:function(){return refresh(true);}});
      keyEditors.push(entry);return entry;
    }
    var initial=makeKeyEditor();if(initial)keyEditor=initial.editor;
    function clearKeys(){
      suppressKeyNotice=true;
      try{keyEditors.forEach(function(entry,index){if(index===0){entry.editor.clear();entry.id=null;entry.property=null;}else entry.editor.dispose();});keyEditors=keyEditors.slice(0,1);keyEditor=keyEditors.length?keyEditors[0].editor:null;}
      finally{suppressKeyNotice=false;keyNotice('reset');}
    }
    function openKeys(target,property){
      var id=JSON.stringify([key(target),property]),entry=keyEditors.find(function(item){return item.id===id;});
      if(!entry)entry=keyEditors.find(function(item){return item.id===null;})||makeKeyEditor();
      if(!entry)return false;entry.id=id;entry.property=property;keyEditor=entry.editor;return entry.editor.open(target,property);
    }
    function readKeyTimelines(){
      if(retired||!validAccepted())return null;
      var entries=keyEditors.filter(function(entry){return entry.id!==null;}),views=entries.map(function(entry){return entry.editor.readKeyTimeline();});
      if(!views.length||views.some(function(view){return !view||!view.isCurrent();})||views.some(function(view){return !sameView(views[0],view);}))return null;
      function isCurrent(){return !retired&&validAccepted()&&entries.length===keyEditors.filter(function(entry){return entry.id!==null;}).length&&entries.every(function(entry,i){return keyEditors.includes(entry)&&views[i].isCurrent();});}
      function selectKeys(keys){
        try{
        if(!isCurrent()||!Array.isArray(keys)||keys.length>32)return false;
        var proposed=new Map(entries.map(function(entry){return [entry,[]];})),seen=new Set();
        for(var address of keys){
          if(!address||typeof address!=='object'||Array.isArray(address)||Object.keys(address).length!==2||!Object.prototype.hasOwnProperty.call(address,'target')||!Object.prototype.hasOwnProperty.call(address,'frame')||!Number.isFinite(address.frame))return false;
          var t=address.target;
          if(!t||typeof t!=='object'||Array.isArray(t)||Object.keys(t).length!==3||!['kind','target','property'].every(function(name){return Object.prototype.hasOwnProperty.call(t,name);})||t.kind!=='property'||typeof t.property!=='string')return false;
          var stable=View.normalizeTarget(t.target);
          if(!stable)return false;
          var index=views.findIndex(function(view){return key(view.target)===key(stable)&&view.property===t.property;});
          if(index<0)return false;
          var id=JSON.stringify([index,address.frame]);if(seen.has(id))return false;seen.add(id);proposed.get(entries[index]).push(address.frame);
        }
        selectionBatch=proposed;suppressKeyNotice=true;
        try{
          if(!isCurrent()||views.some(function(view,i){return !view.canSelectFrames(proposed.get(entries[i]));})||!isCurrent())return false;
          for(var i=0;i<views.length;i++)if(!views[i].selectFrames(proposed.get(entries[i])))return false;
        }finally{selectionBatch=null;suppressKeyNotice=false;}
        keyNotice('selection');return true;
        }catch(_){return false;}
      }
      var value={pin:views[0].pin,frame:views[0].frame,viewGeneration:views[0].viewGeneration,
        tracks:views.map(function(view){return Object.freeze({target:view.target,property:view.property,items:view.items});})};
      Object.freeze(value.tracks);Object.defineProperties(value,{isCurrent:{value:isCurrent},selectKeys:{value:selectKeys}});return Object.freeze(value);
    }
    function captureSelectedKeys(){
      if(retired)return null;
      if(!keyEditors.some(function(entry){return entry.id!==null;}))return keyEditor?keyEditor.captureSelectedKeys():null;
      var view=readKeyTimelines();if(!view)return null;
      var keys=[];view.tracks.forEach(function(track){track.items.forEach(function(item){if(item.selected)keys.push(Object.freeze({target:Object.freeze({kind:'property',target:track.target,property:track.property}),frame:item.frame}));});});
      if(keys.length>32)return null;Object.freeze(keys);
      var value={pin:view.pin,frame:view.frame,viewGeneration:view.viewGeneration,keys:keys};
      var capturedAccepted=accepted,ticket=serial,entries=keyEditors.filter(function(entry){return entry.id!==null;}),ids=entries.map(function(entry){return entry.id;});
      var proofs=entries.map(function(entry){return entry.editor.captureSelectedKeys();});
      if(proofs.some(function(proof){return !proof||typeof proof.isSelectionCurrent!=='function'||proof.isSelectionCurrent()!==true;})||
        JSON.stringify(proofs.flatMap(function(proof){return proof.keys;}))!==JSON.stringify(keys)||view.isCurrent()!==true)return null;
      function isSelectionCurrent(lease){try{
        var now=ports.readContext(lease),p=now&&now.pin;
        return !retired&&accepted===capturedAccepted&&serial===ticket&&section.parentNode===container&&(!ports.isMotion||ports.isMotion())&&
          selectionInfo(lease).signature===capturedAccepted.selection.signature&&now&&now.frame===value.frame&&now.viewGeneration===value.viewGeneration&&p&&
          ['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'].every(function(k){return p[k]===value.pin[k];})&&
          entries.length===keyEditors.filter(function(entry){return entry.id!==null;}).length&&entries.every(function(entry,i){
            return keyEditors.includes(entry)&&entry.id===ids[i]&&proofs[i].isSelectionCurrent(lease)===true;});
      }catch(_){return false;}}
      // The strict public predicate still requires a currently presented view, including ready(false) refusals.
      Object.defineProperties(value,{isCurrent:{value:view.isCurrent},isSelectionCurrent:{value:isSelectionCurrent}});return Object.freeze(value);
    }
    function make(tag,text){var node=doc.createElement(tag);node.textContent=text;return node;}
    function notify(message){if(!retired)status.textContent=message;}
    function selectionInfo(lease){
      var value=ports.readSelection(lease),primary=value==null?null:View.normalizeTarget(value);
      if(value!=null&&!primary)throw new Error('Invalid active native target');
      var values=ports.readSelections?ports.readSelections(lease):(primary?[primary]:[]);
      if(!Array.isArray(values))throw new Error('Invalid native selection');
      var selected=[],seen=new Set();values.forEach(function(value){var t=View.normalizeTarget(value);if(!t||seen.has(key(t)))throw new Error('Invalid selected native target');seen.add(key(t));selected.push(t);});
      var contextId=ports.readContextId?ports.readContextId(lease):primary?primary.contextId:selected[0]&&selected[0].contextId;
      if(typeof contextId!=='string'||!contextId||primary&&primary.contextId!==contextId||selected.some(function(t){return t.contextId!==contextId;}))throw new Error('Uncorrelated native selection context');
      return Object.freeze({primary:primary,selected:Object.freeze(selected),contextId:contextId,signature:JSON.stringify([contextId,primary?key(primary):null,Array.from(seen).sort()])});
    }
    function current(token,ticket,signature){try{return !retired&&ticket===serial&&section.parentNode===container&&(!ports.isMotion||ports.isMotion())&&View.isCurrentContext(token,ports.readContext())&&(signature===undefined||selectionInfo().signature===signature);}catch(_){return false;}}
    function validAccepted(){return accepted&&current(accepted.token,serial,accepted.selection.signature);}
    function readPreference(){try{var value=ports.readPreference&&ports.readPreference('nemo-motion-filter');if(modes.includes(value))mode=value;}catch(_){}}
    function savePreference(){try{if(ports.writePreference)ports.writePreference('nemo-motion-filter',mode);}catch(_){}}
    function button(label,action){var node=make('button',label);node.type='button';node.addEventListener('click',action);toolbar.appendChild(node);controls.push([node,action]);return node;}
    function clearValues(){valueEditors.forEach(function(editor){editor.dispose();});valueEditors=[];}
    function clearBody(){clearValues();clearKeys();rowBindings.forEach(function(pair){pair[0].removeEventListener('click',pair[1]);});rowBindings=[];body.replaceChildren();}
    function labelTarget(t,token){return (ports.labelTarget&&ports.labelTarget(t,token.pin))||((t.strokeId===undefined?'Layer ':'Element ')+(t.strokeId===undefined?t.layerUid:t.strokeId));}
    function wantedTargets(targets,selection){
      if(!targets.some(function(item){return item.kind==='context'&&item.contextId===selection.contextId;}))throw new Error('Native active context is absent');
      var chosen=selection.selected.length?selection.selected:scope==='reveal'?targets.filter(function(item){return item.kind==='layer'&&item.target.contextId===selection.contextId;}).map(function(item){return item.target;}):selection.primary?[selection.primary]:[];
      var ids=new Set(chosen.map(key)),parents=new Set(chosen.filter(function(t){return t.strokeId===undefined;}).map(function(t){return JSON.stringify([t.contextId,t.layerUid]);}));
      chosen.forEach(function(t){if(!targets.some(function(item){return item.target&&key(item.target)===key(t);}))throw new Error('Native selection target is absent');});
      return targets.filter(function(item){return item.target&&item.target.contextId===selection.contextId&&(ids.has(key(item.target))||scope==='reveal'&&contentOnly&&item.kind==='element'&&parents.has(JSON.stringify([item.target.contextId,item.target.layerUid])));}).map(function(item){return item.target;});
    }
    function groupKey(targets){return JSON.stringify(targets.map(key));}
    function filteredRows(group){var rows=[],unknown=false;
      if(collapsed)return {rows:rows,unknown:false};
      group.rows.forEach(function(row){
        if(propertySet&&propertySet.indexOf(row.property)<0)return;
        var result=View.filterRow(row,mode);if(!result.available){unknown=true;return;}if(!result.matches)return;
        if(contentOnly){result=View.filterRow(row,'modified');if(!result.available){unknown=true;return;}if(!result.matches)return;}
        var name=row.controlName||(ports.labelProperty&&ports.labelProperty(row.property))||row.property;
        if(!search||String(name).toLowerCase().indexOf(search)>=0||String(labelTarget(group.target,accepted.token)).toLowerCase().indexOf(search)>=0)rows.push({row:row,label:name});
      });return {rows:rows,unknown:unknown};
    }
    // This ABI does not expose the owners required for complete baseline U.
    // Missing categories are not a positive capability declaration.
    function appendRevealLimits(fragment){
      if(!contentOnly||collapsed)return false;
      var block=make('div','');block.className='native-motion-unavailable-groups';block.setAttribute('role','note');block.dataset.completeness='partial';
      [['discreteTiming','Parent, blend and time remapping','Keyed owner facts are unavailable.'],
       ['textAnimators','Text animators','Keyed animator identities are unavailable.'],
       ['conditionalGeometry','Conditional and geometry properties','Required property kinds and prerequisites are unavailable.'],
       ['folderAncestors','Folder reveal','Folder identity and expansion are unavailable.']].forEach(function(entry){
        var line=make('div',entry[1]+': '+entry[2]);line.className='native-motion-unavailable-group';line.dataset.family=entry[0];block.appendChild(line);
      });
      fragment.appendChild(block);return true;
    }
    function paintRows(){
      if(!validAccepted()){accepted=null;clearBody();notify('Native properties are unavailable.');return false;}
      if(['errors','effects','mattes'].includes(mode)){clearBody();notify('This Motion filter is unavailable.');return false;}
      var fragment=make('div',''),unknown=false,shown=0,nextBindings=[],nextValues=[],selected=new Set(accepted.selection.selected.map(key));
      if(!selected.size&&accepted.selection.primary)selected.add(key(accepted.selection.primary));
      accepted.targets.forEach(function(item){
        if(item.kind==='context'){if(item.contextId===accepted.selection.contextId){var heading=make('div','Context '+item.contextId);heading.className='phdr';fragment.appendChild(heading);}return;}
        if(item.target.contextId!==accepted.selection.contextId)return;
        var t=item.target,pick=make('button',labelTarget(t,accepted.token));pick.type='button';pick.setAttribute('aria-pressed',String(selected.has(key(t))));
        var token=accepted.token,ticket=serial,signature=accepted.selection.signature;
        var choose=function(event){if(pick.parentNode===fragment&&fragment.parentNode===body)return select(t,token,ticket,signature,!!(event.shiftKey||event.ctrlKey||event.metaKey));};
        pick.addEventListener('click',choose);nextBindings.push([pick,choose]);fragment.appendChild(pick);
      });
      accepted.groups.forEach(function(group){
        var filtered=filteredRows(group);unknown=unknown||filtered.unknown;if(!filtered.rows.length)return;
        var title=make('div',labelTarget(group.target,accepted.token));title.className='native-motion-target-header';title.dataset.targetKey=key(group.target);fragment.appendChild(title);
        filtered.rows.forEach(function(entry){var row=entry.row,line=make('div',''),name=make('span',entry.label);line.className='native-motion-property-row';line.dataset.property=row.property;line.dataset.targetKey=key(group.target);line.appendChild(name);
          var f=row.authored,detail=f.availability!=='available'?'Property details unavailable':f.keyCount>0?'Animated ('+f.keyCount+' keys)':f.hasStaticOverride?'Static override':f.expressionEnabled?'Expression':'Default';line.appendChild(make('span',detail));
          if(keyEditor){
            var keysButton=make('button','Keys');keysButton.type='button';
            keysButton.setAttribute('aria-label','Edit keys for '+entry.label);
            var token=accepted.token,ticket=serial,signature=accepted.selection.signature;
            var onOpenKeys=function(){
              if(!current(token,ticket,signature)||keysButton.parentNode!==line)return;
              return openKeys(group.target,row.property);
            };
            keysButton.addEventListener('click',onOpenKeys);nextBindings.push([keysButton,onOpenKeys]);line.appendChild(keysButton);
          }
          if(f.availability==='available'&&f.expressionEnabled&&typeof ports.editProperty==='function'){
            var bakeButton=make('button','Bake expression'),bakeToken=accepted.token,bakeTicket=serial,bakeSignature=accepted.selection.signature;
            bakeButton.type='button';bakeButton.setAttribute('aria-label','Bake expression for '+entry.label);
            var onBake=async function(){
              function isCurrent(){return current(bakeToken,bakeTicket,bakeSignature)&&bakeButton.parentNode===line;}
              if(!isCurrent()||bakeButton.disabled)return;
              bakeButton.disabled=true;
              try{await ports.editProperty({command:'object.property.edit',target:group.target,property:row.property,action:{kind:'bake'}},bakeToken.pin,isCurrent);}
              catch(_){if(isCurrent())notify('Expression could not be baked.');}
              finally{if(isCurrent())bakeButton.disabled=false;}
            };
            bakeButton.addEventListener('click',onBake);nextBindings.push([bakeButton,onBake]);line.appendChild(bakeButton);
          }
          if(Value&&typeof ports.readProperty==='function'&&row.authored.availability==='available')nextValues.push({container:line,target:group.target,row:row});
          fragment.appendChild(line);shown++;});
      });
      // Unsupported native families do not become an empty global-U result.
      if(appendRevealLimits(fragment))unknown=true;
      if(!validAccepted()){accepted=null;clearBody();notify('Native properties are unavailable.');return false;}
      rowBindings.forEach(function(pair){pair[0].removeEventListener('click',pair[1]);});rowBindings=nextBindings;clearValues();body.replaceChildren(fragment);
      nextValues.forEach(function(item){var editor=Value.create(item.container,{document:doc,readContext:ports.readContext,readProperty:ports.readProperty,get editProperty(){return item.row.property==='order'?null:ports.editProperty;}},item.target,item.row);valueEditors.push(editor);Promise.resolve(editor.refresh()).catch(function(){editor.cancel();});});
      notify(collapsed?'Properties collapsed.':contentOnly&&unknown?'Partial reveal: some property groups are unavailable.':unknown?'Some property filters are unavailable.':shown?(keyEditor?'Select Keys to inspect native animation.':'Properties are read-only.'):accepted.groups.length?'No matching properties.':'Select a native target to view properties.');return true;
    }
    function paint(){try{return paintRows();}catch(_){accepted=null;clearBody();notify('Native properties are unavailable.');return false;}}
    function setFilter(value){if(retired||!modes.includes(value))return false;mode=value;savePreference();return paint();}
    function refreshPaint(){return refresh(false,true);}
    function reveal(property,add){
      if(retired||!Object.values(shortcuts).includes(property))return Promise.resolve(false);
      if(!add&&!collapsed&&propertySet&&propertySet.length===1&&propertySet[0]===property){propertySet=null;collapsed=true;return Promise.resolve(paint());}
      scope='reveal';collapsed=false;contentOnly=false;
      if(add){var all=new Set();if(accepted)accepted.groups.forEach(function(group){group.rows.forEach(function(row){if(Object.values(shortcuts).concat(['order']).includes(row.property))all.add(row.property);});});var base=Array.from(all);propertySet=propertySet?propertySet.slice():base.slice();var i=propertySet.indexOf(property);if(i<0)propertySet.push(property);else propertySet.splice(i,1);if(base.length===propertySet.length&&base.every(function(key){return propertySet.includes(key);}))propertySet=null;}else propertySet=[property];
      return refreshPaint();
    }
    function revealContent(){if(retired)return Promise.resolve(false);var showing=contentOnly&&!propertySet&&!collapsed;scope='reveal';propertySet=null;contentOnly=!showing;collapsed=showing;return refreshPaint();}
    function editable(node){while(node&&node!==container){if(['INPUT','SELECT','TEXTAREA'].includes(node.tagName)||node.isContentEditable)return true;node=node.parentNode;}return false;}
    function onKey(event){if(retired||event.defaultPrevented||event.altKey||event.ctrlKey||event.metaKey||editable(event.target))return;
      var k=String(event.key||'').toLowerCase();if(shortcuts[k]){event.preventDefault();return reveal(shortcuts[k],!!event.shiftKey);}if(k==='u'){event.preventDefault();return revealContent();}if(k==='e'||k==='m'){event.preventDefault();notify('This Motion reveal is unavailable.');}}
    async function yieldControl(){if(ports.yieldControl)await ports.yieldControl();else await new Promise(function(resolve){setTimeout(resolve,0);});}
    async function collect(view,token,ticket,signature){var items=[],seen=new Set(),categories=new Map();
      for(var pageNumber=0;pageNumber<maxPages;pageNumber++){
        if(!current(token,ticket,signature))throw new Error('Native UI read cancelled');
        var page=await client.load(view);if(!current(token,ticket,signature)||!page.available)throw new Error('Native property read is unavailable');
        page.items.forEach(function(item){var id=page.kind==='targets'?item.kind==='context'?'context:'+item.contextId:item.kind+':'+key(item.target):item.property;if(seen.has(id))throw new Error('Duplicate native page identity');seen.add(id);items.push(item);});
        page.unavailableCategories.forEach(function(c){var old=categories.get(c.category);if(old&&old.reason!==c.reason)throw new Error('Conflicting native category');categories.set(c.category,c);});
        var next=View.continuation(page);if(!next)return Object.freeze({items:Object.freeze(items),categories:Object.freeze(Array.from(categories.values()))});view=next;
        await yieldControl();
      }throw new Error('Explicit native read budget reached');
    }
    async function refresh(force,repaint){var ticket=serial,token,selection;
      try{token=View.captureContext(ports.readContext());selection=selectionInfo();}catch(_){token=null;}
      if(!token){accepted=null;clearBody();notify('Native properties are unavailable.');return false;}
      if(!force&&accepted&&current(accepted.token,ticket,accepted.selection.signature)){
        try{var wanted=wantedTargets(accepted.targets,selection);if(accepted.groupKey===groupKey(wanted))return repaint?paint():true;}catch(_){}
      }
      ticket=++serial;loading=true;accepted=null;clearBody();notify('Loading properties…');
      try{
        var discovery=await collect({kind:'targets',offset:0,limit:8},token,ticket,selection.signature),targets=discovery.items,wanted=wantedTargets(targets,selection),groups=[],categories=new Map(discovery.categories.map(function(c){return [c.category,c];}));
        for(var t of wanted){var page=await collect({kind:'properties',target:t,offset:0,limit:8},token,ticket,selection.signature);groups.push(Object.freeze({target:t,rows:page.items}));page.categories.forEach(function(c){categories.set(c.category,c);});}
        if(!current(token,ticket,selection.signature))throw new Error('Native selection changed');
        accepted=Object.freeze({token:token,targets:targets,groups:Object.freeze(groups),selection:selection,groupKey:groupKey(wanted),categories:Object.freeze(Array.from(categories.values()))});loading=false;return paint();
      }catch(_){if(!retired&&ticket===serial&&section.parentNode===container){loading=false;accepted=null;clearBody();notify('Native properties are unavailable.');}return false;}
    }
    async function select(target,token,ticket,signature,preserve){if(!current(token,ticket,signature)||typeof ports.selectTarget!=='function')return false;
      try{if(await ports.selectTarget(View.normalizeTarget(target),token.pin,Object.freeze({preserveSelection:preserve}))!==true){notify('Native selection is unavailable.');return false;}
        if(retired||ticket!==serial)return false;var selected=selectionInfo();if(!selected.primary||key(selected.primary)!==key(target)){notify('Native selection did not confirm.');return false;}
        scope='inspector';propertySet=null;contentOnly=false;collapsed=false;return refresh(true);
      }catch(_){if(!retired)notify('Native selection is unavailable.');return false;}}
    function cancel(){valueEditors.forEach(function(editor){editor.cancel();});var keyCancelled=keyEditors.length>0;clearKeys();if(retired||!loading)return keyCancelled;serial++;loading=false;accepted=null;clearBody();notify('Property read cancelled.');return true;}
    function dispose(){if(retired)return;retired=true;serial++;loading=false;client.dispose();accepted=null;clearBody();keyEditors.forEach(function(entry){entry.editor.dispose();});keyEditors=[];keyEditor=null;container.removeEventListener('keydown',onKey);searchInput.removeEventListener('input',onSearch);controls.forEach(function(pair){pair[0].removeEventListener('click',pair[1]);});if(section.parentNode===container)container.removeChild(section);}
    button('All',function(){scope='inspector';propertySet=null;contentOnly=false;collapsed=false;mode='all';savePreference();return refreshPaint();});
    Object.keys(shortcuts).forEach(function(k){button(k.toUpperCase(),function(){return reveal(shortcuts[k],false);});});button('U',function(){return revealContent();});
    ['animated','modified','expressions'].forEach(function(value){button(value,function(){return setFilter(value);});});
    button('E',function(){notify('Effect reveal is unavailable.');});button('M',function(){notify('Matte reveal is unavailable.');});button('Cancel',cancel);
    var searchInput=make('input','');searchInput.type='search';searchInput.setAttribute('aria-label','Search properties');function onSearch(){search=String(searchInput.value||'').toLowerCase();paint();}searchInput.addEventListener('input',onSearch);toolbar.appendChild(searchInput);
    container.addEventListener('keydown',onKey);readPreference();
    return Object.freeze({refresh:refresh,notice:notify,setFilter:setFilter,reveal:reveal,revealContent:revealContent,refuseReveal:function(){notify('This Motion reveal is unavailable.');return false;},cancel:cancel,dispose:dispose,element:section,readKeyTimeline:function(){return !retired&&validAccepted()&&keyEditor?keyEditor.readKeyTimeline():null;},readKeyTimelines:readKeyTimelines,captureSelectedKeys:captureSelectedKeys});
  }
  function createCaller(ports){var panel=null,retired=false;
    async function update(force){if(retired)return false;if(!ports.isMotion()){if(panel)panel.dispose();panel=null;return false;}if(!panel)panel=create(ports.container,ports);return panel.refresh(force);}
    return Object.freeze({update:update,notice:function(message){if(panel&&!retired)panel.notice(message);},shortcut:function(k,shift){if(retired||!ports.isMotion())return Promise.resolve(false);if(!panel)panel=create(ports.container,ports);k=String(k).toLowerCase();return shortcuts[k]?panel.reveal(shortcuts[k],!!shift):k==='u'?panel.revealContent():['e','m'].includes(k)?Promise.resolve(panel.refuseReveal()):Promise.resolve(false);},readKeyTimeline:function(){return !retired&&ports.isMotion()&&panel?panel.readKeyTimeline():null;},readKeyTimelines:function(){return !retired&&ports.isMotion()&&panel?panel.readKeyTimelines():null;},captureSelectedKeys:function(){return !retired&&panel?panel.captureSelectedKeys():null;},isMounted:function(){return !panel||panel.element.parentNode===ports.container;},setFilter:function(mode){return panel?panel.setFilter(mode):false;},cancel:function(){return panel?panel.cancel():false;},dispose:function(){retired=true;if(panel)panel.dispose();panel=null;}});
  }
  return Object.freeze({create:create,createCaller:createCaller});
}));
