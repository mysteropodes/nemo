//! T12: the export/preview decoder port over real ffmpeg sessions, with two
//! lossless sources whose pixels are a known function of (x, y, frame).
use super::tests::{ffmpeg_shared, gen_video};
use super::DesktopVideoDecoder;
use native_engine::native_media_frame_export::NativeVideoDecoder;

// Source A: 16x8, 20 frames, pixel (x, y, n) = (10n, 8x, 16y, 255).
fn source_a() -> String {
    gen_video(
        "t12_a_16x8_20f.mov",
        "nullsrc=s=16x8:r=10:d=2,format=rgba,geq=r='10*N':g='8*X':b='16*Y':a=255",
        &["-c:v", "png"],
    )
    .to_string_lossy()
    .into_owned()
}

// Source B: 8x6, 12 frames, pixel (x, y, n) = (200, 5n, 30x, 255).
fn source_b() -> String {
    gen_video(
        "t12_b_8x6_12f.mov",
        "nullsrc=s=8x6:r=12:d=1,format=rgba,geq=r=200:g='5*N':b='30*X':a=255",
        &["-c:v", "png"],
    )
    .to_string_lossy()
    .into_owned()
}

fn pixel(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    rgba[at..at + 4].try_into().unwrap()
}

#[test]
fn two_sources_decode_their_own_exact_pixels_at_the_requested_frame() {
    let (a, b) = (source_a(), source_b());
    let _ffmpeg = ffmpeg_shared(); // after fixture generation: never nested
    let mut decoder = DesktopVideoDecoder::default();
    let ia = decoder.open(&a).unwrap();
    let ib = decoder.open(&b).unwrap();
    assert_ne!(ia.session, ib.session);
    assert_eq!((ia.width, ia.height, ia.frame_count), (16, 8, 20));
    assert_eq!((ib.width, ib.height, ib.frame_count), (8, 6, 12));
    // Out of order and interleaved: each answer is its own source's frame.
    for (frame_a, frame_b) in [(3u32, 7u32), (11, 2), (0, 11), (19, 0)] {
        let pa = decoder.decode(ia.session, frame_a).unwrap();
        let pb = decoder.decode(ib.session, frame_b).unwrap();
        assert_eq!(pa.len(), 16 * 8 * 4);
        assert_eq!(pb.len(), 8 * 6 * 4);
        assert_eq!(pixel(&pa, 16, 5, 3), [10 * frame_a as u8, 40, 48, 255]);
        assert_eq!(pixel(&pb, 8, 4, 1), [200, 5 * frame_b as u8, 120, 255]);
    }
    decoder.close(ia.session).unwrap();
    decoder.close(ib.session).unwrap();
}

#[test]
fn a_session_closes_exactly_once_and_cannot_decode_after() {
    let a = source_a();
    let _ffmpeg = ffmpeg_shared(); // after fixture generation: never nested
    let mut decoder = DesktopVideoDecoder::default();
    let info = decoder.open(&a).unwrap();
    decoder.decode(info.session, 4).unwrap();
    decoder.close(info.session).unwrap();
    assert!(decoder.close(info.session).is_err());
    assert!(decoder.decode(info.session, 4).is_err());
    assert!(decoder.sessions.is_empty());
}

#[test]
fn dropping_the_decoder_closes_every_session_still_open() {
    let a = source_a();
    let _ffmpeg = ffmpeg_shared(); // after fixture generation: never nested
    let mut decoder = DesktopVideoDecoder::default();
    let first = decoder.open(&a).unwrap();
    let second = decoder.open(&a).unwrap();
    decoder.decode(first.session, 1).unwrap();
    decoder.decode(second.session, 9).unwrap();
    let running = decoder
        .sessions
        .values()
        .filter(|s| s.proc.is_some())
        .count();
    assert_eq!(running, 2, "both sessions hold a live ffmpeg child");
    drop(decoder); // must kill and reap both children without panicking
}

/// Decodes `frames` of `path` through the real export port, closing the session:
/// ((width, height, frame_count), RGBA per frame). Shared with the T12 host test.
pub(super) fn decoded_frames(path: &str, frames: &[u32]) -> ((u32, u32, u32), Vec<Vec<u8>>) {
    let mut decoder = DesktopVideoDecoder::default();
    let info = decoder.open(path).unwrap();
    let out = frames
        .iter()
        .map(|&n| decoder.decode(info.session, n).unwrap())
        .collect();
    decoder.close(info.session).unwrap();
    ((info.width, info.height, info.frame_count), out)
}
