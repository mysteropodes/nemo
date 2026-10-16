//! Real staged bytes for the verifier tests: PNGs from the repository's own
//! encoder (`png_output::encode_rgba8`), SVG documents in the baseline wrapper
//! shape, an independent bitwise CRC-32 and hand-built PNG chunks.
use super::*;
use crate::export_job::native_media_export::native_media_export_plan::{
    ExportPins, FrameFormat, FrameRange, MediaExportPlan, OutputKind,
};
use crate::png_output::encode_rgba8;
use crate::scheduler::OutputSpec;
use std::io::Cursor;

pub(super) fn plan(
    format: FrameFormat,
    start: u32,
    end: u32,
    width: u32,
    height: u32,
) -> MediaExportPlan {
    MediaExportPlan::new(
        OutputKind::Sequence,
        format,
        ExportPins {
            instance_id: "instance-1".into(),
            document_id: "document-1".into(),
            content_revision: 7,
            document_snapshot_id: "snapshot-7".into(),
            context_id: "scene-root".into(),
        },
        OutputSpec::new("frame", "rgba8", width, height, "srgb", "straight").unwrap(),
        FrameRange::new(start, end).unwrap(),
        32,
    )
    .unwrap()
}

/// A real RGBA8 PNG whose pixels depend on `seed` (so frames differ).
pub(super) fn png_bytes(width: u32, height: u32, seed: u8) -> Vec<u8> {
    let rgba: Vec<u8> = (0..width * height * 4)
        .map(|i| (i as u8).wrapping_mul(7).wrapping_add(seed))
        .collect();
    let encoded = encode_rgba8(width, height, &rgba).unwrap();
    encoded.to_vec()
}

pub(super) fn svg_text(width: u32, height: u32) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">\n<path d=\"M0,0 Z\"/>\n</svg>"
    )
}

/// A decoder that shares no code with the verifier: `(width, height, bit depth is 8, RGBA)`.
pub(super) fn decoded_header(bytes: &[u8]) -> (u32, u32, bool, bool) {
    let reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let info = reader.info();
    (
        info.width,
        info.height,
        info.bit_depth == png::BitDepth::Eight,
        info.color_type == png::ColorType::Rgba,
    )
}

/// Bitwise CRC-32 (the standard check value of "123456789" is 0xCBF43926).
pub(super) fn crc32_bitwise(data: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

pub(super) fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&crc32_bitwise(&body).to_be_bytes());
    out
}

pub(super) fn ihdr(width: u32, height: u32) -> Vec<u8> {
    let mut data = width.to_be_bytes().to_vec();
    data.extend_from_slice(&height.to_be_bytes());
    data.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &data)
}

pub(super) const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Signature + the given chunks, concatenated.
pub(super) fn assemble(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut out = SIGNATURE.to_vec();
    for part in chunks {
        out.extend_from_slice(part);
    }
    out
}

pub(super) struct Owned {
    pub(super) name: String,
    pub(super) bytes: Vec<u8>,
}

pub(super) fn owned(names: &[String], mut make: impl FnMut(usize) -> Vec<u8>) -> Vec<Owned> {
    names
        .iter()
        .enumerate()
        .map(|(index, name)| Owned {
            name: name.clone(),
            bytes: make(index),
        })
        .collect()
}

pub(super) fn staged(files: &[Owned]) -> Vec<StagedFile<'_>> {
    files
        .iter()
        .map(|file| StagedFile {
            name: &file.name,
            bytes: &file.bytes,
        })
        .collect()
}
