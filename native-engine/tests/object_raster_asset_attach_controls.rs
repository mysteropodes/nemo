//! Negative and mutation controls of the asset transaction. A control is a
//! standalone single-flaw copy of a rule that must FAIL a named case while the
//! real function passes it (and the flaw must not break the other cases), or a
//! probe check that shows an assertion elsewhere is load-bearing.
use crate::object_document::object_raster_asset_kernels::object_raster_asset_use::asset_in_use;
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetPlacement, AssetRecord, AssetSource, AssetTarget,
    ProjectEnvelope, ProjectReferenceState,
};
use crate::revision::object_revision::encoded_retention_budget::EncodedRetentionBudget;
use crate::revision::{ObjectRevisionOwner, ObjectSnapshot};
use std::sync::Arc;

include!("object_raster_asset_attach_support.rs");

fn id(text: &str) -> AssetId {
    AssetId::new(text.to_string()).expect("asset id")
}

fn record(name: &str, source: AssetSource) -> Arc<AssetRecord> {
    Arc::new(
        AssetRecord::new(id(name), format!("{name}.png"), AssetKind::Image, source)
            .expect("record"),
    )
}

fn own_source(png: &str) -> AssetSource {
    let bytes = oracle_bytes(png);
    let limit = bytes.capacity() + 1;
    AssetSource::admitted_embedded("image/png".to_string(), Arc::new(bytes), limit).expect("source")
}

fn inventory(records: Vec<Arc<AssetRecord>>, placements: Vec<AssetPlacement>) -> AssetInventory {
    let limit = placements.len();
    AssetInventory::new(records, placements, 1024, limit).expect("inventory")
}

/// A project that carries the still-image reference `ref` (red png).
fn referenced_project() -> (ProjectEnvelope, Arc<Vec<u8>>) {
    let mut history = open_history();
    history
        .attach_reference_png(
            0,
            "ref".to_string(),
            "ref.png".to_string(),
            &oracle_bytes("red-2x1"),
        )
        .expect("reference attach");
    let project = history
        .acquire_snapshot(1)
        .expect("snapshot")
        .project()
        .expect("project")
        .clone();
    let ProjectReferenceState::StillImage(reference) = project.reference_state() else {
        panic!("reference carried");
    };
    let allocation = Arc::clone(reference.encoded_allocation());
    (project, allocation)
}

fn bare_project() -> ProjectEnvelope {
    open_history()
        .acquire_snapshot(0)
        .expect("snapshot")
        .project()
        .expect("project")
        .clone()
}

fn placement_for(asset: &str) -> AssetPlacement {
    AssetPlacement::new(
        "placement-1".to_string(),
        id(asset),
        AssetTarget::Drawing {
            context_id: "scene-root".to_string(),
            layer_uid: "drawing".to_string(),
            stroke_id: None,
        },
        0,
    )
    .expect("placement")
}

// ---- single-flaw copies of `asset_in_use` -----------------------------------------------------------

fn ignores_placements(project: &ProjectEnvelope, asset: &AssetId) -> bool {
    let Some(inventory) = project.asset_inventory() else {
        return false;
    };
    let ProjectReferenceState::StillImage(reference) = project.reference_state() else {
        return false;
    };
    reference.id() == asset.as_str()
        || inventory
            .get(asset)
            .and_then(|r| r.source().encoded_allocation())
            .is_some_and(|a| Arc::ptr_eq(a, reference.encoded_allocation()))
}

fn ignores_shared_allocation(project: &ProjectEnvelope, asset: &AssetId) -> bool {
    let Some(inventory) = project.asset_inventory() else {
        return false;
    };
    if inventory.placements().iter().any(|p| p.asset_id() == asset) {
        return true;
    }
    let ProjectReferenceState::StillImage(reference) = project.reference_state() else {
        return false;
    };
    reference.id() == asset.as_str()
}

fn ignores_the_reference_id(project: &ProjectEnvelope, asset: &AssetId) -> bool {
    let Some(inventory) = project.asset_inventory() else {
        return false;
    };
    if inventory.placements().iter().any(|p| p.asset_id() == asset) {
        return true;
    }
    let ProjectReferenceState::StillImage(reference) = project.reference_state() else {
        return false;
    };
    inventory
        .get(asset)
        .and_then(|r| r.source().encoded_allocation())
        .is_some_and(|a| Arc::ptr_eq(a, reference.encoded_allocation()))
}

struct Case {
    name: &'static str,
    project: ProjectEnvelope,
    asset: AssetId,
    in_use: bool,
}

fn cases() -> Vec<Case> {
    let (referenced, shared) = referenced_project();
    let by_id = referenced.with_asset_inventory(Some(inventory(
        vec![record("ref", own_source("blue-2x1"))],
        Vec::new(),
    )));
    let by_allocation = referenced.with_asset_inventory(Some(inventory(
        vec![record(
            "twin",
            AssetSource::admitted_embedded("image/png".to_string(), shared, 1 << 20).unwrap(),
        )],
        Vec::new(),
    )));
    let free = referenced.with_asset_inventory(Some(inventory(
        vec![record("free", own_source("blue-2x1"))],
        Vec::new(),
    )));
    let by_placement = bare_project().with_asset_inventory(Some(inventory(
        vec![record("placed", own_source("blue-2x1"))],
        vec![placement_for("placed")],
    )));
    let unplaced = bare_project().with_asset_inventory(Some(inventory(
        vec![record("loose", own_source("blue-2x1"))],
        Vec::new(),
    )));
    vec![
        Case {
            name: "placement",
            project: by_placement,
            asset: id("placed"),
            in_use: true,
        },
        Case {
            name: "reference id",
            project: by_id,
            asset: id("ref"),
            in_use: true,
        },
        Case {
            name: "shared allocation",
            project: by_allocation,
            asset: id("twin"),
            in_use: true,
        },
        Case {
            name: "unreferenced beside a reference",
            project: free,
            asset: id("free"),
            in_use: false,
        },
        Case {
            name: "unreferenced without a reference",
            project: unplaced,
            asset: id("loose"),
            in_use: false,
        },
        Case {
            name: "no catalog",
            project: bare_project(),
            asset: id("anything"),
            in_use: false,
        },
    ]
}

#[test]
fn the_real_predicate_passes_every_named_case() {
    for case in cases() {
        assert_eq!(
            asset_in_use(&case.project, &case.asset),
            case.in_use,
            "{}",
            case.name
        );
    }
}

#[test]
fn each_single_flaw_copy_fails_exactly_its_named_case() {
    type Rule = fn(&ProjectEnvelope, &AssetId) -> bool;
    let mutants: [(&str, Rule, &str); 3] = [
        ("ignores placements", ignores_placements, "placement"),
        (
            "ignores shared allocation",
            ignores_shared_allocation,
            "shared allocation",
        ),
        (
            "ignores the reference id",
            ignores_the_reference_id,
            "reference id",
        ),
    ];
    let all = cases();
    for (label, rule, broken) in mutants {
        for case in &all {
            let answer = rule(&case.project, &case.asset);
            if case.name == broken {
                assert_ne!(answer, case.in_use, "{label} must fail its case {broken}");
            } else {
                assert_eq!(answer, case.in_use, "{label} must still pass {}", case.name);
            }
        }
    }
}

// ---- probes -----------------------------------------------------------------------------------------

#[test]
fn a_leaked_alias_keeps_the_charge_so_the_zero_assertions_are_load_bearing() {
    let budget = EncodedRetentionBudget::new(1 << 20).unwrap();
    let mut owner = ObjectRevisionOwner::from_admitted(open_history().acquire_snapshot(0).unwrap());
    let bytes = oracle_bytes("blue-2x1");
    let source = AssetSource::admitted_embedded(
        "image/png".to_string(),
        Arc::new(bytes.clone()),
        bytes.capacity() + 1,
    )
    .unwrap();
    let leak = Arc::clone(source.encoded_allocation().unwrap());
    let cost = leak.capacity();
    owner
        .publish_admitted_asset_in(record("leaky", source), &budget)
        .unwrap();
    assert_eq!(budget.used(), Ok(cost));
    drop(owner);
    // The owner and every revision are gone, yet the leaked alias still pins the bytes.
    assert_eq!(budget.used(), Ok(cost));
    drop(leak);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn the_production_entry_publishes_into_the_shared_ledger() {
    let mut owner = ObjectRevisionOwner::from_admitted(open_history().acquire_snapshot(0).unwrap());
    let bytes = oracle_bytes("red-2x1");
    let mut permit = ObjectSnapshot::reserve_encoded(bytes.len()).unwrap();
    let mut buffer = permit.allocate().unwrap();
    buffer.copy_from_slice(&bytes);
    let allocation = Arc::new(buffer);
    let source =
        AssetSource::admitted_embedded("image/png".to_string(), Arc::clone(&allocation), 1 << 20)
            .unwrap();
    permit.bind(&allocation).unwrap();
    assert_eq!(
        owner.publish_admitted_asset(record("shared", source)),
        Ok(1)
    );
    let head = owner.acquire(1).unwrap();
    let held = head.project().unwrap().asset_inventory().unwrap().records();
    assert!(Arc::ptr_eq(
        held[0].source().encoded_allocation().unwrap(),
        &allocation
    ));
}
