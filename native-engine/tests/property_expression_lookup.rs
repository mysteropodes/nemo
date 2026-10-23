//! Oracle and control tests for the expression lookup kernel (D16-K1, #2278).
//!
//! `ORACLE` below was produced by a Node 22.22 (Unicode 16.0) generator kept
//! outside the repo (`gen.js`, sha256
//! 830a348ca257798c379d6bf50f9c0170bf808c54f7bb41437122951de567f272) that cuts `controlsOf`,
//! `normalizeControlName`, `findControl`, `trackFor`, `_exprFps`, `_num`,
//! `findLayerIndexByRef`, `findLayerIndexByUid`, `exprLayerControl`,
//! `exprKeyAt`, `exprNumKeys`, `nearestKeyIndex`, `exprNearestKeyAtFrame`,
//! `aliasKey`, `aliasNearestKey`, `_markerEntry`, `_markerApi` and
//! `_aliasMarkerApi` VERBATIM from
//! `src/js/motion.js` (sha256
//! d38fcb0502e395045543a571531b15a1185c04106e04db165bd2949ed863d7ec), with
//! shims only for `state`, `SM.t`, `_exprTick`, `_ectx` and `_controlValue`
//! (which reports the resolved layer and `findControl` index, not a sample).
//!
//! A line is `HEAD | TOKEN..`; a token is `input=output`. Strings are `~`
//! (null) or `'` + text with every char outside `[A-Za-z0-9_.-]` as `%{hex}`.
//! Numbers are JS `String(n)` with `-0` kept and compare on f64 bits; `-` is
//! null and `E` a Motion throw.
//! * `N`: name = normalized. `D id`: layers `name,uid,ctl:ctl..` (`.` none).
//! * `R doc`: ref = `findLayerIndexByRef,findLayerIndexByUid,layerControl`.
//! * `F doc layer`: control name = `findControl` index (`-1` none).
//! * `T id`: keys `frame/v_v..`; `n`: track = count; `K id fps`: i = key;
//!   `Q id fps`: frame = nearest key; `X id fps ctxfps`: `k:i` = `aliasKey`,
//!   `s:t` = `aliasNearestKey`. A key is `frame,time,S<v>|V<v:..>,index`.
//! * `M id`: markers `frame,name,color`; `c`: list = count; `A id fps`: query
//!   (string or number) = marker; `P`: frame = nearest; `L`: seconds = alias
//!   `nearestKey`. A marker is `frame,time,name,color,index`. `~` is no track
//!   or no list. `H`: the Unicode fingerprint (see its test).

use crate::evaluation::property_expression_lookup::{
    find_control, key_at, key_at_floored, nearest_key_at_frame, nearest_key_seconds,
    normalize_control_name, num_keys, ControlRecord, Fps, KeyEntry, KeyRecord, KeyTrack, KeyValue,
    LayerRecord, LayerTable, LookupError, LookupRefusal as R, MarkerEntry, MarkerList,
    MarkerQuery as Q, MarkerRecord, MAX_CONTROLS, MAX_KEYS, MAX_LAYERS, MAX_MARKERS,
    MAX_NAME_BYTES, MAX_VALUE_DIMS,
};
use std::collections::HashMap;

const ORACLE: &str = r#"
N | ~=' '=' '%{20}=' 'Speed='speed '%{20}%{20}SPEED%{20}%{20}='speed 'Turn%{20}Amt='turn%{20}amt '%{c9}t%{e9}='%{e9}t%{e9} '%{130}stanbul='i%{307}stanbul 'I%{307}='i%{307} '%{3a3}%{391}%{3a3}='%{3c3}%{3b1}%{3c2} '%{391}%{3a3}='%{3b1}%{3c2} '%{391}%{3a3}.='%{3b1}%{3c2}. '%{391}%{301}%{3a3}='%{3b1}%{301}%{3c2} '%{391}%{3a3}%{301}='%{3b1}%{3c2}%{301} '%{391}%{27}%{3a3}='%{3b1}%{27}%{3c2} '%{3a3}='%{3c3}
N | '%{391}%{3a3}%{391}='%{3b1}%{3c3}%{3b1} '%{391}%{3a3}%{20}%{392}='%{3b1}%{3c2}%{20}%{3b2} '%{391}%{295}%{3a3}='%{3b1}%{295}%{3c2} '%{391}%{3a3}%{295}='%{3b1}%{3c3}%{295} '%{391}%{1acf}%{3a3}='%{3b1}%{1acf}%{3c3} '%{391}%{a7ce}%{3a3}='%{3b1}%{a7ce}%{3c3} '%{391}%{3a3}%{a7ce}='%{3b1}%{3c2}%{a7ce} '%{a7ce}='%{a7ce} '%{a7d2}%{a7d4}='%{a7d2}%{a7d4} '%{16ea0}='%{16ea0} '%{16ebb}%{3a3}='%{16ebb}%{3c3} '%{391}%{16ebb}%{3a3}='%{3b1}%{16ebb}%{3c3} '%{1e9e}='%{df} '%{df}='%{df} '%{1c5}='%{1c6} '%{3a9}='%{3c9}
N | 'K='k '%{c5}='%{e5} '%{390}='%{390} '%{10400}='%{10428} '%{ff21}%{ff22}%{ff23}='%{ff41}%{ff42}%{ff43} '%{216b}='%{217b} '%{24d0}%{24b6}='%{24d0}%{24d0} '%{85}x='%{85}x 'x%{feff}='x '%{feff}x%{180e}='x%{180e} '%{200b}x='%{200b}x '%{9}Ab%{9}='ab '%{a}Ab%{a}='ab '%{b}Ab%{b}='ab '%{c}Ab%{c}='ab '%{d}Ab%{d}='ab
N | '%{20}Ab%{20}='ab '%{a0}Ab%{a0}='ab '%{1680}Ab%{1680}='ab '%{2000}Ab%{2000}='ab '%{200a}Ab%{200a}='ab '%{2028}Ab%{2028}='ab '%{2029}Ab%{2029}='ab '%{202f}Ab%{202f}='ab '%{205f}Ab%{205f}='ab '%{3000}Ab%{3000}='ab '%{feff}Ab%{feff}='ab '%{85}Ab%{85}='%{85}ab%{85} '%{180e}Ab%{180e}='%{180e}ab%{180e} '%{200b}Ab%{200b}='%{200b}ab%{200b} '%{1c}Ab%{1c}='%{1c}ab%{1c} '%{1f}Ab%{1f}='%{1f}ab%{1f}
D d0 | 
D d1 | 'Ball,'u1,'Speed:'speed:'%{20}Turn%{20} 'u1,'Ball,'Turn:~:' 'Ball,'u3,'%{391}%{3a3}:'%{130} 'ball,'u4,. ~,'u5,'x%{feff}:'x%{85}:'%{a0}X ',',. 'Rig%{20},'u1,. ~,~,'%{3a3}
D d2 | 'A,'B,. 'B,'A,'Speed '0,'1,'Speed:'SPEED
R d0 | '=-1,-1,E 'Ball=-1,-1,E 'ball=-1,-1,E 'u1=-1,-1,E 'u3=-1,-1,E 'u4=-1,-1,E 'u5=-1,-1,E 'u9=-1,-1,E 'Rig=-1,-1,E 'Rig%{20}=-1,-1,E 'A=-1,-1,E 'B=-1,-1,E '0=-1,-1,E '1=-1,-1,E 'BALL=-1,-1,E '%{20}Ball=-1,-1,E
R d1 | '=-1,-1,E 'Ball=0,1,1 'ball=3,-1,3 'u1=1,0,0 'u3=2,2,2 'u4=3,3,3 'u5=4,4,4 'u9=-1,-1,E 'Rig=-1,-1,E 'Rig%{20}=6,-1,6 'A=-1,-1,E 'B=-1,-1,E '0=-1,-1,E '1=-1,-1,E 'BALL=-1,-1,E '%{20}Ball=-1,-1,E
F d1 0 | ~=-1 '=-1 'speed=0 '%{20}SPEED%{20}=0 'Turn=2 'turn=2 'missing=-1 '%{3b1}%{3c2}=-1 '%{391}%{3a3}=-1 'i%{307}=-1 'x=-1 'X=-1 '%{3c3}=-1 '%{a0}x%{a0}=-1
F d1 1 | ~=-1 '=-1 'speed=-1 '%{20}SPEED%{20}=-1 'Turn=0 'turn=0 'missing=-1 '%{3b1}%{3c2}=-1 '%{391}%{3a3}=-1 'i%{307}=-1 'x=-1 'X=-1 '%{3c3}=-1 '%{a0}x%{a0}=-1
F d1 2 | ~=-1 '=-1 'speed=-1 '%{20}SPEED%{20}=-1 'Turn=-1 'turn=-1 'missing=-1 '%{3b1}%{3c2}=0 '%{391}%{3a3}=0 'i%{307}=1 'x=-1 'X=-1 '%{3c3}=-1 '%{a0}x%{a0}=-1
F d1 3 | ~=-1 '=-1 'speed=-1 '%{20}SPEED%{20}=-1 'Turn=-1 'turn=-1 'missing=-1 '%{3b1}%{3c2}=-1 '%{391}%{3a3}=-1 'i%{307}=-1 'x=-1 'X=-1 '%{3c3}=-1 '%{a0}x%{a0}=-1
F d1 4 | ~=-1 '=-1 'speed=-1 '%{20}SPEED%{20}=-1 'Turn=-1 'turn=-1 'missing=-1 '%{3b1}%{3c2}=-1 '%{391}%{3a3}=-1 'i%{307}=-1 'x=0 'X=0 '%{3c3}=-1 '%{a0}x%{a0}=0
F d1 5 | ~=-1 '=-1 'speed=-1 '%{20}SPEED%{20}=-1 'Turn=-1 'turn=-1 'missing=-1 '%{3b1}%{3c2}=-1 '%{391}%{3a3}=-1 'i%{307}=-1 'x=-1 'X=-1 '%{3c3}=-1 '%{a0}x%{a0}=-1
F d1 6 | ~=-1 '=-1 'speed=-1 '%{20}SPEED%{20}=-1 'Turn=-1 'turn=-1 'missing=-1 '%{3b1}%{3c2}=-1 '%{391}%{3a3}=-1 'i%{307}=-1 'x=-1 'X=-1 '%{3c3}=-1 '%{a0}x%{a0}=-1
F d1 7 | ~=-1 '=-1 'speed=-1 '%{20}SPEED%{20}=-1 'Turn=-1 'turn=-1 'missing=-1 '%{3b1}%{3c2}=-1 '%{391}%{3a3}=-1 'i%{307}=-1 'x=-1 'X=-1 '%{3c3}=0 '%{a0}x%{a0}=-1
R d2 | '=-1,-1,E 'Ball=-1,-1,E 'ball=-1,-1,E 'u1=-1,-1,E 'u3=-1,-1,E 'u4=-1,-1,E 'u5=-1,-1,E 'u9=-1,-1,E 'Rig=-1,-1,E 'Rig%{20}=-1,-1,E 'A=0,1,1 'B=1,0,0 '0=2,-1,2 '1=2,2,2 'BALL=-1,-1,E '%{20}Ball=-1,-1,E
F d2 0 | ~=-1 '=-1 'speed=-1 '%{20}SPEED%{20}=-1 'Turn=-1 'turn=-1 'missing=-1 '%{3b1}%{3c2}=-1 '%{391}%{3a3}=-1 'i%{307}=-1 'x=-1 'X=-1 '%{3c3}=-1 '%{a0}x%{a0}=-1
F d2 1 | ~=-1 '=-1 'speed=0 '%{20}SPEED%{20}=0 'Turn=-1 'turn=-1 'missing=-1 '%{3b1}%{3c2}=-1 '%{391}%{3a3}=-1 'i%{307}=-1 'x=-1 'X=-1 '%{3c3}=-1 '%{a0}x%{a0}=-1
F d2 2 | ~=-1 '=-1 'speed=0 '%{20}SPEED%{20}=0 'Turn=-1 'turn=-1 'missing=-1 '%{3b1}%{3c2}=-1 '%{391}%{3a3}=-1 'i%{307}=-1 'x=-1 'X=-1 '%{3c3}=-1 '%{a0}x%{a0}=-1
T t0 | 
T t1 | 0/5
T t2 | 0/1_2 10/3 20/-0_0.1 30/. 31/7_8_9
T t3 | -12/1 2.5/2 2.5/3 7/4
T t4 | 10/1 0/2 30/3 5/4 20/5
n | ~=0 t0=0 t1=1 t2=5 t3=4 t4=5
K ~ 24 | 0=- 1=- 2=- 3=- 4=- 5=- 6=- -1=- -0=- 1.5=- 0.5=- 4.999=- 5.5=- 2.0000001=- 1e+300=- -1e+300=-
Q ~ 24 | -12=- -4.75=- 0=- 2.5=- 4.75=- 5=- 7=- 10=- -Infinity=- 15=- 25=- 30=- 30.5=- 31=- -0=- Infinity=-
X ~ 24 30 | k:1.5=- k:2.9=- k:-0.5=- k:NaN=- k:Infinity=- k:1e+300=- k:0=- k:5=- s:0.2=- s:0.3333333333333333=- s:1e+307=- s:-1e+307=- s:NaN=- s:Infinity=- s:-Infinity=- s:0.5=-
K ~ 29.97 | 0=- 1=- 2=- 3=- 4=- 5=- 6=- -1=- -0=- 1.5=- 0.5=- 4.999=- 5.5=- 2.0000001=- 1e+300=- -1e+300=-
K t0 24 | 0=- 1=- 2=- 3=- 4=- 5=- 6=- -1=- -0=- 1.5=- 0.5=- 4.999=- 5.5=- 2.0000001=- 1e+300=- -1e+300=-
Q t0 24 | -12=- -4.75=- 0=- 2.5=- 4.75=- 5=- 7=- 10=- -Infinity=- 15=- 25=- 30=- 30.5=- 31=- -0=- Infinity=-
X t0 24 30 | k:1.5=- k:2.9=- k:-0.5=- k:NaN=- k:Infinity=- k:1e+300=- k:0=- k:5=- s:0.2=- s:0.3333333333333333=- s:1e+307=- s:-1e+307=- s:NaN=- s:Infinity=- s:-Infinity=- s:0.5=-
K t0 29.97 | 0=- 1=- 2=- 3=- 4=- 5=- 6=- -1=- -0=- 1.5=- 0.5=- 4.999=- 5.5=- 2.0000001=- 1e+300=- -1e+300=-
K t1 24 | 0=- 1=0,0,S5,1 2=- 3=- 4=- 5=- 6=- -1=- -0=- 1.5=- 0.5=- 4.999=- 5.5=- 2.0000001=- 1e+300=- -1e+300=-
Q t1 24 | -12=0,0,S5,1 -4.75=0,0,S5,1 0=0,0,S5,1 2.5=0,0,S5,1 4.75=0,0,S5,1 5=0,0,S5,1 7=0,0,S5,1 10=0,0,S5,1 -Infinity=0,0,S5,1 15=0,0,S5,1 25=0,0,S5,1 30=0,0,S5,1 30.5=0,0,S5,1 31=0,0,S5,1 -0=0,0,S5,1 Infinity=0,0,S5,1
X t1 24 30 | k:1.5=0,0,S5,1 k:2.9=- k:-0.5=- k:NaN=- k:Infinity=- k:1e+300=- k:0=- k:5=- s:0.2=0,0,S5,1 s:0.3333333333333333=0,0,S5,1 s:1e+307=0,0,S5,1 s:-1e+307=0,0,S5,1 s:NaN=0,0,S5,1 s:Infinity=0,0,S5,1 s:-Infinity=0,0,S5,1 s:0.5=0,0,S5,1
K t1 29.97 | 0=- 1=0,0,S5,1 2=- 3=- 4=- 5=- 6=- -1=- -0=- 1.5=- 0.5=- 4.999=- 5.5=- 2.0000001=- 1e+300=- -1e+300=-
K t2 24 | 0=- 1=0,0,V1:2,1 2=10,0.4166666666666667,S3,2 3=20,0.8333333333333334,V-0:0.1,3 4=30,1.25,V,4 5=31,1.2916666666666667,V7:8:9,5 6=- -1=- -0=- 1.5=E 0.5=- 4.999=E 5.5=- 2.0000001=E 1e+300=- -1e+300=-
Q t2 24 | -12=0,0,V1:2,1 -4.75=0,0,V1:2,1 0=0,0,V1:2,1 2.5=0,0,V1:2,1 4.75=0,0,V1:2,1 5=0,0,V1:2,1 7=10,0.4166666666666667,S3,2 10=10,0.4166666666666667,S3,2 -Infinity=0,0,V1:2,1 15=10,0.4166666666666667,S3,2 25=20,0.8333333333333334,V-0:0.1,3 30=30,1.25,V,4 30.5=30,1.25,V,4 31=31,1.2916666666666667,V7:8:9,5 -0=0,0,V1:2,1 Infinity=31,1.2916666666666667,V7:8:9,5
X t2 24 30 | k:1.5=0,0,V1:2,1 k:2.9=10,0.4166666666666667,S3,2 k:-0.5=- k:NaN=- k:Infinity=- k:1e+300=- k:0=- k:5=31,1.2916666666666667,V7:8:9,5 s:0.2=10,0.4166666666666667,S3,2 s:0.3333333333333333=10,0.4166666666666667,S3,2 s:1e+307=31,1.2916666666666667,V7:8:9,5 s:-1e+307=0,0,V1:2,1 s:NaN=0,0,V1:2,1 s:Infinity=0,0,V1:2,1 s:-Infinity=0,0,V1:2,1 s:0.5=10,0.4166666666666667,S3,2
K t2 29.97 | 0=- 1=0,0,V1:2,1 2=10,0.33366700033366703,S3,2 3=20,0.6673340006673341,V-0:0.1,3 4=30,1.001001001001001,V,4 5=31,1.0343677010343677,V7:8:9,5 6=- -1=- -0=- 1.5=E 0.5=- 4.999=E 5.5=- 2.0000001=E 1e+300=- -1e+300=-
K t3 24 | 0=- 1=-12,-0.5,S1,1 2=2.5,0.10416666666666667,S2,2 3=2.5,0.10416666666666667,S3,3 4=7,0.2916666666666667,S4,4 5=- 6=- -1=- -0=- 1.5=E 0.5=- 4.999=- 5.5=- 2.0000001=E 1e+300=- -1e+300=-
Q t3 24 | -12=-12,-0.5,S1,1 -4.75=-12,-0.5,S1,1 0=2.5,0.10416666666666667,S2,2 2.5=2.5,0.10416666666666667,S2,2 4.75=2.5,0.10416666666666667,S3,3 5=7,0.2916666666666667,S4,4 7=7,0.2916666666666667,S4,4 10=7,0.2916666666666667,S4,4 -Infinity=-12,-0.5,S1,1 15=7,0.2916666666666667,S4,4 25=7,0.2916666666666667,S4,4 30=7,0.2916666666666667,S4,4 30.5=7,0.2916666666666667,S4,4 31=7,0.2916666666666667,S4,4 -0=2.5,0.10416666666666667,S2,2 Infinity=7,0.2916666666666667,S4,4
X t3 24 30 | k:1.5=-12,-0.5,S1,1 k:2.9=2.5,0.10416666666666667,S2,2 k:-0.5=- k:NaN=- k:Infinity=- k:1e+300=- k:0=- k:5=- s:0.2=7,0.2916666666666667,S4,4 s:0.3333333333333333=7,0.2916666666666667,S4,4 s:1e+307=7,0.2916666666666667,S4,4 s:-1e+307=-12,-0.5,S1,1 s:NaN=2.5,0.10416666666666667,S2,2 s:Infinity=2.5,0.10416666666666667,S2,2 s:-Infinity=2.5,0.10416666666666667,S2,2 s:0.5=7,0.2916666666666667,S4,4
K t3 29.97 | 0=- 1=-12,-0.4004004004004004,S1,1 2=2.5,0.08341675008341676,S2,2 3=2.5,0.08341675008341676,S3,3 4=7,0.2335669002335669,S4,4 5=- 6=- -1=- -0=- 1.5=E 0.5=- 4.999=- 5.5=- 2.0000001=E 1e+300=- -1e+300=-
K t4 24 | 0=- 1=10,0.4166666666666667,S1,1 2=0,0,S2,2 3=30,1.25,S3,3 4=5,0.20833333333333334,S4,4 5=20,0.8333333333333334,S5,5 6=- -1=- -0=- 1.5=E 0.5=- 4.999=E 5.5=- 2.0000001=E 1e+300=- -1e+300=-
Q t4 24 | -12=10,0.4166666666666667,S1,1 -4.75=10,0.4166666666666667,S1,1 0=10,0.4166666666666667,S1,1 2.5=0,0,S2,2 4.75=0,0,S2,2 5=0,0,S2,2 7=0,0,S2,2 10=0,0,S2,2 -Infinity=10,0.4166666666666667,S1,1 15=0,0,S2,2 25=30,1.25,S3,3 30=30,1.25,S3,3 30.5=20,0.8333333333333334,S5,5 31=20,0.8333333333333334,S5,5 -0=10,0.4166666666666667,S1,1 Infinity=20,0.8333333333333334,S5,5
X t4 24 30 | k:1.5=10,0.4166666666666667,S1,1 k:2.9=0,0,S2,2 k:-0.5=- k:NaN=- k:Infinity=- k:1e+300=- k:0=- k:5=20,0.8333333333333334,S5,5 s:0.2=0,0,S2,2 s:0.3333333333333333=0,0,S2,2 s:1e+307=20,0.8333333333333334,S5,5 s:-1e+307=10,0.4166666666666667,S1,1 s:NaN=10,0.4166666666666667,S1,1 s:Infinity=10,0.4166666666666667,S1,1 s:-Infinity=10,0.4166666666666667,S1,1 s:0.5=0,0,S2,2
K t4 29.97 | 0=- 1=10,0.33366700033366703,S1,1 2=0,0,S2,2 3=30,1.001001001001001,S3,3 4=5,0.16683350016683351,S4,4 5=20,0.6673340006673341,S5,5 6=- -1=- -0=- 1.5=E 0.5=- 4.999=E 5.5=- 2.0000001=E 1e+300=- -1e+300=-
M m0 | 
M m1 | 10,'Hit,'%{23}f00 0,',~ 20,'Hit,' 5,~,'%{23}0f0 15,'hit,~ -5,'Pr%{e9}%{20}roll,~
M m2 | 4,'a,~ 8,'b,~ 6,'c,~ 6,'d,~
M m3 | 1e+308,'far,~ -1e+308,'neg,~
c | ~=0 m0=0 m1=6 m2=4 m3=2
A ~ 24 | 0=- 1=- 2=- 3=- 4=- 6=- 7=- -1=- -0.5=- -0=- 0.5=- 1.9=- 3.999=- 6.5=- 1e+300=- -1e+300=-
A ~ 24 | NaN=- Infinity=- -Infinity=- '=- 'Hit=- 'hit=- 'HIT=- 'Pr%{e9}%{20}roll=- 'a=- 'c=- 'd=- 'zzz=- '1=- '%{20}Hit=-
P ~ 24 | -5=- -2.5=- 0=- 2.5=- 5=- 6=- 7=- 7.5=- 12.5=- 17.5=- -0=- NaN=- Infinity=- -Infinity=- 1e+308=- -1e+308=-
L ~ 24 | 0=- 0.25=- 0.3125=- 0.3333333333333333=- -1=- 1e+306=- 1e+308=- -1e+308=- NaN=- Infinity=-
A m0 24 | 0=- 1=- 2=- 3=- 4=- 6=- 7=- -1=- -0.5=- -0=- 0.5=- 1.9=- 3.999=- 6.5=- 1e+300=- -1e+300=-
A m0 24 | NaN=- Infinity=- -Infinity=- '=- 'Hit=- 'hit=- 'HIT=- 'Pr%{e9}%{20}roll=- 'a=- 'c=- 'd=- 'zzz=- '1=- '%{20}Hit=-
P m0 24 | -5=- -2.5=- 0=- 2.5=- 5=- 6=- 7=- 7.5=- 12.5=- 17.5=- -0=- NaN=- Infinity=- -Infinity=- 1e+308=- -1e+308=-
L m0 24 | 0=- 0.25=- 0.3125=- 0.3333333333333333=- -1=- 1e+306=- 1e+308=- -1e+308=- NaN=- Infinity=-
A m1 24 | 0=- 1=10,0.4166666666666667,'Hit,'%{23}f00,1 2=0,0,',~,2 3=20,0.8333333333333334,'Hit,~,3 4=5,0.20833333333333334,','%{23}0f0,4 6=-5,-0.20833333333333334,'Pr%{e9}%{20}roll,~,6 7=- -1=- -0.5=- -0=- 0.5=- 1.9=10,0.4166666666666667,'Hit,'%{23}f00,1 3.999=20,0.8333333333333334,'Hit,~,3 6.5=-5,-0.20833333333333334,'Pr%{e9}%{20}roll,~,6 1e+300=- -1e+300=-
A m1 24 | NaN=- Infinity=- -Infinity=- '=0,0,',~,2 'Hit=10,0.4166666666666667,'Hit,'%{23}f00,1 'hit=15,0.625,'hit,~,5 'HIT=- 'Pr%{e9}%{20}roll=-5,-0.20833333333333334,'Pr%{e9}%{20}roll,~,6 'a=- 'c=- 'd=- 'zzz=- '1=- '%{20}Hit=-
P m1 24 | -5=-5,-0.20833333333333334,'Pr%{e9}%{20}roll,~,6 -2.5=0,0,',~,2 0=0,0,',~,2 2.5=0,0,',~,2 5=5,0.20833333333333334,','%{23}0f0,4 6=5,0.20833333333333334,','%{23}0f0,4 7=5,0.20833333333333334,','%{23}0f0,4 7.5=10,0.4166666666666667,'Hit,'%{23}f00,1 12.5=10,0.4166666666666667,'Hit,'%{23}f00,1 17.5=20,0.8333333333333334,'Hit,~,3 -0=0,0,',~,2 NaN=0,0,',~,2 Infinity=0,0,',~,2 -Infinity=0,0,',~,2 1e+308=10,0.4166666666666667,'Hit,'%{23}f00,1 -1e+308=10,0.4166666666666667,'Hit,'%{23}f00,1
L m1 24 | 0=0,0,',~,2 0.25=5,0.20833333333333334,','%{23}0f0,4 0.3125=10,0.4166666666666667,'Hit,'%{23}f00,1 0.3333333333333333=10,0.4166666666666667,'Hit,'%{23}f00,1 -1=-5,-0.20833333333333334,'Pr%{e9}%{20}roll,~,6 1e+306=10,0.4166666666666667,'Hit,'%{23}f00,1 1e+308=0,0,',~,2 -1e+308=0,0,',~,2 NaN=0,0,',~,2 Infinity=0,0,',~,2
A m1 30 | 0=- 1=10,0.3333333333333333,'Hit,'%{23}f00,1 2=0,0,',~,2 3=20,0.6666666666666666,'Hit,~,3 4=5,0.16666666666666666,','%{23}0f0,4 6=-5,-0.16666666666666666,'Pr%{e9}%{20}roll,~,6 7=- -1=- -0.5=- -0=- 0.5=- 1.9=10,0.3333333333333333,'Hit,'%{23}f00,1 3.999=20,0.6666666666666666,'Hit,~,3 6.5=-5,-0.16666666666666666,'Pr%{e9}%{20}roll,~,6 1e+300=- -1e+300=-
A m1 30 | NaN=- Infinity=- -Infinity=- '=0,0,',~,2 'Hit=10,0.3333333333333333,'Hit,'%{23}f00,1 'hit=15,0.5,'hit,~,5 'HIT=- 'Pr%{e9}%{20}roll=-5,-0.16666666666666666,'Pr%{e9}%{20}roll,~,6 'a=- 'c=- 'd=- 'zzz=- '1=- '%{20}Hit=-
P m1 30 | -5=-5,-0.16666666666666666,'Pr%{e9}%{20}roll,~,6 -2.5=0,0,',~,2 0=0,0,',~,2 2.5=0,0,',~,2 5=5,0.16666666666666666,','%{23}0f0,4 6=5,0.16666666666666666,','%{23}0f0,4 7=5,0.16666666666666666,','%{23}0f0,4 7.5=10,0.3333333333333333,'Hit,'%{23}f00,1 12.5=10,0.3333333333333333,'Hit,'%{23}f00,1 17.5=20,0.6666666666666666,'Hit,~,3 -0=0,0,',~,2 NaN=0,0,',~,2 Infinity=0,0,',~,2 -Infinity=0,0,',~,2 1e+308=10,0.3333333333333333,'Hit,'%{23}f00,1 -1e+308=10,0.3333333333333333,'Hit,'%{23}f00,1
L m1 30 | 0=0,0,',~,2 0.25=10,0.3333333333333333,'Hit,'%{23}f00,1 0.3125=10,0.3333333333333333,'Hit,'%{23}f00,1 0.3333333333333333=10,0.3333333333333333,'Hit,'%{23}f00,1 -1=-5,-0.16666666666666666,'Pr%{e9}%{20}roll,~,6 1e+306=10,0.3333333333333333,'Hit,'%{23}f00,1 1e+308=0,0,',~,2 -1e+308=0,0,',~,2 NaN=0,0,',~,2 Infinity=0,0,',~,2
A m2 24 | 0=- 1=4,0.16666666666666666,'a,~,1 2=8,0.3333333333333333,'b,~,2 3=6,0.25,'c,~,3 4=6,0.25,'d,~,4 6=- 7=- -1=- -0.5=- -0=- 0.5=- 1.9=4,0.16666666666666666,'a,~,1 3.999=6,0.25,'c,~,3 6.5=- 1e+300=- -1e+300=-
A m2 24 | NaN=- Infinity=- -Infinity=- '=- 'Hit=- 'hit=- 'HIT=- 'Pr%{e9}%{20}roll=- 'a=4,0.16666666666666666,'a,~,1 'c=6,0.25,'c,~,3 'd=6,0.25,'d,~,4 'zzz=- '1=- '%{20}Hit=-
P m2 24 | -5=4,0.16666666666666666,'a,~,1 -2.5=4,0.16666666666666666,'a,~,1 0=4,0.16666666666666666,'a,~,1 2.5=4,0.16666666666666666,'a,~,1 5=4,0.16666666666666666,'a,~,1 6=6,0.25,'c,~,3 7=8,0.3333333333333333,'b,~,2 7.5=8,0.3333333333333333,'b,~,2 12.5=8,0.3333333333333333,'b,~,2 17.5=8,0.3333333333333333,'b,~,2 -0=4,0.16666666666666666,'a,~,1 NaN=4,0.16666666666666666,'a,~,1 Infinity=4,0.16666666666666666,'a,~,1 -Infinity=4,0.16666666666666666,'a,~,1 1e+308=4,0.16666666666666666,'a,~,1 -1e+308=4,0.16666666666666666,'a,~,1
L m2 24 | 0=4,0.16666666666666666,'a,~,1 0.25=6,0.25,'c,~,3 0.3125=8,0.3333333333333333,'b,~,2 0.3333333333333333=8,0.3333333333333333,'b,~,2 -1=4,0.16666666666666666,'a,~,1 1e+306=4,0.16666666666666666,'a,~,1 1e+308=4,0.16666666666666666,'a,~,1 -1e+308=4,0.16666666666666666,'a,~,1 NaN=4,0.16666666666666666,'a,~,1 Infinity=4,0.16666666666666666,'a,~,1
A m3 24 | 0=- 1=1e+308,4.1666666666666665e+306,'far,~,1 2=-1e+308,-4.1666666666666665e+306,'neg,~,2 3=- 4=- 6=- 7=- -1=- -0.5=- -0=- 0.5=- 1.9=1e+308,4.1666666666666665e+306,'far,~,1 3.999=- 6.5=- 1e+300=- -1e+300=-
A m3 24 | NaN=- Infinity=- -Infinity=- '=- 'Hit=- 'hit=- 'HIT=- 'Pr%{e9}%{20}roll=- 'a=- 'c=- 'd=- 'zzz=- '1=- '%{20}Hit=-
P m3 24 | -5=1e+308,4.1666666666666665e+306,'far,~,1 -2.5=1e+308,4.1666666666666665e+306,'far,~,1 0=1e+308,4.1666666666666665e+306,'far,~,1 2.5=1e+308,4.1666666666666665e+306,'far,~,1 5=1e+308,4.1666666666666665e+306,'far,~,1 6=1e+308,4.1666666666666665e+306,'far,~,1 7=1e+308,4.1666666666666665e+306,'far,~,1 7.5=1e+308,4.1666666666666665e+306,'far,~,1 12.5=1e+308,4.1666666666666665e+306,'far,~,1 17.5=1e+308,4.1666666666666665e+306,'far,~,1 -0=1e+308,4.1666666666666665e+306,'far,~,1 NaN=1e+308,4.1666666666666665e+306,'far,~,1 Infinity=1e+308,4.1666666666666665e+306,'far,~,1 -Infinity=1e+308,4.1666666666666665e+306,'far,~,1 1e+308=1e+308,4.1666666666666665e+306,'far,~,1 -1e+308=-1e+308,-4.1666666666666665e+306,'neg,~,2
L m3 24 | 0=1e+308,4.1666666666666665e+306,'far,~,1 0.25=1e+308,4.1666666666666665e+306,'far,~,1 0.3125=1e+308,4.1666666666666665e+306,'far,~,1 0.3333333333333333=1e+308,4.1666666666666665e+306,'far,~,1 -1=1e+308,4.1666666666666665e+306,'far,~,1 1e+306=1e+308,4.1666666666666665e+306,'far,~,1 1e+308=1e+308,4.1666666666666665e+306,'far,~,1 -1e+308=1e+308,4.1666666666666665e+306,'far,~,1 NaN=1e+308,4.1666666666666665e+306,'far,~,1 Infinity=1e+308,4.1666666666666665e+306,'far,~,1
H | d23c7984
"#;

fn s(tok: &str) -> Option<String> {
    let mut rest = tok.strip_prefix('\'')?;
    let mut out = String::new();
    while let Some(at) = rest.find("%{") {
        let end = rest.find('}').unwrap();
        out.push_str(&rest[..at]);
        out.push(char::from_u32(u32::from_str_radix(&rest[at + 2..end], 16).unwrap()).unwrap());
        rest = &rest[end + 1..];
    }
    Some(out + rest)
}

fn enc(v: Option<&str>) -> String {
    let esc = |c: char| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '.' | '-' => c.to_string(),
        _ => format!("%{{{:x}}}", c as u32),
    };
    v.map_or("~".into(), |v| {
        format!("'{}", v.chars().map(esc).collect::<String>())
    })
}

fn f(tok: &str) -> f64 {
    tok.parse().unwrap_or_else(|_| panic!("number {tok}"))
}

fn b(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}

fn key(r: Result<Option<KeyEntry>, LookupError>) -> String {
    let k = match r {
        Err(LookupError::KeyIndexNotInteger) => return "E".into(),
        r => r.unwrap(),
    };
    k.map_or("-".into(), |k| {
        let v = match k.value {
            KeyValue::Scalar(v) => format!("S{}", b(v)),
            KeyValue::Vector(v) => {
                format!("V{}", v.into_iter().map(b).collect::<Vec<_>>().join(":"))
            }
        };
        format!("{},{},{v},{}", b(k.frame), b(k.time), k.index)
    })
}

fn marker(m: Option<MarkerEntry>) -> String {
    m.map_or("-".into(), |m| {
        let (name, color) = (enc(Some(&m.name)), enc(m.color.as_deref()));
        format!("{},{},{name},{color},{}", b(m.frame), b(m.time), m.index)
    })
}

/// Rewrites an expected key or marker so its numbers read as f64 bits.
fn bits_of(expected: &str, is_key: bool) -> String {
    if expected == "-" || expected == "E" {
        return expected.into();
    }
    let mut p: Vec<String> = expected.split(',').map(str::to_owned).collect();
    p[0] = b(f(&p[0]));
    p[1] = b(f(&p[1]));
    if is_key {
        let (tag, rest) = p[2].split_at(1);
        let vs: Vec<String> = rest
            .split(':')
            .filter(|x| !x.is_empty())
            .map(|x| b(f(x)))
            .collect();
        p[2] = format!("{tag}{}", vs.join(":"));
    }
    p.join(",")
}

fn idx(i: Option<usize>) -> String {
    i.map_or("-1".into(), |i| i.to_string())
}

fn fps(tok: &str) -> Fps {
    Fps::new(f(tok)).unwrap()
}

fn ctl(name: Option<&str>) -> ControlRecord {
    let name = name.map(Into::into);
    ControlRecord { name }
}

fn mk(frame: f64, name: Option<&str>) -> MarkerRecord {
    let (name, color) = (name.map(Into::into), None);
    MarkerRecord { frame, name, color }
}

fn kr(frame: f64, value: &[f64]) -> KeyRecord {
    let value = value.to_vec();
    KeyRecord { frame, value }
}

type Row = (Vec<String>, String, String);
type Alt<'a> = &'a dyn Fn(&Row) -> String;

#[derive(Default)]
struct Oracle {
    docs: HashMap<String, LayerTable>,
    tracks: HashMap<String, KeyTrack>,
    markers: HashMap<String, MarkerList>,
    rows: Vec<Row>,
    fingerprint: String,
}

fn load() -> Oracle {
    let mut o = Oracle::default();
    for line in ORACLE.lines().filter(|l| !l.is_empty()) {
        let (head, toks) = line.split_once(" |").unwrap();
        let head: Vec<String> = head.split(' ').map(str::to_owned).collect();
        let toks = toks.split(' ').filter(|t| !t.is_empty());
        let id = head.get(1).cloned().unwrap_or_default();
        match head[0].as_str() {
            "D" => {
                let layers = toks.map(|t| {
                    let p: Vec<&str> = t.split(',').collect();
                    let names = p[2].split(':').filter(|c| *c != ".");
                    let controls = names.map(|n| ctl(s(n).as_deref())).collect();
                    LayerRecord {
                        name: s(p[0]),
                        layer_uid: s(p[1]),
                        controls,
                    }
                });
                o.docs
                    .insert(id, LayerTable::new(layers.collect()).unwrap());
            }
            "T" => {
                let keys = toks.map(|t| {
                    let (fr, v) = t.split_once('/').unwrap();
                    let v: Vec<f64> = v.split('_').filter(|x| *x != ".").map(f).collect();
                    kr(f(fr), &v)
                });
                o.tracks.insert(id, KeyTrack::new(keys.collect()).unwrap());
            }
            "M" => {
                let ms = toks.map(|t| {
                    let p: Vec<&str> = t.split(',').collect();
                    let (frame, name, color) = (f(p[0]), s(p[1]), s(p[2]));
                    MarkerRecord { frame, name, color }
                });
                o.markers.insert(id, MarkerList::new(ms.collect()).unwrap());
            }
            "H" => o.fingerprint = toks.collect(),
            kind => {
                for t in toks {
                    let (i, e) = t.split_once('=').unwrap();
                    let e = match kind {
                        "K" | "Q" | "X" => bits_of(e, true),
                        "A" | "P" | "L" => bits_of(e, false),
                        _ => e.to_owned(),
                    };
                    o.rows.push((head.clone(), i.to_owned(), e));
                }
            }
        }
    }
    o
}

fn run(o: &Oracle, head: &[String], i: &str) -> String {
    let track = |id: &str| o.tracks.get(id);
    let empty = MarkerList::new(vec![]).unwrap();
    let list = || o.markers.get(&head[1]).unwrap_or(&empty);
    let (fps, ctx) = (|| fps(&head[2]), || fps(&head[3]));
    match head[0].as_str() {
        "N" => enc(Some(&normalize_control_name(s(i).as_deref()).unwrap())),
        "R" => {
            let (d, r) = (&o.docs[&head[1]], s(i).unwrap());
            let ctl = match d.layer_control(&r, None) {
                Err(LookupError::UnknownLayer) => "E".into(),
                c => c.unwrap().layer_index.to_string(),
            };
            let (by_ref, by_uid) = (d.find_by_ref(&r).unwrap(), d.find_by_uid(&r).unwrap());
            format!("{},{},{ctl}", idx(by_ref), idx(by_uid))
        }
        "F" => {
            let l = &o.docs[&head[1]].layers()[head[2].parse::<usize>().unwrap()];
            idx(find_control(&l.controls, s(i).as_deref()).unwrap())
        }
        "n" => num_keys(track(i)).to_string(),
        "K" => key(key_at(track(&head[1]), f(i), fps())),
        "Q" => key(nearest_key_at_frame(track(&head[1]), f(i), fps())),
        "X" => match i.split_once(':').unwrap() {
            ("k", i) => key(Ok(key_at_floored(track(&head[1]), f(i), fps()))),
            (_, t) => key(Ok(nearest_key_seconds(track(&head[1]), f(t), ctx(), fps()))),
        },
        "c" => o.markers.get(i).map_or(0, MarkerList::count).to_string(),
        "A" => {
            let name = s(i);
            let q = name.as_deref().map_or_else(|| Q::Number(f(i)), Q::Name);
            marker(list().at(q, fps()).unwrap())
        }
        "P" => marker(list().nearest(f(i), fps())),
        "L" => marker(list().nearest_key_seconds(f(i), fps())),
        k => panic!("row kind {k}"),
    }
}

#[test]
fn oracle_rows_match_node_bit_for_bit() {
    let o = load();
    assert_eq!(o.rows.len(), 997);
    for (head, i, e) in &o.rows {
        assert_eq!(&run(&o, head, i), e, "{} {i}", head.join(" "));
    }
}

/// `eq! { actual => expected; .. }`: one `assert_eq!` per pair.
macro_rules! eq {
    ($($a:expr => $b:expr;)*) => { $(assert_eq!($a, $b);)* };
}

/// Each plausible misreading of a Motion rule disagrees with some oracle row,
/// and a deliberately wrong expectation fails the row comparison.
#[test]
fn oracle_rejects_rule_misreadings() {
    let o = load();
    let empty = MarkerList::new(vec![]).unwrap();
    let field = |v: &str, n: usize| v.split(',').nth(n).unwrap().to_owned();
    let docs = |r: &Row| (&o.docs[&r.0[1]], s(&r.1).unwrap());
    let uid_first = |r: &Row| {
        let (d, x) = docs(r);
        idx(d.find_by_uid(&x).unwrap().or(d.find_by_ref(&x).unwrap()))
    };
    let by_ref = |r: &Row| {
        let (d, x) = docs(r);
        d.find_by_ref(&x)
            .unwrap()
            .map_or("E".into(), |i| i.to_string())
    };
    // The alias nearestKey without the inner `_num`: an infinite target is at
    // distance infinity from every marker, so the first would win.
    let no_num = |r: &Row| {
        let (l, fps) = (o.markers.get(&r.0[1]).unwrap_or(&empty), fps(&r.0[2]));
        let target = f(&r.1) * fps.get();
        let infinite = l.at(Q::Number(1.0), fps).unwrap();
        marker(if target.is_finite() {
            l.nearest(target, fps)
        } else {
            infinite
        })
    };
    let wrong = |r: &Row| r.2.replacen(',', "1,", 1);
    let alternatives: [(&str, Alt); 6] = [
        // Rust's own `trim` / `to_lowercase` instead of the JS ones.
        ("N", &|r| {
            enc(Some(&s(&r.1).unwrap_or_default().trim().to_lowercase()))
        }),
        // findLayerIndexByRef by uid before name.
        ("R", &|r| {
            format!("{},{},{}", uid_first(r), field(&r.2, 1), field(&r.2, 2))
        }),
        // layerControl by reference (name first) instead of uid first.
        ("R", &|r| {
            format!("{},{},{}", field(&r.2, 0), field(&r.2, 1), by_ref(r))
        }),
        // Flooring the raw key index (Motion throws on an in-range fraction).
        ("K", &|r| {
            key(key_at(o.tracks.get(&r.0[1]), f(&r.1).floor(), fps(&r.0[2])))
        }),
        ("L", &no_num),
        // A wrong expectation: the real output must then fail the comparison.
        ("K", &|r| run(&o, &r.0, &r.1).replace(&r.2, &wrong(r))),
    ];
    for (kind, alt) in alternatives {
        let rows = o.rows.iter().filter(|r| r.0[0] == kind);
        assert!(rows.filter(|r| alt(r) != r.2).count() > 0, "{kind}");
    }
}

/// Toolchain tripwire, no version check: Node's FNV-1a over every scalar `c` of
/// `c`, `normalizeControlName(c)`, `0x110000` and (but U+03A3) the final-sigma
/// pair `2 * [ΑΣc ends in ς] + [ΑcΣ ends in ς]` fails only on a real mismatch.
#[test]
fn unicode_results_match_node_on_this_toolchain() {
    let n = |x: &str| normalize_control_name(Some(x)).unwrap();
    let sig = |x: String| u32::from(n(&x).contains('\u{3c2}'));
    let mut h: u32 = 0x811c_9dc5;
    let mut feed = |x: u32| h = (h ^ x).wrapping_mul(0x0100_0193);
    for ch in (0..=0x10_ffff).filter_map(char::from_u32) {
        feed(ch as u32);
        n(&ch.to_string()).chars().for_each(|o| feed(o as u32));
        feed(0x11_0000);
        if ch != '\u{3a3}' {
            feed(2 * sig(format!("\u{391}\u{3a3}{ch}")) + sig(format!("\u{391}{ch}\u{3a3}")));
        }
    }
    assert_eq!(
        format!("{h:x}"),
        load().fingerprint,
        "re-measure JS_* tables"
    );
}

#[test]
fn hand_written_resolution_cases() {
    let layer = |name: &str, uid: &str, ctls: &[&str]| LayerRecord {
        name: Some(name.into()),
        layer_uid: Some(uid.into()),
        controls: ctls.iter().map(|c| ctl(Some(c))).collect(),
    };
    let t = [layer("X", "Y", &[]), layer("Y", "X", &["Gain", " gain "])];
    let t = LayerTable::new(t.to_vec()).unwrap();
    let lc = |r: &str, n: &str| {
        t.layer_control(r, Some(n))
            .map(|c| (c.layer_index, c.control))
    };
    let fps24 = Fps::new(24.0).unwrap();
    let tr = KeyTrack::new(vec![kr(0.0, &[1.0]), kr(10.0, &[2.0, 3.0])]).unwrap();
    let at = |i: f64| key_at(Some(&tr), i, fps24);
    let near = |x: f64| key(nearest_key_at_frame(Some(&tr), x, fps24));
    let ml = [mk(8.0, Some("b")), mk(4.0, None), mk(6.0, Some("b"))];
    let ml = MarkerList::new(ml.to_vec()).unwrap();
    let m_at = |q: Q| ml.at(q, fps24).unwrap().map(|m| m.index);
    let m_near = |x: f64| ml.nearest(x, fps24).map(|m| m.index);
    eq! {
        t.find_by_ref("Y") => Ok(Some(1));
        lc("Y", "GAIN") => Ok((0, None));
        lc("X", "\u{a0}gain") => Ok((1, Some(0)));
        lc("", "gain") => Err(LookupError::UnknownLayer);
        near(5.0) => key(at(1.0));
        near(5.5) => key(at(2.0));
        near(-1e9) => key(at(1.0));
        at(1.5) => Err(LookupError::KeyIndexNotInteger);
        (at(2.5), at(0.5), key_at(None, 1.0, fps24)) => (Ok(None), Ok(None), Ok(None));
        at(2.0).unwrap().map(|k| (k.time, k.value)) => Some((10.0 / 24.0, KeyValue::Vector(vec![2.0, 3.0])));
        at(1.0).unwrap().map(|k| k.value) => Some(KeyValue::Scalar(1.0));
        (m_at(Q::Name("")), m_at(Q::Name("b")), m_at(Q::Name("B"))) => (Some(2), Some(1), None);
        (m_at(Q::Number(3.7)), m_at(Q::Number(-0.5)), m_at(Q::Number(f64::NAN))) => (Some(3), None, None);
        (m_near(7.0), m_near(5.0), m_near(f64::INFINITY)) => (Some(1), Some(2), Some(2));
        ml.nearest_key_seconds(0.25, fps24).map(|m| m.index) => Some(3);
    }
}

#[test]
fn refusals_are_typed() {
    let long = "x".repeat(MAX_NAME_BYTES + 1);
    let named = |n: &str| LayerRecord {
        name: Some(n.into()),
        layer_uid: None,
        controls: vec![],
    };
    let busy = LayerRecord {
        controls: vec![ctl(None); MAX_CONTROLS + 1],
        ..named("a")
    };
    let layers = |v: Vec<LayerRecord>| LayerTable::new(v).err();
    let keys = |v: Vec<KeyRecord>| KeyTrack::new(v).err();
    let markers = |v: Vec<MarkerRecord>| MarkerList::new(v).err();
    let t = LayerTable::new(vec![named("a")]).unwrap();
    let (tr, fps) = (
        KeyTrack::new(vec![kr(0.0, &[1.0])]).unwrap(),
        Fps::new(24.0).unwrap(),
    );
    let ml = MarkerList::new(vec![mk(0.0, None)]).unwrap();
    eq! {
        layers(vec![named(&long[1..]); MAX_LAYERS]) => None;
        layers(vec![LayerRecord { controls: vec![ctl(None); MAX_CONTROLS], ..named("a") }]) => None;
        keys(vec![kr(0.0, &[0.0; MAX_VALUE_DIMS]); MAX_KEYS]) => None;
        markers(vec![mk(0.0, Some(&long[1..])); MAX_MARKERS]) => None;
        layers(vec![named(&long)]) => Some(R::NameTooLong);
        layers(vec![named("a"); MAX_LAYERS + 1]) => Some(R::TooManyLayers);
        layers(vec![busy]) => Some(R::TooManyControls);
        t.find_by_ref(&long) => Err(R::NameTooLong);
        t.find_by_uid(&long) => Err(R::NameTooLong);
        t.layer_control("a", Some(&long)) => Err(LookupError::Refused(R::NameTooLong));
        normalize_control_name(Some(&long)) => Err(R::NameTooLong);
        keys(vec![kr(0.0, &[0.0]); MAX_KEYS + 1]) => Some(R::TooManyKeys);
        keys(vec![kr(0.0, &[0.0; MAX_VALUE_DIMS + 1])]) => Some(R::TooManyValueDims);
        keys(vec![kr(f64::NAN, &[])]) => Some(R::NonFiniteRecord);
        keys(vec![kr(0.0, &[f64::INFINITY])]) => Some(R::NonFiniteRecord);
        markers(vec![mk(0.0, None); MAX_MARKERS + 1]) => Some(R::TooManyMarkers);
        markers(vec![mk(f64::NAN, None)]) => Some(R::NonFiniteRecord);
        markers(vec![mk(0.0, Some(&long))]) => Some(R::NameTooLong);
        ml.at(Q::Name(&long), fps) => Err(R::NameTooLong);
    }
    for bad in [0.0, -24.0, -0.0, f64::NAN, f64::INFINITY] {
        assert_eq!(Fps::new(bad), Err(R::InvalidFps));
    }
    for q in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(key_at(Some(&tr), q, fps), Err(R::NonFiniteQuery.into()));
    }
    let near = |q: f64| nearest_key_at_frame(Some(&tr), q, fps).map(|k| k.map(|k| k.index));
    eq! { near(f64::NAN) => Err(R::NonFiniteQuery.into()); near(f64::INFINITY) => Ok(Some(1)); }
}
