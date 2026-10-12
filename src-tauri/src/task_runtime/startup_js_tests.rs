use super::*;
use std::path::PathBuf;

struct Fixture {
    root: PathBuf,
    native: LoadLog,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nemo-startup-js-{}", uuid::Uuid::new_v4()));
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
    fn stream(&self) -> JsStream {
        JsStream::from_native(&self.native).unwrap()
    }
    fn event(&self, sequence: u32, stage: &str) -> Value {
        json!({"schema":"nemo.startup-js/1", "runId":self.native.run,
            "loadGeneration":1, "sequence":sequence, "stage":stage})
    }
    fn accept(&self, stream: &mut JsStream, event: &Value) -> Result<(), Refusal> {
        stream.accept_json(
            "main",
            &origin(),
            std::process::id(),
            &serde_json::to_vec(event).unwrap(),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
fn origin() -> tauri::Url {
    tauri::Url::parse("tauri://localhost/index.html").unwrap()
}
fn retained(stream: &JsStream) -> Value {
    stream.projection()["records"].clone()
}

#[test]
fn actual_s2a_factory_transcript_is_provisional_until_native_started_matches() {
    let mut f = Fixture::new();
    let mut stream = f.stream();
    let helper = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests/desktop/startup-observer-bootstrap.js");
    let script = r#"const {createStartupProbe}=require(process.argv[1]);
const doc={readyState:'complete',getElementById:()=>null,addEventListener(){},removeEventListener(){}};
const win={document:doc,addEventListener(){},removeEventListener(){}};win.top=win;
const frames=new Map(),events=[];let id=0;
const probe=createStartupProbe({enabled:true,runId:process.argv[2],loadGeneration:1},
{window:win,document:doc,emit:e=>{events.push(e);return true},getComputedStyle:()=>({}),
requestAnimationFrame:f=>{frames.set(++id,f);return id},cancelAnimationFrame:id=>frames.delete(id)});
for(let n=0;n<2;n++){const [key,fn]=frames.entries().next().value;frames.delete(key);fn()}
probe.drained().then(()=>{probe.dispose();process.stdout.write(JSON.stringify(events))});"#;
    let output = std::process::Command::new("node")
        .args(["-e", script])
        .arg(helper)
        .arg(&f.native.run)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.len() < MAX_BYTES * MAX_EVENTS);
    let events: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(events.len(), 5);
    for event in events {
        f.accept(&mut stream, &event).unwrap();
    }
    assert_eq!(stream.projection()["status"], "provisional");
    assert_eq!(stream.projection()["accepted"], 5);
    assert_eq!(stream.projection()["loadGeneration"], 1);
    let run = f.native.run.clone();
    f.native.confirm_creation("main", std::process::id(), &run);
    f.native.observe(
        "native-page-started",
        "main",
        &origin(),
        std::process::id(),
        &run,
    );
    stream.reconcile_native(&f.native).unwrap();
    assert_eq!(stream.projection()["status"], "native-start-correlated");
    assert_eq!(stream.projection()["sealed"], false);
    assert_eq!(stream.projection()["visiblePaint"], "not-observed");
}

#[test]
fn native_context_constructor_refuses_unowned_or_malformed_boot_values() {
    for field in 0..5 {
        let mut f = Fixture::new();
        match field {
            0 => f.native.run = "private-run".into(),
            1 => f.native.task_key = "private-key".into(),
            2 => f.native.pid = 0,
            3 => f.native.refused = true,
            _ => f.native.phase = 4,
        }
        assert!(matches!(
            JsStream::from_native(&f.native),
            Err(Refusal::Native)
        ));
    }
}

#[test]
fn native_sender_origin_label_pid_are_not_json_claims() {
    for variant in 0..5 {
        let f = Fixture::new();
        let mut stream = f.stream();
        let event = f.event(1, "early-script");
        let label = if variant == 0 { "foreign" } else { "main" };
        let url = match variant {
            1 => "https://localhost/index.html",
            2 => "about:blank",
            3 => "tauri://localhost/index.html?private=secret",
            _ => "tauri://localhost/index.html",
        };
        let pid = if variant == 4 {
            std::process::id() + 1
        } else {
            std::process::id()
        };
        assert_eq!(
            stream.accept_json(
                label,
                &tauri::Url::parse(url).unwrap(),
                pid,
                &serde_json::to_vec(&event).unwrap()
            ),
            Err(Refusal::Sender)
        );
        assert_eq!(retained(&stream), json!([]));
        assert!(!stream.projection().to_string().contains("secret"));
    }
}

#[test]
fn identity_sequence_extra_null_private_wrong_type_and_duplicate_fields_are_atomic() {
    for variant in 0..10 {
        let f = Fixture::new();
        let mut stream = f.stream();
        let mut event = f.event(1, "early-script");
        match variant {
            0 => event["runId"] = json!("private-run"),
            1 => event["loadGeneration"] = json!(2),
            2 => event["schema"] = json!("private-schema"),
            3 => event["sequence"] = json!(2),
            4 => event["sequence"] = json!(1.5),
            5 => event["top"] = json!(true),
            6 => event["message"] = json!("private-message"),
            7 => event["category"] = Value::Null,
            8 => event["sequence"] = json!("1"),
            _ => event["stage"] = json!("private-stage"),
        }
        assert!(f.accept(&mut stream, &event).is_err());
        assert_eq!(retained(&stream), json!([]));
        assert_eq!(stream.native_phase, 0);
        assert!(!stream.projection().to_string().contains("private"));
        f.accept(&mut stream, &f.event(1, "early-script")).unwrap();
        assert_eq!(stream.projection()["accepted"], 1);
    }
    let f = Fixture::new();
    let mut stream = f.stream();
    let wire = serde_json::to_string(&f.event(1, "early-script")).unwrap();
    let duplicate = wire.replacen('{', "{\"sequence\":99,", 1);
    assert_eq!(
        stream.accept_json("main", &origin(), std::process::id(), duplicate.as_bytes()),
        Err(Refusal::Payload)
    );
    assert_eq!(retained(&stream), json!([]));
}

#[test]
fn stage_and_ordinal_mismatches_do_not_advance_retained_progress() {
    let f = Fixture::new();
    let mut stream = f.stream();
    f.accept(&mut stream, &f.event(1, "early-script")).unwrap();
    let before = retained(&stream);
    for stage in ["early-script", "window-load", "startup-control", "raf-ack"] {
        assert!(f.accept(&mut stream, &f.event(2, stage)).is_err());
        assert_eq!(retained(&stream), before);
    }
    f.accept(&mut stream, &f.event(2, "dom-ready")).unwrap();
    let mut control = f.event(3, "startup-control");
    control["control"] = json!("start-open");
    control["visibility"] = json!("visible");
    f.accept(&mut stream, &control).unwrap();
    for bad in [0, 2, 3, u32::MAX] {
        let mut raf = f.event(4, "raf-ack");
        raf["ordinal"] = json!(bad);
        assert_eq!(f.accept(&mut stream, &raf), Err(Refusal::Stage));
    }
    for ordinal in [1, 2] {
        let mut raf = f.event(3 + ordinal, "raf-ack");
        raf["ordinal"] = json!(ordinal);
        f.accept(&mut stream, &raf).unwrap();
    }
    let mut third = f.event(6, "raf-ack");
    third["ordinal"] = json!(3);
    assert_eq!(f.accept(&mut stream, &third), Err(Refusal::Stage));
}

#[test]
fn fixed_error_categories_and_unknown_control_cannot_become_ready_or_raw_data() {
    let f = Fixture::new();
    let mut stream = f.stream();
    f.accept(&mut stream, &f.event(1, "early-script")).unwrap();
    let mut error = f.event(2, "js-error");
    error["category"] = json!("private-error");
    assert_eq!(f.accept(&mut stream, &error), Err(Refusal::Stage));
    error["category"] = json!("unknown");
    f.accept(&mut stream, &error).unwrap();
    let mut ready = f.event(3, "ready-state-observed");
    ready["readyState"] = json!("complete");
    f.accept(&mut stream, &ready).unwrap();
    let mut control = f.event(4, "startup-control");
    control["control"] = json!("start-open");
    control["visibility"] = json!("unknown");
    f.accept(&mut stream, &control).unwrap();
    let mut raf = f.event(5, "raf-ack");
    raf["ordinal"] = json!(1);
    assert_eq!(f.accept(&mut stream, &raf), Err(Refusal::Stage));
    let mut refused = f.event(5, "probe-refused");
    refused["reason"] = json!("unsupported-document");
    f.accept(&mut stream, &refused).unwrap();
    assert!(f.accept(&mut stream, &f.event(6, "dom-ready")).is_err());
    assert_eq!(stream.projection()["probeRefused"], true);
    assert!(!stream.projection().to_string().contains("private-error"));
}

#[test]
fn event_byte_bounds_and_rejection_counters_are_bounded() {
    let f = Fixture::new();
    let mut stream = f.stream();
    for bytes in [
        vec![],
        vec![b'x'; MAX_BYTES + 1],
        b"[]".to_vec(),
        b"{broken".to_vec(),
    ] {
        assert_eq!(
            stream.accept_json("main", &origin(), std::process::id(), &bytes),
            Err(Refusal::Payload)
        );
    }
    f.accept(&mut stream, &f.event(1, "early-script")).unwrap();
    for sequence in 2..=15 {
        let mut error = f.event(sequence, "js-error");
        error["category"] = json!("TypeError");
        f.accept(&mut stream, &error).unwrap();
    }
    for _ in 0..100 {
        assert_eq!(
            f.accept(&mut stream, &f.event(16, "dom-ready")),
            Err(Refusal::Limit)
        );
    }
    assert_eq!(stream.projection()["accepted"], 15);
    assert_eq!(stream.projection()["rejected"], 15);
    assert_eq!(stream.projection()["rejectionOverflow"], true);
    assert!(stream.projection().to_string().len() < MAX_BYTES * MAX_EVENTS);
}

#[test]
fn navigation_replacement_close_and_native_mismatch_fence_irreversibly() {
    for reason in [Fence::Navigation, Fence::Replacement, Fence::Closed] {
        let f = Fixture::new();
        let mut stream = f.stream();
        f.accept(&mut stream, &f.event(1, "early-script")).unwrap();
        let before = retained(&stream);
        stream.fence(reason);
        stream.fence(Fence::Closed);
        assert_eq!(stream.fence, Some(reason));
        assert_eq!(stream.reconcile_native(&f.native), Err(Refusal::Fenced));
        assert_eq!(
            f.accept(&mut stream, &f.event(2, "dom-ready")),
            Err(Refusal::Fenced)
        );
        assert_eq!(retained(&stream), before);
    }
    for field in 0..5 {
        let mut f = Fixture::new();
        let mut stream = f.stream();
        match field {
            0 => f.native.run = uuid::Uuid::new_v4().to_string(),
            1 => f.native.task_key = "b".repeat(64),
            2 => f.native.pid += 1,
            3 => f.native.refused = true,
            _ => {
                stream.native_phase = 2;
                f.native.phase = 1;
            }
        }
        assert_eq!(stream.reconcile_native(&f.native), Err(Refusal::Native));
        assert_eq!(stream.fence, Some(Fence::NativeMismatch));
    }
}
