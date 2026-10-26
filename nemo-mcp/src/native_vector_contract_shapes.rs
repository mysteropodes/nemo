//! Private vector record/appearance shape admission. The transport facade retains command and receipt ownership.
//! Exact keys, finite/range checks, anchor and two-stop gradient limits are the unchanged L01-J validators.
use crate::native_contract::exact;
use crate::native_contract::vector::{
    finite, non_negative, one_of, target, unit, CAPS, FAMILY, JOINS, MAX_ANCHORS, ORDERS, PROFILES,
};
use serde_json::Value;

#[path = "native_vector_contract_texture.rs"]
mod texture;

fn point(value: &Value) -> bool {
    exact(value, &["x", "y"], &[]).is_some() && finite(&value["x"]) && finite(&value["y"])
}

fn solid(value: &Value) -> bool {
    exact(value, &["kind", "r", "g", "b", "a"], &[]).is_some()
        && value["kind"] == "solid"
        && ["r", "g", "b", "a"].iter().all(|key| unit(&value[*key]))
}

fn geometry(value: &Value) -> bool {
    exact(
        value,
        &[
            "kind",
            "closed",
            "coordinateSpace",
            "handleSpace",
            "segments",
        ],
        &[],
    )
    .is_some()
        && value["kind"] == "cubic-path"
        && value["closed"].is_boolean()
        && value["coordinateSpace"] == "document-world"
        && value["handleSpace"] == "relative"
        && value["segments"].as_array().is_some_and(|segments| {
            (2..=MAX_ANCHORS).contains(&segments.len())
                && segments.iter().all(|segment| {
                    exact(segment, &["point", "handleIn", "handleOut"], &[]).is_some()
                        && ["point", "handleIn", "handleOut"]
                            .iter()
                            .all(|key| point(&segment[*key]))
                })
        })
}

fn dashes(value: &Value) -> bool {
    value.as_array().is_some_and(|items| {
        let total: f64 = items.iter().filter_map(Value::as_f64).sum();
        !items.is_empty() && items.iter().all(non_negative) && total > 0.0 && total.is_finite()
    })
}

fn body(value: &Value) -> bool {
    let optional = ["miterLimit", "dashArray", "dashOffset", "paintOrder"];
    let required = [
        "stroke",
        "hasRealStroke",
        "strokeWidth",
        "strokeCap",
        "strokeJoin",
        "opacity",
    ];
    exact(value, &required, &optional).is_some_and(|members| {
        let stroke = &value["stroke"];
        (stroke.is_null() || solid(stroke))
            && value["hasRealStroke"].as_bool() == Some(!stroke.is_null())
            && non_negative(&value["strokeWidth"])
            && one_of(&value["strokeCap"], &CAPS)
            && one_of(&value["strokeJoin"], &JOINS)
            && unit(&value["opacity"])
            && members.get("miterLimit").is_none_or(non_negative)
            && members.get("dashArray").is_none_or(dashes)
            && members.get("dashOffset").is_none_or(finite)
            && members.get("paintOrder").is_none_or(|v| one_of(v, &ORDERS))
    })
}

fn anchor(value: &Value) -> bool {
    exact(value, &["point", "handleIn", "handleOut", "width"], &[]).is_some()
        && ["point", "handleIn", "handleOut"]
            .iter()
            .all(|key| point(&value[*key]))
        && finite(&value["width"])
}

fn width_profile(value: &Value) -> bool {
    value.as_array().is_some_and(|stops| {
        let mut previous = 0.0;
        stops.iter().all(|stop| {
            let t = stop["t"]
                .as_f64()
                .filter(|t| unit(&stop["t"]) && *t >= previous);
            if let Some(t) = t {
                previous = t;
            }
            exact(stop, &["t", "width"], &[]).is_some() && t.is_some() && finite(&stop["width"])
        })
    })
}

fn pressure(value: &Value) -> bool {
    let optional = [
        "centerSegments",
        "widthProfile",
        "strokeProfile",
        "profileBase",
    ];
    exact(value, &[], &optional).is_some_and(|members| {
        members
            .get("centerSegments")
            .is_none_or(|v| v.as_array().is_some_and(|items| items.iter().all(anchor)))
            && members.get("widthProfile").is_none_or(width_profile)
            && members
                .get("strokeProfile")
                .is_none_or(|v| one_of(v, &PROFILES))
            && members.get("profileBase").is_none_or(non_negative)
    })
}

pub(super) fn shape(value: &Value, for_set: bool) -> bool {
    let kind = value["kind"].as_str().unwrap_or("");
    let members: &[&str] = match kind {
        "rect" => &["tl", "tr", "br", "bl"],
        "ellipse" => &["startAngle", "sweep", "innerRadius"],
        "star" => &["pointCount", "innerRatio", "cornerRadius"],
        _ => return false,
    };
    let mut optional = members.to_vec();
    if !for_set {
        optional.push("box");
    }
    exact(value, &["kind"], &optional).is_some_and(|object| {
        members
            .iter()
            .all(|key| object.get(*key).is_none_or(finite))
            && object.get("box").is_none_or(|bounds| {
                exact(bounds, &["x1", "y1", "x2", "y2"], &[]).is_some()
                    && ["x1", "y1", "x2", "y2"]
                        .iter()
                        .all(|key| finite(&bounds[*key]))
            })
    })
}

fn gradient(value: &Value) -> bool {
    let number = |v: &Value| {
        v.as_f64()
            .is_some_and(|n| n.is_finite() && (n as f32).is_finite())
    };
    exact(value, &["kind", "from", "to", "stops"], &[]).is_some()
        && one_of(&value["kind"], &["linear", "radial"])
        && ["from", "to"].iter().all(|key| {
            value[*key]
                .as_array()
                .is_some_and(|p| p.len() == 2 && p.iter().all(number))
        })
        && (((value["to"][0].as_f64().unwrap() - value["from"][0].as_f64().unwrap()).powi(2)
            + (value["to"][1].as_f64().unwrap() - value["from"][1].as_f64().unwrap()).powi(2))
        .sqrt() as f32)
            .is_finite()
        && value["stops"].as_array().is_some_and(|stops| {
            stops.len() == 2
                && stops.iter().all(|stop| {
                    exact(stop, &["offset", "color"], &[]).is_some()
                        && number(&stop["offset"])
                        && stop["color"].as_str().is_some_and(|c| {
                            [4, 7, 9].contains(&c.len())
                                && c.starts_with('#')
                                && c.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
                        })
                })
        })
}
fn fill_trace(value: &Value) -> bool {
    let pair = |v: &Value| {
        v.as_array()
            .is_some_and(|p| p.len() == 2 && p.iter().all(finite))
    };
    exact(value, &[], &["seed", "seeds", "walls", "gapPx"]).is_some_and(|o| {
        (o.contains_key("seed") || o.contains_key("seeds"))
            && o.get("seed").is_none_or(pair)
            && o.get("seeds")
                .is_none_or(|v| v.as_array().is_some_and(|s| s.iter().all(pair)))
            && o.get("walls").is_none_or(|v| {
                v.as_array().is_some_and(|ids| {
                    ids.iter()
                        .all(|id| id.as_str().is_some_and(|s| !s.is_empty()))
                })
            })
            && o.get("gapPx").is_none_or(non_negative)
    })
}

fn extension(value: &Value, has_stroke: bool) -> bool {
    exact(
        value,
        &[],
        &[
            "pressure",
            "paramShape",
            "boxAngle",
            "fillGradient",
            "fillTrace",
            "isMask",
            "maskMode",
            "maskFeather",
            "linkedFillId",
            "isLinkedFillCompanion",
            "brushTexture",
        ],
    )
    .is_some_and(|object| {
        !object.is_empty()
            && object.get("brushTexture").is_none_or(texture::brush_texture)
            && object
                .get("pressure")
                .is_none_or(|v| pressure(v) && !has_stroke)
            && object.get("paramShape").is_none_or(|v| shape(v, false))
            && object.get("boxAngle").is_none_or(finite)
            && object.get("linkedFillId").is_none_or(|value| {
                value.as_str().is_some_and(|id| {
                    !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
                })
            })
            && object
                .get("isLinkedFillCompanion")
                .is_none_or(Value::is_boolean)
            && (object.get("isLinkedFillCompanion") != Some(&Value::Bool(true))
                || (object.contains_key("linkedFillId") && !object.contains_key("pressure")))
            && (!object.contains_key("linkedFillId")
                || object.get("isLinkedFillCompanion") == Some(&Value::Bool(true))
                || object
                    .get("pressure")
                    .and_then(|p| p.get("centerSegments"))
                    .and_then(Value::as_array)
                    .is_some_and(|points| points.len() >= 2))
            && object
                .get("isMask")
                .is_none_or(|v| v.is_null() || v.is_boolean())
            && object
                .get("maskMode")
                .is_none_or(|v| v.is_null() || v.is_string())
            && object
                .get("maskFeather")
                .is_none_or(|v| v.is_null() || finite(v))
            && object.get("fillTrace").is_none_or(fill_trace)
            && object
                .get("fillGradient")
                .is_none_or(|v| gradient(v) && !object.contains_key("pressure"))
    })
}

pub(super) fn record_shape(value: &Value, create: bool) -> bool {
    exact(
        value,
        &[
            "schemaVersion",
            "family",
            "target",
            "geometry",
            "fill",
            "vector",
        ],
        &["vectorExt"],
    )
    .is_some_and(|object| {
        value["schemaVersion"] == 1
            && value["family"] == FAMILY
            && target(&value["target"], create)
            && geometry(&value["geometry"])
            && (value["fill"].is_null() || solid(&value["fill"]))
            && body(&value["vector"])
            && (value["vectorExt"].get("linkedFillId").is_none()
                || (value["target"]["frameScope"]["kind"] == "authored"
                    && (value["vectorExt"]["isLinkedFillCompanion"] != true
                        || !value["fill"].is_null())))
            && object
                .get("vectorExt")
                .is_none_or(|ext| extension(ext, value["vector"]["hasRealStroke"] == true))
    })
}

#[test]
fn linked_pair_record_shapes_preserve_roles_and_reject_invalid_metadata() {
    use serde_json::json;
    let point = json!({"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    let companion = json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","layerUid":"L","strokeId":"backdrop","frameScope":{"kind":"authored","frame":0}},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":[point.clone(),point]},
        "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"round","strokeJoin":"round","opacity":1},
        "vectorExt":{"linkedFillId":"pair","isLinkedFillCompanion":true}});
    assert!(record_shape(&companion, false));
    let anchor =
        json!({"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":1});
    let mut primary = companion.clone();
    primary["vectorExt"] = json!({"linkedFillId":"pair","isLinkedFillCompanion":false,"pressure":{"centerSegments":[anchor.clone(),anchor]}});
    assert!(record_shape(&primary, false));
    primary["vectorExt"]
        .as_object_mut()
        .unwrap()
        .remove("isLinkedFillCompanion");
    assert!(record_shape(&primary, false));
    let mut ordinary = companion.clone();
    ordinary["vectorExt"] = json!({"isLinkedFillCompanion":false});
    assert!(record_shape(&ordinary, false));
    for (path, value) in [
        ("/vectorExt/linkedFillId", Value::Null),
        ("/vectorExt/linkedFillId", json!("")),
        ("/vectorExt/linkedFillId", json!("x".repeat(129))),
        ("/vectorExt/linkedFillId", json!("bad\u{0000}id")),
        ("/vectorExt/isLinkedFillCompanion", Value::Null),
        ("/fill", Value::Null),
        ("/target/frameScope/kind", json!("evaluated")),
    ] {
        let mut bad = companion.clone();
        *bad.pointer_mut(path).unwrap() = value;
        assert!(!record_shape(&bad, false), "{path}");
    }
    let mut missing = companion.clone();
    missing["vectorExt"]
        .as_object_mut()
        .unwrap()
        .remove("linkedFillId");
    assert!(!record_shape(&missing, false));
    let mut short = primary;
    short["vectorExt"]["pressure"]["centerSegments"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(!record_shape(&short, false));
}

#[test]
fn ellipse_parameters_use_their_own_closed_finite_field_set() {
    use serde_json::json;
    let ellipse = json!({"kind":"ellipse","startAngle":-15,"sweep":240.5,"innerRadius":0.25});
    assert!(shape(&ellipse, true));
    let mut stored = ellipse.clone();
    stored["box"] = json!({"x1":0,"y1":0,"x2":30,"y2":20});
    assert!(shape(&stored, false));
    assert!(!shape(&stored, true));
    for key in ["startAngle", "sweep", "innerRadius"] {
        let mut invalid = ellipse.clone();
        invalid[key] = json!("not-a-number");
        assert!(!shape(&invalid, true));
    }
    let mut foreign = ellipse;
    foreign["tl"] = json!(1);
    assert!(!shape(&foreign, false));
}
