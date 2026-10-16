//! Import id remap kernel: literal pins and baseline oracle rows (L10-G).
use super::rows::{parse, Oracle, ORACLE};
use super::*;

fn oracle() -> Oracle {
    parse(ORACLE)
}
fn origin(revision: u64) -> OriginIds {
    OriginIds::new(revision)
}
fn layers(revision: u64, ids: &[&str]) -> OriginIds {
    origin(revision).with_ids(RemapKind::Layer, ids.iter().copied())
}
fn table(origin: &OriginIds, entries: &[SourceEntry]) -> RemapTable {
    build_remap_table(origin, entries).unwrap()
}
fn ids(kind: RemapKind, sources: &[&str]) -> Vec<SourceEntry> {
    sources
        .iter()
        .map(|id| SourceEntry::mapped(kind, id))
        .collect()
}

#[test]
fn oracle_row_counts_are_pinned() {
    let o = oracle();
    assert_eq!(o.figma.len(), 40);
    assert_eq!(o.psd.len(), 40);
    assert_eq!(o.layer_names.len(), 10);
    assert_eq!(o.uids.len(), 40);
    assert_eq!(o.boards.len(), 5);
}

#[test]
fn figma_frame_names_match_every_baseline_row() {
    for (name, index, expected) in oracle().figma {
        assert_eq!(
            figma_frame_layer_name(name.as_deref(), index),
            expected,
            "{name:?} {index}"
        );
    }
}

#[test]
fn psd_layer_names_match_every_baseline_row() {
    for (name, imported, expected) in oracle().psd {
        assert_eq!(
            psd_layer_name(name.as_deref(), imported),
            expected,
            "{name:?} {imported}"
        );
    }
}

#[test]
fn next_layer_name_matches_every_baseline_row() {
    for (names, expected) in oracle().layer_names {
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        assert_eq!(next_layer_name(&refs), expected, "{names:?}");
    }
}

#[test]
fn name_default_pins_keep_the_baseline_and_never_uniquify() {
    assert_eq!(figma_frame_layer_name(None, 0), "Figma 1");
    assert_eq!(figma_frame_layer_name(Some(""), 2), "Figma 3");
    assert_eq!(figma_frame_layer_name(Some(" "), 2), " ");
    assert_eq!(figma_frame_layer_name(Some("0"), 2), "0");
    assert_eq!(psd_layer_name(None, 0), "Calque PSD 1");
    assert_eq!(psd_layer_name(Some(""), 6), "Calque PSD 7");
    // Imported names are used as they come: two frames named alike stay alike (baseline).
    let a = figma_frame_layer_name(Some("Frame A"), 0);
    let b = figma_frame_layer_name(Some("Frame A"), 1);
    assert_eq!((a.as_str(), b.as_str()), ("Frame A", "Frame A"));
    assert_eq!(next_layer_name(&["Layer 1", "Layer 3"]), "Layer 2");
    assert_eq!(next_layer_name(&[]), "Layer 1");
}

#[test]
fn id_scheme_pins_follow_the_layer_create_owner() {
    assert_eq!(
        allocate_ids(&layers(0, &["L0"]), RemapKind::Layer, 2).unwrap(),
        ["native-layer:1:0", "native-layer:1:1"]
    );
    // The owner's own literal: an imported id already holds suffix 0 at revision 1.
    assert_eq!(
        allocate_ids(&layers(0, &["L0", "native-layer:1:0"]), RemapKind::Layer, 1).unwrap(),
        ["native-layer:1:1"]
    );
    assert_eq!(
        allocate_ids(&layers(4, &[]), RemapKind::Layer, 1).unwrap(),
        ["native-layer:5:0"]
    );
    for (kind, tag) in [
        (RemapKind::Stroke, "stroke"),
        (RemapKind::Component, "component"),
        (RemapKind::Module, "module"),
        (RemapKind::Asset, "asset"),
    ] {
        assert_eq!(kind.tag(), tag);
        assert_eq!(
            allocate_ids(&origin(9), kind, 2).unwrap(),
            [format!("native-{tag}:10:0"), format!("native-{tag}:10:1")]
        );
    }
}

#[test]
fn probing_takes_the_first_free_suffix_and_counts_only_its_kind() {
    let o = layers(
        0,
        &["native-layer:1:0", "native-layer:1:1", "native-layer:1:3"],
    )
    .with_ids(RemapKind::Stroke, ["native-stroke:1:0"]);
    assert_eq!(
        allocate_ids(&o, RemapKind::Layer, 3).unwrap(),
        ["native-layer:1:2", "native-layer:1:4", "native-layer:1:5"]
    );
    assert_eq!(
        allocate_ids(&o, RemapKind::Stroke, 1).unwrap(),
        ["native-stroke:1:1"]
    );
    assert_eq!(
        allocate_ids(&o, RemapKind::Module, 1).unwrap(),
        ["native-module:1:0"]
    );
    assert_eq!(
        (o.count(RemapKind::Layer), o.count(RemapKind::Asset)),
        (3, 0)
    );
}

#[test]
fn revision_overflow_is_a_typed_refusal() {
    let max = 9_007_199_254_740_991_u64;
    assert_eq!(
        build_remap_table(&origin(max), &[]),
        Err(RemapError::RevisionExhausted)
    );
    assert_eq!(
        allocate_ids(&origin(u64::MAX), RemapKind::Layer, 1),
        Err(RemapError::RevisionExhausted)
    );
    assert_eq!(
        allocate_ids(&origin(max - 1), RemapKind::Layer, 1).unwrap(),
        ["native-layer:9007199254740991:0"]
    );
}

#[test]
fn every_kind_maps_one_to_one_with_an_explicit_skipped_set() {
    let o = layers(7, &["L0"]);
    let mut batch = ids(RemapKind::Layer, &["ly_a", "ly_b"]);
    batch.extend(ids(RemapKind::Stroke, &["s1", "s2", "s3"]));
    batch.push(SourceEntry::mapped(RemapKind::Component, "sym_a"));
    batch.extend(ids(RemapKind::Module, &["sbm1", "sbm2"]));
    batch.push(SourceEntry::mapped(RemapKind::Asset, &psd_layer_key(0)));
    batch.push(SourceEntry::skipped(
        RemapKind::Asset,
        &psd_layer_key(1),
        "hidden",
    ));
    batch.push(SourceEntry::mapped(
        RemapKind::Asset,
        &figma_image_key("h4sh"),
    ));
    batch.push(SourceEntry::mapped(RemapKind::Asset, &dropped_file_key(2)));
    let t = table(&o, &batch);
    assert_eq!(t.next_revision(), 8);
    assert_eq!(t.source_count(), 12);
    assert_eq!((t.mapped().len(), t.skipped().len()), (11, 1));
    assert_eq!(
        t.native_id(RemapKind::Layer, "ly_a"),
        Some("native-layer:8:0")
    );
    assert_eq!(
        t.native_id(RemapKind::Layer, "ly_b"),
        Some("native-layer:8:1")
    );
    assert_eq!(
        t.native_id(RemapKind::Stroke, "s3"),
        Some("native-stroke:8:2")
    );
    assert_eq!(
        t.native_id(RemapKind::Component, "sym_a"),
        Some("native-component:8:0")
    );
    assert_eq!(
        t.native_id(RemapKind::Module, "sbm2"),
        Some("native-module:8:1")
    );
    assert_eq!(
        t.native_id(RemapKind::Asset, "psd-layer:0"),
        Some("native-asset:8:0")
    );
    assert_eq!(t.native_id(RemapKind::Asset, "psd-layer:1"), None);
    assert_eq!(
        t.skip_reason(RemapKind::Asset, "psd-layer:1"),
        Some("hidden")
    );
    assert_eq!(
        t.native_id(RemapKind::Asset, "figma-image:h4sh"),
        Some("native-asset:8:1")
    );
    assert_eq!(
        t.native_id(RemapKind::Asset, "file:2"),
        Some("native-asset:8:2")
    );
    // The same text under another kind is another entry.
    assert_eq!(t.native_id(RemapKind::Stroke, "ly_a"), None);
}

#[test]
fn skipped_entries_consume_no_id() {
    let batch = vec![
        SourceEntry::mapped(RemapKind::Layer, "a"),
        SourceEntry::skipped(RemapKind::Layer, "skipped", "no geometry"),
        SourceEntry::mapped(RemapKind::Layer, "b"),
    ];
    let t = table(&origin(0), &batch);
    assert_eq!(t.native_id(RemapKind::Layer, "a"), Some("native-layer:1:0"));
    assert_eq!(t.native_id(RemapKind::Layer, "b"), Some("native-layer:1:1"));
}

#[test]
fn references_rewrite_inside_the_batch_and_survive_outside_it() {
    let o = layers(2, &["ly_old", "parent é "]).with_ids(RemapKind::Asset, ["asset-1"]);
    let t = table(
        &o,
        &[
            SourceEntry::mapped(RemapKind::Layer, "ly_new"),
            SourceEntry::skipped(RemapKind::Layer, "ly_skip", "locked"),
        ],
    );
    let rewritten = t.resolve(&o, RemapKind::Layer, "ly_new").unwrap();
    assert_eq!(rewritten, Resolved::Batch("native-layer:3:0".to_owned()));
    let kept = t.resolve(&o, RemapKind::Layer, "parent é ").unwrap();
    assert_eq!(kept, Resolved::Outside("parent é ".to_owned()));
    assert_eq!(kept.as_str().as_bytes(), "parent é ".as_bytes());
    assert_eq!(
        t.resolve(&o, RemapKind::Asset, "asset-1").unwrap(),
        Resolved::Outside("asset-1".to_owned())
    );
    assert_eq!(
        t.resolve(&o, RemapKind::Layer, "ly_missing"),
        Err(RemapError::DanglingReference {
            kind: RemapKind::Layer,
            reference: "ly_missing".to_owned()
        })
    );
    // Ids are per kind: an origin layer id is not an origin asset id.
    assert!(t.resolve(&o, RemapKind::Asset, "ly_old").is_err());
    assert_eq!(
        t.resolve(&o, RemapKind::Layer, "ly_skip"),
        Err(RemapError::ReferenceToSkipped {
            kind: RemapKind::Layer,
            reference: "ly_skip".to_owned()
        })
    );
}

#[test]
fn absent_references_stay_absent_and_lists_are_all_or_nothing() {
    let o = layers(0, &["keep"]);
    let t = table(&o, &ids(RemapKind::Layer, &["a", "b"]));
    assert_eq!(t.resolve_optional(&o, RemapKind::Layer, None), Ok(None));
    assert_eq!(
        t.resolve_optional(&o, RemapKind::Layer, Some("b")),
        Ok(Some(Resolved::Batch("native-layer:1:1".to_owned())))
    );
    let ok = t
        .resolve_all(&o, RemapKind::Layer, &["b", "keep", "a"])
        .unwrap();
    let got: Vec<&str> = ok.iter().map(Resolved::as_str).collect();
    assert_eq!(got, ["native-layer:1:1", "keep", "native-layer:1:0"]);
    let failure = t
        .resolve_all(&o, RemapKind::Layer, &["a", "keep", "nope", "also-nope"])
        .unwrap_err();
    assert_eq!(failure.position, 2);
    assert_eq!(
        failure.error,
        RemapError::DanglingReference {
            kind: RemapKind::Layer,
            reference: "nope".to_owned()
        }
    );
    assert_eq!(t.resolve_all(&o, RemapKind::Layer, &[]), Ok(vec![]));
}

#[test]
fn invalid_duplicate_colliding_and_unexplained_entries_are_typed_errors() {
    let o = layers(3, &["taken"]).with_ids(RemapKind::Stroke, ["x"]);
    let bad = |entries: Vec<SourceEntry>| build_remap_table(&o, &entries).unwrap_err();
    let l = RemapKind::Layer;
    assert_eq!(
        bad(ids(l, &["ok", ""])),
        RemapError::InvalidSourceId { index: 1 }
    );
    assert_eq!(
        bad(ids(l, &["  "])),
        RemapError::InvalidSourceId { index: 0 }
    );
    assert_eq!(
        bad(ids(l, &["a\0b"])),
        RemapError::InvalidSourceId { index: 0 }
    );
    let long = "x".repeat(256);
    assert_eq!(
        bad(ids(l, &["a", long.as_str()])),
        RemapError::InvalidSourceId { index: 1 }
    );
    let edge = "y".repeat(255);
    assert!(build_remap_table(&o, &ids(l, &[edge.as_str()])).is_ok());
    assert!(
        !valid_id(&"\u{e9}".repeat(128)),
        "255-byte limit counts bytes"
    );
    assert_eq!(
        bad(ids(l, &["a", "b", "a"])),
        RemapError::DuplicateSource { index: 2, first: 0 }
    );
    assert_eq!(
        bad(ids(l, &["taken"])),
        RemapError::CollidesWithOrigin { index: 0 }
    );
    // Same text under a different kind is neither a duplicate nor a collision.
    let mixed = vec![
        SourceEntry::mapped(RemapKind::Layer, "z"),
        SourceEntry::mapped(RemapKind::Stroke, "z"),
        SourceEntry::mapped(RemapKind::Layer, "x"),
    ];
    assert_eq!(build_remap_table(&o, &mixed).unwrap().mapped().len(), 3);
    assert_eq!(
        bad(vec![SourceEntry::mapped(RemapKind::Stroke, "x")]),
        RemapError::CollidesWithOrigin { index: 0 }
    );
    assert_eq!(
        bad(vec![SourceEntry::skipped(l, "q", "  ")]),
        RemapError::EmptySkipReason { index: 0 }
    );
    assert_eq!(
        bad(vec![SourceEntry::skipped(l, "taken", "why")]),
        RemapError::CollidesWithOrigin { index: 0 }
    );
    // Entries are checked in order: the first failing one names the error.
    assert_eq!(
        bad(ids(l, &["ok", "\0", "ok"])),
        RemapError::InvalidSourceId { index: 1 }
    );
}

#[test]
fn a_refused_batch_leaves_origin_and_later_allocation_untouched() {
    let o = layers(5, &["taken", "native-layer:6:0"]);
    let before = o.clone();
    let good = ids(RemapKind::Layer, &["a", "b"]);
    let first = table(&o, &good);
    assert!(build_remap_table(&o, &ids(RemapKind::Layer, &["a", "a"])).is_err());
    assert!(build_remap_table(&o, &ids(RemapKind::Layer, &["taken"])).is_err());
    assert_eq!(o, before);
    assert_eq!(table(&o, &good), first, "a refusal consumed no id");
    assert_eq!(
        first.native_id(RemapKind::Layer, "a"),
        Some("native-layer:6:1")
    );
}

#[test]
fn same_origin_and_order_give_the_same_table_and_reorder_permutes_suffixes() {
    let o = layers(1, &["native-layer:2:1"]);
    let forward = ids(RemapKind::Layer, &["a", "b", "c", "d"]);
    let mut backward = forward.clone();
    backward.reverse();
    let t1 = table(&o, &forward);
    assert_eq!(table(&o, &forward), t1);
    let t2 = table(&o, &backward);
    let mut left = t1.native_ids(RemapKind::Layer);
    let mut right = t2.native_ids(RemapKind::Layer);
    left.sort_unstable();
    right.sort_unstable();
    assert_eq!(left, right, "same ids, different assignment");
    assert_eq!(
        t1.native_ids(RemapKind::Layer),
        [
            "native-layer:2:0",
            "native-layer:2:2",
            "native-layer:2:3",
            "native-layer:2:4"
        ]
    );
    assert_eq!(
        t2.native_id(RemapKind::Layer, "d"),
        Some("native-layer:2:0")
    );
    assert_eq!(
        t2.native_id(RemapKind::Layer, "a"),
        Some("native-layer:2:4")
    );
    // Interleaving other kinds changes nothing for a kind.
    let mut mixed = Vec::new();
    for id in ["a", "b", "c", "d"] {
        mixed.push(SourceEntry::mapped(RemapKind::Module, id));
        mixed.push(SourceEntry::mapped(RemapKind::Layer, id));
    }
    assert_eq!(
        table(&o, &mixed).native_ids(RemapKind::Layer),
        t1.native_ids(RemapKind::Layer)
    );
}

#[test]
fn batch_table_equals_source_free_allocation() {
    let o = layers(3, &["native-layer:4:1"]);
    let t = table(&o, &ids(RemapKind::Layer, &["p", "q", "r"]));
    let free = allocate_ids(&o, RemapKind::Layer, 3).unwrap();
    assert_eq!(
        t.native_ids(RemapKind::Layer),
        free.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert_eq!(
        allocate_ids(&o, RemapKind::Layer, 0).unwrap(),
        Vec::<String>::new()
    );
}
