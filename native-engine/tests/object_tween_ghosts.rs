//! Independent original overlay policy plus real retained native frame/producer lifetime.
use super::*;
use crate::request_receipts::DispatchErrorCode as Error;

#[test]
fn native_tween_ghost_policy_matches_original_and_keeps_its_snapshot_after_edit_and_drop() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/tween-baseline/oracle.json")).unwrap();
    for c in oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["operation"] == "ghosts")
    {
        let mut owner =
            NativeObjectHistory::new("ghost", document(&c["input"]["frames"], true)).unwrap();
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let original = encode_storage_project(snapshot.document()).unwrap();
        let mut q = c["input"]["query"].clone();
        q["atRevision"] = json!(0);
        q["documentSnapshotId"] = json!(snapshot.snapshot_id());
        let read = snapshot
            .query_tween_ghosts_json(&serde_json::to_vec(&q).unwrap())
            .unwrap();
        let mut actual = vec![];
        for f in read.frames() {
            let source = if f.kind == crate::TweenGhostKind::AllKeys {
                f.frame
            } else {
                snapshot.document().layers[0]
                    .timing
                    .value()
                    .unwrap()
                    .exposure(f.frame)
                    .1
                    .unwrap()
            };
            for r in snapshot
                .document()
                .objects
                .iter()
                .filter(|r| r.target.layer_uid == "L0" && r.target.frame_scope.frame == source)
            {
                let visual = f.vector_visual(&r);
                if visual.omit {
                    continue;
                }
                actual.push(
                    json!({"kind":f.kind,"frame":f.frame,"id":r.target.stroke_id,"visual":visual}),
                );
            }
        }
        numeric(&json!(actual), &c["expected"], "ghost-policy");
        let command = request(
            &owner,
            "edit",
            "tween.generate",
            json!({"command":"object.tween.generate","layerUid":"L0"}),
        );
        assert_eq!(dispatch(&mut owner, &command)["ok"], true);
        assert_eq!(owner.content_revision(), 1);
        drop(owner);
        drop(snapshot);
        assert_eq!(read.snapshot().content_revision(), 0);
        assert_eq!(
            encode_storage_project(read.snapshot().document()).unwrap(),
            original
        );
        for (i, f) in read.frames().iter().enumerate() {
            if f.kind == crate::TweenGhostKind::AllKeys {
                let paths = read.all_keys_paths(i).unwrap();
                assert!(!paths.is_empty());
                for path in paths {
                    let record = read
                        .snapshot()
                        .document()
                        .objects
                        .iter()
                        .find(|r| {
                            r.target.stroke_id == path.source_id
                                && r.target.frame_scope.frame == f.frame
                        })
                        .unwrap();
                    assert_eq!(path.geometry, record.geometry);
                }
                assert!(matches!(read.prepare_packet(i), Err(Error::InvalidRequest)));
                continue;
            }
            let witness = read.prepare_frame(i).unwrap();
            assert_eq!(witness.content_revision(), 0);
            assert_eq!(
                witness.document_snapshot_id(),
                read.snapshot().snapshot_id()
            );
            assert_eq!(witness.frame(), f.frame);
        }
        assert!(matches!(
            read.prepare_frame(read.frames().len()),
            Err(Error::InvalidRequest)
        ));
        let mut bad = q.clone();
        bad["atRevision"] = json!(1);
        assert!(matches!(
            read.snapshot()
                .query_tween_ghosts_json(&serde_json::to_vec(&bad).unwrap()),
            Err(Error::StaleRevision)
        ));
        for key in ["from", "to", "frame"] {
            let mut bad = q.clone();
            bad[key] = json!(999);
            assert!(read
                .snapshot()
                .query_tween_ghosts_json(&serde_json::to_vec(&bad).unwrap())
                .is_err());
        }
    }
}

#[test]
fn native_tween_onion_frame_keeps_real_duplicator_copies_with_an_empty_host() {
    use crate::object_frame_packet::duplicators::tests::{empty_host_pool, snapshot};
    use crate::object_render_scene::{ordered_entries, PaintEntry};
    let source = snapshot(empty_host_pool());
    let q = json!({"atRevision":source.content_revision(),"documentSnapshotId":source.snapshot_id(),"contextId":"scene-root","layerUid":"H0","frame":1,"onion":true,"ghostAll":false,"from":1,"to":2,"previousOpacity":30,"nextOpacity":30,"mode":"default"});
    let read = source
        .query_tween_ghosts_json(&serde_json::to_vec(&q).unwrap())
        .unwrap();
    assert_eq!(read.frames().len(), 1);
    let witness = read.prepare_frame(0).unwrap();
    let paths = ordered_entries(witness.plan())
        .map(|entry| match entry {
            PaintEntry::Duplicator(copy) => {
                assert_eq!(copy.layer_index, 1);
                copy.path.clone()
            }
            _ => panic!("expected native copies, not raw frame sources"),
        })
        .collect::<Vec<_>>();
    assert_eq!(paths.len(), 3);
    assert_ne!(paths[0], paths[1]);
}

#[test]
fn native_tween_ghost_all_uses_stored_geometry_and_native_combines_without_raw_fallback() {
    use crate::object_document::object_group_kernels::object_fill_trace_ops::native::{path, ring};
    use crate::object_document::{GroupTable, Presence};
    use kurbo::Shape;
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/tween-baseline/oracle.json")).unwrap();
    for case in oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["operation"] == "ghost-combine")
    {
        let frames = json!([{"isKeyframe":true,"strokes":case["input"]["shapes"]},{"strokes":[]}]);
        let mut doc = document(&frames, true);
        let mode = &case["input"]["groups"]["g"]["combineMode"];
        let groups: GroupTable = serde_json::from_value(json!({"contextId":"scene-root",
            "entries":[{"gid":"g","combineMode":mode,"order":["a","b"]}],
            "members":[{"strokeId":"a","groupId":"g"},{"strokeId":"b","groupId":"g"}]}))
        .unwrap();
        doc.layers[0].groups = Presence::Present(groups);
        let owner = NativeObjectHistory::new("ghost-groups", doc.clone()).unwrap();
        let pin = owner.acquire_snapshot(0).unwrap();
        let query = |pin: &crate::revision::ObjectSnapshot| {
            json!({"atRevision":0,"documentSnapshotId":pin.snapshot_id(),
            "contextId":"scene-root","layerUid":"L0","frame":1,"onion":false,"ghostAll":true,
            "from":0,"to":1,"previousOpacity":30,"nextOpacity":30,"mode":"default"})
        };
        let read = pin
            .query_tween_ghosts_json(&serde_json::to_vec(&query(&pin)).unwrap())
            .unwrap();
        let actual = read.all_keys_paths(0).unwrap();
        let expected: Vec<crate::object_document::ObjectRecord> = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| serde_json::from_value(record(s, 0)).unwrap())
            .collect();
        assert_eq!(actual.len(), expected.len(), "{mode}");
        let paths = |records: &[crate::object_document::ObjectRecord]| {
            records
                .iter()
                .map(|r| path(&ring(r), true))
                .collect::<Vec<_>>()
        };
        let expected = paths(&expected);
        let actual: Vec<_> = actual
            .iter()
            .map(|g| {
                let mut r = doc.objects[0].clone();
                r.geometry = g.geometry.clone();
                path(&ring(&r), true)
            })
            .collect();
        let area = |paths: &[kurbo::BezPath]| paths.iter().map(|p| p.area().abs()).sum::<f64>();
        assert!((area(&actual) - area(&expected)).abs() < 1e-6, "{mode}");
        for x in (-5..70).step_by(5) {
            for y in (-5..50).step_by(5) {
                let p = kurbo::Point::new(x as f64 + 0.3, y as f64 + 0.4);
                assert_eq!(
                    actual.iter().any(|s| s.winding(p) != 0),
                    expected.iter().any(|s| s.winding(p) != 0),
                    "{mode} {p:?}"
                );
            }
        }
        // Native boolean admission is explicit: a curved member is never returned raw as a successful combine.
        doc.objects[0].geometry.segments[0].handle_out.x = 8.into();
        let curved = NativeObjectHistory::new("ghost-curved", doc.clone())
            .unwrap()
            .acquire_snapshot(0)
            .unwrap();
        let read = curved
            .query_tween_ghosts_json(&serde_json::to_vec(&query(&curved)).unwrap())
            .unwrap();
        assert!(matches!(read.all_keys_paths(0), Err(Error::Unavailable)));
        // With no combining group, even a moving/parametric object contributes its stored key geometry.
        doc.layers[0].groups = Presence::Missing;
        doc.property_holders=Presence::Present(serde_json::from_value(json!([{"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"a"},"properties":[{"property":"position","static":[100,200]}]}])).unwrap());
        let raw = NativeObjectHistory::new("ghost-raw", doc.clone())
            .unwrap()
            .acquire_snapshot(0)
            .unwrap();
        let read = raw
            .query_tween_ghosts_json(&serde_json::to_vec(&query(&raw)).unwrap())
            .unwrap();
        assert_eq!(
            read.all_keys_paths(0).unwrap()[0].geometry,
            doc.objects[0].geometry
        );
    }
}
