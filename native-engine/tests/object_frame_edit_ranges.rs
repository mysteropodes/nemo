//! The range kernel against the verbatim dense baseline editor (fbe4b8a3).
//!
//! Every oracle row is one run of the baseline function (src/js/app.js insertFrame, removeFrame,
//! removeFrameSpan; src/js/timeline.js duplicateKeyframe, extendExposure, splitLayerAtPlayhead in a
//! Node vm) over three layers that carry inPoint, outPoint, markers and timeLink, plus the
//! composition markers and the work-area end. Layers are written `IN/OUT/MARKERS/TL`: a bound is
//! `u` (absent), `n` (null) or the frame number; markers are `x` (no array), `e` (empty) or a comma
//! list; the link is `u`, `n` or `L`. The kernel output is re-encoded to the same tokens and
//! compared as text, so absent, null and an explicit 0 can never be confused. Mutation controls
//! live in object_frame_edit_ranges_controls.rs.
use super::{
    split_cut, FrameBound, FrameEditKind, LayerRanges, LayerRole, RangeEdit, RangeEditError,
    SplitCutOutcome, TimeLinkOutcome, TimeLinkSlot,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-frame-edit-baseline/ranges.txt");

pub(crate) fn parse_bound(text: &str) -> FrameBound {
    match text {
        "u" => FrameBound::Absent,
        "n" => FrameBound::Null,
        frame => FrameBound::At(frame.parse().unwrap()),
    }
}

pub(crate) fn bound_text(bound: FrameBound) -> String {
    match bound {
        FrameBound::Absent => "u".to_string(),
        FrameBound::Null => "n".to_string(),
        FrameBound::At(frame) => frame.to_string(),
    }
}

pub(crate) fn parse_markers(text: &str) -> Option<Vec<FrameBound>> {
    match text {
        "x" => None,
        "e" => Some(Vec::new()),
        list => Some(list.split(',').map(parse_bound).collect()),
    }
}

pub(crate) fn markers_text(markers: &Option<Vec<FrameBound>>) -> String {
    match markers {
        None => "x".to_string(),
        Some(list) if list.is_empty() => "e".to_string(),
        Some(list) => list
            .iter()
            .map(|marker| bound_text(*marker))
            .collect::<Vec<String>>()
            .join(","),
    }
}

fn parse_link(text: &str) -> TimeLinkSlot {
    match text {
        "u" => TimeLinkSlot::Absent,
        "n" => TimeLinkSlot::Null,
        "L" => TimeLinkSlot::Linked,
        other => panic!("bad link token {other}"),
    }
}

fn link_text(link: TimeLinkSlot) -> &'static str {
    match link {
        TimeLinkSlot::Absent => "u",
        TimeLinkSlot::Null => "n",
        TimeLinkSlot::Linked => "L",
    }
}

pub(crate) fn parse_layer(text: &str) -> LayerRanges {
    let part: Vec<&str> = text.split('/').collect();
    LayerRanges {
        in_point: parse_bound(part[0]),
        out_point: parse_bound(part[1]),
        markers: parse_markers(part[2]),
        time_link: parse_link(part[3]),
    }
}

pub(crate) fn layer_text(layer: &LayerRanges) -> String {
    format!(
        "{}/{}/{}/{}",
        bound_text(layer.in_point),
        bound_text(layer.out_point),
        markers_text(&layer.markers),
        link_text(layer.time_link)
    )
}

/// One insert / remove / span / extend / dupkey row: `OP TOTAL CURSOR COUNT WA COMP LOCKS ROLES
/// L0 L1 L2 | TOTAL2 WA2 COMP2 L0' L1' L2'` (the tail is `refused - - - - -` when the baseline
/// leaves the document untouched).
pub(crate) struct Row {
    pub(crate) op: &'static str,
    pub(crate) total: u32,
    pub(crate) cursor: u32,
    pub(crate) count: u32,
    pub(crate) wa: i64,
    pub(crate) comp: Option<Vec<FrameBound>>,
    pub(crate) roles: &'static str,
    pub(crate) layers: Vec<LayerRanges>,
    pub(crate) expect: Vec<&'static str>,
}

impl Row {
    pub(crate) fn kind(&self) -> FrameEditKind {
        match self.op {
            "insert" => FrameEditKind::InsertFrame,
            "remove" => FrameEditKind::RemoveFrame,
            "span" => FrameEditKind::RemoveFrameSpan { count: self.count },
            "extend" => FrameEditKind::ExtendExposure { count: self.count },
            "dupkey" => FrameEditKind::DuplicateKeyframe,
            other => panic!("bad op {other}"),
        }
    }

    pub(crate) fn role(&self, index: usize) -> LayerRole {
        if self.roles.as_bytes()[index] == b'P' {
            LayerRole::Padded
        } else {
            LayerRole::Targeted
        }
    }

    pub(crate) fn refused(&self) -> bool {
        self.expect[0] == "refused"
    }
}

fn is_data(line: &&str) -> bool {
    !line.is_empty() && !line.starts_with('#')
}

fn parse_row(line: &'static str) -> Row {
    let t: Vec<&'static str> = line.split(' ').collect();
    Row {
        op: t[0],
        total: t[1].parse().unwrap(),
        cursor: t[2].parse().unwrap(),
        count: if t[3] == "-" {
            0
        } else {
            t[3].parse().unwrap()
        },
        wa: t[4].parse().unwrap(),
        comp: parse_markers(t[5]),
        roles: t[7],
        layers: t[8..11].iter().map(|text| parse_layer(text)).collect(),
        expect: t[11..].to_vec(),
    }
}

pub(crate) fn all_rows() -> Vec<Row> {
    ORACLE
        .lines()
        .filter(is_data)
        .filter(|line| !line.starts_with("split "))
        .map(parse_row)
        .collect()
}

pub(crate) fn rows(op: &str) -> Vec<Row> {
    all_rows().into_iter().filter(|row| row.op == op).collect()
}

/// The tokens an operation yields: new total, new work-area end, composition markers, three layers.
pub(crate) type Applied = Result<Vec<String>, RangeEditError>;

pub(crate) fn kernel(row: &Row) -> Applied {
    let edit = RangeEdit {
        kind: row.kind(),
        cursor: row.cursor,
        total: row.total,
    };
    let mut out = vec![
        edit.new_total()?.to_string(),
        edit.work_area_out(row.wa)?.to_string(),
        markers_text(&edit.comp_markers(&row.comp)?),
    ];
    for (index, layer) in row.layers.iter().enumerate() {
        out.push(layer_text(&edit.layer_ranges(row.role(index), layer)?));
    }
    Ok(out)
}

pub(crate) fn expected(row: &Row) -> Applied {
    if row.refused() {
        Err(RangeEditError::WouldEmptyTimeline)
    } else {
        Ok(row.expect.iter().map(|token| token.to_string()).collect())
    }
}

/// Number of non-split oracle rows on which `applier` disagrees with the baseline.
pub(crate) fn mismatches(applier: &dyn Fn(&Row) -> Applied) -> usize {
    all_rows()
        .iter()
        .filter(|row| applier(row) != expected(row))
        .count()
}

/// `split EFFIN EFFOUT CUT LAYER (cut|refused) LAYER2 SECOND`.
pub(crate) struct SplitRow {
    pub(crate) eff_in: i64,
    pub(crate) eff_out: i64,
    pub(crate) cut: i64,
    pub(crate) original: LayerRanges,
    pub(crate) expect: Vec<&'static str>,
}

pub(crate) fn split_rows() -> Vec<SplitRow> {
    ORACLE
        .lines()
        .filter(is_data)
        .filter(|line| line.starts_with("split "))
        .map(|line| {
            let t: Vec<&'static str> = line.split(' ').collect();
            SplitRow {
                eff_in: t[1].parse().unwrap(),
                eff_out: t[2].parse().unwrap(),
                cut: t[3].parse().unwrap(),
                original: parse_layer(t[4]),
                expect: t[5..].to_vec(),
            }
        })
        .collect()
}

pub(crate) fn split_kernel(row: &SplitRow) -> Vec<String> {
    match split_cut(row.cut, row.eff_in, row.eff_out, &row.original) {
        SplitCutOutcome::OutsideLayer => {
            vec!["refused".to_string(), "-".to_string(), "-".to_string()]
        }
        SplitCutOutcome::Cut(cut) => vec![
            "cut".to_string(),
            layer_text(&cut.original),
            layer_text(&cut.second),
        ],
    }
}

pub(crate) fn split_mismatches(applier: &dyn Fn(&SplitRow) -> Vec<String>) -> usize {
    split_rows()
        .iter()
        .filter(|row| {
            let want: Vec<String> = row.expect.iter().map(|token| token.to_string()).collect();
            applier(row) != want
        })
        .count()
}

fn ranges_moved(row: &Row) -> bool {
    !row.refused()
        && (row.expect[2] != markers_text(&row.comp)
            || (0..3).any(|index| row.expect[3 + index] != layer_text(&row.layers[index])))
}

#[test]
fn every_shift_row_matches_the_kernel() {
    assert_eq!(all_rows().len(), 1012);
    assert_eq!(mismatches(&kernel), 0);
}

#[test]
fn each_operation_matches_on_its_own_rows() {
    for (op, count) in [
        ("insert", 275),
        ("remove", 275),
        ("span", 112),
        ("extend", 175),
        ("dupkey", 175),
    ] {
        let list = rows(op);
        assert_eq!(list.len(), count, "{op} rows");
        for row in &list {
            assert_eq!(kernel(row), expected(row), "{op} {:?}", row.expect);
        }
    }
}

#[test]
fn every_split_row_matches_the_kernel() {
    assert_eq!(split_rows().len(), 792);
    assert_eq!(split_mismatches(&split_kernel), 0);
    for row in split_rows() {
        if let SplitCutOutcome::Cut(cut) =
            split_cut(row.cut, row.eff_in, row.eff_out, &row.original)
        {
            let linked = row.original.time_link == TimeLinkSlot::Linked;
            assert_eq!(cut.time_link == TimeLinkOutcome::TimeLinkDropped, linked);
            assert_eq!(cut.second.time_link, TimeLinkSlot::Absent);
        }
    }
}

#[test]
fn shift_ranges_flag_agrees_with_the_oracle() {
    for (op, flag) in [
        ("insert", true),
        ("remove", true),
        ("span", false),
        ("extend", false),
        ("dupkey", false),
    ] {
        let list = rows(op);
        assert_eq!(list[0].kind().shift_ranges(), flag, "{op} flag");
        let moved = list.iter().filter(|row| ranges_moved(row)).count();
        assert_eq!(moved > 0, flag, "{op}: baseline moved {moved} rows");
    }
}

#[test]
fn oracle_rows_are_internally_consistent() {
    for row in all_rows() {
        if row.refused() {
            assert!(
                row.total == 1 || row.op == "span",
                "{} refused at {}",
                row.op,
                row.total
            );
            continue;
        }
        let want_total = match row.op {
            "insert" | "dupkey" => row.total + 1,
            "remove" => row.total - 1,
            "span" => row.total - row.count,
            _ => row.total + row.count,
        };
        assert_eq!(row.expect[0], want_total.to_string());
        for (index, layer) in row.layers.iter().enumerate() {
            let out = parse_layer(row.expect[3 + index]);
            assert_eq!(
                out.time_link, layer.time_link,
                "time link never moves on {}",
                row.op
            );
            assert_eq!(layer_text(&out), row.expect[3 + index], "token round trip");
            assert_eq!(
                out.markers.as_ref().map(Vec::len),
                layer.markers.as_ref().map(Vec::len),
                "marker count"
            );
        }
        let comp = parse_markers(row.expect[2]);
        assert_eq!(comp.as_ref().map(Vec::len), row.comp.as_ref().map(Vec::len));
    }
}

fn input_bounds(row: &Row) -> Vec<FrameBound> {
    let mut all = Vec::new();
    for layer in &row.layers {
        all.push(layer.in_point);
        all.push(layer.out_point);
        all.extend(layer.markers.iter().flatten().copied());
    }
    all
}

fn count_bounds(op: &str, test: &dyn Fn(&Row, FrameBound) -> bool) -> usize {
    rows(op)
        .iter()
        .filter(|row| !row.refused())
        .map(|row| {
            input_bounds(row)
                .into_iter()
                .filter(|b| test(row, *b))
                .count()
        })
        .sum()
}

fn layers_moved(op: &str) -> usize {
    rows(op)
        .iter()
        .filter(|row| !row.refused())
        .map(|row| {
            (0..3)
                .filter(|index| row.expect[3 + index] != layer_text(&row.layers[*index]))
                .count()
        })
        .sum()
}

fn comp_moved(op: &str) -> usize {
    rows(op)
        .iter()
        .filter(|row| !row.refused() && row.expect[2] != markers_text(&row.comp))
        .count()
}

#[test]
fn oracle_reaches_every_branch() {
    // Each count was produced a second time in Node (tools/kernel_port.js) from the same text.
    assert_eq!(layers_moved("insert"), 261);
    assert_eq!(layers_moved("remove"), 478);
    for op in ["span", "extend", "dupkey"] {
        assert_eq!(layers_moved(op), 0, "{op} moves no layer range");
        assert_eq!(comp_moved(op), 0, "{op} moves no composition marker");
    }
    assert_eq!(comp_moved("insert"), 77);
    assert_eq!(comp_moved("remove"), 74);
    let at_cursor = |row: &Row, b: FrameBound| b == FrameBound::At(i64::from(row.cursor));
    assert_eq!(count_bounds("insert", &at_cursor), 219);
    assert_eq!(count_bounds("remove", &at_cursor), 198);
    let zero = |_: &Row, b: FrameBound| b == FrameBound::At(0);
    assert_eq!(count_bounds("insert", &zero), 205);
    assert_eq!(count_bounds("remove", &zero), 188);
    let null = |_: &Row, b: FrameBound| b == FrameBound::Null;
    assert_eq!(count_bounds("insert", &null), 227);
    assert_eq!(count_bounds("remove", &null), 198);
    let absent = |_: &Row, b: FrameBound| b == FrameBound::Absent;
    assert_eq!(count_bounds("insert", &absent), 841);
    assert_eq!(count_bounds("remove", &absent), 794);
    let roles = |role: u8| -> usize {
        rows("insert")
            .iter()
            .map(|row| row.roles.bytes().filter(|r| *r == role).count())
            .sum()
    };
    assert_eq!(roles(b'G'), 434);
    assert_eq!(roles(b'P'), 391);
    assert_eq!(
        rows("remove").iter().filter(|row| row.refused()).count(),
        16
    );
    assert_eq!(rows("span").iter().filter(|row| row.refused()).count(), 12);
    let collapsed = rows("remove")
        .iter()
        .filter(|row| !row.refused())
        .flat_map(|row| row.layers.iter().map(move |layer| (row.cursor, layer)))
        .filter(|(cursor, layer)| {
            layer
                .markers
                .iter()
                .flatten()
                .any(|m| *m == FrameBound::At(i64::from(*cursor) + 1))
        })
        .count();
    assert_eq!(collapsed, 69);
    let clamped = rows("remove")
        .iter()
        .filter(|row| !row.refused())
        .filter(|row| {
            row.wa == i64::from(row.total) - 1 && row.expect[1] == (row.total - 2).to_string()
        })
        .count();
    assert_eq!(clamped, 226);
    let split = split_rows();
    let cut: Vec<&SplitRow> = split.iter().filter(|r| r.expect[0] == "cut").collect();
    assert_eq!((cut.len(), split.len() - cut.len()), (216, 576));
    let on = |link: TimeLinkSlot| cut.iter().filter(|r| r.original.time_link == link).count();
    assert_eq!((on(TimeLinkSlot::Linked), on(TimeLinkSlot::Null)), (72, 72));
    let copied = cut
        .iter()
        .filter(|r| r.original.markers.as_ref().map_or(false, |m| !m.is_empty()))
        .count();
    assert_eq!(copied, 108);
}
