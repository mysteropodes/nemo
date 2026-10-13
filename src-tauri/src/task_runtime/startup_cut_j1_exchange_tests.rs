use super::super::{cut::Phase, transact};
use super::exchange_tests::{origin, Fixture};
use super::*;
use footer::{Ack, Stage};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use tauri::ipc::InvokeBody;
#[test]
fn valid_divergent_footer_replay_spends_the_exchange_without_reset() {
    let f = Fixture::new();
    f.drain();
    let body = f.body();
    assert_eq!(f.ack(Ack::Drain, &body), Ok(true));
    let InvokeBody::Raw(bytes) = body else {
        panic!("fixture raw");
    };
    let mut v: Value = serde_json::from_slice(&bytes).unwrap();
    v["late"] = json!(1);
    v["status"] = json!("partial");
    let changed = InvokeBody::Raw(serde_json::to_vec(&v).unwrap());
    assert!(f.ack(Ack::Drain, &changed).is_err());
    assert!(dispatch::next(&f.state).is_err());
}
#[test]
fn missing_pending_or_footer_timeout_keeps_dispatch_exit_distinct_from_owner_work() {
    for requested in [false, true] {
        let f = Fixture::boot(requested);
        f.state.0.lock().unwrap().owner.as_mut().unwrap().deadline =
            Some(Instant::now() + Duration::from_millis(30));
        let work = dispatch::prepare(&f.state, "main", &origin(), std::process::id()).unwrap();
        let (owner_exit, exchange_exit) = {
            let slot = f.state.0.lock().unwrap();
            (
                slot.owner.as_ref().unwrap().exited.clone(),
                slot.exchange.exited.clone(),
            )
        };
        assert!(!Arc::ptr_eq(&owner_exit, &exchange_exit));
        let mut emitted = 0;
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    dispatch::run(work, |_| {
                        f.free();
                        emitted += 1;
                        Ok(())
                    })
                })
                .join()
                .unwrap()
        });
        assert_eq!(emitted, usize::from(requested));
        assert!(owner_exit.load(Ordering::Acquire));
        assert!(exchange_exit.load(Ordering::Acquire));
        assert_eq!(
            f.state.0.lock().unwrap().exchange.stage as u8,
            Stage::Failed as u8
        );
    }
}
#[test]
fn both_native_ack_kinds_reject_health_ownership_overlap_and_deadline_changes() {
    for kind in [Ack::Drain, Ack::Snapshot] {
        for fault in [
            "mode", "root", "store", "owner", "overlap", "deadline", "adapter", "poison",
        ] {
            let f = Fixture::new();
            f.drain();
            let body = f.body();
            if kind == Ack::Snapshot {
                f.ack(Ack::Drain, &body).unwrap();
                dispatch::next(&f.state).unwrap().unwrap();
            }
            match fault {
                "mode" => {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(
                        f.root.join("reports/startup-js.json"),
                        std::fs::Permissions::from_mode(0o644),
                    )
                    .unwrap();
                }
                "root" => {
                    std::fs::rename(f.root.join("reports"), f.root.join("old-reports")).unwrap();
                    std::fs::create_dir(f.root.join("reports")).unwrap();
                }
                "poison" => {
                    let slot = f.state.0.clone();
                    let _ = std::thread::spawn(move || {
                        let _guard = slot.lock().unwrap();
                        panic!("fixture Slot poison");
                    })
                    .join();
                }
                _ => {}
            }
            let result = footer::acknowledge_after(
                &f.state,
                "main",
                Ok(&origin()),
                std::process::id(),
                &body,
                kind,
                || {
                    f.free();
                    match fault {
                        "store" => {
                            let other = Fixture::new();
                            *f.state.1.lock().unwrap() = other.state.1.lock().unwrap().clone();
                        }
                        "owner" => f.state.0.lock().unwrap().owner = None,
                        "overlap" => assert!(f.ack(kind, &body).is_err()),
                        "deadline" => {
                            f.state.0.lock().unwrap().owner.as_mut().unwrap().deadline =
                                Some(Instant::now() + Duration::from_secs(1200))
                        }
                        "adapter" => {
                            f.state
                                .0
                                .lock()
                                .unwrap()
                                .bridge
                                .as_mut()
                                .unwrap()
                                .adapter_refused = true
                        }
                        _ => {}
                    }
                },
            );
            assert!(result.is_err(), "{fault}");
            assert!(dispatch::next(&f.state).is_err());
            if fault != "poison" {
                assert!(f
                    .state
                    .0
                    .lock()
                    .unwrap()
                    .owner
                    .as_ref()
                    .is_none_or(|b| b.session.state() == owner::State::Refused));
            }
        }
    }
}
#[test]
fn footer_h1_is_unlocked_and_final_original_native_lock_cannot_admit_past_deadline() {
    let f = Fixture::new();
    f.drain();
    let body = f.body();
    let deadline = Instant::now() + Duration::from_secs(3);
    f.state.0.lock().unwrap().owner.as_mut().unwrap().deadline = Some(deadline);
    let (send, recv) = std::sync::mpsc::channel();
    let (held_send, held_recv) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let fref = &f;
        let worker = scope.spawn(move || {
            footer::acknowledge_after(
                &fref.state,
                "main",
                Ok(&origin()),
                std::process::id(),
                &body,
                Ack::Drain,
                || {
                    fref.free();
                    send.send(()).unwrap();
                    held_recv.recv().unwrap();
                },
            )
        });
        recv.recv_timeout(Duration::from_secs(5)).unwrap();
        let held = f.native.lock().unwrap();
        held_send.send(()).unwrap();
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        drop(held);
        assert!(worker.join().unwrap().is_err());
    });
    assert_eq!(
        f.state
            .0
            .lock()
            .unwrap()
            .owner
            .as_ref()
            .unwrap()
            .session
            .state(),
        owner::State::Refused
    );
}
#[test]
fn actual_generated_j1_vm_defers_acks_and_preserves_live_late_footer_in_real_slot() {
    use std::process::{Command, Stdio};
    let f = Fixture::new();
    let script =
        super::super::super::injection::build_script(Some(&f.native.lock().unwrap()), true, true)
            .unwrap()
            .unwrap();
    let program = r#"const vm=require('vm'),rl=require('readline').createInterface({input:process.stdin});
const out=v=>process.stdout.write(JSON.stringify(v)+'\n'),wait=new Map(),timer=setTimeout(()=>process.exit(7),10000);
let callback,probe,escaped,seq=0,footers=0;const flush=async()=>{for(let i=0;i<16;i++)await Promise.resolve()};
function target(){return {addEventListener(){},removeEventListener(){}}}
rl.on('line',async line=>{try{const v=JSON.parse(line);
if(v.script){const frames=new Map(),doc=Object.assign(target(),{readyState:'complete',getElementById:()=>null});
const win=Object.assign(target(),{document:doc,location:{href:'tauri://localhost/index.html'},getComputedStyle:()=>({display:'block',visibility:'visible',opacity:'1'}),requestAnimationFrame:f=>{frames.set(1,f);return 1},cancelAnimationFrame:i=>frames.delete(i)});win.top=win;
win.__TAURI_INTERNALS__={invoke(command,bytes){if(command!=='nemo_test_startup_record')footers++;const id=++seq;out({command,bytes:[...bytes],id});return new Promise(r=>wait.set(id,r))}};
win.__TAURI__={event:{listen(n,f,t){callback=f;return Promise.resolve(()=>{})}}};
probe=vm.runInNewContext(v.script,{window:win,document:doc,URL,TextEncoder});escaped=[...frames.values()][0];await flush();out({ready:true});
}else if(v.event){if(v.late)escaped();callback({payload:v.event});await flush();out(v.late?{snapshotBlocked:footers===1}:{blocked:footers===0,pending:probe.state().pending});
}else {wait.get(v.id)(v.ack);wait.delete(v.id);await flush();if(v.done){await probe.drained();probe.dispose();clearTimeout(timer);rl.close();out({done:true})}}
}catch(e){clearTimeout(timer);process.exit(8)}});"#;
    let mut child = Command::new("node")
        .args(["-e", program])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = std::io::BufReader::new(child.stdout.take().unwrap());
    writeln!(input, "{}", json!({"script":script})).unwrap();
    let mut early = None;
    let mut drain_ack = None;
    let mut saw_late = false;
    loop {
        let mut line = String::new();
        assert!(output.read_line(&mut line).unwrap() > 0);
        assert!(line.len() < 8192);
        let v: Value = serde_json::from_str(&line).unwrap();
        if v["ready"] == true {
            f.free();
            writeln!(
                input,
                "{}",
                json!({"event":serde_json::to_value(f.drain()).unwrap()})
            )
            .unwrap();
        } else if v["blocked"] == true {
            assert!(v["pending"].as_u64().unwrap() > 0);
            writeln!(input, "{}", early.take().unwrap()).unwrap();
        } else if v["snapshotBlocked"] == true {
            writeln!(input, "{}", drain_ack.take().unwrap()).unwrap();
        } else if v["done"] == true {
            break;
        } else {
            let bytes: Vec<u8> = serde_json::from_value(v["bytes"].clone()).unwrap();
            assert!(bytes.len() <= 1024);
            let body = InvokeBody::Raw(bytes.clone());
            let command = v["command"].as_str().unwrap();
            let ack = if command == "nemo_test_startup_record" {
                transact(&f.state, "main", Ok(&origin()), std::process::id(), &body).unwrap()
            } else {
                let kind = if command.ends_with("drained") {
                    Ack::Drain
                } else {
                    Ack::Snapshot
                };
                f.ack(kind, &body).unwrap()
            };
            let answer = json!({"id":v["id"],"ack":ack,"done":command.ends_with("snapshot")});
            if v["id"] == 1 {
                early = Some(answer);
            } else if command.ends_with("drained") {
                drain_ack = Some(answer);
                f.free();
                writeln!(input,"{}",json!({"event":serde_json::to_value(dispatch::next(&f.state).unwrap().unwrap()).unwrap(),"late":true})).unwrap();
            } else {
                if command.ends_with("snapshot") {
                    let footer: Value = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(footer["status"], "partial");
                    assert!(footer["late"].as_u64().unwrap() > 0);
                    saw_late = true;
                }
                writeln!(input, "{}", answer).unwrap();
            }
        }
    }
    assert!(child.wait().unwrap().success());
    assert!(saw_late);
    assert_eq!(
        f.state.0.lock().unwrap().exchange.stage as u8,
        Stage::Complete as u8
    );
    assert_eq!(
        f.state
            .0
            .lock()
            .unwrap()
            .owner
            .as_mut()
            .unwrap()
            .session
            .pending()
            .unwrap()
            .phase(),
        Phase::Draining
    );
}
