//! Admission tests that run the real `reference_image_asset::validate_png` (L06-D).
use super::oracle::{
    decide_native, embedded_record, empty, hex, inventory, refused, req, rows, unhex,
};
use super::*;
use crate::object_document::AssetKind;
use crate::reference_image_asset::{ImageAssetError, ImageAssetLimits};
use std::sync::Arc;

#[test]
fn every_oracle_png_row_is_decided_by_the_real_validator() {
    let table = rows("png");
    assert_eq!(table.len(), 11);
    let mut news = 0;
    for row in &table {
        let name = row["name"].as_str().unwrap();
        let bytes = unhex(row["hex"].as_str().unwrap());
        assert_eq!(bytes.len() as u64, row["len"].as_u64().unwrap(), "{name}");
        let decision = decide_native(&req("req-p", "pic.png", "image/png", &bytes), &empty());
        match row["expect"].as_str().unwrap() {
            "new" => {
                let AdmissionDecision::New(asset) = decision else {
                    panic!("{name}: expected New, got {decision:?}");
                };
                news += 1;
                let dims = (
                    row["width"].as_u64().unwrap(),
                    row["height"].as_u64().unwrap(),
                );
                let got = asset.dimensions.unwrap();
                assert_eq!((got.0 as u64, got.1 as u64), dims, "{name}");
                assert_eq!(
                    hex(&asset.digest),
                    row["sha256"].as_str().unwrap(),
                    "{name}"
                );
                assert_eq!(asset.kind, AssetKind::Image);
            }
            "corrupt" => assert_eq!(refused(decision), Refusal::Corrupt, "{name}"),
            "unsupported" => {
                assert_eq!(refused(decision), Refusal::UnsupportedEncoding, "{name}")
            }
            "too-large" => assert_eq!(refused(decision), Refusal::ImageTooLarge, "{name}"),
            other => panic!("unknown expectation {other}"),
        }
    }
    assert_eq!(news, 3, "oracle must keep its three valid rows");
}

#[test]
fn a_real_valid_duplicate_resolves_to_the_held_asset_and_allocation() {
    let table = rows("png");
    let row = table
        .iter()
        .find(|r| r["name"] == "valid-rgba-2x2")
        .unwrap();
    let bytes = unhex(row["hex"].as_str().unwrap());
    let held = embedded_record("asset-held", "held.png", "image/png", bytes.clone());
    let inv = inventory(vec![held.clone()]);
    let decision = decide_native(&req("req-q", "again.png", "image/png", &bytes), &inv);
    let AdmissionDecision::Duplicate {
        existing,
        allocation,
    } = decision
    else {
        panic!("expected Duplicate");
    };
    assert_eq!(existing.as_str(), "asset-held");
    assert!(Arc::ptr_eq(
        &allocation,
        held.source().encoded_allocation().unwrap()
    ));
}

#[test]
fn the_port_error_mapping_is_total_and_the_limits_are_accepted() {
    use super::png_port::{check_png, map_error, MAX_DECODER_BYTES};
    let cases = [
        (ImageAssetError::Malformed, PngCheck::Corrupt),
        (ImageAssetError::Unsupported, PngCheck::Unsupported),
        (ImageAssetError::EncodedBudget, PngCheck::TooLarge),
        (ImageAssetError::DecodedBudget, PngCheck::TooLarge),
        (ImageAssetError::DecoderBudget, PngCheck::TooLarge),
        (ImageAssetError::DimensionBudget, PngCheck::TooLarge),
        (ImageAssetError::InvalidLimits, PngCheck::Unavailable),
        (ImageAssetError::Allocation, PngCheck::Unavailable),
    ];
    for (error, expected) in cases {
        assert_eq!(map_error(error), expected, "{error:?}");
    }
    let limits = ImageAssetLimits::new(
        MAX_ENCODED_BYTES,
        MAX_RGBA_BYTES,
        MAX_DECODER_BYTES,
        MAX_DIMENSION,
        MAX_DIMENSION,
    );
    assert!(limits.is_ok());
    assert_eq!(check_png(&[1, 2, 3]), PngCheck::Corrupt);
}
