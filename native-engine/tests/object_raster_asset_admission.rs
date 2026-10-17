//! Decision tests of the raster/media asset admission kernel (L06-D).
use super::oracle::{
    content_digest, embedded_record, empty, hex, inventory, png_like, refused, req, rows, samples,
    unhex,
};
use super::*;
use crate::object_document::{AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource};
use std::cell::Cell;
use std::sync::Arc;

const PNG_LIKE_64_SHA256: &str = "f82aca3ee5c0519c2269fcb5c6a964a0203311dd9f809ea2404307ddef647ad3";

fn weak_digest(bytes: &[u8]) -> [u8; 32] {
    [bytes.len() as u8; 32]
}
fn valid_2x2(_: &[u8]) -> PngCheck {
    PngCheck::Valid {
        width: 2,
        height: 2,
    }
}
fn decide_stub(request: &AdmissionRequest<'_>, inv: &AssetInventory) -> AdmissionDecision {
    let ports = AdmissionPorts {
        digest: &content_digest,
        png: &valid_2x2,
    };
    decide(request, inv, &ports)
}

#[test]
fn a_fresh_png_is_new_with_every_field_pinned() {
    let bytes = png_like(1, 64);
    let decision = decide_stub(&req("req-1", "pic.png", "image/png", &bytes), &empty());
    let AdmissionDecision::New(asset) = decision else {
        panic!("expected New");
    };
    assert_eq!(asset.id.as_str(), "asset-req-1");
    assert_eq!(asset.name, "pic.png");
    assert_eq!(asset.kind, AssetKind::Image);
    assert_eq!(asset.media_type, "image/png");
    assert_eq!(hex(&asset.digest), PNG_LIKE_64_SHA256);
    assert_eq!(asset.dimensions, Some((2, 2)));
}

#[test]
fn sha256_oracle_rows_match_node_crypto() {
    let table = rows("sha256");
    assert_eq!(table.len(), 11);
    for row in &table {
        let bytes = unhex(row["hex"].as_str().unwrap());
        assert_eq!(bytes.len() as u64, row["len"].as_u64().unwrap());
        assert_eq!(
            hex(&content_digest(&bytes)),
            row["sha256"].as_str().unwrap()
        );
    }
    let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    assert_eq!(hex(&content_digest(b"abc")), abc);
}

#[test]
fn equal_bytes_under_another_name_reuse_the_id_and_the_allocation() {
    let bytes = png_like(1, 64);
    let held = embedded_record("asset-first", "first.png", "image/png", bytes.clone());
    let inv = inventory(vec![held.clone()]);
    let decision = decide_stub(&req("req-2", "second.png", "image/png", &bytes), &inv);
    let AdmissionDecision::Duplicate {
        existing,
        allocation,
    } = decision
    else {
        panic!("expected Duplicate");
    };
    assert_eq!(existing.as_str(), "asset-first");
    let held_allocation = held.source().encoded_allocation().unwrap();
    assert!(Arc::ptr_eq(&allocation, held_allocation));
}

#[test]
fn same_digest_but_different_bytes_is_not_a_duplicate() {
    let held = png_like(1, 64);
    let incoming = png_like(2, 64);
    assert_eq!(weak_digest(&held), weak_digest(&incoming));
    let inv = inventory(vec![embedded_record("asset-a", "a.png", "image/png", held)]);
    let ports = AdmissionPorts {
        digest: &weak_digest,
        png: &valid_2x2,
    };
    let decision = decide(&req("req-3", "b.png", "image/png", &incoming), &inv, &ports);
    assert!(
        matches!(decision, AdmissionDecision::New(_)),
        "{decision:?}"
    );
}

#[test]
fn a_replayed_request_resolves_to_the_asset_it_created() {
    let bytes = png_like(3, 40);
    let request = req("req-4", "pic.png", "image/png", &bytes);
    let AdmissionDecision::New(created) = decide_stub(&request, &empty()) else {
        panic!("expected New");
    };
    let committed = embedded_record(created.id.as_str(), "pic.png", "image/png", bytes.clone());
    let again = decide_stub(&request, &inventory(vec![committed]));
    match again {
        AdmissionDecision::Duplicate { existing, .. } => assert_eq!(existing, created.id),
        other => panic!("expected Duplicate, got {other:?}"),
    }
    assert_eq!(
        decide_stub(&request, &empty()),
        AdmissionDecision::New(created)
    );
}

#[test]
fn minting_is_pure_and_a_collision_is_refused_not_suffixed() {
    assert_eq!(mint_asset_id("r1").unwrap(), mint_asset_id("r1").unwrap());
    assert_eq!(mint_asset_id("r1").unwrap().as_str(), "asset-r1");
    assert_ne!(mint_asset_id("r1").unwrap(), mint_asset_id("r2").unwrap());
    let long = "x".repeat(250);
    for bad in ["", "   ", "a\0b", long.as_str()] {
        assert_eq!(mint_asset_id(bad), Err(Refusal::RequestId), "{bad:?}");
    }
    assert_eq!(mint_asset_id(&"x".repeat(249)).unwrap().as_str().len(), 255);
    let other = embedded_record("asset-req-5", "o.png", "image/png", png_like(9, 40));
    let bytes = png_like(4, 40);
    let decision = decide_stub(
        &req("req-5", "p.png", "image/png", &bytes),
        &inventory(vec![other]),
    );
    assert_eq!(refused(decision), Refusal::IdCollision);
}

#[test]
fn a_full_catalog_refuses_new_assets_but_still_resolves_duplicates() {
    let mut records = Vec::new();
    for i in 0..MAX_ASSETS {
        let bytes = png_like((i % 251) as u8, 16 + i / 251);
        records.push(embedded_record(
            &format!("a{i}"),
            "n.png",
            "image/png",
            bytes,
        ));
    }
    let inv = inventory(records);
    assert_eq!(inv.records().len(), MAX_ASSETS);
    let fresh = png_like(7, 4000);
    assert_eq!(
        refused(decide_stub(&req("r", "n.png", "image/png", &fresh), &inv)),
        Refusal::CatalogFull
    );
    let member = png_like(0, 16);
    let AdmissionDecision::Duplicate { existing, .. } =
        decide_stub(&req("r", "n.png", "image/png", &member), &inv)
    else {
        panic!("expected Duplicate");
    };
    assert_eq!(existing.as_str(), "a0");
}

#[test]
fn a_duplicate_never_reaches_the_png_port_but_a_fresh_input_does() {
    let bytes = png_like(9, 40);
    let held = embedded_record("asset-held", "held.png", "image/png", bytes.clone());
    let inv = inventory(vec![held.clone()]);
    let calls = Cell::new(0u32);
    let corrupt = |_: &[u8]| {
        calls.set(calls.get() + 1);
        PngCheck::Corrupt
    };
    let ports = AdmissionPorts {
        digest: &content_digest,
        png: &corrupt,
    };
    let AdmissionDecision::Duplicate {
        existing,
        allocation,
    } = decide(&req("req-d", "copy.png", "image/png", &bytes), &inv, &ports)
    else {
        panic!("expected Duplicate");
    };
    assert_eq!(existing.as_str(), "asset-held");
    assert!(Arc::ptr_eq(
        &allocation,
        held.source().encoded_allocation().unwrap()
    ));
    assert_eq!(
        calls.get(),
        0,
        "an equal admitted asset is not decoded again"
    );
    let fresh = png_like(8, 40);
    let decision = decide(&req("req-e", "new.png", "image/png", &fresh), &inv, &ports);
    assert_eq!(refused(decision), Refusal::Corrupt);
    assert_eq!(calls.get(), 1, "a non-duplicate is checked exactly once");
}

#[test]
fn declared_type_matches_case_insensitively_and_exactly() {
    let bytes = png_like(1, 32);
    let go = |declared: &str| decide_stub(&req("r", "n.png", declared, &bytes), &empty());
    assert!(matches!(go("IMAGE/PNG"), AdmissionDecision::New(_)));
    let mismatch = Refusal::TypeMismatch {
        sniffed: "image/png",
    };
    for declared in [
        "image/jpeg",
        "",
        "image/png; charset=x",
        " image/png",
        "image/*",
    ] {
        assert_eq!(refused(go(declared)), mismatch, "{declared:?}");
    }
}

#[test]
fn every_recognised_non_png_is_a_typed_decoder_unavailable() {
    for (media_type, kind, bytes) in samples() {
        if media_type == "image/png" {
            continue;
        }
        let calls = Cell::new(0u32);
        let counting = |_: &[u8]| {
            calls.set(calls.get() + 1);
            PngCheck::Unavailable
        };
        let ports = AdmissionPorts {
            digest: &content_digest,
            png: &counting,
        };
        let ok = decide(&req("r", "n.bin", media_type, &bytes), &empty(), &ports);
        assert_eq!(
            refused(ok),
            Refusal::DecoderUnavailable { media_type },
            "{kind:?}"
        );
        assert_eq!(calls.get(), 0, "{media_type}: the PNG port must not run");
        let wrong = decide(&req("r", "n.bin", "image/png", &bytes), &empty(), &ports);
        assert_eq!(
            refused(wrong),
            Refusal::TypeMismatch {
                sniffed: media_type
            }
        );
    }
}

#[test]
fn empty_oversize_and_limit_sized_inputs_take_distinct_paths() {
    let digests = Cell::new(0u32);
    let pngs = Cell::new(0u32);
    let digest = |b: &[u8]| {
        digests.set(digests.get() + 1);
        weak_digest(b)
    };
    let png = |_: &[u8]| {
        pngs.set(pngs.get() + 1);
        PngCheck::Valid {
            width: 1,
            height: 1,
        }
    };
    let ports = AdmissionPorts {
        digest: &digest,
        png: &png,
    };
    let go = |bytes: &[u8]| decide(&req("r", "n.png", "image/png", bytes), &empty(), &ports);
    assert_eq!(refused(go(&[][..])), Refusal::Empty);
    let over = png_like(0, MAX_ENCODED_BYTES + 1);
    assert_eq!(refused(go(over.as_slice())), Refusal::Oversize);
    assert_eq!(
        (digests.get(), pngs.get()),
        (0, 0),
        "no work before the gate"
    );
    let at_limit = png_like(0, MAX_ENCODED_BYTES);
    assert!(matches!(go(at_limit.as_slice()), AdmissionDecision::New(_)));
    assert_eq!((digests.get(), pngs.get()), (1, 1));
}

#[test]
fn port_results_map_to_distinct_refusals_and_dimensions_are_rechecked() {
    let bytes = png_like(5, 32);
    let go = |check: PngCheck| {
        let png = move |_: &[u8]| check;
        let ports = AdmissionPorts {
            digest: &content_digest,
            png: &png,
        };
        decide(&req("r", "n.png", "image/png", &bytes), &empty(), &ports)
    };
    let valid = |width, height| PngCheck::Valid { width, height };
    assert_eq!(refused(go(PngCheck::Corrupt)), Refusal::Corrupt);
    assert_eq!(
        refused(go(PngCheck::Unsupported)),
        Refusal::UnsupportedEncoding
    );
    assert_eq!(refused(go(PngCheck::TooLarge)), Refusal::ImageTooLarge);
    assert_eq!(
        refused(go(PngCheck::Unavailable)),
        Refusal::ResourceUnavailable
    );
    assert_eq!(refused(go(valid(0, 4))), Refusal::Corrupt);
    assert_eq!(refused(go(valid(4, 0))), Refusal::Corrupt);
    assert_eq!(refused(go(valid(8193, 1))), Refusal::ImageTooLarge);
    assert_eq!(refused(go(valid(1, 8193))), Refusal::ImageTooLarge);
    match go(valid(8192, 8192)) {
        AdmissionDecision::New(asset) => assert_eq!(asset.dimensions, Some((8192, 8192))),
        other => panic!("expected New at the dimension limit, got {other:?}"),
    }
}

#[test]
fn unrecognised_bytes_and_bad_names_are_refused() {
    let png = png_like(1, 32);
    let go = |name: &str, bytes: &[u8]| decide_stub(&req("r", name, "image/png", bytes), &empty());
    let short: &[u8] = &[1, 2, 3];
    let svg: &[u8] = b"<svg xmlns=''/>";
    assert_eq!(refused(go("n.png", short)), Refusal::UnrecognizedSignature);
    assert_eq!(refused(go("n.png", svg)), Refusal::UnrecognizedSignature);
    let long = "n".repeat(256);
    for name in ["", "  ", "a\0b", long.as_str()] {
        assert_eq!(refused(go(name, png.as_slice())), Refusal::Name, "{name:?}");
    }
    let n255 = "n".repeat(255);
    assert!(matches!(
        go(n255.as_str(), png.as_slice()),
        AdmissionDecision::New(_)
    ));
}

#[test]
fn the_name_rule_agrees_with_the_asset_record_owner() {
    let (a255, a256) = ("a".repeat(255), "a".repeat(256));
    let (e127, e128) = ("é".repeat(127), "é".repeat(128));
    let names = [
        "",
        " ",
        "\t\n",
        "ok.png",
        "a\0b",
        "é",
        a255.as_str(),
        a256.as_str(),
        e127.as_str(),
        e128.as_str(),
    ];
    for name in names {
        let source =
            AssetSource::admitted_embedded("image/png".to_string(), Arc::new(vec![1u8; 4]), 64)
                .unwrap();
        let id = AssetId::new("id".to_string()).unwrap();
        let owner = AssetRecord::new(id, name.to_string(), AssetKind::Image, source).is_ok();
        assert_eq!(valid_name(name), owner, "{name:?}");
    }
}

#[test]
fn limit_constants_agree_with_the_accepted_owners() {
    assert_eq!(
        MAX_DIMENSION as usize * MAX_DIMENSION as usize * 4,
        MAX_RGBA_BYTES
    );
    assert_eq!(MAX_ENCODED_BYTES, 16 * 1024 * 1024);
    let records = |n: usize| -> Vec<Arc<AssetRecord>> {
        (0..n)
            .map(|i| embedded_record(&format!("a{i}"), "n", "image/png", vec![1]))
            .collect()
    };
    assert!(AssetInventory::new(records(MAX_ASSETS), Vec::new(), MAX_ASSETS, 0).is_ok());
    assert!(AssetInventory::new(records(MAX_ASSETS + 1), Vec::new(), MAX_ASSETS, 0).is_err());
}
