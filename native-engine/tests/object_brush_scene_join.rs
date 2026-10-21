//! Prospective normal-frame acceptance. Native I/O, scheduler and GPU paths are
//! real; no manually assembled scene or replacement anchor contour is supplied.
use super::*;
use crate::history::NativeObjectHistory;
use crate::object_document::{BitmapBrushFields, FrameScopeKind};
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    ordered_entries, prepare_object_composition, prepare_object_paint_plan, PaintEntry,
};
use crate::revision::{ImageResourceBudget, ObjectSnapshot};
use base64::Engine;
use serde_json::{json, Value};

fn limits() -> crate::read_queries::brush_read::BrushReadLimits {
    crate::read_queries::brush_read::BrushReadLimits::new(1 << 20, 1 << 20, 16 << 20, 512, 512)
        .unwrap()
}
const AUTHORED_PIXELS: [u8; 16] = [0, 0, 255, 255, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 255, 255];
fn png() -> String {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&AUTHORED_PIXELS)
            .unwrap();
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
fn project(drawing: Value) -> Value {
    json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":48,"canvasH":48,"fps":24},"drawing":drawing,"viewportReference":null,
        "assets":[{"id":"ink","name":"Authored ink","kind":"image","source":{"kind":"embedded",
            "mediaType":"image/png","encoding":"base64","data":png()}}],"assetInventory":{"assetIds":["ink"]}})
}
fn owner(drawing: Value) -> NativeObjectHistory {
    NativeObjectHistory::from_project_file(
        "native-normal-brush",
        project(drawing).to_string().as_bytes(),
    )
    .unwrap()
}
fn selector(pin: &ObjectSnapshot) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: pin.instance_id(),
        document_id: pin.document_id(),
        content_revision: pin.content_revision(),
        document_snapshot_id: pin.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    }
}
fn runtime() -> BrushRuntime {
    BrushRuntime::for_controls(BrushBitmapStore::with_budget(
        ImageResourceBudget::isolated(32 << 20).unwrap(),
    ))
}
fn scene(
    pin: &ObjectSnapshot,
    runtime: &mut BrushRuntime,
    previous: Option<&BrushSources>,
) -> crate::object_render_scene::ObjectRenderScene {
    let selected = selector(pin);
    let packet = prepare_object_frame(pin, &selected).unwrap();
    prepare_object_render_scene_with_brush(&packet, &selected, runtime, previous, limits(), None)
        .unwrap()
}
fn bitmap(
    scene: &crate::object_render_scene::ObjectRenderScene,
) -> &crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::BrushBitmap {
    match &scene.brushes.as_ref().unwrap().paints[0].ink {
        InkState::Ink(ink) => &ink.bitmap,
        _ => panic!("ink required"),
    }
}
fn trimmed() -> Value {
    let mut drawing = BitmapBrushFields::test_drawing();
    drawing["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L","strokeId":"anchor"},
        "properties":[{"property":"trimEnd","static":[50]}]}]);
    drawing
}
#[test]
fn normal_source_keeps_actual_triangle_and_exactly_one_authored_png_at_its_order_slot() {
    let owner = owner(BitmapBrushFields::test_drawing());
    let pin = owner.acquire_snapshot(0).unwrap();
    let before = owner.serialize_project_file(0, pin.snapshot_id()).unwrap();
    let mut services = runtime();
    let s = scene(&pin, &mut services, None);
    let plan = prepare_object_paint_plan(&s, &selector(&pin)).unwrap();
    let labels: Vec<_> = ordered_entries(&plan)
        .map(|entry| match entry {
            PaintEntry::Contour(_) => "anchor",
            PaintEntry::Brush(_) => "ink",
            _ => "unexpected",
        })
        .collect();
    assert_eq!(labels, ["anchor", "ink"]);
    assert!(s.rasters.is_empty());
    assert_eq!(s.contours.len(), 1);
    assert_eq!(s.brushes.as_ref().unwrap().paints[0].source_index, 1);
    assert_eq!(bitmap(&s).rgba8(), AUTHORED_PIXELS);
    assert_eq!(bitmap(&s).dimensions(), (2, 2));
    let again = scene(&pin, &mut services, s.brushes.as_deref());
    assert!(bitmap(&s).same_allocation_as(bitmap(&again)));
    assert_eq!(
        s.brushes
            .as_ref()
            .unwrap()
            .images()
            .next()
            .unwrap()
            .data
            .id(),
        again
            .brushes
            .as_ref()
            .unwrap()
            .images()
            .next()
            .unwrap()
            .data
            .id()
    );
    let witness = prepare_object_composition(&plan, &selector(&pin)).unwrap();
    assert_eq!(witness.scene_resources().unwrap().len(), 1);
    let versions = plan
        .scene_resource_versions()
        .unwrap()
        .into_iter()
        .collect();
    witness.verify_scene_resources(&versions).unwrap();
    assert_eq!(
        owner.serialize_project_file(0, pin.snapshot_id()).unwrap(),
        before
    );
}
#[test]
fn imported_custom_tip_keeps_authored_erased_png_instead_of_regenerating_a_builtin_mask() {
    let mut drawing = BitmapBrushFields::test_drawing();
    drawing["objects"][0]["vectorExt"]["bitmapBrush"]["bitmapBrushSpec"]["tip"] =
        json!("abr:opaque stored mask");
    let owner = owner(drawing);
    let pin = owner.acquire_snapshot(0).unwrap();
    let mut services = runtime();
    let s = scene(&pin, &mut services, None);
    assert_eq!(bitmap(&s).rgba8(), AUTHORED_PIXELS);
    let saved = owner.serialize_project_file(0, pin.snapshot_id()).unwrap();
    let fresh = NativeObjectHistory::from_project_file("reopened-brush", &saved).unwrap();
    let reopened = fresh.acquire_snapshot(0).unwrap();
    let mut fresh_runtime = runtime();
    let next = scene(&reopened, &mut fresh_runtime, None);
    assert_eq!(bitmap(&s).rgba8(), bitmap(&next).rgba8());
    assert_eq!(
        fresh
            .serialize_project_file(0, reopened.snapshot_id())
            .unwrap(),
        saved
    );
}
#[test]
fn real_trim_holder_reaches_normal_frame_but_enabled_expression_and_foreign_holder_are_refused() {
    let original = owner(trimmed());
    let pin = original.acquire_snapshot(0).unwrap();
    let mut services = runtime();
    let s = scene(&pin, &mut services, None);
    assert!(bitmap(&s).rgba8().chunks_exact(4).any(|p| p[3] != 0));
    assert_ne!(bitmap(&s).dimensions(), (2, 2));
    for kind in ["expression", "foreign"] {
        let mut drawing = trimmed();
        if kind == "expression" {
            drawing["propertyHolders"][0]["properties"][0]["expression"] =
                json!({"enabled":true,"code":"50"});
        } else {
            let mut plain = drawing["objects"][0].clone();
            plain["target"]["strokeId"] = json!("plain");
            plain.as_object_mut().unwrap().remove("vectorExt");
            drawing["objects"].as_array_mut().unwrap().push(plain);
            drawing["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
                .as_array_mut()
                .unwrap()
                .push(json!({"layerUid":"L","strokeId":"plain","naturalPosition":2,"order":{}}));
            drawing["propertyHolders"][0]["target"]["strokeId"] = json!("plain");
        }
        let bad = owner(drawing);
        let snapshot = bad.acquire_snapshot(0).unwrap();
        assert_eq!(
            prepare_object_frame(&snapshot, &selector(&snapshot)).unwrap_err(),
            crate::object_frame_packet::ObjectFrameError::Unavailable,
            "{kind}"
        );
    }
}
#[test]
fn ambiguous_or_missing_companion_and_tiny_quota_refuse_without_owner_publication() {
    for kind in ["missing", "ambiguous"] {
        let mut drawing = BitmapBrushFields::test_drawing();
        if kind == "missing" {
            drawing["items"]["raster"][0]["item"]["brushGroupId"] = json!("other");
        } else {
            let mut item = drawing["items"]["raster"][0].clone();
            item["target"]["strokeId"] = json!("companion2");
            drawing["items"]["raster"]
                .as_array_mut()
                .unwrap()
                .push(item);
            drawing["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
                .as_array_mut()
                .unwrap()
                .push(
                    json!({"layerUid":"L","strokeId":"companion2","naturalPosition":2,"order":{}}),
                );
        }
        let owner = owner(drawing);
        let pin = owner.acquire_snapshot(0).unwrap();
        let before = owner.serialize_project_file(0, pin.snapshot_id()).unwrap();
        let packet = prepare_object_frame(&pin, &selector(&pin)).unwrap();
        let mut services = runtime();
        assert!(prepare_object_render_scene_with_brush(
            &packet,
            &selector(&pin),
            &mut services,
            None,
            limits(),
            None
        )
        .is_err());
        assert_eq!(services.resident_count(), 0);
        assert_eq!(owner.content_revision(), 0);
        assert_eq!(
            owner.serialize_project_file(0, pin.snapshot_id()).unwrap(),
            before
        );
    }
    let owner = owner(BitmapBrushFields::test_drawing());
    let pin = owner.acquire_snapshot(0).unwrap();
    let packet = prepare_object_frame(&pin, &selector(&pin)).unwrap();
    let budget = ImageResourceBudget::isolated(1).unwrap();
    let mut services = BrushRuntime::for_controls(BrushBitmapStore::with_budget(budget.clone()));
    assert!(prepare_object_render_scene_with_brush(
        &packet,
        &selector(&pin),
        &mut services,
        None,
        limits(),
        None
    )
    .is_err());
    assert_eq!(budget.used().unwrap(), 0);
    assert_eq!(owner.content_revision(), 0);
}
#[cfg(feature = "application")]
#[test]
fn real_metal_normal_triangle_png_order_and_scheduler_leases_survive_runtime_and_owner_drop() {
    use crate::compositor::Compositor;
    use crate::object_render_scene::prepare_scheduled_object_composition_from_witness;
    use crate::scheduler::{
        EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
    };
    let owner = owner(BitmapBrushFields::test_drawing());
    let pin = owner.acquire_snapshot(0).unwrap();
    let mut services = runtime();
    let s = scene(&pin, &mut services, None);
    let plan = prepare_object_paint_plan(&s, &selector(&pin)).unwrap();
    let witness = prepare_object_composition(&plan, &selector(&pin)).unwrap();
    let versions = plan.scene_resource_versions().unwrap();
    assert_eq!(versions.len(), 1);
    let pins = ObjectSchedulePins {
        instance_id: pin.instance_id().into(),
        document_id: pin.document_id().into(),
        content_revision: 0,
        document_snapshot_id: pin.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame: 0,
        scope: ObjectScheduleScope::Authored,
    };
    let key = EvaluationKey::new(
        pin.snapshot_id(),
        "scene-root",
        0,
        "final",
        OutputSpec::new("frame", "rgba8", 48, 48, "srgb", "straight").unwrap(),
        versions,
    )
    .unwrap();
    let mut scheduler = FrameScheduler::new();
    let work = scheduler.schedule_object(pin.clone(), pins, key).unwrap();
    drop(services);
    drop(s);
    drop(owner);
    drop(pin);
    let mut gpu = Compositor::new().expect("normal brush GPU acceptance requires real Metal");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    {
        let live =
            prepare_scheduled_object_composition_from_witness(&scheduler, &work, &witness).unwrap();
        let output = gpu.compose_scheduled_object(&live).unwrap();
        let rgba = gpu.readback_scheduled_object_rgba8(&output, &live).unwrap();
        let bytes = rgba.bytes(&live).unwrap();
        let pixel = |x: usize, y: usize| -> [u8; 4] {
            bytes[(y * 48 + x) * 4..(y * 48 + x + 1) * 4]
                .try_into()
                .unwrap()
        };
        assert_eq!(pixel(8, 8), [0, 255, 0, 255], "real anchor triangle fill");
        assert_eq!(
            pixel(13, 17),
            [0, 0, 255, 255],
            "exactly one companion above actual anchor"
        );
        assert_eq!(
            pixel(18, 13),
            [0, 255, 0, 255],
            "authored erased hole reveals actual triangle fill"
        );
        assert_eq!(
            pixel(30, 8),
            [0, 255, 0, 255],
            "triangle interior outside companion"
        );
        assert_eq!(
            pixel(40, 40),
            [255; 4],
            "outside triangle: no square substitute"
        );
        let png = crate::png_output::encode_scheduled_object_png(&mut gpu, &output, &live, 1 << 20)
            .unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(png.bytes(&live).unwrap()))
            .read_info()
            .unwrap();
        let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut decoded).unwrap();
        assert_eq!(
            &decoded[..info.buffer_size()],
            bytes,
            "normal export encoding preserves actual Metal pixels"
        );
    }
    scheduler.succeed(work.work_id()).unwrap();
    let leases = scheduler.lease_counters();
    assert_eq!(leases.live(), 0);
    assert_eq!(leases.acquired(), leases.released());
}
