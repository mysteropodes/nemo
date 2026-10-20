//! Project custom brush definitions only; execution remains with the brush provider.
//! Preserve catalog insertion order and native immutable revision ownership.
use super::{ProjectMetadata,ProjectMetadataError};
use crate::object_document::{object_deserialize,Presence};
use serde::{Deserialize,Serialize};
use serde::ser::SerializeMap;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug,Clone,PartialEq,Serialize)]
#[serde(rename_all="camelCase")]
struct StampSegment {
    point:[f64;2],
    #[serde(skip_serializing_if="Presence::missing")] handle_in:Presence<[f64;2]>,
    #[serde(skip_serializing_if="Presence::missing")] handle_out:Presence<[f64;2]>,
}
object_deserialize!(StampSegment {
    point:[f64;2],#[serde(default)] handle_in:Presence<[f64;2]>,#[serde(default)] handle_out:Presence<[f64;2]>
});
#[derive(Debug,Clone,PartialEq,Serialize)]
#[serde(rename_all="camelCase")]
struct CustomStamp {
    segments:Vec<StampSegment>,
    #[serde(skip_serializing_if="Presence::missing")] closed:Presence<bool>,
    #[serde(skip_serializing_if="Presence::missing")] point_count:Presence<f64>,
}
object_deserialize!(CustomStamp {
    segments:Vec<StampSegment>,#[serde(default)] closed:Presence<bool>,#[serde(default)] point_count:Presence<f64>
});
#[derive(Debug,Clone,Default,PartialEq,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct BrushPresetRecord {
    #[serde(skip_serializing_if="Presence::missing")] bristle_count:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] dash_gap:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] edge_noise:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] fixed_angle:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] grain:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] nib_size:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] noise:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] opacity:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] opacity_jitter:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] poly_sides:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] pressure_end:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] pressure_mid:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] pressure_start:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] rotation_jitter:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] roughness:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] roundness:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] scatter:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] scribble_angle_spread:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] scribble_count:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] scribble_len:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] scribble_len_jitter:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] scribble_spread:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] scribble_width:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] sharpness:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] size_jitter:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] space_jitter:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] spacing:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] tip_corner:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] value_jitter:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] saved_brush_size:Presence<f64>,
    #[serde(skip_serializing_if="Presence::missing")] rotation_mode:Presence<String>,
    #[serde(skip_serializing_if="Presence::missing")] scatter_distribution:Presence<String>,
    #[serde(skip_serializing_if="Presence::missing")] tip_shape:Presence<String>,
    #[serde(skip_serializing_if="Presence::missing")] label:Presence<String>,
    #[serde(skip_serializing_if="Presence::missing")] marker_tip:Presence<bool>,
    #[serde(skip_serializing_if="Presence::missing")] custom_stamp:Presence<CustomStamp>,
}
object_deserialize!(BrushPresetRecord {
    #[serde(default)] bristle_count:Presence<f64>,
    #[serde(default)] dash_gap:Presence<f64>,
    #[serde(default)] edge_noise:Presence<f64>,
    #[serde(default)] fixed_angle:Presence<f64>,
    #[serde(default)] grain:Presence<f64>,
    #[serde(default)] nib_size:Presence<f64>,
    #[serde(default)] noise:Presence<f64>,
    #[serde(default)] opacity:Presence<f64>,
    #[serde(default)] opacity_jitter:Presence<f64>,
    #[serde(default)] poly_sides:Presence<f64>,
    #[serde(default)] pressure_end:Presence<f64>,
    #[serde(default)] pressure_mid:Presence<f64>,
    #[serde(default)] pressure_start:Presence<f64>,
    #[serde(default)] rotation_jitter:Presence<f64>,
    #[serde(default)] roughness:Presence<f64>,
    #[serde(default)] roundness:Presence<f64>,
    #[serde(default)] scatter:Presence<f64>,
    #[serde(default)] scribble_angle_spread:Presence<f64>,
    #[serde(default)] scribble_count:Presence<f64>,
    #[serde(default)] scribble_len:Presence<f64>,
    #[serde(default)] scribble_len_jitter:Presence<f64>,
    #[serde(default)] scribble_spread:Presence<f64>,
    #[serde(default)] scribble_width:Presence<f64>,
    #[serde(default)] sharpness:Presence<f64>,
    #[serde(default)] size_jitter:Presence<f64>,
    #[serde(default)] space_jitter:Presence<f64>,
    #[serde(default)] spacing:Presence<f64>,
    #[serde(default)] tip_corner:Presence<f64>,
    #[serde(default)] value_jitter:Presence<f64>,
    #[serde(default)] saved_brush_size:Presence<f64>,
    #[serde(default)] rotation_mode:Presence<String>,
    #[serde(default)] scatter_distribution:Presence<String>,
    #[serde(default)] tip_shape:Presence<String>,
    #[serde(default)] label:Presence<String>,
    #[serde(default)] marker_tip:Presence<bool>,
    #[serde(default)] custom_stamp:Presence<CustomStamp>
});
impl BrushPresetRecord {
    fn valid(&self) -> bool {
        [&self.bristle_count,&self.dash_gap,&self.edge_noise,&self.fixed_angle,&self.grain,&self.nib_size,&self.noise,&self.opacity,&self.opacity_jitter,&self.poly_sides,&self.pressure_end,&self.pressure_mid,&self.pressure_start,&self.rotation_jitter,&self.roughness,&self.roundness,&self.scatter,&self.scribble_angle_spread,&self.scribble_count,&self.scribble_len,&self.scribble_len_jitter,&self.scribble_spread,&self.scribble_width,&self.sharpness,&self.size_jitter,&self.space_jitter,&self.spacing,&self.tip_corner,&self.value_jitter,&self.saved_brush_size].iter()
            .all(|v|v.value().is_none_or(|n|n.is_finite())) && self.custom_stamp.value().is_none_or(|stamp|
            stamp.point_count.value().is_none_or(|n|n.is_finite()) && stamp.segments.iter().all(|s|
                s.point.iter().all(|n|n.is_finite()) && [&s.handle_in,&s.handle_out].iter().all(|h|
                    h.value().is_none_or(|p|p.iter().all(|n|n.is_finite())))))
    }
}
#[derive(Debug,Clone,Default,PartialEq)]
pub(crate) struct ProjectPresetCatalog(Vec<(String,BrushPresetRecord)>);
impl ProjectPresetCatalog {
    pub(crate) fn entries(&self) -> &[(String,BrushPresetRecord)] { &self.0 }
    fn valid(&self) -> bool {
        let mut seen=HashSet::new();self.0.iter().all(|(id,value)|seen.insert(id.as_str()) && value.valid())
    }
}
impl Serialize for ProjectPresetCatalog {
    fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error> {
        let mut map=serializer.serialize_map(Some(self.0.len()))?;
        for (key,value) in &self.0 { map.serialize_entry(key,value)?; }map.end()
    }
}
impl<'de> Deserialize<'de> for ProjectPresetCatalog {
    fn deserialize<D:serde::Deserializer<'de>>(deserializer:D)->Result<Self,D::Error> {
        struct CatalogVisitor;
        impl<'de> serde::de::Visitor<'de> for CatalogVisitor {
            type Value=ProjectPresetCatalog;
            fn expecting(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { f.write_str("an ordered custom preset object") }
            fn visit_map<A:serde::de::MapAccess<'de>>(self,mut map:A)->Result<Self::Value,A::Error> {
                let mut entries=Vec::new();let mut seen=HashSet::new();
                while let Some((key,value))=map.next_entry::<String,BrushPresetRecord>()? {
                    if !seen.insert(key.clone()) { return Err(serde::de::Error::custom("duplicate custom preset id")); }
                    entries.push((key,value));
                }
                Ok(ProjectPresetCatalog(entries))
            }
        }
        deserializer.deserialize_map(CatalogVisitor)
    }
}
impl ProjectMetadata {
    pub(crate) fn with_custom_brush_presets(mut self,presets:Option<ProjectPresetCatalog>)->Result<Self,ProjectMetadataError> {
        if presets.as_ref().is_some_and(|p|!p.valid()) { return Err(ProjectMetadataError::InvalidPresets); }
        self.custom_brush_presets=presets.map(Arc::new);Ok(self)
    }
    pub(crate) fn custom_brush_presets(&self)->Option<&ProjectPresetCatalog> { self.custom_brush_presets.as_deref() }
}
#[cfg(test)]
#[path="../tests/project_metadata_presets.rs"]
mod tests;
