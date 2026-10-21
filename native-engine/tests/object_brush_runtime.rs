//! Prospective real native normal-frame/resource tests; no synthetic scene.
use super::*;
use crate::compositor::Compositor;
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::BrushBitmapStore;
use crate::history::NativeObjectHistory;
use crate::object_document::{BitmapBrushFields, FrameScopeKind};
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{prepare_object_composition, ObjectCompositionWitness};
use crate::revision::{ImageResourceBudget, ObjectSnapshot};
use serde_json::{json, Value};

impl BitmapBrushFields {
    pub(crate) fn runtime_test_project(trim: bool) -> Value {
        let mut drawing = Self::test_drawing();
        drawing["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,
            "family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
        if trim {
            drawing["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L","strokeId":"anchor"},
                "properties":[{"property":"trimEnd","static":[50]}]}]);
        }
        json!({"format":"nemo.native-project-document","formatVersion":2,
            "project":{"canvasW":64,"canvasH":64,"fps":24},"drawing":drawing,"viewportReference":null,
            "assets":[{"id":"ink","name":"Authored brush","kind":"image","source":{"kind":"embedded",
                "mediaType":"image/png","encoding":"base64","data":"iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADUlEQVR4nGNgYPgPBgAU9AX7uVYDfQAAAABJRU5ErkJggg=="}}],
            "assetInventory":{"assetIds":["ink"]}})
    }
    pub(crate) fn runtime_test_owner(trim: bool) -> NativeObjectHistory {
        NativeObjectHistory::from_project_file(
            "runtime-brush",
            Self::runtime_test_project(trim).to_string().as_bytes(),
        )
        .unwrap()
    }
}
impl ObjectBrushRuntime {
    pub(crate) fn test_render(gpu: &mut Compositor, witness: &ObjectCompositionWitness) -> Vec<u8> {
        use crate::object_render_scene::prepare_scheduled_object_composition_from_witness;
        use crate::scheduler::{
            EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
        };
        let packet = witness.plan().packet();
        let snapshot = packet.snapshot();
        let pins = ObjectSchedulePins {
            instance_id: snapshot.instance_id().into(),
            document_id: snapshot.document_id().into(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id().into(),
            context_id: packet.context_id().into(),
            frame: packet.frame(),
            scope: ObjectScheduleScope::Authored,
        };
        let key = EvaluationKey::new(
            snapshot.snapshot_id(),
            packet.context_id(),
            packet.frame(),
            "final",
            OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap(),
            witness.plan().scene_resource_versions().unwrap(),
        )
        .unwrap();
        let mut scheduler = FrameScheduler::new();
        let work = scheduler
            .schedule_object(snapshot.clone(), pins, key)
            .unwrap();
        assert!(
            scheduler.lease_counters().live() > 0,
            "actual scheduled snapshot/resources are leased"
        );
        let pixels = {
            let live =
                prepare_scheduled_object_composition_from_witness(&scheduler, &work, witness)
                    .unwrap();
            let output = gpu.compose_scheduled_object(&live).unwrap();
            gpu.readback_scheduled_object_rgba8(&output, &live)
                .unwrap()
                .bytes(&live)
                .unwrap()
                .to_vec()
        };
        scheduler.succeed(work.work_id()).unwrap();
        let leases = scheduler.lease_counters();
        assert_eq!(leases.live(), 0);
        assert_eq!(leases.acquired(), leases.released());
        pixels
    }
}
fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
fn admission(snapshot: &ObjectSnapshot, frame: u32) -> ObjectBrushAdmission {
    let expected = selector(snapshot, frame);
    let packet = prepare_object_exposure_frame(snapshot, &expected).unwrap();
    ObjectBrushAdmission::prepare(&packet, &expected).unwrap()
}
fn metal() -> Compositor {
    let gpu = Compositor::new().expect("brush runtime acceptance requires a real Metal adapter");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    gpu
}
fn witness(
    runtime: &mut ObjectBrushRuntime,
    admission: &ObjectBrushAdmission,
    gpu: &mut Compositor,
) -> ObjectCompositionWitness {
    let plan = runtime
        .materialize(admission, &mut |ids| {
            gpu.retire_object_brush_resources(ids, (64, 64)).is_ok()
        })
        .unwrap();
    prepare_object_composition(
        &plan,
        &selector(admission.packet().snapshot(), admission.packet().frame()),
    )
    .unwrap()
}
fn pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * 64 + x) * 4..(y * 64 + x + 1) * 4]
        .try_into()
        .unwrap()
}
#[test]
fn cpu_admission_retains_real_packet_and_does_not_allocate_brush_pixels() {
    let owner = BitmapBrushFields::runtime_test_owner(false);
    let pin = owner.acquire_snapshot(0).unwrap();
    let before = owner.serialize_project_file(0, pin.snapshot_id()).unwrap();
    let budget = ImageResourceBudget::isolated(1 << 20).unwrap();
    let runtime = ObjectBrushRuntime::with_store(BrushBitmapStore::with_budget(budget.clone()));
    let admitted = admission(&pin, 7);
    assert_eq!(admitted.packet().frame(), 7);
    assert_eq!(admitted.packet().records().len(), 1);
    assert_eq!(
        admitted
            .packet()
            .records()
            .next()
            .unwrap()
            .record()
            .target()
            .frame(),
        0
    );
    assert_eq!(admitted.packet().raster_records().count(), 1);
    assert_eq!(
        budget.used().unwrap(),
        0,
        "CPU admission cannot decode or bake before renderer retirement"
    );
    assert_eq!(
        owner.serialize_project_file(0, pin.snapshot_id()).unwrap(),
        before
    );
    drop(runtime);
    assert_eq!(budget.used().unwrap(), 0);
}
#[test]
fn cpu_foreign_pins_and_mismatched_companion_refuse_before_materializer() {
    let owner = BitmapBrushFields::runtime_test_owner(false);
    let pin = owner.acquire_snapshot(0).unwrap();
    let expected = selector(&pin, 0);
    let packet = prepare_object_exposure_frame(&pin, &expected).unwrap();
    let mut bad = expected.clone();
    bad.document_snapshot_id = "foreign";
    assert!(ObjectBrushAdmission::prepare(&packet, &bad).is_err());
    let mut wire = BitmapBrushFields::runtime_test_project(false);
    wire["drawing"]["items"]["raster"][0]["item"]["brushGroupId"] = json!("not the anchor group");
    let owner =
        NativeObjectHistory::from_project_file("bad-brush", wire.to_string().as_bytes()).unwrap();
    let pin = owner.acquire_snapshot(0).unwrap();
    let expected = selector(&pin, 0);
    let packet = prepare_object_exposure_frame(&pin, &expected).unwrap();
    assert!(ObjectBrushAdmission::prepare(&packet, &expected).is_err());
    assert_eq!(owner.content_revision(), 0);
}
#[test]
fn gpu_normal_materialization_reuses_versions_and_keeps_real_triangle_and_png_pixels() {
    let owner = BitmapBrushFields::runtime_test_owner(false);
    let pin = owner.acquire_snapshot(0).unwrap();
    let saved = owner.serialize_project_file(0, pin.snapshot_id()).unwrap();
    let a = admission(&pin, 0);
    let mut gpu = metal();
    let device = gpu.device().clone();
    let mut runtime = ObjectBrushRuntime::default();
    let first = witness(&mut runtime, &a, &mut gpu);
    let pixels = ObjectBrushRuntime::test_render(&mut gpu, &first);
    assert_eq!(
        pixel(&pixels, 8, 8),
        [0, 255, 0, 255],
        "actual document triangle fill"
    );
    assert_eq!(
        pixel(&pixels, 13, 15),
        [0, 0, 255, 255],
        "authored companion paints above real triangle"
    );
    assert_eq!(
        pixel(&pixels, 40, 40),
        [255; 4],
        "outside original triangle, not a square substitute"
    );
    // A one-byte cap cannot hold a header. Unsupported proves the bitmap
    // refusal runs before header sizing/allocation, never a contour-only SVG.
    use crate::export_job::native_vector_export::native_vector_export_svg::{
        serialize_svg_frame, SvgExportError, SvgFrameSpec,
    };
    assert_eq!(
        serialize_svg_frame(first.plan(), SvgFrameSpec::new(64, 64, 1).unwrap()),
        Err(SvgExportError::Unsupported {
            source_object_index: 1,
            reason: "retained bitmap brush paint has no SVG representation"
        })
    );
    let first_image = first.brush_images().next().unwrap().data.id();
    let next = witness(&mut runtime, &a, &mut gpu);
    assert_eq!(
        first_image,
        next.brush_images().next().unwrap().data.id(),
        "cache hit keeps actual ImageData identity"
    );
    assert_eq!(
        first.plan().scene_resource_versions().unwrap(),
        next.plan().scene_resource_versions().unwrap()
    );
    assert_eq!(pixels, ObjectBrushRuntime::test_render(&mut gpu, &next));
    let held = witness(&mut runtime, &admission(&pin, 7), &mut gpu);
    assert_eq!(
        first_image,
        held.brush_images().next().unwrap().data.id(),
        "different requested frame retains same held brush allocation"
    );
    assert_eq!(pixels, ObjectBrushRuntime::test_render(&mut gpu, &held));
    assert_eq!(gpu.device(), &device);
    assert_eq!(
        owner.serialize_project_file(0, pin.snapshot_id()).unwrap(),
        saved
    );
    runtime.release();
    assert!(
        runtime
            .materialize(&a, &mut |ids| gpu
                .retire_object_brush_resources(ids, (64, 64))
                .is_ok())
            .is_err(),
        "released owner cannot start a new build"
    );
    drop(runtime);
    drop(owner);
    drop(pin);
    assert_eq!(
        pixels,
        ObjectBrushRuntime::test_render(&mut gpu, &first),
        "old witness leases survive runtime/owner drop"
    );
}
#[test]
fn gpu_real_trim_edit_changes_declared_version_before_scheduling_and_retains_old_pixels() {
    let mut owner = BitmapBrushFields::runtime_test_owner(true);
    let old = owner.acquire_snapshot(0).unwrap();
    let mut gpu = metal();
    let mut runtime = ObjectBrushRuntime::default();
    let first = witness(&mut runtime, &admission(&old, 0), &mut gpu);
    let pixels = ObjectBrushRuntime::test_render(&mut gpu, &first);
    let old_versions = first.plan().scene_resource_versions().unwrap();
    let edit = json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L","strokeId":"anchor"},
        "property":"trimEnd","action":{"kind":"setValue","frame":0,"values":[25]}});
    let candidate = owner
        .prepare_property_edit_json(edit.to_string().as_bytes())
        .unwrap();
    assert_eq!(owner.commit_property_edit(candidate), Ok(1));
    let current = owner.acquire_snapshot(1).unwrap();
    let second = witness(&mut runtime, &admission(&current, 0), &mut gpu);
    assert_ne!(
        old_versions,
        second.plan().scene_resource_versions().unwrap(),
        "version must be ready before EvaluationKey"
    );
    let changed = ObjectBrushRuntime::test_render(&mut gpu, &second);
    assert_ne!(pixels, changed, "25% and50% of real triangle must differ");
    assert_eq!(
        pixels,
        ObjectBrushRuntime::test_render(&mut gpu, &first),
        "actual renderer restores old atlas pixels"
    );
    assert_eq!(changed, ObjectBrushRuntime::test_render(&mut gpu, &second));
    owner.undo(1).unwrap();
    let undone = owner.acquire_snapshot(2).unwrap();
    let restored = witness(&mut runtime, &admission(&undone, 0), &mut gpu);
    assert_eq!(pixels, ObjectBrushRuntime::test_render(&mut gpu, &restored));
}
#[test]
fn gpu_refused_budget_materialization_cannot_publish_or_mutate_the_native_owner() {
    let owner = BitmapBrushFields::runtime_test_owner(false);
    let pin = owner.acquire_snapshot(0).unwrap();
    let before = owner.serialize_project_file(0, pin.snapshot_id()).unwrap();
    let a = admission(&pin, 0);
    let budget = ImageResourceBudget::isolated(1).unwrap();
    let mut runtime = ObjectBrushRuntime::with_store(BrushBitmapStore::with_budget(budget.clone()));
    let mut gpu = metal();
    let device = gpu.device().clone();
    assert!(runtime
        .materialize(&a, &mut |ids| gpu
            .retire_object_brush_resources(ids, (64, 64))
            .is_ok())
        .is_err());
    assert_eq!(budget.used().unwrap(), 0);
    assert_eq!(gpu.device(), &device);
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(
        owner.serialize_project_file(0, pin.snapshot_id()).unwrap(),
        before
    );
}
