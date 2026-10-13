use crate::reference_image_asset::reference_png_scanlines::{verify, Scanlines, SCRATCH};
use crate::reference_image_asset::{
    decode_png, decode_png_pixels, validate_png, ImageAssetError, ImageAssetLimits,
};
use std::cell::Cell;

// Original frozen corpus; pixel expectations are independent manual values.
const RGBA: &str = "89504e470d0a1a0a0000000d49484452000000030000000108060000001be014b40000001549444154789c63105432feef129ad650de319301001b41047d056017fe0000000049454e44ae426082";
const RGB: &str = "89504e470d0a1a0a0000000d4948445200000002000000020802000000fdd49a730000001649444154789c63e051b2389162c4c0cec1c9c5cd0300124401fe0da78d180000000049454e44ae426082";
fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn limits() -> ImageAssetLimits {
    ImageAssetLimits::new(1024, 1024, 1024 * 1024, 64, 64).unwrap()
}
fn crc(data: &[u8]) -> [u8; 4] {
    let mut crc = !0u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb88320
            } else {
                crc >> 1
            };
        }
    }
    (!crc).to_be_bytes()
}
fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut result = (data.len() as u32).to_be_bytes().to_vec();
    result.extend_from_slice(kind);
    result.extend_from_slice(data);
    result.extend_from_slice(&crc(&result[4..]));
    result
}
fn png(width: u32, height: u32, chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut header = width.to_be_bytes().to_vec();
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut result = b"\x89PNG\r\n\x1a\n".to_vec();
    result.extend(chunk(b"IHDR", &header));
    for data in chunks {
        result.extend_from_slice(data);
    }
    result.extend(chunk(b"IEND", &[]));
    result
}
// Manual zlib stored block + Adler, independent of the production inflater/encoder.
fn stored(raw: &[u8]) -> Vec<u8> {
    let length = u16::try_from(raw.len()).unwrap();
    let mut result = vec![0x78, 0x01, 0x01];
    result.extend(length.to_le_bytes());
    result.extend((!length).to_le_bytes());
    result.extend_from_slice(raw);
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in raw {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    result.extend(((b << 16) | a).to_be_bytes());
    result
}
fn refuses(source: &[u8]) {
    assert_eq!(
        validate_png(source, limits()),
        Err(ImageAssetError::Malformed)
    );
    assert_eq!(
        decode_png(source, limits()).unwrap_err(),
        ImageAssetError::Malformed
    );
    let called = Cell::new(false);
    let result = decode_png_pixels::<ImageAssetError>(source, limits(), |_| {
        called.set(true);
        panic!("malformed stream must refuse before RGBA allocation")
    });
    assert_eq!(result.unwrap_err(), ImageAssetError::Malformed);
    assert!(!called.get());
}

#[test]
fn original_rgb_rgba_pixels_and_corrected_width_surplus() {
    let rgba = bytes(RGBA);
    let pixels = decode_png(&rgba, limits()).unwrap();
    assert_eq!(
        pixels.rgba8(),
        [17, 34, 51, 255, 68, 85, 102, 128, 119, 136, 153, 0]
    );
    assert_eq!(
        decode_png(&bytes(RGB), limits()).unwrap().rgba8(),
        [12, 34, 56, 255, 200, 100, 50, 255, 7, 8, 9, 255, 10, 11, 12, 255]
    );
    let mut shortened = rgba;
    shortened[16..20].copy_from_slice(&2u32.to_be_bytes());
    let checksum = crc(&shortened[12..29]);
    shortened[29..33].copy_from_slice(&checksum);
    refuses(&shortened);
}

#[test]
fn surplus_one_byte_full_row_and_missing_rows_or_filter_refuse_before_pixels() {
    for (height, raw) in [
        (1, vec![0, 17, 34, 51, 255, 0]),
        (1, vec![0, 17, 34, 51, 255, 0, 17, 34, 51, 255]),
        (2, vec![0, 17, 34, 51, 255]),
        (1, vec![17, 34, 51, 255]),
    ] {
        refuses(&png(1, height, &[chunk(b"IDAT", &stored(&raw))]));
    }
}

#[test]
fn every_idat_split_and_empty_chunks_preserve_literal_pixels() {
    let original = bytes(RGBA);
    let compressed = &original[41..62];
    // Frozen IDAT data starts after its length/type header at byte 41.
    for split in 0..=compressed.len() {
        let split_png = png(
            3,
            1,
            &[
                chunk(b"IDAT", &compressed[..split]),
                chunk(b"IDAT", &[]),
                chunk(b"IDAT", &compressed[split..]),
                chunk(b"IDAT", &[]),
            ],
        );
        assert_eq!(verify(&split_png, 12, 1), Ok(()), "split {split}");
        assert_eq!(
            decode_png(&split_png, limits()).unwrap().rgba8(),
            [17, 34, 51, 255, 68, 85, 102, 128, 119, 136, 153, 0]
        );
    }
    let singles: Vec<_> = compressed.iter().map(|b| chunk(b"IDAT", &[*b])).collect();
    assert_eq!(validate_png(&png(3, 1, &singles), limits()), Ok((3, 1)));
}

#[test]
fn ancillary_crc_stays_with_png_and_idat_must_remain_contiguous() {
    let compressed = stored(&[0, 17, 34, 51, 255]);
    let text = chunk(b"tEXt", b"key\0value");
    let valid = png(
        1,
        1,
        &[text.clone(), chunk(b"IDAT", &compressed), text.clone()],
    );
    assert_eq!(
        decode_png(&valid, limits()).unwrap().rgba8(),
        [17, 34, 51, 255]
    );
    let split = compressed.len() / 2;
    refuses(&png(
        1,
        1,
        &[
            chunk(b"IDAT", &compressed[..split]),
            text.clone(),
            chunk(b"IDAT", &compressed[split..]),
        ],
    ));
    let mut bad_crc = text;
    *bad_crc.last_mut().unwrap() ^= 1;
    let bad = png(1, 1, &[chunk(b"IDAT", &compressed), bad_crc]);
    assert_eq!(verify(&bad, 4, 1), Ok(()));
    assert_eq!(
        decode_png(&bad, limits()).unwrap_err(),
        ImageAssetError::Malformed
    );
}

#[test]
fn zlib_header_adler_truncation_and_trailing_streams_refuse() {
    let good = stored(&[0, 17, 34, 51, 255]);
    let mut header = good.clone();
    header[0] ^= 1;
    let mut adler = good.clone();
    *adler.last_mut().unwrap() ^= 1;
    let mut garbage = good.clone();
    garbage.push(0);
    let mut second = good.clone();
    second.extend_from_slice(&good);
    for data in [
        header,
        adler,
        good[..good.len() - 1].to_vec(),
        garbage,
        second,
    ] {
        refuses(&png(1, 1, &[chunk(b"IDAT", &data)]));
    }
    refuses(&png(1, 1, &[chunk(b"IDAT", &good), chunk(b"IDAT", &good)]));
}

#[test]
fn fixed_scratch_boundaries_and_final_sentinel_do_not_discard_extra_output() {
    for length in [SCRATCH - 1, SCRATCH, SCRATCH + 1, 2 * SCRATCH + 1] {
        let raw = vec![3; length];
        let encoded = stored(&raw);
        let source = png(1, 1, &[chunk(b"IDAT", &encoded)]);
        assert_eq!(verify(&source, length - 1, 1), Ok(()));
        assert_eq!(
            verify(&source, length - 2, 1),
            Err(ImageAssetError::Malformed)
        );
        let mut guard = Scanlines::new(length - 1, 1).unwrap();
        let mut scratch = [0; SCRATCH];
        for byte in &encoded {
            guard.feed(&[*byte], &mut scratch).unwrap();
        }
        assert_eq!(guard.finish(&mut scratch), Ok(()));
    }
}

#[test]
fn stalled_final_drain_checked_length_and_chunk_bounds_fail_closed() {
    assert!(matches!(
        Scanlines::new(usize::MAX, 1),
        Err(ImageAssetError::DecodedBudget)
    ));
    assert!(matches!(
        Scanlines::new(usize::MAX / 2, 4),
        Err(ImageAssetError::DecodedBudget)
    ));
    assert!(Scanlines::new(0, 1).is_err());
    assert!(Scanlines::new(4, 0).is_err());
    let mut guard = Scanlines::new(4, 1).unwrap();
    let mut scratch = [0; SCRATCH];
    assert_eq!(guard.feed(&[0x78, 0x01], &mut scratch), Ok(()));
    assert_eq!(guard.finish(&mut scratch), Err(ImageAssetError::Malformed));
    let source = png(1, 1, &[chunk(b"IDAT", &stored(&[0, 17, 34, 51, 255]))]);
    for length in 0..source.len() {
        assert!(verify(&source[..length], 4, 1).is_err());
    }
    let mut invalid = source;
    invalid[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(verify(&invalid, 4, 1), Err(ImageAssetError::Malformed));
}

#[test]
#[cfg(all(feature = "history", feature = "project_media_codec"))]
fn malformed_managed_wire_and_codec_refuse_without_escaping_a_snapshot() {
    use crate::object_codec::{decode_allocated_project, encoded_wire_test, ProjectWireError};
    use crate::revision::ObjectSnapshot;
    use base64::Engine;
    let original = bytes(RGBA);
    let invalid = png(2, 1, &[chunk(b"IDAT", &original[41..62])]);
    let mut wire = encoded_wire_test::wire();
    wire["assets"][0]["data"] = base64::prelude::BASE64_STANDARD.encode(invalid).into();
    let bytes = serde_json::to_vec(&wire).unwrap();
    let result = ObjectSnapshot::from_managed_project_wire(
        "strict".into(),
        &bytes,
        encoded_wire_test::limits(),
    );
    assert_eq!(
        result.unwrap_err(),
        ProjectWireError::Png(ImageAssetError::Malformed)
    );
    // Exercise the same production codec with an observable isolated permit.
    // The capped snapshot's private test seam is deliberately not expanded.
    let budget = ObjectSnapshot::encoded_test_budget(1024);
    let mut charge = None;
    let called = Cell::new(false);
    let result = decode_allocated_project(&bytes, encoded_wire_test::limits(), |n| {
        called.set(true);
        charge = Some(budget.reserve(n).map_err(|_| ProjectWireError::Budget)?);
        charge
            .as_mut()
            .unwrap()
            .allocate()
            .map_err(|_| ProjectWireError::Budget)
    });
    assert_eq!(
        result.unwrap_err(),
        ProjectWireError::Png(ImageAssetError::Malformed)
    );
    assert!(called.get());
    assert!(budget.used().unwrap() > 0);
    drop(charge);
    assert_eq!(budget.used(), Ok(0));
}
