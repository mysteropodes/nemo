//! Negative and mutation controls of the Image placement holder (L06-I): single-flaw copies of the element rule each
//! fail their named cases, the oracle really separates the strict uid holder from the baseline's name fallback, the
//! lifted arm is live, and the production file stays free of panics, clocks, I/O and `super::` imports.

use super::*;
use crate::object_document::media_timing::object_media_link_edit::{
    check_inventory, PlacementRefusal,
};
use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Flaw {
    None,
    CountsReferenceScope,
    IgnoresLayer,
    IgnoresContext,
    IgnoresItems,
    CountsTextBlocks,
    IgnoresPaths,
}

/// A test-local copy of the element rule with at most one deliberate flaw.
fn model(flaw: Flaw, doc: &ObjectDocument, context: &str, layer: &str, stroke: &str) -> bool {
    let same = |target: &ObjectTarget| {
        (flaw == Flaw::CountsReferenceScope
            || matches!(target.scope_kind(), FrameScopeKind::Authored))
            && (flaw == Flaw::IgnoresContext || target.context_id() == context)
            && (flaw == Flaw::IgnoresLayer || target.layer_uid() == layer)
            && target.stroke_id() == stroke
    };
    let in_paths = flaw != Flaw::IgnoresPaths && doc.objects().iter().any(|r| same(r.target()));
    let in_items = flaw != Flaw::IgnoresItems
        && doc.item_records().is_some_and(|items| {
            items.raster_entries().iter().any(|e| same(&e.target))
                || (flaw == Flaw::CountsTextBlocks
                    && items.text_entries().iter().any(|e| same(&e.target)))
        });
    in_paths || in_items
}

const CASES: [(&str, &str, &str, &str, bool); 9] = [
    ("authored path", "scene-root", "L", "ink-1", true),
    ("raster item on L", "scene-root", "L", "r0", true),
    ("raster item on M", "scene-root", "M", "r1", true),
    ("reference scope", "scene-root", "M", "ref-1", false),
    ("text block", "scene-root", "M", "t0", false),
    ("path on another layer", "scene-root", "M", "ink-1", false),
    ("raster on another layer", "scene-root", "L", "r1", false),
    ("missing stroke", "scene-root", "L", "ghost", false),
    ("another context", "shot-9", "L", "ink-1", false),
];

#[test]
fn the_real_rule_and_the_unflawed_copy_agree_on_every_named_case() {
    let doc = document(&drawing());
    for (name, context, layer, stroke, expected) in CASES {
        assert_eq!(
            image_element_exists(&doc, context, layer, stroke),
            expected,
            "{name}"
        );
        assert_eq!(
            model(Flaw::None, &doc, context, layer, stroke),
            expected,
            "{name}"
        );
    }
}

#[test]
fn each_single_flaw_copy_fails_exactly_its_named_cases() {
    let doc = document(&drawing());
    let failing = |flaw: Flaw| -> Vec<&'static str> {
        CASES
            .iter()
            .filter(|(_, context, layer, stroke, expected)| {
                model(flaw, &doc, context, layer, stroke) != *expected
            })
            .map(|(name, ..)| *name)
            .collect()
    };
    assert_eq!(failing(Flaw::None), Vec::<&str>::new());
    assert_eq!(failing(Flaw::CountsReferenceScope), ["reference scope"]);
    assert_eq!(
        failing(Flaw::IgnoresLayer),
        ["path on another layer", "raster on another layer"]
    );
    assert_eq!(failing(Flaw::IgnoresContext), ["another context"]);
    assert_eq!(
        failing(Flaw::IgnoresItems),
        ["raster item on L", "raster item on M"]
    );
    assert_eq!(failing(Flaw::CountsTextBlocks), ["text block"]);
    assert_eq!(failing(Flaw::IgnoresPaths), ["authored path"]);
}

/// The baseline binds a library entry to its layer by uid first and by name second; `fallback` switches the second step.
fn resolve(entry: &Value, layers: &Value, fallback: bool) -> Option<String> {
    let layers = layers.as_array().unwrap();
    if let Some(uid) = entry["layerUid"].as_str() {
        if let Some(found) = layers.iter().find(|l| l["layerUid"] == uid) {
            return Some(found["layerUid"].as_str().unwrap().to_owned());
        }
    }
    if !fallback {
        return None;
    }
    let name = entry["layerName"].as_str()?;
    layers
        .iter()
        .find(|l| l["name"] == name)
        .map(|found| found["layerUid"].as_str().unwrap().to_owned())
}

#[test]
fn the_oracle_separates_the_strict_uid_holder_from_the_baseline_name_fallback() {
    let (mut fallback_differs, mut strict_differs) = (0, 0);
    for row in rows("R") {
        let entry: Value = serde_json::from_str(row[2]).unwrap();
        let layers: Value = serde_json::from_str(row[3]).unwrap();
        let baseline = if row[4] == "-" {
            None
        } else {
            Some(row[4].to_owned())
        };
        // The uid-then-name copy IS the baseline on every one of the 36 rows ...
        assert_eq!(resolve(&entry, &layers, true), baseline, "{}", row[1]);
        // ... and the strict uid-only holder is not: it differs exactly where the baseline needed the name.
        if resolve(&entry, &layers, false) != baseline {
            strict_differs += 1;
            assert_eq!(row[5], "name", "{}", row[1]);
        }
        if resolve(&entry, &layers, true) != resolve(&entry, &layers, false) {
            fallback_differs += 1;
        }
    }
    assert_eq!((strict_differs, fallback_differs), (6, 6));
}

#[test]
fn no_image_placement_is_refused_as_unowned_any_more() {
    let doc = document(&drawing());
    let mut targets = vec![
        layer_target("L"),
        layer_target("nope"),
        element_target("L", "ink-1"),
        element_target("L", "r0"),
        element_target("L", "ghost"),
        element_target("M", "t0"),
        element_target("M", "ref-1"),
        AssetTarget::Drawing {
            context_id: "shot-9".to_owned(),
            layer_uid: "L".to_owned(),
            stroke_id: None,
        },
    ];
    targets.push(element_target("nope", "r0"));
    let mut seen = Vec::new();
    for target in targets {
        let verdict = check_inventory(
            &doc,
            &AssetInventory::new(
                vec![image_record("img")],
                vec![placement("p", "img", target, 0)],
                1,
                1,
            )
            .unwrap(),
        );
        assert_ne!(verdict, Err(PlacementRefusal::ImageOwner));
        seen.push(verdict);
    }
    // Both outcomes occur, so the loop is not vacuous: holders that exist pass, dangling ones are typed.
    assert!(seen.contains(&Ok(())));
    assert!(seen.contains(&Err(PlacementRefusal::Context)));
    assert!(seen.contains(&Err(PlacementRefusal::Layer)));
    assert!(seen.contains(&Err(PlacementRefusal::Element)));
}

#[test]
fn the_production_file_has_no_panics_clock_io_or_super_imports() {
    let source = include_str!("../src/object_raster_asset_placement.rs");
    let production = source.split("#[cfg(all(test").next().unwrap();
    for forbidden in [
        "super::",
        ".unwrap(",
        ".expect(",
        "panic!",
        "unreachable!",
        "todo!",
        "unimplemented!",
        "std::fs",
        "std::net",
        "std::process",
        "SystemTime",
        "Instant",
        "println!",
    ] {
        assert!(
            !production.contains(forbidden),
            "production uses {forbidden}"
        );
    }
    // The only crate imports are document-layer ones, so the module adds no layer edge.
    for line in production.lines().filter(|l| l.contains("crate::")) {
        assert!(line.contains("crate::object_document::"), "{line}");
    }
}
