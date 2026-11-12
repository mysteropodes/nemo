//! nemo-test-harness: test-codec => reference_image_asset_tests
use crate::reference_image_asset::{decode_png, validate_png, ImageAssetError, ImageAssetLimits};

// Frozen minimal PNG containers: literal IHDR, zlib scanlines and CRCs.
// Accepted-pixel expectations below are manual, not decoder/encoder goldens.
const RGBA: &str = "89504e470d0a1a0a0000000d49484452000000030000000108060000001be014b40000001549444154789c63105432feef129ad650de319301001b41047d056017fe0000000049454e44ae426082";
const RGB: &str = "89504e470d0a1a0a0000000d4948445200000002000000020802000000fdd49a730000001649444154789c63e051b2389162c4c0cec1c9c5cd0300124401fe0da78d180000000049454e44ae426082";
const RGB_TRNS: &str = "89504e470d0a1a0a0000000d4948445200000002000000020802000000fdd49a730000000674524e53000c00220038b89dc0800000001649444154789c63e051b2389162c4c0cec1c9c5cd0300124401fe0da78d180000000049454e44ae426082";
fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn limits() -> ImageAssetLimits {
    ImageAssetLimits::new(1024, 1024, 1024 * 1024, 64, 64).unwrap()
}
fn crc(input: &[u8]) -> [u8; 4] {
    let mut value = !0u32;
    for &byte in input {
        value ^= u32::from(byte);
        for _ in 0..8 {
            value = if value & 1 == 1 {
                (value >> 1) ^ 0xedb88320
            } else {
                value >> 1
            };
        }
    }
    (!value).to_be_bytes()
}
fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let checksum = crc(&out[4..]);
    out.extend_from_slice(&checksum);
    out
}
fn header(
    mut png: Vec<u8>,
    width: u32,
    height: u32,
    depth: u8,
    color: u8,
    interlace: u8,
) -> Vec<u8> {
    png[16..20].copy_from_slice(&width.to_be_bytes());
    png[20..24].copy_from_slice(&height.to_be_bytes());
    png[24] = depth;
    png[25] = color;
    png[28] = interlace;
    let checksum = crc(&png[12..29]);
    png[29..33].copy_from_slice(&checksum);
    png
}
fn inserted(png: &[u8], after: usize, chunk: &[u8]) -> Vec<u8> {
    let mut out = png[..after].to_vec();
    out.extend_from_slice(chunk);
    out.extend_from_slice(&png[after..]);
    out
}

fn parity(source: &[u8], budget: ImageAssetLimits) -> Result<(u32, u32), ImageAssetError> {
    let validation = validate_png(source, budget);
    assert_eq!(
        validation,
        decode_png(source, budget).map(|asset| asset.dimensions())
    );
    validation
}
#[test]
fn borrowed_validation_has_literal_dimensions_and_current_decoder_path_parity() {
    for (fixture, dimensions) in [(RGBA, (3, 1)), (RGB, (2, 2))] {
        let source = bytes(fixture);
        let before = source.clone();
        assert_eq!(parity(&source, limits()), Ok(dimensions));
        assert_eq!(source, before);
        for len in 0..source.len() {
            assert!(parity(&source[..len], limits()).is_err());
        }
        let mut trailing = source.clone();
        trailing.push(0);
        assert_eq!(parity(&trailing, limits()), Err(ImageAssetError::Malformed));
    }
    // Corrected width2 must refuse the surplus pixels before row delivery.
    assert_eq!(
        parity(&header(bytes(RGBA), 2, 1, 8, 6, 0), limits()),
        Err(ImageAssetError::Malformed)
    );
    for (w, h) in [(3, 2), (3, 3)] {
        // Correct IHDR CRC: insufficient declared rows must still refuse.
        assert!(parity(&header(bytes(RGBA), w, h, 8, 6, 0), limits()).is_err());
    }
    for (depth, color, interlace) in [(16, 6, 0), (8, 3, 0), (8, 0, 0), (8, 4, 0), (8, 6, 1)] {
        assert_eq!(
            parity(
                &header(bytes(RGBA), 3, 1, depth, color, interlace),
                limits()
            ),
            Err(ImageAssetError::Unsupported)
        );
    }
    for kind in [b"acTL", b"fcTL", b"fdAT"] {
        assert_eq!(
            parity(&inserted(&bytes(RGBA), 33, &chunk(kind, &[0; 8])), limits()),
            Err(ImageAssetError::Unsupported)
        );
    }
    assert_eq!(
        parity(&bytes(RGB_TRNS), limits()),
        Err(ImageAssetError::Unsupported)
    );
}
#[test]
fn borrowed_validation_checks_late_checksums_and_all_caller_budgets() {
    let source = bytes(RGBA);
    for index in [29, 62, 77] {
        let mut bad = source.clone();
        bad[index] ^= 1;
        assert_eq!(parity(&bad, limits()), Err(ImageAssetError::Malformed));
    }
    let text = chunk(b"tEXt", b"key\0value");
    let valid = inserted(&source, 33, &text);
    assert_eq!(parity(&valid, limits()), Ok((3, 1)));
    let mut corrupt = text;
    *corrupt.last_mut().unwrap() ^= 1;
    assert_eq!(
        parity(&inserted(&source, 33, &corrupt), limits()),
        Err(ImageAssetError::Malformed)
    );
    let mut adler = source.clone();
    adler[61] ^= 1;
    let crc = crc(&adler[37..62]);
    adler[62..66].copy_from_slice(&crc);
    assert_eq!(parity(&adler, limits()), Err(ImageAssetError::Malformed));
    for (cap, error) in [
        (
            ImageAssetLimits::new(source.len() - 1, 12, 1024 * 1024, 3, 1).unwrap(),
            ImageAssetError::EncodedBudget,
        ),
        (
            ImageAssetLimits::new(source.len(), 11, 1024 * 1024, 3, 1).unwrap(),
            ImageAssetError::DecodedBudget,
        ),
        (
            ImageAssetLimits::new(source.len(), 12, 1, 3, 1).unwrap(),
            ImageAssetError::DecoderBudget,
        ),
        (
            ImageAssetLimits::new(source.len(), 12, 1024 * 1024, 2, 1).unwrap(),
            ImageAssetError::DimensionBudget,
        ),
    ] {
        assert_eq!(parity(&source, cap), Err(error));
    }
    assert_eq!(
        parity(
            &source,
            ImageAssetLimits::new(source.len(), 12, 1024 * 1024, 3, 1).unwrap()
        ),
        Ok((3, 1))
    );
}

#[test]
fn literal_rgba_preserves_opaque_half_zero_alpha_hidden_rgb_and_source_bytes() {
    let mut source = bytes(RGBA);
    let asset = decode_png(&source, limits()).unwrap();
    assert_eq!(asset.dimensions(), (3, 1));
    assert_eq!(asset.row_stride(), 12);
    assert_eq!(
        asset.rgba8(),
        [17, 34, 51, 255, 68, 85, 102, 128, 119, 136, 153, 0]
    );
    assert_eq!(asset.encoded_png(), source);
    source.fill(0);
    drop(source);
    assert_eq!(asset.encoded_png(), bytes(RGBA));
    assert_eq!(asset.rgba8()[4..8], [68, 85, 102, 128]);
    let clone = asset.clone();
    assert!(std::ptr::eq(asset.rgba8().as_ptr(), clone.rgba8().as_ptr()));
    assert!(std::ptr::eq(
        asset.encoded_png().as_ptr(),
        clone.encoded_png().as_ptr()
    ));
    drop(asset);
    assert_eq!(clone.rgba8()[8..], [119, 136, 153, 0]);
}

#[test]
fn literal_rgb_normalizes_rows_to_rgba_with_exact_opaque_alpha() {
    let source = bytes(RGB);
    let asset = decode_png(&source, limits()).unwrap();
    assert_eq!(asset.dimensions(), (2, 2));
    assert_eq!(asset.row_stride(), 8);
    assert_eq!(
        asset.rgba8(),
        [12, 34, 56, 255, 200, 100, 50, 255, 7, 8, 9, 255, 10, 11, 12, 255]
    );
    assert_eq!(asset.encoded_png(), source);
    assert_eq!(decode_png(&source, limits()).unwrap(), asset);
}

#[test]
fn mandatory_limits_cannot_be_zero_or_unbounded() {
    for (encoded, rgba, decoder, width, height) in [
        (0, 1024, 1024, 64, 64),
        (1024, 0, 1024, 64, 64),
        (1024, 1024, 0, 64, 64),
        (1024, 1024, 1024, 0, 64),
        (1024, 1024, 1024, 64, 0),
        (usize::MAX, 1024, 1024, 64, 64),
        (1024, usize::MAX, 1024, 64, 64),
        (1024, 1024, usize::MAX, 64, 64),
        (1024, 1024, 1024, u32::MAX, 64),
        (1024, 1024, 1024, 64, u32::MAX),
    ] {
        assert_eq!(
            ImageAssetLimits::new(encoded, rgba, decoder, width, height).unwrap_err(),
            ImageAssetError::InvalidLimits
        );
    }
}

#[test]
fn encoded_normalized_decoder_and_dimension_budgets_are_independent() {
    let source = bytes(RGBA);
    assert_eq!(
        decode_png(
            &source,
            ImageAssetLimits::new(source.len() - 1, 12, 1024 * 1024, 3, 1).unwrap()
        )
        .unwrap_err(),
        ImageAssetError::EncodedBudget
    );
    assert_eq!(
        decode_png(
            &source,
            ImageAssetLimits::new(source.len(), 11, 1024 * 1024, 3, 1).unwrap()
        )
        .unwrap_err(),
        ImageAssetError::DecodedBudget
    );
    assert_eq!(
        decode_png(
            &source,
            ImageAssetLimits::new(source.len(), 12, 1, 3, 1).unwrap()
        )
        .unwrap_err(),
        ImageAssetError::DecoderBudget
    );
    assert_eq!(
        decode_png(
            &source,
            ImageAssetLimits::new(source.len(), 12, 1024 * 1024, 2, 1).unwrap()
        )
        .unwrap_err(),
        ImageAssetError::DimensionBudget
    );
    assert!(decode_png(
        &source,
        ImageAssetLimits::new(source.len(), 12, 1024 * 1024, 3, 1).unwrap()
    )
    .is_ok());
    let rgb = bytes(RGB);
    assert_eq!(
        decode_png(
            &rgb,
            ImageAssetLimits::new(1024, 12, 1024 * 1024, 2, 2).unwrap()
        )
        .unwrap_err(),
        ImageAssetError::DecodedBudget,
        "RGB output budget includes normalized alpha bytes"
    );
}

#[test]
fn zero_huge_and_overflow_dimensions_refuse_before_pixel_allocation() {
    for (w, h) in [(0, 1), (1, 0), (u32::MAX, u32::MAX), (4097, 1), (1, 4097)] {
        assert!(decode_png(&header(bytes(RGBA), w, h, 8, 6, 0), limits()).is_err());
    }
}

#[test]
fn unsupported_bit_depth_palette_grayscale_interlace_and_animation_are_not_normalized() {
    for (depth, color, interlace) in [(16, 6, 0), (8, 3, 0), (8, 0, 0), (8, 4, 0), (8, 6, 1)] {
        assert_eq!(
            decode_png(
                &header(bytes(RGBA), 3, 1, depth, color, interlace),
                limits()
            )
            .unwrap_err(),
            ImageAssetError::Unsupported
        );
    }
    for kind in [b"acTL", b"fcTL", b"fdAT"] {
        let animation = inserted(&bytes(RGBA), 33, &chunk(kind, &[0; 8]));
        assert_eq!(
            decode_png(&animation, limits()).unwrap_err(),
            ImageAssetError::Unsupported
        );
    }
    let transparent_rgb = bytes(RGB_TRNS);
    assert_eq!(
        decode_png(&transparent_rgb, limits()).unwrap_err(),
        ImageAssetError::Unsupported
    );
}

#[test]
fn signature_truncation_critical_ancillary_crc_and_zlib_checksums_are_strict() {
    let source = bytes(RGBA);
    for len in 0..source.len() {
        assert!(
            decode_png(&source[..len], limits()).is_err(),
            "prefix {len}"
        );
    }
    let mut bad = source.clone();
    bad[0] = 0;
    assert_eq!(
        decode_png(&bad, limits()).unwrap_err(),
        ImageAssetError::Malformed
    );
    for index in [29, 62, 77] {
        let mut bad = source.clone();
        bad[index] ^= 1;
        assert_eq!(
            decode_png(&bad, limits()).unwrap_err(),
            ImageAssetError::Malformed
        );
    }
    let text = chunk(b"tEXt", b"key\0value");
    let valid = inserted(&source, 33, &text);
    assert!(decode_png(&valid, limits()).is_ok());
    let mut corrupt = text.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert_eq!(
        decode_png(&inserted(&source, 33, &corrupt), limits()).unwrap_err(),
        ImageAssetError::Malformed
    );
    let mut adler = source.clone();
    adler[61] ^= 1;
    let checksum = crc(&adler[37..62]);
    adler[62..66].copy_from_slice(&checksum);
    assert_eq!(
        decode_png(&adler, limits()).unwrap_err(),
        ImageAssetError::Malformed
    );
}

#[test]
fn end_marker_trailing_data_and_late_animation_cannot_be_ignored() {
    let source = bytes(RGBA);
    let mut trailing = source.clone();
    trailing.push(0);
    assert_eq!(
        decode_png(&trailing, limits()).unwrap_err(),
        ImageAssetError::Malformed
    );
    let late = inserted(&source, 66, &chunk(b"fcTL", &[0; 26]));
    assert_eq!(
        decode_png(&late, limits()).unwrap_err(),
        ImageAssetError::Unsupported
    );
    let end_crc = inserted(&source, 66, &chunk(b"tEXt", b"end\0checked"));
    assert!(decode_png(&end_crc, limits()).is_ok());
    let mut late_crc = end_crc.clone();
    let late_end = 66 + chunk(b"tEXt", b"end\0checked").len() - 1;
    late_crc[late_end] ^= 1;
    assert_eq!(
        decode_png(&late_crc, limits()).unwrap_err(),
        ImageAssetError::Malformed
    );
    let mut bad = end_crc.clone();
    let end = bad.len() - 1;
    bad[end] ^= 1;
    assert_eq!(
        decode_png(&bad, limits()).unwrap_err(),
        ImageAssetError::Malformed
    );
    let mut huge = source.clone();
    huge[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(
        decode_png(&huge, limits()).unwrap_err(),
        ImageAssetError::Malformed
    );
}

// Only a small input row and compressed bytes are held: never a full golden.
// The encoder builds boundary-size inputs; expected channel bytes are literals.
fn capacity_source(width: u32, height: u32, color: png::ColorType) -> Vec<u8> {
    use std::io::Write;
    let channels = if color == png::ColorType::Rgb { 3 } else { 4 };
    let mut row = Vec::with_capacity(width as usize * channels);
    for x in 0..width {
        let pixel = match x % 3 {
            0 => [17, 34, 51, 255],
            1 => [68, 85, 102, 128],
            _ => [119, 136, 153, 0],
        };
        row.extend_from_slice(&pixel[..channels]);
    }
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Balanced);
        encoder.set_filter(png::Filter::NoFilter);
        let mut writer = encoder.write_header().unwrap();
        {
            let mut stream = writer.stream_writer().unwrap();
            for _ in 0..height {
                stream.write_all(&row).unwrap();
            }
            stream.finish().unwrap();
        }
        writer.finish().unwrap();
    }
    assert!(encoded.len() <= 16 * 1024 * 1024);
    encoded
}

fn capacity_pixels(color: png::ColorType, width: u32, height: u32) {
    let mut encoded = capacity_source(width, height, color);
    let expected_size = width as usize * height as usize * 4;
    let budget = ImageAssetLimits::new(
        encoded.len(),
        expected_size,
        64 * 1024 * 1024,
        width,
        height,
    )
    .unwrap();
    let asset = decode_png(&encoded, budget).unwrap();
    assert_eq!(asset.dimensions(), (width, height));
    assert_eq!(asset.row_stride(), width as usize * 4);
    assert_eq!(asset.rgba8().len(), expected_size);
    assert_eq!(asset.encoded_png(), encoded);
    for y in [0, height / 2, height - 1] {
        for (x, mut pixel) in [
            (0, [17, 34, 51, 255]),
            (1, [68, 85, 102, 128]),
            (2, [119, 136, 153, 0]),
            (
                width - 1,
                if (width - 1) % 3 == 0 {
                    [17, 34, 51, 255]
                } else if (width - 1) % 3 == 1 {
                    [68, 85, 102, 128]
                } else {
                    [119, 136, 153, 0]
                },
            ),
        ] {
            if color == png::ColorType::Rgb {
                pixel[3] = 255;
            }
            let at = (y as usize * width as usize + x as usize) * 4;
            assert_eq!(&asset.rgba8()[at..at + 4], pixel, "pixel({x},{y})");
        }
    }
    let clone = asset.clone();
    assert_eq!(asset.rgba8().as_ptr(), clone.rgba8().as_ptr());
    assert_eq!(asset.encoded_png().as_ptr(), clone.encoded_png().as_ptr());
    encoded.fill(0);
    drop(encoded);
    drop(asset);
    assert_eq!(clone.rgba8().len(), expected_size);
    assert_eq!(&clone.rgba8()[..4], [17, 34, 51, 255]);
}

#[test]
fn capacity_small_4097_and_8192_dimension_boundaries() {
    capacity_pixels(png::ColorType::Rgba, 4097, 1);
    capacity_pixels(png::ColorType::Rgb, 8192, 2);
    assert!(ImageAssetLimits::new(
        16 * 1024 * 1024,
        256 * 1024 * 1024,
        64 * 1024 * 1024,
        8192,
        8192
    )
    .is_ok());
    assert_eq!(
        ImageAssetLimits::new(1024, 256 * 1024 * 1024 + 1, 1024, 8192, 8192).unwrap_err(),
        ImageAssetError::InvalidLimits
    );
    assert_eq!(
        ImageAssetLimits::new(1024, 256 * 1024 * 1024, 64 * 1024 * 1024 + 1, 8192, 8192)
            .unwrap_err(),
        ImageAssetError::InvalidLimits
    );
    assert_eq!(
        ImageAssetLimits::new(1024, 1024, 1024, 8193, 1).unwrap_err(),
        ImageAssetError::InvalidLimits
    );
}

// Root must allocate one exclusive heavy-cpu slot before these ACTUAL decodes.
// They stay in normal discovery; tiny preflight uses --skip capacity_large.
#[test]
fn capacity_large_rgba8192_returns_exact_256mib_straight_pixels() {
    capacity_pixels(png::ColorType::Rgba, 8192, 8192);
}
#[test]
fn capacity_large_rgb8192_streams_into_exact_256mib_normalized_pixels() {
    capacity_pixels(png::ColorType::Rgb, 8192, 8192);
}
