//! Existing bundled-font parser oracles and native admission/lifetime controls.
use super::*;

const ROBOTO: &[u8] = include_bytes!("../../src/fonts/Roboto-Regular.ttf");
fn limits() -> FontAssetLimits {
    FontAssetLimits::new(2 * 1024 * 1024, 256).unwrap()
}
fn table_record(bytes: &[u8], tag: &[u8; 4]) -> usize {
    let count = u16::from_be_bytes(bytes[4..6].try_into().unwrap()) as usize;
    (0..count)
        .map(|i| 12 + i * 16)
        .find(|&i| &bytes[i..i + 4] == tag)
        .unwrap()
}
fn table_offset(bytes: &[u8], tag: &[u8; 4]) -> usize {
    let index = table_record(bytes, tag);
    u32::from_be_bytes(bytes[index + 8..index + 12].try_into().unwrap()) as usize
}

#[test]
fn eight_bundled_faces_match_retained_opentype_parser_metadata() {
    // Constants characterized with actual existing src/js/opentype.min.js,
    // not regenerated from this native implementation.
    let fixtures: [(&[u8], u16, u16); 8] = [
        (ROBOTO, 2048, 363),
        (include_bytes!("../../src/fonts/Roboto-Bold.ttf"), 2048, 363),
        (include_bytes!("../../src/fonts/DejaVuSans.ttf"), 2048, 6241),
        (
            include_bytes!("../../src/fonts/DejaVuSans-Bold.ttf"),
            2048,
            6184,
        ),
        (
            include_bytes!("../../src/fonts/DejaVuSerif.ttf"),
            2048,
            3525,
        ),
        (
            include_bytes!("../../src/fonts/DejaVuSerif-Bold.ttf"),
            2048,
            3503,
        ),
        (
            include_bytes!("../../src/fonts/Manrope-Regular.ttf"),
            2000,
            742,
        ),
        (
            include_bytes!("../../src/fonts/Inter-Regular.ttf"),
            2048,
            2933,
        ),
    ];
    for (bytes, units, glyphs) in fixtures {
        let admitted = validate_font(bytes, limits()).unwrap();
        assert_eq!(
            (admitted.units_per_em, admitted.glyph_count),
            (units, glyphs)
        );
    }
}

#[test]
fn limits_are_explicit_and_refuse_before_font_parsing() {
    for (encoded, tables) in [
        (0, 1),
        (MAX_ENCODED + 1, 1),
        (1024, 0),
        (1024, MAX_TABLES + 1),
    ] {
        assert!(matches!(
            FontAssetLimits::new(encoded, tables),
            Err(FontAssetError::Limit)
        ));
    }
    assert!(matches!(
        validate_font(ROBOTO, FontAssetLimits::new(ROBOTO.len() - 1, 256).unwrap()),
        Err(FontAssetError::Limit)
    ));
    assert!(matches!(
        validate_font(ROBOTO, FontAssetLimits::new(ROBOTO.len(), 1).unwrap()),
        Err(FontAssetError::Limit)
    ));
    assert!(matches!(
        validate_font(&[], limits()),
        Err(FontAssetError::Limit)
    ));
}

#[test]
fn unsupported_containers_and_png_are_never_font_admission_proofs() {
    for bytes in [
        b"OTTO".as_slice(),
        b"ttcf",
        b"wOFF",
        b"wOF2",
        b"true",
        b"\x89PNG\r\n\x1a\n",
    ] {
        assert!(matches!(
            validate_font(bytes, limits()),
            Err(FontAssetError::Unsupported)
        ));
        assert!(matches!(
            admit_reserved_font(Arc::new(bytes.to_vec()), limits()),
            Err(FontAssetError::Unsupported)
        ));
    }
}

#[test]
fn truncated_sfnt_and_table_ranges_are_refused() {
    for length in [4, 8, 12, 64, ROBOTO.len() / 2] {
        assert!(validate_font(&ROBOTO[..length], limits()).is_err());
    }
    let mut offset = ROBOTO.to_vec();
    offset[20..24].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(
        validate_font(&offset, limits()),
        Err(FontAssetError::Invalid)
    ));
    let mut length = ROBOTO.to_vec();
    length[24..28].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(
        validate_font(&length, limits()),
        Err(FontAssetError::Invalid)
    ));
}

#[test]
fn duplicate_and_missing_required_tables_do_not_admit() {
    let mut duplicate = ROBOTO.to_vec();
    let tag: [u8; 4] = duplicate[12..16].try_into().unwrap();
    duplicate[28..32].copy_from_slice(&tag);
    assert!(matches!(
        validate_font(&duplicate, limits()),
        Err(FontAssetError::Invalid)
    ));
    for tag in [
        b"head", b"maxp", b"hhea", b"hmtx", b"cmap", b"name", b"loca", b"glyf",
    ] {
        let mut bytes = ROBOTO.to_vec();
        let record = table_record(&bytes, tag);
        bytes[record..record + 4].copy_from_slice(b"zzzz");
        assert!(
            matches!(
                validate_font(&bytes, limits()),
                Err(FontAssetError::Invalid)
            ),
            "{tag:?}"
        );
    }
}

#[test]
fn required_font_metadata_is_validated_without_glyph_layout() {
    let head = table_offset(ROBOTO, b"head");
    let maxp = table_offset(ROBOTO, b"maxp");
    let mut magic = ROBOTO.to_vec();
    magic[head + 12..head + 16].copy_from_slice(&0u32.to_be_bytes());
    assert!(matches!(
        validate_font(&magic, limits()),
        Err(FontAssetError::Invalid)
    ));
    let mut units = ROBOTO.to_vec();
    units[head + 18..head + 20].copy_from_slice(&0u16.to_be_bytes());
    assert!(matches!(
        validate_font(&units, limits()),
        Err(FontAssetError::Invalid)
    ));
    let mut glyphs = ROBOTO.to_vec();
    glyphs[maxp + 4..maxp + 6].copy_from_slice(&0u16.to_be_bytes());
    assert!(matches!(
        validate_font(&glyphs, limits()),
        Err(FontAssetError::Invalid)
    ));
}

#[test]
fn capacity_not_length_controls_the_preallocated_source_bound() {
    let mut bytes = Vec::with_capacity(ROBOTO.len() + 1024);
    bytes.extend_from_slice(ROBOTO);
    let capacity = bytes.capacity();
    let bytes = Arc::new(bytes);
    assert!(matches!(
        admit_reserved_font(
            Arc::clone(&bytes),
            FontAssetLimits::new(ROBOTO.len(), 256).unwrap()
        ),
        Err(FontAssetError::Limit)
    ));
    let admitted = admit_reserved_font(
        Arc::clone(&bytes),
        FontAssetLimits::new(capacity, 256).unwrap(),
    )
    .unwrap();
    assert!(Arc::ptr_eq(admitted.encoded_allocation(), &bytes));
    assert_eq!(admitted.encoded_allocation().capacity(), capacity);
}

#[test]
fn proof_clones_keep_exact_bytes_until_the_last_native_pin() {
    let bytes = Arc::new(ROBOTO.to_vec());
    let weak = Arc::downgrade(&bytes);
    let admitted = admit_reserved_font(Arc::clone(&bytes), limits()).unwrap();
    assert_eq!(admitted.media_type(), "font/ttf");
    assert_eq!(
        admitted.metadata(),
        FontAssetMetadata {
            units_per_em: 2048,
            glyph_count: 363
        }
    );
    assert!(Arc::ptr_eq(admitted.encoded_allocation(), &bytes));
    let pin = admitted.clone();
    drop((bytes, admitted));
    assert!(weak.upgrade().is_some());
    let retained = pin.into_encoded();
    assert_eq!(retained.as_slice(), ROBOTO);
    assert_eq!(Arc::strong_count(&retained), 1);
    drop(retained);
    assert!(weak.upgrade().is_none());
}

#[test]
fn empty_horizontal_metrics_table_cannot_hide_a_declared_full_metric_count() {
    let hhea = table_offset(ROBOTO, b"hhea");
    let maxp = table_offset(ROBOTO, b"maxp");
    assert_eq!(
        u16::from_be_bytes(ROBOTO[hhea + 34..hhea + 36].try_into().unwrap()),
        363
    );
    assert_eq!(
        u16::from_be_bytes(ROBOTO[maxp + 4..maxp + 6].try_into().unwrap()),
        363
    );
    let mut bytes = ROBOTO.to_vec();
    let record = table_record(&bytes, b"hmtx");
    bytes[record + 12..record + 16].copy_from_slice(&0u32.to_be_bytes());
    assert!(matches!(
        validate_font(&bytes, limits()),
        Err(FontAssetError::Invalid)
    ));
    assert!(matches!(
        admit_reserved_font(Arc::new(bytes), limits()),
        Err(FontAssetError::Invalid)
    ));
}

#[test]
fn short_and_long_location_tables_require_enough_ordered_in_range_offsets() {
    let fixtures: [(&[u8], i16); 2] = [
        (ROBOTO, 0),
        (include_bytes!("../../src/fonts/DejaVuSans.ttf"), 1),
    ];
    for (original, format) in fixtures {
        let head = table_offset(original, b"head");
        assert_eq!(
            i16::from_be_bytes(original[head + 50..head + 52].try_into().unwrap()),
            format
        );
        let width = if format == 0 { 2 } else { 4 };
        let loca_record = table_record(original, b"loca");
        let loca = table_offset(original, b"loca");
        let maxp = table_offset(original, b"maxp");
        let count = usize::from(u16::from_be_bytes(
            original[maxp + 4..maxp + 6].try_into().unwrap(),
        ));
        let glyf_record = table_record(original, b"glyf");
        let glyph_bytes = u32::from_be_bytes(
            original[glyf_record + 12..glyf_record + 16]
                .try_into()
                .unwrap(),
        );
        let mut short = original.to_vec();
        short[loca_record + 12..loca_record + 16]
            .copy_from_slice(&((count * width) as u32).to_be_bytes());
        assert!(matches!(
            validate_font(&short, limits()),
            Err(FontAssetError::Invalid)
        ));
        let mut descending = original.to_vec();
        descending[loca..loca + width].fill(0xff);
        descending[loca + width..loca + width * 2].fill(0);
        assert!(matches!(
            validate_font(&descending, limits()),
            Err(FontAssetError::Invalid)
        ));
        let mut outside = original.to_vec();
        // All entries stay ascending; only the final required offset is beyond
        // the glyf table. This independently isolates the range guard.
        let last = loca + count * width;
        if format == 0 {
            assert!(glyph_bytes < u32::from(u16::MAX) * 2);
            outside[last..last + width].copy_from_slice(&u16::MAX.to_be_bytes());
        } else {
            outside[last..last + width].copy_from_slice(&u32::MAX.to_be_bytes());
        }
        assert!(matches!(
            validate_font(&outside, limits()),
            Err(FontAssetError::Invalid)
        ));
        assert_eq!(
            validate_font(original, limits()).unwrap().glyph_count,
            count as u16
        );
    }
}

#[test]
fn invalid_location_format_is_not_interpreted_as_short_offsets() {
    let head = table_offset(ROBOTO, b"head");
    for format in [-1i16, 2] {
        let mut bytes = ROBOTO.to_vec();
        bytes[head + 50..head + 52].copy_from_slice(&format.to_be_bytes());
        assert!(matches!(
            validate_font(&bytes, limits()),
            Err(FontAssetError::Invalid)
        ));
    }
}
