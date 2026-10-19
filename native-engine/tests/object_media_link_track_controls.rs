//! Negative controls for the placement wire (G42, I16b): the equality the round
//! trip tests rely on must tell apart every authored field, the baseline's
//! default-to-zero collapse must not be imported, and the production file must
//! stay free of panics and `super::` imports.

use super::*;
use crate::object_codec::project::{decode_project, encode_project, ProjectWireLimits};
use crate::object_document::ProjectEnvelope;

const SAVED: &str = r#"{"format":"nemo.native-project-document","formatVersion":2,"project":{"canvasW":320,"canvasH":180,"fps":24},"drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":24,"layers":[{"layerUid":"L"},{"layerUid":"M"}],"objects":[]},"viewportReference":null,"assets":[{"id":"v1","name":"take-1","kind":"video","source":{"kind":"linked","locator":"clips/take-1.mov"}},{"id":"v2","name":"take-2","kind":"video","source":{"kind":"linked","locator":"clips/take-2.mov"}}],"assetInventory":{"assetIds":["v1","v2"],"placements":[{"id":"p1","assetId":"v1","target":{"kind":"drawing","contextId":"scene-root","layerUid":"L","strokeId":"s1"},"offsetFrames":5},{"id":"p2","assetId":"v2","target":{"kind":"drawing","contextId":"scene-root","layerUid":"M"},"offsetFrames":-5}]}}"#;

fn limits() -> ProjectWireLimits {
    ProjectWireLimits::new(1 << 20, 1 << 16).unwrap()
}
fn decode(text: &str) -> ProjectEnvelope {
    decode_project(text.as_bytes(), limits()).unwrap()
}

#[test]
fn the_unmutated_placement_table_round_trips_byte_for_byte() {
    let project = decode(SAVED);
    let saved = encode_project(&project, limits()).unwrap();
    // `fps` is written by the metadata codec (24 -> 24.0), everything else is
    // the placement table this leaf owns.
    let text = String::from_utf8(saved).unwrap();
    let tail = |s: &str| s[s.find("\"assetInventory\"").unwrap()..].to_owned();
    assert_eq!(tail(&text), tail(SAVED));
    assert_eq!(decode(&text).asset_inventory(), project.asset_inventory());
}

#[test]
fn every_authored_field_changes_the_decoded_inventory() {
    let original = decode(SAVED);
    let mutations: [(&str, &str, &str); 7] = [
        ("offset sign", r#""offsetFrames":5"#, r#""offsetFrames":-5"#),
        (
            "offset value",
            r#""offsetFrames":-5"#,
            r#""offsetFrames":-6"#,
        ),
        ("placement id", r#""id":"p2""#, r#""id":"p3""#),
        (
            "asset id",
            r#""assetId":"v2","target""#,
            r#""assetId":"v1","target""#,
        ),
        (
            "layer",
            r#""layerUid":"M"},"offsetFrames""#,
            r#""layerUid":"L"},"offsetFrames""#,
        ),
        ("stroke dropped", r#","strokeId":"s1""#, ""),
        (
            "context",
            r#""contextId":"scene-root","layerUid":"M""#,
            r#""contextId":"scene-2","layerUid":"M""#,
        ),
    ];
    for (name, from, to) in mutations {
        assert_eq!(SAVED.matches(from).count(), 1, "{name}: anchor is unique");
        let mutated = SAVED.replace(from, to);
        // Every mutation keeps the targets distinct, so each one decodes and
        // must show up in the inventory the round trip tests compare.
        let project = decode(&mutated);
        assert_ne!(
            project.asset_inventory(),
            original.asset_inventory(),
            "{name} must be visible in the decoded inventory"
        );
    }
}

#[test]
fn swapping_the_placement_order_is_a_different_inventory() {
    let original = decode(SAVED);
    let first = r#"{"id":"p1","assetId":"v1","target":{"kind":"drawing","contextId":"scene-root","layerUid":"L","strokeId":"s1"},"offsetFrames":5}"#;
    let second = r#"{"id":"p2","assetId":"v2","target":{"kind":"drawing","contextId":"scene-root","layerUid":"M"},"offsetFrames":-5}"#;
    let swapped = SAVED.replace(&format!("{first},{second}"), &format!("{second},{first}"));
    assert_ne!(swapped, SAVED);
    let project = decode(&swapped);
    assert_ne!(project.asset_inventory(), original.asset_inventory());
    let ids: Vec<&str> = project
        .asset_inventory()
        .unwrap()
        .placements()
        .iter()
        .map(|p| p.id())
        .collect();
    assert_eq!(ids, ["p2", "p1"]);
}

#[test]
fn a_zero_default_would_be_a_different_inventory_than_the_refusal() {
    // What a reader copying the baseline's `||0` would build for a missing
    // offset, against the explicit-zero value the wire does accept.
    let explicit = SAVED.replace(r#""offsetFrames":5"#, r#""offsetFrames":0"#);
    let missing = SAVED.replace(r#","offsetFrames":5"#, "");
    assert_ne!(explicit, missing);
    assert_eq!(
        decode(&explicit).asset_inventory().unwrap().placements()[0]
            .offset_frames()
            .as_i64()
            .expect("original integer fixture"),
        0
    );
    assert_eq!(
        decode_project(missing.as_bytes(), limits()).unwrap_err(),
        ProjectWireError::Invalid
    );
}

#[test]
fn production_source_has_no_panics_and_no_super_imports() {
    let source = include_str!("../src/object_media_link_track.rs");
    let production = &source[..source.find("#[cfg(test)]").unwrap()];
    for forbidden in [
        "unwrap(",
        "expect(",
        "panic!",
        "unreachable!",
        "todo!",
        "use super::",
    ] {
        assert!(
            !production.contains(forbidden),
            "production wire code must not contain {forbidden}"
        );
    }
}
