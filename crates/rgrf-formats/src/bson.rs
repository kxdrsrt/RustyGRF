//! BSON files of the client (`data\contentdata\*.bson` and others) as JSON text, and back.
//! The format is MongoDB's BSON (GRF Editor's `Bson`/`Json`, with the permission of its author,
//! see `docs/05-licensing.md`). Values JSON has no name for are written as `{"$oid": ...}`,
//! `{"$date": ms}`, `{"$binary": hex, "$type": n}`, `{"$timestamp": n}`, `{"$undefined": true}`,
//! and a 64-bit integer that would read back as a 32-bit one as `{"$numberLong": "n"}`, so that
//! the round trip gives the same bytes.

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BsonError {
    #[error("the BSON data ends in the middle of a value (at byte {0})")]
    Truncated(usize),
    #[error("unsupported BSON type {0} at byte {1}")]
    Type(u8, usize),
    #[error("a document at byte {0} does not end where it says")]
    Length(usize),
    #[error("JSON: {0}")]
    Json(String),
    #[error("this value cannot be written as BSON: {0}")]
    Unwritable(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Double(f64),
    String(String),
    Doc(Vec<(String, Value)>),
    Array(Vec<Value>),
    Binary(u8, Vec<u8>),
    Undefined,
    ObjectId([u8; 12]),
    Bool(bool),
    Date(i64),
    Null,
    Int32(i32),
    Timestamp(u64),
    Int64(i64),
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], BsonError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&e| e <= self.b.len())
            .ok_or(BsonError::Truncated(self.at))?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn i32(&mut self) -> Result<i32, BsonError> {
        let s = self.take(4)?;
        Ok(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn i64(&mut self) -> Result<i64, BsonError> {
        let s = self.take(8)?;
        Ok(i64::from_le_bytes(s.try_into().expect("8 bytes")))
    }

    fn cstring(&mut self) -> Result<String, BsonError> {
        let rest = &self.b[self.at..];
        let n = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or(BsonError::Truncated(self.at))?;
        let s = String::from_utf8_lossy(&rest[..n]).into_owned();
        self.at += n + 1;
        Ok(s)
    }

    fn document(&mut self) -> Result<Vec<(String, Value)>, BsonError> {
        let start = self.at;
        let len = usize::try_from(self.i32()?).map_err(|_| BsonError::Length(start))?;
        let end = start
            .checked_add(len)
            .filter(|&e| len >= 5 && e <= self.b.len())
            .ok_or(BsonError::Length(start))?;
        let mut items = Vec::new();
        while self.at + 1 < end {
            let at = self.at;
            let kind = self.take(1)?[0];
            let key = self.cstring()?;
            let v = match kind {
                1 => Value::Double(f64::from_le_bytes(
                    self.take(8)?.try_into().expect("8 bytes"),
                )),
                2 => {
                    let n = usize::try_from(self.i32()?).map_err(|_| BsonError::Truncated(at))?;
                    let s = self.take(n.max(1))?;
                    Value::String(String::from_utf8_lossy(&s[..s.len() - 1]).into_owned())
                }
                3 => Value::Doc(self.document()?),
                4 => Value::Array(self.document()?.into_iter().map(|(_, v)| v).collect()),
                5 => {
                    let n = usize::try_from(self.i32()?).map_err(|_| BsonError::Truncated(at))?;
                    let sub = self.take(1)?[0];
                    Value::Binary(sub, self.take(n)?.to_vec())
                }
                6 => Value::Undefined,
                7 => Value::ObjectId(self.take(12)?.try_into().expect("12 bytes")),
                8 => Value::Bool(self.take(1)?[0] != 0),
                9 => Value::Date(self.i64()?),
                10 => Value::Null,
                16 => Value::Int32(self.i32()?),
                17 => Value::Timestamp(self.i64()? as u64),
                18 => Value::Int64(self.i64()?),
                k => return Err(BsonError::Type(k, at)),
            };
            items.push((key, v));
        }
        if self.take(1)?[0] != 0 || self.at != end {
            return Err(BsonError::Length(start));
        }
        Ok(items)
    }
}

/// The document of a BSON file.
pub fn parse(bytes: &[u8]) -> Result<Vec<(String, Value)>, BsonError> {
    Reader { b: bytes, at: 0 }.document()
}

fn put_cstring(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(s.as_bytes());
    out.push(0);
}

fn write_document(out: &mut Vec<u8>, items: &[(String, &Value)]) {
    let start = out.len();
    out.extend([0; 4]);
    for (k, v) in items {
        let (kind, body) = encode(v);
        out.push(kind);
        put_cstring(out, k);
        out.extend(body);
    }
    out.push(0);
    let len = (out.len() - start) as i32;
    out[start..start + 4].copy_from_slice(&len.to_le_bytes());
}

fn encode(v: &Value) -> (u8, Vec<u8>) {
    let mut b = Vec::new();
    let kind = match v {
        Value::Double(d) => {
            b.extend(d.to_le_bytes());
            1
        }
        Value::String(s) => {
            b.extend((s.len() as i32 + 1).to_le_bytes());
            put_cstring(&mut b, s);
            2
        }
        Value::Doc(items) => {
            let refs: Vec<(String, &Value)> = items.iter().map(|(k, v)| (k.clone(), v)).collect();
            write_document(&mut b, &refs);
            3
        }
        Value::Array(items) => {
            let refs: Vec<(String, &Value)> = items
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v))
                .collect();
            write_document(&mut b, &refs);
            4
        }
        Value::Binary(sub, data) => {
            b.extend((data.len() as i32).to_le_bytes());
            b.push(*sub);
            b.extend_from_slice(data);
            5
        }
        Value::Undefined => 6,
        Value::ObjectId(id) => {
            b.extend_from_slice(id);
            7
        }
        Value::Bool(x) => {
            b.push(u8::from(*x));
            8
        }
        Value::Date(ms) => {
            b.extend(ms.to_le_bytes());
            9
        }
        Value::Null => 10,
        Value::Int32(n) => {
            b.extend(n.to_le_bytes());
            16
        }
        Value::Timestamp(n) => {
            b.extend(n.to_le_bytes());
            17
        }
        Value::Int64(n) => {
            b.extend(n.to_le_bytes());
            18
        }
    };
    (kind, b)
}

/// The bytes of a BSON file with this document.
pub fn to_bytes(doc: &[(String, Value)]) -> Vec<u8> {
    let mut out = Vec::new();
    let refs: Vec<(String, &Value)> = doc.iter().map(|(k, v)| (k.clone(), v)).collect();
    write_document(&mut out, &refs);
    out
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_json(v: &Value, depth: usize, out: &mut String) {
    let pad = |n: usize| "  ".repeat(n);
    match v {
        Value::Double(d) if d.is_finite() => {
            // a whole double keeps its ".0" so that it reads back as a double
            if d.fract() == 0.0 && d.abs() < 1e15 {
                out.push_str(&format!("{d:.1}"));
            } else {
                out.push_str(&format!("{d}"));
            }
        }
        Value::Double(d) => out.push_str(&format!("{{\"$double\": \"{d}\"}}")),
        Value::String(s) => json_string(s, out),
        Value::Doc(items) if items.is_empty() => out.push_str("{}"),
        Value::Doc(items) => {
            out.push_str("{\n");
            for (i, (k, v)) in items.iter().enumerate() {
                out.push_str(&pad(depth + 1));
                json_string(k, out);
                out.push_str(": ");
                write_json(v, depth + 1, out);
                out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad(depth));
            out.push('}');
        }
        Value::Array(items) if items.is_empty() => out.push_str("[]"),
        Value::Array(items) => {
            out.push_str("[\n");
            for (i, v) in items.iter().enumerate() {
                out.push_str(&pad(depth + 1));
                write_json(v, depth + 1, out);
                out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad(depth));
            out.push(']');
        }
        Value::Binary(sub, d) => out.push_str(&format!(
            "{{\"$binary\": \"{}\", \"$type\": {sub}}}",
            hex(d)
        )),
        Value::Undefined => out.push_str("{\"$undefined\": true}"),
        Value::ObjectId(id) => out.push_str(&format!("{{\"$oid\": \"{}\"}}", hex(id))),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Date(ms) => out.push_str(&format!("{{\"$date\": {ms}}}")),
        Value::Null => out.push_str("null"),
        Value::Int32(n) => out.push_str(&n.to_string()),
        Value::Timestamp(n) => out.push_str(&format!("{{\"$timestamp\": {n}}}")),
        Value::Int64(n) if i32::try_from(*n).is_ok() => {
            out.push_str(&format!("{{\"$numberLong\": \"{n}\"}}"));
        }
        Value::Int64(n) => out.push_str(&n.to_string()),
    }
}

/// The document as JSON text.
pub fn to_json(doc: &[(String, Value)]) -> String {
    let mut s = String::new();
    write_json(&Value::Doc(doc.to_vec()), 0, &mut s);
    s.push('\n');
    s
}

struct Json<'a> {
    b: &'a [u8],
    at: usize,
}

impl Json<'_> {
    fn err<T>(&self, what: &str) -> Result<T, BsonError> {
        Err(BsonError::Json(format!("{what} at byte {}", self.at)))
    }

    fn ws(&mut self) {
        while self.b.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.b.get(self.at) == Some(&c) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn string(&mut self) -> Result<String, BsonError> {
        if !self.eat(b'"') {
            return self.err("a string was expected");
        }
        let mut out = String::new();
        loop {
            let Some(&c) = self.b.get(self.at) else {
                return self.err("the string does not end");
            };
            self.at += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let Some(&e) = self.b.get(self.at) else {
                        return self.err("the string does not end");
                    };
                    self.at += 1;
                    match e {
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'/' | b'\\' | b'"' => out.push(e as char),
                        b'u' => {
                            let mut unit = self.hex4()?;
                            if (0xd800..0xdc00).contains(&unit)
                                && self.b[self.at..].starts_with(b"\\u")
                            {
                                self.at += 2;
                                let low = self.hex4()?;
                                unit = 0x10000
                                    + ((unit - 0xd800) << 10)
                                    + (low.wrapping_sub(0xdc00) & 0x3ff);
                            }
                            out.push(char::from_u32(unit).unwrap_or('\u{fffd}'));
                        }
                        _ => return self.err("unknown escape"),
                    }
                }
                _ => {
                    // the bytes of a UTF-8 character stay together
                    let start = self.at - 1;
                    let len = match c {
                        0..=0x7f => 1,
                        0xc0..=0xdf => 2,
                        0xe0..=0xef => 3,
                        _ => 4,
                    };
                    let end = (start + len).min(self.b.len());
                    out.push_str(&String::from_utf8_lossy(&self.b[start..end]));
                    self.at = end;
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, BsonError> {
        let s = self.b.get(self.at..self.at + 4);
        let v = s
            .and_then(|s| std::str::from_utf8(s).ok())
            .and_then(|s| u32::from_str_radix(s, 16).ok());
        match v {
            Some(v) => {
                self.at += 4;
                Ok(v)
            }
            None => self.err("a \\u escape needs four hex digits"),
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, BsonError> {
        if depth > 128 {
            return self.err("nested too deeply");
        }
        self.ws();
        match self.b.get(self.at) {
            Some(b'{') => {
                self.at += 1;
                let mut items: Vec<(String, Value)> = Vec::new();
                if !self.eat(b'}') {
                    loop {
                        let k = self.string()?;
                        if !self.eat(b':') {
                            return self.err("':' was expected");
                        }
                        items.push((k, self.value(depth + 1)?));
                        if self.eat(b',') {
                            continue;
                        }
                        if self.eat(b'}') {
                            break;
                        }
                        return self.err("',' or '}' was expected");
                    }
                }
                Ok(special(items))
            }
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                if !self.eat(b']') {
                    loop {
                        items.push(self.value(depth + 1)?);
                        if self.eat(b',') {
                            continue;
                        }
                        if self.eat(b']') {
                            break;
                        }
                        return self.err("',' or ']' was expected");
                    }
                }
                Ok(Value::Array(items))
            }
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b't') if self.b[self.at..].starts_with(b"true") => {
                self.at += 4;
                Ok(Value::Bool(true))
            }
            Some(b'f') if self.b[self.at..].starts_with(b"false") => {
                self.at += 5;
                Ok(Value::Bool(false))
            }
            Some(b'n') if self.b[self.at..].starts_with(b"null") => {
                self.at += 4;
                Ok(Value::Null)
            }
            Some(c) if c.is_ascii_digit() || *c == b'-' => {
                let start = self.at;
                while self
                    .b
                    .get(self.at)
                    .is_some_and(|c| c.is_ascii_digit() || b"+-.eE".contains(c))
                {
                    self.at += 1;
                }
                let text = std::str::from_utf8(&self.b[start..self.at]).unwrap_or("");
                if let Ok(n) = text.parse::<i64>() {
                    return Ok(i32::try_from(n).map_or(Value::Int64(n), Value::Int32));
                }
                text.parse::<f64>()
                    .map(Value::Double)
                    .or_else(|_| self.err("a bad number"))
            }
            _ => self.err("a value was expected"),
        }
    }
}

/// The `$`-objects this module writes become their BSON types again.
fn special(items: Vec<(String, Value)>) -> Value {
    let get = |k: &str| items.iter().find(|(n, _)| n == k).map(|(_, v)| v);
    let unhex = |s: &str| -> Option<Vec<u8>> {
        s.len()
            .is_multiple_of(2)
            .then(|| {
                (0..s.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
                    .collect()
            })
            .flatten()
    };
    if items.len() == 1 {
        match (&items[0].0[..], &items[0].1) {
            ("$oid", Value::String(s)) => {
                if let Some(id) = unhex(s).and_then(|b| <[u8; 12]>::try_from(b).ok()) {
                    return Value::ObjectId(id);
                }
            }
            ("$date", Value::Int32(n)) => return Value::Date(i64::from(*n)),
            ("$date", Value::Int64(n)) => return Value::Date(*n),
            ("$timestamp", Value::Int32(n)) if *n >= 0 => return Value::Timestamp(*n as u64),
            ("$timestamp", Value::Int64(n)) if *n >= 0 => return Value::Timestamp(*n as u64),
            ("$undefined", Value::Bool(true)) => return Value::Undefined,
            ("$numberLong", Value::String(s)) => {
                if let Ok(n) = s.parse() {
                    return Value::Int64(n);
                }
            }
            ("$double", Value::String(s)) => {
                if let Ok(n) = s.parse() {
                    return Value::Double(n);
                }
            }
            _ => {}
        }
    }
    if items.len() == 2
        && let (Some(Value::String(h)), Some(Value::Int32(t))) = (get("$binary"), get("$type"))
        && let (Some(d), Ok(t)) = (unhex(h), u8::try_from(*t))
    {
        return Value::Binary(t, d);
    }
    Value::Doc(items)
}

/// The document of a JSON text (it must be an object).
pub fn from_json(text: &str) -> Result<Vec<(String, Value)>, BsonError> {
    let mut j = Json {
        b: text.trim_start_matches('\u{feff}').as_bytes(),
        at: 0,
    };
    let v = j.value(0)?;
    j.ws();
    if j.at != j.b.len() {
        return j.err("text after the value");
    }
    match v {
        Value::Doc(items) => Ok(items),
        _ => Err(BsonError::Unwritable(
            "the top level must be an object".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<(String, Value)> {
        vec![
            (
                "name".into(),
                Value::String("Poring \"x\"\n\u{ac00}".into()),
            ),
            ("n".into(), Value::Int32(-5)),
            ("big".into(), Value::Int64(1 << 40)),
            ("small_long".into(), Value::Int64(7)),
            ("ratio".into(), Value::Double(2.0)),
            ("half".into(), Value::Double(0.5)),
            ("ok".into(), Value::Bool(true)),
            ("none".into(), Value::Null),
            (
                "list".into(),
                Value::Array(vec![
                    Value::Int32(1),
                    Value::Doc(vec![]),
                    Value::Array(vec![]),
                ]),
            ),
            ("oid".into(), Value::ObjectId([1; 12])),
            ("when".into(), Value::Date(1_700_000_000_000)),
            ("bin".into(), Value::Binary(0, vec![0, 255, 16])),
            (
                "inner".into(),
                Value::Doc(vec![
                    ("k".into(), Value::Undefined),
                    ("t".into(), Value::Timestamp(9)),
                ]),
            ),
        ]
    }

    #[test]
    fn bson_goes_through_json_and_back_to_the_same_bytes() {
        let doc = sample();
        let bytes = to_bytes(&doc);
        assert_eq!(parse(&bytes).unwrap(), doc);
        let json = to_json(&doc);
        assert!(json.contains("\"ratio\": 2.0") && json.contains("\"$numberLong\": \"7\""));
        assert_eq!(from_json(&json).unwrap(), doc);
        assert_eq!(to_bytes(&from_json(&json).unwrap()), bytes);
    }

    #[test]
    fn damaged_bson_and_bad_json_are_refused() {
        let bytes = to_bytes(&sample());
        assert!(parse(&bytes[..bytes.len() - 3]).is_err());
        let mut bad = bytes.clone();
        bad[4] = 99; // an unknown type
        assert!(matches!(parse(&bad), Err(BsonError::Type(99, 4))));
        for text in ["", "[1]", "{\"a\": }", "{\"a\": 1} x", "{\"a\": \"\\x\"}"] {
            assert!(from_json(text).is_err(), "{text}");
        }
        assert_eq!(
            from_json("{\"s\": \"\\ud83d\\ude00\"}").unwrap()[0].1,
            Value::String("\u{1f600}".into())
        );
    }
}
