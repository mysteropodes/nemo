//! Baseline oracle rows: the shipped `svgDocument`/`exportFrameSVGString` run in a
//! Node vm (rows P D F) plus the derived number/percent rows (N C), pinned
//! against the serializer's wrapper and formatters.
use super::*;

const ORACLE: &str = include_str!("fixtures/native-vector-export-svg/oracle.txt");

pub(super) fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => panic!("dangling escape in oracle row"),
        }
    }
    out
}

pub(super) fn rows(op: &str) -> Vec<Vec<String>> {
    ORACLE
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(|cols| cols[0] == op)
        .map(|cols| cols[1..].iter().map(|c| unescape(c)).collect())
        .collect()
}

pub(super) fn bits(hex: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(hex, 16).unwrap())
}

#[test]
fn baseline_wrapper_rows_match_svg_document_and_call_trace() {
    assert_eq!(rows("P"), vec![vec![XML_PREAMBLE.to_owned()]]);
    let d = rows("D");
    assert_eq!(d.len(), 42);
    for row in &d {
        let (w, h) = (row[0].parse().unwrap(), row[1].parse().unwrap());
        assert_eq!(svg_document(&row[2], w, h), row[3], "{row:?}");
    }
    let f = rows("F");
    assert_eq!(f.len(), 4);
    for row in &f {
        let (w, h): (u32, u32) = (row[1].parse().unwrap(), row[2].parse().unwrap());
        assert_eq!(svg_document(&row[3], w, h), row[4]);
        let trace = format!(
            "buildFrame:{}|dimensions|visible:true|exportSVG:true:0,0,{w},{h}|visible:false",
            row[0]
        );
        assert_eq!(row[5], trace);
    }
}

#[test]
fn number_rows_pin_shortest_round_trip_text_and_the_exponent_divergence() {
    let (mut same, mut exponent) = (0, 0);
    for row in rows("N") {
        let x = bits(&row[0]);
        let ours = format_number(x);
        assert_eq!(ours, row[2], "{row:?}");
        assert!(!ours.contains('e') && !ours.contains('E'));
        assert_eq!(ours.parse::<f64>().unwrap(), x, "round trip {ours}");
        match row[3].as_str() {
            "S" => {
                assert_eq!(ours, row[1], "JS String(x) agrees");
                same += 1;
            }
            "X" => {
                assert!(row[1].contains('e'));
                assert_ne!(ours, row[1], "deliberate positional text");
                exponent += 1;
            }
            other => panic!("class {other}"),
        }
    }
    assert_eq!((same, exponent), (253, 57));
    assert_eq!(format_number(-0.0), "0");
    assert_eq!(format_number(1e21), "1000000000000000000000");
    assert_eq!(format_number(1e-7), "0.0000001");
}

#[test]
fn channel_rows_pin_the_exact_decimal_shift() {
    let c = rows("C");
    assert_eq!(c.len(), 189);
    for row in &c {
        assert_eq!(channel_percent(bits(&row[0])), row[1], "{row:?}");
    }
    for (value, text) in [
        (0.0, "0"),
        (1.0, "100"),
        (0.5, "50"),
        (0.001, "0.1"),
        (0.2, "20"),
        (0.30000000000000004, "30.000000000000004"),
    ] {
        assert_eq!(channel_percent(value), text);
    }
}
