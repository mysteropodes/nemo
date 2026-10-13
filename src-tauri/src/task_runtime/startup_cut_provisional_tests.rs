use super::super::retention::Store;
use super::*;
use std::{
    io::Write,
    process::{Command, Stdio},
};

struct Fixture {
    root: std::path::PathBuf,
    native: LoadLog,
    js: JsStream,
    store: Store,
    version: u64,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nemo-provisional-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let file = std::fs::File::create(root.join("native.json")).unwrap();
        let mut native = LoadLog::new(
            uuid::Uuid::new_v4().to_string(),
            "a".repeat(64),
            std::process::id(),
            file,
        );
        let run = native.run.clone();
        let url = tauri::Url::parse("tauri://localhost/index.html").unwrap();
        native.confirm_creation("main", native.pid, &run);
        native.observe("native-page-started", "main", &url, native.pid, &run);
        native.observe("native-page-finished", "main", &url, native.pid, &run);
        let js = JsStream::from_native(&native).unwrap();
        let initial = serde_json::json!({"adapterRefused":false,"js":js.projection()});
        let store = Store::open(&root, &root, &initial).unwrap();
        store.publish(1, &initial).unwrap();
        Self {
            root,
            native,
            js,
            store,
            version: 1,
        }
    }
    fn snapshot(&self) -> Value {
        serde_json::json!({"adapterRefused":false,"js":self.js.projection()})
    }
    fn proof(&self) -> RetentionEvidence {
        self.store
            .current_health(self.version, &self.snapshot())
            .unwrap()
    }
    fn cut(&self) -> Cut {
        Cut::request(&self.native, &self.js, Some(&self.proof())).unwrap()
    }
    fn admit(&mut self, records: &Value) {
        let url = tauri::Url::parse("tauri://localhost/index.html").unwrap();
        for record in records.as_array().unwrap() {
            self.js
                .accept_json(
                    "main",
                    &url,
                    self.native.pid,
                    &serde_json::to_vec(record).unwrap(),
                )
                .unwrap();
        }
        self.version += 1;
        self.store.publish(self.version, &self.snapshot()).unwrap();
    }
    fn check(&self, cut: &Cut, footer: &Value) -> Result<(), &'static str> {
        cut.validate_provisional(
            &serde_json::to_vec(footer).unwrap(),
            &self.native,
            &self.js,
            Some(&self.proof()),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn pins(c: &Cut) -> Value {
    serde_json::json!({"run":c.run,"task":c.task,"pid":c.pid,"id":c.id,
        "phase":format!("{:?}",c.phase),"native":c.native,"js":c.js,
        "version":c.version,"footer":format!("{:?}",c.footer)})
}
fn actual_kj(run: &str, cut: &str) -> Value {
    let program = r#"const fs=require('fs'),v=JSON.parse(fs.readFileSync(0,'utf8')),records=[],frames=new Map();let id=0;
const {createStartupProbe}=require(v.source),target=()=>({addEventListener(){},removeEventListener(){}});
const document=Object.assign(target(),{readyState:'complete',getElementById:()=>null});
const window=Object.assign(target(),{document});window.top=window;
const probe=createStartupProbe({enabled:true,runId:v.run,loadGeneration:1},{window,document,
emit:e=>{records.push(e);return true},getComputedStyle:()=>({}),
requestAnimationFrame:f=>{frames.set(++id,f);return id},cancelAnimationFrame:i=>frames.delete(i)});
const escaped=frames.values().next().value;
for(let i=0;i<2;i++){const [id,f]=frames.entries().next().value;frames.delete(id);f()}
(async()=>{const first=await probe.beginCut(v.cut);escaped();const last=probe.cutState();
process.stdout.write(JSON.stringify({first,last,records}));probe.dispose()})();"#;
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/desktop/startup-observer-bootstrap.js");
    let mut child = Command::new("node")
        .args(["-e", program])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let bytes =
        serde_json::to_vec(&serde_json::json!({"source":source,"run":run,"cut":cut})).unwrap();
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn real_drain_and_later_live_late_snapshot_share_validator_without_terminal_or_write() {
    let mut f = Fixture::new();
    let mut cut = f.cut();
    cut.begin_drain().unwrap();
    let output = actual_kj(&f.native.run, cut.id());
    f.admit(&output["records"]);
    let before = pins(&cut);
    let disk = std::fs::read(f.root.join("startup-js.json")).unwrap();
    assert_eq!(output["first"]["late"], 0);
    assert_eq!(output["last"]["late"], 1);
    assert_eq!(output["last"]["status"], "partial");
    for footer in [&output["first"], &output["first"], &output["last"]] {
        assert_eq!(f.check(&cut, footer), Ok(()));
        assert_eq!(pins(&cut), before);
    }
    assert_eq!(std::fs::read(f.root.join("startup-js.json")).unwrap(), disk);
    assert_eq!(f.store.current_health(2, &f.snapshot()).unwrap().version, 2);
    let bytes = serde_json::to_vec(&output["last"]).unwrap();
    assert_eq!(
        cut.finish(&bytes, &f.native, &f.js, Some(&f.proof())),
        Ok(Phase::SealedPartial)
    );
    assert!(f.check(&cut, &output["last"]).is_err());
}

#[test]
fn requested_terminal_missing_and_refused_cuts_cannot_use_provisional_route() {
    for kind in ["requested", "observed", "missing", "refused"] {
        let mut f = Fixture::new();
        let mut cut = f.cut();
        let output = actual_kj(&f.native.run, cut.id());
        f.admit(&output["records"]);
        if kind != "requested" {
            cut.begin_drain().unwrap();
        }
        match kind {
            "observed" => {
                assert_eq!(
                    cut.finish(
                        &serde_json::to_vec(&output["first"]).unwrap(),
                        &f.native,
                        &f.js,
                        Some(&f.proof())
                    ),
                    Ok(Phase::SealedObserved)
                );
            }
            "missing" => {
                cut.missing(&f.native, &f.js, Some(&f.proof())).unwrap();
            }
            "refused" => {
                assert!(cut
                    .finish(b"{}", &f.native, &f.js, Some(&f.proof()))
                    .is_err());
            }
            _ => {}
        }
        let before = pins(&cut);
        assert!(f.check(&cut, &output["first"]).is_err());
        assert_eq!(pins(&cut), before);
    }
}

#[test]
fn shared_typed_parser_refuses_identity_schema_type_duplicate_and_counter_mutants_readonly() {
    let mut f = Fixture::new();
    let mut cut = f.cut();
    cut.begin_drain().unwrap();
    let output = actual_kj(&f.native.run, cut.id());
    f.admit(&output["records"]);
    let before = pins(&cut);
    for (key, value) in [
        ("runId", serde_json::json!("foreign")),
        ("cutId", serde_json::json!(uuid::Uuid::new_v4().to_string())),
        ("loadGeneration", serde_json::json!(2)),
        ("visiblePaint", serde_json::json!("observed")),
        ("generated", serde_json::json!(16)),
        ("sent", serde_json::json!(6)),
        ("sent", serde_json::json!(-1)),
        ("sent", serde_json::json!(1.5)),
        ("sent", serde_json::json!("5")),
        ("failed", serde_json::json!(1)),
        ("pending", serde_json::json!(1)),
        ("late", serde_json::json!(1)),
        ("extra", serde_json::json!(true)),
    ] {
        let mut bad = output["first"].clone();
        bad[key] = value;
        assert!(f.check(&cut, &bad).is_err(), "{key}");
        assert_eq!(pins(&cut), before);
    }
    let full = serde_json::to_string(&output["first"]).unwrap();
    for bytes in [
        full.replacen("{", "{\"sent\":5,", 1).into_bytes(),
        Vec::new(),
        vec![b' '; 1025],
        vec![0xff],
    ] {
        assert!(cut
            .validate_provisional(&bytes, &f.native, &f.js, Some(&f.proof()))
            .is_err());
        assert_eq!(pins(&cut), before);
    }
    assert_eq!(f.check(&cut, &output["first"]), Ok(())); // caller owns failure latch
}

#[test]
fn real_current_health_versions_and_foreign_native_context_must_match() {
    let mut f = Fixture::new();
    let old = f.proof();
    let provisional = f.cut();
    let transcript = actual_kj(&f.native.run, provisional.id());
    f.admit(&transcript["records"]);
    let mut cut = f.cut();
    cut.begin_drain().unwrap();
    let output = actual_kj(&f.native.run, cut.id());
    let bytes = serde_json::to_vec(&output["first"]).unwrap();
    let before = pins(&cut);
    assert!(cut
        .validate_provisional(&bytes, &f.native, &f.js, Some(&old))
        .is_err());
    assert!(cut
        .validate_provisional(&bytes, &f.native, &f.js, None)
        .is_err());
    assert_eq!(f.check(&cut, &output["first"]), Ok(()));
    f.native.pid += 1;
    assert!(f.check(&cut, &output["first"]).is_err());
    f.native.pid -= 1;
    assert_eq!(pins(&cut), before);
    let mut foreign = Fixture::new();
    foreign.admit(&actual_kj(&foreign.native.run, cut.id())["records"]);
    assert!(cut
        .validate_provisional(&bytes, &foreign.native, &foreign.js, Some(&foreign.proof()))
        .is_err());
    assert_eq!(pins(&cut), before);
    std::fs::write(f.root.join("startup-js.json"), b"foreign retained bytes").unwrap();
    assert!(f.store.current_health(f.version, &f.snapshot()).is_err());
    assert!(cut
        .validate_provisional(&bytes, &f.native, &f.js, None)
        .is_err());
    assert_eq!(pins(&cut), before);
}
