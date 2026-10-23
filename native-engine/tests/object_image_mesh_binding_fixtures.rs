//! Literal and scoped document fixtures for the image-mesh binding history contract.
//! These builders use the existing admitted-asset fixture; publication and assertions stay in the history tests.
use super::support;
use serde_json::{json, Value};

pub(super) fn mesh() -> Value {
    // Literal corpus fixture: eight boundary vertices, one interior vertex and eight triangles.
    json!({"outline":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5]],
        "verts":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5],[0.5,0.5]],
        "tris":[8,0,1,8,1,2,8,2,3,8,3,4,8,4,5,8,5,6,8,6,7,8,7,0],
        "offsets":[[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0]],"cols":2,"rows":2})
}

pub(super) fn target(context: &str, layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"contextId":context,"layerUid":layer,"frameScope":{"kind":"authored","frame":frame},
        "strokeId":stroke})
}

pub(super) fn at(frame: u32) -> Value {
    target("scene-root", "L0", frame, &format!("raster-{frame}"))
}

pub(super) fn inner(frame: u32) -> Value {
    target(
        "component-A",
        "inner-only",
        frame,
        &format!("inner-{frame}"),
    )
}

pub(super) fn entry(target: Value, tag: Option<&str>) -> Value {
    let mut item = support::raster_item("img-1", 3, 5, 200, 100);
    if let Some(id) = tag {
        item["meshId"] = json!(id);
    }
    json!({"target":target,"item":item})
}

pub(super) fn push_raster(doc: &mut Value, target: Value, tag: Option<&str>) {
    doc["items"]["raster"]
        .as_array_mut()
        .unwrap()
        .push(entry(target, tag));
}

pub(super) fn fixture_path(context: &str, layer: &str, frame: u32, stroke: &str) -> Value {
    let mut path = support::legacy_record(layer, stroke);
    path["target"] = target(context, layer, frame, stroke);
    path
}

pub(super) fn text(target: Value) -> Value {
    json!({"target":target,"block":{"text":"Hi",
        "font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,
        "color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}}})
}

pub(super) fn drawing(frames: u32, tag: Option<&str>) -> Value {
    let raster: Vec<_> = (0..frames).map(|frame| entry(at(frame), tag)).collect();
    let mut value = support::drawing(json!({"totalFrames":frames,
        "layers":[{"layerUid":"L0"},{"layerUid":"L2"}],"items":{"raster":raster}}));
    if let Some(id) = tag {
        let mut store = serde_json::Map::new();
        store.insert(id.to_string(), mesh());
        value["imageMeshes"] = Value::Object(store);
    }
    value
}

pub(super) fn element(layer: &str, stroke: &str, position: u32) -> Value {
    json!({"layerUid":layer,"strokeId":stroke,"naturalPosition":position,"order":{}})
}

pub(super) fn scope(frame: u32, elements: Vec<Value>) -> Value {
    json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements})
}

pub(super) fn mixed() -> Value {
    let mut value = drawing(4, None);
    value["formatVersion"] = json!(2);
    // Raster-array indices disagree with authored mixed slots; frame 2 has a path at slot 1,
    // and frame 3 has only slot 0. Different stroke IDs across frames must not prevent propagation.
    value["items"]["raster"] = json!([
        entry(at(0), None),
        entry(at(1), None),
        entry(target("scene-root", "L0", 0, "other-0"), None)
    ]);
    value["objects"] = json!([
        (0, "path-0"),
        (2, "path-2-first"),
        (2, "path-2"),
        (3, "path-3")
    ]
    .map(|(frame, stroke)| fixture_path("scene-root", "L0", frame, stroke)));
    value["items"]["text"] = json!([text(target("scene-root", "L0", 1, "text-1"))]);
    let e = |stroke, position| element("L0", stroke, position);
    value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
        "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":[
            {"layerUid":"L0","naturalPosition":0,"order":{}},
            {"layerUid":"L2","naturalPosition":1,"order":{}}],"scopes":[
            scope(0,vec![e("path-0",0),e("raster-0",1),e("other-0",2)]),
            scope(1,vec![e("text-1",0),e("raster-1",1)]),
            scope(2,vec![e("path-2-first",0),e("path-2",1)]),scope(3,vec![e("path-3",0)])]}]});
    value
}

pub(super) fn component_drawing() -> Value {
    let mut value = drawing(2, Some("im_1"));
    value["formatVersion"] = json!(2);
    for frame in 0..3 {
        push_raster(&mut value, inner(frame), Some("im_1"));
    }
    value["componentTables"] = json!({"components":{"format":"nemo.native-component-set",
        "formatVersion":1,"components":[{"symbolId":"component-A","totalFrames":3,
            "layers":[{"layerUid":"inner-only"}],"objects":[]}],"instances":[]}});
    let root_scopes: Vec<_> = (0..2)
        .map(|frame| scope(frame, vec![element("L0", &format!("raster-{frame}"), 0)]))
        .collect();
    let inner_scopes: Vec<_> = (0..3)
        .map(|frame| {
            scope(
                frame,
                vec![element("inner-only", &format!("inner-{frame}"), 0)],
            )
        })
        .collect();
    value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
        "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":[
            {"layerUid":"L0","naturalPosition":0,"order":{}},{"layerUid":"L2","naturalPosition":1,"order":{}}],
            "scopes":root_scopes},{"contextId":"component-A","layers":[
            {"layerUid":"inner-only","naturalPosition":0,"order":{}}],"scopes":inner_scopes}]});
    value
}

pub(super) fn component_mixed() -> Value {
    let mut value = component_drawing();
    let other = target("component-A", "inner-only", 2, "other-inner");
    push_raster(&mut value, other.clone(), Some("im_1"));
    value["componentTables"]["components"]["components"][0]["objects"] =
        json!([0, 2].map(|frame| fixture_path(
            "component-A",
            "inner-only",
            frame,
            &format!("inner-path-{frame}")
        )));
    value["items"]["text"] = json!([text(target("component-A", "inner-only", 1, "inner-text"))]);
    let e = |stroke, position| element("inner-only", stroke, position);
    value["compositionOrder"]["contexts"][1]["scopes"] = json!([
        scope(0, vec![e("inner-path-0", 0), e("inner-0", 1)]),
        scope(1, vec![e("inner-1", 0), e("inner-text", 1)]),
        scope(
            2,
            vec![e("inner-path-2", 0), e("inner-2", 1), e("other-inner", 2)]
        )
    ]);
    value
}

pub(super) fn vertex_holder(layer: &str) -> Value {
    json!({"target":{"contextId":"scene-root","layerUid":layer,"strokeId":"im_1"},
        "properties":[{"property":"vtx8","static":[0.1,0.2],"track":{"keys":[
            {"frame":0,"v":[0.1,0.2]},{"frame":23,"v":[0.3,0.4]}]}}]})
}

pub(super) fn retained_vertex_holder(layer: &str, id: &str) -> Value {
    let mut holder = vertex_holder(layer);
    holder["target"]["strokeId"] = json!(id);
    holder["meshSource"] = json!({"meshId":id});
    holder
}

pub(super) fn tagged_copy_drawing() -> Value {
    let mut value = drawing(24, Some("im_1"));
    push_raster(
        &mut value,
        target("scene-root", "L2", 0, "copy"),
        Some("im_1"),
    );
    value
}

pub(super) fn duplicated_layer_drawing() -> Value {
    let mut value = drawing(24, Some("im_1"));
    for frame in 0..24 {
        let copy = target("scene-root", "L2", frame, &format!("copy-{frame}"));
        push_raster(&mut value, copy, Some("im_1"));
    }
    value
}

pub(super) fn animated_drawing() -> Value {
    let mut value = drawing(24, Some("im_1"));
    value["propertyHolders"] = json!([vertex_holder("L0")]);
    value
}

pub(super) fn animated_duplicate_drawing() -> Value {
    let mut value = tagged_copy_drawing();
    value["propertyHolders"] = json!([vertex_holder("L0"), vertex_holder("L2")]);
    let row = include_str!("fixtures/object-rig-record/oracle.txt")
        .lines()
        .find(|line| line.starts_with("rig\tmesh_bind_two_bones_rebind_replaces\t"))
        .unwrap();
    let rig: Value = serde_json::from_str(row.split('\t').nth(2).unwrap()).unwrap();
    value["layers"][0]["rig"] = rig.clone();
    value["layers"][1]["rig"] = rig;
    value
}

pub(super) fn component_timed_drawing() -> Value {
    let mut value = component_drawing();
    // Timing records retain the drawing family's local scene-root marker inside components;
    // the containing component and frame selector supply the actual context identity.
    value["componentTables"]["components"]["components"][0]["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
            "contextId":"scene-root","keys":[{"frame":0},{"frame":1},{"frame":2}],"samples":[]});
    value
}

pub(super) fn payload(action: Value) -> Value {
    json!({"command":"object.image.mesh.edit","action":action})
}

pub(super) fn attach(target: Value) -> Value {
    payload(json!({"kind":"attach","target":target,"mesh":mesh()}))
}

pub(super) fn detach(target: Value) -> Value {
    payload(json!({"kind":"detach","target":target}))
}

pub(super) fn duplicate(layer: &str) -> Value {
    payload(json!({"kind":"duplicate","contextId":"scene-root","layerUid":layer}))
}

pub(super) fn implicit_component_drawing() -> Value {
    let mut value = component_timed_drawing();
    value["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap()
        .retain(|context| context["contextId"] != "component-A");
    value["items"]["text"] = json!((0..3)
        .map(|frame| text(target(
            "component-A",
            "inner-only",
            frame,
            &format!("inner-text-{frame}")
        )))
        .collect::<Vec<_>>());
    value
}

pub(super) fn video_actions() -> [(&'static str, Value); 2] {
    [
        (
            "video-attach",
            json!({"kind":"attachVideo","contextId":"scene-root","layerUid":"L0","mesh":mesh()}),
        ),
        (
            "video-detach",
            json!({"kind":"detachVideo","contextId":"scene-root","layerUid":"L0"}),
        ),
    ]
}

pub(super) fn refused_envelopes() -> [(&'static str, &'static str, Value, &'static str); 3] {
    [
        (
            "cancel",
            "cancelledBeforeDispatch",
            json!(true),
            "cancelled_before_dispatch",
        ),
        ("stale", "expectedRevision", json!(99), "stale_revision"),
        ("foreign", "documentId", json!("foreign"), "wrong_document"),
    ]
}

#[cfg(feature = "application")]
pub(super) mod pixel_consumer {
    use crate::compositor::Compositor;
    use crate::export_job::{
        export_one_object_frame, ExportArtifact, JobStatus, ObjectHostExportRequest,
        StagedArtifactPort,
    };
    use crate::history::NativeObjectHistory;
    use crate::object_codec::decode_storage_project;
    use crate::object_document::{
        AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource, FrameScopeKind,
        ProjectEnvelope, ProjectMetadata,
    };
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
    use crate::object_render_scene::ObjectCompositionWitness;
    use crate::object_render_scene::{
        prepare_object_composition, prepare_object_navigated_viewport_composition,
        prepare_object_paint_plan, prepare_object_render_scene,
        prepare_object_viewport_composition, prepare_scheduled_object_composition_from_witness,
    };
    use crate::revision::ObjectSnapshot;
    use crate::scheduler::{
        EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
    };
    use serde_json::{json, Value};
    use std::{io::Cursor, sync::Arc};

    #[derive(Default)]
    struct Png(Vec<u8>);
    impl StagedArtifactPort for Png {
        fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn write_frame(&mut self, _: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
            assert_eq!(name, "frame_0001.png");
            assert!(self.0.is_empty());
            self.0.extend_from_slice(bytes);
            Ok(())
        }
        fn publish(
            &mut self,
            _: &str,
            target: &str,
            files: &[String],
        ) -> Result<ExportArtifact, String> {
            assert_eq!(files, ["frame_0001.png"]);
            assert!(self.0.starts_with(b"\x89PNG\r\n\x1a\n"));
            Ok(ExportArtifact {
                target: target.into(),
                files: files.to_vec(),
            })
        }
        fn cleanup(&mut self, _: &str) -> Result<(), String> {
            Ok(())
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
    pub(crate) fn composition(
        snapshot: &ObjectSnapshot,
        frame: u32,
        viewport: bool,
        zoom: f64,
    ) -> ObjectCompositionWitness {
        let selector = selector(snapshot, frame);
        let packet = prepare_object_frame(snapshot, &selector).unwrap();
        let scene = prepare_object_render_scene(&packet, &selector).unwrap();
        let plan = prepare_object_paint_plan(&scene, &selector).unwrap();
        if viewport && zoom != 1. {
            prepare_object_navigated_viewport_composition(
                &plan,
                &selector,
                None,
                false,
                zoom,
                [0., 0.],
            )
            .unwrap()
        } else if viewport {
            prepare_object_viewport_composition(&plan, &selector, None).unwrap()
        } else {
            prepare_object_composition(&plan, &selector).unwrap()
        }
    }
    pub(crate) fn pixels(gpu: &mut Compositor, witness: &ObjectCompositionWitness) -> Vec<u8> {
        let snapshot = witness.plan().packet().snapshot();
        let mut scheduler = FrameScheduler::new();
        let resources = witness
            .scene_resources()
            .map(|r| r.declared_versions())
            .unwrap_or_default();
        let key = EvaluationKey::new(
            snapshot.snapshot_id(),
            "scene-root",
            witness.frame(),
            witness.expected_quality(),
            OutputSpec::new("frame", "rgba8", 1920, 1080, "srgb", "straight").unwrap(),
            resources,
        )
        .unwrap();
        let work = scheduler
            .schedule_object(
                snapshot.clone(),
                ObjectSchedulePins {
                    instance_id: snapshot.instance_id().into(),
                    document_id: snapshot.document_id().into(),
                    content_revision: snapshot.content_revision(),
                    document_snapshot_id: snapshot.snapshot_id().into(),
                    context_id: "scene-root".into(),
                    frame: witness.frame(),
                    scope: ObjectScheduleScope::Authored,
                },
                key,
            )
            .unwrap();
        let live =
            prepare_scheduled_object_composition_from_witness(&scheduler, &work, witness).unwrap();
        let image = gpu.compose_scheduled_object(&live).unwrap();
        let readback = gpu.readback_scheduled_object_rgba8(&image, &live).unwrap();
        let bytes = readback.bytes(&live).unwrap().to_vec();
        drop(live);
        scheduler.succeed(work.work_id()).unwrap();
        assert_eq!(scheduler.lease_counters().live(), 0);
        bytes
    }
    pub(crate) fn plain_frames(gpu: &mut Compositor, owner: &NativeObjectHistory) -> Vec<Vec<u8>> {
        let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
        [0, 12, 23]
            .map(|frame| pixels(gpu, &composition(&snapshot, frame, true, 1.)))
            .to_vec()
    }

    pub(crate) fn fixture(with_widget: bool) -> (NativeObjectHistory, Value) {
        let baseline: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/mesh/project.json")).unwrap();
        let mut mesh = baseline["imageMeshes"]["im_1"].clone();
        mesh["offsets"][8] = json!([0, 0]);
        let target = |frame| {
            json!({"contextId":"scene-root","layerUid":"L0",
            "frameScope":{"kind":"authored","frame":frame},"strokeId":format!("raster-{frame}")})
        };
        let scopes: Vec<_> = (0..24).map(|frame| json!({"frameScope":{"kind":"authored","frame":frame},
            "elements":[{"layerUid":"L0","strokeId":format!("raster-{frame}"),"naturalPosition":0,"order":{}}]})).collect();
        let mut drawing = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":24,
            "layers":[{"layerUid":"L0"}],"objects":[],"items":{"raster":(0..24).map(|frame|
                json!({"target":target(frame),"item":{"assetId":"img-1","x":320,"y":180,"width":200,"height":200}})).collect::<Vec<_>>()},
            "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
                "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],"scopes":scopes}]}});
        drawing["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
            "properties":[{"property":"position","static":[0,0],"track":{"keys":[
                {"frame":0,"v":[0,0]},{"frame":12,"v":[40,20]},{"frame":23,"v":[80,40]}]}}]}]);
        if with_widget {
            drawing["layers"]
                .as_array_mut()
                .unwrap()
                .push(json!({"layerUid":"W",
                "widget":{"kind":"slider","x":{"key":"k","min":0,"max":1}}}));
            drawing["compositionOrder"]["contexts"][0]["layers"]
                .as_array_mut()
                .unwrap()
                .push(json!({"layerUid":"W","naturalPosition":1,"order":{}}));
        }
        let source = AssetSource::admitted_embedded(
            "image/png".into(),
            Arc::new(include_bytes!("../../tests/fixtures/mesh/assets/image.png").to_vec()),
            1 << 20,
        )
        .unwrap();
        let asset = Arc::new(
            AssetRecord::new(
                AssetId::new("img-1".into()).unwrap(),
                "image.png".into(),
                AssetKind::Image,
                source,
            )
            .unwrap(),
        );
        let inventory = AssetInventory::new(vec![asset], Vec::new(), 16, 0).unwrap();
        let project = ProjectEnvelope::no_media(
            ProjectMetadata::new(1920, 1080, 24.).unwrap(),
            decode_storage_project(drawing.to_string().as_bytes()).unwrap(),
        )
        .with_asset_inventory(Some(inventory));
        let owner = NativeObjectHistory::from_admitted_project("mesh-gpu", project).unwrap();
        (owner, mesh)
    }

    pub(crate) fn exported(
        owner: &NativeObjectHistory,
        gpu: &mut Compositor,
        viewport: &ObjectCompositionWitness,
        frame: u32,
        stage: &str,
    ) -> Vec<u8> {
        let snapshot = viewport.plan().packet().snapshot();
        let mut scheduler = FrameScheduler::new();
        let mut port = Png::default();
        let outcome = export_one_object_frame(
            owner,
            &mut scheduler,
            &mut port,
            gpu,
            ObjectHostExportRequest {
                request_id: format!("mesh-{stage}-{frame}"),
                target: "png".into(),
                pins: ObjectSchedulePins {
                    instance_id: snapshot.instance_id().into(),
                    document_id: snapshot.document_id().into(),
                    content_revision: snapshot.content_revision(),
                    document_snapshot_id: snapshot.snapshot_id().into(),
                    context_id: "scene-root".into(),
                    frame,
                    scope: ObjectScheduleScope::Authored,
                },
                output: OutputSpec::new("frame", "rgba8", 1920, 1080, "srgb", "straight").unwrap(),
                maximum_encoded_bytes: 4 << 20,
            },
            prepare_object_composition(viewport.plan(), &selector(snapshot, frame)).unwrap(),
        );
        assert_eq!(outcome.receipt.unwrap().status, JobStatus::Succeeded);
        assert!(outcome.release.cleanup_complete);
        assert_eq!(scheduler.lease_counters().live(), 0);
        let mut decoder = png::Decoder::new(Cursor::new(port.0)).read_info().unwrap();
        let mut decoded = vec![0; decoder.output_buffer_size().unwrap()];
        let info = decoder.next_frame(&mut decoded).unwrap();
        assert_eq!((info.width, info.height), (1920, 1080));
        decoded.truncate(info.buffer_size());
        decoded
    }
}
