'use strict';
// Actual bridge/adapter with fake DOM, timers, fetch and updater-native boundaries.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const Platform=require('../src/js/adapters/updater-platform.js');
const bridge=fs.readFileSync(path.join(__dirname,'../src/js/updater-bridge.js'),'utf8');
const variants=[['proposed',bridge]];
if(process.env.NEMO_UPDATER_BASELINE)variants.push(['baseline',fs.readFileSync(process.env.NEMO_UPDATER_BASELINE,'utf8')]);
const turn=()=>new Promise(setImmediate);
function gate(){let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};}
function element(){
  const listeners={},classes=new Set(),children=[];let text='',html='',className='';
  return {listeners,style:{},disabled:false,title:'',children,
    get className(){return className;},set className(v){className=v;classes.clear();v.split(' ').filter(Boolean).forEach(c=>classes.add(c));},
    classList:{contains:k=>classes.has(k),add:k=>classes.add(k),remove:k=>classes.delete(k)},
    get textContent(){return text;},set textContent(v){text=v;children.length=0;},get innerHTML(){return html;},set innerHTML(v){html=v;children.length=0;},
    appendChild:e=>children.push(e),addEventListener:(type,fn)=>listeners[type]=fn,fire:type=>listeners[type]&&listeners[type]()};
}
function fixture(source,options={}){
  const ids=['app-version-txt','app-version-txt-about','status-text','app-check-update','app-update-status','mac-update-btn','web-changelog-row','web-changelog-toggle','web-changelog-list'];
  const nodes=Object.fromEntries(ids.map(id=>[id,element()])),log=[],toasts=[],warnings=[],timers=[],events={},docEvents={},progress=[];
  const strings={updaterDownloadingPct:'downloading {pct}',updaterInstalledStatus:'installed {v}',updaterInstalledToast:'installed toast {v}',updaterInstallFailedToast:'install failed {e}',
    updaterCheckFailedStatus:'check failed {e}',updaterCheckFailedToast:'check toast {e}',updaterNewVersionMsg:'new {v}{body}',updaterNewVersionStatus:'new version {v}',updaterRestartConfirm:'restart {v}',updaterMacInstalledToast:'mac installed {v}'};
  const root={SM:{t:key=>strings[key]||key},NemoUpdaterPlatform:Platform};
  const document={readyState:options.loading?'loading':'complete',title:'Nemo old',body:element(),getElementById:id=>nodes[id]||null,createElement:()=>element(),addEventListener:(name,fn)=>docEvents[name]=fn};
  const update={version:'2.0.0',body:'Release body',async downloadAndInstall(callback){assert.equal(this,update);log.push('install');
    for(const event of [{event:'Started',data:{contentLength:100}},{event:'Progress',data:{chunkLength:25}},{event:'Progress',data:{chunkLength:300}},{event:'Finished',data:{}}]){callback(event);progress.push(nodes['app-update-status'].textContent);}
    if(options.installGate)await options.installGate.promise;if(options.installError)throw options.installError;
  }};
  let confirms=0,checks=0,fetches=0;
  if(!options.browser){
    root.__TAURI__={app:{async getVersion(){log.push('version');if(options.versionError)throw options.versionError;return '1.2.3';}},
      updater:{async check(){assert.equal(this,root.__TAURI__.updater);checks++;log.push('check');if(options.checkError)throw options.checkError;return options.noUpdate?null:update;}},
      dialog:{async confirm(message,settings){log.push(['confirm',message,settings]);const index=confirms++;if(options.confirmGate&&index===0)return options.confirmGate.promise;
        if(options.confirmError)throw options.confirmError;return (options.confirmations||[false])[index]||false;}},
      process:{async relaunch(){log.push('relaunch');if(options.relaunchError)throw options.relaunchError;}},
      event:{listen(name,handler){log.push(['listen',name]);events[name]=handler;return Promise.resolve(()=>{});}}};
    if(options.noProcess)delete root.__TAURI__.process;if(options.noUpdater)delete root.__TAURI__.updater;if(options.noDialog)delete root.__TAURI__.dialog;
  }
  const sandbox={window:root,document,navigator:{platform:options.mac?'MacIntel':'Linux'},SM:root.SM,console:{warn:(...args)=>warnings.push(args)},
    showToast:value=>toasts.push(value),setTimeout:(fn,delay)=>{timers.push({fn,delay});return timers.length;},
    fetch:async(url,args)=>{fetches++;log.push(['fetch',url,args]);if(options.fetchError)throw options.fetchError;return {ok:!options.fetchStatus,status:options.fetchStatus,json:async()=>options.commits||[{commit:{message:'<b>literal title</b>\nrest',author:{date:'2026-01-01T00:00:00Z'}}}]};}};
  vm.runInNewContext(source,sandbox,{filename:'updater-bridge.js'});
  return {root,nodes,document,log,toasts,warnings,timers,events,docEvents,progress,update,
    checks:()=>checks,fetches:()=>fetches,manual:()=>nodes['app-check-update'].fire('click'),macClick:()=>nodes['mac-update-btn'].fire('click'),settle:turn};
}
for(const [variant,source] of variants){
  test(variant+': version/status, menu event and exact4000ms startup remain independent of manual install',async()=>{
    const f=fixture(source);await f.settle();for(const id of ['app-version-txt','app-version-txt-about'])assert.equal(f.nodes[id].textContent,'1.2.3');
    assert.equal(f.document.title,'Nemo v1.2.3');assert.equal(f.nodes['status-text'].textContent,'Nemo v1.2.3');assert.deepEqual(f.timers.map(t=>t.delay),[4000]);assert.equal(f.checks(),0);
    assert.equal(typeof f.events['menu-check-update'],'function');f.events['menu-check-update']();await f.settle();assert.equal(f.checks(),1);assert.equal(f.log.includes('install'),false);
  });
  test(variant+': async manual false confirmation never installs, while pending button stays disabled',async()=>{
    const confirm=gate(),f=fixture(source,{confirmGate:confirm});f.manual();await f.settle();assert.equal(f.nodes['app-check-update'].disabled,true);assert.equal(f.log.includes('install'),false);
    confirm.resolve(false);await f.settle();assert.equal(f.nodes['app-check-update'].disabled,false);assert.equal(f.log.includes('install'),false);assert.equal(f.nodes['app-update-status'].textContent,'new version 2.0.0');
    assert.deepEqual(JSON.parse(JSON.stringify(f.log.find(x=>Array.isArray(x)&&x[0]==='confirm'))),['confirm','new 2.0.0\n\nRelease body',{title:'updaterAvailableDialogTitle'}]);
  });
  test(variant+': install emits bounded percentage and installed/restart-declined status',async()=>{
    const f=fixture(source,{confirmations:[true,false]});f.manual();await f.settle();assert.equal(f.log.filter(x=>x==='install').length,1);
    assert.ok(f.progress.includes('downloading 25'));assert.ok(f.progress.includes('downloading 100'));assert.equal(f.progress.at(-1),'updaterInstalling');
    assert.equal(f.nodes['app-update-status'].textContent,'installed 2.0.0');assert.ok(f.toasts.includes('updaterDownloading'));assert.ok(f.toasts.includes('installed toast 2.0.0'));assert.equal(f.log.includes('relaunch'),false);
  });
  test(variant+': accepted restart relaunches once; absent process skips restart confirm',async()=>{
    const f=fixture(source,{confirmations:[true,true]});f.manual();await f.settle();assert.equal(f.log.filter(x=>x==='relaunch').length,1);assert.equal(f.toasts.includes('installed toast 2.0.0'),false);
    const g=fixture(source,{confirmations:[true,true],noProcess:true});g.manual();await g.settle();assert.equal(g.log.filter(x=>Array.isArray(x)&&x[0]==='confirm').length,1);assert.ok(g.toasts.includes('installed toast 2.0.0'));
  });
  test(variant+': check, install, confirm and relaunch failures preserve distinct existing dispositions',async()=>{
    for(const [key,expected] of [['checkError','check failed bad'],['confirmError','check failed bad'],['installError','updaterInstallFailedStatus'],['relaunchError','updaterInstallFailedStatus']]){
      const f=fixture(source,{[key]:Error('bad'),confirmations:[true,true]});f.manual();await f.settle();assert.equal(f.nodes['app-update-status'].textContent,expected,key);assert.equal(f.nodes['app-check-update'].disabled,false);
      assert.equal(f.warnings.length,1);assert.ok(f.toasts.some(t=>t.includes('bad')));
    }
  });
  test(variant+': startup on nonMac remains silent for failure/up-to-date but still prompts for an update',async()=>{
    for(const options of [{noUpdate:true},{checkError:Error('offline')},{}]){
      const f=fixture(source,options);f.timers[0].fn();await f.settle();assert.equal(f.checks(),1);assert.equal(f.toasts.length,0);
      const confirmations=f.log.filter(x=>Array.isArray(x)&&x[0]==='confirm');assert.equal(confirmations.length,options.noUpdate||options.checkError?0:1);
      assert.equal(f.log.includes('install'),false);
    }
  });
  test(variant+': Mac startup populates button without prompt, explicit download installs and next click relaunches',async()=>{
    const f=fixture(source,{mac:true});assert.ok(f.document.body.classList.contains('mac-overlay-titlebar'));f.timers[0].fn();await f.settle();assert.equal(f.nodes['mac-update-btn'].className,'state-available');
    assert.equal(f.log.some(x=>Array.isArray(x)&&x[0]==='confirm'),false);f.macClick();await f.settle();assert.equal(f.nodes['mac-update-btn'].className,'state-installed');assert.ok(f.toasts.includes('mac installed 2.0.0'));
    f.macClick();await f.settle();assert.equal(f.log.filter(x=>x==='relaunch').length,1);assert.equal(f.nodes['app-update-status'].textContent,'');
  });
  test(variant+': Mac download spinner and failed install revert to available with raw error',async()=>{
    const hold=gate(),f=fixture(source,{mac:true,installGate:hold,installError:Error('signature mismatch')});f.timers[0].fn();await f.settle();f.macClick();await f.settle();assert.equal(f.nodes['mac-update-btn'].className,'state-downloading');
    hold.resolve();await f.settle();assert.equal(f.nodes['mac-update-btn'].className,'state-available');assert.ok(f.toasts.includes('install failed signature mismatch'));
  });
  test(variant+': Mac silent check failures warn without button, confirm or toast',async()=>{
    const f=fixture(source,{mac:true,checkError:Error('offline')});f.timers[0].fn();await f.settle();assert.equal(f.nodes['mac-update-btn'].className,'');assert.equal(f.toasts.length,0);assert.equal(f.warnings.length,1);
  });
  test(variant+': browser/manual unavailable and app-only version remain faithful without native checks',async()=>{
    const f=fixture(source,{browser:true});f.manual();await f.settle();assert.deepEqual(f.toasts,['updaterUnavailable']);assert.equal(f.timers.length,0);assert.equal(f.document.title,'Nemo old');assert.equal(f.fetches(),0);
    const g=fixture(source,{noUpdater:true});await g.settle();assert.equal(g.document.title,'Nemo v1.2.3');assert.equal(g.timers.length,0);g.manual();await g.settle();assert.ok(g.toasts.includes('updaterUnavailable'));
  });
  test(variant+': changelog lazily fetches once and renders untrusted commit title as text',async()=>{
    const f=fixture(source,{browser:true});assert.equal(f.nodes['web-changelog-row'].style.display,'flex');assert.equal(f.fetches(),0);await f.nodes['web-changelog-toggle'].fire('click');
    assert.equal(f.fetches(),1);assert.equal(f.nodes['web-changelog-list'].children[0].children[0].textContent,'<b>literal title</b>');
    await f.nodes['web-changelog-toggle'].fire('click');assert.equal(f.nodes['web-changelog-list'].style.display,'none');await f.nodes['web-changelog-toggle'].fire('click');assert.equal(f.fetches(),1);
    assert.equal(f.log.find(x=>Array.isArray(x)&&x[0]==='fetch')[1],'https://api.github.com/repos/mysteropodes/nemo/commits?sha=main&per_page=15');
  });
  test(variant+': failed changelog is retryable on re-open and desktop never fetches it',async()=>{
    const f=fixture(source,{browser:true,fetchStatus:503});await f.nodes['web-changelog-toggle'].fire('click');assert.equal(f.nodes['web-changelog-list'].children[0].textContent,'settingsWebChangelogFailed');assert.equal(f.warnings.length,1);
    await f.nodes['web-changelog-toggle'].fire('click');await f.nodes['web-changelog-toggle'].fire('click');assert.equal(f.fetches(),2);
    const g=fixture(source);assert.equal(g.nodes['web-changelog-toggle'].listeners.click,undefined);assert.equal(g.fetches(),0);
  });
  test(variant+': deferred DOM init and swallowed version failure preserve startup behavior',async()=>{
    const f=fixture(source,{loading:true,versionError:Error('version unavailable')});assert.equal(f.log.length,0);assert.equal(f.timers.length,0);f.docEvents.DOMContentLoaded();await f.settle();assert.equal(f.document.title,'Nemo old');assert.equal(f.timers[0].delay,4000);assert.equal(f.warnings.length,0);
  });
  test(variant+': missing dialog is a known check failure, never permission to install',async()=>{
    const f=fixture(source,{noDialog:true});f.manual();await f.settle();assert.equal(f.log.includes('install'),false);assert.ok(f.nodes['app-update-status'].textContent.startsWith('check failed '));assert.equal(f.warnings.length,1);
  });
}
test('injected platform forwards original handles/options/progress and exact returned promises',async()=>{
  const update={},callback=()=>{},options={title:'title'},promise=Promise.resolve('same'),calls=[];
  const ports=Object.fromEntries(['available','hasVersion','version','check','install','confirm','canRelaunch','relaunch','canListen','listen'].map(name=>[name,(...args)=>{calls.push([name,...args]);return promise;}]));
  const p=Platform.create(ports);for(const method of ['available','hasVersion','version','check','canRelaunch','relaunch','canListen'])assert.equal(p[method](),promise);
  assert.equal(p.install(update,callback),promise);assert.equal(p.confirm('message',options),promise);assert.equal(p.listen('menu-check-update',callback),promise);
  assert.strictEqual(calls.find(c=>c[0]==='install')[1],update);assert.strictEqual(calls.find(c=>c[0]==='install')[2],callback);assert.strictEqual(calls.find(c=>c[0]==='confirm')[2],options);assert.ok(Object.isFrozen(p));
});
test('desktop adapter reads current native capabilities lazily and preserves native method receivers',async()=>{
  const root={},p=Platform.desktop(root);assert.equal(p.available(),false);assert.equal(p.hasVersion(),false);
  root.__TAURI__={app:{getVersion(){assert.equal(this,root.__TAURI__.app);return 'version';}},updater:{check(){assert.equal(this,root.__TAURI__.updater);return 'check';}},dialog:{confirm(){assert.equal(this,root.__TAURI__.dialog);return false;}},process:{relaunch(){assert.equal(this,root.__TAURI__.process);return 'restart';}},event:{listen(){assert.equal(this,root.__TAURI__.event);return 'unlisten';}}};
  assert.ok(p.available());assert.equal(p.version(),'version');assert.equal(p.check(),'check');assert.equal(p.confirm('msg',{}),false);assert.equal(p.relaunch(),'restart');assert.equal(p.listen('event',()=>{}),'unlisten');
  delete root.__TAURI__.updater;assert.equal(p.available(),undefined);assert.equal(p.hasVersion(),root.__TAURI__.app);
});
if(variants.length===2)test('baseline/proposed observable traces match across explicit updater and lazy browser workflows',async()=>{
  const scenarios=[{options:{confirmations:[false]},action:async f=>{f.manual();await f.settle();}},
    {options:{confirmations:[true,false]},action:async f=>{f.manual();await f.settle();}},
    {options:{confirmations:[true,true],relaunchError:Error('restart refused')},action:async f=>{f.manual();await f.settle();}},
    {options:{mac:true},action:async f=>{f.timers[0].fn();await f.settle();f.macClick();await f.settle();f.macClick();await f.settle();}},
    {options:{browser:true},action:async f=>{await f.nodes['web-changelog-toggle'].fire('click');await f.nodes['web-changelog-toggle'].fire('click');await f.nodes['web-changelog-toggle'].fire('click');}},
    {options:{browser:true,fetchStatus:503},action:async f=>{await f.nodes['web-changelog-toggle'].fire('click');}},
    {options:{checkError:Error('offline')},action:async f=>{f.timers[0].fn();await f.settle();}}];
  function trace(f){return JSON.parse(JSON.stringify({log:f.log,toasts:f.toasts,warnings:f.warnings.map(w=>[w[0],w[1]&&w[1].message]),timers:f.timers.map(t=>t.delay),progress:f.progress,title:f.document.title,
    ui:Object.fromEntries(Object.entries(f.nodes).map(([id,n])=>[id,{text:n.textContent,html:n.innerHTML,disabled:n.disabled,title:n.title,className:n.className,display:n.style.display}]))}));}
  for(const scenario of scenarios){const proposed=fixture(variants[0][1],scenario.options),baseline=fixture(variants[1][1],scenario.options);await proposed.settle();await baseline.settle();await scenario.action(proposed);await scenario.action(baseline);assert.deepEqual(trace(proposed),trace(baseline));}
});
