//! Owner-level frame-structure tests: the strict payload, typed refusals, hand-computed insert / extend / duplicate /
//! remove results on small dense documents, and the baseline lock policy. The oracle rows (verbatim baseline editor
//! at fbe4b8a3) run through the history owner in object_frame_edit_structure_history.rs.
//!
//! The lock cases need a native document that carries `locked: true`; that admission is the accepted F06 policy
//! (2435379d). Without it `ObjectFillOwner::new` refuses the document and these tests fail at their `expect`.
use super::*;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_frame_edit_structure_rows.rs"]
#[allow(dead_code)]
mod rows;

use build::{document, exposure_string, render_layer, Build};
use rows::parse_layers;

fn doc(text: &str) -> ObjectDocument {
    document(&parse_layers(text), Build::V1)
}
fn plan_doc(text: &str) -> ObjectDocument {
    document(&parse_layers(text), Build::V2)
}
fn owner(document: ObjectDocument) -> ObjectFillOwner {
    ObjectFillOwner::new("frame-structure", document).unwrap()
}
fn locked_owner(text: &str, locked: &[usize]) -> ObjectFillOwner {
    let mut document = doc(text);
    for &layer in locked {
        document.layers[layer].locked = Presence::Present(true);
    }
    ObjectFillOwner::new("frame-structure", document)
        .expect("a locked layer is admitted by the F06 layer policy (2435379d)")
}
fn head(o: &ObjectFillOwner) -> ObjectDocument {
    o.acquire_snapshot(o.content_revision())
        .unwrap()
        .document()
        .clone()
}
fn prepare(
    o: &ObjectFillOwner,
    payload: &Value,
) -> Result<PreparedFrameStructure, DispatchErrorCode> {
    o.prepare_frame_structure_json(payload.to_string().as_bytes())
}
/// Prepare and commit; the published document, or the typed refusal.
fn apply(o: &mut ObjectFillOwner, payload: Value) -> Result<ObjectDocument, DispatchErrorCode> {
    let candidate = prepare(o, &payload)?;
    o.commit_frame_structure(candidate)?;
    Ok(head(o))
}
fn text(document: &ObjectDocument) -> Vec<String> {
    (0..document.layers.len())
        .map(|layer| render_layer(document, layer))
        .collect()
}
fn insert(cell: u32) -> Value {
    json!({"command":"object.frames.insert","cell":cell})
}
fn insert_into(cell: u32, layers: &[&str]) -> Value {
    json!({"command":"object.frames.insert","cell":cell,"layerUids":layers})
}
fn extend(cell: u32, count: u32) -> Value {
    json!({"command":"object.frames.extend","cell":cell,"count":count})
}
fn duplicate(layer: &str, cell: u32) -> Value {
    json!({"command":"object.frames.duplicate","layerUid":layer,"cell":cell})
}
fn remove(from: u32, to: u32) -> Value {
    json!({"command":"object.frames.remove","from":from,"to":to})
}

const TWO: &str = "K=a.1.1;.;K=b.2.2;.|K;.;K;.";

#[test]
fn each_command_admits_exactly_its_own_fields_and_names_itself() {
    let o = owner(doc(TWO));
    for (payload, command) in [
        (insert(1), "object.frames.insert"),
        (insert_into(1, &["L1"]), "object.frames.insert"),
        (extend(1, 2), "object.frames.extend"),
        (duplicate("L0", 0), "object.frames.duplicate"),
        (remove(1, 2), "object.frames.remove"),
    ] {
        assert_eq!(prepare(&o, &payload).unwrap().command(), command);
    }
    assert_eq!(o.content_revision(), 0, "preparing publishes nothing");
}

#[test]
fn malformed_payloads_refuse_as_invalid_request_and_publish_nothing() {
    let o = owner(doc(TWO));
    let valid = extend(1, 2).to_string();
    let mut raw: Vec<String> = vec![
        "null".into(),
        "[]".into(),
        r#"["object.frames.extend",1,2]"#.into(),
        valid.replace(r#""count":2"#, r#""count":2,"count":3"#),
        valid.replace("}", r#","extra":true}"#),
        valid.replace(r#""cell":1"#, r#""cell":null"#),
        valid.replace(r#""cell":1"#, r#""cell":"1""#),
        valid.replace(r#""cell":1"#, r#""cell":1.5"#),
        valid.replace(r#""cell":1"#, r#""cell":-1"#),
        valid.replace(r#""cell":1"#, r#""cell":4294967296"#),
        valid.replace(r#""count":2"#, r#""count":"2""#),
        valid.replace(r#","count":2"#, ""),
        valid.replace("object.frames.extend", "object.frames.unknown"),
        valid.replace("object.frames.extend", "object.layer.create"),
        format!("{valid}{}", " ".repeat(4097)),
    ];
    // fields that belong to another command, and wrong shapes of the optional list
    for foreign in [
        r#"{"command":"object.frames.extend","cell":1,"count":2,"layerUids":["L0"]}"#,
        r#"{"command":"object.frames.insert","cell":1,"count":2}"#,
        r#"{"command":"object.frames.insert","cell":1,"from":0}"#,
        r#"{"command":"object.frames.insert","cell":1,"layerUids":[]}"#,
        r#"{"command":"object.frames.insert","cell":1,"layerUids":["L0","L0"]}"#,
        r#"{"command":"object.frames.insert","cell":1,"layerUids":[""]}"#,
        r#"{"command":"object.frames.insert","cell":1,"layerUids":[1]}"#,
        r#"{"command":"object.frames.insert","cell":1,"layerUids":null}"#,
        r#"{"command":"object.frames.insert","cell":1,"layerUids":"L0"}"#,
        r#"{"command":"object.frames.insert"}"#,
        r#"{"command":"object.frames.duplicate","cell":0}"#,
        r#"{"command":"object.frames.duplicate","layerUid":"","cell":0}"#,
        r#"{"command":"object.frames.duplicate","layerUid":"L0","cell":0,"count":1}"#,
        r#"{"command":"object.frames.remove","from":0}"#,
        r#"{"command":"object.frames.remove","from":0,"to":1,"cell":0}"#,
    ] {
        raw.push(foreign.to_owned());
    }
    for payload in raw {
        assert_eq!(
            o.prepare_frame_structure_json(payload.as_bytes())
                .unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{payload}"
        );
    }
    assert_eq!(o.content_revision(), 0);
}

#[test]
fn semantic_refusals_are_typed_and_publish_nothing() {
    let mut o = owner(doc(TWO));
    let before = head(&o);
    for (payload, code) in [
        (insert(4), DispatchErrorCode::InvalidRequest),
        (insert_into(1, &["nope"]), DispatchErrorCode::NotFound),
        (extend(4, 1), DispatchErrorCode::InvalidRequest),
        (extend(0, 1000), DispatchErrorCode::InvalidRequest),
        (duplicate("nope", 0), DispatchErrorCode::NotFound),
        (duplicate("L0", 4), DispatchErrorCode::InvalidRequest),
        (remove(2, 1), DispatchErrorCode::InvalidRequest),
        (remove(0, 4), DispatchErrorCode::InvalidRequest),
    ] {
        assert_eq!(
            apply(&mut o, payload.clone()).unwrap_err(),
            code,
            "{payload}"
        );
    }
    assert_eq!((o.content_revision(), head(&o)), (0, before));
}

#[test]
fn insert_splices_the_selected_layers_and_pads_the_others_at_the_end() {
    let mut o = owner(doc(TWO));
    let out = apply(&mut o, insert_into(1, &["L1"])).unwrap();
    // L1 gets a hold after frame 1 and its later key moves; L0 only grows by a trailing hold.
    assert_eq!(text(&out), ["K=a.1.1;.;K=b.2.2;.;.", "K;.;.;K;."]);
    assert_eq!(out.total_frames, 5);
    let mut all = owner(doc(TWO));
    let out = apply(&mut all, insert(1)).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;.;K=b.2.2;.", "K;.;.;K;."]);
}

#[test]
fn extend_splices_every_layer_by_the_requested_count() {
    let mut o = owner(doc(TWO));
    let out = apply(&mut o, extend(0, 3)).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;.;.;.;K=b.2.2;.", "K;.;.;.;.;K;."]);
    assert_eq!(out.total_frames, 7);
    assert_eq!(o.content_revision(), 1);
    assert_eq!(exposure_string(&out, 0), "*0000*5");
}

#[test]
fn extend_with_a_zero_count_is_a_semantic_no_op() {
    let mut o = owner(doc(TWO));
    let candidate = prepare(&o, &extend(1, 0)).unwrap();
    assert_eq!(o.commit_frame_structure(candidate), Ok(0));
    assert_eq!(o.content_revision(), 0);
}

#[test]
fn duplicate_copies_what_the_cell_shows_with_the_original_ids() {
    // a key
    let mut o = owner(doc("K=a.1.1;.|K;."));
    let out = apply(&mut o, duplicate("L0", 0)).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;K=a.1.1;.", "K;.;."]);
    // a hold shows its key
    let mut o = owner(doc("K=a.1.1;.;.|K;.;."));
    let out = apply(&mut o, duplicate("L0", 2)).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;.;K=a.1.1", "K;.;.;."]);
    // an in-between shows its OWN strokes, not the key's (the baseline quirk)
    let mut o = owner(doc("K=a.1.1;I=s.3.3;.|K;.;."));
    let out = apply(&mut o, duplicate("L0", 1)).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;I=s.3.3;K=s.3.3;.", "K;.;.;."]);
    assert_eq!(exposure_string(&out, 0), "*~*2");
}

#[test]
fn duplicate_with_nothing_to_show_is_a_semantic_no_op() {
    for (layers, layer, cell) in [
        (".;K=a.1.1|K;.", "L0", 0),
        ("K;.|K=a.1.1;.", "L0", 1),
        ("K=a.1.1;I|K;.", "L0", 1),
    ] {
        let mut o = owner(doc(layers));
        let candidate = prepare(&o, &duplicate(layer, cell)).unwrap();
        assert_eq!(
            o.commit_frame_structure(candidate),
            Ok(0),
            "{layers} {cell}"
        );
        assert_eq!(o.content_revision(), 0);
    }
}

#[test]
fn remove_drops_the_span_and_never_empties_the_timeline() {
    let mut o = owner(doc(TWO));
    let out = apply(&mut o, remove(1, 2)).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.", "K;."]);
    let mut o = owner(doc(TWO));
    let candidate = prepare(&o, &remove(0, 3)).unwrap();
    assert_eq!(o.commit_frame_structure(candidate), Ok(0));
    assert_eq!(head(&o), doc(TWO));
}

#[test]
fn a_version_two_order_plan_follows_every_operation() {
    let mut o = owner(plan_doc(TWO));
    let out = apply(&mut o, duplicate("L0", 2)).unwrap();
    let triples = |frame| build::element_triples(&out, "L0", frame);
    assert_eq!(triples(2), [("b".to_owned(), 0, 2)]);
    assert_eq!(
        triples(3),
        [("b".to_owned(), 0, 2)],
        "the copy keeps id, natural position and order"
    );
    assert!(triples(1).is_empty());
}

#[test]
fn locked_layers_follow_the_baseline_policy() {
    // F5 with a selection: the locked layer only grows by the end pad.
    let mut o = locked_owner(TWO, &[0]);
    let out = apply(&mut o, insert_into(1, &["L0", "L1"])).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;K=b.2.2;.;.", "K;.;.;K;."]);
    // F5 without a selection skips locked layers too.
    let mut o = locked_owner(TWO, &[1]);
    let out = apply(&mut o, insert(1)).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;.;K=b.2.2;.", "K;.;K;.;."]);
    // an explicit selection of locked layers only has nothing to splice and refuses
    let mut o = locked_owner(TWO, &[0]);
    assert_eq!(
        apply(&mut o, insert_into(1, &["L0"])).unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    assert_eq!(o.content_revision(), 0);
    // no selection and every layer locked: the timeline still grows by one hold on every layer
    let mut o = locked_owner(TWO, &[0, 1]);
    let out = apply(&mut o, insert(1)).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;K=b.2.2;.;.", "K;.;K;.;."]);
    assert_eq!(out.total_frames, 5);
    // extend and remove ignore locks
    let mut o = locked_owner(TWO, &[0, 1]);
    let out = apply(&mut o, extend(1, 1)).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;.;K=b.2.2;.", "K;.;.;K;."]);
    let out = apply(&mut o, remove(0, 0)).unwrap();
    assert_eq!(out.total_frames, 4);
    // duplicate refuses a locked source layer but splices other locked layers
    let mut o = locked_owner(TWO, &[0]);
    assert_eq!(
        apply(&mut o, duplicate("L0", 0)).unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    let mut o = locked_owner(TWO, &[1]);
    let out = apply(&mut o, duplicate("L0", 0)).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;K=a.1.1;.;K=b.2.2;.", "K;.;.;K;."]);
}

#[test]
fn a_layer_without_authored_timing_is_typed_unavailable_and_never_invented() {
    let mut value = build::document_json(&parse_layers(TWO), Build::V1);
    value["layers"][1].as_object_mut().unwrap().remove("timing");
    let untimed = build::decode(&value);
    // every operation that has to splice the untimed layer refuses
    for payload in [
        insert(1),
        insert_into(1, &["L1"]),
        extend(1, 1),
        duplicate("L0", 0),
    ] {
        let mut o = owner(untimed.clone());
        assert_eq!(
            apply(&mut o, payload.clone()).unwrap_err(),
            DispatchErrorCode::Unavailable,
            "{payload}"
        );
        assert_eq!(o.content_revision(), 0);
    }
    // an untargeted untimed layer is simply left alone by F5, as its length is not authored
    let mut o = owner(untimed);
    let out = apply(&mut o, insert_into(1, &["L0"])).unwrap();
    assert_eq!(out.total_frames, 5);
    assert!(out.layers[1].timing.missing());
    assert_eq!(render_layer(&out, 0), "K=a.1.1;.;.;K=b.2.2;.");
}
