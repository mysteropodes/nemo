//! #2256 A-comp-edit-timeline part 4: a NEW linked-video layer inside a declared component (the editor of an entered component), through the real
//! history owner. The layer joins the component's own stack, the placement names the component, and the whole import is one revision and one history entry.
use super::*;

/// #2256: the scene (6 frames, instance layer `ly_inst`) and a declared component `sym_a` (4 frames, layers `ly_in_a`, `ly_in_b`), saved as a project file.
fn component_project() -> NativeObjectHistory {
    let timing = |keys: &[u32]| {
        let keys: Vec<Value> = keys.iter().map(|frame| json!({"frame":frame})).collect();
        json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":keys,"samples":[]})
    };
    let layer = |uid: &str| json!({"layerUid":uid,"name":uid,"visible":true,"locked":false,"color":"#4a9eff","parentLayerUid":null,"timing":timing(&[0])});
    let rows = |uids: &[&str]| -> Vec<Value> {
        uids.iter()
            .enumerate()
            .map(|(at, uid)| json!({"layerUid":uid,"naturalPosition":at,"order":{}}))
            .collect()
    };
    let drawing = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":6,
        "layers":[layer("ly_scene"),layer("ly_inst")],"objects":[],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":rows(&["ly_scene","ly_inst"]),"scopes":[]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"},
        "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":[{"symbolId":"sym_a","name":"A","totalFrames":4,"fps":24,
                "layers":[layer("ly_in_a"),layer("ly_in_b")],"objects":[]}],
            "instances":[{"contextId":"scene-root","layerUid":"ly_inst","symbolId":"sym_a","symPlayMode":"loop","symSpeed":1,"symPlacedAt":0,"symSingleFrame":0}]}}});
    let wire = json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24},"drawing":drawing,"viewportReference":null,"assets":[],"assetInventory":{"assetIds":[],"placements":[]}});
    NativeObjectHistory::from_project_file("video-component", wire.to_string().as_bytes()).unwrap()
}
fn component_request(
    owner: &NativeObjectHistory,
    id: &str,
    context: Option<&str>,
) -> OpacityRequest {
    let mut input = request(owner, id, json!({"kind":"newLayer"}));
    if let Some(context) = context {
        input.payload["contextId"] = json!(context);
    }
    input
}
/// The component's layer uids and the scene's, read from the saved document.
fn layer_uids(owner: &NativeObjectHistory) -> (Vec<String>, Vec<String>) {
    let file = saved(owner);
    let names = |layers: &Value| -> Vec<String> {
        layers
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["layerUid"].as_str().unwrap().to_owned())
            .collect()
    };
    (
        names(&file["drawing"]["layers"]),
        names(&file["drawing"]["componentTables"]["components"]["components"][0]["layers"]),
    )
}

#[test]
fn a_new_video_layer_inside_a_component_joins_its_stack_with_one_revision_and_one_undo() {
    let mut owner = component_project();
    let before = saved_bytes(&owner);
    let input = component_request(&owner, "video-in", Some("sym_a"));
    assert_eq!(owner.video_link_status(&input).unwrap(), None);
    let reply = owner
        .dispatch_video_link(&input, &facts("/clips/take.mov"))
        .unwrap();
    assert!(reply.is_ok(), "{reply:?}");
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    let (scene, component) = layer_uids(&owner);
    assert_eq!(
        scene,
        vec!["ly_scene", "ly_inst"],
        "the scene stack is untouched"
    );
    assert_eq!(component.len(), 3, "the component gained exactly one layer");
    assert_eq!(component[..2], ["ly_in_a", "ly_in_b"]);
    assert_eq!(component[2], "native-layer:1:0");
    assert_eq!(
        result(&reply),
        json!({"applied":true,"historyEntriesAdded":1,"assetId":"asset-video-in","placementId":"placement-video-in",
            "target":{"contextId":"sym_a","layerUid":"native-layer:1:0"},"newLayer":true,
            "sourceWidth":1920,"sourceHeight":1080,"sourceFrameCount":48})
    );
    let file = saved(&owner);
    let placement = &file["assetInventory"]["placements"][0];
    assert_eq!(
        placement["target"],
        json!({"kind":"drawing","contextId":"sym_a","layerUid":"native-layer:1:0"})
    );
    assert_eq!(
        file["drawing"]["componentTables"]["components"]["components"][0]["layers"][2]["name"],
        "take"
    );
    assert_eq!(
        file["drawing"]["componentTables"]["components"]["components"][0]["totalFrames"],
        4
    );
    // Save and reopen keeps the placement and the layer.
    let reopened = NativeObjectHistory::from_project_file("desktop", &saved_bytes(&owner)).unwrap();
    assert_eq!(saved(&reopened), file);
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(
        saved_bytes(&owner),
        before,
        "one Undo restores the complete bytes"
    );
    assert_eq!(owner.redo(2), Ok(3));
    assert_eq!(saved(&owner), file, "Redo reproduces the import");
}

#[test]
fn a_video_layer_in_a_component_refuses_an_undeclared_context_a_replace_with_a_context_and_a_blank_one(
) {
    for (mode, intent, context, expected) in [
        (
            "undeclared",
            json!({"kind":"newLayer"}),
            json!("sym_missing"),
            DispatchErrorCode::NotFound,
        ),
        (
            "replace-with-context",
            json!({"kind":"replace","placementId":"p"}),
            json!("sym_a"),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            "blank",
            json!({"kind":"newLayer"}),
            json!(" "),
            DispatchErrorCode::InvalidRequest,
        ),
    ] {
        let mut owner = component_project();
        let mut input = request(&owner, mode, intent);
        input.payload["contextId"] = context;
        let reply = owner.video_link_status(&input).unwrap().expect(mode);
        assert_eq!(code(&reply), expected, "{mode}");
        let reply = owner
            .dispatch_video_link(&input, &facts("/clips/take.mov"))
            .unwrap();
        assert_eq!(code(&reply), expected, "{mode}");
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (0, (0, 0)),
            "{mode}"
        );
    }
}

#[test]
fn the_scene_root_stays_the_default_and_an_explicit_scene_root_is_the_same_import() {
    for context in [None, Some("scene-root")] {
        let mut owner = component_project();
        let input = component_request(&owner, "video-scene", context);
        let reply = owner
            .dispatch_video_link(&input, &facts("/clips/take.mov"))
            .unwrap();
        assert!(reply.is_ok(), "{context:?}");
        let (scene, component) = layer_uids(&owner);
        assert_eq!((scene.len(), component.len()), (3, 2), "{context:?}");
        assert_eq!(result(&reply)["target"]["contextId"], "scene-root");
    }
}
