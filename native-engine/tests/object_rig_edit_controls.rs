//! Controls of the rig edit transaction that need the private pieces (L03-O): the pure rules against
//! literal rigs, their refusals (each leaves the input untouched), the replace-not-stack control, the
//! strict decoder's defaults, the planner's unsupported-family count and the refusal-to-code table.
use super::*;
use crate::object_codec::decode_storage_project;
use crate::object_snapshot::object_rig_edit_read::ControlEntry;
use plan::PlannedBind;
use serde_json::{json, Number};

const SEGMENTS_1: &str = r#"[{"point":[0,0],"handleIn":[0,0],"handleOut":[0,0]},{"point":[100,0],"handleIn":[0,0],"handleOut":[0,0]}]"#;
const SEGMENTS_2: &str = r#"[{"point":[100,0],"handleIn":[0,0],"handleOut":[0,0]},{"point":[200,0],"handleIn":[0,0],"handleOut":[0,0]}]"#;

/// Two bones (bone2 with its own radius 80), one bind of stroke `s1` with a weight list per vertex.
fn rig_text(binds: &str) -> String {
    format!(
        r#"{{"bones":{{"bone1":{{"segments":{SEGMENTS_1},"restSegments":{SEGMENTS_1},"closed":false}},"bone2":{{"segments":{SEGMENTS_2},"restSegments":{SEGMENTS_2},"closed":false,"radius":80}}}},"ikChains":{{}},"nextId":3,"binds":{binds}}}"#
    )
}

const ONE_BIND: &str = r#"[{"strokeId":"s1","rest":[[10,-20],[190,-20]],"restHandles":[{"i":null,"o":null},{"i":null,"o":null}],"weights":[[{"boneId":"bone1","ptIndex":0,"w":0.5},{"boneId":"bone2","ptIndex":0,"w":0.25}],[{"boneId":"bone1","ptIndex":1,"w":0.5}]],"rotate":false}]"#;

fn rig(binds: &str) -> RigRecord {
    serde_json::from_str(&rig_text(binds)).unwrap()
}

fn stroke(id: &str) -> RigTarget {
    RigTarget::Stroke(id.to_owned())
}

fn segment(x: i64, y: i64) -> RigSegment {
    let zero = || Presence::Present(Some([Number::from(0i64), Number::from(0i64)]));
    RigSegment {
        point: [Number::from(x), Number::from(y)],
        handle_in: zero(),
        handle_out: zero(),
    }
}

fn planned(id: &str, w: f64) -> PlannedBind {
    PlannedBind {
        stroke_id: id.to_owned(),
        rest: vec![[Number::from(10i64), Number::from(-20i64)]],
        rest_handles: vec![[None, None]],
        weights: vec![vec![ControlEntry {
            bone_id: "bone1".to_owned(),
            pt_index: 0,
            w,
        }]],
    }
}

#[test]
fn the_number_helpers_restate_the_baseline_arithmetic() {
    // onMove: `hVec = pointer - anchor`, the other handle `[-hVec.x, -hVec.y]`.
    assert_eq!(
        rules::moved_handles([100.0, 0.0], [130.0, 10.0], false),
        ([30.0, 10.0], Some([-30.0, -10.0]))
    );
    assert_eq!(
        rules::moved_handles([100.0, 0.0], [130.0, 10.0], true),
        ([30.0, 10.0], None)
    );
    // Math.max(1, distance).
    for (input, want) in [
        (0.4, 1.0),
        (1.0, 1.0),
        (1.5, 1.5),
        (120.5, 120.5),
        (-5.0, 1.0),
    ] {
        assert_eq!(rules::radius_floor(input), want, "{input}");
    }
    // Math.max(0, Math.min(100, v)) / 100.
    for (input, want) in [
        (40.0, 0.4),
        (33.0, 0.33),
        (250.0, 1.0),
        (-5.0, 0.0),
        (100.0, 1.0),
        (0.0, 0.0),
    ] {
        assert_eq!(rules::weight_fraction(input), want, "{input}");
    }
    // JSON.stringify spelling: integral values without a fraction, -0 as 0, no NaN or infinity.
    for (input, want) in [
        (0.0, "0"),
        (-0.0, "0"),
        (2.0, "2"),
        (-3.0, "-3"),
        (2.5, "2.5"),
        (1e9, "1000000000"),
    ] {
        assert_eq!(
            rules::js_number(input).unwrap().to_string(),
            want,
            "{input}"
        );
    }
    assert!(rules::js_number(f64::NAN).is_none());
    assert!(rules::js_number(f64::INFINITY).is_none());
    assert!(rules::js_number(f64::NEG_INFINITY).is_none());
}

#[test]
fn every_rule_refuses_with_its_code_and_leaves_the_input_rig_alone() {
    let rig = rig(ONE_BIND);
    let kept = rig.clone();
    let s1 = stroke("s1");
    let refused: Vec<(
        &str,
        Result<RigRecord, DispatchErrorCode>,
        DispatchErrorCode,
    )> = vec![
        (
            "move of an unknown bone",
            rules::move_segment(&rig, "bone9", 0, SegmentPart::Point, [1.0, 1.0], false),
            DispatchErrorCode::NotFound,
        ),
        (
            "move of a vertex past the bone",
            rules::move_segment(&rig, "bone1", 2, SegmentPart::HandleOut, [1.0, 1.0], false),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            "radius of an unknown bone",
            rules::set_radius(&rig, "bone9", 5.0),
            DispatchErrorCode::NotFound,
        ),
        (
            "weight of an unknown stroke",
            rules::set_weight(&rig, &stroke("nope"), 0, 0, 10.0),
            DispatchErrorCode::NotFound,
        ),
        (
            "weight of a stroke named as a mesh",
            rules::set_weight(&rig, &RigTarget::Mesh("s1".to_owned()), 0, 0, 10.0),
            DispatchErrorCode::NotFound,
        ),
        (
            "weight of a vertex past the bind",
            rules::set_weight(&rig, &s1, 2, 0, 10.0),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            "weight entry past the list",
            rules::set_weight(&rig, &s1, 1, 1, 10.0),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            "removal of an entry past the list",
            rules::remove_weight(&rig, &s1, 1, 1),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            "add of a bone the vertex has",
            rules::add_weight(&rig, &s1, 0, "bone1", 0.0),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            "add of an unknown bone",
            rules::add_weight(&rig, &s1, 0, "bone9", 0.0),
            DispatchErrorCode::NotFound,
        ),
        (
            "removal of a bone a weight names",
            rules::remove_bone(&rig, "bone1"),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            "removal of an unknown bone",
            rules::remove_bone(&rig, "bone9"),
            DispatchErrorCode::NotFound,
        ),
        (
            "removal of an unknown chain",
            rules::remove_ik(&rig, "bone2:1"),
            DispatchErrorCode::NotFound,
        ),
        (
            "removal of an unknown bind",
            rules::remove_bind(&rig, &stroke("s2")),
            DispatchErrorCode::NotFound,
        ),
        (
            "redraw of an unknown bone",
            rules::draw_bone(
                &rig,
                Some("bone9"),
                vec![segment(0, 0), segment(1, 0)],
                false,
            ),
            DispatchErrorCode::NotFound,
        ),
    ];
    for (name, result, want) in refused {
        assert_eq!(result, Err(want), "{name}");
    }
    assert_eq!(rig, kept, "no rule mutates the stored rig");
}

#[test]
fn the_rules_write_exactly_what_the_baseline_handlers_write() {
    let rig = rig(ONE_BIND);
    let s1 = stroke("s1");
    // A value equal to the stored one keeps the stored record (a no-op cannot rewrite bytes).
    assert_eq!(rules::set_radius(&rig, "bone2", 80.0).unwrap(), rig);
    assert_eq!(rules::set_weight(&rig, &s1, 0, 0, 50.0).unwrap(), rig);
    assert_eq!(
        rules::move_segment(&rig, "bone1", 1, SegmentPart::Point, [100.0, 0.0], false).unwrap(),
        rig
    );
    // Math.max(1, v) on a bone that had no radius.
    let small = rules::set_radius(&rig, "bone1", 0.4).unwrap();
    assert_eq!(small.bone("bone1").unwrap().radius_value(), Some(1.0));
    assert_eq!(small.bone("bone2").unwrap().radius_value(), Some(80.0));
    // The popover percent input writes only the one entry.
    let edited = rules::set_weight(&rig, &s1, 0, 1, 30.0).unwrap();
    assert_eq!(edited.binds[0].weights[0][0].w_value(), 0.5);
    assert_eq!(edited.binds[0].weights[0][1].w_value(), 0.3);
    assert_eq!(edited.binds[0].weights[1], rig.binds[0].weights[1]);
    // The add row is `{boneId, offset, w: 0.5}`, appended; removal keeps the order of the rest.
    let added = rules::add_weight(&rig, &s1, 1, "bone2", 0.25).unwrap();
    let list = &added.binds[0].weights[1];
    assert_eq!(list.len(), 2);
    assert_eq!(list[1].bone_id, "bone2");
    assert_eq!(list[1].offset_value(), Some(0.25));
    assert_eq!(list[1].w_value(), 0.5);
    assert!(
        list[1].pt_index.value().is_none(),
        "the legacy offset shape has no ptIndex"
    );
    let removed = rules::remove_weight(&rig, &s1, 0, 0).unwrap();
    assert_eq!(removed.binds[0].weights[0].len(), 1);
    assert_eq!(removed.binds[0].weights[0][0].bone_id, "bone2");
    // Redrawing keeps the position, the radius and the counter; a fresh bone takes the counter.
    let redrawn = rules::draw_bone(
        &rig,
        Some("bone2"),
        vec![segment(5, 5), segment(9, 9)],
        true,
    )
    .unwrap();
    assert_eq!(redrawn.bone("bone2").unwrap().radius_value(), Some(80.0));
    assert_eq!(redrawn.next_id, 3);
    assert_eq!(redrawn.bones.entries()[1].0, "bone2");
    assert!(redrawn.bone("bone2").unwrap().closed);
    let fresh = rules::draw_bone(&rig, None, vec![segment(0, 0), segment(1, 1)], false).unwrap();
    assert_eq!(fresh.next_id, 4);
    assert_eq!(fresh.bones.entries().last().unwrap().0, "bone3");
    assert_eq!(fresh.bone("bone3").unwrap().radius_value(), None);
    assert_eq!(
        fresh.bone("bone3").unwrap().segments,
        fresh.bone("bone3").unwrap().rest_segments
    );
    // rigSetIK files the chain under the end reference and rewriting it is idempotent.
    let refs = [
        &RigRef {
            bone_id: "bone1".to_owned(),
            vi: 0,
        },
        &RigRef {
            bone_id: "bone1".to_owned(),
            vi: 1,
        },
        &RigRef {
            bone_id: "bone2".to_owned(),
            vi: 1,
        },
    ];
    let once = rules::set_ik(&rig, refs, true, [100.0, 94.5]).unwrap();
    let chain = once.ik_chains.get("bone2:1").unwrap();
    assert_eq!(
        (chain.l1.to_string(), chain.l2.to_string(), chain.flip),
        ("100".to_owned(), "94.5".to_owned(), true)
    );
    assert_eq!(
        rules::set_ik(&once, refs, true, [100.0, 94.5]).unwrap(),
        once
    );
    assert_eq!(rules::remove_ik(&once, "bone2:1").unwrap(), rig);
}

#[test]
fn a_rebind_replaces_the_old_bind_where_a_stacking_variant_would_keep_both() {
    let two = r#"[{"strokeId":"s1","rest":[[1,1]],"weights":[[]],"rotate":false},{"strokeId":"s2","rest":[[2,2]],"weights":[[]],"rotate":false}]"#;
    let rig = rig(two);
    let replaced = rules::apply_binds(&rig, &[planned("s1", 0.75)], true).unwrap();
    let ids: Vec<&str> = replaced
        .binds
        .iter()
        .map(|bind| bind.stroke_id.value().map(String::as_str).unwrap())
        .collect();
    assert_eq!(
        ids,
        ["s2", "s1"],
        "the old bind is removed and the new one appended"
    );
    assert!(replaced.binds[1].rotate);
    assert_eq!(replaced.binds[1].weights[0][0].w_value(), 0.75);
    assert_eq!(
        replaced.binds[0], rig.binds[1],
        "the other bind is untouched"
    );
    // The control: pushing without removing (the bug) leaves two binds of the same stroke.
    let mut stacked = rig.clone();
    stacked.binds.push(replaced.binds[1].clone());
    let stacked_ids: Vec<&str> = stacked
        .binds
        .iter()
        .map(|bind| bind.stroke_id.value().map(String::as_str).unwrap())
        .collect();
    assert_eq!(stacked_ids, ["s1", "s2", "s1"]);
    assert_ne!(stacked, replaced);
    // An old save with two binds of one target is ambiguous: refused, never guessed.
    assert_eq!(
        plan::bind_index(&stacked, BindTarget::Stroke("s1")),
        Err(plan::PlanRefusal::AmbiguousBind)
    );
    assert_eq!(plan::bind_index(&stacked, BindTarget::Stroke("s2")), Ok(1));
    assert_eq!(
        plan::bind_index(&stacked, BindTarget::Stroke("s9")),
        Err(plan::PlanRefusal::NoBind)
    );
    assert_eq!(
        rules::set_weight(&stacked, &stroke("s1"), 0, 0, 10.0),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(
        rules::remove_bind(&stacked, &stroke("s1"))
            .unwrap()
            .binds
            .len(),
        1,
        "removal drops both"
    );
}

#[test]
fn the_decoder_defaults_are_the_baseline_defaults() {
    let (layer, action) = parse::parse_edit(
        br#"{"command":"object.rig.edit","layerUid":"L0","action":{"kind":"autoAssign","frame":3}}"#,
    )
    .unwrap();
    assert_eq!(layer, "L0");
    assert!(matches!(
        action,
        RigAction::AutoAssign {
            frame: 3,
            default_radius: None,
            rotate: false,
            softness: None,
            paths: true,
            meshes: true
        }
    ));
    let (_, action) = parse::parse_edit(
        br#"{"command":"object.rig.edit","layerUid":"L0","action":{"kind":"resetWeights","target":{"meshId":"im_1"},"vertex":2}}"#,
    )
    .unwrap();
    match action {
        RigAction::ResetWeights {
            target,
            vertex,
            radius,
            softness,
        } => {
            assert_eq!(
                (target, vertex, radius, softness),
                (RigTarget::Mesh("im_1".to_owned()), 2, None, 0.0)
            );
        }
        other => panic!("{other:?}"),
    }
    let (_, action) = parse::parse_edit(
        br#"{"command":"object.rig.edit","layerUid":"L0","action":{"kind":"bindStroke","strokeId":"s1","frame":0,"boneIds":["bone2"]}}"#,
    )
    .unwrap();
    match action {
        RigAction::BindStroke {
            stroke_id,
            frame,
            bone_ids,
            radius,
            rotate,
            softness,
        } => {
            assert_eq!(stroke_id, "s1");
            assert_eq!((frame, radius, rotate, softness), (0, None, false, None));
            assert_eq!(bone_ids, Some(vec!["bone2".to_owned()]));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        parse::parse_edit(br#"{"command":"object.rig.edit","layerUid":"L0","action":{"kind":"drawBone","segments":[],"closed":false}}"#).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
}

#[test]
fn the_planner_counts_records_of_an_unsupported_family_as_skipped() {
    // The baseline skips a CompoundPath and counts it; a record of any family the rig does not bind is
    // the native equivalent. The codec admits only the two path families, so the family is swapped on the
    // in-memory record the planner reads.
    let target = |id: &str, frame: u32| {
        json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},
            "layerUid":"L0","strokeId":id})
    };
    let record = |id: &str, frame: u32| {
        json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path","target":target(id, frame),
            "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":[
            {"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
            {"point":{"x":9,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]},
            "fill":{"kind":"solid","r":0.5,"g":0.5,"b":0.5,"a":1}})
    };
    let text = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,
        "layers":[{"layerUid":"L0"}],
        "objects":[record("a1", 0), record("a2", 0), record("b1", 1)]})
    .to_string();
    let document = decode_storage_project(text.as_bytes()).unwrap();
    let mut records = document.objects().to_vec();
    assert_eq!(records.len(), 3);
    let mut foreign = records[0].clone();
    foreign.family = "compound-path".to_owned();
    foreign.target.stroke_id = "c1".to_owned();
    records.insert(1, foreign);
    let ids = |found: &[&crate::object_document::ObjectRecord]| -> Vec<String> {
        found
            .iter()
            .map(|r| r.target().stroke_id().to_owned())
            .collect()
    };
    let (paths, skipped) = plan::split_records(&records, "L0", 0);
    assert_eq!(
        (ids(&paths), skipped),
        (vec!["a1".to_owned(), "a2".to_owned()], 1)
    );
    let (paths, skipped) = plan::split_records(&records, "L0", 1);
    assert_eq!(
        (ids(&paths), skipped),
        (vec!["b1".to_owned()], 0),
        "another frame"
    );
    let (paths, skipped) = plan::split_records(&records, "L1", 0);
    assert_eq!((paths.len(), skipped), (0, 0), "another layer");
}

#[test]
fn a_plan_refusal_maps_to_a_stable_dispatch_code() {
    use crate::object_snapshot::object_rig_edit_read::KernelRefusal;
    use plan::PlanRefusal::*;
    let table = [
        (NoLayer, DispatchErrorCode::NotFound),
        (NoRig, DispatchErrorCode::NotFound),
        (NoStroke, DispatchErrorCode::NotFound),
        (NoBind, DispatchErrorCode::NotFound),
        (NoBone, DispatchErrorCode::NotFound),
        (AmbiguousBind, DispatchErrorCode::InvalidRequest),
        (UnknownBone, DispatchErrorCode::InvalidRequest),
        (DuplicateBone, DispatchErrorCode::InvalidRequest),
        (UnsupportedFamily, DispatchErrorCode::InvalidRequest),
        (EmptyStroke, DispatchErrorCode::InvalidRequest),
        (BadVertex, DispatchErrorCode::InvalidRequest),
        (
            Kernel(KernelRefusal::Unavailable),
            DispatchErrorCode::Unavailable,
        ),
        (
            Kernel(KernelRefusal::Unresolved),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            Kernel(KernelRefusal::AnchorMissing),
            DispatchErrorCode::InvalidRequest,
        ),
    ];
    for (refusal, want) in table {
        assert_eq!(refusal.code(), want, "{refusal:?}");
    }
}
