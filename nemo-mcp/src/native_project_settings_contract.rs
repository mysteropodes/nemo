//! Exact F04 public shape. Values remain owned by native project history.
use super::exact;
use serde_json::Value;
fn finite(value: Option<&Value>) -> bool {
    value.and_then(Value::as_f64).is_some_and(f64::is_finite)
}
fn dimension(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_u64)
        .is_some_and(|v| (1..=u32::MAX as u64).contains(&v))
}
fn background(value: Option<&Value>) -> bool {
    value.and_then(Value::as_str).is_some_and(|s| {
        let digits = s.strip_prefix('#').unwrap_or(s);
        matches!(digits.len(), 3 | 6 | 8) && digits.bytes().all(|b| b.is_ascii_hexdigit())
    })
}
pub(super) fn request(value: &Value) -> bool {
    exact(value, &["command", "action"], &[]).is_some_and(|p| {
        p.get("command").and_then(Value::as_str) == Some("object.project.settings.edit")
            && p.get("action")
                .is_some_and(|a| match a.get("kind").and_then(Value::as_str) {
                    Some("setCanvasSize") => exact(a, &["kind", "width", "height"], &[])
                        .is_some_and(|a| dimension(a.get("width")) && dimension(a.get("height"))),
                    Some("setFps") => {
                        exact(a, &["kind", "fps"], &[]).is_some_and(|a| finite(a.get("fps")))
                    }
                    Some("setCanvasBackground") => exact(a, &["kind", "color"], &[])
                        .is_some_and(|a| background(a.get("color"))),
                    Some("setWorkArea") => exact(a, &["kind", "inFrame", "outFrame"], &[])
                        .is_some_and(|a| finite(a.get("inFrame")) && finite(a.get("outFrame"))),
                    _ => false,
                })
    })
}
pub(super) fn result(value: &Value) -> bool {
    super::object::visibility_result(value)
}
pub(crate) fn revision_matches(expected: Option<u64>, revision: u64, value: &Value) -> bool {
    result(value)
        && expected.and_then(|before| before.checked_add(u64::from(value["applied"] == true)))
            == Some(revision)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::NativeApplicationRequest;
    use serde_json::json;
    fn payload(action: Value) -> Value {
        json!({"command":"object.project.settings.edit","action":action})
    }
    fn envelope(payload: Value) -> NativeApplicationRequest {
        serde_json::from_value(json!({"apiVersion":2,"requestId":"settings","instanceId":"instance","documentId":"document",
        "expectedRevision":7,"operation":"command.document.object.project.settings.edit","payload":payload})).unwrap()
    }
    #[test]
    fn exact_owner_actions_preserve_finite_command_domain_and_storage_absence_boundary() {
        for a in [
            json!({"kind":"setCanvasSize","width":1,"height":4294967295u64}),
            json!({"kind":"setFps","fps":0.5}),
            json!({"kind":"setFps","fps":-5}),
            json!({"kind":"setFps","fps":90}),
            json!({"kind":"setWorkArea","inFrame":45.5,"outFrame":-0.25}),
            json!({"kind":"setCanvasBackground","color":"#aBc"}),
            json!({"kind":"setCanvasBackground","color":"AbC12380"}),
        ] {
            let p = payload(a);
            assert!(request(&p));
            assert!(envelope(p).validate().is_ok());
        }
        // Optional storage fields do not authorize new clear/partial command variants.
        for a in [
            json!({"kind":"setCanvasBackground","color":null}),
            json!({"kind":"setCanvasBackground"}),
            json!({"kind":"setWorkArea","inFrame":1}),
            json!({"kind":"setWorkArea","inFrame":null,"outFrame":1}),
            json!({"kind":"setCanvasSize","width":0,"height":1}),
            json!({"kind":"setCanvasSize","width":1.5,"height":1}),
            json!({"kind":"setCanvasSize","width":4294967296u64,"height":1}),
            json!({"kind":"setFps","fps":"24"}),
            json!({"kind":"setCanvasBackground","color":"#abcd"}),
            json!({"kind":"setCanvasBackground","color":"white"}),
            json!({"kind":"setTotalFrames","totalFrames":12}),
            json!({"kind":"setFps","fps":24,"path":"private"}),
        ] {
            let p = payload(a);
            assert!(!request(&p));
            assert!(envelope(p).validate_input().is_err());
        }
        let mut r = envelope(payload(json!({"kind":"setFps","fps":24})));
        r.expected_revision = None;
        assert!(r.validate_input().is_err());
        let descriptor = crate::capabilities::native_catalog()
            .capability_for_operation(&r.operation)
            .unwrap();
        assert_eq!(descriptor["id"], "native.project-settings");
        assert_eq!(descriptor["authority"]["fallback"], "none");
    }
    #[test]
    fn receipt_has_exact_applied_history_and_revision_correlation() {
        let yes = json!({"applied":true,"historyEntriesAdded":1});
        let no = json!({"applied":false,"historyEntriesAdded":0});
        assert!(revision_matches(Some(7), 8, &yes));
        assert!(revision_matches(Some(7), 7, &no));
        assert!(!revision_matches(Some(7), 7, &yes));
        assert!(!revision_matches(Some(7), 8, &no));
        assert!(!revision_matches(None, 8, &yes));
        assert!(!revision_matches(Some(u64::MAX), 0, &yes));
        for r in [
            json!({"applied":true,"historyEntriesAdded":0}),
            json!({"applied":false,"historyEntriesAdded":1}),
            json!({"applied":true,"historyEntriesAdded":1,"project":{}}),
            json!({"applied":1,"historyEntriesAdded":1}),
        ] {
            assert!(!result(&r));
        }
    }
}
