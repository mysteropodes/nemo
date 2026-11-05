//! Mograph duplicators and effector layers (`ld.duplicator`, `ld.isEffectorLayer`/`ld.effector`) become the records of the
//! native duplicator table. The descriptor keeps the baseline member names; the load-time effector-channel migration
//! has already folded legacy offsets into `channels`.
use super::classify::is_default;
use super::LegacyImportError;
use serde_json::{json, Map, Value};

pub(crate) const MAPPED: &[&str] = &["duplicator", "isEffectorLayer", "effector"];
/// Editor-only members the baseline never persists meaningfully (`_dupEditSource` is transient).
const TRANSIENT: &[&str] = &["_dupEditSource"];

#[derive(Default)]
pub(crate) struct Tables {
    pub(crate) duplicators: Vec<Value>,
    pub(crate) effector_layers: Vec<Value>,
}

impl Tables {
    pub(crate) fn is_empty(&self) -> bool {
        self.duplicators.is_empty() && self.effector_layers.is_empty()
    }
    pub(crate) fn to_json(&self) -> Value {
        json!({"format": "nemo.native-duplicator-set", "formatVersion": 1,
            "duplicators": self.duplicators, "effectorLayers": self.effector_layers})
    }
}

pub(crate) fn convert_layer(
    context: &str,
    path: &str,
    map: &Map<String, Value>,
    uid: &str,
    out: &mut Tables,
) -> Result<(), LegacyImportError> {
    if let Some(value) = map.get("duplicator").filter(|v| !is_default(v)) {
        let mut descriptor = super::object(value, &format!("{path}.duplicator"))?.clone();
        for key in TRANSIENT {
            descriptor.remove(*key);
        }
        out.duplicators
            .push(json!({"contextId": context, "layerUid": uid, "duplicator": descriptor}));
    }
    let is_effector = map.get("isEffectorLayer").is_some_and(|v| !is_default(v));
    match (is_effector, map.get("effector").filter(|v| !is_default(v))) {
        (true, Some(effector)) => {
            super::object(effector, &format!("{path}.effector"))?;
            out.effector_layers
                .push(json!({"contextId": context, "layerUid": uid, "effector": effector}));
        }
        (true, None) => {
            return Err(LegacyImportError::unsupported(
                format!("{path}.effector"),
                "an effector layer needs its effector record",
            ))
        }
        (false, Some(_)) => {
            return Err(LegacyImportError::unsupported(
                format!("{path}.effector"),
                "an effector record needs isEffectorLayer",
            ))
        }
        (false, None) => {}
    }
    Ok(())
}
