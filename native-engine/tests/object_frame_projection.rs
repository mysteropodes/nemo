//! Original numerical fixtures through real native source geometry and the pure batch adapter.
//! Root's Frame/Scene/paint/clip integration and every runtime check remain separately UNRUN.
use super::{project_layer_3d, transform_layer_2d, CameraProjection, CameraView, ProjectionError};
use crate::evaluation::object_vector_kernels::render_vector_eval::{
    evaluate_vector_rest, EvalSegment, EvaluatedVector,
};
use crate::evaluation::property_projection_3d::{ExtraDelta, Pose3D};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_frame_packet::{
    FrameScopeKind, MotionValues, ObjectFrameError, ObjectFrameSelector,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const VECTORS: &str = include_str!("fixtures/render-vector-eval/oracle.txt");
fn number(bits: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(bits, 16).unwrap())
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-10, "{a} != {b}");
}
fn motion() -> MotionValues {
    MotionValues {
        position: [12., -5.],
        anchor: [0., 0.],
        rotation: 0.,
        scale: [100., 100.],
        opacity: 100.,
    }
}
fn pose() -> Pose3D {
    Pose3D {
        position: [12., -5.],
        anchor: [0., 0.],
        rotation: 0.,
        scale: [100., 100.],
        position_z: 2000.,
        rotation_x: 0.,
        rotation_y: 0.,
    }
}
fn vector(index: usize, closed: bool, segments: Vec<EvalSegment>) -> EvaluatedVector {
    EvaluatedVector {
        source_object_index: index,
        closed,
        segments,
    }
}
fn seg(point: [f64; 2], handle_in: [f64; 2], handle_out: [f64; 2]) -> EvalSegment {
    EvalSegment {
        point,
        handle_in,
        handle_out,
    }
}

#[test]
fn original_exact_pathfx_parametric_fixtures_keep_numeric_closed_and_source_identity() {
    let mut seen = Vec::new();
    for line in VECTORS.lines().filter(|line| line.starts_with("vj1\t")) {
        let row: Vec<_> = line.split('\t').collect();
        if row[3] != "exact"
            || !(row[1].starts_with("plain-absent-stack@")
                || row[1].starts_with("rect-corner-tl-animated@"))
        {
            continue;
        }
        let doc = decode_storage_project(row[10].as_bytes()).unwrap();
        let snapshot = ObjectSnapshot::new("projection-fixture", doc).unwrap();
        let rest = evaluate_vector_rest(&snapshot, 0, row[2].parse().unwrap()).unwrap();
        let expected: Vec<Vec<f64>> = row[8]
            .split(';')
            .map(|s| s.split(',').map(number).collect())
            .collect();
        let batch = [rest.clone()];
        let moved = transform_layer_2d(&batch, &motion()).unwrap();
        let projected = project_layer_3d(&batch, &pose(), [320., 180.], &[None]).unwrap();
        assert_eq!(moved[0].closed, row[7] == "1");
        assert_eq!(projected[0].closed, rest.closed);
        assert_eq!(projected[0].source_object_index, rest.source_object_index);
        assert_eq!(projected[0].segments.len(), expected.len());
        for ((a, b), e) in moved[0]
            .segments
            .iter()
            .zip(&projected[0].segments)
            .zip(expected)
        {
            near(a.point[0], e[0] + 12.);
            near(a.point[1], e[1] - 5.);
            near(b.point[0], 160. + (e[0] + 12. - 160.) / 2.);
            near(b.point[1], 90. + (e[1] - 5. - 90.) / 2.);
            for (actual, wanted) in a.handle_in.into_iter().chain(a.handle_out).zip(&e[2..]) {
                near(actual, *wanted);
            }
            for (actual, wanted) in b.handle_in.into_iter().chain(b.handle_out).zip(&e[2..]) {
                near(actual, wanted / 2.);
            }
        }
        assert_eq!(rest, batch[0], "owned output never edits retained input");
        seen.push(row[1]);
    }
    assert!(seen.contains(&"plain-absent-stack@0"));
    assert!(seen.contains(&"rect-corner-tl-animated@0"));
}

#[test]
fn original_project3d_segments_literal_oracle_is_reused_without_regeneration() {
    let line = include_str!("fixtures/property-projection-3d/oracle.txt")
        .lines()
        .find(|line| line.starts_with("S "))
        .unwrap();
    let row: Vec<_> = line.split_whitespace().collect();
    let nums = |text: &str| text.split(',').map(number).collect::<Vec<_>>();
    let p = nums(row[1]);
    let bounds = nums(row[2]);
    let canvas = nums(row[3]);
    let parse = |text: &str| {
        text.split('|')
            .map(|text| {
                let fields: Vec<_> = text.split(';').collect();
                let point = nums(fields[0]);
                let handle = |text: &str| {
                    if text == "-" {
                        [0., 0.]
                    } else {
                        let n = nums(text);
                        [n[0], n[1]]
                    }
                };
                seg([point[0], point[1]], handle(fields[1]), handle(fields[2]))
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(row[4], "-");
    let input = vector(77, true, parse(row[5]));
    // The oracle supplies a whole-layer box larger than this stroke. A second actual path
    // reaches that box's opposite corner so the adapter derives exactly that union itself.
    let edge = vector(
        78,
        false,
        vec![seg(
            [bounds[0] + bounds[2], bounds[1] + bounds[3]],
            [0., 0.],
            [0., 0.],
        )],
    );
    let pose = Pose3D {
        position: [p[0], p[1]],
        anchor: [p[2], p[3]],
        rotation: p[4],
        scale: [p[5], p[6]],
        position_z: p[7],
        rotation_x: p[8],
        rotation_y: p[9],
    };
    let output =
        project_layer_3d(&[input, edge], &pose, [canvas[0], canvas[1]], &[None, None]).unwrap();
    let expected = parse(row[6]);
    for (a, b) in output[0].segments.iter().zip(expected) {
        for (a, b) in a
            .point
            .into_iter()
            .chain(a.handle_in)
            .chain(a.handle_out)
            .zip(b.point.into_iter().chain(b.handle_in).chain(b.handle_out))
        {
            near(a, b);
        }
    }
    assert_eq!(output[0].source_object_index, 77);
    assert!(output[0].closed);
}

#[test]
fn whole_layer_pivot_uses_cubic_extrema_not_control_hull_or_individual_record_bounds() {
    // This cubic has y=300t(1-t), hence max y=75, despite handles at y=100.
    let curve = vector(
        7,
        false,
        vec![
            seg([0., 0.], [0., 0.], [0., 100.]),
            seg([100., 0.], [0., 100.], [0., 0.]),
        ],
    );
    let point = vector(9, false, vec![seg([200., 0.], [0., 0.], [0., 0.])]);
    let batch = [curve, point];
    let mut m = motion();
    m.position = [0., 0.];
    m.anchor = [10., 5.];
    m.rotation = 180.;
    let output = transform_layer_2d(&batch, &m).unwrap();
    near(output[0].segments[0].point[0], 220.);
    near(output[0].segments[0].point[1], 85.);
    near(output[1].segments[0].point[0], 20.);
    near(output[1].segments[0].point[1], 85.);
    let mut p = pose();
    p.position = [0., 0.];
    p.anchor = [10., 5.];
    p.rotation = 180.;
    p.position_z = 0.;
    let output = project_layer_3d(&batch, &p, [320., 180.], &[None, None]).unwrap();
    near(output[0].segments[0].point[0], 220.);
    near(output[0].segments[0].point[1], 85.);
    near(output[1].segments[0].point[0], 20.);
    near(output[1].segments[0].point[1], 85.);
}

#[test]
fn perspective_projects_absolute_handles_and_per_copy_depth_without_double_layer_position() {
    let batch = [vector(
        4,
        false,
        vec![
            seg([0., 0.], [0., 0.], [50., 0.]),
            seg([100., 0.], [0., 0.], [0., 0.]),
        ],
    )];
    let mut p = pose();
    p.position = [0., 0.];
    p.position_z = 0.;
    p.rotation_y = 90.;
    let output = project_layer_3d(&batch, &p, [200., 100.], &[None]).unwrap();
    near(output[0].segments[0].point[0], 51.21951219512195);
    near(output[0].segments[0].point[1], 1.21951219512195);
    near(output[0].segments[0].handle_out[0], -1.21951219512195);
    near(output[0].segments[0].handle_out[1], -1.21951219512195);
    p = pose();
    let clones = [batch[0].clone(), batch[0].clone()];
    let output = project_layer_3d(
        &clones,
        &p,
        [320., 180.],
        &[
            None,
            Some(ExtraDelta {
                dz: Some(2000.),
                ..ExtraDelta::default()
            }),
        ],
    )
    .unwrap();
    near(output[0].segments[0].point[0], 86.); // (0+12-160)/2 +160, consumed once
    near(output[1].segments[0].point[0], 160. - 148. / 3.);
    assert_eq!(output[0].source_object_index, 4);
    assert_eq!(output[1].source_object_index, 4);
}

fn source() -> Value {
    json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24,"cameraLayerOn":true,
            "cameraKeys":[{"frame":0,"x":160,"y":90,"w":320},{"frame":4,"x":160,"y":90,"w":160}]},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":8,
            "layers":[{"layerUid":"L0"}],"objects":[]},"viewportReference":null,"assets":[]})
}
fn owner(source: &Value) -> NativeObjectHistory {
    NativeObjectHistory::from_project_file("projection-camera", &source.to_string().into_bytes())
        .unwrap()
}
fn snapshot(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn pins(s: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}

#[test]
fn actual_stored_camera_is_explicit_frame_sampled_and_retained_without_session_state() {
    let o = owner(&source());
    let s = snapshot(&o);
    let selector = pins(&s, 2);
    let batch = [vector(
        3,
        true,
        vec![seg([320., 180.], [-30., 0.], [0., 20.])],
    )];
    let canvas = CameraProjection::for_snapshot(&s, &selector, CameraView::Canvas).unwrap();
    assert_eq!(canvas.matrix(), None);
    assert_eq!(canvas.apply_vectors(&s, &selector, &batch).unwrap(), batch);
    let camera = CameraProjection::for_snapshot(&s, &selector, CameraView::StoredCamera).unwrap();
    let matrix = camera.matrix().unwrap();
    for (a, b) in matrix
        .into_iter()
        .zip([4. / 3., 0., 0., 4. / 3., -160. / 3., -30.])
    {
        near(a, b);
    }
    let output = camera.apply_vectors(&s, &selector, &batch).unwrap();
    near(output[0].segments[0].point[0], 1120. / 3.);
    near(output[0].segments[0].point[1], 210.);
    near(output[0].segments[0].handle_in[0], -40.);
    near(output[0].segments[0].handle_out[1], 80. / 3.);
    assert_eq!(output[0].source_object_index, 3);
    assert!(output[0].closed);
    assert_eq!(o.history_depths(), (0, 0));
    let saved = o.serialize_project_file(0, s.snapshot_id()).unwrap();
    assert!(!String::from_utf8(saved.clone())
        .unwrap()
        .contains("cameraView"));
    let reopened = NativeObjectHistory::from_project_file("fresh", &saved).unwrap();
    let fresh = snapshot(&reopened);
    assert_eq!(
        CameraProjection::for_snapshot(&fresh, &pins(&fresh, 2), CameraView::StoredCamera)
            .unwrap()
            .matrix(),
        Some(matrix)
    );
}

#[test]
fn dormant_camera_invalid_pose_wrong_source_frame_and_nonfinite_outputs_refuse_exactly() {
    let mut value = source();
    value["project"]["cameraKeys"][0]["w"] = json!(0);
    let o = owner(&value);
    let s = snapshot(&o);
    assert!(CameraProjection::for_snapshot(&s, &pins(&s, 0), CameraView::Canvas).is_ok());
    assert_eq!(
        CameraProjection::for_snapshot(&s, &pins(&s, 0), CameraView::StoredCamera).unwrap_err(),
        ProjectionError::CameraUnavailable
    );
    let o = owner(&source());
    let s = snapshot(&o);
    let selector = pins(&s, 2);
    let camera = CameraProjection::for_snapshot(&s, &selector, CameraView::StoredCamera).unwrap();
    let mut wrong = selector.clone();
    wrong.instance_id = "foreign";
    assert_eq!(
        CameraProjection::for_snapshot(&s, &wrong, CameraView::Canvas).unwrap_err(),
        ProjectionError::Frame(ObjectFrameError::WrongInstance)
    );
    wrong = selector.clone();
    wrong.context_id = "component";
    assert_eq!(
        CameraProjection::for_snapshot(&s, &wrong, CameraView::Canvas).unwrap_err(),
        ProjectionError::UnsupportedContext
    );
    assert_eq!(
        camera.apply_vectors(&s, &pins(&s, 3), &[]),
        Err(ProjectionError::FrameMismatch)
    );
    let other = owner(&source());
    let foreign = snapshot(&other);
    assert!(camera
        .apply_vectors(&foreign, &pins(&foreign, 2), &[])
        .is_err());
    let batch = [vector(0, false, vec![seg([0., 0.], [0., 0.], [0., 0.])])];
    assert_eq!(
        project_layer_3d(&batch, &pose(), [320., 180.], &[]),
        Err(ProjectionError::MismatchedExtras)
    );
    assert_eq!(
        project_layer_3d(&batch, &pose(), [0., 180.], &[None]),
        Err(ProjectionError::InvalidCanvas)
    );
    let mut bad = motion();
    bad.position[0] = f64::NAN;
    assert_eq!(
        transform_layer_2d(&batch, &bad),
        Err(ProjectionError::NonFinite)
    );
    let mut bad = pose();
    bad.rotation_x = f64::INFINITY;
    assert_eq!(
        project_layer_3d(&batch, &bad, [320., 180.], &[None]),
        Err(ProjectionError::NonFinite)
    );
    let huge = [vector(
        0,
        false,
        vec![seg([f64::MAX, 0.], [0., 0.], [0., 0.])],
    )];
    assert_eq!(
        camera.apply_vectors(&s, &selector, &huge),
        Err(ProjectionError::NonFinite)
    );
    assert_eq!(
        project_layer_3d(&[], &pose(), [320., 180.], &[]).unwrap(),
        []
    );
}
