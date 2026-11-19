//! Thor patches (`.thor`): a header, the data blocks (zlib or LZMA streams) and a zlib-compressed
//! file table at the end. Two modes: `0x30` (a patch with any number of files and delete
//! directives) and `0x21` (one client executable followed by the marker `SEALED!`).
//!
//! The layout follows GRF Editor's `Thor`/`ThorHeader`/`ThorEntry` (Tokeiburu, used with
//! permission, see docs/05-licensing.md). Checked against files written by GRF Editor's own `GrfCL`
//! (mode 0x30 and 0x21, tools/golden/run-patch-in-vm.sh): they open here with the exact content of
//! the source folder, and `GrfCL` opens the files written here. A Thor from a real patcher
//! (Thor Patcher, game updates) was not available, see docs/02-format-notes.md.
//!
//! An opened Thor is an ordinary [`crate::Archive`]: its offsets are absolute in the file, so the
//! archive reads the file through a 46-byte shift ([`crate::io::Source::Shifted`]).

use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::container::{Container, Entry, HEADER_LEN, Header, Kind, PatchInfo, TableInfo, flags};
use crate::encode::{Method, encode};
use crate::error::{Error, Result};
use crate::io::ReadAt;
use crate::name::GrfPath;

pub const MAGIC: &[u8; 24] = b"ASSF (C) 2007 Aeomin DEV";
const MODE_PATCH: u16 = 0x30;
const MODE_EXE: u16 = 0x21;
const MAX_TABLE_SIZE: u64 = 1 << 30;
/// File name of the hash list GRF Editor adds to a patch (`path=0xCRC32` lines of the stored streams).
pub const INTEGRITY_FILE: &str = "data.integrity";

fn corrupt(msg: impl Into<String>) -> Error {
    Error::CorruptTable(msg.into())
}

struct Cursor<'a> {
    d: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&e| e <= self.d.len())
            .ok_or_else(|| corrupt("Thor header or table is truncated"))?;
        let s = &self.d[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn i32(&mut self) -> Result<i32> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(self.i32()? as u32)
    }
    fn i16(&mut self) -> Result<i16> {
        let b = self.take(2)?;
        Ok(i16::from_le_bytes([b[0], b[1]]))
    }
    fn name(&mut self) -> Result<GrfPath> {
        let n = usize::from(self.u8()?);
        let b = self.take(n)?;
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        Ok(GrfPath::new(&b[..end]))
    }
}

fn size_field(v: i32, what: &str) -> Result<u32> {
    u32::try_from(v).map_err(|_| corrupt(format!("Thor entry with negative {what}")))
}

/// Reads the header and file table of a Thor file. Offsets of the entries are those of the file;
/// the returned container describes the archive as seen through a 46-byte shift.
pub fn read_container(src: &dyn ReadAt) -> Result<Container> {
    let len = src.len()?;
    let mut head = vec![0u8; len.min(512) as usize];
    src.read_exact_at(&mut head, 0)?;
    if head.len() < MAGIC.len() || head[..MAGIC.len()] != MAGIC[..] {
        return Err(Error::NotGrf("Thor magic not found".into()));
    }
    let mut c = Cursor {
        d: &head,
        at: MAGIC.len(),
    };
    let merge_into_grf = c.u8()? != 0;
    let _declared = c.i32()?; // the count in the header is not reliable (GRF Editor ignores it)
    let mode = c.i16()? as u16;
    let target_len = usize::from(c.u8()?);
    let target_grf = c.take(target_len)?.to_vec();
    let (entries, table) = match mode {
        MODE_PATCH => {
            let zsize = size_field(c.i32()?, "table size")?;
            let table_off = u64::from(c.u32()?);
            table_off
                .checked_add(u64::from(zsize))
                .filter(|&e| e <= len)
                .ok_or_else(|| corrupt("Thor file table lies outside the file"))?;
            let mut packed = vec![0u8; zsize as usize];
            src.read_exact_at(&mut packed, table_off)?;
            let mut table = Vec::new();
            flate2::read::ZlibDecoder::new(&packed[..])
                .take(MAX_TABLE_SIZE + 1)
                .read_to_end(&mut table)
                .map_err(|e| corrupt(format!("Thor file table does not decompress: {e}")))?;
            if table.len() as u64 > MAX_TABLE_SIZE {
                return Err(corrupt("Thor file table is larger than 1 GiB"));
            }
            let mut t = Cursor { d: &table, at: 0 };
            let mut entries = Vec::new();
            while t.at < table.len() {
                let name = t.name()?;
                if t.u8()? & 1 != 0 {
                    entries.push(Entry {
                        name,
                        zsize: 0,
                        zsize_aligned: 0,
                        size: 0,
                        flags: flags::REMOVE,
                        offset: 0,
                    });
                    continue;
                }
                let offset = u64::from(t.u32()?);
                let zsize = size_field(t.i32()?, "stored size")?;
                let size = size_field(t.i32()?, "size")?;
                entries.push(Entry {
                    name,
                    zsize,
                    zsize_aligned: zsize,
                    size,
                    flags: flags::FILE,
                    offset,
                });
            }
            let info = TableInfo {
                abs_offset: HEADER_LEN + table_off,
                prefix_len: 0,
                zsize,
                size: table.len() as u32,
                key_block: 0,
            };
            (entries, (info, table_off))
        }
        MODE_EXE => {
            c.u8()?; // a zero byte after the (empty) target name
            let table_off = c.at as u64;
            let zsize = size_field(c.i32()?, "stored size")?;
            let size = size_field(c.i32()?, "size")?;
            let name = c.name()?;
            let offset = c.at as u64;
            let entry = Entry {
                name,
                zsize,
                zsize_aligned: zsize,
                size,
                flags: flags::FILE,
                offset,
            };
            let info = TableInfo {
                abs_offset: HEADER_LEN + table_off,
                prefix_len: 0,
                zsize: 0,
                size: 0,
                key_block: 0,
            };
            (vec![entry], (info, table_off))
        }
        m => return Err(Error::NotGrf(format!("unknown Thor mode 0x{m:x}"))),
    };
    let (table, table_off) = table;
    let mut magic = [0u8; 16];
    magic.copy_from_slice(&MAGIC[..16]);
    Ok(Container {
        header: Header {
            magic,
            key: [0; 14],
            table_offset: table_off,
            seed: 0,
            entry_count: entries.len() as u32,
            version: u32::from(mode),
            kind: Kind::Thor,
            patch: Some(PatchInfo {
                merge_into_grf,
                target_grf,
            }),
        },
        table,
        entries,
        file_len: len + HEADER_LEN,
    })
}

/// One file of a patch to write: its path and content, or a delete directive.
pub struct ThorFile {
    pub name: GrfPath,
    /// `None`: delete this path in the target.
    pub data: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct ThorOptions {
    /// Merge into the target GRF (paths inside `data\`) instead of the client folder.
    pub merge_into_grf: bool,
    /// Name of the target GRF; empty for the client's default.
    pub target_grf: Vec<u8>,
    pub method: Method,
    /// Add the `data.integrity` hash list (`path=0xCRC32` of the file content, as real Thor patches have it).
    pub integrity: bool,
    pub overwrite: bool,
}

impl Default for ThorOptions {
    fn default() -> Self {
        Self {
            merge_into_grf: false,
            target_grf: Vec::new(),
            method: Method::DEFAULT,
            integrity: false,
            overwrite: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThorReport {
    pub mode: u16,
    pub files: usize,
    pub removals: usize,
    /// Files whose stream is shared with an earlier identical one.
    pub shared: usize,
    pub bytes: u64,
}

fn name_bytes(n: &GrfPath) -> Result<&[u8]> {
    let b = n.as_bytes();
    if b.is_empty() || b.len() > 255 {
        return Err(Error::InvalidInput(format!(
            "Thor needs path names of 1 to 255 bytes, got {}",
            b.len()
        )));
    }
    Ok(b)
}

/// Writes a Thor patch. Mode 0x21 is chosen for a single `.exe`, else mode 0x30. Never writes over
/// an existing file unless `overwrite`; the file appears only when complete.
pub fn write(dst: &Path, files: &[ThorFile], opts: &ThorOptions) -> Result<ThorReport> {
    if files.is_empty() {
        return Err(Error::InvalidInput(
            "a Thor patch needs at least one file".into(),
        ));
    }
    if dst.exists() && !opts.overwrite {
        return Err(Error::TargetExists(dst.display().to_string()));
    }
    let single_exe = files.len() == 1
        && files[0].data.is_some()
        && files[0]
            .name
            .as_bytes()
            .rsplit(|&b| b == b'.')
            .next()
            .is_some_and(|e| e.eq_ignore_ascii_case(b"exe"));
    let tmp = crate::pipeline::temp_path(dst);
    let result = write_to(&tmp, files, opts, single_exe);
    match result {
        Ok(report) => {
            crate::pipeline::commit(&tmp, dst, opts.overwrite)?;
            Ok(report)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn header_bytes(
    mode: u16,
    merge: bool,
    count: usize,
    target: &[u8],
    table_zsize: u32,
    table_off: u32,
) -> Vec<u8> {
    let mut h = MAGIC.to_vec();
    h.push(u8::from(merge));
    h.extend((count as i32).to_le_bytes());
    h.extend(mode.to_le_bytes());
    h.push(target.len() as u8);
    h.extend(target);
    if mode == MODE_EXE {
        h.push(0);
    } else {
        h.extend((table_zsize as i32).to_le_bytes());
        h.extend((table_off as i32).to_le_bytes());
    }
    h
}

fn write_to(
    tmp: &Path,
    files: &[ThorFile],
    opts: &ThorOptions,
    single_exe: bool,
) -> Result<ThorReport> {
    let mode = if single_exe { MODE_EXE } else { MODE_PATCH };
    let merge = opts.merge_into_grf && !single_exe;
    let target: &[u8] = if merge { &opts.target_grf } else { &[] };
    if target.len() > 255 {
        return Err(Error::InvalidInput(
            "target GRF name longer than 255 bytes".into(),
        ));
    }
    let mut out = OpenOptions::new().write(true).create_new(true).open(tmp)?;
    let count = files.len() + usize::from(opts.integrity && !single_exe);
    let placeholder = header_bytes(mode, merge, count, target, 0, 0);
    out.write_all(&placeholder)?;
    let mut report = ThorReport {
        mode,
        files: 0,
        removals: 0,
        shared: 0,
        bytes: 0,
    };
    if single_exe {
        let f = &files[0];
        let data = f.data.as_deref().unwrap_or_default();
        let stream = encode(opts.method, data);
        let name = name_bytes(&f.name)?;
        out.write_all(&(stream.len() as i32).to_le_bytes())?;
        out.write_all(&(data.len() as i32).to_le_bytes())?;
        out.write_all(&[name.len() as u8])?;
        out.write_all(name)?;
        out.write_all(&stream)?;
        out.write_all(b"SEALED!")?;
        report.files = 1;
        report.bytes = data.len() as u64;
        out.sync_all()?;
        return Ok(report);
    }

    // (name, offset, stored size, size) or a removal
    struct Placed {
        name: Vec<u8>,
        at: Option<(u32, u32, u32)>,
    }
    let mut placed: Vec<Placed> = Vec::with_capacity(count);
    // streams stored so far, by (CRC32, length): offset, stored size, bytes
    type Stored = Vec<(u32, u32, Vec<u8>)>;
    let mut seen: std::collections::HashMap<(u32, usize), Stored> =
        std::collections::HashMap::new();
    let mut pos = placeholder.len() as u64;
    let mut integrity: Vec<u8> = Vec::new();
    let mut store = |out: &mut std::fs::File,
                     report: &mut ThorReport,
                     name: &[u8],
                     data: &[u8],
                     integrity: &mut Vec<u8>|
     -> Result<Placed> {
        let stream = encode(opts.method, data);
        let crc = crc32fast::hash(&stream);
        // identical streams are stored once (GRF Editor redirects entries with equal CRC32)
        let bucket = seen.entry((crc, stream.len())).or_default();
        let at = if let Some((off, z, _)) = bucket.iter().find(|(_, _, s)| *s == stream) {
            report.shared += 1;
            (*off, *z, data.len() as u32)
        } else {
            let off = u32::try_from(pos)
                .map_err(|_| Error::TooLarge("a Thor patch cannot exceed 4 GiB".into()))?;
            out.write_all(&stream)?;
            pos += stream.len() as u64;
            bucket.push((off, stream.len() as u32, stream.clone()));
            (off, stream.len() as u32, data.len() as u32)
        };
        report.bytes += data.len() as u64;
        integrity.extend(name);
        integrity.extend(format!("=0x{:08x}\r\n", crc32fast::hash(data)).bytes());
        Ok(Placed {
            name: name.to_vec(),
            at: Some(at),
        })
    };
    for f in files {
        let name = name_bytes(&f.name)?;
        match &f.data {
            Some(data) => {
                placed.push(store(&mut out, &mut report, name, data, &mut integrity)?);
                report.files += 1;
            }
            None => {
                placed.push(Placed {
                    name: name.to_vec(),
                    at: None,
                });
                report.removals += 1;
            }
        }
    }
    if opts.integrity && !integrity.is_empty() {
        let text = integrity.clone();
        placed.push(store(
            &mut out,
            &mut report,
            INTEGRITY_FILE.as_bytes(),
            &text,
            &mut Vec::new(),
        )?);
        report.files += 1;
    }
    let mut table = Vec::new();
    for p in &placed {
        table.push(p.name.len() as u8);
        table.extend(&p.name);
        match p.at {
            None => table.push(1),
            Some((off, z, size)) => {
                table.push(0);
                table.extend(off.to_le_bytes());
                table.extend(z.to_le_bytes());
                table.extend(size.to_le_bytes());
            }
        }
    }
    let packed = encode(Method::Zlib { level: 9 }, &table);
    let table_off = u32::try_from(pos)
        .map_err(|_| Error::TooLarge("a Thor patch cannot exceed 4 GiB".into()))?;
    out.write_all(&packed)?;
    out.seek(SeekFrom::Start(0))?;
    out.write_all(&header_bytes(
        mode,
        merge,
        count,
        target,
        packed.len() as u32,
        table_off,
    ))?;
    out.sync_all()?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Archive;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("rgrf-thor-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn files() -> Vec<ThorFile> {
        let big: Vec<u8> = (0..20_000).map(|i| (i % 251) as u8).collect();
        vec![
            ThorFile {
                name: GrfPath::new(b"data\\a.txt".to_vec()),
                data: Some(b"hello thor".to_vec()),
            },
            ThorFile {
                name: GrfPath::new(b"data\\old.spr".to_vec()),
                data: None,
            },
            ThorFile {
                name: GrfPath::new(b"data\\sprite\\b.bin".to_vec()),
                data: Some(big.clone()),
            },
            ThorFile {
                name: GrfPath::new(b"data\\sprite\\copy.bin".to_vec()),
                data: Some(big),
            },
            ThorFile {
                name: GrfPath::new(b"data\\empty.txt".to_vec()),
                data: Some(Vec::new()),
            },
        ]
    }

    #[test]
    fn writes_and_reads_a_patch_with_removals_and_shared_streams() {
        let dir = tmp("patch");
        let dst = dir.join("p.thor");
        let opts = ThorOptions {
            merge_into_grf: true,
            target_grf: b"data.grf".to_vec(),
            integrity: true,
            ..ThorOptions::default()
        };
        let r = write(&dst, &files(), &opts).unwrap();
        assert_eq!(
            (r.mode, r.files, r.removals, r.shared),
            (MODE_PATCH, 5, 1, 1)
        );
        let a = Archive::open(&dst).unwrap();
        let h = a.header();
        assert_eq!((h.kind, h.version), (Kind::Thor, 0x30));
        let p = h.patch.as_ref().unwrap();
        assert!(p.merge_into_grf);
        assert_eq!(p.target_grf, b"data.grf");
        // 5 files + the removal + the integrity list
        assert_eq!(a.len(), 6);
        assert!(a.entry(1).unwrap().is_removal() && a.entry(1).unwrap().is_marker());
        assert_eq!(a.read(0).unwrap().data, b"hello thor");
        assert_eq!(a.read(2).unwrap().data, a.read(3).unwrap().data);
        assert_eq!(a.entry(2).unwrap().offset, a.entry(3).unwrap().offset);
        assert!(a.read(4).unwrap().data.is_empty());
        let integrity = a.read(5).unwrap().data;
        let text = String::from_utf8(integrity).unwrap();
        assert!(text.contains("data\\a.txt=0x"), "{text}");
        let rep = crate::verify::verify(
            &a,
            crate::verify::VerifyOptions::default(),
            &crate::progress::Control::default(),
        )
        .unwrap();
        assert_eq!(rep.exit_code(), 0, "{:?}", rep.issues);
        // the CRC32 of the content are checked and agree
        assert!(
            rep.issues
                .iter()
                .any(|i| i.kind == crate::verify::IssueKind::Integrity
                    && i.message.contains("agree")),
            "{:?}",
            rep.issues
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn merge_applies_delete_directives_to_lower_priority_archives_only() {
        use crate::merge::merge;
        use crate::progress::Control;
        use crate::repack::RepackOptions;
        let dir = tmp("merge");
        let base = dir.join("base.rgz");
        let patch = dir.join("p.thor");
        let later = dir.join("later.rgz");
        let f = |n: &str, d: &[u8]| (GrfPath::new(n.as_bytes().to_vec()), d.to_vec());
        crate::rgz::write(
            &base,
            &[
                f("data\\keep.txt", b"k"),
                f("data\\gone.txt", b"g"),
                f("data\\back.txt", b"old"),
            ],
            6,
            false,
        )
        .unwrap();
        let t = |n: &str, d: Option<&[u8]>| ThorFile {
            name: GrfPath::new(n.as_bytes().to_vec()),
            data: d.map(<[u8]>::to_vec),
        };
        write(
            &patch,
            &[
                t("data\\GONE.txt", None),
                t("data\\back.txt", None),
                t("data\\none.txt", None),
            ],
            &ThorOptions::default(),
        )
        .unwrap();
        crate::rgz::write(&later, &[f("data\\back.txt", b"new")], 6, false).unwrap();
        let (b, p, l) = (
            Archive::open(&base).unwrap(),
            Archive::open(&patch).unwrap(),
            Archive::open(&later).unwrap(),
        );
        let out = dir.join("out.grf");
        // priority: later (highest), patch, base; the directive removes base's files, but a file
        // of the higher-priority archive is added again
        let rep = merge(
            &[&l, &p, &b],
            &out,
            &RepackOptions::default(),
            &Control::default(),
        )
        .unwrap();
        let g = Archive::open(&out).unwrap();
        let mut names: Vec<String> = g
            .entries()
            .iter()
            .map(|e| e.name.display(crate::NameMode::Cp1252))
            .collect();
        names.sort();
        assert_eq!(names, ["data\\back.txt", "data\\keep.txt"]);
        let back = g.find(&GrfPath::new(b"data\\back.txt".to_vec())).unwrap();
        assert_eq!(g.read(back).unwrap().data, b"new");
        assert_eq!((rep.sources[1].removals, rep.sources[1].removed), (3, 2));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_single_exe_becomes_a_mode_0x21_patch() {
        let dir = tmp("exe");
        let dst = dir.join("client.thor");
        let exe: Vec<u8> = (0..5000).map(|i| (i * 7 % 256) as u8).collect();
        let f = [ThorFile {
            name: GrfPath::new(b"ragexe.exe".to_vec()),
            data: Some(exe.clone()),
        }];
        let r = write(&dst, &f, &ThorOptions::default()).unwrap();
        assert_eq!((r.mode, r.files), (MODE_EXE, 1));
        let bytes = std::fs::read(&dst).unwrap();
        assert!(bytes.ends_with(b"SEALED!"));
        let a = Archive::open(&dst).unwrap();
        assert_eq!(a.header().version, 0x21);
        assert_eq!(a.len(), 1);
        assert_eq!(a.read(0).unwrap().data, exe);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn damaged_patches_are_errors_not_panics() {
        let dir = tmp("bad");
        let dst = dir.join("p.thor");
        write(&dst, &files(), &ThorOptions::default()).unwrap();
        let good = std::fs::read(&dst).unwrap();
        for cut in [3usize, 20, 40, 60, good.len() / 2] {
            let p = dir.join(format!("cut{cut}.thor"));
            std::fs::write(&p, &good[..cut.min(good.len() - 1)]).unwrap();
            assert!(Archive::open(&p).is_err(), "cut {cut}");
        }
        let mut wrong_mode = good.clone();
        wrong_mode[29..31].copy_from_slice(&0x77u16.to_le_bytes());
        let p = dir.join("mode.thor");
        std::fs::write(&p, &wrong_mode).unwrap();
        assert!(Archive::open(&p).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn refuses_to_overwrite_and_bad_names() {
        let dir = tmp("over");
        let dst = dir.join("p.thor");
        write(&dst, &files(), &ThorOptions::default()).unwrap();
        assert!(matches!(
            write(&dst, &files(), &ThorOptions::default()),
            Err(Error::TargetExists(_))
        ));
        let long = [ThorFile {
            name: GrfPath::new(vec![b'a'; 300]),
            data: Some(vec![1]),
        }];
        assert!(write(&dir.join("x.thor"), &long, &ThorOptions::default()).is_err());
        assert!(
            !dir.join("x.thor").exists()
                && !crate::pipeline::temp_path(&dir.join("x.thor")).exists()
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
