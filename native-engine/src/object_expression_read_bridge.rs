//! Fixed guest vocabulary. All record values come from SnapshotReads; this
//! prelude carries no document state or writable application capability.
pub(super) const PRELUDE: &str = r#"
const _get = function(op, ref, prop, at) {
 return JSON.parse(__read(op, ref || '', prop || '', at === undefined ? frame : at));
};
const _num = function(v) { v = Number(v); return isFinite(v) ? v : 0; };
const _shape = function(a) { return a.length === 1 ? a[0] : a; };
const _at = function(f) { return _shape(_get('raw', '', '', f === undefined ? frame : _num(f))); };
const _comp = function() { return _get('comp'); };
const _fps = function() { return _comp().fps; };
const _velocity = function(f) {
 f = f === undefined ? frame : _num(f);
 const a = _get('raw', '', '', f - 0.5), b = _get('raw', '', '', f + 0.5), fps = _fps();
 return _shape(b.map(function(v,i) { return (v - (a[i] || 0)) * fps; }));
};
const _speed = function(f) {
 const v = _velocity(f);
 return typeof v === 'number' ? Math.abs(v) : Math.sqrt(v.reduce(function(s,n) { return s+n*n; },0));
};
const _sample = function(uid,p,f) { return _shape(_get('value',uid,p,_num(f))); };
const _control = function(uid,n,f) {
 const p = _get('control',uid,String(n));
 return _sample(uid || _get('owner'),p,f === undefined ? frame : _num(f));
};
const control = function(n,f) { return _control('',n,f); };
const layerControl = function(ref,n,f) {
 const uid = _get('layer-uid',String(ref));
 if (uid === null) throw new Error('Unknown layer: '+String(ref));
 return _control(uid,n,f);
};
const _layer = function(ref) {
 const uid = _get('layer',String(ref));
 if (uid === null) return null;
 const info = _get('info',uid), out = {};
 for (const p of ['name','index','hasParent']) out[p] = info[p];
 for (const p of ['position','anchor','rotation','scale','opacity']) {
  const v = _sample(uid,p,frame);
  if (Array.isArray(v)) Object.defineProperty(v,'at',{
   value:function(f) { return _sample(uid,p,_num(f)); },writable:true,configurable:true
  });
  out[p] = v;
 }
 out.at = function(p,f) { return _sample(uid,String(p),_num(f)); };
 out.control = function(n,f) { return _control(uid,n,f); };
 // A layer snapshot's parent is an identity, not another eager layer snapshot.
 out.parent = _get('parent',uid);
 return out;
};
const layer = _layer;
const _keys = {
 at: function(i) { return _get('key','','',Math.floor(_num(i))); },
 nearest: function(f) { return _get('nearest-key','','',_num(f)); }
};
Object.defineProperty(_keys,'count',{get:function() { return _get('key-count'); }});
const self = { at:_at, velocity:_velocity, speed:_speed, keys:_keys, control:control };
for (const p of ['property','name','index','isShape','hasParent'])
 Object.defineProperty(self,p,{get:function() { const i=_get('info'); return i === null ? null : i[p]; }});
Object.defineProperty(self,'parent',{get:function() { const i=_get('info'); return i && i.parent ? _layer(i.parent.name) : null; }});
const comp = {};
for (const p of ['width','height','fps','frames','layers','name'])
 Object.defineProperty(comp,p,{get:function() { return _comp()[p]; }});
const thisComp = {};
for (const p of ['width','height','frameDuration','duration','numLayers','name'])
 Object.defineProperty(thisComp,p,{get:function() { return _comp()[p]; }});
thisComp.layer = layer;
const key = _keys.at;
const nearestKey = function(t) { return _get('nearest-key','','',_num(t)*_fps()); };
const valueAtFrame = _at;
const valueAtTime = function(t) { return _at(_num(t)*_fps()); };
const velocityAtTime = function(t) { return _velocity(_num(t)*_fps()); };
const speedAtTime = function(t) { return _speed(_num(t)*_fps()); };
const timeToFrames = function(t,fps) { return (t === undefined ? frame/_fps() : _num(t))*(fps === undefined ? _fps() : _num(fps)); };
const framesToTime = function(f,fps) { const r=fps === undefined ? _fps() : _num(fps); return r === 0 ? 0 : (f === undefined ? frame : _num(f))/r; };
const toFrames = timeToFrames, toSeconds = framesToTime;
"#;
