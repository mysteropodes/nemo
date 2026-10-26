//! U34 (#2362) move-to-layer through the real history owner, in the v1 and v2 forms: the stroke moves with its
//! linked-fill companion and its dabs, identities kept, in the baseline order (fill, dabs, stroke) above the target's
//! drawing; nothing is duplicated; held cells are promoted; one history entry; atomic refusals.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{FrameScopeKind, Presence};
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;

fn owner(cells: &str, form: build::Build, edit: impl Fn(&mut Value)) -> NativeObjectHistory {
    let mut v = build::document_json(&rows::parse_layers(cells), form);
    for record in v["objects"].as_array_mut().unwrap() {
        record["family"] = json!("editable-vector-path");
        record["vector"] = json!({"stroke":{"kind":"solid","r":0,"g":0,"b":0,"a":1},"hasRealStroke":true,"strokeWidth":2,
            "strokeCap":"round","strokeJoin":"round","miterLimit":10,"dashOffset":0,"opacity":1});
        let ext = match record["target"]["strokeId"].as_str().unwrap() {
            "a" => {
                json!({"linkedFillId":"lf","brushTexture":{"brushGroupId":"g","brushTexturePreset":"charcoal-smooth","preTextureOpacity":1},
                    "pressure":{"centerSegments":[
                        {"point":{"x":0,"y":1},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":0.5},
                        {"point":{"x":10,"y":1},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":3}],
                        "widthProfile":[{"t":0,"width":0.5},{"t":1,"width":3}]}})
            }
            "af" => json!({"linkedFillId":"lf","isLinkedFillCompanion":true}),
            "d1" | "d2" => json!({"brushTexture":{"brushGroupId":"g","isBrushTextureCopy":true}}),
            _ => Value::Null,
        };
        if !ext.is_null() {
            record["vectorExt"] = ext;
        }
        // A pressure ribbon and its companion are painted through their fills.
        if matches!(record["target"]["strokeId"].as_str(), Some("a" | "af")) {
            record["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":6,"strokeCap":"round","strokeJoin":"round","opacity":1});
        }
    }
    edit(&mut v);
    NativeObjectHistory::new(
        "labs-move",
        decode_storage_project(v.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(o.acquire_snapshot(o.content_revision()).unwrap().document()).unwrap()
}
/// The cell's stroke ids in natural stacking order, and its exposure.
fn cell(o: &NativeObjectHistory, layer: &str, frame: u32) -> (&'static str, Vec<String>) {
    let s = o.acquire_snapshot(o.content_revision()).unwrap();
    let d = s.document();
    let kind = d
        .layers
        .iter()
        .find(|l| l.layer_uid == layer)
        .unwrap()
        .timing
        .value()
        .unwrap()
        .exposure(frame)
        .0;
    let mut records: Vec<_> = d
        .objects
        .iter()
        .filter(|r| {
            r.target.layer_uid == layer
                && r.target.frame_scope.kind == FrameScopeKind::Authored
                && r.target.frame_scope.frame == frame
        })
        .collect();
    if let Presence::Present(plan) = &d.composition_order {
        let at = |id: &str| {
            plan.contexts
                .iter()
                .flat_map(|c| &c.scopes)
                .filter(|sc| sc.frame_scope.frame == frame)
                .flat_map(|sc| &sc.elements)
                .find(|e| e.layer_uid == layer && e.stroke_id == id)
                .map(|e| e.natural_position)
                .unwrap()
        };
        records.sort_by_key(|r| at(&r.target.stroke_id));
    }
    (
        kind,
        records.iter().map(|r| r.target.stroke_id.clone()).collect(),
    )
}
fn run(o: &mut NativeObjectHistory, payload: Value) -> Value {
    let mut payload = payload;
    payload["command"] = json!("object.labs.moveToLayer");
    let request = json!({"apiVersion":2,"requestId":format!("move-{}", o.content_revision()),"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":"command.document.object.labs.moveToLayer","payload":payload});
    let typed = serde_json::from_value(request).unwrap();
    serde_json::to_value(o.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
}
const CELLS: &str = "K=a.1.10,af.2.10,d1.3.10,d2.4.10,b.5.10;.;.|K=c.1.10;.;.";

#[test]
fn the_stroke_moves_with_its_fill_and_dabs_above_the_target_drawing_in_one_entry() {
    for form in [build::Build::V1, build::Build::V2] {
        let mut o = owner(CELLS, form, |_| {});
        let before = bytes(&o);
        let reply = run(
            &mut o,
            json!({"fromLayer":"L0","toLayer":"L1","frame":0,"strokeIds":["a"]}),
        );
        assert_eq!(
            reply["result"],
            json!({"applied":true,"historyEntriesAdded":1}),
            "{reply}"
        );
        assert_eq!(cell(&o, "L0", 0), ("key", vec!["b".to_string()]));
        assert_eq!(
            cell(&o, "L1", 0),
            (
                "key",
                ["c", "af", "d1", "d2", "a"].map(String::from).to_vec()
            )
        );
        let after = bytes(&o);
        let undone = o.undo(o.content_revision()).unwrap();
        assert_eq!(bytes(&o), before);
        o.redo(undone).unwrap();
        assert_eq!(bytes(&o), after);
    }
}

#[test]
fn held_cells_are_promoted_so_the_key_keeps_its_strokes_and_nothing_is_duplicated() {
    for form in [build::Build::V1, build::Build::V2] {
        let mut o = owner(CELLS, form, |_| {});
        let reply = run(
            &mut o,
            json!({"fromLayer":"L0","toLayer":"L1","frame":1,"strokeIds":["b","a"]}),
        );
        assert_eq!(reply["result"]["applied"], true, "{reply}");
        assert_eq!(
            cell(&o, "L0", 0).1,
            ["a", "af", "d1", "d2", "b"].map(String::from).to_vec(),
            "the shown key is untouched"
        );
        assert_eq!(cell(&o, "L0", 1), ("key", vec![]));
        assert_eq!(
            cell(&o, "L1", 1),
            (
                "key",
                ["c", "b", "af", "d1", "d2", "a"].map(String::from).to_vec()
            )
        );
        assert_eq!(cell(&o, "L1", 0).1, vec!["c".to_string()]);
        assert_eq!(o.history_depths(), (1, 0));
    }
}

#[test]
fn refusals_publish_nothing() {
    for (edit, payload, code) in [
        (
            (|_: &mut Value| {}) as fn(&mut Value),
            json!({"fromLayer":"L0","toLayer":"L0","frame":0,"strokeIds":["a"]}),
            "invalid_request",
        ),
        (
            |v: &mut Value| v["layers"][1]["locked"] = json!(true),
            json!({"fromLayer":"L0","toLayer":"L1","frame":0,"strokeIds":["a"]}),
            "unavailable",
        ),
        (
            |_: &mut Value| {},
            json!({"fromLayer":"L0","toLayer":"L1","frame":0,"strokeIds":["z"]}),
            "not_found",
        ),
        (
            |_: &mut Value| {},
            json!({"fromLayer":"L0","toLayer":"L9","frame":0,"strokeIds":["a"]}),
            "not_found",
        ),
        (
            |_: &mut Value| {},
            json!({"fromLayer":"L0","toLayer":"L1","frame":3,"strokeIds":["a"]}),
            "invalid_request",
        ),
        (
            |_: &mut Value| {},
            json!({"fromLayer":"L0","toLayer":"L1","frame":0,"strokeIds":["a","a"]}),
            "invalid_request",
        ),
    ] {
        let mut o = owner(CELLS, build::Build::V2, edit);
        let before = bytes(&o);
        let reply = run(&mut o, payload.clone());
        assert_eq!(reply["error"]["code"], code, "{payload} {reply}");
        assert_eq!((bytes(&o), o.history_depths()), (before, (0, 0)));
    }
    let mut collide = owner(
        "K=a.1.10,af.2.10,d1.3.10,d2.4.10,b.5.10;.;.|K=a.1.10;.;.",
        build::Build::V1,
        |v| {
            let objects = v["objects"].as_array_mut().unwrap();
            let last = objects.len() - 1;
            objects[last]["vectorExt"] = Value::Null;
            objects[last].as_object_mut().unwrap().remove("vectorExt");
        },
    );
    let before = bytes(&collide);
    let reply = run(
        &mut collide,
        json!({"fromLayer":"L0","toLayer":"L1","frame":0,"strokeIds":["b"]}),
    );
    assert_eq!(reply["result"]["applied"], true, "a different id moves");
    let undone = collide.undo(collide.content_revision()).unwrap();
    let _ = undone;
    assert_eq!(bytes(&collide), before);
    let reply = run(
        &mut collide,
        json!({"fromLayer":"L0","toLayer":"L1","frame":0,"strokeIds":["a"]}),
    );
    assert_eq!(
        reply["error"]["code"], "unavailable",
        "the target already holds an 'a'"
    );
}
