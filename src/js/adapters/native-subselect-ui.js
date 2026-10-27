/* C3 point-type presentation. Reuse the bound controls; the controller alone owns
 * disposable point selection and guards every native edit. No legacy state reads. */
(function(root,factory){
  var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeSubselectUI=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var bindings=new WeakMap(),ids=['btn-pt-corner','btn-pt-smooth','btn-pt-symmetric'];
  function refresh(root){var binding=bindings.get(root);if(binding)binding.refresh();}
  function mount(root,container){
    var row=root.document.getElementById('sp-pointtype-row'),old=bindings.get(root);
    if(old&&old.container===container&&old.row===row){old.refresh();return old;}
    if(old)old.retire();
    var buttons=ids.map(function(id){return root.document.getElementById(id);});
    if(!row||!row.parentNode||buttons.some(function(button){return !button||!row.contains(button);}))return null;
    var parent=row.parentNode,marker=root.document.createComment('native point-type position');
    var saved={hidden:row.hidden,display:row.style.display,disabled:buttons.map(function(button){return button.disabled;})},retired=false;
    parent.insertBefore(marker,row);container.appendChild(row);
    var binding={row:row,container:container,refresh:function(){
      if(retired||row.parentNode!==container)return;
      var controller=root.NemoNativeSubselectController,selection=controller&&controller.selectionState();
      var available=!!selection&&selection.count>0;
      row.hidden=!available;row.style.display=available?'flex':'none';
      buttons.forEach(function(button){button.disabled=!available;});
    },retire:function(){
      if(retired)return;retired=true;
      if(bindings.get(root)===binding)bindings.delete(root);
      if(row.parentNode===container){
        if(marker.parentNode===parent)parent.insertBefore(row,marker);
        row.hidden=saved.hidden;row.style.display=saved.display;
        buttons.forEach(function(button,i){button.disabled=saved.disabled[i];});
      }
      if(marker.parentNode===parent)parent.removeChild(marker);
    }};
    bindings.set(root,binding);binding.refresh();return binding;
  }
  return Object.freeze({mount:mount,refresh:refresh});
}));
