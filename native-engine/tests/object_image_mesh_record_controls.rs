//! Negative and guard controls for the image-mesh store record (L03-J, F17).
//!
//! Every rejection below is a single flaw on an otherwise admitted mesh, next to its
//! admitted twin. The single-flaw mutants of the same rules live in
//! `object_image_mesh_record_mutants.rs`.

use super::oracle_rows::load_rows;
use super::{ImageMesh, ImageMeshAdmissionError, ImageMeshStore, MeshIdSequence};
use serde_json::{Number, Value};

pub(crate) const POINTS3: &str = "[[0,0],[1,0],[0,1]]";

pub(crate) fn mesh(outline: &str, verts: &str, tris: &str, extra: &str) -> String {
    format!(r#"{{"outline":{outline},"verts":{verts},"tris":{tris}{extra}}}"#)
}

pub(crate) fn base(extra: &str) -> String {
    mesh(POINTS3, POINTS3, "[0,1,2]", extra)
}

fn one(id: &str, body: &str) -> String {
    format!("{{{}:{body}}}", Value::from(id))
}

fn admission(text: &str) -> Result<(), ImageMeshAdmissionError> {
    serde_json::from_str::<ImageMeshStore>(text)
        .expect("wire decodes")
        .validate()
}

#[test]
fn the_admitted_twin_passes_so_each_rejection_is_its_single_flaw() {
    assert_eq!(admission(&one("im_1", &base(""))), Ok(()));
    assert_eq!(
        admission(&one(
            "im_1",
            &base(r#","offsets":[[0,0],[0,0],[0,0]],"cols":2,"rows":64"#)
        )),
        Ok(())
    );
    assert_eq!(admission("{}"), Ok(()));
}

#[test]
fn single_flaw_meshes_are_rejected_with_the_typed_error() {
    use ImageMeshAdmissionError as E;
    let cases: [(&str, String, E); 14] = [
        (
            "tris length 4",
            mesh(POINTS3, POINTS3, "[0,1,2,0]", ""),
            E::TrianglesNotTriples,
        ),
        (
            "tris length 2",
            mesh(POINTS3, POINTS3, "[0,1]", ""),
            E::TrianglesNotTriples,
        ),
        (
            "index equals verts length",
            mesh(POINTS3, POINTS3, "[0,1,3]", ""),
            E::TriangleIndexOutOfRange,
        ),
        (
            "index far outside",
            mesh(POINTS3, POINTS3, "[0,1,4000000000]", ""),
            E::TriangleIndexOutOfRange,
        ),
        (
            "outline longer than verts",
            mesh("[[0,0],[1,0],[0,1],[1,1]]", POINTS3, "[0,1,2]", ""),
            E::OutlineLongerThanVerts,
        ),
        (
            "second prefix point differs",
            mesh(POINTS3, "[[0,0],[1,1],[0,1]]", "[0,1,2]", ""),
            E::OutlinePrefixMismatch,
        ),
        (
            "last prefix point differs",
            mesh(POINTS3, "[[0,0],[1,0],[0,2]]", "[0,1,2]", ""),
            E::OutlinePrefixMismatch,
        ),
        (
            "offsets one short",
            base(r#","offsets":[[0,0],[0,0]]"#),
            E::OffsetsLengthMismatch,
        ),
        (
            "offsets one long",
            base(r#","offsets":[[0,0],[0,0],[0,0],[0,0]]"#),
            E::OffsetsLengthMismatch,
        ),
        (
            "explicit empty offsets over three verts",
            base(r#","offsets":[]"#),
            E::OffsetsLengthMismatch,
        ),
        ("cols zero", base(r#","cols":0"#), E::DensityOutOfRange),
        ("cols 65", base(r#","cols":65"#), E::DensityOutOfRange),
        ("rows zero", base(r#","rows":0"#), E::DensityOutOfRange),
        ("rows 65", base(r#","rows":65"#), E::DensityOutOfRange),
    ];
    for (name, body, expected) in cases {
        assert_eq!(admission(&one("im_1", &body)), Err(expected), "{name}");
        let wrapped: ImageMesh = serde_json::from_str(&body).expect("mesh decodes");
        assert_eq!(wrapped.validate(), Err(expected), "{name} (mesh level)");
    }
}

#[test]
fn store_level_flaws_are_rejected() {
    let body = base("");
    assert_eq!(
        admission(&one("", &body)),
        Err(ImageMeshAdmissionError::EmptyMeshId)
    );
    let duplicate = format!("{{\"im_1\":{body},\"im_1\":{body}}}");
    let error = admission(&duplicate).expect_err("duplicate id");
    assert_eq!(error, ImageMeshAdmissionError::DuplicateMeshId);
    assert!(error.is_duplicate());
    assert!(!ImageMeshAdmissionError::EmptyMeshId.is_duplicate());
    // The same id spelled twice in different positions is still one key.
    let spread = format!("{{\"im_1\":{body},\"im_2\":{body},\"im_1\":{body}}}");
    assert_eq!(
        admission(&spread),
        Err(ImageMeshAdmissionError::DuplicateMeshId)
    );
    assert_eq!(
        admission(&one("im_9007199254740992", &body)),
        Err(ImageMeshAdmissionError::IdSequenceUnsafe)
    );
    assert_eq!(admission(&one("im_9007199254740991", &body)), Ok(()));
}

#[test]
fn malformed_wire_shapes_never_decode() {
    let wire: [(&str, String); 17] = [
        ("cols as a float", one("a", &base(r#","cols":4.0"#))),
        ("cols negative", one("a", &base(r#","cols":-1"#))),
        ("cols null", one("a", &base(r#","cols":null"#))),
        ("offsets null", one("a", &base(r#","offsets":null"#))),
        (
            "fractional index",
            one("a", &mesh(POINTS3, POINTS3, "[0,1,2.5]", "")),
        ),
        (
            "string index",
            one("a", &mesh(POINTS3, POINTS3, r#"["0","1","2"]"#, "")),
        ),
        (
            "typed array as an object",
            one("a", &mesh(POINTS3, POINTS3, r#"{"0":0,"1":1,"2":2}"#, "")),
        ),
        (
            "point of three numbers",
            one("a", &mesh("[[0,0,0]]", "[[0,0,0]]", "[]", "")),
        ),
        (
            "point of one number",
            one("a", &mesh("[[0]]", "[[0]]", "[]", "")),
        ),
        ("unknown member", one("a", &base(r#","bones":[]"#))),
        ("missing verts", one("a", r#"{"outline":[],"tris":[]}"#)),
        ("missing outline", one("a", r#"{"verts":[],"tris":[]}"#)),
        ("missing tris", one("a", r#"{"outline":[],"verts":[]}"#)),
        (
            "coordinate beyond f64",
            one("a", &mesh("[[1e999,0]]", "[[1e999,0]]", "[]", "")),
        ),
        ("null mesh", one("a", "null")),
        ("array mesh", one("a", "[]")),
        ("array store", "[]".to_string()),
    ];
    for (name, text) in wire {
        assert!(
            serde_json::from_str::<ImageMeshStore>(&text).is_err(),
            "{name}"
        );
    }
    let message = serde_json::from_str::<ImageMeshStore>(&one("a", &base(r#","bones":[]"#)))
        .expect_err("unknown member")
        .to_string();
    assert!(message.contains("unknown field"), "{message}");
}

#[test]
fn non_finite_numbers_cannot_enter_the_wire_or_a_number() {
    assert!(Number::from_f64(f64::NAN).is_none());
    assert!(Number::from_f64(f64::INFINITY).is_none());
    assert!(Number::from_f64(f64::NEG_INFINITY).is_none());
    assert!(serde_json::from_str::<ImageMeshStore>(&one(
        "a",
        &mesh("[[1e999,0]]", "[[1e999,0]]", "[]", "")
    ))
    .is_err());
}

// ---- guards ---------------------------------------------------------------------------------

#[test]
fn effective_views_are_detached_copies_and_the_state_check_can_fail() {
    let text = r#"{"im_1":{"outline":[[0,0],[1,0],[0,1]],"verts":[[0,0],[1,0],[0,1]],"tris":[0,1,2],"offsets":[[0,0],[0.5,0],[0,0]],"cols":2,"rows":2}}"#;
    let decoded: ImageMeshStore = serde_json::from_str(text).expect("store");
    let before = decoded.clone();
    let mesh = &decoded.entries()[0].1;
    let mut offsets = mesh.effective_offsets();
    offsets[1][0] = 9.0;
    let mut rest = mesh.rest_points();
    rest[0] = [7.0, 7.0];
    let mut outline = mesh.outline_points();
    outline.clear();
    assert_eq!(decoded, before, "reads must not reach the stored record");
    assert_eq!(mesh.effective_offsets()[1], [0.5, 0.0]);
    let mut tampered = before.clone();
    tampered.entries[0].1.tris.reverse();
    assert_ne!(
        tampered, decoded,
        "the equality used above can detect a changed store"
    );
}

#[test]
fn admission_and_sequence_are_pure_over_the_store() {
    let text = one("im_3", &base(""));
    let decoded: ImageMeshStore = serde_json::from_str(&text).expect("store");
    let before = decoded.clone();
    assert_eq!(decoded.validate(), Ok(()));
    let mut sequence = MeshIdSequence::default();
    sequence.observe_store(&decoded).expect("safe");
    assert_eq!(sequence.issue(&decoded).as_deref(), Ok("im_4"));
    assert_eq!(decoded, before);
    assert!(decoded.get("").is_none());
    assert!(decoded.get("im_3").is_some() && decoded.get("im_4").is_none());
}

#[test]
fn the_oracle_is_not_vacuous() {
    let rows = load_rows();
    let with_triangles = rows
        .iter()
        .filter(|r| r.input.contains("\"tris\":[") && !r.input.contains("\"tris\":[]"))
        .count();
    assert!(with_triangles >= 20, "{with_triangles}");
    let defaulted = rows.iter().filter(|r| r.baseline != r.input).count();
    assert!(
        defaulted >= 6,
        "rows where load() filled in defaults: {defaulted}"
    );
    let posed = rows
        .iter()
        .filter(|r| r.input.contains("[0.1,0]") || r.input.contains("[-0.05,0.25]"))
        .count();
    assert!(posed >= 3, "rows carrying a non-zero static pose: {posed}");
    let advanced = rows
        .iter()
        .filter(|r| r.issued[0] != "im_1" && r.issued[0] != "im_2")
        .count();
    assert!(advanced >= 20, "{advanced}");
}
