//! Oracle fixture parser for the import id remap kernel (L10-G). The fixture is generated from the
//! baseline `fbe4b8a3` by `tools/gen.mjs` (see HANDOFF.md); this file only decodes its rows.

pub(crate) const ORACLE: &str = include_str!("fixtures/object-import-remap-ids/oracle.txt");

/// `%XX` bytes, `$` is the empty string, `~` is null/undefined.
pub(crate) fn dec(token: &str) -> Option<String> {
    match token {
        "~" => None,
        "$" => Some(String::new()),
        _ => {
            let bytes = token.as_bytes();
            let mut out = Vec::with_capacity(bytes.len());
            let mut at = 0;
            while at < bytes.len() {
                if bytes[at] == b'%' {
                    let hex = std::str::from_utf8(&bytes[at + 1..at + 3]).unwrap();
                    out.push(u8::from_str_radix(hex, 16).unwrap());
                    at += 3;
                } else {
                    out.push(bytes[at]);
                    at += 1;
                }
            }
            Some(String::from_utf8(out).unwrap())
        }
    }
}

fn list(token: &str) -> Vec<String> {
    if token == "-" {
        return Vec::new();
    }
    token.split(',').map(|part| dec(part).unwrap()).collect()
}

/// `N F` / `N P`: (name or None, index, resulting name).
pub(crate) type NameRow = (Option<String>, usize, String);

/// One migrated storyboard module: id, type, symbol id.
pub(crate) type ModuleRow = (String, String, Option<String>);

#[derive(Debug)]
pub(crate) struct Board {
    pub(crate) case: String,
    pub(crate) modules: Vec<ModuleRow>,
    pub(crate) chains: Vec<(String, Vec<String>)>,
    pub(crate) audio: Vec<(String, Vec<String>)>,
    pub(crate) next_id: u64,
}

#[derive(Debug, Default)]
pub(crate) struct Oracle {
    pub(crate) figma: Vec<NameRow>,
    pub(crate) psd: Vec<NameRow>,
    pub(crate) layer_names: Vec<(Vec<String>, String)>,
    pub(crate) uids: Vec<String>,
    pub(crate) boards: Vec<Board>,
}

fn pairs(section: &str) -> Vec<(String, Vec<String>)> {
    section
        .split_whitespace()
        .map(|item| {
            let (id, rest) = item.split_once('=').unwrap();
            (dec(id).unwrap(), list(rest))
        })
        .collect()
}

pub(crate) fn parse(text: &str) -> Oracle {
    let mut out = Oracle::default();
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts[0] {
            "N" if parts[1] == "L" => {
                let (head, result) = line[4..].split_once(" => ").unwrap();
                out.layer_names.push((list(head), dec(result).unwrap()));
            }
            "N" => {
                assert_eq!(parts[4], "=>", "{line}");
                let row = (
                    dec(parts[2]),
                    parts[3].parse().unwrap(),
                    dec(parts[5]).unwrap(),
                );
                match parts[1] {
                    "F" => out.figma.push(row),
                    "P" => out.psd.push(row),
                    _ => unreachable!("{line}"),
                }
            }
            "U" => {
                assert_eq!(parts[1].parse::<usize>().unwrap(), list(parts[2]).len());
                out.uids = list(parts[2]);
            }
            "S" => out.boards.push(parse_board(line)),
            _ => unreachable!("{line}"),
        }
    }
    out
}

fn parse_board(line: &str) -> Board {
    let sections: Vec<&str> = line[2..].split(" | ").collect();
    assert_eq!(sections.len(), 5, "{line}");
    let modules = sections[1]
        .split_whitespace()
        .map(|item| {
            let mut it = item.splitn(3, ':');
            (
                dec(it.next().unwrap()).unwrap(),
                it.next().unwrap().to_owned(),
                dec(it.next().unwrap()),
            )
        })
        .collect();
    Board {
        case: sections[0].to_owned(),
        modules,
        chains: pairs(sections[2]),
        audio: pairs(sections[3]),
        next_id: sections[4]
            .strip_prefix("nextId=")
            .unwrap()
            .parse()
            .unwrap(),
    }
}
