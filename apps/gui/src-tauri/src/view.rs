//! How the entry list is arranged: filter, sort and a folder tree over the matches. Pure
//! functions over display names (`\` separated), so they are testable without an archive.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;

/// What one list shows. `layout` is `flat`, `tree` (folders and files) or `folders` (only folders).
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ViewSpec {
    pub query: String,
    /// Kinds of file to keep (`image`, `sprite`, ...); empty keeps all.
    pub kinds: Vec<String>,
    /// Smallest and largest unpacked size in bytes (0 = no limit).
    pub min_size: u64,
    pub max_size: u64,
    /// `archive` (stored order), `name`, `path`, `type`, `size`, `stored` or `offset`.
    pub sort: String,
    pub desc: bool,
    pub layout: String,
    /// Flat layout only: just the files directly in this folder (or below it with `recursive`).
    pub folder: Option<String>,
    pub recursive: bool,
    /// Tree layouts: open every folder when the result is small enough to read.
    pub auto_expand: bool,
}

/// Facts about an entry the arrangement needs.
#[derive(Clone, Copy, Debug)]
pub struct Meta {
    pub size: u32,
    pub stored: u32,
    pub offset: u64,
}

/// Results with at most this many files open all their folders on their own.
pub const AUTO_EXPAND_LIMIT: usize = 3000;

pub fn base_name(name: &str) -> &str {
    name.rsplit('\\').next().unwrap_or(name)
}

pub fn dir_of(name: &str) -> &str {
    name.rsplit_once('\\').map_or("", |(d, _)| d)
}

fn extension(name: &str) -> String {
    let b = base_name(name);
    b.rsplit_once('.')
        .map_or(String::new(), |(_, e)| e.to_ascii_lowercase())
}

/// Orders `idx` by `key`; ties keep the stored order, so the result is stable and repeatable.
pub fn sort_indices(
    idx: &mut [usize],
    names: &[String],
    meta: &dyn Fn(usize) -> Meta,
    key: &str,
    desc: bool,
) {
    match key {
        "name" => idx.sort_by_cached_key(|&i| (base_name(&names[i]).to_lowercase(), i)),
        "path" => idx.sort_by_cached_key(|&i| (names[i].to_lowercase(), i)),
        "type" => idx.sort_by_cached_key(|&i| {
            (extension(&names[i]), base_name(&names[i]).to_lowercase(), i)
        }),
        "size" => idx.sort_by_key(|&i| (meta(i).size, i)),
        "stored" => idx.sort_by_key(|&i| (meta(i).stored, i)),
        "offset" => idx.sort_by_key(|&i| (meta(i).offset, i)),
        _ => idx.sort_unstable(),
    }
    if desc {
        idx.reverse();
    }
}

/// Keeps the entries of the wanted kinds and sizes.
pub fn filter_indices(
    idx: &mut Vec<usize>,
    names: &[String],
    meta: &dyn Fn(usize) -> Meta,
    kind_of: &dyn Fn(&str) -> &'static str,
    spec: &ViewSpec,
) {
    if spec.kinds.is_empty() && spec.min_size == 0 && spec.max_size == 0 {
        return;
    }
    idx.retain(|&i| {
        let size = u64::from(meta(i).size);
        (spec.kinds.is_empty() || spec.kinds.iter().any(|k| k == kind_of(&names[i])))
            && size >= spec.min_size
            && (spec.max_size == 0 || size <= spec.max_size)
    });
}

/// Keeps the entries in `folder` (case-insensitive), with its subfolders when `recursive`.
pub fn in_folder(idx: &mut Vec<usize>, names: &[String], folder: &str, recursive: bool) {
    let want = folder.trim_matches('\\').to_lowercase();
    idx.retain(|&i| {
        let d = dir_of(&names[i]).to_lowercase();
        if recursive {
            want.is_empty() || d == want || d.starts_with(&format!("{want}\\"))
        } else {
            d == want
        }
    });
}

#[derive(Debug)]
pub struct Node {
    pub name: String,
    pub path: String,
    pub key: String,
    pub folders: Vec<usize>,
    /// Entry indices in the order of the sort.
    pub files: Vec<usize>,
    /// Files and bytes in this folder and below.
    pub total_files: usize,
    pub total_bytes: u64,
}

#[derive(Debug)]
pub struct Tree {
    /// `nodes[0]` is the root (no name); every other node is a folder.
    pub nodes: Vec<Node>,
    by_key: HashMap<String, usize>,
}

impl Tree {
    /// A tree over `idx` (files stay in the order of `idx`); folders are ordered by name, or by
    /// the bytes below them with `by_size`, and reversed with `desc`.
    pub fn build(
        names: &[String],
        idx: &[usize],
        meta: &dyn Fn(usize) -> Meta,
        by_size: bool,
        desc: bool,
    ) -> Self {
        let mut t = Tree {
            nodes: vec![Node {
                name: String::new(),
                path: String::new(),
                key: String::new(),
                folders: Vec::new(),
                files: Vec::new(),
                total_files: 0,
                total_bytes: 0,
            }],
            by_key: HashMap::new(),
        };
        for &i in idx {
            let dir = dir_of(&names[i]);
            let node = t.folder_node(dir);
            t.nodes[node].files.push(i);
        }
        // children are created after their parents, so one backwards pass adds up the totals
        for n in (0..t.nodes.len()).rev() {
            let own_files = t.nodes[n].files.len();
            let own_bytes: u64 = t.nodes[n]
                .files
                .iter()
                .map(|&i| u64::from(meta(i).size))
                .sum();
            let (mut files, mut bytes) = (own_files, own_bytes);
            for &c in &t.nodes[n].folders {
                files += t.nodes[c].total_files;
                bytes += t.nodes[c].total_bytes;
            }
            t.nodes[n].total_files = files;
            t.nodes[n].total_bytes = bytes;
        }
        for n in 0..t.nodes.len() {
            let mut kids = std::mem::take(&mut t.nodes[n].folders);
            if by_size {
                kids.sort_by_key(|&c| t.nodes[c].total_bytes);
            } else {
                kids.sort_by_cached_key(|&c| t.nodes[c].name.to_lowercase());
            }
            if desc {
                kids.reverse();
            }
            t.nodes[n].folders = kids;
        }
        t
    }

    /// The node of a folder path, made with its parents when new.
    fn folder_node(&mut self, dir: &str) -> usize {
        if dir.is_empty() {
            return 0;
        }
        let key = dir.to_lowercase();
        if let Some(&n) = self.by_key.get(&key) {
            return n;
        }
        let (parent, name) = match dir.rsplit_once('\\') {
            Some((p, n)) => (self.folder_node(p), n),
            None => (0, dir),
        };
        let id = self.nodes.len();
        self.nodes.push(Node {
            name: name.to_string(),
            path: dir.to_string(),
            key: key.clone(),
            folders: Vec::new(),
            files: Vec::new(),
            total_files: 0,
            total_bytes: 0,
        });
        self.nodes[parent].folders.push(id);
        self.by_key.insert(key, id);
        id
    }

    pub fn find(&self, path: &str) -> Option<usize> {
        let key = path.trim_matches('\\').to_lowercase();
        if key.is_empty() {
            Some(0)
        } else {
            self.by_key.get(&key).copied()
        }
    }

    /// Entry indices below the folder, in tree order.
    pub fn files_below(&self, node: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            out.extend(&self.nodes[n].files);
            stack.extend(self.nodes[n].folders.iter().rev());
        }
        out
    }

    /// Every folder key, for "expand all".
    pub fn all_keys(&self) -> HashSet<String> {
        self.nodes.iter().skip(1).map(|n| n.key.clone()).collect()
    }

    /// The folders opened when an archive is shown, as GRF Editor does: a single top folder
    /// (`data`) is open, and so is every single folder below it that holds nothing else.
    pub fn opening_keys(&self) -> HashSet<String> {
        let mut keys = HashSet::new();
        let mut n = 0;
        while let [only] = self.nodes[n].folders[..] {
            keys.insert(self.nodes[only].key.clone());
            n = only;
            if !self.nodes[n].files.is_empty() {
                break;
            }
        }
        keys
    }
}

/// One line of a tree: a folder (`node`) or a file (`entry`) at `depth`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    Folder { node: usize, depth: u16 },
    File { entry: usize, depth: u16 },
}

enum Todo {
    Folder(usize, u16),
    Files(usize, u16),
}

/// The lines to show with the given folders open: in each folder the subfolders first, then its
/// files. With `files` false only folders are listed.
pub fn visible(tree: &Tree, expanded: &HashSet<String>, files: bool) -> Vec<Line> {
    let mut out = Vec::new();
    // a work list, so that a deep tree cannot overflow the stack
    let mut todo = Vec::new();
    if files {
        todo.push(Todo::Files(0, 0));
    }
    todo.extend(
        tree.nodes[0]
            .folders
            .iter()
            .rev()
            .map(|&c| Todo::Folder(c, 0)),
    );
    while let Some(t) = todo.pop() {
        match t {
            Todo::Files(n, depth) => out.extend(
                tree.nodes[n]
                    .files
                    .iter()
                    .map(|&entry| Line::File { entry, depth }),
            ),
            Todo::Folder(n, depth) => {
                out.push(Line::Folder { node: n, depth });
                if expanded.contains(&tree.nodes[n].key) {
                    if files {
                        todo.push(Todo::Files(n, depth + 1));
                    }
                    todo.extend(
                        tree.nodes[n]
                            .folders
                            .iter()
                            .rev()
                            .map(|&c| Todo::Folder(c, depth + 1)),
                    );
                }
            }
        }
    }
    out
}

/// The folders above `path` (not the folder itself), as expanded-set keys.
pub fn ancestors(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let parts: Vec<&str> = path.split('\\').filter(|p| !p.is_empty()).collect();
    for p in parts.iter().take(parts.len().saturating_sub(1)) {
        if !cur.is_empty() {
            cur.push('\\');
        }
        cur.push_str(p);
        out.push(cur.to_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        [
            r"data\sprite\a\x.spr",
            r"data\sprite\a\y.act",
            r"data\sprite\b\z.spr",
            r"data\texture\t.bmp",
            r"Data\Texture\u.bmp",
            r"root.txt",
        ]
        .map(String::from)
        .to_vec()
    }

    fn meta(i: usize) -> Meta {
        Meta {
            size: [10, 500, 30, 4000, 20, 7][i],
            stored: [5, 250, 30, 100, 10, 7][i],
            offset: [60, 50, 40, 30, 20, 10][i],
        }
    }

    fn all() -> Vec<usize> {
        (0..6).collect()
    }

    #[test]
    fn the_single_top_folder_opens_and_single_folders_below_it() {
        let n: Vec<String> = [r"data\sprite\a\x.spr", r"data\sprite\b\z.spr"]
            .map(String::from)
            .to_vec();
        let t = Tree::build(&n, &[0, 1], &meta, false, false);
        // data holds only sprite, sprite holds two folders: both open, a and b stay closed
        let keys = t.opening_keys();
        assert_eq!(keys, HashSet::from([r"data".into(), r"data\sprite".into()]));
        // two top folders (data and Data are one, root.txt is a file at the top): data opens
        let t = Tree::build(&names(), &all(), &meta, false, false);
        assert!(t.opening_keys().contains("data"));
        let two: Vec<String> = [r"data\a.txt", r"other\b.txt"].map(String::from).to_vec();
        assert!(
            Tree::build(&two, &[0, 1], &meta, false, false)
                .opening_keys()
                .is_empty()
        );
    }

    #[test]
    fn folders_follow_a_sort_by_size() {
        // below data: sprite (10 + 500 + 30 bytes) and texture (4000 + 20)
        let kids = |by_size, desc| {
            let t = Tree::build(&names(), &all(), &meta, by_size, desc);
            let data = t.find("data").unwrap();
            t.nodes[data]
                .folders
                .iter()
                .map(|&c| t.nodes[c].name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(kids(false, false), ["sprite", "texture"]);
        assert_eq!(kids(true, true), ["texture", "sprite"]);
        assert_eq!(kids(true, false), ["sprite", "texture"]);
    }

    #[test]
    fn sorting_by_every_key_is_stable_and_reversible() {
        let n = names();
        let by = |key: &str, desc: bool| {
            let mut v = all();
            sort_indices(&mut v, &n, &meta, key, desc);
            v
        };
        assert_eq!(by("archive", false), [0, 1, 2, 3, 4, 5]);
        assert_eq!(by("size", false), [5, 0, 4, 2, 1, 3]);
        assert_eq!(by("size", true), [3, 1, 2, 4, 0, 5]);
        assert_eq!(by("stored", false), [0, 5, 4, 2, 3, 1]);
        assert_eq!(by("offset", false), [5, 4, 3, 2, 1, 0]);
        // by name ignores the folder and the case
        assert_eq!(by("name", false), [5, 3, 4, 0, 1, 2]);
        assert_eq!(by("type", false)[0], 1); // act sorts before bmp, spr, txt
        assert_eq!(by("path", false), [0, 1, 2, 3, 4, 5]); // case-insensitive: Data\Texture next to data\texture
    }

    #[test]
    fn filters_by_kind_size_and_folder() {
        let n = names();
        let kind = |s: &str| match s.rsplit('.').next().unwrap() {
            "spr" => "sprite",
            "act" => "animation",
            "bmp" => "image",
            _ => "file",
        };
        let spec = |kinds: &[&str], min, max| ViewSpec {
            kinds: kinds.iter().map(|s| s.to_string()).collect(),
            min_size: min,
            max_size: max,
            ..ViewSpec::default()
        };
        let run = |s: &ViewSpec| {
            let mut v = all();
            filter_indices(&mut v, &n, &meta, &kind, s);
            v
        };
        assert_eq!(run(&spec(&["image"], 0, 0)), [3, 4]);
        assert_eq!(run(&spec(&["sprite", "animation"], 0, 0)), [0, 1, 2]);
        assert_eq!(run(&spec(&[], 25, 600)), [1, 2]);
        assert_eq!(run(&spec(&[], 0, 0)).len(), 6);
        let mut v = all();
        in_folder(&mut v, &n, r"DATA\sprite\a", false);
        assert_eq!(v, [0, 1]);
        let mut v = all();
        in_folder(&mut v, &n, r"data\sprite", true);
        assert_eq!(v, [0, 1, 2]);
        let mut v = all();
        in_folder(&mut v, &n, r"data\sprite", false);
        assert!(v.is_empty());
        let mut v = all();
        in_folder(&mut v, &n, "", false);
        assert_eq!(v, [5], "the archive root holds root.txt");
    }

    #[test]
    fn the_tree_merges_cases_counts_and_opens_folders() {
        let n = names();
        let t = Tree::build(&n, &all(), &meta, false, false);
        // data (sprite (a, b), texture) -> folders; Data\Texture is the same folder as data\texture
        let data = t.find("data").unwrap();
        assert_eq!(t.nodes[data].total_files, 5);
        assert_eq!(t.nodes[data].total_bytes, 10 + 500 + 30 + 4000 + 20);
        let tex = t.find(r"DATA\TEXTURE").unwrap();
        assert_eq!(t.nodes[tex].files, [3, 4]);
        assert_eq!(t.nodes[0].files, [5]);
        assert_eq!(t.files_below(data).len(), 5);

        let none = HashSet::new();
        let closed = visible(&t, &none, true);
        assert_eq!(
            closed,
            [
                Line::Folder {
                    node: data,
                    depth: 0
                },
                Line::File { entry: 5, depth: 0 }
            ]
        );

        let mut open: HashSet<String> = HashSet::new();
        open.insert("data".into());
        open.insert("data\\sprite".into());
        open.insert("data\\sprite\\a".into());
        let lines = visible(&t, &open, true);
        let shape: Vec<String> = lines
            .iter()
            .map(|l| match *l {
                Line::Folder { node, depth } => format!("{depth}F:{}", t.nodes[node].name),
                Line::File { entry, depth } => format!("{depth}f:{entry}"),
            })
            .collect();
        assert_eq!(
            shape,
            [
                "0F:data",
                "1F:sprite",
                "2F:a",
                "3f:0",
                "3f:1",
                "2F:b",
                "1F:texture",
                "0f:5"
            ]
        );
        // folders only, everything open
        let only = visible(&t, &t.all_keys(), false);
        assert_eq!(only.len(), t.nodes.len() - 1);
        assert!(only.iter().all(|l| matches!(l, Line::Folder { .. })));
    }

    #[test]
    fn ancestors_name_the_folders_to_open() {
        assert_eq!(
            ancestors(r"Data\Sprite\a\x.spr"),
            ["data", "data\\sprite", "data\\sprite\\a"]
        );
        assert!(ancestors("root.txt").is_empty());
    }
}
