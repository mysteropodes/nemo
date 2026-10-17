//! Flat organizational folders and shy rows; no structural parenting or owner.
use crate::object_document::{object_deserialize, ObjectLayer, Presence};
use serde::{de, Deserialize, Deserializer, Serialize};
use std::{collections::BTreeMap, fmt};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayerFolder {
    #[serde(skip_serializing_if = "Presence::missing")]
    name: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    collapsed: Presence<bool>,
}
object_deserialize!(LayerFolder {
    #[serde(default)] name: Presence<String>,
    #[serde(default)] collapsed: Presence<bool>
});
impl LayerFolder {
    pub fn name(&self) -> Option<&str> {
        self.name.value().map(String::as_str)
    }
    pub fn collapsed(&self) -> bool {
        self.collapsed.value().copied().unwrap_or(false)
    }
}

/// BTreeMap serialization is deterministic; its decoder must not collapse
/// duplicate raw IDs before the native codec can reject the document.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct LayerFolders(BTreeMap<String, LayerFolder>);
impl LayerFolders {
    pub fn get(&self, id: &str) -> Option<&LayerFolder> {
        self.0.get(id)
    }
}
impl<'de> Deserialize<'de> for LayerFolders {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = LayerFolders;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map of distinct organizational folder IDs")
            }
            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut folders = BTreeMap::new();
                while let Some(id) = map.next_key::<String>()? {
                    if folders.contains_key(&id) {
                        return Err(de::Error::custom("duplicate organizational folder ID"));
                    }
                    let entry = map.next_value::<LayerFolder>()?;
                    folders.insert(id, entry);
                }
                Ok(LayerFolders(folders))
            }
        }
        d.deserialize_map(Visitor)
    }
}

pub(crate) fn validate_layer_organization(
    layers: &[ObjectLayer],
    folders: &Presence<Option<LayerFolders>>,
) -> Result<(), &'static str> {
    let table = folders.value().and_then(Option::as_ref);
    if table.is_some_and(|table| table.0.keys().any(String::is_empty)) {
        return Err("empty organizational folder ID");
    }
    for layer in layers {
        if let Some(id) = layer.folder_id() {
            if table.and_then(|table| table.get(id)).is_none() {
                return Err("unresolved organizational folder membership");
            }
        }
    }
    Ok(())
}
