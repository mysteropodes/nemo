//! Signature sniffing and the baseline name table of the admission kernel (L06-D).
use super::oracle::{bmp_with_dib, expect_sniffed, ftyp_with, riff_with, rows, samples};
use super::sniff::{declared_media_type_for_name, extension_of, sniff_media_type};
use crate::object_document::AssetKind;

#[test]
fn every_container_signature_is_recognised_with_its_kind() {
    let table = samples();
    assert_eq!(table.len(), 13);
    for (media_type, kind, bytes) in table {
        assert_eq!(
            sniff_media_type(&bytes),
            Some(expect_sniffed(media_type, kind)),
            "{media_type}"
        );
    }
}

#[test]
fn signature_variants_and_the_mp3_frame_sync_are_recognised() {
    let gif87 = sniff_media_type(b"GIF87a\x01\x00");
    assert_eq!(gif87, Some(expect_sniffed("image/gif", AssetKind::Image)));
    for dib in [12, 40, 52, 56, 64, 108, 124] {
        let got = sniff_media_type(&bmp_with_dib(dib));
        assert_eq!(
            got,
            Some(expect_sniffed("image/bmp", AssetKind::Image)),
            "{dib}"
        );
    }
    for brand in [
        b"avis", b"iso2", b"iso6", b"mp41", b"mp42", b"avc1", b"M4V ", b"dash",
    ] {
        let want = if brand == b"avis" {
            "image/avif"
        } else {
            "video/mp4"
        };
        let got = sniff_media_type(&ftyp_with(brand)).map(|s| s.media_type);
        assert_eq!(got, Some(want), "{brand:?}");
    }
    let sync = sniff_media_type(&[0xFFu8, 0xFB, 0x90, 0x00]);
    assert_eq!(sync, Some(expect_sniffed("audio/mpeg", AssetKind::Audio)));
}

#[test]
fn short_or_look_alike_headers_are_not_recognised() {
    let png = samples().remove(0).2;
    assert_eq!(
        sniff_media_type(&png[..7]),
        None,
        "one byte short of a PNG signature"
    );
    let jpeg = [0xFFu8, 0xD8];
    assert_eq!(
        sniff_media_type(&jpeg),
        None,
        "two bytes are not a JPEG signature"
    );
    assert_eq!(
        sniff_media_type(&riff_with(b"AVI ")),
        None,
        "other RIFF forms"
    );
    assert_eq!(
        sniff_media_type(b"RIFF\x00\x00\x00\x00WEB"),
        None,
        "RIFF cut inside the form"
    );
    assert_eq!(
        sniff_media_type(b"BM\x00\x00"),
        None,
        "BMP shorter than its DIB header"
    );
    assert_eq!(
        sniff_media_type(&bmp_with_dib(41)),
        None,
        "BM with an unknown DIB size"
    );
    for brand in [b"heic", b"mif1", b"M4A ", b"3gp4"] {
        assert_eq!(sniff_media_type(&ftyp_with(brand)), None, "{brand:?}");
    }
    assert_eq!(
        sniff_media_type(b"\x00\x00\x00\x18ftyp"),
        None,
        "ftyp without a brand"
    );
    // Reserved MPEG version, reserved layer, bad bitrate and bad sample rate.
    for header in [
        [0xFFu8, 0xE8, 0x90, 0x00],
        [0xFF, 0xF9, 0x90, 0x00],
        [0xFF, 0xFB, 0xF0, 0x00],
        [0xFF, 0xFB, 0x9C, 0x00],
    ] {
        assert_eq!(sniff_media_type(&header), None, "{header:?}");
    }
    let nothing: [&[u8]; 4] = [
        b"",
        b"<svg xmlns='http://www.w3.org/2000/svg'/>",
        b"OTTO",
        b"wOFF",
    ];
    for bytes in nothing {
        assert_eq!(sniff_media_type(bytes), None, "{bytes:?}");
    }
}

#[test]
fn a_jpeg_is_never_read_as_an_mp3_frame_sync() {
    let jpeg = [0xFFu8, 0xD8, 0xFF, 0xFB, 0x90, 0x00];
    assert_eq!(
        sniff_media_type(&jpeg),
        Some(expect_sniffed("image/jpeg", AssetKind::Image))
    );
}

#[test]
fn every_baseline_mime_row_is_reproduced_by_the_name_table() {
    let table = rows("mime");
    assert_eq!(table.len(), 31);
    let mut strings = 0;
    let mut escapes = 0;
    for row in &table {
        let name = row["name"].as_str().unwrap();
        assert_eq!(extension_of(name), row["ext"].as_str().unwrap(), "{name:?}");
        if row["type"] == "string" {
            strings += 1;
            let mime = row["mime"].as_str().unwrap();
            assert_eq!(declared_media_type_for_name(name), mime, "{name:?}");
        } else {
            // Baseline quirk: `{...}[ext]||'image/png'` returns the inherited
            // Object member for `constructor` / `__proto__`. Native falls back.
            escapes += 1;
            assert!(row["mime"].is_null());
            assert_eq!(declared_media_type_for_name(name), "image/png", "{name:?}");
        }
    }
    assert_eq!((strings, escapes), (29, 2));
}

#[test]
fn literal_name_table_pins() {
    let pins = [
        ("photo.png", "image/png"),
        ("x.JPG", "image/jpeg"),
        ("x.jpeg", "image/jpeg"),
        ("x.svg", "image/svg+xml"),
        ("x.avif", "image/avif"),
        ("clip.mp4", "image/png"),
        ("trailing.", "image/png"),
        ("", "image/png"),
    ];
    for (name, mime) in pins {
        assert_eq!(declared_media_type_for_name(name), mime, "{name:?}");
    }
    assert_eq!(extension_of("archive.tar.gz"), "gz");
    assert_eq!(extension_of("dir.v2/clip"), "v2/clip");
    assert_eq!(extension_of("img.png\n"), "png\n");
    assert_eq!(extension_of(".png"), "png");
}
