//! Owner-level key-retime tests: the strict payload, typed refusals, hand-computed `moveKeyframe` results on small
//! dense documents (each literal printed by tools/hand_cases.js from the JS model that model_check.js proves against the
//! baseline rows), the entry cap of the dense hole fill, the version-two order plan and the lock policy. The oracle rows
//! (verbatim baseline editor at fbe4b8a3) run through the history owner in object_exposure_edit_retime_history.rs.
//!
//! The lock cases need a native document that carries `locked: true`; that admission is the accepted F06 policy
//! (2435379d). Without it `ObjectFillOwner::new` refuses the document and these tests fail at their `expect`.
use super::*;
use crate::object_document::Presence;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_exposure_edit_retime_rows.rs"]
#[allow(dead_code)]
mod rows;

use build::{
    document, element_triples, expected_triples, exposure_string, render_layer, scope_shape, uid,
    untouched, Build,
};
use rows::parse_layers;

fn owner(document: ObjectDocument) -> ObjectFillOwner {
    ObjectFillOwner::new("exposure-retime", document).unwrap()
}
fn locked_owner(text: &str, locked: &[usize]) -> ObjectFillOwner {
    let mut input = document(&parse_layers(text), Build::V1);
    for &layer in locked {
        input.layers[layer].locked = Presence::Present(true);
    }
    ObjectFillOwner::new("exposure-retime", input)
        .expect("a locked layer is admitted by the F06 layer policy (2435379d)")
}
fn head(o: &ObjectFillOwner) -> ObjectDocument {
    o.acquire_snapshot(o.content_revision())
        .unwrap()
        .document()
        .clone()
}
fn mv(layer: &str, from: u32, to: u32) -> Value {
    json!({"command":"object.layer.key.move","layerUid":layer,"from":from,"to":to})
}
fn prepare(
    o: &ObjectFillOwner,
    payload: &Value,
) -> Result<PreparedExposureRetime, DispatchErrorCode> {
    o.prepare_exposure_retime_json(payload.to_string().as_bytes())
}
/// Prepare and commit; the published document, or the typed refusal.
fn apply(o: &mut ObjectFillOwner, payload: Value) -> Result<ObjectDocument, DispatchErrorCode> {
    let candidate = prepare(o, &payload)?;
    o.commit_exposure_retime(candidate)?;
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
fn pairs_of(candidate: &PreparedExposureRetime) -> Vec<[u32; 4]> {
    candidate
        .pairs()
        .iter()
        .map(|pair| [pair.from_a, pair.from_b, pair.to_a, pair.to_b])
        .collect()
}
type Moved = (Vec<String>, Vec<String>, Option<u32>, Vec<[u32; 4]>);
/// One move in all four document forms: the same cells, exposures, landing frame and pair list in every form, the
/// v2 order elements of every cell equal to the cell's strokes, nothing outside the layer's records changed, and
/// the published document admitted.
fn moved(layers: &str, layer: &str, from: u32, to: u32) -> Moved {
    let mut seen: Option<Moved> = None;
    for form in Build::ALL {
        let mut o = owner(document(&parse_layers(layers), form));
        let input = head(&o);
        let candidate = prepare(&o, &mv(layer, from, to)).unwrap();
        let (applied, pairs) = (candidate.applied_to(), pairs_of(&candidate));
        o.commit_exposure_retime(candidate).unwrap();
        let out = head(&o);
        let got = (text(&out), exposures(&out), applied, pairs);
        if form.v2() {
            for (index, cells) in parse_layers(&got.0.join("|")).iter().enumerate() {
                for (frame, cell) in cells.iter().enumerate() {
                    assert_eq!(
                        element_triples(&out, &uid(index), frame as u32),
                        expected_triples(cell),
                        "{form:?} layer {index} frame {frame}"
                    );
                }
            }
        }
        assert_eq!(untouched(&input), untouched(&out), "{form:?}");
        ObjectSnapshot::validate_drawing(&out)
            .unwrap_or_else(|error| panic!("{form:?}: storage admission: {error}"));
        match &seen {
            None => seen = Some(got),
            Some(first) => assert_eq!(first, &got, "{form:?}"),
        }
    }
    seen.unwrap()
}
fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|item| (*item).to_owned()).collect()
}

const BARE7: &str = "K;.;.;.;.;.;.";
fn with_bare(first: &str) -> String {
    format!("{first}|{BARE7}")
}

#[test]
fn the_command_admits_exactly_its_own_fields() {
    let o = owner(document(
        &parse_layers("K=a.1.1;.;K=b.2.2;.|K;.;.;."),
        Build::V1,
    ));
    assert!(prepare(&o, &mv("L0", 0, 1)).is_ok());
    let valid = mv("L0", 0, 1).to_string();
    let raw: Vec<String> = vec![
        "null".into(),
        "[]".into(),
        r#"["object.layer.key.move","L0",0,1]"#.into(),
        valid.replace(r#""from":0"#, r#""from":0,"from":1"#),
        valid.replace('}', r#","extra":true}"#),
        valid.replace(r#","from":0"#, ""),
        valid.replace(r#","to":1"#, ""),
        valid.replace(r#""layerUid":"L0","#, ""),
        valid.replace(r#""command":"object.layer.key.move","#, ""),
        valid.replace(r#""from":0"#, r#""from":null"#),
        valid.replace(r#""to":1"#, r#""to":null"#),
        valid.replace(r#""from":0"#, r#""from":"0""#),
        valid.replace(r#""to":1"#, r#""to":1.5"#),
        valid.replace(r#""from":0"#, r#""from":-1"#),
        valid.replace(r#""to":1"#, r#""to":4294967296"#),
        valid.replace(r#""layerUid":"L0""#, r#""layerUid":""#),
        valid.replace(r#""layerUid":"L0""#, r#""layerUid":0"#),
        valid.replace(r#""layerUid":"L0""#, r#""layerUid":null"#),
        valid.replace("object.layer.key.move", "object.layer.key.promote"),
        valid.replace("object.layer.key.move", "object.layer.create"),
        valid.replace("object.layer.key.move", ""),
        format!("{valid}{}", " ".repeat(4097)),
    ];
    for payload in raw {
        assert_eq!(
            o.prepare_exposure_retime_json(payload.as_bytes())
                .unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{payload}"
        );
    }
    assert_eq!(o.content_revision(), 0, "preparing publishes nothing");
}

#[test]
fn semantic_refusals_are_typed_and_publish_nothing() {
    let mut o = owner(document(
        &parse_layers("K=a.1.1;I=s.3.3;K=b.2.2;.|K;.;.;."),
        Build::V1,
    ));
    let before = head(&o);
    for (payload, code) in [
        (mv("nope", 0, 1), DispatchErrorCode::NotFound),
        (mv("nope", 0, 0), DispatchErrorCode::NotFound),
        (mv("L0", 1, 2), DispatchErrorCode::InvalidRequest),
        (mv("L0", 3, 2), DispatchErrorCode::InvalidRequest),
        (mv("L1", 1, 3), DispatchErrorCode::InvalidRequest),
        (mv("L0", 0, 4), DispatchErrorCode::InvalidRequest),
        (mv("L0", 0, 400), DispatchErrorCode::InvalidRequest),
        (mv("L0", 4, 0), DispatchErrorCode::InvalidRequest),
        (mv("L0", 400, 0), DispatchErrorCode::InvalidRequest),
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
fn a_key_moves_and_every_later_key_ripples_by_the_same_delta() {
    let doc = with_bare("K=a.1.1;.;K=b.2.2;.;K=c.3.3;.;.");
    let (cells, shown, applied, pairs) = moved(&doc, "L0", 0, 1);
    assert_eq!(cells, strings(&[".;K=a.1.1;.;K=b.2.2;.;K=c.3.3;.", BARE7]));
    assert_eq!(shown, strings(&["-*1*3*5", "*000000"]));
    assert_eq!(applied, Some(1));
    assert_eq!(pairs, [[0, 2, 1, 3], [2, 4, 3, 5]]);
    // the dragged key was the first one; dragged in the middle, only the keys after it follow
    let (cells, _, applied, _) = moved(&doc, "L0", 2, 3);
    assert_eq!(cells, strings(&["K=a.1.1;.;.;K=b.2.2;.;K=c.3.3;.", BARE7]));
    assert_eq!(applied, Some(3));
    // leftwards the later keys follow too, and the vacated cells hold the previous key
    let doc = with_bare("K=a.1.1;.;.;K=b.2.2;.;K=c.3.3;.");
    let (cells, shown, applied, pairs) = moved(&doc, "L0", 3, 1);
    assert_eq!(cells, strings(&["K=a.1.1;K=b.2.2;.;K=c.3.3;.;.;.", BARE7]));
    assert_eq!(shown, strings(&["**1*333", "*000000"]));
    assert_eq!(
        (applied, pairs),
        (Some(1), vec![[0, 3, 0, 1], [3, 5, 1, 3]])
    );
}

#[test]
fn the_ripple_is_clamped_so_the_last_key_stays_on_the_timeline() {
    let doc = with_bare("K=a.1.1;.;K=b.2.2;.;K=c.3.3;.;.");
    // asked for frame 6, the last key may only reach frame 6: the whole group moves 2
    let (cells, shown, applied, pairs) = moved(&doc, "L0", 0, 6);
    assert_eq!(cells, strings(&[".;.;K=a.1.1;.;K=b.2.2;.;K=c.3.3", BARE7]));
    assert_eq!(shown, strings(&["--*2*4*", "*000000"]));
    assert_eq!(
        (applied, pairs),
        (Some(2), vec![[0, 2, 2, 4], [2, 4, 4, 6]])
    );
    // without a later key there is nothing to clamp against: the key goes where it was asked
    let doc = with_bare("K=a.1.1;.;K=b.2.2;.;.;.;.");
    let (cells, _, applied, pairs) = moved(&doc, "L0", 2, 6);
    assert_eq!(cells, strings(&["K=a.1.1;.;.;.;.;.;K=b.2.2", BARE7]));
    assert_eq!((applied, pairs), (Some(6), vec![[0, 2, 0, 6]]));
    // a last key already on the last frame leaves no room: the move clamps to zero, which is a no-op
    let mut o = owner(document(
        &parse_layers(&with_bare("K=a.1.1;.;K=b.2.2;.;K=c.3.3;.;K=d.4.4")),
        Build::V1,
    ));
    let candidate = prepare(&o, &mv("L0", 0, 5)).unwrap();
    assert_eq!((candidate.applied_to(), candidate.pairs().len()), (None, 0));
    assert_eq!(o.commit_exposure_retime(candidate), Ok(0));
    assert_eq!(o.content_revision(), 0);
}

#[test]
fn a_destination_replaces_what_was_there_and_a_vacated_key_cell_holds() {
    // key c lands on key b (b is lost), key d follows; frames 3 and 4 hold key d's predecessor
    let doc = "K=a.1.1;K=b.2.2;.;K=c.3.3;K=d.4.4;.|K;.;.;.;.;.";
    let (cells, shown, applied, pairs) = moved(doc, "L0", 3, 1);
    assert_eq!(
        cells,
        strings(&["K=a.1.1;K=c.3.3;K=d.4.4;.;.;.", "K;.;.;.;.;."])
    );
    assert_eq!(shown, strings(&["***222", "*00000"]));
    assert_eq!(applied, Some(1));
    // pairs are reported for every consecutive pair of the OLD keys, collapsed ones included
    assert_eq!(pairs, [[0, 1, 0, 1], [1, 3, 1, 1], [3, 4, 1, 2]]);
    // a key dragged before a neighbour keeps the neighbour where it is
    let doc = "K=a.1.1;I=s.3.3;K=b.2.2;.;K=c.3.3;.|K;.;.;.;.;.";
    let (cells, shown, applied, pairs) = moved(doc, "L0", 4, 1);
    assert_eq!(
        cells,
        strings(&["K=a.1.1;K=c.3.3;K=b.2.2;.;.;.", "K;.;.;.;.;."])
    );
    assert_eq!(shown, strings(&["***222", "*00000"]));
    assert_eq!(
        (applied, pairs),
        (Some(1), vec![[0, 2, 0, 2], [2, 4, 2, 1]])
    );
}

#[test]
fn a_shortened_tween_squashes_its_in_betweens_and_the_later_one_wins() {
    let doc = with_bare("K=a.1.1;I=s.3.3;I=t.4.4;I=u.5.5;K=b.2.2;.;.");
    let (cells, shown, applied, pairs) = moved(&doc, "L0", 4, 2);
    assert_eq!(cells, strings(&["K=a.1.1;I=u.5.5;K=b.2.2;.;.;.;.", BARE7]));
    assert_eq!(shown, strings(&["*~*2222", "*000000"]));
    assert_eq!((applied, pairs), (Some(2), vec![[0, 4, 0, 2]]));
    // two spans: the first shrinks onto its new extent, the second keeps its sample between the moved keys
    let doc = "K=a.1.1;I=s.3.3;I=t.4.4;I=u.5.5;K=b.2.2;I=v.6.6;K=c.7.7;.|K;.;.;.;.;.;.;.";
    let (cells, shown, _, pairs) = moved(doc, "L0", 4, 3);
    assert_eq!(
        cells,
        strings(&[
            "K=a.1.1;I=s.3.3;I=u.5.5;K=b.2.2;I=v.6.6;K=c.7.7;.;.",
            "K;.;.;.;.;.;.;."
        ])
    );
    assert_eq!(shown, strings(&["*~~*~*55", "*0000000"]));
    assert_eq!(pairs, [[0, 4, 0, 3], [4, 6, 3, 5]]);
}

#[test]
fn a_lengthened_tween_lays_its_in_betweens_by_position_and_a_dense_one_fills_its_holes() {
    // dense span stretched from 4 to 6 frames: s (manual) and t (manual off) keep their flags where they are laid,
    // the two holes get copies of the nearest sample with the manual flag dropped
    let doc = with_bare("K=a.1.1;M=s.3.3;N=t.4.4;I=u.5.5;K=b.2.2;.;.");
    let (cells, shown, applied, _) = moved(&doc, "L0", 4, 6);
    let expected = "K=a.1.1;I=s.3.3;M=s.3.3;N=t.4.4;I=u.5.5;I=u.5.5;K=b.2.2";
    assert_eq!(cells, strings(&[expected, BARE7]));
    assert_eq!(shown, strings(&["*~!~~~*", "*000000"]));
    assert_eq!(applied, Some(6));
    // a sparse span (one sample over a long hold) stays sparse: no hole is filled
    let doc = with_bare("K=a.1.1;.;I=s.3.3;.;K=b.2.2;.;.");
    let (cells, shown, _, _) = moved(&doc, "L0", 4, 6);
    assert_eq!(cells, strings(&["K=a.1.1;.;.;I=s.3.3;.;.;K=b.2.2", BARE7]));
    assert_eq!(shown, strings(&["*00~00*", "*000000"]));
    // the first key dragged right: its span is moved and stretched with the keys after it
    let doc = "K=a.1.1;I=s.3.3;I=t.4.4;K=b.2.2;.;.;.;.|K;.;.;.;.;.;.;.";
    let (cells, shown, _, pairs) = moved(doc, "L0", 0, 2);
    assert_eq!(
        cells,
        strings(&[".;.;K=a.1.1;I=s.3.3;I=t.4.4;K=b.2.2;.;.", "K;.;.;.;.;.;.;."])
    );
    assert_eq!(shown, strings(&["--*~~*55", "*0000000"]));
    assert_eq!(pairs, [[0, 3, 2, 5]]);
}

#[test]
fn the_dense_hole_fill_obeys_the_timing_entry_cap() {
    // 4000 samples between two keys is a legal timing; stretching it fills every hole, and 4096 entries is the cap
    let mut first = vec!["K"];
    first.extend(std::iter::repeat("I").take(4000));
    first.push("K");
    let rest = 9000 - first.len();
    first.extend(std::iter::repeat(".").take(rest));
    let mut second = vec!["K"];
    second.extend(std::iter::repeat(".").take(8999));
    let at_cap = format!("{}|{}", first.join(";"), second.join(";"));
    let mut o = owner(document(&parse_layers(&at_cap), Build::V1));
    let out = apply(&mut o, mv("L0", 4001, 4095)).unwrap();
    let timing = serde_json::to_value(out.layers[0].timing.value().unwrap()).unwrap();
    assert_eq!(timing["keys"], json!([{"frame":0},{"frame":4095}]));
    assert_eq!(timing["samples"].as_array().unwrap().len(), 4094);
    for target in [4096, 4097, 5000, 8999] {
        let mut o = owner(document(&parse_layers(&at_cap), Build::V1));
        assert_eq!(
            apply(&mut o, mv("L0", 4001, target)).unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "target {target}"
        );
        assert_eq!(o.content_revision(), 0);
    }
}

#[test]
fn a_version_two_order_plan_follows_the_moved_content_and_splits_a_shared_scope() {
    let layers = "K=a.1.1,b.2.2;.;K=c.3.3;.|K=x.4.4;.;.;.";
    let mut o = owner(document(&parse_layers(layers), Build::V2));
    assert_eq!(
        scope_shape(&head(&o)),
        ["0[L0:a,L0:b,L1:x]", "2[L0:c]"],
        "one shared scope at frame 0"
    );
    let out = apply(&mut o, mv("L0", 0, 1)).unwrap();
    assert_eq!(
        scope_shape(&out),
        ["0[L1:x]", "1[L0:a,L0:b]", "3[L0:c]"],
        "the layer's elements leave the shared scope, the other layer's stay"
    );
    let triples = |frame| element_triples(&out, "L0", frame);
    assert_eq!(
        triples(1),
        [("a".to_owned(), 0, 1), ("b".to_owned(), 1, 2)],
        "ids, natural positions and authored order travel with the key"
    );
    assert_eq!(triples(3), [("c".to_owned(), 0, 3)]);
    assert!(triples(0).is_empty() && triples(2).is_empty());
    assert_eq!(
        text(&out),
        strings(&[".;K=a.1.1,b.2.2;.;K=c.3.3", "K=x.4.4;.;.;."])
    );
}

#[test]
fn a_same_frame_drag_is_a_no_op_before_any_other_check() {
    let layers = "K=a.1.1;.;K=b.2.2;.|K;.;.;.";
    for (layer, frame) in [("L0", 0), ("L0", 1), ("L1", 3)] {
        let mut o = owner(document(&parse_layers(layers), Build::V1));
        let candidate = prepare(&o, &mv(layer, frame, frame)).unwrap();
        assert_eq!(
            o.commit_exposure_retime(candidate),
            Ok(0),
            "{layer} {frame}"
        );
        assert_eq!(o.content_revision(), 0);
    }
}

#[test]
fn a_same_frame_drag_on_a_locked_layer_is_still_a_no_op() {
    // the baseline returns on the same frame before it asks about the lock
    let mut o = locked_owner("K=a.1.1;.;K=b.2.2;.|K;.;.;.", &[0]);
    let candidate = prepare(&o, &mv("L0", 0, 0)).unwrap();
    assert_eq!(o.commit_exposure_retime(candidate), Ok(0));
    assert_eq!(o.content_revision(), 0);
}

#[test]
fn locked_layers_refuse_and_another_locked_layer_changes_nothing() {
    let layers = "K=a.1.1;.;K=b.2.2;.|K;.;.;.";
    let mut o = locked_owner(layers, &[0]);
    assert_eq!(
        apply(&mut o, mv("L0", 0, 1)).unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    assert_eq!(o.content_revision(), 0);
    let mut o = locked_owner(layers, &[1]);
    let out = apply(&mut o, mv("L0", 0, 1)).unwrap();
    assert_eq!(text(&out), [".;K=a.1.1;.;K=b.2.2", "K;.;.;."]);
    assert_eq!(out.layers[1].locked, Presence::Present(true));
}

#[test]
fn a_layer_without_authored_timing_is_typed_unavailable_and_never_invented() {
    let mut value = build::document_json(&parse_layers("K=a.1.1;.;K=b.2.2;.|K;.;.;."), Build::V1);
    value["layers"][0].as_object_mut().unwrap().remove("timing");
    let untimed = build::decode(&value);
    let mut o = owner(untimed.clone());
    assert_eq!(
        apply(&mut o, mv("L0", 0, 1)).unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    assert_eq!((o.content_revision(), head(&o)), (0, untimed));
}
