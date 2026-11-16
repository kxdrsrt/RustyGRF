//! Archive paths are bytes. Decoding to text is for display and for file names on disk only.

use std::fmt;

/// How path bytes are shown or written to disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NameMode {
    /// Windows-1252, as GRF Editor / GrfCL show names (Korean names appear as mojibake). Default,
    /// because it round-trips and matches existing extracted folders.
    #[default]
    Cp1252,
    /// Windows-949 (Korean), readable names. Falls back to `Raw` for bytes that are not valid CP949.
    Cp949,
    /// UTF-8, only if the bytes are valid UTF-8; otherwise falls back to `Raw`.
    Utf8,
    /// Every byte as the code point of the same value (Latin-1). Lossless, for display only.
    Raw,
}

impl NameMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "cp1252" | "1252" | "windows-1252" => Some(Self::Cp1252),
            "cp949" | "949" | "korean" => Some(Self::Cp949),
            "utf8" | "utf-8" => Some(Self::Utf8),
            "raw" | "latin1" => Some(Self::Raw),
            _ => None,
        }
    }
}

const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

fn cp1252_char(b: u8) -> char {
    match b {
        0x80..=0x9F => CP1252_HIGH[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}

fn cp1252_byte(c: char) -> Option<u8> {
    let cp = u32::from(c);
    if cp < 0x80 || (0xA0..=0xFF).contains(&cp) {
        return u8::try_from(cp).ok();
    }
    CP1252_HIGH
        .iter()
        .position(|&h| h == c)
        .and_then(|i| u8::try_from(0x80 + i).ok())
}

/// A path inside an archive: raw bytes, separator `\` as stored.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GrfPath(Box<[u8]>);

impl GrfPath {
    pub fn new(bytes: impl Into<Box<[u8]>>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Text for display or for a file name, see [`NameMode`].
    pub fn display(&self, mode: NameMode) -> String {
        match mode {
            NameMode::Cp1252 => self.0.iter().map(|&b| cp1252_char(b)).collect(),
            NameMode::Raw => self.0.iter().map(|&b| char::from(b)).collect(),
            NameMode::Utf8 => match std::str::from_utf8(&self.0) {
                Ok(s) => s.to_owned(),
                Err(_) => self.display(NameMode::Raw),
            },
            NameMode::Cp949 => {
                let (text, had_errors) = encoding_rs::EUC_KR.decode_without_bom_handling(&self.0);
                if had_errors {
                    self.display(NameMode::Raw)
                } else {
                    text.into_owned()
                }
            }
        }
    }

    /// Inverse of [`GrfPath::display`]: the bytes a user-typed name stands for, if representable.
    pub fn from_display(text: &str, mode: NameMode) -> Option<Self> {
        let bytes: Vec<u8> = match mode {
            NameMode::Cp1252 => text.chars().map(cp1252_byte).collect::<Option<_>>()?,
            NameMode::Raw => text
                .chars()
                .map(|c| u8::try_from(u32::from(c)).ok())
                .collect::<Option<_>>()?,
            NameMode::Utf8 => text.as_bytes().to_vec(),
            NameMode::Cp949 => {
                let (bytes, _, had_errors) = encoding_rs::EUC_KR.encode(text);
                if had_errors {
                    return None;
                }
                bytes.into_owned()
            }
        };
        Some(Self::new(bytes))
    }

    /// Key for comparing paths like the client does: ASCII case-insensitive, `/` equals `\`.
    /// Double-byte characters (lead byte 0x81-0xFE plus a valid CP949 trail byte) are kept as they
    /// are, so the trail bytes of Korean characters are not case-folded. The client's exact rule is
    /// not verified (only 9 of 281,303 real names are affected, and none differ by trail case).
    pub fn key(&self) -> Vec<u8> {
        let b = &self.0;
        let mut out = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            let is_trail = |x: u8| matches!(x, 0x41..=0x5A | 0x61..=0x7A | 0x81..=0xFE);
            if b[i] >= 0x81 && i + 1 < b.len() && is_trail(b[i + 1]) {
                out.extend_from_slice(&b[i..i + 2]);
                i += 2;
            } else {
                out.push(if b[i] == b'/' {
                    b'\\'
                } else {
                    b[i].to_ascii_lowercase()
                });
                i += 1;
            }
        }
        out
    }

    pub fn file_name_bytes(&self) -> &[u8] {
        let i = self.0.iter().rposition(|&b| b == b'\\' || b == b'/');
        i.map_or(&self.0[..], |i| &self.0[i + 1..])
    }

    pub fn extension_lowercase(&self) -> Option<Vec<u8>> {
        let name = self.file_name_bytes();
        let dot = name.iter().rposition(|&b| b == b'.')?;
        Some(name[dot + 1..].to_ascii_lowercase())
    }
}

impl fmt::Debug for GrfPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GrfPath({:?})", self.display(NameMode::Raw))
    }
}

impl fmt::Display for GrfPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display(NameMode::default()))
    }
}

/// Lowercase hexadecimal of raw bytes (stable machine-readable form of a name).
pub fn hex(bytes: &[u8]) -> String {
    use fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Case-insensitive `*`/`?` wildcard match on archive paths (`/` and `\` are equal).
pub fn glob_match(pattern: &str, path: &str) -> bool {
    fn norm(s: &str) -> Vec<char> {
        s.chars()
            .map(|c| {
                if c == '/' {
                    '\\'
                } else {
                    c.to_ascii_lowercase()
                }
            })
            .collect()
    }
    let (p, t) = (norm(pattern), norm(path));
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cp1252_roundtrips_every_byte() {
        let all: Vec<u8> = (0..=255u8).collect();
        let p = GrfPath::new(all.clone());
        let shown = p.display(NameMode::Cp1252);
        assert_eq!(shown.chars().count(), 256);
        assert_eq!(
            GrfPath::from_display(&shown, NameMode::Cp1252)
                .unwrap()
                .as_bytes(),
            &all[..]
        );
    }

    #[test]
    fn raw_roundtrips_every_byte() {
        let all: Vec<u8> = (0..=255u8).collect();
        let p = GrfPath::new(all.clone());
        let shown = p.display(NameMode::Raw);
        assert_eq!(
            GrfPath::from_display(&shown, NameMode::Raw)
                .unwrap()
                .as_bytes(),
            &all[..]
        );
    }

    #[test]
    fn cp1252_matches_grf_editor_mojibake() {
        // "유저인터페이스" in CP949, as GRF Editor shows it (see tests/golden/README.md)
        let bytes = [
            0xC0, 0xAF, 0xC0, 0xFA, 0xC0, 0xCE, 0xC5, 0xCD, 0xC6, 0xE4, 0xC0, 0xCC, 0xBD, 0xBA,
        ];
        let p = GrfPath::new(bytes.to_vec());
        assert_eq!(p.display(NameMode::Cp1252), "À¯ÀúÀÎÅÍÆäÀÌ½º");
        assert_eq!(p.display(NameMode::Cp949), "유저인터페이스");
        assert_eq!(
            GrfPath::from_display("유저인터페이스", NameMode::Cp949)
                .unwrap()
                .as_bytes(),
            &bytes
        );
    }

    #[test]
    fn utf8_and_cp949_fall_back_to_raw_when_invalid() {
        let p = GrfPath::new(vec![b'a', 0xFF, 0xFE]);
        assert_eq!(p.display(NameMode::Utf8), "a\u{ff}\u{fe}");
        assert_eq!(p.display(NameMode::Cp949), "a\u{ff}\u{fe}");
    }

    #[test]
    fn key_ignores_ascii_case_and_separator() {
        let a = GrfPath::new(b"data/Sprite\\A.SPR".to_vec());
        let b = GrfPath::new(b"DATA\\sprite/a.spr".to_vec());
        assert_eq!(a.key(), b.key());
        let c = GrfPath::new(vec![b'd', 0xC0]);
        let d = GrfPath::new(vec![b'D', 0xE0]);
        assert_ne!(c.key(), d.key(), "non-ASCII bytes are not folded");
    }

    #[test]
    fn key_does_not_fold_trail_bytes_of_double_byte_characters() {
        let upper_trail = GrfPath::new(vec![b'd', 0xB0, 0x41, b'x']);
        let lower_trail = GrfPath::new(vec![b'd', 0xB0, 0x61, b'x']);
        assert_ne!(upper_trail.key(), lower_trail.key());
        // a separator after a high byte is still a separator
        let slash = GrfPath::new(vec![b'a', 0xC0, b'/', b'b']);
        let back = GrfPath::new(vec![b'a', 0xC0, b'\\', b'B']);
        assert_eq!(slash.key(), back.key());
    }

    #[test]
    fn glob_works() {
        assert!(glob_match("data/sprite/*.spr", "data\\sprite\\a\\b.SPR"));
        assert!(glob_match("*job4*", "data\\x\\Job4_a.act"));
        assert!(glob_match("a?c", "ABC"));
        assert!(!glob_match("a?c", "ac"));
        assert!(glob_match("*", ""));
        assert!(!glob_match("*.spr", "x.act"));
    }

    #[test]
    fn extension_and_file_name() {
        let p = GrfPath::new(b"data\\a\\B.Spr".to_vec());
        assert_eq!(p.file_name_bytes(), b"B.Spr");
        assert_eq!(p.extension_lowercase().unwrap(), b"spr");
        assert!(
            GrfPath::new(b"data\\noext".to_vec())
                .extension_lowercase()
                .is_none()
        );
    }
}
