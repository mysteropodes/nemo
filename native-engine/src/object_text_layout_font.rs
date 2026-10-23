//! Font reference of a text block record (G38a / L06-F).
//!
//! The baseline stores a bare key string, `root.data.vectorFont` (`Roboto-Bold`;
//! the panel dropdown spells it `vector:Roboto-Bold`). The native record stores
//! a tagged reference instead, so a font is always either one of the eight
//! bundled faces or a Font asset of the project inventory:
//!
//! - `{"kind":"bundled","fontKey":"Roboto-Bold"}`
//! - `{"kind":"asset","assetId":"<AssetId>"}`
//!
//! `BUNDLED_FONT_KEYS` is `VECTOR_FONTS` of `vector-text-bridge.js` at fbe4b8a3,
//! in table order. [`FontRef::from_legacy_key`] is the explicit mapping from a
//! baseline key; any other key (`Google:<family>-Regular`, a misspelling, a
//! prefixed twice key) is refused, never guessed. Whether an asset id really is
//! a retained, validated Font asset is the retained-font provider's question
//! (G41, Assistant2); this value only names it.

use crate::object_document::{object_deserialize, AssetId, Presence};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

/// `VECTOR_FONTS` keys of the baseline, in table order.
pub(crate) const BUNDLED_FONT_KEYS: [&str; 8] = [
    "Roboto-Regular",
    "Roboto-Bold",
    "DejaVuSans-Regular",
    "DejaVuSans-Bold",
    "DejaVuSerif-Regular",
    "DejaVuSerif-Bold",
    "Manrope-Regular",
    "Inter-Regular",
];

/// One of [`BUNDLED_FONT_KEYS`]; cannot be built from any other string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BundledFont(&'static str);

impl BundledFont {
    /// Exact, case-sensitive table lookup (no `vector:` prefix here).
    pub(crate) fn from_key(key: &str) -> Option<Self> {
        BUNDLED_FONT_KEYS
            .iter()
            .find(|candidate| **candidate == key)
            .map(|candidate| BundledFont(*candidate))
    }

    pub(crate) fn key(self) -> &'static str {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FontRef {
    Bundled(BundledFont),
    Asset(AssetId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FontRefError {
    /// A bundled key or legacy key that is not in the table.
    UnknownKey,
    /// `kind` is neither `bundled` nor `asset`.
    Kind,
    /// Members that do not belong to the `kind`, or a required one is absent.
    Members,
    /// The asset id is blank, over 255 bytes or contains NUL.
    AssetId,
}

impl std::fmt::Display for FontRefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "font reference refused: {:?}", self)
    }
}

/// The two members a font object may carry; the rest is refused by the
/// strict object decoder.
pub(crate) struct FontRefWire {
    kind: String,
    font_key: Presence<String>,
    asset_id: Presence<String>,
}
object_deserialize!(FontRefWire {
    kind: String,
    #[serde(default)] font_key: Presence<String>,
    #[serde(default)] asset_id: Presence<String>
});

impl FontRef {
    pub(crate) fn from_wire(wire: FontRefWire) -> Result<Self, FontRefError> {
        match wire.kind.as_str() {
            "bundled" => {
                if !wire.asset_id.missing() {
                    return Err(FontRefError::Members);
                }
                let key = wire.font_key.value().ok_or(FontRefError::Members)?;
                BundledFont::from_key(key)
                    .map(FontRef::Bundled)
                    .ok_or(FontRefError::UnknownKey)
            }
            "asset" => {
                if !wire.font_key.missing() {
                    return Err(FontRefError::Members);
                }
                let id = wire.asset_id.value().ok_or(FontRefError::Members)?;
                AssetId::new(id.clone())
                    .map(FontRef::Asset)
                    .map_err(|_| FontRefError::AssetId)
            }
            _ => Err(FontRefError::Kind),
        }
    }

    /// A baseline `vectorFont` value: the bare key, or the dropdown spelling
    /// with ONE leading `vector:`. Anything outside the table is refused.
    pub(crate) fn from_legacy_key(key: &str) -> Result<Self, FontRefError> {
        let bare = key.strip_prefix("vector:").unwrap_or(key);
        BundledFont::from_key(bare)
            .map(FontRef::Bundled)
            .ok_or(FontRefError::UnknownKey)
    }

    /// The key the layout kernel hands to its font catalog: the table key of a
    /// bundled face, the asset id of a font asset.
    pub(crate) fn layout_key(&self) -> &str {
        match self {
            FontRef::Bundled(font) => font.key(),
            FontRef::Asset(id) => id.as_str(),
        }
    }
}

impl Serialize for FontRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("FontRef", 2)?;
        match self {
            FontRef::Bundled(font) => {
                state.serialize_field("kind", "bundled")?;
                state.serialize_field("fontKey", font.key())?;
            }
            FontRef::Asset(id) => {
                state.serialize_field("kind", "asset")?;
                state.serialize_field("assetId", id.as_str())?;
            }
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for FontRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = FontRefWire::deserialize(deserializer)?;
        FontRef::from_wire(wire).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[path = "../tests/object_text_layout_font.rs"]
mod tests;
