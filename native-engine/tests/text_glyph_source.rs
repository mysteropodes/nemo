//! Oracle tests of the pure glyph-source arithmetic against REAL opentype.js
//! data (PURE and XF rows of the oracle; nothing here is computed by the code
//! under test).
use super::oracle::{design, num, outline, points, rows, text};
use super::{
    assemble_composite, design_commands, scale_commands, transform_points, ComponentTransform,
    DesignPoint,
};

fn transform_of(component: &serde_json::Value) -> ComponentTransform {
    ComponentTransform {
        x_scale: num(&component["xs"]),
        scale01: num(&component["s01"]),
        scale10: num(&component["s10"]),
        y_scale: num(&component["ys"]),
        dx: num(&component["dx"]),
        dy: num(&component["dy"]),
    }
}

#[test]
fn oracle_points_become_the_opentype_path_commands() {
    let pure = rows("PURE");
    let mut with_ink = 0;
    for row in &pure {
        let commands = design_commands(&points(&row["pts"])).expect("points are terminated");
        assert_eq!(
            commands,
            design(&row["cmds"]),
            "{} unit {}",
            text(&row["font"]),
            row["unit"]
        );
        if !commands.is_empty() {
            with_ink += 1;
        }
    }
    assert_eq!(pure.len(), 22);
    // Everything but the space glyph has ink (the notdef box included).
    assert_eq!(with_ink, 21);
}

#[test]
fn oracle_scaling_reproduces_get_path_bit_for_bit() {
    let mut checked = 0;
    for row in rows("PURE") {
        let commands = design(&row["cmds"]);
        for scaled in row["scaled"].as_array().expect("scaled rows") {
            let got = scale_commands(
                &commands,
                num(&row["upm"]),
                num(&scaled["x"]),
                num(&scaled["y"]),
                num(&scaled["size"]),
            )
            .expect("finite scale");
            let expected = outline(&scaled["cmds"]);
            assert_eq!(
                got.len(),
                expected.len(),
                "{} unit {}",
                text(&row["font"]),
                row["unit"]
            );
            for (index, (g, e)) in got.iter().zip(&expected).enumerate() {
                assert_eq!(
                    g,
                    e,
                    "{} unit {} command {index}",
                    text(&row["font"]),
                    row["unit"]
                );
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 44);
}

#[test]
fn oracle_composite_assembly_equals_the_opentype_points() {
    let mut composites = 0;
    let mut multi_contour = 0;
    for row in rows("PURE") {
        if row["composite"] != true {
            continue;
        }
        composites += 1;
        let parts: Vec<(Vec<DesignPoint>, ComponentTransform)> = row["comps"]
            .as_array()
            .expect("components")
            .iter()
            .map(|component| (points(&component["pts"]), transform_of(component)))
            .collect();
        assert!(!parts.is_empty());
        if parts
            .iter()
            .any(|(part, _)| part.iter().filter(|p| p.last_of_contour).count() > 1)
        {
            multi_contour += 1;
        }
        assert_eq!(
            assemble_composite(&parts),
            points(&row["pts"]),
            "{} unit {}",
            text(&row["font"]),
            row["unit"]
        );
    }
    // Roboto e-acute and A-ring, DejaVu Sans U+01D5 U+0149 a-diaeresis (the first two
    // nest composites), Inter e-acute.
    assert_eq!(composites, 6);
    assert_eq!(multi_contour, 3);
}

#[test]
fn oracle_component_transform_equals_opentype_dt() {
    let xf = rows("XF");
    assert_eq!(xf.len(), 40);
    let mut skewed = 0;
    for row in &xf {
        let t = transform_of(row);
        if t.scale01 != 0.0 || t.scale10 != 0.0 {
            skewed += 1;
        }
        assert_eq!(
            transform_points(&points(&row["input"]), &t),
            points(&row["output"])
        );
    }
    assert_eq!(skewed, 31, "rows exercising the off-diagonal members");
}

#[test]
fn oracle_has_not_been_hollowed_out() {
    let fonts = rows("FONT");
    assert_eq!(fonts.len(), 8);
    assert_eq!(rows("GLYPH").len(), 123);
    assert_eq!(rows("CASE").len(), 17);
    let composite_glyphs: u64 = fonts
        .iter()
        .map(|font| font["composite"].as_u64().expect("count"))
        .sum();
    assert_eq!(
        composite_glyphs,
        183 + 183 + 2599 + 2582 + 1399 + 1508 + 152 + 1882
    );
    // The pinned restore of the baseline: the Roboto upm and glyph count.
    let roboto = &fonts[0];
    assert_eq!(text(&roboto["font"]), "Roboto-Regular");
    assert_eq!(num(&roboto["upm"]), 2048.0);
    assert_eq!(roboto["glyphs"], 363);
}
