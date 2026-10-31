//! #2371: every layer-row fact is read from its native record; a fact with no storage is absent, never invented.
use super::{layer_members, model_members};
use crate::history::object_history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::ObjectDocument;
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
    value["items"] = json!({"raster":[raster("ly_a","r0","img-1")]});
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
    assert_eq!(
        rows[4]["mattesMore"],
        json!([{"uid":"extra","mode":"alpha"}])
    );
    // Baseline `matteSourceIndicesInUse` never consulted `mattesMore`: an additional-matte source carries no badge.
    assert!(rows[2].get("isMatteSource").is_none());
    // A stored null blend is not a named blend.
    assert!(rows[5].get("blendMode").is_none());
    assert!(rows[1].get("isMatteSource").is_none());
}

#[test]
fn the_stored_link_source_and_follow_target_are_reported() {
    let rows = rows(&flat(
        json!([{"layerUid":"lead"},{"layerUid":"linked","range":{"timeLink":{"uid":"lead"}},
            "followPath":{"targetLayerUid":"lead","align":true}}]), json!({})));
    assert_eq!((&rows[1]["timeLinkUid"], &rows[1]["followPathUid"]), (&json!("lead"), &json!("lead")));
    assert!(rows[0].get("timeLinkUid").is_none() && rows[0].get("followPathUid").is_none());
    assert_eq!((&rows[1]["followPathAlign"], rows[0].get("followPathAlign")), (&json!(true), None)); // orient toggle: true only
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
        json!([
            "linkGroup",
            "kind:camera",
            "kind:guide",
            "kind:montage",
            "kind:video"
        ])
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
}

/// A fully populated row: every member the read model can add, long names, a folder and two extra mattes (native storage admits no non-null parent).
fn rich_layers(count: usize) -> Value {
    let uid = |i: usize| format!("ly_{i:05}_mh3k2j9a4f");
    Value::Array(
        (0..count)
            .map(|i| {
                json!({"layerUid":uid(i),
                    "name":format!("Layer {i} \u{2014} Forme {} (d\u{e9}tail tr\u{e8}s long du nom, pour la mesure)", i % 17),
                    "visible":true,"locked":i % 3 == 0,"shy":i % 5 == 0,"color":"#5FA875",
                    "parentLayerUid":Value::Null,
                    "folderId":format!("folder-{:03}", i % 40),
                    "blendMode":"multiply","matteMode":"alphaInverted","matteSourceLayerUid":uid((i + 1) % count),
                    "mattesMore":[{"uid":uid((i + 2) % count),"mode":"luma"},{"uid":uid((i + 3) % count),"mode":"alpha"}]})
            })
            .collect(),
    )
}
fn rich_file(count: usize) -> Vec<u8> {
    let folders: Map<String, Value> = (0..40)
        .map(|f| {
            (
                format!("folder-{f:03}"),
                json!({"name":format!("Folder {f}"),"collapsed":f % 2 == 0}),
            )
        })
        .collect();
    let holders: Vec<Value> = (0..count)
        .map(|i| {
            let value = if i % 2 == 0 { json!({"static":[50.5]}) } else {
                json!({"static":[100],"track":{"keys":[{"frame":0,"v":[100]},{"frame":3,"v":[0]}]}})
            };
            let mut record = json!({"property":"opacity"});
            record.as_object_mut().unwrap().extend(value.as_object().unwrap().clone());
            json!({"target":{"contextId":"scene-root","layerUid":format!("ly_{i:05}_mh3k2j9a4f")},"properties":[record]})
        })
        .collect();
    let drawing = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4,
        "layers":rich_layers(count),"layerFolders":folders,"propertyHolders":holders,"objects":[]});
    serde_json::to_vec(&json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},"drawing":drawing,"viewportReference":null,"assets":[]}))
    .unwrap()
}

/// #2371 review: the whole project-info answer used to hard-fail above 64 KiB, and the richer rows put that line
/// at about 135 layers (measured 484 bytes per row). A few hundred fully populated rows (baseline Release to Layers makes one layer per shape) must read.
#[test]
fn rich_rows_stay_within_the_ui_bound() {
    const OLD_LIMIT: usize = 65536;
    const NEW_LIMIT: usize = 4 * 1024 * 1024;
    for count in [150usize, 400, 1000, 4096] {
        let owner = NativeObjectHistory::from_project_file("rich", &rich_file(count)).unwrap();
        let info = owner.project_file_info().unwrap();
        let bytes = serde_json::to_vec(&info).unwrap().len();
        let per_row = serde_json::to_vec(&info["layers"][1]).unwrap().len();
        let layers = info["layers"].as_array().unwrap();
        assert_eq!(layers.len(), count);
        for key in [
            "name",
            "kind",
            "folderId",
            "blendMode",
            "matteMode",
            "matteSourceLayerUid",
            "mattesMore",
            "opacity",
            "parentLayerUid",
            "color",
        ] {
            assert!(
                layers[1].get(key).is_some(),
                "{key} missing from the rich row"
            );
        }
        assert_eq!(info["layerFolders"].as_array().unwrap().len(), 40);
        assert!(
            per_row > 400 && per_row < 600,
            "rich row is {per_row} bytes"
        );
        assert!(bytes < NEW_LIMIT, "{count} rich layers need {bytes} bytes");
        if count >= 150 {
            // The earlier bound would have refused this answer outright.
            assert!(
                bytes > OLD_LIMIT,
                "{count} rich layers fit the old bound ({bytes})"
            );
        }
    }
}

/// Every matte fact the engine can emit must be an answer the metadata decoder admits: unbounded lists and folders,
/// empty and long strings (a stored file may carry any mode name).
#[test]
fn unbounded_stored_values_are_emitted_as_stored_not_truncated() {
    let many = 300usize;
    let mut layers: Vec<Value> = (0..many)
        .map(|i| json!({"layerUid":format!("s{i}")}))
        .collect();
    layers[0] = json!({"layerUid":"s0","blendMode":"","matteMode":"alpha","matteSourceLayerUid":"s1",
        "mattesMore":(2..many).map(|i| json!({"uid":format!("s{i}"),"mode":""})).collect::<Vec<_>>()});
    let folders: Map<String, Value> = (0..1500)
        .map(|f| (format!("fold-{f:04}"), json!({"collapsed":false})))
        .collect();
    let document = flat(Value::Array(layers), json!({"layerFolders":folders}));
    let rows = rows(&document);
    assert_eq!(rows[0]["mattesMore"].as_array().unwrap().len(), many - 2);
    assert_eq!(rows[0]["mattesMore"][0]["mode"], json!(""));
    assert_eq!(rows[0]["blendMode"], json!(""));
    assert_eq!(
        model_members(&document)["layerFolders"]
            .as_array()
            .unwrap()
            .len(),
        1500
    );
}

fn matte_sources(layers: Value) -> Vec<bool> {
    let document = flat(layers, json!({}));
    rows(&document)
        .iter()
        .map(|row| row.get("isMatteSource") == Some(&json!(true)))
        .collect()
}

/// Baseline `matteSourceIndicesInUse` (`timeline.js`, 1065e9d8): an active matte without a stored source uid consumes
/// the layer below (the legacy implicit adjacency; the native loader stores what the file says and does not migrate it).
#[test]
fn a_matte_without_a_source_uid_marks_the_layer_below() {
    assert_eq!(
        matte_sources(
            json!([{"layerUid":"a","matteMode":"alpha"},{"layerUid":"b"},{"layerUid":"c"}])
        ),
        [false, true, false]
    );
    // The last layer has nothing below it.
    assert_eq!(
        matte_sources(json!([{"layerUid":"a"},{"layerUid":"b","matteMode":"luma"}])),
        [false, false]
    );
    // An inactive matte (`none`) consumes nothing, with or without a uid.
    assert_eq!(
        matte_sources(json!([{"layerUid":"a","matteMode":"none"},{"layerUid":"b"}])),
        [false, false]
    );
}

#[test]
fn explicit_dangling_and_self_sources_follow_baseline() {
    // An explicit uid wins over adjacency.
    assert_eq!(
        matte_sources(
            json!([{"layerUid":"a","matteMode":"alpha","matteSourceLayerUid":"c"},{"layerUid":"b"},{"layerUid":"c"}])
        ),
        [false, false, true]
    );
    // A dangling uid resolves to nothing and does not fall back to the layer below.
    assert_eq!(
        matte_sources(
            json!([{"layerUid":"a","matteMode":"alpha","matteSourceLayerUid":"ghost"},{"layerUid":"b"}])
        ),
        [false, false]
    );
    // A self reference is skipped (the first OTHER layer with that uid), so it marks nothing.
    assert_eq!(
        matte_sources(
            json!([{"layerUid":"a","matteMode":"alpha","matteSourceLayerUid":"a"},{"layerUid":"b"}])
        ),
        [false, false]
    );
    // An additional matte source alone does not mark a layer (baseline ignored `mattesMore` here); the first does.
    assert_eq!(
        matte_sources(
            json!([{"layerUid":"a","matteMode":"alpha","matteSourceLayerUid":"b",
            "mattesMore":[{"uid":"c","mode":"luma"}]},{"layerUid":"b"},{"layerUid":"c"}])
        ),
        [false, true, false]
    );
}

/// #2372: the effects panel resends the whole stack, so a row carries it (keyed parameters included) within the bound.
#[test]
fn a_stored_effect_stack_is_carried_in_its_row_until_the_row_or_document_bound() {
    let stack = json!([{"type":"blur","enabled":true,"p1":4,"p2":null},
        {"type":"vignette","enabled":false,"p1":0.5,"p5":2}]);
    let document = flat(
        json!([{"layerUid":"plain"},
            {"layerUid":"empty","isEffectLayer":true,"effects":[]},
            {"layerUid":"fx","effects":stack}]),
        json!({}),
    );
    let got = rows(&document);
    assert!(got[0].get("effects").is_none());
    assert!(got[1].get("effects").is_none(), "an empty stack is absent");
    assert_eq!(got[2]["effects"], stack);
    assert!(got[2].get("effectsOversize").is_none());
    // Past the per-row bound the stack is named oversize, never truncated.
    let many: Vec<Value> = (0..45)
        .map(|i| json!({"type":format!("effect-{i:04}-{}", "x".repeat(60)),"enabled":true,"p1":i}))
        .collect();
    let big = flat(json!([{"layerUid":"big","effects":many}]), json!({}));
    let got = rows(&big);
    assert!(got[0].get("effects").is_none());
    assert_eq!(got[0]["effectsOversize"], json!(true));
}

/// #2372: the path-effects panel resends the whole `pathFx` stack, so a row carries it as stored (absent, null and numeric members
/// stay distinct) until the row bound; an empty stack is absent and a stack past the bound is named oversize.
#[test]
fn a_stored_path_effect_stack_is_carried_in_its_row_until_the_row_bound() {
    let stack = json!([{"type":"zigzag","enabled":false,"p1":4,"p2":null},{"type":"roughen"}]);
    let document = flat(
        json!([{"layerUid":"plain"},
            {"layerUid":"empty","pathFx":[]},
            {"layerUid":"fx","pathFx":stack}]),
        json!({}),
    );
    let got = rows(&document);
    assert!(got[0].get("pathFx").is_none());
    assert!(got[1].get("pathFx").is_none(), "an empty stack is absent");
    assert_eq!(got[2]["pathFx"], stack);
    assert!(got[2].get("pathFxOversize").is_none());
    let many: Vec<Value> = (0..100)
        .map(|i| json!({"type":"zigzag","enabled":true,"p1":i,"p2":1.5,"p3":2.5}))
        .collect();
    let big = flat(json!([{"layerUid":"big","pathFx":many}]), json!({}));
    let got = rows(&big);
    assert!(got[0].get("pathFx").is_none());
    assert_eq!(got[0]["pathFxOversize"], json!(true));
}

/// #2372: the read bound is at least what an edit can write. A full 32-entry stack of baseline panel entries (all eight
/// parameters, the longest built-in type name) and a ~60-entry path-effect stack both stay listed, and a stack past the
/// bound degrades to a flagged row beside intact neighbours instead of failing the whole table.
#[test]
fn stacks_an_edit_can_write_stay_readable_and_oversize_degrades_to_a_flag() {
    let effect = |i: usize| {
        json!({"type":"chromaticAberration","enabled":true,"p1":i as f64 + 0.25,"p2":0.5,"p3":0.125,"p4":100,
            "p5":0.333333,"p6":0.75,"p7":12.5,"p8":0.1})
    };
    let at_limit: Vec<Value> = (0..30).map(effect).collect();
    let edit_bytes = serde_json::to_string(&json!(at_limit)).unwrap().len();
    assert!(edit_bytes > 2048 && edit_bytes + 215 <= 4096, "{edit_bytes}");
    let fx: Vec<Value> = (0..60)
        .map(|i| json!({"type":"zigzag","enabled":true,"p1":i,"p2":1.5,"p3":2.5}))
        .collect();
    let huge: Vec<Value> = (0..200).map(effect).collect();
    let document = flat(
        json!([{"layerUid":"full","effects":at_limit,"pathFx":fx},
            {"layerUid":"after","effects":[{"type":"blur","enabled":true}]},
            {"layerUid":"huge","effects":huge,"pathFx":[{"type":"zigzag"}]}]),
        json!({}),
    );
    let got = rows(&document);
    assert_eq!(got[0]["effects"].as_array().unwrap().len(), 30);
    assert_eq!(got[0]["pathFx"].as_array().unwrap().len(), 60);
    assert!(got[0].get("effectsOversize").is_none() && got[0].get("pathFxOversize").is_none());
    assert_eq!(got[1]["effects"].as_array().unwrap().len(), 1);
    assert_eq!(got[2]["effectsOversize"], json!(true));
    assert!(got[2].get("effects").is_none());
    assert_eq!(got[2]["pathFx"].as_array().unwrap().len(), 1);
}
