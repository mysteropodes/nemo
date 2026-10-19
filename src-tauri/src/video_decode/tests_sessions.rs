//! Several simultaneous sessions: isolation and parallelism.

use super::decode::decode_at;
use super::tests::*;
use super::*;

#[test]
fn concurrent_sessions_decode_in_parallel_not_serialized() {
    use std::sync::Arc as StdArc;
    use std::thread;
    use std::time::Instant as StdInstant;

    let path_a = make_test_video();
    let path_b = gen_video(
        "h264_320_concurrency_b.mp4",
        "testsrc2=size=320x240:rate=30:duration=3",
        &[
            "-c:v",
            h264_fixture_encoder(),
            "-pix_fmt",
            "yuv420p",
            "-g",
            "15",
        ],
    );

    let sessions = StdArc::new(VideoSessions::default());
    let (sa, _, _) = open_session_core_t(path_a.to_str().unwrap()).unwrap();
    let (sb, _, _) = open_session_core_t(path_b.to_str().unwrap()).unwrap();
    let (id_a, id_b) = (1u32, 2u32);
    sessions
        .0
        .lock()
        .unwrap()
        .insert(id_a, StdArc::new(Mutex::new(sa)));
    sessions
        .0
        .lock()
        .unwrap()
        .insert(id_b, StdArc::new(Mutex::new(sb)));

    fn work(sessions: StdArc<VideoSessions>, id: u32) -> u128 {
        let t0 = StdInstant::now();
        for target in [50i64, 5, 55, 2, 58, 8] {
            let arc = sessions.0.lock().unwrap().get(&id).unwrap().clone();
            let mut s = arc.lock().unwrap();
            decode_frame_core(&mut s, target).unwrap();
        }
        t0.elapsed().as_millis()
    }

    let (s1, s2) = (sessions.clone(), sessions.clone());
    let t_wall = StdInstant::now();
    let ha = thread::spawn(move || work(s1, id_a));
    let hb = thread::spawn(move || work(s2, id_b));
    let da = ha.join().unwrap();
    let db = hb.join().unwrap();
    let wall = t_wall.elapsed().as_millis();

    eprintln!(
        "[concurrency] A={da}ms B={db}ms wall={wall}ms sum={}ms",
        da + db
    );
    assert!(
        wall < (da + db) * 8 / 10,
        "wall={wall}ms not meaningfully less than sum={}ms — sessions may still be serializing",
        da + db
    );
}

// Reproduces LIVE report ("j'ai mis 3 vidéos ça a du mal à lire les 3
// en temps réel" — log from the running app showed near-EVERY frame
// respawning even for 1-frame steps, across 3 simultaneous sessions).
// Simulates 3 sessions "playing" together: each tick, all three are
// asked for their next sequential frame (mirroring the JS playback
// loop's onFrameChanged, which calls every native-video layer once
// per tick) with spawn_readahead fired after each — the exact
// decode_video_frame command sequence, minus the Tauri IPC layer.
#[test]
fn three_simultaneous_sessions_playback_mostly_avoids_respawns() {
    let pa = make_test_video(); // 320x240, 60 frames
    let pb = gen_video(
        "play3_b.mp4",
        "testsrc2=size=320x240:rate=30:duration=3",
        &[
            "-c:v",
            h264_fixture_encoder(),
            "-pix_fmt",
            "yuv420p",
            "-g",
            "15",
        ],
    );
    let pc = gen_video(
        "play3_c.mp4",
        "testsrc2=size=320x240:rate=30:duration=3",
        &[
            "-c:v",
            h264_fixture_encoder(),
            "-pix_fmt",
            "yuv420p",
            "-g",
            "15",
        ],
    );
    let (sa, _, _) = open_session_core_t(pa.to_str().unwrap()).unwrap();
    let (sb, _, _) = open_session_core_t(pb.to_str().unwrap()).unwrap();
    let (sc, _, _) = open_session_core_t(pc.to_str().unwrap()).unwrap();
    let sessions = [
        std::sync::Arc::new(Mutex::new(sa)),
        std::sync::Arc::new(Mutex::new(sb)),
        std::sync::Arc::new(Mutex::new(sc)),
    ];

    let mut respawn_count = 0u32;
    let mut total = 0u32;
    for tick in 0..40i64 {
        for arc in &sessions {
            let (seeked, cache_hit) = {
                let mut s = arc.lock().unwrap();
                let hit = s.cache.iter().any(|(idx, _)| *idx == tick);
                let seeked = if hit {
                    let pos = s.cache.iter().position(|(idx, _)| *idx == tick).unwrap();
                    let entry = s.cache.remove(pos).unwrap();
                    s.cache.push_front(entry);
                    false
                } else {
                    decode_at(&mut s, tick, "foreground").unwrap().1
                };
                (seeked, hit)
            };
            total += 1;
            if seeked {
                respawn_count += 1;
            }
            if !cache_hit {
                spawn_readahead(arc.clone(), tick);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    eprintln!(
        "[diag] three-session playback: {respawn_count}/{total} requests needed a fresh spawn"
    );
    assert!(
        respawn_count <= sessions.len() as u32 * 3,
        "{respawn_count}/{total} requests respawned — expected steady-state playback to avoid this almost entirely (only early warm-up spawns)"
    );
}
