//! What the client's Lua tables name, checked against the files that exist.
//!
//! Each table has its own rule: a name like `poring` in `jobname.lub` is a sprite in
//! `data\sprite\<monsters>\`, an item's `identifiedResourceName` is an icon below
//! `data\texture\<interface>\item\`, a hat effect names an `.str` below `data\texture\effect\`.
//! The folders were found by comparing every name of the real tables with the files of the kRO
//! client (the share found is in `docs/09-formats.md`), not by reading the client's code, so a
//! name that is "missing" here is a name the table has and no archive holds. Where two folders
//! are known (the interface folder is Korean in some clients and English in others), a file in
//! either counts.
//!
//! Not checked: names the client builds from several parts at run time (a weapon sprite is job,
//! gender and weapon), scripts that are code and not data, and names in UTF-8 scripts (they cannot
//! be compared with the CP949 names of the archives).

use std::collections::{HashMap, HashSet};

use rayon::prelude::*;

use crate::lua_table::{self, Assign, Key, Table, Tables, Text, Value};
use crate::map_files::MapSource;
use crate::refs::Problem;

/// A source that can be read from several threads.
pub type Src<'a> = &'a (dyn MapSource + Sync);

/// One missing file named by a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableProblem {
    /// The rule that found it (`jobname`, `items`, ...).
    pub table: &'static str,
    /// The script, as a path (keys of the archive, lower case).
    pub file: Vec<u8>,
    pub problem: Problem,
}

#[derive(Debug, Default)]
pub struct TableReport {
    /// Scripts that were read.
    pub files: usize,
    /// Names that were looked up (each once per script).
    pub names: usize,
    /// Names left out because their script is UTF-8.
    pub skipped_utf8: usize,
    /// Compiled scripts that could not be read.
    pub unreadable: Vec<Vec<u8>>,
    pub problems: Vec<TableProblem>,
}

const UI_KO: &[u8] = b"\xc0\xaf\xc0\xfa\xc0\xce\xc5\xcd\xc6\xe4\xc0\xcc\xbd\xba";
const UI_EN: &[u8] = b"userinterface";
const MONSTER: &[u8] = b"\xb8\xf3\xbd\xba\xc5\xcd";
const HEADGEAR: &[u8] = b"\xbe\xc7\xbc\xbc\xbb\xe7\xb8\xae";
const ITEM_SPRITE: &[u8] = b"\xbe\xc6\xc0\xcc\xc5\xdb";
const ROBE: &[u8] = b"\xb7\xce\xba\xea";
const HUMAN_BODY: &[u8] = b"\xc0\xce\xb0\xa3\xc1\xb7\\\xb8\xf6\xc5\xeb";
const MALE: &[u8] = b"\xb3\xb2";
const FEMALE: &[u8] = b"\xbf\xa9";

/// Where the client reads its scripts, first the usual folder, then the old one.
const ROOTS: [&[u8]; 2] = [b"data\\luafiles514\\lua files\\", b"data\\lua files\\"];

type Alts = Vec<Vec<Vec<u8>>>;

/// Something a table needs: every path of at least one alternative must exist.
struct Need {
    kind: &'static str,
    alts: Alts,
    note: String,
}

struct Found {
    file: Vec<u8>,
    need: Need,
}

#[derive(Default)]
struct RuleOut {
    found: Vec<Found>,
    files: usize,
    names: usize,
    skipped: usize,
    unreadable: Vec<Vec<u8>>,
}

impl RuleOut {
    fn merge(&mut self, o: RuleOut) {
        self.found.extend(o.found);
        self.files += o.files;
        self.names += o.names;
        self.skipped += o.skipped;
        self.unreadable.extend(o.unreadable);
    }
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn ends_with_ext(name: &[u8], exts: &[&str]) -> bool {
    let Some(dot) = name.iter().rposition(|&b| b == b'.') else {
        return false;
    };
    exts.iter()
        .any(|e| name[dot + 1..].eq_ignore_ascii_case(e.as_bytes()))
}

fn one(path: Vec<u8>) -> Alts {
    vec![vec![path]]
}

/// `data\texture\<interface folder>\<rel>` in the Korean and the English spelling.
fn ui(rel: &[u8]) -> Alts {
    [UI_KO, UI_EN]
        .iter()
        .map(|d| vec![cat(&[b"data\\texture\\", d, b"\\", rel])])
        .collect()
}

/// Lower case like `GrfPath::key`: double-byte characters are kept.
fn fold(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let trail = |x: u8| matches!(x, 0x41..=0x5A | 0x61..=0x7A | 0x81..=0xFE);
        if b[i] >= 0x81 && i + 1 < b.len() && trail(b[i + 1]) {
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

struct Script {
    path: Vec<u8>,
    tables: Tables,
    assigns: Vec<Assign>,
    text: Text,
}

fn load_path(src: Src, path: &[u8], out: &mut RuleOut) -> Option<Script> {
    let data = src.read(path)?;
    let Some((text, kind)) = lua_table::source(&data) else {
        out.unreadable.push(path.to_vec());
        return None;
    };
    out.files += 1;
    let assigns = lua_table::parse(&text);
    let mut tables = Tables::default();
    tables.add(assigns.clone());
    Some(Script {
        path: path.to_vec(),
        tables,
        assigns,
        text: kind,
    })
}

/// A script of the client by its path below `lua files` without the extension.
fn script(src: Src, rel: &str, out: &mut RuleOut) -> Option<Script> {
    for root in ROOTS {
        for ext in ["lub", "lua"] {
            let path = cat(&[root, rel.as_bytes(), b".", ext.as_bytes()]);
            if src.exists(&path) {
                return load_path(src, &path, out);
            }
        }
    }
    None
}

impl Script {
    /// The name when it can be compared with an archive path.
    fn name<'a>(&self, out: &mut RuleOut, s: &'a [u8]) -> Option<&'a [u8]> {
        if s.is_empty() || s.contains(&0) {
            return None;
        }
        if self.text == Text::Utf8 && !s.is_ascii() {
            out.skipped += 1;
            return None;
        }
        out.names += 1;
        Some(s)
    }

    fn table(&self, name: &str) -> Option<&Table> {
        self.tables.map.get(name)
    }
}

/// Every string value of a table that looks like a file with one of `exts`, with its field name.
fn files_in<'a>(t: &'a Table, exts: &[&str], mut f: impl FnMut(Option<&'a str>, &'a [u8])) {
    lua_table::strings(t, |n, s| {
        if ends_with_ext(s, exts) {
            f(n, s);
        }
    });
}

/// A rule for tables whose values are file names below one folder (or several).
fn simple(
    src: Src,
    out: &mut RuleOut,
    rel: &str,
    table: Option<&str>,
    exts: &[&str],
    kind: &'static str,
    need: impl Fn(&[u8]) -> Alts,
) {
    let Some(sc) = script(src, rel, out) else {
        return;
    };
    let tables: Vec<&Table> = match table {
        Some(n) => sc.table(n).into_iter().collect(),
        None => sc.tables.map.values().collect(),
    };
    let mut found = Vec::new();
    for t in tables {
        files_in(t, exts, |_, s| {
            if let Some(n) = sc.name(out, s) {
                found.push(Found {
                    file: sc.path.clone(),
                    need: Need {
                        kind,
                        alts: need(n),
                        note: String::new(),
                    },
                });
            }
        });
    }
    out.found.extend(found);
}

fn push(out: &mut RuleOut, sc: &Script, kind: &'static str, alts: Alts, note: String) {
    out.found.push(Found {
        file: sc.path.clone(),
        need: Need { kind, alts, note },
    });
}

fn sprite_pair(dir: &[u8], name: &[u8]) -> Vec<Vec<u8>> {
    vec![cat(&[dir, name, b".spr"]), cat(&[dir, name, b".act"])]
}

fn jobname(src: Src, out: &mut RuleOut) {
    let Some(sc) = script(src, "datainfo\\jobname", out) else {
        return;
    };
    let ids = script(src, "datainfo\\npcidentity", out);
    let Some(table) = sc.table("JobNameTable") else {
        return;
    };
    let npc = b"data\\sprite\\npc\\".as_slice();
    let monster = cat(&[b"data\\sprite\\", MONSTER, b"\\"]);
    let homunculus = b"data\\sprite\\homun\\".as_slice();
    let body = |g: &[u8]| cat(&[b"data\\sprite\\", HUMAN_BODY, b"\\", g, b"\\"]);
    for (k, v) in &table.fields {
        let Some(raw) = v.as_str() else { continue };
        let id = match k {
            Key::Path(p) => ids
                .as_ref()
                .and_then(|i| i.tables.number(p))
                .or_else(|| sc.tables.number(p)),
            Key::Num(n) => Some(*n),
            _ => None,
        };
        let Some(id) = id.map(|n| n as i64) else {
            continue;
        };
        let Some(name) = sc.name(out, raw) else {
            continue;
        };
        let note = format!("id {id}");
        if name.len() > 4 && ends_with_ext(name, &["gr2"]) {
            push(
                out,
                &sc,
                "missing_npc_model",
                one(cat(&[b"data\\model\\3dmob\\", name])),
                note,
            );
            continue;
        }
        let (kind, alts) = match id {
            1000..4000 | 20000.. => (
                "missing_monster_sprite",
                vec![sprite_pair(&monster, name), sprite_pair(npc, name)],
            ),
            0..1000 | 10000..20000 => (
                "missing_npc_sprite",
                vec![sprite_pair(npc, name), sprite_pair(&monster, name)],
            ),
            // homunculi and mercenaries (the mercenaries are human sprites)
            6000..7000 => (
                "missing_helper_sprite",
                vec![
                    sprite_pair(homunculus, name),
                    sprite_pair(&body(MALE), name),
                    sprite_pair(&body(FEMALE), name),
                ],
            ),
            _ => continue,
        };
        push(out, &sc, kind, alts, note);
    }
}

fn headgear(src: Src, out: &mut RuleOut) {
    let Some(sc) = script(src, "datainfo\\accname", out) else {
        return;
    };
    let Some(table) = sc.table("AccNameTable") else {
        return;
    };
    for (k, v) in &table.fields {
        let Some(raw) = v.as_str() else { continue };
        let Some(name) = sc.name(out, raw) else {
            continue;
        };
        // a headgear exists for the genders it is made for
        let alts = [
            (MALE, MALE, ""),
            (FEMALE, FEMALE, ""),
            (MALE, MALE, "_doram"),
            (FEMALE, FEMALE, "_doram"),
        ]
        .iter()
        .map(|(dir, prefix, suffix)| {
            let d = cat(&[
                b"data\\sprite\\",
                HEADGEAR,
                b"\\",
                dir,
                suffix.as_bytes(),
                b"\\",
                prefix,
            ]);
            sprite_pair(&d, name)
        })
        .collect();
        let note = match k {
            Key::Path(p) => p.clone(),
            _ => String::new(),
        };
        push(out, &sc, "missing_headgear_sprite", alts, note);
    }
}

fn pets(src: Src, out: &mut RuleOut) {
    let Some(sc) = script(src, "datainfo\\petinfo", out) else {
        return;
    };
    let monster = cat(&[b"data\\sprite\\", MONSTER, b"\\"]);
    let npc = b"data\\sprite\\npc\\".as_slice();
    let values = |t: &Table| -> Vec<Vec<u8>> {
        t.fields
            .iter()
            .filter_map(|(_, v)| v.as_str().map(<[u8]>::to_vec))
            .collect()
    };
    if let Some(t) = sc.table("PetNameTable") {
        for raw in values(t) {
            if let Some(n) = sc.name(out, &raw) {
                let alts = vec![sprite_pair(&monster, n), sprite_pair(npc, n)];
                push(out, &sc, "missing_pet_sprite", alts, String::new());
            }
        }
    }
    if let Some(t) = sc.table("PetIllustNameTable") {
        for raw in values(t) {
            if let Some(n) = sc.name(out, &raw) {
                push(
                    out,
                    &sc,
                    "missing_pet_illustration",
                    ui(&cat(&[b"illust\\", n])),
                    String::new(),
                );
            }
        }
    }
    if let Some(t) = sc.table("PetAccActNameTable") {
        for raw in values(t) {
            let Some(n) = sc.name(out, &raw) else {
                continue;
            };
            let stem = n.strip_suffix(b".act").or_else(|| n.strip_suffix(b".ACT"));
            let alts = match stem {
                Some(s) => vec![sprite_pair(&monster, s)],
                None => one(cat(&[&monster, n])),
            };
            push(out, &sc, "missing_pet_accessory", alts, String::new());
        }
    }
}

fn robes(src: Src, out: &mut RuleOut) {
    let Some(sc) = script(src, "datainfo\\spriterobename", out) else {
        return;
    };
    let Some(table) = sc.table("RobeNameTable") else {
        return;
    };
    let base = cat(&[b"data\\sprite\\", ROBE, b"\\"]);
    let listed = src.list(&base);
    if listed.is_empty() {
        return;
    }
    let folders: HashSet<Vec<u8>> = listed
        .iter()
        .filter_map(|k| {
            let rest = &k[base.len()..];
            let end = rest.iter().position(|&b| b == b'\\')?;
            Some(rest[..end].to_vec())
        })
        .collect();
    for (_, v) in &table.fields {
        let Some(raw) = v.as_str() else { continue };
        let Some(name) = sc.name(out, raw) else {
            continue;
        };
        if !folders.contains(&fold(name)) {
            push(
                out,
                &sc,
                "missing_robe_folder",
                one(cat(&[&base, name, b"\\"])),
                String::new(),
            );
        }
    }
}

fn skill_icons(src: Src, out: &mut RuleOut) {
    let Some(sc) = script(src, "skillinfoz\\skillinfolist", out) else {
        return;
    };
    let Some(table) = sc.table("SKILL_INFO_LIST") else {
        return;
    };
    for (k, v) in &table.fields {
        let Value::Table(entry) = v else { continue };
        let Some(Value::Str(raw)) = entry.positional().next() else {
            continue;
        };
        let Some(name) = sc.name(out, raw) else {
            continue;
        };
        let note = match k {
            Key::Path(p) => p.clone(),
            _ => String::new(),
        };
        push(
            out,
            &sc,
            "missing_skill_icon",
            ui(&cat(&[b"item\\", name, b".bmp"])),
            note,
        );
    }
}

fn map_names(src: Src, out: &mut RuleOut) {
    for rel in [
        "mapskydata\\mapskydata",
        "worldviewdata\\worldviewdata_table",
        "agit\\agitconfig",
        "datainfo\\kaframovemapservicelist",
    ] {
        simple(src, out, rel, None, &["rsw", "gat"], "missing_map", |n| {
            one(cat(&[b"data\\", n]))
        });
    }
}

fn images(src: Src, out: &mut RuleOut) {
    for rel in ["signboardlist", "worldviewdata\\worldviewdata_list"] {
        simple(
            src,
            out,
            rel,
            None,
            &["bmp", "tga", "jpg", "png"],
            "missing_ui_image",
            |n| {
                let mut a = one(cat(&[b"data\\texture\\", n]));
                a.extend(ui(n));
                a
            },
        );
    }
}

fn effects(src: Src, out: &mut RuleOut) {
    simple(
        src,
        out,
        "stateicon\\stateiconimginfo",
        Some("StateIconImgList"),
        &["tga", "bmp", "png", "jpg"],
        "missing_state_icon",
        |n| one(cat(&[b"data\\texture\\effect\\", n])),
    );
    if let Some(sc) = script(src, "hateffectinfo\\hateffectinfo", out)
        && let Some(t) = sc.table("hatEffectTable")
    {
        lua_table::strings(t, |n, s| {
            if n == Some("resourceFileName")
                && let Some(name) = sc.name(out, s)
            {
                let alts = one(cat(&[b"data\\texture\\effect\\", name]));
                push(out, &sc, "missing_hat_effect", alts, String::new());
            }
        });
    }
    simple(
        src,
        out,
        "hateffectinfo\\footprinteffectinfo",
        None,
        &["png", "str", "bmp", "tga"],
        "missing_hat_effect",
        |n| one(cat(&[b"data\\texture\\effect\\", n])),
    );
    simple(
        src,
        out,
        "cashemotion\\cashemotionlist",
        None,
        &["act", "spr"],
        "missing_emotion_sprite",
        |n| one(cat(&[b"data\\sprite\\", n])),
    );
}

fn sounds(src: Src, out: &mut RuleOut) {
    if let Some(sc) = script(src, "skilleffectinfo\\skilleffectinfolist", out)
        && let Some(t) = sc.table("SKILL_EFFECT_INFO_LIST")
    {
        lua_table::strings(t, |n, s| {
            if n.is_some_and(|n| n.to_ascii_lowercase().ends_with("wavefilename"))
                && let Some(name) = sc.name(out, s)
            {
                let alts = one(cat(&[b"data\\wav\\", name]));
                push(out, &sc, "missing_skill_sound", alts, String::new());
            }
        });
    }
    simple(
        src,
        out,
        "datainfo\\weapontable",
        Some("WeaponHitWaveNameTable"),
        &["wav"],
        "missing_weapon_sound",
        |n| one(cat(&[b"data\\wav\\", n])),
    );
}

/// The textures of the particle scripts of the maps (`effecttool\<map>.lub`).
fn effect_tool(src: Src, out: &mut RuleOut) {
    let mut files: Vec<Vec<u8>> = Vec::new();
    let mut seen = HashSet::new();
    for root in ROOTS {
        let dir = cat(&[root, b"effecttool\\"]);
        let mut keys = src.list(&dir);
        keys.sort();
        for k in keys {
            let name = k[dir.len()..].to_vec();
            if ends_with_ext(&k, &["lub", "lua"]) && seen.insert(name) {
                files.push(k);
            }
        }
    }
    let parts: Vec<RuleOut> = files
        .par_iter()
        .map(|path| {
            let mut o = RuleOut::default();
            let Some(sc) = load_path(src, path, &mut o) else {
                return o;
            };
            let mut found = Vec::new();
            for a in &sc.assigns {
                let Value::Table(t) = &a.value else { continue };
                lua_table::strings(t, |n, s| {
                    let base: &[u8] = match n {
                        Some("texture") => b"data\\texture\\",
                        Some("str") => b"data\\texture\\effect\\",
                        _ => return,
                    };
                    if let Some(name) = sc.name(&mut o, s) {
                        found.push(Found {
                            file: sc.path.clone(),
                            need: Need {
                                kind: "missing_map_effect_texture",
                                alts: one(cat(&[base, name])),
                                note: String::new(),
                            },
                        });
                    }
                });
            }
            o.found = found;
            o
        })
        .collect();
    for p in parts {
        out.merge(p);
    }
}

/// Icons, collection pictures and the sprite of every item of the item tables (`itemInfo`).
fn items(src: Src, out: &mut RuleOut) {
    let mut paths: Vec<Vec<u8>> = Vec::new();
    for root in ROOTS {
        for ext in ["lub", "lua"] {
            let p = cat(&[root, b"datainfo\\iteminfo.", ext.as_bytes()]);
            if src.exists(&p) {
                paths.push(p);
            }
        }
    }
    for k in src.list(b"system\\iteminfo") {
        if ends_with_ext(&k, &["lub", "lua"]) {
            paths.push(k);
        }
    }
    let mut done = HashSet::new();
    for path in paths {
        if !done.insert(fold(&path)) {
            continue;
        }
        let Some(sc) = load_path(src, &path, out) else {
            continue;
        };
        let mut seen: HashSet<Vec<u8>> = HashSet::new();
        for a in &sc.assigns {
            let Value::Table(t) = &a.value else { continue };
            for (k, v) in &t.fields {
                let Value::Table(item) = v else { continue };
                for field in ["identifiedResourceName", "unidentifiedResourceName"] {
                    let Some(raw) = item.get_str(field) else {
                        continue;
                    };
                    let Some(name) = sc.name(out, raw) else {
                        continue;
                    };
                    if !seen.insert(name.to_vec()) {
                        continue;
                    }
                    let note = match k {
                        Key::Num(n) => format!("item {n}"),
                        _ => String::new(),
                    };
                    push(
                        out,
                        &sc,
                        "missing_item_icon",
                        ui(&cat(&[b"item\\", name, b".bmp"])),
                        note.clone(),
                    );
                    push(
                        out,
                        &sc,
                        "missing_item_collection",
                        ui(&cat(&[b"collection\\", name, b".bmp"])),
                        note.clone(),
                    );
                    push(
                        out,
                        &sc,
                        "missing_item_sprite",
                        vec![sprite_pair(
                            &cat(&[b"data\\sprite\\", ITEM_SPRITE, b"\\"]),
                            name,
                        )],
                        note,
                    );
                }
            }
        }
    }
}

type Rule = (&'static str, fn(Src, &mut RuleOut));

const RULES: &[Rule] = &[
    ("jobname", jobname),
    ("headgear", headgear),
    ("pets", pets),
    ("robes", robes),
    ("skill icons", skill_icons),
    ("maps", map_names),
    ("images", images),
    ("effects", effects),
    ("sounds", sounds),
    ("effecttool", effect_tool),
    ("items", items),
];

/// The names of the rules, in the order they are listed in a report.
pub fn rule_names() -> Vec<&'static str> {
    RULES.iter().map(|r| r.0).collect()
}

fn first_missing(src: Src, need: &Need) -> Option<Vec<u8>> {
    let mut best: Option<(usize, Vec<u8>)> = None;
    for alt in &need.alts {
        let missing: Vec<&Vec<u8>> = alt.iter().filter(|p| !src.exists(p)).collect();
        if missing.is_empty() {
            return None;
        }
        if best.as_ref().is_none_or(|b| missing.len() < b.0) {
            best = Some((missing.len(), missing[0].clone()));
        }
    }
    best.map(|b| b.1)
}

/// Checks the names of every table the rules know. Tables an archive does not have are skipped.
pub fn check_tables(src: Src) -> TableReport {
    let parts: Vec<(&'static str, RuleOut)> = RULES
        .par_iter()
        .map(|(name, rule)| {
            let mut o = RuleOut::default();
            rule(src, &mut o);
            (*name, o)
        })
        .collect();
    let mut report = TableReport::default();
    let mut seen: HashSet<(Vec<u8>, &'static str, Vec<u8>)> = HashSet::new();
    let mut by_rule: HashMap<&'static str, Vec<Found>> = HashMap::new();
    for (name, o) in parts {
        report.files += o.files;
        report.names += o.names;
        report.skipped_utf8 += o.skipped;
        report.unreadable.extend(o.unreadable);
        by_rule.entry(name).or_default().extend(o.found);
    }
    for (name, _) in RULES {
        let Some(found) = by_rule.remove(name) else {
            continue;
        };
        let missing: Vec<Option<Vec<u8>>> = found
            .par_iter()
            .map(|f| first_missing(src, &f.need))
            .collect();
        for (f, m) in found.into_iter().zip(missing) {
            let Some(path) = m else { continue };
            if seen.insert((f.file.clone(), f.need.kind, path.clone())) {
                report.problems.push(TableProblem {
                    table: name,
                    file: f.file,
                    problem: Problem {
                        kind: f.need.kind,
                        path,
                        note: f.need.note,
                    },
                });
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Files(HashMap<Vec<u8>, Vec<u8>>);
    impl MapSource for Files {
        fn read(&self, path: &[u8]) -> Option<Vec<u8>> {
            self.0.get(&fold(path)).cloned()
        }
        fn exists(&self, path: &[u8]) -> bool {
            self.0.contains_key(&fold(path))
        }
        fn list(&self, prefix: &[u8]) -> Vec<Vec<u8>> {
            self.0
                .keys()
                .filter(|k| k.starts_with(&fold(prefix)))
                .cloned()
                .collect()
        }
    }

    fn files(list: &[(&str, &str)]) -> Files {
        Files(
            list.iter()
                .map(|(p, d)| (fold(p.as_bytes()), d.as_bytes().to_vec()))
                .collect(),
        )
    }

    #[test]
    fn each_table_names_its_own_folder() {
        let root = "data\\luafiles514\\lua files\\";
        let src = files(&[
            (
                &format!("{root}datainfo\\npcidentity.lub"),
                "jobtbl = { JT_PORING = 1002, JT_KAFRA = 100, JT_GONE = 1003 }",
            ),
            (
                &format!("{root}datainfo\\jobname.lub"),
                "JobNameTable = { [jobtbl.JT_PORING] = \"poring\", [jobtbl.JT_KAFRA] = \"kafra\", [jobtbl.JT_GONE] = \"gone\" }",
            ),
            ("data\\sprite\\npc\\kafra.spr", ""),
            ("data\\sprite\\npc\\kafra.act", ""),
            ("data\\sprite\\poring.spr", ""),
            (
                &format!("{root}stateicon\\stateiconimginfo.lub"),
                "StateIconImgList = { { \"A.tga\", \"B.tga\" } }",
            ),
            ("data\\texture\\effect\\a.tga", ""),
            (
                "system\\iteminfo.lub",
                "tbl = { [501] = { identifiedResourceName = \"red\", unidentifiedResourceName = \"red\" } }",
            ),
            ("data\\texture\\userinterface\\item\\red.bmp", ""),
            ("data\\texture\\userinterface\\collection\\red.bmp", ""),
        ]);
        let r = check_tables(&src);
        let mut got: Vec<(&str, Vec<u8>)> = r
            .problems
            .iter()
            .map(|p| (p.problem.kind, p.problem.path.clone()))
            .collect();
        got.sort();
        // the monsters have no sprite in either folder, a state icon is missing and the item has
        // no sprite; everything else is found (the English interface folder counts)
        let mut want = vec![
            (
                "missing_item_sprite",
                cat(&[b"data\\sprite\\", ITEM_SPRITE, b"\\red.spr"]),
            ),
            (
                "missing_monster_sprite",
                cat(&[b"data\\sprite\\", MONSTER, b"\\gone.spr"]),
            ),
            (
                "missing_monster_sprite",
                cat(&[b"data\\sprite\\", MONSTER, b"\\poring.spr"]),
            ),
            (
                "missing_state_icon",
                b"data\\texture\\effect\\B.tga".to_vec(),
            ),
        ];
        want.sort();
        assert_eq!(got, want);
    }
}
