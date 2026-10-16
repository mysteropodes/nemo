//! Value grammar of the serP/desP oracle and of the literal pin tables (G14/G22/G13, leaf L01-C).
//!
//! `!` undefined, `^` null, `T`/`F`, `n<16 hex>` the IEEE 754 bits of a number (oracle), a decimal number
//! (pins only; `-0` is negative zero, non-finite text is refused), a JSON string, `[a,b]` and `{"k":v}`.
//! Numbers compare by bit pattern, so `-0` and `0` are different values; an object compares member by member in
//! insertion order. A missing key and a key holding `!` both read as `Value::Undef`.

#[derive(Debug, Clone)]
pub(crate) enum Value {
    Undef,
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

pub(crate) static UNDEF: Value = Value::Undef;

impl Value {
    pub(crate) fn get(&self, key: &str) -> &Value {
        match self {
            Value::Obj(members) => members
                .iter()
                .find(|(k, _)| k.as_str() == key)
                .map(|(_, v)| v)
                .unwrap_or(&UNDEF),
            _ => &UNDEF,
        }
    }

    /// Walks `a.b.0.c`: object keys and array indexes; anything absent reads as `Undef`.
    pub(crate) fn at(&self, path: &str) -> &Value {
        let mut cur = self;
        for part in path.split('.') {
            cur = match cur {
                Value::Arr(items) => match part.parse::<usize>() {
                    Ok(i) => items.get(i).unwrap_or(&UNDEF),
                    Err(_) => &UNDEF,
                },
                other => other.get(part),
            };
        }
        cur
    }

    pub(crate) fn keys(&self) -> Vec<&str> {
        match self {
            Value::Obj(members) => members.iter().map(|(k, _)| k.as_str()).collect(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn is_undef(&self) -> bool {
        matches!(self, Value::Undef)
    }

    pub(crate) fn is_obj(&self) -> bool {
        matches!(self, Value::Obj(_))
    }

    pub(crate) fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(x) => Some(*x),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub(crate) fn items(&self) -> &[Value] {
        match self {
            Value::Arr(items) => items.as_slice(),
            _ => &[],
        }
    }

    pub(crate) fn same(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Undef, Value::Undef) | (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Num(a), Value::Num(b)) => {
                a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
            }
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Arr(a), Value::Arr(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.same(y))
            }
            (Value::Obj(a), Value::Obj(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|((ka, va), (kb, vb))| ka == kb && va.same(vb))
            }
            _ => false,
        }
    }

    /// Compact text for failure messages, clipped so a large array cannot flood the output.
    pub(crate) fn show(&self) -> String {
        let mut s = String::new();
        self.write(&mut s);
        if s.len() > 160 {
            let mut cut = 160;
            while !s.is_char_boundary(cut) {
                cut -= 1;
            }
            s.truncate(cut);
            s.push_str("...");
        }
        s
    }

    fn write(&self, out: &mut String) {
        match self {
            Value::Undef => out.push('!'),
            Value::Null => out.push('^'),
            Value::Bool(true) => out.push('T'),
            Value::Bool(false) => out.push('F'),
            Value::Num(x) => out.push_str(&format!("{}", x)),
            Value::Str(s) => out.push_str(&format!("{:?}", s)),
            Value::Arr(items) => {
                out.push('[');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write(out);
                }
                out.push(']');
            }
            Value::Obj(members) => {
                out.push('{');
                for (i, (k, v)) in members.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&format!("{:?}:", k));
                    v.write(out);
                }
                out.push('}');
            }
        }
    }
}

pub(crate) fn parse_value(text: &str) -> Result<Value, String> {
    let mut p = Parser {
        b: text.as_bytes(),
        i: 0,
    };
    let v = p.value()?;
    if p.i != p.b.len() {
        return Err(format!("trailing bytes at {}", p.i));
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected {:?} at {}", c as char, self.i))
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        let c = match self.peek() {
            Some(c) => c,
            None => return Err("unexpected end of input".to_string()),
        };
        match c {
            b'!' => self.atom(Value::Undef),
            b'^' => self.atom(Value::Null),
            b'T' => self.atom(Value::Bool(true)),
            b'F' => self.atom(Value::Bool(false)),
            b'"' => self.string().map(Value::Str),
            b'[' => self.array(),
            b'{' => self.object(),
            b'n' => self.hex_number(),
            b'-' | b'0'..=b'9' => self.decimal(),
            other => Err(format!("unexpected {:?} at {}", other as char, self.i)),
        }
    }

    fn atom(&mut self, v: Value) -> Result<Value, String> {
        self.i += 1;
        Ok(v)
    }

    fn hex_number(&mut self) -> Result<Value, String> {
        let start = self.i + 1;
        let digits = match self.b.get(start..start + 16) {
            Some(d) => d,
            None => return Err(format!("short hex number at {}", self.i)),
        };
        if !digits
            .iter()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
        {
            return Err(format!("non-hex digit in number at {}", self.i));
        }
        let text = std::str::from_utf8(digits).map_err(|e| e.to_string())?;
        let bits = u64::from_str_radix(text, 16).map_err(|e| e.to_string())?;
        self.i = start + 16;
        Ok(Value::Num(f64::from_bits(bits)))
    }

    fn decimal(&mut self) -> Result<Value, String> {
        let start = self.i;
        while let Some(c) = self.peek() {
            if matches!(c, b',' | b']' | b'}' | b':') {
                break;
            }
            self.i += 1;
        }
        let text = std::str::from_utf8(&self.b[start..self.i]).map_err(|e| e.to_string())?;
        match text.parse::<f64>() {
            Ok(x) if x.is_finite() => Ok(Value::Num(x)),
            _ => Err(format!("bad decimal number {:?}", text)),
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = match self.peek() {
                Some(c) => c,
                None => return Err("unterminated string".to_string()),
            };
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = match self.peek() {
                        Some(e) => e,
                        None => return Err("dangling escape".to_string()),
                    };
                    self.i += 1;
                    match e {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'u' => {
                            let hex = match self.b.get(self.i..self.i + 4) {
                                Some(h) => h,
                                None => return Err("short \\u escape".to_string()),
                            };
                            let text = std::str::from_utf8(hex).map_err(|e| e.to_string())?;
                            let cp = u32::from_str_radix(text, 16).map_err(|e| e.to_string())?;
                            let ch = match char::from_u32(cp) {
                                Some(ch) => ch,
                                None => return Err("surrogate escape".to_string()),
                            };
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                            self.i += 4;
                        }
                        other => return Err(format!("bad escape {:?}", other as char)),
                    }
                }
                other => out.push(other),
            }
        }
        String::from_utf8(out).map_err(|e| e.to_string())
    }

    fn array(&mut self) -> Result<Value, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Value::Arr(items));
        }
        loop {
            items.push(self.value()?);
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::Arr(items));
                }
                _ => return Err(format!("bad array separator at {}", self.i)),
            }
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.expect(b'{')?;
        let mut members = Vec::new();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Value::Obj(members));
        }
        loop {
            let key = self.string()?;
            self.expect(b':')?;
            members.push((key, self.value()?));
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Obj(members));
                }
                _ => return Err(format!("bad object separator at {}", self.i)),
            }
        }
    }
}
