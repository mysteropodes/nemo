//! Cross-row invariants over the serP/desP oracle (G14/G22/G13, leaf L01-C).
//!
//! Each invariant states, over every one of the 60 cases, a rule the baseline follows: a stroke flag is
//! authoritative, a pressure ribbon has no stroke channel, `serP` never invents an optional member, every stored
//! coordinate sits on the 1/1000 grid, a round trip is stable. They are written from the baseline source, not
//! copied from the oracle, so a damaged row fails them even when the damage is self-consistent.

use super::value::Value;
use super::Oracle;

fn truthy(v: &Value) -> bool {
    match v {
        Value::Undef | Value::Null => false,
        Value::Bool(b) => *b,
        Value::Num(x) => *x != 0.0 && !x.is_nan(),
        Value::Str(s) => !s.is_empty(),
        _ => true,
    }
}

pub(crate) fn js_round(v: f64) -> f64 {
    let f = v.floor();
    if v - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

fn num(x: f64) -> Value {
    Value::Num(x)
}

fn must(cond: bool, id: &str, what: &str) -> Result<(), String> {
    if cond {
        Ok(())
    } else {
        Err(format!("case {}: {}", id, what))
    }
}

fn each(o: &Oracle, f: fn(&str, &Value) -> Result<(), String>) -> Result<(), String> {
    for (id, case) in &o.cases {
        f(id, case)?;
    }
    Ok(())
}

fn has_key(list: &Value, key: &str) -> bool {
    list.items().iter().any(|k| k.as_str() == Some(key))
}

/// `desP(serP(desP(d)))` serialises to the same dictionary again: storing a normalised stroke is stable.
pub(crate) fn inv_round_trip_is_stable(o: &Oracle) -> Result<(), String> {
    each(o, |id, c| {
        must(
            c.get("out2").same(c.get("out")),
            id,
            "out2 differs from out",
        )
    })
}

/// The anchor count survives (nothing is dropped or added) and closure is independent of the segments.
pub(crate) fn inv_counts_and_closure(o: &Oracle) -> Result<(), String> {
    each(o, |id, c| {
        let n = c.at("in.segments").items().len() as f64;
        must(
            c.at("paper.segCount").same(&num(n)),
            id,
            "paper segment count",
        )?;
        must(
            c.at("out.segments").items().len() as f64 == n,
            id,
            "out segment count",
        )?;
        must(
            c.at("out2.segments").items().len() as f64 == n,
            id,
            "out2 segment count",
        )?;
        let closed = Value::Bool(c.at("in.closed").same(&Value::Bool(true)));
        must(
            c.at("out.closed").same(&closed),
            id,
            "out.closed is not in.closed === true",
        )?;
        must(
            c.at("paper.closed").same(&closed),
            id,
            "paper.closed is not in.closed === true",
        )
    })
}

/// `hasRealStroke` is authoritative: false input restores no stroke, and serP reports a stroke only for a
/// non-ribbon item that really has one; the white fallback is written but never counts.
pub(crate) fn inv_real_stroke_flag(o: &Oracle) -> Result<(), String> {
    each(o, |id, c| {
        let is_vb = c.at("in.isVectorBrush").same(&Value::Bool(true));
        let paper_none = c.at("paper.strokeColor").same(&Value::Null);
        if c.at("in.hasRealStroke").same(&Value::Bool(false)) {
            must(paper_none, id, "hasRealStroke false must restore no stroke")?;
        }
        let has = Value::Bool(!is_vb && !paper_none);
        must(
            c.at("out.hasRealStroke").same(&has),
            id,
            "out.hasRealStroke is not 'a real stroke exists'",
        )?;
        if !is_vb && paper_none && c.at("in.bitmapBrushSpec").is_undef() {
            let white = Value::Str("#ffffff".to_string());
            must(
                c.at("out.strokeColor").same(&white),
                id,
                "fallback stroke colour is not #ffffff",
            )?;
        }
        Ok(())
    })
}

/// A pressure ribbon has no stroke channel; its centerline, width profile, intent and base travel by value;
/// non-ribbon strokes drop all of them.
pub(crate) fn inv_vector_brush_rules(o: &Oracle) -> Result<(), String> {
    each(o, |id, c| {
        let ribbon = [
            "isVectorBrush",
            "isFillShape",
            "centerSegments",
            "widthProfile",
            "strokeProfile",
            "profileBase",
        ];
        if !c.at("in.isVectorBrush").same(&Value::Bool(true)) {
            for k in ribbon {
                must(
                    c.at("out").get(k).is_undef(),
                    id,
                    "non-ribbon stroke kept a ribbon member",
                )?;
                must(
                    !has_key(c.at("paper.dataKeys"), k),
                    id,
                    "non-ribbon item stamped a ribbon member",
                )?;
            }
            return Ok(());
        }
        must(
            c.at("out.isVectorBrush").same(&Value::Bool(true)),
            id,
            "ribbon flag lost",
        )?;
        must(
            c.at("out.strokeColor").same(&Value::Null),
            id,
            "ribbon strokeColor must be null",
        )?;
        must(
            c.at("out.hasRealStroke").same(&Value::Bool(false)),
            id,
            "ribbon hasRealStroke must be false",
        )?;
        let fill_shape = truthy(c.at("in.isFillShape"));
        must(
            c.at("out.isFillShape").is_undef() != fill_shape,
            id,
            "isFillShape handling",
        )?;
        if !fill_shape {
            must(
                c.at("paper.strokeColor").same(&Value::Null),
                id,
                "keyline must null the Paper stroke",
            )?;
        } else if !c.at("in.hasRealStroke").same(&Value::Bool(false)) {
            must(
                c.at("paper.strokeColor").same(c.at("in.strokeColor")),
                id,
                "fill-shape keeps its Paper stroke",
            )?;
        }
        for k in [
            "centerSegments",
            "widthProfile",
            "strokeProfile",
            "profileBase",
        ] {
            let kept = truthy(c.at("in").get(k));
            must(
                c.at("out").get(k).is_undef() != kept,
                id,
                "pressure member presence follows truthiness",
            )?;
            if kept {
                must(
                    c.at("out").get(k).same(c.at("in").get(k)),
                    id,
                    "pressure member changed by value",
                )?;
            }
        }
        for (k, flag) in [
            ("centerSegments", "paper.centerSegmentsSameRef"),
            ("widthProfile", "paper.widthProfileSameRef"),
        ] {
            let same = Value::Bool(truthy(c.at("in").get(k)));
            must(
                c.at(flag).same(&same),
                id,
                "pressure array is carried by reference",
            )?;
        }
        Ok(())
    })
}

/// width, cap, join, miter limit, dash, opacity (with the `op` argument) and fill follow the `desP` defaults.
pub(crate) fn inv_stroke_style_rules(o: &Oracle) -> Result<(), String> {
    each(o, |id, c| {
        let width = if truthy(c.at("in.strokeWidth")) {
            c.at("in.strokeWidth").clone()
        } else {
            num(3.0)
        };
        must(
            c.at("paper.strokeWidth").same(&width),
            id,
            "width: 0/absent becomes 3",
        )?;
        must(c.at("out.strokeWidth").same(&width), id, "out width")?;
        for (k, p) in [("strokeCap", "round"), ("strokeJoin", "round")] {
            let want = match c.at("in").get(k) {
                Value::Str(s) => Value::Str(s.clone()),
                _ => Value::Str(p.to_string()),
            };
            must(
                c.at("paper").get(k).same(&want),
                id,
                "cap/join default is round",
            )?;
            must(c.at("out").get(k).same(&want), id, "out cap/join")?;
        }
        let miter = if c.at("in.miterLimit").is_undef() {
            num(10.0)
        } else {
            c.at("in.miterLimit").clone()
        };
        must(
            c.at("paper.miterLimit").same(&miter),
            id,
            "miter limit: absent reads Paper's 10",
        )?;
        must(c.at("out.miterLimit").same(&miter), id, "out miter limit")?;
        let dashed = !c.at("in.dashArray").items().is_empty();
        let dash = if dashed {
            c.at("in.dashArray").clone()
        } else {
            Value::Arr(Vec::new())
        };
        must(
            c.at("paper.dashArray").same(&dash),
            id,
            "empty dash array is not applied",
        )?;
        must(
            c.at("out.dashArray").is_undef() != dashed,
            id,
            "dashArray only when non-empty",
        )?;
        let offset = if c.at("in.dashOffset").is_undef() {
            num(0.0)
        } else {
            c.at("in.dashOffset").clone()
        };
        must(c.at("paper.dashOffset").same(&offset), id, "dash offset")?;
        must(c.at("out.dashOffset").same(&offset), id, "out dash offset")?;
        let opacity = match (c.get("op"), c.at("in.opacity")) {
            (Value::Num(op), _) => num(*op),
            (_, Value::Undef) => num(1.0),
            (_, stored) => stored.clone(),
        };
        must(
            c.at("paper.opacity").same(&opacity),
            id,
            "opacity: op overrides, absent is 1, 0 stays 0",
        )?;
        must(c.at("out.opacity").same(&opacity), id, "out opacity")?;
        match c.at("in.fillColor") {
            Value::Str(s) if !s.is_empty() => {
                must(
                    c.at("paper.fillColor").as_str().is_some(),
                    id,
                    "a colour string paints a fill",
                )?;
                must(
                    c.at("out.fillColor").same(c.at("paper.fillColor")),
                    id,
                    "out fill is the Paper fill",
                )
            }
            _ => {
                must(
                    c.at("paper.fillColor").same(&Value::Null),
                    id,
                    "no colour string restores no fill",
                )?;
                must(
                    c.at("out.fillColor").same(&Value::Null),
                    id,
                    "no fill reads back null",
                )
            }
        }
    })
}

/// `serP` returns the always-present members, and every other member only when the input carried it, unchanged.
pub(crate) fn inv_nothing_invented(o: &Oracle) -> Result<(), String> {
    each(o, |id, c| {
        const ALWAYS: [&str; 11] = [
            "segments",
            "closed",
            "strokeColor",
            "hasRealStroke",
            "strokeWidth",
            "strokeCap",
            "strokeJoin",
            "miterLimit",
            "fillColor",
            "opacity",
            "dashOffset",
        ];
        for part in ["out", "out2"] {
            for k in c.get(part).keys() {
                if ALWAYS.contains(&k) {
                    continue;
                }
                must(
                    !c.at("in").get(k).is_undef(),
                    id,
                    "serP returned a member the input did not carry",
                )?;
                must(
                    c.get(part).get(k).same(c.at("in").get(k)),
                    id,
                    "optional member changed",
                )?;
            }
        }
        let carried = c.at("in.paramShape");
        let flag = Value::Bool(truthy(carried));
        must(
            c.at("paper.paramShapeSameRef").same(&flag),
            id,
            "paramShape is stamped by reference",
        )?;
        must(
            c.at("out.paramShape").is_undef() != truthy(carried),
            id,
            "paramShape presence",
        )
    })
}

/// Every stored coordinate (point and both handles) is a multiple of 1/1000 as `_r3` writes it, `-0` included.
pub(crate) fn inv_coordinates_snap(o: &Oracle) -> Result<(), String> {
    each(o, |id, c| {
        for part in ["out", "out2"] {
            for seg in c.at(&format!("{}.segments", part)).items() {
                for k in ["point", "handleIn", "handleOut"] {
                    for x in seg.get(k).items() {
                        let v = match x.as_f64() {
                            Some(v) => v,
                            None => return Err(format!("case {}: non-numeric coordinate", id)),
                        };
                        let snapped = js_round(v * 1000.0) / 1000.0;
                        must(
                            snapped.to_bits() == v.to_bits(),
                            id,
                            "coordinate is not on the 1/1000 grid",
                        )?;
                    }
                }
            }
        }
        Ok(())
    })
}

pub(crate) const INVARIANTS: &[(&str, fn(&Oracle) -> Result<(), String>)] = &[
    ("round trip stable", inv_round_trip_is_stable),
    ("counts and closure", inv_counts_and_closure),
    ("real stroke flag", inv_real_stroke_flag),
    ("vector brush rules", inv_vector_brush_rules),
    ("stroke style rules", inv_stroke_style_rules),
    ("nothing invented", inv_nothing_invented),
    ("coordinates snap", inv_coordinates_snap),
];

pub(crate) fn check_all_invariants(o: &Oracle) -> Result<(), String> {
    for (name, f) in INVARIANTS {
        f(o).map_err(|e| format!("{}: {}", name, e))?;
    }
    Ok(())
}
