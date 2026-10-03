//! Strict immutable resource wire admission; native-engine owns cubic geometry.

use super::{bounded_id, host_error, HostResult};
use native_engine::{
    render_geometry::{ClosedCubicPath, CubicSegment},
    render_scene::{GeometryPaintInput, LayerGeometry, OpaqueSrgbPaint},
};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    collections::BTreeSet,
    io::{self, Write},
};

const MAX_RESOURCE_BYTES: usize = 4 * 1_048_576;
const MAX_RESOURCES: usize = 64;
const MAX_LAYERS_PER_RESOURCE: usize = 256;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GeometryResourceInput {
    resource_id: String,
    resource_version: String,
    layers: Vec<GeometryLayerInput>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeometryLayerInput {
    layer_uid: String,
    bounds: [f64; 4],
    transform: [f64; 6],
    paint: PaintInput,
    // Missing means the historical rectangle. Present null must never become one.
    #[serde(
        default,
        deserialize_with = "present_path",
        skip_serializing_if = "Option::is_none"
    )]
    path: Option<PathInput>,
}

fn present_path<'de, D: Deserializer<'de>>(decoder: D) -> Result<Option<PathInput>, D::Error> {
    PathInput::deserialize(decoder).map(Some)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PaintInput {
    red: u8,
    green: u8,
    blue: u8,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PathInput {
    version: u32,
    closed: bool,
    segments: Vec<SegmentInput>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SegmentInput {
    point: [f64; 2],
    handle_in: [f64; 2],
    handle_out: [f64; 2],
}

struct ResourceSize(usize);

impl Write for ResourceSize {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_RESOURCE_BYTES.saturating_sub(self.0) {
            return Err(io::Error::other("geometry resources exceed 4 MiB"));
        }
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn admit_resources(
    inputs: &[GeometryResourceInput],
    expected: &BTreeSet<&str>,
) -> HostResult<Vec<GeometryPaintInput>> {
    if inputs.is_empty() || inputs.len() > MAX_RESOURCES {
        return Err(host_error(
            "invalid_request",
            "invalid geometry resource count",
        ));
    }
    // Count the canonical typed resource array without allocating another copy.
    // Both bootstrap and replacement pass here before publication. Replacement
    // retains its separate, stricter whole-request fingerprint size check.
    serde_json::to_writer(&mut ResourceSize(0), inputs)
        .map_err(|_| host_error("invalid_request", "geometry resources exceed 4 MiB"))?;
    inputs
        .iter()
        .map(|input| {
            if !bounded_id(&input.resource_id)
                || !bounded_id(&input.resource_version)
                || input.layers.is_empty()
                || input.layers.len() > MAX_LAYERS_PER_RESOURCE
            {
                return Err(host_error(
                    "invalid_request",
                    "invalid geometry resource identity or size",
                ));
            }
            let actual: BTreeSet<_> = input
                .layers
                .iter()
                .map(|layer| layer.layer_uid.as_str())
                .collect();
            if actual.len() != input.layers.len() || &actual != expected {
                return Err(host_error(
                    "invalid_request",
                    "geometry and projection layerUid sets differ",
                ));
            }
            let layers = input
                .layers
                .iter()
                .map(GeometryLayerInput::admit)
                .collect::<HostResult<Vec<_>>>()?;
            GeometryPaintInput::new(&input.resource_id, &input.resource_version, layers)
                .map_err(|error| host_error("invalid_request", error.to_string()))
        })
        .collect()
}

impl GeometryLayerInput {
    fn admit(&self) -> HostResult<LayerGeometry> {
        let paint = OpaqueSrgbPaint::new(self.paint.red, self.paint.green, self.paint.blue);
        let layer = match &self.path {
            None => LayerGeometry::new(&self.layer_uid, self.bounds, self.transform, paint),
            Some(path) => {
                if path.version != 1 || !path.closed {
                    return Err(host_error(
                        "invalid_request",
                        "unsupported cubic path version or closure",
                    ));
                }
                let path = ClosedCubicPath::new(
                    path.segments
                        .iter()
                        .map(|segment| CubicSegment {
                            point: segment.point,
                            handle_in: segment.handle_in,
                            handle_out: segment.handle_out,
                        })
                        .collect(),
                )
                .map_err(|message| host_error("invalid_request", message))?;
                if self.bounds != path.control_bounds() {
                    return Err(host_error(
                        "invalid_request",
                        "path bounds differ from the native control hull",
                    ));
                }
                LayerGeometry::closed_path(&self.layer_uid, path, self.transform, paint)
            }
        };
        layer.map_err(|error| host_error("invalid_request", error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_application_contract::{
        admit_project, replacement_fingerprint, NativeBootstrapRequest, NativeReplacementRequest,
    };
    use serde_json::{json, Value};

    fn resource() -> Value {
        json!({"resourceId":"geometry", "resourceVersion":"v1", "layers":[{
        "layerUid":"layer", "bounds":[40,40,80,80], "transform":[1,0,0,1,0,0],
        "paint":{"red":255,"green":0,"blue":0},
        "path":{"version":1,"closed":true,"segments":[
            {"point":[40,40],"handleIn":[0,0],"handleOut":[0,40]},
            {"point":[80,40],"handleIn":[0,40],"handleOut":[0,0]}
        ]}}]})
    }

    fn admit(value: Value) -> HostResult<Vec<GeometryPaintInput>> {
        let input = serde_json::from_value(value)
            .map_err(|error| host_error("invalid_request", error.to_string()))?;
        admit_resources(&[input], &BTreeSet::from(["layer"]))
    }

    #[test]
    fn strict_path_schema_and_native_envelopes_fail_closed() {
        let valid = resource();
        let expected = GeometryPaintInput::new(
            "geometry",
            "v1",
            vec![LayerGeometry::closed_path(
                "layer",
                ClosedCubicPath::new(vec![
                    CubicSegment {
                        point: [40.0, 40.0],
                        handle_in: [0.0, 0.0],
                        handle_out: [0.0, 40.0],
                    },
                    CubicSegment {
                        point: [80.0, 40.0],
                        handle_in: [0.0, 40.0],
                        handle_out: [0.0, 0.0],
                    },
                ])
                .unwrap(),
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                OpaqueSrgbPaint::new(255, 0, 0),
            )
            .unwrap()],
        )
        .unwrap();
        assert_eq!(admit(valid.clone()).unwrap(), vec![expected]);
        let path = &valid["layers"][0]["path"];
        let mut invalid = vec![Value::Null, json!({})];
        for (field, value) in [
            ("version", json!(2)),
            ("closed", json!(false)),
            ("segments", json!([])),
            ("segments", json!([path["segments"][0]])),
            ("segments", json!(vec![path["segments"][0].clone(); 257])),
            ("stroke", json!(true)),
            ("contours", json!([])),
        ] {
            let mut changed = path.clone();
            changed[field] = value;
            invalid.push(changed);
        }
        for field in ["version", "closed", "segments"] {
            let mut changed = path.clone();
            changed.as_object_mut().unwrap().remove(field);
            invalid.push(changed);
        }
        for field in ["point", "handleIn", "handleOut"] {
            let mut changed = path.clone();
            changed["segments"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            invalid.push(changed);
        }
        let mut unknown = path.clone();
        unknown["segments"][0]["pressure"] = json!(1);
        invalid.push(unknown);
        let mut overflow = path.clone();
        overflow["segments"][0]["point"] = json!([f64::MAX, 40]);
        overflow["segments"][0]["handleIn"] = json!([f64::MAX, 0]);
        invalid.push(overflow);
        for path in invalid {
            let mut changed = valid.clone();
            changed["layers"][0]["path"] = path;
            assert!(admit(changed).is_err());
        }
        for (field, value) in [
            ("bounds", json!([40, 40, 80, 79])),
            ("transform", json!([1, 0, 0, 1, 1e12, 0])),
            ("transform", json!([0, 0, 0, 0, 0, 0])),
            ("stroke", json!({})),
            ("gradient", json!({})),
            ("image", json!("image")),
        ] {
            let mut changed = valid.clone();
            changed["layers"][0][field] = value;
            assert!(admit(changed).is_err(), "{field}");
        }
        for field in ["point", "handleIn", "handleOut"] {
            for token in ["1e400", "NaN", "Infinity", "null"] {
                // Set a stable marker so each of the three fields is exercised.
                let mut marked = valid.clone();
                marked["layers"][0]["path"]["segments"][0][field] = json!([12345, 0]);
                let wire = serde_json::to_string(&marked)
                    .unwrap()
                    .replace("12345", token);
                assert!(serde_json::from_str::<GeometryResourceInput>(&wire).is_err());
            }
        }
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut typed: GeometryResourceInput = serde_json::from_value(valid.clone()).unwrap();
            typed.layers[0].path.as_mut().unwrap().segments[0].handle_out[0] = bad;
            assert!(admit_resources(&[typed], &BTreeSet::from(["layer"])).is_err());
        }
        let mut maximum = valid;
        maximum["layers"][0]["path"]["segments"] = json!(maximum["layers"][0]["path"]["segments"]
            .as_array()
            .unwrap()
            .iter()
            .cycle()
            .take(256)
            .cloned()
            .collect::<Vec<_>>());
        assert!(admit(maximum).is_ok());
    }

    // IDs are opaque nonempty strings in the existing projection contract.
    // Repeating a long layer ID across 64 distinct resources makes a valid exact
    // serialized-size boundary without introducing an ignored padding field.
    fn sized_inputs(target: usize) -> (Value, Vec<GeometryResourceInput>) {
        let mut inputs: Vec<GeometryResourceInput> = (0..64)
            .map(|index| {
                let mut input: GeometryResourceInput = serde_json::from_value(resource()).unwrap();
                input.resource_id = format!("g{index:02}");
                input
            })
            .collect();
        let base = serde_json::to_vec(&inputs).unwrap().len();
        let growth = target - base;
        let layer = format!("layer{}", "x".repeat(growth / 64));
        for input in &mut inputs {
            input.layers[0].layer_uid = layer.clone();
        }
        inputs[0]
            .resource_version
            .push_str(&"x".repeat(growth % 64));
        assert_eq!(serde_json::to_vec(&inputs).unwrap().len(), target);
        (
            json!({"format":"nemo.native-opacity-document","formatVersion":1,"totalFrames":1,
            "layers":[{"layerUid":layer,"motionStatic":{"opacity":[100]}}]}),
            inputs,
        )
    }

    #[test]
    fn bootstrap_and_replacement_enforce_resource_and_whole_request_caps() {
        for size in [
            MAX_RESOURCE_BYTES - 100_000,
            MAX_RESOURCE_BYTES - 1,
            MAX_RESOURCE_BYTES,
            MAX_RESOURCE_BYTES + 1,
        ] {
            let (projection, resources) = sized_inputs(size);
            let bootstrap: NativeBootstrapRequest = serde_json::from_value(json!({
                "apiVersion":2,"instanceId":"fixture","projection":projection,
                "resources":resources,"viewport":null
            }))
            .unwrap();
            assert_eq!(
                admit_project(&bootstrap.projection, &bootstrap.resources).is_ok(),
                size <= MAX_RESOURCE_BYTES,
                "bootstrap at {size}"
            );
            let replacement = NativeReplacementRequest {
                api_version: 2,
                request_id: "replace".into(),
                instance_id: "fixture".into(),
                document_id: "document".into(),
                expected_revision: 0,
                projection: bootstrap.projection,
                resources: bootstrap.resources,
            };
            assert_eq!(
                admit_project(&replacement.projection, &replacement.resources).is_ok(),
                size <= MAX_RESOURCE_BYTES,
                "replacement resource admission at {size}"
            );
            let whole_size = serde_json::to_vec(&replacement).unwrap().len();
            assert_eq!(
                replacement_fingerprint(&replacement).is_ok(),
                whole_size <= MAX_RESOURCE_BYTES
            );
            if size == MAX_RESOURCE_BYTES - 1 {
                assert!(
                    whole_size > MAX_RESOURCE_BYTES,
                    "resource-only admission cannot relax replacement cap"
                );
            }
        }
    }
}
