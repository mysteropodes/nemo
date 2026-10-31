//! Tests of the duplicator inspector edit (#2256 Q4). The expected clamps are the baseline `wireNum` bounds of the
//! Mograph duplicator panel (`timeline.js` at 1065e9d8): rows/cols 1..30, count 1..500, the rest unbounded.
use super::fixture::{decode, document, duplicator_table, layer, wire};
use super::*;
use serde_json::{json, Value};

fn full() -> Value {
    json!({"mode":"grid","rows":2,"cols":3,"spacingX":150,"spacingY":150,"count":8,"radius":200,
        "startAngle":0,"endAngle":null,"radialOrient":false,"pathLayerUid":null,"pathAlignTangent":true,
        "seed":17,"staggerRandom":{"position":false,"rotation":false,"scale":false,"opacity":false,"hue":false},
        "timeOffset":{"enabled":false,"offsetFrames":1,"direction":"forward"},
        "sourceMode":"shape","effectors":[]})
}

fn world(descriptor: Value) -> ObjectDocument {
    let layers = [
        layer("ly_dup", "Host", &[(0, "h1")]),
        layer("ly_path", "Path", &[(0, "p1")]),
        layer("ly_plain", "Plain", &[(0, "x1")]),
    ];
    decode(&document(
        &layers,
        json!({"componentTables":{"duplicators":duplicator_table(&[("ly_dup", descriptor)])}}),
    ))
}

fn stored(document: &ObjectDocument) -> Value {
    wire(document)["componentTables"]["duplicators"]["duplicators"][0]["duplicator"].clone()
}

fn edit_with(f: impl FnOnce(&mut ConfigEdit)) -> ConfigEdit {
    let mut edit = ConfigEdit::default();
    f(&mut edit);
    edit
}

fn apply_edit(document: &ObjectDocument, edit: &ConfigEdit) -> SetupPlan {
    plan_duplicator_config(document, "ly_dup", edit).expect("edit is admissible")
}

#[test]
fn each_member_is_applied_and_every_other_member_keeps_its_stored_bytes() {
    let cases: Vec<(ConfigEdit, &str, Value)> = vec![
        (
            edit_with(|e| e.mode = Some("radial".into())),
            "mode",
            json!("radial"),
        ),
        (edit_with(|e| e.rows = Some(5.0)), "rows", json!(5)),
        (edit_with(|e| e.cols = Some(7.0)), "cols", json!(7)),
        (
            edit_with(|e| e.spacing_x = Some(33.5)),
            "spacingX",
            json!(33.5),
        ),
        (
            edit_with(|e| e.spacing_y = Some(-12.0)),
            "spacingY",
            json!(-12),
        ),
        (edit_with(|e| e.count = Some(40.0)), "count", json!(40)),
        (edit_with(|e| e.radius = Some(310.0)), "radius", json!(310)),
        (
            edit_with(|e| e.start_angle = Some(45.0)),
            "startAngle",
            json!(45),
        ),
        (
            edit_with(|e| e.radial_orient = Some(true)),
            "radialOrient",
            json!(true),
        ),
        (
            edit_with(|e| e.path_align_tangent = Some(false)),
            "pathAlignTangent",
            json!(false),
        ),
        (
            edit_with(|e| e.seed = Some(999_999)),
            "seed",
            json!(999_999),
        ),
    ];
    let before = world(full());
    for (edit, key, expected) in cases {
        let plan = apply_edit(&before, &edit);
        assert!(plan.changed, "{key}");
        let after = stored(&plan.document);
        assert_eq!(after[key], expected, "{key}");
        let mut rest = after.clone();
        let mut original = full();
        rest.as_object_mut().unwrap().remove(key);
        original.as_object_mut().unwrap().remove(key);
        assert_eq!(rest, original, "{key}: no other member moves");
        assert_eq!(
            decode(&wire(&plan.document)),
            plan.document,
            "{key}: round trip"
        );
    }
}

#[test]
fn the_nested_stagger_and_time_offset_members_edit_one_channel_at_a_time() {
    let before = world(full());
    let stagger = edit_with(|e| {
        e.stagger_random = Some(StaggerEdit {
            hue: Some(true),
            ..StaggerEdit::default()
        });
    });
    let after = stored(&apply_edit(&before, &stagger).document);
    assert_eq!(
        after["staggerRandom"],
        json!({"position":false,"rotation":false,"scale":false,"opacity":false,"hue":true})
    );
    assert_eq!(after["timeOffset"], full()["timeOffset"]);
    let time = edit_with(|e| {
        e.time_offset = Some(TimeEdit {
            offset_frames: Some(2.5),
            direction: Some("centerOut".into()),
            ..TimeEdit::default()
        });
    });
    let after = stored(&apply_edit(&before, &time).document);
    assert_eq!(
        after["timeOffset"],
        json!({"enabled":false,"offsetFrames":2.5,"direction":"centerOut"})
    );
    assert_eq!(after["staggerRandom"], full()["staggerRandom"]);
}

#[test]
fn a_never_authored_stagger_or_time_offset_starts_from_the_baseline_lazy_literal() {
    let before = world(json!({"mode":"grid"}));
    let stagger = edit_with(|e| {
        e.stagger_random = Some(StaggerEdit {
            rotation: Some(true),
            ..StaggerEdit::default()
        });
    });
    assert_eq!(
        stored(&apply_edit(&before, &stagger).document)["staggerRandom"],
        json!({"rotation":true})
    );
    let time = edit_with(|e| {
        e.time_offset = Some(TimeEdit {
            enabled: Some(true),
            ..TimeEdit::default()
        })
    });
    assert_eq!(
        stored(&apply_edit(&before, &time).document)["timeOffset"],
        json!({"enabled":true,"offsetFrames":1,"direction":"forward"})
    );
    let empty = edit_with(|e| {
        e.stagger_random = Some(StaggerEdit::default());
        e.time_offset = Some(TimeEdit::default());
    });
    assert!(
        !apply_edit(&before, &empty).changed,
        "naming nothing creates nothing"
    );
}

#[test]
fn values_are_clamped_to_the_baseline_wire_num_bounds() {
    let before = world(full());
    let cases = [
        (edit_with(|e| e.rows = Some(99.0)), "rows", json!(30)),
        (edit_with(|e| e.rows = Some(0.0)), "rows", json!(1)),
        (edit_with(|e| e.cols = Some(-4.0)), "cols", json!(1)),
        (edit_with(|e| e.cols = Some(31.0)), "cols", json!(30)),
        (edit_with(|e| e.count = Some(9999.0)), "count", json!(500)),
        (edit_with(|e| e.count = Some(0.2)), "count", json!(1)),
        (edit_with(|e| e.count = Some(12.5)), "count", json!(12.5)),
        (
            edit_with(|e| e.spacing_x = Some(-1.0e6)),
            "spacingX",
            json!(-1_000_000),
        ),
    ];
    for (edit, key, expected) in cases {
        assert_eq!(
            stored(&apply_edit(&before, &edit).document)[key],
            expected,
            "{key}"
        );
    }
}

#[test]
fn an_edit_that_changes_nothing_publishes_nothing() {
    let before = world(full());
    let same = edit_with(|e| {
        e.mode = Some("grid".into());
        e.rows = Some(2.0);
        e.cols = Some(3.0);
        e.spacing_x = Some(150.0);
        e.seed = Some(17);
        e.radial_orient = Some(false);
        e.time_offset = Some(TimeEdit {
            enabled: Some(false),
            offset_frames: Some(1.0),
            direction: Some("forward".into()),
        });
    });
    let plan = apply_edit(&before, &same);
    assert!(!plan.changed);
    assert_eq!(plan.document, before);
    assert!(!apply_edit(&before, &ConfigEdit::default()).changed);
    let clamped = edit_with(|e| e.rows = Some(2.0));
    assert!(!apply_edit(&before, &clamped).changed);
}

#[test]
fn refusals_are_typed_and_nothing_is_edited() {
    let before = world(full());
    let rows = edit_with(|e| e.rows = Some(3.0));
    assert_eq!(
        plan_duplicator_config(&before, "ly_plain", &rows).unwrap_err(),
        SetupError::NotADuplicator("ly_plain".into())
    );
    assert_eq!(
        plan_duplicator_config(&before, "nope", &rows).unwrap_err(),
        SetupError::UnknownLayer("nope".into())
    );
    let bad_mode = edit_with(|e| e.mode = Some("spiral".into()));
    assert_eq!(
        plan_duplicator_config(&before, "ly_dup", &bad_mode).unwrap_err(),
        SetupError::ConfigInvalid
    );
    let bad_direction = edit_with(|e| {
        e.time_offset = Some(TimeEdit {
            direction: Some("sideways".into()),
            ..TimeEdit::default()
        });
    });
    assert_eq!(
        plan_duplicator_config(&before, "ly_dup", &bad_direction).unwrap_err(),
        SetupError::ConfigInvalid
    );
    for seed in [1_000_000_u64, u64::from(u32::MAX)] {
        let wide = edit_with(|e| e.seed = Some(seed));
        assert_eq!(
            plan_duplicator_config(&before, "ly_dup", &wide).unwrap_err(),
            SetupError::SeedRange,
            "{seed}"
        );
    }
    for seed in [0_u64, 999_999] {
        let ok = edit_with(|e| e.seed = Some(seed));
        assert!(
            plan_duplicator_config(&before, "ly_dup", &ok).is_ok(),
            "{seed}"
        );
    }
}

#[cfg(feature = "evaluation")]
mod placement {
    use super::*;
    use crate::evaluation::object_component_kernels::duplicator_placement_modes::{
        duplicator_count, grid_cols, mode_offset,
    };
    use crate::evaluation::object_component_kernels::object_duplicator_evaluate_spec::describe;

    fn record(plan: &SetupPlan) -> DuplicatorDescriptor {
        plan.document
            .duplicator_set()
            .and_then(|set| set.duplicator("scene-root", "ly_dup"))
            .map(|r| r.duplicator.clone())
            .expect("record")
    }

    #[test]
    fn grid_placement_after_the_edit_is_the_baseline_formula() {
        let edit = edit_with(|e| {
            e.rows = Some(3.0);
            e.cols = Some(4.0);
            e.spacing_x = Some(80.0);
            e.spacing_y = Some(60.0);
        });
        let descriptor = record(&apply_edit(&world(full()), &edit));
        let view = describe(&descriptor);
        let count = duplicator_count(&view.spec);
        assert_eq!(count, 12.0);
        let cols = grid_cols(&view.spec);
        let at = |k| mode_offset(&view.spec, k, count, cols, None).unwrap();
        // (k % cols) * spacingX, floor(k / cols) * spacingY
        assert_eq!((at(5).base_dx, at(5).base_dy), (80.0, 60.0));
        assert_eq!((at(11).base_dx, at(11).base_dy), (240.0, 120.0));
        let capped = edit_with(|e| {
            e.rows = Some(500.0);
            e.cols = Some(500.0);
        });
        let capped = record(&apply_edit(&world(full()), &capped));
        assert_eq!(duplicator_count(&describe(&capped).spec), 900.0, "30 x 30");
    }

    #[test]
    fn radial_placement_after_the_edit_is_the_baseline_formula() {
        let edit = edit_with(|e| {
            e.mode = Some("radial".into());
            e.count = Some(4.0);
            e.radius = Some(100.0);
            e.start_angle = Some(90.0);
            e.radial_orient = Some(true);
        });
        let descriptor = record(&apply_edit(&world(full()), &edit));
        let view = describe(&descriptor);
        let count = duplicator_count(&view.spec);
        assert_eq!(count, 4.0);
        let at = mode_offset(&view.spec, 1, count, grid_cols(&view.spec), None).unwrap();
        assert!((at.base_dx + 100.0).abs() < 1e-9 && at.base_dy.abs() < 1e-9);
        assert_eq!(at.base_rot, 180.0);
    }

    #[test]
    fn path_mode_reads_its_count_cap_and_alignment_from_the_edit() {
        let edit = edit_with(|e| {
            e.mode = Some("path".into());
            e.count = Some(5000.0);
            e.path_align_tangent = Some(false);
        });
        let descriptor = record(&apply_edit(&world(full()), &edit));
        let view = describe(&descriptor);
        assert_eq!(duplicator_count(&view.spec), 500.0);
        assert!(!view.spec.path_align_tangent);
        assert_eq!(view.spec.mode, Some("path"));
    }
}
