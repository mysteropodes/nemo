//! Actual public NativeObjectHistory command wrapper used by the native UI/MCP host.
//! Register once under the existing binding-history test parent with feature application;
//! that parent already requires history, evaluation and project_media_codec. These controls do
//! not substitute the opacity-only NativeApplication or claim raw MCP transport acceptance.
use crate::history::NativeObjectHistory;
use crate::request_receipts::OpacityRequest;
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[path = "object_image_mesh_binding_fixtures.rs"]
mod fixtures;
#[path = "object_raster_asset_commands_support.rs"]
mod support;
use fixtures::*;

fn wire(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&support::bytes(owner)).unwrap()
}
fn request(
    owner: &NativeObjectHistory,
    id: &str,
    operation: &str,
    payload: Value,
) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":id,
        "instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "expectedRevision":owner.content_revision(),"operation":operation,"payload":payload}))
    .unwrap()
}
fn dispatch(owner: &mut NativeObjectHistory, request: &OpacityRequest) -> Value {
    // This is the exact public route NativeObjectHost::dispatch calls, including its real bake
    // preparation callback. A refusal or None is a genuine failure; there is no lower-level retry.
    let response = owner
        .dispatch_object_command(request)
        .expect("public object-command wrapper accepted correlation")
        .expect("public object-command wrapper owns this operation");
    serde_json::to_value(response).unwrap()
}
fn mesh_command(owner: &mut NativeObjectHistory, id: &str, payload: Value) -> Value {
    let request = request(
        owner,
        id,
        "command.document.object.image.mesh.edit",
        payload,
    );
    let reply = dispatch(owner, &request);
    assert_eq!(reply["ok"], true, "{id}: {reply}");
    reply
}
fn history_command(owner: &mut NativeObjectHistory, id: &str, undo: bool) -> Value {
    let (operation, command) = if undo {
        ("command.document.object.undo", "object.undo")
    } else {
        ("command.document.object.redo", "object.redo")
    };
    let request = request(owner, id, operation, json!({"command":command}));
    let reply = dispatch(owner, &request);
    assert_eq!(reply["ok"], true, "{id}: {reply}");
    reply
}
fn assert_tag(owner: &NativeObjectHistory, target: Value, expected: Option<&str>) {
    let drawing = wire(owner);
    let raster = drawing["items"]["raster"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["target"] == target)
        .expect("retained full raster target");
    assert_eq!(raster["item"]["meshId"].as_str(), expected, "{target}");
}
fn assert_layer_tags(owner: &NativeObjectHistory, layer: &str, expected: Option<&str>) {
    let drawing = wire(owner);
    let entries: Vec<_> = drawing["items"]["raster"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| {
            entry["target"]["contextId"] == "scene-root" && entry["target"]["layerUid"] == layer
        })
        .collect();
    assert_eq!(
        entries.len(),
        24,
        "all applicable retained frames in {layer}"
    );
    let mut frames = BTreeSet::new();
    for entry in entries {
        assert_eq!(entry["target"]["frameScope"]["kind"], "authored");
        frames.insert(entry["target"]["frameScope"]["frame"].as_u64().unwrap());
        assert_tag(owner, entry["target"].clone(), expected);
    }
    assert_eq!(frames, (0..24).collect::<BTreeSet<_>>());
}
fn project_file(owner: &NativeObjectHistory) -> Vec<u8> {
    let snapshot = support::head(owner);
    owner
        .serialize_project_file(owner.content_revision(), snapshot.snapshot_id())
        .unwrap()
}

#[test]
fn public_binding_reaches_the_native_scene_for_every_scrub_frame_after_history_and_reopen() {
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
    use crate::object_render_scene::{
        prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
    };
    use crate::object_snapshot::object_raster_asset_resources::AssetPixels;

    let mut value = drawing(24, None);
    value["formatVersion"] = json!(2);
    value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
        "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":[
            {"layerUid":"L0","naturalPosition":0,"order":{}},
            {"layerUid":"L2","naturalPosition":1,"order":{}}],
            "scopes":(0..24).map(|frame| scope(frame,vec![element("L0",&format!("raster-{frame}"),0)])).collect::<Vec<_>>()}]});
    let mut owner = support::owner(&value);
    mesh_command(&mut owner, "scene-attach", attach(at(13)));
    let attached = support::head(&owner);
    let saved = project_file(&owner);
    history_command(&mut owner, "scene-undo", true);
    let undone = support::head(&owner);
    history_command(&mut owner, "scene-redo", false);
    let redone = support::head(&owner);
    let reopened = NativeObjectHistory::from_project_file("scene-reopened", &saved).unwrap();
    let mut version = None;
    // The unbound state establishes the real fixture/asset/order route first.
    // Tagged states must use that same production consumer, never a flat-image fallback.
    for (label, snapshot, expected) in [
        ("undo", undone, None),
        ("attached", attached, Some("im_1")),
        ("redo", redone, Some("im_1")),
        ("reopened", support::head(&reopened), Some("im_1")),
    ] {
        for frame in 0..24 {
            let selector = ObjectFrameSelector {
                instance_id: snapshot.instance_id(),
                document_id: snapshot.document_id(),
                content_revision: snapshot.content_revision(),
                document_snapshot_id: snapshot.snapshot_id(),
                context_id: "scene-root",
                scope_kind: FrameScopeKind::Authored,
                frame,
            };
            let packet = prepare_object_frame(&snapshot, &selector).unwrap();
            assert_eq!(packet.raster_records().len(), 1, "{label}/{frame}");
            let raster = packet.raster_records().next().unwrap().entry();
            assert_eq!(serde_json::to_value(&raster.target).unwrap(), at(frame));
            assert_eq!(raster.item.mesh_id(), expected, "{label}/{frame}");
            assert_eq!(raster.item.asset_id().as_str(), "img-1");
            let retained = snapshot
                .document()
                .item_records()
                .unwrap()
                .raster_entries()
                .iter()
                .find(|entry| entry.target == raster.target)
                .unwrap();
            assert!(
                std::ptr::eq(raster, retained),
                "retained identity at {label}/{frame}"
            );
            let scene = prepare_object_render_scene(&packet, &selector)
                .unwrap_or_else(|error| panic!("native mesh scene {label}/{frame}: {error:?}"));
            let plan = prepare_object_paint_plan(&scene, &selector).unwrap();
            let resources = plan.scene_resource_versions().unwrap();
            assert_eq!(resources.len(), 1, "one retained image at {label}/{frame}");
            assert_eq!(resources[0].0, "raster-asset:img-1");
            if let Some(version) = &version {
                assert_eq!(&resources[0].1, version);
            } else {
                version = Some(resources[0].1.clone());
            }
            let composition = prepare_object_composition(&plan, &selector).unwrap();
            assert!(
                composition.has_scene_images(),
                "native image at {label}/{frame}"
            );
            let pixels = AssetPixels::retain_for_scene(&snapshot, "img-1", &[]).unwrap();
            assert_eq!(pixels.encoded_png(), support::png());
            assert_eq!(pixels.snapshot().snapshot_id(), snapshot.snapshot_id());
        }
    }
}

#[test]
fn public_wrapper_attaches_detaches_and_releases_only_the_last_reference_across_24_frames() {
    let mut value = drawing(24, None);
    for frame in 0..24 {
        push_raster(
            &mut value,
            target("scene-root", "L2", frame, &format!("copy-{frame}")),
            None,
        );
    }
    let mut owner = support::owner(&value);
    let initial = support::bytes(&owner);
    assert_layer_tags(&owner, "L0", None);
    assert_layer_tags(&owner, "L2", None);
    mesh_command(&mut owner, "public-attach", attach(at(13)));
    assert_layer_tags(&owner, "L0", Some("im_1"));
    assert_layer_tags(&owner, "L2", None);
    assert_eq!(wire(&owner)["imageMeshes"]["im_1"], mesh());
    mesh_command(
        &mut owner,
        "public-share",
        payload(json!({"kind":"propagate",
        "target":target("scene-root","L2",7,"copy-7"),"meshId":"im_1"})),
    );
    assert_layer_tags(&owner, "L2", Some("im_1"));
    let shared = support::bytes(&owner);
    mesh_command(&mut owner, "public-detach-first", detach(at(9)));
    assert_layer_tags(&owner, "L0", None);
    assert_layer_tags(&owner, "L2", Some("im_1"));
    assert_eq!(wire(&owner)["imageMeshes"]["im_1"], mesh());
    let first_detached = support::bytes(&owner);
    mesh_command(
        &mut owner,
        "public-detach-last",
        detach(target("scene-root", "L2", 18, "copy-18")),
    );
    assert_layer_tags(&owner, "L0", None);
    assert_layer_tags(&owner, "L2", None);
    assert!(wire(&owner)["imageMeshes"].get("im_1").is_none());
    let last_detached = support::bytes(&owner);
    assert_ne!(
        last_detached, initial,
        "an authored empty store retains its presence"
    );
    assert_eq!(owner.history_depths(), (4, 0));
    history_command(&mut owner, "public-undo-last", true);
    assert_eq!(support::bytes(&owner), first_detached);
    assert_layer_tags(&owner, "L2", Some("im_1"));
    history_command(&mut owner, "public-undo-first", true);
    assert_eq!(support::bytes(&owner), shared);
    assert_layer_tags(&owner, "L0", Some("im_1"));
    history_command(&mut owner, "public-redo-first", false);
    assert_eq!(support::bytes(&owner), first_detached);
    history_command(&mut owner, "public-redo-last", false);
    assert_eq!(support::bytes(&owner), last_detached);
    assert_layer_tags(&owner, "L0", None);
    assert_layer_tags(&owner, "L2", None);
    assert!(wire(&owner)["imageMeshes"].get("im_1").is_none());
    let reopened =
        NativeObjectHistory::from_project_file("public-detached-reopen", &project_file(&owner))
            .unwrap();
    assert_eq!(support::bytes(&reopened), last_detached);
    assert_layer_tags(&reopened, "L0", None);
    assert_layer_tags(&reopened, "L2", None);
    assert!(wire(&reopened)["imageMeshes"].get("im_1").is_none());
}

#[test]
fn public_duplicate_workflow_preserves_all_destination_tags_and_receipts_through_history_and_reopen(
) {
    let mut owner = support::owner(&duplicated_layer_drawing());
    let initial = support::bytes(&owner);
    let initial_targets: Vec<_> = wire(&owner)["items"]["raster"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["target"].clone())
        .collect();
    let duplicate_request = request(
        &owner,
        "public-duplicate",
        "command.document.object.image.mesh.edit",
        duplicate("L2"),
    );
    let duplicate_reply = dispatch(&mut owner, &duplicate_request);
    assert_eq!(duplicate_reply["ok"], true);
    assert_eq!(
        duplicate_reply["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(duplicate_reply["contentRevision"], 1);
    assert_eq!(owner.history_depths(), (1, 0));
    assert_layer_tags(&owner, "L0", Some("im_1"));
    assert_layer_tags(&owner, "L2", Some("im_2"));
    let duplicated = support::bytes(&owner);
    assert_eq!(
        wire(&owner)["imageMeshes"]["im_1"],
        wire(&owner)["imageMeshes"]["im_2"]
    );
    let sculpt = mesh_command(
        &mut owner,
        "public-sculpt-copy",
        json!({
        "command":"object.image.mesh.edit","meshId":"im_2",
        "action":{"kind":"setOffset","index":8,"du":0.25,"dv":-0.5}}),
    );
    assert_eq!(
        sculpt["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(wire(&owner)["imageMeshes"]["im_1"], mesh());
    assert_eq!(
        wire(&owner)["imageMeshes"]["im_2"]["offsets"][8],
        json!([0.25, -0.5])
    );
    let sculpted = support::bytes(&owner);
    let undo_sculpt = history_command(&mut owner, "public-undo-sculpt", true);
    assert_eq!(
        undo_sculpt["result"],
        json!({"applied":true,"historyEntriesAdded":0})
    );
    assert_eq!(support::bytes(&owner), duplicated);
    let undo_duplicate = history_command(&mut owner, "public-undo-duplicate", true);
    assert_eq!(
        undo_duplicate["result"],
        json!({"applied":true,"historyEntriesAdded":0})
    );
    assert_eq!(support::bytes(&owner), initial);
    assert_eq!(owner.history_depths(), (0, 2));
    // A retained public receipt cannot execute an old duplicate again or erase the pending redo.
    let before_replay = support::state(&owner);
    assert_eq!(dispatch(&mut owner, &duplicate_request), duplicate_reply);
    assert_eq!(support::state(&owner), before_replay);
    let redo_duplicate = history_command(&mut owner, "public-redo-duplicate", false);
    assert_eq!(
        redo_duplicate["result"],
        json!({"applied":true,"historyEntriesAdded":0})
    );
    assert_eq!(support::bytes(&owner), duplicated);
    let redo_sculpt = history_command(&mut owner, "public-redo-sculpt", false);
    assert_eq!(
        redo_sculpt["result"],
        json!({"applied":true,"historyEntriesAdded":0})
    );
    assert_eq!(support::bytes(&owner), sculpted);
    assert_eq!(owner.history_depths(), (2, 0));
    let saved = project_file(&owner);
    let mut reopened =
        NativeObjectHistory::from_project_file("public-duplicate-reopen", &saved).unwrap();
    assert_eq!(project_file(&reopened), saved);
    assert_eq!(support::bytes(&reopened), sculpted);
    assert_eq!(reopened.history_depths(), (0, 0));
    assert_layer_tags(&reopened, "L0", Some("im_1"));
    assert_layer_tags(&reopened, "L2", Some("im_2"));
    let reopened_targets: Vec<_> = wire(&reopened)["items"]["raster"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["target"].clone())
        .collect();
    assert_eq!(
        reopened_targets, initial_targets,
        "all 48 full native targets survive the workflow"
    );
    let copied_again = mesh_command(
        &mut reopened,
        "public-duplicate-after-reopen",
        duplicate("L2"),
    );
    assert_eq!(
        copied_again["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_layer_tags(&reopened, "L0", Some("im_1"));
    assert_layer_tags(&reopened, "L2", Some("im_3"));
    assert_eq!(
        wire(&reopened)["imageMeshes"]["im_2"],
        wire(&reopened)["imageMeshes"]["im_3"]
    );
    history_command(&mut reopened, "public-undo-reopened-copy", true);
    assert_eq!(support::bytes(&reopened), sculpted);
}

fn rgb(bytes: &[u8], x: usize, y: usize, expected: [u8; 3]) {
    let offset = (y * 1920 + x) * 4;
    for channel in 0..3 {
        assert!(
            bytes[offset + channel].abs_diff(expected[channel]) <= 2,
            "pixel {x},{y} channel {channel}: {} != {}",
            bytes[offset + channel],
            expected[channel]
        );
    }
}

#[test]
fn original_mesh_pixels_reach_the_real_metal_viewport_compositor_and_png_at_three_frames() {
    use crate::compositor::Compositor;
    use fixtures::pixel_consumer::{composition, exported, fixture, pixels, plain_frames};
    let (mut owner, mesh) = fixture(false);
    let mut gpu = Compositor::new().expect("the original pixel criterion requires real Metal");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    let plain = plain_frames(&mut gpu, &owner);
    let (widget_owner, _) = fixture(true);
    assert_eq!(
        plain[0],
        pixels(
            &mut gpu,
            &composition(&widget_owner.acquire_snapshot(0).unwrap(), 0, true, 1.)
        ),
        "a native widget contributes zero pixels over all 1920x1080 pixels"
    );
    let plain_zoomed = pixels(
        &mut gpu,
        &composition(&owner.acquire_snapshot(0).unwrap(), 0, true, 0.75),
    );
    for (stage, action) in [
        (
            "rest",
            json!({"command":"object.image.mesh.edit","action":{"kind":"attach","target":at(13),"mesh":mesh}}),
        ),
        (
            "deformed",
            json!({"command":"object.image.mesh.edit","meshId":"im_1","action":{"kind":"setOffset","index":8,"du":0.1,"dv":0}}),
        ),
        (
            "mask",
            json!({"command":"object.image.mesh.edit","meshId":"im_1","action":{"kind":"setOffset","index":0,"du":0.25,"dv":0.25}}),
        ),
    ] {
        mesh_command(&mut owner, stage, action);
        let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
        for (index, (frame, dx, dy)) in [(0, 0, 0), (12, 40, 20), (23, 80, 40)]
            .into_iter()
            .enumerate()
        {
            let viewport = composition(&snapshot, frame, true, 1.);
            assert!(
                viewport.raster_image(0).is_none(),
                "a native mesh cannot expose the flat-image draw"
            );
            let rendered = pixels(&mut gpu, &viewport);
            assert_eq!(rendered.len(), 1920 * 1080 * 4);
            if stage == "rest" {
                let plain = &plain[index];
                let differing = plain
                    .chunks_exact(4)
                    .zip(rendered.chunks_exact(4))
                    .filter(|(a, b)| a != b)
                    .count();
                assert!(
                    differing <= 52,
                    "original rest tolerance: {differing}/2073600 pixels"
                );
                assert!(plain
                    .iter()
                    .zip(&rendered)
                    .all(|(a, b)| a.abs_diff(*b) <= 1));
            }
            rgb(&rendered, 240 + dx, 180 + dy, [220, 30, 30]);
            rgb(&rendered, 400 + dx, 180 + dy, [30, 60, 220]);
            rgb(&rendered, 150 + dx, 180 + dy, [255, 255, 255]);
            rgb(
                &rendered,
                330 + dx,
                180 + dy,
                if stage == "rest" {
                    [30, 60, 220]
                } else {
                    [220, 30, 30]
                },
            );
            if stage == "mask" {
                rgb(&rendered, 230 + dx, 90 + dy, [255, 255, 255]);
            }
            if frame == 0 {
                let zoomed = pixels(&mut gpu, &composition(&snapshot, frame, true, 0.75));
                if stage == "rest" {
                    let differing = plain_zoomed
                        .chunks_exact(4)
                        .zip(zoomed.chunks_exact(4))
                        .filter(|(a, b)| a != b)
                        .count();
                    assert!(differing <= 52, "zoomed rest seam tolerance: {differing}");
                    assert!(plain_zoomed
                        .iter()
                        .zip(&zoomed)
                        .all(|(a, b)| a.abs_diff(*b) <= 1));
                }
                // z=.75 about canvas centre (960,540), applied once.
                rgb(&zoomed, 420, 270, [220, 30, 30]);
                rgb(&zoomed, 540, 270, [30, 60, 220]);
                rgb(&zoomed, 353, 270, [255, 255, 255]);
                rgb(
                    &zoomed,
                    488,
                    270,
                    if stage == "rest" {
                        [30, 60, 220]
                    } else {
                        [220, 30, 30]
                    },
                );
                if stage == "mask" {
                    rgb(&zoomed, 413, 203, [255, 255, 255]);
                }
            }
            assert_eq!(
                rendered,
                exported(&owner, &mut gpu, &viewport, frame, stage)
            );
        }
    }
}
