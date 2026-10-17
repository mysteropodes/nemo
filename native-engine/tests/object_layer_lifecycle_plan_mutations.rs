//! Mutation controls for the oracle (L02-E): small stand-alone models of the
//! plan rules, each in a faithful form and in one deliberately wrong form, replayed
//! over the oracle rows. The faithful model must agree with every row, and each
//! mutant must disagree with an exact, pinned number of rows (counts produced by
//! `tools/shadow_check.js`). A mutant that agreed everywhere would mean the oracle
//! cannot see that mistake; a changed count means a row set was altered.

use super::fixture::PALETTE;
use super::tests::{ints, rows, texts};
use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    None,
    Floor,
    GapAsInsert,
    SelectionBySlot,
    RefuseCoverAll,
    ActiveFollows,
    NoUnique,
    AlwaysSelection,
    ClickedOnly,
    Trim,
}

fn count_of(row: &Value, key: &str) -> usize {
    row["in"][key].as_u64().expect("count") as usize
}

/// Reorder model: (names after, active, selected, anchor) as the oracle prints them.
fn reorder(row: &Value, fault: Fault) -> (Vec<String>, i64, Vec<i64>, i64) {
    let input = &row["in"];
    let count = count_of(row, "n");
    let mut block: Vec<usize> = Vec::new();
    for raw in ints(&input["moving"]) {
        if raw >= 0 && (raw as usize) < count && !block.contains(&(raw as usize)) {
            block.push(raw as usize);
        }
    }
    block.sort_unstable();
    let gap = input["gap"].as_f64().expect("gap");
    let floor = gap.floor();
    let gap = match fault {
        Fault::Floor => floor,
        _ if gap - floor >= 0.5 => floor + 1.0,
        _ => floor,
    };
    let gap = gap.max(0.0).min(count as f64) as usize;
    let mut insert_at = (0..gap).filter(|i| !block.contains(i)).count();
    if fault == Fault::GapAsInsert {
        insert_at = gap.min(count - block.len());
    }
    let in_place = block.iter().enumerate().all(|(j, o)| *o == insert_at + j);
    let mut order: Vec<usize> = (0..count).collect();
    if !in_place {
        let kept: Vec<usize> = (0..count).filter(|i| !block.contains(i)).collect();
        order = [&kept[..insert_at], &block[..], &kept[insert_at..]].concat();
    }
    let place = |old: i64| -> i64 {
        if fault == Fault::SelectionBySlot {
            return old;
        }
        order.iter().position(|o| *o as i64 == old).expect("layer") as i64
    };
    let anchor = input["anchor"].as_i64().expect("anchor");
    (
        order.iter().map(|i| format!("L{i}")).collect(),
        place(input["active"].as_i64().expect("active")),
        ints(&input["sel"])
            .into_iter()
            .map(|old| place(old))
            .collect(),
        if anchor < 0 { -1 } else { place(anchor) },
    )
}

fn reorder_misses(fault: Fault) -> usize {
    rows("reorder")
        .iter()
        .filter(|row| {
            let out = &row["o"];
            let want = (
                texts(&out["order"]),
                out["active"].as_i64().expect("active"),
                ints(&out["sel"]),
                out["anchor"].as_i64().expect("anchor"),
            );
            reorder(row, fault) != want
        })
        .count()
}

/// Delete model: `None` is a refusal, else (kept names, active slot).
fn delete(row: &Value, fault: Fault) -> Option<(Vec<String>, usize)> {
    let input = &row["in"];
    let (count, active) = (
        count_of(row, "n"),
        input["active"].as_i64().expect("active") as usize,
    );
    if count <= 1 {
        return None;
    }
    let mut targets: Vec<usize> = Vec::new();
    let chosen = ints(&input["sel"]);
    let picks = if chosen.is_empty() {
        vec![active as i64]
    } else {
        chosen
    };
    for index in picks {
        if !targets.contains(&(index as usize)) {
            targets.push(index as usize);
        }
    }
    targets.sort_unstable_by(|a, b| b.cmp(a));
    if fault == Fault::RefuseCoverAll && targets.len() == count {
        return None;
    }
    let mut remaining = count;
    let mut removed: Vec<usize> = Vec::new();
    for &index in &targets {
        if remaining > 1 {
            removed.push(index);
            remaining -= 1;
        }
    }
    let kept: Vec<usize> = (0..count).filter(|i| !removed.contains(i)).collect();
    let mut slot = active.min(kept.len() - 1);
    if fault == Fault::ActiveFollows {
        slot = kept.iter().position(|k| *k == active).unwrap_or(slot);
    }
    Some((kept.iter().map(|i| format!("L{i}")).collect(), slot))
}

fn delete_misses(fault: Fault) -> usize {
    rows("delete")
        .iter()
        .filter(|row| {
            let out = &row["o"];
            let want = if out["refused"] == "last" {
                None
            } else {
                Some((
                    texts(&out["kept"]),
                    out["active"].as_u64().expect("active") as usize,
                ))
            };
            delete(row, fault) != want
        })
        .count()
}

fn unique(base: &str, names: &[String], fault: Fault) -> String {
    if fault == Fault::NoUnique || !names.iter().any(|n| n == base) {
        return base.to_string();
    }
    let mut number = 2;
    while names.iter().any(|n| *n == format!("{base} {number}")) {
        number += 1;
    }
    format!("{base} {number}")
}

fn unique_misses(fault: Fault) -> usize {
    let unique_rows = rows("unique");
    let plain = unique_rows.iter().filter(|row| {
        let (names, base) = (
            texts(&row["in"]["names"]),
            row["in"]["base"].as_str().expect("base"),
        );
        unique(base, &names, fault) != row["o"]["name"].as_str().expect("name")
    });
    let copies = rows("duplicate")
        .iter()
        .filter(|row| {
            let names = texts(&row["in"]["names"]);
            let source = &names[row["in"]["src"].as_u64().expect("src") as usize];
            let made = texts(&row["o"]["names"]).pop().expect("copy");
            unique(&format!("{source} copy"), &names, fault) != made
        })
        .count();
    plain.count() + copies
}

fn recolor(row: &Value, fault: Fault) -> Vec<String> {
    let input = &row["in"];
    let mut colors: Vec<String> = (0..count_of(row, "n"))
        .map(|i| PALETTE[i % 5].to_string())
        .collect();
    let (selected, clicked) = (ints(&input["sel"]), input["li"].as_i64().expect("li"));
    let painted = selected.len() > 1
        && fault != Fault::ClickedOnly
        && (fault == Fault::AlwaysSelection || selected.contains(&clicked));
    let targets = if painted { selected } else { vec![clicked] };
    for index in targets {
        colors[index as usize] = input["hex"].as_str().expect("hex").to_string();
    }
    colors
}

fn recolor_misses(fault: Fault) -> usize {
    rows("recolor")
        .iter()
        .filter(|row| recolor(row, fault) != texts(&row["o"]["colors"]))
        .count()
}

fn rename_misses(fault: Fault) -> usize {
    rows("rename")
        .iter()
        .filter(|row| {
            let mut names = texts(&row["in"]["names"]);
            let (index, name) = (
                row["in"]["idx"].as_u64().expect("idx") as usize,
                row["in"]["name"].as_str().expect("name"),
            );
            names[index] = if fault == Fault::Trim {
                name.trim().to_string()
            } else {
                name.to_string()
            };
            names != texts(&row["o"]["names"])
        })
        .count()
}

#[test]
fn faithful_models_agree_with_every_oracle_row() {
    assert_eq!(reorder_misses(Fault::None), 0);
    assert_eq!(delete_misses(Fault::None), 0);
    assert_eq!(unique_misses(Fault::None), 0);
    assert_eq!(recolor_misses(Fault::None), 0);
    assert_eq!(rename_misses(Fault::None), 0);
}

#[test]
fn reorder_mistakes_are_visible_to_the_oracle() {
    assert_eq!(
        reorder_misses(Fault::Floor),
        4,
        "floor instead of Math.round"
    );
    assert_eq!(
        reorder_misses(Fault::GapAsInsert),
        114,
        "gap used as the insert index"
    );
    assert_eq!(
        reorder_misses(Fault::SelectionBySlot),
        197,
        "selection kept by slot, not by layer"
    );
}

#[test]
fn delete_mistakes_are_visible_to_the_oracle() {
    assert_eq!(
        delete_misses(Fault::RefuseCoverAll),
        14,
        "refusing instead of sparing the lowest slot"
    );
    assert_eq!(
        delete_misses(Fault::ActiveFollows),
        24,
        "active layer followed instead of its slot number"
    );
}

#[test]
fn naming_color_and_rename_mistakes_are_visible_to_the_oracle() {
    assert_eq!(
        unique_misses(Fault::NoUnique),
        4 + 3,
        "copy names without the first-free rule"
    );
    assert_eq!(
        recolor_misses(Fault::AlwaysSelection),
        3,
        "selection painted when the click is outside it"
    );
    assert_eq!(recolor_misses(Fault::ClickedOnly), 7, "selection ignored");
    assert_eq!(rename_misses(Fault::Trim), 1, "name trimmed");
}
