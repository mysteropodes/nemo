//! Independent verifier of staged export bytes (G44).
//!
//! Everything is read back from the BYTES of each staged file; nothing is taken
//! from a field an encoder reported. For a plan batch (or the whole plan) it
//! checks the file count, the exact names in strictly ascending order, and for
//! every file:
//! - PNG: signature, every chunk's length and CRC-32, `IHDR` first with a
//!   non-zero width/height, at least one `IDAT`, `IEND` last with no trailing
//!   bytes. The pixel stream is not inflated (no decoder is involved);
//! - SVG: UTF-8, the baseline XML preamble, a root `svg` start tag with the SVG
//!   namespace and plain-integer `width` / `height` whose `viewBox` is
//!   `0 0 <width> <height>`, closed by `</svg>` with nothing after it;
//! and that the width and height equal the pinned [`OutputSpec`] dimensions of
//! the plan. A mismatch is a typed [`VerifyFailure`], never a clamp or a retry.

use crate::export_job::native_media_export::native_media_export_plan::{
    FrameFormat, MediaExportPlan,
};

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const SVG_PREAMBLE: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";

/// One staged file as the artifact port holds it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StagedFile<'a> {
    pub(crate) name: &'a str,
    pub(crate) bytes: &'a [u8],
}

/// Which names a batch's files carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NameSpace {
    /// What a host job over just this batch stages (`frame_0001..` from 1).
    Staged,
    /// The names in the complete output.
    Final,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedFile {
    pub(crate) name: String,
    pub(crate) byte_len: usize,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// PNG `(bit depth, colour type)` from `IHDR`; `None` for SVG.
    pub(crate) png_format: Option<(u8, u8)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedOutput {
    pub(crate) files: Vec<VerifiedFile>,
    pub(crate) total_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VerifyFailure {
    UnknownBatch {
        index: u64,
    },
    CountMismatch {
        expected: u64,
        actual: usize,
    },
    OutOfOrder {
        index: usize,
    },
    NameMismatch {
        index: usize,
        expected: String,
        actual: String,
    },
    EmptyFile {
        name: String,
    },
    Malformed {
        name: String,
        reason: &'static str,
    },
    DimensionMismatch {
        name: String,
        expected: (u32, u32),
        actual: (u32, u32),
    },
}

pub(crate) fn verify_batch(
    plan: &MediaExportPlan,
    batch_index: u64,
    space: NameSpace,
    files: &[StagedFile<'_>],
) -> Result<VerifiedOutput, VerifyFailure> {
    let batch = plan
        .batch(batch_index)
        .ok_or(VerifyFailure::UnknownBatch { index: batch_index })?;
    let expected = match space {
        NameSpace::Staged => batch.staged_names,
        NameSpace::Final => batch.final_names,
    };
    check_files(plan, &expected, files)
}

/// The complete published output: every manifest name, in order.
pub(crate) fn verify_output(
    plan: &MediaExportPlan,
    files: &[StagedFile<'_>],
) -> Result<VerifiedOutput, VerifyFailure> {
    if plan.frame_count() != files.len() as u64 {
        return Err(VerifyFailure::CountMismatch {
            expected: plan.frame_count(),
            actual: files.len(),
        });
    }
    let expected: Vec<String> = (1..=plan.frame_count())
        .filter_map(|ordinal| plan.entry(ordinal).map(|entry| entry.name))
        .collect();
    check_files(plan, &expected, files)
}

fn check_files(
    plan: &MediaExportPlan,
    expected: &[String],
    files: &[StagedFile<'_>],
) -> Result<VerifiedOutput, VerifyFailure> {
    if expected.len() != files.len() {
        return Err(VerifyFailure::CountMismatch {
            expected: expected.len() as u64,
            actual: files.len(),
        });
    }
    // Ordering first: a swap or duplicate is reported as such, not as a name gap.
    let mut previous: Option<u64> = None;
    for (index, file) in files.iter().enumerate() {
        if let Some(ordinal) = ordinal_of(file.name, plan.format()) {
            if previous.is_some_and(|last| ordinal <= last) {
                return Err(VerifyFailure::OutOfOrder { index });
            }
            previous = Some(ordinal);
        }
    }
    let pinned = plan.output().dimensions();
    let mut verified = Vec::with_capacity(files.len());
    let mut total_bytes = 0u64;
    for (index, (want, file)) in expected.iter().zip(files).enumerate() {
        if file.name != want {
            return Err(VerifyFailure::NameMismatch {
                index,
                expected: want.clone(),
                actual: file.name.to_owned(),
            });
        }
        if file.bytes.is_empty() {
            return Err(VerifyFailure::EmptyFile {
                name: file.name.to_owned(),
            });
        }
        let checked = match plan.format() {
            FrameFormat::Png => check_png(file.bytes),
            FrameFormat::Svg => check_svg(file.bytes),
        };
        let (width, height, png_format) = checked.map_err(|reason| VerifyFailure::Malformed {
            name: file.name.to_owned(),
            reason,
        })?;
        if (width, height) != pinned {
            return Err(VerifyFailure::DimensionMismatch {
                name: file.name.to_owned(),
                expected: pinned,
                actual: (width, height),
            });
        }
        total_bytes += file.bytes.len() as u64;
        verified.push(VerifiedFile {
            name: file.name.to_owned(),
            byte_len: file.bytes.len(),
            width,
            height,
            png_format,
        });
    }
    Ok(VerifiedOutput {
        files: verified,
        total_bytes,
    })
}

/// The ordinal in `frame_<digits>.<ext>`, if the name has that shape.
fn ordinal_of(name: &str, format: FrameFormat) -> Option<u64> {
    let digits = name
        .strip_prefix("frame_")?
        .strip_suffix(format.extension())?
        .strip_suffix('.')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

type Checked = Result<(u32, u32, Option<(u8, u8)>), &'static str>;

fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    for (index, slot) in table.iter_mut().enumerate() {
        let mut value = index as u32;
        for _ in 0..8 {
            value = if value & 1 == 1 {
                0xEDB8_8320 ^ (value >> 1)
            } else {
                value >> 1
            };
        }
        *slot = value;
    }
    table
}

fn crc32(table: &[u32; 256], data: &[u8]) -> u32 {
    let mut value = u32::MAX;
    for &byte in data {
        value = table[((value ^ u32::from(byte)) & 0xFF) as usize] ^ (value >> 8);
    }
    !value
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn check_png(bytes: &[u8]) -> Checked {
    if bytes.len() < PNG_SIGNATURE.len() || bytes[..8] != PNG_SIGNATURE {
        return Err("missing PNG signature");
    }
    let table = crc_table();
    let mut position = 8usize;
    let mut header: Option<(u32, u32, u8, u8)> = None;
    let mut has_data = false;
    loop {
        if position == bytes.len() {
            return Err("missing IEND chunk");
        }
        if bytes.len() - position < 12 {
            return Err("truncated chunk header");
        }
        let length = be_u32(&bytes[position..]) as usize;
        let kind = &bytes[position + 4..position + 8];
        let data_end = position
            .checked_add(8)
            .and_then(|start| start.checked_add(length))
            .ok_or("chunk length overflows")?;
        let end = data_end.checked_add(4).ok_or("chunk length overflows")?;
        if end > bytes.len() {
            return Err("truncated chunk");
        }
        if crc32(&table, &bytes[position + 4..data_end]) != be_u32(&bytes[data_end..]) {
            return Err("chunk CRC mismatch");
        }
        if header.is_none() {
            if kind != b"IHDR" || length != 13 {
                return Err("first chunk is not a 13 byte IHDR");
            }
            let data = &bytes[position + 8..data_end];
            let (width, height) = (be_u32(data), be_u32(&data[4..]));
            if width == 0 || height == 0 {
                return Err("IHDR has a zero dimension");
            }
            if data[10] != 0 || data[11] != 0 || data[12] > 1 {
                return Err("IHDR method is not compression 0, filter 0, interlace 0 or 1");
            }
            header = Some((width, height, data[8], data[9]));
        } else if kind == b"IHDR" {
            return Err("duplicate IHDR");
        }
        has_data |= kind == b"IDAT";
        position = end;
        if kind == b"IEND" {
            if length != 0 {
                return Err("IEND carries data");
            }
            if !has_data {
                return Err("no IDAT before IEND");
            }
            if position != bytes.len() {
                return Err("bytes after IEND");
            }
            let (width, height, depth, color) = header.ok_or("missing IHDR")?;
            return Ok((width, height, Some((depth, color))));
        }
    }
}

fn set_once<'a>(slot: &mut Option<&'a str>, value: &'a str) -> Result<(), &'static str> {
    if slot.is_some() {
        return Err("duplicate root attribute");
    }
    *slot = Some(value);
    Ok(())
}

fn plain_dimension(text: &str) -> Option<u32> {
    let plain = !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    if plain {
        text.parse().ok()
    } else {
        None
    }
}

fn check_svg(bytes: &[u8]) -> Checked {
    let text = std::str::from_utf8(bytes).map_err(|_| "SVG is not UTF-8")?;
    let rest = text
        .strip_prefix(SVG_PREAMBLE)
        .ok_or("missing XML preamble")?;
    let rest = rest
        .strip_prefix("<svg ")
        .ok_or("root element is not svg")?;
    let tag_end = rest.find('>').ok_or("unterminated root tag")?;
    if !rest[tag_end + 1..].ends_with("</svg>") {
        return Err("missing closing svg tag");
    }
    let (mut namespace, mut width, mut height, mut view_box) = (None, None, None, None);
    let mut cursor = &rest[..tag_end];
    loop {
        cursor = cursor.trim_start_matches(' ');
        if cursor.is_empty() {
            break;
        }
        let (name, after) = cursor.split_once("=\"").ok_or("malformed root attribute")?;
        let (value, remainder) = after
            .split_once('"')
            .ok_or("unterminated attribute value")?;
        match name {
            "xmlns" => set_once(&mut namespace, value)?,
            "width" => set_once(&mut width, value)?,
            "height" => set_once(&mut height, value)?,
            "viewBox" => set_once(&mut view_box, value)?,
            _ => {}
        }
        cursor = remainder;
    }
    if namespace != Some(SVG_NAMESPACE) {
        return Err("missing SVG namespace");
    }
    let width_text = width.ok_or("missing width")?;
    let height_text = height.ok_or("missing height")?;
    let parsed_width = plain_dimension(width_text).ok_or("width is not a plain integer")?;
    let parsed_height = plain_dimension(height_text).ok_or("height is not a plain integer")?;
    if view_box != Some(format!("0 0 {width_text} {height_text}").as_str()) {
        return Err("viewBox is not 0 0 width height");
    }
    Ok((parsed_width, parsed_height, None))
}

#[cfg(test)]
#[path = "../tests/native_media_export_verify_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/native_media_export_verify_stage.rs"]
mod stage;
#[cfg(test)]
#[path = "../tests/native_media_export_verify.rs"]
mod tests;
