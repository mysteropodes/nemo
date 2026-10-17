//! L08-J negative and mutation controls. Each model re-states one baseline rule independently of the
//! production function; the unmutated model must agree with production and every oracle row, and every
//! mutant must flip exactly the rows named here (sets derived by `tools/verify_controls_claims.js` over the
//! same oracle, not guessed). A control that flips nothing is itself pinned by a hand literal.
use super::oracle_rows::{layer_from, pairs, rows};
use super::*;
use serde_json::{json, Value};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Freeze {
    Model,
    KeepNone,
    Above,
    TrustEmpty,
    WrapLast,
}
type Slot = (String, Option<String>, Option<String>);

fn slots_of(layers: &Value) -> Vec<Slot> {
    layers
        .as_array()
        .unwrap()
        .iter()
        .map(|layer| {
            (
                layer["layerUid"].as_str().unwrap().to_string(),
                layer["matteMode"].as_str().map(str::to_string),
                layer["matteSourceLayerUid"].as_str().map(str::to_string),
            )
        })
        .collect()
}

fn model_freeze(slots: &[Slot], mutant: Freeze) -> Vec<Option<String>> {
    let n = slots.len();
    let mut out = vec![None; n];
    for (i, (_, mode, source)) in slots.iter().enumerate() {
        let on = mode
            .as_deref()
            .is_some_and(|m| !m.is_empty() && (m != "none" || mutant == Freeze::KeepNone));
        let missing = source
            .as_deref()
            .map_or(true, |s| s.is_empty() && mutant != Freeze::TrustEmpty);
        let neighbor = match mutant {
            Freeze::Above => i.checked_sub(1),
            Freeze::WrapLast => Some((i + 1) % n),
            _ => (i + 1 < n).then_some(i + 1),
        };
        out[i] = match neighbor {
            Some(k) if on && missing => Some(slots[k].0.clone()),
            _ => source.clone(),
        };
    }
    out
}

fn production_freeze(slots: &[Slot]) -> Vec<Option<String>> {
    let mut layers: Vec<ObjectLayer> = slots
        .iter()
        .map(|(uid, mode, source)| {
            layer_from(&json!({"layerUid": uid, "matteMode": mode, "matteSourceLayerUid": source}))
        })
        .collect();
    freeze_legacy_matte_sources(&mut layers);
    layers
        .iter()
        .map(|layer| layer.matte_source_uid().map(str::to_string))
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Blend {
    Model,
    NeverClear,
    EmptyIsNormal,
    FoldCase,
}

fn model_blend(mode: &str, mutant: Blend) -> Option<String> {
    let clears = match mutant {
        Blend::Model => mode == "normal",
        Blend::NeverClear => false,
        Blend::EmptyIsNormal => mode == "normal" || mode.is_empty(),
        Blend::FoldCase => mode.to_lowercase() == "normal",
    };
    (!clears).then(|| mode.to_string())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Remap {
    Model,
    KeepUnmappedSource,
    KeepUnmappedExtras,
    ReverseExtras,
}
type Remapped = (Option<String>, Vec<(String, String)>);

fn model_remap(input: &Value, map: &serde_json::Map<String, Value>, mutant: Remap) -> Remapped {
    let lookup = |uid: &str| map.get(uid).and_then(Value::as_str).map(str::to_string);
    let source = input["matteSourceLayerUid"].as_str().and_then(|old| {
        lookup(old).or_else(|| (mutant == Remap::KeepUnmappedSource).then(|| old.to_string()))
    });
    let mut extras: Vec<(String, String)> = pairs(&input["mattesMore"])
        .into_iter()
        .filter_map(|(uid, mode)| {
            lookup(&uid)
                .or_else(|| (mutant == Remap::KeepUnmappedExtras).then(|| uid.clone()))
                .map(|mapped| (mapped, mode))
        })
        .collect();
    if mutant == Remap::ReverseExtras {
        extras.reverse();
    }
    (source, extras)
}

fn production_remap(input: &Value, map: &serde_json::Map<String, Value>) -> Remapped {
    let mut layer = layer_from(&json!({
        "layerUid": "self", "matteMode": "alpha",
        "matteSourceLayerUid": input["matteSourceLayerUid"], "mattesMore": input["mattesMore"]}));
    remap_compositing_refs(&mut layer, &|uid: &str| {
        map.get(uid).and_then(Value::as_str).map(str::to_string)
    });
    (
        layer.matte_source_uid().map(str::to_string),
        layer
            .mattes_more()
            .iter()
            .map(|extra| (extra.uid.clone(), extra.mode.clone()))
            .collect(),
    )
}

fn row_remapped(row: &Value) -> Remapped {
    (
        row["matteSourceLayerUid"].as_str().map(str::to_string),
        pairs(&row["mattesMore"]),
    )
}

fn freeze_flips(mutant: Freeze) -> Vec<String> {
    rows("freeze")
        .iter()
        .filter(|row| {
            let want: Vec<Option<String>> = row["sources"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().map(str::to_string))
                .collect();
            model_freeze(&slots_of(&row["layers"]), mutant) != want
        })
        .map(|row| row["id"].as_str().unwrap().to_string())
        .collect()
}

fn blend_flips(mutant: Blend) -> Vec<String> {
    rows("blend-write")
        .iter()
        .filter(|row| {
            model_blend(row["mode"].as_str().unwrap(), mutant)
                != row["stored"].as_str().map(str::to_string)
        })
        .map(|row| row["mode"].as_str().unwrap().to_string())
        .collect()
}

fn remap_flips(mutant: Remap) -> Vec<String> {
    rows("remap")
        .iter()
        .filter(|row| {
            let map = row["uidMap"].as_object().unwrap();
            model_remap(&row["input"], map, mutant) != row_remapped(row)
        })
        .map(|row| row["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn unmutated_models_agree_with_production_and_every_oracle_row() {
    for row in rows("freeze") {
        let slots = slots_of(&row["layers"]);
        let want: Vec<Option<String>> = row["sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect();
        assert_eq!(model_freeze(&slots, Freeze::Model), want, "{}", row["id"]);
        assert_eq!(production_freeze(&slots), want, "{}", row["id"]);
    }
    for row in rows("blend-write") {
        let mode = row["mode"].as_str().unwrap();
        let want = row["stored"].as_str().map(str::to_string);
        assert_eq!(model_blend(mode, Blend::Model), want, "{mode:?}");
        assert_eq!(blend_write(mode).map(str::to_string), want, "{mode:?}");
    }
    for row in rows("remap") {
        let map = row["uidMap"].as_object().unwrap();
        let want = row_remapped(&row);
        assert_eq!(
            model_remap(&row["input"], map, Remap::Model),
            want,
            "{}",
            row["id"]
        );
        assert_eq!(production_remap(&row["input"], map), want, "{}", row["id"]);
    }
}

#[test]
fn each_freeze_mutant_flips_exactly_its_named_rows() {
    assert!(freeze_flips(Freeze::Model).is_empty());
    assert_eq!(freeze_flips(Freeze::KeepNone), ["none-is-skipped"]);
    assert_eq!(
        freeze_flips(Freeze::Above),
        [
            "above",
            "last-layer-stays-unset",
            "empty-string-source-is-frozen",
            "chain-each-takes-next",
            "unknown-mode-string-is-frozen"
        ]
    );
    assert_eq!(
        freeze_flips(Freeze::TrustEmpty),
        ["empty-string-source-is-frozen"]
    );
    assert_eq!(freeze_flips(Freeze::WrapLast), ["last-layer-stays-unset"]);
}

#[test]
fn each_blend_and_remap_mutant_flips_exactly_its_named_rows() {
    assert!(blend_flips(Blend::Model).is_empty());
    assert_eq!(blend_flips(Blend::NeverClear), ["normal"]);
    assert_eq!(blend_flips(Blend::EmptyIsNormal), [""]);
    assert!(remap_flips(Remap::Model).is_empty());
    assert_eq!(remap_flips(Remap::KeepUnmappedSource), ["source-unmapped"]);
    assert_eq!(
        remap_flips(Remap::KeepUnmappedExtras),
        ["extras-partly-mapped", "extras-none-mapped"]
    );
    assert_eq!(remap_flips(Remap::ReverseExtras), ["all-mapped"]);
}

#[test]
fn the_case_fold_blend_mutant_is_invisible_to_the_rows_so_a_hand_literal_pins_it() {
    assert!(blend_flips(Blend::FoldCase).is_empty());
    assert_eq!(model_blend("Normal", Blend::FoldCase), None);
    assert_eq!(blend_write("Normal"), Some("Normal"));
    assert_eq!(blend_write("NORMAL"), Some("NORMAL"));
}

#[test]
fn a_corrupted_oracle_literal_is_detected() {
    let row = rows("freeze").remove(0);
    assert_eq!(row["id"], "above");
    let slots = slots_of(&row["layers"]);
    let produced = production_freeze(&slots);
    assert_eq!(produced, [Some("b".to_string()), None, None]);
    let corrupted: Vec<Option<String>> = vec![None, None, None];
    assert_ne!(
        produced, corrupted,
        "a wrong literal must not agree with production"
    );
    let mut remap = rows("remap").remove(1);
    assert_eq!(remap["id"], "source-unmapped");
    remap["matteSourceLayerUid"] = json!("x1");
    let map = remap["uidMap"].as_object().unwrap();
    assert_ne!(production_remap(&remap["input"], map), row_remapped(&remap));
}

#[test]
fn models_agree_with_production_on_every_three_layer_stack() {
    let modes: [Option<&str>; 4] = [None, Some(""), Some("none"), Some("alpha")];
    let sources: [Option<&str>; 3] = [None, Some(""), Some("zz")];
    let mut combos: Vec<(Option<&str>, Option<&str>)> = Vec::new();
    for mode in modes {
        for source in sources {
            combos.push((mode, source));
        }
    }
    let mut checked = 0;
    for a in &combos {
        for b in &combos {
            for c in &combos {
                let slots: Vec<Slot> = [a, b, c]
                    .iter()
                    .enumerate()
                    .map(|(i, (mode, source))| {
                        (
                            format!("u{i}"),
                            mode.map(str::to_string),
                            source.map(str::to_string),
                        )
                    })
                    .collect();
                assert_eq!(
                    model_freeze(&slots, Freeze::Model),
                    production_freeze(&slots)
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 12 * 12 * 12);
}

#[test]
fn the_oracle_spans_the_rules_the_models_restate() {
    let ids = |kind: &str| -> Vec<String> {
        rows(kind)
            .iter()
            .map(|r| r["id"].as_str().unwrap_or("").to_string())
            .collect()
    };
    for id in [
        "none-is-skipped",
        "explicit-source-untouched",
        "empty-string-source-is-frozen",
        "last-layer-stays-unset",
    ] {
        assert!(ids("freeze").contains(&id.to_string()), "{id}");
    }
    for id in [
        "all-mapped",
        "source-unmapped",
        "extras-partly-mapped",
        "extras-none-mapped",
    ] {
        assert!(ids("remap").contains(&id.to_string()), "{id}");
    }
    let modes: Vec<String> = rows("blend-write")
        .iter()
        .map(|r| r["mode"].as_str().unwrap().to_string())
        .collect();
    for mode in ["normal", "multiply", "bogus", ""] {
        assert!(modes.contains(&mode.to_string()), "{mode:?}");
    }
}
