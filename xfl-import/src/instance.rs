//! Scene layers that only place one symbol, as component instance layers.
//!
//! The symbol is a component (`Ctx::component`); the layer keeps `symbolId`,
//! its outer keyframes (a `componentFrame` pick where the shown frame is
//! fixed, `blankOverride` where the layer is empty) and, for a playing
//! graphic, one play mode (`symPlayMode`, `symPlacedAt`). Read against the
//! baseline `resolveSymbolFrameIdx` and `getEffectiveStrokes` (app.js).
//!
//! The instance transform becomes layer Motion. The baseline poses a layer as
//! `p' = pivot + R(rotation)·S(scale)·(p − pivot) + position`, the pivot being
//! the centre of the content's curve bounds plus `anchor` (`computeMotionMat`,
//! `transformSegments` in motion.js, `motionPivot` in engine-bridge.js). An
//! XFL matrix without skew is `R·S` too, so with the pivot held on the tween's
//! transformation point (anchor = point − bounds centre, re-keyed wherever the
//! content bounds change), position is the point's travel and the eased
//! Flash properties map onto eased keys one for one. A simple ease
//! `t + e·t·(1 − t)` is exactly the cubic with tangents (1, 1 + e) and
//! (1, 1 − e); a custom ease is exact when every inner anchor has symmetric
//! handles (a waypoint carries one tangent). Anything else is baked, one
//! linear key per frame.

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use std::rc::Rc;

use serde_json::{json, Map, Value};

use crate::convert::{Ctx, MAX_FRAMES};
use crate::draw::{span_at, symbol_frame};
use crate::ease::{check_custom_ease, Ease};
use crate::model::*;
use crate::tween::{ClassicTween, Decomposed};

/// Below this a shear left by matrix rounding is dropped (radians).
const SKEW_EPS: f64 = 1e-4;

/// Component ids: `xfl-sym-` and a slug of the library name, numbered on a
/// collision, assigned in library order (stable for a given file).
pub(crate) fn symbol_ids(symbols: &[LibrarySymbol]) -> HashMap<&str, String> {
    let (mut used, mut out) = (HashSet::new(), HashMap::new());
    for s in symbols {
        let mut slug = String::new();
        for ch in s.name.chars() {
            if ch.is_ascii_alphanumeric() {
                slug.push(ch.to_ascii_lowercase());
            } else if !slug.is_empty() && !slug.ends_with('-') {
                slug.push('-');
            }
        }
        let slug = slug.trim_end_matches('-');
        let base = format!("xfl-sym-{}", if slug.is_empty() { "symbol" } else { slug });
        let (mut id, mut n) = (base.clone(), 2);
        while !used.insert(id.clone()) {
            id = format!("{base}-{n}");
            n += 1;
        }
        out.entry(s.name.as_str()).or_insert(id);
    }
    out
}

/// A Flash ease as Motion `curvePoints`, when one can be exact.
pub(crate) fn curve_points(ease: &Ease) -> Option<Value> {
    let wp = |p: Point, t: Point| json!({"x": p.x, "y": p.y, "tx": t.x, "ty": t.y});
    match ease {
        Ease::Simple { acceleration } => {
            let e = (acceleration / 100.0).clamp(-1.0, 1.0);
            let (o, i) = (Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 });
            Some(json!([
                wp(o, Point { x: 1.0, y: 1.0 + e }),
                wp(i, Point { x: 1.0, y: 1.0 - e })
            ]))
        }
        Ease::Custom { points } => {
            let n = points.len();
            if check_custom_ease(points) != Ok(true) || points[0].x != 0.0 || points[n - 1].x != 1.0
            {
                return None;
            }
            if points.windows(4).step_by(3).any(|s| s[3].x <= s[0].x) {
                return None;
            }
            let tangent = |a: Point, b: Point| Point {
                x: 3.0 * (b.x - a.x),
                y: 3.0 * (b.y - a.y),
            };
            let mut out = Vec::new();
            for k in (0..n).step_by(3) {
                let p = points[k];
                let leave = (k + 1 < n).then(|| tangent(p, points[k + 1]));
                let arrive = (k > 0).then(|| tangent(points[k - 1], p));
                let t = match (leave, arrive) {
                    (Some(l), Some(a)) if (l.x - a.x).abs() > 1e-9 || (l.y - a.y).abs() > 1e-9 => {
                        return None
                    }
                    (Some(t), _) | (None, Some(t)) => t,
                    (None, None) => return None,
                };
                out.push(wp(p, t));
            }
            Some(Value::Array(out))
        }
    }
}

/// One Motion key: frame, value, and the curve leaving it (`None` holds).
type Key = (u32, Vec<f64>, Option<Value>);

/// A track as `motion` keys, a `motionStatic` value, or nothing when neutral.
fn track(keys: &[Key], neutral: &[f64]) -> (Option<Value>, Option<Value>) {
    let Some(first) = keys.first() else {
        return (None, None);
    };
    if keys.iter().all(|k| k.1 == first.1) {
        return (None, (first.1 != neutral).then(|| json!(first.1)));
    }
    let list: Vec<Value> = keys
        .iter()
        .map(|(f, v, curve)| match curve {
            Some(c) => json!({"frame": f, "v": v, "curvePoints": c}),
            None => json!({"frame": f, "v": v, "hold": true}),
        })
        .collect();
    (Some(json!({ "keys": list })), None)
}

fn flat(cx: &mut Ctx, why: &str, loc: &str) -> Option<Map<String, Value>> {
    cx.report
        .skip(format!("symbol layer flattened: {why}"), loc);
    None
}

/// Layer `index` of the scene as an instance layer, or `None` to flatten it.
pub(crate) fn instance_layer(
    scene: &Timeline,
    index: usize,
    total: u32,
    cx: &mut Ctx,
) -> Option<Map<String, Value>> {
    let layer = &scene.layers[index];
    let loc = layer.name.as_str();
    let mut content: Vec<(&Frame, &SymbolInstance)> = Vec::new();
    for span in layer.frames.iter().filter(|s| s.index < total) {
        match span.elements.as_slice() {
            [] => {}
            [Element::SymbolInstance(i)] => content.push((span, i)),
            _ => return None,
        }
    }
    let lib = content.first()?.1.library_item.as_str();
    if content.iter().any(|c| c.1.library_item != lib) {
        return flat(cx, "it places several symbols", loc);
    }
    let sym = *cx.symbols.get(lib)?;
    let len = cx.frame_count(sym);
    if len > MAX_FRAMES {
        return flat(cx, "symbol longer than 1000000 frames", loc);
    }
    let len = len.max(1);
    let end = |s: &Frame| s.index.saturating_add(s.duration.max(1)).min(total);
    // Frame selection: a fixed pick, or the one play mode of the layer.
    let mut picks: Vec<Option<u32>> = Vec::new();
    let mut play: Option<(bool, i64)> = None;
    for (s, i) in &content {
        let graphic = i.symbol_type == SymbolType::Graphic;
        if !graphic {
            cx.report
                .skip("movie clip or button drawn at its first frame", loc);
        }
        let first = symbol_frame(i, 0, len);
        let once = i.loop_mode == "play once";
        if !graphic
            || i.loop_mode == "single frame"
            || len == 1
            || end(s) - s.index == 1
            || (once && first == len - 1)
        {
            picks.push(Some(first));
            continue;
        }
        play.get_or_insert((once, i64::from(s.index) - i64::from(first)));
        picks.push(None);
    }
    let shown = |k: usize, f: u32| -> u32 {
        match (picks[k], play) {
            (Some(p), _) => p,
            (None, Some((once, placed))) => {
                let e = (i64::from(f) - placed).max(0);
                (if once {
                    e.min(i64::from(len) - 1)
                } else {
                    e % i64::from(len)
                }) as u32
            }
            (None, None) => 0,
        }
    };
    for (k, (s, i)) in content.iter().enumerate() {
        if picks[k].is_none()
            && (s.index..end(s)).any(|f| shown(k, f) != symbol_frame(i, f - s.index, len))
        {
            return flat(cx, "graphic playback that one play mode cannot hold", loc);
        }
    }
    let tweens: HashMap<u32, Rc<ClassicTween>> = cx.tweens_of(scene, index).clone();
    let pivot = content
        .iter()
        .find_map(|c| tweens.get(&c.0.index))
        .map_or(content[0].1.transformation_point, |t| t.pivot);
    let moved =
        |t: &ClassicTween| (t.pivot.x - pivot.x).abs() > 1e-9 || (t.pivot.y - pivot.y).abs() > 1e-9;
    if content
        .iter()
        .filter_map(|c| tweens.get(&c.0.index))
        .any(|t| moved(t))
    {
        return flat(
            cx,
            "classic tweens around different transformation points",
            loc,
        );
    }
    let skew = |d: &Decomposed| crate::guide::wrap(d.skew_x - d.skew_y).abs();
    let mut worst: f64 = 0.0;
    for (s, i) in &content {
        worst = worst.max(skew(&Decomposed::from_instance(&i.matrix, pivot)));
        if let Some(t) = tweens.get(&s.index) {
            worst = worst.max(skew(&t.from)).max(skew(&t.to));
        }
    }
    let mut out = Map::new();
    out.insert("symbolId".into(), json!(cx.ids_of(sym)));
    let mut tracks: [Vec<Key>; 5] = Default::default();
    if worst > SKEW_EPS {
        let m = content[0].1.matrix;
        let fixed = content
            .iter()
            .all(|c| c.1.matrix == m && !tweens.contains_key(&c.0.index));
        if !fixed {
            return flat(cx, "skewed symbol instance (Motion has no skew)", loc);
        }
        out.insert("symMatrix".into(), json!([m.a, m.b, m.c, m.d, m.tx, m.ty]));
        cx.component(sym);
        for (s, i) in &content {
            tracks[3].push((s.index, vec![100.0 * i.alpha], None));
        }
    } else {
        if worst > 1e-9 {
            cx.report.skip(
                "shear below 0.0001 rad left by matrix rounding (dropped)",
                loc,
            );
        }
        motion_keys(&content, &tweens, pivot, total, &mut tracks, cx, loc);
        let comp = cx.component(sym);
        let mut last: Option<Vec<f64>> = None;
        for (k, (s, _)) in content.iter().enumerate() {
            for f in s.index..end(s) {
                let Some(c) = comp.centers[shown(k, f) as usize] else {
                    continue;
                };
                let a = vec![pivot.x - c.x, pivot.y - c.y];
                if last.as_ref() != Some(&a) {
                    tracks[4].push((f, a.clone(), None));
                    last = Some(a);
                }
            }
        }
        let fixed_widths = comp.fixed_widths;
        // Without rotation or scale the pivot moves nothing.
        if tracks[1].iter().all(|k| k.1 == [0.0]) && tracks[2].iter().all(|k| k.1 == [100.0, 100.0])
        {
            tracks[4].clear();
        }
        let scaled = tracks[2].iter().any(|k| k.1 != [100.0, 100.0]);
        if scaled && fixed_widths {
            cx.report.skip(
                "fixed-width strokes inside a scaled component (Motion scales them)",
                loc,
            );
        }
        if tracks[2].iter().any(|k| k.1[0].abs() != k.1[1].abs()) {
            cx.report.skip(
                "stroke width under a non-uniform instance scale (mean of the two scales)",
                loc,
            );
        }
    }
    let (mut motion, mut statics) = (Map::new(), Map::new());
    let names = ["position", "rotation", "scale", "opacity", "anchor"];
    let neutral: [&[f64]; 5] = [&[0.0, 0.0], &[0.0], &[100.0, 100.0], &[100.0], &[0.0, 0.0]];
    for p in 0..5 {
        let (m, s) = track(&tracks[p], neutral[p]);
        if let Some(m) = m {
            cx.report.count("motionKeys", tracks[p].len() as u64);
            motion.insert(names[p].into(), m);
        }
        if let Some(s) = s {
            statics.insert(names[p].into(), s);
        }
    }
    for (key, map) in [("motion", motion), ("motionStatic", statics)] {
        if !map.is_empty() {
            out.insert(key.into(), Value::Object(map));
        }
    }
    if let Some((once, placed)) = play {
        if once {
            out.insert("symPlayMode".into(), json!("once"));
        }
        if placed != 0 {
            out.insert("symPlacedAt".into(), json!(placed));
        }
    }
    // Outer keyframes: one per XFL span, blank where nothing is placed.
    let at: HashMap<*const Frame, usize> = content
        .iter()
        .enumerate()
        .map(|(k, c)| (c.0 as *const Frame, k))
        .collect();
    let mut frames = vec![json!({"strokes": []}); total as usize];
    let mut previous = None;
    for f in 0..total {
        let span = span_at(layer, f).map(|s| s as *const Frame);
        if f == 0 || span != previous {
            let mut key = json!({"isKeyframe": true, "strokes": []});
            match span.and_then(|s| at.get(&s)) {
                Some(&k) => {
                    if let Some(p) = picks[k] {
                        key["componentFrame"] = json!(p);
                    }
                }
                None => key["blankOverride"] = json!(true),
            }
            frames[f as usize] = key;
            cx.report.count("keyframes", 1);
        }
        previous = span;
    }
    out.insert("frames".into(), Value::Array(frames));
    cx.report.count("instanceLayers", 1);
    Some(out)
}

/// Position, rotation, scale and opacity keys of the content spans, with the
/// pivot held on `pivot`. Rotation is unwrapped across keys.
fn motion_keys(
    content: &[(&Frame, &SymbolInstance)],
    tweens: &HashMap<u32, Rc<ClassicTween>>,
    pivot: Point,
    total: u32,
    tracks: &mut [Vec<Key>; 5],
    cx: &mut Ctx,
    loc: &str,
) {
    let values = |d: &Decomposed, alpha: f64, rot: f64| -> [Vec<f64>; 4] {
        [
            vec![d.x - pivot.x, d.y - pivot.y],
            vec![rot.to_degrees()],
            vec![100.0 * d.scale_x, 100.0 * d.scale_y],
            vec![100.0 * alpha],
        ]
    };
    let mut expected: Option<f64> = None;
    for (s, i) in content {
        let end = s.index.saturating_add(s.duration.max(1)).min(total);
        let unwrap =
            |raw: f64| expected.map_or(raw, |r| raw + 2.0 * PI * ((r - raw) / (2.0 * PI)).round());
        let Some(t) = tweens.get(&s.index) else {
            let d = Decomposed::from_instance(&i.matrix, pivot);
            let r = unwrap(d.skew_y);
            for (p, v) in values(&d, i.alpha, r).into_iter().enumerate() {
                tracks[p].push((s.index, v, None));
            }
            expected = Some(r);
            continue;
        };
        let (d0, a0) = t.sample(s.index);
        let r0 = unwrap(d0.skew_y);
        let turn = r0 - d0.skew_y;
        let bake_all = t.guide.is_some() || t.end_frame >= total;
        let eases = [
            &t.eases.position,
            &t.eases.rotation,
            &t.eases.scale,
            &t.eases.color,
        ];
        let start = values(&d0, a0, r0);
        let mut baked = false;
        for p in 0..4 {
            if p == 2 && !t.tween_scale {
                tracks[p].push((s.index, start[p].clone(), None));
                continue;
            }
            let curve = (!bake_all).then(|| curve_points(eases[p])).flatten();
            if let Some(c) = curve {
                tracks[p].push((s.index, start[p].clone(), Some(c)));
                continue;
            }
            let linear = curve_points(&Ease::Simple { acceleration: 0.0 });
            for f in s.index..end {
                let (d, a) = t.sample(f);
                let v = values(&d, a, d.skew_y + turn)[p].clone();
                tracks[p].push((f, v, linear.clone()));
            }
            baked = true;
        }
        if baked {
            let why = if t.guide.is_some() {
                "motion guide (baked into one Motion key per frame)"
            } else if t.end_frame >= total {
                "classic tween cut by the end of the scene (baked into one Motion key per frame)"
            } else {
                "custom ease Motion cannot hold (baked into one Motion key per frame)"
            };
            cx.report.skip(why, loc);
        }
        expected = Some(r0 + (t.sample(t.end_frame).0.skew_y - d0.skew_y));
    }
}
