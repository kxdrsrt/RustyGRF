//! What animations (.act) and effects (.str) name, checked against the files that exist: the
//! sprite next to an animation, the images its layers use, the sounds its frames play and the
//! textures of an effect. Only names inside binary files are read, whose folder is fixed;
//! names in Lua tables are not (each table has its own base folder).

use crate::act::{Act, V2_1};
use crate::map_files::MapSource;
use crate::spr::Spr;
use crate::str_effect;

/// One thing wrong with a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// A short stable word: `unreadable_act`, `unreadable_sprite`, `missing_sprite`,
    /// `image_out_of_range`, `sound_out_of_range`, `missing_sound`, `unreadable_effect`,
    /// `missing_effect_texture`.
    pub kind: &'static str,
    /// The archive path the problem is about (the missing file), empty when there is none.
    pub path: Vec<u8>,
    /// Further words (what was expected).
    pub note: String,
}

fn problem(kind: &'static str, path: Vec<u8>, note: impl Into<String>) -> Problem {
    Problem {
        kind,
        path,
        note: note.into(),
    }
}

/// Checks the animation at archive path `path` with content `data`.
pub fn check_act(src: &dyn MapSource, path: &[u8], data: &[u8]) -> Vec<Problem> {
    let mut out = Vec::new();
    let act = match Act::parse(data) {
        Ok(a) => a,
        Err(e) => return vec![problem("unreadable_act", Vec::new(), e.to_string())],
    };
    // the sprite next to it
    let Some(dot) = path.iter().rposition(|&b| b == b'.') else {
        return out;
    };
    let mut spr_path = path[..=dot].to_vec();
    spr_path.extend(b"spr");
    let mut counts = None;
    match src.read(&spr_path) {
        None if !src.exists(&spr_path) => out.push(problem("missing_sprite", spr_path, "")),
        None => {} // a loose file: it exists, but there is nothing to read it from
        Some(d) => match Spr::parse(&d) {
            Ok(s) => counts = Some((s.indexed.len() as i64, s.rgba.len() as i64)),
            Err(e) => out.push(problem("unreadable_sprite", spr_path, e.to_string())),
        },
    }
    // images the layers ask for
    if let Some((indexed, rgba)) = counts {
        let mut worst: std::collections::BTreeMap<i32, i64> = Default::default();
        for (_, _, _, l) in act.layers() {
            if l.sprite_index < 0 {
                continue;
            }
            let have = if l.sprite_type == 0 { indexed } else { rgba };
            if i64::from(l.sprite_index) >= have {
                let w = worst.entry(l.sprite_type).or_default();
                *w = (*w).max(i64::from(l.sprite_index));
            }
        }
        for (ty, idx) in worst {
            let (kind, have) = if ty == 0 {
                ("palette", indexed)
            } else {
                ("true-colour", rgba)
            };
            out.push(problem(
                "image_out_of_range",
                Vec::new(),
                format!("a layer uses {kind} image {idx}, the sprite has {have}"),
            ));
        }
    }
    // sounds played by a frame ("atk" marks the frame that hits and is no file); before ACT 2.1
    // the field is not a sound number
    if act.version < V2_1 {
        return out;
    }
    let mut seen = std::collections::BTreeSet::new();
    for f in act.actions.iter().flat_map(|a| &a.frames) {
        let Ok(id) = usize::try_from(f.sound_id) else {
            continue;
        };
        let Some(raw) = act.sounds.get(id) else {
            out.push(problem(
                "sound_out_of_range",
                Vec::new(),
                format!(
                    "a frame plays sound {id}, the table has {}",
                    act.sounds.len()
                ),
            ));
            continue;
        };
        // the name as the bytes of the file, not as text: Korean names are not Latin-1
        let name: Vec<u8> = raw.iter().copied().take_while(|&b| b != 0).collect();
        if name.is_empty() || name.eq_ignore_ascii_case(b"atk") || !seen.insert(name.clone()) {
            continue;
        }
        let mut p = b"data\\wav\\".to_vec();
        p.extend(&name);
        if !src.exists(&p) {
            out.push(problem("missing_sound", p, ""));
        }
    }
    out
}

/// Checks the effect at archive path `path` with content `data`: every texture must be next to
/// it or in `data\texture\effect\`.
pub fn check_str(src: &dyn MapSource, path: &[u8], data: &[u8]) -> Vec<Problem> {
    let s = match str_effect::parse(data) {
        Ok(s) => s,
        Err(e) => return vec![problem("unreadable_effect", Vec::new(), e.to_string())],
    };
    let dir = path
        .iter()
        .rposition(|&b| b == b'\\')
        .map_or(&path[..0], |p| &path[..=p]);
    let mut out = Vec::new();
    for name in str_effect::texture_names(&s) {
        let mut near = dir.to_vec();
        near.extend(&name);
        let mut shared = b"data\\texture\\effect\\".to_vec();
        shared.extend(&name);
        if !src.exists(&near) && !src.exists(&shared) {
            out.push(problem("missing_effect_texture", shared, ""));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::act::{Action, Frame, Layer, V2_5, sound_from_name};
    use crate::spr::{IndexedImage, PALETTE_LEN, Version};
    use std::collections::HashMap;

    struct Files(HashMap<Vec<u8>, Vec<u8>>);
    impl MapSource for Files {
        fn read(&self, path: &[u8]) -> Option<Vec<u8>> {
            self.0.get(&path.to_ascii_lowercase()).cloned()
        }
        fn exists(&self, path: &[u8]) -> bool {
            self.0.contains_key(&path.to_ascii_lowercase())
        }
    }

    fn act(image: i32, sounds: &[&str], plays: &[i32], version: crate::Version) -> Vec<u8> {
        let frame = |sound_id: i32| Frame {
            layers: vec![Layer {
                sprite_index: image,
                ..Layer::default()
            }],
            sound_id,
            ..Frame::default()
        };
        Act {
            version,
            reserved: [0; 10],
            actions: vec![Action {
                frames: plays.iter().map(|&p| frame(p)).collect(),
                interval: 4.0,
            }],
            sounds: sounds.iter().map(|n| sound_from_name(n).unwrap()).collect(),
            trailing: vec![],
            warnings: vec![],
        }
        .to_bytes(version)
        .unwrap()
    }

    fn sprite() -> Vec<u8> {
        Spr {
            version: Version::V2_1,
            indexed: vec![IndexedImage {
                width: 2,
                height: 2,
                pixels: vec![1; 4],
            }],
            rgba: vec![],
            palette: Some(Box::new([0u8; PALETTE_LEN])),
            warnings: vec![],
        }
        .to_bytes(Version::V2_1)
        .unwrap()
    }

    #[test]
    fn missing_pieces_of_an_animation_are_named() {
        let mut f = HashMap::new();
        f.insert(b"data\\a.spr".to_vec(), sprite());
        f.insert(b"data\\wav\\here.wav".to_vec(), b"RIFF".to_vec());
        let src = Files(f);
        let ok = act(0, &[], &[-1], V2_5);
        assert!(check_act(&src, b"data\\a.act", &ok).is_empty());
        let kinds = |data: &[u8], path: &[u8]| -> Vec<&'static str> {
            check_act(&src, path, data).iter().map(|p| p.kind).collect()
        };
        assert_eq!(kinds(&ok, b"data\\b.act"), ["missing_sprite"]);
        assert_eq!(
            kinds(&act(3, &[], &[-1], V2_5), b"data\\a.act"),
            ["image_out_of_range"]
        );
        // sound 0 has no file, 1 is the hit marker, 2 is there, 5 is outside the table
        let sounds = act(0, &["gone.wav", "atk", "here.wav"], &[0, 1, 2, 5], V2_5);
        let found = check_act(&src, b"data\\a.act", &sounds);
        assert_eq!(
            found.iter().map(|p| p.kind).collect::<Vec<_>>(),
            ["missing_sound", "sound_out_of_range"]
        );
        assert_eq!(found[0].path, b"data\\wav\\gone.wav");
        assert_eq!(kinds(b"not an act", b"data\\a.act"), ["unreadable_act"]);
    }

    #[test]
    fn broken_effects_are_named_not_panicked_on() {
        let src = Files(HashMap::new());
        let p = check_str(&src, b"data\\texture\\effect\\x.str", b"nope");
        assert_eq!(p[0].kind, "unreadable_effect");
    }
}
