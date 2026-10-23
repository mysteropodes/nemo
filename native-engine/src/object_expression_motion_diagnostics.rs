//! Pure diagnostic projection and original last-result publication per batch.
use crate::evaluation::property_expression_runtime::Diagnostic;
use crate::object_document::{ExpressionHolder, PropertyHolderTarget};
use crate::object_expression_runtime::motion_frame::commit::{
    same_source, ObjectMotionFrameControl, RecordedFrame,
};
use crate::object_expression_runtime::motion_frame::MotionEffect;
use crate::revision::ObjectSnapshot;

#[derive(Debug)]
pub(super) struct PublishedDiagnostic {
    target: ExpressionHolder,
    property: String,
    frame: u32,
    source: ObjectSnapshot,
    origin_snapshot: String,
    code: String,
    diagnostic: Option<Diagnostic>,
}

impl ObjectMotionFrameControl {
    pub(super) fn retire_finalized_diagnostics(&mut self, snapshot: &ObjectSnapshot) {
        self.diagnostics.retain(|entry| {
            same_source(&entry.source, snapshot)
                && expression_code(snapshot, &entry.target, &entry.property)
                    == Some(entry.code.as_str())
        });
    }

    /// Some(null) is an actual successful clear; None is unvisited or stale.
    /// The frame names the outer Motion read, including its recursive effects.
    /// Authored code stays an internal witness. This never evaluates or drains.
    pub fn diagnostic_for(
        &self,
        snapshot: &ObjectSnapshot,
        target: &PropertyHolderTarget,
        property: &str,
        frame: f64,
    ) -> Option<serde_json::Value> {
        self.diagnostic_for_holder(
            snapshot,
            &ExpressionHolder::Property(target.clone()),
            property,
            frame,
        )
    }
    pub(crate) fn diagnostic_for_holder(
        &self,
        snapshot: &ObjectSnapshot,
        target: &ExpressionHolder,
        property: &str,
        frame: f64,
    ) -> Option<serde_json::Value> {
        let entry = self.diagnostics.iter().find(|entry| {
            &entry.target == target
                && entry.property == property
                && f64::from(entry.frame) == frame
                && same_source(&entry.source, snapshot)
                && expression_code(snapshot, target, property) == Some(entry.code.as_str())
        })?;
        Some(
            entry
                .diagnostic
                .as_ref()
                .map(diagnostic_value)
                .unwrap_or(serde_json::Value::Null),
        )
    }

    /// Typed attribution only, never authored code or Debug-parsed state.
    pub fn diagnostic_records(&self) -> serde_json::Value {
        serde_json::Value::Array(
            self.diagnostics
                .iter()
                .map(|entry| {
                    serde_json::json!({
                        "instanceId": entry.source.instance_id(),
                        "documentId": entry.source.document_id(),
                        "target": crate::object_frame_packet::expression_sources::diagnostic_target(&entry.target),
                        "property": entry.property,
                        "frame": entry.frame,
                        "documentSnapshotId": entry.source.snapshot_id(),
                        "evaluatedDocumentSnapshotId": entry.origin_snapshot,
                        "diagnostic": entry.diagnostic.as_ref().map(diagnostic_value)
                    })
                })
                .collect(),
        )
    }
}

fn diagnostic_value(diagnostic: &Diagnostic) -> serde_json::Value {
    match diagnostic {
        Diagnostic::MustReturnNumber => serde_json::json!({"kind":"mustReturnNumber"}),
        Diagnostic::MustReturnNumberOrXY => serde_json::json!({"kind":"mustReturnNumberOrXY"}),
        Diagnostic::TooDeep => serde_json::json!({"kind":"tooDeep"}),
        Diagnostic::Timeout => serde_json::json!({"kind":"timeout"}),
        Diagnostic::Runtime { message, line } => {
            serde_json::json!({"kind":"runtime","message":message,"line":line})
        }
        Diagnostic::Syntax { message, line } => {
            serde_json::json!({"kind":"syntax","message":message,"line":line})
        }
    }
}

pub(super) fn expression_code<'a>(
    snapshot: &'a ObjectSnapshot,
    target: &ExpressionHolder,
    property: &str,
) -> Option<&'a str> {
    crate::object_frame_packet::expression_sources::code(snapshot, target, property)
}

fn touched(record: &RecordedFrame, entry: &PublishedDiagnostic) -> bool {
    record.effects.iter().any(|effect| {
        let dependencies = match effect {
            MotionEffect::Evaluated {
                target,
                property,
                result,
            } => {
                if target == &entry.target && property == &entry.property {
                    return true;
                }
                &result.dependencies
            }
            MotionEffect::Refused { dependencies, .. } => dependencies,
        };
        dependencies.iter().any(|dependency| {
            dependency.target == entry.target && dependency.property == entry.property
        })
    })
}

fn publish_diagnostic(
    ledger: &mut Vec<PublishedDiagnostic>,
    record: &RecordedFrame,
    source: &ObjectSnapshot,
    target: &ExpressionHolder,
    property: &str,
    diagnostic: &Option<Diagnostic>,
) {
    ledger.retain(|entry| {
        &entry.target != target || entry.property != property || entry.frame != record.frame
    });
    // Preflight verified every expression's target/code on this immutable origin.
    let code = expression_code(&record.origin, target, property)
        .unwrap()
        .to_owned();
    ledger.push(PublishedDiagnostic {
        target: target.clone(),
        property: property.to_owned(),
        frame: record.frame,
        source: source.clone(),
        origin_snapshot: record.origin.snapshot_id().to_owned(),
        code,
        diagnostic: diagnostic.clone(),
    });
}

pub(super) fn publish_records(ledger: &mut Vec<PublishedDiagnostic>, records: &[RecordedFrame]) {
    // Original last-result semantics retire the previous read for touched keys.
    // All actual frames of this retained batch stay queryable, not just its last.
    ledger.retain(|entry| !records.iter().any(|record| touched(record, entry)));
    for record in records {
        for effect in &record.effects {
            let dependencies = match effect {
                MotionEffect::Evaluated { result, .. } => &result.dependencies,
                MotionEffect::Refused { dependencies, .. } => dependencies,
            };
            // Recursive reads finish before the outer property reports.
            for dependency in dependencies {
                publish_diagnostic(
                    ledger,
                    record,
                    &record.source,
                    &dependency.target,
                    &dependency.property,
                    &dependency.diagnostic,
                );
            }
            if let MotionEffect::Evaluated {
                target,
                property,
                result,
            } = effect
            {
                publish_diagnostic(
                    ledger,
                    record,
                    &record.source,
                    target,
                    property,
                    &result.diagnostic,
                );
            }
        }
    }
}

/// Preserve all observed diagnostics at their real old source. Only an actual
/// committed timeout is proven to remain the disabled expression's diagnostic
/// on the fresh source. An old outer success is not a current-source clear.
pub(super) fn publish_committed_timeouts(
    ledger: &mut Vec<PublishedDiagnostic>,
    records: &[RecordedFrame],
    fresh: &ObjectSnapshot,
) {
    publish_records(ledger, records);
    for record in records {
        for effect in &record.effects {
            let dependencies = match effect {
                MotionEffect::Evaluated { result, .. } => &result.dependencies,
                MotionEffect::Refused { dependencies, .. } => dependencies,
            };
            for dependency in dependencies.iter().filter(|effect| effect.disable) {
                publish_diagnostic(
                    ledger,
                    record,
                    fresh,
                    &dependency.target,
                    &dependency.property,
                    &Some(Diagnostic::Timeout),
                );
            }
            if let MotionEffect::Evaluated {
                target,
                property,
                result,
            } = effect
            {
                if result.disable.is_some() {
                    publish_diagnostic(
                        ledger,
                        record,
                        fresh,
                        target,
                        property,
                        &Some(Diagnostic::Timeout),
                    );
                }
            }
        }
    }
}

#[cfg(all(test, feature = "test-object_expression_runtime"))]
#[test]
fn original_diagnostic_wire_preserves_null_and_zero_lines() {
    assert_eq!(
        diagnostic_value(&Diagnostic::Runtime {
            message: "runtime".into(),
            line: None
        }),
        serde_json::json!({"kind":"runtime","message":"runtime","line":null}),
    );
    assert_eq!(
        diagnostic_value(&Diagnostic::Syntax {
            message: "syntax".into(),
            line: Some(0)
        }),
        serde_json::json!({"kind":"syntax","message":"syntax","line":0}),
    );
}
