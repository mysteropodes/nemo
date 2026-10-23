// Promise is installed for internal async classes and job detection, then
// hidden from guest globals along with constructors and host-like capabilities.
pub(super) const HARDEN: &str = r#"(() => {
'use strict';
const internalPromise = Promise;
const fnProtos = [Function.prototype,
 Object.getPrototypeOf(function*(){}), Object.getPrototypeOf(async function(){}),
 Object.getPrototypeOf(async function*(){})];
for (const p of [...fnProtos, internalPromise.prototype])
 Object.defineProperty(p, 'constructor', {value:undefined,writable:false,configurable:false});
Object.defineProperty(Math, 'random', {value:undefined,writable:false,configurable:false});
if (typeof Array.fromAsync !== 'undefined')
 Object.defineProperty(Array, 'fromAsync', {value:undefined,writable:false,configurable:false});
for (const n of ['eval','Function','Date','performance','Promise','SharedArrayBuffer','Atomics',
 'WebAssembly','WeakRef','FinalizationRegistry','Proxy','window','document','process','require',
 'fetch','XMLHttpRequest','WebSocket','console','setTimeout','queueMicrotask','Deno','Bun'])
 Object.defineProperty(globalThis,n,{value:undefined,writable:false,configurable:false});
const seen=[];
function freeze(o) {
 if(o===null || (typeof o!=='object' && typeof o!=='function') || seen.indexOf(o)>=0) return;
 seen.push(o);
 for(const k of Object.getOwnPropertyNames(o)) {
  const d=Object.getOwnPropertyDescriptor(o,k);
  if(d && 'value' in d) freeze(d.value);
  if(d && d.get) freeze(d.get); if(d && d.set) freeze(d.set);
 }
 const p=Object.getPrototypeOf(o); if(p) freeze(p);
 Object.freeze(o);
}
for(const p of fnProtos) freeze(p);
freeze(internalPromise);
freeze(globalThis);
return 1;
})()"#;
