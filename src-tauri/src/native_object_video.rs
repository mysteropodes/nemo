//! T12: the platform video decoder the native preview/export owner uses, installed
//! once by the desktop shell (this seam may not depend on the shell's media
//! module). Without an installed decoder a drawn video is a typed refusal when
//! its session opens; frames without video never need one.
use native_engine::native_media_frame_export::{
    with_placement_source_frame_count, NativeVideoDecoder, NativeVideoInfo,
};
use native_engine::{commands::ResponseEnvelope, object_snapshot::ObjectSnapshot};
use std::sync::OnceLock;

static FACTORY: OnceLock<fn() -> Box<dyn NativeVideoDecoder>> = OnceLock::new();

/// Called once by the shell before any project opens; a second call is ignored.
pub(crate) fn install_video_decoder(factory: fn() -> Box<dyn NativeVideoDecoder>) {
    let _ = FACTORY.set(factory);
}

/// A fresh decoder, owned by one export or one preview frame.
pub(crate) fn video_decoder() -> Box<dyn NativeVideoDecoder> {
    FACTORY
        .get()
        .map_or_else(|| Box::new(Uninstalled) as _, |factory| factory())
}

struct Uninstalled;
impl NativeVideoDecoder for Uninstalled {
    fn open(&mut self, path: &str) -> Result<NativeVideoInfo, String> {
        Err(format!(
            "no native video decoder is installed to open {path}"
        ))
    }
    fn decode(&mut self, session: u64, _: u32) -> Result<Vec<u8>, String> {
        Err(format!("video session {session} is not open"))
    }
    fn close(&mut self, session: u64) -> Result<(), String> {
        Err(format!("video session {session} is not open"))
    }
}

/// The placement read's `sourceFrameCount`, from a session opened on the read's
/// own snapshot and asset and closed once (2232/6287401377); other reads pass.
pub(super) fn with_source_frame_count(
    response: ResponseEnvelope,
    snapshot: &ObjectSnapshot,
) -> ResponseEnvelope {
    if response
        .result()
        .is_none_or(|result| result["kind"] != "layerPlacement")
    {
        return response;
    }
    let mut decoder = video_decoder();
    with_placement_source_frame_count(response, snapshot, decoder.as_mut(), &linked_file_readable)
}

/// Whether a linked desktop video path can be read now (the MissingLink probe).
pub(super) fn linked_file_readable(path: &str) -> bool {
    std::fs::File::open(path).is_ok_and(|file| file.metadata().is_ok_and(|m| m.is_file()))
}

/// Wall-clock milliseconds, the link verdict timestamp.
pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unreadable_link_is_missing() {
        let dir = std::env::temp_dir().join(format!("nemo-t12-link-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("clip.mov");
        std::fs::write(&file, b"x").unwrap();
        assert!(linked_file_readable(&file.to_string_lossy()));
        assert!(!linked_file_readable(
            &dir.join("gone.mov").to_string_lossy()
        ));
        assert!(!linked_file_readable(&dir.to_string_lossy()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn without_an_installed_decoder_opening_a_video_is_refused() {
        let mut decoder = Uninstalled;
        assert!(decoder
            .open("clip.mov")
            .unwrap_err()
            .contains("no native video decoder"));
        assert!(decoder.close(0).is_err());
    }
}
