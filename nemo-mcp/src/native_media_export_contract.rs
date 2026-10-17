//! L09-K native SVG and media export jobs: closed request payloads. Shape only: the one native owner
//! repeats every rule on the pinned snapshot and stays the only authority. The result is the shared
//! `JobReceipt`, which (like the PNG job) is bounded by the common response check and the v2 schema.
use super::{bounded_identifier, exact, frame};
use serde_json::Value;

pub(crate) const SVG_BEGIN: &str = "job.export.svg.begin";
pub(crate) const MEDIA_BEGIN: &str = "job.export.media.begin";
const CONTROLS: [&str; 4] = [
    "job.export.svg.status",
    "job.export.svg.cancel",
    "job.export.media.status",
    "job.export.media.cancel",
];
/// MAX_OBJECT_EXPORT_FRAMES: frames of one host job.
const MAX_JOB_FRAMES: usize = 32;
/// One receipt lists at most this many frame names (a response is at most 4096 bytes).
const MAX_SEQUENCE_FRAMES: u64 = 120;
/// The object export OutputSpec bound.
const MAX_DIMENSION: u64 = 4096;

pub(crate) fn owns(operation: &str) -> bool {
    operation == SVG_BEGIN || operation == MEDIA_BEGIN || CONTROLS.contains(&operation)
}

fn id(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(bounded_identifier)
}

fn scene_root(value: Option<&Value>) -> bool {
    value.and_then(Value::as_str) == Some("scene-root")
}

fn svg_begin(payload: &Value) -> bool {
    let members = ["documentSnapshotId", "contextId", "outputHandle", "frames"];
    exact(payload, &members, &[]).is_some_and(|m| {
        id(m.get("documentSnapshotId"))
            && scene_root(m.get("contextId"))
            && id(m.get("outputHandle"))
            && m.get("frames")
                .and_then(Value::as_array)
                .is_some_and(|frames| {
                    (1..=MAX_JOB_FRAMES).contains(&frames.len())
                        && frames
                            .iter()
                            .map(|value| frame(Some(value)))
                            .collect::<Option<Vec<u64>>>()
                            .is_some_and(|all| all.windows(2).all(|pair| pair[0] < pair[1]))
                })
    })
}

/// Absent members mean the project work area; both present: `start <= end`; a still names its frame.
fn range(value: &Value, kind: &str) -> bool {
    let Some(m) = exact(value, &[], &["start", "end"]) else {
        return false;
    };
    let bound = |key: &str| match m.get(key) {
        None => Some(None),
        Some(found) => frame(Some(found)).map(Some),
    };
    let (Some(start), Some(end)) = (bound("start"), bound("end")) else {
        return false;
    };
    if let (Some(first), Some(last)) = (start, end) {
        if last < first || (kind == "sequence" && last - first + 1 > MAX_SEQUENCE_FRAMES) {
            return false;
        }
    }
    kind != "still" || matches!((start, end), (Some(first), Some(last)) if first == last)
}

fn output_spec(value: &Value) -> bool {
    let members = [
        "kind",
        "format",
        "width",
        "height",
        "colorInterpretation",
        "alphaMode",
    ];
    exact(value, &members, &[]).is_some_and(|m| {
        [
            ("kind", "frame"),
            ("format", "rgba8"),
            ("colorInterpretation", "srgb"),
            ("alphaMode", "straight"),
        ]
        .iter()
        .all(|(key, expected)| m.get(*key).and_then(Value::as_str) == Some(*expected))
            && ["width", "height"].iter().all(|key| {
                m.get(*key)
                    .and_then(Value::as_u64)
                    .is_some_and(|size| (1..=MAX_DIMENSION).contains(&size))
            })
    })
}

/// `fps || project fps` and `MP4_QUALITY_QV[quality || 'high'] || '65'`: 0 and "" fall back natively.
fn video(value: &Value) -> bool {
    exact(value, &[], &["fps", "quality"]).is_some_and(|m| {
        m.get("fps").is_none_or(|fps| {
            fps.as_f64()
                .is_some_and(|rate| rate.is_finite() && rate >= 0.0)
        }) && m.get("quality").is_none_or(|quality| {
            quality.as_str().is_some_and(|word| {
                word.len() <= 32
                    && word
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            })
        })
    })
}

fn media_begin(payload: &Value) -> bool {
    let required = [
        "documentSnapshotId",
        "contextId",
        "outputHandle",
        "output",
        "range",
    ];
    exact(payload, &required, &["outputSpec", "video"]).is_some_and(|m| {
        let Some(output) = exact(&payload["output"], &["kind", "frameFormat"], &[]) else {
            return false;
        };
        let (Some(kind), Some(format)) = (
            output.get("kind").and_then(Value::as_str),
            output.get("frameFormat").and_then(Value::as_str),
        ) else {
            return false;
        };
        id(m.get("documentSnapshotId"))
            && scene_root(m.get("contextId"))
            && id(m.get("outputHandle"))
            && matches!(kind, "still" | "sequence" | "video")
            && matches!(format, "png" | "svg")
            && !(kind == "video" && format != "png")
            && range(&payload["range"], kind)
            && (format == "png") == m.contains_key("outputSpec")
            && m.get("outputSpec").is_none_or(output_spec)
            && m.get("video").is_none_or(|v| kind == "video" && video(v))
    })
}

pub(crate) fn request(operation: &str, payload: &Value) -> bool {
    match operation {
        SVG_BEGIN => svg_begin(payload),
        MEDIA_BEGIN => media_begin(payload),
        _ => exact(payload, &["jobId"], &[]).is_some_and(|m| id(m.get("jobId"))),
    }
}

#[cfg(test)]
#[path = "native_media_export_contract_tests.rs"]
mod tests;
