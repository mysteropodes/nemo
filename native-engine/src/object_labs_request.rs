//! The strict request of every U35/U36 Labs operation (#2362): each command admits exactly its own fields, and the
//! baseline clamps happen in the caller, so an out-of-range value here is refused, not clamped.
use crate::commands::object_fill_owner::object_labs_generate::Ease;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{Map, Value};

/// One public operation per generator; the payload `command` repeats the operation suffix.
pub(crate) const OPS: [&str; 8] = [
    "command.document.object.labs.speedLines",
    "command.document.object.labs.boil",
    "command.document.object.labs.followPath",
    "command.document.object.labs.referenceFill",
    "command.document.object.labs.pingpong",
    "command.document.object.labs.retime",
    "command.document.object.labs.interval",
    "command.document.object.labs.moveToLayer",
];

/// One typed generator request; every command admits exactly its own fields.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LabsRequest {
    SpeedLines {
        layer_uid: String,
        frame: u32,
        center: [f64; 2],
        count: u32,
        r0: f64,
        r1: f64,
        jitter: f64,
        seed: u32,
        color: [f64; 4],
        brush_size: f64,
        opacity: f64,
    },
    Boil {
        layer_uid: String,
        frame: u32,
        frames: u32,
        amplitude: f64,
        seed_step: i64,
    },
    FollowPath {
        layer_uid: String,
        frame: u32,
        path_id: String,
        stroke_ids: Vec<String>,
        frames: u32,
        ease: Ease,
        remove_trajectory: bool,
    },
    ReferenceFill {
        layer_uid: String,
        frame: u32,
        point: [f64; 2],
        color: String,
        opacity: f64,
    },
    Pingpong {
        from_layer: String,
        to_layer: String,
        first: u32,
        last: u32,
        times: u32,
    },
    Retime {
        from_layer: String,
        to_layer: String,
        first: u32,
        last: u32,
        spacing: u32,
    },
    Interval {
        layer_uid: String,
        first: u32,
        last: u32,
        count: u32,
        ease: Ease,
    },
    MoveToLayer {
        from_layer: String,
        to_layer: String,
        frame: u32,
        stroke_ids: Vec<String>,
    },
}

fn fields<'a>(
    value: &'a Value,
    keys: &[&str],
) -> Result<&'a Map<String, Value>, DispatchErrorCode> {
    let map = value.as_object().ok_or(DispatchErrorCode::InvalidRequest)?;
    if map.len() != keys.len() || !keys.iter().all(|key| map.contains_key(*key)) {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    Ok(map)
}
fn number(
    map: &Map<String, Value>,
    key: &str,
    min: f64,
    max: f64,
) -> Result<f64, DispatchErrorCode> {
    map[key]
        .as_f64()
        .filter(|v| v.is_finite() && (min..=max).contains(v))
        .ok_or(DispatchErrorCode::InvalidRequest)
}
fn integer(
    map: &Map<String, Value>,
    key: &str,
    min: u64,
    max: u64,
) -> Result<u32, DispatchErrorCode> {
    map[key]
        .as_u64()
        .filter(|v| (min..=max).contains(v))
        .and_then(|v| u32::try_from(v).ok())
        .ok_or(DispatchErrorCode::InvalidRequest)
}
fn text(map: &Map<String, Value>, key: &str) -> Result<String, DispatchErrorCode> {
    map[key]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .map(str::to_owned)
        .ok_or(DispatchErrorCode::InvalidRequest)
}

/// Strict payload: the baseline clamps happen in the caller, so an out-of-range value here is refused, not clamped.
pub(crate) fn parse_labs_request(
    operation: &str,
    payload: &Value,
) -> Result<LabsRequest, DispatchErrorCode> {
    use DispatchErrorCode::InvalidRequest;
    let command = payload
        .get("command")
        .and_then(Value::as_str)
        .ok_or(InvalidRequest)?;
    let suffix = operation
        .strip_prefix("command.document.")
        .ok_or(InvalidRequest)?;
    if !OPS.contains(&operation) || command != suffix {
        return Err(InvalidRequest);
    }
    match command {
        "object.labs.speedLines" => {
            let m = fields(
                payload,
                &[
                    "command",
                    "layerUid",
                    "frame",
                    "cx",
                    "cy",
                    "count",
                    "r0",
                    "r1",
                    "jitter",
                    "seed",
                    "color",
                    "brushSize",
                    "opacity",
                ],
            )?;
            let color = m["color"]
                .as_array()
                .filter(|c| c.len() == 4)
                .ok_or(InvalidRequest)?;
            let mut rgba = [0.0; 4];
            for (slot, value) in rgba.iter_mut().zip(color) {
                *slot = value
                    .as_f64()
                    .filter(|v| (0.0..=1.0).contains(v))
                    .ok_or(InvalidRequest)?;
            }
            const SPAN: f64 = 1e7;
            Ok(LabsRequest::SpeedLines {
                layer_uid: text(m, "layerUid")?,
                frame: integer(m, "frame", 0, u64::from(u32::MAX))?,
                center: [number(m, "cx", -SPAN, SPAN)?, number(m, "cy", -SPAN, SPAN)?],
                count: integer(m, "count", 3, 400)?,
                r0: number(m, "r0", -SPAN, SPAN)?,
                r1: number(m, "r1", -SPAN, SPAN)?,
                jitter: number(m, "jitter", 0.0, 1.0)?,
                seed: integer(m, "seed", 0, u64::from(u32::MAX))?,
                color: rgba,
                brush_size: number(m, "brushSize", 0.0, 10_000.0)?,
                opacity: number(m, "opacity", 0.0, 1.0)?,
            })
        }
        "object.labs.boil" => {
            let m = fields(
                payload,
                &[
                    "command",
                    "layerUid",
                    "frame",
                    "frames",
                    "amplitude",
                    "seedStep",
                ],
            )?;
            let seed_step = m["seedStep"]
                .as_i64()
                .filter(|v| v.unsigned_abs() <= u64::from(u32::MAX))
                .ok_or(InvalidRequest)?;
            Ok(LabsRequest::Boil {
                layer_uid: text(m, "layerUid")?,
                frame: integer(m, "frame", 0, u64::from(u32::MAX))?,
                frames: integer(m, "frames", 1, 24)?,
                amplitude: number(m, "amplitude", 0.2, 1e6)?,
                seed_step,
            })
        }
        "object.labs.followPath" => {
            let m = fields(
                payload,
                &[
                    "command",
                    "layerUid",
                    "frame",
                    "pathId",
                    "strokeIds",
                    "frames",
                    "ease",
                    "removeTrajectory",
                ],
            )?;
            let ids = m["strokeIds"]
                .as_array()
                .filter(|ids| (1..=256).contains(&ids.len()))
                .ok_or(InvalidRequest)?;
            let mut stroke_ids = Vec::with_capacity(ids.len());
            for id in ids {
                let id = id
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 256)
                    .ok_or(InvalidRequest)?;
                if stroke_ids.iter().any(|seen: &String| seen == id) {
                    return Err(InvalidRequest);
                }
                stroke_ids.push(id.to_owned());
            }
            Ok(LabsRequest::FollowPath {
                layer_uid: text(m, "layerUid")?,
                frame: integer(m, "frame", 0, u64::from(u32::MAX))?,
                path_id: text(m, "pathId")?,
                stroke_ids,
                frames: integer(m, "frames", 2, 120)?,
                ease: Ease::parse(m["ease"].as_str().ok_or(InvalidRequest)?)?,
                remove_trajectory: m["removeTrajectory"].as_bool().ok_or(InvalidRequest)?,
            })
        }
        "object.labs.referenceFill" => {
            let m = fields(
                payload,
                &["command", "layerUid", "frame", "point", "color", "opacity"],
            )?;
            let point = m["point"]
                .as_array()
                .filter(|p| p.len() == 2)
                .ok_or(InvalidRequest)?;
            let at = |i: usize| {
                point[i]
                    .as_f64()
                    .filter(|v| v.is_finite() && v.abs() <= 1e12)
                    .ok_or(InvalidRequest)
            };
            Ok(LabsRequest::ReferenceFill {
                layer_uid: text(m, "layerUid")?,
                frame: integer(m, "frame", 0, u64::from(u32::MAX))?,
                point: [at(0)?, at(1)?],
                color: text(m, "color")?,
                opacity: number(m, "opacity", 0.0, 1.0)?,
            })
        }
        "object.labs.pingpong" | "object.labs.retime" => {
            let (key, low, high) = if command == "object.labs.pingpong" {
                ("times", 1, 50)
            } else {
                ("spacing", 1, 12)
            };
            let m = fields(
                payload,
                &[
                    "command",
                    "fromLayer",
                    "toLayer",
                    "fromFrame",
                    "toFrame",
                    key,
                ],
            )?;
            let (from_layer, to_layer) = (text(m, "fromLayer")?, text(m, "toLayer")?);
            let first = integer(m, "fromFrame", 0, u64::from(u32::MAX))?;
            let last = integer(m, "toFrame", 0, u64::from(u32::MAX))?;
            let n = integer(m, key, low, high)?;
            Ok(if command == "object.labs.pingpong" {
                LabsRequest::Pingpong {
                    from_layer,
                    to_layer,
                    first,
                    last,
                    times: n,
                }
            } else {
                LabsRequest::Retime {
                    from_layer,
                    to_layer,
                    first,
                    last,
                    spacing: n,
                }
            })
        }
        "object.labs.interval" => {
            let m = fields(
                payload,
                &[
                    "command",
                    "layerUid",
                    "fromFrame",
                    "toFrame",
                    "count",
                    "ease",
                ],
            )?;
            Ok(LabsRequest::Interval {
                layer_uid: text(m, "layerUid")?,
                first: integer(m, "fromFrame", 0, u64::from(u32::MAX))?,
                last: integer(m, "toFrame", 0, u64::from(u32::MAX))?,
                count: integer(m, "count", 1, 999)?,
                ease: Ease::parse(m["ease"].as_str().ok_or(InvalidRequest)?)?,
            })
        }
        "object.labs.moveToLayer" => {
            let m = fields(
                payload,
                &["command", "fromLayer", "toLayer", "frame", "strokeIds"],
            )?;
            let ids = m["strokeIds"]
                .as_array()
                .filter(|ids| (1..=256).contains(&ids.len()))
                .ok_or(InvalidRequest)?;
            let mut stroke_ids: Vec<String> = Vec::with_capacity(ids.len());
            for id in ids {
                let id = id
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 256)
                    .ok_or(InvalidRequest)?;
                if stroke_ids.iter().any(|seen| seen == id) {
                    return Err(InvalidRequest);
                }
                stroke_ids.push(id.to_owned());
            }
            Ok(LabsRequest::MoveToLayer {
                from_layer: text(m, "fromLayer")?,
                to_layer: text(m, "toLayer")?,
                frame: integer(m, "frame", 0, u64::from(u32::MAX))?,
                stroke_ids,
            })
        }
        _ => Err(InvalidRequest),
    }
}
