//! Link-report tests of the native rig record (L03-N, I11): the typed replacement for the silent
//! `relinkRigBinds` drop, checked against the executed baseline verdicts (`rel` and `grd` rows).

use super::oracle_rows::{
    foreign_object, guard, layer, legacy_object, mesh_bind, reference_object, rel_rows, rig_rows,
    rig_with_binds, stroke_bind, v1, vector_object,
};
use super::{
    bind_link, weight_ref_issues, BindLink, RigAdmissionError, RigRecord, WeightRefIssue,
    WeightRefProblem,
};
use crate::object_codec::decode_project;
use crate::object_document::ObjectRecord;

fn rig(text: &str) -> RigRecord {
    serde_json::from_str(text).expect("rig decodes")
}

fn reencode(record: &RigRecord) -> String {
    serde_json::to_string(record).expect("rig encodes")
}

fn object(text: &str) -> ObjectRecord {
    serde_json::from_str(text).expect("object decodes")
}

#[test]
fn relink_rows_decide_the_native_link_verdicts_and_nothing_is_dropped() {
    // expected typed reasons, hand-derived from relinkRigBinds
    let linked = |n: usize| BindLink::Linked { vertex_count: n };
    let expected: Vec<(&str, Vec<BindLink>)> = vec![
        ("stroke_count_matches", vec![linked(4)]),
        (
            "stroke_vertex_count_changed",
            vec![BindLink::StrokeVertexCount { stroke: 5, rest: 4 }],
        ),
        ("stroke_missing", vec![BindLink::StrokeMissing]),
        ("mesh_matches", vec![linked(9)]),
        (
            "mesh_vertex_count_changed",
            vec![BindLink::MeshVertexCount { mesh: 9, rest: 8 }],
        ),
        ("mesh_not_in_store", vec![BindLink::MeshMissing]),
        (
            "mixed_order_is_kept",
            vec![linked(3), linked(9), BindLink::StrokeMissing, linked(4)],
        ),
    ];
    let rows = rel_rows();
    let mut seen = 0;
    for (name, want) in &expected {
        let row = rows.iter().find(|r| r.name == *name).unwrap();
        let spec = &row.spec;
        let objects: Vec<ObjectRecord> = spec["strokes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                object(&legacy_object(
                    "R",
                    s["strokeId"].as_str().unwrap(),
                    0,
                    s["n"].as_u64().unwrap() as usize,
                ))
            })
            .collect();
        let binds: Vec<String> = spec["binds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| match (b.get("strokeId"), b.get("meshId")) {
                (Some(id), None) => {
                    stroke_bind(id.as_str().unwrap(), b["rest"].as_array().unwrap().len())
                }
                (None, Some(id)) => {
                    mesh_bind(id.as_str().unwrap(), b["rest"].as_array().unwrap().len())
                }
                _ => unreachable!("rows with both ids are tested separately"),
            })
            .collect();
        let record = rig(&rig_with_binds(&binds));
        let meshes: Vec<(String, usize)> = spec["meshes"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(id, cols)| (id.clone(), (cols.as_u64().unwrap() as usize + 1).pow(2)))
            .collect();
        let lookup = |id: &str| meshes.iter().find(|(m, _)| m == id).map(|(_, n)| *n);
        let got: Vec<BindLink> = record
            .binds
            .iter()
            .map(|bind| bind_link(bind, "R", 0, &objects, Some(&lookup)))
            .collect();
        assert_eq!(&got, want, "{name}");
        // the baseline kept a bind exactly when native reports it linked; the native record keeps all
        let kept: Vec<bool> = got
            .iter()
            .map(|g| matches!(g, BindLink::Linked { .. }))
            .collect();
        // mesh_without_raster is a raster check native cannot make yet (see the next test)
        assert_eq!(kept, row.kept, "{name}");
        assert_eq!(
            record.binds.len(),
            row.kept.len(),
            "{name}: nothing was filtered away"
        );
        assert_eq!(record.validate(), Ok(()), "{name}");
        seen += 1;
    }
    assert_eq!(seen, 7);
}

#[test]
fn raster_presence_and_both_id_binds_are_the_documented_gaps_against_relink() {
    let rows = rel_rows();
    // the baseline drops a mesh bind whose raster is absent although mesh and count match; the
    // native link report has no raster object to look at yet (L03-R) and says Linked on the store alone
    let row = rows
        .iter()
        .find(|r| r.name == "mesh_without_raster")
        .unwrap();
    assert_eq!(row.kept, [false]);
    let record = rig(&rig_with_binds(&[mesh_bind("im_1", 9)]));
    let nine = |id: &str| (id == "im_1").then_some(9usize);
    assert_eq!(
        bind_link(&record.binds[0], "R", 0, &[], Some(&nine)),
        BindLink::Linked { vertex_count: 9 }
    );
    assert_eq!(
        rows.iter().find(|r| r.name == "mesh_matches").unwrap().live,
        ["raster"]
    );
    assert_eq!(
        rows.iter()
            .find(|r| r.name == "stroke_count_matches")
            .unwrap()
            .live,
        ["path"]
    );
    // a bind with both ids resolved as a mesh bind in the baseline; native names exactly one target
    let both = rows
        .iter()
        .find(|r| r.name == "both_ids_resolve_as_mesh")
        .unwrap();
    assert_eq!(both.kept, [true]);
    assert_eq!(both.live, ["raster"]);
    let text = r#"{"bones":{},"ikChains":{},"nextId":1,"binds":[{"strokeId":"s1","meshId":"im_1","rest":[[0,0]],"weights":[[]],"rotate":false}]}"#;
    let both_native = rig(text);
    assert_eq!(both_native.binds[0].target(), None);
    assert_eq!(
        both_native.validate(),
        Err(RigAdmissionError::BindTargetInvalid)
    );
}

#[test]
fn guard_rows_pin_compound_empty_and_open_dispositions() {
    // rigBindStroke: null, CompoundPath and an empty path return false; an open path binds
    assert_eq!(guard("bind_stroke_null_target"), "false");
    assert_eq!(guard("bind_stroke_compound_path"), "false");
    assert_eq!(guard("bind_stroke_empty_path"), "false");
    assert_eq!(guard("bind_stroke_open_path"), "true");
    assert_eq!(guard("binds_after_guards"), r#"[["o2",3],["o1",1]]"#);
    assert_eq!(guard("bind_mesh_raster_without_mesh_id"), "false");
    assert_eq!(guard("bind_mesh_mesh_not_in_store"), "false");
    // native: an open editable-vector-path is a bindable path record, its vertex count is the segment count
    let open = vector_object("R", "o2", 0, 3);
    let record = rig(&rig_with_binds(&[stroke_bind("o2", 3)]));
    let objects = [object(&open)];
    assert_eq!(
        bind_link(&record.binds[0], "R", 0, &objects, None),
        BindLink::Linked { vertex_count: 3 }
    );
    // a record of any other family is a typed Unsupported, the CompoundPath guard
    let foreign = object(&foreign_object("R", "o2"));
    assert_eq!(
        bind_link(
            &record.binds[0],
            "R",
            0,
            std::slice::from_ref(&foreign),
            None
        ),
        BindLink::StrokeFamilyUnsupported
    );
}

#[test]
fn the_document_reports_links_by_layer_frame_scope_and_provider() {
    let rig_text = rig_with_binds(&[
        stroke_bind("s1", 4),
        mesh_bind("im_1", 9),
        stroke_bind("s2", 3),
    ]);
    let objects = [
        legacy_object("R", "s1", 0, 4),
        legacy_object("R", "s1", 1, 5),
        reference_object("R", "s2", 0, 3),
        legacy_object("Other", "s2", 0, 3),
    ];
    let layers = [
        "{\"layerUid\":\"Other\"}".to_string(),
        layer("R", &rig_text),
    ];
    let document = decode_project(v1(&layers, &objects).as_bytes()).expect("document");
    let nine = |id: &str| (id == "im_1").then_some(9usize);
    // frame 0: s1 matches, the mesh matches, s2 exists only as a reference record and on another layer
    assert_eq!(
        document.rig_links("R", 0, Some(&nine)),
        Some(vec![
            BindLink::Linked { vertex_count: 4 },
            BindLink::Linked { vertex_count: 9 },
            BindLink::StrokeMissing
        ])
    );
    // frame 1 holds a 5-vertex s1: the bind is reported, not dropped
    assert_eq!(
        document.rig_links("R", 1, Some(&nine)).unwrap()[0],
        BindLink::StrokeVertexCount { stroke: 5, rest: 4 }
    );
    // no mesh provider wired: neither linked nor missing
    assert_eq!(
        document.rig_links("R", 0, None).unwrap()[1],
        BindLink::MeshStoreUnavailable
    );
    let none = |_: &str| -> Option<usize> { None };
    assert_eq!(
        document.rig_links("R", 0, Some(&none)).unwrap()[1],
        BindLink::MeshMissing
    );
    assert_eq!(document.rig_links("Other", 0, Some(&nine)), None);
    assert_eq!(document.rig_links("nobody", 0, Some(&nine)), None);
}

#[test]
fn weight_references_to_missing_bones_or_points_are_reported_not_rejected() {
    let text = r#"{"bones":{"bone1":{"segments":[{"point":[0,0]},{"point":[1,0]}],"restSegments":[{"point":[0,0]},{"point":[1,0]}],"closed":false}},"ikChains":{},"nextId":2,"binds":[{"strokeId":"s","rest":[[0,0],[1,0]],"weights":[[{"boneId":"bone1","ptIndex":1,"w":1},{"boneId":"gone","ptIndex":0,"w":1}],[{"boneId":"bone1","ptIndex":2,"w":1},{"boneId":"bone1","offset":3,"w":1},{"boneId":"gone","offset":0,"w":1}]],"rotate":false}]}"#;
    let record = rig(text);
    assert_eq!(record.validate(), Ok(()));
    assert_eq!(reencode(&record), text);
    let at = |vertex, entry, problem| WeightRefIssue {
        bind: 0,
        vertex,
        entry,
        problem,
    };
    assert_eq!(
        weight_ref_issues(&record),
        vec![
            at(0, 1, WeightRefProblem::NoSuchBone),
            at(1, 0, WeightRefProblem::PointOutOfRange),
            at(1, 2, WeightRefProblem::NoSuchBone),
        ]
    );
    for row in rig_rows() {
        assert!(
            weight_ref_issues(&rig(row.export)).is_empty(),
            "{}",
            row.name
        );
    }
}
