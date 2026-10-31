//! #2372: Link time and Follow path are admitted exactly as the native planner reads them.
use super::tests::P;
use serde_json::{json, Value};

/// #2372: Link time and Follow path are closed shapes; the engine repeats every rule on original bytes.
#[test]
fn the_layer_time_link_and_follow_path_writes_are_closed_shapes() {
    let ok = |op: &str, payload: Value| {
        crate::native_contract::validate_request(&format!("{P}{op}"), &payload)
    };
    let link = || json!({"command":"object.layer.timeLink.set","contextId":"scene-root","layerUid":"L","sourceLayerUid":"S","frame":3});
    let with = |key: &str, value: Value| {
        let mut payload = link();
        payload[key] = value;
        payload
    };
    assert!(ok("layer.timeLink.set", link()));
    assert!(
        ok("layer.timeLink.set", with("sourceLayerUid", Value::Null)),
        "Time: None"
    );
    assert!(ok("layer.timeLink.set", with("mode", json!("in"))));
    assert!(ok("layer.timeLink.set", with("srcAnchor", json!("out"))));
    assert!(!ok("layer.timeLink.set", with("mode", json!("sideways"))));
    assert!(!ok("layer.timeLink.set", with("srcAnchor", json!("both"))));
    assert!(
        !ok("layer.timeLink.set", with("sourceLayerUid", json!(""))),
        "an empty uid is not a source"
    );
    assert!(!ok("layer.timeLink.set", with("frame", json!(-1))));
    assert!(
        !ok("layer.timeLink.set", with("extra", json!(1))),
        "closed members"
    );
    assert!(!ok(
        "layer.timeLink.set",
        with("command", json!("object.layer.parent.set"))
    ));
    let mut absent = link();
    absent.as_object_mut().unwrap().remove("sourceLayerUid");
    assert!(!ok("layer.timeLink.set", absent), "null is explicit");
    let follow = || json!({"command":"object.layer.followPath.set","contextId":"scene-root","layerUid":"L","targetLayerUid":"P"});
    assert!(ok("layer.followPath.set", follow()));
    let mut none = follow();
    none["targetLayerUid"] = Value::Null;
    assert!(ok("layer.followPath.set", none), "Chemin : Aucun");
    let mut empty = follow();
    empty["targetLayerUid"] = json!("");
    assert!(!ok("layer.followPath.set", empty));
    let mut extra = follow();
    extra["frame"] = json!(0);
    assert!(!ok("layer.followPath.set", extra), "no frame member");
    let mut missing = follow();
    missing.as_object_mut().unwrap().remove("targetLayerUid");
    assert!(!ok("layer.followPath.set", missing));
    for align in [json!(true), json!(false)] {
        let mut aligned = follow();
        aligned["align"] = align;
        assert!(ok("layer.followPath.set", aligned), "the optional orient toggle");
    }
    for bad in [json!("true"), json!(1), Value::Null] {
        let mut aligned = follow();
        aligned["align"] = bad;
        assert!(!ok("layer.followPath.set", aligned), "align is a boolean");
    }
}
