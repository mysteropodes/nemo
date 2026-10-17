//! The oracle must exercise every branch the kernel has: each predicate below is written twice (here and in
//! tools/reach_counts.js) and its count over documents.txt is pinned, so a regenerated oracle that loses a
//! branch fails instead of passing vacuously.
use super::build::Cell;
use super::tests::{rows, Row};

fn sample(c: &Cell) -> bool {
    "IMN".contains(c.flag)
}
fn has(layer: &[Cell], frame: usize) -> bool {
    layer.get(frame).is_some_and(|c| !c.strokes.is_empty())
}
fn one(r: &Row) -> usize {
    r.args.parse().unwrap()
}
fn nums(r: &Row) -> Vec<usize> {
    r.args.split(',').map(|n| n.parse().unwrap()).collect()
}
fn span(r: &Row) -> (usize, usize) {
    let (a, b) = r.args.split_once(':').unwrap();
    (a.parse().unwrap(), b.parse().unwrap())
}
fn stroke_cells(layers: &[Vec<Cell>]) -> usize {
    layers
        .iter()
        .flatten()
        .filter(|c| !c.strokes.is_empty())
        .count()
}
fn gap_split(r: &Row) -> bool {
    if !(r.op == "gap" && r.roles.starts_with("GP")) {
        return false;
    }
    (one(r) + 1..r.input[0].len()).any(|f| has(&r.input[0], f) && has(&r.input[1], f))
}
fn gap_merge(r: &Row) -> bool {
    if !(r.op == "gap" && r.roles.starts_with("PG")) {
        return false;
    }
    (one(r) + 1..r.input[0].len()).any(|f| has(&r.input[1], f) && has(&r.input[0], f + 1))
}
fn promote_one(r: &Row) -> bool {
    r.op == "promote" && r.roles == "a" && r.chg
}
fn blank_source(r: &Row) -> bool {
    if !promote_one(r) {
        return false;
    }
    let cell = one(r);
    let keys: Vec<usize> = (0..cell).filter(|f| r.input[0][*f].flag == 'K').collect();
    r.input[0][cell].flag == '.'
        && keys
            .last()
            .is_some_and(|k| r.input[0][*k].strokes.is_empty())
}
fn demote_with_sample_strokes(r: &Row) -> bool {
    if !(r.op == "demote" && r.chg) {
        return false;
    }
    let (a, b) = span(r);
    let inside = |f: usize, c: &Cell| f > a && f < b && sample(c) && !c.strokes.is_empty();
    r.input[0].iter().enumerate().any(|(f, c)| inside(f, c))
}
fn hold_after_sample(r: &Row) -> bool {
    if !(r.op == "promote" && r.roles == "c") {
        return false;
    }
    let cells = nums(r);
    let (input, output) = (&r.input[0], &r.output[0]);
    (0..cells.len()).any(|a| {
        (a + 1..cells.len()).any(|b| {
            let (i, j) = (cells[a], cells[b]);
            i < j
                && sample(&input[i])
                && input[j].flag == '.'
                && !input[i].strokes.is_empty()
                && !input[i + 1..=j].iter().any(|c| c.flag == 'K')
                && output[j].strokes == input[i].strokes
        })
    })
}
const PREDICATES: [(&str, fn(&Row) -> bool); 22] = [
    ("gap_first_spliced", |r| {
        r.op == "gap" && r.roles.starts_with('G')
    }),
    ("gap_first_padded", |r| {
        r.op == "gap" && r.roles.starts_with('P')
    }),
    ("gap_three_layers", |r| r.op == "gap" && r.roles.len() == 3),
    ("gap_scope_split", gap_split),
    ("gap_scope_merge", gap_merge),
    ("rm_single", |r| r.op == "rm" && r.roles == "f"),
    ("rm_span", |r| r.op == "rm" && r.roles == "s"),
    ("rm_refused", |r| r.op == "rm" && !r.chg),
    ("rm_drops_strokes", |r| {
        r.op == "rm" && r.chg && stroke_cells(&r.input) > stroke_cells(&r.output)
    }),
    ("promote_held_copies", |r| {
        promote_one(r) && r.input[0][one(r)].flag == '.' && !r.output[0][one(r)].strokes.is_empty()
    }),
    ("promote_held_blank_source", blank_source),
    ("promote_unexposed", |r| {
        promote_one(r)
            && r.input[0][..=one(r)].iter().all(|c| c.flag != 'K')
            && r.input[0][one(r)].flag == '.'
    }),
    ("promote_sample_keeps", |r| {
        promote_one(r)
            && sample(&r.input[0][one(r)])
            && r.output[0][one(r)].strokes.len() == r.input[0][one(r)].strokes.len()
    }),
    ("promote_key_nochange", |r| {
        r.op == "promote" && r.roles == "a" && !r.chg
    }),
    ("promote_multi", |r| {
        r.op == "promote" && r.roles == "c" && nums(r).len() > 1
    }),
    ("promote_hold_after_sample", hold_after_sample),
    ("clearkey_drops_strokes", |r| {
        r.op == "clearkey" && r.chg && !r.input[0][one(r)].strokes.is_empty()
    }),
    ("clearkey_nochange", |r| r.op == "clearkey" && !r.chg),
    ("clear_sample", |r| {
        r.op == "clear" && r.chg && nums(r).iter().any(|c| sample(&r.input[0][*c]))
    }),
    ("clear_nochange", |r| r.op == "clear" && !r.chg),
    ("demote_drops_sample_strokes", demote_with_sample_strokes),
    ("demote_nochange", |r| r.op == "demote" && !r.chg),
];
const REACH: [usize; 22] = [
    184, 55, 89, 50, 10, 111, 122, 43, 161, 28, 2, 25, 44, 46, 128, 10, 29, 80, 60, 14, 18, 78,
];

#[test]
fn oracle_reaches_every_branch() {
    let all = rows();
    for (index, (name, predicate)) in PREDICATES.iter().enumerate() {
        let count = all.iter().filter(|row| predicate(row)).count();
        assert_eq!(count, REACH[index], "reach {name}");
        assert!(count > 0, "reach {name} is vacuous");
    }
}
