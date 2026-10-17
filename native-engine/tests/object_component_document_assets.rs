//! StoryBoard sound-asset resolution and the legacy project listing (L04-I).
//!
//! A StoryBoard sound names its audio by `assetId` (the baseline's inline `dataB64`
//! becomes a project asset). The drawing alone cannot resolve it, so resolution is a
//! project-level check against the asset inventory: every control here builds the real
//! `AssetInventory` value and calls the real resolver. The golden document carries
//! exactly one sound, `sbm12`, produced by the verbatim legacy migration.
use super::tests::{document, rows};
use super::*;
use crate::object_document::{AssetRecord, AssetSource};
use crate::project_structure::parse_project_structure;
use std::sync::Arc;

const LEGACY: &str = include_str!("../../tests/fixtures/components/project.json");

pub(crate) fn inventory_of(id: &str, kind: AssetKind) -> AssetInventory {
    let record = AssetRecord::new(
        AssetId::new(id.to_string()).unwrap(),
        "Beat".to_string(),
        kind,
        AssetSource::linked("beat.wav".to_string(), 4096).unwrap(),
    )
    .unwrap();
    AssetInventory::new(vec![Arc::new(record)], Vec::new(), 8, 0).unwrap()
}
pub(crate) fn audio_inventory(kind: AssetKind) -> AssetInventory {
    inventory_of(sound_asset(), kind)
}
fn sound_asset() -> &'static str {
    let sounds: Vec<Vec<&'static str>> =
        rows("M").into_iter().filter(|r| r[2] == "sound").collect();
    assert_eq!(sounds.len(), 1, "the golden document holds one sound");
    sounds[0][6]
}

#[test]
fn the_golden_sound_resolves_against_an_audio_asset() {
    assert_eq!(sound_asset(), "asset-audio-sbm12");
    let inventory = audio_inventory(AssetKind::Audio);
    assert_eq!(resolve_sound_assets(&document(), Some(&inventory)), Ok(()));
}

#[test]
fn a_sound_cannot_resolve_without_an_inventory_or_with_another_asset() {
    assert_eq!(
        resolve_sound_assets(&document(), None),
        Err(SoundAssetError::Unresolved)
    );
    let other = inventory_of("asset-audio-other", AssetKind::Audio);
    assert_eq!(
        resolve_sound_assets(&document(), Some(&other)),
        Err(SoundAssetError::Unresolved)
    );
}

#[test]
fn only_an_audio_record_satisfies_a_sound() {
    for kind in [AssetKind::Image, AssetKind::Video, AssetKind::Font] {
        let inventory = audio_inventory(kind);
        assert_eq!(
            resolve_sound_assets(&document(), Some(&inventory)),
            Err(SoundAssetError::NotAudio),
            "{kind:?}"
        );
    }
}

#[test]
fn documents_without_a_sound_need_no_inventory() {
    let mut without_board = document();
    if let Presence::Present(tables) = &mut without_board.component_tables {
        tables.storyboard = Presence::Missing;
    }
    assert_eq!(resolve_sound_assets(&without_board, None), Ok(()));
    let plain =
        crate::object_codec::decode_storage_project(super::tests::base().as_bytes()).unwrap();
    assert_eq!(resolve_sound_assets(&plain, None), Ok(()));
}

#[test]
fn the_legacy_listing_equals_the_baseline_project_file() {
    let structure = parse_project_structure(LEGACY).unwrap();
    let ids: Vec<&str> = rows("P").iter().map(|r| r[1]).collect();
    assert_eq!(structure.component_ids(), ids);
    let expected: Vec<(&str, &str)> = rows("Q").iter().map(|r| (r[1], r[2])).collect();
    assert_eq!(structure.component_instances(), expected);
}

#[test]
fn the_legacy_listing_reads_only_well_formed_component_data() {
    let text = r#"{"layers":[{"layerUid":"a","symbolId":"s1","frames":[]},{"layerUid":"b","symbolId":"","frames":[]},{"symbolId":"s2","frames":[]},{"layerUid":"d","frames":[]},{"layerUid":"e","symbolId":7,"frames":[]}],"symbols":{"s2":{},"s1":{}}}"#;
    let structure = parse_project_structure(text).unwrap();
    // Sorted by id (the retained value keeps object members in key order); layers
    // without a uid, with an empty or non-string symbolId are not instances.
    assert_eq!(structure.component_ids(), vec!["s1", "s2"]);
    assert_eq!(structure.component_instances(), vec![("a", "s1")]);
    let no_symbols = r#"{"layers":[{"frames":[]}]}"#;
    let structure = parse_project_structure(no_symbols).unwrap();
    assert!(structure.component_ids().is_empty());
    assert!(structure.component_instances().is_empty());
    let array_symbols = r#"{"layers":[{"frames":[]}],"symbols":["s1"]}"#;
    let structure = parse_project_structure(array_symbols).unwrap();
    assert!(structure.component_ids().is_empty());
}

/// The project wire is where a drawing meets its inventory: the same bytes that the
/// owner path checks at commit are checked while a project file is decoded.
#[cfg(feature = "project_media_codec")]
mod wire {
    use super::super::tests::{document, golden_value};
    use crate::object_codec::project_settings_test_codec::{
        decode_project, encode_project, ProjectWireLimits,
    };
    use serde_json::{json, Value};

    fn limits() -> ProjectWireLimits {
        ProjectWireLimits::new(8 * 1024 * 1024, 1024 * 1024).unwrap()
    }
    fn legacy(drawing: &Value) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":640,"canvasH":360,"fps":24},"drawing":drawing,
            "viewportReference":null,"assets":[]}))
        .unwrap()
    }
    fn catalog(drawing: &Value, assets: Value) -> Vec<u8> {
        let ids: Vec<Value> = assets
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["id"].clone())
            .collect();
        serde_json::to_vec(&json!({
            "format":"nemo.native-project-document","formatVersion":2,
            "project":{"canvasW":640,"canvasH":360,"fps":24},"drawing":drawing,
            "viewportReference":null,"assets":assets,
            "assetInventory":{"assetIds":ids,"placements":[]}}))
        .unwrap()
    }
    fn linked(id: &str, kind: &str) -> Value {
        json!([{"id":id,"name":"Beat","kind":kind,"source":{"kind":"linked","locator":"beat.bin"}}])
    }

    #[test]
    fn a_catalog_project_resolves_the_sound_and_round_trips_the_tables() {
        let drawing = golden_value();
        let wire = catalog(&drawing, linked("asset-audio-sbm12", "audio"));
        let project = decode_project(&wire, limits()).unwrap();
        assert_eq!(project.drawing(), &document());
        let encoded = encode_project(&project, limits()).unwrap();
        let out: Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(out["drawing"], drawing);
        assert_eq!(decode_project(&encoded, limits()).unwrap(), project);
        // Only the asset changes in each refused wire below.
        for assets in [
            linked("asset-audio-sbm12", "image"),
            linked("asset-audio-other", "audio"),
            json!([]),
        ] {
            assert!(decode_project(&catalog(&drawing, assets), limits()).is_err());
        }
    }

    #[test]
    fn a_project_without_an_inventory_cannot_hold_a_sound_but_keeps_the_other_tables() {
        let drawing = golden_value();
        assert!(decode_project(&legacy(&drawing), limits()).is_err());
        let mut quiet = drawing;
        quiet["componentTables"]
            .as_object_mut()
            .unwrap()
            .remove("storyboard");
        let project = decode_project(&legacy(&quiet), limits()).unwrap();
        assert!(project.drawing().component_set().is_some());
        let encoded = encode_project(&project, limits()).unwrap();
        let out: Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(out["drawing"], quiet);
    }
}
