//! #2267 text inside an entered component: the closed shapes the item contract admits for it.
use super::*;
use serde_json::json;

/// #2267 T-text-context: a text transaction may name a declared component's context and carry the copy key; a raster item stays scene-root.
#[test]
fn text_transactions_name_a_component_and_carry_the_copy_key_but_raster_items_stay_in_the_scene() {
    let mut inside = stable();
    inside["contextId"] = json!("comp-a");
    let mut made = created();
    made["contextId"] = json!("comp-a");
    for payload in [
        json!({"command":"object.text.create","target":made,"block":block()}),
        json!({"command":"object.text.create","target":created(),"block":block(),"keyframe":"copy"}),
        json!({"command":"object.text.edit","stableTarget":inside,"patch":{"text":"x"}}),
        json!({"command":"object.text.edit","stableTarget":stable(),"patch":{"text":"x"},"frame":7,"keyframe":"copy"}),
        json!({"command":"object.text.delete","stableTarget":inside,"frame":0,"keyframe":"copy"}),
        json!({"command":"object.text.delete","stableTarget":stable(),"keyframe":"copy"}),
    ] {
        assert!(request(ITEM, &payload), "{payload}");
    }
    for payload in [
        json!({"command":"object.text.edit","stableTarget":stable(),"patch":{"text":"x"},"keyframe":"paste"}),
        json!({"command":"object.text.edit","stableTarget":stable(),"patch":{"text":"x"},"frame":7}),
        json!({"command":"object.text.edit","stableTarget":stable(),"patch":{"text":"x"},"frame":-1,"keyframe":"copy"}),
        json!({"command":"object.text.edit","stableTarget":stable(),"patch":{"text":"x"},"frame":4294967296_u64,"keyframe":"copy"}),
        json!({"command":"object.text.create","target":created(),"block":block(),"frame":3}),
        json!({"command":"object.text.create","target":created(),"block":block(),"keyframe":"copy","extra":1}),
        json!({"command":"object.raster.move","stableTarget":{"contextId":"comp-a","frameScope":{"kind":"authored","frame":0},"layerUid":"l","strokeId":"r"},"x":1,"y":2}),
        json!({"command":"object.raster.delete","stableTarget":stable(),"keyframe":"copy"}),
    ] {
        assert!(!request(ITEM, &payload), "{payload}");
    }
}

/// #2267 a text animator command may name the declared component whose layer holds it.
#[test]
fn text_animator_commands_may_name_a_component_context() {
    let animator = json!({"id":"ta-1","selector":{},"props":{"opacity":50}});
    for payload in [
        json!({"command":"object.text.animator.upsert","layerUid":"inner","animator":animator}),
        json!({"command":"object.text.animator.upsert","layerUid":"inner","contextId":"comp-a","animator":animator}),
        json!({"command":"object.text.animator.remove","layerUid":"inner","contextId":"comp-a","animatorId":"ta-1"}),
        json!({"command":"object.text.animator.edit","layerUid":"inner","contextId":"comp-a","animatorId":"ta-1","edit":{"kind":"setEnabled","enabled":false}}),
    ] {
        assert!(request(ITEM, &payload), "{payload}");
    }
    for payload in [
        json!({"command":"object.text.animator.remove","layerUid":"inner","contextId":"","animatorId":"ta-1"}),
        json!({"command":"object.text.animator.remove","layerUid":"inner","contextId":7,"animatorId":"ta-1"}),
        json!({"command":"object.text.animator.remove","layerUid":"inner","contextId":null,"animatorId":"ta-1"}),
        json!({"command":"object.raster.delete","stableTarget":stable(),"contextId":"comp-a"}),
    ] {
        assert!(!request(ITEM, &payload), "{payload}");
    }
}
