/* C3 point-type presentation. Reuse the bound controls; the controller alone owns
 * disposable point selection and guards every native edit. No legacy state reads.
 * #2356: on a native project the legacy SM.setPointType entry is a fail-closed wrapper (it returns false without a word), so a click on
 * Corner / Smooth / Symmetric / Collapse is taken here, in the capture phase of the row, and goes to the controller's one native pointType edit. */
(function(root,factory){
  var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeSubselectUI=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var bindings=new WeakMap(),ids=['btn-pt-corner','btn-pt-smooth','btn-pt-symmetric','btn-pt-collapse'];
  function refresh(root){var binding=bindings.get(root);if(binding)binding.refresh();}
  function mount(root,container){
    var row=root.document.getElementById('sp-pointtype-row'),old=bindings.get(root);
    if(old&&old.container===container&&old.row===row){old.refresh();return old;}
    if(old)old.retire();
    var buttons=ids.map(function(id){return root.document.getElementById(id);});
    if(!row||!row.parentNode||buttons.some(function(button){return !button||!row.contains(button);}))return null;
    var parent=row.parentNode,marker=root.document.createComment('native point-type position');
    var types={'btn-pt-corner':'corner','btn-pt-smooth':'smooth','btn-pt-symmetric':'symmetric','btn-pt-collapse':'collapse'};
    function onClick(e){
      var node=e.target,button=node&&node.closest?node.closest('button'):node,type=button&&types[button.id];
      if(!type)return;
      if(e.stopImmediatePropagation)e.stopImmediatePropagation();
      if(e.preventDefault)e.preventDefault();
      var controller=root.NemoNativeSubselectController;
      if(!controller||typeof controller.setPointType!=='function'){if(root.showToast)root.showToast('Native subselect is unavailable.');return;}
      // A refusal (no selection, a stale frame, an unavailable owner) is reported by the controller's own toast; nothing falls back to the legacy writer.
      Promise.resolve(controller.setPointType(type)).catch(function(error){if(root.showToast)root.showToast(error&&error.message||String(error));});
    }
    row.addEventListener('click',onClick,true);
    var saved={hidden:row.hidden,display:row.style.display,disabled:buttons.map(function(button){return button.disabled;})},retired=false;
    parent.insertBefore(marker,row);container.appendChild(row);
    var binding={row:row,container:container,refresh:function(){
      if(retired||row.parentNode!==container)return;
      var controller=root.NemoNativeSubselectController,selection=controller&&controller.selectionState();
      var available=!!selection&&selection.count>0;
      row.hidden=!available;row.style.display=available?'flex':'none';
      // #2356: with one vertex selected its current type is the active button (Collapse is an action, never a state).
      buttons.forEach(function(button){button.disabled=!available;if(button.classList)button.classList.toggle('ac',available&&selection.pointType===types[button.id]);});
    },retire:function(){
      if(retired)return;retired=true;row.removeEventListener('click',onClick,true);
      if(bindings.get(root)===binding)bindings.delete(root);
      if(row.parentNode===container){
        if(marker.parentNode===parent)parent.insertBefore(row,marker);
        row.hidden=saved.hidden;row.style.display=saved.display;
        buttons.forEach(function(button,i){button.disabled=saved.disabled[i];if(button.classList)button.classList.remove('ac');});
      }
      if(marker.parentNode===parent)parent.removeChild(marker);
    }};
    bindings.set(root,binding);binding.refresh();return binding;
  }
  return Object.freeze({mount:mount,refresh:refresh});
}));
