//! Closed source descriptors of the existing atomic import operation; no source I/O.
use super::ImportAssetSource;
use crate::history::object_history::object_shot_kernels::native_import_transaction::{
    wire, ImportBatch,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) struct SourceSpec {
    pub(super) source_id: String,
    pub(super) name: String,
    pub(super) media_type: String,
}
pub(super) struct SourceArgs {
    pub(super) snapshot_id: String,
    pub(super) specs: Vec<SourceSpec>,
    payload: Value,
}
fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}
pub(super) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
}
fn exact(value: &Value, keys: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))
    })
}
fn text(value: &Value) -> Result<&str, DispatchErrorCode> {
    value
        .as_str()
        .filter(|text| !text.is_empty() && !text.contains('\0'))
        .ok_or_else(invalid)
}
fn descriptor(media_type: &str, source: &Value) -> Result<(), DispatchErrorCode> {
    let valid = match (media_type, source["kind"].as_str()) {
        ("image/png", Some("file")) => {
            exact(source, &["kind", "path"]) && text(&source["path"]).is_ok()
        }
        ("font/ttf", Some("bundled")) => {
            exact(source, &["kind", "fontKey"])
                && matches!(
                    source["fontKey"].as_str(),
                    Some(
                        "Roboto-Regular"
                            | "Roboto-Bold"
                            | "DejaVuSans-Regular"
                            | "DejaVuSans-Bold"
                            | "DejaVuSerif-Regular"
                            | "DejaVuSerif-Bold"
                            | "Manrope-Regular"
                            | "Inter-Regular"
                    )
                )
        }
        ("font/ttf", Some("google")) => {
            exact(source, &["kind", "family", "weight", "italic"])
                && text(&source["family"])
                    .is_ok_and(|family| !family.trim().is_empty() && family.len() <= 255)
                && source["weight"]
                    .as_u64()
                    .is_some_and(|weight| (1..=1000).contains(&weight))
                && source["italic"].is_boolean()
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(invalid())
    }
}
impl SourceArgs {
    pub(super) fn parse(payload: &Value) -> Result<Self, DispatchErrorCode> {
        let mut body = payload.as_object().cloned().ok_or_else(invalid)?;
        let snapshot_id = body
            .remove("documentSnapshotId")
            .and_then(|v| v.as_str().map(str::to_owned))
            .filter(|id| identifier(id))
            .ok_or_else(invalid)?;
        body.remove("lifecycleGeneration")
            .and_then(|v| v.as_u64())
            .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)
            .ok_or_else(invalid)?;
        let sources = body.remove("sources").ok_or_else(invalid)?;
        let sources = sources
            .as_array()
            .filter(|rows| !rows.is_empty() && rows.len() <= 1024)
            .ok_or_else(invalid)?;
        let mut ids = BTreeSet::new();
        let mut specs = Vec::with_capacity(sources.len());
        for source in sources {
            if !exact(source, &["sourceId", "name", "mediaType", "source"]) {
                return Err(invalid());
            }
            let source_id = text(&source["sourceId"])?;
            if source_id.trim().is_empty()
                || source_id.len() > 255
                || !ids.insert(source_id.to_owned())
            {
                return Err(invalid());
            }
            let name = text(&source["name"])?;
            let media_type = text(&source["mediaType"])?;
            descriptor(media_type, &source["source"])?;
            specs.push(SourceSpec {
                source_id: source_id.into(),
                name: name.into(),
                media_type: media_type.into(),
            });
        }
        // Asset-only batches are valid. New declarations may not alias an existing
        // asset declaration/skip, even if the new source has no authored item.
        for row in body
            .get("assets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .chain(
                body.get("skipped")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|row| row["kind"] == "asset"),
            )
        {
            if row["sourceId"].as_str().is_some_and(|id| ids.contains(id)) {
                return Err(invalid());
            }
        }
        Ok(Self {
            snapshot_id,
            specs,
            payload: Value::Object(body),
        })
    }
    pub(super) fn batch(&self, origin: &ObjectSnapshot) -> Result<ImportBatch, DispatchErrorCode> {
        let bytes = serde_json::to_vec(&self.payload).map_err(|_| invalid())?;
        wire::parse_batch_at(origin, &bytes).map_err(|error| error.code())
    }
    pub(super) fn matches(&self, sources: &[ImportAssetSource<'_>]) -> bool {
        sources.len() == self.specs.len()
            && self.specs.iter().zip(sources).all(|(spec, bytes)| {
                spec.source_id == bytes.source_id
                    && spec.name == bytes.name
                    && spec.media_type == bytes.media_type
            })
    }
}
