//! GrfCL's `addFakeClientinfo`: a file of definitions (`__definitions`) with a quick CRC32 of
//! files of archives, made from a text file (`def.txt`). Ported from GRF Editor's
//! `Crc32QuickHash.GenerateHashData` (with the permission of its author, see
//! `docs/05-licensing.md`).
//!
//! The text has blocks: a line `archive.grf?` and then one pattern per line (`*` and `?`, a
//! leading `-` removes what matched before) up to a line with `#end`. Lines starting with `//`
//! are comments.

use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;

use crate::hashlist::wildcard;
use crate::name::NameMode;
use crate::{Archive, DecodeStatus};

fn crc_table() -> [u32; 256] {
    std::array::from_fn(|i| {
        (0..8).fold(i as u32, |c, _| {
            if c & 1 == 1 {
                (c >> 1) ^ 0xedb8_8320
            } else {
                c >> 1
            }
        })
    })
}

fn crc_step(table: &[u32; 256], crc: u32, byte: u8) -> u32 {
    (crc >> 8) ^ table[((crc ^ u32::from(byte)) & 0xff) as usize]
}

/// CRC32 of the first and last 2048 bytes and the length when the data is longer than 4096 bytes.
pub fn crc32_quick(data: &[u8]) -> u32 {
    let table = crc_table();
    let mut crc = !0u32;
    if data.len() > 4096 {
        for &b in data[..2048].iter().chain(&data[data.len() - 2048..]) {
            crc = crc_step(&table, crc, b);
        }
        for i in 0..4 {
            crc = crc_step(&table, crc, (data.len() >> (i * 8)) as u8);
        }
    } else {
        for &b in data {
            crc = crc_step(&table, crc, b);
        }
    }
    !crc
}

/// .NET's `BinaryWriter.Write(string)`: the length of the UTF-8 bytes in 7-bit groups, then the bytes.
fn put_string(out: &mut Vec<u8>, s: &str) {
    let mut n = s.len();
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
    out.extend_from_slice(s.as_bytes());
}

/// The blocks of a definitions text: archive path and its patterns.
pub fn parse_definitions(text: &str) -> Vec<(String, Vec<String>)> {
    let mut blocks: Vec<(String, Vec<String>)> = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let line = line.trim_matches(' ');
        if line.starts_with("//") {
            continue;
        }
        let Some(name) = line.strip_suffix('?') else {
            continue;
        };
        let at = match blocks.iter().position(|(n, _)| n == name) {
            Some(i) => i,
            None => {
                blocks.push((name.to_string(), Vec::new()));
                blocks.len() - 1
            }
        };
        for line in lines.by_ref() {
            if line.starts_with("//") {
                continue;
            }
            blocks[at].1.push(line.to_string());
            if line.contains("#end") {
                break;
            }
        }
    }
    blocks
}

/// How many files of each archive were hashed.
pub type Report = Vec<(String, usize)>;

/// Builds the compressed definitions. `open` gives the archive of a path in the text. Returns the
/// bytes and, per archive, how many files were hashed.
pub fn generate(
    text: &str,
    open: &dyn Fn(&str) -> Result<Archive, String>,
) -> Result<(Vec<u8>, Report), String> {
    let blocks = parse_definitions(text);
    let mut out = vec![1, 0];
    out.extend((blocks.len() as i32).to_le_bytes());
    let mut report = Vec::new();
    for (path, patterns) in &blocks {
        let a = open(path)?;
        let names: Vec<(usize, String)> = a
            .entries()
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.is_marker())
            .map(|(i, e)| (i, e.name.display(NameMode::Cp1252)))
            .collect();
        let mut chosen: Vec<usize> = Vec::new();
        for pattern in patterns {
            let (remove, pat) = pattern
                .strip_prefix('-')
                .map_or((false, pattern.as_str()), |p| (true, p));
            if remove {
                chosen.retain(|&i| !wildcard(pat, &names[i].1));
            } else {
                chosen.extend((0..names.len()).filter(|&i| wildcard(pat, &names[i].1)));
            }
        }
        let mut seen = std::collections::HashSet::new();
        chosen.retain(|i| seen.insert(*i));
        put_string(&mut out, &format!("{path}\0"));
        out.extend((chosen.len() as i32).to_le_bytes());
        let mut done = 0;
        for &i in &chosen {
            let (entry, name) = &names[i];
            let Ok(d) = a.read(*entry) else { continue };
            if d.status != DecodeStatus::Ok {
                continue;
            }
            out.push(name.chars().count() as u8);
            put_string(&mut out, &format!("{name}\0"));
            out.extend(crc32_quick(&d.data).to_le_bytes());
            done += 1;
        }
        report.push((path.clone(), done));
    }
    let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
    z.write_all(&out).map_err(|e| e.to_string())?;
    Ok((z.finish().map_err(|e| e.to_string())?, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quick_crc_takes_both_ends_and_the_length_of_long_data() {
        assert_eq!(
            crc32_quick(b"123456789"),
            0xcbf4_3926,
            "short data: the plain CRC32"
        );
        let a: Vec<u8> = (0..9000u32).map(|i| (i * 7) as u8).collect();
        let mut b = a.clone();
        b[3000] ^= 1; // not in the first or last 2048 bytes
        assert_eq!(crc32_quick(&a), crc32_quick(&b));
        b[10] ^= 1;
        assert_ne!(crc32_quick(&a), crc32_quick(&b));
        assert_ne!(
            crc32_quick(&a),
            crc32_quick(&a[..8999]),
            "the length counts"
        );
    }

    #[test]
    fn definitions_have_blocks_with_patterns_and_comments() {
        let text = "// c\n data.grf? \n*.lub\n-*old*\n#end\nother.grf?\n// no\na?\n#end\n";
        let b = parse_definitions(text);
        // the line with the spaces is trimmed; the patterns include the line with #end
        assert_eq!(b.len(), 2);
        assert_eq!(
            b[0],
            (
                "data.grf".to_string(),
                vec!["*.lub".into(), "-*old*".into(), "#end".into()]
            )
        );
        assert_eq!(b[1].1, vec!["a?".to_string(), "#end".to_string()]);
    }

    #[test]
    fn strings_are_written_as_dotnet_does() {
        let mut v = Vec::new();
        put_string(&mut v, "ab\0");
        assert_eq!(v, [3, b'a', b'b', 0]);
        let mut long = Vec::new();
        put_string(&mut long, &"x".repeat(200));
        assert_eq!(&long[..2], &[0xc8, 0x01]);
    }
}
