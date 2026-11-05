//! #2372 R-lane-a-fixes: a scene layer row says `hasMotion` only when its own Position, Anchor, Rotation or Scale is away from the identity pose
//! (a key, an expression or a non-default static value), so a pointer on the canvas is mapped through that pose before a record is stored.
use super::layer_members;
use crate::object_codec::decode_storage_project;
use serde_json::json;

#[test]
fn only_a_layer_whose_pose_is_not_the_identity_reports_motion() {
    let target = |uid: &str| json!({"contextId":"scene-root","layerUid":uid});
    let uids = ["still", "default", "moved", "keyed", "scaled", "opacity", "anchored"];
    let document = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":12,"objects":[],
        "layers": uids.iter().map(|uid| json!({"layerUid":uid})).collect::<Vec<_>>(),
        "propertyHolders":[
            {"target":target("default"),"properties":[{"property":"position","static":[0,0]},
                {"property":"scale","static":[100,100]},{"property":"rotation","static":[0]},{"property":"anchor","static":[0,0]}]},
            {"target":target("moved"),"properties":[{"property":"position","static":[120,-60]}]},
            {"target":target("keyed"),"properties":[{"property":"rotation","static":[0],
                "track":{"keys":[{"frame":0,"v":[0]},{"frame":10,"v":[90]}]}}]},
            {"target":target("scaled"),"properties":[{"property":"scale","static":[100,50]}]},
            {"target":target("opacity"),"properties":[{"property":"opacity","static":[50]}]},
            {"target":target("anchored"),"properties":[{"property":"anchor","static":[0,5]}]}]});
    let rows = layer_members(&decode_storage_project(document.to_string().as_bytes()).unwrap());
    let flagged: Vec<bool> = rows.iter().map(|row| row.get("hasMotion") == Some(&json!(true))).collect();
    assert_eq!(flagged, [false, false, true, true, true, false, true], "{uids:?}");
}
