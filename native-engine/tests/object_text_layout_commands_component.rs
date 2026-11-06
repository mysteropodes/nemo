//! #2267 T-text-context: the text-block transactions address a DECLARED component context like the scene (baseline: `enterSymbol`
//! swaps `state.layers`, so the Text tool places and edits text on the component's own layers), and a held cell is keyed with a copy of what
//! it showed before a text commit (the baseline's ensureKeyframe), in the same revision: one history entry per gesture.
use super::super::support::{head, owner, prepare, run, state, items_of};
use crate::request_receipts::DispatchErrorCode;
use super::block_of;
use serde_json::{json, Value};

const CTX: &str = "comp-a";

fn layers(uids: &[&str]) -> Value {
    json!(uids.iter().enumerate().map(|(i, l)| json!({"layerUid":l,"naturalPosition":i,"order":{}})).collect::<Vec<_>>())
}
fn at(layer: &str, frame: u32, stroke: &str, ctx: &str) -> Value {
    json!({"contextId":ctx,"frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":stroke})
}
fn timing(keys: &[u32]) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root",
        "keys":keys.iter().map(|k| json!({"frame":k})).collect::<Vec<_>>(),"samples":[]})
}
/// A scene layer `L1` and a host layer placing component `comp-a` (8 frames, layer `inner`).
fn doc() -> Value {
    let mut d = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":24,
        "layers":[{"layerUid":"L1","timing":timing(&[0])},{"layerUid":"host-a","timing":timing(&[0])}],"objects":[]});
    d["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[
        {"contextId":"scene-root","layers":layers(&["L1","host-a"]),"scopes":[]},
        {"contextId":CTX,"layers":layers(&["inner"]),"scopes":[]}]});
    d["componentTables"] = json!({"components":{"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":CTX,"totalFrames":8,"layers":[{"layerUid":"inner","timing":timing(&[0])}],"objects":[]}],
        "instances":[{"contextId":"scene-root","layerUid":"host-a","symbolId":CTX,"symPlayMode":"loop"}]}});
    d
}
fn block(text: &str) -> Value {
    json!({"text":text,"font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,"color":"#112233","align":"left","fixedWidth":null,"fixedHeight":null,
        "bold":false,"italic":false,"underline":false,"strike":false,"letterSpacing":0,"wordSpacing":0,"lineHeightMult":1.25,"textCase":"none",
        "anchorTopLeft":{"x":10,"y":12}})
}

#[test]
fn text_is_created_edited_and_deleted_inside_a_component() {
    let mut o = owner(&doc());
    let before = state(&o);
    let made = run(&mut o, &json!({"command":"object.text.create","target":at("inner", 0, "", CTX),"block":block("Hi")}));
    assert_eq!(made, Ok(1), "{:?}", made);
    let items = items_of(&o);
    let text = items["text"].as_array().unwrap();
    assert_eq!(text.len(), 1);
    assert_eq!(text[0]["target"]["contextId"], CTX);
    assert_eq!(text[0]["target"]["layerUid"], "inner");
    let id = text[0]["target"]["strokeId"].as_str().unwrap().to_string();
    assert_eq!(block_of(&text[0]["block"]).encode().unwrap(), block_of(&block("Hi")).encode().unwrap());
    assert_eq!(o.history_depths(), (1, 0));
    // The scene root holds none.
    assert!(text.iter().all(|t| t["target"]["contextId"] == CTX));
    let edited = run(&mut o, &json!({"command":"object.text.edit","stableTarget":at("inner", 0, &id, CTX),"patch":{"text":"Hello"}}));
    assert_eq!(edited, Ok(2), "{:?}", edited);
    assert_eq!(items_of(&o)["text"][0]["block"]["text"], "Hello");
    let gone = run(&mut o, &json!({"command":"object.text.delete","stableTarget":at("inner", 0, &id, CTX)}));
    assert_eq!(gone, Ok(3), "{:?}", gone);
    assert_eq!(items_of(&o)["text"].as_array().map_or(0, |t| t.len()), 0);
    for _ in 0..3 {
        let r = o.content_revision();
        o.undo(r).unwrap();
    }
    assert_eq!(state(&o).3, before.3);
    let _ = head(&o);
}

fn texts(o: &crate::history::NativeObjectHistory) -> Vec<(String, u64, String)> {
    let mut rows: Vec<(String, u64, String)> = items_of(o)["text"].as_array().map_or(vec![], |list| list.iter().map(|t| (
        t["target"]["strokeId"].as_str().unwrap().to_string(), t["target"]["frameScope"]["frame"].as_u64().unwrap(), t["block"]["text"].as_str().unwrap().to_string())).collect());
    rows.sort();
    rows
}
fn seeded() -> (crate::history::NativeObjectHistory, String) {
    let mut o = owner(&doc());
    run(&mut o, &json!({"command":"object.text.create","target":at("L1", 0, "", "scene-root"),"block":block("Key")})).unwrap();
    let id = texts(&o)[0].0.clone();
    (o, id)
}

#[test]
fn a_text_edit_on_a_held_frame_keys_that_cell_with_a_copy_and_leaves_the_key() {
    let (mut o, id) = seeded();
    let before = state(&o);
    let edited = run(&mut o, &json!({"command":"object.text.edit","stableTarget":at("L1", 0, &id, "scene-root"),"patch":{"text":"Held"},"frame":3,"keyframe":"copy"}));
    assert_eq!(edited, Ok(2), "{:?}", edited);
    assert_eq!(texts(&o), vec![(id.clone(), 0, "Key".to_string()), (id.clone(), 3, "Held".to_string())], "the key keeps its text; the held cell holds the edited copy under the same id");
    assert_eq!(o.history_depths(), (2, 0), "one history entry for the gesture");
    o.undo(2).unwrap();
    assert_eq!(texts(&o), vec![(id.clone(), 0, "Key".to_string())]);
    assert_eq!(state(&o).3, before.3, "one undo restores the document byte for byte");
    // Without the copy key the same edit lands on the key itself (Motion edits the key a held frame inherits).
    let mut inplace = seeded();
    assert_eq!(run(&mut inplace.0, &json!({"command":"object.text.edit","stableTarget":at("L1", 0, &inplace.1, "scene-root"),"patch":{"text":"Moved"}})), Ok(2));
    assert_eq!(texts(&inplace.0), vec![(inplace.1.clone(), 0, "Moved".to_string())]);
}

#[test]
fn a_text_create_and_delete_on_a_held_frame_key_the_cell_first() {
    let (mut o, id) = seeded();
    let made = run(&mut o, &json!({"command":"object.text.create","target":at("L1", 0, "", "scene-root").as_object().map(|t| { let mut t = t.clone(); t["frameScope"] = json!({"kind":"authored","frame":4}); Value::Object(t) }).unwrap(),"block":block("New"),"keyframe":"copy"}));
    assert_eq!(made, Ok(2), "{:?}", made);
    let rows = texts(&o);
    assert_eq!(rows.len(), 3, "the key, the copy of the key's text at frame 4 and the new block");
    assert!(rows.contains(&(id.clone(), 4, "Key".to_string())), "{rows:?}");
    assert_eq!(o.history_depths(), (2, 0));
    let gone = run(&mut o, &json!({"command":"object.text.delete","stableTarget":at("L1", 0, &id, "scene-root"),"frame":4,"keyframe":"copy"}));
    assert_eq!(gone, Ok(3), "{:?}", gone);
    let rows = texts(&o);
    assert!(rows.contains(&(id.clone(), 0, "Key".to_string())) && !rows.iter().any(|r| r.0 == id && r.1 == 4), "{rows:?}");
    // A cell that is already a key is edited as it is: nothing is copied.
    let (mut keyed, kid) = seeded();
    assert_eq!(run(&mut keyed, &json!({"command":"object.text.edit","stableTarget":at("L1", 0, &kid, "scene-root"),"patch":{"text":"Same"},"frame":0,"keyframe":"copy"})), Ok(2));
    assert_eq!(texts(&keyed), vec![(kid, 0, "Same".to_string())]);
}

#[test]
fn the_copy_key_members_are_strict_and_a_component_held_cell_is_keyed_with_a_copy_too() {
    let (o, id) = seeded();
    let edit = |extra: Value| { let mut p = json!({"command":"object.text.edit","stableTarget":at("L1", 0, &id, "scene-root"),"patch":{"text":"X"}}); for (k, v) in extra.as_object().unwrap() { p[k] = v.clone(); } p };
    assert_eq!(prepare(&o, &edit(json!({"keyframe":"paste"}))).err(), Some(DispatchErrorCode::InvalidRequest), "only the word copy");
    assert_eq!(prepare(&o, &edit(json!({"frame":3}))).err(), Some(DispatchErrorCode::InvalidRequest), "a landing frame needs the copy key");
    assert_eq!(prepare(&o, &edit(json!({"frame":-1,"keyframe":"copy"}))).err(), Some(DispatchErrorCode::InvalidRequest));
    assert_eq!(prepare(&o, &edit(json!({"frame":24,"keyframe":"copy"}))).err(), Some(DispatchErrorCode::InvalidRequest), "past the scene");
    assert_eq!(prepare(&o, &json!({"command":"object.text.create","target":at("L1", 0, "", "scene-root"),"block":block("a"),"frame":2})).err(), Some(DispatchErrorCode::InvalidRequest), "a create names its cell by its target");
    // A component: the key cell takes the copy key as a plain edit; its held cell is keyed with a copy of the key's text (the cell promotion copies items there too).
    let mut c = owner(&doc());
    run(&mut c, &json!({"command":"object.text.create","target":at("inner", 0, "", CTX),"block":block("In")})).unwrap();
    let cid = texts(&c)[0].0.clone();
    let key = json!({"command":"object.text.edit","stableTarget":at("inner", 0, &cid, CTX),"patch":{"text":"Out"},"frame":0,"keyframe":"copy"});
    assert_eq!(run(&mut c, &key), Ok(2));
    let held = json!({"command":"object.text.edit","stableTarget":at("inner", 0, &cid, CTX),"patch":{"text":"Held"},"frame":3,"keyframe":"copy"});
    assert_eq!(run(&mut c, &held), Ok(3), "a component's held cell is keyed with a copy, then edited");
    assert_eq!(texts(&c), vec![(cid.clone(), 0, "Out".to_string()), (cid.clone(), 3, "Held".to_string())], "the key keeps its text");
    assert_eq!(c.history_depths(), (3, 0), "one entry for the gesture");
    c.undo(3).unwrap();
    assert_eq!(texts(&c), vec![(cid.clone(), 0, "Out".to_string())]);
    let beyond = json!({"command":"object.text.edit","stableTarget":at("inner", 0, &cid, CTX),"patch":{"text":"Held"},"frame":8,"keyframe":"copy"});
    assert_eq!(prepare(&c, &beyond).err(), Some(DispatchErrorCode::InvalidRequest), "the component has 8 frames");
    let undeclared = json!({"command":"object.text.create","target":at("inner", 0, "", "comp-b"),"block":block("a")});
    assert_eq!(prepare(&c, &undeclared).err(), Some(DispatchErrorCode::Unavailable));
    let _ = head(&c);
}

#[test]
fn a_locked_component_layer_refuses_text_and_leaves_the_document_untouched() {
    let mut d = doc();
    d["componentTables"]["components"]["components"][0]["layers"][0]["locked"] = json!(true);
    let o = owner(&d);
    let before = state(&o);
    let made = prepare(&o, &json!({"command":"object.text.create","target":at("inner", 0, "", CTX),"block":block("Nope")}));
    assert_eq!(made.err(), Some(DispatchErrorCode::Unavailable));
    assert_eq!(state(&o), before);
}

#[test]
fn a_component_layer_holds_its_own_text_animators_one_entry_per_gesture_read_back_and_reopened() {
    let mut o = owner(&doc());
    let json = |o: &crate::history::NativeObjectHistory| serde_json::to_value(head(o).document()).unwrap();
    let before = json(&o);
    let animator = json!({"id":"ta-1","selector":{"start":0,"end":100,"basedOn":"chars"},"props":{"position":[20,0],"opacity":50}});
    let upsert = json!({"command":"object.text.animator.upsert","layerUid":"inner","contextId":CTX,"animator":animator});
    assert_eq!(run(&mut o, &upsert), Ok(1), "one revision");
    assert_eq!(o.history_depths(), (1, 0), "one history entry");
    let after = json(&o);
    let comp = &after["componentTables"]["components"]["components"][0]["layers"][0]["textAnimators"];
    assert_eq!(comp[0]["id"], "ta-1", "the component's layer holds it: {comp}");
    assert_eq!(after["layers"], before["layers"], "the scene's layers are untouched");
    let query = |layer: &str, ctx: Option<&str>, frame: u32| {
        let snap = head(&o);
        let mut view = json!({"kind":"animators","layerUid":layer,"frame":frame,"offset":0,"limit":1024});
        if let Some(c) = ctx {
            view["contextId"] = json!(c);
        }
        snap.query_text_json(json!({"instanceId":snap.instance_id(),"documentId":snap.document_id(),"atRevision":snap.content_revision(),
            "documentSnapshotId":snap.snapshot_id(),"view":view}).to_string().as_bytes())
    };
    let chunk = query("inner", Some(CTX), 0).unwrap();
    assert_eq!(chunk["contextId"], CTX);
    let data: Value = serde_json::from_str(chunk["text"].as_str().unwrap()).unwrap();
    assert_eq!(data["animators"][0]["id"], "ta-1");
    assert!(query("inner", None, 0).is_err(), "the scene has no layer 'inner'");
    assert!(query("inner", Some(CTX), 8).is_err(), "frame 8 is past the component's 8 frames");
    assert!(query("inner", Some("comp-b"), 0).is_err(), "an undeclared context");
    let edit = json!({"command":"object.text.animator.edit","layerUid":"inner","contextId":CTX,"animatorId":"ta-1","edit":{"kind":"setEnabled","enabled":false}});
    assert_eq!(run(&mut o, &edit), Ok(2));
    let remove = json!({"command":"object.text.animator.remove","layerUid":"inner","contextId":CTX,"animatorId":"ta-1"});
    assert_eq!(run(&mut o, &remove), Ok(3));
    o.undo(o.content_revision()).unwrap();
    o.undo(o.content_revision()).unwrap();
    assert_eq!(json(&o), after, "undo walks back to the upsert");
    let reopened = owner(&after);
    assert_eq!(json(&reopened), after, "saves and reopens unchanged");
    o.undo(o.content_revision()).unwrap();
    assert_eq!(json(&o), before);
}

#[test]
fn text_animators_in_a_component_refuse_an_undeclared_context_a_foreign_layer_and_a_locked_layer() {
    let o = owner(&doc());
    let animator = json!({"id":"ta-1","selector":{},"props":{}});
    let upsert = |layer: &str, ctx: &str| json!({"command":"object.text.animator.upsert","layerUid":layer,"contextId":ctx,"animator":animator});
    assert!(prepare(&o, &upsert("inner", "comp-b")).is_err(), "undeclared context");
    assert_eq!(prepare(&o, &upsert("L1", CTX)).err(), Some(DispatchErrorCode::NotFound), "a scene layer is not the component's");
    assert_eq!(prepare(&o, &upsert("inner", "")).err(), Some(DispatchErrorCode::InvalidRequest));
    let mut d = doc();
    d["componentTables"]["components"]["components"][0]["layers"][0]["locked"] = json!(true);
    assert_eq!(prepare(&owner(&d), &upsert("inner", CTX)).err(), Some(DispatchErrorCode::Unavailable));
}
