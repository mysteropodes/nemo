//! Oracle tests for the linked-media state kernel (G32, L09-C).
//!
//! `fixtures/object-media-link-state/oracle.txt` was produced by running the
//! SHIPPED `keyFor`, `primeCache`, `_entriesForKey`, `_markEntryBroken/Ok`,
//! `_resolveDesktopPath`, `_resolveWebHandle`, `resolveAsync`,
//! `requestPermissionForAll`, `getHandleFile` (`src/js/linked-media.js`) and
//! `_videoEntryFor`, `_markVideoEntryBroken/Ok`, `retryWebLinkedSessions` plus
//! the verbatim web branch of `_layerFrameSyncAsync`
//! (`src/js/native-video-bridge.js`), all cut verbatim at fbe4b8a3 and evaluated
//! in a Node `vm` (generator `tools/gen.js`). Only platform facts are faked: a
//! clock, a recording library, a desktop reader, a handle store, `FileReader`.
//!
//! Strings are `-` (absent), `=` (empty) or percent-encoded bytes. Rows:
//! `K path handle key`, `E entries op args => entries' n`, and the scripted
//! worlds `R script => trace`, `W|Wb script => trace` (grammar in the generator
//! header). The executor below plays the adapter: it turns a scripted platform
//! mode into the kernel's typed outcome and mirrors the platform's one state
//! change (a granted permission prompt makes the handle readable).

use super::controls::Mutation;
use super::world::{dec, enc, entries_of, entries_tok, reason_of, run_r, run_w};
use super::LinkedRef;

const ORACLE: &str = include_str!("fixtures/object-media-link-state/oracle.txt");

#[derive(Default)]
pub(crate) struct Run {
    pub k: usize,
    pub e: usize,
    pub r: usize,
    pub w: usize,
    pub bad: Vec<String>,
}

fn k_row(t: &[&str], m: Mutation) -> Option<String> {
    let (p, h) = (dec(t[1]), dec(t[2]));
    let r = LinkedRef {
        linked_path: p.as_deref(),
        linked_handle_id: h.as_deref(),
    };
    let got = m.key(&r).map_or("-".to_owned(), |k| {
        let tag = k.tag();
        enc(Some(tag.as_str()))
    });
    (got != t[3]).then(|| format!("got {got}, expected {}", t[3]))
}

fn e_row(head: &str, tail: &str, m: Mutation) -> Option<String> {
    let t: Vec<&str> = head.split(' ').collect();
    let mut lib = entries_of(t[1]);
    let args = |i: usize| (dec(t[i]), dec(t[i + 1]));
    let got = match t[2] {
        "eb" | "eo" | "ek" => {
            let (p, h) = args(3);
            let r = LinkedRef {
                linked_path: p.as_deref(),
                linked_handle_id: h.as_deref(),
            };
            let tail_tok = match t[2] {
                "eb" => u8::from(m.mark_broken(&mut lib, &r, reason_of(t[5]).expect("reason")))
                    .to_string(),
                "eo" => u8::from(m.mark_ok(&mut lib, &r)).to_string(),
                _ => {
                    let hit = m.entries_for_key(&lib, &r);
                    if hit.is_empty() {
                        "-".to_owned()
                    } else {
                        hit.iter()
                            .map(usize::to_string)
                            .collect::<Vec<_>>()
                            .join(".")
                    }
                }
            };
            format!("{} {tail_tok}", entries_tok(&lib))
        }
        "vb" | "vo" => {
            let uid = dec(t[3]);
            let reload = if t[2] == "vb" {
                m.mark_video_broken(&mut lib, uid.as_deref(), reason_of(t[4]).expect("reason"))
            } else {
                super::mark_video_ok(&mut lib, uid.as_deref())
            };
            format!("{} {}", entries_tok(&lib), u8::from(reload))
        }
        other => panic!("E op {other}"),
    };
    (got != tail).then(|| format!("got {got}, expected {tail}"))
}

pub(crate) fn run(m: Mutation) -> Run {
    let mut r = Run::default();
    for (n, line) in ORACLE.lines().enumerate() {
        let op = line.split(' ').next().expect("op");
        let fail = match op {
            "K" => {
                r.k += 1;
                k_row(&line.split(' ').collect::<Vec<_>>(), m)
            }
            "E" => {
                r.e += 1;
                let (head, tail) = line.split_once(" => ").expect("E row");
                e_row(head, tail, m)
            }
            "R" | "W" | "Wb" => {
                let (head, want) = line.split_once(" => ").expect("script row");
                let script = &head[op.len() + 1..];
                let got = if op == "R" {
                    r.r += 1;
                    run_r(script, m)
                } else {
                    r.w += 1;
                    run_w(script, op == "Wb", m)
                };
                (got != want).then(|| format!("script {script}\n  got  {got}\n  want {want}"))
            }
            other => panic!("row op {other}"),
        };
        if let Some(why) = fail {
            r.bad.push(format!("{}: {} {why}", n + 1, &op[..1]));
        }
    }
    r
}

#[test]
fn generated_oracle_matches_baseline() {
    let r = run(Mutation::None);
    assert!(
        r.bad.is_empty(),
        "{} rows differ:\n{}",
        r.bad.len(),
        r.bad[..r.bad.len().min(3)].join("\n")
    );
}

#[test]
fn oracle_has_the_documented_row_counts() {
    let r = run(Mutation::None);
    assert_eq!((r.k, r.e, r.r, r.w), (289, 204, 143, 58));
}

#[test]
fn oracle_pins_the_cooldown_boundary_for_both_backends() {
    // Failure at t=1000: closed at +3999, open at +4000 (second begin is S).
    for (name, mode) in [("a", "gone"), ("b", "perm"), ("e", "readfail")] {
        let at = |dt: i64| {
            run_r(
                &format!("t=1000;env.{name}={mode};b.{name};t={};b.{name}", 1000 + dt),
                Mutation::None,
            )
        };
        let code = |dt: i64| at(dt).split('|').last().unwrap().chars().next().unwrap();
        assert_eq!(
            (code(3999), code(4000), code(4001)),
            ('C', 'S', 'S'),
            "{name}"
        );
        assert_eq!(code(-1), 'C', "a clock that went backwards keeps cooling");
    }
}
