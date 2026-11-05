/* Baseline project files open as a NEW native document (the Rust host converts them; the file is never modified). This
   adapter holds the UI side: recognising the host's loss report, the notices, the Save-As guard that keeps the original file
   safe, and the two entry points that start from text instead of a path (an old autosave, a history snapshot). */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeLegacyImport=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var FOLDER='legacy-import',MAX_WARNINGS=50;
  function plain(v){return !!v&&typeof v==='object'&&!Array.isArray(v);}
  function text(v,limit){return typeof v==='string'?v.slice(0,limit):'';}
  // The marker is the host's `legacyImport` member on an open receipt or an inactive-project metadata object.
  function reportOf(value){
    var raw=plain(value)?value.legacyImport:null;
    if(!plain(raw))return null;
    var warnings=(Array.isArray(raw.warnings)?raw.warnings:[]).slice(0,MAX_WARNINGS).filter(plain).map(function(w){
      return {path:text(w.path,300),code:text(w.code,64),message:text(w.message,300)};});
    var total=Number.isSafeInteger(raw.warningsTotal)&&raw.warningsTotal>=warnings.length?raw.warningsTotal:warnings.length;
    return Object.freeze({warnings:Object.freeze(warnings),warningsTotal:total,
      sourceVersion:typeof raw.sourceVersion==='number'?raw.sourceVersion:null,
      migrations:Array.isArray(raw.migrations)?raw.migrations.map(function(m){return text(m,80);}):[],
      ignored:Array.isArray(raw.ignored)?raw.ignored.map(function(m){return text(m,80);}):[],
      counts:plain(raw.counts)?raw.counts:{}});
  }
  function isLegacy(value){return reportOf(value)!==null;}
  // Baseline wording for the open toast, then what the conversion means for the user's next Save.
  function notice(name,report,translate){
    var message='Opened: '+name+' — imported as a new project; the original file is not modified. Use Save As to keep your work.';
    if(!report)return message;
    var newer=report.warnings.filter(function(w){return w.code==='newer_format_version';})[0];
    if(newer&&typeof translate==='function'&&report.sourceVersion!==null){
      message=translate('toastFileFromNewerVersionHint')+report.sourceVersion+translate('toastUpdateAppBeforeResave')+' — '+message;
    }
    var lost=report.warningsTotal-(newer?1:0);
    if(lost>0){
      var first=report.warnings.filter(function(w){return w!==newer;})[0];
      message+=' '+lost+' item(s) were not carried over'+(first?' (first: '+first.path+')':'')+'.';
    }
    return message;
  }
  function normalize(path){return String(path||'').replace(/\\/g,'/').replace(/\/+/g,'/').toLowerCase();}
  // Save As must never pick the baseline file itself: that is the one write the open promised not to make.
  function samePath(a,b){return !!a&&!!b&&normalize(a)===normalize(b);}
  function safeName(name){
    var clean=String(name||'').replace(/[\\/:*?"<>|\u0000-\u001f]+/g,' ').replace(/\s+/g,' ').trim().slice(0,80);
    return clean||'Untitled';
  }
  // Text -> file -> the ordinary path admission, so the host stays the only converter. deps: appDataDir(), fs{mkdir,writeTextFile},
  // open(path) resolving to the open result.
  async function openText(deps,json,name){
    if(typeof json!=='string'||!json)return false;
    var base=String(await deps.appDataDir()).replace(/[\\/]+$/,'')+'/'+FOLDER;
    await deps.fs.mkdir(base,{recursive:true});
    var path=base+'/'+safeName(name)+'.json';
    await deps.fs.writeTextFile(path,json);
    return deps.open(path);
  }
  // A baseline project: a JSON object holding `layers` (or the oldest `frames`) and no native `format`.
  function looksLikeProject(json){
    try{var value=JSON.parse(json);return plain(value)&&!('format' in value)&&(Array.isArray(value.layers)||Array.isArray(value.frames));}
    catch(_){return false;}
  }
  function isNativeText(json){return /^\s*\{[\s\S]{0,400}?"format"\s*:\s*"nemo\.native-/.test(String(json));}
  // A history snapshot written by the baseline app is a baseline project. A native snapshot is not restored through this route.
  async function restoreSnapshot(deps,path,name){
    var json=await deps.fs.readTextFile(path);
    if(isNativeText(json))return false;
    return openText(deps,json,name+' (restored)');
  }
  // Start-screen Resume: the Rust recovery store first (native.list/resume), then an old baseline autosave in the browser slot.
  // legacy: {present():Promise<boolean>, resume():Promise<*>}.
  function combineResume(native,legacy){
    return Object.freeze({
      presence:function(publish){
        return native.presence(function(found){
          if(found){publish(true);return;}
          legacy.present().then(publish,function(){publish(false);});
        });
      },
      resume:async function(){
        var entries=[];
        try{entries=await native.list();}catch(_){entries=[];}
        if(entries.length)return native.resume();
        var legacyPresent=false;
        try{legacyPresent=await legacy.present();}catch(_){legacyPresent=false;}
        return legacyPresent?legacy.resume():native.resume();
      }
    });
  }
  // The old autosave slot: localStorage first, IndexedDB after a quota overflow (project.js autosaveWrite).
  function autosaveSource(deps){
    async function read(){
      var value=null;
      try{value=deps.storage.getItem('nemo-auto');}catch(_){value=null;}
      if(!value&&deps.idb)try{value=await deps.idb.get('nemo-auto');}catch(_){value=null;}
      return typeof value==='string'&&looksLikeProject(value)?value:null;
    }
    return Object.freeze({
      present:async function(){return (await read())!==null;},
      resume:async function(){
        try{
          var json=await read();
          if(json===null||isNativeText(json))throw new Error('No baseline autosave');
          var opened=await openText(deps,json,'Session');
          if(opened)deps.notice('Session resumed');
          else deps.notice(deps.corruptText());
          return !!opened;
        }catch(_){deps.notice(deps.corruptText());return false;}
      }
    });
  }
  // Binds to the desktop native project. ports: object (the file UI), open(path), name(), notice(text), restored(), idb, storage.
  // Returns null off the desktop, so browser mode keeps the baseline importer.
  function bind(root,ports){
    if(!ports.object||!root.__TAURI__||!root.__TAURI__.fs||!root.__TAURI__.path)return null;
    var tauri=root.__TAURI__;
    var deps={appDataDir:function(){return tauri.path.appDataDir();},fs:tauri.fs,open:ports.open,notice:ports.notice,
      storage:ports.storage||root.localStorage,idb:ports.idb,corruptText:function(){return root.SM.t('toastCannotResumeSessionCorrupt');}};
    return Object.freeze({
      // A version restore replaces the open native document with the (converted) baseline snapshot.
      owns:function(){return ports.object.isActive()===true;},
      restore:async function(path){
        var done=await restoreSnapshot(deps,path,ports.name());
        if(done&&ports.restored)ports.restored();
        return !!done;
      },
      combineResume:function(native){return combineResume(native,autosaveSource(deps));}
    });
  }
  return Object.freeze({bind:bind,reportOf:reportOf,isLegacy:isLegacy,notice:notice,samePath:samePath,safeName:safeName,openText:openText,
    isNativeText:isNativeText,looksLikeProject:looksLikeProject,restoreSnapshot:restoreSnapshot,combineResume:combineResume,autosaveSource:autosaveSource,FOLDER:FOLDER});
}));
