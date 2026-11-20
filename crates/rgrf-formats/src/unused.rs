//! Which files of a client nothing names (`rgrf client unused`).
//!
//! The client finds most files by names written in maps, animations, effects, Lua tables, XML
//! and the program itself, often without the folder and sometimes without the extension, or
//! put together from pieces. So names are collected generously: every piece of text that could be
//! a file name goes into a set (lowercase, both with and without the extension), and a file counts
//! as named when its name is in the set. A file reported as unnamed is therefore *probably* not
//! used, never certainly: names built at run time cannot be seen.

use std::collections::HashSet;

/// Separators between names inside text or compiled Lua: control bytes (this includes the NULs of
/// string lengths) and quotes, brackets and other punctuation that is not part of a file name.
fn is_separator(b: u8) -> bool {
    b < 0x20
        || matches!(
            b,
            b'"' | b'\''
                | b','
                | b';'
                | b'('
                | b')'
                | b'<'
                | b'>'
                | b'='
                | b'|'
                | b'['
                | b']'
                | b'{'
                | b'}'
                | 0x7f
        )
}

/// The set of names found so far (lowercase bytes).
#[derive(Default)]
pub struct Names {
    set: HashSet<Vec<u8>>,
}

impl Names {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `name` as a file name (the last path piece, with and without its extension).
    pub fn add_name(&mut self, name: &[u8]) {
        let base = name
            .rsplit(|&b| b == b'\\' || b == b'/')
            .next()
            .unwrap_or(name);
        if base.is_empty() || base.len() > 120 {
            return;
        }
        let lower = base.to_ascii_lowercase();
        if let Some(dot) = lower.iter().rposition(|&b| b == b'.').filter(|&d| d > 0) {
            self.set.insert(lower[..dot].to_vec());
        }
        self.set.insert(lower);
    }

    /// Adds every name-like piece of arbitrary bytes (text, compiled Lua, a program).
    pub fn scan(&mut self, bytes: &[u8]) {
        for token in bytes.split(|&b| is_separator(b)) {
            if token.len() < 2 || token.len() > 240 {
                continue;
            }
            self.add_name(token);
            // "a.bmp b.bmp" and "icon\\x.bmp 3": pieces between spaces are names too
            if token.contains(&b' ') {
                for piece in token.split(|&b| b == b' ').filter(|p| p.len() >= 2) {
                    self.add_name(piece);
                }
            }
        }
    }

    /// Whether a file (its path or its name) is named.
    pub fn names(&self, path: &[u8]) -> bool {
        let base = path
            .rsplit(|&b| b == b'\\' || b == b'/')
            .next()
            .unwrap_or(path)
            .to_ascii_lowercase();
        let stem = base
            .iter()
            .rposition(|&b| b == b'.')
            .filter(|&d| d > 0)
            .map(|d| &base[..d]);
        self.set.contains(&base) || stem.is_some_and(|s| self.set.contains(s))
    }

    pub fn merge(&mut self, other: &Names) {
        self.set.extend(other.set.iter().cloned());
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }
}

// ---- the analysis of a whole client ----

use std::collections::BTreeMap;

use rayon::prelude::*;

use crate::act::Act;
use crate::map_files::{MapSource, effect_textures, resolve};

/// A file that wins in the client, with its stored size.
pub struct FileInfo {
    pub path: Vec<u8>,
    pub zsize: u64,
}

/// What the analysis needs to know about a client: the files that win (first archive wins) and
/// their content.
pub trait Client: MapSource + Sync {
    fn files(&self) -> &[FileInfo];
    /// The content of `files()[i]`.
    fn read_file(&self, i: usize) -> Option<Vec<u8>>;
}

/// Folders (below `data\`) whose files the analysis looks at.
const AREAS: [&[u8]; 4] = [
    b"data\\texture\\",
    b"data\\sprite\\",
    b"data\\wav\\",
    b"data\\model\\",
];

/// CP949 bytes of `유저인터페이스` (the interface folder).
const INTERFACE: &[u8] = b"\xc0\xaf\xc0\xfa\xc0\xce\xc5\xcd\xc6\xe4\xc0\xcc\xbd\xba";

/// Folders of maps: their files are named in maps and models, so "named by nothing" is reliable.
/// Everything else (effects, interface, sprites, sounds, 3D monster bones) is put together from
/// pieces by the client or its tables.
pub fn names_are_composed(folder: &[u8]) -> bool {
    let low = folder.to_ascii_lowercase();
    let starts = |p: &[u8]| low.starts_with(p);
    let map_folder =
        starts(b"data\\texture\\") || (starts(b"data\\model") && !starts(b"data\\model\\3dmob"));
    !map_folder
        || starts(b"data\\texture\\effect")
        || starts(b"data\\texture\\userinterface")
        || folder.starts_with(&[b"data\\texture\\".as_slice(), INTERFACE].concat())
}

pub struct Folder {
    /// The path of the folder (at most three levels), CP949 bytes.
    pub name: Vec<u8>,
    pub files: u64,
    pub unnamed: u64,
    pub unnamed_bytes: u64,
    pub examples: Vec<Vec<u8>>,
}

pub struct Analysis {
    pub names_found: usize,
    pub files_looked_at: u64,
    pub unnamed: u64,
    pub unnamed_bytes: u64,
    /// Unnamed files in folders of maps (reliable).
    pub unnamed_reliable: u64,
    pub unnamed_reliable_bytes: u64,
    /// Folders with unnamed files, the biggest first.
    pub folders: Vec<Folder>,
    /// Every unnamed file, the biggest first.
    pub listed: Vec<(Vec<u8>, u64)>,
}

/// Finds the files in textures, sprites, sounds and models that nothing names. `extra` are more
/// texts that name files (programs and settings next to the archives).
pub fn analyze<C: Client>(client: &C, extra: &[Vec<u8>]) -> Analysis {
    let files = client.files();
    let ext = |p: &[u8]| {
        p.rsplit(|&b| b == b'.')
            .next()
            .unwrap_or(&[])
            .to_ascii_lowercase()
    };
    let parts: Vec<Names> = (0..files.len())
        .into_par_iter()
        .filter(|&i| {
            matches!(
                ext(&files[i].path).as_slice(),
                b"txt"
                    | b"xml"
                    | b"lua"
                    | b"lub"
                    | b"ini"
                    | b"csv"
                    | b"json"
                    | b"act"
                    | b"str"
                    | b"rsw"
            )
        })
        .map(|i| {
            let mut n = Names::new();
            let path = &files[i].path;
            let Some(data) = client.read_file(i) else {
                return n;
            };
            match ext(path).as_slice() {
                b"act" => {
                    if let Ok(a) = Act::parse(&data) {
                        for s in &a.sounds {
                            n.add_name(s.split(|&b| b == 0).next().unwrap_or(&[]));
                        }
                    }
                }
                b"str" => {
                    if let Ok(t) = effect_textures(&data) {
                        t.iter().for_each(|x| n.add_name(x));
                    }
                }
                b"rsw" => {
                    if let Ok(list) = resolve(&String::from_utf8_lossy(path), client) {
                        list.iter().for_each(|f| n.add_name(&f.path));
                    }
                }
                _ => n.scan(&data),
            }
            n
        })
        .collect();
    let mut named = Names::new();
    parts.iter().for_each(|p| named.merge(p));
    extra.iter().for_each(|t| named.scan(t));

    let mut folders: BTreeMap<Vec<u8>, Folder> = BTreeMap::new();
    let mut listed = Vec::new();
    for f in files {
        let low = f.path.to_ascii_lowercase();
        if !AREAS.iter().any(|a| low.starts_with(a)) {
            continue;
        }
        let parts: Vec<&[u8]> = f.path.split(|&b| b == b'\\').collect();
        let name = parts[..parts.len().saturating_sub(1).min(3)].join(&b'\\');
        let fo = folders.entry(name.clone()).or_insert(Folder {
            name,
            files: 0,
            unnamed: 0,
            unnamed_bytes: 0,
            examples: Vec::new(),
        });
        fo.files += 1;
        if !named.names(&f.path) {
            fo.unnamed += 1;
            fo.unnamed_bytes += f.zsize;
            if fo.examples.len() < 3 {
                fo.examples.push(f.path.clone());
            }
            listed.push((f.path.clone(), f.zsize));
        }
    }
    let mut out = Analysis {
        names_found: named.len(),
        files_looked_at: 0,
        unnamed: 0,
        unnamed_bytes: 0,
        unnamed_reliable: 0,
        unnamed_reliable_bytes: 0,
        folders: Vec::new(),
        listed,
    };
    for fo in folders.values() {
        out.files_looked_at += fo.files;
        out.unnamed += fo.unnamed;
        out.unnamed_bytes += fo.unnamed_bytes;
        if !names_are_composed(&fo.name) {
            out.unnamed_reliable += fo.unnamed;
            out.unnamed_reliable_bytes += fo.unnamed_bytes;
        }
    }
    out.folders = folders.into_values().filter(|f| f.unnamed > 0).collect();
    out.folders.sort_by(|a, b| {
        b.unnamed_bytes
            .cmp(&a.unnamed_bytes)
            .then(a.name.cmp(&b.name))
    });
    out.listed.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_names_in_text_lua_and_compiled_strings() {
        let mut n = Names::new();
        n.scan(b"icon = \"UI\\\\Basic\\\\Btn_OK.bmp\"; other='poring'\n<file>x y.tga</file>");
        // a compiled Lua string: 4-byte length, the text, a NUL
        n.scan(b"\x0b\x00\x00\x00sprite\\a.spr\x00");
        for p in [
            "data\\texture\\btn_ok.BMP",
            "data\\sprite\\poring.spr",
            "y.tga",
            "a.spr",
        ] {
            assert!(n.names(p.as_bytes()), "{p}");
        }
        assert!(!n.names(b"data\\texture\\unrelated.bmp"));
    }

    #[test]
    fn korean_names_are_kept_as_bytes() {
        let mut n = Names::new();
        n.scan(b"\xb8\xf3\xbd\xba\xc5\xcd\\\xc6\xf7\xb8\xae.spr");
        assert!(n.names(b"data\\sprite\\\xb8\xf3\xbd\xba\xc5\xcd\\\xc6\xf7\xb8\xae.SPR"));
    }
}
