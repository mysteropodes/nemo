use super::*;
use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

struct Fixture {
    root: PathBuf,
    native: LoadLog,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("nemo-injection-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let file = std::fs::File::create(root.join("native.json")).unwrap();
        Self {
            root,
            native: LoadLog::new(
                uuid::Uuid::new_v4().to_string(),
                "a".repeat(64),
                std::process::id(),
                file,
            ),
        }
    }
    fn script(&self) -> String {
        build_script(Some(&self.native), true, true)
            .unwrap()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

fn run(script: &str, mode: &str) -> Value {
    // No alternate probe implementation: execute the ACTUAL generated source
    // in a VM with narrow fixture ports and actual TextEncoder/Uint8Array.
    let program = r#"const fs=require('fs'),vm=require('vm'),assert=require('assert');
const input=JSON.parse(fs.readFileSync(0,'utf8')),mode=input.mode,calls=[],actions=[],cancelled=[];
function target(){const listeners=new Map();return {listeners,
addEventListener(n,f){if(!listeners.has(n))listeners.set(n,new Set);listeners.get(n).add(f)},
removeEventListener(n,f){listeners.get(n)?.delete(f)},
dispatch(n,e){for(const f of [...(listeners.get(n)||[])])f(e)}}}
const doc=Object.assign(target(),{readyState:'complete',getElementById:()=>null});
const frames=new Map();let id=0;
const win=Object.assign(target(),{document:doc,location:{href:'tauri://localhost/index.html'},
getComputedStyle:()=>({display:'block',visibility:'visible',opacity:'1'}),
requestAnimationFrame:f=>{frames.set(++id,f);return id},cancelAnimationFrame:id=>{cancelled.push(id);frames.delete(id)}});
win.top=win;
const urls={foreign:'https://private.example/secret',query:'tauri://localhost/index.html?secret=1',
fragment:'tauri://localhost/index.html#secret',blank:'about:blank',credentials:'tauri://private@localhost/index.html',
port:'tauri://localhost:99/index.html',path:'tauri://localhost/private'};
if(urls[mode])win.location.href=urls[mode];
if(mode==='subframe')win.top={};
if(mode==='throw-top')Object.defineProperty(win,'top',{get(){throw Error('private-top')}});
if(mode==='synthetic')doc.readyState='loading';
win.__TAURI_INTERNALS__={invoke(command,body){
calls.push({command,raw:ArrayBuffer.isView(body),length:body.byteLength,event:JSON.parse(new TextDecoder().decode(body))});
if(mode==='reject')return Promise.reject(Error('private-rejection'));
if(mode==='invoke-throw')throw Error('private-invoke');
return Promise.resolve(mode==='false'?false:mode==='object-ack'?{accepted:true}:true)}};
if(mode==='missing')delete win.__TAURI_INTERNALS__;
if(mode==='throw-port')Object.defineProperty(win,'__TAURI_INTERNALS__',{get(){throw Error('private-port')}});
class OversizeEncoder{encode(){return new Uint8Array(1025)}}
const module={exports:{untouched:true}};
const context={window:win,document:doc,URL,TextEncoder:mode==='oversize'?OversizeEncoder:TextEncoder,module,
console:new Proxy({},{get:()=>()=>actions.push('console')})};
const before=Object.getOwnPropertyDescriptors(win),globalBefore=Object.keys(context);
const probe=vm.runInNewContext(input.script,context);
(async()=>{
if(probe)await probe.drained();
if(mode==='synthetic'){doc.dispatch('DOMContentLoaded',{isTrusted:false});if(probe)await probe.drained()}
let late;
if(mode==='dispose'&&probe){late=[...frames.values()][0];probe.dispose();if(late)late()}
for(let n=0;n<2;n++){const item=frames.entries().next().value;if(!item)break;frames.delete(item[0]);item[1]()}
if(probe)await probe.drained();
const state=probe?probe.state():null;
if(probe)probe.dispose();
assert.deepStrictEqual(Object.getOwnPropertyDescriptors(win),before);
assert.deepStrictEqual(Object.keys(context),globalBefore);assert.deepStrictEqual(module.exports,{untouched:true});
const listeners=[...win.listeners.values(),...doc.listeners.values()].reduce((n,s)=>n+s.size,0);
process.stdout.write(JSON.stringify({calls,state,actions,frames:frames.size,listeners,cancelled}));
})().catch(()=>{process.exitCode=1});"#;
    let mut child = Command::new("node")
        .args(["-e", program])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let bytes = serde_json::to_vec(&json!({"script":script,"mode":mode})).unwrap();
    assert!(bytes.len() < 32 * 1024);
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "offline VM failed");
    assert!(output.stdout.len() < 32 * 1024);
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn generated_accepted_factory_uses_native_boot_raw_utf8_fixed_command_and_original_sequences() {
    let f = Fixture::new();
    let result = run(&f.script(), "valid");
    let calls = result["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 5);
    let mut validator = super::super::js::JsStream::from_native(&f.native).unwrap();
    for (index, call) in calls.iter().enumerate() {
        assert_eq!(call["command"], "nemo_test_startup_record");
        assert_eq!(call["raw"], true);
        assert!(call["length"].as_u64().unwrap() <= 1024);
        assert_eq!(call["event"]["runId"], f.native.run);
        assert_eq!(call["event"]["loadGeneration"], 1);
        assert_eq!(call["event"]["sequence"], index + 1);
        validator
            .accept_json(
                "main",
                &tauri::Url::parse("tauri://localhost/index.html").unwrap(),
                std::process::id(),
                &serde_json::to_vec(&call["event"]).unwrap(),
            )
            .unwrap();
    }
    assert_eq!(validator.projection()["status"], "provisional");
    assert_eq!(result["state"]["sent"], 5);
    assert_eq!(result["state"]["rafAcks"], 2);
    assert_eq!(result["state"]["visiblePaint"], "not-observed");
    assert_eq!(result["frames"], 0);
    assert_eq!(result["listeners"], 0);
    assert_eq!(result["actions"], json!([]));
}

#[test]
fn builder_default_debug_shared_and_refused_native_are_not_scripts() {
    let mut f = Fixture::new();
    assert_eq!(build_script(Some(&f.native), false, true), Ok(None));
    assert_eq!(build_script(Some(&f.native), true, false), Ok(None));
    assert_eq!(build_script(None, true, true), Err(BuildError::NativeBoot));
    f.native.refused = true;
    assert_eq!(
        build_script(Some(&f.native), true, true),
        Err(BuildError::NativeBoot)
    );
    f.native.refused = false;
    f.native.run = "private-unowned-run".into();
    assert_eq!(
        build_script(Some(&f.native), true, true),
        Err(BuildError::NativeBoot)
    );
}

#[test]
fn actual_script_refuses_foreign_query_subframe_blank_and_missing_throwing_ports() {
    let f = Fixture::new();
    let script = f.script();
    for mode in [
        "foreign",
        "query",
        "fragment",
        "blank",
        "credentials",
        "port",
        "path",
        "subframe",
        "throw-top",
        "missing",
        "throw-port",
    ] {
        let result = run(&script, mode);
        assert_eq!(result["calls"], json!([]), "{mode}");
        assert_eq!(result["state"], Value::Null);
        assert_eq!(result["listeners"], 0);
        assert_eq!(result["frames"], 0);
        assert_eq!(result["actions"], json!([]));
        assert!(!result.to_string().contains("private"));
    }
}

#[test]
fn strict_ack_false_object_rejection_and_oversize_bytes_never_claim_delivery_or_raf() {
    let f = Fixture::new();
    let script = f.script();
    for mode in ["false", "object-ack", "reject", "invoke-throw", "oversize"] {
        let result = run(&script, mode);
        assert_eq!(result["state"]["sent"], 0);
        assert_eq!(result["state"]["refused"], true);
        assert_eq!(result["state"]["rafAcks"], 0);
        assert!(result["state"]["failed"].as_u64().unwrap() > 0);
        assert_eq!(result["frames"], 0);
        assert_eq!(result["listeners"], 0);
        assert!(!result.to_string().contains("private"));
        if mode == "oversize" {
            assert_eq!(result["calls"], json!([]));
        }
    }
}

#[test]
fn real_generated_probe_handles_synthetic_readiness_and_late_owned_disposal_without_global_handle()
{
    let f = Fixture::new();
    let script = f.script();
    let synthetic = run(&script, "synthetic");
    assert_eq!(synthetic["state"]["domEvent"], false);
    assert_eq!(synthetic["state"]["refused"], true);
    let disposed = run(&script, "dispose");
    assert_eq!(disposed["state"]["disposed"], true);
    assert!(disposed["state"]["late"].as_u64().unwrap() > 0);
    assert_eq!(disposed["state"]["rafAcks"], 0);
    assert_eq!(disposed["cancelled"].as_array().unwrap().len(), 1);
    assert_eq!(disposed["listeners"], 0);
    assert_eq!(disposed["frames"], 0);
}
