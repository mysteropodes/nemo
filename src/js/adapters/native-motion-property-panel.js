/* Multi-target native Motion presentation with public native key actions. No local document writes.
 * Native queries retain the exact five-field selector. readSelections and active
 * context are UI selection ports; lifecycle/frame/view generations never enter it.
 * Every cursor is consumed as returned. Reads yield/cancel between whole pages. */
(function(root,factory){
  var api=factory(typeof module==='object'&&module.exports?require('./native-motion-property-view.js'):root.NemoNativeMotionPropertyView,
    typeof module==='object'&&module.exports?require('./native-motion-property-keys.js'):root.NemoNativeMotionPropertyKeys,
    typeof module==='object'&&module.exports?require('./native-motion-property-value.js'):root.NemoNativeMotionPropertyValue,
    typeof module==='object'&&module.exports?require('./native-motion-expression-editor.js'):root.NemoNativeMotionExpressionEditor);
  if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeMotionPropertyPanel=api;
}(typeof globalThis==='object'?globalThis:this,function(View,Keys,Value,Expr){
  'use strict';
  var shortcuts={p:'position',a:'anchor',r:'rotation',s:'scale',t:'opacity'};
  // Baseline propsFor() order (motion.js PROPS / PROPS_WITH_3D); anything else keeps its native order after these.
  var baselineOrder=['position','positionZ','anchor','rotation','rotationX','rotationY','scale','opacity','order'];
  var modes=['all','animated','modified','expressions','errors','effects','mattes'];
  function key(t){return JSON.stringify([t.contextId,t.layerUid,t.strokeId===undefined?null:t.strokeId]);}
  function create(container,ports){
    if(!View||!container||!ports||!ports.document||typeof ports.query!=='function'||typeof ports.readContext!=='function'||typeof ports.readSelection!=='function')throw new TypeError('Native Motion panel ports are unavailable');
    var doc=ports.document,retired=false,serial=0,client=View.createConsumer(ports),accepted=null;
    var propertySet=null,mode='all',contentOnly=false,collapsed=false,search='',scope='inspector',rowBindings=[],valueEditors=[],loading=false;
    var expressionRows=[],expressionFlight=null,expressionWanted=false,expressionScope=null,linkRows=[];
    var maxPages=ports.pageBudget===undefined?Infinity:ports.pageBudget;
    if(maxPages!==Infinity&&(!Number.isSafeInteger(maxPages)||maxPages<1))throw new TypeError('Invalid explicit native Motion read budget');
    var section=make('section',''),toolbar=make('div',''),status=make('div',''),body=make('div',''),controls=[];
    section.className='native-motion-property-panel';section.setAttribute('aria-label','Motion properties');
    // Baseline sidebar chrome is only the layer name + Transform group. The toolbar holds Cancel while a read is in flight;
    // status is a dim line for loading/unavailable states and stays hidden in steady state.
    toolbar.className='native-motion-toolbar';toolbar.hidden=true;toolbar.setAttribute('role','toolbar');
    status.className='native-motion-status';status.hidden=true;status.setAttribute('role','status');body.className='native-motion-property-rows';
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
      // Baseline-style on-demand drawer: an indented .lrow.motion-expr-editor row placed under its property row.
      var holder=make('div',''),entry={id:null,property:null,target:null,editor:null,holder:holder};
      holder.className='lrow motion-expr-editor motion-keys-drawer';holder.hidden=true;section.appendChild(holder);
      entry.editor=Keys.create(holder,{document:doc,readContext:ports.readContext,readProperty:ports.readProperty,
        keyTimelineChanged:keyNotice,acceptKeySelection:function(frames){return acceptKeySelection(entry,frames);},
        get editPropertyTrack(){return entry.property==='order'?null:ports.editPropertyTrack;},get showContextMenu(){return ports.showContextMenu;},translate:function(k){return ports.translate?ports.translate(k):k;},refresh:function(){return refresh(true);}});
      keyEditors.push(entry);return entry;
    }
    var initial=makeKeyEditor(),keysAvailable=!!initial,rowIndex=new Map();if(initial)keyEditor=initial.editor;
    // D18-C: the expression editor drawer, one at a time, seated under its row's slot exactly like a keys drawer (same slot, so the
    // grid side re-reserves the same height through keyNotice). Absent when the editor ports are not offered: f(x) then only bakes.
    var exprDrawer=(function(){
      if(!Expr||typeof Expr.create!=='function'||typeof ports.readProperty!=='function'||typeof ports.editProperty!=='function')return null;
      var holder=make('div',''),entry={id:null,holder:holder,editor:null};holder.className='lrow motion-expr-editor motion-expr-drawer';holder.hidden=true;section.appendChild(holder);
      entry.editor=Expr.create({document:doc,readContext:ports.readContext,readProperty:ports.readProperty,get editProperty(){return ports.editProperty;},
        get showContextMenu(){return ports.showContextMenu;},showToast:function(m){if(typeof ports.showToast==='function')ports.showToast(m);else notify(m);},translate:ports.translate,
        otherLayerNames:function(uid){return (typeof ports.layerNames==='function'?ports.layerNames():[]).filter(function(l){return l.layerUid!==uid&&typeof l.name==='string';}).map(function(l){return l.name;});},
        loadControls:function(target){return (typeof module==='object'&&module.exports?require('./native-motion-control-rows.js'):globalThis.NemoNativeMotionControlRows).loadControls({readContext:ports.readContext,query:ports.query},target);},layoutChanged:keyNotice});
      if(!entry.editor)return null;entry.editor.mount(holder);return entry;
    })();
    function exprPlace(){if(!exprDrawer||exprDrawer.id===null)return;var slot=rowIndex.get(exprDrawer.id);if(!slot||!body.contains(slot)){exprClose();return;}if(exprDrawer.holder.parentNode!==slot)slot.appendChild(exprDrawer.holder);exprDrawer.holder.hidden=false;}
    function exprClose(){if(!exprDrawer||exprDrawer.id===null)return;exprDrawer.id=null;exprDrawer.editor.close();exprDrawer.holder.hidden=true;if(exprDrawer.holder.parentNode!==section)section.appendChild(exprDrawer.holder);}
    function exprToggle(target,property,dimension){
      if(!exprDrawer)return false;var id=rowId(target,property);if(exprDrawer.id===id){exprClose();return true;}
      exprClose();exprDrawer.id=id;exprPlace();if(exprDrawer.id===null)return false;exprDrawer.editor.open(target,property,dimension);return true;
    }
    function stow(entry){entry.holder.hidden=true;if(entry.holder.parentNode!==section)section.appendChild(entry.holder);}
    function clearKeys(){
      suppressKeyNotice=true;
      try{keyEditors.forEach(function(entry,index){if(index===0){entry.editor.clear();entry.id=null;entry.property=null;entry.target=null;stow(entry);}else{entry.editor.dispose();if(entry.holder.parentNode)entry.holder.parentNode.removeChild(entry.holder);}});keyEditors=keyEditors.slice(0,1);keyEditor=keyEditors.length?keyEditors[0].editor:null;}
      finally{suppressKeyNotice=false;keyNotice('reset');}
    }
    function rowId(target,property){return JSON.stringify([key(target),property]);}
    function place(entry){var slot=rowIndex.get(entry.id);if(!slot||!body.contains(slot))return false;if(entry.holder.parentNode!==slot)slot.appendChild(entry.holder);entry.holder.hidden=false;return true;}
    // A repaint rebuilds the rows: re-seat each open drawer under its row, or close it when the row is filtered away.
    function replaceDrawers(){
      suppressKeyNotice=true;
      try{keyEditors.forEach(function(entry){if(entry.id===null)return;if(!place(entry)){entry.editor.clear();entry.id=null;entry.property=null;entry.target=null;stow(entry);}});}
      finally{suppressKeyNotice=false;}exprPlace();if(exprDrawer&&exprDrawer.id!==null)exprDrawer.editor.refresh();keyNotice('reset');
    }
    function openKeys(target,property){
      var id=rowId(target,property),entry=keyEditors.find(function(item){return item.id===id;});
      if(!entry)entry=keyEditors.find(function(item){return item.id===null;})||makeKeyEditor();
      if(!entry)return false;entry.id=id;entry.property=property;entry.target=target;keyEditor=entry.editor;place(entry);return entry.editor.open(target,property);
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
    function notify(message){if(retired)return;status.textContent=message||'';status.hidden=!message;}
    function setLoading(value){loading=value;toolbar.hidden=!value;}
    function tr(keyName,fallback){var value;try{value=ports.translate?ports.translate(keyName):undefined;}catch(_){value=undefined;}return typeof value==='string'&&value&&value!==keyName?value:fallback;}
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
    function clearBody(){linkRows.forEach(function(item){try{item.dispose();}catch(_){}});linkRows=[];releaseExpressionRows();clearValues();clearKeys();exprClose();rowBindings.forEach(function(pair){pair[0].removeEventListener(pair[2]||'click',pair[1]);});rowBindings=[];body.replaceChildren();}
    // Interim element names: the native read model carries no element names yet (read-model gap), so derive
    // "<kind> <n>" from the stable strokeId prefix and the element's 1-based ordinal among its layer's elements.
    function elementLabel(t){
      var prefix=String(t.strokeId).split(':')[0],base=prefix==='native-stroke'?tr('autoNameShape','Shape'):prefix==='native-text'?tr('autoNameText','Text'):prefix==='native-raster'?'Image':'Element';
      var siblings=accepted?accepted.targets.filter(function(item){return item.kind==='element'&&item.target.contextId===t.contextId&&item.target.layerUid===t.layerUid;}):[];
      var at=siblings.findIndex(function(item){return key(item.target)===key(t);});return at<0?base:base+' '+(at+1);
    }
    function labelTarget(t,token){
      var named=ports.labelTarget&&ports.labelTarget(t,token.pin);if(named)return named;
      return t.strokeId===undefined?'Layer '+t.layerUid:elementLabel(t);
    }
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
      });
      var rank=function(entry){var at=baselineOrder.indexOf(entry.row.property);return at<0?baselineOrder.length:at;};
      rows=rows.map(function(entry,i){return {entry:entry,i:i};}).sort(function(a,b){return rank(a.entry)-rank(b.entry)||a.i-b.i;}).map(function(item){return item.entry;});
      return {rows:rows,unknown:unknown};
    }
    // This ABI does not expose the owners required for complete baseline U. Missing categories are not a positive
    // capability declaration, so they surface only under the explicit U reveal, once, as one dim group-row footer;
    // the default view degrades silently (rows the ABI marks unavailable are simply omitted).
    function appendRevealLimits(fragment){
      if(!contentOnly||collapsed)return false;
      var footer=make('div','');footer.className='lrow motion-group-row native-motion-unavailable-groups';footer.setAttribute('role','note');footer.dataset.completeness='partial';
      [['discreteTiming','Parent, blend and time remapping'],['textAnimators','Text animators'],['conditionalGeometry','Conditional and geometry properties'],['folderAncestors','Folder reveal']].forEach(function(entry){
        var item=make('span',entry[1]);item.className='native-motion-unavailable-group';item.dataset.family=entry[0];footer.appendChild(item);
      });
      fragment.appendChild(footer);return true;
    }
    async function readExpressionState(item,token,ticket,signature,frame,flight){
      await readExpressionStateCore(item,token,ticket,signature,frame,flight);
      // The f(x) indicator mirrors the diagnostic line that the core just settled (visible error text = error).
      if(item.fx&&item.node.parentNode===item.line){item.fx.err=item.isError===true&&!item.node.hidden;item.fx.text=item.node.textContent;paintFx(item.fx);}
    }
    async function readExpressionStateCore(item,token,ticket,signature,frame,flight){
      item.isError=false;
      function valid(){var context;try{context=ports.readContext();}catch(_){return false;}return expressionFlight===flight&&!flight.superseded&&current(token,ticket,signature)&&context.frame===frame&&body.contains(item.line)&&item.node.parentNode===item.line;}
      if(!valid()||typeof ports.readProperty!=='function')return;
      var selector={instanceId:token.pin.instanceId,documentId:token.pin.documentId,atRevision:token.pin.contentRevision,documentSnapshotId:token.pin.documentSnapshotId,
        view:{kind:'expressionState',target:item.target,property:item.property,frame:frame}};
      try{
        var result=await ports.readProperty(selector);
        if(!valid())return;
        if(!result||result.kind!=='expressionState'||result.frame!==frame||result.property!==item.property||key(result.target)!==key(item.target)||
            ['instanceId','documentId','atRevision','documentSnapshotId'].some(function(k){return result[k]!==selector[k];})||
            !result.expression||typeof result.expression.present!=='boolean'||typeof result.expression.enabled!=='boolean'||!result.evaluation)throw Error('Invalid expression state');
        var evaluation=result.evaluation,diagnostic=evaluation.state==='evaluated'?evaluation.diagnostic:null;
        if(!result.expression.present){item.node.hidden=true;return;}
        item.node.hidden=false;item.node.textContent='';item.node.dataset.errorLine='-1';
        if(evaluation.state!=='evaluated'){item.node.textContent='Expression has not been evaluated for this frame.';return;}
        if(diagnostic===null){item.node.hidden=true;return;}
        item.isError=true;
        var translations={mustReturnNumber:'exprErrorMustReturnNumber',mustReturnNumberOrXY:'exprErrorMustReturnNumberOrXY',tooDeep:'exprErrorTooDeep',timeout:'exprErrorTimeout'};
        if(Object.prototype.hasOwnProperty.call(translations,diagnostic.kind)){
          var translated=typeof ports.translate==='function'?ports.translate(translations[diagnostic.kind]):null;
          item.node.textContent=typeof translated==='string'&&translated!==translations[diagnostic.kind]?translated:({mustReturnNumber:'Expression must return a number.',mustReturnNumberOrXY:'Expression must return a number or coordinates.',tooDeep:'Too many chained expression references.',timeout:'Expression took too long and was disabled.'})[diagnostic.kind];
        }else if(['runtime','syntax'].includes(diagnostic.kind)&&typeof diagnostic.message==='string'){
          item.node.textContent=diagnostic.message;
          if(Number.isInteger(diagnostic.line)&&diagnostic.line>0)item.node.dataset.errorLine=String(diagnostic.line);
        }else throw Error('Invalid expression diagnostic');
      }catch(_){if(valid()){item.isError=false;item.node.hidden=false;item.node.textContent='Expression diagnostics are unavailable.';item.node.dataset.errorLine='-1';}}
    }
    function paintFx(item){
      var bad=item.err===true,text=bad?item.text:'';
      item.fx.className='lico motion-expr-btn'+(item.on?' on':'')+(bad?' err':'');
      var label=bad?tr('exprErrorPrefix','Expression error: ')+text:item.on?tr('nativeExprActiveBake','Expression active \u2014 click to bake'):tr('nativeExprNone','No expression on this property');
      item.fx.title=label;item.fx.setAttribute('aria-label',label+' ('+item.label+')');
      item.fx.innerHTML='<svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2"><path d="M8 4c-2 0-3 1.2-3 3v10c0 2-1 3-3 3M16 4c2 0 3 1.2 3 3v10c0 2 1 3 3 3M9 12h6"/></svg>';
    }
    function invalidateExpressionDisplay(){
      expressionRows.forEach(function(item){item.node.hidden=true;item.node.textContent='';item.node.dataset.errorLine='-1';if(item.fx){item.fx.err=false;paintFx(item.fx);}});
    }
    function releaseExpressionRows(){
      invalidateExpressionDisplay();expressionRows=[];expressionScope=null;expressionWanted=false;
      // Keep the live flight marker until settlement. Retiring rows must not
      // admit another overlapping request batch through the old panel API.
      if(expressionFlight)expressionFlight.superseded=true;
    }
    function refreshExpressionStates(){
      if(retired||!validAccepted()||!expressionRows.length){invalidateExpressionDisplay();expressionWanted=false;return;}
      var context;try{context=ports.readContext();}catch(_){invalidateExpressionDisplay();return;}
      if(!context||!View.isCurrentContext(accepted.token,context)){invalidateExpressionDisplay();return;}
      var scopeKey=JSON.stringify([context.pin,context.frame,context.viewGeneration,accepted.selection.signature]);
      if(expressionScope!==scopeKey){invalidateExpressionDisplay();expressionScope=scopeKey;}
      if(expressionFlight){expressionWanted=true;expressionFlight.superseded=true;return;}
      var flight={},token=accepted.token,ticket=serial,signature=accepted.selection.signature;
      expressionWanted=false;expressionFlight=flight;
      // One finite visible-row batch; paints while it runs coalesce into one
      // latest-context read after settlement, preserving controls and drafts.
      Promise.all(expressionRows.map(function(item){return readExpressionState(item,token,ticket,signature,context.frame,flight);})).finally(function(){
        if(expressionFlight!==flight)return;
        expressionFlight=null;
        if(expressionWanted){expressionWanted=false;refreshExpressionStates();}
      });
    }
    function paintRows(){
      if(!validAccepted()){accepted=null;clearBody();notify('Native properties are unavailable.');return false;}
      if(['errors','effects','mattes'].includes(mode)){clearBody();notify('This Motion filter is unavailable.');return false;}
      var fragment=make('div',''),unknown=false,nextBindings=[],nextValues=[],nextExpressions=[],nextRows=new Map(),lastLayer=null,nextLinks=[];
      // The Path and Time rows (baseline Parent/Follow Path/Time block) belong to ONE active layer, never to a multi-layer selection.
      var onlyLayer=new Set(accepted.groups.map(function(g){return key({contextId:g.target.contextId,layerUid:g.target.layerUid});})).size===1;
      if(!accepted.groups.length){var none=make('div',tr('noLayerSelected','No layer selected'));none.className='motion-props-layername';fragment.appendChild(none);}
      accepted.groups.forEach(function(group){
        var filtered=filteredRows(group);unknown=unknown||filtered.unknown;
        var target=group.target,layerKey=JSON.stringify([target.contextId,target.layerUid]);
        if(layerKey!==lastLayer){lastLayer=layerKey;var name=make('div',labelTarget({contextId:target.contextId,layerUid:target.layerUid},accepted.token));name.className='motion-props-layername';name.dataset.layerUid=target.layerUid;fragment.appendChild(name);
          if(onlyLayer&&typeof ports.layerLinkRows==='function'){try{var linked=ports.layerLinkRows({contextId:target.contextId,layerUid:target.layerUid});if(linked){fragment.appendChild(linked.node);nextLinks.push(linked);}}catch(_){}}}
        var isElement=target.strokeId!==undefined,title=make('div',''),label=make('span',isElement?tr('hdrTransformElement','Transform (element)'):tr('hdrTransform','Transform'));
        title.className='lrow motion-group-row';title.dataset.targetKey=key(target);if(isElement)label.title=labelTarget(target,accepted.token);title.appendChild(label);
        // Baseline hide-unanimated funnel: same persisted nemo-motion-filter preference, 'animated' <-> 'all'.
        var hiding=mode==='animated',funnel=make('span',''),fToken=accepted.token,fTicket=serial,fSignature=accepted.selection.signature;
        funnel.className='motion-filter-btn'+(hiding?' on':'');funnel.title=hiding?tr('motionFilterShowAll','Show all properties'):tr('motionFilterHideUnanimated','Show only animated properties');
        funnel.innerHTML='<svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 5h18l-7 8v6l-4-2v-4z"/></svg>';
        var onFunnel=function(event){if(event&&event.stopPropagation)event.stopPropagation();if(!current(fToken,fTicket,fSignature))return false;return setFilter(mode==='animated'?'all':'animated');};
        funnel.addEventListener('click',onFunnel);nextBindings.push([funnel,onFunnel]);title.appendChild(funnel);fragment.appendChild(title);
        filtered.rows.forEach(function(entry){var row=entry.row,line=make('div',''),name=make('div',''),nameText=make('span',entry.label);line.className='lrow motion-prop-row';line.dataset.property=row.property;line.dataset.targetKey=key(group.target);
          var f=row.authored;
          var token=accepted.token,ticket=serial,signature=accepted.selection.signature;
          if(f.availability==='available'&&(typeof ports.editProperty==='function'||typeof ports.readProperty==='function')){
            // Baseline f(x) indicator: .on when an expression is enabled, .err from the live diagnostic; click offers Bake.
            var fx=make('div',''),fxItem={fx:fx,on:f.expressionEnabled===true,err:false,line:line,label:entry.label};
            fx.setAttribute('role','button');fx.setAttribute('tabindex','0');paintFx(fxItem);
            var offer=function(event){
              if(event&&event.stopPropagation)event.stopPropagation();if(event&&event.preventDefault&&event.type==='contextmenu')event.preventDefault();
              if(!fxItem.on||typeof ports.editProperty!=='function'||typeof ports.showContextMenu!=='function'||!current(token,ticket,signature)||!body.contains(line))return false;
              var action=async function(){
                function isCurrent(){return current(token,ticket,signature)&&body.contains(line);}
                if(!isCurrent()||fxItem.busy)return;fxItem.busy=true;
                try{await ports.editProperty({command:'object.property.edit',target:group.target,property:row.property,action:{kind:'bake'}},token.pin,isCurrent);}
                catch(_){if(isCurrent())notify('Expression could not be baked.');}
                finally{fxItem.busy=false;}
              };
              ports.showContextMenu(event&&event.clientX||0,event&&event.clientY||0,[{label:tr('exprBake','Bake expression'),action:action}]);return true;
            };
            var onFxKey=function(event){if(event&&(event.key==='Enter'||event.key===' ')){if(event.preventDefault)event.preventDefault();return offer(event);}};
            // D18-C baseline: a click on f(x) opens/closes the expression editor; Bake moves to the context menu (and stays the click without an editor).
            var toggle=function(event){
              if(!exprDrawer)return offer(event);
              if(event&&event.stopPropagation)event.stopPropagation();
              if(!current(token,ticket,signature)||!body.contains(line))return false;
              return exprToggle(group.target,row.property,row.dimension);
            };
            var onFxToggleKey=function(event){if(event&&(event.key==='Enter'||event.key===' ')){if(event.preventDefault)event.preventDefault();return toggle(event);}};
            fx.addEventListener('click',toggle);fx.addEventListener('contextmenu',offer);fx.addEventListener('keydown',onFxToggleKey);
            nextBindings.push([fx,toggle],[fx,offer,'contextmenu'],[fx,onFxToggleKey,'keydown']);line.appendChild(fx);
          }
          name.className='lnm motion-prop-name';name.appendChild(nameText);line.appendChild(name);
          var slot=make('div','');slot.className='native-motion-property-slot';line.appendChild(slot);
          var drawerSlot=make('div','');drawerSlot.className='motion-keys-slot';nextRows.set(rowId(group.target,row.property),drawerSlot);
          var openDrawer=keysAvailable?function(){if(!current(token,ticket,signature)||!body.contains(line))return false;return openKeys(group.target,row.property);}:null;
          if(Value&&typeof ports.readProperty==='function'&&row.authored.availability==='available')nextValues.push({container:slot,target:group.target,row:row,openKeys:openDrawer});
          if(typeof ports.readProperty==='function'&&f.availability==='available'){
            var diagnostic=make('span','');diagnostic.className='motion-expr-error';diagnostic.setAttribute('role','status');diagnostic.hidden=true;line.appendChild(diagnostic);
            nextExpressions.push({node:diagnostic,line:line,target:group.target,property:row.property,fx:typeof fxItem==='object'?fxItem:null});
          }
          line._nemoPick=Object.freeze({target:group.target,property:row.property,controlType:row.controlType===undefined?null:row.controlType,controlName:row.controlName===undefined?null:row.controlName});
          fragment.appendChild(line);fragment.appendChild(drawerSlot);});
      });
      // Unsupported native families do not become an empty global-U result.
      if(appendRevealLimits(fragment))unknown=true;
      if(!validAccepted()){accepted=null;clearBody();notify('Native properties are unavailable.');return false;}
      rowBindings.forEach(function(pair){pair[0].removeEventListener(pair[2]||'click',pair[1]);});rowBindings=nextBindings;clearValues();linkRows.forEach(function(item){try{item.dispose();}catch(_){}});linkRows=nextLinks;body.replaceChildren(fragment);rowIndex=nextRows;replaceDrawers();
      nextValues.forEach(function(item){var editor=Value.create(item.container,{document:doc,readContext:ports.readContext,readProperty:ports.readProperty,dimLabels:ports.dimLabels,translate:ports.translate,get editProperty(){return item.row.property==='order'?null:ports.editProperty;},openKeys:item.openKeys},item.target,item.row);valueEditors.push(editor);Promise.resolve(editor.refresh()).catch(function(){editor.cancel();});});
      expressionRows=nextExpressions;refreshExpressionStates();
      notify('');return true;
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
        try{var wanted=wantedTargets(accepted.targets,selection);if(accepted.groupKey===groupKey(wanted)){if(repaint)return paint();refreshExpressionStates();linkRows.forEach(function(item){try{item.refresh();}catch(_){}});return true;}}catch(_){}
      }
      ticket=++serial;setLoading(true);accepted=null;clearBody();notify('Loading properties…');
      try{
        var discovery=await collect({kind:'targets',offset:0,limit:8},token,ticket,selection.signature),targets=discovery.items,wanted=wantedTargets(targets,selection),groups=[],categories=new Map(discovery.categories.map(function(c){return [c.category,c];}));
        for(var t of wanted){var page=await collect({kind:'properties',target:t,offset:0,limit:8},token,ticket,selection.signature);groups.push(Object.freeze({target:t,rows:page.items}));page.categories.forEach(function(c){categories.set(c.category,c);});}
        if(!current(token,ticket,selection.signature))throw new Error('Native selection changed');
        accepted=Object.freeze({token:token,targets:targets,groups:Object.freeze(groups),selection:selection,groupKey:groupKey(wanted),categories:Object.freeze(Array.from(categories.values()))});setLoading(false);return paint();
      }catch(_){if(!retired&&ticket===serial&&section.parentNode===container){setLoading(false);accepted=null;clearBody();notify('Native properties are unavailable.');}return false;}
    }
    async function select(target,token,ticket,signature,preserve){if(!current(token,ticket,signature)||typeof ports.selectTarget!=='function')return false;
      try{if(await ports.selectTarget(View.normalizeTarget(target),token.pin,Object.freeze({preserveSelection:preserve}))!==true){notify('Native selection is unavailable.');return false;}
        if(retired||ticket!==serial)return false;var selected=selectionInfo();if(!selected.primary||key(selected.primary)!==key(target)){notify('Native selection did not confirm.');return false;}
        scope='inspector';propertySet=null;contentOnly=false;collapsed=false;return refresh(true);
      }catch(_){if(!retired)notify('Native selection is unavailable.');return false;}}
    function cancel(){valueEditors.forEach(function(editor){editor.cancel();});var keyCancelled=keyEditors.length>0;clearKeys();if(retired||!loading)return keyCancelled;serial++;setLoading(false);accepted=null;clearBody();notify('Property read cancelled.');return true;}
    function dispose(){if(retired)return;retired=true;serial++;setLoading(false);client.dispose();accepted=null;clearBody();keyEditors.forEach(function(entry){entry.editor.dispose();});keyEditors=[];keyEditor=null;if(exprDrawer){exprDrawer.editor.dispose();exprDrawer.holder.remove&&exprDrawer.holder.remove();}container.removeEventListener('keydown',onKey);controls.forEach(function(pair){pair[0].removeEventListener('click',pair[1]);});if(section.parentNode===container)container.removeChild(section);}
    button('Cancel',cancel);
    function setSearch(value){if(retired)return false;search=String(value||'').toLowerCase();return paint();}
    container.addEventListener('keydown',onKey);readPreference();
    return Object.freeze({refresh:refresh,notice:notify,setFilter:setFilter,setSearch:setSearch,select:function(target){return validAccepted()?select(target,accepted.token,serial,accepted.selection.signature,false):Promise.resolve(false);},reveal:reveal,revealContent:revealContent,refuseReveal:function(){notify('This Motion reveal is unavailable.');return false;},cancel:cancel,dispose:dispose,element:section,readKeyTimeline:function(){return !retired&&validAccepted()&&keyEditor?keyEditor.readKeyTimeline():null;},readKeyTimelines:readKeyTimelines,captureSelectedKeys:captureSelectedKeys});
  }
  function createCaller(ports){var panel=null,retired=false;
    async function update(force){if(retired)return false;if(!ports.isMotion()){if(panel)panel.dispose();panel=null;return false;}if(!panel)panel=create(ports.container,ports);return panel.refresh(force);}
    return Object.freeze({update:update,notice:function(message){if(panel&&!retired)panel.notice(message);},shortcut:function(k,shift){if(retired||!ports.isMotion())return Promise.resolve(false);if(!panel)panel=create(ports.container,ports);k=String(k).toLowerCase();return shortcuts[k]?panel.reveal(shortcuts[k],!!shift):k==='u'?panel.revealContent():['e','m'].includes(k)?Promise.resolve(panel.refuseReveal()):Promise.resolve(false);},readKeyTimeline:function(){return !retired&&ports.isMotion()&&panel?panel.readKeyTimeline():null;},readKeyTimelines:function(){return !retired&&ports.isMotion()&&panel?panel.readKeyTimelines():null;},captureSelectedKeys:function(){return !retired&&panel?panel.captureSelectedKeys():null;},isMounted:function(){return !panel||panel.element.parentNode===ports.container;},setFilter:function(mode){return panel?panel.setFilter(mode):false;},setSearch:function(value){return panel?panel.setSearch(value):false;},select:function(target){return panel?panel.select(target):Promise.resolve(false);},cancel:function(){return panel?panel.cancel():false;},dispose:function(){retired=true;if(panel)panel.dispose();panel=null;}});
  }
  return Object.freeze({create:create,createCaller:createCaller});
}));
