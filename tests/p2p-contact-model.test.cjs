'use strict';
// Real model and UI sources; storage/crypto/clock/base64/DOM are local fake boundaries.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const Model=require('../src/js/adapters/p2p-contact-model.js');
const source=fs.readFileSync(path.join(__dirname,'../src/js/p2p-contacts.js'),'utf8');
const variants=[['proposed',source]];
if(process.env.NEMO_P2P_BASELINE)variants.push(['baseline',fs.readFileSync(process.env.NEMO_P2P_BASELINE,'utf8')]);
const invite=(k,n='Friend',t=-999999999)=>btoa(JSON.stringify({k,n,t}));
function element(){
  const listeners={};let html='',text='';const children=[];
  return {listeners,style:{},children,value:'',className:'',title:'',
    get textContent(){return text;},set textContent(v){text=v;children.length=0;},get innerHTML(){return html;},set innerHTML(v){html=v;children.length=0;},
    appendChild:v=>children.push(v),addEventListener:(type,fn)=>listeners[type]=fn,
    fire:(type,event={})=>listeners[type]&&listeners[type](event),select(){this.selected=true;}};
}
function fixture(script,options={}){
  const values=new Map(Object.entries(options.storage||{})),log=[],toasts=[],docEvents={},state={userProfile:{name:options.name===undefined?'Alice':options.name}};
  const nodes=Object.fromEntries(['p2p-contacts-list','p2p-my-code','p2p-copy-code','p2p-add-contact','p2p-invite-input'].map(id=>[id,element()]));
  const storage={getItem(key){log.push(['get',key]);if(options.readError)throw Error('storage read failed');return values.get(key)||null;},setItem(key,value){log.push(['set',key,value]);if(options.writeError)throw Error('storage write failed');values.set(key,value);}};
  const crypto={getRandomValues(array){log.push(['random',array.length]);if(options.cryptoError)throw Error('crypto unavailable');for(let i=0;i<array.length;i++)array[i]=i;return array;}};
  let clock=1000;const document={readyState:options.loading?'loading':'complete',getElementById:id=>nodes[id]||null,createElement:()=>element(),
    addEventListener:(type,fn)=>docEvents[type]=fn,execCommand:command=>log.push(['exec',command])};
  const root={NemoP2PContactModel:Model,SM:{t:key=>key},showToast:value=>toasts.push(value)};
  const navigator=options.legacyClipboard?{}:{clipboard:{writeText:value=>{log.push(['clipboard',value]);return Promise.resolve();}}};
  const sandbox={window:root,state,document,localStorage:storage,crypto,btoa,atob,Date:{now:()=>++clock},navigator,SM:root.SM,showToast:root.showToast};
  if(options.storageGetterError)Object.defineProperty(sandbox,'localStorage',{get(){throw Error('storage blocked');}});
  if(options.noCrypto)delete sandbox.crypto;
  let error;try{vm.runInNewContext(script,sandbox,{filename:'p2p-contacts.js'});}catch(e){error=e;}
  return {root,state,nodes,values,log,toasts,docEvents,error,storage,crypto,
    add:code=>root.SMP2P.addContact(code),remove:key=>root.SMP2P.removeContact(key),myInvite:()=>root.SMP2P.myInviteCode()};
}
const plain=value=>JSON.parse(JSON.stringify(value));
for(const [variant,script] of variants){
  test(variant+': identity and contact storage use original keys/order; exact public API and invite survive reload',()=>{
    const f=fixture(script);assert.equal(f.error,undefined);assert.deepEqual(Object.keys(f.root.SMP2P).sort(),['addContact','myInviteCode','removeContact']);
    assert.deepEqual(f.log.slice(0,3),[['get','nemo-p2p-contacts'],['get','nemo-p2p-identity'],['random',16]]);
    assert.equal(f.state.p2pIdentity.publicKey,'000102030405060708090a0b0c0d0e0f');assert.equal(f.state.p2pIdentity.createdAt,1001);
    const code=JSON.parse(atob(f.nodes['p2p-my-code'].value));assert.deepEqual(code,{k:f.state.p2pIdentity.publicKey,n:'Alice',t:1002});
    const reload=fixture(script,{storage:Object.fromEntries(f.values)});assert.equal(reload.state.p2pIdentity.publicKey,f.state.p2pIdentity.publicKey);assert.equal(reload.log.some(x=>x[0]==='random'),false);
  });
  test(variant+': invalid/self/duplicate invitations keep state/storage stable; accepted old invite stays pending',()=>{
    const f=fixture(script),before=f.log.filter(x=>x[0]==='set').length;
    assert.deepEqual(plain(f.add('not base64')), {ok:false,error:'Code invalide'});assert.deepEqual(plain(f.add(invite(f.state.p2pIdentity.publicKey))),{ok:false,error:'C’est ton propre code'});
    const accepted=f.add(' \n'+invite('remote','Friend',0)+'\t ');assert.equal(accepted.ok,true);assert.equal(accepted.contact.status,'pending');assert.equal(accepted.contact.pairedAt,1003);
    assert.deepEqual(plain(f.add(invite('remote','Other'))),{ok:false,error:'Déjà dans tes contacts'});assert.equal(f.state.p2pContacts.length,1);
    assert.equal(f.log.filter(x=>x[0]==='set').length,before+1);assert.equal(f.nodes['p2p-contacts-list'].children[0].children[0].title,'p2pWaiting');
    f.myInvite();f.myInvite();assert.equal(f.state.p2pContacts[0].status,'pending');
  });
  test(variant+': add click/Enter and removal render through existing UI without changing public returns',()=>{
    const f=fixture(script),input=f.nodes['p2p-invite-input'];input.value=invite('remote','<b>name</b>');f.nodes['p2p-add-contact'].fire('click');assert.equal(input.value,'');
    const row=f.nodes['p2p-contacts-list'].children[0];assert.equal(row.children[1].textContent,'<b>name</b>');assert.equal(row.children[2].textContent,'remote…');assert.equal(f.toasts.at(-1),'toastContactAdded<b>name</b>');
    input.value='invalid';input.fire('keydown',{key:'Enter'});assert.equal(input.value,'invalid');assert.equal(f.toasts.at(-1),'Code invalide');
    row.children[3].fire('click');assert.equal(f.state.p2pContacts.length,0);assert.equal(f.nodes['p2p-contacts-list'].children[0].textContent,'p2pNoContacts');assert.equal(f.remove('missing'),undefined);
  });
  test(variant+': blank add and nonEnter keys send no write/toast; clipboard and legacy copy stay local',()=>{
    const f=fixture(script);const before=f.log.length;f.nodes['p2p-invite-input'].value='  ';f.nodes['p2p-add-contact'].fire('click');f.nodes['p2p-invite-input'].fire('keydown',{key:'Escape'});assert.equal(f.log.length,before);assert.equal(f.toasts.length,0);
    f.nodes['p2p-copy-code'].fire('click');assert.equal(f.nodes['p2p-my-code'].selected,true);assert.deepEqual(f.log.at(-1),['clipboard',f.nodes['p2p-my-code'].value]);assert.equal(f.toasts.at(-1),'toastCodeCopied');
    const g=fixture(script,{legacyClipboard:true});g.nodes['p2p-copy-code'].fire('click');assert.deepEqual(g.log.at(-1),['exec','copy']);
  });
  test(variant+': persisted connected status remains legacy UI truth, not an observed live handshake',()=>{
    const f=fixture(script,{storage:{'nemo-p2p-contacts':JSON.stringify([{publicKey:'old-key',displayName:'Old',status:'connected',pairedAt:7}])}});
    assert.equal(f.state.p2pContacts[0].status,'connected');const dot=f.nodes['p2p-contacts-list'].children[0].children[0];assert.equal(dot.title,'p2pConnected');assert.ok(dot.style.cssText.includes('#4caf50'));
    f.add(invite('new'));assert.equal(f.state.p2pContacts[1].status,'pending');assert.equal(f.state.p2pContacts[0].status,'connected');
  });
  test(variant+': malformed JSON and storage read/write failures preserve original fallbacks and in-memory mutations',()=>{
    for(const options of [{storage:{'nemo-p2p-identity':'{','nemo-p2p-contacts':'{'}},{readError:true},{writeError:true}]){
      const f=fixture(script,options);assert.equal(f.error,undefined);assert.equal(f.state.p2pContacts.length,0);assert.equal(f.state.p2pIdentity.publicKey.length,32);assert.equal(f.add(invite('remote')).ok,true);assert.equal(f.state.p2pContacts[0].status,'pending');
    }
  });
  test(variant+': malformed stored null is not silently repaired; object-shaped contacts retain later model failure',()=>{
    const f=fixture(script,{storage:{'nemo-p2p-contacts':'null'}});assert.ok(f.error);assert.equal(f.state.p2pContacts,null);assert.equal(f.root.SMP2P,undefined);
    const g=fixture(script,{storage:{'nemo-p2p-contacts':'{}'}});assert.equal(g.error,undefined);assert.throws(()=>g.add(invite('remote')),/some is not a function/);assert.deepEqual(plain(g.state.p2pContacts),{});
  });
  test(variant+': Latin1 names encode while nonLatin1 name errors remain visible and crypto failures abort initialization',()=>{
    const f=fixture(script,{name:'Élodie'});assert.equal(JSON.parse(atob(f.nodes['p2p-my-code'].value)).n,'Élodie');
    const g=fixture(script,{name:'雪'});assert.ok(g.error);assert.equal(g.error.name,'InvalidCharacterError');assert.equal(g.root.SMP2P,undefined);assert.equal(g.state.p2pIdentity.publicKey.length,32);
    const h=fixture(script,{cryptoError:true});assert.ok(h.error);assert.equal(h.error.message,'crypto unavailable');assert.equal(h.root.SMP2P,undefined);
  });
  test(variant+': a throwing storage property getter remains inside original read/write guards',()=>{
    const f=fixture(script,{storageGetterError:true});assert.equal(f.error,undefined);
    assert.equal(f.state.p2pContacts.length,0);assert.equal(f.state.p2pIdentity.publicKey.length,32);
    assert.equal(f.add(invite('remote')).ok,true);assert.equal(f.state.p2pContacts[0].status,'pending');
  });
  test(variant+': an existing identity does not resolve an unavailable crypto global',()=>{
    const f=fixture(script,{noCrypto:true,storage:{'nemo-p2p-identity':JSON.stringify({publicKey:'saved'})}});
    assert.equal(f.error,undefined);assert.equal(f.state.p2pIdentity.publicKey,'saved');
    assert.equal(f.log.some(x=>x[0]==='random'),false);
  });
  test(variant+': deferred DOM initialization preserves early identity/model and attaches original public API',()=>{
    const f=fixture(script,{loading:true,name:''});assert.equal(f.nodes['p2p-my-code'].value,'');assert.ok(f.state.p2pIdentity);assert.equal(typeof f.root.SMP2P.addContact,'function');
    f.docEvents.DOMContentLoaded();assert.equal(JSON.parse(atob(f.nodes['p2p-my-code'].value)).n,'Animateur');assert.equal(f.nodes['p2p-contacts-list'].children[0].textContent,'p2pNoContacts');
  });
}
function modelFixture(options={}){
  const state={userProfile:{name:'Alice'}},stored=new Map(Object.entries(options.storage||{})),calls=[];let time=0;
  const storage={getItem:k=>{calls.push(['get',k]);return stored.get(k)||null;},setItem:(k,v)=>{calls.push(['set',k,v]);stored.set(k,v);}};
  const model=Model.create({state,storage,crypto:{getRandomValues:a=>{calls.push(['random',a.length]);a.fill(0xab);return a;}},now:()=>++time,encodeBase64:btoa,decodeBase64:atob});
  return {model,state,stored,calls};
}
test('actual injected model retains original shared state/array identity and filter replacement semantics',()=>{
  const f=modelFixture(),original=f.state.p2pContacts;assert.strictEqual(f.model.contacts(),original);assert.equal(f.model.addContact(invite('remote')).ok,true);assert.strictEqual(f.model.contacts(),original);
  assert.equal(original[0].pairedAt,2);f.model.removeContact('remote');assert.notStrictEqual(f.model.contacts(),original);assert.equal(original.length,1);assert.equal(f.state.p2pContacts.length,0);assert.ok(Object.isFrozen(f.model));
});
test('actual model keeps permissive original invite parsing, opaque saved identity and ignored timestamp',()=>{
  const f=modelFixture({storage:{'nemo-p2p-identity':JSON.stringify({publicKey:42,extra:'keep'})}});assert.deepEqual(f.state.p2pIdentity,{publicKey:42,extra:'keep'});assert.equal(f.calls.some(c=>c[0]==='random'),false);
  assert.deepEqual(f.model.parseInviteCode(invite('old','',-1)),{publicKey:'old',displayName:'Contact'});assert.equal(f.model.parseInviteCode(null),null);assert.equal(f.model.parseInviteCode(btoa('{}')),null);
  assert.deepEqual(f.model.addContact(invite(42)),{ok:false,error:'C’est ton propre code'});const result=f.model.addContact(invite(123,'number key'));assert.equal(result.ok,true);assert.equal(result.contact.publicKey,123);assert.equal(result.contact.status,'pending');
});
if(variants.length===2)test('baseline/proposed UI/state/storage/copy traces remain equal across normal and malformed fixtures',()=>{
  function trace(f){return plain({state:f.state,storage:Object.fromEntries(f.values),log:f.log,toasts:f.toasts,error:f.error&&{name:f.error.name,message:f.error.message},
    code:f.nodes['p2p-my-code'].value,publicAPI:f.root.SMP2P&&Object.keys(f.root.SMP2P),rows:f.nodes['p2p-contacts-list'].children.map(row=>({text:row.textContent,children:row.children.map(c=>({text:c.textContent,title:c.title,style:c.style.cssText}))}))});}
  for(const options of [{},{writeError:true},{name:'Élodie'},{name:'雪'},{storage:{'nemo-p2p-contacts':'null'}},{storage:{'nemo-p2p-contacts':JSON.stringify([{publicKey:'saved',displayName:'Legacy',status:'connected'}])}}]){
    const proposed=fixture(variants[0][1],options),baseline=fixture(variants[1][1],options);
    if(!proposed.error){for(const f of [proposed,baseline]){f.add(invite('remote','Name'));f.nodes['p2p-copy-code'].fire('click');f.remove('remote');}}
    assert.deepEqual(trace(proposed),trace(baseline));
  }
});
