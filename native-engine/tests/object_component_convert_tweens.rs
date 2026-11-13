//! Evidence defect 3 follow-up (#2112): merging layers into a component keeps their tween spans. The merged layers keep
//! their uids inside the component, the tween store is keyed by layer uid alone (a component's inner layer is served by it),
//! so deleting the source scene layers must not take the spans with it.
use super::fixture::{decode, s2_document, uids, wire};
use super::*;
use serde_json::json;

fn tweened() -> serde_json::Value {
    let mut doc = s2_document();
    let options = json!({"resamplePoints":50,"step":1,"skipManual":true,"easing":[]});
    doc["tweens"] = json!({"layers":[
        {"layerUid":"ly_a","options":options,"spans":[]},
        {"layerUid":"ly_b","options":options,"spans":[]},
        {"layerUid":"ly_x","options":options,"spans":[]}]});
    doc
}

#[test]
fn merged_layers_keep_their_tween_entries_and_a_layer_left_in_the_scene_keeps_its_own() {
    let doc = decode(&tweened());
    let plan = plan_convert(&doc, 5, &uids(&["ly_c", "ly_a", "ly_b"])).unwrap();
    let after = wire(&plan.document);
    let tweened: Vec<&str> = after["tweens"]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap())
        .collect();
    assert_eq!(tweened, ["ly_a", "ly_b", "ly_x"], "no tween entry is dropped by the merge");
    assert_eq!(decode(&after), plan.document, "storage round trip");
}
