// ---- PROJECT MANAGEMENT (New / Open / Save / Recent) ----
// The app used to just auto-restore whatever was in localStorage and drop
// you straight into the canvas — fine for a single ongoing sketch, but not
// how a real app like Callipeg/Animate starts: a landing screen with New
// Project (pick canvas size + fps), Open (a real file from disk), and a
// Recent Projects list, all backed by actual files via Tauri's fs/dialog
// plugins instead of the browser download/upload dance the old Save/Load
// buttons did.
(function(){
  var RECENTS_KEY='sm-recents',MAX_RECENTS=8;
  var currentPath=null,currentName='Untitled';

  function tauriOk(){return typeof window.__TAURI__!=='undefined';}
  function getRecents(){try{return JSON.parse(localStorage.getItem(RECENTS_KEY)||'[]');}catch(e){return[];}}
  function setRecents(list){try{localStorage.setItem(RECENTS_KEY,JSON.stringify(list));}catch(e){}}
  function touchRecent(path,name,meta){
    var list=getRecents().filter(function(r){return r.path!==path;});
    list.unshift({path:path,name:name,canvasW:meta&&meta.canvasW,canvasH:meta&&meta.canvasH,fps:meta&&meta.fps,lastOpened:Date.now()});
    setRecents(list.slice(0,MAX_RECENTS));
  }
  function removeRecent(path){setRecents(getRecents().filter(function(r){return r.path!==path;}));}

  function updateCurrentLabel(){
    var el=document.getElementById('proj-current');if(!el)return;
    el.textContent=currentPath?(currentName+' — '+currentPath):(currentName+' (not saved)');
    el.title=currentPath||'';
  }

  function baseName(path){
    var parts=path.split(/[\\/]/);var f=parts[parts.length-1]||path;
    return f.replace(/\.json$/i,'');
  }

  // ---- New / Open / Save (native fs, real files on disk) ----
  function newProject(cfg){
    if(state.activeSymbolId)exitToScene();
    while(userLayers.length>0)userLayers.pop().remove();state.layers=[];
    Object.keys(_symbolPaperLayers).forEach(function(k){_symbolPaperLayers[k].forEach(function(l){l.remove();});});_symbolPaperLayers={};
    state.symbols={};state.openSymbolTabs=[];state.activeSymbolId=null;
    state.canvasW=cfg.w;state.canvasH=cfg.h;state.canvasBg='#ffffff';state.fps=cfg.fps;
    state.totalFrames=24;state.waIn=0;state.waOut=23;
    window._waIn=0;window._waOut=23;window._totalF=24;
    state.motionArcs={};state.currentFrame=0;
    createUserLayer('Layer 1');activateUL(0);
    drawStage();loadFrame(0);renderOS();renderArcs();updateUI();renderSymbolTabs();
    syncDocFields();
    currentPath=null;currentName=cfg.name||'Untitled';updateCurrentLabel();
    try{localStorage.setItem('sm-auto',window.SM.exportJSON());}catch(e){}
    showToast('New project created');
  }

  async function writeProjectTo(path){
    var json=window.SM.exportJSON();
    await window.__TAURI__.fs.writeTextFile(path,json);
    currentPath=path;currentName=baseName(path);updateCurrentLabel();
    touchRecent(path,currentName,{canvasW:state.canvasW,canvasH:state.canvasH,fps:state.fps});
    renderRecents();
    try{localStorage.setItem('sm-auto',json);}catch(e){}
  }
  async function saveAs(){
    if(!tauriOk()){showToast('Save As requires the desktop app');return;}
    saveAllLayerFrames();
    var path=await window.__TAURI__.dialog.save({title:'Save Project As',defaultPath:currentName+'.json',filters:[{name:'StrokeMotion Project',extensions:['json']}]});
    if(!path)return;
    await writeProjectTo(path);
    showToast('Saved: '+baseName(path));
  }
  async function save(){
    if(!currentPath){await saveAs();return;}
    if(!tauriOk()){showToast('Save requires the desktop app');return;}
    saveAllLayerFrames();
    await writeProjectTo(currentPath);
    showToast('Saved');
  }
  async function openPath(path){
    if(!tauriOk())return;
    try{
      var json=await window.__TAURI__.fs.readTextFile(path);
      window.SM.importJSON(json,true);
      currentPath=path;currentName=baseName(path);updateCurrentLabel();
      touchRecent(path,currentName,{canvasW:state.canvasW,canvasH:state.canvasH,fps:state.fps});
      renderRecents();
      hideStartScreen();
      showToast('Opened: '+currentName);
    }catch(e){
      showToast('Could not open file — it may have moved or been deleted');
      removeRecent(path);renderRecents();
    }
  }
  async function openDialog(){
    if(!tauriOk()){document.getElementById('file-input').click();return;}
    var path=await window.__TAURI__.dialog.open({title:'Open Project',multiple:false,filters:[{name:'StrokeMotion Project',extensions:['json']}]});
    if(!path)return;
    await openPath(Array.isArray(path)?path[0]:path);
  }

  window.SMProject={save:save,saveAs:saveAs,open:openDialog,openPath:openPath,newProject:function(cfg){newProject(cfg);hideStartScreen();}};

  // ---- Start screen ----
  function hideStartScreen(){document.getElementById('start-screen').classList.add('hid');}
  function showStartScreen(){document.getElementById('start-screen').classList.remove('hid');}

  function renderRecents(){
    var wrap=document.getElementById('start-recent-list');if(!wrap)return;
    var list=getRecents();
    wrap.innerHTML='';
    if(!list.length){wrap.innerHTML='<div class="start-empty">No recent projects yet</div>';return;}
    list.forEach(function(r){
      var row=document.createElement('div');row.className='start-recent-row';
      var icon=document.createElement('span');icon.className='material-symbols-rounded';icon.textContent='';
      var info=document.createElement('div');
      var nm=document.createElement('div');nm.className='start-recent-name';nm.textContent=r.name;
      var meta=document.createElement('div');meta.className='start-recent-meta';
      var dims=r.canvasW?(r.canvasW+'×'+r.canvasH+' · '+r.fps+'fps · '):'';
      meta.textContent=dims+new Date(r.lastOpened).toLocaleDateString();
      info.appendChild(nm);info.appendChild(meta);
      var x=document.createElement('div');x.className='start-recent-x';x.textContent='×';x.title='Remove from list';
      x.addEventListener('click',function(e){e.stopPropagation();removeRecent(r.path);renderRecents();});
      row.appendChild(icon);row.appendChild(info);row.appendChild(x);
      row.addEventListener('click',function(){openPath(r.path);});
      wrap.appendChild(row);
    });
  }

  function initStartScreen(){
    var hasAutosave=false;
    try{hasAutosave=!!localStorage.getItem('sm-auto');}catch(e){}
    var resumeCard=document.getElementById('start-resume');
    if(hasAutosave){
      resumeCard.style.display='';
      document.getElementById('start-cards').classList.add('has-resume');
    }
    renderRecents();

    document.getElementById('start-resume').addEventListener('click',function(){
      currentPath=null;currentName='Untitled';updateCurrentLabel();
      hideStartScreen();showToast('Session resumed');
    });
    document.getElementById('start-new').addEventListener('click',function(){
      document.getElementById('start-newpanel').style.display='block';
    });
    document.getElementById('start-open').addEventListener('click',openDialog);
    document.getElementById('np-cancel').addEventListener('click',function(){document.getElementById('start-newpanel').style.display='none';});
    document.getElementById('np-preset').addEventListener('change',function(){
      document.getElementById('np-custom-row').style.display=this.value==='custom'?'flex':'none';
    });
    document.getElementById('np-create').addEventListener('click',function(){
      var preset=document.getElementById('np-preset').value;
      var w,h;
      if(preset==='custom'){w=parseInt(document.getElementById('np-w').value)||1920;h=parseInt(document.getElementById('np-h').value)||1080;}
      else{var parts=preset.split('x');w=parseInt(parts[0]);h=parseInt(parts[1]);}
      var fps=parseInt(document.getElementById('np-fps').value)||24;
      var name=document.getElementById('np-name').value.trim()||'Untitled';
      // must go through the public wrapper — the bare newProject() above
      // never hides the start screen itself (only window.SMProject.newProject
      // does), which is exactly why "Create" was dropping you right back
      // on the start screen instead of into the canvas.
      window.SMProject.newProject({w:w,h:h,fps:fps,name:name});
      document.getElementById('start-newpanel').style.display='none';
    });

    // Project panel buttons (right-hand Project section)
    document.getElementById('btn-save').addEventListener('click',save);
    document.getElementById('btn-saveas').addEventListener('click',saveAs);
    document.getElementById('btn-open').addEventListener('click',openDialog);
    document.getElementById('btn-new').addEventListener('click',showStartScreen);
    // legacy fallback for non-Tauri (plain browser) testing: upload/download
    document.getElementById('file-input').addEventListener('change',function(e){
      var f=e.target.files[0];if(!f)return;
      var r=new FileReader();r.onload=function(ev){window.SM.importJSON(ev.target.result);hideStartScreen();};
      r.readAsText(f);e.target.value='';
    });

    updateCurrentLabel();
  }

  if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initStartScreen);
  else initStartScreen();
})();
