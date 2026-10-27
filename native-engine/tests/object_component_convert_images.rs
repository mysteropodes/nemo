//! Converting layers that hold a plain image (`component-convert-items`): the image moves into the
//! component and keeps its stack position, so the converted frame paints as before.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{FrameScopeKind, ProjectEnvelope, ProjectMetadata};
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::component_image_tests::inventory;
use crate::object_render_scene::{
    ordered_entries, prepare_object_paint_plan, prepare_object_render_scene, ObjectPaintPlan,
    PaintEntry,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const CONVERT: &str = "command.document.object.component.convert";

fn timing() -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]})
}
fn target(layer: &str, stroke: &str) -> Value {
    json!({"contextId":"scene-root","layerUid":layer,"strokeId":stroke,"frameScope":{"kind":"authored","frame":0}})
}
fn contour(layer: &str, stroke: &str, rect: [i32; 4], rgb: [u8; 3]) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut r = cases["records"][0].clone();
    r["target"] = target(layer, stroke);
    let [x0, y0, x1, y1] = rect;
    r["geometry"]["segments"] =
        json!([[x0,y0],[x1,y0],[x1,y1],[x0,y1]].map(|[x,y]|
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    let [red, green, blue] = rgb.map(|c| f64::from(c) / 255.0);
    r["fill"] = json!({"kind":"solid","r":red,"g":green,"b":blue,"a":1});
    r
}
/// `ly` holds a green image UNDER a red square (scene order: image 0, square 1); `ly2`, above it,
/// holds a blue square.
fn drawing() -> Value {
    let element = |layer: &str, stroke: &str, at: u32| json!({"layerUid":layer,"strokeId":stroke,"naturalPosition":at,"order":{}});
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":4,
        "layers":[{"layerUid":"ly2","name":"Top","timing":timing()},{"layerUid":"ly","name":"Hand","timing":timing()}],
        "objects":[contour("ly","ink",[14,14,22,22],[255,0,0]),contour("ly2","sky",[30,30,40,40],[0,0,255])],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{
            "contextId":"scene-root","layers":[{"layerUid":"ly","naturalPosition":0,"order":{}},
                {"layerUid":"ly2","naturalPosition":1,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":0},
                "elements":[element("ly","img",0),element("ly","ink",1),element("ly2","sky",0)]}]}]},
        "items":{"raster":[{"target":target("ly","img"),
            "item":{"assetId":"img-g","x":16,"y":16,"width":12,"height":12,"opacity":1}}]}})
}
fn owner() -> NativeObjectHistory {
    let document = decode_storage_project(drawing().to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(64, 64, 24.).unwrap(), document)
        .with_asset_inventory(Some(inventory()));
    NativeObjectHistory::from_admitted_project("convert-image", project).unwrap()
}
fn stored(owner: &NativeObjectHistory) -> Value {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    serde_json::from_slice(&encode_storage_project(snapshot.document()).unwrap()).unwrap()
}
fn convert(owner: &mut NativeObjectHistory, layers: &[&str]) -> Value {
    let request = json!({"apiVersion":2,"requestId":format!("convert-{}", layers.join("-")),
        "instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "expectedRevision":owner.content_revision(),"operation":CONVERT,
        "payload":{"command":"object.component.convert","contextId":"scene-root","layerUids":layers}});
    serde_json::to_value(
        owner
            .try_dispatch_object_command(&serde_json::from_value(request).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
fn selector(snapshot: &ObjectSnapshot) -> ObjectFrameSelector<'_> {
    selector_at(snapshot, 0)
}
fn selector_at(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
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
fn plan(owner: &NativeObjectHistory) -> ObjectPaintPlan {
    plan_at(owner, 0)
}
fn plan_at(owner: &NativeObjectHistory, frame: u32) -> ObjectPaintPlan {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let pins = selector_at(&snapshot, frame);
    let packet = prepare_object_exposure_frame(&snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    prepare_object_paint_plan(&scene, &pins).unwrap()
}
/// Paint order: `I` an image (`i` when placed by a component), `C` a contour (`c` placed).
fn order(plan: &ObjectPaintPlan) -> String {
    ordered_entries(plan)
        .map(|entry| match entry {
            PaintEntry::Raster(at) if plan.raster_sources()[at].placed => 'i',
            PaintEntry::Raster(_) => 'I',
            PaintEntry::Contour(_) => 'C',
            PaintEntry::Component(_) => 'c',
            PaintEntry::Text(text) if text.placed => 't',
            PaintEntry::Text(_) => 'T',
            _ => '?',
        })
        .collect()
}

#[test]
fn a_converted_image_moves_into_the_component_and_keeps_its_stack_position() {
    let mut owner = owner();
    let original = stored(&owner);
    assert_eq!(order(&plan(&owner)), "ICC");
    let reply = convert(&mut owner, &["ly"]);
    assert_eq!(reply["ok"], true, "{reply}");
    let after = stored(&owner);
    let symbol = after["componentTables"]["components"]["components"][0]["symbolId"]
        .as_str()
        .unwrap()
        .to_owned();
    let inner = format!("{symbol}:layer-1");
    assert_eq!(
        after["items"]["raster"][0]["target"],
        json!({"contextId":symbol,"layerUid":inner,"strokeId":"img","frameScope":{"kind":"authored","frame":0}})
    );
    let contexts = after["compositionOrder"]["contexts"].as_array().unwrap();
    assert_eq!(contexts.len(), 2);
    assert_eq!(
        contexts[1],
        json!({"contextId":symbol,"layers":[{"layerUid":inner,"naturalPosition":0,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[
                {"layerUid":inner,"strokeId":"img","naturalPosition":0,"order":{}},
                {"layerUid":inner,"strokeId":"ink","naturalPosition":1,"order":{}}]}]})
    );
    assert!(
        contexts[0]["scopes"][0]["elements"]
            .as_array()
            .unwrap()
            .iter()
            .all(|element| element["layerUid"] == "ly2"),
        "the scene keeps only the other layer's elements"
    );
    assert_eq!(
        order(&plan(&owner)),
        "icC",
        "the image still paints under its square"
    );
    let rect = plan(&owner).raster_sources()[0].source.rect;
    assert_eq!(
        [rect.x, rect.y, rect.width, rect.height, rect.rotation],
        [10., 10., 12., 12., 0.]
    );
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(stored(&owner), original);
}

#[test]
fn a_merge_carries_each_layers_images_and_order_into_one_component() {
    let mut owner = owner();
    let reply = convert(&mut owner, &["ly", "ly2"]);
    assert_eq!(reply["ok"], true, "{reply}");
    let after = stored(&owner);
    let symbol = after["componentTables"]["components"]["components"][0]["symbolId"].clone();
    assert_eq!(after["items"]["raster"][0]["target"]["contextId"], symbol);
    assert_eq!(after["items"]["raster"][0]["target"]["layerUid"], "ly");
    let context = &after["compositionOrder"]["contexts"][1];
    assert_eq!(context["contextId"], symbol);
    assert_eq!(
        context["layers"],
        json!([{"layerUid":"ly2","naturalPosition":0,"order":{}},{"layerUid":"ly","naturalPosition":1,"order":{}}])
    );
    assert_eq!(order(&plan(&owner)).len(), 3);
}

#[test]
fn a_converted_image_survives_save_and_reopen() {
    let mut owner = owner();
    assert_eq!(convert(&mut owner, &["ly"])["ok"], true);
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let bytes = owner
        .serialize_project_file(owner.content_revision(), snapshot.snapshot_id())
        .unwrap();
    let reopened = NativeObjectHistory::from_project_file("reopened", &bytes).unwrap();
    assert_eq!(order(&plan(&reopened)), "icC");
}

#[test]
fn an_image_on_a_later_key_moves_with_its_own_frame_scope() {
    let mut doc = drawing();
    doc["layers"][1]["timing"]["keys"] = json!([{"frame":0},{"frame":2}]);
    let mut later = doc["objects"][0].clone();
    later["target"]["frameScope"]["frame"] = json!(2);
    doc["objects"].as_array_mut().unwrap().push(later);
    doc["items"]["raster"][0]["target"]["frameScope"]["frame"] = json!(2);
    let element = |stroke: &str, at: u32| json!({"layerUid":"ly","strokeId":stroke,"naturalPosition":at,"order":{}});
    let scopes = &mut doc["compositionOrder"]["contexts"][0]["scopes"];
    scopes[0]["elements"] = json!([element("ink", 0), {"layerUid":"ly2","strokeId":"sky","naturalPosition":0,"order":{}}]);
    scopes
        .as_array_mut()
        .unwrap()
        .push(json!({"frameScope":{"kind":"authored","frame":2},
        "elements":[element("img", 0), element("ink", 1)]}));
    let document = decode_storage_project(doc.to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(64, 64, 24.).unwrap(), document)
        .with_asset_inventory(Some(inventory()));
    let mut owner = NativeObjectHistory::from_admitted_project("later", project).unwrap();
    assert_eq!(
        (order(&plan_at(&owner, 0)), order(&plan_at(&owner, 2))),
        ("CC".into(), "ICC".into())
    );
    assert_eq!(convert(&mut owner, &["ly", "ly2"])["ok"], true);
    let after = stored(&owner);
    let scopes = &after["compositionOrder"]["contexts"][1]["scopes"];
    assert_eq!(scopes.as_array().unwrap().len(), 2, "{scopes}");
    assert_eq!(scopes[1]["frameScope"]["frame"], 2);
    // A merged component paints its layers in source index order (`natural_component`, the
    // baseline's `sym.layers` concatenation), so `ly2`'s square now comes first; inside `ly` the
    // image stays under its square, on its own key only.
    assert_eq!(
        (order(&plan_at(&owner, 0)), order(&plan_at(&owner, 2))),
        ("cc".into(), "cic".into())
    );
}

#[test]
fn text_brush_and_mesh_bound_items_still_refuse_the_conversion() {
    for (name, mutate) in [
        ("brush", json!({"brushGroupId":"g1"})),
        ("mesh", json!({"meshId":"m1"})),
    ] {
        let mut doc = drawing();
        for (key, value) in mutate.as_object().unwrap() {
            doc["items"]["raster"][0]["item"][key] = value.clone();
        }
        if name == "mesh" {
            doc["imageMeshes"] = json!({"m1":{
                "outline":[[0,0],[1,0],[1,1],[0,1]],"verts":[[0,0],[1,0],[1,1],[0,1]],
                "tris":[0,1,2,0,2,3],"offsets":[[0,0],[0,0],[0,0],[0,0]],"cols":1,"rows":1}});
        }
        let document = decode_storage_project(doc.to_string().as_bytes())
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let project =
            ProjectEnvelope::no_media(ProjectMetadata::new(64, 64, 24.).unwrap(), document)
                .with_asset_inventory(Some(inventory()));
        let mut owner = NativeObjectHistory::from_admitted_project(name, project)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let reply = convert(&mut owner, &["ly"]);
        assert_eq!(reply["error"]["code"], "unavailable", "{name}: {reply}");
        assert_eq!(
            reply["error"]["message"],
            "component-convert-items: animated text, brush and mesh-bound items cannot enter a component yet.",
            "{name}"
        );
        assert_eq!(owner.content_revision(), 0, "{name}");
    }
}

#[test]
#[ignore = "requires the explicitly coordinated real Metal/GPU validation slot"]
fn gpu_converting_a_layer_with_an_image_leaves_the_frame_pixel_identical() {
    use crate::compositor::Compositor;
    use crate::object_render_scene::{
        object_raster_resource_versions, prepare_object_composition,
        prepare_scheduled_object_composition_from_witness,
    };
    use crate::scheduler::{
        EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
    };
    let mut gpu = Compositor::new().expect("real Metal required");
    let mut render = |owner: &NativeObjectHistory| {
        let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
        let plan = plan(owner);
        let witness = prepare_object_composition(&plan, &selector(&snapshot)).unwrap();
        let output = OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap();
        let key = EvaluationKey::new(
            snapshot.snapshot_id(),
            "scene-root",
            0,
            "final",
            output,
            object_raster_resource_versions(&plan).unwrap(),
        )
        .unwrap();
        let pins = ObjectSchedulePins {
            instance_id: snapshot.instance_id().into(),
            document_id: snapshot.document_id().into(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id().into(),
            context_id: "scene-root".into(),
            frame: 0,
            scope: ObjectScheduleScope::Authored,
        };
        let mut scheduler = FrameScheduler::new();
        let work = scheduler.schedule_object(snapshot, pins, key).unwrap();
        let live =
            prepare_scheduled_object_composition_from_witness(&scheduler, &work, &witness).unwrap();
        let result = gpu.compose_scheduled_object(&live).unwrap();
        gpu.readback_scheduled_object_rgba8(&result, &live)
            .unwrap()
            .bytes(&live)
            .unwrap()
            .to_vec()
    };
    let mut owner = owner();
    let before = render(&owner);
    let at =
        |pixels: &[u8], x: usize, y: usize| pixels[(y * 64 + x) * 4..(y * 64 + x + 1) * 4].to_vec();
    assert_eq!(
        at(&before, 18, 18),
        [255, 0, 0, 255],
        "the square over the image"
    );
    assert_eq!(at(&before, 11, 11), [0, 200, 0, 255], "the image around it");
    for layers in [vec!["ly"], vec!["ly", "ly2"]] {
        let mut converted = self::owner();
        assert_eq!(convert(&mut converted, &layers)["ok"], true);
        assert!(render(&converted) == before, "{layers:?} changed the frame");
    }
    assert_eq!(convert(&mut owner, &["ly"])["ok"], true);
}

#[test]
#[cfg(feature = "font_asset_admission")]
fn a_converted_text_block_moves_into_the_component_with_its_glyphs_and_stack_position() {
    let mut doc = drawing();
    doc["items"]["text"] = json!([{"target":target("ly","words"),"block":{"text":"Hi",
        "font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":24,"color":"#112233",
        "align":"left","anchorTopLeft":{"x":4,"y":4}}}]);
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .insert(
            2,
            json!({"layerUid":"ly","strokeId":"words","naturalPosition":2,"order":{}}),
        );
    let document = decode_storage_project(doc.to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(64, 64, 24.).unwrap(), document)
        .with_asset_inventory(Some(inventory()));
    let mut owner = NativeObjectHistory::from_admitted_project("text", project).unwrap();
    let glyphs = |plan: &ObjectPaintPlan| -> Vec<Vec<[f64; 2]>> {
        ordered_entries(plan)
            .filter_map(|entry| match entry {
                PaintEntry::Text(text) => Some(
                    text.item
                        .painted()
                        .map(|(c, _)| c.contour.segments()[0].point)
                        .collect(),
                ),
                _ => None,
            })
            .collect()
    };
    let before = plan(&owner);
    assert_eq!(order(&before), "ICTC");
    assert_eq!(convert(&mut owner, &["ly"])["ok"], true);
    let after_doc = stored(&owner);
    assert_eq!(
        after_doc["items"]["text"][0]["target"]["contextId"],
        after_doc["componentTables"]["components"]["components"][0]["symbolId"]
    );
    let after = plan(&owner);
    assert_eq!(
        order(&after),
        "ictC",
        "the text keeps its place above its square"
    );
    assert_eq!(
        glyphs(&after),
        glyphs(&before),
        "no matrix: the same glyph paths"
    );
}
