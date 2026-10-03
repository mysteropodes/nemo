//! Portable export projection only. Decoding and replay are separate work.
use crate::application::{ReproductionReason, REPRODUCTION_FIXTURE};
use serde::Serialize;
use serde_json::{json, Number};

pub(super) const MAX_COMMANDS: usize = 32;
pub(super) const MAX_BUNDLE_BYTES: usize = 3072;
pub(super) const TARGET: &str = "r08_curve_layer";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::application) struct CapturedCommand {
    pub id: u8,
    pub expected_revision: u64,
    pub value: Number,
    pub revision: u64,
    pub applied: bool,
}

pub(super) fn encode(commands: &[CapturedCommand]) -> Result<Vec<u8>, ReproductionReason> {
    let bytes = serde_json::to_vec(&json!({
        "format": "nemo.native-opacity-reproduction",
        "formatVersion": 1,
        "apiVersion": 2,
        "fixture": {
            "id": REPRODUCTION_FIXTURE.id,
            "version": REPRODUCTION_FIXTURE.version,
            "sha256": REPRODUCTION_FIXTURE.sha256
        },
        "command": "layer.opacity.set",
        "stableTarget": { "layerUid": TARGET },
        "clock": null,
        "seed": null,
        "versions": { "nativeEngine": env!("CARGO_PKG_VERSION") },
        "commands": commands
    }))
    .map_err(|_| ReproductionReason::InvalidProjection)?;
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(ReproductionReason::ByteLimit);
    }
    Ok(bytes)
}
