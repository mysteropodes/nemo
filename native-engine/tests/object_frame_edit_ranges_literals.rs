//! Hand-written acceptance cases for the range kernel.
//!
//! Each case names one oracle row (`PIN`, copied verbatim from tests/fixtures/object-frame-edit-baseline/
//! ranges.txt, a run of the baseline function at fbe4b8a3) and states the same input and the expected
//! output a second time as typed values, without going through the text parser. A pin that is edited
//! or removed from the fixture fails `every_pin_is_an_oracle_row`; a kernel that drifts from the typed
//! values fails the case. Rows chosen: a value AT the cursor, strictly after it, padded versus
//! targeted layers, absent / null / explicit 0, collapse onto the cursor, work-area clamps, the
//! operations that shift nothing, and the cut that drops a link.
use super::tests::ORACLE;
use super::{
    split_cut, FrameBound, FrameEditKind, LayerRanges, LayerRole, RangeEdit, RangeEditError,
    SplitCutOutcome, TimeLinkOutcome, TimeLinkSlot,
};

const PINS: &[&str] = &[
    "insert 5 2 - 4 x 001 GGP 5/5/2,6/u u/6/x/L 6/2/x/L 6 5 x 6/6/2,7/u u/7/x/L 6/2/x/L",
    "insert 5 2 - 4 3,3 001 PGP 3/u/x/u u/3/x/n u/u/3,3/L 6 5 4,4 3/u/x/u u/4/x/n u/u/3,3/L",
    "insert 5 2 - 4 e 000 GPP 2/u/x/u u/2/x/n u/u/2,2/L 6 5 e 2/u/x/u u/2/x/n u/u/2,2/L",
    "insert 5 0 - 4 e 001 GPP n/u/x/u u/n/x/n u/u/n,n/L 6 5 e n/u/x/u u/n/x/n u/u/n,n/L",
    "insert 5 0 - 4 0,0 010 PPG 0/u/x/u u/0/x/n u/u/0,0/L 6 5 0,0 0/u/x/u u/0/x/n u/u/0,0/L",
    "insert 5 1 - 5 0,u,0 001 GGP 1/2/1/L 1/2/2,4,u/u 2/3/n/L 6 5 0,u,0 1/3/1/L 1/3/3,5,u/u 2/3/n/L",
    "insert 5 4 - 2 x 010 GPP 5/u/2/u 1/n/0/L 2/u/u/u 6 3 x 6/u/2/u 1/n/0/L 2/u/u/u",
    "remove 5 2 - 4 4 000 --- 6/n/1,4/L 0/5/6/u 6/4/4,2,3/u 4 3 3 5/n/1,3/L 0/4/5/u 5/3/3,2,2/u",
    "remove 5 2 - 4 3,3 001 --- 3/u/x/u u/3/x/n u/u/3,3/L 4 3 2,2 2/u/x/u u/2/x/n u/u/2,2/L",
    "remove 5 2 - 5 1 000 --- 1/u/4/n 1/1/1,u,n/u u/0/6,4/u 4 3 1 1/u/3/n 1/1/1,u,n/u u/0/5,3/u",
    "remove 1 0 - 0 x 000 --- u/u/x/u u/u/x/n u/u/u,u/L refused - - - - -",
    "span 4 1 2 3 4 010 --- 1/1/e/n 0/u/3,u,5/u 4/n/5/L 2 1 4 1/1/e/n 0/u/3,u,5/u 4/n/5/L",
    "extend 5 0 3 4 0,0 000 --- 0/u/x/u u/0/x/n u/u/0,0/L 8 7 0,0 0/u/x/u u/0/x/n u/u/0,0/L",
    "extend 5 2 1 4 x 000 --- 4/u/x/u u/4/x/n u/u/4,4/L 6 5 x 4/u/x/u u/4/x/n u/u/4,4/L",
    "dupkey 5 2 - 4 3,3 000 --- 3/u/x/u u/3/x/n u/u/3,3/L 6 5 3,3 3/u/x/u u/3/x/n u/u/3,3/L",
    "span 3 0 3 2 0,u 001 --- 0/u/u,3/u u/4/3/L 1/4/3,1,1/u refused - - - - -",
    "split 0 3 2 2/5/e/L cut 2/1/e/u 2/3/e/u",
    "split 0 3 2 n/n/x/L cut n/1/x/u 2/3/x/u",
    "split 0 3 2 n/n/u,n,2/n cut n/1/u,n,2/n 2/3/u,n,2/u",
    "split 1 4 3 2/5/u,n,2/u cut 2/2/u,n,2/u 3/4/u,n,2/u",
    "split 1 4 4 u/u/x/u cut u/3/x/u 4/4/x/u",
    "split 1 4 1 u/u/x/u refused - -",
    "split 1 4 5 u/u/x/u refused - -",
];

fn at(frame: i64) -> FrameBound {
    FrameBound::At(frame)
}

fn markers(list: &[FrameBound]) -> Option<Vec<FrameBound>> {
    Some(list.to_vec())
}

fn layer(
    in_point: FrameBound,
    out_point: FrameBound,
    markers: Option<Vec<FrameBound>>,
    time_link: TimeLinkSlot,
) -> LayerRanges {
    LayerRanges {
        in_point,
        out_point,
        markers,
        time_link,
    }
}

fn edit(kind: FrameEditKind, cursor: u32, total: u32) -> RangeEdit {
    RangeEdit {
        kind,
        cursor,
        total,
    }
}

const A: FrameBound = FrameBound::Absent;
const N: FrameBound = FrameBound::Null;
const NO_LINK: TimeLinkSlot = TimeLinkSlot::Absent;
const LINK: TimeLinkSlot = TimeLinkSlot::Linked;
const NULL_LINK: TimeLinkSlot = TimeLinkSlot::Null;
const TARGET: LayerRole = LayerRole::Targeted;
const PAD: LayerRole = LayerRole::Padded;

/// Every layer of `inputs` through the edit with its role; returns the three results.
fn run(op: &RangeEdit, inputs: &[(LayerRole, LayerRanges)]) -> Vec<LayerRanges> {
    inputs
        .iter()
        .map(|(role, input)| op.layer_ranges(*role, input).unwrap())
        .collect()
}

#[test]
fn every_pin_is_an_oracle_row() {
    for pin in PINS.iter().copied() {
        assert_eq!(
            ORACLE.lines().filter(|line| *line == pin).count(),
            1,
            "pin missing or duplicated in the fixture: {pin}"
        );
    }
}

#[test]
fn insert_shifts_only_values_after_the_cursor_on_targeted_layers() {
    // Pin `insert 5 2 - 4 x 001 GGP ...`: layer 2 is locked, so it is padded and keeps even its large value.
    let e = edit(FrameEditKind::InsertFrame, 2, 5);
    let out = run(
        &e,
        &[
            (
                TARGET,
                layer(at(5), at(5), markers(&[at(2), at(6)]), NO_LINK),
            ),
            (TARGET, layer(A, at(6), None, LINK)),
            (PAD, layer(at(6), at(2), None, LINK)),
        ],
    );
    assert_eq!(
        out[0],
        layer(at(6), at(6), markers(&[at(2), at(7)]), NO_LINK)
    );
    assert_eq!(out[1], layer(A, at(7), None, LINK));
    assert_eq!(out[2], layer(at(6), at(2), None, LINK));
    assert_eq!(e.new_total(), Ok(6));
    assert_eq!(e.work_area_out(4), Ok(5));
    assert_eq!(e.comp_markers(&None), Ok(None));
}

#[test]
fn insert_leaves_padded_layers_alone_but_shifts_composition_markers() {
    // Pin `insert 5 2 - 4 3,3 001 PGP ...`: only the middle layer (the one that really got the slot) moves.
    let e = edit(FrameEditKind::InsertFrame, 2, 5);
    let out = run(
        &e,
        &[
            (PAD, layer(at(3), A, None, NO_LINK)),
            (TARGET, layer(A, at(3), None, NULL_LINK)),
            (PAD, layer(A, A, markers(&[at(3), at(3)]), LINK)),
        ],
    );
    assert_eq!(out[0], layer(at(3), A, None, NO_LINK));
    assert_eq!(out[1], layer(A, at(4), None, NULL_LINK));
    assert_eq!(out[2], layer(A, A, markers(&[at(3), at(3)]), LINK));
    assert_eq!(
        e.comp_markers(&markers(&[at(3), at(3)])),
        Ok(markers(&[at(4), at(4)]))
    );
}

#[test]
fn a_value_at_the_cursor_never_moves() {
    // Pins `insert 5 2 - 4 e 000 GPP` (value 2 at cursor 2) and `remove 5 2 - 4 3,3 001` (3 collapses onto 2).
    let insert = edit(FrameEditKind::InsertFrame, 2, 5);
    let at_cursor = layer(at(2), A, None, NO_LINK);
    assert_eq!(
        insert.layer_ranges(TARGET, &at_cursor),
        Ok(at_cursor.clone())
    );
    let remove = edit(FrameEditKind::RemoveFrame, 2, 5);
    assert_eq!(
        remove.layer_ranges(TARGET, &at_cursor),
        Ok(at_cursor.clone())
    );
    let after = layer(at(3), A, None, NO_LINK);
    assert_eq!(
        remove.layer_ranges(TARGET, &after),
        Ok(layer(at(2), A, None, NO_LINK))
    );
    assert_eq!(
        remove.comp_markers(&markers(&[at(3), at(3)])),
        Ok(markers(&[at(2), at(2)]))
    );
}

#[test]
fn absent_null_and_explicit_zero_stay_three_different_values() {
    // Pins `insert 5 0 - 4 e 001 GPP` (null / absent) and `insert 5 0 - 4 0,0 010 PPG` (explicit 0):
    // cursor 0, so 0 is not after the cursor, and null / absent are not numbers at all.
    let e = edit(FrameEditKind::InsertFrame, 0, 5);
    let null_row = [
        (TARGET, layer(N, A, None, NO_LINK)),
        (PAD, layer(A, N, None, NULL_LINK)),
        (PAD, layer(A, A, markers(&[N, N]), LINK)),
    ];
    assert_eq!(
        run(&e, &null_row),
        null_row.iter().map(|r| r.1.clone()).collect::<Vec<_>>()
    );
    let zero_row = [
        (PAD, layer(at(0), A, None, NO_LINK)),
        (PAD, layer(A, at(0), None, NULL_LINK)),
        (TARGET, layer(A, A, markers(&[at(0), at(0)]), LINK)),
    ];
    assert_eq!(
        run(&e, &zero_row),
        zero_row.iter().map(|r| r.1.clone()).collect::<Vec<_>>()
    );
    assert_ne!(N, A);
    assert_ne!(A, at(0));
    assert_ne!(N, at(0));
    assert_eq!(
        e.comp_markers(&markers(&[at(0), at(0)])),
        Ok(markers(&[at(0), at(0)]))
    );
    assert_eq!(e.comp_markers(&markers(&[])), Ok(markers(&[])));
    assert_eq!(e.comp_markers(&None), Ok(None));
}

#[test]
fn marker_lists_keep_absent_and_null_entries_and_their_order() {
    // Pin `insert 5 1 - 5 0,u,0 001 GGP`: marker lists [2,4,u] and [n] beside real frames; cursor 1.
    let e = edit(FrameEditKind::InsertFrame, 1, 5);
    let out = run(
        &e,
        &[
            (TARGET, layer(at(1), at(2), markers(&[at(1)]), LINK)),
            (
                TARGET,
                layer(at(1), at(2), markers(&[at(2), at(4), A]), NO_LINK),
            ),
            (PAD, layer(at(2), at(3), markers(&[N]), LINK)),
        ],
    );
    assert_eq!(out[0], layer(at(1), at(3), markers(&[at(1)]), LINK));
    assert_eq!(
        out[1],
        layer(at(1), at(3), markers(&[at(3), at(5), A]), NO_LINK)
    );
    assert_eq!(out[2], layer(at(2), at(3), markers(&[N]), LINK));
    assert_eq!(
        e.comp_markers(&markers(&[at(0), A, at(0)])),
        Ok(markers(&[at(0), A, at(0)]))
    );
    assert_eq!(e.work_area_out(5), Ok(5));
}

#[test]
fn insert_on_the_last_frame_shifts_a_value_beyond_it() {
    // Pin `insert 5 4 - 2 x 010 GPP`: cursor 4 is the last frame; in point 5 > 4 moves, the work area end 2 -> 3.
    let e = edit(FrameEditKind::InsertFrame, 4, 5);
    let out = run(
        &e,
        &[
            (TARGET, layer(at(5), A, markers(&[at(2)]), NO_LINK)),
            (PAD, layer(at(1), N, markers(&[at(0)]), LINK)),
            (PAD, layer(at(2), A, markers(&[A]), NO_LINK)),
        ],
    );
    assert_eq!(out[0], layer(at(6), A, markers(&[at(2)]), NO_LINK));
    assert_eq!(out[1], layer(at(1), N, markers(&[at(0)]), LINK));
    assert_eq!(out[2], layer(at(2), A, markers(&[A]), NO_LINK));
    assert_eq!((e.new_total(), e.work_area_out(2)), (Ok(6), Ok(3)));
}

#[test]
fn removal_shifts_every_layer_down_whatever_its_role() {
    // Pin `remove 5 2 - 4 4 000 ...`: cursor 2 of 5, composition marker 4.
    let e = edit(FrameEditKind::RemoveFrame, 2, 5);
    let out = run(
        &e,
        &[
            (PAD, layer(at(6), N, markers(&[at(1), at(4)]), LINK)),
            (TARGET, layer(at(0), at(5), markers(&[at(6)]), NO_LINK)),
            (
                PAD,
                layer(at(6), at(4), markers(&[at(4), at(2), at(3)]), NO_LINK),
            ),
        ],
    );
    assert_eq!(out[0], layer(at(5), N, markers(&[at(1), at(3)]), LINK));
    assert_eq!(out[1], layer(at(0), at(4), markers(&[at(5)]), NO_LINK));
    assert_eq!(
        out[2],
        layer(at(5), at(3), markers(&[at(3), at(2), at(2)]), NO_LINK)
    );
    assert_eq!(e.new_total(), Ok(4));
    assert_eq!(e.work_area_out(4), Ok(3));
    assert_eq!(e.comp_markers(&markers(&[at(4)])), Ok(markers(&[at(3)])));
}

#[test]
fn removal_with_absent_null_and_early_markers() {
    // Pin `remove 5 2 - 5 1 000 ...`: absent / null marker entries and a composition marker before the cursor stay.
    let e = edit(FrameEditKind::RemoveFrame, 2, 5);
    let out = run(
        &e,
        &[
            (TARGET, layer(at(1), A, markers(&[at(4)]), NULL_LINK)),
            (
                TARGET,
                layer(at(1), at(1), markers(&[at(1), A, N]), NO_LINK),
            ),
            (TARGET, layer(A, at(0), markers(&[at(6), at(4)]), NO_LINK)),
        ],
    );
    assert_eq!(out[0], layer(at(1), A, markers(&[at(3)]), NULL_LINK));
    assert_eq!(
        out[1],
        layer(at(1), at(1), markers(&[at(1), A, N]), NO_LINK)
    );
    assert_eq!(out[2], layer(A, at(0), markers(&[at(5), at(3)]), NO_LINK));
    assert_eq!(e.comp_markers(&markers(&[at(1)])), Ok(markers(&[at(1)])));
    assert_eq!(e.work_area_out(5), Ok(3));
}

#[test]
fn work_area_end_follows_each_operation() {
    // Insert (pin `insert 5 1 - 5`): an end already on the new last frame stays, a lower one moves +1.
    let insert = |cursor: u32| edit(FrameEditKind::InsertFrame, cursor, 5);
    assert_eq!(insert(1).work_area_out(5), Ok(5));
    assert_eq!(insert(4).work_area_out(2), Ok(3));
    // Removal (pin `remove 5 2 - 5`): an end beyond the new last frame is pulled back; one inside is kept.
    let remove = edit(FrameEditKind::RemoveFrame, 2, 5);
    assert_eq!(remove.work_area_out(5), Ok(3));
    assert_eq!(remove.work_area_out(3), Ok(3));
    // Span removal (pin `span 4 1 2 3`) clamps the same way.
    let span = edit(FrameEditKind::RemoveFrameSpan { count: 2 }, 1, 4);
    assert_eq!((span.new_total(), span.work_area_out(3)), (Ok(2), Ok(1)));
    // Extend exposure (pin `extend 5 0 3 4`) raises a lower end to the new last frame.
    let extend = edit(FrameEditKind::ExtendExposure { count: 3 }, 0, 5);
    assert_eq!(
        (extend.new_total(), extend.work_area_out(4)),
        (Ok(8), Ok(7))
    );
    // Duplicate keyframe (pin `dupkey 5 2 - 4`) is +1 like F5.
    let dup = edit(FrameEditKind::DuplicateKeyframe, 2, 5);
    assert_eq!((dup.new_total(), dup.work_area_out(4)), (Ok(6), Ok(5)));
}

#[test]
fn span_removal_extend_and_duplicate_shift_no_range_at_all() {
    // Pins `span 4 1 2 3 ...`, `extend 5 2 1 4 ...`, `dupkey 5 2 - 4 ...`: values after the cursor keep their numbers.
    // This is the baseline quirk the acceptance text asks to be pinned, not an endorsement of it.
    let span = edit(FrameEditKind::RemoveFrameSpan { count: 2 }, 1, 4);
    let span_out = run(
        &span,
        &[
            (TARGET, layer(at(1), at(1), Some(Vec::new()), NULL_LINK)),
            (
                TARGET,
                layer(at(0), A, markers(&[at(3), A, at(5)]), NO_LINK),
            ),
            (TARGET, layer(at(4), N, markers(&[at(5)]), LINK)),
        ],
    );
    assert_eq!(
        span_out[1],
        layer(at(0), A, markers(&[at(3), A, at(5)]), NO_LINK)
    );
    assert_eq!(span_out[2], layer(at(4), N, markers(&[at(5)]), LINK));
    assert_eq!(span.comp_markers(&markers(&[at(4)])), Ok(markers(&[at(4)])));
    let extend = edit(FrameEditKind::ExtendExposure { count: 1 }, 2, 5);
    let after = layer(at(4), A, None, NO_LINK);
    assert_eq!(extend.layer_ranges(TARGET, &after), Ok(after.clone()));
    let dup = edit(FrameEditKind::DuplicateKeyframe, 2, 5);
    for doubled in [
        layer(at(3), A, None, NO_LINK),
        layer(A, at(3), None, NULL_LINK),
        layer(A, A, markers(&[at(3), at(3)]), LINK),
    ] {
        assert_eq!(dup.layer_ranges(TARGET, &doubled), Ok(doubled.clone()));
    }
    assert_eq!(
        dup.comp_markers(&markers(&[at(3), at(3)])),
        Ok(markers(&[at(3), at(3)]))
    );
    for kind in [span.kind, extend.kind, dup.kind] {
        assert!(!kind.shift_ranges());
    }
    assert!(FrameEditKind::InsertFrame.shift_ranges());
    assert!(FrameEditKind::RemoveFrame.shift_ranges());
}

#[test]
fn refusals_are_typed_and_nothing_is_returned() {
    // Pins `remove 1 0 - 0 ...` and `span 3 0 3 2 ...`: the baseline leaves the document untouched.
    let last_frame = edit(FrameEditKind::RemoveFrame, 0, 1);
    assert_eq!(
        last_frame.new_total(),
        Err(RangeEditError::WouldEmptyTimeline)
    );
    assert_eq!(
        last_frame.work_area_out(0),
        Err(RangeEditError::WouldEmptyTimeline)
    );
    assert_eq!(
        last_frame.comp_markers(&None),
        Err(RangeEditError::WouldEmptyTimeline)
    );
    let any = layer(at(1), at(2), None, LINK);
    assert_eq!(
        last_frame.layer_ranges(TARGET, &any),
        Err(RangeEditError::WouldEmptyTimeline)
    );
    let all = edit(FrameEditKind::RemoveFrameSpan { count: 3 }, 0, 3);
    assert_eq!(all.new_total(), Err(RangeEditError::WouldEmptyTimeline));
}

#[test]
fn inputs_the_baseline_cannot_express_are_typed_refusals() {
    // Not baseline behavior (the editor never forms these); the kernel refuses instead of guessing.
    let outside = edit(FrameEditKind::InsertFrame, 5, 5);
    assert_eq!(
        outside.new_total(),
        Err(RangeEditError::CursorOutsideTimeline)
    );
    let past = edit(FrameEditKind::RemoveFrameSpan { count: 3 }, 3, 5);
    assert_eq!(past.new_total(), Err(RangeEditError::SpanOutsideTimeline));
    let empty_span = edit(FrameEditKind::RemoveFrameSpan { count: 0 }, 1, 5);
    assert_eq!(empty_span.new_total(), Err(RangeEditError::EmptyEdit));
    let empty_extend = edit(FrameEditKind::ExtendExposure { count: 0 }, 1, 5);
    assert_eq!(empty_extend.new_total(), Err(RangeEditError::EmptyEdit));
    let wide = edit(FrameEditKind::RemoveFrameSpan { count: u32::MAX }, 1, 5);
    assert_eq!(wide.new_total(), Err(RangeEditError::Overflow));
    let full = edit(FrameEditKind::InsertFrame, 0, u32::MAX);
    assert_eq!(full.new_total(), Err(RangeEditError::Overflow));
    let grow = edit(FrameEditKind::ExtendExposure { count: 2 }, 0, u32::MAX - 1);
    assert_eq!(grow.new_total(), Err(RangeEditError::Overflow));
}

#[test]
fn a_frame_number_at_the_integer_limit_refuses_the_whole_layer() {
    let e = edit(FrameEditKind::InsertFrame, 0, 3);
    let input = layer(at(1), A, markers(&[at(2), at(i64::MAX)]), NO_LINK);
    assert_eq!(
        e.layer_ranges(TARGET, &input),
        Err(RangeEditError::Overflow)
    );
    assert_eq!(input.in_point, at(1), "the input is borrowed, never edited");
    let r = edit(FrameEditKind::RemoveFrame, 0, 3);
    assert_eq!(
        r.layer_ranges(TARGET, &layer(at(i64::MAX), A, None, NO_LINK)),
        Ok(layer(at(i64::MAX - 1), A, None, NO_LINK))
    );
    assert_eq!(
        e.comp_markers(&markers(&[at(i64::MAX)])),
        Err(RangeEditError::Overflow)
    );
}

fn cut(
    cut_at: i64,
    eff_in: i64,
    eff_out: i64,
    input: &LayerRanges,
) -> (LayerRanges, LayerRanges, TimeLinkOutcome) {
    match split_cut(cut_at, eff_in, eff_out, input) {
        SplitCutOutcome::Cut(done) => (done.original, done.second, done.time_link),
        SplitCutOutcome::OutsideLayer => panic!("cut {cut_at} in {eff_in}..{eff_out} was refused"),
    }
}

#[test]
fn a_cut_drops_the_link_and_writes_hard_in_out_values() {
    // Pin `split 0 3 2 2/5/e/L`: the stored in point (2) and out point (5) differ from the resolved edges (0, 3).
    let linked = layer(at(2), at(5), Some(Vec::new()), LINK);
    let (first, second, outcome) = cut(2, 0, 3, &linked);
    assert_eq!(first, layer(at(2), at(1), Some(Vec::new()), NO_LINK));
    assert_eq!(second, layer(at(2), at(3), Some(Vec::new()), NO_LINK));
    assert_eq!(outcome, TimeLinkOutcome::TimeLinkDropped);
    // Pin `split 0 3 2 n/n/x/L`: a null in point is kept as null while the link goes.
    let (first, second, outcome) = cut(2, 0, 3, &layer(N, N, None, LINK));
    assert_eq!(first, layer(N, at(1), None, NO_LINK));
    assert_eq!(second, layer(at(2), at(3), None, NO_LINK));
    assert_eq!(outcome, TimeLinkOutcome::TimeLinkDropped);
}

#[test]
fn a_cut_without_a_live_link_keeps_the_slot_exactly() {
    // Pin `split 1 4 3 2/5/u,n,2/u`: absent stays absent, markers are copied to both halves, the stored
    // out point 5 is replaced by the cut and the resolved out edge 4.
    let plain = layer(at(2), at(5), markers(&[A, N, at(2)]), NO_LINK);
    let (first, second, outcome) = cut(3, 1, 4, &plain);
    assert_eq!(first, layer(at(2), at(2), markers(&[A, N, at(2)]), NO_LINK));
    assert_eq!(
        second,
        layer(at(3), at(4), markers(&[A, N, at(2)]), NO_LINK)
    );
    assert_eq!(outcome, TimeLinkOutcome::Unchanged);
    // Pin `split 0 3 2 n/n/u,n,2/n`: a null link is not a link; it is kept on the first half only.
    let nulled = layer(N, N, markers(&[A, N, at(2)]), NULL_LINK);
    let (first, second, outcome) = cut(2, 0, 3, &nulled);
    assert_eq!(first, layer(N, at(1), markers(&[A, N, at(2)]), NULL_LINK));
    assert_eq!(
        second,
        layer(at(2), at(3), markers(&[A, N, at(2)]), NO_LINK)
    );
    assert_eq!(outcome, TimeLinkOutcome::Unchanged);
}

#[test]
fn a_cut_outside_the_resolved_edges_is_refused() {
    // Pins `split 1 4 1`, `split 1 4 5` refused and `split 1 4 4` cut: the in edge itself and anything past
    // the out edge refuse; the out edge is the last cut position (halves 1..3 and 4..4).
    let plain = layer(A, A, None, NO_LINK);
    assert_eq!(split_cut(1, 1, 4, &plain), SplitCutOutcome::OutsideLayer);
    assert_eq!(split_cut(0, 1, 4, &plain), SplitCutOutcome::OutsideLayer);
    assert_eq!(split_cut(5, 1, 4, &plain), SplitCutOutcome::OutsideLayer);
    let (first, second, _) = cut(4, 1, 4, &plain);
    assert_eq!(first, layer(A, at(3), None, NO_LINK));
    assert_eq!(second, layer(at(4), at(4), None, NO_LINK));
}
