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

#[test]
fn pressure_motion_collapsed_input_and_uncleaned_outline_match_before_crossing_resolution() {
    let oracle:Value=serde_json::from_str(include_str!("fixtures/subselect-baseline/motion.json")).unwrap();
    let row=oracle["rows"].as_array().unwrap().iter().find(|r|r["name"]=="collapsed/vertices-zero-and-size").unwrap();
    let input=&row["outlineInput"];
    let center=row["center"].as_array().unwrap().iter().map(|s|serde_json::from_value(s.clone()).unwrap()).collect::<Vec<SegOut>>();
    let widths=row["center"].as_array().unwrap().iter().map(|s|s["width"].as_f64().unwrap()).collect::<Vec<_>>();
    let (points,mut widths)=sample_centerline(&center,&widths,None).unwrap();
    for w in &mut widths {*w*=0.6;}
    let expected_points:Vec<[f64;2]>=serde_json::from_value(input["pts"].clone()).unwrap();
    let expected_widths:Vec<f64>=serde_json::from_value(input["widths"].clone()).unwrap();
    assert_eq!(points,expected_points,"first divergence must not be moved to centerline sampling");
    assert_eq!(widths,expected_widths.iter().map(|w|w*0.6).collect::<Vec<_>>());
    let raw=outline_samples_raw(&points,&widths).unwrap();
    let expected:Vec<SegOut>=serde_json::from_value(row["outlineBeforeCleanup"].clone()).unwrap();
    assert_eq!(raw.len(),expected.len());
    let mut error=0f64;
    for (a,b) in raw.iter().zip(&expected) {for (a,b) in a.point.iter().chain(&a.handle_in).chain(&a.handle_out).zip(b.point.iter().chain(&b.handle_in).chain(&b.handle_out)) {error=error.max((a-b).abs());}}
    assert!(error<1e-12,"uncleaned cap/edge error {error}");
    assert_eq!(row["expected"].as_array().unwrap().len(),4,"retain frozen expected");
    assert_eq!(outline_samples(&points,&widths).unwrap().len(),19,"characterized native actual: first divergence is crossing cleanup");
    assert!(raw.iter().zip(raw.iter().cycle().skip(1)).any(|(a,b)|a.point==b.point && a.handle_out[0]*b.handle_in[1]-a.handle_out[1]*b.handle_in[0]!=0.),"non-collinear epsilon side loops are the bounded unavailable class");
}

#[cfg(feature="evaluation")]
#[test]
fn pressure_motion_zero_spine_trim_sampling_precedes_the_unresolved_crossing_disposition() {
    let oracle:Value=serde_json::from_str(include_str!("fixtures/subselect-baseline/motion.json")).unwrap();
    for (name,actual_count,expected_count) in [("collapsed/trim",52,53),("collapsed/trim-wins",52,53),("collapsed/full-trim-wins",65,83),("collapsed-matching/trim",37,50),("collapsed-matching/trim-wins",37,50),("collapsed-matching/full-trim-wins",82,81)] {
        let row=oracle["rows"].as_array().unwrap().iter().find(|r|r["name"]==name).unwrap();
        let center:Vec<SegOut>=serde_json::from_value(row["center"].clone()).unwrap();
        let scalar=|k:&str,default:f64|row["raw"][k][0].as_f64().unwrap_or(default);
        let (pts,ws)=crate::evaluation::object_vector_kernels::render_vector_eval::vertex::trim_ribbon_samples(&center,&[],[scalar("trimStart",0.),scalar("trimEnd",100.),scalar("trimOffset",0.)]).unwrap();
        let expected_points:Vec<[f64;2]>=serde_json::from_value(row["outlineInput"]["pts"].clone()).unwrap();
        assert_eq!(pts.len(),expected_points.len(),"{name} sample identity/count");
        for (a,b) in pts.iter().zip(expected_points) {for j in 0..2 {assert!((a[j]-b[j]).abs()<1e-12,"{name} sampled coordinate");}}
        let actual=outline_samples(&pts,&ws).unwrap().len();
        assert_eq!(actual,actual_count,"retain native actual");
        assert_eq!(row["expected"].as_array().unwrap().len(),expected_count,"retain frozen expected");
    }
}
