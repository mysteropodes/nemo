//! Pure generated source only: no builder attachment, host command or app run.
use super::{js::JsStream, LoadLog};
use serde_json::json;

const PROBE: &str = include_str!("../../../tests/desktop/startup-observer-bootstrap.js");
#[derive(Debug, PartialEq, Eq)]
pub(super) enum BuildError {
    NativeBoot,
}

pub(super) fn build_script(
    native: Option<&LoadLog>,
    enabled: bool,
    release: bool,
) -> Result<Option<String>, BuildError> {
    if !enabled || !release {
        return Ok(None);
    }
    let boot = JsStream::from_native(native.ok_or(BuildError::NativeBoot)?)
        .map_err(|_| BuildError::NativeBoot)?
        .projection();
    let config =
        json!({"enabled":true, "runId":boot["runId"], "loadGeneration":boot["loadGeneration"]});
    // The accepted expression is embedded verbatim apart from its terminal
    // statement delimiter. Lexical module masking prevents page CJS exports.
    let factory = PROBE.trim_end().trim_end_matches(';');
    Ok(Some(format!(
        r#"(function(module) {{
  'use strict';
  try {{
    const win = window, doc = document;
    if (win.top !== win || win.document !== doc) return;
    const url = new URL(win.location.href);
    if (url.protocol !== 'tauri:' || url.hostname !== 'localhost' || url.port !== ''
      || url.username !== '' || url.password !== '' || url.search !== '' || url.hash !== ''
      || !['', '/', '/index.html'].includes(url.pathname)) return;
    const ipc = win.__TAURI_INTERNALS__, invoke = ipc && ipc.invoke;
    if (typeof invoke !== 'function') return;
    const factory = {factory};
    return factory.createStartupProbe({config}, {{
      window: win, document: doc,
      emit(event) {{
        const bytes = new TextEncoder().encode(JSON.stringify(event));
        if (bytes.byteLength > 1024) return false;
        return Promise.resolve(invoke.call(ipc, 'nemo_test_startup_record', bytes)).then(ack => ack === true);
      }},
      getComputedStyle: node => win.getComputedStyle(node),
      requestAnimationFrame: callback => win.requestAnimationFrame(callback),
      cancelAnimationFrame: id => win.cancelAnimationFrame(id)
    }});
  }} catch (_) {{ /* Missing/throwing ports remain silent/Partial, never fallback. */ }}
}})(undefined);"#
    )))
}

#[cfg(test)]
#[path = "startup_injection_tests.rs"]
mod tests;
