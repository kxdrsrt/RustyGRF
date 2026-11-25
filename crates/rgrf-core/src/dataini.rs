//! The client's `DATA.INI`: which archives the client reads, in which order.
//!
//! ```text
//! [Data]
//! 0=odin.grf
//! 1=cdata.grf
//! 2=data.grf
//! ```
//!
//! The **lowest index wins** (it is the highest priority), measured on the client. Index 0 is read too (measured on the
//! 2018-06-20 client, `0=` first entry); a file may also start at 1.
//!
//! This module reads and edits the file without losing anything else in it: the line ending (CRLF or LF), file names as the
//! raw bytes they are (CP949), comments and other sections are written back as they were. After every edit the entries are
//! numbered one after the other again, starting at the index the file started with (a file that starts at 1 keeps starting
//! at 1). Archive names are compared ignoring ASCII case, like the file system of the client does.

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    index: u32,
    /// The name as it is in the file (bytes, usually ASCII).
    name: Vec<u8>,
}

/// Where an entry goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// Highest priority (the lowest index), like a mod archive.
    First,
    /// Lowest priority.
    Last,
    /// This position in the order, counted from 0 (clamped to the end).
    At(usize),
    Before(String),
    After(String),
}

/// One line of the listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub index: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataIni {
    /// Everything up to and including the line before the first entry (usually `[Data]`).
    head: Vec<Vec<u8>>,
    entries: Vec<Entry>,
    /// Everything after the entries (other sections, comments between entries land here).
    tail: Vec<Vec<u8>>,
    newline: &'static [u8],
    start: u32,
    ended_with_newline: bool,
}

fn is_data_header(line: &[u8]) -> bool {
    let t = trim(line);
    t.len() >= 6
        && t[0] == b'['
        && t[t.len() - 1] == b']'
        && t[1..t.len() - 1].eq_ignore_ascii_case(b"data")
}

fn trim(b: &[u8]) -> &[u8] {
    let s = b
        .iter()
        .position(|c| !c.is_ascii_whitespace())
        .unwrap_or(b.len());
    let e = b
        .iter()
        .rposition(|c| !c.is_ascii_whitespace())
        .map_or(s, |p| p + 1);
    &b[s..e]
}

/// `12=data.grf` -> (12, "data.grf"); anything else (comments, other keys) -> None.
fn parse_entry(line: &[u8]) -> Option<Entry> {
    let t = trim(line);
    let eq = t.iter().position(|&c| c == b'=')?;
    let key = trim(&t[..eq]);
    let name = trim(&t[eq + 1..]);
    if key.is_empty() || !key.iter().all(u8::is_ascii_digit) || name.is_empty() {
        return None;
    }
    let index = std::str::from_utf8(key).ok()?.parse().ok()?;
    Some(Entry {
        index,
        name: name.to_vec(),
    })
}

/// Text of a name for display: UTF-8 if it is valid, else CP949 (the client's encoding for Korean names).
pub fn display_name(name: &[u8]) -> String {
    match std::str::from_utf8(name) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::EUC_KR.decode(name).0.into_owned(),
    }
}

/// The bytes a name typed by the user is written as: ASCII as it is, anything else in CP949.
fn encode_name(name: &str) -> Result<Vec<u8>> {
    if name.is_ascii() {
        return Ok(name.as_bytes().to_vec());
    }
    let (bytes, _, bad) = encoding_rs::EUC_KR.encode(name);
    if bad {
        return Err(Error::InvalidInput(format!(
            "{name}: the name cannot be written in CP949, the client's encoding"
        )));
    }
    Ok(bytes.into_owned())
}

fn check_name(name: &str) -> Result<Vec<u8>> {
    let t = name.trim();
    if t.is_empty() {
        return Err(Error::InvalidInput("an archive name is empty".into()));
    }
    if t.contains(['/', '\\', '=', '\r', '\n']) {
        return Err(Error::InvalidInput(format!(
            "{t}: DATA.INI lists archive names next to the client, without a path (and without '=')"
        )));
    }
    encode_name(t)
}

impl DataIni {
    /// Reads the text of a `DATA.INI`. Never fails: an empty or odd file gives an empty list.
    pub fn parse(bytes: &[u8]) -> Self {
        // the client is a Windows program: an empty file gets CRLF
        let newline: &'static [u8] = if bytes.is_empty() || bytes.windows(2).any(|w| w == b"\r\n") {
            b"\r\n"
        } else {
            b"\n"
        };
        let ended = bytes.last().is_some_and(|&c| c == b'\n');
        let mut lines: Vec<&[u8]> = if bytes.is_empty() {
            Vec::new()
        } else {
            bytes.split(|&c| c == b'\n').collect()
        };
        if ended {
            lines.pop();
        }
        let (mut head, mut tail, mut entries) = (Vec::new(), Vec::new(), Vec::new());
        let mut in_data = false;
        for raw in lines {
            let line = raw.strip_suffix(b"\r").unwrap_or(raw);
            if trim(line).first() == Some(&b'[') {
                in_data = is_data_header(line);
                if entries.is_empty() {
                    head.push(line.to_vec())
                } else {
                    tail.push(line.to_vec())
                }
                continue;
            }
            match in_data.then(|| parse_entry(line)).flatten() {
                Some(e) => {
                    // after other sections' lines a later [Data] entry would be odd; keep them together anyway
                    entries.push(e);
                }
                None if entries.is_empty() => head.push(line.to_vec()),
                None => tail.push(line.to_vec()),
            }
        }
        // priority order: the lowest index first (stable for equal indexes, as in the file)
        entries.sort_by_key(|e| e.index);
        let start = entries.first().map_or(0, |e| e.index);
        Self {
            head,
            entries,
            tail,
            newline,
            start,
            ended_with_newline: ended || bytes.is_empty(),
        }
    }

    /// The archives in priority order (the first wins).
    pub fn items(&self) -> Vec<Item> {
        self.entries
            .iter()
            .map(|e| Item {
                index: e.index,
                name: display_name(&e.name),
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The index the numbering starts at.
    pub fn start(&self) -> u32 {
        self.start
    }

    /// Entries that share an index (which one wins is not measured).
    pub fn duplicate_indexes(&self) -> Vec<u32> {
        let mut d: Vec<u32> = self
            .entries
            .windows(2)
            .filter(|w| w[0].index == w[1].index)
            .map(|w| w[0].index)
            .collect();
        d.dedup();
        d
    }

    /// Position in the order (0 = first), ignoring ASCII case.
    pub fn position(&self, name: &str) -> Option<usize> {
        let n = encode_name(name.trim()).ok()?;
        self.entries
            .iter()
            .position(|e| e.name.eq_ignore_ascii_case(&n))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.position(name).is_some()
    }

    fn resolve(
        &self,
        place: &Place,
        len_after_removal: usize,
        skip: Option<usize>,
    ) -> Result<usize> {
        let find = |n: &str| -> Result<usize> {
            let p = self
                .position(n)
                .ok_or_else(|| Error::InvalidInput(format!("{n} is not in DATA.INI")))?;
            if skip == Some(p) {
                return Err(Error::InvalidInput(format!(
                    "{n}: cannot be placed relative to itself"
                )));
            }
            // positions after the entry that is taken out move up by one
            Ok(if skip.is_some_and(|s| s < p) {
                p - 1
            } else {
                p
            })
        };
        Ok(match place {
            Place::First => 0,
            Place::Last => len_after_removal,
            Place::At(n) => (*n).min(len_after_removal),
            Place::Before(n) => find(n)?,
            Place::After(n) => find(n)? + 1,
        })
    }

    /// Adds an archive. Fails when it is already listed (use [`DataIni::move_to`]).
    pub fn add(&mut self, name: &str, place: &Place) -> Result<usize> {
        let bytes = check_name(name)?;
        if self
            .entries
            .iter()
            .any(|e| e.name.eq_ignore_ascii_case(&bytes))
        {
            return Err(Error::InvalidInput(format!(
                "{} is already in DATA.INI",
                name.trim()
            )));
        }
        let at = self.resolve(place, self.entries.len(), None)?;
        self.entries.insert(
            at,
            Entry {
                index: 0,
                name: bytes,
            },
        );
        self.renumber(None);
        Ok(at)
    }

    pub fn remove(&mut self, name: &str) -> Result<usize> {
        let at = self
            .position(name)
            .ok_or_else(|| Error::InvalidInput(format!("{} is not in DATA.INI", name.trim())))?;
        self.entries.remove(at);
        self.renumber(None);
        Ok(at)
    }

    pub fn move_to(&mut self, name: &str, place: &Place) -> Result<usize> {
        let from = self
            .position(name)
            .ok_or_else(|| Error::InvalidInput(format!("{} is not in DATA.INI", name.trim())))?;
        let at = self.resolve(place, self.entries.len() - 1, Some(from))?;
        let e = self.entries.remove(from);
        self.entries.insert(at, e);
        self.renumber(None);
        Ok(at)
    }

    /// Puts another archive in the slot of one that is listed.
    pub fn replace(&mut self, old: &str, new: &str) -> Result<usize> {
        let bytes = check_name(new)?;
        let at = self
            .position(old)
            .ok_or_else(|| Error::InvalidInput(format!("{} is not in DATA.INI", old.trim())))?;
        if self
            .entries
            .iter()
            .enumerate()
            .any(|(i, e)| i != at && e.name.eq_ignore_ascii_case(&bytes))
        {
            return Err(Error::InvalidInput(format!(
                "{} is already in DATA.INI",
                new.trim()
            )));
        }
        self.entries[at].name = bytes;
        Ok(at)
    }

    /// Numbers the entries one after the other (gaps and duplicates gone), from `start` or from the index the file began at.
    pub fn renumber(&mut self, start: Option<u32>) {
        if let Some(s) = start {
            self.start = s;
        }
        for (i, e) in self.entries.iter_mut().enumerate() {
            e.index = self.start + i as u32;
        }
    }

    /// The file as it should be written.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut lines: Vec<Vec<u8>> = self.head.clone();
        if !lines.iter().any(|l| is_data_header(l)) {
            lines.push(b"[Data]".to_vec());
        }
        for e in &self.entries {
            let mut l = format!("{}=", e.index).into_bytes();
            l.extend_from_slice(&e.name);
            lines.push(l);
        }
        lines.extend(self.tail.iter().cloned());
        let mut out = lines.join(self.newline);
        if self.ended_with_newline || self.entries.is_empty() {
            out.extend_from_slice(self.newline);
        }
        out
    }

    /// A new, empty file (`[Data]` only), numbered from `start`.
    pub fn new_file(start: u32) -> Self {
        Self {
            head: vec![b"[Data]".to_vec()],
            entries: Vec::new(),
            tail: Vec::new(),
            newline: b"\r\n",
            start,
            ended_with_newline: true,
        }
    }
}

/// One change, so that the command line and the MCP server do the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Add {
        names: Vec<String>,
        place: Place,
    },
    Remove {
        names: Vec<String>,
    },
    Move {
        name: String,
        place: Place,
    },
    Replace {
        old: String,
        new: String,
    },
    /// Closes gaps and duplicate indexes; `start` sets the first index (default: the one the file has).
    Normalize {
        start: Option<u32>,
    },
}

impl DataIni {
    /// Applies a change; on an error the list is left as it was. Several names are added in the order given
    /// (`Place::First` with two names keeps their order at the top).
    pub fn apply(&mut self, op: &Op) -> Result<()> {
        let before = self.clone();
        let r = self.apply_inner(op);
        if r.is_err() {
            *self = before;
        }
        r
    }

    fn apply_inner(&mut self, op: &Op) -> Result<()> {
        match op {
            Op::Add { names, place } => {
                if names.is_empty() {
                    return Err(Error::InvalidInput("no archive name given".into()));
                }
                // for First/At/Before/After each next name follows the previous one
                let mut place = place.clone();
                for n in names {
                    let at = self.add(n, &place)?;
                    place = match place {
                        Place::Last => Place::Last,
                        _ => Place::At(at + 1),
                    };
                }
            }
            Op::Remove { names } => {
                if names.is_empty() {
                    return Err(Error::InvalidInput("no archive name given".into()));
                }
                for n in names {
                    self.remove(n)?;
                }
            }
            Op::Move { name, place } => {
                self.move_to(name, place)?;
            }
            Op::Replace { old, new } => {
                self.replace(old, new)?;
            }
            Op::Normalize { start } => self.renumber(*start),
        }
        Ok(())
    }
}

/// The `DATA.INI` a path means: the file itself, or the one in a client folder (name matched ignoring case).
pub fn locate(path: &std::path::Path) -> Result<std::path::PathBuf> {
    if path.is_dir() {
        let found = std::fs::read_dir(path)?.filter_map(|e| e.ok()).find(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case("DATA.INI"))
        });
        return found
            .map(|e| e.path())
            .ok_or_else(|| Error::InvalidInput(format!("no DATA.INI in {}", path.display())));
    }
    Ok(path.to_path_buf())
}

/// Replaces `path` through a temporary file next to it. The old file is not modified (a hard link to it keeps the old
/// content), and a failed write leaves it untouched.
pub fn write_atomic(path: &std::path::Path, data: &[u8]) -> Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!("{name}.rgrf-tmp"));
    std::fs::write(&tmp, data)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(d: &DataIni) -> Vec<String> {
        d.items()
            .into_iter()
            .map(|i| format!("{}={}", i.index, i.name))
            .collect()
    }

    #[test]
    fn unchanged_file_is_written_back_exactly() {
        for text in [
            "[Data]\r\n0=odin.grf\r\n1=data.grf\r\n",
            "[Data]\n1=cdata.grf\n2=data.grf\n",
            "[Data]\r\n1=cdata.grf\r\n",
            "; my comment\r\n[Data]\r\n0=a.grf\r\n\r\n[Other]\r\nx=y\r\n",
        ] {
            assert_eq!(
                DataIni::parse(text.as_bytes()).to_bytes(),
                text.as_bytes(),
                "{text:?}"
            );
        }
    }

    #[test]
    fn add_remove_move_keep_numbering_style() {
        let mut d = DataIni::parse(b"[Data]\r\n1=cdata.grf\r\n2=core.grf\r\n3=data.grf\r\n");
        assert_eq!(d.start(), 1);
        d.add("odin.grf", &Place::First).unwrap();
        assert_eq!(
            names(&d),
            ["1=odin.grf", "2=cdata.grf", "3=core.grf", "4=data.grf"]
        );
        d.add("skin.grf", &Place::After("cdata.grf".into()))
            .unwrap();
        assert_eq!(
            names(&d),
            [
                "1=odin.grf",
                "2=cdata.grf",
                "3=skin.grf",
                "4=core.grf",
                "5=data.grf"
            ]
        );
        d.move_to("data.grf", &Place::First).unwrap();
        assert_eq!(d.items()[0].name, "data.grf");
        d.move_to("data.grf", &Place::Last).unwrap();
        assert_eq!(d.items().last().unwrap().name, "data.grf");
        d.remove("SKIN.grf").unwrap();
        assert_eq!(
            names(&d),
            ["1=odin.grf", "2=cdata.grf", "3=core.grf", "4=data.grf"]
        );
        assert_eq!(
            d.to_bytes(),
            b"[Data]\r\n1=odin.grf\r\n2=cdata.grf\r\n3=core.grf\r\n4=data.grf\r\n"
        );
    }

    #[test]
    fn move_before_and_after_with_the_entry_taken_out_first() {
        let mut d = DataIni::parse(b"[Data]\n0=a.grf\n1=b.grf\n2=c.grf\n3=d.grf\n");
        d.move_to("a.grf", &Place::After("c.grf".into())).unwrap();
        assert_eq!(names(&d), ["0=b.grf", "1=c.grf", "2=a.grf", "3=d.grf"]);
        d.move_to("d.grf", &Place::Before("b.grf".into())).unwrap();
        assert_eq!(names(&d), ["0=d.grf", "1=b.grf", "2=c.grf", "3=a.grf"]);
        assert!(d.move_to("b.grf", &Place::Before("b.grf".into())).is_err());
        d.move_to("c.grf", &Place::At(0)).unwrap();
        assert_eq!(d.items()[0].name, "c.grf");
        d.move_to("c.grf", &Place::At(99)).unwrap();
        assert_eq!(d.items().last().unwrap().name, "c.grf");
    }

    #[test]
    fn errors_are_clear() {
        let mut d = DataIni::parse(b"[Data]\n0=a.grf\n");
        assert!(
            d.add("A.GRF", &Place::Last)
                .unwrap_err()
                .to_string()
                .contains("already")
        );
        assert!(
            d.remove("zzz.grf")
                .unwrap_err()
                .to_string()
                .contains("not in DATA.INI")
        );
        assert!(
            d.add("sub/x.grf", &Place::Last)
                .unwrap_err()
                .to_string()
                .contains("without a path")
        );
        assert!(d.add("", &Place::Last).is_err());
        assert!(d.add("x.grf", &Place::Before("nope.grf".into())).is_err());
        assert_eq!(names(&d), ["0=a.grf"], "failed edits change nothing");
    }

    #[test]
    fn gaps_and_duplicates_are_closed_by_normalize() {
        let mut d = DataIni::parse(b"[Data]\n5=b.grf\n2=a.grf\n2=a2.grf\n9=c.grf\n");
        assert_eq!(d.duplicate_indexes(), [2]);
        assert_eq!(d.items()[0].name, "a.grf");
        d.renumber(Some(0));
        assert_eq!(names(&d), ["0=a.grf", "1=a2.grf", "2=b.grf", "3=c.grf"]);
        assert!(d.duplicate_indexes().is_empty());
    }

    #[test]
    fn replace_keeps_the_slot() {
        let mut d = DataIni::parse(b"[Data]\n0=a.grf\n1=b.grf\n2=c.grf\n");
        d.replace("b.grf", "b2.grf").unwrap();
        assert_eq!(names(&d), ["0=a.grf", "1=b2.grf", "2=c.grf"]);
        assert!(d.replace("b2.grf", "c.grf").is_err());
    }

    #[test]
    fn other_lines_and_sections_survive_an_edit() {
        let mut d = DataIni::parse(
            b"; header\r\n[Data]\r\n; priority\r\n0=a.grf\r\n1=b.grf\r\n[Misc]\r\nk=v\r\n",
        );
        d.add("odin.grf", &Place::First).unwrap();
        assert_eq!(
            d.to_bytes(),
            b"; header\r\n[Data]\r\n; priority\r\n0=odin.grf\r\n1=a.grf\r\n2=b.grf\r\n[Misc]\r\nk=v\r\n".to_vec()
        );
    }

    #[test]
    fn korean_names_stay_the_bytes_they_were() {
        // 한글.grf in CP949
        let raw = b"[Data]\r\n0=\xc7\xd1\xb1\xdb.grf\r\n1=data.grf\r\n";
        let mut d = DataIni::parse(raw);
        assert_eq!(d.items()[0].name, "한글.grf");
        assert!(d.contains("한글.grf"));
        d.add("odin.grf", &Place::First).unwrap();
        assert_eq!(
            d.to_bytes(),
            b"[Data]\r\n0=odin.grf\r\n1=\xc7\xd1\xb1\xdb.grf\r\n2=data.grf\r\n".to_vec()
        );
        d.add("스킨.grf", &Place::Last).unwrap();
        assert!(
            d.to_bytes()
                .windows(4)
                .any(|w| w == [0xbd, 0xba, 0xc5, 0xb2]),
            "written in CP949"
        );
    }

    #[test]
    fn several_names_keep_their_order() {
        let mut d = DataIni::parse(b"[Data]\n0=data.grf\n");
        d.apply(&Op::Add {
            names: vec!["a.grf".into(), "b.grf".into()],
            place: Place::First,
        })
        .unwrap();
        assert_eq!(names(&d), ["0=a.grf", "1=b.grf", "2=data.grf"]);
        d.apply(&Op::Add {
            names: vec!["c.grf".into(), "d.grf".into()],
            place: Place::After("a.grf".into()),
        })
        .unwrap();
        assert_eq!(
            d.items()
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>(),
            ["a.grf", "c.grf", "d.grf", "b.grf", "data.grf"]
        );
        let before = d.clone();
        assert!(
            d.apply(&Op::Add {
                names: vec!["e.grf".into(), "a.grf".into()],
                place: Place::Last
            })
            .is_err()
        );
        assert_eq!(d, before, "a failed change leaves the list as it was");
    }

    #[test]
    fn empty_and_headerless_input() {
        assert_eq!(DataIni::parse(b"").to_bytes(), b"[Data]\r\n");
        let mut d = DataIni::new_file(0);
        d.add("data.grf", &Place::Last).unwrap();
        assert_eq!(d.to_bytes(), b"[Data]\r\n0=data.grf\r\n");
        let mut e = DataIni::parse(b"junk\n");
        e.add("a.grf", &Place::Last).unwrap();
        assert_eq!(e.to_bytes(), b"junk\n[Data]\n0=a.grf\n");
    }

    #[test]
    fn section_name_is_case_insensitive_and_keys_must_be_numbers() {
        let d = DataIni::parse(b"[DATA]\n0=a.grf\nfoo=bar\n 1 = b.grf \n");
        assert_eq!(names(&d), ["0=a.grf", "1=b.grf"]);
    }
}
