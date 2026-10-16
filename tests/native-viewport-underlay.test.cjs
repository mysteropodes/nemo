'use strict';
// Independent DOM oracle for native presentation ownership and geometry.
const test=require('node:test'),assert=require('node:assert/strict');
const {create}=require('../src/js/adapters/native-viewport-underlay.js');
const CLASS='nemo-native-underlay',KEYS=['left','top','right','bottom'].map(x=>'--nemo-native-'+x);
const A=Object.freeze({instanceId:'instance-A',documentId:'document-A',lifecycleGeneration:1});
const B=Object.freeze({instanceId:'instance-A',documentId:'document-B',lifecycleGeneration:2});
function fixture(initial={}){
 const classes=new Set(initial.classes||[]),styles=new Map(Object.entries(initial.styles||{})),priorities=new Map(Object.entries(initial.priorities||{}));
 const classList={add(...xs){xs.forEach(x=>classes.add(x));},remove(...xs){xs.forEach(x=>classes.delete(x));},contains:x=>classes.has(x),toggle(x,on){if(on===undefined)on=!classes.has(x);on?classes.add(x):classes.delete(x);return on;}};
 const style={setProperty(k,v,p=''){styles.set(k,String(v));priorities.set(k,p);},getPropertyValue:k=>styles.get(k)||'',removeProperty(k){const old=styles.get(k)||'';styles.delete(k);priorities.delete(k);return old;},getPropertyPriority:k=>priorities.get(k)||''};
 const document={documentElement:{classList,style,clientWidth:1280,clientHeight:900}};
 let rect={left:40,top:60,right:1000,bottom:700,width:960,height:640};
 const api=create({document,measure:()=>rect});
 return {api,document,classes,styles,priorities,set rect(v){rect=v;},get rect(){return rect;}};
}
function opaque(f){assert.equal(f.classes.has(CLASS),false);for(const k of KEYS)assert.equal(f.styles.has(k),false);}
function edges(f,values){assert.equal(f.classes.has(CLASS),true);assert.deepEqual(KEYS.map(k=>f.styles.get(k)),values.map(v=>v+'px'));}
test('construction leaves ordinary document opaque and does not invent a presentation',()=>{const f=fixture();opaque(f);f.api.refresh(A);opaque(f);});
test('valid correlated owner presentation enables only the measured canvas hole',()=>{const f=fixture();f.api.present(A);edges(f,[40,60,1000,700]);});
test('same-owner resize refresh uses fresh measured edges',()=>{const f=fixture();f.api.present(A);f.rect={left:72.5,top:103.25,right:1150.75,bottom:801,width:1078.25,height:697.75};f.api.refresh({...A});edges(f,[72.5,103.25,1150.75,801]);});
test('stale owner cannot refresh or clear a newer document',()=>{const f=fixture();f.api.present(A);f.rect={left:10,top:20,right:800,bottom:600,width:790,height:580};f.api.present(B);edges(f,[10,20,800,600]);f.rect={left:90,top:90,right:900,bottom:800,width:810,height:710};f.api.refresh(A);f.api.clear(A);edges(f,[10,20,800,600]);f.api.clear({...B});opaque(f);});
test('instance and lifecycle generation are both part of owner identity',()=>{for(const changed of[{...A,instanceId:'other'},{...A,lifecycleGeneration:2}]){const f=fixture();f.api.present(A);f.api.clear(changed);edges(f,[40,60,1000,700]);f.api.refresh(changed);edges(f,[40,60,1000,700]);}});
for(const [name,rect]of Object.entries({missing:null,nan:{left:NaN,top:0,right:10,bottom:10,width:10,height:10},infinite:{left:0,top:0,right:Infinity,bottom:10,width:Infinity,height:10},zeroWidth:{left:0,top:0,right:0,bottom:10,width:0,height:10},zeroHeight:{left:0,top:0,right:10,bottom:0,width:10,height:0},negativeWidth:{left:10,top:0,right:0,bottom:10,width:-10,height:10}})){
 test('invalid '+name+' measurement refuses and removes prior transparent hole',()=>{const f=fixture();f.api.present(A);f.rect=rect;assert.throws(()=>f.api.refresh(A));opaque(f);assert.throws(()=>f.api.present(B));opaque(f);});
}
test('clear without owner restores ordinary opacity and preserves unrelated styling',()=>{const f=fixture({classes:['unrelated'],styles:{'--unrelated':'yes'}});f.api.present(A);f.api.clear();opaque(f);assert.ok(f.classes.has('unrelated'));assert.equal(f.styles.get('--unrelated'),'yes');});
test('clear restores preexisting managed property values and priorities',()=>{const initial=Object.fromEntries(KEYS.map((k,i)=>[k,(i+1)+'px'])),priorities=Object.fromEntries(KEYS.map(k=>[k,'important'])),f=fixture({styles:initial,priorities});f.api.present(A);f.api.clear(A);assert.equal(f.classes.has(CLASS),false);assert.deepEqual(Object.fromEntries(f.styles),initial);for(const k of KEYS)assert.equal(f.priorities.get(k),'important');});
test('dispose removes owned hole and later presentation cannot re-enable it',()=>{const f=fixture();f.api.present(A);f.api.dispose();opaque(f);assert.throws(()=>f.api.present(B));opaque(f);f.api.dispose();opaque(f);});
test('clipped measured rectangle uses viewport intersection as absolute CSS edges',()=>{const f=fixture();f.rect={left:-20,top:-10,width:1500,height:1000};f.api.present(A);edges(f,[0,0,1280,900]);f.document.documentElement.clientWidth=800;f.document.documentElement.clientHeight=600;f.api.refresh(A);edges(f,[0,0,800,600]);});
test('positive rectangle outside viewport refuses and clears prior hole',()=>{const f=fixture();f.api.present(A);f.rect={left:1280,top:0,width:10,height:20};assert.throws(()=>f.api.refresh(A));opaque(f);});
test('invalid owner cannot activate a new hole',()=>{for(const owner of[null,{...A,instanceId:''},{...A,documentId:'x'.repeat(129)},{...A,lifecycleGeneration:0},{...A,lifecycleGeneration:1.5}]){const f=fixture();assert.throws(()=>f.api.present(owner));opaque(f);}});
test('preexisting active class refuses second ownership',()=>{assert.throws(()=>fixture({classes:[CLASS]}));});
