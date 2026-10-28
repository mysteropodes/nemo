//! Shared producer against the frozen original-editor pressure cases, independently of command/record adaptation.
use super::*;
use serde_json::Value;

#[test]
fn pressure_ribbons_match_all_frozen_subselect_before_and_after_outlines() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/subselect-baseline/pressure.json")).unwrap();
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 180);
    let mut failures = vec![];
    for (index, case) in oracle["cases"].as_array().unwrap().iter().enumerate() {
        for phase in ["before", "expected"] {
            let source = &case[phase];
            let centers = source["center"].as_array().unwrap();
            let segments: Vec<SegOut> = centers
                .iter()
                .map(|s| serde_json::from_value(s.clone()).unwrap())
                .collect();
            let widths: Vec<f64> = centers
                .iter()
                .map(|s| s["width"].as_f64().unwrap())
                .collect();
            let result = if let Some(profile) = case["profile"].as_array() {
                let profile: Vec<_> = profile
                    .iter()
                    .map(|s| (s["t"].as_f64().unwrap(), s["width"].as_f64().unwrap()))
                    .collect();
                profiled_outline(&segments, &profile)
            } else {
                pressure_outline(&segments, &widths)
            };
            let actual = match result {
                Ok(actual) => actual,
                Err(error) => {
                    failures.push(format!("case {index} {} {phase}: {error:?}", case["name"]));
                    continue;
                }
            };
            let expected: Vec<SegOut> = serde_json::from_value(source["outline"].clone()).unwrap();
            if actual.len() != expected.len() {
                failures.push(format!(
                    "case {index} {} {phase}: {} != {} segments",
                    case["name"],
                    actual.len(),
                    expected.len()
                ));
                continue;
            }
            let max_error = actual
                .iter()
                .zip(&expected)
                .flat_map(|(a, b)| {
                    a.point
                        .iter()
                        .chain(&a.handle_in)
                        .chain(&a.handle_out)
                        .zip(b.point.iter().chain(&b.handle_in).chain(&b.handle_out))
                        .map(|(a, b)| (a - b).abs())
                })
                .fold(0f64, |max, error| {
                    if error.is_finite() {
                        max.max(error)
                    } else {
                        f64::INFINITY
                    }
                });
            if max_error >= 1e-8 {
                failures.push(format!(
                    "case {index} {} {phase}: max error {max_error}",
                    case["name"]
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
