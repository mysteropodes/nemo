//! L08-K negative and mutation controls. An independent model restates the baseline rules over the oracle's JSON
//! scenarios; the unmutated model must equal the oracle (the verbatim baseline statements) and the production join,
//! and each named mutant must flip EXACTLY the rows the generator recorded for it (computed there by the JS twin of
//! this model over the same rows, not copied from production). A corrupted oracle literal must be detected.
use super::tests::{inputs_of, lines_of, norm, plan_of, rows, wire_of};
use serde_json::{json, Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutant {
    Unmutated,
    LastUidMatch,
    SelfUidMatches,
    ExtrasKeepDangling,
    ExtrasWithoutFirstMatte,
    FolderKeepsBlend,
    HiddenKeepsEffects,
    BorrowTransitive,
    BorrowAfterOwn,
    NoneMatteKept,
    WidgetBeforeFolder,
}

const MUTANTS: [(&str, Mutant); 10] = [
    ("last-uid-match", Mutant::LastUidMatch),
    ("self-uid-matches", Mutant::SelfUidMatches),
    ("extras-keep-dangling", Mutant::ExtrasKeepDangling),
    (
        "extras-without-first-matte",
        Mutant::ExtrasWithoutFirstMatte,
    ),
    ("folder-keeps-blend", Mutant::FolderKeepsBlend),
    ("hidden-keeps-effects", Mutant::HiddenKeepsEffects),
    ("borrow-transitive", Mutant::BorrowTransitive),
    ("borrow-after-own", Mutant::BorrowAfterOwn),
    ("none-matte-kept", Mutant::NoneMatteKept),
    ("widget-before-folder", Mutant::WidgetBeforeFolder),
];

fn text<'a>(layer: &'a Value, key: &str) -> Option<&'a str> {
    layer.get(key).and_then(Value::as_str)
}

fn flag(layer: &Value, key: &str) -> bool {
    layer.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn find(layers: &[Value], uid: &str, own: usize, m: Mutant) -> Option<usize> {
    let mut hits = (0..layers.len()).filter(|&j| {
        (j != own || m == Mutant::SelfUidMatches) && text(&layers[j], "layerUid") == Some(uid)
    });
    if m == Mutant::LastUidMatch {
        hits.last()
    } else {
        hits.next()
    }
}

fn role(layer: &Value, m: Mutant) -> &'static str {
    if layer.get("visible").and_then(Value::as_bool) == Some(false) {
        "empty"
    } else if m == Mutant::WidgetBeforeFolder && flag(layer, "isWidgetLayer") {
        "empty"
    } else if flag(layer, "isFolderLayer") {
        "folder"
    } else if flag(layer, "isWidgetLayer") {
        "empty"
    } else if flag(layer, "isEffectLayer") {
        "effect"
    } else {
        "plain"
    }
}

fn project_effect(entry: &Value) -> Value {
    let mut out = Map::new();
    out.insert("effectType".into(), entry["type"].clone());
    out.insert(
        "enabled".into(),
        json!(entry["enabled"].as_bool().unwrap_or(false)),
    );
    for k in 1..=8 {
        if let Some(v) = entry.get(format!("p{k}")).filter(|v| !v.is_null()) {
            out.insert(format!("p{k}"), v.clone());
        }
    }
    Value::Object(out)
}

fn own_effects(layer: &Value) -> Vec<Value> {
    layer
        .get("effects")
        .and_then(Value::as_array)
        .map(|entries| entries.iter().map(project_effect).collect())
        .unwrap_or_default()
}

fn effective(layers: &[Value], index: usize, role: &str, m: Mutant) -> Vec<Value> {
    if role == "empty" && m != Mutant::HiddenKeepsEffects {
        return Vec::new();
    }
    let mut inherited = Vec::new();
    let from = text(&layers[index], "effectsFrom").filter(|uid| !uid.is_empty());
    if let Some(source) = from.and_then(|uid| find(layers, uid, index, m)) {
        inherited = own_effects(&layers[source]);
        if m == Mutant::BorrowTransitive {
            let second = text(&layers[source], "effectsFrom").filter(|uid| !uid.is_empty());
            if let Some(deep) = second.and_then(|uid| find(layers, uid, source, m)) {
                let mut chain = own_effects(&layers[deep]);
                chain.extend(inherited);
                inherited = chain;
            }
        }
    }
    let own = own_effects(&layers[index]);
    if m == Mutant::BorrowAfterOwn {
        own.into_iter().chain(inherited).collect()
    } else {
        inherited.into_iter().chain(own).collect()
    }
}

fn model(layers: &[Value], m: Mutant) -> Vec<Value> {
    let n = layers.len();
    let mut out: Vec<Map<String, Value>> = vec![Map::new(); n];
    for i in 0..n {
        let layer = &layers[i];
        let r = role(layer, m);
        let reads =
            r == "plain" || (m == Mutant::FolderKeepsBlend && (r == "folder" || r == "effect"));
        if reads {
            if let Some(b) = text(layer, "blendMode").filter(|b| !b.is_empty() && *b != "normal") {
                out[i].insert("blendMode".into(), json!(b));
            }
            let kept = |mm: &&str| !mm.is_empty() && (*mm != "none" || m == Mutant::NoneMatteKept);
            if let Some(mm) = text(layer, "matteMode").filter(kept) {
                out[i].insert("matteMode".into(), json!(mm));
            }
        }
        if r == "effect" {
            out[i].insert("isEffectLayer".into(), json!(true));
        }
        if r == "folder" {
            out[i].insert("isFolderLayer".into(), json!(true));
        }
        let eff = effective(layers, i, r, m);
        if !eff.is_empty() {
            out[i].insert("effects".into(), Value::Array(eff));
        }
    }
    for i in 0..n {
        if !out[i].contains_key("matteMode") {
            continue;
        }
        let source = text(&layers[i], "matteSourceLayerUid").filter(|uid| !uid.is_empty());
        if let Some(j) = source.and_then(|uid| find(layers, uid, i, m)) {
            out[i].insert("matteSourceIndex".into(), json!(j));
        }
    }
    for i in 0..n {
        if !out[i].contains_key("matteMode") && m != Mutant::ExtrasWithoutFirstMatte {
            continue;
        }
        let mut more = Vec::new();
        for extra in layers[i]
            .get("mattesMore")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let uid = text(extra, "uid").unwrap_or("");
            let mode = text(extra, "mode").unwrap_or("");
            if uid.is_empty() || mode.is_empty() || mode == "none" {
                continue;
            }
            match find(layers, uid, i, m) {
                Some(j) => more.push(json!({"mode": mode, "sourceIndex": j})),
                None if m == Mutant::ExtrasKeepDangling => more.push(json!({"mode": mode})),
                None => {}
            }
        }
        if !more.is_empty() {
            out[i].insert("mattesMore".into(), Value::Array(more));
        }
    }
    for i in 0..n {
        if !out[i].contains_key("isFolderLayer") {
            continue;
        }
        let uid = text(&layers[i], "layerUid").unwrap_or("");
        let kids: Vec<usize> = (0..n)
            .filter(|&j| {
                j != i && !uid.is_empty() && text(&layers[j], "parentLayerUid") == Some(uid)
            })
            .collect();
        out[i].insert("folderChildIndices".into(), json!(kids));
    }
    out.into_iter().map(Value::Object).collect()
}

#[test]
fn the_unmutated_model_equals_the_oracle_and_production_on_every_row() {
    for row in rows() {
        let want = norm(Value::Array(row.wire.clone()));
        assert_eq!(
            norm(Value::Array(model(&row.layers, Mutant::Unmutated))),
            want,
            "model {}",
            row.id
        );
        let made = wire_of(&plan_of(&inputs_of(&row.layers), 1.0));
        assert_eq!(norm(Value::Array(made)), want, "production {}", row.id);
    }
}

#[test]
fn each_mutant_flips_exactly_the_rows_the_generator_recorded() {
    let recorded = lines_of("M ");
    assert_eq!(recorded.len(), MUTANTS.len());
    for (name, mutant) in MUTANTS {
        let line = recorded
            .iter()
            .find(|line| line["name"] == name)
            .unwrap_or_else(|| panic!("no recorded mutant {name}"));
        let want: Vec<String> = line["flips"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_str().unwrap().to_owned())
            .collect();
        assert!(!want.is_empty(), "{name} must flip something");
        let flipped: Vec<String> = rows()
            .into_iter()
            .filter(|row| {
                norm(Value::Array(model(&row.layers, mutant)))
                    != norm(Value::Array(row.wire.clone()))
            })
            .map(|row| row.id)
            .collect();
        assert_eq!(flipped, want, "{name}");
    }
}

#[test]
fn a_corrupted_oracle_literal_is_detected_by_the_production_comparison() {
    let all = rows();
    let row = all.iter().find(|r| r.id == "full-stack").unwrap();
    let mut corrupted = row.wire.clone();
    corrupted[0]["matteSourceIndex"] = json!(3);
    let made = norm(Value::Array(wire_of(&plan_of(
        &inputs_of(&row.layers),
        1.0,
    ))));
    assert_eq!(made, norm(Value::Array(row.wire.clone())));
    assert_ne!(made, norm(Value::Array(corrupted)));
    let mut dropped = row.wire.clone();
    dropped[4]["effects"] = json!([]);
    assert_ne!(made, norm(Value::Array(dropped)));
}

#[test]
fn the_oracle_spans_every_slot_role_and_resolution_rule() {
    let all = rows();
    let any = |pred: &dyn Fn(&Value) -> bool| all.iter().any(|r| r.wire.iter().any(pred));
    assert!(any(&|w| w.get("isEffectLayer").is_some()));
    assert!(any(&|w| w.get("isFolderLayer").is_some()));
    assert!(any(&|w| w["folderChildIndices"]
        .as_array()
        .is_some_and(|k| !k.is_empty())));
    assert!(any(&|w| w.get("matteSourceIndex").is_some()));
    assert!(any(&|w| w.get("mattesMore").is_some()));
    assert!(any(&|w| w.get("blendMode").is_some()));
    assert!(any(&|w| w["effects"]
        .as_array()
        .is_some_and(|e| e.len() >= 2)));
    assert!(all
        .iter()
        .any(|r| r.layers.iter().any(|l| l["visible"] == json!(false))));
    assert!(all
        .iter()
        .any(|r| r.layers.iter().any(|l| l["isWidgetLayer"] == json!(true))));
    assert!(all.iter().any(|r| r
        .layers
        .iter()
        .any(|l| l["effects"][1]["p2"].is_null() && l["effects"][1].get("p2").is_some())));
    assert!(all.iter().any(|r| r.wire.is_empty() || r.layers.len() == 1));
}

fn stack_of(variants: [usize; 3]) -> Vec<Value> {
    variants
        .iter()
        .enumerate()
        .map(|(pos, variant)| {
            let mut layer = json!({"layerUid": format!("U{pos}")});
            match variant % 5 {
                1 => layer["visible"] = json!(false),
                2 => layer["isFolderLayer"] = json!(true),
                3 => layer["isEffectLayer"] = json!(true),
                4 => layer["isWidgetLayer"] = json!(true),
                _ => {}
            }
            if (variant / 5) % 2 == 1 {
                layer["blendMode"] = json!("multiply");
                layer["matteMode"] = json!("alpha");
                layer["matteSourceLayerUid"] = json!(format!("U{}", (pos + 1) % 3));
            }
            if (variant / 10) % 2 == 1 {
                layer["effectsFrom"] = json!(format!("U{}", (pos + 2) % 3));
                layer["effects"] = json!([{"type": "blur", "enabled": true, "p1": pos + 1}]);
                layer["parentLayerUid"] = json!(format!("U{}", (pos + 1) % 3));
            }
            layer
        })
        .collect()
}

#[test]
fn the_model_and_production_agree_on_every_three_layer_stack_of_the_small_grid() {
    let mut checked = 0;
    for a in 0..20 {
        for b in 0..20 {
            for c in 0..20 {
                let layers = stack_of([a, b, c]);
                let want = norm(Value::Array(model(&layers, Mutant::Unmutated)));
                let made = norm(Value::Array(wire_of(&plan_of(&inputs_of(&layers), 1.0))));
                assert_eq!(made, want, "{a} {b} {c}");
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 8000);
}
