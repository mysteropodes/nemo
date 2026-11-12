//! Metal readback of a pressure ribbon carrying a brush-texture preset (#2361): the hidden ribbon anchor paints nothing and
//! the baseline dabs paint where the baseline stamper put them. The expected pixels are certified by the baseline run over
//! Paper.js (`tests/native-object-vector-fixtures/brush-dab-oracle.json` `ribbonCases[].pixels`): pixel centres at least
//! 0.9 px from every dab edge, composited last-made dab lowest over the white canvas, plus pixel centres inside the hidden
//! ribbon that no dab covers. A missing real adapter is a test FAILURE, never a skip; Metal readback is not an installed result.
use super::ObjectOutputSpec;
use crate::compositor::Compositor;
use crate::object_render_scene::prepare_object_composition;
use serde_json::Value;

#[path = "render_vector_support.rs"]
mod support;
use support::*;

const ORACLE: &str =
    include_str!("../../tests/native-object-vector-fixtures/brush-dab-oracle.json");

fn render(gpu: &mut Compositor, record: &Value) -> Vec<u8> {
    let snapshot = snapshot(&scene_doc(std::slice::from_ref(record)));
    let plan = plan_of(&snapshot);
    let prepared = prepare_object_composition(&plan, &selector(&snapshot, 0)).expect("witness");
    let output = ObjectOutputSpec::new(64, 64, "white-srgb-normal-rgba8-area").expect("output");
    let result = gpu.compose_object(&prepared, output).expect("compose");
    gpu.readback_object_rgba8(&result)
        .expect("readback")
        .bytes()
        .to_vec()
}

fn probes(case: &Value, key: &str) -> Vec<Probe> {
    case["pixels"][key]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p[0].as_u64().unwrap() as usize,
                p[1].as_u64().unwrap() as usize,
                [0, 1, 2, 3].map(|i| p[2][i].as_u64().unwrap() as u8),
            )
        })
        .collect()
}

fn cases() -> Vec<Value> {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    oracle["ribbonCases"].as_array().unwrap().clone()
}

#[test]
fn a_textured_pressure_ribbon_paints_the_baseline_dabs_and_nothing_of_its_hidden_outline() {
    let mut gpu = Compositor::new().expect("the ribbon pixels need a real headless Metal adapter");
    let mut hidden = 0;
    for case in cases() {
        let id = case["id"].as_str().unwrap();
        let bytes = render(&mut gpu, &case["record"]);
        assert_eq!(bytes.len(), SIZE * SIZE * 4, "{id}");
        let (dabs, free) = (probes(&case, "dabProbes"), probes(&case, "hiddenProbes"));
        assert!(dabs.len() >= 18, "{id}: certified dab probes");
        let failures = probe_failures(&bytes, &[dabs, free.clone()].concat(), 3);
        assert!(failures.is_empty(), "{id}: {failures:#?}");
        hidden += free.len();
    }
    assert!(
        hidden >= 16,
        "probes where only the camouflaged ribbon could paint"
    );
}

#[test]
fn the_probes_see_the_seed_and_the_camouflage() {
    let mut gpu = Compositor::new().expect("the ribbon pixels need a real headless Metal adapter");
    let case = cases()
        .into_iter()
        .find(|c| c["id"] == "steady-light")
        .unwrap();
    let all = [probes(&case, "dabProbes"), probes(&case, "hiddenProbes")].concat();
    // Another seed stamps other dabs.
    let mut reseeded = case["record"].clone();
    reseeded["vectorExt"]["brushTexture"]["strokeSeed"] =
        Value::from(case["seed"].as_u64().unwrap() - 1);
    assert!(
        !probe_failures(&render(&mut gpu, &reseeded), &all, 3).is_empty(),
        "a changed seed moves the dabs"
    );
    // Without the camouflage the ribbon paints its ink at its own opacity: the probes only the hidden ribbon could paint fail.
    let mut plain = case["record"].clone();
    plain["vectorExt"]
        .as_object_mut()
        .unwrap()
        .remove("brushTexture");
    plain["vector"]["opacity"] = case["opacity"].clone();
    let shown = probe_failures(&render(&mut gpu, &plain), &probes(&case, "hiddenProbes"), 3);
    assert_eq!(
        shown.len(),
        probes(&case, "hiddenProbes").len(),
        "every hidden-ribbon probe sees the ink: {shown:?}"
    );
}
