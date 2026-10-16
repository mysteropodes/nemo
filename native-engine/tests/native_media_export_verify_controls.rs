//! Mutation controls for the verifier: each splices ONE mistake into real
//! encoder output (or a baseline-shaped SVG) and must be caught with the
//! specific typed reason.
use super::stage::{
    assemble, chunk, crc32_bitwise, ihdr, owned, plan, png_bytes, staged, svg_text,
};
use super::*;

fn png_failure(bytes: Vec<u8>, width: u32, height: u32) -> Result<VerifiedOutput, VerifyFailure> {
    let plan = plan(FrameFormat::Png, 0, 0, width, height);
    let files = owned(&plan.batch(0).unwrap().staged_names, |_| bytes.clone());
    verify_output(&plan, &staged(&files))
}

/// Offset of the first IDAT chunk's length field (4 bytes before its type).
fn idat_at(bytes: &[u8]) -> usize {
    bytes.windows(4).position(|w| w == b"IDAT").unwrap() - 4
}

fn svg_failure(text: &str) -> Result<VerifiedOutput, VerifyFailure> {
    let plan = plan(FrameFormat::Svg, 0, 0, 320, 180);
    let files = owned(&plan.batch(0).unwrap().staged_names, |_| {
        text.as_bytes().to_vec()
    });
    verify_output(&plan, &staged(&files))
}

fn reason(result: Result<VerifiedOutput, VerifyFailure>) -> &'static str {
    match result {
        Err(VerifyFailure::Malformed { reason, .. }) => reason,
        other => panic!("expected a malformed failure, got {other:?}"),
    }
}

#[test]
fn unmutated_pass_through_is_clean() {
    assert!(png_failure(png_bytes(16, 16, 1), 16, 16).is_ok());
    assert!(svg_failure(&svg_text(320, 180)).is_ok());
}

#[test]
fn mutation_png_signature_and_crc_flips_are_caught() {
    let good = png_bytes(16, 16, 1);
    let mut bad_signature = good.clone();
    bad_signature[1] = b'p';
    assert_eq!(
        reason(png_failure(bad_signature, 16, 16)),
        "missing PNG signature"
    );
    // IHDR width byte: the stored CRC no longer matches.
    let mut bad_width = good.clone();
    bad_width[19] ^= 0x01;
    assert_eq!(reason(png_failure(bad_width, 16, 16)), "chunk CRC mismatch");
    // One bit inside the compressed pixel stream.
    let mut bad_data = good.clone();
    bad_data[idat_at(&good) + 8 + 3] ^= 0x10;
    assert_eq!(reason(png_failure(bad_data, 16, 16)), "chunk CRC mismatch");
}

#[test]
fn mutation_consistent_header_rewrite_is_caught_by_the_dimension_check_not_the_crc() {
    let good = png_bytes(16, 16, 1);
    // Rewrite IHDR width to 17 AND repair its CRC: only the pinned size can object.
    let mut rewritten = good.clone();
    rewritten[16..20].copy_from_slice(&17u32.to_be_bytes());
    let crc = crc32_bitwise(&rewritten[12..29]);
    rewritten[29..33].copy_from_slice(&crc.to_be_bytes());
    assert_eq!(
        png_failure(rewritten, 16, 16),
        Err(VerifyFailure::DimensionMismatch {
            name: "frame_0001.png".into(),
            expected: (16, 16),
            actual: (17, 16),
        })
    );
}

#[test]
fn mutation_png_structure_edits_are_caught() {
    let good = png_bytes(16, 16, 1);
    let mut trailing = good.clone();
    trailing.push(0);
    assert_eq!(reason(png_failure(trailing, 16, 16)), "bytes after IEND");
    let mut cut_header = good.clone();
    cut_header.truncate(good.len() - 5);
    assert_eq!(
        reason(png_failure(cut_header, 16, 16)),
        "truncated chunk header"
    );
    let mut cut_data = good.clone();
    cut_data.truncate(idat_at(&good) + 8 + 10);
    assert_eq!(reason(png_failure(cut_data, 16, 16)), "truncated chunk");
    let idat_first = assemble(&[
        chunk(b"IDAT", &[1, 2, 3]),
        ihdr(16, 16),
        chunk(b"IEND", &[]),
    ]);
    assert_eq!(
        reason(png_failure(idat_first, 16, 16)),
        "first chunk is not a 13 byte IHDR"
    );
    let double_header = assemble(&[ihdr(16, 16), ihdr(16, 16), chunk(b"IEND", &[])]);
    assert_eq!(reason(png_failure(double_header, 16, 16)), "duplicate IHDR");
    let zero = assemble(&[ihdr(0, 16), chunk(b"IDAT", &[1]), chunk(b"IEND", &[])]);
    assert_eq!(
        reason(png_failure(zero, 16, 16)),
        "IHDR has a zero dimension"
    );
    let iend_with_data = assemble(&[ihdr(16, 16), chunk(b"IDAT", &[1]), chunk(b"IEND", &[0])]);
    assert_eq!(
        reason(png_failure(iend_with_data, 16, 16)),
        "IEND carries data"
    );
}

#[test]
fn mutation_svg_root_edits_are_caught() {
    let good = svg_text(320, 180);
    let cases: Vec<(String, &str)> = vec![
        (
            good.replace("viewBox=\"0 0 320 180\"", "viewBox=\"0 0 180 320\""),
            "viewBox is not 0 0 width height",
        ),
        (
            good.replace(" xmlns=\"http://www.w3.org/2000/svg\"", ""),
            "missing SVG namespace",
        ),
        (
            good.replace("width=\"320\"", "width=\"0320\""),
            "width is not a plain integer",
        ),
        (
            good.replace("height=\"180\"", "height=\"180.0\""),
            "height is not a plain integer",
        ),
        (
            good.replace("width=\"320\"", "width=\"320\" width=\"320\""),
            "duplicate root attribute",
        ),
        (
            good.replace("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n", ""),
            "missing XML preamble",
        ),
        (format!("{good}\n"), "missing closing svg tag"),
        (good.replace("<svg ", "<g "), "root element is not svg"),
        (good.replace("width=\"320\" ", ""), "missing width"),
        (good.replace("height=\"180\" ", ""), "missing height"),
    ];
    for (text, expected) in cases {
        assert_eq!(reason(svg_failure(&text)), expected, "{text}");
    }
    let invalid = vec![0xFFu8, 0xFE];
    let plan = plan(FrameFormat::Svg, 0, 0, 320, 180);
    let files = owned(&plan.batch(0).unwrap().staged_names, |_| invalid.clone());
    assert_eq!(
        reason(verify_output(&plan, &staged(&files))),
        "SVG is not UTF-8"
    );
}
