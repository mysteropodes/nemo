//! The oracle must keep reaching every branch the retime decides on: a regenerated retime.txt that silently dropped a
//! branch (a ripple, the clamp, a destination that replaces an unmoved key or a sample, a squashed span, a dense stretch
//! that fills holes, a collapsed span, a key inside the new span, a manual in-between that survives) would leave a vacuous
//! green run. The counts are pinned and printed independently by tools/reach_counts.js (the same predicates written a
//! second time, straight from the rows of the file).
use super::build::Cell;
use super::rows::{rows, Row};

fn keys(cells: &[Cell]) -> Vec<i64> {
    cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.flag == 'K')
        .map(|(frame, _)| frame as i64)
        .collect()
}
fn is_sample(cell: &Cell) -> bool {
    matches!(cell.flag, 'I' | 'M' | 'N')
}
fn changed(row: &Row) -> bool {
    row.outcome == "changed"
}
fn landed(row: &Row) -> i64 {
    i64::from(row.applied.unwrap())
}
/// A consecutive key pair of the input with its new frames, derived from the landing frame alone.
struct Pair {
    a: i64,
    b: i64,
    na: i64,
    nb: i64,
    samples: i64,
    inside: bool,
}
fn pairs_of(row: &Row) -> Vec<Pair> {
    let (before, after) = row.subject();
    let old = keys(before);
    let delta = landed(row) - row.from;
    let moved = |frame: i64| {
        if frame >= row.from {
            frame + delta
        } else {
            frame
        }
    };
    let new_keys = keys(after);
    old.windows(2)
        .map(|window| {
            let (a, b) = (window[0], window[1]);
            let (na, nb) = (moved(a), moved(b));
            Pair {
                a,
                b,
                na,
                nb,
                samples: (a + 1..b)
                    .filter(|frame| is_sample(&before[*frame as usize]))
                    .count() as i64,
                inside: new_keys.iter().any(|key| *key > na && *key < nb),
            }
        })
        .collect()
}
fn moved_pair(pair: &Pair) -> bool {
    pair.samples > 0 && (pair.na != pair.a || pair.nb != pair.b)
}
fn relayed(pair: &Pair) -> bool {
    moved_pair(pair) && pair.nb > pair.na && !pair.inside
}
fn any_pair(row: &Row, test: &dyn Fn(&Pair) -> bool) -> bool {
    changed(row) && pairs_of(row).iter().any(test)
}
fn later_key(row: &Row) -> bool {
    keys(row.subject().0).iter().any(|key| *key > row.from)
}
/// (source, destination) of every key the baseline moved.
fn moves(row: &Row) -> Vec<(i64, i64)> {
    let delta = landed(row) - row.from;
    keys(row.subject().0)
        .into_iter()
        .filter(|key| *key >= row.from)
        .map(|key| (key, key + delta))
        .collect()
}
fn count(list: &[Row], select: &dyn Fn(&Row) -> bool) -> usize {
    list.iter().filter(|row| select(row)).count()
}
fn outcomes(list: &[Row]) -> [usize; 6] {
    ["changed", "noop", "same", "locked", "range", "notkey"]
        .map(|outcome| count(list, &|row| row.outcome == outcome))
}

#[test]
fn oracle_reaches_every_branch() {
    let (locked, unlocked): (Vec<Row>, Vec<Row>) = rows().into_iter().partition(Row::any_lock);
    assert_eq!((unlocked.len(), locked.len()), (557, 66));
    let reach = [
        count(&unlocked, &|row| changed(row) && later_key(row)),
        count(&unlocked, &|row| changed(row) && !later_key(row)),
        count(&unlocked, &|row| changed(row) && landed(row) > row.from),
        count(&unlocked, &|row| changed(row) && landed(row) < row.from),
        count(&unlocked, &|row| changed(row) && landed(row) != row.to),
        count(&unlocked, &|row| {
            let (before, _) = row.subject();
            // `moves` reads the landing frame, which only a changed row has
            changed(row) && {
                let all = moves(row);
                all.iter().any(|(_, dest)| {
                    before[*dest as usize].flag == 'K' && !all.iter().any(|(src, _)| src == dest)
                })
            }
        }),
        count(&unlocked, &|row| {
            let (before, _) = row.subject();
            changed(row)
                && moves(row)
                    .iter()
                    .any(|(_, dest)| is_sample(&before[*dest as usize]))
        }),
        count(&unlocked, &|row| {
            let (before, after) = row.subject();
            changed(row)
                && before
                    .iter()
                    .zip(after.iter())
                    .any(|(old, new)| old.flag == 'K' && !old.strokes.is_empty() && new.flag == '.')
        }),
        count(&unlocked, &|row| any_pair(row, &relayed)),
        count(&unlocked, &|row| {
            any_pair(row, &|pair| {
                relayed(pair)
                    && pair.samples == pair.b - pair.a - 1
                    && pair.nb - pair.na > pair.b - pair.a
            })
        }),
        count(&unlocked, &|row| {
            any_pair(row, &|pair| {
                relayed(pair)
                    && pair.samples == pair.b - pair.a - 1
                    && pair.nb - pair.na < pair.b - pair.a
            })
        }),
        count(&unlocked, &|row| {
            any_pair(row, &|pair| {
                relayed(pair) && pair.samples < pair.b - pair.a - 1
            })
        }),
        count(&unlocked, &|row| {
            any_pair(row, &|pair| moved_pair(pair) && pair.nb <= pair.na)
        }),
        count(&unlocked, &|row| {
            any_pair(row, &|pair| {
                moved_pair(pair) && pair.nb > pair.na && pair.inside
            })
        }),
        count(&unlocked, &|row| {
            let (before, after) = row.subject();
            changed(row)
                && before.iter().any(|cell| cell.flag == 'M')
                && after.iter().any(|cell| cell.flag == 'M')
        }),
        count(&unlocked, &|row| {
            let (before, after) = row.subject();
            changed(row)
                && before.iter().any(|cell| cell.flag == 'N')
                && after.iter().any(|cell| cell.flag == 'N')
        }),
    ];
    assert_eq!(
        reach,
        [168, 149, 150, 167, 60, 53, 212, 270, 142, 20, 24, 55, 49, 15, 167, 84]
    );
    // outcome classes [changed, noop, same, locked, range, notkey]: only a locked subject is ever refused as locked
    assert_eq!(outcomes(&unlocked), [317, 69, 104, 0, 19, 48]);
    assert_eq!(outcomes(&locked), [28, 5, 13, 20, 0, 0]);
}
