//! Owner-level key-exposure tests: the strict payload, typed refusals, hand-computed F6 / Clear Keyframe / Delete Frames /
//! Remove Tween results on small dense documents (each literal printed by tools/hand_cases.js from the JS model that
//! model_check.js proves against the baseline rows), and the baseline lock policy. The oracle rows (verbatim baseline editor
//! at fbe4b8a3) run through the history owner in object_exposure_edit_key_history.rs.
//!
//! The lock cases need a native document that carries `locked: true`; that admission is the accepted F06 policy
//! (2435379d). Without it `ObjectFillOwner::new` refuses the document and these tests fail at their `expect`.
use super::*;
use crate::object_codec::encode_storage_project;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;

use build::{document, element_triples, exposure_string, render_layer, Build};
use rows::parse_layers;

fn doc(text: &str) -> ObjectDocument {
    document(&parse_layers(text), Build::V1)
}
fn plan_doc(text: &str) -> ObjectDocument {
    document(&parse_layers(text), Build::V2)
}
fn owner(document: ObjectDocument) -> ObjectFillOwner {
    ObjectFillOwner::new("exposure-key", document).unwrap()
}
fn locked_owner(text: &str, locked: &[usize]) -> ObjectFillOwner {
    let mut document = doc(text);
    for &layer in locked {
        document.layers[layer].locked = Presence::Present(true);
    }
    ObjectFillOwner::new("exposure-key", document)
        .expect("a locked layer is admitted by the F06 layer policy (2435379d)")
}
fn head(o: &ObjectFillOwner) -> ObjectDocument {
    o.acquire_snapshot(o.content_revision())
        .unwrap()
        .document()
        .clone()
}
fn prepare(o: &ObjectFillOwner, payload: &Value) -> Result<PreparedExposureKey, DispatchErrorCode> {
    o.prepare_exposure_key_json(payload.to_string().as_bytes())
}
/// Prepare and commit; the published document, or the typed refusal.
fn apply(o: &mut ObjectFillOwner, payload: Value) -> Result<ObjectDocument, DispatchErrorCode> {
    let candidate = prepare(o, &payload)?;
    o.commit_exposure_key(candidate)?;
    Ok(head(o))
}
fn text(document: &ObjectDocument) -> Vec<String> {
    (0..document.layers.len())
        .map(|layer| render_layer(document, layer))
        .collect()
}
fn exposures(document: &ObjectDocument) -> Vec<String> {
    (0..document.layers.len())
        .map(|layer| exposure_string(document, layer))
        .collect()
}
fn promote(cells: &[u32]) -> Value {
    json!({"command":"object.layer.key.promote","cells":cells})
}
fn promote_in(cells: &[u32], layers: &[&str]) -> Value {
    json!({"command":"object.layer.key.promote","cells":cells,"layerUids":layers})
}
fn clear(layer: &str, cell: u32) -> Value {
    json!({"command":"object.layer.key.clear","layerUid":layer,"cell":cell})
}
fn clear_cells(targets: &[(&str, u32)]) -> Value {
    let list: Vec<Value> = targets
        .iter()
        .map(|(layer, frame)| json!({"layerUid":layer,"frame":frame}))
        .collect();
    json!({"command":"object.layer.key.clear_cells","targets":list})
}
fn tween(layer: &str, from: u32, to: u32) -> Value {
    json!({"command":"object.layer.tween.remove","layerUid":layer,"from":from,"to":to})
}

const TWO: &str = "K=a.1.1;.;K=b.2.2;.|K;.;K;.";
const SAMPLES: &str = "K=a.1.1;I=s.3.3;.|K;.;.";
const TWEENED: &str = "K=a.1.1;I=s.3.3;M=t.4.4;K=b.2.2|K;.;.;.";

#[test]
fn each_command_admits_exactly_its_own_fields_and_names_itself() {
    let o = owner(doc(TWO));
    for (payload, command) in [
        (promote(&[1]), "object.layer.key.promote"),
        (promote_in(&[1, 3], &["L1"]), "object.layer.key.promote"),
        (clear("L0", 0), "object.layer.key.clear"),
        (
            clear_cells(&[("L0", 0), ("L1", 2)]),
            "object.layer.key.clear_cells",
        ),
        (tween("L0", 0, 3), "object.layer.tween.remove"),
    ] {
        assert_eq!(prepare(&o, &payload).unwrap().command(), command);
    }
    assert_eq!(o.content_revision(), 0, "preparing publishes nothing");
}

#[test]
fn malformed_payloads_refuse_as_invalid_request_and_publish_nothing() {
    let o = owner(doc(TWO));
    let valid = promote_in(&[1], &["L0"]).to_string();
    let mut raw: Vec<String> = vec![
        "null".into(),
        "[]".into(),
        r#"["object.layer.key.promote",[1]]"#.into(),
        valid.replace(r#""cells":[1]"#, r#""cells":[1],"cells":[2]"#),
        valid.replace('}', r#","extra":true}"#),
        valid.replace(r#""cells":[1]"#, r#""cells":null"#),
        valid.replace(r#""cells":[1]"#, r#""cells":"1""#),
        valid.replace(r#""cells":[1]"#, r#""cells":[1.5]"#),
        valid.replace(r#""cells":[1]"#, r#""cells":[-1]"#),
        valid.replace(r#""cells":[1]"#, r#""cells":[4294967296]"#),
        valid.replace(r#""cells":[1]"#, r#""cells":["1"]"#),
        valid.replace(r#""cells":[1]"#, r#""cells":[]"#),
        valid.replace(r#""cells":[1]"#, r#""cells":[1,1]"#),
        valid.replace(r#""cells":[1],"#, ""),
        valid.replace("object.layer.key.promote", "object.layer.key.unknown"),
        valid.replace("object.layer.key.promote", "object.layer.create"),
        format!("{valid}{}", " ".repeat(131_073)),
    ];
    // fields that belong to another command, and wrong shapes of the optional and required lists
    for other in [
        r#"{"command":"object.layer.key.promote","cells":[1],"layerUids":[]}"#,
        r#"{"command":"object.layer.key.promote","cells":[1],"layerUids":["L0","L0"]}"#,
        r#"{"command":"object.layer.key.promote","cells":[1],"layerUids":[""]}"#,
        r#"{"command":"object.layer.key.promote","cells":[1],"layerUids":[1]}"#,
        r#"{"command":"object.layer.key.promote","cells":[1],"layerUids":null}"#,
        r#"{"command":"object.layer.key.promote","cells":[1],"layerUids":"L0"}"#,
        r#"{"command":"object.layer.key.promote","cells":[1],"layerUid":"L0"}"#,
        r#"{"command":"object.layer.key.promote","cells":[1],"cell":1}"#,
        r#"{"command":"object.layer.key.promote","cells":[1],"targets":[]}"#,
        r#"{"command":"object.layer.key.promote","cells":[1],"from":0}"#,
        r#"{"command":"object.layer.key.promote","layerUids":["L0"]}"#,
        r#"{"command":"object.layer.key.clear","cell":0}"#,
        r#"{"command":"object.layer.key.clear","layerUid":"","cell":0}"#,
        r#"{"command":"object.layer.key.clear","layerUid":"L0"}"#,
        r#"{"command":"object.layer.key.clear","layerUid":"L0","cell":0,"cells":[0]}"#,
        r#"{"command":"object.layer.key.clear","layerUid":"L0","cell":0,"layerUids":["L0"]}"#,
        r#"{"command":"object.layer.key.clear_cells"}"#,
        r#"{"command":"object.layer.key.clear_cells","targets":[]}"#,
        r#"{"command":"object.layer.key.clear_cells","targets":null}"#,
        r#"{"command":"object.layer.key.clear_cells","targets":[{"layerUid":"L0","frame":0},{"layerUid":"L0","frame":0}]}"#,
        r#"{"command":"object.layer.key.clear_cells","targets":[{"layerUid":"","frame":0}]}"#,
        r#"{"command":"object.layer.key.clear_cells","targets":[{"layerUid":"L0","frame":-1}]}"#,
        r#"{"command":"object.layer.key.clear_cells","targets":[{"layerUid":"L0"}]}"#,
        r#"{"command":"object.layer.key.clear_cells","targets":[{"layerUid":"L0","frame":0,"extra":1}]}"#,
        r#"{"command":"object.layer.key.clear_cells","targets":[["L0",0]]}"#,
        r#"{"command":"object.layer.key.clear_cells","targets":[{"layerUid":"L0","frame":0}],"cells":[0]}"#,
        r#"{"command":"object.layer.tween.remove","layerUid":"L0","from":0}"#,
        r#"{"command":"object.layer.tween.remove","layerUid":"","from":0,"to":1}"#,
        r#"{"command":"object.layer.tween.remove","layerUid":"L0","from":0,"to":1,"cell":0}"#,
        r#"{"command":"object.layer.tween.remove","layerUid":"L0","from":-1,"to":1}"#,
    ] {
        raw.push(other.to_owned());
    }
    for payload in raw {
        assert_eq!(
            o.prepare_exposure_key_json(payload.as_bytes()).unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{}",
            &payload[..payload.len().min(160)]
        );
    }
    assert_eq!(o.content_revision(), 0);
}

#[test]
fn semantic_refusals_are_typed_and_publish_nothing() {
    let mut o = owner(doc(TWO));
    let before = head(&o);
    for (payload, code) in [
        (promote(&[4]), DispatchErrorCode::InvalidRequest),
        (promote(&[1, 4]), DispatchErrorCode::InvalidRequest),
        (promote_in(&[1], &["nope"]), DispatchErrorCode::NotFound),
        (
            promote_in(&[1], &["L0", "nope"]),
            DispatchErrorCode::NotFound,
        ),
        (clear("nope", 0), DispatchErrorCode::NotFound),
        (clear("L0", 4), DispatchErrorCode::InvalidRequest),
        (
            clear_cells(&[("L0", 0), ("nope", 0)]),
            DispatchErrorCode::NotFound,
        ),
        (
            clear_cells(&[("L0", 0), ("L1", 4)]),
            DispatchErrorCode::InvalidRequest,
        ),
        (tween("nope", 0, 2), DispatchErrorCode::NotFound),
        (tween("L0", 2, 1), DispatchErrorCode::InvalidRequest),
        (tween("L0", 0, 4), DispatchErrorCode::InvalidRequest),
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
fn every_frame_edit_error_maps_to_its_typed_dispatch_code() {
    for (error, code) in [
        (FrameEditError::Invalid, DispatchErrorCode::InvalidRequest),
        (FrameEditError::UnknownLayer, DispatchErrorCode::NotFound),
        (FrameEditError::Unsupported, DispatchErrorCode::Unavailable),
        (
            FrameEditError::MissingTiming,
            DispatchErrorCode::Unavailable,
        ),
        (
            FrameEditError::PreservedOutOfRange,
            DispatchErrorCode::Unavailable,
        ),
    ] {
        assert_eq!(refusal(error), code, "{error:?}");
    }
}

#[test]
fn promote_copies_what_the_cell_shows_with_the_original_ids() {
    // a hold shows its key: every layer gets a key at the cell, L0 with a copy of key a, L1 with its blank key
    let mut o = owner(doc(TWO));
    let out = apply(&mut o, promote(&[1])).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;K=a.1.1;K=b.2.2;.", "K;K;K;."]);
    assert_eq!(exposures(&out), ["***2", "***2"]);
    assert_eq!(o.content_revision(), 1);
    // only the named layer changes
    let mut o = owner(doc(TWO));
    let out = apply(&mut o, promote_in(&[1], &["L1"])).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;K=b.2.2;.", "K;K;K;."]);
    // the last cell shows the later key
    let mut o = owner(doc(TWO));
    let out = apply(&mut o, promote_in(&[3], &["L0"])).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;K=b.2.2;K=b.2.2", "K;.;K;."]);
    assert_eq!(exposures(&out), ["*0**", "*0*2"]);
    // a cell list promotes each hold
    let mut o = owner(doc(TWO));
    let out = apply(&mut o, promote(&[1, 3])).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;K=a.1.1;K=b.2.2;K=b.2.2", "K;K;K;K"]);
    // an in-between keeps its OWN strokes, an automatic or a manual one alike
    for sample in ["I", "M"] {
        let mut o = owner(doc(&SAMPLES.replace('I', sample)));
        let out = apply(&mut o, promote_in(&[1], &["L0"])).unwrap();
        assert_eq!(text(&out), ["K=a.1.1;K=s.3.3;.", "K;.;."]);
        assert_eq!(exposures(&out), ["**1", "*00"]);
    }
    // a blank key is not copied; a cell before every key becomes a blank key
    let mut o = owner(doc("K;.;K=a.1.1|K;.;."));
    let out = apply(&mut o, promote_in(&[1], &["L0"])).unwrap();
    assert_eq!(text(&out), ["K;K;K=a.1.1", "K;.;."]);
    let mut o = owner(doc(".;.;K=a.1.1;.|K;.;.;."));
    let out = apply(&mut o, promote_in(&[0, 1], &["L0"])).unwrap();
    assert_eq!(text(&out), ["K;K;K=a.1.1;.", "K;.;.;."]);
    assert_eq!(exposures(&out), ["***2", "*000"]);
}

#[test]
fn promote_cell_order_decides_what_a_later_hold_shows() {
    // promoted one after another: after cell 1 became a key the hold at 2 shows IT, in the other order the original key a
    let mut o = owner(doc(SAMPLES));
    let out = apply(&mut o, promote_in(&[1, 2], &["L0"])).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;K=s.3.3;K=s.3.3", "K;.;."]);
    let mut o = owner(doc(SAMPLES));
    let out = apply(&mut o, promote_in(&[2, 1], &["L0"])).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;K=s.3.3;K=a.1.1", "K;.;."]);
}

#[test]
fn a_layer_selection_is_one_atomic_edit_applied_in_document_order() {
    let one = {
        let mut o = owner(doc(TWO));
        apply(&mut o, promote_in(&[1], &["L0", "L1"])).unwrap()
    };
    let other = {
        let mut o = owner(doc(TWO));
        apply(&mut o, promote_in(&[1], &["L1", "L0"])).unwrap()
    };
    assert_eq!(
        one, other,
        "the order the caller names layers in changes nothing"
    );
    assert_eq!(
        encode_storage_project(&one).unwrap(),
        encode_storage_project(&other).unwrap()
    );
    assert_eq!(text(&one), ["K=a.1.1;K=a.1.1;K=b.2.2;.", "K;K;K;."]);
}

#[test]
fn keys_that_are_already_keys_are_a_semantic_no_op() {
    for cells in [&[0u32][..], &[0, 2][..]] {
        let mut o = owner(doc(TWO));
        let candidate = prepare(&o, &promote(cells)).unwrap();
        assert_eq!(o.commit_exposure_key(candidate), Ok(0));
        assert_eq!(o.content_revision(), 0);
        assert_eq!(head(&o), doc(TWO));
    }
}

#[test]
fn clear_key_leaves_the_previous_key_held_and_an_unreached_start_unexposed() {
    let mut o = owner(doc(TWO));
    let out = apply(&mut o, clear("L0", 2)).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;.;.", "K;.;K;."]);
    assert_eq!(exposures(&out), ["*000", "*0*2"]);
    // frame 0 has no earlier key: the layer is unexposed there, the later key stays
    let mut o = owner(doc(TWO));
    let out = apply(&mut o, clear("L1", 0)).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;K=b.2.2;.", ".;.;K;."]);
    assert_eq!(exposures(&out), ["*0*2", "--*2"]);
    // a hold and an in-between are not keys: nothing to clear
    for (layers, cell) in [(TWO, 1), (SAMPLES, 1)] {
        let mut o = owner(doc(layers));
        let candidate = prepare(&o, &clear("L0", cell)).unwrap();
        assert_eq!(o.commit_exposure_key(candidate), Ok(0), "{layers} {cell}");
        assert_eq!(o.content_revision(), 0);
    }
}

#[test]
fn clear_cells_turn_keys_and_in_betweens_into_holds_across_layers() {
    let mut o = owner(doc(TWO));
    let out = apply(&mut o, clear_cells(&[("L0", 0), ("L1", 2)])).unwrap();
    assert_eq!(text(&out), [".;.;K=b.2.2;.", "K;.;.;."]);
    assert_eq!(exposures(&out), ["--*2", "*000"]);
    let mut o = owner(doc(SAMPLES));
    let out = apply(&mut o, clear_cells(&[("L0", 1)])).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;.", "K;.;."]);
    let mut o = owner(doc(TWO));
    let candidate = prepare(&o, &clear_cells(&[("L0", 1), ("L1", 3)])).unwrap();
    assert_eq!(
        o.commit_exposure_key(candidate),
        Ok(0),
        "holds are already holds"
    );
    assert_eq!(o.content_revision(), 0);
}

#[test]
fn remove_tween_removes_only_what_is_strictly_between_the_pair() {
    let mut o = owner(doc(TWEENED));
    let out = apply(&mut o, tween("L0", 0, 3)).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;.;K=b.2.2", "K;.;.;."]);
    assert_eq!(exposures(&out), ["*00*", "*000"]);
    assert_eq!(out.total_frames, 4, "the frame count is untouched");
    // adjacent keys have nothing between them
    let mut o = owner(doc(TWEENED));
    let candidate = prepare(&o, &tween("L0", 0, 1)).unwrap();
    assert_eq!(o.commit_exposure_key(candidate), Ok(0));
    assert_eq!(o.content_revision(), 0);
    // like the dense editor, a key sitting inside the span goes too
    let mut o = owner(doc("K=a.1.1;K=c.5.5;I=s.3.3;K=b.2.2|K;.;.;."));
    let out = apply(&mut o, tween("L0", 0, 3)).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;.;K=b.2.2", "K;.;.;."]);
}

#[test]
fn a_version_two_order_plan_follows_every_operation() {
    let mut o = owner(plan_doc(TWO));
    let out = apply(&mut o, promote_in(&[1], &["L0"])).unwrap();
    let triples = |document: &ObjectDocument, frame| element_triples(document, "L0", frame);
    assert_eq!(triples(&out, 1), [("a".to_owned(), 0, 1)]);
    assert_eq!(
        triples(&out, 2),
        [("b".to_owned(), 0, 2)],
        "the copy keeps id, natural position and order"
    );
    let out = apply(&mut o, clear("L0", 2)).unwrap();
    assert!(triples(&out, 2).is_empty());
    assert_eq!(triples(&out, 1), [("a".to_owned(), 0, 1)]);
}

#[test]
fn locked_layers_follow_the_baseline_policy() {
    // F6 skips a locked layer and still changes the others
    let mut o = locked_owner(TWO, &[0]);
    let out = apply(&mut o, promote(&[1])).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;K=b.2.2;.", "K;K;K;."]);
    let mut o = locked_owner(TWO, &[1]);
    let out = apply(&mut o, promote(&[1])).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;K=a.1.1;K=b.2.2;.", "K;.;K;."]);
    // a locked layer that is not targeted stays out of the way
    let mut o = locked_owner(TWO, &[0]);
    let out = apply(&mut o, promote_in(&[1], &["L1"])).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;K=b.2.2;.", "K;K;K;."]);
    // nothing could change and a locked layer was in the way: the baseline's "layer locked", typed Unavailable
    for (locked, payload) in [
        (vec![0], promote_in(&[1], &["L0"])),
        (vec![0], promote(&[2])),
        (vec![0, 1], promote(&[1])),
        (vec![0], clear("L0", 0)),
        (vec![0], clear("L0", 1)),
        (vec![0], clear_cells(&[("L0", 0)])),
        (vec![0], clear_cells(&[("L0", 0), ("L1", 1)])),
        (vec![0], tween("L0", 0, 2)),
    ] {
        let mut o = locked_owner(TWO, &locked);
        assert_eq!(
            apply(&mut o, payload.clone()).unwrap_err(),
            DispatchErrorCode::Unavailable,
            "{locked:?} {payload}"
        );
        assert_eq!(o.content_revision(), 0);
    }
    // Delete Frames skips the locked layer and clears the rest
    let mut o = locked_owner(TWO, &[0]);
    let out = apply(&mut o, clear_cells(&[("L0", 0), ("L1", 2)])).unwrap();
    assert_eq!(text(&out), ["K=a.1.1;.;K=b.2.2;.", "K;.;.;."]);
}

#[test]
fn a_layer_without_authored_timing_is_typed_unavailable_and_never_invented() {
    let mut value = build::document_json(&parse_layers(TWO), Build::V1);
    value["layers"][1].as_object_mut().unwrap().remove("timing");
    let untimed = build::decode(&value);
    // an untimed layer that has to change refuses the whole request: L0 would have changed, nothing is published
    for payload in [
        promote(&[1]),
        promote_in(&[1], &["L0", "L1"]),
        clear("L1", 0),
        clear_cells(&[("L0", 0), ("L1", 0)]),
        tween("L1", 0, 2),
    ] {
        let mut o = owner(untimed.clone());
        assert_eq!(
            apply(&mut o, payload.clone()).unwrap_err(),
            DispatchErrorCode::Unavailable,
            "{payload}"
        );
        assert_eq!(o.content_revision(), 0);
        assert_eq!(head(&o), untimed);
    }
    // an untimed layer that is not named is simply left alone
    let mut o = owner(untimed);
    let out = apply(&mut o, promote_in(&[1], &["L0"])).unwrap();
    assert!(out.layers[1].timing.missing());
    assert_eq!(render_layer(&out, 0), "K=a.1.1;K=a.1.1;K=b.2.2;.");
}
