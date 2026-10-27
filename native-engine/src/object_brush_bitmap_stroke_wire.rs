//! #2361: closed intent for one native bitmap-brush Draw stroke. The anchor record and its companion rectangle travel
//! in the request; the baked PNG is a scoped file only the host reads (the PNG stage), never request bytes.
use crate::commands::object_fill_owner::image_import::wire::ImageSource;
use crate::object_document::{ObjectRecord, VECTOR_FAMILY};
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

/// The companion raster's rectangle (centre and size, as `raster.position`/`raster.size`) and opacity.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Companion {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) opacity: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct BitmapStroke {
    pub(crate) document_snapshot_id: String,
    pub(crate) lifecycle_generation: u64,
    /// The anchor: an editable vector path carrying `vectorExt.bitmapBrush`, with an empty stroke id.
    pub(crate) record: ObjectRecord,
    pub(crate) companion: Companion,
    pub(crate) source: ImageSource,
}

/// The anchor's brush group, which pairs it with its companion.
pub(crate) fn brush_group(record: &ObjectRecord) -> Option<&str> {
    record
        .vector_ext
        .value()
        .and_then(|ext| ext.bitmap_brush.value())
        .map(|fields| fields.brush_group_id.as_str())
}

impl BitmapStroke {
    /// A complete anchor record and 256 segments fit 64 KiB; the PNG never does and is not here.
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, DispatchErrorCode> {
        if bytes.len() > 65_536 {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let parsed: Self =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let ImageSource::File { path } = &parsed.source;
        let c = &parsed.companion;
        let finite = |v: f64| v.is_finite() && v.abs() <= 1.0e12;
        let target = &parsed.record.target;
        if parsed.lifecycle_generation == 0
            || parsed.lifecycle_generation > 9_007_199_254_740_991
            || parsed.document_snapshot_id.is_empty()
            || parsed.document_snapshot_id.len() > 128
            || path.is_empty()
            || path.contains('\0')
            || parsed.record.family != VECTOR_FAMILY
            || !target.stroke_id.is_empty()
            || target.context_id != "scene-root"
            || brush_group(&parsed.record).is_none_or(str::is_empty)
            || !(finite(c.x) && finite(c.y) && finite(c.width) && finite(c.height))
            || c.width <= 0.0
            || c.height <= 0.0
            || !(0.0..=1.0).contains(&c.opacity)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        Ok(parsed)
    }
}
