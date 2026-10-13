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
    const factory = {factory}, pins = {config};
    const probe = factory.createStartupProbe(pins, {{
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
    // Only provisional JS evidence. Native pending-cut authority and both-
    // producer fencing/retained late evidence are separate runtime leaves.
    let retired = false, failed = false, unlisten = null, cutId = null;
    let drain = null, snapshot = null;
    const uuid = /^[0-9a-f]{{8}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{12}}$/;
    function removeListener() {{
      const stop = unlisten; unlisten = null;
      if (stop) Promise.resolve().then(() => stop()).catch(() => {{ failed = true; }});
    }}
    async function send(command, footer) {{
      if (retired || failed) throw Error('startup exchange unavailable');
      const bytes = new TextEncoder().encode(JSON.stringify(footer));
      if (bytes.byteLength > 1024 || await invoke.call(ipc, command, bytes) !== true)
        throw Error('startup exchange refused');
    }}
    function receive(event) {{
      if (retired || failed) return;
      let run, load, id, phase;
      try {{
        const value = event && event.payload;
        if (!value || typeof value !== 'object' || Array.isArray(value)
          || Object.keys(value).length !== 4
          || !Object.keys(value).every(k => ['runId','loadGeneration','cutId','phase'].includes(k))) return;
        run = value.runId; load = value.loadGeneration; id = value.cutId; phase = value.phase;
        if (run !== pins.runId || load !== pins.loadGeneration || typeof id !== 'string'
          || !uuid.test(id) || !['drain','snapshot'].includes(phase)
          || (cutId !== null && cutId !== id)) return;
      }} catch (_) {{ return; }}
      if (phase === 'drain' && drain === null) {{
        cutId = id;
        drain = probe.beginCut(id).then(footer => send('nemo_test_startup_cut_drained', footer))
          .catch(() => {{ failed = true; }});
      }} else if (phase === 'snapshot' && drain !== null && snapshot === null) {{
        // Read LIVE counters after drain/ACK; its frozen first footer is stale
        // if escaped callbacks arrive. Even this snapshot is not a final seal.
        snapshot = drain.then(() => send('nemo_test_startup_cut_snapshot', probe.cutState()))
          .catch(() => {{ failed = true; }});
      }}
    }}
    Promise.resolve().then(() => {{
      if (retired) return;
      const tauri = win.__TAURI__, events = tauri && tauri.event, listen = events && events.listen;
      if (typeof listen !== 'function') throw Error('startup listener unavailable');
      return listen.call(events, 'nemo-test-startup-cut', receive,
        {{ target: {{ kind: 'WebviewWindow', label: 'main' }} }});
    }}).then(stop => {{
      if (typeof stop !== 'function') {{ if (!retired) failed = true; return; }}
      unlisten = stop; if (retired) removeListener();
    }}).catch(() => {{ failed = true; }});
    return Object.freeze({{ state: probe.state, drained: probe.drained,
      beginCut: probe.beginCut, cutState: probe.cutState,
      dispose() {{ retired = true; removeListener(); return probe.dispose(); }} }});
  }} catch (_) {{ /* Missing/throwing ports remain silent/Partial, never fallback. */ }}
}})(undefined);"#
    )))
}

#[cfg(test)]
#[path = "startup_injection_tests.rs"]
mod tests;
