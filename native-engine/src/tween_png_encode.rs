//! Bounded brush PNG serialization on the existing readback allocation budget.
use crate::read_queries::{reserve_readback_bytes, ReadbackReservation};
use std::io::{Cursor, Write};

pub(crate) struct BrushPng {
    bytes: Vec<u8>,
    _charge: ReadbackReservation,
}
impl AsRef<[u8]> for BrushPng {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

pub(crate) fn encode_brush_png(
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<BrushPng, &'static str> {
    let raw = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .filter(|&n| n > 0 && n == rgba.len())
        .ok_or("invalid brush dimensions")?;
    // A conservative fixed buffer includes scanlines, deflate and chunk overhead.
    // Cursor over a slice cannot grow; an unexpectedly larger PNG refuses cleanly.
    let capacity = raw
        .checked_add(raw / 16)
        .and_then(|n| n.checked_add(65536))
        .filter(|&n| n <= 16 << 20)
        .ok_or("brush PNG capacity unavailable")?;
    let mut charge = reserve_readback_bytes(capacity)?;
    let mut bytes = charge.allocate()?;
    let mut cursor = Cursor::new(bytes.as_mut_slice());
    {
        let mut encoder = png::Encoder::new(&mut cursor, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_filter(png::Filter::NoFilter);
        encoder.set_compression(png::Compression::High);
        let mut writer = encoder
            .write_header()
            .map_err(|_| "brush PNG header refused")?;
        {
            let mut stream = writer
                .stream_writer_with_size(4096)
                .map_err(|_| "brush PNG stream refused")?;
            stream
                .write_all(rgba)
                .map_err(|_| "brush PNG capacity unavailable")?;
            stream.finish().map_err(|_| "brush PNG stream incomplete")?;
        }
        writer.finish().map_err(|_| "brush PNG incomplete")?;
    }
    let length = cursor.position() as usize;
    bytes.truncate(length);
    Ok(BrushPng {
        bytes,
        _charge: charge,
    })
}
