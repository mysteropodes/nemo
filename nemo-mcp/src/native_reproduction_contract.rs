//! Closed response shapes for the bounded native reproduction operations.
use super::{exact, revision};
use crate::contract::bounded_identifier;
use serde_json::Value;

pub(super) fn report_request(value: &Value) -> bool {
    exact(value, &["expectedContentRevision", "expectedSequence"], &[]).is_some_and(|token| {
        revision(token.get("expectedContentRevision")).is_some()
            && revision(token.get("expectedSequence")).is_some()
    })
}

pub(super) fn report_result(payload: &Value, value: &Value) -> bool {
    report_request(payload)
        && exact(
            value,
            &["bundle", "verifiedContentRevision", "verifiedSequence"],
            &[],
        )
        .is_some_and(|report| {
            report.get("verifiedContentRevision") == payload.get("expectedContentRevision")
                && report.get("verifiedSequence") == payload.get("expectedSequence")
                && revision(report.get("verifiedSequence"))
                    .is_some_and(|sequence| sequence < 9_007_199_254_740_991)
                && report.get("bundle").is_some_and(bundle)
        })
}

pub(super) fn bundle(value: &Value) -> bool {
    let Some(wire) = exact(
        value,
        &[
            "format",
            "formatVersion",
            "apiVersion",
            "fixture",
            "command",
            "stableTarget",
            "clock",
            "seed",
            "versions",
            "commands",
        ],
        &[],
    ) else {
        return false;
    };
    let fixture = wire
        .get("fixture")
        .and_then(|value| exact(value, &["id", "version", "sha256"], &[]));
    let target = wire
        .get("stableTarget")
        .and_then(|value| exact(value, &["layerUid"], &[]));
    let versions = wire
        .get("versions")
        .and_then(|value| exact(value, &["nativeEngine"], &[]));
    let commands = wire.get("commands").and_then(Value::as_array);
    wire.get("format").and_then(Value::as_str) == Some("nemo.native-opacity-reproduction")
        && wire.get("formatVersion").and_then(Value::as_u64) == Some(1)
        && wire.get("apiVersion").and_then(Value::as_u64) == Some(2)
        && wire.get("command").and_then(Value::as_str) == Some("layer.opacity.set")
        && wire.get("clock") == Some(&Value::Null)
        && wire.get("seed") == Some(&Value::Null)
        && fixture.is_some_and(|fixture| {
            fixture
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(bounded_identifier)
                && fixture.get("version").and_then(Value::as_u64) == Some(1)
                && fixture
                    .get("sha256")
                    .and_then(Value::as_str)
                    .is_some_and(|hash| {
                        hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                    })
        })
        && target.is_some_and(|target| {
            target
                .get("layerUid")
                .and_then(Value::as_str)
                .is_some_and(bounded_identifier)
        })
        && versions.is_some_and(|versions| {
            versions
                .get("nativeEngine")
                .and_then(Value::as_str)
                .is_some_and(|version| !version.is_empty())
        })
        && commands.is_some_and(|commands| {
            (1..=32).contains(&commands.len())
                && commands.iter().enumerate().all(|(index, command)| {
                    exact(
                        command,
                        &["id", "expectedRevision", "value", "revision", "applied"],
                        &[],
                    )
                    .is_some_and(|command| {
                        command.get("id").and_then(Value::as_u64) == Some(index as u64 + 1)
                            && revision(command.get("expectedRevision")).is_some()
                            && command
                                .get("value")
                                .and_then(Value::as_f64)
                                .is_some_and(|value| (0.0..=100.0).contains(&value))
                            && revision(command.get("revision")).is_some()
                            && command.get("applied").is_some_and(Value::is_boolean)
                    })
                })
        })
        && serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= 3072)
}

fn reproduction_state(value: &Value) -> bool {
    exact(
        value,
        &["revision", "opacity", "undoDepth", "redoDepth"],
        &[],
    )
    .is_some_and(|state| {
        revision(state.get("revision")).is_some()
            && state
                .get("opacity")
                .and_then(Value::as_f64)
                .is_some_and(|value| (0.0..=100.0).contains(&value))
            && ["undoDepth", "redoDepth"].iter().all(|key| {
                state
                    .get(*key)
                    .and_then(Value::as_u64)
                    .is_some_and(|value| value <= 32)
            })
    })
}

pub(super) fn validate_result(operation: &str, value: &Value) -> bool {
    match operation {
        "command.reproduction.opt_in" | "query.reproduction.status" => exact(
            value,
            &["state", "reason", "commandCount", "exportable"],
            &[],
        )
        .is_some_and(|status| {
            status
                .get("state")
                .and_then(Value::as_str)
                .is_some_and(|state| ["disabled", "recording", "invalid"].contains(&state))
                && status.get("reason").is_some_and(|reason| {
                    reason.is_null()
                        || reason.as_str().is_some_and(|reason| {
                            [
                                "not_catalog",
                                "not_pristine",
                                "already_opted_in",
                                "not_opted_in",
                                "empty_journal",
                                "failed_mutation",
                                "changed_retry",
                                "unsupported_transition",
                                "released",
                                "replaced",
                                "command_limit",
                                "byte_limit",
                                "invalid_projection",
                            ]
                            .contains(&reason)
                        })
                })
                && status
                    .get("commandCount")
                    .and_then(Value::as_u64)
                    .is_some_and(|count| count <= 32)
                && status.get("exportable").is_some_and(Value::is_boolean)
        }),
        "query.reproduction.export" => exact(value, &["bundle"], &[])
            .and_then(|result| result.get("bundle"))
            .is_some_and(bundle),
        "query.reproduction.replay" => {
            exact(value, &["initial", "steps"], &[]).is_some_and(|report| {
                report.get("initial").is_some_and(reproduction_state)
                    && report
                        .get("steps")
                        .and_then(Value::as_array)
                        .is_some_and(|steps| {
                            (1..=32).contains(&steps.len())
                                && steps.iter().enumerate().all(|(index, step)| {
                                    exact(
                                        step,
                                        &["id", "expectedRevision", "applied", "state"],
                                        &[],
                                    )
                                    .is_some_and(|step| {
                                        step.get("id").and_then(Value::as_u64)
                                            == Some(index as u64 + 1)
                                            && revision(step.get("expectedRevision")).is_some()
                                            && step.get("applied").is_some_and(Value::is_boolean)
                                            && step.get("state").is_some_and(reproduction_state)
                                    })
                                })
                        })
            })
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn report_contract_requires_closed_safe_tokens_and_exact_response_binding() {
        let payload = json!({"expectedContentRevision":1,"expectedSequence":2});
        let bundle = json!({
            "format":"nemo.native-opacity-reproduction", "formatVersion":1, "apiVersion":2,
            "fixture":{"id":"native-opacity-static", "version":1, "sha256":"a".repeat(64)},
            "command":"layer.opacity.set", "stableTarget":{"layerUid":"r08_curve_layer"},
            "clock":null, "seed":null, "versions":{"nativeEngine":"0.1.0"},
            "commands":[{"id":1,"expectedRevision":0,"value":40,"revision":1,"applied":true}]
        });
        let result = json!({"bundle":bundle,"verifiedContentRevision":1,"verifiedSequence":2});
        let operation = "query.reproduction.report";
        let mut request = crate::contract::NativeApplicationRequest {
            api_version: 2,
            request_id: "report".into(),
            instance_id: "instance".into(),
            document_id: "document".into(),
            operation: operation.into(),
            payload: payload.clone(),
            expected_revision: None,
            cancelled_before_dispatch: false,
        };
        assert!(
            request.validate().is_ok(),
            "feature declaration must expose report"
        );
        assert!(super::super::validate_result(
            operation, "document", &payload, &result
        ));
        request.expected_revision = Some(1);
        assert!(request.validate().is_err());
        request.expected_revision = None;
        for bad in [
            json!({}),
            json!([]),
            json!({"expectedContentRevision":1}),
            json!({"expectedContentRevision":1,"expectedSequence":"2"}),
            json!({"expectedContentRevision":1,"expectedSequence":2,"extra":true}),
            json!({"expectedContentRevision":1,"expectedSequence":-1}),
            json!({"expectedContentRevision":1,"expectedSequence":1.5}),
            json!({"expectedContentRevision":9007199254740992_u64,"expectedSequence":2}),
            json!({"expectedContentRevision":1,"expectedSequence":9007199254740992_u64}),
        ] {
            request.payload = bad.clone();
            assert!(request.validate().is_err());
            assert!(!report_result(&bad, &result));
        }
        for (field, value) in [
            ("verifiedContentRevision", json!(2)),
            ("verifiedSequence", json!(1)),
            ("bundle", json!("escaped")),
            ("extra", json!(true)),
        ] {
            let mut wrong = result.clone();
            wrong[field] = value;
            assert!(!report_result(&payload, &wrong));
        }
        let mut missing = result.clone();
        missing.as_object_mut().unwrap().remove("verifiedSequence");
        assert!(!report_result(&payload, &missing));
        let mut exhausted = result;
        exhausted["verifiedSequence"] = json!(9007199254740991_u64);
        assert!(!report_result(
            &json!({"expectedContentRevision":1,"expectedSequence":9007199254740991_u64}),
            &exhausted
        ));
    }

    #[test]
    fn reproduction_bundle_and_results_are_closed_and_bounded() {
        let valid = json!({
            "format":"nemo.native-opacity-reproduction", "formatVersion":1, "apiVersion":2,
            "fixture":{"id":"catalog", "version":1, "sha256":"a".repeat(64)},
            "command":"layer.opacity.set", "stableTarget":{"layerUid":"r08_curve_layer"},
            "clock":null, "seed":null, "versions":{"nativeEngine":"0.1.0"},
            "commands":[{"id":1,"expectedRevision":0,"value":40,"revision":1,"applied":true}]
        });
        assert!(bundle(&valid));
        assert!(validate_result(
            "query.reproduction.export",
            &json!({"bundle":valid})
        ));
        let mut wrong = valid.clone();
        wrong["formatVersion"] = json!(99);
        assert!(!bundle(&wrong));
        let mut open = valid.clone();
        open["private"] = json!("secret");
        assert!(!bundle(&open));
        let mut overlong = valid.clone();
        overlong["fixture"]["id"] = json!("x".repeat(3072));
        assert!(!bundle(&overlong));
        assert!(validate_result(
            "command.reproduction.opt_in",
            &json!({
                "state":"recording","reason":null,"commandCount":0,"exportable":false
            })
        ));
        assert!(!validate_result(
            "query.reproduction.status",
            &json!({
                "state":"recording","reason":null,"commandCount":0,"exportable":false,"path":"private"
            })
        ));
    }
}
