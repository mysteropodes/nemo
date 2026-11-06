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
    let paint = || json!({"command":"object.layer.color.set","layerUid":"L","color":"#112233"});
    assert!(ok("layer.color.set", paint()));
    let mut selected = paint();
    selected["selected"] = json!(["L", "M"]);
    assert!(ok("layer.color.set", selected), "the optional selection");
    for bad in [json!("#123"), json!("112233"), json!(""), json!("#1122334"), json!(7)] {
        let mut wrong = paint();
        wrong["color"] = bad;
        assert!(!ok("layer.color.set", wrong));
    }
    let mut holes = paint();
    holes["selected"] = json!(["L", ""]);
    assert!(!ok("layer.color.set", holes), "an empty identity is never a layer");
    let mut stray = paint();
    stray["frame"] = json!(0);
    assert!(!ok("layer.color.set", stray));
    let mut unnamed = paint();
    unnamed.as_object_mut().unwrap().remove("layerUid");
    assert!(!ok("layer.color.set", unnamed));
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

/// #2371 D36: the layer kinds of the + menu and folder membership are closed shapes; the engine repeats every rule on original bytes.
#[test]
fn the_layer_kind_creation_and_folder_membership_writes_are_closed_shapes() {
    let ok = |op: &str, payload: Value| {
        crate::native_contract::validate_request(&format!("{P}{op}"), &payload)
    };
    let kind = |kind: &str| json!({"command":"object.layer.kind.create","kind":kind,"name":"Null 1","frame":3});
    for name in ["null", "effect", "guide", "folder", "effector"] {
        assert!(ok("layer.kind.create", kind(name)), "{name}");
    }
    for bad in ["widget", "", "Null"] {
        assert!(!ok("layer.kind.create", kind(bad)), "{bad}");
    }
    let with = |key: &str, value: Value| {
        let mut payload = kind("null");
        payload[key] = value;
        payload
    };
    assert!(ok("layer.kind.create", with("pos", json!([160.5, 90]))));
    assert!(ok("layer.kind.create", with("children", json!(["L0", "L1"]))));
    assert!(ok("layer.kind.create", with("target", json!("L0"))));
    assert!(!ok("layer.kind.create", with("pos", json!([1, 2, 3]))));
    assert!(!ok("layer.kind.create", with("pos", json!([1e12, 0]))));
    assert!(!ok("layer.kind.create", with("children", json!([""]))));
    assert!(!ok("layer.kind.create", with("children", json!("L0"))));
    assert!(!ok("layer.kind.create", with("target", json!(""))));
    assert!(!ok("layer.kind.create", with("frame", json!(-1))));
    assert!(!ok("layer.kind.create", with("extra", json!(1))));
    let mut unnamed = kind("null");
    unnamed.as_object_mut().unwrap().remove("name");
    assert!(!ok("layer.kind.create", unnamed));
    let mut unframed = kind("null");
    unframed.as_object_mut().unwrap().remove("frame");
    assert!(!ok("layer.kind.create", unframed));

    let reparent = || json!({"command":"object.layer.folder.reparent","layerUids":["L0","L1"],"folderUid":"F","frame":3});
    assert!(ok("layer.folder.reparent", reparent()));
    let mut out = reparent();
    out["folderUid"] = Value::Null;
    assert!(ok("layer.folder.reparent", out.clone()), "out of the folder");
    out["reorder"] = json!({"gap":3,"selection":{"active":"L0","selected":["L0"],"anchor":null}});
    assert!(ok("layer.folder.reparent", out), "a drop that also reorders");
    let mut empty = reparent();
    empty["layerUids"] = json!([]);
    assert!(!ok("layer.folder.reparent", empty));
    let mut hole = reparent();
    hole["layerUids"] = json!(["L0", ""]);
    assert!(!ok("layer.folder.reparent", hole));
    let mut blank = reparent();
    blank["folderUid"] = json!("");
    assert!(!ok("layer.folder.reparent", blank));
    let mut gapless = reparent();
    gapless["reorder"] = json!({"selection":{"active":"L0","selected":[]}});
    assert!(!ok("layer.folder.reparent", gapless));
    let mut unframed = reparent();
    unframed.as_object_mut().unwrap().remove("frame");
    assert!(!ok("layer.folder.reparent", unframed));
}

/// #2371 / #2372: the in/out bar context menu is a closed shape; the engine repeats every rule on original bytes.
#[test]
fn the_layer_bar_menu_write_is_a_closed_shape() {
    let ok = |payload: Value| {
        crate::native_contract::validate_request(&format!("{P}layer.range.arrange"), &payload)
    };
    let arrange = |action: &str| json!({"command":"object.layer.range.arrange","contextId":"scene-root","layerUids":["L0","L1"],"action":action,"frame":3});
    for action in ["reset", "alignLeft", "alignCenter", "alignRight", "distribute", "flip"] {
        assert!(ok(arrange(action)), "{action}");
    }
    let mut stagger = arrange("stagger");
    stagger["step"] = json!(-4);
    assert!(ok(stagger.clone()), "a signed whole step");
    stagger["step"] = json!(1.5);
    assert!(!ok(stagger), "the step is a whole number of frames");
    assert!(!ok(arrange("shuffle")));
    let with = |key: &str, value: Value| {
        let mut payload = arrange("flip");
        payload[key] = value;
        payload
    };
    assert!(!ok(with("layerUids", json!([]))));
    assert!(!ok(with("layerUids", json!(["L0", ""]))));
    assert!(!ok(with("layerUids", json!("L0"))));
    assert!(!ok(with("frame", json!(-1))));
    assert!(!ok(with("contextId", json!(""))));
    assert!(!ok(with("extra", json!(1))));
    assert!(!ok(with("command", json!("object.layer.range.drag"))));
    let mut unframed = arrange("flip");
    unframed.as_object_mut().unwrap().remove("frame");
    assert!(!ok(unframed));
}
