//! RGZ patches (`.rgz`): a gzip stream that holds a plain list of records, `f` (file: name
//! length, name with NUL, data length, data), `d` (folder: length, name with NUL) and the end
//! mark `end\0` (read as an `e` record, then two stray bytes). The layout follows GRF
//! Editor's `Rgz`/`RgzEntry` (Tokeiburu, used with permission, see docs/05-licensing.md). Checked
//! against a file written by GRF Editor's `GrfCL` (opens here with the exact content of the source
//! folder) and `GrfCL -rgzAnalysis` reads the files written here, see docs/02-format-notes.md; an RGZ
//! from a real patch server was not available.
//!
//! An opened RGZ is an ordinary [`crate::Archive`] over the decompressed bytes in memory; its
//! entries are stored uncompressed (flags 0).

use std::io::{Read, Write};
use std::path::Path;

use flate2::Compression;
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;

use crate::container::{Container, Entry, HEADER_LEN, Header, Kind, TableInfo, flags};
use crate::error::{Error, Result};
use crate::name::GrfPath;

/// Largest decompressed RGZ that is opened (it is held in memory).
pub const MAX_DECOMPRESSED: u64 = 2 << 30;

pub fn is_gzip(head: &[u8]) -> bool {
    head.starts_with(&[0x1f, 0x8b])
}

/// Decompresses `gz` and lists its records. The returned buffer starts with 46 unused bytes so
/// that an entry's `offset` is its position in the decompressed data.
pub fn read_container(gz: impl Read) -> Result<(Container, Vec<u8>)> {
    let mut data = vec![0u8; HEADER_LEN as usize];
    MultiGzDecoder::new(gz)
        .take(MAX_DECOMPRESSED + 1)
        .read_to_end(&mut data)
        .map_err(|e| Error::CorruptTable(format!("RGZ does not decompress: {e}")))?;
    if (data.len() as u64) > MAX_DECOMPRESSED + HEADER_LEN {
        return Err(Error::TooLarge(format!(
            "RGZ is larger than {} MiB when decompressed",
            MAX_DECOMPRESSED >> 20
        )));
    }
    let corrupt = |m: &str| Error::CorruptTable(format!("RGZ: {m}"));
    let mut entries = Vec::new();
    let mut p = HEADER_LEN as usize;
    while p < data.len() {
        let kind = data[p];
        p += 1;
        match kind {
            b'f' => {
                let n = usize::from(*data.get(p).ok_or_else(|| corrupt("truncated"))?);
                let name = data
                    .get(p + 1..p + 1 + n)
                    .ok_or_else(|| corrupt("name outside the data"))?;
                p += 1 + n;
                let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
                let len = data
                    .get(p..p + 4)
                    .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .ok_or_else(|| corrupt("truncated"))?;
                let len = u32::try_from(len).map_err(|_| corrupt("negative length"))?;
                p += 4;
                if p as u64 + u64::from(len) > data.len() as u64 {
                    return Err(corrupt("file data runs past the end"));
                }
                entries.push(Entry {
                    name: GrfPath::new(&name[..end]),
                    zsize: len,
                    zsize_aligned: len,
                    size: len,
                    // an empty file must not look like a marker
                    flags: if len == 0 { flags::FILE } else { 0 },
                    offset: p as u64 - HEADER_LEN,
                });
                p += len as usize;
            }
            b'd' => {
                let n = usize::from(*data.get(p).ok_or_else(|| corrupt("truncated"))?);
                p += 1 + n;
                if p > data.len() {
                    return Err(corrupt("folder name outside the data"));
                }
            }
            // the end mark `end\0`
            b'e' => break,
            // a few stray bytes at the very end are tolerated, anything else is damage
            _ if data.len() - p < 8 => break,
            c => return Err(corrupt(&format!("unknown record type 0x{c:02x}"))),
        }
    }
    let total = data.len() as u64;
    let mut magic = [0u8; 16];
    magic[..3].copy_from_slice(b"RGZ");
    Ok((
        Container {
            header: Header {
                magic,
                key: [0; 14],
                table_offset: 0,
                seed: 0,
                entry_count: entries.len() as u32,
                version: 0,
                kind: Kind::Rgz,
                patch: None,
            },
            table: TableInfo {
                abs_offset: total,
                prefix_len: 0,
                zsize: 0,
                size: 0,
                key_block: 0,
            },
            entries,
            file_len: total,
        },
        data,
    ))
}

/// Writes an RGZ the way GRF Editor does (checked against a file `GrfCL` wrote): before every file
/// a folder record with the folder of that file, then the file record; the stream ends with
/// `end` and a NUL. Never over an existing file unless `overwrite`; the file appears only when
/// complete.
pub fn write(dst: &Path, files: &[(GrfPath, Vec<u8>)], level: u32, overwrite: bool) -> Result<()> {
    if dst.exists() && !overwrite {
        return Err(Error::TargetExists(dst.display().to_string()));
    }
    let mut raw = Vec::new();
    for (name, data) in files {
        let b = name.as_bytes();
        if b.is_empty() || b.len() > 254 {
            return Err(Error::InvalidInput(format!(
                "RGZ needs path names of 1 to 254 bytes, got {}",
                b.len()
            )));
        }
        if let Some(i) = b.iter().rposition(|&c| c == b'\\' || c == b'/') {
            raw.push(b'd');
            raw.push(i as u8 + 1);
            raw.extend(&b[..i]);
            raw.push(0);
        }
        let size = i32::try_from(data.len())
            .map_err(|_| Error::TooLarge("a file in an RGZ is limited to 2 GiB".into()))?;
        raw.push(b'f');
        raw.push(b.len() as u8 + 1);
        raw.extend(b);
        raw.push(0);
        raw.extend(size.to_le_bytes());
        raw.extend(data);
    }
    raw.extend(b"end\0");
    let tmp = crate::pipeline::temp_path(dst);
    let result = (|| -> Result<()> {
        let f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        let mut gz = GzEncoder::new(f, Compression::new(level.min(9)));
        gz.write_all(&raw)?;
        gz.finish()?.sync_all()?;
        Ok(())
    })();
    match result {
        Ok(()) => crate::pipeline::commit(&tmp, dst, overwrite),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Archive;

    #[test]
    fn writes_and_reads_files_folders_and_an_empty_file() {
        let dir = std::env::temp_dir().join(format!("rgrf-rgz-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dst = dir.join("p.rgz");
        let big: Vec<u8> = (0..30_000).map(|i| (i % 253) as u8).collect();
        let files = vec![
            (GrfPath::new(b"data\\a.txt".to_vec()), b"abc".to_vec()),
            (GrfPath::new(b"data\\sprite\\b.bin".to_vec()), big.clone()),
            (GrfPath::new(b"readme.txt".to_vec()), Vec::new()),
        ];
        write(&dst, &files, 6, false).unwrap();
        assert!(matches!(
            write(&dst, &files, 6, false),
            Err(Error::TargetExists(_))
        ));
        let a = Archive::open(&dst).unwrap();
        assert_eq!(a.header().kind, Kind::Rgz);
        assert_eq!(a.len(), 3);
        for (i, (name, data)) in files.iter().enumerate() {
            assert_eq!(a.entry(i).unwrap().name, *name);
            let d = a.read(i).unwrap();
            assert_eq!((d.status, d.data), (crate::DecodeStatus::Ok, data.clone()));
        }
        assert!(!a.entry(2).unwrap().is_marker());
        let rep = crate::verify::verify(
            &a,
            crate::verify::VerifyOptions::default(),
            &crate::progress::Control::default(),
        )
        .unwrap();
        assert_eq!(rep.exit_code(), 0, "{:?}", rep.issues);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn damaged_rgz_is_an_error_not_a_panic() {
        let dir = std::env::temp_dir().join(format!("rgrf-rgz-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ok = dir.join("ok.rgz");
        write(
            &ok,
            &[(GrfPath::new(b"a.txt".to_vec()), vec![7; 500])],
            6,
            false,
        )
        .unwrap();
        let good = std::fs::read(&ok).unwrap();
        // a record that claims more data than there is
        let mut raw = vec![b'f', 6];
        raw.extend(b"a.txt\0");
        raw.extend(1_000_000i32.to_le_bytes());
        raw.extend([1, 2, 3]);
        let mut gz = GzEncoder::new(Vec::new(), Compression::new(1));
        gz.write_all(&raw).unwrap();
        let p = dir.join("long.rgz");
        std::fs::write(&p, gz.finish().unwrap()).unwrap();
        assert!(Archive::open(&p).is_err());
        let p = dir.join("cut.rgz");
        std::fs::write(&p, &good[..good.len() / 2]).unwrap();
        assert!(Archive::open(&p).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
