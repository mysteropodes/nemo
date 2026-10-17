/* Read-only multi-target native Motion presentation. No document writes.
 * Native queries retain the exact five-field selector. readSelections and active
 * context are UI selection ports; lifecycle/frame/view generations never enter it.
 * Every cursor is consumed as returned. Reads yield/cancel between whole pages. */
(function(root,factory){
  var api=factory(typeof module==='object'&&module.exports?require('./native-motion-property-view.js'):root.NemoNativeMotionPropertyView);
  if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeMotionPropertyPanel=api;
}(typeof globalThis==='object'?globalThis:this,function(View){
  'use strict';
  var shortcuts={p:'position',a:'anchor',r:'rotation',s:'scale',t:'opacity'};
  var modes=['all','animated','modified','expressions','errors','effects','mattes'];
  function key(t){return JSON.stringify([t.contextId,t.layerUid,t.strokeId===undefined?null:t.strokeId]);}
  function create(container,ports){
    if(!View||!container||!ports||!ports.document||typeof ports.query!=='function'||typeof ports.readContext!=='function'||typeof ports.readSelection!=='function')throw new TypeError('Native Motion panel ports are unavailable');
    var doc=ports.document,retired=false,serial=0,client=View.createConsumer(ports),accepted=null;
    var propertySet=null,mode='all',contentOnly=false,collapsed=false,search='',scope='inspector',rowBindings=[],loading=false;
    var maxPages=ports.pageBudget===undefined?Infinity:ports.pageBudget;
    if(maxPages!==Infinity&&(!Number.isSafeInteger(maxPages)||maxPages<1))throw new TypeError('Invalid explicit native Motion read budget');
    var section=make('section',''),toolbar=make('div',''),status=make('div',''),body=make('div',''),controls=[];
    section.className='native-motion-property-panel';section.setAttribute('aria-label','Motion properties');
    toolbar.setAttribute('role','toolbar');status.setAttribute('role','status');body.className='native-motion-property-rows';
    section.appendChild(toolbar);section.appendChild(status);section.appendChild(body);container.appendChild(section);
    function make(tag,text){var node=doc.createElement(tag);node.textContent=text;return node;}
    function notify(message){if(!retired)status.textContent=message;}
    function selectionInfo(){
      var value=ports.readSelection(),primary=value==null?null:View.normalizeTarget(value);
      if(value!=null&&!primary)throw new Error('Invalid active native target');
      var values=ports.readSelections?ports.readSelections():(primary?[primary]:[]);
      if(!Array.isArray(values))throw new Error('Invalid native selection');
      var selected=[],seen=new Set();values.forEach(function(value){var t=View.normalizeTarget(value);if(!t||seen.has(key(t)))throw new Error('Invalid selected native target');seen.add(key(t));selected.push(t);});
      var contextId=ports.readContextId?ports.readContextId():primary?primary.contextId:selected[0]&&selected[0].contextId;
      if(typeof contextId!=='string'||!contextId||primary&&primary.contextId!==contextId||selected.some(function(t){return t.contextId!==contextId;}))throw new Error('Uncorrelated native selection context');
      return Object.freeze({primary:primary,selected:Object.freeze(selected),contextId:contextId,signature:JSON.stringify([contextId,primary?key(primary):null,Array.from(seen).sort()])});
    }
    function current(token,ticket,signature){try{return !retired&&ticket===serial&&section.parentNode===container&&(!ports.isMotion||ports.isMotion())&&View.isCurrentContext(token,ports.readContext())&&(signature===undefined||selectionInfo().signature===signature);}catch(_){return false;}}
    function validAccepted(){return accepted&&current(accepted.token,serial,accepted.selection.signature);}
    function readPreference(){try{var value=ports.readPreference&&ports.readPreference('nemo-motion-filter');if(modes.includes(value))mode=value;}catch(_){}}
    function savePreference(){try{if(ports.writePreference)ports.writePreference('nemo-motion-filter',mode);}catch(_){}}
    function button(label,action){var node=make('button',label);node.type='button';node.addEventListener('click',action);toolbar.appendChild(node);controls.push([node,action]);return node;}
    function clearBody(){rowBindings.forEach(function(pair){pair[0].removeEventListener('click',pair[1]);});rowBindings=[];body.replaceChildren();}
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
      var fragment=make('div',''),unknown=false,shown=0,nextBindings=[],selected=new Set(accepted.selection.selected.map(key));
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
          var f=row.authored,detail=f.availability!=='available'?'Property details unavailable':f.keyCount>0?'Animated ('+f.keyCount+' keys)':f.hasStaticOverride?'Static override':f.expressionEnabled?'Expression':'Default';line.appendChild(make('span',detail));fragment.appendChild(line);shown++;});
      });
      // Unsupported native families do not become an empty global-U result.
      if(appendRevealLimits(fragment))unknown=true;
      if(!validAccepted()){accepted=null;clearBody();notify('Native properties are unavailable.');return false;}
      rowBindings.forEach(function(pair){pair[0].removeEventListener('click',pair[1]);});rowBindings=nextBindings;body.replaceChildren(fragment);
      notify(collapsed?'Properties collapsed.':contentOnly&&unknown?'Partial reveal: some property groups are unavailable.':unknown?'Some property filters are unavailable.':shown?'Properties are read-only.':accepted.groups.length?'No matching properties.':'Select a native target to view properties.');return true;
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
    function cancel(){if(retired||!loading)return false;serial++;loading=false;accepted=null;clearBody();notify('Property read cancelled.');return true;}
    function dispose(){if(retired)return;retired=true;serial++;loading=false;client.dispose();accepted=null;clearBody();container.removeEventListener('keydown',onKey);searchInput.removeEventListener('input',onSearch);controls.forEach(function(pair){pair[0].removeEventListener('click',pair[1]);});if(section.parentNode===container)container.removeChild(section);}
    button('All',function(){scope='inspector';propertySet=null;contentOnly=false;collapsed=false;mode='all';savePreference();return refreshPaint();});
    Object.keys(shortcuts).forEach(function(k){button(k.toUpperCase(),function(){return reveal(shortcuts[k],false);});});button('U',function(){return revealContent();});
    ['animated','modified','expressions'].forEach(function(value){button(value,function(){return setFilter(value);});});
    button('E',function(){notify('Effect reveal is unavailable.');});button('M',function(){notify('Matte reveal is unavailable.');});button('Cancel',cancel);
    var searchInput=make('input','');searchInput.type='search';searchInput.setAttribute('aria-label','Search properties');function onSearch(){search=String(searchInput.value||'').toLowerCase();paint();}searchInput.addEventListener('input',onSearch);toolbar.appendChild(searchInput);
    container.addEventListener('keydown',onKey);readPreference();
    return Object.freeze({refresh:refresh,setFilter:setFilter,reveal:reveal,revealContent:revealContent,refuseReveal:function(){notify('This Motion reveal is unavailable.');return false;},cancel:cancel,dispose:dispose,element:section});
  }
  function createCaller(ports){var panel=null,retired=false;
    async function update(force){if(retired)return false;if(!ports.isMotion()){if(panel)panel.dispose();panel=null;return false;}if(!panel)panel=create(ports.container,ports);return panel.refresh(force);}
    return Object.freeze({update:update,shortcut:function(k,shift){if(retired||!ports.isMotion())return Promise.resolve(false);if(!panel)panel=create(ports.container,ports);k=String(k).toLowerCase();return shortcuts[k]?panel.reveal(shortcuts[k],!!shift):k==='u'?panel.revealContent():['e','m'].includes(k)?Promise.resolve(panel.refuseReveal()):Promise.resolve(false);},isMounted:function(){return !panel||panel.element.parentNode===ports.container;},setFilter:function(mode){return panel?panel.setFilter(mode):false;},cancel:function(){return panel?panel.cancel():false;},dispose:function(){retired=true;if(panel)panel.dispose();panel=null;}});
  }
  return Object.freeze({create:create,createCaller:createCaller});
}));
