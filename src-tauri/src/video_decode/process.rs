//! The ffmpeg sidecar path and the piped rawvideo decode child: spawning
//! a process positioned at a frame and reading one RGBA8 frame from it.

use std::io::Read;
use std::process::{Child, ChildStdout, Command, Stdio};

use super::session::VideoSession;

/// Resolves the bundled ffmpeg sidecar's actual path — Tauri places
/// `externalBin` entries alongside the main executable (confirmed
/// empirically: `target/release/ffmpeg` and `target/debug/ffmpeg` both
/// exist next to the `nemo`/`nemo_lib`-linked binary), stripped of the
/// `-<target-triple>` suffix `tauri.conf.json`'s `binaries/ffmpeg-*`
/// source file carries. No `tauri::AppHandle` needed — every caller here
/// is deep in per-frame session logic, far from any command's handle.
#[cfg(not(test))]
pub(super) fn ffmpeg_path() -> std::path::PathBuf {
    // Test override: `cargo test`'s current_exe() is a `target/*/deps/
    // nemo_lib-<hash>` test binary, whose parent dir has no sidecar next
    // to it (only `target/release/ffmpeg` and `target/debug/ffmpeg`,
    // confirmed empirically, do) — tests point this at the bundled binary
    // directly instead. Inert/unset in the real running app.
    if let Ok(p) = std::env::var("NEMO_TEST_FFMPEG_PATH") {
        return std::path::PathBuf::from(p);
    }
    let mut p = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("ffmpeg")))
        .unwrap_or_else(|| std::path::PathBuf::from("ffmpeg"));
    if cfg!(windows) {
        p.set_extension("exe");
    }
    p
}

// Fixture generation, probing, and decoding share one immutable test selection.
#[cfg(test)]
pub(super) fn ffmpeg_path() -> std::path::PathBuf {
    super::tests::test_ffmpeg_path().clone()
}

/// One running (or just-spawned) ffmpeg decode process for a session.
/// `next_frame` is which frame index the process's stdout will yield
/// NEXT — sequential requests (frame N+1 right after N) need no respawn,
/// matching v1's "cheap sequential fast path" exactly.
pub(super) struct FfmpegProc {
    pub(super) child: Child,
    pub(super) stdout: std::io::BufReader<ChildStdout>,
    pub(super) next_frame: i64,
}

/// Kills any running process for this session and spawns a fresh one that
/// yields rawvideo RGBA8 frames starting at `frame_index`. `-ss` (before
/// `-i`) with `-accurate_seek` (ffmpeg's default since 2.1 — not passed
/// explicitly, relying on the well-established default rather than
/// pinning a flag that could regress silently on some exotic build) seeks
/// to the nearest keyframe at/before the target and decodes-and-discards
/// forward internally, so the FIRST frame this process's stdout yields is
/// already the exact target — no per-frame walk-and-discard loop needed
/// on our side for the normal case (see decode_at for the tail-retry
/// exception, which DOES need one).
pub(super) fn spawn_at(s: &mut VideoSession, frame_index: i64) -> Result<(), String> {
    if let Some(mut p) = s.proc.take() {
        let _ = p.child.kill();
        let _ = p.child.wait(); // reap — avoid leaking zombie processes across many scrub-driven respawns
    }
    let mut cmd = Command::new(ffmpeg_path());
    cmd.args(["-v", "error", "-nostdin"]);
    if frame_index > 0 {
        let seconds = frame_index as f64 / s.fps;
        cmd.args(["-ss", &format!("{seconds:.6}")]);
    }
    cmd.args([
        "-i", &s.path, "-f", "rawvideo", "-pix_fmt", "rgba", "-vsync", "0", "-an", "-sn", "-",
    ]);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("spawn ffmpeg: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("ffmpeg spawned with no stdout pipe")?;
    // Buffer sized to one full frame (or 64KB, whichever is larger) so a
    // single read_exact() below is satisfied by one or two underlying
    // syscalls even at 4K, not dozens of small reads.
    let cap = s.frame_bytes.max(64 * 1024);
    s.proc = Some(FfmpegProc {
        child,
        stdout: std::io::BufReader::with_capacity(cap, stdout),
        next_frame: frame_index,
    });
    Ok(())
}

/// Blocking read of exactly one tightly-packed RGBA8 frame from the
/// session's current process. `-f rawvideo -pix_fmt rgba` writes frames
/// with NO row padding (unlike the AVFrame plane stride v1 had to unpad
/// row-by-row) — width*height*4 bytes, done, no conversion loop at all.
pub(super) fn read_one_frame(s: &mut VideoSession) -> Result<Vec<u8>, String> {
    let proc = s.proc.as_mut().ok_or("no active ffmpeg process")?;
    let mut buf = vec![0u8; s.frame_bytes];
    match proc.stdout.read_exact(&mut buf) {
        Ok(()) => {
            proc.next_frame += 1;
            Ok(buf)
        }
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            Err("decode: stream exhausted".to_string())
        }
        Err(e) => Err(format!("read: {e}")),
    }
}
