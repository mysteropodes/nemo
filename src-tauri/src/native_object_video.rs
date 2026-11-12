//! T12: the platform video decoder the native preview/export owner uses, installed
//! once by the desktop shell (this seam may not depend on the shell's media
//! module). Without an installed decoder a drawn video is a typed refusal when
//! its session opens; frames without video never need one.
use native_engine::native_media_frame_export::{
    with_placement_source_frame_count, NativeVideoDecoder, NativeVideoInfo,
};
use native_engine::{commands::ResponseEnvelope, object_snapshot::ObjectSnapshot};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::SystemTime;

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
    let mut decoder = CachedCount::new(&COUNTS, video_decoder);
    with_placement_source_frame_count(response, snapshot, &mut decoder, &linked_file_readable)
}

/// A remapped layer reads its placement once per frame: the frame count of an
/// unchanged source (same canonical path, file identity, length and mtime) is remembered so a
/// decoder opens once per source version, not once per frame (CLAUDE.md §5bis(a)).
const COUNT_CACHE_ENTRIES: usize = 64;
type CountKey = (PathBuf, (u64, u64), u64, SystemTime);
type CountCache = Mutex<Vec<(CountKey, u32)>>;
static COUNTS: CountCache = Mutex::new(Vec::new());

fn count_key(path: &str) -> Option<CountKey> {
    let canonical = std::fs::canonicalize(path).ok()?;
    let metadata = std::fs::metadata(&canonical).ok()?;
    let modified = metadata.modified().ok()?;
    Some((
        canonical,
        file_identity(&metadata),
        metadata.len(),
        modified,
    ))
}

/// A same-size, same-mtime file copied over the source is a new source version.
#[cfg(unix)]
fn file_identity(metadata: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (metadata.dev(), metadata.ino())
}
#[cfg(not(unix))]
fn file_identity(_: &std::fs::Metadata) -> (u64, u64) {
    (0, 0)
}

fn remember(cache: &CountCache, key: CountKey, count: u32) {
    let mut entries = cache.lock().unwrap_or_else(PoisonError::into_inner);
    entries.retain(|(known, _)| *known != key);
    if entries.len() >= COUNT_CACHE_ENTRIES {
        entries.remove(0);
    }
    entries.push((key, count));
}

/// The placement read's decoder: a hit answers open/close from the cache without
/// creating or opening a decoder; a miss opens the real one and records the count
/// once its session closed. The caller's readability probe still runs first.
struct CachedCount<'a> {
    cache: &'a CountCache,
    make: fn() -> Box<dyn NativeVideoDecoder>,
    inner: Option<Box<dyn NativeVideoDecoder>>,
    hit: bool,
    miss: Option<(u64, CountKey, u32)>,
}
impl<'a> CachedCount<'a> {
    fn new(cache: &'a CountCache, make: fn() -> Box<dyn NativeVideoDecoder>) -> Self {
        Self {
            cache,
            make,
            inner: None,
            hit: false,
            miss: None,
        }
    }
}
impl NativeVideoDecoder for CachedCount<'_> {
    fn open(&mut self, path: &str) -> Result<NativeVideoInfo, String> {
        let key = count_key(path);
        let known = key.as_ref().and_then(|key| {
            let entries = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
            entries
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, count)| *count)
        });
        if let Some(frame_count) = known {
            self.hit = true;
            return Ok(NativeVideoInfo {
                session: 0,
                width: 0,
                height: 0,
                frame_count,
            });
        }
        let info = self.inner.get_or_insert_with(self.make).open(path)?;
        self.miss = key.map(|key| (info.session, key, info.frame_count));
        Ok(info)
    }
    fn decode(&mut self, session: u64, source_frame: u32) -> Result<Vec<u8>, String> {
        match self.inner.as_mut() {
            Some(decoder) if !self.hit => decoder.decode(session, source_frame),
            _ => Err(format!("video session {session} is not open")),
        }
    }
    fn close(&mut self, session: u64) -> Result<(), String> {
        if std::mem::take(&mut self.hit) {
            return Ok(());
        }
        let decoder = self
            .inner
            .as_mut()
            .ok_or_else(|| format!("video session {session} is not open"))?;
        decoder.close(session)?;
        if let Some((_, key, count)) = self
            .miss
            .take()
            .filter(|(open, _, count)| *open == session && *count > 0)
        {
            remember(self.cache, key, count);
        }
        Ok(())
    }
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

    static OPENS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    struct Counting;
    impl NativeVideoDecoder for Counting {
        fn open(&mut self, _: &str) -> Result<NativeVideoInfo, String> {
            OPENS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(NativeVideoInfo {
                session: 9,
                width: 8,
                height: 4,
                frame_count: 48,
            })
        }
        fn decode(&mut self, _: u64, _: u32) -> Result<Vec<u8>, String> {
            Err("never decodes".into())
        }
        fn close(&mut self, session: u64) -> Result<(), String> {
            (session == 9)
                .then_some(())
                .ok_or_else(|| "wrong session".into())
        }
    }
    fn counting() -> Box<dyn NativeVideoDecoder> {
        Box::new(Counting)
    }
    fn read(cache: &CountCache, path: &str) -> Option<u32> {
        let mut decoder = CachedCount::new(cache, counting);
        let info = decoder.open(path).ok()?;
        decoder.close(info.session).ok()?;
        Some(info.frame_count)
    }

    #[test]
    fn an_unchanged_source_opens_one_decoder_across_placement_reads() {
        let dir = std::env::temp_dir().join(format!("nemo-t12-count-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("clip.mov");
        std::fs::write(&file, b"x").unwrap();
        let (cache, path) = (
            CountCache::new(Vec::new()),
            file.to_string_lossy().into_owned(),
        );
        let before = OPENS.load(std::sync::atomic::Ordering::SeqCst);
        for _ in 0..24 {
            assert_eq!(read(&cache, &path), Some(48));
        }
        assert_eq!(OPENS.load(std::sync::atomic::Ordering::SeqCst) - before, 1);
        // A changed source is a new version; a missing one is never cached.
        std::fs::write(&file, b"longer").unwrap();
        assert_eq!(read(&cache, &path), Some(48));
        assert_eq!(OPENS.load(std::sync::atomic::Ordering::SeqCst) - before, 2);
        std::fs::remove_file(&file).unwrap();
        assert_eq!(read(&cache, &path), Some(48));
        assert_eq!(read(&cache, &path), Some(48));
        assert_eq!(OPENS.load(std::sync::atomic::Ordering::SeqCst) - before, 4);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_count_cache_is_bounded_oldest_first() {
        let cache = CountCache::new(Vec::new());
        let key = |n: u64| {
            (
                PathBuf::from(format!("/clip-{n}.mov")),
                (1, n),
                n,
                SystemTime::UNIX_EPOCH,
            )
        };
        for n in 0..=COUNT_CACHE_ENTRIES as u64 {
            remember(&cache, key(n), 12);
        }
        let entries = cache.lock().unwrap();
        assert_eq!(entries.len(), COUNT_CACHE_ENTRIES);
        assert!(entries.iter().all(|(k, _)| *k != key(0)));
        assert_eq!(entries.last().unwrap().0, key(COUNT_CACHE_ENTRIES as u64));
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
