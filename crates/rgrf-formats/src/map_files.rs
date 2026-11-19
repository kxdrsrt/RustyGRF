//! The files one map needs, for extracting it (GRF Editor's `MapExtractor`) and for checks.
//!
//! Starting from `data\<map>.rsw` the world names its ground (GND, with ground textures), its
//! altitude (GAT), its models (RSM, with their textures), its sounds and the water kind. Besides
//! what GRF Editor's map extractor collects (world, ground and its textures, altitude, models and
//! their textures, the effect script `effecttool\<map>.lub` and its textures) this also lists the
//! sounds, the pictures of the water cycle and the minimap.
//!
//! Paths are archive bytes (CP949 for Korean names); lookups go through [`MapSource`], so the
//! caller decides what "exists" means (one archive, a client with several, loose files).

use std::collections::BTreeSet;

use crate::gnd::GndSummary;
use crate::rsm::Rsm;
use crate::rsw::{Object, Rsw};

/// CP949 bytes of `워터`, the folder of the water pictures below `data\texture\`.
const WATER_DIR: &[u8] = b"data\\texture\\\xbf\xf6\xc5\xcd\\";
/// CP949 bytes of `유저인터페이스\map\`, the folder of the minimaps below `data\texture\`.
const MINIMAP_DIR: &[u8] =
    b"data\\texture\\\xc0\xaf\xc0\xfa\xc0\xce\xc5\xcd\xc6\xe4\xc0\xcc\xbd\xba\\map\\";
const EFFECT_SCRIPT_DIR: &[u8] = b"data\\luafiles514\\lua files\\effecttool\\";

/// How an archive (or a client) answers for a path, compared like the client does.
pub trait MapSource {
    /// The decoded content, if the path exists and can be read.
    fn read(&self, path: &[u8]) -> Option<Vec<u8>>;
    fn exists(&self, path: &[u8]) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    World,
    Ground,
    Altitude,
    GroundTexture,
    Model,
    ModelTexture,
    Sound,
    Water,
    Minimap,
    EffectScript,
    EffectTexture,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::Ground => "ground",
            Self::Altitude => "altitude",
            Self::GroundTexture => "ground texture",
            Self::Model => "model",
            Self::ModelTexture => "model texture",
            Self::Sound => "sound",
            Self::Water => "water",
            Self::Minimap => "minimap",
            Self::EffectScript => "effect script",
            Self::EffectTexture => "effect texture",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapFile {
    /// Full archive path (`data\...`).
    pub path: Vec<u8>,
    pub kind: Kind,
    pub found: bool,
    /// The file that names this one (`None` for the map's own files).
    pub named_by: Option<Vec<u8>>,
    /// Set when the file exists but could not be read to follow what it names.
    pub problem: Option<String>,
}

/// Path comparison like the client: ASCII case-insensitive, `/` equals `\`; the second byte of a
/// double-byte character stays as it is.
fn fold(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] >= 0x81
            && i + 1 < b.len()
            && matches!(b[i + 1], 0x41..=0x5A | 0x61..=0x7A | 0x81..=0xFE)
        {
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

fn cstr(raw: &[u8]) -> &[u8] {
    &raw[..raw.iter().position(|&b| b == 0).unwrap_or(raw.len())]
}

fn join(prefix: &[u8], name: &[u8]) -> Vec<u8> {
    let name = cstr(name);
    let name = name
        .strip_prefix(b"\\")
        .or_else(|| name.strip_prefix(b"/"))
        .unwrap_or(name);
    [prefix, name].concat()
}

/// The map's base name from `prontera`, `prontera.rsw` or `data\prontera.rsw`.
pub fn map_base(map: &str) -> &str {
    let base = map.rsplit(['\\', '/']).next().unwrap_or(map);
    let lower = base.to_ascii_lowercase();
    if lower.ends_with(".rsw") || lower.ends_with(".gat") || lower.ends_with(".gnd") {
        &base[..base.len() - 4]
    } else {
        base
    }
}

struct Collect<'a, S: MapSource> {
    src: &'a S,
    out: Vec<MapFile>,
    seen: BTreeSet<Vec<u8>>,
}

impl<S: MapSource> Collect<'_, S> {
    /// Adds `path` once; returns whether it was new and exists.
    fn add(&mut self, path: Vec<u8>, kind: Kind, named_by: Option<&[u8]>) -> bool {
        // an empty name in a list (seen: sound objects without a file) leaves only the folder
        if path.ends_with(b"\\") || path.ends_with(b"/") || !self.seen.insert(fold(&path)) {
            return false;
        }
        let found = self.src.exists(&path);
        self.out.push(MapFile {
            path,
            kind,
            found,
            named_by: named_by.map(<[u8]>::to_vec),
            problem: None,
        });
        found
    }

    fn problem(&mut self, path: &[u8], msg: String) {
        let key = fold(path);
        if let Some(f) = self.out.iter_mut().find(|f| fold(&f.path) == key) {
            f.problem = Some(msg);
        }
    }
}

/// Every file `map` needs, the map's own files first. Errs only when the world file is missing.
pub fn resolve(map: &str, src: &impl MapSource) -> Result<Vec<MapFile>, String> {
    let base = map_base(map).as_bytes().to_vec();
    let rsw_path = [b"data\\".as_slice(), &base, b".rsw"].concat();
    let mut c = Collect {
        src,
        out: Vec::new(),
        seen: BTreeSet::new(),
    };
    if !c.add(rsw_path.clone(), Kind::World, None) {
        return Err(format!(
            "{} is not in the archive",
            String::from_utf8_lossy(&rsw_path)
        ));
    }
    let rsw = src
        .read(&rsw_path)
        .ok_or_else(|| "the world file cannot be read".to_string())
        .and_then(|d| Rsw::parse(&d).map_err(|e| e.to_string()));
    let rsw = match rsw {
        Ok(r) => Some(r),
        Err(e) => {
            c.problem(&rsw_path, format!("cannot be read: {e}"));
            None
        }
    };
    // ground and altitude: as the world names them, else after the map
    let named = |field: Option<&[u8; 40]>, ext: &[u8]| match field
        .map(|f| cstr(f))
        .filter(|f| !f.is_empty())
    {
        Some(f) => join(b"data\\", f),
        None => [b"data\\".as_slice(), &base, ext].concat(),
    };
    let gnd_path = named(rsw.as_ref().map(|r| &r.ground), b".gnd");
    let gat_path = named(rsw.as_ref().map(|r| &r.altitude), b".gat");
    if c.add(gnd_path.clone(), Kind::Ground, Some(&rsw_path)) {
        match src.read(&gnd_path).map(|d| GndSummary::parse(&d)) {
            Some(Ok(g)) => {
                for t in &g.textures {
                    c.add(
                        join(b"data\\texture\\", t),
                        Kind::GroundTexture,
                        Some(&gnd_path),
                    );
                }
            }
            Some(Err(e)) => c.problem(&gnd_path, format!("cannot be read: {e}")),
            None => c.problem(&gnd_path, "cannot be read".into()),
        }
    }
    c.add(gat_path, Kind::Altitude, Some(&rsw_path));

    if let Some(r) = &rsw {
        let mut models = Vec::new();
        for o in &r.objects {
            match o {
                Object::Model(m) => {
                    let p = join(b"data\\model\\", &m.model_name);
                    if c.add(p.clone(), Kind::Model, Some(&rsw_path)) {
                        models.push(p);
                    }
                }
                Object::Sound(s) => {
                    c.add(
                        join(b"data\\wav\\", &s.wave_name),
                        Kind::Sound,
                        Some(&rsw_path),
                    );
                }
                _ => {}
            }
        }
        for m in models {
            match src.read(&m).map(|d| Rsm::parse(&d)) {
                Some(Ok(rsm)) => {
                    for t in rsm.all_textures() {
                        c.add(join(b"data\\texture\\", &t), Kind::ModelTexture, Some(&m));
                    }
                }
                Some(Err(e)) => c.problem(&m, format!("cannot be read: {e}")),
                None => c.problem(&m, "cannot be read".into()),
            }
        }
        // the water cycle: water<kind><00..31>.jpg, as far as the pictures go
        if let Some(w) = &r.water {
            let kind = w.kind.clamp(0, 9);
            for n in 0..32 {
                let p = [WATER_DIR, format!("water{kind}{n:02}.jpg").as_bytes()].concat();
                if !src.exists(&p) {
                    break;
                }
                c.add(p, Kind::Water, Some(&rsw_path));
            }
        }
    }

    let minimap = [MINIMAP_DIR, &base, b".bmp"].concat();
    if src.exists(&minimap) {
        c.add(minimap, Kind::Minimap, None);
    }
    let script = [EFFECT_SCRIPT_DIR, &base, b".lub"].concat();
    if src.exists(&script) {
        c.add(script.clone(), Kind::EffectScript, None);
        match src.read(&script).map(|d| effect_textures(&d)) {
            Some(Ok(textures)) => {
                for t in textures {
                    c.add(
                        join(b"data\\texture\\", &t),
                        Kind::EffectTexture,
                        Some(&script),
                    );
                }
            }
            Some(Err(e)) => c.problem(&script, format!("cannot be read: {e}")),
            None => c.problem(&script, "cannot be read".into()),
        }
    }
    Ok(c.out)
}

/// The `texture = "..."` values of an effect script (compiled or text), as bytes.
pub fn effect_textures(data: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let text: Vec<u8> = if data.starts_with(b"\x1bLua") {
        let chunk = crate::luac::parse_any(data).map_err(|e| e.to_string())?;
        // one char per byte, so the strings come back as the bytes they are
        let latin1 = |b: &[u8]| b.iter().map(|&c| char::from(c)).collect::<String>();
        crate::luadec::decompile(&chunk, &latin1)
            .chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
            .collect()
    } else {
        data.to_vec()
    };
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut i = 0;
    while let Some(at) = find(&text[i..], b"texture") {
        let mut j = i + at + b"texture".len();
        i = j;
        // `texture` as a whole word, then `=` and a quoted string
        let before = (i - b"texture".len()).checked_sub(1).map(|k| text[k]);
        if before.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_') {
            continue;
        }
        while text.get(j).is_some_and(|b| b.is_ascii_whitespace()) {
            j += 1;
        }
        if text.get(j) != Some(&b'=') {
            continue;
        }
        j += 1;
        while text.get(j).is_some_and(|b| b.is_ascii_whitespace()) {
            j += 1;
        }
        let Some(&quote) = text.get(j).filter(|&&q| q == b'"' || q == b'\'') else {
            continue;
        };
        j += 1;
        let mut s = Vec::new();
        while let Some(&b) = text.get(j) {
            j += 1;
            match b {
                b'\\' => {
                    let Some(&e) = text.get(j) else { break };
                    j += 1;
                    match e {
                        b'n' => s.push(b'\n'),
                        b't' => s.push(b'\t'),
                        b'0'..=b'9' => {
                            let mut v = u32::from(e - b'0');
                            for _ in 0..2 {
                                match text.get(j) {
                                    Some(&d) if d.is_ascii_digit() => {
                                        v = v * 10 + u32::from(d - b'0');
                                        j += 1;
                                    }
                                    _ => break,
                                }
                            }
                            s.push(u8::try_from(v).unwrap_or(b'?'));
                        }
                        other => s.push(other),
                    }
                }
                b if b == quote => break,
                b => s.push(b),
            }
        }
        i = j;
        if !s.is_empty() && seen.insert(fold(&s)) {
            out.push(s);
        }
    }
    Ok(out)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Mem(HashMap<Vec<u8>, Vec<u8>>);

    impl MapSource for Mem {
        fn read(&self, path: &[u8]) -> Option<Vec<u8>> {
            self.0.get(&fold(path)).cloned()
        }
        fn exists(&self, path: &[u8]) -> bool {
            self.0.contains_key(&fold(path))
        }
    }

    #[test]
    fn empty_names_in_lists_are_not_files() {
        let mut c = Collect {
            src: &Mem(HashMap::new()),
            out: Vec::new(),
            seen: BTreeSet::new(),
        };
        assert!(!c.add(join(b"data\\wav\\", &[0u8; 80]), Kind::Sound, None));
        assert!(c.out.is_empty());
        c.add(join(b"data\\wav\\", b"a.wav\0\0"), Kind::Sound, None);
        assert_eq!(c.out[0].path, b"data\\wav\\a.wav");
    }

    #[test]
    fn map_names_are_taken_in_every_usual_form() {
        for m in [
            "prontera",
            "prontera.rsw",
            "data\\prontera.RSW",
            "data/prontera.gat",
        ] {
            assert_eq!(map_base(m), "prontera", "{m}");
        }
    }

    #[test]
    fn effect_script_textures_are_read_with_escapes_and_once() {
        let text = br#"
            Emitter_emitterInfo = {
              [1] = { texture = "effect\\smoke1.bmp", size = 3 },
              [2] = { texture = 'effect\\smoke1.BMP' },
              [3] = { mytexture = "not this.bmp", texture="\199\209.tga" },
            }"#;
        assert_eq!(
            effect_textures(text).unwrap(),
            vec![b"effect\\smoke1.bmp".to_vec(), b"\xc7\xd1.tga".to_vec()]
        );
    }

    #[test]
    fn a_missing_world_is_an_error_and_missing_parts_are_listed() {
        let src = Mem(HashMap::new());
        assert!(resolve("nowhere", &src).is_err());
        // a world file that cannot be parsed: listed with its problem, ground and altitude
        // after the map name, both missing
        let mut m = HashMap::new();
        m.insert(fold(b"data\\x.rsw"), b"GRSW broken".to_vec());
        let files = resolve("x", &Mem(m)).unwrap();
        let kinds: Vec<_> = files.iter().map(|f| (f.kind, f.found)).collect();
        assert_eq!(
            kinds,
            [
                (Kind::World, true),
                (Kind::Ground, false),
                (Kind::Altitude, false)
            ]
        );
        assert!(files[0].problem.is_some());
        assert_eq!(files[1].path, b"data\\x.gnd");
    }
}
