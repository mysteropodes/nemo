//! #2256: the optional component `contextId` on the nine frame and key edits that have a component route.
use super::*;
use serde_json::json;

fn literal(operation: &str) -> Value {
    match operation {
        INSERT => json!({"command":"object.frames.insert","cell":3}),
        EXTEND => json!({"command":"object.frames.extend","cell":3,"count":2}),
        DUPLICATE => json!({"command":"object.frames.duplicate","layerUid":"layer-a","cell":3}),
        REMOVE => json!({"command":"object.frames.remove","from":2,"to":4}),
        KEY_PROMOTE => json!({"command":"object.layer.key.promote","cells":[3,5]}),
        KEY_BLANK => json!({"command":"object.layer.key.blank","layerUid":"layer-a","cell":3}),
        KEY_CLEAR => json!({"command":"object.layer.key.clear","layerUid":"layer-a","cell":3}),
        KEY_CLEAR_CELLS => {
            json!({"command":"object.layer.key.clear_cells","targets":[{"layerUid":"layer-a","frame":3}]})
        }
        TWEEN_REMOVE => {
            json!({"command":"object.layer.tween.remove","layerUid":"layer-a","from":2,"to":6})
        }
        _ => unreachable!(),
    }
}

#[test]
fn a_component_context_id_is_an_optional_opaque_name_on_the_nine_timing_edits() {
    // #2256: the declared component the edit works inside; absent is the scene root.
    for operation in [
        INSERT,
        EXTEND,
        DUPLICATE,
        REMOVE,
        KEY_PROMOTE,
        KEY_BLANK,
        KEY_CLEAR,
        KEY_CLEAR_CELLS,
        TWEEN_REMOVE,
    ] {
        let mut payload = literal(operation);
        assert!(request(operation, &payload), "{operation}");
        payload["contextId"] = json!("sym_a");
        assert!(
            request(operation, &payload),
            "{operation} accepts a component id"
        );
        payload["contextId"] = json!("");
        assert!(
            !request(operation, &payload),
            "{operation} refuses an empty id"
        );
        payload["contextId"] = json!(7);
        assert!(
            !request(operation, &payload),
            "{operation} refuses a non-string id"
        );
    }
}

fn moves(operation: &str) -> Value {
    match operation {
        KEY_MOVE => json!({"command":"object.layer.key.move","layerUid":"layer-a","from":2,"to":4}),
        MOVE => {
            json!({"command":"object.frames.move","selection":[{"layerUid":"layer-a","frame":2}],"layerUid":"layer-b","frame":3})
        }
        REPEAT => {
            json!({"command":"object.frames.repeat","fromLayerUid":"layer-a","toLayerUid":"layer-b","fromFrame":1,"toFrame":3,"times":2})
        }
        CLIPBOARD_COPY => {
            json!({"command":"object.frames.clipboard.copy","selection":[{"layerUid":"layer-a","frame":2}]})
        }
        CLIPBOARD_CUT => {
            json!({"command":"object.frames.clipboard.cut","selection":[{"layerUid":"layer-a","frame":2}]})
        }
        CLIPBOARD_PASTE => json!({"command":"object.frames.clipboard.paste","instanceId":"i","documentId":"d","layerUid":"layer-a","frame":1,
            "cells":[{"relLayer":0,"relFrame":0,"keyframe":true,"records":[]}]}),
        SELECTION_DUPLICATE => {
            json!({"command":"object.frames.clipboard.duplicate","layerUid":"layer-a","selection":[{"layerUid":"layer-a","frame":2}]})
        }
        _ => unreachable!(),
    }
}

#[test]
fn a_component_context_id_is_an_optional_opaque_name_on_move_repeat_and_the_clipboard() {
    // #2256 part 2: key move, whole-cell Move, Repeat, Copy, Cut, Paste and Duplicate Selection work inside a declared component.
    for operation in [
        KEY_MOVE,
        MOVE,
        REPEAT,
        CLIPBOARD_COPY,
        CLIPBOARD_CUT,
        CLIPBOARD_PASTE,
        SELECTION_DUPLICATE,
    ] {
        let mut payload = moves(operation);
        assert!(request(operation, &payload), "{operation}");
        payload["contextId"] = json!("sym_a");
        assert!(request(operation, &payload), "{operation} accepts a component id");
        payload["contextId"] = json!("");
        assert!(!request(operation, &payload), "{operation} refuses an empty id");
        payload["contextId"] = json!(7);
        assert!(!request(operation, &payload), "{operation} refuses a non-string id");
    }
    // Key move keeps its own optional captured Arc layer next to the context.
    let mut both = moves(KEY_MOVE);
    both["activeLayerUid"] = json!("layer-b");
    both["contextId"] = json!("sym_a");
    assert!(request(KEY_MOVE, &both));
}
