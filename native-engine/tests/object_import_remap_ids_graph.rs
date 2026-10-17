//! Import id remap kernel: property loops and the baseline storyboard / layer-uid corpus (L10-G).
use super::rows::{parse, Board, Oracle, ORACLE};
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

/// Module entries in module order, then one component entry per distinct symbol in order of first use.
fn board_entries(board: &Board) -> (Vec<SourceEntry>, Vec<String>) {
    let mut entries: Vec<SourceEntry> = board
        .modules
        .iter()
        .map(|(id, _, _)| SourceEntry::mapped(RemapKind::Module, id))
        .collect();
    let mut symbols: Vec<String> = Vec::new();
    for symbol in board.modules.iter().filter_map(|(_, _, s)| s.as_ref()) {
        if !symbols.contains(symbol) {
            symbols.push(symbol.clone());
        }
    }
    entries.extend(
        symbols
            .iter()
            .map(|id| SourceEntry::mapped(RemapKind::Component, id)),
    );
    (entries, symbols)
}
fn text(list: &[Resolved]) -> Vec<String> {
    list.iter().map(|r| r.as_str().to_owned()).collect()
}
fn refs(list: &[String]) -> Vec<&str> {
    list.iter().map(String::as_str).collect()
}

#[test]
fn every_storyboard_reference_resolves_through_the_batch() {
    let o = origin(7);
    for board in oracle().boards {
        let (entries, symbols) = board_entries(&board);
        let t = table(&o, &entries);
        assert_eq!(
            t.source_count(),
            board.modules.len() + symbols.len(),
            "{}",
            board.case
        );
        for (position, (id, _, _)) in board.modules.iter().enumerate() {
            let want = format!("native-module:8:{position}");
            assert_eq!(t.native_id(RemapKind::Module, id), Some(want.as_str()));
        }
        for (montage, chain) in board.chains.iter().chain(board.audio.iter()) {
            assert!(t.resolve(&o, RemapKind::Module, montage).is_ok());
            let got = t.resolve_all(&o, RemapKind::Module, &refs(chain)).unwrap();
            assert_eq!(got.len(), chain.len());
            assert!(got.iter().all(|r| matches!(r, Resolved::Batch(_))));
            let distinct: HashSet<&str> = got.iter().map(Resolved::as_str).collect();
            assert_eq!(distinct.len(), chain.len(), "{} {montage}", board.case);
        }
        for (_, _, symbol) in &board.modules {
            if let Some(symbol) = symbol {
                assert!(matches!(
                    t.resolve(&o, RemapKind::Component, symbol),
                    Ok(Resolved::Batch(_))
                ));
            }
        }
    }
}

#[test]
fn two_montages_with_shared_symbols_pin_the_exact_native_ids() {
    let o = origin(7);
    let board = oracle()
        .boards
        .into_iter()
        .find(|b| b.case == "twoMontagesSharedSymbols")
        .unwrap();
    assert_eq!(board.next_id, 11);
    let (entries, symbols) = board_entries(&board);
    assert_eq!(symbols, ["sym_a", "sym_b"]);
    let t = table(&o, &entries);
    let native = |kind, id: &str| t.resolve(&o, kind, id).unwrap().as_str().to_owned();
    assert_eq!(native(RemapKind::Module, "sbm1"), "native-module:8:0");
    assert_eq!(native(RemapKind::Module, "sbm2"), "native-module:8:1");
    let chain1 = t
        .resolve_all(&o, RemapKind::Module, &["sbm5", "sbm6"])
        .unwrap();
    let chain2 = t
        .resolve_all(&o, RemapKind::Module, &["sbm7", "sbm8", "sbm9"])
        .unwrap();
    let audio2 = t.resolve_all(&o, RemapKind::Module, &["sbm10"]).unwrap();
    assert_eq!(text(&chain1), ["native-module:8:2", "native-module:8:3"]);
    assert_eq!(
        text(&chain2),
        [
            "native-module:8:4",
            "native-module:8:5",
            "native-module:8:6"
        ]
    );
    assert_eq!(text(&audio2), ["native-module:8:7"]);
    assert_eq!(
        native(RemapKind::Component, "sym_a"),
        "native-component:8:0"
    );
    assert_eq!(
        native(RemapKind::Component, "sym_b"),
        "native-component:8:1"
    );
}

#[test]
fn symbols_outside_the_batch_survive_and_unknown_ones_are_dangling() {
    let board = oracle()
        .boards
        .into_iter()
        .find(|b| b.case == "withAudio")
        .unwrap();
    let modules_only: Vec<SourceEntry> = board_entries(&board)
        .0
        .into_iter()
        .filter(|e| e.kind == RemapKind::Module)
        .collect();
    let known = origin(3).with_ids(RemapKind::Component, ["sym_a", "sym_b"]);
    let t = table(&known, &modules_only);
    assert_eq!(
        t.resolve(&known, RemapKind::Component, "sym_a"),
        Ok(Resolved::Outside("sym_a".to_owned()))
    );
    let unknown = origin(3).with_ids(RemapKind::Component, ["sym_a"]);
    let t = table(&unknown, &modules_only);
    assert_eq!(
        t.resolve(&unknown, RemapKind::Component, "sym_b"),
        Err(RemapError::DanglingReference {
            kind: RemapKind::Component,
            reference: "sym_b".to_owned()
        })
    );
    // A chain naming a module that is not part of the batch fails as a whole at its position.
    let failure = t
        .resolve_all(&unknown, RemapKind::Module, &["sbm1", "sbm2", "sbm99"])
        .unwrap_err();
    assert_eq!(failure.position, 2);
}

#[test]
fn total_source_equals_mapped_plus_skipped_for_every_shape() {
    for n in 0..=40_usize {
        for every in [0_usize, 1, 2, 3, 7] {
            let batch: Vec<SourceEntry> = (0..n)
                .map(|i| {
                    let kind = RemapKind::ALL[i % 5];
                    let id = format!("src-{i}");
                    if every > 0 && i % every == 0 {
                        SourceEntry::skipped(kind, &id, "unsupported")
                    } else {
                        SourceEntry::mapped(kind, &id)
                    }
                })
                .collect();
            let t = table(&layers(rev_for(n), &["L0"]), &batch);
            assert_eq!(t.source_count(), n);
            assert_eq!(t.mapped().len() + t.skipped().len(), n);
            let all: HashSet<&str> = t.mapped().iter().map(|m| m.native.as_str()).collect();
            assert_eq!(all.len(), t.mapped().len(), "native ids are unique");
            for kind in RemapKind::ALL {
                let got = t.native_ids(kind);
                let want: Vec<String> = (0..got.len())
                    .map(|k| format!("native-{}:{}:{k}", kind.tag(), rev_for(n) + 1))
                    .collect();
                assert_eq!(got, want.iter().map(String::as_str).collect::<Vec<_>>());
            }
            for entry in &batch {
                let mapped = t.native_id(entry.kind, &entry.id).is_some();
                let skipped = t.skip_reason(entry.kind, &entry.id).is_some();
                assert!(mapped != skipped, "{} is in exactly one set", entry.id);
            }
        }
    }
}
fn rev_for(n: usize) -> u64 {
    n as u64 * 3
}
#[test]
fn baseline_layer_uids_map_one_to_one_and_a_repeated_uid_is_refused() {
    let o = oracle();
    assert!(o
        .uids
        .iter()
        .all(|uid| valid_id(uid) && uid.starts_with("ly_")));
    let refs: Vec<&str> = o.uids.iter().map(String::as_str).collect();
    let batch = ids(RemapKind::Layer, &refs);
    let t = table(&origin(11), &batch);
    assert_eq!(
        (t.source_count(), t.mapped().len(), t.skipped().len()),
        (40, 40, 0)
    );
    let natives: HashSet<&str> = t.mapped().iter().map(|m| m.native.as_str()).collect();
    assert_eq!(natives.len(), 40);
    assert_eq!(
        t.native_id(RemapKind::Layer, refs[0]),
        Some("native-layer:12:0")
    );
    assert_eq!(
        t.native_id(RemapKind::Layer, refs[39]),
        Some("native-layer:12:39")
    );
    let mut repeated = batch.clone();
    repeated.push(SourceEntry::mapped(RemapKind::Layer, refs[5]));
    assert_eq!(
        build_remap_table(&origin(11), &repeated),
        Err(RemapError::DuplicateSource {
            index: 40,
            first: 5
        })
    );
}
#[test]
fn replaying_a_table_resolves_without_allocating() {
    let o = layers(0, &["old"]);
    let t = table(&o, &ids(RemapKind::Layer, &["a", "b"]));
    let snapshot = t.clone();
    for _ in 0..50 {
        assert_eq!(
            t.resolve(&o, RemapKind::Layer, "a").unwrap(),
            Resolved::Batch("native-layer:1:0".to_owned())
        );
        assert!(t.resolve(&o, RemapKind::Layer, "old").is_ok());
        assert!(t.resolve(&o, RemapKind::Layer, "nope").is_err());
    }
    assert_eq!(t, snapshot);
    assert_eq!(t.mapped().len(), 2);
}
