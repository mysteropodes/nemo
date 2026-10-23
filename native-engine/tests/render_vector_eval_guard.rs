//! Packet admission over property holders (child of object_frame_packet.rs, declared by patch 3 only): a holder table is
//! admitted exactly when `holders_fully_consumed` says the evaluation join reads every property in it, and the packet then
//! evaluates to the oracle's anchors. Anything else stays `Unavailable`, as before.
use super::{prepare_object_frame, ObjectFrameError, ObjectFrameSelector};
use crate::evaluation::object_vector_kernels::render_vector_eval::evaluate_record;
use crate::evaluation::object_vector_kernels::render_vector_eval::tests::{
    check, row, rows, snapshot,
};
use crate::object_document::FrameScopeKind;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const NOT_CONSUMED: [&str; 3] = [
    "rect-foreign-kind-tracks-are-ignored@5",
    "plain-vector-with-foreign-tracks-is-not-rebuilt@5",
    "star-foreign-kind-tracks-are-ignored@5",
];

fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}

fn authored_at(doc: &str, frame: u32) -> Value {
    let mut value: Value = serde_json::from_str(doc).expect("oracle document is json");
    value["objects"][0]["target"]["frameScope"]["frame"] = json!(frame);
    value
}

#[test]
fn a_packet_over_consumed_shape_tracks_evaluates_to_the_oracle() {
    let (_, all) = rows();
    let (mut consumed, mut refused) = (0, 0);
    for source in all.iter().filter(|r| r.doc.contains("propertyHolders")) {
        let snap = snapshot(&authored_at(&source.doc, source.frame).to_string());
        let packet = prepare_object_frame(&snap, &selector(&snap, source.frame));
        if NOT_CONSUMED.contains(&source.id.as_str()) {
            assert_eq!(
                packet.err(),
                Some(ObjectFrameError::Unavailable),
                "{}",
                source.id
            );
            refused += 1;
            continue;
        }
        let packet = packet.unwrap_or_else(|e| panic!("{}: {e:?}", source.id));
        let record = packet.records().next().expect("one record");
        let got = evaluate_record(&packet, record).expect("packet entry");
        check(&got, source).unwrap_or_else(|m| panic!("{m}"));
        consumed += 1;
    }
    assert_eq!((consumed, refused), (37, 3));
}

#[test]
fn every_other_holder_keeps_the_packet_unavailable() {
    let (_, all) = rows();
    let base: Value = authored_at(&row(&all, "rect-corner-tl-animated@5").doc, 5);
    let try_packet = |value: &Value| {
        let snap = snapshot(&value.to_string());
        prepare_object_frame(&snap, &selector(&snap, 5)).err()
    };
    assert_eq!(try_packet(&base), None, "the consumed table is admitted");

    let mut transform = base.clone();
    transform["propertyHolders"][0]["properties"]
        .as_array_mut()
        .expect("properties")
        .push(json!({"property": "position", "static": [1, 2]}));
    let snap = snapshot(&transform.to_string());
    let packet = prepare_object_frame(&snap, &selector(&snap, 5))
        .expect("element Position is consumed by the native packet evaluator");
    let got = evaluate_record(&packet, packet.records().next().expect("one record"))
        .expect("actual transformed packet entry");
    let mut translated = row(&all, "rect-corner-tl-animated@5").clone();
    for anchor in &mut translated.expected {
        anchor[0] += 1.;
        anchor[1] += 2.;
    }
    check(&got, &translated).expect("the original oracle translated by literal [1,2]");

    let mut opacity = base.clone();
    opacity["propertyHolders"][0]["properties"]
        .as_array_mut()
        .expect("properties")
        .push(json!({"property": "opacity", "static": [25]}));
    let mut live_position = transform.clone();
    live_position["propertyHolders"][0]["properties"]
        .as_array_mut()
        .expect("properties")
        .last_mut()
        .expect("added Position")["expression"] = json!({"enabled": true, "code": "[1,2]"});
    let mut live = base.clone();
    live["propertyHolders"][0]["properties"][0]["expression"] =
        json!({"enabled": true, "code": "1"});
    let mut layer = base.clone();
    layer["propertyHolders"]
        .as_array_mut()
        .expect("holders")
        .push(
            json!({"target": {"contextId": "scene-root", "layerUid": "L1"},
               "properties": [{"property": "rotation", "static": [50]}]}),
        );
    let mut legacy = base.clone();
    let record = legacy["objects"][0].as_object_mut().expect("record");
    record.remove("vectorExt");
    record.remove("vector");
    record.insert("family".into(), json!("closed-solid-fill-cubic-path"));
    for (name, value) in [
        ("unconsumed opacity", opacity),
        ("enabled Position expression", live_position),
        ("enabled expression", live),
        ("layer holder", layer),
        ("legacy family", legacy),
    ] {
        assert_eq!(
            try_packet(&value),
            Some(ObjectFrameError::Unavailable),
            "{name}"
        );
    }
}
