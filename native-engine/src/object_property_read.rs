//! Read-only property sampling facade; no second owner or publication authority.
use crate::object_codec::PropertyHolderTarget;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
impl ObjectSnapshot {
    /// Raw values from this exact retained snapshot. Expressions are not evaluated.
    pub fn sample_property_raw(
        &self,
        target: &PropertyHolderTarget,
        property: &str,
        frame: f64,
    ) -> Result<Vec<f64>, DispatchErrorCode> {
        #[cfg(feature = "evaluation")]
        {
            use crate::object_frame_packet::{sample_property_raw, PropertySampleError};
            sample_property_raw(self, target, property, frame).map_err(|error| match error {
                PropertySampleError::InvalidFrame => DispatchErrorCode::InvalidRequest,
                PropertySampleError::MissingTarget => DispatchErrorCode::NotFound,
                PropertySampleError::UnsupportedProperty | PropertySampleError::Unavailable => {
                    DispatchErrorCode::Unavailable
                }
            })
        }
        #[cfg(not(feature = "evaluation"))]
        {
            let _ = (target, property, frame);
            Err(DispatchErrorCode::Unavailable)
        }
    }
}

use crate::object_codec::object_deserialize;
use crate::request_receipts::{OpacityRequest, ResponseEnvelope};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug)]
struct ReadSelector {
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    view: ReadView,
}
object_deserialize!(ReadSelector {
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    view: ReadView
});
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum ReadFields {
    ExpressionContext {
        target: PropertyHolderTarget,
        property: String,
        frame: f64,
    },
    RawSample {
        target: PropertyHolderTarget,
        property: String,
        frame: f64,
    },
    AuthoredKeys {
        target: PropertyHolderTarget,
        property: String,
        offset: u32,
        limit: u8,
    },
}
#[derive(Debug)]
struct ReadView(ReadFields);
impl<'de> Deserialize<'de> for ReadView {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MapOnly;
        impl<'de> serde::de::Visitor<'de> for MapOnly {
            type Value = ReadView;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a pinned property read view object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<ReadView, A::Error> {
                ReadFields::deserialize(serde::de::value::MapAccessDeserializer::new(map))
                    .map(ReadView)
            }
        }
        deserializer.deserialize_map(MapOnly)
    }
}
impl ObjectSnapshot {
    /// Bounded projection from this existing retained snapshot. The original
    /// raw sampler above remains the sole evaluator; authored keys are storage DTOs.
    pub(crate) fn query_property_read_json(
        &self,
        bytes: &[u8],
    ) -> Result<Value, DispatchErrorCode> {
        use DispatchErrorCode::{
            InvalidRequest, StaleRevision, Unavailable, WrongDocument, WrongInstance,
        };
        if bytes.len() > 4096 {
            return Err(InvalidRequest);
        }
        let selector: ReadSelector = serde_json::from_slice(bytes).map_err(|_| InvalidRequest)?;
        if selector.at_revision > 9_007_199_254_740_991 || selector.document_snapshot_id.is_empty()
        {
            return Err(InvalidRequest);
        }
        if selector.instance_id != self.instance_id() {
            return Err(WrongInstance);
        }
        if selector.document_id != self.document_id() {
            return Err(WrongDocument);
        }
        if selector.at_revision != self.content_revision()
            || selector.document_snapshot_id != self.snapshot_id()
        {
            return Err(StaleRevision);
        }
        let (kind, target, property) = match &selector.view.0 {
            ReadFields::ExpressionContext {
                target, property, ..
            } => ("expressionContext", target, property),
            ReadFields::RawSample {
                target, property, ..
            } => ("rawSample", target, property),
            ReadFields::AuthoredKeys {
                target, property, ..
            } => ("authoredKeys", target, property),
        };
        if target.context_id().is_empty()
            || target.layer_uid().is_empty()
            || target.stroke_id().is_some_and(str::is_empty)
            || property.is_empty()
        {
            return Err(InvalidRequest);
        }
        if matches!(property.as_str(), "order" | "timeRemap") {
            return Err(Unavailable);
        }
        let holder = self.metadata_holder(target)?;
        let dimension = holder
            .metadata_in_document(self.document(), property)
            .ok_or(Unavailable)?
            .dimension();
        let mut result = json!({"instanceId":self.instance_id(),"documentId":self.document_id(),
            "atRevision":self.content_revision(),"documentSnapshotId":self.snapshot_id(),
            "kind":kind,"target":target,"property":property,"dimension":dimension});
        match &selector.view.0 {
            ReadFields::ExpressionContext { frame, .. } => {
                if !frame.is_finite() {
                    return Err(InvalidRequest);
                }
                #[cfg(feature = "evaluation")]
                {
                    let reads =
                        crate::evaluation::property_expression_snapshot_reads::SnapshotReads::bind(
                            self, target, property, *frame,
                        );
                    result["frame"] = json!(frame);
                    result["self"] = reads.info("").map_err(|_| Unavailable)?;
                    if let Some(mesh) = reads.mesh().map_err(|_| Unavailable)? {
                        result["mesh"] = mesh;
                    }
                    result["keyCount"] = json!(reads.key_count().map_err(|_| Unavailable)?);
                    match reads.public_comp() {
                        Ok(comp) => {
                            result["comp"] = comp;
                            result["compUnavailable"] = Value::Null;
                        }
                        Err(_) => {
                            result["comp"] = Value::Null;
                            result["compUnavailable"] = json!("projectMetadata");
                        }
                    }
                }
                #[cfg(not(feature = "evaluation"))]
                return Err(Unavailable);
            }
            ReadFields::RawSample { frame, .. } => {
                if !frame.is_finite() {
                    return Err(InvalidRequest);
                }
                // Explicit raw self sampling ignores active expressions; no effective-value claim.
                result["frame"] = json!(frame);
                result["value"] = json!(self.sample_property_raw(target, property, *frame)?);
            }
            ReadFields::AuthoredKeys { offset, limit, .. } => {
                if *limit == 0 || *limit > 8 {
                    return Err(InvalidRequest);
                }
                let track = holder
                    .properties()
                    .iter()
                    .find(|r| r.property() == property)
                    .and_then(|r| r.track.value());
                let keys = track.map_or(&[][..], |t| t.keys.as_slice());
                let start = *offset as usize;
                if start > keys.len() {
                    return Err(InvalidRequest);
                }
                let end = start.saturating_add(*limit as usize).min(keys.len());
                result["trackPresent"] = json!(track.is_some());
                // Serialize exact admitted records: missing/null/empty curves and handles survive.
                result["items"] =
                    serde_json::to_value(&keys[start..end]).map_err(|_| Unavailable)?;
                result["nextOffset"] = if end < keys.len() {
                    json!(u32::try_from(end).map_err(|_| Unavailable)?)
                } else {
                    Value::Null
                };
            }
        }
        // Never silently trim a key or replace authored metadata to meet the page budget.
        if serde_json::to_vec(&result).map_err(|_| Unavailable)?.len() > 3072 {
            return Err(Unavailable);
        }
        Ok(result)
    }
    pub(super) fn dispatch_property_read(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        use DispatchErrorCode::{Internal, InvalidRequest, Unavailable};
        let fields = ["atRevision", "documentSnapshotId", "view"];
        if request
            .payload
            .as_object()
            .is_none_or(|p| p.len() != fields.len() || fields.iter().any(|k| !p.contains_key(*k)))
        {
            return Ok(self.read_failure(
                request,
                InvalidRequest,
                "Property read requires only revision, snapshot and view.",
            ));
        }
        let selector = json!({"instanceId":request.instance_id,"documentId":request.document_id,
            "atRevision":request.payload["atRevision"],"documentSnapshotId":request.payload["documentSnapshotId"],
            "view":request.payload["view"]});
        let result = match self
            .query_property_read_json(&serde_json::to_vec(&selector).map_err(|_| InvalidRequest)?)
        {
            Ok(result) => result,
            Err(code) => {
                return Ok(self.read_failure(
                    request,
                    code,
                    "Pinned property read is invalid or unavailable.",
                ))
            }
        };
        let response = ResponseEnvelope {
            api_version: request.api_version,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            result: Some(result),
            error: None,
        };
        if serde_json::to_vec(&response).map_err(|_| Internal)?.len()
            > crate::object_snapshot::MAX_MESSAGE_BYTES
        {
            return Ok(self.read_failure(
                request,
                Unavailable,
                "Complete property read response exceeds 4096 bytes.",
            ));
        }
        Ok(response)
    }
}
