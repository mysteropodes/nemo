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
  // Declared up here (not down with the rest of the tab-bar code below) so
  // it's already initialized by the time initCloseGuard's IIFE runs and
  // calls markSaved() for its boot-time baseline — markSaved touches
  // activeTab()/renderTabBar(), both of which read `tabs`. Textually
  // "var tabs=[]" further down would only be hoisted as undefined at that
  // point, not yet assigned array — .find()/.forEach() on it would throw.
  var tabs=[],activeTabId=null;
  var inactiveEntry=window.NemoNativeOpacityProjectEntry.createInactiveEntry(window,{
    readUi:function p39bReadUi(){return {activeTabId:activeTabId,name:currentName,path:currentPath,frame:state.currentFrame};},
    commit:function p39bCommit(meta){tabs.push({id:makeTabId(),name:meta.name,path:meta.path,json:null,nativePreparation:meta});renderTabBar();return true;},
    remove:function p39bRemove(meta){
      var index=tabs.findIndex(function(t){return t.nativePreparation===meta;});if(index<0)return false;
      tabs.splice(index,1);renderTabBar();return true;
    },
    notice:tabAddNotice
  });
  var nativeTabsUI=window.NemoNativeObjectProjectTabsUI;
  function readNativeTabsUI(){return {tabs:tabs,activeTabId:activeTabId,path:currentPath,name:currentName};}
  var nativeInactiveTabs=nativeTabsUI.bind(window,{read:readNativeTabsUI,makeId:makeTabId,
    file:function(){return nativeSavedUI&&nativeSavedUI.nativeObject;},dirty:isDirty,notice:tabAddNotice,render:renderTabBar,
    activateTab:function(id,path,name){activeTabId=id;currentPath=path;currentName=name;updateCurrentLabel();}
  });
  function p39bUseInactiveEntry(){try{var native=window.NemoNativeOpacityCutover;return !!activeTabId&&(native?native.blocksLegacy()!==false:'NemoNativeOpacityProject' in window);}catch(_){return !!activeTabId;}}

  function tauriOk(){return typeof window.__TAURI__!=='undefined';}
  function importProjectJSON(json,silent,allowOccludedAdmission){return window.NemoNativeOpacityProject?window.NemoNativeOpacityProject.importJSON(json,silent,allowOccludedAdmission):window.SM.importJSON(json,silent);}
  function projectJSON(){return window.NemoNativeOpacityProjectEntry.documentJSON(window);}
  function nativeOpenReady(receipt,first){return window.NemoNativeOpacityProjectEntry.ready(window,receipt,first);}
  function revealOpenedProject(first){return window.NemoNativeOpacityProjectEntry.reveal(window,first,
    {hide:hideStartScreen,show:showStartScreen,repaint:SMProjectEntry.repaint,
      raf:function(callback){return window.requestAnimationFrame(callback);}});}
  function saveFramesIfLegacy(){return window.NemoNativeOpacityProjectEntry.saveFramesIfLegacy(window,saveAllLayerFrames);}
  function afterMaybe(value,next){return value&&typeof value.then==='function'?value.then(next):next(value);}
  function releaseNative(kind){var surface=window.NemoNativeOpacityLegacySurface;if(surface===undefined)return window.NemoNativeOpacityCutover===undefined&&window.NemoNativeOpacityProject===undefined?null:false;try{return surface&&typeof surface.allowProjectTransition==='function'&&typeof surface.releaseProjectTransition==='function'?surface.releaseProjectTransition(window,kind):false;}catch(e){return false;}}
  // Browser-mode autosave: localStorage first (sync, ~5-10MB quota), always
  // mirrored to IndexedDB (async, no practical size ceiling) so a project
  // with embedded media doesn't silently lose its autosave the moment it
  // outgrows localStorage — see project-nemo-web-public-beta memory.
  function autosaveWrite(json){
    var admission=window.NemoNativeOpacityProjectEntry;
    if(!admission||typeof admission.admitAutosaveBytes!=='function'||admission.admitAutosaveBytes(window,json)!==true)return {status:'unavailable'};
    var quotaHit=false;
    try{localStorage.setItem('nemo-auto',json);}catch(e){quotaHit=true;}
    if(window.SMIdb)window.SMIdb.set('nemo-auto',json).catch(function(){});
    if(quotaHit)try{localStorage.removeItem('nemo-auto');}catch(e2){} // stale/truncated slot would win over IDB on next boot otherwise
  }
  function getRecents(){try{return JSON.parse(localStorage.getItem(RECENTS_KEY)||'[]');}catch(e){return[];}}
  function setRecents(list,strict){try{localStorage.setItem(RECENTS_KEY,JSON.stringify(list));}catch(e){if(strict)throw e;}}
  function touchRecent(path,name,meta,strict){
    var list=getRecents().filter(function(r){return r.path!==path;});
    list.unshift({path:path,name:name,canvasW:meta&&meta.canvasW,canvasH:meta&&meta.canvasH,fps:meta&&meta.fps,lastOpened:Date.now()});
    setRecents(list.slice(0,MAX_RECENTS),strict);
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


  // ---- New / Open / Save (native fs, real files on disk) ----
  function newProjectNow(cfg){
    savedBaselineReady=false;
    if(window.SMLabs&&window.SMLabs.resetAll)window.SMLabs.resetAll(); // see labs-core.js's own comment — a Labs prototype must never silently carry into a new project
    if(state.activeSymbolId)exitToScene();
    while(userLayers.length>0)userLayers.pop().remove();state.layers=[];
    Object.keys(_symbolPaperLayers).forEach(function(k){_symbolPaperLayers[k].forEach(function(l){l.remove();});});_symbolPaperLayers={};
    state.symbols={};state.openSymbolTabs=[];state.activeSymbolId=null;
    state.trackRoles={}; // feedback #151 — see state.trackRoles' own comment, app.js
    state.imageMeshes={}; // image-mesh.js — project-level registry, same reset as trackRoles/symbols above
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
    state.guides={h:[],v:[]};
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
    if(window.NemoOpacityApplication)window.NemoOpacityApplication.documentChanged();
    var freshJson=projectJSON();markSaved(freshJson);autosaveWrite(freshJson);
    showToast('New project created');
  }
  function newProject(cfg,stillCurrent,onCreationFailure){return afterMaybe(releaseNative('new-project'),function(admitted){if(admitted===false||!stillCurrent()||typeof window.n20AllowLegacyWrite==='function'&&window.n20AllowLegacyWrite('create-layer')!==true)return false;try{return newProjectNow(cfg);}catch(e){if(onCreationFailure)onCreationFailure(e);return false;}});}

  // Last successfully persisted document, for the close-with-unsaved-work
  // guard below. Unavailable boot/creation must stay dirty until a baseline
  // has actually been admitted; the byte adapter's null sentinel is UI-only.
  var dirtyBaseline=window.NemoProjectDirtyBaseline.create();
  var savedBaselineReady=false;
  function markSaved(json){
    dirtyBaseline.markSaved(json);
    savedBaselineReady=true;
    var t=activeTab();
    if(t)t.dirty=false;
    renderTabBar();
  }
  function isDirty(){return nativeSavedUI?nativeSavedUI.isDirty():true;}

  var nativeSavedUI=window.NemoProjectNativeSave.bindProjectUI(window,{
    read:function(){return {tabId:activeTabId,path:currentPath,name:currentName,canvasW:state.canvasW,canvasH:state.canvasH,fps:state.fps};},
    serialize:projectJSON,prepare:saveFramesIfLegacy,
    commit:function(path,name){
      currentPath=path;currentName=name;updateCurrentLabel();
      var tab=activeTab();if(tab){tab.path=path;tab.name=name;}
    },
    touchRecent:touchRecent,renderRecents:renderRecents,markSaved:markSaved,
    remember:function(json){try{localStorage.setItem('nemo-auto',json);}catch(e){}},
    isDirty:function(){return !savedBaselineReady||dirtyBaseline.isDirty(projectJSON);},
    invalidate:function(){savedBaselineReady=false;try{renderTabBar();}catch(e){}},
    notice:showToast,hide:hideStartScreen,show:showStartScreen,opened:ensureInitialTab,repaint:renderTabBar,
    // Crash-recovery cleanup (#2360 R4); bound below, after the scheduler exists.
    recoveryDiscard:function(documentId,extra){return nativeRecoveryCleanup?nativeRecoveryCleanup.discardDocument(documentId,extra):null;}
  });
  async function writeProjectTo(path,ticket,mode){projectJSON();return nativeSavedUI.write(path,ticket,mode);}
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
    saveFramesIfLegacy();
    var json=projectJSON();
    downloadJson((currentName||'Untitled')+'.json',json);
    markSaved(json);
    autosaveWrite(json);
    showToast(SM.t('toastDownloadedSuffix')+currentName+'.json');
  }
  async function saveAs(){
    if(!tauriOk()){saveAsDownload();return;}
    return nativeSavedUI.saveAs(writeProjectTo);
  }
  async function save(){
    if(!tauriOk()){saveAsDownload();return;}
    return nativeSavedUI.save(writeProjectTo);
  }
  async function openPath(path){
    if(!nativeSavedUI.allowOpen()){try{window.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__&&window.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__('allow-open-refused');}catch(_){}return false;}
    if(nativeSavedUI.nativeObject&&nativeSavedUI.nativeObject.isActive())return nativeInactiveTabs.openPath(path);
    if(p39bUseInactiveEntry())return inactiveEntry.openPath(path);
    if(!tauriOk())return;
    var nativeOpened=await nativeSavedUI.openNative(path);if(nativeOpened!==null)return nativeOpened;
    var json;
    try{json=await window.__TAURI__.fs.readTextFile(path);}
    catch(e){showToast('Could not read file — it may have moved or been deleted');return false;}
    try{
      var opened=await importProjectJSON(json,true,true);
      if(!opened||!nativeOpenReady(opened))throw new Error('Native project admission or replacement failed');
      await revealOpenedProject(opened);
      markSaved(projectJSON());
      currentPath=path;currentName=window.SMProjectDocument.baseName(path);updateCurrentLabel();
      touchRecent(path,currentName,{canvasW:state.canvasW,canvasH:state.canvasH,fps:state.fps});
      renderRecents();
      ensureInitialTab();
      showToast('Opened: '+currentName);
    }catch(e){showToast('Could not open project — native admission or presentation failed');return false;}
  }
  async function openDialog(){
    if(!nativeSavedUI.allowOpen())return false;
    if(!(nativeSavedUI.nativeObject&&nativeSavedUI.nativeObject.isActive())&&p39bUseInactiveEntry())return inactiveEntry.openPicker();
    if(!tauriOk()){document.getElementById('file-input').click();return;}
    var path=await window.__TAURI__.dialog.open({title:'Open Project',multiple:false,filters:[{name:'Nemo Project',extensions:['json']}]});
    if(!path)return;try{await window.__TAURI__.window.getCurrentWindow().setFocus();}catch(_){/* Native presentation still gates Open. */}
    await openPath(Array.isArray(path)?path[0]:path);
  }

  // Crash-recovery writer for native projects (#2360 R2): idle-timed, reads cached session facts only; bytes are written Rust-side.
  try{if(window.NemoNativeObjectRecovery&&nativeSavedUI.nativeObject&&nativeSavedUI.nativeObject.recoveryPin){
    var nativeRecovery=window.NemoNativeObjectRecovery.bind(window,{pin:nativeSavedUI.nativeObject.recoveryPin,
      slot:function(){return {path:currentPath,name:currentName};},notice:showToast});
    if(nativeRecovery)nativeRecovery.start();}}catch(e){console.warn('[recovery] scheduler unavailable',e);}
  var nativeRecoveryCleanup=null;
  try{if(nativeRecovery&&window.NemoNativeObjectRecoveryCleanup)nativeRecoveryCleanup=window.NemoNativeObjectRecoveryCleanup.bind(window,{scheduler:nativeRecovery});}
  catch(e){console.warn('[recovery] cleanup unavailable',e);}
  // ---- Version history (v15) — the existing 30s autosave (timeline.js)
  // only ever held ONE slot ('nemo-auto' in localStorage): a crash right
  // after a bad edit overwrote the one good copy with the bad one on the
  // very next tick. This keeps a dense rolling history of real files on
  // disk (Tauri fs — no localStorage quota risk with base64 media embedded
  // in every snapshot) so "I crashed 10 min ago" can recover "the version
  // from 11 min ago", not just "whatever was last written".
  var HISTORY_MAX=120; // 60 min of history at the existing 30s cadence
  function simpleHash(s){var h=0;for(var i=0;i<s.length;i++)h=(h*31+s.charCodeAt(i))|0;return (h>>>0).toString(36);}
  function historyKey(){return currentPath?(window.SMProjectDocument.baseName(currentPath).replace(/[^a-z0-9]+/gi,'-').toLowerCase()+'-'+simpleHash(currentPath)):'untitled-autosave';}
  async function historyDir(){
    var base=await window.__TAURI__.path.appDataDir();
    return base.replace(/[\\/]+$/,'')+'/history/'+historyKey();
  }
  function writeVersionSnapshot(json,dir){return window.NemoProjectNativeVersionRestore.writeSnapshot(json,dir,window.__TAURI__.fs,HISTORY_MAX,function(){return Date.now();});}
  async function pushVersionSnapshot(json){
    if(!tauriOk())return; // browser preview: 'nemo-auto' single-slot fallback only
    try{await writeVersionSnapshot(json,await historyDir());}
    catch(e){console.warn('[history] snapshot failed',e);}
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
  var nativeVersionRestore=window.NemoProjectNativeVersionRestore.bind(window,{
    metadata:function(){return {path:currentPath,name:currentName};},
    backupTarget:historyDir,reveal:revealOpenedProject,
    modal:function(){return document.getElementById('history-modal');},
    backup:writeVersionSnapshot,
    backupFailed:function(error){console.warn('[history] pre-restore snapshot failed',error);},
    restored:function(){ensureInitialTab();if(historyUi)historyUi.close();showToast(SM.t('toastVersionRestored'));},
    unavailable:function(){showToast('Could not finish restoring this version.');}
  });
  async function restoreVersion(path){
    if(tauriOk()||'NemoNativeOpacityCutover' in window||'NemoNativeOpacityProject' in window)return nativeVersionRestore.restore(path);
    // Capture the current document before import, then record that snapshot
    // only after a successful restore. Rejected input must leave both the
    // current project and its version history unchanged.
    var json=await window.__TAURI__.fs.readTextFile(path);
    try{window.SMProjectDocument.parse(json);}catch(e){return false;}
    saveAllLayerFrames();
    var previousJson=window.SM.exportJSON();
    if(!await importProjectJSON(json,true))return false;
    try{await pushVersionSnapshot(previousJson);}catch(e){console.warn('[history] pre-restore snapshot failed',e);}
    ensureInitialTab();
    showToast(SM.t('toastVersionRestored'));
    return true;
  }

  function relTime(ts){
    var s=Math.max(0,Math.round((Date.now()-ts)/1000));
    if(s<60)return 'il y a '+s+'s';
    var m=Math.round(s/60);
    if(m<60)return 'il y a '+m+' min';
    var h=Math.round(m/60);
    return 'il y a '+h+'h'+(m%60?Math.round(m%60)+'min':'');
  }
  var historyUi=null;
  function openHistoryModal(){
    if(!historyUi)historyUi=window.NemoProjectHistoryPlacement.bind(window,{
      context:function(){return {path:currentPath,name:currentName};},
      list:function(){return tauriOk()?listVersionHistory():Promise.resolve([]);},
      restore:restoreVersion,notice:showToast,
      relTime:relTime
    });
    return historyUi.open();
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
    if(!root){showToast(SM.t('hsSetSyncFolderFirst'));return;}
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

  function nativeNewAllowed(){
    try{return nativeInactiveTabs.canReplaceActiveProject()===true&&
      typeof nativeSavedUI.nativeObject.canOpen==='function'&&nativeSavedUI.nativeObject.canOpen()===true;}catch(_){return false;}
  }
  function nativeNewPin(p){
    p=p||window.NemoNativeObjectProject.desktop(window).readPresentedObjectSession();if(!p)throw new Error('Native editor return is unavailable');
    return Object.freeze({tabId:activeTabId,instanceId:p.instanceId,documentId:p.documentId,contentRevision:p.contentRevision,documentSnapshotId:p.documentSnapshotId,lifecycleGeneration:p.lifecycleGeneration});
  }
  var nativeNewWait=0;/* #2372: a presentation hidden only by the native queue (queue-pending) is waited for, bounded (adapters/native-new-return-pin.js); null = refuse as before. */function nativeNewSoon(holds,ok,fail){var N=window.NemoNativeNewReturnPin;return N?N.soon(window,holds,ok,fail):null;}
  function cancelNativeNew(context){
    if(!nativeNewReturn)return;var token=++nativeNewWait,pinned=nativeNewReturn;
    function restore(current){if(!context.submitted&&!context.inFlight&&nativeNewAllowed()&&Object.keys(pinned).every(function(k){return current[k]===pinned[k];})){nativeNewReturn=null;hideStartScreen();SMProjectEntry.repaint();return true;}}
    try{if(restore(nativeNewPin()))return true;}
    catch(_){nativeNewSoon(function(){return token===nativeNewWait&&nativeNewReturn===pinned;},function(p){try{restore(nativeNewPin(p));}catch(_){}});}
    showStartScreen();return false;
  }
  var newProjectGeneration=0;window.SMProject={nativeTabs:nativeInactiveTabs,nativeObjectProject:function(){return nativeSavedUI&&nativeSavedUI.nativeObject||null;},save:save,saveAs:saveAs,open:openDialog,openPath:openPath,newProject:function(cfg,isCurrent,onCreationFailure){
    if(nativeSavedUI&&nativeSavedUI.nativeObject){
      if(!nativeNewAllowed())return tabAddNotice('New project unavailable — close the inactive project or finish the pending project operation first.');
      return typeof nativeSavedUI.nativeObject.newProject==='function'?nativeSavedUI.nativeObject.newProject(cfg,isCurrent,onCreationFailure):false;
    }
    if(tauriOk()||window.NemoNativeObjectProject)return false;
    var generation=++newProjectGeneration;return afterMaybe(newProject(cfg,function(){return generation===newProjectGeneration&&(!isCurrent||isCurrent());},onCreationFailure),function(admitted){if(admitted===false)return false;try{hideStartScreen();ensureInitialTab();}catch(e){if(onCreationFailure)onCreationFailure(e);return false;}});},
    // "A project is now open, show the editor" — hideStartScreen +
    // ensureInitialTab, the pair newProject above already runs. Exported
    // (2026-09 QA sweep) because kitsu.js called those two by their bare
    // names, which live only in this closure: opening a shot from Kitsu
    // threw ReferenceError right after the shot had loaded, so the start
    // screen stayed up, no tab was created, and the user got
    // "hideStartScreen is not defined" in the shot list instead of the
    // success toast.
    enterEditor:function(){hideStartScreen();ensureInitialTab();},pushVersionSnapshot:pushVersionSnapshot,listVersionHistory:listVersionHistory,restoreVersion:restoreVersion,autosaveWrite:autosaveWrite,
    getSyncFolder:getSyncFolder,chooseSyncFolder:chooseSyncFolder,disableSync:disableSync,publishToShared:publishToShared,checkSharedUpdates:checkSharedUpdates,pullAndMerge:pullAndMerge,
    // Stable per-project filesystem-safe identifier — same slug/hash
    // feedback-bridge.js's local + shared feedback storage keys off, so a
    // feedback thread and this project's own history/sync folders always
    // agree on which project they belong to without re-deriving the logic.
    getProjectKey:historyKey,profileDir:profileDir,isDirty:isDirty,markSaved:function(){try{markSaved(projectJSON());}catch(e){}},getCurrentLabel:getCurrentLabel,
    refreshActiveTabDirtyDot:refreshActiveTabDirtyDot,
    // Read-only list of every OTHER open project tab (feedback #109: "voir
    // apparaître les différents projet test1, test2... afin de pouvoir les
    // glisser comme un componant dans un autre projet") — transplant.js
    // consumes this to offer picking a source project without a file dialog.
    // Each tab already holds a full exportJSON()-format snapshot the moment
    // it's not the active one (switchToTab's own snapshotActiveIntoTab), so
    // this needed no new storage — just a reader. The active tab is excluded
    // outright (nothing to transplant from yourself, and its own .json field
    // is stale until the next tab switch — see snapshotActiveIntoTab).
    getOpenTabs:function(){
      return tabs.filter(function(t){return t.id!==activeTabId;})
        .map(function(t){return {id:t.id,name:t.name,json:t.json};})
        .filter(function(t){return !!t.json;});
    }};

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
    try{markSaved(projectJSON());}catch(e){}
    if(tauriOk()&&window.__TAURI__.window&&window.__TAURI__.window.getCurrentWindow){
      try{
        var appWindow=window.__TAURI__.window.getCurrentWindow();
        var allowConfirmedClose=false;
        appWindow.onCloseRequested(async function(event){
          if(allowConfirmedClose)return; // confirmed quit-without-saving keeps the recovery (parity)
          if(!isDirty()){ // clean quit: drop the ACTIVE document's crash recovery only (parked dirty tabs keep theirs), bounded, best effort
            if(nativeRecoveryCleanup)try{await nativeRecoveryCleanup.cleanQuit();}catch(_){}
            return;
          }
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
  // `tabs`/`activeTabId` themselves are declared up near RECENTS_KEY —
  // see the comment there for why.
  function makeTabId(){return 't'+Date.now().toString(36)+Math.random().toString(36).slice(2,6);}
  function activeTab(){return tabs.find(function(t){return t.id===activeTabId;});}
  function snapshotActiveIntoTab(){
    var t=activeTab();if(!t)return;
    saveAllLayerFrames();
    t.json=window.SM.exportJSON();
    // Captured HERE, before any import happens — isDirty()'s raw-string
    // comparison against the saved baseline is only trustworthy same-generation
    // (live export vs. live export). Carrying this as a plain boolean past
    // an import boundary avoids the trap below (see switchToTab).
    t.dirty=isDirty();
    t.name=currentName;t.path=currentPath;
  }
  function ensureInitialTab(){
    // Called after every entry point that makes a project "live" (new,
    // opened, or resumed autosave) — only actually creates a tab the FIRST
    // time (app boot), otherwise just refreshes the active tab's own
    // name/path so e.g. a fresh Save As renames its own tab in place
    // rather than spawning a duplicate.
    if(!tabs.length){
      activeTabId=makeTabId();
      // isDirty() here reflects whatever markSaved() call already ran
      // before this point (newProject/openPath: clean) or didn't (Resume
      // Last Session deliberately never calls markSaved — an unsaved
      // autosave restore correctly starts dirty, matching the close-guard's
      // own pre-existing behavior).
      tabs.push({id:activeTabId,name:currentName,json:null,path:currentPath,dirty:isDirty()});
    }else{
      var t=activeTab();
      if(t){t.name=currentName;t.path=currentPath;}
    }
    renderTabBar();
  }
  function populatedTabTransitionAllowed(){
    try{
      if('__TAURI__' in window||'NemoNativeOpacityProject' in window)return false;
      var location=window.location;
      if(!location||!['http:','https:'].includes(location.protocol)||
        typeof location.hostname!=='string'||!location.hostname)return false;
      if('NemoNativeOpacityCutover' in window){
        var native=window.NemoNativeOpacityCutover;
        return !!native&&typeof native.blocksLegacy==='function'&&native.blocksLegacy()===false;
      }
      return true;
    }catch(error){return false;}
  }
  function switchToTab(id){
    if(id===activeTabId)return;
    var target=tabs.find(function(t){return t.id===id;});if(!target)return;if(target.nativeInactiveProject)return nativeInactiveTabs.activateClick(target.nativeInactiveProject);
    if(target.nativePreparation||target.json&&!populatedTabTransitionAllowed())return tabAddNotice('Switching project tabs is unavailable in this build.');
    var blankAdmission=target.json?null:releaseNative('tab-switch');if(blankAdmission===false)return false;
    snapshotActiveIntoTab();
    function entered(){
      currentPath=target.path||null;currentName=target.name;updateCurrentLabel();
      renderTabBar();
    }
    if(target.json){
      return afterMaybe(importProjectJSON(target.json,true),function(imported){
        if(!imported)return;
        activeTabId=id;
        // importJSON normalizes (fills defaults, pads frames — same trap
        // openPath's own comment already documents), so a fresh exportJSON()
        // right after this import can differ textually from target.json even
        // with zero real changes. target.dirty was captured pre-import.
        dirtyBaseline.restoreTab(target.dirty,function(){return window.SM.exportJSON();});
        entered();
      });
    }
    return afterMaybe(blankAdmission,function(admitted){
      if(admitted===false)return false;activeTabId=id;newProjectNow({w:1920,h:1080,fps:24,name:target.name});entered();
    }); // markSaved belongs to the new tab
  }
  var tabAddGeneration=0,tabAdmissionMessage=null,nativeNewReturn=null;
  function tabAddNotice(message){
    if(!tabAdmissionMessage){var bar=document.getElementById('project-tabs-bar');tabAdmissionMessage=document.createElement('div');tabAdmissionMessage.setAttribute('role','alert');
      Object.assign(tabAdmissionMessage.style,{marginLeft:'auto',minWidth:'0',maxWidth:'60%',overflow:'hidden',textOverflow:'ellipsis',whiteSpace:'nowrap',color:'#ffb5a8',fontSize:'12px'});bar.appendChild(tabAdmissionMessage);}tabAdmissionMessage.textContent=message;tabAdmissionMessage.style.display=message?'block':'none';return false;}
  function addTab(){
    if(nativeSavedUI&&nativeSavedUI.nativeObject){
      if(!nativeNewAllowed())
        return tabAddNotice('New project unavailable — close the inactive project or finish the pending project operation first.');
      // Native New replaces the active project through its existing form and
      // confirmation; it never snapshots the document into a JavaScript tab.
      var unready='New project unavailable — the native editor is not ready.',enter=function(){tabAddNotice('');showStartScreen();document.getElementById('start-new').click();
        if(nativeSavedUI.nativeObject.isActive())showToast('Creating a new project replaces the current project.');return true;};
      try{if(document.getElementById('start-screen').classList.contains('hid')&&nativeSavedUI.nativeObject.isActive())nativeNewReturn=nativeNewPin();}
      catch(_){var token=++nativeNewWait,tab=activeTabId;return nativeNewSoon(function(){return token===nativeNewWait&&tab===activeTabId&&document.getElementById('start-screen').classList.contains('hid');},
        function(p){try{nativeNewReturn=nativeNewPin(p);}catch(_){tabAddNotice(unready);return;}enter();},function(status){if(status!=='changed')tabAddNotice(unready);})||tabAddNotice(unready);}
      return enter();
    }
    var generation=++tabAddGeneration,initialTab=activeTabId,initialName=currentName,initialPath=currentPath;function denied(){return generation===tabAddGeneration&&activeTabId===initialTab&&currentName===initialName&&currentPath===initialPath?tabAddNotice('New project unavailable — native admission was denied.'):false;}
    function admitted(ok){
      if(generation!==tabAddGeneration||activeTabId!==initialTab||currentName!==initialName||currentPath!==initialPath)return false;if(ok===false||typeof window.n20AllowLegacyWrite==='function'&&window.n20AllowLegacyWrite('create-layer')!==true)return denied();
      tabAddNotice('');snapshotActiveIntoTab();var n=tabs.length+1,id=makeTabId();
      tabs.push({id:id,name:'Untitled '+n,json:null,path:null});
      activeTabId=id;newProjectNow({w:1920,h:1080,fps:24,name:'Untitled '+n});currentPath=null;currentName='Untitled '+n;updateCurrentLabel();renderTabBar();}
    try{var result=releaseNative('tab-add');return result&&typeof result.then==='function'?result.then(admitted,denied):admitted(result);}catch(e){return denied();}
  }
  function closeTab(id){
    var idx=tabs.findIndex(function(t){return t.id===id;});if(idx<0)return;if(tabs[idx].nativeInactiveProject)return nativeInactiveTabs.close(tabs[idx].nativeInactiveProject);
    if(tabs.length===1){showToast(SM.t('hsCannotCloseLastTab'));return;}
    if(tabs[idx].nativePreparation)return inactiveEntry.close(tabs[idx].nativePreparation);
    var wasActive=id===activeTabId;
    function closed(){tabs.splice(idx,1);renderTabBar();}
    if(!wasActive){closed();return;}
    if(tabs.some(function(t){return !!t.nativePreparation;})||!populatedTabTransitionAllowed())return tabAddNotice('Closing the active project tab is unavailable in this build.');
    var next=tabs[idx>0?idx-1:1];
    function enterBlank(admitted){if(admitted===false)return false;activeTabId=next.id;newProjectNow({w:1920,h:1080,fps:24,name:next.name});
      currentPath=next.path||null;currentName=next.name;updateCurrentLabel();closed();}
    if(next.json){
      return afterMaybe(importProjectJSON(next.json,true),function(imported){
        if(!imported)return;
        activeTabId=next.id;
        dirtyBaseline.restoreTab(next.dirty,function(){return window.SM.exportJSON();}); // see switchToTab's comment for why
        currentPath=next.path||null;currentName=next.name;updateCurrentLabel();closed();
      });
    }
    return afterMaybe(releaseNative('tab-close'),enterBlank);
  }
  function startTabRename(id){return nativeTabsUI.rename(window,id,{read:readNativeTabsUI,render:renderTabBar,
    renameActive:function(value){currentName=value;updateCurrentLabel();}});}
  // Per-tab "unsaved changes" dirty state. The active tab's own document
  // lives in live `state`, not in its tab.json (that only gets refreshed on
  // switch-away by snapshotActiveIntoTab) — so it needs isDirty()'s live
  // exportJSON() comparison. An inactive tab's content is frozen since the
  // switch away from it, and t.dirty was captured at that exact moment
  // (see snapshotActiveIntoTab) — just read it back, no comparison needed.
  function tabIsDirty(t){
    return t.id===activeTabId?isDirty():!!t.dirty;
  }
  function renderTabBar(){return nativeTabsUI.render(window,{read:readNativeTabsUI,dirty:tabIsDirty,
    native:function(){return !!(nativeSavedUI&&nativeSavedUI.nativeObject);},switchTab:switchToTab,closeTab:closeTab,renameTab:startTabRename});}
  // Cheap DOM-only refresh for the active tab's dot, called from the
  // existing 30s autosave tick (timeline.js) — reuses the exportJSON() it
  // already pays for instead of triggering a second full-document
  // serialization just to keep this indicator current while the user
  // keeps editing the same tab without switching/saving/adding another.
  function refreshActiveTabDirtyDot(liveJson){return nativeTabsUI.refreshDirty(window,activeTabId,function(){
    return nativeSavedUI.nativeObject&&nativeSavedUI.nativeObject.isActive()?isDirty():!savedBaselineReady||dirtyBaseline.isDirtyBytes(liveJson);
  });}
  document.getElementById('project-tab-add')&&document.getElementById('project-tab-add').addEventListener('click',addTab);

  // ---- Start screen ----
  function hideStartScreen(){document.getElementById('start-screen').classList.add('hid');}
  var startScreenView=null; // re-decides the Resume card on every show
  function showStartScreen(){document.getElementById('start-screen').classList.remove('hid');if(startScreenView)startScreenView.refresh();}

  function renderRecents(){
    var snapshot;
    SMProjectEntry.renderRecents(document,{
      readRows:function(){
        snapshot=getRecents();
        if(!snapshot.length)return [];
        return snapshot.map(function(r,index){
          return {key:index,name:r.name,canvasW:r.canvasW,canvasH:r.canvasH,fps:r.fps,lastOpened:r.lastOpened};
        });
      },
      translate:function(key){return window.SM.t(key);},
      openRow:function(key){openPath(snapshot[key].path);},
      removeRow:function(key){removeRecent(snapshot[key].path);renderRecents();}
    });
  }

  function resumeAutosave(){
      // Load autosave explicitly; boot may still hold a blank project.
      // Native Resume follows Open's occluded admission and final revealed
      // presentation before publishing its tab or success.
      var auto=null;
      try{auto=localStorage.getItem('nemo-auto');}catch(e){}
      var applyAuto=function(auto){
        function applied(imported){
          if(auto&&(!imported||!nativeOpenReady(imported)))throw new Error('Invalid or unpresented autosave');
          return afterMaybe(revealOpenedProject(imported),function(){
            currentPath=null;currentName='Untitled';updateCurrentLabel();
            ensureInitialTab();showToast('Session resumed');
          });
        }
        try{
          var importing=auto?importProjectJSON(auto,true,true):true;
          var result=afterMaybe(importing,applied);
          return result&&typeof result.catch==='function'?result.catch(function(){showToast(SM.t('toastCannotResumeSessionCorrupt'));}):result;
        }catch(e){showToast(SM.t('toastCannotResumeSessionCorrupt'));}
      };
      if(auto)applyAuto(auto);
      else if(window.SMIdb)window.SMIdb.get('nemo-auto').then(applyAuto).catch(function(){applyAuto(null);});
      else applyAuto(null);
  }
  // Native desktop Resume (#2360 R3): the card follows the Rust recovery store, never the legacy nemo-auto slot.
  var nativeResume=window.NemoNativeObjectRecoveryResume&&nativeSavedUI.nativeObject&&nativeSavedUI.nativeObject.openRecovered?
    window.NemoNativeObjectRecoveryResume.bind(window,{openRecovered:nativeSavedUI.openRecovered,
      allowOpen:nativeSavedUI.allowOpen,notice:showToast,ownSlots:function(){return nativeRecovery?nativeRecovery.claim():[];}}):null;
  function initStartScreen(){
    SMProjectEntry.initializeTabs(document);
    startScreenView=SMProjectEntry.bindStartScreen(document,{
      readResumePresence:function(){if(nativeResume)return false;try{return !!localStorage.getItem('nemo-auto');}catch(e){return false;}},
      // Media-heavy autosave can be present only in IndexedDB after quota
      // overflow. Resolve to a boolean before crossing the view boundary.
      readResumePresenceAsync:function(publish){
        if(nativeResume){nativeResume.presence(publish);return;}
        if(window.SMIdb)window.SMIdb.get('nemo-auto').then(function(v){publish(!!v);}).catch(function(){});},
      renderRecents:renderRecents,resume:nativeResume?nativeResume.resume:resumeAutosave,open:openDialog,
      createProject:function(config,current,onCreationFailure){return window.SMProject.newProject(config,current,onCreationFailure);},
      showStartScreen:showStartScreen,cancelProjectEntry:cancelNativeNew,notice:showToast
    });

    // Project panel buttons (right-hand Project section)
    document.getElementById('btn-save').addEventListener('click',save);
    document.getElementById('btn-saveas').addEventListener('click',saveAs);
    document.getElementById('btn-open').addEventListener('click',openDialog);
    document.getElementById('btn-new').addEventListener('click',showStartScreen);
    // Version history modal (v15)
    var histBtn=document.getElementById('btn-history');
    if(histBtn)histBtn.addEventListener('click',openHistoryModal);
    document.getElementById('file-input').addEventListener('change',function(e){
      var f=e.target.files[0];if(!f)return;
      var r=new FileReader();r.onload=function(ev){try{var result=afterMaybe(importProjectJSON(ev.target.result,true,true),function(imported){if(!imported||!nativeOpenReady(imported))throw new Error('Invalid or unpresented project');return afterMaybe(revealOpenedProject(imported),function(){markSaved(projectJSON());currentPath=null;currentName=window.SMProjectDocument.baseName(f.name)||'Untitled';updateCurrentLabel();ensureInitialTab();showToast('Opened: '+currentName);});});if(result&&typeof result.catch==='function')result.catch(function(){showToast('Could not open file — it may be invalid or corrupted');});}catch(err){showToast('Could not open file — it may be invalid or corrupted');}};
      r.onerror=function(){showToast('Could not open file — it may be invalid or corrupted');};
      r.readAsText(f);e.target.value='';
    });

    updateCurrentLabel();
  }

  if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initStartScreen);
  else initStartScreen();
})();
