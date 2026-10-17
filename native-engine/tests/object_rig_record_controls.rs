//! Document-level admission of the rig record (L03-N, I11): typed codec kinds on both wires, encode
//! refusal, the unsupported-family guard and the id counter against the admission boundary.

use super::flaw_tests::{
    bind, bone, ik, rig_text, twin, twin_ik, with_binds, with_bones, with_ik, WEIGHT,
};
use super::oracle_rows::{foreign_object, layer, legacy_object, mesh_bind, stroke_bind, v1, v2};
use super::{validate_rig_layers, RigAdmissionError, RigRecord, MAX_SAFE_SEQUENCE};
use crate::object_codec::{
    decode_ordered_project, decode_project, decode_storage_project, encode_project,
    encode_storage_project, ObjectCodecErrorKind,
};
use crate::object_document::{ObjectLayer, ObjectRecord, Presence};
use RigAdmissionError as E;

fn rig_layer(text: &str) -> ObjectLayer {
    let mut carrier = ObjectLayer::identified("R".to_string());
    carrier.rig = Presence::Present(serde_json::from_str(text).expect("rig decodes"));
    carrier
}

#[test]
fn documents_reject_a_bad_rig_with_typed_kinds_on_both_wires() {
    let cases: Vec<(&str, String, ObjectCodecErrorKind)> = vec![
        (
            "empty bone id",
            with_bones(&[bone("", 2, 2, "")]),
            ObjectCodecErrorKind::Invalid,
        ),
        (
            "duplicate bone id",
            with_bones(&[bone("bone1", 2, 2, ""), bone("bone1", 2, 2, "")]),
            ObjectCodecErrorKind::DuplicateId,
        ),
        (
            "duplicate ik chain",
            with_ik(&[twin_ik(), twin_ik()]),
            ObjectCodecErrorKind::DuplicateId,
        ),
        (
            "ik bone missing",
            with_ik(&[ik(("bone9", 0), ("bone1", 1), ("bone2", 1))]),
            ObjectCodecErrorKind::Invalid,
        ),
        (
            "two targets",
            with_binds(&[bind(r#""strokeId":"s","meshId":"m","#, 3, 3, None, WEIGHT)]),
            ObjectCodecErrorKind::Invalid,
        ),
        (
            "unknown member",
            twin().replacen("\"nextId\":3", "\"nextId\":3,\"pose\":{}", 1),
            ObjectCodecErrorKind::Unsupported,
        ),
    ];
    for (name, text, kind) in &cases {
        let layers = [layer("R", text)];
        let one = decode_project(v1(&layers, &[]).as_bytes())
            .expect_err(name)
            .kind();
        assert_eq!(one, *kind, "{name}: v1");
        let two = decode_ordered_project(v2(&layers, &["R"], &[]).as_bytes())
            .expect_err(name)
            .kind();
        assert_eq!(two, *kind, "{name}: v2");
        let storage = decode_storage_project(v2(&layers, &["R"], &[]).as_bytes());
        assert!(storage.is_err(), "{name}: storage");
    }
    // the same layer with the twin rig is accepted on both wires: the refusal is the rig's alone
    let ok = [layer("R", &twin())];
    assert!(decode_project(v1(&ok, &[]).as_bytes()).is_ok());
    assert!(decode_ordered_project(v2(&ok, &["R"], &[]).as_bytes()).is_ok());
}

#[test]
fn encoding_an_inadmissible_rig_is_refused_without_bytes_on_both_wires() {
    let good = v1(&[layer("R", &twin())], &[]);
    let mut document = decode_project(good.as_bytes()).expect("twin document");
    assert_eq!(encode_project(&document).unwrap(), good.as_bytes());
    document.layers[0] = rig_layer(&with_bones(&[
        bone("bone1", 2, 2, ""),
        bone("bone1", 2, 2, ""),
    ]));
    assert_eq!(
        encode_project(&document).expect_err("v1 refused").kind(),
        ObjectCodecErrorKind::DuplicateId
    );
    let text = v2(&[layer("R", &twin())], &["R"], &[]);
    let mut stored = decode_storage_project(text.as_bytes()).expect("v2 twin");
    assert_eq!(encode_storage_project(&stored).unwrap(), text.as_bytes());
    stored.layers[0] = rig_layer(&with_bones(&[bone("", 2, 2, "")]));
    assert_eq!(
        encode_storage_project(&stored)
            .expect_err("v2 refused")
            .kind(),
        ObjectCodecErrorKind::Invalid
    );
}

fn record(text: &str) -> ObjectRecord {
    serde_json::from_str(text).expect("object decodes")
}

#[test]
fn a_bind_to_a_stroke_of_an_unsupported_family_is_a_typed_unsupported() {
    let layer_r = rig_layer(&with_binds(&[stroke_bind("x", 3)]));
    let foreign = record(&foreign_object("R", "x"));
    assert_eq!(
        validate_rig_layers(&[layer_r.clone()], &[foreign.clone()]),
        Err(E::StrokeFamilyUnsupported)
    );
    // the record decodes as an ObjectRecord, so the refusal is the rig guard's
    assert_eq!(foreign.family, "compound-path");
    // the same stroke id on another layer, a supported family, or a mesh target is not refused
    let elsewhere = record(&foreign_object("Other", "x"));
    assert_eq!(
        validate_rig_layers(&[layer_r.clone()], &[elsewhere]),
        Ok(())
    );
    for object in [
        legacy_object("R", "x", 0, 3),
        super::oracle_rows::vector_object("R", "x", 1, 3),
    ] {
        assert_eq!(
            validate_rig_layers(&[layer_r.clone()], &[record(&object)]),
            Ok(())
        );
    }
    let mesh_layer = rig_layer(&with_binds(&[mesh_bind("x", 3)]));
    assert_eq!(
        validate_rig_layers(&[mesh_layer], &[foreign.clone()]),
        Ok(())
    );
    // a layer with no rig and a stroke of the same name is untouched
    assert_eq!(
        validate_rig_layers(&[ObjectLayer::identified("R".into())], &[foreign]),
        Ok(())
    );
    // the codec refuses such a record anyway (object loop), as Unsupported
    let text = v1(
        &[layer("R", &with_binds(&[stroke_bind("x", 3)]))],
        &[foreign_object("R", "x")],
    );
    assert_eq!(
        decode_project(text.as_bytes()).expect_err("refused").kind(),
        ObjectCodecErrorKind::Unsupported
    );
}

#[test]
fn rig_admission_runs_for_every_rigged_layer_and_stops_at_the_first_flaw() {
    let empty = rig_layer(&with_bones(&[bone("", 2, 2, "")]));
    let duplicate = rig_layer(&with_bones(&[
        bone("bone1", 2, 2, ""),
        bone("bone1", 2, 2, ""),
    ]));
    let good = rig_layer(&twin());
    let plain = ObjectLayer::identified("P".into());
    assert_eq!(
        validate_rig_layers(&[plain.clone(), good.clone()], &[]),
        Ok(())
    );
    assert_eq!(validate_rig_layers(&[plain.clone()], &[]), Ok(()));
    assert_eq!(
        validate_rig_layers(&[plain.clone(), good.clone(), empty.clone()], &[]),
        Err(E::EmptyBoneId)
    );
    assert_eq!(
        validate_rig_layers(&[empty.clone(), duplicate.clone()], &[]),
        Err(E::EmptyBoneId)
    );
    assert_eq!(
        validate_rig_layers(&[duplicate, good, empty], &[]),
        Err(E::DuplicateBoneId)
    );
}

#[test]
fn the_id_counter_arithmetic_matches_the_admission_boundary() {
    // a rig that admission accepts can always issue a fresh bone id that does not collide
    let mut rig: RigRecord = serde_json::from_str(&rig_text(
        &[bone("bone4", 2, 2, ""), bone("arm", 2, 2, "")],
        &[],
        "5",
        &[],
    ))
    .unwrap();
    assert_eq!(rig.validate(), Ok(()));
    let id = rig.fresh_id("bone").unwrap();
    assert_eq!(id, "bone5");
    assert!(rig.bone(&id).is_none());
    assert_eq!(rig.next_id, 6);
    // the unsafe boundary agrees with admission: the exhausted counter is still an admitted value
    rig.next_id = MAX_SAFE_SEQUENCE;
    assert_eq!(rig.validate(), Ok(()));
    assert_eq!(rig.fresh_id("bone"), Err(E::IdSequenceExhausted));
}
