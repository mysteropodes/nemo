// Project entry presentation: tabs, recent rows and Open/Resume canvas repaint.
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
  // Paths, recent storage and document operations stay in the caller. This view
  // only formats display rows and delegates the two ordinary user actions.
  function renderRecentProjects(document,ports){
    var wrap=document.getElementById('start-recent-list');
    if(!wrap)return;
    var list=ports.readRows();
    wrap.innerHTML='';
    if(!list.length){
      wrap.innerHTML='<div class="start-empty">'+ports.translate('startNoRecents')+'</div>';
      return;
    }
    list.forEach(function(r){
      var row=document.createElement('div');
      row.className='start-recent-row';
      var icon=document.createElement('span');
      icon.className='start-recent-dot';
      var info=document.createElement('div');
      var nm=document.createElement('div');
      nm.className='start-recent-name';
      nm.textContent=r.name;
      var meta=document.createElement('div');
      meta.className='start-recent-meta';
      var dims=r.canvasW?(r.canvasW+'×'+r.canvasH+' · '+r.fps+'fps · '):'';
      meta.textContent=dims+new Date(r.lastOpened).toLocaleDateString();
      info.appendChild(nm);
      info.appendChild(meta);
      var x=document.createElement('div');
      x.className='start-recent-x';
      x.textContent='×';
      x.title=ports.translate('startRemoveFromList');
      x.addEventListener('click',function(e){e.stopPropagation();ports.removeRow(r.key);});
      row.appendChild(icon);
      row.appendChild(info);
      row.appendChild(x);
      row.addEventListener('click',function(){ports.openRow(r.key);});
      wrap.appendChild(row);
    });
  }
  // This guard owns only one form interaction. The caller owns creation and
  // rechecks the current predicate before any native/document mutation.
  function bindNewProjectForm(document,ports){
    var newPanel=document.getElementById('start-newpanel');
    var admissionMessage=document.createElement('div');
    admissionMessage.setAttribute('role','alert');
    admissionMessage.style.display='none';
    admissionMessage.style.color='#ffb5a8';
    admissionMessage.style.fontSize='12px';
    admissionMessage.style.marginTop='8px';
    newPanel.appendChild(admissionMessage);
    var createGeneration=0,createInFlight=false,createSubmitted=false;
    document.getElementById('start-new').addEventListener('click',function(){
      if(!createInFlight)createSubmitted=false;
      admissionMessage.style.display='none';newPanel.style.display='block';
    });
    document.getElementById('np-cancel').addEventListener('click',function(){
      var context={submitted:createSubmitted,inFlight:createInFlight};
      ++createGeneration;createInFlight=false;
      // A native entry can restore only its unchanged ready owner. The
      // ordinary start-screen form keeps its existing default presentation.
      if(typeof ports.cancelProjectEntry==='function'){
        try{if(ports.cancelProjectEntry(context)===false)return false;}catch(_){return false;}
      }
      admissionMessage.style.display='none';newPanel.style.display='none';
    });
    document.getElementById('np-preset').addEventListener('change',function(){
      document.getElementById('np-custom-row').style.display=this.value==='custom'?'flex':'none';
    });
    document.getElementById('np-create').addEventListener('click',function(){
      if(createInFlight)return false;
      createSubmitted=true;
      var preset=document.getElementById('np-preset').value;
      var w,h;
      if(preset==='custom'){
        w=parseInt(document.getElementById('np-w').value)||1920;
        h=parseInt(document.getElementById('np-h').value)||1080;
      }else{
        var parts=preset.split('x');w=parseInt(parts[0]);h=parseInt(parts[1]);
      }
      var fps=parseInt(document.getElementById('np-fps').value)||24;
      var name=document.getElementById('np-name').value.trim()||'Untitled';
      var generation=++createGeneration,creationFailed=false;
      createInFlight=true;admissionMessage.style.display='none';
      function current(){
        return generation===createGeneration&&newPanel.style.display!=='none'&&
          !document.getElementById('start-screen').classList.contains('hid');
      }
      current.requestCurrent=function(){return generation===createGeneration;};
      function unavailable(){
        if(!current())return false;
        admissionMessage.textContent=creationFailed?
          'New project creation failed after admission; the document may have changed. Reopen a project before editing.':
          'New project unavailable — native admission was denied.';
        admissionMessage.style.display='block';ports.notice(admissionMessage.textContent);return false;
      }
      function completed(admitted){
        if(generation!==createGeneration)return false;
        createInFlight=false;
        if(admitted===false)return unavailable();
        newPanel.style.display='none';return admitted;
      }
      function failed(){
        if(generation!==createGeneration)return false;
        createInFlight=false;return unavailable();
      }
      function creationFailure(){
        if(generation!==createGeneration)return;
        creationFailed=true;ports.showStartScreen();newPanel.style.display='block';
      }
      try{
        var result=ports.createProject({w:w,h:h,fps:fps,name:name},current,creationFailure);
        return result&&typeof result.then==='function'?result.then(completed,failed):completed(result);
      }catch(e){return failed();}
    });
  }
  // Storage and admission stay in project.js. This view receives only presence
  // booleans and the existing actions, never an autosave or document payload.
  function bindStartScreen(document,ports){
    var screen=document.getElementById('start-screen');
    var resume=document.getElementById('start-resume');
    var cards=document.getElementById('start-cards');
    var open=document.getElementById('start-open');
    if(!screen||!resume||!cards||!open||!document.getElementById('start-newpanel'))return;
    // The card follows the store each time the start screen is shown (refresh), not just at startup: a Save As, New or
    // resume can discard or supersede every slot. A newer refresh wins over a slower earlier answer.
    var token=0;
    function setResume(on){
      resume.style.display=on?'':'none';
      if(on)cards.classList.add('has-resume');else cards.classList.remove('has-resume');
    }
    function refresh(){
      var mine=++token;
      if(ports.readResumePresence()){setResume(true);return;}
      ports.readResumePresenceAsync(function(available){if(mine===token)setResume(!!available);});
    }
    refresh();
    ports.renderRecents();
    resume.addEventListener('click',function(event){
      var done=function(){if(!screen.classList.contains('hid'))refresh();};
      var result=ports.resume(event);
      Promise.resolve(result).then(done,done);
      return result;
    });
    bindNewProjectForm(document,ports);
    // Preserve the native button's default Enter/Space activation. Only stop
    // propagation so document playback shortcuts cannot cancel these keys.
    function keepOpenKeysLocal(event){
      if(event.key==='Enter'||event.key===' ')event.stopPropagation();
    }
    open.addEventListener('keydown',keepOpenKeysLocal);
    open.addEventListener('keyup',keepOpenKeysLocal);
    open.addEventListener('click',ports.open);
    return {refresh:refresh};
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
  window.SMProjectEntry={repaint:repaintProjectEntry,initializeTabs:initializeProjectTabs,
    renderTabs:renderProjectTabs,renderRecents:renderRecentProjects,bindNewProjectForm:bindNewProjectForm,
    bindStartScreen:bindStartScreen};
})();
