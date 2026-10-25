//! Raster item value (G01 / L06-C): the persistent payload of one raster item
//! as the baseline `serR` / `desR` pair at fbe4b8a3 stores it, minus the bytes.
//!
//! The record REFERENCES an asset by its stable `AssetId`; it never embeds a
//! data URL, a source or a locator. Whether the asset is embedded or linked is
//! read from the project `AssetInventory` ([`RasterItem::source_form`]); a
//! linked asset's locator stays in the inventory and never reaches this record.
//!
//! Members (camelCase, the baseline dict names): `assetId`, `x`, `y` (the raster
//! CENTRE, Paper `position`), `width`, `height` (display size), `opacity`
//! (absent reads as the baseline `1`), `rotation` (degrees, absent reads as
//! `0`; `serR` omits it when zero) and the reserved passthrough tags
//! `isBitmapBrush`, `brushGroupId`, `isBrushTextureCopy` (lane 7) and `meshId`
//! (lane 3), preserved verbatim and never interpreted here. Absent, default
//! and explicit values stay distinct on a round trip, as does the spelling
//! class of every number (`64` is not `64.0`).
//!
//! Strict decode: object only, unknown or duplicate members refused, `null`
//! refused everywhere, width and height positive and finite, opacity within
//! `0..=1`, passthrough ids non-empty and at most 255 bytes without NUL. Group
//! membership (`groupId`) and the text block members (`isText`...) are other
//! records' members and are NOT accepted here. This value is not yet an
//! `ObjectRecord` family: the document/codec join waits for the generalized
//! record family (L06-L, depends on G14).
//!
//! Files: `object_raster_asset_geometry.rs` (the live-Raster readings and the
//! error type) and this record.

use crate::object_document::{object_deserialize, AssetId, AssetInventory, AssetKind, Presence};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Number;

#[path = "object_raster_asset_geometry.rs"]
pub(crate) mod geometry;

use geometry::{capture_geometry, ImageRect, RasterItemError, RasterSurface};

const MAX_TAG_BYTES: usize = 255;

/// How the referenced asset is held by the project, read from the inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RasterSourceForm {
    Embedded,
    Linked,
}

/// The reserved passthrough values of a live raster, with JS truthiness: `false`
/// and the empty string are what `serR` leaves out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RasterTags {
    pub(crate) is_bitmap_brush: bool,
    pub(crate) brush_group_id: String,
    pub(crate) is_brush_texture_copy: bool,
    pub(crate) mesh_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RasterItem {
    #[serde(serialize_with = "serialize_asset_id")]
    asset_id: AssetId,
    x: Number,
    y: Number,
    width: Number,
    height: Number,
    #[serde(skip_serializing_if = "Presence::missing")]
    opacity: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    rotation: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    is_bitmap_brush: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    brush_group_id: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    is_brush_texture_copy: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    mesh_id: Presence<String>,
}

fn serialize_asset_id<S: Serializer>(id: &AssetId, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(id.as_str())
}

struct RasterItemWire {
    asset_id: String,
    x: Number,
    y: Number,
    width: Number,
    height: Number,
    opacity: Presence<Number>,
    rotation: Presence<Number>,
    is_bitmap_brush: Presence<bool>,
    brush_group_id: Presence<String>,
    is_brush_texture_copy: Presence<bool>,
    mesh_id: Presence<String>,
}
object_deserialize!(RasterItemWire {
    asset_id: String, x: Number, y: Number, width: Number, height: Number,
    #[serde(default)] opacity: Presence<Number>,
    #[serde(default)] rotation: Presence<Number>,
    #[serde(default)] is_bitmap_brush: Presence<bool>,
    #[serde(default)] brush_group_id: Presence<String>,
    #[serde(default)] is_brush_texture_copy: Presence<bool>,
    #[serde(default)] mesh_id: Presence<String>
});

impl<'de> Deserialize<'de> for RasterItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = RasterItemWire::deserialize(deserializer)?;
        RasterItem::from_wire(wire).map_err(serde::de::Error::custom)
    }
}

fn number_of(value: f64) -> Result<Number, RasterItemError> {
    Number::from_f64(value).ok_or(RasterItemError::NonFinite)
}

fn f64_of(number: &Number) -> f64 {
    number.as_f64().unwrap_or(f64::NAN)
}

fn valid_tag(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TAG_BYTES && !value.contains('\0')
}

fn tag_ok(value: &Presence<String>) -> bool {
    value.value().is_none_or(|text| valid_tag(text))
}

impl RasterItem {
    fn from_wire(wire: RasterItemWire) -> Result<Self, RasterItemError> {
        let asset_id = AssetId::new(wire.asset_id).map_err(|_| RasterItemError::AssetId)?;
        let item = Self {
            asset_id,
            x: wire.x,
            y: wire.y,
            width: wire.width,
            height: wire.height,
            opacity: wire.opacity,
            rotation: wire.rotation,
            is_bitmap_brush: wire.is_bitmap_brush,
            brush_group_id: wire.brush_group_id,
            is_brush_texture_copy: wire.is_brush_texture_copy,
            mesh_id: wire.mesh_id,
        };
        item.validate()?;
        Ok(item)
    }

    fn validate(&self) -> Result<(), RasterItemError> {
        let (x, y) = (f64_of(&self.x), f64_of(&self.y));
        let (width, height) = (f64_of(&self.width), f64_of(&self.height));
        if ![x, y, width, height].iter().all(|v| v.is_finite()) {
            return Err(RasterItemError::NonFinite);
        }
        if width <= 0.0 || height <= 0.0 {
            return Err(RasterItemError::Extent);
        }
        if let Some(rotation) = self.rotation.value() {
            if !f64_of(rotation).is_finite() {
                return Err(RasterItemError::NonFinite);
            }
        }
        if let Some(opacity) = self.opacity.value() {
            let opacity = f64_of(opacity);
            if !opacity.is_finite() {
                return Err(RasterItemError::NonFinite);
            }
            if !(0.0..=1.0).contains(&opacity) {
                return Err(RasterItemError::Opacity);
            }
        }
        if !tag_ok(&self.brush_group_id) || !tag_ok(&self.mesh_id) {
            return Err(RasterItemError::Tag);
        }
        Ok(())
    }

    /// Strict decode of one record's JSON text (see the module notes).
    pub(crate) fn decode(text: &str) -> Result<Self, RasterItemError> {
        let wire: RasterItemWire =
            serde_json::from_str(text).map_err(|_| RasterItemError::Malformed)?;
        Self::from_wire(wire)
    }

    /// Canonical JSON text; member order is fixed, absent members stay absent.
    pub(crate) fn encode(&self) -> Result<String, RasterItemError> {
        serde_json::to_string(self).map_err(|_| RasterItemError::Malformed)
    }

    /// `serR` for a live raster: geometry, `opacity` (always written, `undefined`
    /// reads as 1), `rotation` only when non-zero and each passthrough tag only
    /// when truthy. The asset id is the stable reference the caller resolved.
    pub(crate) fn capture(
        asset_id: AssetId,
        surface: &RasterSurface,
        tags: &RasterTags,
    ) -> Result<Self, RasterItemError> {
        let geometry = capture_geometry(surface)?;
        let truthy = |flag: bool| {
            if flag {
                Presence::Present(true)
            } else {
                Presence::Missing
            }
        };
        let text = |value: &str| {
            if value.is_empty() {
                Presence::Missing
            } else {
                Presence::Present(value.to_string())
            }
        };
        let item = Self {
            asset_id,
            x: number_of(geometry.x)?,
            y: number_of(geometry.y)?,
            width: number_of(geometry.width)?,
            height: number_of(geometry.height)?,
            opacity: Presence::Present(number_of(surface.opacity.unwrap_or(1.0))?),
            rotation: match geometry.rotation {
                Some(rotation) => Presence::Present(number_of(rotation)?),
                None => Presence::Missing,
            },
            is_bitmap_brush: truthy(tags.is_bitmap_brush),
            brush_group_id: text(&tags.brush_group_id),
            is_brush_texture_copy: truthy(tags.is_brush_texture_copy),
            mesh_id: text(&tags.mesh_id),
        };
        item.validate()?;
        Ok(item)
    }

    pub(crate) fn asset_id(&self) -> &AssetId {
        &self.asset_id
    }
    /// The raster centre (`position`), not a top-left corner.
    pub(crate) fn center(&self) -> (f64, f64) {
        (f64_of(&self.x), f64_of(&self.y))
    }
    pub(crate) fn size(&self) -> (f64, f64) {
        (f64_of(&self.width), f64_of(&self.height))
    }
    /// Authored opacity; `None` when absent.
    pub(crate) fn opacity_authored(&self) -> Option<f64> {
        self.opacity.value().map(f64_of)
    }
    /// Effective opacity: absent reads as the baseline `1`.
    pub(crate) fn opacity(&self) -> f64 {
        self.opacity_authored().unwrap_or(1.0)
    }
    /// Authored rotation in degrees; `None` when absent.
    pub(crate) fn rotation_authored(&self) -> Option<f64> {
        self.rotation.value().map(f64_of)
    }
    /// Effective rotation: absent reads as `0`.
    pub(crate) fn rotation(&self) -> f64 {
        self.rotation_authored().unwrap_or(0.0)
    }
    pub(crate) fn is_bitmap_brush(&self) -> Option<bool> {
        self.is_bitmap_brush.value().copied()
    }
    pub(crate) fn brush_group_id(&self) -> Option<&str> {
        self.brush_group_id.value().map(String::as_str)
    }
    pub(crate) fn is_brush_texture_copy(&self) -> Option<bool> {
        self.is_brush_texture_copy.value().copied()
    }
    pub(crate) fn mesh_id(&self) -> Option<&str> {
        self.mesh_id.value().map(String::as_str)
    }

    /// The engine rect of this record: the stored centre shifted to a top-left
    /// origin, the stored size, and the stored rotation (absent is 0), exactly
    /// the rotated branch of `rasterImageRect`.
    pub(crate) fn display_rect(&self) -> ImageRect {
        let (x, y) = self.center();
        let (width, height) = self.size();
        ImageRect {
            x: x - width / 2.0,
            y: y - height / 2.0,
            width,
            height,
            rotation: self.rotation(),
        }
    }

    /// Selection clone only: the same immutable bitmap moves with its anchor; never a rebake.
    pub(crate) fn selection_companion_clone(
        &self,
        group: &str,
        offset: f64,
    ) -> Result<Self, RasterItemError> {
        if !offset.is_finite() {
            return Err(RasterItemError::NonFinite);
        }
        let mut next = self.clone();
        next.brush_group_id = Presence::Present(group.to_string());
        if offset != 0.0 {
            let (x, y) = self.center();
            next.x = Number::from_f64(x + offset).ok_or(RasterItemError::NonFinite)?;
            next.y = Number::from_f64(y + offset).ok_or(RasterItemError::NonFinite)?;
        }
        next.validate()?;
        Ok(next)
    }

    /// Resolve the reference against the project inventory. A missing asset and
    /// a non-image asset are typed refusals; the result says embedded or linked
    /// without exposing bytes or a locator.
    pub(crate) fn source_form(
        &self,
        inventory: &AssetInventory,
    ) -> Result<RasterSourceForm, RasterItemError> {
        let record = inventory
            .get(&self.asset_id)
            .ok_or(RasterItemError::MissingAsset)?;
        if record.kind() != AssetKind::Image {
            return Err(RasterItemError::NotImage);
        }
        Ok(if record.source().locator().is_some() {
            RasterSourceForm::Linked
        } else {
            RasterSourceForm::Embedded
        })
    }
}

#[cfg(test)]
#[path = "../tests/object_raster_asset_item_codec.rs"]
mod codec_tests;
#[cfg(test)]
#[path = "../tests/object_raster_asset_item_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_raster_asset_item_oracle.rs"]
pub(crate) mod oracle;
#[cfg(test)]
#[path = "../tests/object_raster_asset_item.rs"]
mod tests;
