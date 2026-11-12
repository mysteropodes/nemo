//! Bitmap tween recipes are native transient work, never persisted client-supplied pixels.
pub(crate) use crate::object_document::tween::BitmapTweenBake;
use crate::object_document::{
    BitmapBrushFields, ObjectRecord, ObjectTarget, Presence, RasterEntry,
};
use crate::tween_kernels::*;
use serde_json::json;
pub(super) fn bitmap_fields(r: &ObjectRecord) -> Option<&BitmapBrushFields> {
    r.vector_ext.value().and_then(|v| v.bitmap_brush.value())
}
pub(super) struct PairBitmap {
    fields: BitmapBrushFields,
    side: i8,
    seed: u32,
}
impl PairBitmap {
    pub(super) fn new(a: &ObjectRecord, b: &ObjectRecord, from: u32, mi: i32) -> Option<Self> {
        let (aa, bb) = (bitmap_fields(a), bitmap_fields(b));
        let mut fields = aa.or(bb)?.clone();
        fields.tween_generated = Presence::Present(true);
        fields.pre_texture_stroke = Presence::Missing;
        fields.pre_texture_opacity = Presence::Missing;
        fields.bitmap_pressure_profile = Presence::Missing;
        Some(Self {
            fields,
            side: match (aa, bb) {
                (Some(_), Some(_)) => 0,
                (Some(_), _) => -1,
                _ => 1,
            },
            seed: from
                .wrapping_mul(7919)
                .wrapping_add((mi as u32).wrapping_mul(131))
                .wrapping_add(1),
        })
    }
    pub(super) fn emit(
        &self,
        anchor: &mut ObjectRecord,
        t: f64,
    ) -> R<Option<(RasterEntry, BitmapTweenBake)>> {
        let mut ext = anchor.vector_ext.value().cloned().unwrap_or_default();
        ext.bitmap_brush = Presence::Present(self.fields.clone());
        anchor.vector_ext = Presence::Present(ext);
        let mul = match self.side {
            -1 => 1. - t,
            1 => t,
            _ => 1.,
        };
        if mul <= 0.02 {
            return Ok(None);
        }
        let mut target = anchor.target.clone();
        target.stroke_id.push_str("_bitmap");
        let opacity = self.fields.bitmap_brush_spec.opacity.as_f64().unwrap() * mul;
        let item=serde_json::from_value(json!({"assetId":"native-tween-pending","x":0,"y":0,"width":1,"height":1,"opacity":opacity,"isBrushTextureCopy":true,"isBitmapBrush":true,"brushGroupId":self.fields.brush_group_id})).map_err(|_|crate::request_receipts::DispatchErrorCode::InvalidRequest)?;
        let mut fields = self.fields.clone();
        fields.bitmap_brush_spec.seed = self.seed.into();
        fields.bitmap_brush_spec.pressure = false;
        Ok(Some((
            RasterEntry {
                target: target.clone(),
                item,
            },
            BitmapTweenBake {
                target,
                anchor: anchor.clone(),
                fields,
                opacity,
            },
        )))
    }
}
