//! Real retained scene/paint/Vello/SVG consumers of the original stored rig oracle.
//! No Recorder sink and no caller-supplied bones, bind, pose or mesh provider.
//! The v2 wrapper supplies explicit natural order only; original rig/rest/expected
//! fixture values remain unchanged. Mesh and combined geometry stages stay explicit.
#[path = "rig_pose_eval_rows.rs"]
mod rows;
use crate::export_job::native_vector_export::native_vector_export_svg::{
    serialize_svg_frame, SvgFrameSpec,
};
use crate::object_codec::encode_storage_project;
use crate::object_render_scene::object_scene_style_emit::prepare_item;
use crate::object_render_scene::{prepare_object_render_scene, ObjectSceneError};
use rows::{close, row, rows};
use serde_json::json;
use vello::kurbo::Affine;
use vello::Scene;

#[path = "rig_pose_scene_join_fixture.rs"]
mod fixture;
use fixture::{
    assert_path, document, expected_path, expected_segments, packet, plan, selector, snapshot_of,
    ARM,
};

#[test]
fn original_path_pose_rows_reach_the_real_retained_scene_and_vello_consumer() {
    let mut checked = 0;
    for row in rows().into_iter().filter(|r| {
        !r.strokes.is_empty()
            && r.meshes.as_object().unwrap().is_empty()
            && r.expect["throws"] == false
    }) {
        let mut source = document(&row);
        // The original open path has no paint. Give it a real native stroke so
        // the real Vello consumer is exercised without changing geometry/oracle.
        for record in source["objects"].as_array_mut().unwrap() {
            if record["geometry"]["closed"] == false {
                record["vector"]["stroke"] = json!({"kind":"solid","r":0,"g":0,"b":0,"a":1});
                record["vector"]["hasRealStroke"] = json!(true);
            }
        }
        let snapshot = snapshot_of(&source);
        let plan = plan(&snapshot, 0, false);
        assert_eq!(plan.commands().len(), row.strokes.len());
        for command in plan.commands() {
            let record = command.source().record();
            let expected = expected_path(
                &expected_segments(&row, &record.target.stroke_id),
                record.geometry.closed,
            );
            assert_path(command.path(), &expected, &row.name);
            let paint = prepare_item(&command.style().unwrap()).unwrap();
            let mut actual = Scene::new();
            let mut control = Scene::new();
            paint.draw(&mut actual, Affine::IDENTITY, command.path());
            paint.draw(&mut control, Affine::IDENTITY, &expected);
            assert!(actual.encoding().n_paths > 0, "{}", row.name);
            assert_eq!(actual.encoding().n_paths, control.encoding().n_paths);
            assert_eq!(
                actual.encoding().path_data,
                control.encoding().path_data,
                "{}: Vello must encode the original posed path",
                row.name
            );
            // This is CPU encoding by the compositor's production paint type;
            // device rendering/readback remains a separately booked check.
        }
        checked += 1;
    }
    assert_eq!(checked, 10);
}

#[test]
fn real_svg_serializes_the_same_posed_contours_and_keeps_source_geometry_rest() {
    let mut checked = 0;
    for row in rows().into_iter().filter(|r| {
        !r.strokes.is_empty()
            && r.strokes.iter().all(|s| s["closed"] == true)
            && r.meshes.as_object().unwrap().is_empty()
            && r.expect["throws"] == false
    }) {
        let snapshot = snapshot_of(&document(&row));
        let before = encode_storage_project(snapshot.document()).unwrap();
        let plan = plan(&snapshot, 0, false);
        let svg =
            serialize_svg_frame(&plan, SvgFrameSpec::new(512, 512, 1_000_000).unwrap()).unwrap();
        let text = std::str::from_utf8(&svg.bytes).unwrap();
        assert_eq!(svg.path_count, row.strokes.len());
        for command in plan.commands() {
            let segments = command.contour().segments();
            let expected = expected_segments(&row, &command.source().record().target.stroke_id);
            for (actual, expected) in segments.iter().zip(&expected) {
                let values = [
                    actual.point[0],
                    actual.point[1],
                    actual.handle_in[0],
                    actual.handle_in[1],
                    actual.handle_out[0],
                    actual.handle_out[1],
                ];
                for (a, b) in values.iter().zip(expected) {
                    assert!(close(*a, *b));
                }
            }
            // The real serializer must read the retained contour, not the rest
            // source. Pin its literal move endpoint and every cubic from that
            // same command; oracle comparison above independently checks pose.
            let number = |value: f64| {
                if value == 0.0 {
                    "0".into()
                } else {
                    format!("{value}")
                }
            };
            let point = |p: [f64; 2]| format!("{},{}", number(p[0]), number(p[1]));
            let mut data = format!("<path d=\"M{}", point(segments[0].point));
            for (index, a) in segments.iter().enumerate() {
                let b = segments[(index + 1) % segments.len()];
                data.push_str(&format!(
                    " C{} {} {}",
                    point([a.point[0] + a.handle_out[0], a.point[1] + a.handle_out[1]]),
                    point([b.point[0] + b.handle_in[0], b.point[1] + b.handle_in[1]]),
                    point(b.point)
                ));
            }
            data.push_str(" Z\"");
            assert!(text.contains(&data), "{}: {text}", row.name);
        }
        assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
        checked += 1;
    }
    assert_eq!(checked, 9);
}

#[test]
fn retained_scene_and_export_are_detached_repeatable_and_held_at_one_source_key() {
    let mut source = document(&row(ARM));
    source["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing",
        "formatVersion":1,"family":"drawing","contextId":"scene-root",
        "keys":[{"frame":0},{"frame":2}],"samples":[]});
    source["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"frameScope":{"kind":"authored","frame":2},"elements":[]}));
    let snapshot = snapshot_of(&source);
    let before = encode_storage_project(snapshot.document()).unwrap();
    let first = plan(&snapshot, 0, true);
    let held = plan(&snapshot, 1, true);
    let again = plan(&snapshot, 0, true);
    assert_eq!(
        first.commands().next().unwrap().path(),
        held.commands().next().unwrap().path()
    );
    assert_eq!(
        first.commands().next().unwrap().path(),
        again.commands().next().unwrap().path()
    );
    let spec = SvgFrameSpec::new(512, 512, 1_000_000).unwrap();
    let mut detached = serialize_svg_frame(&first, spec).unwrap();
    let original = detached.bytes.clone();
    detached.bytes.fill(0);
    assert_eq!(serialize_svg_frame(&again, spec).unwrap().bytes, original);
    assert_eq!(plan(&snapshot, 2, true).commands().len(), 0);
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
}

#[test]
fn unlinked_path_bind_keeps_stored_geometry_and_does_not_mutate_the_rig() {
    let mut source = document(&row(ARM));
    source["layers"][0]["rig"]["binds"][0]["strokeId"] = json!("missing");
    let snapshot = snapshot_of(&source);
    let packet = packet(&snapshot, 0, false);
    let poses = super::packet_poses(&packet);
    assert!(matches!(
        &poses[&0].as_ref().unwrap().binds[0],
        crate::evaluation::object_image_mesh_kernels::rig_pose_eval::BindOutcome::Unlinked(_)
    ));
    let plan = plan(&snapshot, 0, false);
    let command = plan.commands().next().unwrap();
    assert_eq!(command.contour().segments()[1].point, [190.0, -20.0]);
    assert_eq!(
        snapshot.document().layers()[0].rig().unwrap().binds.len(),
        1
    );
}

#[test]
fn real_kernel_failure_refuses_before_any_scene_is_retained() {
    for (name, reason) in [(
        "offset_entry_naming_a_missing_bone_throws",
        "stored rig pose evaluation refused the pinned frame",
    )] {
        let snapshot = snapshot_of(&document(&row(name)));
        let packet = packet(&snapshot, 0, false);
        let before = encode_storage_project(snapshot.document()).unwrap();
        assert!(
            matches!(prepare_object_render_scene(&packet, &selector(&snapshot, 0)),
            Err(ObjectSceneError::UnsupportedGeometry {source_object_index:0, reason:r}) if r == reason)
        );
        assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    }
}

#[test]
fn original_rest_and_duplicate_bind_order_are_real_negative_controls() {
    for (name, edit) in [(ARM, 0), ("duplicate_binds_for_one_stroke_later_wins", 1)] {
        let row = row(name);
        let original = snapshot_of(&document(&row));
        let mut changed = document(&row);
        if edit == 0 {
            changed["layers"][0].as_object_mut().unwrap().remove("rig");
        } else {
            changed["layers"][0]["rig"]["binds"]
                .as_array_mut()
                .unwrap()
                .reverse();
        }
        let changed = snapshot_of(&changed);
        assert_ne!(
            plan(&original, 0, false).commands().next().unwrap().path(),
            plan(&changed, 0, false).commands().next().unwrap().path()
        );
    }
}

#[test]
fn combined_rig_path_effects_are_explicit_and_wrong_packet_pins_are_atomic() {
    let mut source = document(&row("open_path_bind_rotate"));
    source["layers"][0]["pathFx"] = json!([{"type":"zigzag","p1":4,"p2":5}]);
    let snapshot = snapshot_of(&source);
    let packet = packet(&snapshot, 0, false);
    assert!(
        matches!(prepare_object_render_scene(&packet, &selector(&snapshot, 0)),
        Err(ObjectSceneError::UnsupportedGeometry {reason, ..})
        if reason == "rig pose with path effects, animated shape rebuild or brush mirrors is unavailable")
    );
    let mut pins = selector(&snapshot, 0);
    pins.content_revision += 1;
    assert_eq!(
        prepare_object_render_scene(&packet, &pins).unwrap_err(),
        ObjectSceneError::WrongRevision
    );
    pins.content_revision -= 1;
    pins.frame = 1;
    assert_eq!(
        prepare_object_render_scene(&packet, &pins).unwrap_err(),
        ObjectSceneError::WrongFrame
    );
}

#[test]
fn root_provider_ignores_foreign_context_shadow_in_both_storage_orders() {
    let original = document(&row(ARM));
    let expected = plan(&snapshot_of(&original), 0, false)
        .commands()
        .next()
        .unwrap()
        .path()
        .clone();
    for (first, short) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut source = original.clone();
        let mut foreign = source["objects"][0].clone();
        foreign["target"]["contextId"] = json!("component:opaque:雪");
        foreign["geometry"]["closed"] = json!(true);
        // Null rest handles retain their LIVE stored counterparts: this makes
        // choosing the foreign base visible even though bind.rest is unchanged.
        foreign["geometry"]["segments"][3]["handleIn"]["x"] = json!(777);
        if short {
            foreign["geometry"]["segments"]
                .as_array_mut()
                .unwrap()
                .pop();
        }
        if first {
            source["objects"].as_array_mut().unwrap().insert(0, foreign);
        } else {
            source["objects"].as_array_mut().unwrap().push(foreign);
        }
        let mut order = source["compositionOrder"]["contexts"][0].clone();
        order["contextId"] = json!("component:opaque:雪");
        source["compositionOrder"]["contexts"]
            .as_array_mut()
            .unwrap()
            .push(order);
        let snapshot = snapshot_of(&source);
        let packet = packet(&snapshot, 0, false);
        assert_eq!(packet.records().len(), 1);
        assert!(matches!(
            snapshot.document().rig_links("L1", 0, None).unwrap()[0],
            crate::object_document::object_rig_record::BindLink::Linked { vertex_count: 4 }
        ));
        assert_eq!(
            plan(&snapshot, 0, false).commands().next().unwrap().path(),
            &expected
        );
        assert!(matches!(
            &super::packet_poses(&packet)[&0].as_ref().unwrap().binds[0],
            crate::evaluation::object_image_mesh_kernels::rig_pose_eval::BindOutcome::Posed
        ));
    }
}

#[test]
fn same_opaque_uid_in_real_component_tables_cannot_supply_the_root_rig() {
    let mut source = document(&row(ARM));
    let expected = plan(&snapshot_of(&source), 0, false)
        .commands()
        .next()
        .unwrap()
        .path()
        .clone();
    let mut inner = source["objects"][0].clone();
    inner["target"]["contextId"] = json!("component:opaque:雪");
    inner["geometry"]["segments"][3]["handleIn"]["x"] = json!(777);
    let mut inner_layer = source["layers"][0].clone();
    inner_layer["rig"]["binds"] = json!([]);
    source["componentTables"] = json!({"components":{
        "format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"component:opaque:雪","totalFrames":4,
            "layers":[inner_layer],"objects":[inner]}],"instances":[]}});
    let snapshot = snapshot_of(&source);
    assert_eq!(
        snapshot
            .document()
            .component_set()
            .unwrap()
            .components
            .len(),
        1
    );
    assert_eq!(
        plan(&snapshot, 0, false).commands().next().unwrap().path(),
        &expected
    );
}

#[test]
fn unavailable_unselected_mesh_bind_preserves_paths_but_selected_mesh_refuses_atomically() {
    let row = row("mixed_path_mesh_and_binds_relink_drops");
    let mut source = document(&row);
    let snapshot = snapshot_of(&source);
    let path_packet = packet(&snapshot, 0, false);
    assert!(super::packet_poses(&path_packet)[&0]
        .as_ref()
        .unwrap()
        .binds
        .iter()
        .any(|bind| matches!(
            bind,
            crate::evaluation::object_image_mesh_kernels::rig_pose_eval::BindOutcome::Unavailable(
                _
            )
        )));
    let positive = plan(&snapshot, 0, false);
    assert_eq!(positive.commands().len(), 2);
    for command in positive.commands() {
        let id = &command.source().record().target.stroke_id;
        assert_path(
            command.path(),
            &expected_path(&expected_segments(&row, id), true),
            id,
        );
    }
    // A real selected raster uses the existing document item/order admission.
    // Its production source refuses BEFORE pixel attachment or scene retention;
    // available contours must not turn this into a successful subset scene.
    source["items"] = json!({"raster":[{
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
            "layerUid":"L1","strokeId":"raster:mesh"},
        "item":{"assetId":"image:source","x":150,"y":0,"width":300,"height":100,"meshId":"im_1"}
    }]});
    source["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"L1","strokeId":"raster:mesh",
            "naturalPosition":2,"order":{}}));
    let snapshot = snapshot_of(&source);
    let packet = packet(&snapshot, 0, false);
    assert_eq!(packet.raster_records().len(), 1);
    assert!(
        matches!(prepare_object_render_scene(&packet, &selector(&snapshot, 0)),
        Err(ObjectSceneError::UnsupportedGeometry {reason, ..})
        if reason == "native mesh rig pose is unavailable")
    );
}

#[test]
fn legacy_pose_must_match_selected_stored_id_closed_flag_and_topology() {
    let snapshot = snapshot_of(&document(&row(ARM)));
    let packet = packet(&snapshot, 0, false);
    let record = packet.records().next().unwrap();
    let poses = super::packet_poses(&packet);
    let pose = &poses[&0].as_ref().unwrap().strokes[0];
    let batches = Default::default();
    assert!(super::compile(&packet, record, Some(pose), &batches).is_ok());
    for field in 0..3 {
        let mut wrong = pose.clone();
        match field {
            0 => wrong.stroke_id.push_str(":foreign"),
            1 => wrong.closed = !wrong.closed,
            _ => {
                wrong.segments.pop();
            }
        }
        assert!(matches!(
            super::compile(&packet, record, Some(&wrong), &batches),
            Err("rig pose topology differs from the selected stored path")
        ));
    }
}
