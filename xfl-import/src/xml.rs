//! Minimal owned element tree over quick-xml. XFL files are small enough
//! (a few MB) that a tree is simpler and safer than streaming, and every
//! reader below only needs attribute lookups and child walks.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::XflError;

/// Hard nesting limit: real XFL documents nest groups a few dozen levels deep.
const MAX_DEPTH: usize = 512;

#[derive(Debug, Clone, Default)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn attr_f64(&self, key: &str) -> Option<f64> {
        self.attr(key)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
    }

    pub fn attr_u32(&self, key: &str) -> Option<u32> {
        self.attr(key).and_then(|v| v.trim().parse::<u32>().ok())
    }

    pub fn attr_bool(&self, key: &str) -> Option<bool> {
        match self.attr(key)? {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// Children of the first child called `wrapper` (XFL wraps lists: `<layers><DOMLayer/>…`).
    pub fn list<'a>(&'a self, wrapper: &str) -> impl Iterator<Item = &'a Node> + 'a {
        self.child(wrapper)
            .into_iter()
            .flat_map(|w| w.children.iter())
    }
}

fn local_name(raw: &[u8]) -> String {
    let local = match raw.iter().rposition(|b| *b == b':') {
        Some(i) => &raw[i + 1..],
        None => raw,
    };
    String::from_utf8_lossy(local).into_owned()
}

fn start_node(e: &BytesStart<'_>, file: &str) -> Result<Node, XflError> {
    let mut node = Node {
        name: local_name(e.name().as_ref()),
        ..Node::default()
    };
    for attr in e.attributes() {
        let attr = attr.map_err(|err| XflError::Xml {
            file: file.to_string(),
            message: err.to_string(),
        })?;
        let key = local_name(attr.key.as_ref());
        let value = attr
            .unescape_value()
            .map_err(|err| XflError::Xml {
                file: file.to_string(),
                message: err.to_string(),
            })?
            .into_owned();
        node.attrs.push((key, value));
    }
    Ok(node)
}

/// Parse a whole XML document and return its root element.
pub fn parse(bytes: &[u8], file: &str) -> Result<Node, XflError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    let xml_err = |message: String| XflError::Xml {
        file: file.to_string(),
        message,
    };
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| xml_err(e.to_string()))?
        {
            Event::Start(e) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(xml_err(format!("nesting deeper than {MAX_DEPTH}")));
                }
                stack.push(start_node(&e, file)?);
            }
            Event::Empty(e) => {
                let node = start_node(&e, file)?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(node),
                    None if root.is_none() => root = Some(node),
                    None => return Err(xml_err("more than one root element".into())),
                }
            }
            Event::End(_) => {
                let node = stack
                    .pop()
                    .ok_or_else(|| xml_err("unbalanced end tag".into()))?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(node),
                    None if root.is_none() => root = Some(node),
                    None => return Err(xml_err("more than one root element".into())),
                }
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text
                        .push_str(&t.decode().map_err(|e| xml_err(e.to_string()))?);
                }
            }
            Event::CData(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&String::from_utf8_lossy(&t));
                }
            }
            Event::GeneralRef(r) => {
                if let Some(top) = stack.last_mut() {
                    let name = r.decode().map_err(|e| xml_err(e.to_string()))?;
                    let resolved = match name.as_ref() {
                        "lt" => Some("<".to_string()),
                        "gt" => Some(">".to_string()),
                        "amp" => Some("&".to_string()),
                        "apos" => Some("'".to_string()),
                        "quot" => Some("\"".to_string()),
                        _ => r.resolve_char_ref().ok().flatten().map(|c| c.to_string()),
                    };
                    if let Some(s) = resolved {
                        top.text.push_str(&s);
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    if !stack.is_empty() {
        return Err(xml_err("unexpected end of file".into()));
    }
    root.ok_or_else(|| xml_err("no root element".into()))
}
