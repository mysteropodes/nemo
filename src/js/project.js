// ---- PROJECT MANAGEMENT (New / Open / Save / Recent) ----
// The app used to just auto-restore whatever was in localStorage and drop
// you straight into the canvas — fine for a single ongoing sketch, but not
// how a real app like Callipeg/Animate starts: a landing screen with New
// Project (pick canvas size + fps), Open (a real file from disk), and a
// Recent Projects list, all backed by actual files via Tauri's fs/dialog
// plugins instead of the browser download/upload dance the old Save/Load
// buttons did.
(function(){
  var RECENTS_KEY='nemo-recents',MAX_RECENTS=8;
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
    // v12: default timeline length is 5 SECONDS of frames at the project's
    // own fps (was a flat 24 frames — only 1s at 24fps, or under a second
    // at 30/60fps), per explicit request. Every entry point that creates a
    // blank project (New Project form, a "+" tab, a blank tab restored on
    // close) funnels through here, so this one change covers all of them.
    var defFrames=Math.max(1,Math.round(cfg.fps*5));
    state.totalFrames=defFrames;state.waIn=0;state.waOut=defFrames-1;
    window._waIn=0;window._waOut=defFrames-1;window._totalF=defFrames;
    state.motionArcs={};state.tweenOverrides={};state.currentFrame=0;
    // Viewport/UI state, not document content — reset the same way
    // fitCanvas()/resetView() already do, so a new project doesn't silently
    // inherit the previous project's stage rotation.
    state.canvasRotation=0;
    state.palettes=[{id:'p0',name:'Palette 1',colors:['#000000','#ffffff','#ff0000','#ff8800','#ffee00','#00cc44','#0088ff','#8833ff']}];
    state.activePaletteIdx=0;
    state.customBrushPresets={};
    state.perspectiveEnabled=false;state.perspectiveMode='2pt';state.perspectiveDensity=24;state.perspectiveVPs=null;
    state.audioTracks=[];if(window.SMAudio)SMAudio.reload();
    state.refMedia=null;if(window.SMReference)SMReference.reload();
    state.layerFolders={};state.layerLinkGroups={};
    state.cameraKeys=[];state.cameraLayerOn=false;state.cameraView=false;
    createUserLayer('Layer 1');activateUL(0);
    drawStage();loadFrame(0);renderOS();renderArcs();updateUI();renderSymbolTabs();
    // view.zoom/center aren't part of the project data (never saved), so
    // without this a new project silently inherited whatever zoom was left
    // over from before (or Paper's raw default) instead of fitting the new
    // canvas size to the viewport — reported as "le canvas n'est pas fit".
    window.SM.fitCanvas();
    if(window.renderPaletteGrid)window.renderPaletteGrid();
    syncDocFields();
    currentPath=null;currentName=cfg.name||'Untitled';updateCurrentLabel();
    try{localStorage.setItem('nemo-auto',window.SM.exportJSON());}catch(e){}
    showToast('New project created');
  }

  async function writeProjectTo(path){
    var json=window.SM.exportJSON();
    await window.__TAURI__.fs.writeTextFile(path,json);
    currentPath=path;currentName=baseName(path);updateCurrentLabel();
    touchRecent(path,currentName,{canvasW:state.canvasW,canvasH:state.canvasH,fps:state.fps});
    renderRecents();
    try{localStorage.setItem('nemo-auto',json);}catch(e){}
  }
  async function saveAs(){
    if(!tauriOk()){showToast('Save As requires the desktop app');return;}
    saveAllLayerFrames();
    var path=await window.__TAURI__.dialog.save({title:'Save Project As',defaultPath:currentName+'.json',filters:[{name:'Nemo Project',extensions:['json']}]});
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
      ensureInitialTab();
      showToast('Opened: '+currentName);
    }catch(e){
      showToast('Could not open file — it may have moved or been deleted');
      removeRecent(path);renderRecents();
    }
  }
  async function openDialog(){
    if(!tauriOk()){document.getElementById('file-input').click();return;}
    var path=await window.__TAURI__.dialog.open({title:'Open Project',multiple:false,filters:[{name:'Nemo Project',extensions:['json']}]});
    if(!path)return;
    await openPath(Array.isArray(path)?path[0]:path);
  }

  // ---- Version history (v15) — the existing 30s autosave (timeline.js)
  // only ever held ONE slot ('nemo-auto' in localStorage): a crash right
  // after a bad edit overwrote the one good copy with the bad one on the
  // very next tick. This keeps a dense rolling history of real files on
  // disk (Tauri fs — no localStorage quota risk with base64 media embedded
  // in every snapshot) so "I crashed 10 min ago" can recover "the version
  // from 11 min ago", not just "whatever was last written".
  var HISTORY_MAX=120; // 60 min of history at the existing 30s cadence
  function simpleHash(s){var h=0;for(var i=0;i<s.length;i++)h=(h*31+s.charCodeAt(i))|0;return (h>>>0).toString(36);}
  function historyKey(){return currentPath?(baseName(currentPath).replace(/[^a-z0-9]+/gi,'-').toLowerCase()+'-'+simpleHash(currentPath)):'untitled-autosave';}
  async function historyDir(){
    var base=await window.__TAURI__.path.appDataDir();
    return base.replace(/[\\/]+$/,'')+'/history/'+historyKey();
  }
  async function pushVersionSnapshot(json){
    if(!tauriOk())return; // browser preview: 'nemo-auto' single-slot fallback only
    try{
      var dir=await historyDir();
      await window.__TAURI__.fs.mkdir(dir,{recursive:true});
      await window.__TAURI__.fs.writeTextFile(dir+'/'+Date.now()+'.json',json);
      var entries=await window.__TAURI__.fs.readDir(dir);
      var names=entries.filter(function(e){return /\.json$/.test(e.name);}).map(function(e){return e.name;}).sort();
      while(names.length>HISTORY_MAX){
        var victim=names.shift();
        try{await window.__TAURI__.fs.remove(dir+'/'+victim);}catch(e){}
      }
    }catch(e){console.warn('[history] snapshot failed',e);}
  }
  async function listVersionHistory(){
    if(!tauriOk())return [];
    try{
      var dir=await historyDir();
      var entries=await window.__TAURI__.fs.readDir(dir);
      return entries.filter(function(e){return /\.json$/.test(e.name);})
        .map(function(e){return {ts:parseInt(e.name,10),path:dir+'/'+e.name};})
        .filter(function(v){return !isNaN(v.ts);})
        .sort(function(a,b){return b.ts-a.ts;});
    }catch(e){return [];}
  }
  async function restoreVersion(path){
    saveAllLayerFrames();
    var json=await window.__TAURI__.fs.readTextFile(path);
    window.SM.importJSON(json,true);
    ensureInitialTab();
    showToast('Version restaurée');
  }

  function relTime(ts){
    var s=Math.max(0,Math.round((Date.now()-ts)/1000));
    if(s<60)return 'il y a '+s+'s';
    var m=Math.round(s/60);
    if(m<60)return 'il y a '+m+' min';
    var h=Math.round(m/60);
    return 'il y a '+h+'h'+(m%60?Math.round(m%60)+'min':'');
  }
  async function openHistoryModal(){
    var modal=document.getElementById('history-modal'),list=document.getElementById('history-list');
    if(!modal||!list)return;
    if(!tauriOk()){list.innerHTML='<div style="font-size:11px;color:var(--text-dim)">Historique disque disponible uniquement dans l\'app desktop.</div>';modal.style.display='flex';return;}
    list.innerHTML='<div style="font-size:11px;color:var(--text-dim)">Chargement…</div>';
    modal.style.display='flex';
    var versions=await listVersionHistory();
    if(!versions.length){list.innerHTML='<div style="font-size:11px;color:var(--text-dim)">Aucun instantané pour l\'instant — revenez dans 30s.</div>';return;}
    list.innerHTML='';
    versions.forEach(function(v){
      var row=document.createElement('div');
      row.style.cssText='display:flex;align-items:center;justify-content:space-between;padding:6px 8px;border-radius:4px;cursor:pointer;font-size:11px;';
      row.onmouseenter=function(){row.style.background='var(--panel3)';};
      row.onmouseleave=function(){row.style.background='';};
      var d=new Date(v.ts);
      var abs=d.toLocaleTimeString(undefined,{hour:'2-digit',minute:'2-digit',second:'2-digit'});
      row.innerHTML='<span>'+relTime(v.ts)+' <span style="color:var(--text-dim)">('+abs+')</span></span>';
      var btn=document.createElement('button');
      btn.className='pbtn';btn.textContent='Restaurer';btn.style.fontSize='10px';
      btn.addEventListener('click',function(e){
        e.stopPropagation();
        if(!confirm('Restaurer cette version ? Le document actuel non sauvegardé sera remplacé.'))return;
        restoreVersion(v.path).then(function(){modal.style.display='none';});
      });
      row.appendChild(btn);
      list.appendChild(row);
    });
  }

  // ---- Team sync (v16, Phase 2) — async, NOT realtime. Reuses the exact
  // history-snapshot pattern above (a plain directory of timestamped JSON
  // files, no server), except the directory lives in a user-chosen SHARED
  // folder (kDrive/S3 mount etc. — just a path from the app's point of
  // view) and is namespaced by profile id instead of file-path hash, so
  // every collaborator writes to their own subfolder without clobbering
  // each other. "Check for updates" lists the latest snapshot per OTHER
  // profile; "Merge" pulls one in via window.SM.mergeRemoteSnapshot (app.js)
  // — new strokes merge automatically, same-strokeId edits surface as a
  // Phase 1 revision pair via the existing ghost/accept/reject UI.
  var SYNC_MAX_PER_PROFILE=30;
  function syncFolderKey(){return 'nemo-sync-'+historyKey();}
  function getSyncFolder(){try{return localStorage.getItem(syncFolderKey())||null;}catch(e){return null;}}
  function setSyncFolder(path){try{if(path)localStorage.setItem(syncFolderKey(),path);else localStorage.removeItem(syncFolderKey());}catch(e){}}
  function profileDir(root,profileId){return root.replace(/[\\/]+$/,'')+'/'+profileId;}
  async function chooseSyncFolder(){
    if(!tauriOk()){showToast('Sync équipe nécessite l\'app desktop');return null;}
    var path=await window.__TAURI__.dialog.open({title:'Dossier partagé (kDrive, S3 monté, etc.)',directory:true});
    if(!path)return null;
    path=Array.isArray(path)?path[0]:path;
    setSyncFolder(path);
    showToast('Dossier de sync configuré');
    return path;
  }
  function disableSync(){setSyncFolder(null);}
  async function publishToShared(){
    var root=getSyncFolder();
    if(!root){showToast('Configurez un dossier de sync d\'abord');return;}
    if(!tauriOk()){showToast('Sync équipe nécessite l\'app desktop');return;}
    saveAllLayerFrames();
    var json=window.SM.exportJSON();
    var dir=profileDir(root,state.userProfile.id);
    try{
      await window.__TAURI__.fs.mkdir(dir,{recursive:true});
      await window.__TAURI__.fs.writeTextFile(dir+'/_profile.json',JSON.stringify(state.userProfile));
      await window.__TAURI__.fs.writeTextFile(dir+'/'+Date.now()+'.json',json);
      var entries=await window.__TAURI__.fs.readDir(dir);
      var names=entries.filter(function(e){return /^\d+\.json$/.test(e.name);}).map(function(e){return e.name;}).sort();
      while(names.length>SYNC_MAX_PER_PROFILE){
        var victim=names.shift();
        try{await window.__TAURI__.fs.remove(dir+'/'+victim);}catch(e){}
      }
      showToast('Publié pour l\'équipe');
    }catch(e){console.warn('[sync] publish failed',e);showToast('Échec de la publication');}
  }
  async function checkSharedUpdates(){
    var root=getSyncFolder();
    if(!root||!tauriOk())return [];
    var out=[];
    try{
      var entries=await window.__TAURI__.fs.readDir(root);
      for(var i=0;i<entries.length;i++){
        var e=entries[i];
        if(e.name===state.userProfile.id)continue;
        if(e.children===undefined&&e.isDirectory===false)continue; // skip stray files at the root
        var subdir=root.replace(/[\\/]+$/,'')+'/'+e.name;
        try{
          var sub=await window.__TAURI__.fs.readDir(subdir);
          var jsons=sub.filter(function(x){return /^\d+\.json$/.test(x.name);}).map(function(x){return x.name;}).sort();
          if(!jsons.length)continue;
          var latestName=jsons[jsons.length-1];
          var profileName=e.name,profileColor='#888888';
          try{
            var pj=await window.__TAURI__.fs.readTextFile(subdir+'/_profile.json');
            var prof=JSON.parse(pj);
            profileName=prof.name||profileName;profileColor=prof.color||profileColor;
          }catch(e2){}
          out.push({profileId:e.name,profileName:profileName,profileColor:profileColor,path:subdir+'/'+latestName,ts:parseInt(latestName,10)});
        }catch(e3){}
      }
    }catch(e4){console.warn('[sync] check failed',e4);}
    return out.sort(function(a,b){return b.ts-a.ts;});
  }
  async function pullAndMerge(entry){
    if(!tauriOk())return null;
    var json=await window.__TAURI__.fs.readTextFile(entry.path);
    var data=JSON.parse(json);
    var report=window.SM.mergeRemoteSnapshot(data,{id:entry.profileId,name:entry.profileName,color:entry.profileColor});
    saveAllLayerFrames();
    showToast('Fusion de '+entry.profileName+' : +'+report.added+' ajout(s)'+(report.conflicts?', '+report.conflicts+' conflit(s) à résoudre':''));
    return report;
  }

  window.SMProject={save:save,saveAs:saveAs,open:openDialog,openPath:openPath,newProject:function(cfg){newProject(cfg);hideStartScreen();ensureInitialTab();},pushVersionSnapshot:pushVersionSnapshot,listVersionHistory:listVersionHistory,restoreVersion:restoreVersion,
    getSyncFolder:getSyncFolder,chooseSyncFolder:chooseSyncFolder,disableSync:disableSync,publishToShared:publishToShared,checkSharedUpdates:checkSharedUpdates,pullAndMerge:pullAndMerge,
    // Stable per-project filesystem-safe identifier — same slug/hash
    // feedback-bridge.js's local + shared feedback storage keys off, so a
    // feedback thread and this project's own history/sync folders always
    // agree on which project they belong to without re-deriving the logic.
    getProjectKey:historyKey,profileDir:profileDir};

  // ---- Project tabs (real multi-project switching, v11) ----
  // Each tab holds a full independent snapshot of a project as the SAME
  // JSON string window.SM.exportJSON()/importJSON() already use for file
  // save/open — switching tabs is just "export the outgoing one into its
  // slot, import the incoming one", reusing all existing serialization
  // instead of re-architecting `state` into a multi-document structure.
  // Genuinely separate projects (own layers/frames/palettes/etc.), not a
  // cosmetic tab strip — confirmed against the user's own clarification
  // ("real management of several completely different projects").
  var tabs=[],activeTabId=null;
  function makeTabId(){return 't'+Date.now().toString(36)+Math.random().toString(36).slice(2,6);}
  function activeTab(){return tabs.find(function(t){return t.id===activeTabId;});}
  function snapshotActiveIntoTab(){
    var t=activeTab();if(!t)return;
    saveAllLayerFrames();
    t.json=window.SM.exportJSON();t.name=currentName;t.path=currentPath;
  }
  function ensureInitialTab(){
    // Called after every entry point that makes a project "live" (new,
    // opened, or resumed autosave) — only actually creates a tab the FIRST
    // time (app boot), otherwise just refreshes the active tab's own
    // name/path so e.g. a fresh Save As renames its own tab in place
    // rather than spawning a duplicate.
    if(!tabs.length){
      activeTabId=makeTabId();
      tabs.push({id:activeTabId,name:currentName,json:null,path:currentPath});
    }else{
      var t=activeTab();
      if(t){t.name=currentName;t.path=currentPath;}
    }
    renderTabBar();
  }
  function switchToTab(id){
    if(id===activeTabId)return;
    var target=tabs.find(function(t){return t.id===id;});if(!target)return;
    snapshotActiveIntoTab();
    activeTabId=id;
    if(target.json)window.SM.importJSON(target.json,true);
    else newProject({w:1920,h:1080,fps:24,name:target.name});
    currentPath=target.path||null;currentName=target.name;updateCurrentLabel();
    renderTabBar();
  }
  function addTab(){
    snapshotActiveIntoTab();
    var n=tabs.length+1;
    var id=makeTabId();
    tabs.push({id:id,name:'Untitled '+n,json:null,path:null});
    activeTabId=id;
    newProject({w:1920,h:1080,fps:24,name:'Untitled '+n});
    currentPath=null;currentName='Untitled '+n;updateCurrentLabel();
    renderTabBar();
  }
  function closeTab(id){
    var idx=tabs.findIndex(function(t){return t.id===id;});if(idx<0)return;
    if(tabs.length===1){showToast('Impossible de fermer le dernier onglet');return;}
    var wasActive=id===activeTabId;
    tabs.splice(idx,1);
    if(wasActive){
      var next=tabs[Math.max(0,idx-1)];
      activeTabId=next.id; // switchToTab no-ops on equal id, so load directly
      if(next.json)window.SM.importJSON(next.json,true);
      else newProject({w:1920,h:1080,fps:24,name:next.name});
      currentPath=next.path||null;currentName=next.name;updateCurrentLabel();
    }
    renderTabBar();
  }
  function startTabRename(id){
    var el=document.querySelector('.project-tab[data-tab="'+id+'"] .pt-name');if(!el)return;
    var t=tabs.find(function(t){return t.id===id;});if(!t)return;
    var input=document.createElement('input');input.className='pt-rename-input';input.value=t.name;
    el.replaceWith(input);input.focus();input.select();
    function commit(){
      var v=input.value.trim()||t.name;t.name=v;
      if(id===activeTabId){currentName=v;updateCurrentLabel();}
      renderTabBar();
    }
    input.addEventListener('blur',commit);
    input.addEventListener('keydown',function(e){e.stopPropagation();if(e.key==='Enter')input.blur();else if(e.key==='Escape'){input.value=t.name;input.blur();}});
  }
  function renderTabBar(){
    var list=document.getElementById('project-tabs-list');if(!list)return;
    list.innerHTML='';
    tabs.forEach(function(t){
      var el=document.createElement('div');el.className='project-tab'+(t.id===activeTabId?' act':'');el.dataset.tab=t.id;
      var dot=document.createElement('span');dot.className='pt-dot';
      var nm=document.createElement('span');nm.className='pt-name';nm.textContent=t.name;
      var close=document.createElement('span');close.className='pt-close';close.textContent='×';close.title='Fermer l\'onglet';
      close.addEventListener('click',function(e){e.stopPropagation();closeTab(t.id);});
      el.appendChild(dot);el.appendChild(nm);el.appendChild(close);
      el.addEventListener('click',function(){switchToTab(t.id);});
      el.addEventListener('dblclick',function(e){e.stopPropagation();startTabRename(t.id);});
      list.appendChild(el);
    });
  }
  document.getElementById('project-tab-add')&&document.getElementById('project-tab-add').addEventListener('click',addTab);

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
      var icon=document.createElement('span');icon.className='start-recent-dot';
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
    try{hasAutosave=!!localStorage.getItem('nemo-auto');}catch(e){}
    var resumeCard=document.getElementById('start-resume');
    if(hasAutosave){
      resumeCard.style.display='';
      document.getElementById('start-cards').classList.add('has-resume');
    }
    renderRecents();

    document.getElementById('start-resume').addEventListener('click',function(){
      // Was never actually loading the autosave — just hid the start
      // screen and left the blank project created at boot untouched, so
      // "Resume" silently discarded a real, present nemo-auto snapshot
      // (confirmed live: state.layers[*].frames all empty after clicking
      // Resume despite localStorage holding real stroke data).
      var auto=null;
      try{auto=localStorage.getItem('nemo-auto');}catch(e){}
      if(auto){
        try{window.SM.importJSON(auto,true);}
        catch(e){showToast('Impossible de reprendre la session — données corrompues');}
      }
      currentPath=null;currentName='Untitled';updateCurrentLabel();
      hideStartScreen();ensureInitialTab();showToast('Session resumed');
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
    // Version history modal (v15)
    var histBtn=document.getElementById('btn-history');
    if(histBtn)histBtn.addEventListener('click',openHistoryModal);
    var histClose=document.getElementById('history-close');
    if(histClose)histClose.addEventListener('click',function(){document.getElementById('history-modal').style.display='none';});
    var histModal=document.getElementById('history-modal');
    if(histModal)histModal.addEventListener('click',function(e){if(e.target===histModal)histModal.style.display='none';});
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
