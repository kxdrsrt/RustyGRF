//! Carries YOUR OWN edits over to a newer translation, entry by entry (not line by line).
//!
//! Why not a plain file merge: your old files come from an older translation plus your edits, and
//! between the two versions lines move and IDs shift. So "your edit" is defined per entry as: your
//! text for that key never appeared in ANY historical version of the project (git history up to
//! `built_before`). Everything else is taken from the new translation. Handled files
//! (`client-dev/grf/<archive>/data/...`):
//!
//! - `msgstringtable.txt` (cdata): base = the NEW file; lines you shortened to `max_short`
//!   characters or fewer (tabs I/E/M/F, 1st, 2nd, ...) are put back where the new file still has
//!   the same message at that line number as an older project version.
//! - `skillinfolist.lub` (core): base = YOUR file (keeps all your skills, also those the
//!   translation does not have); `SkillName` of a skill that came unchanged from an older project
//!   version is updated to the new name; names you made yourself are kept.
//! - `skilldescript.lub` (core): base = YOUR file; per skill: an entry equal to an older project
//!   version is replaced by the new entry, an entry you edited is kept.
//!
//! Output: `<out>/<archive>/data/...` (only files that change) and `<out>/report.md`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::{
    HistoryBlob, Result, blob, find_ci, history_blobs, is_word, is_ws, join_lines_n, latin1,
    make_dirs, msg, read, rstrip_byte, rstrip_ws, split_lines_n, write,
};

#[derive(Debug, Clone)]
pub struct Options {
    pub repo: PathBuf,
    pub client_dev: PathBuf,
    /// The output of `assemble` of the new translation.
    pub asm: PathBuf,
    /// The output folder; it must not exist.
    pub out: PathBuf,
    /// The date your GRFs were last built (`YYYY-MM-DD`).
    pub built_before: String,
    /// Skills whose `SkillName` is always kept as yours (e.g. `NPC_EARTHQUAKE`).
    pub keep_skill: Vec<String>,
    /// The longest text (before `#`) still counted as your abbreviation.
    pub max_short: usize,
}

impl Options {
    pub fn new(repo: PathBuf, client_dev: PathBuf, asm: PathBuf, out: PathBuf) -> Self {
        Self {
            repo,
            client_dev,
            asm,
            out,
            built_before: "9999-12-31".to_string(),
            keep_skill: Vec::new(),
            max_short: 3,
        }
    }
}

/// `\t[SKID.NAME] = {` at the start of a line: the name and what follows.
fn skid_head(l: &[u8]) -> Option<(&[u8], &[u8])> {
    let rest = l.strip_prefix(b"\t[SKID.")?;
    let n = rest.iter().take_while(|&&b| is_word(b)).count();
    if n == 0 {
        return None;
    }
    let after = rest[n..].strip_prefix(b"] = {")?;
    Some((&rest[..n], after))
}

fn is_start(l: &[u8]) -> Option<&[u8]> {
    let (name, after) = skid_head(l)?;
    after.iter().all(|&b| is_ws(b)).then_some(name)
}

fn is_single(l: &[u8]) -> Option<&[u8]> {
    let (name, after) = skid_head(l)?;
    let t = rstrip_ws(after);
    (t.ends_with(b"}") || t.ends_with(b"},")).then_some(name)
}

fn is_end(l: &[u8]) -> bool {
    l.strip_prefix(b"\t}").is_some_and(|rest| {
        let t = rstrip_ws(rest);
        t.is_empty() || t == b","
    })
}

/// Skills of a file: (name, first line, last line) in the order the names first appear (a name
/// that appears again keeps its place and takes the later lines).
type Entries = Vec<(Vec<u8>, (usize, usize))>;

fn entries(lines: &[&[u8]]) -> Entries {
    let mut out: Entries = Vec::new();
    let mut index: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut put = |name: &[u8], range: (usize, usize)| match index.get(name) {
        Some(&i) => out[i].1 = range,
        None => {
            index.insert(name.to_vec(), out.len());
            out.push((name.to_vec(), range));
        }
    };
    let mut cur: Option<(usize, Vec<u8>)> = None;
    for (i, l) in lines.iter().enumerate() {
        match &cur {
            None => {
                if let Some(name) = is_start(l) {
                    cur = Some((i, name.to_vec()));
                } else if let Some(name) = is_single(l) {
                    put(name, (i, i));
                }
            }
            Some((start, name)) => {
                if is_end(l) {
                    put(name, (*start, i));
                    cur = None;
                }
            }
        }
    }
    out
}

fn get(e: &Entries, name: &[u8]) -> Option<(usize, usize)> {
    e.iter().find(|(n, _)| n == name).map(|(_, r)| *r)
}

/// `^\s*SkillName = "(.*)",?\s*$`: (start of the name, end of the name) in the line.
fn skill_name(line: &[u8]) -> Option<(usize, usize)> {
    let start = line.iter().position(|&b| !is_ws(b)).unwrap_or(line.len());
    let head = b"SkillName = \"";
    if !line[start..].starts_with(head) {
        return None;
    }
    let from = start + head.len();
    let mut t = rstrip_ws(&line[from..]).len() + from;
    if t > from && line[t - 1] == b',' {
        t -= 1;
    }
    (t > from && line[t - 1] == b'"').then_some((from, t - 1))
}

fn norm(block: &[&[u8]]) -> Vec<Vec<u8>> {
    block
        .iter()
        .map(|l| rstrip_ws(rstrip_byte(l, b'\r')).to_vec())
        .collect()
}

/// Every historical version of the files whose repo path ends with `suffix`: (date, content).
fn versions(
    repo: &Path,
    blobs: &[HistoryBlob],
    suffix: &str,
    before: &str,
) -> Result<Vec<Vec<u8>>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for b in blobs {
        if b.path.to_lowercase().ends_with(suffix)
            && b.date.as_str() <= before
            && seen.insert(b.sha.clone())
        {
            out.push(blob(repo, &b.sha)?);
        }
    }
    Ok(out)
}

fn write_out(out: &Path, archive: &str, rel: &str, data: &[u8]) -> Result<()> {
    let mut dest = out.join(archive);
    for part in rel.split('/') {
        dest.push(part);
    }
    if let Some(d) = dest.parent() {
        make_dirs(d)?;
    }
    write(&dest, data)
}

fn ends_with_cr(l: &[u8]) -> bool {
    l.last() == Some(&b'\r')
}

fn msgstringtable(o: &Options, blobs: &[HistoryBlob], rep: &mut Vec<String>) -> Result<()> {
    let rel = "data/msgstringtable.txt";
    let (Some(user_p), Some(new_p)) = (
        find_ci(&o.client_dev.join("grf").join("cdata"), rel),
        find_ci(&o.asm.join("grf"), rel),
    ) else {
        return Ok(());
    };
    let (user_b, new_b) = (read(&user_p)?, read(&new_p)?);
    let (user, new) = (split_lines_n(&user_b), split_lines_n(&new_b));
    let mut seen: HashMap<usize, HashSet<Vec<u8>>> = HashMap::new();
    for b in versions(&o.repo, blobs, "/msgstringtable.txt", &o.built_before)? {
        let b = super::replace_all(&b, b"\r\n", b"\n").0;
        for (i, l) in split_lines_n(&b).into_iter().enumerate() {
            seen.entry(i).or_default().insert(l.to_vec());
        }
    }
    let mut out: Vec<Vec<u8>> = new.iter().map(|l| l.to_vec()).collect();
    let mut applied: Vec<(usize, Vec<u8>, Vec<u8>)> = Vec::new();
    let mut other = 0;
    let empty = HashSet::new();
    for (i, u) in user.iter().enumerate() {
        if i >= new.len() {
            break;
        }
        let u = rstrip_byte(u, b'\r');
        let n = rstrip_byte(new[i], b'\r');
        let seen_i = seen.get(&i).unwrap_or(&empty);
        if u == n || seen_i.contains(u) {
            continue;
        }
        let stem = rstrip_byte(u, b'#').len();
        let short = u.ends_with(b"#") && stem > 0 && stem <= o.max_short && u.is_ascii();
        if short && u.len() < n.len() && seen_i.contains(n) {
            let mut line = u.to_vec();
            if ends_with_cr(new[i]) {
                line.push(b'\r');
            }
            out[i] = line;
            applied.push((i + 1, u.to_vec(), n.to_vec()));
        } else {
            other += 1;
        }
    }
    rep.push(format!(
        "## msgstringtable.txt (cdata)\nBase: the new translation file ({} lines). Your own lines put back: {}.\n",
        new.len(),
        applied.len()
    ));
    for (i, u, n) in &applied {
        rep.push(format!(
            "- line {i}: `{}` (new: `{}`)",
            latin1(u),
            latin1(n)
        ));
    }
    rep.push(format!(
        "\n{other} more lines of your file differ from the new file but are from an older translation style or a different client table (not a short abbreviation); the new text is used.\n"
    ));
    write_out(&o.out, "cdata", rel, &join_lines_n(&out))
}

const SKILL_RELS: [&str; 2] = [
    "data/luafiles514/lua files/skillinfoz/skillinfolist.lub",
    "data/lua files/skillinfoz/skillinfolist.lub",
];
const DESCRIPT_RELS: [&str; 2] = [
    "data/luafiles514/lua files/skillinfoz/skilldescript.lub",
    "data/lua files/skillinfoz/skilldescript.lub",
];

/// A skill, the name in your file and the name in the new translation.
type NameChange = (Vec<u8>, Vec<u8>, Vec<u8>);

fn skills(o: &Options, blobs: &[HistoryBlob], rep: &mut Vec<String>) -> Result<()> {
    let mut hist_names: HashMap<Vec<u8>, HashSet<Vec<u8>>> = HashMap::new();
    let mut hist_desc: HashMap<Vec<u8>, HashSet<Vec<Vec<u8>>>> = HashMap::new();
    for b in versions(&o.repo, blobs, "/skillinfolist.lub", &o.built_before)? {
        let lines = split_lines_n(&b);
        for (k, (s, e)) in entries(&lines) {
            for l in &lines[s..e] {
                if let Some((a, z)) = skill_name(rstrip_byte(l, b'\r')) {
                    let l = rstrip_byte(l, b'\r');
                    hist_names
                        .entry(k.clone())
                        .or_default()
                        .insert(l[a..z].to_vec());
                }
            }
        }
    }
    for b in versions(&o.repo, blobs, "/skilldescript.lub", &o.built_before)? {
        let lines = split_lines_n(&b);
        for (k, (s, e)) in entries(&lines) {
            hist_desc.entry(k).or_default().insert(norm(&lines[s..=e]));
        }
    }
    let core = o.client_dev.join("grf").join("core");
    let new_root = o.asm.join("grf");
    for rel in SKILL_RELS {
        let (Some(u_p), Some(n_p)) = (find_ci(&core, rel), find_ci(&new_root, rel)) else {
            continue;
        };
        let (user_b, new_b) = (read(&u_p)?, read(&n_p)?);
        let mut user: Vec<Vec<u8>> = split_lines_n(&user_b).iter().map(|l| l.to_vec()).collect();
        let new = split_lines_n(&new_b);
        let user_refs: Vec<&[u8]> = user.iter().map(Vec::as_slice).collect();
        let (ue, ne) = (entries(&user_refs), entries(&new));
        let mut newnames: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
        for (k, (s, e)) in &ne {
            for l in &new[*s..*e] {
                let l = rstrip_byte(l, b'\r');
                if let Some((a, z)) = skill_name(l) {
                    newnames.insert(k.clone(), l[a..z].to_vec());
                }
            }
        }
        let (mut kept, mut updated): (Vec<NameChange>, Vec<NameChange>) = (Vec::new(), Vec::new());
        for (k, (s, e)) in &ue {
            for slot in user.iter_mut().take(*e).skip(*s) {
                let line = rstrip_byte(slot, b'\r').to_vec();
                let Some((a, z)) = skill_name(&line) else {
                    continue;
                };
                let old = line[a..z].to_vec();
                let Some(new_name) = newnames.get(k) else {
                    break;
                };
                if &old == new_name {
                    break;
                }
                let keep = o.keep_skill.iter().any(|s| s.as_bytes() == k.as_slice());
                if hist_names.get(k).is_some_and(|h| h.contains(&old)) && !keep {
                    let had_cr = ends_with_cr(slot);
                    let mut l = line[..a].to_vec();
                    l.extend_from_slice(new_name);
                    l.extend_from_slice(&line[z..]);
                    if had_cr {
                        l.push(b'\r');
                    }
                    *slot = l;
                    updated.push((k.clone(), old, new_name.clone()));
                } else {
                    kept.push((k.clone(), old, new_name.clone()));
                }
                break;
            }
        }
        rep.push(format!(
            "## {rel}\nBase: YOUR file ({} skills; the new file has {}). Names kept as yours: {}; updated to the new name: {}.\n",
            ue.len(),
            ne.len(),
            kept.len(),
            updated.len()
        ));
        rep.push("**Your names, kept:**".to_string());
        for (k, old, new_name) in &kept {
            rep.push(format!(
                "- {}: `{}` (new translation: `{}`)",
                latin1(k),
                latin1(old),
                latin1(new_name)
            ));
        }
        rep.push("\n**Updated (they came unchanged from an older project version):**".to_string());
        for (k, old, new_name) in &updated {
            rep.push(format!(
                "- {}: `{}` -> `{}`",
                latin1(k),
                latin1(old),
                latin1(new_name)
            ));
        }
        rep.push(String::new());
        write_out(&o.out, "core", rel, &join_lines_n(&user))?;
    }
    for rel in DESCRIPT_RELS {
        let (Some(u_p), Some(n_p)) = (find_ci(&core, rel), find_ci(&new_root, rel)) else {
            continue;
        };
        let (user_b, new_b) = (read(&u_p)?, read(&n_p)?);
        let (user, new) = (split_lines_n(&user_b), split_lines_n(&new_b));
        let (ue, ne) = (entries(&user), entries(&new));
        let mut sorted: Vec<&(Vec<u8>, (usize, usize))> = ue.iter().collect();
        sorted.sort_by_key(|(_, r)| r.0);
        let mut out: Vec<Vec<u8>> = Vec::new();
        let (mut pos, mut updated) = (0usize, 0usize);
        let mut kept: Vec<String> = Vec::new();
        for (k, (s, e)) in sorted {
            if pos < *s {
                out.extend(user[pos..*s].iter().map(|l| l.to_vec()));
            }
            let mut block: Vec<Vec<u8>> = user[*s..=*e].iter().map(|l| l.to_vec()).collect();
            if let Some((ns, ne_)) = get(&ne, k) {
                let theirs = &new[ns..=ne_];
                let block_refs: Vec<&[u8]> = block.iter().map(Vec::as_slice).collect();
                if norm(&block_refs) != norm(theirs) {
                    if hist_desc
                        .get(k)
                        .is_some_and(|h| h.contains(&norm(&block_refs)))
                    {
                        block = theirs.iter().map(|l| l.to_vec()).collect();
                        updated += 1;
                    } else {
                        kept.push(latin1(k));
                    }
                }
            }
            out.extend(block);
            pos = *e + 1;
        }
        if pos < user.len() {
            out.extend(user[pos..].iter().map(|l| l.to_vec()));
        }
        rep.push(format!(
            "## {rel}\nBase: YOUR file ({} skills; the new file has {}). Updated from the new translation: {updated}; edited by you and kept: {}.\n",
            ue.len(),
            ne.len(),
            kept.len()
        ));
        let shown: Vec<&str> = kept.iter().take(80).map(String::as_str).collect();
        rep.push(format!(
            "**Kept (not found in any older project version):** {}{}\n",
            shown.join(", "),
            if kept.len() > 80 { " ..." } else { "" }
        ));
        write_out(&o.out, "core", rel, &join_lines_n(&out))?;
    }
    Ok(())
}

/// Runs the merge, writes `<out>/report.md` and returns what the script printed.
pub fn run(o: &Options) -> Result<String> {
    if o.out.exists() {
        return msg(format!("{} exists", o.out.display()));
    }
    make_dirs(&o.out)?;
    let blobs = history_blobs(&o.repo)?;
    let mut rep = vec!["# Your edits carried over\n".to_string()];
    msgstringtable(o, &blobs, &mut rep)?;
    skills(o, &blobs, &mut rep)?;
    let report = rep.join("\n");
    write(&o.out.join("report.md"), report.as_bytes())?;
    let printed: Vec<&str> = rep
        .iter()
        .filter(|l| l.starts_with("##") || l.starts_with("Base") || l.starts_with("Names"))
        .map(String::as_str)
        .collect();
    Ok(printed.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_entries_and_names() {
        let text = b"T = {\n\t[SKID.A] = {\n\t\tSkillName = \"Alpha\",\n\t},\n\t[SKID.B] = { SkillName = \"b\" },\n\t[SKID.C] = {\n\t\tx = 1\n\t}\n}\n";
        let lines = split_lines_n(text);
        let e = entries(&lines);
        assert_eq!(
            e.iter()
                .map(|(n, r)| (n.as_slice(), *r))
                .collect::<Vec<_>>(),
            [
                (&b"A"[..], (1, 3)),
                (&b"B"[..], (4, 4)),
                (&b"C"[..], (5, 7))
            ]
        );
        let (a, z) = skill_name(lines[2]).unwrap();
        assert_eq!(&lines[2][a..z], b"Alpha");
        assert!(skill_name(b"\t\tSkillName = Alpha").is_none());
    }
}
