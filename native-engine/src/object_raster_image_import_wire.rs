//! L06-M2: closed intent for one atomic native PNG resource and placement.
//! Source identity remains in the original public request; native code alone reads bytes.
use crate::request_receipts::DispatchErrorCode;
use serde::{
    de::{value::MapAccessDeserializer, MapAccess, Visitor},
    Deserialize, Deserializer,
};
use std::fmt;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum ImageSource {
    File { path: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum ImagePlacement {
    NewLayer {},
    AuthoredFrame {
        #[serde(rename = "layerUid")]
        layer_uid: String,
        frame: u32,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct ImageImport {
    pub(crate) document_snapshot_id: String,
    pub(crate) lifecycle_generation: u64,
    pub(crate) name: String,
    pub(crate) media_type: String,
    pub(crate) source: ImageSource,
    pub(crate) placement: ImagePlacement,
}

impl<'de> Deserialize<'de> for ImageImport {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ClosedMap;
        impl<'de> Visitor<'de> for ClosedMap {
            type Value = ImageImport;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a closed image import object")
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields, rename_all = "camelCase")]
                struct Fields {
                    document_snapshot_id: String,
                    lifecycle_generation: u64,
                    name: String,
                    media_type: String,
                    #[serde(deserialize_with = "closed_map")]
                    source: ImageSource,
                    #[serde(deserialize_with = "closed_map")]
                    placement: ImagePlacement,
                }
                let fields = Fields::deserialize(MapAccessDeserializer::new(map))?;
                Ok(ImageImport {
                    document_snapshot_id: fields.document_snapshot_id,
                    lifecycle_generation: fields.lifecycle_generation,
                    name: fields.name,
                    media_type: fields.media_type,
                    source: fields.source,
                    placement: fields.placement,
                })
            }
        }
        // No visit_seq or intermediate Value: original bytes retain duplicate/unknown member checks.
        deserializer.deserialize_map(ClosedMap)
    }
}

// Tagged enums accept sequence representations by default. These wire fields are objects only.
// Deserialize from the original map so duplicate and unknown members remain visible to the DTO.
pub(crate) fn closed_map<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectOnly<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectOnly<T> {
        type Value = T;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an image source or placement object")
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectOnly(std::marker::PhantomData))
}

impl ImageImport {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, DispatchErrorCode> {
        if bytes.len() > 4096 {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let parsed: Self =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let ImageSource::File { path } = &parsed.source;
        if parsed.lifecycle_generation == 0
            || parsed.lifecycle_generation > 9_007_199_254_740_991
            || parsed.document_snapshot_id.is_empty()
            || parsed.document_snapshot_id.len() > 128
            || parsed.name.trim().is_empty()
            || parsed.name.len() > 255
            || parsed.name.contains('\0')
            || path.is_empty()
            || path.contains('\0')
            || parsed.media_type.is_empty()
            || parsed.media_type.len() > 128
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if let ImagePlacement::AuthoredFrame { layer_uid, .. } = &parsed.placement {
            if layer_uid.is_empty() || layer_uid.len() > 255 || layer_uid.contains('\0') {
                return Err(DispatchErrorCode::InvalidRequest);
            }
        }
        Ok(parsed)
    }
}
