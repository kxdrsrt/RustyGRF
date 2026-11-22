//! Fills entries of the client's text tables that are still Korean (they show as strange
//! characters such as `¿µÈ¥ °ú¿©ÀÇ` in an English client) with the English entry of the same key
//! from the ROenglishRE translation (Pre-Renewal first, then Renewal). Entries that are English
//! already are never touched, and neither are keys the translation does not have.
//!
//! Tables handled (an entry is a block or a line with the same key):
//! - `stateicon/stateiconinfo.lub` (buff and status texts), `skillinfoz/skilldescript.lub`
//!   (skill descriptions),
//! - `mapnametable.txt` (`map.rsw#name#`), `cardprefixnametable.txt` (`id#text#`),
//! - `msgstring_kr_s.lub`: hotkey names "Hotkey2 1-1번" lose the Korean suffix,
//! - `tipoftheday.txt`: the Korean bullet (0xA1F7) and quotes (0xA1 0x6D/0x6E) show as `¡÷` and
//!   `¡m` in an English client; they become `- ` and ` '` / `' `,
//! - `System/itemInfo_Sak.lub`, `System/itemInfo_kro.lua` (loose): item names and descriptions of
//!   items whose name is still Korean; the resource names (sprite file names) are Korean on
//!   purpose and stay.
//!
//! The text is handled as bytes (the original script read it as Latin-1 text).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{
    GrfTool, Result, find_sub, has_high, is_word, is_ws, make_dirs, msg, read, replace_all, write,
};

/// A table that is a sequence of blocks, one per key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocks {
    /// `StateIconList[EFST_IDs.X] = { ... }`
    StateIcon,
    /// `\t[SKID.X] = { ... }`
    Skill,
}

impl Blocks {
    /// The key when a block starts here (`start`: the position is the start of a line).
    fn key_at(self, t: &[u8], p: usize, whole: bool) -> Option<(usize, usize)> {
        let prefix: &[u8] = match self {
            Self::StateIcon => b"StateIconList[EFST_IDs.",
            Self::Skill => b"\t[SKID.",
        };
        if !t[p..].starts_with(prefix) {
            return None;
        }
        let a = p + prefix.len();
        let mut b = a;
        while b < t.len() && is_word(t[b]) {
            b += 1;
        }
        if b == a || t.get(b) != Some(&b']') {
            return None;
        }
        if whole && self == Self::Skill && !t[b + 1..].starts_with(b" = {") {
            return None;
        }
        Some((a, b))
    }

    /// `re.split` at the start of every block: the text before the first one, then one piece
    /// per block.
    fn split(self, t: &[u8]) -> Vec<&[u8]> {
        let mut starts = Vec::new();
        let mut p = 0;
        loop {
            if self.key_at(t, p, true).is_some() {
                starts.push(p);
            }
            match t[p..].iter().position(|&b| b == b'\n') {
                Some(i) => p += i + 1,
                None => break,
            }
            if p >= t.len() {
                break;
            }
        }
        let mut out = Vec::new();
        let mut prev = 0;
        for s in starts {
            out.push(&t[prev..s]);
            prev = s;
        }
        out.push(&t[prev..]);
        out
    }

    fn key(self, block: &[u8]) -> Option<&[u8]> {
        self.key_at(block, 0, false).map(|(a, b)| &block[a..b])
    }
}

/// A block and the closing brace of the whole table, when the block is the last one.
fn split_tail(block: &[u8]) -> (&[u8], &[u8]) {
    for i in 0..block.len() {
        let mut j = i;
        if block[j] == b'\r' {
            j += 1;
        }
        if block.get(j) != Some(&b'\n') {
            continue;
        }
        j += 1;
        if block.get(j) == Some(&b'}') && block[j + 1..].iter().all(|&b| is_ws(b)) {
            return (&block[..i], &block[i..]);
        }
    }
    (block, &[])
}

/// Blocks with Korean bytes replaced by the English block of the same key; the keys filled.
pub fn fill_blocks(ours: &[u8], cands: &[Vec<u8>], kind: Blocks) -> (Vec<u8>, Vec<String>) {
    let mut english: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    for text in cands {
        for b in kind.split(text).into_iter().skip(1) {
            let body = split_tail(b).0;
            if let Some(k) = kind.key(b)
                && !has_high(body)
            {
                english.entry(k.to_vec()).or_insert_with(|| body.to_vec());
            }
        }
    }
    let pieces = kind.split(ours);
    let mut out = pieces[0].to_vec();
    let mut changed = Vec::new();
    for b in &pieces[1..] {
        let (body, tail) = split_tail(b);
        match kind.key(b) {
            Some(k) if has_high(body) && english.contains_key(k) => {
                out.extend_from_slice(&english[k]);
                out.extend_from_slice(tail);
                changed.push(String::from_utf8_lossy(k).into_owned());
            }
            _ => out.extend_from_slice(b),
        }
    }
    (out, changed)
}

/// Lines with their endings; only `\n` separates them.
fn lines(text: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut p = 0;
    while p < text.len() {
        let end = text[p..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(text.len(), |i| p + i + 1);
        out.push(&text[p..end]);
        p = end;
    }
    out
}

fn split_hash(body: &[u8]) -> Vec<&[u8]> {
    body.split(|&b| b == b'#').collect()
}

pub fn fill_lines(ours: &[u8], cands: &[Vec<u8>]) -> (Vec<u8>, Vec<String>) {
    let mut english: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    for text in cands {
        for line in lines(text) {
            let line = trim_eol(line);
            if line.starts_with(b"//") || !line.contains(&b'#') || has_high(line) {
                continue;
            }
            let key = line.split(|&b| b == b'#').next().unwrap_or(&[]);
            english.entry(key.to_vec()).or_insert_with(|| line.to_vec());
        }
    }
    let mut out = Vec::new();
    let mut changed = Vec::new();
    for line in lines(ours) {
        let body = trim_eol(line);
        let eol = &line[body.len()..];
        let fields = split_hash(body);
        let key = fields[0];
        let comment = body.starts_with(b"//");
        if !comment && body.contains(&b'#') && has_high(body) && english.contains_key(key) {
            out.extend_from_slice(&english[key]);
            out.extend_from_slice(eol);
            changed.push(String::from_utf8_lossy(key).into_owned());
        } else if !comment
            && fields.len() > 3
            && !fields[1].is_empty()
            && !has_high(fields[1])
            && fields[2..].iter().any(|f| has_high(f))
        {
            // `key#English name#` followed by left-over Korean: only the first two fields are read
            out.extend_from_slice(fields[0]);
            out.push(b'#');
            out.extend_from_slice(fields[1]);
            out.push(b'#');
            out.extend_from_slice(eol);
            changed.push(String::from_utf8_lossy(key).into_owned());
        } else {
            out.extend_from_slice(line);
        }
    }
    (out, changed)
}

fn trim_eol(line: &[u8]) -> &[u8] {
    let n = line
        .iter()
        .rposition(|&b| b != b'\r' && b != b'\n')
        .map_or(0, |i| i + 1);
    &line[..n]
}

/// The bytes of the Korean bullet and quotes of `tipoftheday.txt` become plain text.
pub fn fill_symbols(ours: &[u8]) -> (Vec<u8>, Vec<String>) {
    let symbols: [(&[u8], &[u8]); 3] = [(b"\xa1\xf7", b"- "), (b"\xa1m", b" '"), (b"\xa1n", b"' ")];
    let mut text = ours.to_vec();
    let mut changed = Vec::new();
    for (old, new) in symbols {
        let (t, n) = replace_all(&text, old, new);
        if n > 0 {
            changed.extend(std::iter::repeat_n(
                String::from_utf8_lossy(old).into_owned(),
                n,
            ));
            text = t;
        }
    }
    (text, changed)
}

/// "Hotkey2 1-1번" (번 = "No.") in the hotkey window loses the Korean suffix.
pub fn fill_hotkeys(ours: &[u8]) -> (Vec<u8>, Vec<String>) {
    let mut out = Vec::with_capacity(ours.len());
    let mut changed = Vec::new();
    let mut i = 0;
    while i < ours.len() {
        let m = &ours[i..];
        if m.len() >= 12
            && m.starts_with(b"Hotkey2 ")
            && m[8].is_ascii_digit()
            && m[9] == b'-'
            && m[10].is_ascii_digit()
            && m[11..].starts_with(b"\xb9\xf8")
        {
            out.extend_from_slice(&m[..11]);
            changed.push("(Hotkey2 \\d-\\d)\u{b9}\u{f8}".to_string());
            i += 13;
        } else {
            out.push(ours[i]);
            i += 1;
        }
    }
    (out, changed)
}

// ---- item tables ----

const ITEM_NAME_FIELDS: [&str; 2] = ["unidentifiedDisplayName", "identifiedDisplayName"];
const ITEM_DESC_FIELDS: [&str; 2] = ["unidentifiedDescriptionName", "identifiedDescriptionName"];

/// `\t[<number>] = {` at the start of a line: (end of the match, the number without zeros).
fn item_start(t: &[u8], p: usize) -> Option<(usize, String)> {
    if !t[p..].starts_with(b"\t[") {
        return None;
    }
    let a = p + 2;
    let mut b = a;
    while b < t.len() && t[b].is_ascii_digit() {
        b += 1;
    }
    if b == a || !t[b..].starts_with(b"] = {") {
        return None;
    }
    let digits = String::from_utf8_lossy(&t[a..b])
        .trim_start_matches('0')
        .to_string();
    Some((
        b + 5,
        if digits.is_empty() {
            "0".into()
        } else {
            digits
        },
    ))
}

/// The text before the first item and the items: (number, text after the opening brace).
fn item_blocks(t: &[u8]) -> (&[u8], Vec<(String, &[u8])>) {
    let mut starts: Vec<(usize, usize, String)> = Vec::new();
    let mut p = 0;
    loop {
        if let Some((end, num)) = item_start(t, p) {
            starts.push((p, end, num));
        }
        match t[p..].iter().position(|&b| b == b'\n') {
            Some(i) => p += i + 1,
            None => break,
        }
        if p >= t.len() {
            break;
        }
    }
    let head_end = starts.first().map_or(t.len(), |s| s.0);
    let mut blocks = Vec::new();
    for (i, (_, end, num)) in starts.iter().enumerate() {
        let stop = starts.get(i + 1).map_or(t.len(), |s| s.0);
        blocks.push((num.clone(), &t[*end..stop]));
    }
    (&t[..head_end], blocks)
}

/// The text of one field of an item block, from its indent to the comma after it.
fn item_field(block: &[u8], key: &str) -> Option<(usize, usize)> {
    if key.ends_with("DescriptionName") {
        let needle = format!("\t\t{key} = {{").into_bytes();
        let mut from = 0;
        while let Some(s) = find_sub(block, &needle, from) {
            let mut e = s + needle.len();
            while e < block.len() {
                if block[e] == b'}' && desc_end_follows(block, e + 1) {
                    return Some((s, e + 1));
                }
                e += 1;
            }
            from = s + 1;
        }
        None
    } else {
        let needle = format!("\t\t{key} = \"").into_bytes();
        let mut from = 0;
        while let Some(s) = find_sub(block, &needle, from) {
            let mut e = s + needle.len();
            while e < block.len() && !matches!(block[e], b'"' | b'\r' | b'\n') {
                e += 1;
            }
            if block.get(e) == Some(&b'"') {
                return Some((s, e + 1));
            }
            from = s + 1;
        }
        None
    }
}

/// `,?\r?\n\t\t[A-Za-z]` or `,?\r?\n\t}` at `k`.
fn desc_end_follows(b: &[u8], mut k: usize) -> bool {
    if b.get(k) == Some(&b',') {
        k += 1;
    }
    if b.get(k) == Some(&b'\r') {
        k += 1;
    }
    if b.get(k) != Some(&b'\n') {
        return false;
    }
    k += 1;
    (b.get(k) == Some(&b'\t')
        && b.get(k + 1) == Some(&b'\t')
        && b.get(k + 2).is_some_and(u8::is_ascii_alphabetic))
        || (b.get(k) == Some(&b'\t') && b.get(k + 1) == Some(&b'}'))
}

/// `\{\s*"(\.\.\.|)"\s*\}\s*$`: a description that is only a placeholder.
fn is_placeholder(c: &[u8]) -> bool {
    let skip = |mut i: usize| {
        while i < c.len() && is_ws(c[i]) {
            i += 1;
        }
        i
    };
    for p in 0..c.len() {
        if c[p] != b'{' {
            continue;
        }
        let mut i = skip(p + 1);
        if c.get(i) != Some(&b'"') {
            continue;
        }
        i += 1;
        if c[i..].starts_with(b"...") && c.get(i + 3) == Some(&b'"') {
            i += 3;
        }
        if c.get(i) != Some(&b'"') {
            continue;
        }
        i = skip(i + 1);
        if c.get(i) != Some(&b'}') {
            continue;
        }
        if skip(i + 1) == c.len() {
            return true;
        }
    }
    false
}

const LABELS: &[(&str, &str)] = &[
    ("무게", "Weight"),
    ("계열", "Class"),
    ("장착", "Job"),
    ("요구 레벨", "Required Level"),
    ("요구레벨", "Required Level"),
    ("방어", "Defense"),
    ("위치", "Position"),
    ("공격", "Attack"),
    ("무기 레벨", "Weapon Level"),
    ("종류", "Type"),
    ("속성", "Element"),
];

const VALUES: &[(&str, &str)] = &[
    ("전 직업", "All Classes"),
    ("전직업", "All Classes"),
    ("노비스를 제외한 전 직업", "All Classes except Novice"),
    ("쉐도우 장비", "Shadow Equipment"),
    ("투구", "Headgear"),
    ("액세서리", "Accessory"),
    ("신발", "Shoes"),
    ("걸칠것", "Garment"),
    ("의상장비", "Costume"),
    ("카드", "Card"),
    ("갑옷", "Armor"),
    ("방패", "Shield"),
    ("한손 지팡이", "One-handed Staff"),
    ("둔기", "Mace"),
    ("단검", "Dagger"),
    ("무기", "Weapon"),
    ("상단", "Upper"),
    ("중단", "Middle"),
    ("하단", "Lower"),
    ("아머", "Armor"),
    ("웨폰", "Weapon"),
    ("쉴드", "Shield"),
    ("슈즈", "Shoes"),
    ("펜던트", "Pendant"),
    ("이어링", "Earring"),
    ("몬스터 알", "Monster Egg"),
    ("테이밍 아이템", "Taming Item"),
    ("화", "Fire"),
    ("수", "Water"),
    ("암", "Dark"),
    ("풍", "Wind"),
    ("지", "Earth"),
    ("염", "Ghost"),
];

const SENTENCES: &[(&str, &str)] = &[
    (
        "타 계정과 거래가 불가능한 아이템 입니다.",
        "This item cannot be traded with other accounts.",
    ),
    (
        "본 아이템을 개봉하시면 청약철회 대상에서 제외 됩니다.",
        "Once this item is opened, the purchase can no longer be withdrawn.",
    ),
    (
        "주의! 장착한 장비만 제련이 가능합니다.",
        "Caution! Only equipped gear can be refined.",
    ),
    (
        "의상 상단 슬롯에 아래의 능력치를 부여한다.",
        "Grants the following stats in the Costume Upper slot.",
    ),
    (
        "의상 중단 슬롯에 아래의 능력치를 부여한다.",
        "Grants the following stats in the Costume Middle slot.",
    ),
    (
        "의상 하단 슬롯에 아래의 능력치를 부여한다.",
        "Grants the following stats in the Costume Lower slot.",
    ),
    (
        "의상 걸칠것 슬롯에 아래의 능력치를 부여한다.",
        "Grants the following stats in the Costume Garment slot.",
    ),
];

fn py_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

/// `re.sub(r"(?<![가-힣])<label>(?=\s*:)", value, text)`
fn replace_label(text: &str, label: &str, value: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut prev: Option<char> = None;
    while i < text.len() {
        let rest = &text[i..];
        let hangul = prev.is_some_and(|c| ('\u{ac00}'..='\u{d7a3}').contains(&c));
        if !hangul
            && rest.starts_with(label)
            && rest[label.len()..]
                .trim_start_matches(py_space)
                .starts_with(':')
        {
            out.push_str(value);
            prev = label.chars().next_back();
            i += label.len();
        } else {
            let c = rest.chars().next().unwrap_or(' ');
            out.push(c);
            prev = Some(c);
            i += c.len_utf8();
        }
    }
    out
}

/// `re.sub(r"(\^[0-9a-fA-F]{6})([^^]*)(\^000000)", ...)`: the text inside a colour pair that is
/// a known value gets its English name.
fn replace_values(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut copied = 0;
    while i < b.len() {
        if b[i] == b'^' && i + 7 <= b.len() && b[i + 1..i + 7].iter().all(u8::is_ascii_hexdigit) {
            let inner_start = i + 7;
            let inner_end = b[inner_start..]
                .iter()
                .position(|&c| c == b'^')
                .map_or(b.len(), |p| inner_start + p);
            if text[inner_end..].starts_with("^000000") {
                out.push_str(&text[copied..inner_start]);
                let inner = &text[inner_start..inner_end];
                let key = inner.trim_matches(py_space);
                match VALUES.iter().find(|(k, _)| *k == key) {
                    Some((_, v)) => out.push_str(v),
                    None => out.push_str(inner),
                }
                out.push_str("^000000");
                i = inner_end + 7;
                copied = i;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&text[copied..]);
    out
}

fn bars_to_underscores(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = 0;
    for c in text.chars() {
        if c == '\u{2015}' {
            run += 1;
            continue;
        }
        flush_bars(&mut out, run);
        run = 0;
        out.push(c);
    }
    flush_bars(&mut out, run);
    out
}

fn flush_bars(out: &mut String, run: usize) {
    if run >= 3 {
        out.push_str(&"_".repeat(23));
    } else {
        out.extend(std::iter::repeat_n('\u{2015}', run));
    }
}

/// One description line (bytes); returned unchanged when it is not Korean (or not valid CP949).
fn translate_item_line(line: &[u8]) -> Result<Vec<u8>> {
    if !has_high(line) {
        return Ok(line.to_vec());
    }
    let Some(ko) = encoding_rs::EUC_KR.decode_without_bom_handling_and_without_replacement(line)
    else {
        return Ok(line.to_vec());
    };
    let mut ko = bars_to_underscores(&ko);
    for (k, v) in SENTENCES {
        ko = ko.replace(k, v);
    }
    for (k, v) in LABELS {
        ko = replace_label(&ko, k, v);
    }
    let ko = replace_values(&ko);
    let (bytes, _, bad) = encoding_rs::EUC_KR.encode(&ko);
    if bad {
        return msg("a translated item line has a character CP949 cannot hold");
    }
    Ok(bytes.into_owned())
}

/// `re.sub(r'"([^"\r\n]*)"', ...)`: every quoted text of a field is translated.
fn translate_item_field(text: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'"' {
            let mut j = i + 1;
            while j < text.len() && !matches!(text[j], b'"' | b'\r' | b'\n') {
                j += 1;
            }
            if text.get(j) == Some(&b'"') {
                out.push(b'"');
                out.extend(translate_item_line(&text[i + 1..j])?);
                out.push(b'"');
                i = j + 1;
                continue;
            }
        }
        out.push(text[i]);
        i += 1;
    }
    Ok(out)
}

/// Item names and descriptions that are Korean, taken from the translation by item number. A
/// description the translation lacks (or has only as `"..."`) is translated from its template.
pub fn fill_items(ours: &[u8], translations: &[Vec<u8>]) -> Result<(Vec<u8>, usize)> {
    let (head, blocks) = item_blocks(ours);
    let mut english: HashMap<String, Vec<&[u8]>> = HashMap::new();
    for t in translations {
        for (num, block) in item_blocks(t).1 {
            english.entry(num).or_default().push(block);
        }
    }
    let mut out = head.to_vec();
    let mut changed = 0;
    for (num, block) in blocks {
        let mut block = block.to_vec();
        for key in ITEM_NAME_FIELDS.iter().chain(ITEM_DESC_FIELDS.iter()) {
            let Some((ms, me)) = item_field(&block, key) else {
                continue;
            };
            if !has_high(&block[ms..me]) {
                continue;
            }
            let mine = block[ms..me].to_vec();
            let cands: Vec<Vec<u8>> = english
                .get(&num)
                .into_iter()
                .flatten()
                .filter_map(|tr| item_field(tr, key).map(|(s, e)| tr[s..e].to_vec()))
                .filter(|c| !has_high(c))
                .collect();
            let real: Vec<&Vec<u8>> = cands.iter().filter(|c| !is_placeholder(c)).collect();
            let text = if let Some(r) = real.first() {
                (*r).clone()
            } else if key.ends_with("DescriptionName")
                && (*key == "identifiedDescriptionName" || cands.is_empty())
            {
                translate_item_field(&mine)?
            } else if let Some(c) = cands.first() {
                c.clone()
            } else {
                continue;
            };
            if text != mine {
                block.splice(ms..me, text);
                changed += 1;
            }
        }
        out.extend_from_slice(format!("\t[{num}] = {{").as_bytes());
        out.extend_from_slice(&block);
    }
    Ok((out, changed))
}

/// Name and description fields that still have Korean bytes.
pub fn korean_fields(text: &[u8]) -> usize {
    item_blocks(text)
        .1
        .iter()
        .map(|(_, b)| {
            ITEM_NAME_FIELDS
                .iter()
                .chain(ITEM_DESC_FIELDS.iter())
                .filter(|k| item_field(b, k).is_some_and(|(s, e)| has_high(&b[s..e])))
                .count()
        })
        .sum()
}

// ---- the whole run ----

const ITEM_FILES: [&str; 2] = ["System/itemInfo_Sak.lub", "System/itemInfo_kro.lua"];
const ITEM_TRANSLATION: &str = "SystemEN/LuaFiles514/itemInfo.lua";

#[derive(Debug, Clone, Copy)]
enum Mode {
    Blocks(Blocks),
    Lines,
    Symbols,
    Hotkeys,
}

const TARGETS: [(&str, Mode); 6] = [
    (
        "data/luafiles514/lua files/stateicon/stateiconinfo.lub",
        Mode::Blocks(Blocks::StateIcon),
    ),
    (
        "data/luafiles514/lua files/skillinfoz/skilldescript.lub",
        Mode::Blocks(Blocks::Skill),
    ),
    ("data/mapnametable.txt", Mode::Lines),
    ("data/cardprefixnametable.txt", Mode::Lines),
    ("data/tipoftheday.txt", Mode::Symbols),
    (
        "data/luafiles514/lua files/msgstring_kr_s.lub",
        Mode::Hotkeys,
    ),
];

#[derive(Debug, Clone)]
pub struct Options {
    /// The client folder with `DATA.INI` (only read).
    pub full: PathBuf,
    /// A checkout of ROenglishRE.
    pub roen: PathBuf,
    /// Where the fixed archive files go (below it with their path inside the archive).
    pub out: PathBuf,
    /// Where the fixed loose files go (with their path inside the client folder).
    pub out_loose: Option<PathBuf>,
}

/// The archives of `DATA.INI`, in order.
fn archives(full: &Path) -> Result<Vec<String>> {
    let p = full.join("DATA.INI");
    let data = read(&p)?;
    let mut names = Vec::new();
    for line in data.split(|&b| b == b'\n') {
        let start = line.iter().position(|&b| !is_ws(b)).unwrap_or(line.len());
        let line = super::rstrip_ws(&line[start..]);
        if let Some(eq) = line.iter().position(|&b| b == b'=')
            && line.to_ascii_lowercase().ends_with(b".grf")
        {
            names.push(super::latin1(&line[eq + 1..]));
        }
    }
    Ok(names)
}

/// The file as the client sees it: the first archive of `DATA.INI` that has it.
fn read_effective(tool: &GrfTool, full: &Path, rel: &str) -> Result<Option<(String, Vec<u8>)>> {
    let path = rel.replace('/', "\\");
    for grf in archives(full)? {
        if let Some(d) = tool.cat(&full.join(&grf), &path)? {
            return Ok(Some((grf, d)));
        }
    }
    Ok(None)
}

fn read_translation(roen: &Path, rel: &str) -> Result<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    for sub in ["Pre-Renewal", "Renewal"] {
        let p = roen.join("Translation").join(sub).join(rel);
        if p.exists() {
            out.push(read(&p)?);
        }
    }
    Ok(out)
}

fn count_high(b: &[u8]) -> usize {
    b.iter().filter(|&&c| c >= 0x80).count()
}

/// The file as the client sees it (`rel` with `/`): the name of the archive that has it and the
/// content, or `None`.
pub type Reader<'a> = &'a mut dyn FnMut(&str) -> Result<Option<(String, Vec<u8>)>>;

/// Runs the fill with the archives of the client's `DATA.INI` (read by `rgrf`); `log` gets the
/// lines the script printed. Returns the number of entries filled.
pub fn run(tool: &GrfTool, opts: &Options, log: &mut dyn FnMut(&str)) -> Result<usize> {
    let mut read_archives = |rel: &str| read_effective(tool, &opts.full, rel);
    run_with(&mut read_archives, opts, log)
}

/// [`run`] with the way to read a table from the archives given. Loose files (`out_loose`) are
/// read from `opts.full`.
pub fn run_with(read_table: Reader, opts: &Options, log: &mut dyn FnMut(&str)) -> Result<usize> {
    let mut total = 0;
    if let Some(loose) = &opts.out_loose {
        let translations = read_translation(&opts.roen, ITEM_TRANSLATION)?;
        for rel in ITEM_FILES {
            let src = opts.full.join(rel);
            if !src.exists() {
                continue;
            }
            let ours = read(&src)?;
            let (new, changed) = fill_items(&ours, &translations)?;
            log(&format!(
                "{rel}: {changed} item fields filled; Korean name/description fields left {} -> {}",
                korean_fields(&ours),
                korean_fields(&new)
            ));
            if changed > 0 {
                let dest = loose.join(rel);
                if let Some(d) = dest.parent() {
                    make_dirs(d)?;
                }
                write(&dest, &new)?;
                total += changed;
            }
        }
    }
    for (rel, mode) in TARGETS {
        let Some((grf, ours)) = read_table(rel)? else {
            log(&format!("{rel}: not in the archives, skipped"));
            continue;
        };
        let (new, changed) = match mode {
            Mode::Symbols => fill_symbols(&ours),
            Mode::Hotkeys => fill_hotkeys(&ours),
            Mode::Blocks(kind) => fill_blocks(&ours, &read_translation(&opts.roen, rel)?, kind),
            Mode::Lines => fill_lines(&ours, &read_translation(&opts.roen, rel)?),
        };
        log(&format!(
            "{rel} ({grf}): {} entries filled; Korean bytes left {} -> {}",
            changed.len(),
            count_high(&ours),
            count_high(&new)
        ));
        if !changed.is_empty() {
            let dest = opts.out.join(rel);
            if let Some(d) = dest.parent() {
                make_dirs(d)?;
            }
            write(&dest, &new)?;
            total += changed.len();
        }
    }
    log(&format!("{total} entries filled"));
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn korean_blocks_take_the_english_block_of_their_key() {
        let ours = b"head\nStateIconList[EFST_IDs.A] = {\n\tname = \"\xbf\xb5\"\n}\nStateIconList[EFST_IDs.B] = {\n\tname = \"ok\"\n}\n";
        let tr = b"StateIconList[EFST_IDs.A] = {\n\tname = \"Soul Link\"\n}\nStateIconList[EFST_IDs.B] = {\n\tname = \"other\"\n}\n".to_vec();
        let (out, changed) = fill_blocks(ours, &[tr], Blocks::StateIcon);
        assert_eq!(changed, ["A"]);
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("Soul Link\"\n}\nStateIconList[EFST_IDs.B]"),
            "{text}"
        );
        assert!(text.contains("\"ok\""));
    }

    #[test]
    fn lines_symbols_and_hotkeys() {
        let (out, changed) = fill_lines(
            b"1#\xb0\xa1#\r\n//c#\xb0\r\n2#ok#\xb0#\n",
            &[b"1#English#\r\n".to_vec()],
        );
        assert_eq!(out, b"1#English#\r\n//c#\xb0\r\n2#ok#\n");
        assert_eq!(changed, ["1", "2"]);
        assert_eq!(fill_symbols(b"a\xa1\xf7b\xa1mc").0, b"a- b 'c");
        assert_eq!(fill_hotkeys(b"Hotkey2 1-1\xb9\xf8 x").0, b"Hotkey2 1-1 x");
    }

    #[test]
    fn item_templates_are_translated() {
        let ours = "\t[1] = {\n\t\tidentifiedDisplayName = \"\u{ac00}\",\n\t\tidentifiedDescriptionName = {\n\t\t\t\"무게 : ^777777N^000000\",\n\t\t\t\"계열 : ^777777단검^000000\"\n\t\t},\n\t\tslotCount = 0\n\t}\n";
        let (cp, _, _) = encoding_rs::EUC_KR.encode(ours);
        let (out, changed) = fill_items(&cp, &[]).unwrap();
        // the description is translated from its template, the name has no English twin
        assert_eq!(changed, 1);
        let (text, _) = encoding_rs::EUC_KR.decode_without_bom_handling(&out);
        assert!(text.contains("\"Weight : ^777777N^000000\""), "{text}");
        assert!(text.contains("\"Class : ^777777Dagger^000000\""), "{text}");
    }
}
