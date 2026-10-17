//! Literal pins of the PSD import plan (L10-F): values written out by hand from the baseline
//! `psd-import-bridge.js` at `fbe4b8a3`; the generated oracle rows are checked in `oracle`.
use super::*;

const SAME: CanvasSize = CanvasSize {
    width: 1920.0,
    height: 1080.0,
};

fn px(name: Option<&str>, width: u32, height: u32) -> PsdLayerInput {
    PsdLayerInput {
        name: name.map(str::to_owned),
        has_canvas: true,
        width,
        height,
        ..PsdLayerInput::default()
    }
}

fn group(name: &str) -> PsdLayerInput {
    PsdLayerInput {
        name: Some(name.to_owned()),
        ..PsdLayerInput::default()
    }
}

fn psd(width: Option<u32>, height: Option<u32>, children: Vec<PsdLayerInput>) -> PsdInput {
    PsdInput {
        width,
        height,
        children,
    }
}

fn plan(p: &PsdInput) -> PsdImportPlan {
    plan_psd_import(p, &SAME).unwrap()
}

#[test]
fn one_layer_is_centred_on_its_rect_with_default_opacity() {
    let mut l = px(Some("Fond"), 1920, 1080);
    l.left = Some(0.0);
    l.top = Some(0.0);
    let got = plan(&psd(Some(1920), Some(1080), vec![l]));
    assert_eq!(got.resize, None);
    assert!(got.skipped.is_empty());
    assert_eq!(got.rows.len(), 1);
    let row = &got.rows[0];
    assert_eq!(row.layer_name, "Fond");
    assert!(row.visible);
    assert_eq!(
        (
            row.rect.center_x,
            row.rect.center_y,
            row.rect.width,
            row.rect.height
        ),
        (960.0, 540.0, 1920, 1080)
    );
    assert_eq!(row.opacity, 1.0);
    assert_eq!(
        row.raster,
        RasterAssetRequest {
            key: "psd-layer:0".to_string(),
            source_index: 0,
            width: 1920,
            height: 1080,
            mime: "image/png",
        }
    );
}

#[test]
fn an_odd_size_gives_a_half_pixel_centre_and_a_negative_origin_is_kept() {
    let mut a = px(Some("Logo"), 101, 55);
    a.left = Some(33.0);
    a.top = Some(-7.0);
    a.opacity = Some(0.5);
    let mut b = px(Some("Off"), 200, 100);
    b.left = Some(-150.0);
    b.top = Some(-80.0);
    let got = plan(&psd(None, None, vec![a, b]));
    let centres: Vec<(f64, f64)> = got
        .rows
        .iter()
        .map(|r| (r.rect.center_x, r.rect.center_y))
        .collect();
    assert_eq!(centres, vec![(83.5, 20.5), (-50.0, -30.0)]);
    assert_eq!(got.rows[0].opacity, 0.5);
}

#[test]
fn undefined_null_and_nan_offsets_are_zero_and_an_explicit_zero_opacity_stays_zero() {
    let mut a = px(None, 10, 20);
    a.left = Some(f64::NAN);
    a.top = Some(-0.0);
    let mut b = px(None, 10, 20);
    b.opacity = Some(0.0);
    let got = plan(&psd(None, None, vec![a, b, px(None, 10, 20)]));
    let at: Vec<(f64, f64, f64)> = got
        .rows
        .iter()
        .map(|r| (r.rect.center_x, r.rect.center_y, r.opacity))
        .collect();
    assert_eq!(
        at,
        vec![(5.0, 10.0, 1.0), (5.0, 10.0, 0.0), (5.0, 10.0, 1.0)]
    );
}

#[test]
fn default_names_count_imported_layers_not_children() {
    let kids = vec![
        px(None, 1, 1),
        group("g1"),
        px(Some(""), 1, 1),
        PsdLayerInput {
            hidden: true,
            ..px(Some("h"), 1, 1)
        },
        px(None, 1, 1),
        px(Some("Keep"), 1, 1),
        px(None, 1, 1),
    ];
    let names: Vec<String> = plan(&psd(None, None, kids))
        .rows
        .into_iter()
        .map(|r| r.layer_name)
        .collect();
    assert_eq!(
        names,
        [
            "Calque PSD 1",
            "Calque PSD 2",
            "Calque PSD 3",
            "Keep",
            "Calque PSD 5"
        ]
    );
}

#[test]
fn a_blank_or_zero_name_is_kept_as_authored() {
    let kids = vec![
        px(Some(" "), 1, 1),
        px(Some("0"), 1, 1),
        px(Some("\u{65e5}\u{672c}\u{8a9e}"), 1, 1),
    ];
    let names: Vec<String> = plan(&psd(None, None, kids))
        .rows
        .into_iter()
        .map(|r| r.layer_name)
        .collect();
    assert_eq!(names, [" ", "0", "\u{65e5}\u{672c}\u{8a9e}"]);
}

#[test]
fn groups_and_hidden_layers_are_reported_and_never_imported() {
    let kids = vec![
        group("Group"),
        PsdLayerInput {
            hidden: true,
            ..px(Some("Hidden"), 4, 4)
        },
        px(Some("A"), 4, 4),
        PsdLayerInput {
            hidden: true,
            ..group("HiddenGroup")
        },
        px(Some("B"), 4, 4),
    ];
    let got = plan(&psd(None, None, kids));
    let imported: Vec<usize> = got.rows.iter().map(|r| r.source_index).collect();
    assert_eq!(imported, vec![2, 4]);
    assert_eq!(
        got.skipped,
        vec![
            SkippedLayer {
                source_index: 0,
                name: Some("Group".to_string()),
                reason: SkipReason::NoCanvas { hidden: false },
            },
            SkippedLayer {
                source_index: 1,
                name: Some("Hidden".to_string()),
                reason: SkipReason::Hidden,
            },
            SkippedLayer {
                source_index: 3,
                name: Some("HiddenGroup".to_string()),
                reason: SkipReason::NoCanvas { hidden: true },
            },
        ]
    );
    assert_eq!((got.imported_count(), got.child_count()), (2, 5));
    assert_eq!(got.rows[1].raster.key, "psd-layer:4");
}

#[test]
fn a_layer_with_a_zero_size_canvas_is_still_imported_like_the_baseline_counts_it() {
    let got = plan(&psd(
        None,
        None,
        vec![px(Some("Empty"), 0, 0), px(Some("Ok"), 10, 20)],
    ));
    assert_eq!(got.rows.len(), 2);
    assert_eq!(
        (got.rows[0].rect.center_x, got.rows[0].rect.width),
        (0.0, 0)
    );
}

#[test]
fn the_canvas_is_resized_only_for_a_truthy_different_psd_size() {
    let table: [(Option<u32>, Option<u32>, (f64, f64), Option<(u32, u32)>); 9] = [
        (Some(1920), Some(1080), (1920.0, 1080.0), None),
        (Some(800), Some(600), (1920.0, 1080.0), Some((800, 600))),
        (Some(1280), Some(1080), (1920.0, 1080.0), Some((1280, 1080))),
        (Some(1920), Some(720), (1920.0, 1080.0), Some((1920, 720))),
        (Some(0), Some(600), (1920.0, 1080.0), None),
        (Some(800), Some(0), (1920.0, 1080.0), None),
        (None, None, (1920.0, 1080.0), None),
        (Some(800), None, (1920.0, 1080.0), None),
        (Some(1920), Some(1080), (1920.5, 1080.0), Some((1920, 1080))),
    ];
    for (w, h, canvas, want) in table {
        let c = CanvasSize {
            width: canvas.0,
            height: canvas.1,
        };
        let got = plan_psd_import(&psd(w, h, vec![px(None, 1, 1)]), &c).unwrap();
        assert_eq!(
            got.resize.map(|r| (r.width, r.height)),
            want,
            "{w:?} x {h:?} on {canvas:?}"
        );
    }
}

#[test]
fn nothing_importable_is_the_baseline_error_and_never_a_partial_plan() {
    let message = "Aucun calque avec des pixels trouv\u{e9} (fichier vide, ou uniquement des groupes/calques masqu\u{e9}s)";
    assert_eq!(NO_PIXELS_MESSAGE, message);
    let kids = vec![
        group("g"),
        PsdLayerInput {
            hidden: true,
            ..px(None, 1, 1)
        },
    ];
    // a size that differs from the canvas: the baseline resized first, the plan publishes nothing
    let err = plan_psd_import(&psd(Some(800), Some(600), kids), &SAME).unwrap_err();
    assert_eq!(err.message(), message);
    match err {
        PsdPlanError::NoPixels {
            child_count,
            skipped,
        } => {
            assert_eq!(child_count, 2);
            assert_eq!(skipped.len(), 2);
        }
        other => panic!("{other:?}"),
    }
    let empty = plan_psd_import(&psd(Some(800), Some(600), vec![]), &SAME).unwrap_err();
    assert_eq!(
        empty,
        PsdPlanError::NoPixels {
            child_count: 0,
            skipped: vec![]
        }
    );
}

#[test]
fn a_non_finite_imported_value_is_refused_but_a_skipped_layers_value_is_ignored() {
    let mut bad = px(None, 1, 1);
    bad.left = Some(f64::INFINITY);
    let err =
        plan_psd_import(&psd(None, None, vec![px(None, 1, 1), bad.clone()]), &SAME).unwrap_err();
    assert_eq!(
        err,
        PsdPlanError::NonFinite {
            source_index: 1,
            field: "left"
        }
    );
    assert!(err.message().contains("PSD layer 1: left"));
    let hidden = PsdLayerInput {
        hidden: true,
        ..bad
    };
    assert!(plan_psd_import(&psd(None, None, vec![hidden, px(None, 1, 1)]), &SAME).is_ok());
}

#[test]
fn the_plan_is_deterministic_and_leaves_the_input_untouched() {
    let kids = vec![px(None, 7, 9), group("g"), px(Some("x"), 3, 3)];
    let input = psd(Some(640), Some(480), kids);
    let before = input.clone();
    assert_eq!(plan(&input), plan(&input));
    assert_eq!(input, before);
}

#[test]
fn the_decision_is_unavailable_until_the_raster_provider_exists() {
    let input = psd(
        Some(800),
        Some(600),
        vec![group("g"), px(None, 2, 2), px(None, 3, 3)],
    );
    match decide_psd_import(&input, &SAME, &PsdPorts::NONE_AVAILABLE).unwrap() {
        PsdImportDecision::Unavailable(u) => {
            assert_eq!(u.reason, PsdBlockReason::RasterAssetProviderUnavailable);
            assert_eq!(u.eligible_count, 2);
            assert_eq!(u.skipped.len(), 1);
        }
        other => panic!("a missing provider must not succeed: {other:?}"),
    }
    let ready = PsdPorts {
        raster_assets: ProviderState::Available,
    };
    assert_eq!(
        decide_psd_import(&input, &SAME, &ready).unwrap(),
        PsdImportDecision::Ready(plan(&input))
    );
    // plan errors do not depend on the provider
    let none = psd(None, None, vec![group("g")]);
    assert_eq!(
        decide_psd_import(&none, &SAME, &ready)
            .unwrap_err()
            .message(),
        NO_PIXELS_MESSAGE
    );
    assert_eq!(
        decide_psd_import(&none, &SAME, &PsdPorts::NONE_AVAILABLE)
            .unwrap_err()
            .message(),
        NO_PIXELS_MESSAGE
    );
}

#[test]
fn toast_texts_are_pinned_by_hand() {
    assert_eq!(
        success_toast(1, " imported"),
        "1 calque imported depuis le PSD"
    );
    assert_eq!(
        success_toast(3, " import\u{e9}"),
        "3 calques import\u{e9}s depuis le PSD"
    );
    assert_eq!(
        failure_toast("PSD import failed: ", None),
        "PSD import failed: erreur inconnue"
    );
    assert_eq!(
        failure_toast("PSD import failed: ", Some("")),
        "PSD import failed: erreur inconnue"
    );
    assert_eq!(
        failure_toast("PSD import failed: ", Some("boom")),
        "PSD import failed: boom"
    );
}
