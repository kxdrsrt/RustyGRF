//! Positional reads without `unsafe` and without a shared cursor, so many threads can read one archive.

use std::fs::File;
use std::io;

pub trait ReadAt: Send + Sync {
    /// Reads exactly `buf.len()` bytes at `offset`.
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()>;
    /// Total length in bytes.
    fn len(&self) -> io::Result<u64>;
    fn is_empty(&self) -> io::Result<bool> {
        Ok(self.len()? == 0)
    }
}

impl ReadAt for File {
    #[cfg(unix)]
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        std::os::unix::fs::FileExt::read_exact_at(self, buf, offset)
    }

    #[cfg(windows)]
    fn read_exact_at(&self, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            match self.seek_read(buf, offset) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => {
                    buf = &mut buf[n..];
                    offset += n as u64;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn len(&self) -> io::Result<u64> {
        Ok(self.metadata()?.len())
    }
}

/// Where an opened archive reads from: the file itself, the file seen behind a 46-byte
/// pseudo header (Thor patches: their offsets are absolute in the file), or a decompressed
/// buffer (RGZ).
pub enum Source {
    File(File),
    /// Reading position `p` of the archive reads position `p - shift` of the file.
    Shifted {
        file: File,
        shift: u64,
    },
    Memory(Vec<u8>),
}

impl ReadAt for Source {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        match self {
            Self::File(f) => f.read_exact_at(buf, offset),
            Self::Shifted { file, shift } => file.read_exact_at(
                buf,
                offset
                    .checked_sub(*shift)
                    .ok_or(io::ErrorKind::UnexpectedEof)?,
            ),
            Self::Memory(v) => v.read_exact_at(buf, offset),
        }
    }

    fn len(&self) -> io::Result<u64> {
        match self {
            Self::File(f) => ReadAt::len(f),
            Self::Shifted { file, shift } => Ok(ReadAt::len(file)? + shift),
            Self::Memory(v) => Ok(v.len() as u64),
        }
    }
}

impl ReadAt for Vec<u8> {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        let start = usize::try_from(offset).map_err(|_| io::ErrorKind::UnexpectedEof)?;
        let end = start
            .checked_add(buf.len())
            .ok_or(io::ErrorKind::UnexpectedEof)?;
        let src = self.get(start..end).ok_or(io::ErrorKind::UnexpectedEof)?;
        buf.copy_from_slice(src);
        Ok(())
    }

    fn len(&self) -> io::Result<u64> {
        Ok(Vec::len(self) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec_reads_at_offset_and_rejects_past_end() {
        let v = vec![1u8, 2, 3, 4, 5];
        let mut b = [0u8; 2];
        v.read_exact_at(&mut b, 2).unwrap();
        assert_eq!(b, [3, 4]);
        assert!(v.read_exact_at(&mut b, 4).is_err());
        assert!(v.read_exact_at(&mut b, u64::MAX).is_err());
    }
}
