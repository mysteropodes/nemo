//! Value/retention oracles only. No decoder, quota, project/history or IO claim.
use super::*;

fn id(value: &str) -> AssetId {
    AssetId::new(value.to_owned()).unwrap()
}
fn drawing(stroke: &str) -> AssetTarget {
    AssetTarget::Drawing {
        context_id: "scene-root".into(),
        layer_uid: "layer-A".into(),
        stroke_id: Some(stroke.into()),
    }
}
fn text(stroke: &str) -> AssetTarget {
    AssetTarget::Text {
        context_id: "scene-root".into(),
        layer_uid: "text-layer".into(),
        stroke_id: Some(stroke.into()),
    }
}
// Opaque bytes stand in for ALREADY codec-admitted/charged allocations.
// They intentionally do not pretend to be a real image/font/audio decoder test.
fn embedded(mime: &str, bytes: &Arc<Vec<u8>>) -> AssetSource {
    AssetSource::admitted_embedded(mime.into(), Arc::clone(bytes), bytes.capacity()).unwrap()
}
fn record(name: &str, kind: AssetKind, source: AssetSource) -> Arc<AssetRecord> {
    Arc::new(AssetRecord::new(id(name), format!("display {name}"), kind, source).unwrap())
}
fn place(name: &str, asset: &str, target: AssetTarget, offset: i64) -> AssetPlacement {
    AssetPlacement::new(name.into(), id(asset), target, offset).unwrap()
}

#[test]
fn mixed_native_values_preserve_exact_identity_source_order_and_placement() {
    let pixels = Arc::new(vec![10, 20, 30]);
    let font = Arc::new(vec![40, 50]);
    let records = vec![
        record("image-A", AssetKind::Image, embedded("image/png", &pixels)),
        record(
            "video-B",
            AssetKind::Video,
            AssetSource::linked("../media/Clip A.MOV".into(), 200).unwrap(),
        ),
        record(
            "audio-C",
            AssetKind::Audio,
            AssetSource::linked("media/voice.ogg".into(), 200).unwrap(),
        ),
        record("font-D", AssetKind::Font, embedded("font/ttf", &font)),
    ];
    let placements = vec![
        place("place-video", "video-B", drawing("video"), -7),
        place("place-image", "image-A", drawing("image"), 12),
        place(
            "place-audio",
            "audio-C",
            AssetTarget::AudioTrack {
                track_uid: "au1".into(),
            },
            0,
        ),
        place("place-font", "font-D", text("text"), 0),
    ];
    let inventory = AssetInventory::new(records.clone(), placements.clone(), 4, 4).unwrap();
    assert_eq!(
        inventory
            .records()
            .iter()
            .map(|r| r.id().as_str())
            .collect::<Vec<_>>(),
        ["image-A", "video-B", "audio-C", "font-D"]
    );
    assert_eq!(inventory.placements(), placements);
    assert_eq!(
        inventory.get(&id("video-B")).unwrap().source().locator(),
        Some("../media/Clip A.MOV")
    );
    assert_eq!(
        inventory.get(&id("image-A")).unwrap().source().media_type(),
        Some("image/png")
    );
    assert_eq!(
        inventory.get(&id("font-D")).unwrap().kind(),
        AssetKind::Font
    );
    assert_eq!(
        inventory.get(&id("audio-C")).unwrap().name(),
        "display audio-C"
    );
    assert_eq!(inventory.placements()[0].id(), "place-video");
    assert_eq!(inventory.placements()[0].asset_id(), &id("video-B"));
    assert_eq!(inventory.placements()[0].target(), &drawing("video"));
    assert_eq!(
        inventory.placements()[0]
            .offset_frames()
            .as_i64()
            .expect("original integer fixture"),
        -7
    );
    assert_eq!(inventory.encoded_allocations().count(), 2);
    for original in records {
        assert!(Arc::ptr_eq(
            inventory.get(original.id()).unwrap(),
            &original
        ));
    }
}

#[test]
fn ids_and_kind_tags_are_strict_without_normalizing_valid_identity() {
    for invalid in ["", "   ", "a\0b"] {
        assert_eq!(AssetId::new(invalid.into()), Err(AssetError::Id));
    }
    assert_eq!(AssetId::new("a".repeat(256)), Err(AssetError::Id));
    assert!(AssetId::new("a".repeat(255)).is_ok());
    assert_eq!(id(" Asset:É ").as_str(), " Asset:É ");
    assert_ne!(id("Asset"), id("asset"));
    for (tag, expected) in [
        ("image", AssetKind::Image),
        ("video", AssetKind::Video),
        ("audio", AssetKind::Audio),
        ("font", AssetKind::Font),
    ] {
        assert_eq!(AssetKind::from_tag(tag), Ok(expected));
    }
    for tag in ["", "Image", "scene", "stillImage", "audio/video"] {
        assert_eq!(AssetKind::from_tag(tag), Err(AssetError::UnsupportedKind));
    }
}

#[test]
fn source_limits_use_retained_capacity_and_preserve_literal_locator_metadata() {
    let mut bytes = Vec::with_capacity(32);
    bytes.extend([1, 2]);
    let bytes = Arc::new(bytes);
    assert_eq!(
        AssetSource::admitted_embedded("font/ttf".into(), Arc::clone(&bytes), 31),
        Err(AssetError::Limit)
    );
    let source = embedded("font/ttf", &bytes);
    assert!(Arc::ptr_eq(source.encoded_allocation().unwrap(), &bytes));
    assert_eq!(source.encoded_allocation().unwrap().capacity(), 32);
    for mime in [
        "",
        "png",
        "image/",
        "/png",
        "image/png/x",
        "image/png; charset=x",
    ] {
        assert_eq!(
            AssetSource::admitted_embedded(mime.into(), Arc::clone(&bytes), 32),
            Err(AssetError::Source)
        );
    }
    assert_eq!(
        AssetSource::admitted_embedded("image/png".into(), Arc::new(vec![]), 32),
        Err(AssetError::Limit)
    );
    assert_eq!(
        AssetSource::admitted_embedded("image/png".into(), Arc::clone(&bytes), 0),
        Err(AssetError::Limit)
    );
    for locator in ["", "a\0b", "12345"] {
        assert_eq!(
            AssetSource::linked(locator.into(), 4),
            Err(AssetError::Source)
        );
    }
    let linked = AssetSource::linked("../Media/A B.wav".into(), 100).unwrap();
    assert_eq!(linked.locator(), Some("../Media/A B.wav"));
    assert!(linked.encoded_allocation().is_none());
    assert!(linked.media_type().is_none());
}

#[test]
fn placements_refuse_invalid_holder_identity_and_unsafe_frame_offsets() {
    for target in [
        drawing(""),
        AssetTarget::AudioTrack {
            track_uid: "".into(),
        },
        AssetTarget::Text {
            context_id: "".into(),
            layer_uid: "l".into(),
            stroke_id: None,
        },
    ] {
        assert_eq!(
            AssetPlacement::new("p".into(), id("a"), target, 0),
            Err(AssetError::Placement)
        );
    }
    assert_eq!(
        AssetPlacement::new("".into(), id("a"), drawing("x"), 0),
        Err(AssetError::Placement)
    );
    for offset in [
        i64::MIN,
        i64::MAX,
        9_007_199_254_740_992,
        -9_007_199_254_740_992,
    ] {
        assert_eq!(
            AssetPlacement::new("p".into(), id("a"), drawing("x"), offset),
            Err(AssetError::Placement)
        );
    }
    for offset in [0, 9_007_199_254_740_991, -9_007_199_254_740_991] {
        assert!(AssetPlacement::new("p".into(), id("a"), drawing("x"), offset).is_ok());
    }
}

#[test]
fn duplicate_missing_and_kind_mismatched_references_cannot_enter_an_inventory() {
    let bytes = Arc::new(vec![1]);
    let image = record("a", AssetKind::Image, embedded("image/png", &bytes));
    assert_eq!(
        AssetInventory::new(vec![Arc::clone(&image), Arc::clone(&image)], vec![], 2, 0),
        Err(AssetError::DuplicateAsset)
    );
    assert_eq!(
        AssetInventory::new(
            vec![Arc::clone(&image)],
            vec![place("p", "missing", drawing("s"), 0)],
            1,
            1
        ),
        Err(AssetError::MissingAsset)
    );
    assert_eq!(
        AssetInventory::new(
            vec![Arc::clone(&image)],
            vec![
                place("p", "a", drawing("s"), 0),
                place("p", "a", drawing("other"), 1)
            ],
            1,
            2
        ),
        Err(AssetError::DuplicatePlacement)
    );
    assert_eq!(
        AssetInventory::new(
            vec![Arc::clone(&image)],
            vec![
                place("p", "a", drawing("s"), 0),
                place("other", "a", drawing("s"), 1)
            ],
            1,
            2
        ),
        Err(AssetError::DuplicatePlacement)
    );
    assert_eq!(
        AssetInventory::new(vec![image], vec![place("p", "a", text("s"), 0)], 1, 1),
        Err(AssetError::KindMismatch)
    );
    for (kind, target) in [
        (AssetKind::Font, drawing("s")),
        (AssetKind::Audio, drawing("s")),
        (
            AssetKind::Video,
            AssetTarget::AudioTrack {
                track_uid: "a".into(),
            },
        ),
    ] {
        let asset = record(
            "a",
            kind,
            AssetSource::linked("retained/source".into(), 100).unwrap(),
        );
        assert_eq!(
            AssetInventory::new(vec![asset], vec![place("p", "a", target, 0)], 1, 1),
            Err(AssetError::KindMismatch)
        );
    }
}

#[test]
fn caller_count_limits_refuse_without_changing_an_existing_inventory() {
    let a = record(
        "a",
        AssetKind::Image,
        AssetSource::linked("media/a".into(), 100).unwrap(),
    );
    let original = AssetInventory::new(vec![Arc::clone(&a)], vec![], 1, 0).unwrap();
    assert_eq!(
        AssetInventory::new(vec![Arc::clone(&a)], vec![], 0, 0),
        Err(AssetError::Limit)
    );
    assert_eq!(
        AssetInventory::new(
            vec![Arc::clone(&a)],
            vec![place("p", "a", drawing("s"), 0)],
            1,
            0
        ),
        Err(AssetError::Limit)
    );
    assert_eq!(original.records().len(), 1);
    assert!(Arc::ptr_eq(original.get(&id("a")).unwrap(), &a));
    assert!(original.placements().is_empty());
    assert!(AssetInventory::new(vec![], vec![], 0, 0).is_ok());
}

#[test]
fn old_values_share_allocations_and_release_only_after_the_last_retained_snapshot() {
    let bytes = Arc::new(vec![1, 2, 3]);
    let weak = Arc::downgrade(&bytes);
    let asset = record("a", AssetKind::Image, embedded("image/png", &bytes));
    let original = AssetInventory::new(vec![Arc::clone(&asset)], vec![], 1, 0).unwrap();
    let pinned = original.clone();
    let replacement_bytes = Arc::new(vec![9, 8]);
    let replacement = record(
        "a",
        AssetKind::Image,
        embedded("image/png", &replacement_bytes),
    );
    let next = AssetInventory::new(vec![replacement], vec![], 1, 0).unwrap();
    assert!(Arc::ptr_eq(
        pinned
            .get(&id("a"))
            .unwrap()
            .source()
            .encoded_allocation()
            .unwrap(),
        &bytes
    ));
    assert!(Arc::ptr_eq(
        next.get(&id("a"))
            .unwrap()
            .source()
            .encoded_allocation()
            .unwrap(),
        &replacement_bytes
    ));
    let removed = AssetInventory::new(vec![], vec![], 0, 0).unwrap();
    assert!(removed.get(&id("a")).is_none());
    drop(original);
    drop(asset);
    drop(bytes);
    assert!(weak.upgrade().is_some());
    drop(pinned);
    assert!(weak.upgrade().is_none());
}

#[test]
fn repeated_asset_references_expose_the_same_allocation_to_the_existing_owner_ledger() {
    let bytes = Arc::new(vec![1, 2]);
    let a = record("a", AssetKind::Image, embedded("image/png", &bytes));
    let b = record("b", AssetKind::Image, embedded("image/png", &bytes));
    let inventory = AssetInventory::new(vec![a, b], vec![], 2, 0).unwrap();
    let allocations: Vec<_> = inventory.encoded_allocations().collect();
    assert_eq!(allocations.len(), 2);
    assert!(allocations
        .iter()
        .all(|allocation| Arc::ptr_eq(allocation, &bytes)));
    // Do not sum these as two charges: pointer identity deduplication and
    // preallocation are the SAME existing owner ledger's mandatory next join.
}
