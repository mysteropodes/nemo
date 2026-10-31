'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto');
const Runtime=require('../src/js/adapters/i18n-runtime.js');
const dataPath=path.resolve(__dirname,'../src/js/i18n.js');
const source=()=>fs.readFileSync(dataPath,'utf8');
function dictionarySource(){const s=source(),a=s.indexOf('  var I18N='),b=s.indexOf('\n  };',a);
  assert.ok(a>=0&&b>a);return s.slice(a,b+'\n  };'.length);}
function dictionaries(){return vm.runInNewContext(dictionarySource()+';I18N');}
const small={en:{hello:'Hello',onlyEnglish:'English fallback',markup:'<b>Hello</b>'},fr:{hello:'Bonjour',markup:'<b>Bonjour</b>'},
  ja:{hello:'こんにちは',markup:'<b>こんにちは</b>'},es:{hello:'Hola',markup:'<b>Hola</b>'}};
function node(attributes={}){return {attributes,textContent:'',title:'',placeholder:'',innerHTML:'',value:'',listeners:new Map(),
  getAttribute(k){return this.attributes[k]||null;},addEventListener(k,fn){this.listeners.set(k,fn);}};}
function fixture(options={}){
  const selector=node(),text=node({'data-i18n':'hello'}),title=node({'data-i18n-title':'hello'}),
    placeholder=node({'data-i18n-placeholder':'hello'}),html=node({'data-i18n-html':'markup'}),label=node(),explicit=node({'data-i18n-title':'hello'});
  label.textContent='  A property  ';const listeners=new Map(),writes=[],reads=[];
  const doc={readyState:options.loading===false?'complete':'loading',
    querySelectorAll(s){return ({'[data-i18n]':[text],'[data-i18n-title]':[title,explicit],
      '[data-i18n-placeholder]':[placeholder],'[data-i18n-html]':[html],'.pl':[label,explicit]})[s]||[];},
    getElementById(id){return id==='settings-language'?selector:null;},addEventListener(k,fn){listeners.set(k,fn);}};
  const callbacks=[],root={state:options.state||{},SM:{retained:true,afterI18n:callbacks}},native=Object.freeze({revision:7});
  root.nativeDocument=native;
  for(const key of ['NemoNativeObjectProject','NemoNativeObjectProjectFile','__TAURI__'])
    Object.defineProperty(root,key,{get(){throw Error('localization resolved native authority');}});
  const storage={getItem(key){reads.push(key);if(options.deniedGet)throw Error('denied');return options.saved||null;},
    setItem(key,value){if(options.deniedSet)throw Error('denied');writes.push([key,value]);}};
  const ports={root,document:doc,state:()=>root.state,storage:()=>storage};
  return {root,doc,ports,storage,writes,reads,callbacks,selector,text,title,placeholder,html,label,explicit,native,
    ready(){doc.readyState='complete';const fn=listeners.get('DOMContentLoaded');assert.equal(typeof fn,'function');fn();},
    install(table=small){return Runtime.install(ports,table);}};
}
test('all four retained dictionaries preserve exact bytes, key counts and fixed translations',()=>{
  const digest=crypto.createHash('sha256').update(dictionarySource()).digest('hex');
  assert.equal(digest,'83bb534e68165257280e3a8cd48c6c0e9468ddb7f29e5576127465b5a1f2aa0d');
  const data=dictionaries();assert.deepEqual(Object.keys(data),['en','fr','ja','es']);
  for(const [locale,title] of Object.entries({en:'Settings',fr:'Réglages',ja:'設定',es:'Ajustes'})){
    assert.equal(Object.keys(data[locale]).length,2215);assert.equal(data[locale].settingsTitle,title);
  }
});
test('data loader calls the explicit runtime once with live state/storage ports',()=>{
  const h=fixture(),calls=[],window=h.root;
  window.NemoI18nRuntime={install(ports,table){calls.push({ports,table});}};
  const context={window,document:h.doc,localStorage:h.storage};
  Object.defineProperty(context,'state',{get(){return window.state;}});
  vm.runInNewContext(source(),context,{filename:'src/js/i18n.js'});
  assert.equal(calls.length,1);const {ports,table}=calls[0];
  assert.equal(ports.root,window);assert.equal(ports.document,h.doc);assert.equal(ports.storage(),h.storage);
  window.state={language:'ja'};assert.equal(ports.state(),window.state);assert.equal(table.ja.settingsTitle,'設定');
  assert.equal(h.reads.length,0,'data loading does not initialize or rewrite preferences itself');
});
test('loading order exposes the existing namespace before deferred DOM initialization',()=>{
  const h=fixture(),array=h.callbacks;h.install();
  assert.equal(h.root.SM.retained,true);assert.equal(h.root.SM.afterI18n,array);
  assert.equal(h.root.SM.t('hello'),'Bonjour','pre-init default remains French');
  assert.equal(h.text.textContent,'');assert.equal(h.reads.length,0);
  h.ready();assert.equal(h.root.state.language,'en');assert.equal(h.text.textContent,'Hello');
  assert.deepEqual(h.reads,['nemo-lang']);assert.equal(h.writes.length,0);
});
for(const locale of ['en','fr','ja','es'])test('mounted values and saved '+locale+' remain consistent',()=>{
  const h=fixture({saved:locale,state:{language:'en'}});h.install();h.ready();
  const expected=small[locale].hello;assert.equal(h.root.state.language,locale);
  assert.equal(h.text.textContent,expected);assert.equal(h.title.title,expected);assert.equal(h.placeholder.placeholder,expected);
  assert.equal(h.html.innerHTML,small[locale].markup);assert.equal(h.selector.value,locale);
  assert.equal(h.label.title,'A property');assert.equal(h.explicit.title,expected);
  assert.equal(h.root.nativeDocument,h.native);assert.equal(h.native.revision,7);
});
test('existing selector event changes language and refreshes dynamic settings/Labs/help callbacks',()=>{
  const h=fixture(),dynamic={settings:'',labs:'',help:''},calls=[];h.install();
  h.callbacks.push(()=>{dynamic.settings=h.root.SM.t('hello');calls.push('settings');},()=>{throw Error('isolated callback');},
    ()=>{dynamic.labs=h.root.SM.t('hello');calls.push('labs');},()=>{dynamic.help=h.root.SM.t('hello');calls.push('help');});
  h.ready();h.selector.value='fr';h.selector.listeners.get('change').call(h.selector);
  assert.deepEqual(dynamic,{settings:'Bonjour',labs:'Bonjour',help:'Bonjour'});
  assert.deepEqual(calls,['settings','labs','help','settings','labs','help']);
  assert.deepEqual(h.writes,[['nemo-lang','fr']]);assert.equal(h.text.textContent,'Bonjour');
});
test('unsupported language does not change preferences or repaint mounted values',()=>{
  const h=fixture();h.install();h.ready();let calls=0;h.callbacks.push(()=>calls++);
  assert.equal(h.root.SM.setLanguage('xx'),undefined);assert.equal(h.root.state.language,'en');
  assert.equal(h.text.textContent,'Hello');assert.equal(h.writes.length,0);assert.equal(calls,0);
});
test('missing-key and unsupported-current-language fallbacks retain the existing lookup contract',()=>{
  const h=fixture();h.install();h.ready();h.root.SM.setLanguage('ja');
  assert.equal(h.root.SM.t('onlyEnglish'),'English fallback');assert.equal(h.root.SM.t('unknown'),'unknown');
  h.root.state.language='xx';assert.equal(h.root.SM.t('hello'),'Bonjour');
});
for(const denied of ['deniedGet','deniedSet'])test(denied+' does not disable current language presentation',()=>{
  const h=fixture({[denied]:true,state:{language:'ja'}});h.install();h.ready();
  assert.equal(h.root.state.language,'ja');h.root.SM.setLanguage('es');
  assert.equal(h.text.textContent,'Hola');assert.equal(h.selector.value,'es');assert.equal(h.native.revision,7);
});
test('a throwing storage accessor is isolated, not replaced by project persistence',()=>{
  const h=fixture();h.ports.storage=()=>{throw Error('storage access denied');};h.install();h.ready();
  assert.equal(h.root.state.language,'en');h.root.SM.setLanguage('fr');assert.equal(h.text.textContent,'Bonjour');
});
test('state port is read at use time rather than retaining an obsolete UI state object',()=>{
  const h=fixture();h.install();h.ready();const previous=h.root.state;
  h.root.state={language:'es'};h.root.SM.applyI18n();assert.equal(h.text.textContent,'Hola');
  h.root.SM.setLanguage('ja');assert.equal(h.root.state.language,'ja');assert.equal(previous.language,'en');
});
test('already mounted documents initialize immediately and malformed saved choice retains default policy',()=>{
  const h=fixture({loading:false,saved:'invalid'});h.install();
  assert.equal(h.root.state.language,'en');assert.equal(h.text.textContent,'Hello');assert.equal(h.selector.listeners.size,1);
});
test('runtime never fabricates missing editor state or resolves a document owner',()=>{
  const h=fixture();delete h.root.state;h.install();assert.equal(h.root.SM.t('hello'),'Bonjour');
  assert.throws(()=>h.root.SM.setLanguage('en'));assert.equal(h.root.state,undefined);assert.equal(h.root.nativeDocument,h.native);
});
test('production data and actual runtime keep all four language lookups callable',()=>{
  const h=fixture();h.root.NemoI18nRuntime=Runtime;
  const context={window:h.root,document:h.doc,localStorage:h.storage};
  Object.defineProperty(context,'state',{get(){return h.root.state;}});
  vm.runInNewContext(source(),context,{filename:'src/js/i18n.js'});h.ready();
  for(const [locale,value] of Object.entries({en:'Settings',fr:'Réglages',ja:'設定',es:'Ajustes'})){
    h.root.SM.setLanguage(locale);assert.equal(h.root.SM.t('settingsTitle'),value);
    assert.equal(h.root.SM.t('not-an-existing-key'),'not-an-existing-key');
  }
});
