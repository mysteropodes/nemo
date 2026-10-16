//! Oracle replay of the duplicate, unique-name, rename and recolor rows (L02-E);
//! the generator, fixture and row format are described in
//! `object_layer_lifecycle_plan.rs`.

use super::fixture::{admitted, build, plain, uid, Spec};
use super::tests::{ints, rows, texts, usizes};
use super::{
    plan_duplicate, plan_recolor, plan_rename, unique_layer_name, UnavailableLayerEditPolicy,
};
use serde_json::Value;

fn specs_of(input: &Value) -> Vec<Spec> {
    let (colors, visible) = (
        texts(&input["colors"]),
        input["visible"].as_array().expect("visible"),
    );
    texts(&input["names"])
        .into_iter()
        .enumerate()
        .map(|(index, name)| Spec {
            name,
            color: colors[index].clone(),
            visible: visible[index].as_bool().expect("flag"),
        })
        .collect()
}

#[test]
fn duplicate_replays_every_oracle_row() {
    let mut checked = 0;
    for row in rows("duplicate") {
        let (input, out) = (&row["in"], &row["o"]);
        let before = build(&specs_of(input), 2);
        let source = uid(input["src"].as_i64().expect("src"));
        let plan = plan_duplicate(&before, &source, 7, &UnavailableLayerEditPolicy)
            .unwrap_or_else(|error| panic!("{error:?} for {row}"));
        assert!(admitted(&plan.document), "{row}");
        let layers = &plan.document.layers;
        let names: Vec<String> = layers
            .iter()
            .map(|l| l.name.value().cloned().expect("name"))
            .collect();
        let colors: Vec<String> = layers
            .iter()
            .map(|l| l.color.value().cloned().expect("color"))
            .collect();
        let visible: Vec<bool> = layers
            .iter()
            .map(|l| l.visible.value().copied().expect("visible"))
            .collect();
        assert_eq!(names, texts(&out["names"]), "{row}");
        assert_eq!(colors, texts(&out["colors"]), "{row}");
        let flags: Vec<bool> = out["visible"]
            .as_array()
            .expect("flags")
            .iter()
            .map(|v| v.as_bool().expect("flag"))
            .collect();
        assert_eq!(visible, flags, "{row}");
        assert_eq!(plan.created.as_deref(), Some("native-layer:7:0"), "{row}");
        assert_eq!(
            layers.last().expect("copy").layer_uid,
            "native-layer:7:0",
            "{row}"
        );
        let outcome = plan.selection.as_ref().expect("selection");
        assert_eq!(
            outcome.active as i64,
            out["active"].as_i64().expect("active"),
            "{row}"
        );
        assert_eq!(outcome.selected, usizes(&out["sel"]), "{row}");
        assert_eq!(
            outcome.anchor.map(|slot| slot as i64),
            out["anchor"].as_i64(),
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 12);
}

#[test]
fn unique_name_replays_every_oracle_row() {
    let mut checked = 0;
    for row in rows("unique") {
        let names = texts(&row["in"]["names"]);
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let base = row["in"]["base"].as_str().expect("base");
        assert_eq!(
            unique_layer_name(base, &refs),
            row["o"]["name"].as_str().expect("name"),
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 8);
}

#[test]
fn rename_replays_every_oracle_row() {
    let mut checked = 0;
    for row in rows("rename") {
        let (input, out) = (&row["in"], &row["o"]);
        let names = texts(&input["names"]);
        let specs: Vec<Spec> = plain(names.len())
            .into_iter()
            .zip(names.iter())
            .map(|(spec, name)| Spec {
                name: name.clone(),
                ..spec
            })
            .collect();
        let before = build(&specs, 2);
        let target = uid(input["idx"].as_i64().expect("idx"));
        let name = input["name"].as_str().expect("name");
        let plan =
            plan_rename(&before, &target, name, &UnavailableLayerEditPolicy).expect("rename");
        assert!(admitted(&plan.document), "{row}");
        let after: Vec<String> = plan
            .document
            .layers
            .iter()
            .map(|l| l.name.value().cloned().expect("name"))
            .collect();
        assert_eq!(after, texts(&out["names"]), "{row}");
        let index = input["idx"].as_u64().expect("idx") as usize;
        assert_eq!(plan.changed, names[index] != name, "{row}");
        checked += 1;
    }
    assert_eq!(checked, 6);
}

#[test]
fn recolor_replays_every_oracle_row() {
    let mut checked = 0;
    for row in rows("recolor") {
        let (input, out) = (&row["in"], &row["o"]);
        let count = input["n"].as_u64().expect("n") as usize;
        let before = build(&plain(count), 2);
        let selected: Vec<String> = ints(&input["sel"]).into_iter().map(uid).collect();
        let clicked = uid(input["li"].as_i64().expect("li"));
        let hex = input["hex"].as_str().expect("hex");
        let plan = plan_recolor(
            &before,
            &clicked,
            &selected,
            hex,
            &UnavailableLayerEditPolicy,
        )
        .expect("recolor");
        assert!(admitted(&plan.document), "{row}");
        let colors: Vec<String> = plan
            .document
            .layers
            .iter()
            .map(|l| l.color.value().cloned().expect("color"))
            .collect();
        assert_eq!(colors, texts(&out["colors"]), "{row}");
        assert_eq!(
            plan.touched.len() as u64,
            out["count"].as_u64().expect("count"),
            "{row}"
        );
        assert!(plan.selection.is_none(), "{row}");
        checked += 1;
    }
    assert_eq!(checked, 18);
}
