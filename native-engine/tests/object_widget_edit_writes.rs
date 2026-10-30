//! Write-from-local through the actual owner: baseline write rows and hand-pinned drags (kernel-gated).
use super::object_widget_edit_rows::*;
use super::*;
use serde_json::{json, Value};

fn triple(token: &str) -> [f64; 3] {
    let parts: Vec<f64> = token.split(',').map(f).collect();
    [parts[0], parts[1], parts[2]]
}

#[test]
fn write_rows_match_the_executed_baseline_writes() {
    let mut checked = 0;
    for row in rows("wr") {
        let cfg: Vec<&str> = row[2].split('|').collect();
        let kind = cfg[0];
        let pair = |token: &str| -> [f64; 2] {
            let parts: Vec<f64> = token.split(',').map(f).collect();
            [parts[0], parts[1]]
        };
        let (size, anchor, angle) = (pair(cfg[1]), pair(cfg[2]), f(cfg[3]));
        let (x, y) = (triple(cfg[4]), (cfg[5] != "-").then(|| triple(cfg[5])));
        let mut set = json!({"size":[jnum(size[0]), jnum(size[1])],
            "x":{"min":jnum(x[0]),"max":jnum(x[1]),"rest":jnum(x[2])}});
        if let Some(y) = y {
            set["y"] = json!({"min":jnum(y[0]),"max":jnum(y[1]),"rest":jnum(y[2])});
        }
        let mut owner = owner_of(&base(1));
        run_ok(&mut owner, "c", "create", create_payload(kind, "W", anchor));
        run_ok(&mut owner, "s", "settings", settings_payload(UID, set));
        let to = match &row[3][..1] {
            "L" => local(pair(&row[3][2..])),
            "W" => {
                let point = pair(&row[3][2..]);
                json!({"kind":"world","point":[jnum(point[0]), jnum(point[1])],
                    "anchor":[jnum(anchor[0]), jnum(anchor[1])],"angleDegrees":jnum(angle)})
            }
            "R" => json!({"kind":"rest"}),
            other => panic!("intent {other}"),
        };
        run_ok(&mut owner, "w", "write", write_payload(UID, json!(0), to));
        let written = wire(&owner);
        let parts: Vec<&str> = row[4].split(';').collect();
        assert_eq!(
            parts.len(),
            if y.is_some() { 2 } else { 1 },
            "{} writes",
            row[1]
        );
        for part in parts {
            let (axis, hex) = part.split_once('=').unwrap();
            let suffix = match (axis, kind) {
                ("x", "slider") => "value",
                (axis, _) => axis,
            };
            // JSON has no negative zero, and neither has the baseline file it mirrors.
            let want = f(hex) + 0.0;
            let got = static_of(&written, &key(UID, suffix));
            assert_eq!(
                got.to_bits(),
                want.to_bits(),
                "{} {} {axis}: {got} vs {want}",
                row[1],
                row[3]
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 241);
}

#[test]
fn a_rotated_and_sized_joystick_maps_the_world_drag_to_x_50_and_y_minus_50_exactly() {
    let mut owner = joystick(Some(json!({"size":[200,200]})));
    let to = json!({"kind":"world","point":[978.3012701892219, 608.3012701892219],
        "anchor":[960,540],"angleDegrees":30});
    run_ok(&mut owner, "w", "write", write_payload(UID, json!(0), to));
    let written = wire(&owner);
    assert_eq!(
        record(&written, &key(UID, "x")),
        json!({"property":key(UID,"x"),"static":[50]})
    );
    assert_eq!(
        record(&written, &key(UID, "y")),
        json!({"property":key(UID,"y"),"static":[-50]})
    );
    assert_eq!(
        owner.history_depths(),
        (3, 0),
        "create, settings and ONE write for both axes"
    );
    owner.undo(3).unwrap();
    let undone = wire(&owner);
    assert_eq!(static_of(&undone, &key(UID, "x")), 0.);
    assert_eq!(static_of(&undone, &key(UID, "y")), 0.);
}

#[test]
fn local_drags_clamp_to_the_axis_range_and_a_slider_writes_only_its_one_control() {
    let mut owner = joystick(Some(json!({"size":[200,200]})));
    run_ok(
        &mut owner,
        "w",
        "write",
        write_payload(UID, json!(0), local([300., -300.])),
    );
    let clamped = wire(&owner);
    assert_eq!(
        (
            static_of(&clamped, &key(UID, "x")),
            static_of(&clamped, &key(UID, "y"))
        ),
        (100., 100.)
    );
    let mut slider = owner_of(&base(1));
    run_ok(
        &mut slider,
        "c",
        "create",
        create_payload("slider", "Slider 1", [960., 540.]),
    );
    run_ok(
        &mut slider,
        "w",
        "write",
        write_payload(UID, json!(0), local([50., 999.])),
    );
    let only = wire(&slider);
    assert_eq!(
        only["propertyHolders"][0]["properties"],
        json!([{"property":key(UID,"value"),"static":[75]}])
    );
}

#[test]
fn the_rest_return_writes_the_rest_values_of_both_axes() {
    let mut owner = joystick(Some(json!({"x":{"rest":40},"y":{"min":0,"rest":150}})));
    run_ok(
        &mut owner,
        "w",
        "write",
        write_payload(UID, json!(0), local([100., 100.])),
    );
    run_ok(
        &mut owner,
        "r",
        "write",
        write_payload(UID, json!(0), json!({"kind":"rest"})),
    );
    let rested = wire(&owner);
    assert_eq!(
        (
            static_of(&rested, &key(UID, "x")),
            static_of(&rested, &key(UID, "y"))
        ),
        (40., 100.)
    );
}

#[test]
fn an_animated_control_gets_a_key_at_the_playhead_and_a_static_one_a_static_value() {
    let mut owner = joystick(None);
    property_edit(
        &mut owner,
        "key",
        &key(UID, "x"),
        json!({"kind":"setKey","frame":0,"values":[0]}),
    );
    run_ok(
        &mut owner,
        "w",
        "write",
        write_payload(UID, json!(12), local([-20., 40.])),
    );
    let keyed = wire(&owner);
    let x = record(&keyed, &key(UID, "x"));
    let keys = x["track"]["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 2);
    assert_eq!(
        (keys[1]["frame"].clone(), keys[1]["v"].clone()),
        (json!(12), json!([-25]))
    );
    assert_eq!(
        x["static"],
        json!([0]),
        "a keyed write leaves the static value alone"
    );
    let y = record(&keyed, &key(UID, "y"));
    assert_eq!(
        y,
        json!({"property":key(UID,"y"),"static":[-50]}),
        "no track appears on a static control"
    );
    assert_eq!(owner.history_depths(), (3, 0));
}

#[test]
fn a_widget_layer_adds_no_objects_and_leaves_the_pinned_frame_packet_records_unchanged() {
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let doc = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases["records"]});
    let mut owner = owner_of(&doc);
    let records = |owner: &NativeObjectHistory| {
        let snapshot = current(owner);
        let selector = ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame: 7,
        };
        let packet = prepare_object_frame(&snapshot, &selector).unwrap();
        let records: Vec<_> = packet
            .records()
            .map(|r| (r.source_object_index(), r.record().clone()))
            .collect();
        (packet.source_layers().len(), records)
    };
    let (layers_before, before) = records(&owner);
    assert!(!before.is_empty(), "the pinned frame has drawn objects");
    // the fixture also holds a record on another frame, so the document total is not the pinned-frame count
    let objects_before = current(&owner).document().objects().len();
    run_ok(
        &mut owner,
        "c",
        "create",
        create_payload("joystick", "Joystick 1", [960., 540.]),
    );
    // The creation declares a layer-level property holder holding only the widget's controls. A widget layer never
    // paints and its controls are read by expressions elsewhere, so that holder is consumed: the pinned-frame packet
    // of the created document is admitted and carries the very same records plus exactly one more source layer.
    let (layers_after, after) = records(&owner);
    assert_eq!((layers_after, &after), (layers_before + 1, &before));
    // The same document without its holder table is not a different frame.
    let mut stripped = wire(&owner);
    stripped
        .as_object_mut()
        .unwrap()
        .remove("propertyHolders")
        .expect("creation declared the holder");
    assert_eq!(records(&owner_of(&stripped)), (layers_after, after));
    assert_eq!(current(&owner).document().objects().len(), objects_before);
}
