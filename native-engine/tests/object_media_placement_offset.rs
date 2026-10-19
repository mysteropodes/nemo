use super::*;
fn audio() -> AssetTarget {
    AssetTarget::AudioTrack {
        track_uid: "a".into(),
    }
}
fn video() -> AssetTarget {
    AssetTarget::Drawing {
        context_id: "scene-root".into(),
        layer_uid: "L".into(),
        stroke_id: None,
    }
}
fn n(v: f64) -> Number {
    Number::from_f64(v).unwrap()
}
#[test]
fn fractional_audio_keeps_the_exact_signed_value_while_video_remains_integer_only() {
    for value in [-3.875, -0.25, 0.125, 9.5] {
        assert!(admitted(&audio(), &n(value)));
        assert!(!admitted(&video(), &n(value)));
    }
    assert!(admitted(&video(), &Number::from(-9)));
    assert!(
        !admitted(&video(), &n(9.0)),
        "old floating token grammar is not widened for Video"
    );
}
#[test]
fn shifting_fractional_audio_does_not_round_or_allocate_a_second_offset() {
    assert_eq!(shifted(&audio(), &n(-3.875), &n(0.625)).unwrap(), n(-3.25));
    assert_eq!(shifted(&audio(), &n(0.125), &n(-0.25)).unwrap(), n(-0.125));
    assert_eq!(
        shifted(&video(), &Number::from(-9), &Number::from(2))
            .unwrap()
            .as_i64(),
        Some(-7)
    );
    assert!(shifted(&video(), &Number::from(0), &n(0.5)).is_none());
}
#[test]
fn safe_bounds_and_overflow_refuse_without_saturating_or_wrapping() {
    assert!(admitted(&audio(), &Number::from(9_007_199_254_740_991_i64)));
    assert!(!admitted(
        &audio(),
        &Number::from(9_007_199_254_740_992_i64)
    ));
    assert!(shifted(
        &audio(),
        &Number::from(9_007_199_254_740_991_i64),
        &Number::from(1)
    )
    .is_none());
    assert!(shifted(
        &video(),
        &Number::from(-9_007_199_254_740_991_i64),
        &Number::from(-1)
    )
    .is_none());
}
#[test]
fn semantic_noop_matches_integer_and_float_without_rewriting_stored_spelling() {
    assert!(equal(&Number::from(5), &n(5.0)));
    assert!(equal(&Number::from(0), &n(-0.0)));
    assert!(!equal(&n(0.25), &n(0.5)));
}
