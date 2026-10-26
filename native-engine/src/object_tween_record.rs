//! Persisted tween intent in the existing object document; generated cells remain ordinary records.
use crate::object_document::{object_deserialize, OrderCurvePoint, Presence};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TweenOverride {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) a_id: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) a_ids: Presence<Vec<String>>,
    pub(crate) b_id: String,
}
object_deserialize!(TweenOverride {
    #[serde(default)] a_id: Presence<String>, #[serde(default)] a_ids: Presence<Vec<String>>, b_id:String
});
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TweenSpan {
    pub(crate) from: u32,
    pub(crate) to: u32,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) easing: Presence<Vec<OrderCurvePoint>>,
    pub(crate) overrides: Vec<TweenOverride>,
    /// Missing: automatic. Present: only these A-side identities enter matching.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) manual_strokes: Presence<Vec<String>>,
}
object_deserialize!(TweenSpan {from:u32,to:u32,#[serde(default)] easing:Presence<Vec<OrderCurvePoint>>,
    overrides:Vec<TweenOverride>,#[serde(default)] manual_strokes:Presence<Vec<String>>});
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TweenOptions {
    pub(crate) resample_points: u32,
    pub(crate) step: u32,
    pub(crate) skip_manual: bool,
    pub(crate) easing: Vec<OrderCurvePoint>,
}
object_deserialize!(TweenOptions {resample_points:u32,step:u32,skip_manual:bool,easing:Vec<OrderCurvePoint>});
impl Default for TweenOptions {
    fn default() -> Self {
        Self {
            resample_points: 50,
            step: 1,
            skip_manual: true,
            easing: vec![],
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayerTweens {
    pub(crate) layer_uid: String,
    pub(crate) options: TweenOptions,
    pub(crate) spans: Vec<TweenSpan>,
}
object_deserialize!(LayerTweens {layer_uid:String,options:TweenOptions,spans:Vec<TweenSpan>});
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub(crate) struct TweenStore {
    pub(crate) layers: Vec<LayerTweens>,
}
object_deserialize!(TweenStore {layers:Vec<LayerTweens>});
fn identity(s: &str) -> bool {
    !s.is_empty() && s.len() <= 255 && !s.contains('\0')
}
pub(crate) fn valid_tween_curve(points: &[OrderCurvePoint]) -> bool {
    if points.len() > 256 {
        return false;
    }
    let mut prev = f64::NEG_INFINITY;
    points.iter().all(|p| {
        let (Some(x), Some(y)) = (p.x.as_f64(), p.y.as_f64()) else {
            return false;
        };
        let ok = x.is_finite()
            && y.is_finite()
            && (0. ..=1.).contains(&x)
            && x > prev
            && [&p.tx, &p.ty].iter().all(|n| {
                n.value()
                    .is_none_or(|n| n.as_f64().is_some_and(f64::is_finite))
            });
        prev = x;
        ok
    })
}
impl TweenOverride {
    fn valid(&self) -> bool {
        identity(&self.b_id)
            && match (self.a_id.value(), self.a_ids.value()) {
                (Some(a), None) => identity(a),
                (None, Some(a)) => {
                    let mut seen = HashSet::new();
                    !a.is_empty()
                        && a.len() <= 128
                        && a.iter().all(|s| identity(s) && seen.insert(s))
                }
                _ => false,
            }
    }
}
impl TweenOptions {
    pub(crate) fn valid(&self) -> bool {
        (2..=256).contains(&self.resample_points)
            && (1..=999).contains(&self.step)
            && valid_tween_curve(&self.easing)
    }
}
impl TweenSpan {
    pub(crate) fn valid(&self) -> bool {
        self.from < self.to
            && self.overrides.len() <= 128
            && self.overrides.iter().all(TweenOverride::valid)
            && self.easing.value().is_none_or(|p| valid_tween_curve(p))
            && self.manual_strokes.value().is_none_or(|list| {
                let mut seen = HashSet::new();
                list.len() <= 128 && list.iter().all(|s| identity(s) && seen.insert(s))
            })
    }
}
impl TweenStore {
    pub(crate) fn valid(&self) -> bool {
        let mut layers = HashSet::new();
        let mut spans = 0usize;
        self.layers.len() <= 4096
            && self.layers.iter().all(|l| {
                spans += l.spans.len();
                let mut keys = HashSet::new();
                identity(&l.layer_uid)
                    && layers.insert(&l.layer_uid)
                    && l.options.valid()
                    && spans <= 4096
                    && l.spans
                        .iter()
                        .all(|s| s.valid() && keys.insert((s.from, s.to)))
            })
    }
    pub(crate) fn layer(&self, uid: &str) -> Option<&LayerTweens> {
        self.layers.iter().find(|l| l.layer_uid == uid)
    }
    pub(crate) fn layer_mut(&mut self, uid: &str) -> &mut LayerTweens {
        let index = self
            .layers
            .iter()
            .position(|l| l.layer_uid == uid)
            .unwrap_or_else(|| {
                self.layers.push(LayerTweens {
                    layer_uid: uid.into(),
                    options: TweenOptions::default(),
                    spans: vec![],
                });
                self.layers.len() - 1
            });
        &mut self.layers[index]
    }
}

impl crate::object_document::ObjectDocument {
    /// Shared authored-cell presence for generation and immutable ghost selection.
    pub(crate) fn tween_frame_has_content(&self, layer: &str, frame: u32) -> bool {
        let in_cell = |t: &crate::object_document::ObjectTarget| {
            t.context_id == "scene-root"
                && t.layer_uid == layer
                && t.frame_scope.kind == crate::object_document::FrameScopeKind::Authored
                && t.frame_scope.frame == frame
        };
        self.objects.iter().any(|r| in_cell(&r.target))
            || self
                .items
                .value()
                .is_some_and(|i| i.raster_entries().iter().any(|r| in_cell(&r.target)))
    }
}

pub(crate) const MAX_TWEEN_BAKES: usize = 128;
/// Detached bake input; no resource or revision authority.
#[derive(Debug, Clone)]
pub(crate) struct BitmapTweenBake {
    pub(crate) target: crate::object_document::ObjectTarget,
    pub(crate) anchor: crate::object_document::ObjectRecord,
    pub(crate) fields: crate::object_document::BitmapBrushFields,
    pub(crate) opacity: f64,
}
