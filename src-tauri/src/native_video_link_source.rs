//! T12: scoped linked-video facts for `command.document.object.asset.video.link`.
//! Canonical path, the image import filesystem scope, regular UTF-8 file, then one
//! probe-then-close session of the installed native decoder. No bytes are read here
//! beyond what the decoder needs to report its dimensions and frame count.
use crate::application_mcp::image_source::ImageReadPermission;
use native_engine::{commands::LinkedVideoFacts, native_media_frame_export::NativeVideoDecoder};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The engine's own locator/path bound.
const MAX_LOCATOR_BYTES: usize = 4096;

pub(crate) type DecoderFactory = fn() -> Box<dyn NativeVideoDecoder>;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum VideoSource {
    File { path: String },
}

/// Every refusal happens before the decoder opens, except the decoder's own verdict;
/// an opened session is always closed, whatever the reported facts.
pub(crate) fn probe(
    source: VideoSource,
    allowed: &dyn Fn(&Path) -> bool,
    decoder: &mut dyn NativeVideoDecoder,
) -> Result<LinkedVideoFacts, String> {
    let VideoSource::File { path } = source;
    if path.contains('\0') || !Path::new(&path).is_absolute() {
        return Err("invalid native video source path".into());
    }
    let resolved = PathBuf::from(path)
        .canonicalize()
        .map_err(|_| "native video source unavailable")?;
    if !allowed(&resolved) {
        return Err("native video source outside filesystem scope".into());
    }
    let metadata = std::fs::metadata(&resolved).map_err(|_| "native video source unavailable")?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("native video source is not a regular file".into());
    }
    let locator = resolved
        .to_str()
        .filter(|locator| !locator.is_empty() && locator.len() <= MAX_LOCATOR_BYTES)
        .ok_or("native video source path is not bounded UTF-8")?
        .to_owned();
    let info = decoder
        .open(&locator)
        .map_err(|_| "native video source could not be decoded")?;
    let closed = decoder.close(info.session);
    if info.width == 0 || info.height == 0 || info.frame_count == 0 {
        return Err("native video source reports no frames or dimensions".into());
    }
    closed.map_err(|_| "native video probe session did not close")?;
    Ok(LinkedVideoFacts {
        locator,
        width: info.width,
        height: info.height,
        frame_count: info.frame_count,
    })
}

/// Off-lock probe with the decoder the shell installed.
pub(crate) async fn resolve(
    source: VideoSource,
    allowed: ImageReadPermission,
) -> Result<LinkedVideoFacts, String> {
    resolve_with(
        source,
        allowed,
        crate::native_dispatch::video::video_decoder,
    )
    .await
}

/// The decoder is created on the blocking worker, so it never crosses threads.
pub(crate) async fn resolve_with(
    source: VideoSource,
    allowed: ImageReadPermission,
    decoder: DecoderFactory,
) -> Result<LinkedVideoFacts, String> {
    tokio::task::spawn_blocking(move || {
        let mut decoder = decoder();
        probe(source, allowed.as_ref(), decoder.as_mut())
    })
    .await
    .map_err(|_| "native video source worker unavailable".to_owned())?
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::application_mcp::revision_sync::video_dispatch::video_source::*;
    use native_engine::native_media_frame_export::NativeVideoInfo;

    pub(crate) struct FakeDecoder {
        pub(crate) info: Result<NativeVideoInfo, String>,
        pub(crate) opened: Vec<String>,
        pub(crate) closed: Vec<u64>,
    }
    impl FakeDecoder {
        pub(crate) fn frames(frame_count: u32) -> Self {
            Self {
                info: Ok(NativeVideoInfo {
                    session: 7,
                    width: 64,
                    height: 36,
                    frame_count,
                }),
                opened: Vec::new(),
                closed: Vec::new(),
            }
        }
    }
    impl NativeVideoDecoder for FakeDecoder {
        fn open(&mut self, path: &str) -> Result<NativeVideoInfo, String> {
            self.opened.push(path.to_owned());
            self.info.clone()
        }
        fn decode(&mut self, _: u64, _: u32) -> Result<Vec<u8>, String> {
            Err("probe never decodes".into())
        }
        fn close(&mut self, session: u64) -> Result<(), String> {
            self.closed.push(session);
            Ok(())
        }
    }

    fn fixture() -> (PathBuf, PathBuf) {
        let directory =
            std::env::temp_dir().join(format!("nemo-video-source-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("clip.mov");
        std::fs::write(&path, b"container bytes").unwrap();
        (directory, path)
    }
    fn file(path: &Path) -> VideoSource {
        VideoSource::File {
            path: path.to_str().unwrap().to_owned(),
        }
    }

    #[test]
    fn scoped_regular_file_is_probed_once_and_its_session_closed() {
        let (directory, path) = fixture();
        let canonical = path.canonicalize().unwrap();
        let mut decoder = FakeDecoder::frames(48);
        let facts = probe(file(&path), &|seen| seen == canonical, &mut decoder).unwrap();
        assert_eq!(facts.locator, canonical.to_str().unwrap());
        assert_eq!((facts.width, facts.height, facts.frame_count), (64, 36, 48));
        assert_eq!(decoder.opened, vec![facts.locator.clone()]);
        assert_eq!(decoder.closed, vec![7]);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn scope_relative_directory_and_nul_paths_refuse_before_the_decoder_opens() {
        let (directory, path) = fixture();
        for (source, allowed) in [
            (file(&path), false),
            (
                VideoSource::File {
                    path: "relative/clip.mov".into(),
                },
                true,
            ),
            (file(&directory), true),
            (
                VideoSource::File {
                    path: format!("{}\0", path.to_str().unwrap()),
                },
                true,
            ),
            (file(&directory.join("absent.mov")), true),
        ] {
            let mut decoder = FakeDecoder::frames(48);
            assert!(probe(source, &|_| allowed, &mut decoder).is_err());
            assert!(decoder.opened.is_empty() && decoder.closed.is_empty());
        }
        std::fs::write(directory.join("empty.mov"), b"").unwrap();
        let mut decoder = FakeDecoder::frames(48);
        assert!(probe(file(&directory.join("empty.mov")), &|_| true, &mut decoder).is_err());
        assert!(decoder.opened.is_empty());
        assert!(serde_json::from_value::<VideoSource>(
            serde_json::json!({"kind":"file","path":"/clip.mov","bytes":"page bytes"})
        )
        .is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn decoder_failure_and_zero_facts_refuse_and_an_opened_session_still_closes() {
        let (directory, path) = fixture();
        let mut failing = FakeDecoder::frames(48);
        failing.info = Err("unsupported codec".into());
        assert!(probe(file(&path), &|_| true, &mut failing).is_err());
        assert!(
            failing.closed.is_empty(),
            "nothing opened, nothing to close"
        );
        for zeroed in ["frames", "width", "height"] {
            let mut decoder = FakeDecoder::frames(if zeroed == "frames" { 0 } else { 48 });
            if let Ok(info) = decoder.info.as_mut() {
                match zeroed {
                    "width" => info.width = 0,
                    "height" => info.height = 0,
                    _ => {}
                }
            }
            assert!(probe(file(&path), &|_| true, &mut decoder).is_err());
            assert_eq!(decoder.closed, vec![7], "{zeroed}");
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
