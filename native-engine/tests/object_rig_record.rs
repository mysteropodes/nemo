//! Oracle-backed and literal tests for the native rig record (L03-N, I11).
//!
//! The oracle rows come from executing the baseline JS (`app.js`, `tools.js`, `rig-bridge.js`,
//! `timeline.js`, `image-mesh.js`, the shipped Paper.js at fbe4b8a3) and are parsed in
//! `object_rig_record_rows.rs`; rejection tables and guards are in `object_rig_record_controls.rs`.
//! Literals below are hand-derived from the baseline source, never produced by the record under test.

use super::oracle_rows::{
    ensure_rows, fresh_rows, guard, ikset_rows, layer, rel_rows, rig_row, rig_rows, v1, v2, ORACLE,
};
use super::{BindTarget, RigAdmissionError, RigPair, RigRecord, MAX_SAFE_SEQUENCE};
use crate::object_codec::{
    decode_ordered_project, decode_project, decode_storage_project, encode_project,
    encode_storage_project, migrate_ordered_project,
};
use crate::object_document::Presence;

fn rig(text: &str) -> RigRecord {
    serde_json::from_str(text).expect("rig decodes")
}

fn reencode(record: &RigRecord) -> String {
    serde_json::to_string(record).expect("rig encodes")
}

fn pair(text: &str) -> RigPair {
    serde_json::from_str(text).expect("pair decodes")
}

#[test]
fn oracle_row_counts_are_pinned() {
    assert_eq!(ensure_rows().len(), 3);
    assert_eq!(fresh_rows().len(), 3);
    assert_eq!(rig_rows().len(), 16);
    assert_eq!(rel_rows().len(), 9);
    assert_eq!(ikset_rows().len(), 3);
    assert_eq!(ORACLE.lines().count(), 49);
}

#[test]
fn baseline_rigs_decode_validate_and_reencode_byte_identically() {
    for row in rig_rows() {
        let record = rig(row.export);
        assert_eq!(record.validate(), Ok(()), "{}", row.name);
        assert_eq!(reencode(&record), row.export, "{}", row.name);
        // cloneRigForSymbol and the exportJSON whitelist keep exactly the same fields.
        assert_eq!(row.symbol, row.export, "{}", row.name);
        assert_eq!(rig(row.symbol), record, "{}", row.name);
    }
}

#[test]
fn baseline_rigs_pass_the_real_codec_byte_for_byte_on_every_wire() {
    for row in rig_rows() {
        let layers = [layer("R", row.export)];
        let text = v1(&layers, &[]);
        let document = decode_project(text.as_bytes()).expect(row.name);
        assert_eq!(
            encode_project(&document).expect(row.name),
            text.as_bytes(),
            "{}: byte-identical v1 document",
            row.name
        );
        assert_eq!(
            document.layers()[0].rig().map(reencode).as_deref(),
            Some(row.export)
        );
        let v2_text = v2(&layers, &["R"], &[]);
        let stored = decode_storage_project(v2_text.as_bytes()).expect(row.name);
        assert_eq!(stored.format_version(), 2);
        assert_eq!(
            encode_storage_project(&stored).expect(row.name),
            v2_text.as_bytes(),
            "{}: byte-identical v2 storage",
            row.name
        );
        let ordered = decode_ordered_project(v2_text.as_bytes()).expect(row.name);
        assert_eq!(
            ordered.layers()[0].rig().map(reencode).as_deref(),
            Some(row.export)
        );
        let plan = super::oracle_rows::plan(&["R"]);
        let migrated = migrate_ordered_project(&document, plan.as_bytes()).expect(row.name);
        assert_eq!(
            migrated.layers()[0].rig().map(reencode).as_deref(),
            Some(row.export)
        );
    }
}

#[test]
fn a_layer_without_a_rig_stays_byte_identical_and_names_no_rig() {
    let plain = v1(&["{\"layerUid\":\"R\"}".to_string()], &[]);
    let document = decode_project(plain.as_bytes()).expect("plain layer");
    assert!(document.layers()[0].rig().is_none());
    let bytes = encode_project(&document).expect("plain encodes");
    assert_eq!(bytes, plain.as_bytes());
    assert!(!String::from_utf8(bytes).unwrap().contains("rig"));
    // an empty rig is present, and is not the absent one
    let empty = v1(&[layer("R", &reencode(&RigRecord::empty()))], &[]);
    let with_empty = decode_project(empty.as_bytes()).expect("empty rig");
    assert!(with_empty.layers()[0].rig().is_some());
    assert_ne!(with_empty, document);
}

#[test]
fn ensure_layer_rig_rows_decide_the_empty_rig_and_the_untouched_existing_one() {
    let rows = ensure_rows();
    let empty = RigRecord::empty();
    // ensureLayerRig({}) and ensureLayerRig({rig:null}) both make {bones:{}, ikChains:{}, binds:[], nextId:1}
    for name in ["fresh_layer", "null_rig_is_replaced"] {
        let row = rows.iter().find(|r| r.name == name).unwrap();
        assert_eq!(rig(row.after), empty, "{name}");
        assert_eq!(row.same_ref, name == "fresh_layer", "{name}");
    }
    assert_eq!(empty.next_id, 1);
    assert!(empty.bones.entries().is_empty() && empty.ik_chains.entries().is_empty());
    assert!(empty.binds.is_empty());
    // the empty rig exports with nextId BEFORE binds (the export order, not the ensureLayerRig literal)
    assert_eq!(reencode(&empty), rig_row("empty").export);
    // an existing rig is returned as it was
    let kept = rows
        .iter()
        .find(|r| r.name == "existing_rig_is_returned_unchanged")
        .unwrap();
    assert!(kept.same_ref);
    let before: serde_json::Value = serde_json::from_str(kept.before).unwrap();
    let after: serde_json::Value = serde_json::from_str(kept.after).unwrap();
    assert_eq!(before["rig"], after);
    assert_eq!(rig(kept.after).next_id, 4);
}

#[test]
fn fresh_ids_follow_the_rig_fresh_id_rows() {
    for row in fresh_rows() {
        let mut record = RigRecord::empty();
        record.next_id = row.start;
        let ids: Vec<String> = row
            .ids
            .iter()
            .map(|_| record.fresh_id(row.prefix).expect(row.name))
            .collect();
        assert_eq!(ids, row.ids, "{}", row.name);
        assert_eq!(record.next_id, row.end, "{}", row.name);
    }
    let mut record = RigRecord::empty();
    assert_eq!(record.fresh_id("bone").unwrap(), "bone1");
    assert_eq!(record.fresh_id("bone").unwrap(), "bone2");
    assert_eq!(record.next_id, 3);
}

#[test]
fn the_id_counter_stops_before_the_unsafe_boundary_and_never_moves_on_refusal() {
    let mut record = RigRecord::empty();
    record.next_id = MAX_SAFE_SEQUENCE - 1;
    assert_eq!(record.fresh_id("bone").unwrap(), "bone9007199254740990");
    assert_eq!(record.next_id, MAX_SAFE_SEQUENCE);
    assert_eq!(
        record.fresh_id("bone"),
        Err(RigAdmissionError::IdSequenceExhausted)
    );
    assert_eq!(record.next_id, MAX_SAFE_SEQUENCE);
    record.next_id = 0;
    assert_eq!(
        record.fresh_id("bone"),
        Err(RigAdmissionError::IdSequenceExhausted)
    );
    assert_eq!(record.next_id, 0);
}

#[test]
fn bone_ids_issued_by_the_tool_never_collide_with_an_admitted_rig() {
    let bones = rig(rig_row("bones_only_open_open_closed").export);
    assert_eq!(bones.next_id, 4);
    let mut next = bones.clone();
    let id = next.fresh_id("bone").unwrap();
    assert_eq!(id, "bone4");
    assert!(bones.bone(&id).is_none());
    assert!(["bone1", "bone2", "bone3"]
        .iter()
        .all(|k| bones.bone(k).is_some()));
}

#[test]
fn drawn_bones_have_the_assigned_literal_shape() {
    let record = rig(rig_row("bones_only_open_open_closed").export);
    let ids: Vec<&str> = record
        .bones
        .entries()
        .iter()
        .map(|(id, _)| id.as_str())
        .collect();
    assert_eq!(ids, ["bone1", "bone2", "bone3"]);
    let counts: Vec<usize> = record
        .bones
        .entries()
        .iter()
        .map(|(_, b)| b.segments.len())
        .collect();
    assert_eq!(counts, [3, 2, 4]);
    // finalizeRigBone stores the live pose and an independent copy as the rest pose
    for (_, bone) in record.bones.entries() {
        assert_eq!(bone.segments, bone.rest_segments);
    }
    let bone1 = record.bone("bone1").unwrap();
    assert_eq!(
        bone1.segments[0].handle_out,
        Presence::Present(Some(pair("[20,0]")))
    );
    assert_eq!(bone1.segments[2].point, pair("[200,20]"));
    assert!(!bone1.closed && record.bone("bone3").unwrap().closed);
    // only the bone whose influence circle was resized has a radius: max(1, |(10,10)-(50,40)|) = 50
    assert_eq!(record.bone("bone1").unwrap().radius_value(), None);
    assert_eq!(record.bone("bone2").unwrap().radius_value(), Some(50.0));
    assert!(record.bone("bone3").unwrap().radius.missing());
    assert_eq!(record.next_id, 4);
}

#[test]
fn a_bound_stroke_has_the_assigned_literal_shape() {
    let record = rig(rig_row("vector_brush_center_data_is_memory_only").export);
    let bind = &record.binds[0];
    assert_eq!(bind.target(), Some(BindTarget::Stroke("v1")));
    assert!(bind.rotate && bind.mesh_id.missing());
    assert_eq!(bind.rest.len(), 6);
    assert_eq!(bind.rest_points()[1], [50.0, -12.0]);
    // vertex (0,-10) is 10 from bone1 point 0 and farther than the 100 radius from every other
    // control point: one entry, t = 1 - 10/100 = 0.9 with no softness
    assert_eq!(bind.weights[0].len(), 1);
    let first = &bind.weights[0][0];
    assert_eq!(
        (first.bone_id.as_str(), first.pt_index.value().copied()),
        ("bone1", Some(0))
    );
    assert_eq!(first.w_value(), 0.9);
    // the rest tangents: this brush outline has zero handles everywhere -> null slots
    let handles = bind.rest_handles.value().expect("restHandles present");
    assert_eq!(handles.len(), 6);
    assert!(handles
        .iter()
        .all(|h| h.i == Presence::Present(None) && h.o == Presence::Present(None)));
    // a rebind of the same shape replaced the first one and kept the order of the others
    let rebinds = rig(rig_row("rebind_replaces_second_stroke_added").export);
    let targets: Vec<_> = rebinds.binds.iter().map(|b| b.target()).collect();
    assert_eq!(
        targets,
        [
            Some(BindTarget::Stroke("s1")),
            Some(BindTarget::Stroke("s2"))
        ]
    );
    assert!(!rebinds.binds[0].rotate && rebinds.binds[1].rotate);
    let only_bone1 = rebinds.binds[0]
        .weights
        .iter()
        .flatten()
        .all(|w| w.bone_id == "bone1");
    assert!(
        only_bone1,
        "the second bind named only bone1 and replaced the first"
    );
}

#[test]
fn bezier_handle_slots_keep_absent_null_and_value_apart() {
    let record = rig(rig_row("one_stroke_two_bones_rotate_soft").export);
    let handles = record.binds[0].rest_handles.value().unwrap();
    // segments: (handleIn 0,0 / out 10,0) (-10,0 / 0,5) (0,-5 / 0,0) (0,0 / 0,0); a zero handle is null
    assert_eq!(handles[0].i, Presence::Present(None));
    assert_eq!(handles[0].o, Presence::Present(Some(pair("[10,0]"))));
    assert_eq!(handles[1].i, Presence::Present(Some(pair("[-10,0]"))));
    assert_eq!(handles[2].o, Presence::Present(None));
    assert!(handles[3].i == Presence::Present(None) && handles[3].o == Presence::Present(None));
    let bare = rig(
        r#"{"bones":{},"ikChains":{},"nextId":1,"binds":[{"strokeId":"s","rest":[[0,0]],"restHandles":[{}],"weights":[[]],"rotate":false}]}"#,
    );
    let slot = &bare.binds[0].rest_handles.value().unwrap()[0];
    assert!(slot.i.missing() && slot.o.missing());
    assert_ne!(slot, &handles[3]);
    assert!(bare.binds[0].weights[0].is_empty());
}

#[test]
fn weight_entries_keep_both_models_and_the_ptindex_discriminator() {
    let record = rig(rig_row("legacy_offset_weights_mixed_with_control_points").export);
    let lengths: Vec<usize> = record.binds[0].weights.iter().map(Vec::len).collect();
    assert_eq!(lengths, [1, 4, 2, 1]);
    let legacy = &record.binds[0].weights[0][0];
    assert!(legacy.pt_index.missing());
    assert_eq!(legacy.offset_value(), Some(10.0));
    // vertex 1: two legacy offset entries, then the control-point entry rigWeighOnePointByControl added
    // (bone2 control point 1, 22.4 from vertex (190,-20) inside the 80 radius), then the manual override
    // the weight popover pushes ({boneId, offset: 0, w: 0.5})
    let mixed = &record.binds[0].weights[1];
    assert!(mixed[0].pt_index.missing() && mixed[1].pt_index.missing());
    assert_eq!(
        (mixed[2].bone_id.as_str(), mixed[2].pt_index.value()),
        ("bone2", Some(&1))
    );
    assert!(mixed[2].offset.missing());
    assert!(mixed[3].pt_index.missing());
    assert_eq!(
        (mixed[3].offset_value(), mixed[3].w_value()),
        (Some(0.0), 0.5)
    );
    // an entry may carry both fields; the offset is kept next to the index
    let both = rig(
        r#"{"bones":{},"ikChains":{},"nextId":1,"binds":[{"meshId":"m","rest":[[0,0]],"weights":[[{"boneId":"b","offset":2,"ptIndex":0,"w":1}]],"rotate":false}]}"#,
    );
    let entry = &both.binds[0].weights[0][0];
    assert_eq!(
        (entry.pt_index.value(), entry.offset_value()),
        (Some(&0), Some(2.0))
    );
    assert_eq!(
        reencode(&both),
        r#"{"bones":{},"ikChains":{},"nextId":1,"binds":[{"meshId":"m","rest":[[0,0]],"weights":[[{"boneId":"b","ptIndex":0,"offset":2,"w":1}]],"rotate":false}]}"#
    );
}

#[test]
fn mesh_binds_carry_world_rest_points_and_no_stroke_id() {
    let record = rig(rig_row("mesh_bind_two_bones_rebind_replaces").export);
    assert_eq!(
        record.binds.len(),
        1,
        "re-binding the same image replaced the first bind"
    );
    let bind = &record.binds[0];
    assert_eq!(bind.target(), Some(BindTarget::Mesh("im_1")));
    assert!(bind.stroke_id.missing() && bind.rest_handles.missing());
    // outline8 + centre = 9 vertices of the 300x100 rect at (0,-50): corners, edge midpoints, centre
    let rest = bind.rest_points();
    assert_eq!(rest.len(), 9);
    assert_eq!(
        (rest[0], rest[2], rest[4], rest[6], rest[8]),
        (
            [0.0, -50.0],
            [300.0, -50.0],
            [300.0, 50.0],
            [0.0, 50.0],
            [150.0, 0.0]
        )
    );
    // the same mesh through a rect rotated by 30 degrees about its centre (150,0): vertex 7 = (0,0)
    let rotated = rig(rig_row("mesh_bind_rotated_rect_and_stroke_bind").export);
    let seven = rotated.binds[0].rest_points()[7];
    let (cos, sin) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    assert!(
        (seven[0] - (150.0 - 150.0 * cos)).abs() < 1e-9 && (seven[1] - (-150.0 * sin)).abs() < 1e-9
    );
    assert_eq!(rotated.binds[1].target(), Some(BindTarget::Stroke("s1")));
    // auto-assign: the mesh is bound first, then the two selectable paths; the hidden brush-texture
    // copy (isBrushTextureCopy) and the CompoundPath got no bind
    let auto = rig(rig_row("auto_assign_mesh_first_then_paths_compound_skipped").export);
    let targets: Vec<_> = auto.binds.iter().map(|b| b.target()).collect();
    assert_eq!(
        targets,
        [
            Some(BindTarget::Mesh("im_1")),
            Some(BindTarget::Stroke("a1")),
            Some(BindTarget::Stroke("a2"))
        ]
    );
    assert_eq!(
        guard("auto_assign_counts"),
        r#"{"n":3,"skippedUnsupported":1,"binds":["im_1","a1","a2"]}"#
    );
}
