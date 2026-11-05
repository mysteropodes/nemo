//! #2267 T-text-context: the text-block transactions address a DECLARED component context like the scene (baseline: `enterSymbol`
//! swaps `state.layers`, so the Text tool places and edits text on the component's own layers).
use super::super::support::{head, owner, run, state, items_of};
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
