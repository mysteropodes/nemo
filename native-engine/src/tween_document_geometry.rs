//! Adapt admitted native records to pure tween geometry and back; no writable state here.
use super::*;
use crate::object_document::{
    ObjectPoint, ObjectRecord, ObjectSegment, Presence, SolidFill, VectorBody, VectorExt,
    VECTOR_FAMILY,
};
use crate::request_receipts::DispatchErrorCode;
use serde_json::Number;
pub(super) type R<T> = Result<T, DispatchErrorCode>;
pub(super) fn number(x: f64) -> R<Number> {
    Number::from_f64(x).ok_or(DispatchErrorCode::InvalidRequest)
}
fn point(p: &ObjectPoint) -> [f64; 2] {
    [p.x.as_f64().unwrap(), p.y.as_f64().unwrap()]
}
pub(super) fn native_point(p: [f64; 2]) -> R<ObjectPoint> {
    Ok(ObjectPoint {
        x: number(p[0])?,
        y: number(p[1])?,
    })
}
fn color(c: Option<&SolidFill>) -> Option<String> {
    c.map(crate::object_document::object_group_kernels::object_boolean_ops::stack::hex8)
}
pub(super) fn stroke(r: &ObjectRecord) -> StrokeIn {
    let pressure = r.vector_ext.value().and_then(|v| v.pressure.value());
    StrokeIn {
        segments: r
            .geometry
            .segments
            .iter()
            .map(|s| SegIn {
                point: point(&s.point),
                handle_in: point(&s.handle_in),
                handle_out: point(&s.handle_out),
            })
            .collect(),
        center_segments: pressure.and_then(|p| p.center_segments.value()).map(|cs| {
            cs.iter()
                .map(|s| CenterSegIn {
                    point: point(&s.point),
                    handle_in: point(&s.handle_in),
                    handle_out: point(&s.handle_out),
                    width: s.width.as_f64().unwrap(),
                })
                .collect()
        }),
        width_profile: pressure.and_then(|p| p.width_profile.value()).map(|p| {
            p.iter()
                .map(|p| WidthSample {
                    t: p.t.as_f64().unwrap(),
                    width: p.width.as_f64().unwrap(),
                })
                .collect()
        }),
        is_vector_brush: pressure.is_some(),
        closed: r.geometry.closed,
        stroke_color: color(r.vector.value().and_then(|v| v.solid_stroke())),
        fill_color: color(r.solid_fill()),
        has_real_stroke: r.vector.value().map(|v| v.has_real_stroke),
    }
}
pub(super) fn native_segments(s: &[SegOut]) -> R<Vec<ObjectSegment>> {
    s.iter()
        .map(|s| {
            Ok(ObjectSegment {
                point: native_point(s.point)?,
                handle_in: native_point(s.handle_in)?,
                handle_out: native_point(s.handle_out)?,
            })
        })
        .collect()
}
pub(super) fn poly_length(s: &StrokeIn) -> f64 {
    let points: Vec<_> = if is_vb(s) {
        s.center_segments
            .as_ref()
            .unwrap()
            .iter()
            .map(|p| p.point)
            .collect()
    } else {
        s.segments.iter().map(|p| p.point).collect()
    };
    points
        .windows(2)
        .map(|p| (p[1][0] - p[0][0]).hypot(p[1][1] - p[0][1]))
        .sum()
}
pub(super) fn blend_number(a: &Number, b: &Number, t: f64) -> R<Number> {
    let a = a.as_f64().unwrap();
    number(a + (b.as_f64().unwrap() - a) * t)
}
fn mix_presence(a: &Presence<Number>, b: &Presence<Number>, t: f64) -> R<Presence<Number>> {
    Ok(match (a.value(), b.value()) {
        (Some(a), Some(b)) => Presence::Present(blend_number(a, b, t)?),
        _ => {
            if t < 0.5 {
                a.clone()
            } else {
                b.clone()
            }
        }
    })
}
pub(super) fn default_body() -> VectorBody {
    VectorBody {
        stroke: Presence::Present(None),
        has_real_stroke: false,
        stroke_width: 3.into(),
        stroke_cap: "round".into(),
        stroke_join: "round".into(),
        miter_limit: Presence::Missing,
        dash_array: Presence::Missing,
        dash_offset: Presence::Missing,
        paint_order: Presence::Missing,
        opacity: 1.into(),
    }
}
/// Only baseline _carryTweenStyle fields ride a generated plain path. Parametric shapes and masks
/// are authored inputs, not identities of the newly computed cubic geometry.
pub(super) fn interpolate_record(
    a: &ObjectRecord,
    b: &ObjectRecord,
    segs: &[SegOut],
    t: f64,
    id: &str,
    frame: u32,
) -> R<ObjectRecord> {
    let near = if t < 0.5 { a } else { b };
    let mut out = near.clone();
    out.target.stroke_id = id.into();
    out.target.frame_scope.frame = frame;
    out.family = VECTOR_FAMILY.into();
    out.geometry.segments = native_segments(segs)?;
    out.geometry.closed = near.geometry.closed;
    let fallback = default_body();
    let av = a.vector.value().unwrap_or(&fallback);
    let bv = b.vector.value().unwrap_or(&fallback);
    let mut body = if t < 0.5 { av.clone() } else { bv.clone() };
    let sw = |v: &VectorBody| {
        let w = v.stroke_width.as_f64().unwrap();
        if w == 0. {
            3.
        } else {
            w
        }
    };
    body.stroke_width = number(sw(av) + (sw(bv) - sw(av)) * t)?;
    body.stroke_cap = av.stroke_cap.clone();
    body.stroke_join = av.stroke_join.clone();
    body.opacity = blend_number(&av.opacity, &bv.opacity, t)?;
    body.dash_offset = mix_presence(&av.dash_offset, &bv.dash_offset, t)?;
    body.miter_limit = mix_presence(&av.miter_limit, &bv.miter_limit, t)?;
    out.vector = Presence::Present(body);
    out.vector_ext = Presence::Missing;
    let ga = a.fill_gradient();
    let gb = b.fill_gradient();
    if ga.is_some() || gb.is_some() {
        let mut ext = VectorExt::default();
        let gradient = match (ga, gb) {
            (Some(a), Some(b)) if a.kind == b.kind => {
                let mut g = a.clone();
                for i in 0..2 {
                    g.from[i] = blend_number(&a.from[i], &b.from[i], t)?;
                    g.to[i] = blend_number(&a.to[i], &b.to[i], t)?;
                    g.stops[i].offset = blend_number(&a.stops[i].offset, &b.stops[i].offset, t)?;
                    g.stops[i].color = blend_hex(&a.stops[i].color, &b.stops[i].color, t);
                }
                Some(g)
            }
            _ => {
                if t < 0.5 {
                    ga.cloned()
                } else {
                    gb.cloned()
                }
            }
        };
        if let Some(g) = gradient {
            ext.fill_gradient = Presence::Present(g);
            out.vector_ext = Presence::Present(ext);
        }
    }
    Ok(out)
}
fn blend_hex(a: &str, b: &str, t: f64) -> String {
    let (Some(aa), Some(bb)) = (
        crate::object_document::bitmap_brush::rgba(a),
        crate::object_document::bitmap_brush::rgba(b),
    ) else {
        return if t < 0.5 { a.into() } else { b.into() };
    };
    let mut channels = [0u8; 4];
    for i in 0..4 {
        channels[i] = (aa[i] as f64 + (bb[i] as f64 - aa[i] as f64) * t)
            .round()
            .clamp(0., 255.) as u8;
    }
    let mut result = format!("#{:02x}{:02x}{:02x}", channels[0], channels[1], channels[2]);
    if channels[3] < 255 {
        result.push_str(&format!("{:02x}", channels[3]));
    }
    result
}

pub(super) fn solid_hex(s: &str) -> R<SolidFill> {
    let rgba =
        crate::object_document::bitmap_brush::rgba(s).ok_or(DispatchErrorCode::InvalidRequest)?;
    Ok(SolidFill {
        kind: "solid".into(),
        r: number(rgba[0] as f64 / 255.)?,
        g: number(rgba[1] as f64 / 255.)?,
        b: number(rgba[2] as f64 / 255.)?,
        a: number(rgba[3] as f64 / 255.)?,
    })
}
