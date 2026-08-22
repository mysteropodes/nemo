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
  // Browser-mode autosave: localStorage first (sync, ~5-10MB quota), always
  // mirrored to IndexedDB (async, no practical size ceiling) so a project
  // with embedded media doesn't silently lose its autosave the moment it
  // outgrows localStorage — see project-nemo-web-public-beta memory.
  function autosaveWrite(json){
    var quotaHit=false;
    try{localStorage.setItem('nemo-auto',json);}catch(e){quotaHit=true;}
    if(window.SMIdb)window.SMIdb.set('nemo-auto',json).catch(function(){});
    if(quotaHit)try{localStorage.removeItem('nemo-auto');}catch(e2){} // stale/truncated slot would win over IDB on next boot otherwise
  }
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
  // Used by the burger menu (timeline.js initAppMenu) now that the old
  // right-panel "Projet" section (and its #proj-current row) is gone —
  // same text updateCurrentLabel used to write there.
  function getCurrentLabel(){
    return currentPath?(currentName+' — '+currentPath):(currentName+' (not saved)');
  }

  function baseName(path){
    var parts=path.split(/[\\/]/);var f=parts[parts.length-1]||path;
    return f.replace(/\.json$/i,'');
  }

  // ---- New / Open / Save (native fs, real files on disk) ----
  function newProject(cfg){
    if(window.SMLabs&&window.SMLabs.resetAll)window.SMLabs.resetAll(); // see labs-core.js's own comment — a Labs prototype must never silently carry into a new project
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
    state.motionArcs={};state.tweenOverrides={};state.tweenEasing={};state.currentFrame=0;
    // Viewport/UI state, not document content — reset the same way
    // fitCanvas()/resetView() already do, so a new project doesn't silently
    // inherit the previous project's stage rotation.
    state.canvasRotation=0;
    state.palettes=[{id:'p0',name:'Palette 1',colors:['#000000','#ffffff','#ff0000','#ff8800','#ffee00','#00cc44','#0088ff','#8833ff']}];
    state.activePaletteIdx=0;
    state.shadowPalette=[{id:'sh1',color:'#ff3355'},{id:'sh2',color:'#ff8800'},{id:'sh3',color:'#ffdd00'},{id:'sh4',color:'#22cc55'},{id:'sh5',color:'#2288ff'},{id:'sh6',color:'#aa33ff'}];
    state.shadowActiveId='sh1';state.shadowMode=false;
    state.customBrushPresets={};
    state.perspectiveEnabled=false;state.perspectiveMode='2pt';state.perspectiveDensity=24;state.perspectiveVPs=null;
    state.symmetryEnabled=false;state.symmetryMode='y';state.symmetryAxis=null;state.symmetryRadialCenter=null;state.symmetryRadialSectors=6;state.symmetryExtend=true;
    state.audioTracks=[];if(window.SMAudio)SMAudio.reload();
    state.refMedia=null;if(window.SMReference)SMReference.reload();
    state.mediaLibrary=[];if(window.SMMediaLibrary)SMMediaLibrary.reload();
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
    try{var freshJson=window.SM.exportJSON();markSaved(freshJson);autosaveWrite(freshJson);}catch(e){}
    showToast('New project created');
  }

  // Last successfully persisted document, for the close-with-unsaved-work
  // guard below. null = "never saved/loaded anything yet" — a brand-new
  // empty document counts as clean until it's actually drawn on (newProject
  // and openPath both stamp it).
  var lastSavedJson=null;
  function markSaved(json){lastSavedJson=json;}
  function isDirty(){
    try{return lastSavedJson!==null&&window.SM.exportJSON()!==lastSavedJson;}
    catch(e){return true;} // can't serialize → assume dirty, never skip the warning
  }

  async function writeProjectTo(path){
    var json=window.SM.exportJSON();
    // Atomic save: write to a sibling temp file, then rename over the real
    // one. A crash/power loss/full disk mid-write must never leave PATH
    // truncated with the previous good version already destroyed — rename
    // is atomic at the OS level, so the project file is always either the
    // old complete version or the new complete one, never a torn middle.
    var tmp=path+'.saving';
    try{
      await window.__TAURI__.fs.writeTextFile(tmp,json);
      await window.__TAURI__.fs.rename(tmp,path);
    }catch(e){
      // rename needs fs:allow-rename, added to capabilities 2026-07-13 —
      // an app built before that (or an exotic FS refusing the rename)
      // lands here. Fall back to the historical direct write rather than
      // failing the save outright: a maybe-torn write on crash still beats
      // guaranteed data loss from refusing to save at all.
      try{await window.__TAURI__.fs.remove(tmp);}catch(_e){}
      await window.__TAURI__.fs.writeTextFile(path,json);
    }
    currentPath=path;currentName=baseName(path);updateCurrentLabel();
    touchRecent(path,currentName,{canvasW:state.canvasW,canvasH:state.canvasH,fps:state.fps});
    renderRecents();
    markSaved(json);
    try{localStorage.setItem('nemo-auto',json);}catch(e){}
  }
  // ---- Browser-mode save (2026-07, "ajoute la possibilité de save un
  // projet en mode web aussi") — no Tauri fs/dialog to write a real path
  // to, so Save/Save As both trigger a browser file download of the exact
  // same JSON a desktop save would write (same format, re-openable via
  // Open's own existing browser fallback — file-input's change handler
  // above already reads it back with importJSON). There's no silent
  // "overwrite the same file" concept in a browser without the File System
  // Access API (not universally supported, and out of scope here) — every
  // browser-mode save is effectively a Save As, a fresh download the user
  // places themselves.
  function downloadJson(filename,json){
    var blob=new Blob([json],{type:'application/json'});
    var url=URL.createObjectURL(blob);
    var a=document.createElement('a');
    a.href=url;a.download=filename;
    document.body.appendChild(a);a.click();document.body.removeChild(a);
    setTimeout(function(){URL.revokeObjectURL(url);},1000);
  }
  function saveAsDownload(){
    saveAllLayerFrames();
    var json=window.SM.exportJSON();
    downloadJson((currentName||'Untitled')+'.json',json);
    markSaved(json);
    autosaveWrite(json);
    showToast(SM.t('toastDownloadedSuffix')+currentName+'.json');
  }
  async function saveAs(){
    if(!tauriOk()){saveAsDownload();return;}
    saveAllLayerFrames();
    var path=await window.__TAURI__.dialog.save({title:'Save Project As',defaultPath:currentName+'.json',filters:[{name:'Nemo Project',extensions:['json']}]});
    if(!path)return;
    // A failed write MUST be loud — without this catch there was no error
    // toast at all: the user saw no "Saved" but nothing else either, easy
    // to miss, and quitting right after meant silent data loss (disk full,
    // permissions, network volume gone...).
    try{await writeProjectTo(path);}
    catch(e){showToast(SM.t('toastSaveFailedSuffix')+(e&&e.message||e));throw e;}
    showToast('Saved: '+baseName(path));
  }
  async function save(){
    if(!tauriOk()){saveAsDownload();return;}
    if(!currentPath){await saveAs();return;}
    saveAllLayerFrames();
    try{await writeProjectTo(currentPath);}
    catch(e){showToast(SM.t('toastSaveFailedSuffix')+(e&&e.message||e));throw e;}
    showToast('Saved');
  }
  async function openPath(path){
    if(!tauriOk())return;
    try{
      var json=await window.__TAURI__.fs.readTextFile(path);
      window.SM.importJSON(json,true);
      // Re-export rather than keeping the file's own text: importJSON
      // normalizes (fills defaults, pads frames), so the round-tripped
      // form is what future exportJSON calls will actually produce —
      // comparing against the raw file text would flag a just-opened
      // untouched project as dirty forever.
      try{markSaved(window.SM.exportJSON());}catch(e){}
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
    // Snapshot the CURRENT state into history before overwriting it with
    // the old version — otherwise "restore from 10 min ago" silently
    // discards up to 30s of work since the last autosave tick, and worse,
    // makes the restore itself irreversible: the pre-restore state was
    // never captured anywhere. With this, a restore is always undoable by
    // restoring the snapshot taken right here.
    try{await pushVersionSnapshot(window.SM.exportJSON());}catch(e){console.warn('[history] pre-restore snapshot failed',e);}
    var json=await window.__TAURI__.fs.readTextFile(path);
    window.SM.importJSON(json,true);
    ensureInitialTab();
    showToast(SM.t('toastVersionRestored'));
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
    if(!tauriOk()){showToast(SM.t('toastTeamSyncRequiresDesktop'));return null;}
    var path=await window.__TAURI__.dialog.open({title:'Dossier partagé (kDrive, S3 monté, etc.)',directory:true});
    if(!path)return null;
    path=Array.isArray(path)?path[0]:path;
    setSyncFolder(path);
    showToast(SM.t('toastSyncFolderConfigured'));
    return path;
  }
  function disableSync(){setSyncFolder(null);}
  async function publishToShared(){
    var root=getSyncFolder();
    if(!root){showToast('Configurez un dossier de sync d\'abord');return;}
    if(!tauriOk()){showToast(SM.t('toastTeamSyncRequiresDesktop'));return;}
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
      showToast(SM.t('toastPublishedForTeam'));
    }catch(e){console.warn('[sync] publish failed',e);showToast(SM.t('toastPublishFailed'));}
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
    showToast('Fusion de '+entry.profileName+' : +'+report.added+' ajout(s)'+(report.conflicts?', '+report.conflicts+SM.t('toastConflictsToResolveSuffix'):''));
    return report;
  }

  window.SMProject={save:save,saveAs:saveAs,open:openDialog,openPath:openPath,newProject:function(cfg){newProject(cfg);hideStartScreen();ensureInitialTab();},pushVersionSnapshot:pushVersionSnapshot,listVersionHistory:listVersionHistory,restoreVersion:restoreVersion,autosaveWrite:autosaveWrite,
    getSyncFolder:getSyncFolder,chooseSyncFolder:chooseSyncFolder,disableSync:disableSync,publishToShared:publishToShared,checkSharedUpdates:checkSharedUpdates,pullAndMerge:pullAndMerge,
    // Stable per-project filesystem-safe identifier — same slug/hash
    // feedback-bridge.js's local + shared feedback storage keys off, so a
    // feedback thread and this project's own history/sync folders always
    // agree on which project they belong to without re-deriving the logic.
    getProjectKey:historyKey,profileDir:profileDir,isDirty:isDirty,markSaved:function(){try{markSaved(window.SM.exportJSON());}catch(e){}},getCurrentLabel:getCurrentLabel};

  // ---- Close-with-unsaved-work guard ----
  // Cmd+Q / window-close with unsaved changes asked NOTHING before this —
  // up to 30s of work (the autosave cadence) vanished without a word, the
  // single worst data-loss path left in the app. Desktop (Tauri): confirm
  // dialog on close request, cancel keeps the app open. Browser preview:
  // the standard beforeunload prompt.
  (function initCloseGuard(){
    // Baseline = whatever document is in memory at startup (the default
    // blank one, or the nemo-auto session timeline.js quietly restored —
    // that restore runs before this script loads, so it can't stamp
    // markSaved itself). Without this, isDirty() stayed null-baselined
    // (never dirty) until the first explicit save/open/new — leaving an
    // auto-restored session entirely unguarded.
    try{markSaved(window.SM.exportJSON());}catch(e){}
    if(tauriOk()&&window.__TAURI__.window&&window.__TAURI__.window.getCurrentWindow){
      try{
        var appWindow=window.__TAURI__.window.getCurrentWindow();
        var allowConfirmedClose=false;
        appWindow.onCloseRequested(async function(event){
          if(allowConfirmedClose||!isDirty())return; // clean/confirmed → close proceeds normally
          // Tauri cannot keep the original native close request pending
          // across an awaited dialog. Cancel it synchronously, then issue a
          // fresh close request after positive confirmation.
          event.preventDefault();
          try{
            var leave=await window.__TAURI__.dialog.ask(
              'Des modifications non sauvegardées seront perdues. Quitter quand même ?',
              {title:'Modifications non sauvegardées',kind:'warning',okLabel:'Quitter sans sauvegarder',cancelLabel:'Annuler'});
            if(leave){
              allowConfirmedClose=true;
              // destroy(), not close() (feedback #27, "quand on fait quitter
              // sans sauvegarder ne quitte pas l'app"): close() re-issues a
              // FRESH CloseRequested event (the comment above already knew
              // this — that's the whole reason allowConfirmedClose exists),
              // so the app's actual exit depends on that second event being
              // dispatched, caught by this same listener, and let through —
              // one more asynchronous round trip that can silently swallow
              // the close. destroy() skips the event loop entirely and forces
              // the window closed immediately, which is Tauri's own
              // documented pattern for exactly this "confirm then really
              // close" flow. core:window:allow-destroy was already granted
              // in capabilities/default.json — just never actually called.
              await appWindow.destroy();
            }
          }catch(e){
            // dialog unavailable (permission/API) — err on the side of NOT
            // losing work: block this close so the user can save manually.
            showToast(SM.t('toastUnsavedChangesSaveBeforeQuit'));
          }
        });
      }catch(e){console.warn('[close-guard] indisponible',e);}
    }else{
      window.addEventListener('beforeunload',function(e){
        if(!isDirty())return;
        e.preventDefault();
        e.returnValue=''; // required by Chrome to actually show the prompt
      });
    }
  })();

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
    if(!list.length){wrap.innerHTML='<div class="start-empty">'+window.SM.t('startNoRecents')+'</div>';return;}
    list.forEach(function(r){
      var row=document.createElement('div');row.className='start-recent-row';
      var icon=document.createElement('span');icon.className='start-recent-dot';
      var info=document.createElement('div');
      var nm=document.createElement('div');nm.className='start-recent-name';nm.textContent=r.name;
      var meta=document.createElement('div');meta.className='start-recent-meta';
      var dims=r.canvasW?(r.canvasW+'×'+r.canvasH+' · '+r.fps+'fps · '):'';
      meta.textContent=dims+new Date(r.lastOpened).toLocaleDateString();
      info.appendChild(nm);info.appendChild(meta);
      var x=document.createElement('div');x.className='start-recent-x';x.textContent='×';x.title=window.SM.t('startRemoveFromList');
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
    function showResumeCard(){
      resumeCard.style.display='';
      document.getElementById('start-cards').classList.add('has-resume');
    }
    if(hasAutosave)showResumeCard();
    // localStorage may be empty because a media-heavy autosave overflowed
    // its quota and only landed in IndexedDB (see autosaveWrite) — check
    // async so "Resume" still appears in that case, just a tick later.
    else if(window.SMIdb)window.SMIdb.get('nemo-auto').then(function(v){if(v)showResumeCard();}).catch(function(){});
    renderRecents();

    document.getElementById('start-resume').addEventListener('click',function(){
      // Was never actually loading the autosave — just hid the start
      // screen and left the blank project created at boot untouched, so
      // "Resume" silently discarded a real, present nemo-auto snapshot
      // (confirmed live: state.layers[*].frames all empty after clicking
      // Resume despite localStorage holding real stroke data).
      var auto=null;
      try{auto=localStorage.getItem('nemo-auto');}catch(e){}
      var applyAuto=function(auto){
        if(auto){
          try{window.SM.importJSON(auto,true);}
          catch(e){showToast(SM.t('toastCannotResumeSessionCorrupt'));}
        }
        currentPath=null;currentName='Untitled';updateCurrentLabel();
        hideStartScreen();ensureInitialTab();showToast('Session resumed');
      };
      if(auto)applyAuto(auto);
      else if(window.SMIdb)window.SMIdb.get('nemo-auto').then(applyAuto).catch(function(){applyAuto(null);});
      else applyAuto(null);
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
