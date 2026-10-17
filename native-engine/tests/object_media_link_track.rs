//! Wire tests for asset placements (G42, I16b): the real project codec reads
//! and writes the `assetInventory.placements` table.
//!
//! `fixtures/object-media-link-track/oracle.txt` rows `O` and `N` were produced
//! from the shipped baseline statements named in `tools/gen.js` (generator,
//! commit fbe4b8a3): `Number.isSafeInteger(r.offsetFrames)` of the native
//! project adapter and `offsetFrames:t.offsetFrames||0` of the audio-track save.
//! Rows: `O <kind> <safe> <canonical> <token>`, `N <variant> <token> <value>`.

use super::*;
use crate::object_codec::project::{
    decode_project, encode_project, prepare_project_wire, ProjectWireLimits,
};
use crate::object_document::{
    AssetKind, AssetPlacement, AssetRecord, AssetSource, ProjectEnvelope,
};
use std::sync::Arc;

const ORACLE: &str = include_str!("fixtures/object-media-link-track/oracle.txt");

const HEAD: &str = r#"{"format":"nemo.native-project-document","formatVersion":2,"project":{"canvasW":320,"canvasH":180,"fps":24},"drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":24,"layers":[{"layerUid":"L"}],"objects":[]},"viewportReference":null,"assets":[{"id":"v1","name":"take-1","kind":"video","source":{"kind":"linked","locator":"clips/take-1.mov"}},{"id":"a1","name":"score","kind":"audio","source":{"kind":"linked","locator":"audio/score.wav"}},{"id":"f1","name":"face","kind":"font","source":{"kind":"linked","locator":"fonts/face.otf"}}],"assetInventory":{"assetIds":["v1","a1","f1"],"placements":"#;

const VIDEO: &str = r#"{"id":"p1","assetId":"v1","target":{"kind":"drawing","contextId":"scene-root","layerUid":"L"},"offsetFrames":-3}"#;
const AUDIO: &str = r#"{"id":"p2","assetId":"a1","target":{"kind":"audioTrack","trackUid":"t1"},"offsetFrames":48}"#;
const VIDEO_PREFIX: &str = r#"{"id":"p1","assetId":"v1","target":{"kind":"drawing","contextId":"scene-root","layerUid":"L"},"offsetFrames":"#;

fn wire(placements: &str) -> String {
    String::from(HEAD) + placements + "}}"
}
fn limits() -> ProjectWireLimits {
    ProjectWireLimits::new(1 << 20, 1 << 16).unwrap()
}
fn decode(text: &str) -> Result<ProjectEnvelope, ProjectWireError> {
    decode_project(text.as_bytes(), limits())
}
fn encode(project: &ProjectEnvelope) -> String {
    String::from_utf8(encode_project(project, limits()).unwrap()).unwrap()
}
fn list(entries: &[&str]) -> String {
    format!("[{}]", entries.join(","))
}
fn drawing(layer: &str) -> AssetTarget {
    AssetTarget::Drawing {
        context_id: "scene-root".into(),
        layer_uid: layer.into(),
        stroke_id: None,
    }
}

fn rows(tag: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|line| line.split(' ').next() == Some(tag))
        .map(|line| line.split(' ').collect())
        .collect()
}

#[test]
fn oracle_has_the_documented_row_counts() {
    assert_eq!(rows("O").len(), 26);
    assert_eq!(rows("D").len(), 99);
    assert_eq!(rows("N").len(), 5);
}

#[test]
fn oracle_offset_tokens_are_accepted_exactly_when_the_baseline_reads_a_safe_integer() {
    let mut accepted = 0;
    for row in rows("O") {
        let (kind, safe, canonical, token) = (row[1], row[2], row[3], row[4]);
        let text = wire(&list(&[&(String::from(VIDEO_PREFIX) + token + "}")]));
        let expected = kind == "int" && safe == "1";
        match decode(&text) {
            Ok(project) => {
                assert!(expected, "token {token} must be refused");
                accepted += 1;
                let value: i64 = canonical.parse().unwrap();
                let inventory = project.asset_inventory().unwrap();
                assert_eq!(inventory.placements()[0].offset_frames(), value);
                assert!(encode(&project).contains(&format!(r#""offsetFrames":{canonical}}}"#)));
            }
            Err(error) => {
                assert!(!expected, "token {token} must be accepted");
                assert_eq!(error, ProjectWireError::Invalid, "token {token}");
            }
        }
    }
    // 13 int tokens within +-(2^53 - 1); `1e3` and `1.0` are numbers the
    // baseline reads as safe integers but the strict wire refuses.
    assert_eq!(accepted, 13);
}

#[test]
fn oracle_absent_and_null_offsets_are_refused_instead_of_defaulted_to_zero() {
    let mut refused = 0;
    for row in rows("N") {
        let (token, saved) = (row[2], row[3]);
        let entry = if token == "-" {
            r#"{"id":"p1","assetId":"v1","target":{"kind":"drawing","contextId":"scene-root","layerUid":"L"}}"#
                .to_owned()
        } else {
            String::from(VIDEO_PREFIX) + token + "}"
        };
        let result = decode(&wire(&list(&[&entry])));
        if token == "-" || token == "null" {
            // The baseline save writes 0 for both; the wire keeps them apart.
            assert_eq!(saved, "0");
            assert_eq!(result.unwrap_err(), ProjectWireError::Invalid);
            refused += 1;
        } else {
            let project = result.unwrap();
            let offset = project.asset_inventory().unwrap().placements()[0].offset_frames();
            assert_eq!(offset.to_string(), saved);
        }
    }
    assert_eq!(refused, 2);
}

#[test]
fn video_placement_round_trips_with_every_authored_field() {
    let project = decode(&wire(&list(&[VIDEO]))).unwrap();
    let inventory = project.asset_inventory().unwrap();
    assert_eq!(inventory.placements().len(), 1);
    let placement = &inventory.placements()[0];
    assert_eq!(placement.id(), "p1");
    assert_eq!(placement.asset_id().as_str(), "v1");
    assert_eq!(placement.offset_frames(), -3);
    assert_eq!(placement.target(), &drawing("L"));
    let saved = encode(&project);
    assert!(saved.contains(&format!(r#""placements":[{VIDEO}]"#)));
    let again = decode(&saved).unwrap();
    assert_eq!(again.asset_inventory(), project.asset_inventory());
    assert_eq!(encode(&again), saved);
}

#[test]
fn explicit_zero_is_a_value_and_the_member_is_always_written() {
    let zero = VIDEO.replace(r#""offsetFrames":-3"#, r#""offsetFrames":0"#);
    let project = decode(&wire(&list(&[&zero]))).unwrap();
    let placement = &project.asset_inventory().unwrap().placements()[0];
    assert_eq!(placement.offset_frames(), 0);
    assert!(encode(&project).contains(&format!(r#""placements":[{zero}]"#)));
}

#[test]
fn stroke_id_absent_is_the_whole_layer_and_null_or_blank_is_invalid() {
    let with_stroke = VIDEO.replace(
        r#""layerUid":"L"}"#,
        r#""layerUid":"L","strokeId":"ink/path_A"}"#,
    );
    let project = decode(&wire(&list(&[&with_stroke]))).unwrap();
    let placement = &project.asset_inventory().unwrap().placements()[0];
    assert_eq!(
        placement.target(),
        &AssetTarget::Drawing {
            context_id: "scene-root".into(),
            layer_uid: "L".into(),
            stroke_id: Some("ink/path_A".into()),
        }
    );
    assert!(encode(&project).contains(&with_stroke));
    for bad in [r#""strokeId":null"#, r#""strokeId":"""#, r#""strokeId":7"#] {
        let entry = VIDEO.replace(r#""layerUid":"L"}"#, &format!(r#""layerUid":"L",{bad}}}"#));
        let error = decode(&wire(&list(&[&entry]))).unwrap_err();
        assert_eq!(error, ProjectWireError::Invalid, "{bad}");
    }
}

#[test]
fn audio_track_placement_round_trips_at_the_wire() {
    let project = decode(&wire(&list(&[AUDIO]))).unwrap();
    let placement = &project.asset_inventory().unwrap().placements()[0];
    assert_eq!(
        placement.target(),
        &AssetTarget::AudioTrack {
            track_uid: "t1".into()
        }
    );
    assert_eq!(placement.offset_frames(), 48);
    assert!(encode(&project).contains(&format!(r#""placements":[{AUDIO}]"#)));
}

#[test]
fn placement_order_is_the_file_order_through_save() {
    let text = wire(&list(&[AUDIO, VIDEO]));
    let project = decode(&text).unwrap();
    let ids: Vec<&str> = project
        .asset_inventory()
        .unwrap()
        .placements()
        .iter()
        .map(|p| p.id())
        .collect();
    assert_eq!(ids, ["p2", "p1"]);
    assert!(encode(&project).contains(&format!(r#""placements":[{AUDIO},{VIDEO}]"#)));
}

#[test]
fn empty_placements_still_encode_as_an_empty_array() {
    let project = decode(&wire("[]")).unwrap();
    assert!(project.asset_inventory().unwrap().placements().is_empty());
    assert!(encode(&project)
        .contains(r#""assetInventory":{"assetIds":["v1","a1","f1"],"placements":[]}"#));
}

#[test]
fn text_target_is_unsupported_and_other_kinds_are_invalid() {
    let text_target = r#"{"id":"p3","assetId":"f1","target":{"kind":"text","contextId":"scene-root","layerUid":"L"},"offsetFrames":0}"#;
    assert_eq!(
        decode(&wire(&list(&[text_target]))).unwrap_err(),
        ProjectWireError::Unsupported
    );
    for kind in ["video", "Drawing", "", "audio"] {
        let entry = VIDEO.replace(r#""kind":"drawing""#, &format!(r#""kind":"{kind}""#));
        let error = decode(&wire(&list(&[&entry]))).unwrap_err();
        assert_eq!(error, ProjectWireError::Invalid, "kind {kind:?}");
    }
    let numeric_kind = VIDEO.replace(r#""kind":"drawing""#, r#""kind":5"#);
    assert_eq!(
        decode(&wire(&list(&[&numeric_kind]))).unwrap_err(),
        ProjectWireError::Invalid
    );
}

#[test]
fn unknown_members_duplicates_and_non_objects_are_invalid() {
    let mixed = r#"{"id":"p1","assetId":"v1","target":{"kind":"drawing","contextId":"scene-root","layerUid":"L","trackUid":"t1"},"offsetFrames":0}"#;
    let audio_extra = AUDIO.replace(r#""trackUid":"t1""#, r#""trackUid":"t1","layerUid":"L""#);
    let cases = [
        VIDEO.replace(r#""offsetFrames":-3"#, r#""offsetFrames":-3,"note":1"#),
        VIDEO.replace(r#""id":"p1""#, r#""id":"p1","id":"p9""#),
        VIDEO.replace(r#","offsetFrames":-3"#, ""),
        VIDEO.replace(r#""assetId":"v1","#, ""),
        VIDEO.replace(r#""id":"p1","#, ""),
        mixed.to_owned(),
        audio_extra,
        r#"["p1","v1"]"#.to_owned(),
        "7".to_owned(),
        VIDEO.replace(
            r#""target":{"kind":"drawing","contextId":"scene-root","layerUid":"L"}"#,
            r#""target":"drawing""#,
        ),
        VIDEO.replace(r#""id":"p1""#, r#""id":"""#),
        VIDEO.replace(r#""offsetFrames":-3"#, r#""offsetFrames":"-3""#),
    ];
    for (n, entry) in cases.iter().enumerate() {
        let error = decode(&wire(&list(&[entry]))).unwrap_err();
        assert_eq!(error, ProjectWireError::Invalid, "case {n}: {entry}");
    }
    assert_eq!(decode(&wire("{}")).unwrap_err(), ProjectWireError::Invalid);
}

#[test]
fn bad_tables_fail_in_prepare_before_any_binary_allocation() {
    let dangling = VIDEO.replace(r#""assetId":"v1""#, r#""assetId":"nope""#);
    let wrong_kind = VIDEO.replace(r#""assetId":"v1""#, r#""assetId":"a1""#);
    let font_on_layer = VIDEO.replace(r#""assetId":"v1""#, r#""assetId":"f1""#);
    let same_id = VIDEO.replace(r#""layerUid":"L""#, r#""layerUid":"M""#);
    let same_target = VIDEO.replace(r#""id":"p1""#, r#""id":"p7""#);
    let tables = [
        list(&[&dangling]),
        list(&[&wrong_kind]),
        list(&[&font_on_layer]),
        list(&[VIDEO, &same_id]),
        list(&[VIDEO, &same_target]),
    ];
    for (n, table) in tables.iter().enumerate() {
        let prepared = prepare_project_wire(wire(table).as_bytes(), limits());
        assert!(
            matches!(prepared, Err(ProjectWireError::Invalid)),
            "table {n} must fail in prepare"
        );
    }
    // The valid placement alone is fine, so the tables above fail on the table.
    assert!(prepare_project_wire(wire(&list(&[VIDEO])).as_bytes(), limits()).is_ok());
}

#[test]
fn the_placement_table_holds_exactly_1024_entries() {
    let entry = |n: usize| {
        VIDEO
            .replace(r#""id":"p1""#, &format!(r#""id":"p{n}""#))
            .replace(r#""layerUid":"L""#, &format!(r#""layerUid":"L{n}""#))
    };
    let table = |count: usize| {
        let entries: Vec<String> = (0..count).map(entry).collect();
        format!("[{}]", entries.join(","))
    };
    let project = decode(&wire(&table(1024))).unwrap();
    assert_eq!(project.asset_inventory().unwrap().placements().len(), 1024);
    assert_eq!(
        decode(&wire(&table(1025))).unwrap_err(),
        ProjectWireError::Invalid
    );
}

#[test]
fn a_font_placement_has_no_wire_spelling_and_the_writer_refuses_it() {
    let base = decode(&wire("[]")).unwrap();
    let font = base.asset_inventory().unwrap().records()[2].clone();
    assert_eq!(font.kind(), AssetKind::Font);
    let placement = AssetPlacement::new(
        "p3".into(),
        font.id().clone(),
        AssetTarget::Text {
            context_id: "scene-root".into(),
            layer_uid: "L".into(),
            stroke_id: None,
        },
        0,
    )
    .unwrap();
    let records = base.asset_inventory().unwrap().records().to_vec();
    let inventory = AssetInventory::new(records, vec![placement], 3, 1).unwrap();
    let project = base.with_asset_inventory(Some(inventory));
    assert_eq!(
        encode_project(&project, limits()).unwrap_err(),
        ProjectWireError::Unsupported
    );
}

#[test]
fn the_writer_borrows_the_catalog_and_emits_wire_order() {
    let record = Arc::new(
        AssetRecord::new(
            AssetId::new("v9".into()).unwrap(),
            "take-9".into(),
            AssetKind::Video,
            AssetSource::linked("clips/take-9.mov".into(), 4096).unwrap(),
        )
        .unwrap(),
    );
    let placement = AssetPlacement::new("p9".into(), record.id().clone(), drawing("L"), 9).unwrap();
    let inventory = AssetInventory::new(vec![Arc::clone(&record)], vec![placement], 1, 1).unwrap();
    let out = outputs(&inventory).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(
        serde_json::to_string(&out).unwrap(),
        r#"[{"id":"p9","assetId":"v9","target":{"kind":"drawing","contextId":"scene-root","layerUid":"L"},"offsetFrames":9}]"#
    );
    assert_eq!(Arc::strong_count(&record), 2);
}
