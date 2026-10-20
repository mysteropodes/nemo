//! Persistent authored animator deltas; no glyph copies or second document owner.
#[cfg(feature = "evaluation")]
use crate::evaluation::property_text_animator::{AnimatorColor, AnimatorProps};
use crate::object_document::{object_deserialize, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PropsRecord {
    #[serde(skip_serializing_if = "Presence::missing")]
    position: Presence<[Option<Number>; 2]>,
    #[serde(skip_serializing_if = "Presence::missing")]
    scale: Presence<[Option<Number>; 2]>,
    #[serde(skip_serializing_if = "Presence::missing")]
    rotation: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    opacity: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    fill_color: Presence<Vec<Option<Number>>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    stroke_color: Presence<Vec<Option<Number>>>,
}
object_deserialize!(PropsRecord {
    #[serde(default)] position: Presence<[Option<Number>;2]>,
    #[serde(default)] scale: Presence<[Option<Number>;2]>,
    #[serde(default)] rotation: Presence<Option<Number>>,
    #[serde(default)] opacity: Presence<Option<Number>>,
    #[serde(default)] fill_color: Presence<Vec<Option<Number>>>,
    #[serde(default)] stroke_color: Presence<Vec<Option<Number>>>
});
fn num(n: &Number) -> Result<f64, &'static str> {
    n.as_f64()
        .filter(|v| v.is_finite())
        .ok_or("nonfinite animator property")
}
fn scalar(v: &Presence<Option<Number>>) -> Result<Option<f64>, &'static str> {
    v.value().and_then(Option::as_ref).map(num).transpose()
}
fn pair(v: &Presence<[Option<Number>; 2]>) -> Result<Option<[Option<f64>; 2]>, &'static str> {
    v.value()
        .map(|v| {
            Ok([
                v[0].as_ref().map(num).transpose()?,
                v[1].as_ref().map(num).transpose()?,
            ])
        })
        .transpose()
}
#[cfg(feature = "evaluation")]
fn color(v: &Presence<Vec<Option<Number>>>) -> Result<Option<AnimatorColor>, &'static str> {
    v.value()
        .map(|v| {
            if !(v.len() == 3 || v.len() == 4) || v[..3].iter().any(Option::is_none) {
                return Err("invalid animator color");
            }
            Ok(AnimatorColor {
                rgb: [
                    num(v[0].as_ref().unwrap())?,
                    num(v[1].as_ref().unwrap())?,
                    num(v[2].as_ref().unwrap())?,
                ],
                alpha: v.get(3).and_then(Option::as_ref).map(num).transpose()?,
            })
        })
        .transpose()
}
impl PropsRecord {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        pair(&self.position)?;
        pair(&self.scale)?;
        scalar(&self.rotation)?;
        scalar(&self.opacity)?;
        for v in [&self.fill_color, &self.stroke_color]
            .into_iter()
            .filter_map(Presence::value)
        {
            if !(v.len() == 3 || v.len() == 4) || v[..3].iter().any(Option::is_none) {
                return Err("invalid animator color");
            }
            for n in v.iter().flatten() {
                num(n)?;
            }
        }
        Ok(())
    }
    #[cfg(feature = "evaluation")]
    pub(crate) fn resolve(&self) -> Result<AnimatorProps, &'static str> {
        Ok(AnimatorProps {
            position: pair(&self.position)?,
            scale: pair(&self.scale)?,
            rotation: scalar(&self.rotation)?,
            opacity: scalar(&self.opacity)?,
            fill_color: color(&self.fill_color)?,
            stroke_color: color(&self.stroke_color)?,
        })
    }
}
