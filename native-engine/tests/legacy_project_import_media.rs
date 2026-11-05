//! Legacy project reader, slice 3: media (rasters, PNG and WAV payloads, image meshes, audio tracks, the reference image),
//! vector text, the per-stroke tags of the vector record, groups, and the import -> save -> reopen round trip of all of it.
use super::refusal_tests::saved;
use super::unit_tests::*;
use super::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

const MEDIA: &str = include_str!("../../tests/fixtures/legacy-import/media.json");
const FIXTURE_TEXT: &str = include_str!("../../tests/fixtures/text/project.json");
const FIXTURE_MEDIA: &str = include_str!("../../tests/fixtures/media/project.json");
const FIXTURE_MESH: &str = include_str!("../../tests/fixtures/mesh/project.json");
const FIXTURE_MASKS: &str = include_str!("../../tests/fixtures/masks-alpha/project.json");

fn media() -> (Converted, Value) {
    let converted = import(MEDIA);
    let wire = file(&converted);
    (converted, wire)
}
fn record<'a>(wire: &'a Value, id: &str) -> &'a Value {
    wire["drawing"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["target"]["strokeId"] == id)
        .unwrap_or_else(|| panic!("record {id}"))
}
fn has_warning(converted: &Converted, path: &str, code: &str) -> bool {
    converted
        .report
        .warnings
        .iter()
        .any(|w| w.path == path && w.code == code)
}

#[test]
fn every_pinned_baseline_media_fixture_imports() {
    for (name, text) in [
        ("text", FIXTURE_TEXT),
        ("media", FIXTURE_MEDIA),
        ("mesh", FIXTURE_MESH),
        ("masks-alpha", FIXTURE_MASKS),
    ] {
        let converted =
            convert_admitted(text.as_bytes()).unwrap_or_else(|e| panic!("{name}: {}", e.message()));
        NativeObjectHistory::from_project_file("fixture", &converted.project_file).unwrap();
    }
    let mask = file(&import(FIXTURE_MASKS));
    let masked = mask["drawing"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["vectorExt"]["isMask"] == true)
        .unwrap();
    assert_eq!(masked["vectorExt"]["maskMode"], "add");
}

#[test]
fn rasters_become_items_over_deduplicated_catalog_assets() {
    let (converted, wire) = media();
    assert_eq!(wire["formatVersion"], 2);
    let assets = wire["assets"].as_array().unwrap();
    let kinds: Vec<(String, String)> = assets
        .iter()
        .map(|a| {
            (
                a["kind"].as_str().unwrap().into(),
                a["source"]["kind"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ("image".into(), "embedded".into()),
            ("image".into(), "linked".into()),
            ("audio".into(), "embedded".into())
        ],
        "one PNG shared by the layer rasters and the reference image, one linked file, one WAV"
    );
    assert_eq!(assets[1]["source"]["locator"], "/Users/example/art/linked.png");
    assert_eq!(assets[1]["name"], "linked.png");
    let rasters = wire["drawing"]["items"]["raster"].as_array().unwrap();
    assert_eq!(rasters.len(), 3);
    assert_eq!(
        rasters[0]["item"],
        json!({"assetId": "legacy-asset-1", "x": 200, "y": 150, "width": 128, "height": 96,
        "opacity": 0.5, "rotation": 15, "meshId": "im_1"})
    );
    assert_eq!(rasters[1]["item"]["assetId"], "legacy-asset-2");
    assert_eq!(
        rasters[2]["item"]["assetId"], "legacy-asset-1",
        "equal payloads share one asset"
    );
    assert_eq!(
        rasters[0]["target"]["strokeId"],
        "legacy-raster:ly_media:0:0"
    );
    assert!(has_warning(
        &converted,
        "layers[1].frames[0].strokes[2].isText",
        "raster_text_flattened"
    ));
    assert_eq!(
        wire["viewportReference"],
        json!({"kind": "still-image", "assetId": "legacy-asset-1", "name": "ref.png",
        "visible": true, "opacity": 0.4, "offsetFrames": 2})
    );
    assert!(converted
        .report
        .ignored
        .contains(&"layers[1].footage".to_owned()));
    assert!(has_warning(&converted, "mediaLibrary", "derived_listing"));
    assert!(
        wire["drawing"]["imageMeshes"]["im_1"]["tris"]
            .as_array()
            .unwrap()
            .len()
            > 3
    );
}

#[test]
fn audio_tracks_keep_their_values_and_the_offset_becomes_the_placement() {
    let (_, wire) = media();
    let track = &wire["drawing"]["audioTracks"][0];
    assert_eq!(track["assetId"], "legacy-asset-3");
    assert_eq!(
        (
            &track["audioId"],
            &track["volume"],
            &track["trimStart"],
            &track["trimEnd"]
        ),
        (&json!("au_score"), &json!(0.8), &json!(0), &Value::Null)
    );
    assert_eq!(track["motionStatic"], json!({"volume": [0.5]}));
    assert_eq!(
        wire["assetInventory"]["placements"][0]["target"],
        json!({"kind": "audioTrack", "trackUid": "au_score"})
    );
    assert_eq!(
        wire["assetInventory"]["placements"][0]["offsetFrames"],
        -3.0
    );
}

#[test]
fn vector_text_becomes_a_block_and_an_unbundled_font_stays_as_outlines() {
    let (converted, wire) = media();
    let text = wire["drawing"]["items"]["text"].as_array().unwrap();
    assert_eq!(text.len(), 1);
    assert_eq!(text[0]["target"]["strokeId"], "s_w0");
    assert_eq!(
        text[0]["block"],
        json!({"text": "NEM", "font": {"kind": "bundled", "fontKey": "Roboto-Regular"}, "size": 48,
        "color": "#222222", "align": "center", "anchorTopLeft": {"x": 100, "y": 120}, "fixedWidth": 300, "bold": true,
        "letterSpacing": 2, "lineHeightMult": 1.5, "textCase": "upper"})
    );
    let ids: Vec<&str> = wire["drawing"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["target"]["layerUid"] == "ly_words")
        .map(|o| o["target"]["strokeId"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["s_g0", "s_g1"],
        "the glyphs of the block are laid out again; the unbundled font keeps its outlines"
    );
    assert!(has_warning(
        &converted,
        "layers[2].frames[0].strokes[3].vectorFont",
        "text_outlined"
    ));
}

#[test]
fn stroke_tags_land_in_the_vector_extension_with_their_values() {
    let (_, wire) = media();
    assert_eq!(
        record(&wire, "s_mask")["vectorExt"],
        json!({"isMask": true, "maskMode": "alpha", "maskFeather": 2})
    );
    assert_eq!(record(&wire, "s_grad")["vectorExt"]["boxAngle"], 0.5);
    assert_eq!(
        record(&wire, "s_grad")["vectorExt"]["fillGradient"]["stops"][1]["color"],
        "#0000ff"
    );
    assert_eq!(
        record(&wire, "s_shape")["vectorExt"]["paramShape"]["kind"],
        "rect"
    );
    assert_eq!(
        record(&wire, "s_fill")["vectorExt"]["fillTrace"],
        json!({"seed": [10, 10], "walls": ["s_a"], "gapPx": 0})
    );
    assert_eq!(
        record(&wire, "s_shadow")["vectorExt"]["shadow"],
        json!({"channelTag": "shadow", "shadowSwatchId": "sw1"})
    );
    assert_eq!(
        record(&wire, "s_tex")["vectorExt"]["brushTexture"],
        json!({"brushGroupId": "bg1", "brushTexturePreset": "chalk",
        "preTextureOpacity": 1, "preTextureStroke": "#112233"})
    );
}

#[test]
fn groups_keep_their_tree_and_repair_what_the_baseline_left_dangling() {
    let (_, wire) = media();
    let table = &wire["drawing"]["layers"][0]["groups"];
    assert_eq!(table["contextId"], "scene-root");
    assert_eq!(
        table["entries"],
        json!([{"gid": "g_pair", "order": ["s_a", "s_b"], "name": "Pair", "combineMode": "none"}])
    );
    assert_eq!(
        table["members"],
        json!([{"strokeId": "s_a", "groupId": "g_pair"}, {"strokeId": "s_b", "groupId": "g_pair"}])
    );
    let mut doc: Value = serde_json::from_str(MEDIA).unwrap();
    doc["layers"][0]["groups"]["g_pair"]["order"] = json!(["s_a", "s_b", "s_gone"]);
    doc["layers"][0]["groups"]["g_lone"] = json!({"name": "Lone", "order": ["s_mask"]});
    let converted = import(&doc.to_string());
    assert!(has_warning(
        &converted,
        "layers[0].groups.g_pair",
        "group_dangling_member"
    ));
    assert!(has_warning(
        &converted,
        "layers[0].groups.g_lone",
        "group_dissolved"
    ));
    assert_eq!(
        file(&converted)["drawing"]["layers"][0]["groups"]["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn the_whole_media_project_saves_and_reopens_identically_and_edits_undo() {
    let (converted, _) = media();
    let mut owner =
        NativeObjectHistory::from_project_file("media-roundtrip", &converted.project_file).unwrap();
    let first = saved(&owner);
    let reopened = NativeObjectHistory::from_project_file("media-roundtrip-2", &first).unwrap();
    assert_eq!(
        saved(&reopened),
        first,
        "save -> reopen -> save is byte identical"
    );
    let wire: Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(
        wire["drawing"]["items"]["raster"].as_array().unwrap().len(),
        3
    );
    assert_eq!(
        wire["drawing"]["items"]["text"].as_array().unwrap().len(),
        1
    );
    assert_eq!(wire["assets"].as_array().unwrap().len(), 3);
    let payload =
        json!({"command": "object.layer.visibility.set", "layerUid": "ly_media", "visible": false});
    let prepared = owner
        .prepare_layer_visibility_json(payload.to_string().as_bytes())
        .unwrap();
    let edited = owner.commit_layer_visibility(prepared).unwrap();
    assert_eq!(owner.history_depths(), (1, 0));
    owner.undo(edited).unwrap();
    assert_eq!(
        saved(&owner),
        first,
        "undo restores the imported document exactly"
    );
}

#[test]
fn media_the_native_engine_cannot_hold_refuses_naming_the_exact_path() {
    let with = |edit: &dyn Fn(&mut Value)| {
        let mut doc: Value = serde_json::from_str(MEDIA).unwrap();
        edit(&mut doc);
        convert_admitted(doc.to_string().as_bytes())
            .err()
            .expect("refused")
    };
    let jpeg = with(&|d| {
        d["layers"][1]["frames"][0]["strokes"][0]["src"] =
            json!("data:image/jpeg;base64,/9j/4AAQSkZJRgABAQ")
    });
    assert_eq!(
        (jpeg.code(), jpeg.path()),
        (
            LEGACY_UNSUPPORTED_FIELD,
            "layers[1].frames[0].strokes[0].src"
        )
    );
    let handle = with(&|d| {
        let s = &mut d["layers"][1]["frames"][0]["strokes"][1];
        s.as_object_mut().unwrap().remove("linkedPath");
        s["linkedHandleId"] = json!("h1");
    });
    assert_eq!(
        handle.path(),
        "layers[1].frames[0].strokes[1].linkedHandleId"
    );
    let mp3 = with(&|d| {
        d["audioTracks"][0]["dataB64"] = json!("data:audio/mpeg;base64,SUQzBAAAAAAAI1RTU0UAAAAP")
    });
    assert_eq!(mp3.path(), "audioTracks[0].dataB64");
    let video = with(&|d| {
        d["refMedia"] =
            json!({"type": "video", "name": "v.mp4", "src": "data:video/mp4;base64,AAAA"})
    });
    assert_eq!(video.path(), "refMedia");
    let footage = with(&|d| d["layers"][1]["footage"] = json!({"kind": "video"}));
    assert_eq!(footage.path(), "layers[1].footage");
    let gradient =
        with(&|d| d["layers"][0]["frames"][0]["strokes"][3]["fillGradient"]["stops"] = json!([]));
    assert_eq!(
        gradient.path(),
        "layers[0].frames[0].strokes[3].fillGradient"
    );
    let raster_group = with(&|d| d["layers"][1]["frames"][0]["strokes"][0]["groupId"] = json!("g"));
    assert_eq!(
        raster_group.path(),
        "layers[1].frames[0].strokes[0].groupId"
    );
}

#[test]
fn layer_and_stroke_effects_pass_to_the_native_records_unchanged() {
    let mut doc: Value = serde_json::from_str(MEDIA).unwrap();
    let stack = json!([{"type": "hue", "enabled": true, "p1": 30}, {"type": "saturation", "enabled": false, "p1": null}]);
    doc["layers"][0]["effects"] = stack.clone();
    doc["layers"][0]["frames"][0]["strokes"][2]["effects"] = stack.clone();
    let wire = file(&import(&doc.to_string()));
    assert_eq!(wire["drawing"]["layers"][0]["effects"], stack);
    assert_eq!(record(&wire, "s_mask")["effects"], stack);
    doc["layers"][0]["effects"] = json!([{"type": "hue"}]);
    let error = convert_admitted(doc.to_string().as_bytes())
        .err()
        .expect("refused");
    assert_eq!(error.path(), "layers[0].effects");
}

struct Host {
    png: Vec<u8>,
}
impl MediaTranscoder for Host {
    fn to_png(&self, mime: &str, bytes: &[u8]) -> Option<Vec<u8>> {
        (mime == "image/jpeg" && bytes.starts_with(&[0xff, 0xd8])).then(|| self.png.clone())
    }
    fn to_wav(&self, mime: &str, bytes: &[u8]) -> Option<Vec<u8>> {
        (mime == "audio/mpeg" && bytes.starts_with(b"ID3")).then(|| {
            let mut wav = b"RIFF\x2c\0\0\0WAVEfmt \x10\0\0\0\x01\0\x02\0\x80\xbb\0\0\0\xee\x02\0\x04\0\x10\0data\x08\0\0\0".to_vec();
            wav.extend_from_slice(&[0, 0, 255, 127, 0, 128, 1, 0]);
            wav
        })
    }
}

#[test]
fn media_in_other_formats_is_converted_by_the_host_and_refused_without_one() {
    use base64::Engine;
    let std = base64::engine::general_purpose::STANDARD;
    let mut doc: Value = serde_json::from_str(MEDIA).unwrap();
    let png_url = doc["layers"][1]["frames"][0]["strokes"][0]["src"]
        .as_str()
        .unwrap()
        .to_owned();
    let png = std.decode(png_url.split_once(',').unwrap().1).unwrap();
    let jpeg = format!(
        "data:image/jpeg;base64,{}",
        std.encode([0xff, 0xd8, 0xff, 0xe0, 0, 16, b'J', b'F', b'I', b'F'])
    );
    let mp3 = format!(
        "data:audio/mpeg;base64,{}",
        std.encode(b"ID3\x04\0\0\0\0\0\0frame-data")
    );
    doc["layers"][1]["frames"][0]["strokes"][0]["src"] = json!(jpeg);
    doc["audioTracks"][0]["dataB64"] = json!(mp3);
    doc["refMedia"]["src"] = json!(jpeg);
    let bytes = doc.to_string();
    let error = convert_admitted(bytes.as_bytes())
        .err()
        .expect("no converter, no import");
    assert_eq!(
        (error.code(), error.path()),
        (
            LEGACY_UNSUPPORTED_FIELD,
            "layers[1].frames[0].strokes[0].src"
        )
    );
    let converted = convert_admitted_with(bytes.as_bytes(), &Host { png: png.clone() })
        .unwrap_or_else(|e| panic!("{}", e.message()));
    let wire = file(&converted);
    assert_eq!(
        wire["assets"][0]["source"]["data"],
        png_url.split_once(',').unwrap().1,
        "the JPEG became the converted PNG"
    );
    assert_eq!(wire["assets"][0]["source"]["mediaType"], "image/png");
    let wav = wire["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["kind"] == "audio")
        .unwrap();
    assert_eq!(
        std.decode(wav["source"]["data"].as_str().unwrap()).unwrap()[..4],
        *b"RIFF"
    );
    NativeObjectHistory::from_project_file("transcoded", &converted.project_file).unwrap();
    let broken = convert_admitted_with(
        bytes.as_bytes(),
        &Host {
            png: b"not a png".to_vec(),
        },
    )
    .err()
    .expect("a bad conversion is refused");
    assert_eq!(broken.path(), "layers[1].frames[0].strokes[0].src");
}
