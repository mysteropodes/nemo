/* Opt-in, one-shot timing control for real responses in an owned desktop task. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else {root.NemoNativeSettingsValidationGate=api;api.install(root);}}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var installed=new WeakMap(),schema='nemo.settings-validation/1',modes=['settings-response','inactive-inspect'];
  function exact(value,keys){return !!value&&typeof value==='object'&&!Array.isArray(value)&&
    Object.keys(value).sort().join(',')===keys.slice().sort().join(',');}
  function admitted(runtime,config){
    return runtime&&runtime.schema==='nemo.native-runtime/1'&&runtime.isolated===true&&runtime.state==='active'&&
      runtime.ownerTokenConfigured===true&&Number.isSafeInteger(runtime.pid)&&runtime.pid>0&&
      Array.isArray(runtime.windows)&&runtime.windows.length===1&&runtime.windows[0]==='main'&&
      /^[a-f0-9]{64}$/.test(runtime.taskKey)&&exact(config,['schema','taskKey','runId','mode'])&&config.schema===schema&&
      config.taskKey===runtime.taskKey&&/^[a-f0-9]{32}$/.test(config.runId)&&modes.includes(config.mode);
  }
  function matches(mode,kind,request,value){
    if(!request||typeof request.requestId!=='string'||!request.instanceId||!request.documentId)return false;
    if(mode==='settings-response')return kind===mode&&request.operation==='command.document.object.project.settings.edit'&&
      request.payload&&request.payload.action&&request.payload.action.kind==='setFps'&&value&&value.ok===true&&
      value.result&&value.result.applied===true&&value.result.historyEntriesAdded===1;
    return kind==='inactive-inspect'&&request.action&&request.action.kind==='inspect'&&
      typeof request.action.preparationToken==='string'&&value&&value.state==='inactive-retained';
  }
  function create(runtime,config,ports){
    if(!admitted(runtime,config))return null;
    var used=false,events=[],serial=0,write=Promise.resolve();
    var pins={schema:schema,taskKey:runtime.taskKey,runId:config.runId,pid:runtime.pid,mode:config.mode};
    function record(phase,request){
      var row={sequence:++serial,at:ports.now(),phase:phase};
      if(request)['requestId','instanceId','documentId','expectedRevision','lifecycleGeneration','documentSnapshotId'].forEach(function(k){
        if(typeof request[k]==='string'||Number.isSafeInteger(request[k]))row[k]=request[k];});
      events.push(row);events=events.slice(-8);
      var bytes=JSON.stringify(Object.assign({},pins,{events:events}));
      write=write.catch(function(){}).then(function(){return ports.write(bytes);});return write;
    }
    var ready=record('ready');
    async function hold(kind,request,value){
      if(used||!matches(config.mode,kind,request,value))return value;
      used=true;var ended=false,timer;
      var timeout=new Promise(function(resolve){timer=ports.setTimer(function(){
        ended=true;record('expired',request).catch(function(){});resolve();},60000);});
      var run=(async function(){
        await ready;if(ended)return;
        await record('held',request);if(ended)return;
        if(ports.notice)ports.notice('Validation: native response held.');
        while(!ended){
          var release=null;try{release=await ports.read();}catch(_){}
          if(ended)return;
          if(exact(release,['schema','taskKey','runId','requestId','action'])&&release.schema===schema&&
             release.taskKey===pins.taskKey&&release.runId===pins.runId&&release.requestId===request.requestId&&release.action==='release'){
            await record('released',request);return;
          }
          await ports.delay(100);
        }
      })();
      try{await Promise.race([run,timeout]);}
      catch(_){record('failed',request).catch(function(){});}
      finally{ended=true;ports.clearTimer(timer);}
      // The native command already completed. Never replace its receipt or retry it.
      return value;
    }
    return Object.freeze({ready:ready,hold:hold});
  }
  function install(root){
    if(installed.has(root))return installed.get(root);
    var entry={gate:null,ready:null};installed.set(root,entry);
    entry.ready=(async function(){
      var api=root.__TAURI__;if(!api||!api.core||!api.fs)return;
      var runtime=await api.core.invoke('nemo_task_runtime');
      if(!runtime||runtime.isolated!==true||runtime.ownerTokenConfigured!==true)return;
      var env=runtime.processEnvironment,dir=env&&env.tempDir;
      if(typeof runtime.dataDir!=='string'||!runtime.dataDir.endsWith('/tauri-data')||
         dir!==runtime.dataDir.slice(0,-11)+'/tmp'||env.NEMO_REPORT_DIR!==runtime.dataDir.slice(0,-11)+'/reports')return;
      async function read(name){var bytes=await api.fs.readTextFile(dir+'/'+name);
        if(typeof bytes!=='string'||bytes.length>4096||new TextEncoder().encode(bytes).length>4096)throw new Error('Validation packet refused');
        return JSON.parse(bytes);}
      var config=await read('settings-validation-config.json');
      var gate=create(runtime,config,{
        now:function(){return Date.now();},setTimer:setTimeout,clearTimer:clearTimeout,
        delay:function(ms){return new Promise(function(resolve){setTimeout(resolve,ms);});},
        read:function(){return read('settings-validation-release.json');},
        write:async function(bytes){await api.fs.writeTextFile(dir+'/settings-validation.pending.json',bytes);
          await api.fs.rename(dir+'/settings-validation.pending.json',dir+'/settings-validation.json');},
        notice:function(message){if(root.showToast)root.showToast(message);}
      });
      if(gate){await gate.ready;entry.gate=gate;}
    })().catch(function(){entry.gate=null;});
    return entry;
  }
  function hold(root,kind,request,promise){
    var entry=install(root);
    // Readiness is an explicit operator prerequisite. Ordinary work never waits
    // for probe discovery, an absent config, or a failed diagnostics filesystem.
    if(!entry.gate)return promise;
    return Promise.resolve(promise).then(function(value){return entry.gate.hold(kind,request,value);});
  }
  return Object.freeze({install:install,hold:hold,create:create});
}));
