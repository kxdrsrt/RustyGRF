//! Thor Patcher executables with packed configuration files (GrfCL's `thorPack` and the reverse):
//! the files follow the program, each zlib compressed, then a compressed table. Ported from GRF
//! Editor's `ThorPacker` (with the permission of its author, see `docs/05-licensing.md`).
//!
//! ```text
//! [program][count u32][table length u32][table offset u32][file data ...][table][length u32]
//! table (before zlib): per file [name length u8][name][compressed size u32][size u32][offset u32]
//! ```
//!
//! Offsets count from the end of the program plus 12 (the start of the file data); the table
//! offset is from the start of the executable; the last number is the packed part's length minus 4.

use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PackError {
    #[error("'{0}' cannot be written in the Windows-1252 encoding of the patcher")]
    Name(String),
    #[error("the name '{0}' is longer than 255 bytes")]
    NameLength(String),
    #[error("this is not a Thor Patcher with packed files")]
    NotPacked,
    #[error("the packed files are damaged")]
    Damaged,
}

type Result<T> = std::result::Result<T, PackError>;

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
    z.write_all(data).expect("writing to memory");
    z.finish().expect("writing to memory")
}

/// Packs `files` (name as the patcher finds it, content) behind `program`.
pub fn pack(program: &[u8], files: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    let mut table = Vec::new();
    for (name, content) in files {
        let (bytes, _, lost) = encoding_rs::WINDOWS_1252.encode(name);
        if lost {
            return Err(PackError::Name(name.clone()));
        }
        let len = u8::try_from(bytes.len()).map_err(|_| PackError::NameLength(name.clone()))?;
        let packed = zlib(content);
        table.push(len);
        table.extend_from_slice(&bytes);
        table.extend((packed.len() as u32).to_le_bytes());
        table.extend((content.len() as u32).to_le_bytes());
        table.extend((data.len() as u32 + 12).to_le_bytes());
        data.extend(packed);
    }
    let table = zlib(&table);
    let table_offset = program.len() + data.len() + 12;
    let mut out = program.to_vec();
    out.extend((files.len() as u32).to_le_bytes());
    out.extend((table.len() as u32).to_le_bytes());
    out.extend((table_offset as u32).to_le_bytes());
    out.extend(data);
    out.extend(table);
    let tail = (out.len() + 4 - program.len() - 4) as u32;
    out.extend(tail.to_le_bytes());
    Ok(out)
}

fn u32_at(b: &[u8], at: usize) -> Result<usize> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
        .ok_or(PackError::Damaged)
}

/// Name and content of a packed file.
pub type PackedFile = (String, Vec<u8>);

/// The files packed into a Thor Patcher, with the length of the program in front of them.
pub fn unpack(exe: &[u8]) -> Result<(usize, Vec<PackedFile>)> {
    let tail = u32_at(exe, exe.len().wrapping_sub(4)).map_err(|_| PackError::NotPacked)?;
    let start = exe
        .len()
        .checked_sub(tail)
        .and_then(|n| n.checked_sub(4))
        .ok_or(PackError::NotPacked)?;
    let count = u32_at(exe, start).map_err(|_| PackError::NotPacked)?;
    let table_offset = u32_at(exe, start + 8).map_err(|_| PackError::NotPacked)?;
    if table_offset < start + 12 || table_offset > exe.len() - 4 || count > 1_000_000 {
        return Err(PackError::NotPacked);
    }
    let mut table = Vec::new();
    ZlibDecoder::new(&exe[table_offset..exe.len() - 4])
        .read_to_end(&mut table)
        .map_err(|_| PackError::NotPacked)?;
    let mut files = Vec::with_capacity(count);
    let mut at = 0;
    for _ in 0..count {
        let len = usize::from(*table.get(at).ok_or(PackError::Damaged)?);
        let name = table.get(at + 1..at + 1 + len).ok_or(PackError::Damaged)?;
        let (packed, size, offset) = (
            u32_at(&table, at + 1 + len)?,
            u32_at(&table, at + 5 + len)?,
            u32_at(&table, at + 9 + len)?,
        );
        at += 13 + len;
        let from = start + offset;
        let stream = exe.get(from..from + packed).ok_or(PackError::Damaged)?;
        let mut content = Vec::with_capacity(size);
        ZlibDecoder::new(stream)
            .read_to_end(&mut content)
            .map_err(|_| PackError::Damaged)?;
        if content.len() != size {
            return Err(PackError::Damaged);
        }
        let (name, _, _) = encoding_rs::WINDOWS_1252.decode(name);
        files.push((name.into_owned(), content));
    }
    Ok((start, files))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_packed_behind_a_program_come_back_and_the_program_is_kept() {
        let program = b"MZ pretend this is the patcher".to_vec();
        let files = vec![
            ("config".to_string(), b"[CONFIG]\nPatchServer=x\n".to_vec()),
            ("LanguageMap.ini".to_string(), vec![]),
            (
                "images/b\u{e9}.png".to_string(),
                (0..5000u32).map(|i| i as u8).collect(),
            ),
        ];
        let exe = pack(&program, &files).unwrap();
        assert_eq!(&exe[..program.len()], &program[..]);
        let (start, back) = unpack(&exe).unwrap();
        assert_eq!((start, back), (program.len(), files));
        // layout: count, table length, table offset right behind the program
        assert_eq!(u32_at(&exe, program.len()).unwrap(), 3);
        assert_eq!(
            u32_at(&exe, exe.len() - 4).unwrap(),
            exe.len() - program.len() - 4
        );
        // not packed, damaged, and names the patcher cannot read
        assert_eq!(unpack(b"just a program").unwrap_err(), PackError::NotPacked);
        let mut bad = exe.clone();
        let n = bad.len();
        bad[n - 30] ^= 0xff;
        assert!(unpack(&bad).is_err());
        assert!(matches!(
            pack(&program, &[("\u{4e2d}".into(), vec![])]),
            Err(PackError::Name(_))
        ));
        assert!(matches!(
            pack(&program, &[("x".repeat(300), vec![])]),
            Err(PackError::NameLength(_))
        ));
    }
}
