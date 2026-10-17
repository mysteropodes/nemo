//! Frozen ORIGINAL39 substrate controls through real immutable native pins.
//! The case block below is copied byte-for-byte from #1696's A0 source; its
//! original block SHA256 is 321e8402f388e061b301e7f98e4071f0b88f0a2b5b1f98aa71ba85bcba328bc8.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_ordered_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_expression_pin::ExpressionSelector;
use crate::object_expression_runtime::{evaluate_for_owner, ExpressionLimits, ExpressionRefusal};
use crate::object_frame_packet::ObjectFrameSelector;
use serde_json::json;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Case {
    name: &'static str,
    code: String,
    value: f64,
    frame: u32,
    expected: &'static str,
    result: Option<f64>,
}

#[rustfmt::skip]
fn cases()->Vec<Case> {
 let mut v=Vec::new();
 let mut add=|name,code:&str,value,frame,expected,result| v.push(Case{name,code:code.into(),value,frame,expected,result});
 add("expression","value + frame / 5",2.,10,"number",Some(4.));
 add("statement-body","var delta=frame/5;return value+delta;",2.,10,"number",Some(4.));
 add("local-object","const sample={delta:frame/5};return value+sample.delta;",2.,10,"number",Some(4.));
 add("local-function","function plus(x){return x+frame/5;}return plus(value);",2.,10,"number",Some(4.));
 add("finite-loop","let out=value;for(let i=0;i<3;i++){out+=1;}return out;",2.,10,"number",Some(5.));
 add("repeat-expression","value + frame / 5",2.,10,"number",Some(4.));
 add("raw-held-input","value + frame / 5",2.,5,"number",Some(3.));
 add("raw-eased-input","value + frame / 5",4.,5,"number",Some(5.));
 add("frame-zero","value + frame / 5",2.,0,"number",Some(2.));
 add("regexp-positive","/a+b/.test('aaab') ? value+frame/5 : -1",2.,10,"number",Some(4.));
 add("primitive-string","'4'",2.,10,"nonnumeric",None);
 add("primitive-bool","true",2.,10,"nonnumeric",None);
 add("object-return","({value:4})",2.,10,"nonnumeric",None);
 add("array-normalization-gap","[4]",2.,10,"nonnumeric",None);
 add("nonfinite","1/0",2.,10,"nonfinite",None);
 add("unsupported-time-context","value+time",2.,10,"runtime-error",None);
 add("runtime-execute-once","(()=>{throw new Error('runtime-once')})()",2.,10,"runtime-error",None);
 add("syntax-error","var = ;",2.,10,"syntax-error",None);
 add("global-write","globalThis.leaked=9;return 4;",2.,10,"runtime-error",None);
 add("prototype-write","Object.prototype.leaked=9;return 4;",2.,10,"runtime-error",None);
 add("constructor-write","Object.constructor=()=>9;return 4;",2.,10,"runtime-error",None);
 add("array-prototype-write","Array.prototype.leaked=9;return 4;",2.,10,"runtime-error",None);
 add("fresh-no-leak","typeof leaked==='undefined' && !('leaked' in Object.prototype) && !('leaked' in Array.prototype) ? 4 : -1",2.,10,"number",Some(4.));
 add("host-capabilities","[typeof window,typeof document,typeof process,typeof require,typeof fetch,typeof XMLHttpRequest,typeof WebSocket,typeof Deno,typeof Bun].every(x=>x==='undefined') ? 4 : -1",2.,10,"number",Some(4.));
 add("clock-random-async","typeof Date==='undefined' && typeof performance==='undefined' && typeof Math.random==='undefined' && typeof Promise==='undefined' && typeof SharedArrayBuffer==='undefined' && typeof Atomics==='undefined' && typeof queueMicrotask==='undefined' ? 4 : -1",2.,10,"number",Some(4.));
 add("eval-denied","eval('4')",2.,10,"runtime-error",None);
 add("function-constructor-denied","(()=>{}).constructor('return 4')()",2.,10,"runtime-error",None);
 add("generator-constructor-denied","(function*(){}).constructor('yield 4')()",2.,10,"runtime-error",None);
 add("async-constructor-denied","(async function(){}).constructor('return 4')()",2.,10,"runtime-error",None);
 add("async-generator-constructor-denied","(async function*(){}).constructor('yield 4')()",2.,10,"runtime-error",None);
 add("dynamic-import","import('file:///tmp/c2a0-no-host-module')",2.,10,"refusal",None);
 add("infinite-loop","while(true){}",2.,10,"fuel",None);
 add("unbraced-loop","while(true);",2.,10,"fuel",None);
 add("catch-and-resume","while(true){try{while(true){}}catch(e){}}",2.,10,"fuel",None);
 add("recursion","function f(){return f();}return f();",2.,10,"stack",None);
 add("allocation-cap","'a'.repeat(16*1024*1024)",2.,10,"memory",None);
 add("regexp-heavy","/^(a+)+$/.test('a'.repeat(256)+'!')",2.,10,"fuel",None);
 v.push(Case{name:"compile-nesting",code:format!("{}1{}","(".repeat(1900),")".repeat(1900)),value:2.,frame:10,expected:"compile-refusal",result:None});
 v.push(Case{name:"input-limit",code:" ".repeat(4097),value:2.,frame:10,expected:"input-limit",result:None});
 v
}

fn owner(case: &Case) -> Result<NativeObjectHistory, String> {
    let document = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":21,
        "layers":[{"layerUid":"L0"}],
        "objects":[{"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
            "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},"layerUid":"L0","strokeId":"s0"},
            "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
                "segments":[{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":1,"y":0}},
                {"point":{"x":10,"y":10},"handleIn":{"x":0,"y":-1},"handleOut":{"x":0,"y":0}}]},
            "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":0.75}}],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":7},"elements":[
                    {"layerUid":"L0","strokeId":"s0","naturalPosition":0,
                        "order":{"static":[case.value],"expression":{"enabled":true,"code":case.code}}}]}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}});
    NativeObjectHistory::new(
        "expression-runtime-matrix",
        decode_ordered_project(&serde_json::to_vec(&document).unwrap())
            .map_err(|error| error.to_string())?
            .into_storage(),
    )
    .map_err(|error| error.to_string())
}

fn evaluate(case: &Case) -> Result<f64, ExpressionRefusal> {
    let owner = match owner(case) {
        Ok(owner) => owner,
        Err(error) if case.code.len() > 4096 && error.contains("byte budget") => {
            // The real document codec refuses the frozen 4097-byte input before
            // a request can be pinned. This preserves the original input and
            // refusal oracle without forging an impossible native snapshot.
            return Err(ExpressionRefusal::InputLimit);
        }
        Err(error) => panic!("native owner admission failed for {}: {error}", case.name),
    };
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let before = (
        encode_storage_project(snapshot.document()).unwrap(),
        owner.content_revision(),
        owner.history_depths(),
    );
    let selector = ExpressionSelector {
        frame: ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame: case.frame,
        },
        source_frame: 7,
        layer_uid: "L0",
        stroke_id: Some("s0"),
    };
    // These are the accepted scratch matrix's explicit test inputs, not a
    // production limit choice. Each request has a fresh runtime.
    let limits = ExpressionLimits::new(4096, 8_388_608, 262_144, 32).unwrap();
    let result = evaluate_for_owner(&owner, &snapshot, &selector, limits);
    let current = owner.acquire_snapshot(owner.content_revision()).unwrap();
    assert_eq!(
        (
            encode_storage_project(current.document()).unwrap(),
            owner.content_revision(),
            owner.history_depths(),
        ),
        before
    );
    result
}

fn assert_case(case: &Case) {
    let result = evaluate(case);
    match case.expected {
        "number" => assert_eq!(result, Ok(case.result.unwrap()), "{}", case.name),
        "nonnumeric" => assert_eq!(
            result,
            Err(ExpressionRefusal::NonPrimitive),
            "{}",
            case.name
        ),
        "nonfinite" => assert_eq!(result, Err(ExpressionRefusal::NonFinite), "{}", case.name),
        "runtime-error" => assert_eq!(result, Err(ExpressionRefusal::Guest), "{}", case.name),
        "syntax-error" => assert_eq!(result, Err(ExpressionRefusal::Syntax), "{}", case.name),
        "refusal" => assert_eq!(
            result,
            Err(ExpressionRefusal::QueuedAsyncWork),
            "{}",
            case.name
        ),
        "fuel" => assert_eq!(result, Err(ExpressionRefusal::Interrupted), "{}", case.name),
        "stack" => assert_eq!(result, Err(ExpressionRefusal::Stack), "{}", case.name),
        "memory" => assert_eq!(result, Err(ExpressionRefusal::Memory), "{}", case.name),
        "input-limit" => assert_eq!(result, Err(ExpressionRefusal::InputLimit), "{}", case.name),
        "compile-refusal" => assert!(
            matches!(
                result,
                Err(ExpressionRefusal::Stack
                    | ExpressionRefusal::Syntax
                    | ExpressionRefusal::Interrupted
                    | ExpressionRefusal::Memory)
            ),
            "{}: {:?}",
            case.name,
            result
        ),
        other => panic!("unmapped frozen oracle {other}"),
    }
    // The accepted matrix requires an independent fresh-Runtime recovery.
    let recovery = Case {
        name: "fresh-recovery",
        code: "typeof leaked==='undefined' && !('leaked' in Object.prototype) && !('leaked' in Array.prototype) && [typeof process,typeof require,typeof fs,typeof fetch,typeof window,typeof document,typeof Promise,typeof eval,typeof Function,typeof Date,typeof performance,typeof Math.random,typeof queueMicrotask].every(x=>x==='undefined') ? value+frame/5 : -1".into(),
        value: 2.0,
        frame: 10,
        expected: "number",
        result: Some(4.0),
    };
    assert_eq!(evaluate(&recovery), Ok(4.0), "recovery after {}", case.name);
}

#[test]
fn original39_child() {
    let Ok(name) = std::env::var("NEMO_ORIGINAL39_CASE") else {
        return;
    };
    let case = if name == "queued-callback" {
        Case {
            name: "queued-callback",
            code: "(async()=>0)().then(()=>{while(true){}});return value+frame/5;".into(),
            value: 2.0,
            frame: 10,
            expected: "refusal",
            result: None,
        }
    } else {
        cases().into_iter().find(|case| case.name == name).unwrap()
    };
    assert_case(&case);
}

fn supervised_child(name: &str) {
    supervised_test(
        "object_expression_runtime_tests::original39_child",
        "NEMO_ORIGINAL39_CASE",
        name,
    );
}

fn supervised_test(test: &str, env: &str, name: &str) {
    let executable = std::env::current_exe().unwrap();
    let mut child = Command::new(&executable)
        .arg("--exact")
        .arg(test)
        .env(env, name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() >= Duration::from_secs(2) {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("watchdog, never engine PASS: {name}");
        }
        thread::sleep(Duration::from_millis(2));
    };
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .take(8192)
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(status.success(), "frozen case {name} failed: {stderr}");
}

#[test]
fn original39_independent_children_with_watchdog() {
    let cases = cases();
    assert_eq!(cases.len(), 39);
    for case in cases {
        supervised_child(case.name);
    }
}

#[test]
fn queued_infinite_callback_is_refused_without_execution() {
    supervised_child("queued-callback");
}

#[test]
fn private_limits_refuse_zero_or_exceeding_accepted_stage_ceiling() {
    assert!(matches!(
        ExpressionLimits::new(0, 8_388_608, 262_144, 32),
        Err(ExpressionRefusal::InvalidLimits)
    ));
    assert!(matches!(
        ExpressionLimits::new(4097, 8_388_608, 262_144, 32),
        Err(ExpressionRefusal::InvalidLimits)
    ));
    assert!(matches!(
        ExpressionLimits::new(4096, 8_388_609, 262_144, 32),
        Err(ExpressionRefusal::InvalidLimits)
    ));
    assert!(matches!(
        ExpressionLimits::new(4096, 8_388_608, 262_145, 32),
        Err(ExpressionRefusal::InvalidLimits)
    ));
    assert!(matches!(
        ExpressionLimits::new(4096, 8_388_608, 262_144, 33),
        Err(ExpressionRefusal::InvalidLimits)
    ));
}

fn order_case(code: &str) -> Case {
    Case {
        name: "normalized-order",
        code: code.into(),
        value: 2.0,
        frame: 10,
        expected: "number",
        result: None,
    }
}

fn order_selector(snapshot: &crate::revision::ObjectSnapshot) -> ExpressionSelector<'_> {
    ExpressionSelector {
        frame: ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame: 10,
        },
        source_frame: 7,
        layer_uid: "L0",
        stroke_id: Some("s0"),
    }
}

fn normalized_order(code: &str) -> Result<f64, ExpressionRefusal> {
    let owner = owner(&order_case(code)).unwrap();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let before = (
        encode_storage_project(snapshot.document()).unwrap(),
        owner.content_revision(),
        owner.history_depths(),
    );
    let result = crate::object_expression_runtime::evaluate_order_for_owner(
        &owner,
        &snapshot,
        &order_selector(&snapshot),
        ExpressionLimits::new(4096, 8_388_608, 262_144, 32).unwrap(),
    );
    assert_eq!(
        (
            encode_storage_project(owner.acquire_snapshot(0).unwrap().document()).unwrap(),
            owner.content_revision(),
            owner.history_depths(),
        ),
        before
    );
    result
}

#[test]
fn order_array_normalization_matches_finite_first_component_and_signed_zero() {
    for code in ["4", "[4]", "[4,99]", "[4,Infinity]", "[4,NaN]"] {
        assert_eq!(normalized_order(code), Ok(4.0), "{code}");
    }
    for code in ["-0", "[-0]"] {
        assert_eq!(
            normalized_order(code).unwrap().to_bits(),
            (-0.0f64).to_bits()
        );
    }
    assert_eq!(
        evaluate(&order_case("[4]")),
        Err(ExpressionRefusal::NonPrimitive)
    );
    assert_eq!(evaluate(&order_case("4")), Ok(4.0));
}

#[test]
fn order_normalization_rejects_invalid_shapes_without_coercion() {
    for code in [
        "[]",
        "new Array(1)",
        "[undefined]",
        "[null]",
        "['4']",
        "[true]",
        "[{}]",
        "[NaN]",
        "[Infinity]",
        "[-Infinity]",
        "[1n]",
        "[Symbol('x')]",
        "'4'",
        "true",
        "({0:4,length:1})",
        "new Number(4)",
    ] {
        assert_eq!(
            normalized_order(code),
            Err(ExpressionRefusal::NonPrimitive),
            "{code}"
        );
    }
    for code in ["NaN", "Infinity", "-Infinity"] {
        assert_eq!(normalized_order(code), Err(ExpressionRefusal::NonFinite));
    }
    // Typed-array intrinsics remain unavailable in this accepted runtime.
    assert_eq!(
        normalized_order("new Float64Array([4])"),
        Err(ExpressionRefusal::Guest)
    );
    assert_eq!(normalized_order("[value+frame/5]"), Ok(4.0));
}

#[test]
fn order_normalization_preserves_slice_and_getter_behavior() {
    let getter = "let n=0;let a=[];Object.defineProperty(a,'0',{get:()=>++n});return a;";
    assert_eq!(normalized_order(getter), Ok(1.0)); // slice reads the original getter once
    let slice = "let a=[99];Object.defineProperty(a,'slice',{value:()=>[5]});return a;";
    assert_eq!(normalized_order(slice), Ok(5.0));
    for code in [
        "let a=[];Object.defineProperty(a,'0',{get:()=>{throw new Error('getter');}});return a;",
        "let a=[4];Object.defineProperty(a,'slice',{get:()=>{throw new Error('slice');}});return a;",
    ] {
        assert_eq!(normalized_order(code), Err(ExpressionRefusal::Guest));
        assert_eq!(normalized_order("[value+frame/5]"), Ok(4.0));
    }
    let bad_slice = "let a=[4];Object.defineProperty(a,'slice',{value:()=>[5,Infinity]});return a;";
    assert_eq!(
        normalized_order(bad_slice),
        Err(ExpressionRefusal::NonPrimitive)
    );
}

#[test]
fn order_normalization_bounded_child() {
    let Ok(case) = std::env::var("NEMO_NORMALIZED_ORDER_CASE") else {
        return;
    };
    let (code, refusal) = match case.as_str() {
        "loop" => (
            "let a=[];Object.defineProperty(a,'0',{get:()=>{while(true){}}});return a;",
            ExpressionRefusal::Interrupted,
        ),
        "stack" => (
            "let a=[];Object.defineProperty(a,'0',{get:function f(){return f();}});return a;",
            ExpressionRefusal::Stack,
        ),
        "memory" => (
            "let a=[];Object.defineProperty(a,'0',{get:()=> 'a'.repeat(16*1024*1024)});return a;",
            ExpressionRefusal::Memory,
        ),
        "queue" => (
            "(async()=>0)().then(()=>{while(true){}});return [4];",
            ExpressionRefusal::QueuedAsyncWork,
        ),
        "import" => (
            "import('file:///tmp/normalized-order-no-loader');return [4];",
            ExpressionRefusal::QueuedAsyncWork,
        ),
        other => panic!("unknown bounded control {other}"),
    };
    assert_eq!(normalized_order(code), Err(refusal));
    assert_eq!(normalized_order("[value+frame/5]"), Ok(4.0));
}

#[test]
fn order_normalization_limits_and_queue_have_independent_watchdogs() {
    for case in ["loop", "stack", "memory", "queue", "import"] {
        supervised_test(
            "object_expression_runtime_tests::order_normalization_bounded_child",
            "NEMO_NORMALIZED_ORDER_CASE",
            case,
        );
    }
}

#[test]
fn normalized_detached_candidate_rechecks_real_owner_after_runtime_retirement() {
    use crate::object_expression_pin::prepare_expression_request;
    use crate::object_expression_runtime::evaluate_order_detached;
    use crate::object_frame_packet::ObjectFrameError;
    let mut owner = owner(&order_case("[4]")).unwrap();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let limits = ExpressionLimits::new(4096, 8_388_608, 262_144, 32).unwrap();
    let request =
        prepare_expression_request(&owner, &snapshot, &order_selector(&snapshot)).unwrap();
    let candidate = evaluate_order_detached(&request, limits).unwrap();
    let payload = json!({"command":"object.fill.set", "stableTarget":{
        "contextId":"scene-root", "frameScope":{"kind":"authored","frame":7},
        "layerUid":"L0", "strokeId":"s0"}, "fill":{"kind":"solid","r":0.5,"g":0.1,"b":0.3,"a":0.5}});
    let fill = owner
        .prepare_fill_json(&serde_json::to_vec(&payload).unwrap())
        .unwrap();
    assert_eq!(owner.commit_fill(fill), Ok(1));
    let after = encode_storage_project(owner.acquire_snapshot(1).unwrap().document()).unwrap();
    assert_eq!(
        request.finalize(&owner, candidate),
        Err(ExpressionRefusal::Pin(ObjectFrameError::WrongRevision))
    );
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(owner.history_depths(), (1, 0));
    let current = owner.acquire_snapshot(1).unwrap();
    assert_eq!(encode_storage_project(current.document()).unwrap(), after);
    assert_eq!(
        crate::object_expression_runtime::evaluate_order_for_owner(
            &owner,
            &current,
            &order_selector(&current),
            limits
        ),
        Ok(4.0)
    );
}

#[test]
fn wrapper_breakout_is_a_syntax_refusal_on_both_entries() {
    // #2088: ending the function early leaves the wrapper tail, a SyntaxError.
    let syntax = cases().into_iter().find(|c| c.expected == "syntax-error");
    for code in [
        "1);})(), (function(){while(true){}})(), (function(){return (1",
        "1);})(), (function(){throw new Error('x')})(), (function(){return (1",
        "1\n}); while (true) {} (function(){",
        "1);} /*",
        "1);} //",
        "1);}, (function(){while(true){}})(), function(){return (1",
    ] {
        assert_case(&Case {
            name: "wrapper-breakout",
            code: code.into(),
            ..syntax.clone().unwrap()
        });
        let order = normalized_order(code);
        assert_eq!(order, Err(ExpressionRefusal::Syntax), "{code:?}");
    }
}
