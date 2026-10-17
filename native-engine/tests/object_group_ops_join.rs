//! L02-G x L02-B join: the table a transaction PERSISTS feeds the combine
//! operand-plan kernel, and a nested combined child stays ONE operand. The only
//! stand-in is the boolean provider port (`CombineFold`): its real provider is
//! outside this crate (lead ruling R3), so the recording port keeps the exact
//! operand lists the kernel hands it. Group structure is built and persisted by
//! the real owner; the facts mirror the record fills.
use super::object_group_ops_tests::{
    apply, bytes, combine, current, document, group, owner_of, set_mode,
};
use crate::object_document::object_group_kernels::object_group_combine_plan::{
    render_combines, CombineFold, CombineLayer, FoldError, MemberFacts,
};
use crate::object_document::object_group_kernels::object_group_hierarchy::CombineMode;
use crate::object_document::object_group_kernels::object_group_record::layer_stroke_ids;

/// Shapes are the item slots they cover, in operand order.
#[derive(Default)]
struct Recorder {
    folds: Vec<(CombineMode, Vec<Vec<usize>>)>,
}

impl CombineFold for Recorder {
    type Shape = Vec<usize>;
    fn member_shape(
        &mut self,
        item: usize,
        _companion: Option<usize>,
    ) -> Result<Vec<usize>, FoldError> {
        Ok(vec![item])
    }
    fn fold(
        &mut self,
        mode: CombineMode,
        operands: &[Vec<usize>],
    ) -> Result<Vec<usize>, FoldError> {
        self.folds.push((mode, operands.to_vec()));
        Ok(operands.concat())
    }
    fn islands(&mut self, shape: &Vec<usize>) -> Result<Vec<Vec<usize>>, FoldError> {
        Ok(vec![shape.clone()])
    }
}

fn facts(count: usize) -> Vec<MemberFacts> {
    (0..count)
        .map(|n| MemberFacts {
            visible: true,
            linked_fill_id: None,
            brush_group_id: None,
            is_linked_fill_companion: false,
            is_vector_brush: false,
            fill_color: Some(format!("#00000{n}")),
            stroke_color: None,
            stroke_width: 1.0,
            opacity: 1.0,
        })
        .collect()
}

#[test]
fn a_persisted_nested_combine_group_is_one_operand_of_its_parent() {
    for version in [1, 2] {
        let mut owner = owner_of(&document(version, &["s1", "s2", "s3", "s4"], None));
        // gi folds s1..s3; go nests gi (as ONE unit) beside the loose s4.
        apply(
            &mut owner,
            "combine",
            &combine(&["s1", "s2", "s3"], "gi", "subtract"),
        )
        .unwrap();
        apply(
            &mut owner,
            "group",
            &group(&["s1", "s2", "s3", "s4"], "go", "outer"),
        )
        .unwrap();
        apply(&mut owner, "mode", &set_mode("go", "unite")).unwrap();
        // Reopen so the plan reads what a saved file holds, not the live plan.
        let reopened = owner_of(&serde_json::from_slice(&bytes(&owner)).unwrap());
        let snapshot = current(&reopened);
        let doc = snapshot.document();
        let strokes = layer_stroke_ids(doc.objects(), "L0", "scene-root");
        assert_eq!(strokes, ["s1", "s2", "s3", "s4"]);
        let stored = doc.layers()[0].groups.value().unwrap();
        let groups = stored.to_layer(&strokes).unwrap();
        let member_facts = facts(strokes.len());
        let layer = CombineLayer::new(&groups, &member_facts).unwrap();
        let mut port = Recorder::default();
        let outcome = render_combines(&layer, &mut port).unwrap();
        assert_eq!(
            port.folds,
            vec![
                (CombineMode::Subtract, vec![vec![0], vec![1], vec![2]]),
                (CombineMode::Unite, vec![vec![0, 1, 2], vec![3]]),
            ],
            "v{version}: the inner result is a single operand"
        );
        assert_eq!(outcome.suppress, [0, 1, 2, 3]);
        assert_eq!(outcome.paths.len(), 1);
        let path = &outcome.paths[0];
        assert_eq!(path.group, "go");
        assert_eq!(path.shape, [0, 1, 2, 3]);
        assert_eq!(path.style.fill.as_deref(), Some("#000003"));
    }
}

#[test]
fn an_unavailable_boolean_provider_is_typed_not_a_fake_result() {
    use crate::object_document::object_group_kernels::object_group_combine_plan::{
        CombineError, UnavailableFold,
    };
    let mut owner = owner_of(&document(1, &["s1", "s2"], None));
    apply(
        &mut owner,
        "combine",
        &combine(&["s1", "s2"], "gi", "unite"),
    )
    .unwrap();
    let snapshot = current(&owner);
    let strokes = layer_stroke_ids(snapshot.document().objects(), "L0", "scene-root");
    let groups = snapshot.document().layers()[0]
        .groups
        .value()
        .unwrap()
        .to_layer(&strokes)
        .unwrap();
    let member_facts = facts(2);
    let layer = CombineLayer::new(&groups, &member_facts).unwrap();
    assert_eq!(
        render_combines(&layer, &mut UnavailableFold).unwrap_err(),
        CombineError::FoldUnavailable
    );
}
