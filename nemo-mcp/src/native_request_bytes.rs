//! Original-byte native bounds at explicit ingress paths, after duplicate/syntax admission.
#[path = "native_request_spans.rs"]
mod spans;
use spans::member;
use std::io;
fn native(bytes: &[u8]) -> bool {
    member(bytes, "apiVersion").and_then(|v| serde_json::from_slice::<u32>(v).ok()) == Some(2)
        && member(bytes, "operation").is_some()
}
fn string_is(bytes: Option<&[u8]>, expected: &str) -> bool {
    bytes
        .and_then(|v| serde_json::from_slice::<String>(v).ok())
        .is_some_and(|v| v == expected)
}
pub(super) const MAX_REQUEST_BYTES: usize = 65_536;
pub(super) fn request_limit(operation: &str) -> usize {
    if operation == "command.document.object.vector.edit" {
        MAX_REQUEST_BYTES
    } else {
        crate::contract::NATIVE_MAX_MESSAGE_BYTES
    }
}
pub(super) fn check(bytes: &[u8]) -> io::Result<()> {
    // Standalone NativeApplicationRequest, authenticated native TCP envelope, or
    // the one actual MCP native command argument path. Arbitrary nested fields
    // named operation/nativeRequest cannot select a larger outer request budget.
    let direct = native(bytes).then_some(bytes);
    let tcp = member(bytes, "nativeRequest").filter(|v| native(v));
    let stdio = string_is(member(bytes, "method"), "tools/call")
        .then(|| member(bytes, "params"))
        .flatten()
        .filter(|p| string_is(member(p, "name"), "nemo_command"))
        .and_then(|p| member(p, "arguments"))
        .filter(|v| native(v));
    // Check every recognized path, not a first-match switch: extra root fields
    // must not mask a real over-budget tools/call argument or TCP request.
    for request in [direct, tcp, stdio].into_iter().flatten() {
        let operation =
            member(request, "operation").and_then(|v| serde_json::from_slice::<String>(v).ok());
        let limit = request_limit(operation.as_deref().unwrap_or(""));
        if request.len() > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("native request exceeds {limit} original bytes"),
            ));
        }
    }
    Ok(())
}
#[cfg(test)]
#[path = "native_request_bytes_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "native_request_framing_tests.rs"]
mod framing_tests;
#[cfg(test)]
#[path = "native_request_limit_tests.rs"]
mod limit_tests;
