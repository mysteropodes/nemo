//! The oracle must keep reaching every branch the transactions decide on: a regenerated keys.txt that silently dropped
//! a branch (a multi-layer F6, a subset selection, a locked layer in the way, a hold that copies content, a sample that
//! keeps its strokes, Delete Frames across layers, a Remove Tween with a sample inside) would leave a vacuous green run.
//! The counts are pinned and printed independently by tools/reach_counts.js (the same predicates written a second time).
use super::build::Cell;
use super::rows::{rows, Row};

fn is_promote(row: &Row) -> bool {
    matches!(row.op.as_str(), "f6" | "at" | "conv")
}
/// Some cell of some layer satisfies `test(input cell, output cell)`.
fn any_cell(row: &Row, test: &dyn Fn(&Cell, &Cell) -> bool) -> bool {
    row.input
        .iter()
        .zip(row.output.iter())
        .any(|(before, after)| {
            before
                .iter()
                .zip(after.iter())
                .any(|(old, new)| test(old, new))
        })
}
fn count(list: &[Row], select: &dyn Fn(&Row) -> bool) -> usize {
    list.iter().filter(|row| select(row)).count()
}
fn outcomes(list: &[Row], op: &str) -> [usize; 3] {
    ["changed", "noop", "locked"].map(|outcome| {
        count(list, &|row| {
            (op.is_empty() || row.op == op) && row.outcome == outcome
        })
    })
}
/// A conv selection that names a cell of a layer other than the active one (the baseline ignores it).
fn conv_with_foreign_cell(row: &Row) -> bool {
    if row.op != "conv" {
        return false;
    }
    let (active, list) = row.args.split_once('/').unwrap();
    list.split(',')
        .any(|pair| pair.split_once(':').unwrap().0 != active)
}
fn clearkey_drops_strokes(row: &Row) -> bool {
    if row.op != "clearkey" || row.outcome != "changed" {
        return false;
    }
    let (layer, frame) = row.args.split_once(':').unwrap();
    let cell = &row.input[layer.parse::<usize>().unwrap()][frame.parse::<usize>().unwrap()];
    !cell.strokes.is_empty()
}

#[test]
fn oracle_reaches_every_branch() {
    let (locked, unlocked): (Vec<Row>, Vec<Row>) = rows().into_iter().partition(Row::any_lock);
    assert_eq!((unlocked.len(), locked.len()), (839, 375));
    let reach = [
        count(&unlocked, &|row| {
            row.op == "f6"
                && row.sel == "-"
                && row.outcome == "changed"
                && row.changed_layers() >= 2
        }),
        count(&unlocked, &|row| row.op == "f6" && row.sel != "-"),
        count(&unlocked, &|row| row.op == "f6" && row.outcome == "noop"),
        count(&unlocked, &|row| row.op == "at" && row.outcome == "changed"),
        count(&unlocked, &|row| row.op == "at" && row.outcome == "noop"),
        count(&unlocked, &|row| {
            row.op == "conv" && row.outcome == "changed" && row.conv_cells().len() >= 2
        }),
        count(&unlocked, &conv_with_foreign_cell),
        count(&unlocked, &|row| {
            row.op == "clearkey" && row.outcome == "changed"
        }),
        count(&unlocked, &|row| {
            row.op == "clearkey" && row.outcome == "noop"
        }),
        count(&unlocked, &clearkey_drops_strokes),
        count(&unlocked, &|row| {
            row.op == "dsel" && row.outcome == "changed" && row.changed_layers() >= 2
        }),
        count(&unlocked, &|row| row.op == "dsel" && row.outcome == "noop"),
        count(&unlocked, &|row| {
            row.op == "demote" && row.outcome == "changed"
        }),
        count(&unlocked, &|row| {
            row.op == "demote" && row.outcome == "noop"
        }),
        count(&unlocked, &|row| {
            is_promote(row) && any_cell(row, &|old, new| old.flag == '.' && !new.strokes.is_empty())
        }),
        count(&unlocked, &|row| {
            is_promote(row)
                && any_cell(row, &|old, new| {
                    matches!(old.flag, 'I' | 'M' | 'N')
                        && new.flag == 'K'
                        && new.strokes == old.strokes
                })
        }),
    ];
    assert_eq!(
        reach,
        [68, 145, 59, 62, 35, 83, 90, 33, 65, 26, 65, 17, 39, 87, 154, 220]
    );
    // outcome classes [changed, noop, locked]: nothing unlocked is ever refused, and every refusal is a locked row
    assert_eq!(outcomes(&unlocked, ""), [569, 270, 0]);
    assert_eq!(outcomes(&locked, ""), [163, 0, 212]);
    // a locked layer in the way: F6 and Delete Frames still change the other layers, every single-layer gesture refuses
    assert_eq!(outcomes(&locked, "f6"), [74, 0, 66]);
    assert_eq!(outcomes(&locked, "dsel"), [89, 0, 51]);
    for single in ["at", "conv", "clearkey", "demote"] {
        let counted = outcomes(&locked, single);
        assert_eq!((counted[0], counted[1]), (0, 0), "{single}");
        assert!(counted[2] > 0, "{single}");
    }
}
