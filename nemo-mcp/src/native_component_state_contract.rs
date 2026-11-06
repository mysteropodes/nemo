//! C-comp-owned-state: the member kinds of `command.document.object.component.state.edit` (the camera keys and markers a declared
//! component owns). Pure shape checks; the native owner repeats every rule and judges the combination of members per kind.
//! Mirrored by `src/js/adapters/native-object-component-contract.js`.
use serde_json::Value;

const KINDS: [&str; 8] = [
    "setCameraKey",
    "removeCameraKey",
    "setCameraEase",
    "clearCamera",
    "addMarker",
    "updateMarker",
    "removeMarker",
    "clearMarkers",
];
const TOKENS: [&str; 5] = ["stateKind", "pair", "quad", "label", "colour"];
const MAX_LABEL: usize = 256;

pub(super) fn admits(token: &str) -> bool {
    TOKENS.contains(&token)
}
/// The static token a spec row names (rows are `'static`, kinds keep the token).
pub(super) fn token(token: &str) -> &'static str {
    TOKENS.iter().find(|t| **t == token).copied().unwrap_or("")
}

fn numbers(value: &Value, count: usize) -> bool {
    value.as_array().is_some_and(|items| {
        items.len() == count && items.iter().all(|n| n.as_f64().is_some_and(f64::is_finite))
    })
}
fn label(value: &Value) -> Option<&str> {
    value
        .as_str()
        .filter(|text| text.len() <= MAX_LABEL && !text.contains('\0'))
}

pub(super) fn check(token: &str, value: &Value) -> bool {
    match token {
        "stateKind" => value.as_str().is_some_and(|kind| KINDS.contains(&kind)),
        "pair" => numbers(value, 2),
        // A cubic-bezier ease: x1 and x2 lie in 0..1.
        "quad" => {
            numbers(value, 4)
                && [0usize, 2]
                    .iter()
                    .all(|at| value[*at].as_f64().is_some_and(|x| (0.0..=1.0).contains(&x)))
        }
        "label" => label(value).is_some(),
        "colour" => label(value).is_some_and(|text| !text.is_empty()),
        _ => false,
    }
}
