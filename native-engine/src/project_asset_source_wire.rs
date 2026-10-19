//! Strict disjoint asset source preparation; shared PNG/font/linked rules preserved.
use super::{
    audio, object, payload, text, AssetKind, AssetSource, ProjectWireError, ProjectWireLimits,
    Source, MAX_LOCATOR,
};
use serde::Deserialize;
use serde_json::value::RawValue;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Embedded<'a> {
    #[serde(borrow)]
    kind: &'a RawValue,
    #[serde(borrow)]
    media_type: &'a RawValue,
    #[serde(borrow)]
    encoding: &'a RawValue,
    #[serde(borrow)]
    data: &'a RawValue,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Linked<'a> {
    #[serde(borrow)]
    kind: &'a RawValue,
    #[serde(borrow)]
    locator: &'a RawValue,
}

pub(super) fn prepare(
    raw: &RawValue,
    kind: AssetKind,
    limits: ProjectWireLimits,
) -> Result<Source, ProjectWireError> {
    // Strict disjoint source shapes: trying both never admits unknown members.
    Ok(if let Ok(source) = object::<Linked<'_>>(raw) {
        if text(source.kind, 8)? != "linked" {
            return Err(ProjectWireError::Unsupported);
        }
        let locator = text(source.locator, MAX_LOCATOR)?;
        AssetSource::linked(locator.clone(), MAX_LOCATOR).map_err(|_| ProjectWireError::Invalid)?;
        Source::Linked(locator)
    } else {
        let source: Embedded<'_> = object(raw)?;
        if text(source.kind, 8)? != "embedded" || text(source.encoding, 6)? != "base64" {
            return Err(ProjectWireError::Unsupported);
        }
        match (kind, text(source.media_type, 9)?.as_str()) {
            (AssetKind::Audio, "audio/wav") => {
                let (data, length) = audio::prepare(source.data, limits.audio)?;
                Source::Wave { data, length }
            }
            (AssetKind::Image, "image/png") => {
                let (data, length) =
                    payload::prepare(source.data, limits.png.ok_or(ProjectWireError::Budget)?.0)?;
                Source::Png { data, length }
            }
            #[cfg(feature = "font_asset_wire")]
            (AssetKind::Font, "font/ttf") => {
                let font = limits.font.ok_or(ProjectWireError::Unsupported)?;
                let (data, length) = payload::prepare(source.data, font.encoded_byte_limit())?;
                Source::Font { data, length }
            }
            _ => return Err(ProjectWireError::Unsupported),
        }
    })
}
