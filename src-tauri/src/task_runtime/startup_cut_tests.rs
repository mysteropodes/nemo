use super::*;
use serde_json::json;
use std::{
    io::Write,
    process::{Command, Stdio},
};

struct Fixture {
    path: std::path::PathBuf,
    native: LoadLog,
    js: JsStream,
}
impl Fixture {
    fn new(loaded: bool) -> Self {
        let path = std::env::temp_dir().join(format!("nemo-cut-{}", uuid::Uuid::new_v4()));
        let file = std::fs::File::create(&path).unwrap();
        let run = uuid::Uuid::new_v4().to_string();
        let mut native = LoadLog::new(run, "a".repeat(64), std::process::id(), file);
        if loaded {
            let run = native.run.clone();
            native.confirm_creation("main", native.pid, &run);
            let url = tauri::Url::parse("tauri://localhost/index.html").unwrap();
            native.observe("native-page-started", "main", &url, native.pid, &run);
            native.observe("native-page-finished", "main", &url, native.pid, &run);
        }
        let js = JsStream::from_native(&native).unwrap();
        Self { path, native, js }
    }
    fn health(&self) -> RetentionEvidence {
        RetentionEvidence {
            run: self.native.run.clone(),
            task: self.native.task_key.clone(),
            pid: self.native.pid,
            version: 1,
        }
    }
    fn cut(&self) -> Cut {
        Cut::request(&self.native, &self.js, Some(&self.health())).unwrap()
    }
    fn admit(&mut self, records: &Value) {
        let url = tauri::Url::parse("tauri://localhost/index.html").unwrap();
        for record in records.as_array().unwrap() {
            let bytes = serde_json::to_vec(record).unwrap();
            self.js
                .accept_json("main", &url, self.native.pid, &bytes)
                .unwrap();
        }
    }
    fn finish(&self, cut: &mut Cut, footer: &Value) -> Result<Phase, &'static str> {
        let bytes = serde_json::to_vec(footer).unwrap();
        cut.finish(&bytes, &self.native, &self.js, Some(&self.health()))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn actual_kj(run: &str, cut: &str, mode: &str) -> Value {
    // Executes the actual accepted KJ factory; no alternate footer generator.
    let program = r#"
const fs=require('fs'),v=JSON.parse(fs.readFileSync(0,'utf8'));
const {createStartupProbe}=require(v.source), records=[], handlers=new Map(), frames=new Map();let id=0;
const target=()=>({addEventListener(n,f){handlers.set(n,f)},removeEventListener(n){handlers.delete(n)}});
const document=Object.assign(target(),{readyState:'complete',getElementById:()=>null});
const window=Object.assign(target(),{document});window.top=window;
const probe=createStartupProbe({enabled:true,runId:v.run,loadGeneration:1},{window,document,
emit:e=>{records.push(e);return v.mode==='failed'?false:true},getComputedStyle:()=>({}),
requestAnimationFrame:f=>{frames.set(++id,f);return id},cancelAnimationFrame:i=>frames.delete(i)});
if(v.mode==='error')handlers.get('error')({isTrusted:true,error:Error('private-payload')});
if(v.mode!=='no-raf'){for(let i=0;i<2;i++){const [id,f]=frames.entries().next().value;frames.delete(id);f()}}
(async()=>{const footer=await probe.beginCut(v.cut);process.stdout.write(JSON.stringify({footer,records}))})();
"#;
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/desktop/startup-observer-bootstrap.js");
    let mut child = Command::new("node")
        .args(["-e", program])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let payload =
        serde_json::to_vec(&json!({"source":source,"run":run,"cut":cut,"mode":mode})).unwrap();
    child.stdin.take().unwrap().write_all(&payload).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn actual_kj_footer_binds_native_prefix_and_exact_duplicate_is_idempotent() {
    let mut f = Fixture::new(true);
    let mut cut = f.cut();
    assert_eq!(cut.phase(), Phase::Requested);
    assert!(uuid(cut.id()));
    assert_ne!(cut.id(), f.native.run);
    assert_eq!(cut.begin_drain(), Ok(()));
    assert_eq!(cut.begin_drain(), Ok(()));
    let output = actual_kj(&f.native.run, cut.id(), "clean");
    f.admit(&output["records"]);
    assert_eq!(output["footer"]["generated"], 5);
    assert_eq!(
        f.finish(&mut cut, &output["footer"]),
        Ok(Phase::SealedObserved)
    );
    assert_eq!(
        f.finish(&mut cut, &output["footer"]),
        Ok(Phase::SealedObserved)
    );
    assert!(cut.begin_drain().is_err());
    let mut changed = output["footer"].clone();
    changed["late"] = json!(1);
    changed["status"] = json!("partial");
    assert!(f.finish(&mut cut, &changed).is_err());
    assert_eq!(cut.phase(), Phase::Refused);
    assert!(f.finish(&mut cut, &output["footer"]).is_err());
}

#[test]
fn failed_ack_error_records_missing_native_load_and_missing_js_are_partial() {
    for (loaded, mode) in [(true, "failed"), (true, "error"), (false, "clean")] {
        let mut f = Fixture::new(loaded);
        let mut cut = f.cut();
        cut.begin_drain().unwrap();
        let output = actual_kj(&f.native.run, cut.id(), mode);
        f.admit(&output["records"]);
        assert_eq!(
            f.finish(&mut cut, &output["footer"]),
            Ok(Phase::SealedPartial)
        );
        assert!(!output.to_string().contains("private-payload"));
    }
    let f = Fixture::new(false);
    let mut cut = f.cut();
    assert_eq!(
        cut.missing(&f.native, &f.js, Some(&f.health())),
        Ok(Phase::SealedPartial)
    );
    assert!(cut.begin_drain().is_err());
}

#[test]
fn owner_health_must_be_native_bound_current_and_nonzero() {
    let f = Fixture::new(true);
    assert!(Cut::request(&f.native, &f.js, None).is_err());
    for variant in ["run", "task", "pid", "zero"] {
        let mut h = f.health();
        match variant {
            "run" => h.run = "foreign".into(),
            "task" => h.task = "b".repeat(64),
            "pid" => h.pid += 1,
            _ => h.version = 0,
        }
        assert!(Cut::request(&f.native, &f.js, Some(&h)).is_err());
        let mut cut = f.cut();
        cut.begin_drain().unwrap();
        assert!(cut.missing(&f.native, &f.js, Some(&h)).is_err());
        assert_eq!(cut.phase(), Phase::Refused);
    }
    let mut cut = f.cut();
    cut.begin_drain().unwrap();
    assert!(cut.missing(&f.native, &f.js, None).is_err());
}

#[test]
fn strict_footer_denies_identity_unknown_duplicate_type_size_and_counter_mutants() {
    let mut f = Fixture::new(true);
    let initial = f.cut();
    let output = actual_kj(&f.native.run, initial.id(), "clean");
    f.admit(&output["records"]);
    let base = output["footer"].clone();
    for (key, value) in [
        ("runId", json!("foreign")),
        ("loadGeneration", json!(2)),
        ("cutId", json!("foreign")),
        ("visiblePaint", json!("observed")),
        ("status", json!("sealed")),
        ("generated", json!(16)),
        ("generated", json!(0)),
        ("sent", json!(6)),
        ("failed", json!(1)),
        ("pending", json!(1)),
        ("late", json!(1)),
        ("dropped", json!(9007199254740992_u64)),
        ("extra", json!(true)),
        ("sent", json!("5")),
        ("sent", json!(-1)),
        ("sent", json!(1.5)),
    ] {
        let mut cut = f.cut();
        cut.begin_drain().unwrap();
        let mut footer = base.clone();
        footer["cutId"] = json!(cut.id());
        footer[key] = value;
        assert!(f.finish(&mut cut, &footer).is_err(), "{key}");
        assert_eq!(cut.phase(), Phase::Refused);
    }
    for bytes in [
        Vec::new(),
        vec![b' '; 1025],
        b"{\"sent\":1,\"sent\":1}".to_vec(),
        vec![0xff],
    ] {
        let mut cut = f.cut();
        cut.begin_drain().unwrap();
        assert!(cut
            .finish(&bytes, &f.native, &f.js, Some(&f.health()))
            .is_err());
    }
    let mut cut = f.cut();
    let mut footer = base.clone();
    footer["cutId"] = json!(cut.id());
    assert!(f.finish(&mut cut, &footer).is_err()); // request is not drain
    let mut cut = f.cut();
    cut.begin_drain().unwrap();
    footer["cutId"] = json!(cut.id());
    let full = serde_json::to_string(&footer).unwrap();
    let duplicate = full.replacen("{", "{\"sent\":5,", 1);
    assert!(cut
        .finish(duplicate.as_bytes(), &f.native, &f.js, Some(&f.health()))
        .is_err());
    let mut cut = f.cut();
    cut.begin_drain().unwrap();
    // Valid footer from another native-generated cut is a replay, not an ACK.
    assert!(f.finish(&mut cut, &footer).is_err());
}

#[test]
fn real_native_prefix_regression_owner_change_refusal_and_fence_cannot_be_clean() {
    for variant in ["owner", "regression", "refused", "fence", "rejection"] {
        let mut f = Fixture::new(true);
        let mut cut = f.cut();
        cut.begin_drain().unwrap();
        let output = actual_kj(&f.native.run, cut.id(), "clean");
        f.admit(&output["records"]);
        match variant {
            "owner" => f.native.pid += 1,
            "regression" => {
                f.native.phase = 2;
                f.native.stages.pop();
            }
            "refused" => f.native.refused = true,
            "fence" => f.js.fence(super::super::super::js::Fence::Closed),
            _ => {
                let url = tauri::Url::parse("tauri://localhost/index.html").unwrap();
                assert!(f.js.accept_json("main", &url, f.native.pid, b"{}").is_err());
            }
        }
        assert_ne!(
            f.finish(&mut cut, &output["footer"]),
            Ok(Phase::SealedObserved)
        );
    }
}

#[test]
fn actual_pending_loss_and_postcut_late_counts_never_become_observed() {
    for key in ["pending", "late", "dropped"] {
        let mut f = Fixture::new(true);
        let mut cut = f.cut();
        cut.begin_drain().unwrap();
        let output = actual_kj(&f.native.run, cut.id(), "clean");
        f.admit(&output["records"]);
        let mut footer = output["footer"].clone();
        footer[key] = json!(1);
        footer["status"] = json!(if key == "pending" {
            "draining"
        } else {
            "partial"
        });
        if key == "pending" {
            footer["generated"] = json!(6);
        }
        assert_eq!(f.finish(&mut cut, &footer), Ok(Phase::SealedPartial));
    }
    let mut f = Fixture::new(true);
    let mut cut = f.cut();
    cut.begin_drain().unwrap();
    let output = actual_kj(&f.native.run, cut.id(), "clean");
    f.admit(&output["records"]);
    let mut h = f.health();
    h.version = 2;
    assert_eq!(
        cut.finish(
            &serde_json::to_vec(&output["footer"]).unwrap(),
            &f.native,
            &f.js,
            Some(&h)
        ),
        Ok(Phase::SealedObserved)
    );
    assert!(f.finish(&mut cut, &output["footer"]).is_err()); // old retention version
    let mut cut = f.cut();
    cut.begin_drain().unwrap();
    let mut footer = output["footer"].clone();
    footer["cutId"] = json!(cut.id());
    assert_eq!(f.finish(&mut cut, &footer), Ok(Phase::SealedObserved));
    let bytes = serde_json::to_vec(&footer).unwrap();
    assert!(cut.finish(&bytes, &f.native, &f.js, Some(&h)).is_err());
    assert_eq!(cut.phase(), Phase::Refused); // newer proof is not exact replay
}

#[test]
fn footer_object_shape_refuses_matching_array_without_provisional_terminalization() {
    let mut f = Fixture::new(true);
    let seed = f.cut();
    let output = actual_kj(&f.native.run, seed.id(), "clean");
    f.admit(&output["records"]);
    for case in ["array", "", " \t\r\n", "null", "7", "true"] {
        let mut cut = f.cut();
        cut.begin_drain().unwrap();
        let mut footer = output["footer"].clone();
        footer["cutId"] = json!(cut.id());
        let array = json!([
            footer["runId"],
            footer["loadGeneration"],
            footer["cutId"],
            footer["status"],
            footer["generated"],
            footer["sent"],
            footer["failed"],
            footer["dropped"],
            footer["late"],
            footer["pending"],
            footer["visiblePaint"]
        ]);
        let bytes = if case == "array" {
            serde_json::to_vec(&array).unwrap()
        } else {
            case.as_bytes().to_vec()
        };
        let before = (cut.version, cut.native.clone(), cut.js.clone());
        assert!(cut
            .validate_provisional(&bytes, &f.native, &f.js, Some(&f.health()))
            .is_err());
        assert_eq!(cut.phase(), Phase::Draining);
        assert_eq!((cut.version, cut.native.clone(), cut.js.clone()), before);
        assert!(cut
            .finish(&bytes, &f.native, &f.js, Some(&f.health()))
            .is_err());
        assert_eq!(cut.phase(), Phase::Refused);
    }
    let mut cut = f.cut();
    cut.begin_drain().unwrap();
    let mut footer = output["footer"].clone();
    footer["cutId"] = json!(cut.id());
    let object = format!(" \t\r\n{}", serde_json::to_string(&footer).unwrap());
    assert_eq!(
        cut.validate_provisional(object.as_bytes(), &f.native, &f.js, Some(&f.health())),
        Ok(())
    );
    assert_eq!(
        cut.finish(object.as_bytes(), &f.native, &f.js, Some(&f.health())),
        Ok(Phase::SealedObserved)
    );
}
