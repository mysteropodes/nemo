//! T12: the native export/preview decoder over the same ffmpeg sessions the
//! webview uses, owned by one export or preview frame instead of the IPC registry.
//!
//! Every session opened here is closed exactly once: by `close` (the media gate
//! closes each one it opened) or, as a safety net, when the decoder drops.
//! No readahead thread is spawned: export reads frames in order, so the running
//! sequential stream already serves the next one.
use super::{decode_frame_core, open_session_core, shutdown_session, VideoSession};
use native_engine::native_media_frame_export::{NativeVideoDecoder, NativeVideoInfo};
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct DesktopVideoDecoder {
    pub(super) sessions: BTreeMap<u64, VideoSession>,
    next: u64,
}

impl NativeVideoDecoder for DesktopVideoDecoder {
    fn open(&mut self, path: &str) -> Result<NativeVideoInfo, String> {
        let (session, frame_count, _) = open_session_core(path)?;
        let frame_count = u32::try_from(frame_count)
            .map_err(|_| format!("{path} has more frames than a native export can address"))?;
        let info = NativeVideoInfo {
            session: self.next,
            width: session.width,
            height: session.height,
            frame_count,
        };
        self.sessions.insert(self.next, session);
        self.next += 1;
        Ok(info)
    }

    fn decode(&mut self, session: u64, source_frame: u32) -> Result<Vec<u8>, String> {
        let s = self
            .sessions
            .get_mut(&session)
            .ok_or_else(|| format!("video session {session} is not open"))?;
        decode_frame_core(s, i64::from(source_frame)).map(|(rgba, _seeked)| rgba)
    }

    fn close(&mut self, session: u64) -> Result<(), String> {
        let mut s = self
            .sessions
            .remove(&session)
            .ok_or_else(|| format!("video session {session} is not open"))?;
        shutdown_session(&mut s);
        Ok(())
    }
}

impl Drop for DesktopVideoDecoder {
    fn drop(&mut self) {
        for (_, mut s) in std::mem::take(&mut self.sessions) {
            shutdown_session(&mut s);
        }
    }
}

/// The shell's decoder factory for the native preview/export owner.
pub(crate) fn boxed_decoder() -> Box<dyn NativeVideoDecoder> {
    Box::new(DesktopVideoDecoder::default())
}
