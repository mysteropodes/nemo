//! Match anchors only; resolve the existing preset catalog and re-stamp each generated frame.
use crate::object_document::VectorTextureFields;
use crate::object_document::{
    BrushPresetRecord, ObjectRecord, Presence, ProjectPresetCatalog, VectorExt,
};
use crate::tween_kernels::*;
use std::collections::BTreeMap;
pub(super) fn texture(r: &ObjectRecord) -> Option<&VectorTextureFields> {
    r.vector_ext.value().and_then(|v| v.texture.value())
}
pub(super) fn is_dab(r: &ObjectRecord) -> bool {
    texture(r).is_some_and(|t| t.is_copy.value() == Some(&true))
}
pub(super) struct PairTexture {
    preset: String,
    resolved: Option<BrushPresetRecord>,
    group: Presence<String>,
    color: String,
    side: i8,
    seed: u32,
}
impl PairTexture {
    pub(super) fn new(
        a: &ObjectRecord,
        b: &ObjectRecord,
        from: u32,
        mi: i32,
        custom: Option<&ProjectPresetCatalog>,
    ) -> Option<Self> {
        let ta = texture(a);
        let tb = texture(b);
        let ka = ta.and_then(|t| t.preset.value());
        let kb = tb.and_then(|t| t.preset.value());
        let preset = ka.or(kb)?.clone();
        static BUILTINS: std::sync::OnceLock<BTreeMap<String, BrushPresetRecord>> =
            std::sync::OnceLock::new();
        let builtins = BUILTINS.get_or_init(|| {
            serde_json::from_str(include_str!("tween_brush_presets.json"))
                .expect("pinned built-in brush records")
        });
        let resolved = if preset == "none" {
            None
        } else {
            builtins
                .get(&preset)
                .or_else(|| {
                    custom.and_then(|c| {
                        c.entries()
                            .iter()
                            .find(|(k, _)| k == &preset)
                            .map(|(_, p)| p)
                    })
                })
                .cloned()
        };
        let pre = |t: Option<&VectorTextureFields>| {
            t.and_then(|t| t.pre_stroke.value())
                .and_then(|s| s.clone())
                .filter(|s| !s.is_empty())
        };
        let color = pre(ta)
            .or_else(|| stroke(a).stroke_color)
            .or_else(|| pre(tb))
            .or_else(|| stroke(b).stroke_color)
            .unwrap_or_else(|| "#000000".into());
        Some(Self {
            preset,
            resolved,
            group: ta
                .and_then(|t| t.group_id.value())
                .or_else(|| tb.and_then(|t| t.group_id.value()))
                .cloned()
                .map(Presence::Present)
                .unwrap_or_default(),
            color,
            side: if ka.is_some() && kb.is_some() {
                0
            } else if ka.is_some() {
                -1
            } else {
                1
            },
            seed: from
                .wrapping_mul(7919)
                .wrapping_add((mi as u32).wrapping_mul(131))
                .wrapping_add(1),
        })
    }
    pub(super) fn emit(&self, anchor: &mut ObjectRecord, t: f64) -> R<Vec<ObjectRecord>> {
        let ext = anchor.vector_ext.value().cloned().unwrap_or_default();
        anchor.vector_ext = Presence::Present(VectorExt {
            texture: Presence::Present(VectorTextureFields {
                preset: Presence::Present(self.preset.clone()),
                group_id: self.group.clone(),
                ..Default::default()
            }),
            ..ext
        });
        let mul = match self.side {
            -1 => 1. - t,
            1 => t,
            _ => 1.,
        };
        let Some(preset) = self.resolved.as_ref().filter(|_| mul > 0.02) else {
            return Ok(vec![]);
        };
        let width = anchor
            .vector
            .value()
            .and_then(|v| v.stroke_width.as_f64())
            .filter(|&v| v != 0.)
            .unwrap_or(3.);
        let ink = solid_hex(&self.color)?;
        build_dabs(&stroke(anchor), preset, width, self.seed, mul)?
            .into_iter()
            .enumerate()
            .map(|(i, d)| {
                let mut r = anchor.clone();
                r.target.stroke_id = format!("{}_dab_{i}", anchor.target.stroke_id);
                r.geometry.segments = native_segments(&d.segments)?;
                r.geometry.closed = true;
                r.fill = Presence::Present(Some(ink.clone()));
                let mut body = default_body();
                body.opacity = number(d.opacity)?;
                r.vector = Presence::Present(body);
                r.effects = Presence::Missing;
                r.vector_ext = Presence::Present(VectorExt {
                    texture: Presence::Present(VectorTextureFields {
                        group_id: self.group.clone(),
                        is_copy: Presence::Present(true),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
                Ok(r)
            })
            .collect()
    }
}
pub(super) fn faded_dabs(
    source: &[ObjectRecord],
    anchor: &ObjectRecord,
    keep: f64,
    frame: u32,
) -> R<Vec<ObjectRecord>> {
    let Some(t) = texture(anchor).filter(|t| !t.preset.missing()) else {
        return Ok(vec![]);
    };
    let Some(group) = t.group_id.value() else {
        return Ok(vec![]);
    };
    let mut out = vec![];
    for r in source
        .iter()
        .filter(|r| is_dab(r) && texture(r).and_then(|t| t.group_id.value()) == Some(group))
    {
        let mut r = r.clone();
        r.target.frame_scope.frame = frame;
        if let Presence::Present(v) = &mut r.vector {
            v.opacity = number(v.opacity.as_f64().unwrap() * keep.clamp(0., 1.))?;
            if v.opacity.as_f64().unwrap() <= 0.02 {
                continue;
            }
        }
        out.push(r);
    }
    Ok(out)
}
