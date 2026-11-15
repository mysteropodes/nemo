//! Bitmap entries under `bin/` (the `bitmapDataHRef` of a `DOMBitmapItem`).
//!
//! Animate stores three kinds of entries:
//! - an encoded image kept as-is (JPEG, and PNG/GIF for some imports),
//!   returned untouched with its media type;
//! - its own lossless ARGB format (magic `03 05`), decoded here to straight
//!   (non-premultiplied) RGBA;
//! - an 8-bit palette format (magic `03 03`), reported as unsupported for now.
//!
//! Lossless layout, as described by the public XFL references (to be
//! confirmed on real files): magic u16, row stride u16, width u16, height u16,
//! frame left/right/top/bottom u32 (twips), has-alpha u8, compressed u8, then
//! the zlib stream split into chunks, each prefixed by its u16 length, ending
//! with a zero length. Pixels are premultiplied ARGB, row by row. Integers are
//! little-endian.

use std::io::Read;

use serde::Serialize;

/// Upper bound on a decoded bitmap (Animate caps bitmaps at 8192×8192).
pub const MAX_PIXELS: u64 = 8192 * 8192;

/// Budget for all bitmaps of one document together; later bitmaps past it
/// are reported instead of decoded.
pub const MAX_TOTAL_DECODED_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BitmapData {
    /// Encoded file bytes kept as stored.
    Encoded {
        mime: String,
        #[serde(skip)]
        bytes: Vec<u8>,
        byte_len: usize,
    },
    /// Decoded straight RGBA8, `width * height * 4` bytes.
    Rgba {
        width: u32,
        height: u32,
        #[serde(skip)]
        pixels: Vec<u8>,
    },
}

impl BitmapData {
    /// Bytes held in memory for this bitmap.
    pub fn byte_len(&self) -> usize {
        match self {
            BitmapData::Encoded { bytes, .. } => bytes.len(),
            BitmapData::Rgba { pixels, .. } => pixels.len(),
        }
    }
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

/// Reads a list of byte slices as one stream.
struct ChunkReader<'a> {
    chunks: &'a [&'a [u8]],
    index: usize,
    offset: usize,
}

impl Read for ChunkReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        while let Some(chunk) = self.chunks.get(self.index) {
            let rest = &chunk[self.offset..];
            if rest.is_empty() {
                self.index += 1;
                self.offset = 0;
                continue;
            }
            let n = rest.len().min(buf.len());
            buf[..n].copy_from_slice(&rest[..n]);
            self.offset += n;
            return Ok(n);
        }
        Ok(0)
    }
}

const HEADER_LEN: usize = 2 + 2 + 2 + 2 + 16 + 1 + 1;

/// Decode one `bin/*.dat` entry.
pub fn decode_bitmap(data: &[u8]) -> Result<BitmapData, String> {
    let encoded = |mime: &str| BitmapData::Encoded {
        mime: mime.to_string(),
        bytes: data.to_vec(),
        byte_len: data.len(),
    };
    if data.starts_with(&[0xFF, 0xD8]) {
        return Ok(encoded("image/jpeg"));
    }
    if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Ok(encoded("image/png"));
    }
    if data.starts_with(b"GIF8") {
        return Ok(encoded("image/gif"));
    }
    if data.starts_with(&[0x03, 0x03]) {
        return Err("8-bit palette bitmap (not decoded yet)".into());
    }
    if !data.starts_with(&[0x03, 0x05]) {
        return Err("unrecognised bitmap data".into());
    }
    let short = || "truncated lossless bitmap header".to_string();
    let stride = u16_at(data, 2).ok_or_else(short)? as usize;
    let width = u16_at(data, 4).ok_or_else(short)? as u32;
    let height = u16_at(data, 6).ok_or_else(short)? as u32;
    let has_alpha = *data.get(24).ok_or_else(short)? != 0;
    let compressed = *data.get(25).ok_or_else(short)? != 0;
    if width == 0 || height == 0 || (width as u64) * (height as u64) > MAX_PIXELS {
        return Err(format!("bitmap size {width}×{height} out of range"));
    }
    if stride != width as usize * 4 {
        return Err(format!("unexpected row stride {stride} for width {width}"));
    }
    let expected = width as usize * height as usize * 4;

    // Collect chunk slices without copying them.
    let mut chunks: Vec<&[u8]> = Vec::new();
    let mut at = HEADER_LEN;
    loop {
        let len = u16_at(data, at)
            .ok_or_else(|| "bitmap chunk list ends without a terminator".to_string())?
            as usize;
        at += 2;
        if len == 0 {
            break;
        }
        let chunk = data
            .get(at..at + len)
            .ok_or_else(|| "bitmap chunk runs past the end".to_string())?;
        chunks.push(chunk);
        at += len;
    }
    let mut stream = ChunkReader {
        chunks: &chunks,
        index: 0,
        offset: 0,
    };
    // The buffer grows with the actual output: header sizes alone never
    // reserve memory, and `take` bounds a decompression bomb.
    let mut pixels = Vec::new();
    let read = if compressed {
        flate2::read::ZlibDecoder::new(stream)
            .take(expected as u64 + 1)
            .read_to_end(&mut pixels)
    } else {
        stream
            .by_ref()
            .take(expected as u64 + 1)
            .read_to_end(&mut pixels)
    };
    read.map_err(|e| format!("bitmap zlib stream: {e}"))?;
    if pixels.len() != expected {
        return Err(format!(
            "bitmap holds {} bytes, expected {expected}",
            pixels.len()
        ));
    }
    // Premultiplied ARGB → straight RGBA, in place.
    for px in pixels.chunks_exact_mut(4) {
        let a = if has_alpha { px[0] } else { 255 };
        let unmul = |c: u8| -> u8 {
            if a == 0 {
                0
            } else if a == 255 {
                c
            } else {
                ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8
            }
        };
        let (r, g, b) = (unmul(px[1]), unmul(px[2]), unmul(px[3]));
        px.copy_from_slice(&[r, g, b, a]);
    }
    Ok(BitmapData::Rgba {
        width,
        height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    pub(crate) fn lossless(
        width: u16,
        height: u16,
        argb: &[u8],
        has_alpha: bool,
        chunk: usize,
    ) -> Vec<u8> {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(argb).unwrap();
        let stream = z.finish().unwrap();
        let mut out = vec![0x03, 0x05];
        out.extend_from_slice(&(width * 4).to_le_bytes());
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        for v in [0u32, width as u32 * 20, 0, height as u32 * 20] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.push(has_alpha as u8);
        out.push(1);
        for c in stream.chunks(chunk) {
            out.extend_from_slice(&(c.len() as u16).to_le_bytes());
            out.extend_from_slice(c);
        }
        out.extend_from_slice(&[0, 0]);
        out
    }

    #[test]
    fn lossless_argb_is_unpremultiplied() {
        // 2×1: opaque red, then half-transparent white (premultiplied 128,128,128).
        let argb = [255, 255, 0, 0, 128, 128, 128, 128];
        let BitmapData::Rgba {
            width,
            height,
            pixels,
        } = decode_bitmap(&lossless(2, 1, &argb, true, 3)).unwrap()
        else {
            panic!()
        };
        assert_eq!((width, height), (2, 1));
        assert_eq!(pixels, vec![255, 0, 0, 255, 255, 255, 255, 128]);
    }

    #[test]
    fn opaque_flag_forces_full_alpha() {
        let argb = [0, 10, 20, 30];
        let BitmapData::Rgba { pixels, .. } =
            decode_bitmap(&lossless(1, 1, &argb, false, 64)).unwrap()
        else {
            panic!()
        };
        assert_eq!(pixels, vec![10, 20, 30, 255]);
    }

    #[test]
    fn encoded_formats_pass_through() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 1, 2];
        assert_eq!(
            decode_bitmap(&jpeg).unwrap(),
            BitmapData::Encoded {
                mime: "image/jpeg".into(),
                bytes: jpeg.to_vec(),
                byte_len: 6
            }
        );
        assert!(
            matches!(decode_bitmap(b"\x89PNG\r\n").unwrap(), BitmapData::Encoded { mime, .. } if mime == "image/png")
        );
    }

    #[test]
    fn malformed_data_is_an_error_not_a_panic() {
        let good = lossless(2, 2, &[0; 16], true, 8);
        for cut in 0..good.len() {
            let _ = decode_bitmap(&good[..cut]);
        }
        assert!(decode_bitmap(&good[..good.len() - 2]).is_err());
        assert!(decode_bitmap(&lossless(2, 2, &[0; 12], true, 8)).is_err()); // too few pixels
        assert!(decode_bitmap(&[0x03, 0x03, 0, 0]).is_err());
        assert!(decode_bitmap(b"junk").is_err());
        let mut huge = good.clone();
        huge[4..6].copy_from_slice(&u16::MAX.to_le_bytes());
        huge[6..8].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(decode_bitmap(&huge).is_err());
    }
}
