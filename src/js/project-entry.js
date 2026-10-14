// Presentation adapter for real project tabs and Open/Resume canvas repaint.
(function(){
  function initializeProjectTabs(document){
    var list=document.getElementById('project-tabs-list');
    if(!list)return null;
    list.setAttribute('role','tablist');
    list.setAttribute('aria-label','Project tabs');
    return list;
  }
  // Consume resolved presentation rows only; project.js retains all tab state
  // and owns the ordinary switch/close/rename guards behind explicit callbacks.
  function renderProjectTabs(document,ports){
    var list=initializeProjectTabs(document);
    if(!list)return; // A missing view must not resolve rows or read dirty state.
    list.innerHTML='';
    ports.readRows().forEach(function(row){
      var el=document.createElement('div');
      el.className='project-tab'+(row.active?' act':'');
      el.dataset.tab=row.id;
      el.setAttribute('role','tab');
      el.setAttribute('aria-label',row.name);
      el.setAttribute('aria-selected',String(row.active));
      if(row.dirty)el.classList.add('dirty');
      var dot=document.createElement('span');
      dot.className='pt-dot';
      var dirtyDot=document.createElement('span');
      dirtyDot.className='pt-dirty';
      dirtyDot.title='Modifications non enregistrées';
      var nm=document.createElement('span');
      nm.className='pt-name';
      nm.textContent=row.name;
      var close=document.createElement('span');
      close.className='pt-close';
      close.textContent='×';
      close.title='Fermer l\'onglet';
      close.addEventListener('click',function(e){e.stopPropagation();ports.closeTab(row.id);});
      el.appendChild(dot);
      el.appendChild(dirtyDot);
      el.appendChild(nm);
      el.appendChild(close);
      el.addEventListener('click',function(){ports.switchTab(row.id);});
      el.addEventListener('dblclick',function(e){e.stopPropagation();ports.renameTab(row.id);});
      list.appendChild(el);
    });
  }
  function repaintProjectEntry(){
    // Native Open/Resume can restore live paths while the canvas stays blank
    // until an explicit render. Cross one complete presentation boundary
    // after hiding the start screen: the canvas ResizeObserver can resize
    // (and clear) the WebGPU surface after callbacks in that first frame.
    // Painting in the following frame keeps the imported scene visible while
    // leaving the viewport and stored document exactly as the import left them.
    requestAnimationFrame(function(){
      requestAnimationFrame(function(){
        if(window.SMEngineBridge&&SMEngineBridge.isEnabled())SMEngineBridge.renderNow();
        else if(typeof view!=='undefined'&&view)view.update();
      });
    });
  }
  window.SMProjectEntry={repaint:repaintProjectEntry,initializeTabs:initializeProjectTabs,renderTabs:renderProjectTabs};
})();
