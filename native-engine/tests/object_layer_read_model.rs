//! #2371: every layer-row fact is read from its native record; a fact with no storage is absent, never invented.
use crate::history::object_history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::ObjectDocument;
use super::{layer_members, model_members};
use serde_json::{json, Map, Value};

#[path = "object_component_convert_fixture.rs"]
mod fixture;

fn target(layer: &str, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":layer,"strokeId":stroke})
}
fn raster(layer: &str, stroke: &str, asset: &str) -> Value {
    json!({"target":target(layer, stroke),"item":{"assetId":asset,"x":0,"y":0,"width":8,"height":8,"opacity":1}})
}
fn text(layer: &str, stroke: &str) -> Value {
    let mut entry = fixture::text_item(layer);
    entry["target"] = target(layer, stroke);
    entry
}
fn flat(layers: Value, extra: Value) -> ObjectDocument {
    let mut doc = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4,
        "layers":layers,"objects":[]});
    for (key, member) in extra.as_object().unwrap() {
        doc[key] = member.clone();
    }
    decode_storage_project(doc.to_string().as_bytes()).expect("admissible fixture")
}
fn rows(document: &ObjectDocument) -> Vec<Map<String, Value>> {
    layer_members(document)
}
fn kinds(document: &ObjectDocument) -> Vec<String> {
    rows(document)
        .iter()
        .map(|row| row["kind"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn structural_kinds_follow_the_baseline_order_and_a_plain_layer_is_a_drawing() {
    let document = flat(
        json!([{"layerUid":"plain"},
            {"layerUid":"folder","isFolderLayer":true,"folderCollapsed":true},
            {"layerUid":"open","isFolderLayer":true},
            {"layerUid":"null","isNullLayer":true},
            {"layerUid":"fx","isEffectLayer":true,"effects":[]},
            {"layerUid":"pad","widget":{"kind":"slider","x":{"key":"k","min":0,"max":1}}}]),
        json!({}),
    );
    assert_eq!(
        kinds(&document),
        ["draw", "folder", "folder", "null", "effect", "widget"]
    );
    let rows = rows(&document);
    assert_eq!(rows[1]["folderCollapsed"], json!(true));
    // Collapsed state is reported only where it is stored; absent is not invented as expanded.
    assert!(rows[2].get("folderCollapsed").is_none());
    assert!(rows[0].get("folderCollapsed").is_none());
    for row in &rows {
        for absent in [
            "componentId",
            "blendMode",
            "matteMode",
            "matteSourceLayerUid",
            "mattesMore",
            "isMatteSource",
            "opacity",
            "opacityAnimated",
            "folderId",
        ] {
            assert!(row.get(absent).is_none(), "{absent} is not stored");
        }
    }
}

#[test]
fn raster_and_text_items_classify_footage_text_and_contradiction() {
    let document = flat(
        json!([{"layerUid":"still"},{"layerUid":"seq"},{"layerUid":"words"},{"layerUid":"both"},{"layerUid":"empty"}]),
        json!({"items":{
            "raster":[raster("still","a","img-1"),raster("seq","a","img-1"),raster("seq","b","img-2"),
                raster("both","a","img-1")],
            "text":[text("words","t0"),text("both","t1")]}}),
    );
    assert_eq!(
        kinds(&document),
        ["image", "sequence", "text", "unknown", "draw"]
    );
}

#[test]
fn vector_content_keeps_a_layer_a_drawing_even_beside_a_raster() {
    let layers = [fixture::layer("ly_a", "A", &[(0, "s1")])];
    let mut value = fixture::document(&layers, json!({}));
    value["items"] =
        json!({"raster":[raster("ly_a","r0","img-1")]});
    value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"ly_a","strokeId":"r0","naturalPosition":1,"order":{}}));
    let document = fixture::decode(&value);
    assert_eq!(kinds(&document), ["draw"]);
}

#[test]
fn organisational_folders_and_membership_come_from_the_folder_table() {
    let document = flat(
        json!([{"layerUid":"in","folderId":"g-one"},{"layerUid":"out"},{"layerUid":"legacy","folderId":""}]),
        json!({"layerFolders":{"g-one":{"name":"Folder 同","collapsed":true},"unused":{}}}),
    );
    let rows = rows(&document);
    assert_eq!(rows[0]["folderId"], json!("g-one"));
    assert!(rows[1].get("folderId").is_none());
    // A legacy falsy membership has no active folder.
    assert!(rows[2].get("folderId").is_none());
    let members = model_members(&document);
    assert_eq!(
        members["layerFolders"],
        json!([{"folderId":"g-one","collapsed":true,"name":"Folder 同"},
            {"folderId":"unused","collapsed":false}])
    );
    let none = flat(json!([{"layerUid":"in"}]), json!({}));
    assert!(model_members(&none).get("layerFolders").is_none());
}

#[test]
fn blend_and_matte_facts_are_copied_and_a_source_is_marked() {
    let document = flat(
        json!([{"layerUid":"src"},
            {"layerUid":"top","blendMode":"multiply","matteMode":"alphaInverted","matteSourceLayerUid":"src"},
            {"layerUid":"extra"},
            {"layerUid":"off","matteMode":"none","matteSourceLayerUid":"extra"},
            {"layerUid":"two","matteMode":"luma","matteSourceLayerUid":"src",
                "mattesMore":[{"uid":"extra","mode":"alpha"}]},
            {"layerUid":"normal","blendMode":null}]),
        json!({}),
    );
    let rows = rows(&document);
    assert_eq!(rows[1]["blendMode"], json!("multiply"));
    assert_eq!(rows[1]["matteMode"], json!("alphaInverted"));
    assert_eq!(rows[1]["matteSourceLayerUid"], json!("src"));
    assert_eq!(rows[0]["isMatteSource"], json!(true));
    // `none` is a stored mode but not an active matte, so its source is not marked by that layer.
    assert_eq!(rows[3]["matteMode"], json!("none"));
    assert_eq!(rows[4]["mattesMore"], json!([{"uid":"extra","mode":"alpha"}]));
    assert_eq!(rows[2]["isMatteSource"], json!(true));
    // A stored null blend is not a named blend.
    assert!(rows[5].get("blendMode").is_none());
    assert!(rows[1].get("isMatteSource").is_none());
}

#[test]
fn static_opacity_is_reported_and_an_animated_track_is_flagged() {
    let target = |uid: &str| json!({"contextId":"scene-root","layerUid":uid});
    let layers = [
        fixture::layer("ly_a", "A", &[(0, "s1")]),
        fixture::layer("ly_b", "B", &[(0, "s2")]),
        fixture::layer("ly_c", "C", &[(0, "s3")]),
    ];
    let document = fixture::decode(&fixture::document(
        &layers,
        json!({"propertyHolders":[
            {"target":target("ly_a"),"properties":[{"property":"opacity","static":[50]}]},
            {"target":target("ly_b"),"properties":[{"property":"opacity","static":[100],
                "track":{"keys":[{"frame":0,"v":[100]},{"frame":10,"v":[0]}]}}]},
            {"target":target("ly_c"),"properties":[{"property":"position","static":[1,2]}]}]}),
    ));
    let rows = rows(&document);
    assert_eq!(rows[0]["opacity"], json!(50));
    assert!(rows[0].get("opacityAnimated").is_none());
    assert_eq!(rows[1]["opacity"], json!(100));
    assert_eq!(rows[1]["opacityAnimated"], json!(true));
    // No opacity record: nothing is invented, not even the default of one.
    assert!(rows[2].get("opacity").is_none());
}

#[test]
fn component_instances_report_their_symbol_and_effector_layers_their_kind() {
    let golden = include_str!("fixtures/object-component-document/oracle.txt")
        .lines()
        .find_map(|line| line.strip_prefix("G\t"))
        .expect("golden row");
    let document = decode_storage_project(golden.as_bytes()).expect("golden document");
    let rows = rows(&document);
    let by_uid = |uid: &str| {
        let at = document
            .layers()
            .iter()
            .position(|layer| layer.layer_uid() == uid)
            .unwrap();
        &rows[at]
    };
    assert_eq!(by_uid("ly_comp_loop")["kind"], json!("component"));
    assert_eq!(by_uid("ly_comp_loop")["componentId"], json!("sym_comp"));
    assert_eq!(by_uid("ly_compb_loop")["componentId"], json!("sym_comp_b"));
    assert_eq!(by_uid("ly_effector")["kind"], json!("effector"));
    assert!(by_uid("ly_effector").get("componentId").is_none());
    assert_eq!(by_uid("ly_toggle")["kind"], json!("draw"));
    assert!(by_uid("ly_toggle").get("componentId").is_none());
}

#[test]
fn typed_gaps_name_exactly_what_has_no_native_storage() {
    let document = flat(json!([{"layerUid":"a"}]), json!({}));
    assert_eq!(
        model_members(&document)["layerModelUnavailable"],
        json!(["linkGroup", "kind:camera", "kind:guide", "kind:montage", "kind:video"])
    );
}

#[test]
fn project_info_carries_the_rows_beside_the_existing_members_within_the_ui_bound() {
    let mut drawing = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4,
        "layers":[{"layerUid":"a","name":"A","visible":true,"locked":false,"shy":true,"color":"#5FA875",
            "parentLayerUid":null,"folderId":"f1"},
            {"layerUid":"b","isFolderLayer":true,"folderCollapsed":true,"blendMode":"screen"}],
        "layerFolders":{"f1":{"name":"F","collapsed":false}},"objects":[]});
    drawing["items"] = json!({"text":[text("a","t0")]});
    let file = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},"drawing":drawing,
        "viewportReference":null,"assets":[]});
    let owner = NativeObjectHistory::from_project_file("info", &serde_json::to_vec(&file).unwrap())
        .unwrap();
    let info = owner.project_file_info().unwrap();
    let a = &info["layers"][0];
    // The existing members are untouched.
    assert_eq!(a["name"], json!("A"));
    assert_eq!(a["shy"], json!(true));
    assert_eq!(a["color"], json!("#5FA875"));
    assert_eq!(a["parentLayerUid"], Value::Null);
    // The added ones.
    assert_eq!(a["kind"], json!("text"));
    assert_eq!(a["folderId"], json!("f1"));
    assert_eq!(info["layers"][1]["kind"], json!("folder"));
    assert_eq!(info["layers"][1]["folderCollapsed"], json!(true));
    assert_eq!(info["layers"][1]["blendMode"], json!("screen"));
    assert_eq!(info["layerFolders"][0]["folderId"], json!("f1"));
    assert!(info["layerModelUnavailable"].is_array());
    // Row bytes stay small enough that the 64 KiB UI bound still holds hundreds of layers.
    let per_layer = serde_json::to_vec(a).unwrap().len();
    assert!(per_layer < 220, "row grew to {per_layer} bytes");
}
