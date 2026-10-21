//! Indexed optimized-media transcode and random access.

use super::tests::*;
use super::*;

// ---- indexed optimized media (the "scrub parfait" pipeline) ----

#[test]
fn optimized_media_transcode_produces_a_valid_index() {
    let target = make_indexed_media();
    let idx: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{target}.idx")).unwrap()).unwrap();
    assert_eq!(
        idx["count"].as_u64().unwrap(),
        60,
        "expected one index entry per source frame"
    );
    assert_eq!(idx["width"].as_u64().unwrap(), 320);
    assert_eq!(idx["height"].as_u64().unwrap(), 240);
    assert!((idx["fps"].as_f64().unwrap() - 30.0).abs() < 0.01);
    let offsets = idx["offsets"].as_array().unwrap();
    assert_eq!(
        offsets.len(),
        61,
        "count+1 offsets (sentinel = file length)"
    );
    let len = std::fs::metadata(&target).unwrap().len();
    assert_eq!(offsets.last().unwrap().as_u64().unwrap(), len);
}

#[test]
fn indexed_session_random_access_never_respawns_and_is_consistent() {
    let target = make_indexed_media();
    let _ffmpeg = ffmpeg_shared();
    let (mut s, frame_count, _) = open_session_core_t(&target).unwrap();
    assert!(
        s.indexed.is_some(),
        "session should have opened in indexed mode"
    );
    assert_eq!(frame_count, 60);

    // Ground truth from a SECOND independent indexed session.
    let (mut s_ref, _, _) = open_session_core_t(&target).unwrap();

    // Brutal scrub pattern: far jumps, backward runs, repeats.
    let pattern = [30i64, 5, 59, 0, 42, 41, 40, 12, 57, 3, 30];
    let mut seen = std::collections::HashMap::new();
    for &f in &pattern {
        let (bytes, seeked) = decode_frame_core(&mut s, f).unwrap();
        assert!(
            !seeked,
            "indexed decode of {f} reported a respawn — should be impossible"
        );
        assert_eq!(bytes.len(), 320 * 240 * 4);
        let truth = decode_frame_core(&mut s_ref, f).unwrap().0;
        assert_eq!(
            bytes, truth,
            "frame {f} differs between two indexed sessions"
        );
        seen.insert(f, bytes);
    }
    // Distinct frames must differ (testsrc2 has a moving pattern).
    assert_ne!(
        seen[&5], seen[&59],
        "distinct frames decoded identical bytes"
    );
}

#[test]
fn indexed_random_seek_cost_is_flat_and_small() {
    // Fixture first (it takes the shared guard), then measure the decoder
    // alone, not concurrent in-process FFmpeg tests.
    let target = make_indexed_media();
    let _timing = ffmpeg_timing_exclusive();
    let (mut s, _, _) = open_session_core_t(&target).unwrap();
    decode_frame_core(&mut s, 0).unwrap(); // absorb converter spawn
    s.cache.clear(); // measure decode, not cache hits
    let mut times = Vec::new();
    // Worst-case scrub: alternating far jumps, never sequential.
    let targets: Vec<i64> = (0..40)
        .map(|i| {
            if i % 2 == 0 {
                (i * 7) % 60
            } else {
                59 - ((i * 11) % 60)
            }
        })
        .collect();
    for f in targets {
        let t0 = std::time::Instant::now();
        decode_frame_core(&mut s, f).unwrap();
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
        s.cache.clear();
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let avg = times.iter().sum::<f64>() / times.len() as f64;
    let p95 = times[(times.len() as f64 * 0.95) as usize];
    eprintln!("[perf] indexed random access: avg={avg:.1}ms p95={p95:.1}ms");
    // Regression guard with headroom (tiny frames decode ~1-3ms; the
    // property under test is FLAT cost with no respawn spikes).
    assert!(
        p95 < 20.0,
        "indexed random access p95 {p95:.1}ms — respawn-like spikes crept back in"
    );
}

#[test]
fn stale_index_is_rejected_not_misread() {
    let target = make_indexed_media();
    let stale = test_fixture_dir().join("stale_copy.mjpeg");
    let stale_s = stale.to_str().unwrap().to_string();
    std::fs::copy(&target, &stale).unwrap();
    std::fs::copy(format!("{target}.idx"), format!("{stale_s}.idx")).unwrap();
    // Truncate the stream so the sidecar's sentinel no longer matches.
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(&stale)
        .unwrap();
    let len = f.metadata().unwrap().len();
    f.set_len(len - 100).unwrap();
    drop(f);
    let _ffmpeg = ffmpeg_shared();
    let err = match open_session_core_t(&stale_s) {
        Err(e) => e,
        Ok(_) => panic!("stale index was accepted — sentinel check is broken"),
    };
    assert!(
        err.contains("stale"),
        "expected a stale-index error, got: {err}"
    );
}
