//! Strict private NoMedia wire; drawing admission always sees original bytes.
use crate::object_document::{ProjectEnvelope, ProjectMetadata, ProjectReferenceState};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

const FORMAT: &str = "nemo.native-project-document";
const MAX_ENVELOPE: usize = 32 * 1024 * 1024;
const MAX_DRAWING: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProjectWireError {
    Budget,
    Invalid,
    Unsupported,
    Drawing(super::ObjectCodecError),
}

/// Explicit caller limits cannot exceed the staged file ceilings.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProjectWireLimits {
    envelope_bytes: usize,
    drawing_bytes: usize,
}
impl ProjectWireLimits {
    pub(crate) fn new(
        envelope_bytes: usize,
        drawing_bytes: usize,
    ) -> Result<Self, ProjectWireError> {
        if envelope_bytes == 0
            || envelope_bytes > MAX_ENVELOPE
            || drawing_bytes == 0
            || drawing_bytes > MAX_DRAWING
        {
            return Err(ProjectWireError::Budget);
        }
        Ok(Self {
            envelope_bytes,
            drawing_bytes,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Input<'a> {
    format: String,
    format_version: u32,
    project: Metadata,
    #[serde(borrow)]
    drawing: &'a RawValue,
    #[serde(borrow)]
    viewport_reference: &'a RawValue,
    #[serde(borrow)]
    assets: &'a RawValue,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    canvas_w: u32,
    canvas_h: u32,
    fps: f64,
}
// Derived decoding alone admits positional arrays; reuse the drawing's
// object-only visitor with strict duplicate/unknown/required-member checks.
crate::object_document::object_deserialize!(Metadata {
    canvas_w: u32,
    canvas_h: u32,
    fps: f64
});

pub(crate) fn decode_project(
    bytes: &[u8],
    limits: ProjectWireLimits,
) -> Result<ProjectEnvelope, ProjectWireError> {
    if bytes.len() > limits.envelope_bytes {
        return Err(ProjectWireError::Budget);
    }
    if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return Err(ProjectWireError::Invalid);
    }
    let wire: Input<'_> = serde_json::from_slice(bytes).map_err(|_| ProjectWireError::Invalid)?;
    if wire.format != FORMAT || wire.format_version != 1 {
        return Err(ProjectWireError::Unsupported);
    }
    if wire.drawing.get().len() > limits.drawing_bytes {
        return Err(ProjectWireError::Budget);
    }
    let assets: Vec<serde::de::IgnoredAny> =
        serde_json::from_str(wire.assets.get()).map_err(|_| ProjectWireError::Invalid)?;
    if wire.viewport_reference.get() != "null" || !assets.is_empty() {
        return Err(ProjectWireError::Unsupported);
    }
    let metadata = ProjectMetadata::new(
        wire.project.canvas_w,
        wire.project.canvas_h,
        wire.project.fps,
    )
    .map_err(|_| ProjectWireError::Invalid)?;
    let drawing = super::decode_storage_project(wire.drawing.get().as_bytes())
        .map_err(ProjectWireError::Drawing)?;
    Ok(ProjectEnvelope::no_media(metadata, drawing))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Output<'a> {
    format: &'static str,
    format_version: u32,
    project: Metadata,
    drawing: &'a RawValue,
    viewport_reference: (),
    assets: [(); 0],
}

// Check each write BEFORE reserving/copying it. Growth is capped too, so
// neither serialized length nor Vec capacity can exceed the caller limit.
struct CappedWriter {
    bytes: Vec<u8>,
    cap: usize,
    refused: bool,
}
impl std::io::Write for CappedWriter {
    fn write(&mut self, chunk: &[u8]) -> std::io::Result<usize> {
        let needed = self.bytes.len().checked_add(chunk.len());
        if needed.is_none_or(|n| n > self.cap) {
            self.refused = true;
            return Err(std::io::Error::other("project wire byte budget"));
        }
        let needed = needed.unwrap();
        if needed > self.bytes.capacity() {
            let capacity = needed
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.cap);
            if self
                .bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .is_err()
            {
                self.refused = true;
                return Err(std::io::Error::other("project wire reservation"));
            }
        }
        self.bytes.extend_from_slice(chunk);
        Ok(chunk.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn bounded_json(value: &impl Serialize, cap: usize) -> Result<Vec<u8>, ProjectWireError> {
    let mut writer = CappedWriter {
        bytes: Vec::new(),
        cap,
        refused: false,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| {
        if writer.refused {
            ProjectWireError::Budget
        } else {
            ProjectWireError::Invalid
        }
    })?;
    Ok(writer.bytes)
}

pub(crate) fn encode_project(
    envelope: &ProjectEnvelope,
    limits: ProjectWireLimits,
) -> Result<Vec<u8>, ProjectWireError> {
    match envelope.reference_state() {
        ProjectReferenceState::NoMedia => {}
    }
    super::validate_storage(envelope.drawing()).map_err(ProjectWireError::Drawing)?;
    let drawing = bounded_json(envelope.drawing(), limits.drawing_bytes)?;
    let raw =
        serde_json::from_slice::<&RawValue>(&drawing).map_err(|_| ProjectWireError::Invalid)?;
    let (canvas_w, canvas_h) = envelope.project().canvas_dimensions();
    let wire = Output {
        format: FORMAT,
        format_version: 1,
        project: Metadata {
            canvas_w,
            canvas_h,
            fps: envelope.project().fps(),
        },
        drawing: raw,
        viewport_reference: (),
        assets: [],
    };
    bounded_json(&wire, limits.envelope_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const EMPTY_DRAWING: &str = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,"layers":[{"layerUid":"0"}],"objects":[]}"#;
    const PREFIX: &str = r#"{"format":"nemo.native-project-document","formatVersion":1,"project":{"canvasW":320,"canvasH":180,"fps":24},"drawing":"#;
    const SUFFIX: &str = r#", "viewportReference":null,"assets":[]}"#;
    fn limits() -> ProjectWireLimits {
        ProjectWireLimits::new(MAX_ENVELOPE, MAX_DRAWING).unwrap()
    }
    fn wire(drawing: &str) -> Vec<u8> {
        format!("{PREFIX}{drawing}{SUFFIX}").into_bytes()
    }
    fn decode(bytes: &[u8]) -> Result<ProjectEnvelope, ProjectWireError> {
        decode_project(bytes, limits())
    }
    fn fixture(version: u32) -> Value {
        let cases: Value = serde_json::from_str(include_str!(
            "../../engineering/application/examples/native-object-v1/cases.json"
        ))
        .unwrap();
        let mut drawing = json!({"format":"nemo.native-object-document","formatVersion":version,
            "totalFrames":21,"layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],
            "objects":cases["records"]});
        if version == 2 {
            drawing["compositionOrder"] = json!({
                "format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
                "contexts":[{"contextId":"scene-root","layers":[
                    {"layerUid":"legacy layer:alpha","naturalPosition":0,"order":{"static":[0]}},
                    {"layerUid":"0","naturalPosition":1,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":7},"elements":[
                    {"layerUid":"legacy layer:alpha","strokeId":"ink/path_A","naturalPosition":0,"order":{}}]}]}],
                "migration":"explicit-plan-required","referencePolicy":"unresolved"});
        }
        drawing
    }

    #[test]
    fn literal_null_wire_roundtrip_is_the_normative_shape() {
        let bytes = wire(EMPTY_DRAWING);
        let project = decode(&bytes).unwrap();
        assert_eq!(project.project().canvas_dimensions(), (320, 180));
        assert_eq!(project.project().fps(), 24.);
        assert_eq!(project.reference_state(), ProjectReferenceState::NoMedia);
        let encoded = encode_project(&project, limits()).unwrap();
        let mut expected: Value = serde_json::from_slice(&bytes).unwrap();
        // The supplied numeric fps is an f64 in the model; 24 and 24.0
        // denote the same input value, and canonical output retains 24.0.
        expected["project"]["fps"] = json!(24.0);
        assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), expected);
        assert_eq!(decode(&encoded).unwrap(), project);
        assert!(!String::from_utf8(encoded).unwrap().contains("runtime"));
    }

    #[test]
    fn full_v1_v2_drawing_order_and_saved_reference_data_survive() {
        for version in [1, 2] {
            let drawing = fixture(version);
            let input = wire(&serde_json::to_string(&drawing).unwrap());
            let project = decode(&input).unwrap();
            assert_eq!(serde_json::to_value(project.drawing()).unwrap(), drawing);
            assert_eq!(
                project.drawing().objects()[1].target().scope_kind(),
                &crate::object_document::FrameScopeKind::Reference
            );
            assert_eq!(project.drawing().composition_order.missing(), version == 1);
            if version == 2 {
                assert_eq!(
                    drawing["compositionOrder"]["contexts"][0]["layers"][0]["order"]["static"][0],
                    0
                );
                assert_eq!(
                    drawing["compositionOrder"]["contexts"][0]["layers"][1]["order"],
                    json!({})
                );
            }
            let clone = project.clone();
            assert!(std::ptr::eq(project.drawing(), clone.drawing()));
            let output = encode_project(&project, limits()).unwrap();
            drop(project);
            assert_eq!(decode(&output).unwrap(), clone);
            assert_eq!(
                serde_json::from_slice::<Value>(&output).unwrap()["drawing"],
                drawing
            );
        }
    }

    #[test]
    fn every_required_top_and_metadata_member_is_strict() {
        let good: Value = serde_json::from_slice(&wire(EMPTY_DRAWING)).unwrap();
        for key in [
            "format",
            "formatVersion",
            "project",
            "drawing",
            "viewportReference",
            "assets",
        ] {
            let mut missing = good.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(
                decode(&serde_json::to_vec(&missing).unwrap()).is_err(),
                "missing {key}"
            );
            let mut raw = serde_json::to_string(&good).unwrap();
            let member = format!("\"{key}\":{}", serde_json::to_string(&good[key]).unwrap());
            raw = raw.replacen(&member, &format!("{member},{member}"), 1);
            assert!(decode(raw.as_bytes()).is_err(), "duplicate {key}");
        }
        for key in ["canvasW", "canvasH", "fps"] {
            let mut missing = good.clone();
            missing["project"].as_object_mut().unwrap().remove(key);
            assert!(decode(&serde_json::to_vec(&missing).unwrap()).is_err());
            let member = format!("\"{key}\":{}", good["project"][key]);
            let raw = serde_json::to_string(&good).unwrap().replacen(
                &member,
                &format!("{member},{member}"),
                1,
            );
            assert!(decode(raw.as_bytes()).is_err(), "duplicate metadata {key}");
        }
        for pointer in ["/runtimeId", "/project/runtimeId", "/drawing/runtimeId"] {
            let mut extra = good.clone();
            if pointer == "/runtimeId" {
                extra["runtimeId"] = json!("private");
            } else if pointer.starts_with("/project") {
                extra["project"]["runtimeId"] = json!("private");
            } else {
                extra["drawing"]["runtimeId"] = json!("private");
            }
            assert!(decode(&serde_json::to_vec(&extra).unwrap()).is_err());
        }
    }

    #[test]
    fn unsupported_shape_types_versions_metadata_and_media_refuse() {
        let good: Value = serde_json::from_slice(&wire(EMPTY_DRAWING)).unwrap();
        for (pointer, replacement) in [
            ("/format", json!("nemo.native-object-document")),
            ("/format", json!(null)),
            ("/formatVersion", json!(2)),
            ("/formatVersion", json!("1")),
            ("/project", json!([320, 180, 24])),
            ("/project", json!(null)),
            ("/project/canvasW", json!(0)),
            ("/project/canvasH", json!(-1)),
            ("/project/canvasW", json!(4294967296u64)),
            ("/project/canvasW", json!(320.5)),
            ("/project/fps", json!(0)),
            ("/project/fps", json!(-1)),
            ("/project/fps", json!("24")),
            ("/project/fps", json!(null)),
            ("/drawing", json!(null)),
            ("/drawing", json!([])),
            (
                "/viewportReference",
                json!({"kind":"still-image","assetId":"media-1"}),
            ),
            ("/viewportReference", json!(false)),
            ("/assets", json!(null)),
            ("/assets", json!({})),
            ("/assets", json!([null])),
            ("/assets", json!([{"id":"orphan","data":"AA=="}])),
        ] {
            let mut bad = good.clone();
            *bad.pointer_mut(pointer).unwrap() = replacement;
            assert!(
                decode(&serde_json::to_vec(&bad).unwrap()).is_err(),
                "{pointer}: {bad}"
            );
        }
        let positional = json!([FORMAT, 1, good["project"], good["drawing"], null, []]);
        assert!(decode(&serde_json::to_vec(&positional).unwrap()).is_err());
        for raw_fps in ["1e400", "NaN", "Infinity"] {
            let raw = String::from_utf8(wire(EMPTY_DRAWING))
                .unwrap()
                .replace("\"fps\":24", &format!("\"fps\":{raw_fps}"));
            assert!(decode(raw.as_bytes()).is_err());
        }
        for suffix in ["x", "{}"] {
            let mut raw = wire(EMPTY_DRAWING);
            raw.extend_from_slice(suffix.as_bytes());
            assert!(decode(&raw).is_err());
        }
    }

    #[test]
    fn original_nested_duplicates_cannot_be_erased_before_drawing_admission() {
        for version in [1, 2] {
            let drawing = serde_json::to_string(&fixture(version)).unwrap();
            for needle in [
                format!("\"formatVersion\":{version}"),
                "\"schemaVersion\":1".into(),
                "\"closed\":true".into(),
                "\"frame\":7".into(),
                "\"a\":0.75".into(),
            ] {
                assert!(drawing.contains(&needle), "mutant must change bytes");
                let duplicate = drawing.replacen(&needle, &format!("{needle},{needle}"), 1);
                assert!(decode(&wire(&duplicate)).is_err());
                // A Value intermediary would erase the duplicate and accept;
                // this positive countercontrol proves that mutant is visible.
                let erased: Value = serde_json::from_str(&duplicate).unwrap();
                assert!(decode(&wire(&serde_json::to_string(&erased).unwrap())).is_ok());
            }
        }
    }

    #[test]
    fn supplied_metadata_is_exact_and_bare_documents_never_implicitly_migrate() {
        let bytes = wire(EMPTY_DRAWING);
        let original = decode(&bytes).unwrap();
        let raw = String::from_utf8(bytes)
            .unwrap()
            .replace("\"fps\":24", "\"fps\":29.97")
            .replace("\"canvasW\":320", "\"canvasW\":640");
        let other = decode(raw.as_bytes()).unwrap();
        assert_eq!(other.project().canvas_dimensions(), (640, 180));
        assert_eq!(other.project().fps(), 29.97);
        assert_eq!(
            decode(&encode_project(&other, limits()).unwrap()).unwrap(),
            other
        );
        assert_eq!(other.drawing(), original.drawing());
        assert!(decode(EMPTY_DRAWING.as_bytes()).is_err());
        let clone = original.clone();
        assert!(decode(&wire("null")).is_err());
        assert_eq!(clone, original);
    }

    #[test]
    fn caller_and_hard_budgets_are_inclusive_checked_on_decode_and_encode() {
        assert!(ProjectWireLimits::new(MAX_ENVELOPE, MAX_DRAWING).is_ok());
        for (envelope, drawing) in [
            (0, 1),
            (1, 0),
            (MAX_ENVELOPE + 1, 1),
            (1, MAX_DRAWING + 1),
            (usize::MAX, 1),
            (1, usize::MAX),
        ] {
            assert!(matches!(
                ProjectWireLimits::new(envelope, drawing),
                Err(ProjectWireError::Budget)
            ));
        }
        let bytes = wire(EMPTY_DRAWING);
        let exact = ProjectWireLimits::new(bytes.len(), EMPTY_DRAWING.len()).unwrap();
        let project = decode_project(&bytes, exact).unwrap();
        assert_eq!(
            decode_project(
                &bytes,
                ProjectWireLimits::new(bytes.len() - 1, MAX_DRAWING).unwrap()
            ),
            Err(ProjectWireError::Budget)
        );
        assert_eq!(
            decode_project(
                &bytes,
                ProjectWireLimits::new(MAX_ENVELOPE, EMPTY_DRAWING.len() - 1).unwrap()
            ),
            Err(ProjectWireError::Budget)
        );
        let output = encode_project(&project, limits()).unwrap();
        let drawing_len = super::super::encode_storage_project(project.drawing())
            .unwrap()
            .len();
        assert_eq!(
            encode_project(
                &project,
                ProjectWireLimits::new(output.len(), drawing_len).unwrap()
            )
            .unwrap(),
            output
        );
        assert_eq!(
            encode_project(
                &project,
                ProjectWireLimits::new(output.len() - 1, MAX_DRAWING).unwrap()
            ),
            Err(ProjectWireError::Budget)
        );
        assert_eq!(
            encode_project(
                &project,
                ProjectWireLimits::new(MAX_ENVELOPE, drawing_len - 1).unwrap()
            ),
            Err(ProjectWireError::Budget)
        );
        // Valid raw drawing whitespace counts against the original-byte cap.
        let prefix = " ".repeat(MAX_DRAWING - EMPTY_DRAWING.len());
        let inclusive = format!("{{{prefix}{}", &EMPTY_DRAWING[1..]);
        assert_eq!(inclusive.len(), MAX_DRAWING);
        assert!(decode(&wire(&inclusive)).is_ok());
        let too_large = inclusive.strip_suffix('}').unwrap().to_owned() + " }";
        assert_eq!(decode(&wire(&too_large)), Err(ProjectWireError::Budget));
        let mut padded = bytes;
        padded.resize(MAX_ENVELOPE, b' ');
        assert!(decode(&padded).is_ok());
        padded.push(b' ');
        assert_eq!(decode(&padded), Err(ProjectWireError::Budget));
    }

    #[test]
    fn valid_oversized_drawing_refuses_without_serializing_an_over_limit_buffer() {
        let mut drawing = super::super::decode_storage_project(EMPTY_DRAWING.as_bytes()).unwrap();
        drawing.layers[0].layer_uid = "L".repeat(MAX_DRAWING + 1);
        super::super::validate_storage(&drawing).unwrap();
        let mut writer = CappedWriter {
            bytes: Vec::new(),
            cap: 1024,
            refused: false,
        };
        assert!(serde_json::to_writer(&mut writer, &drawing).is_err());
        assert!(writer.refused && writer.bytes.len() <= 1024 && writer.bytes.capacity() <= 1024);
        let project =
            ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 24.).unwrap(), drawing);
        assert_eq!(
            encode_project(&project, limits()),
            Err(ProjectWireError::Budget)
        );
    }
}
