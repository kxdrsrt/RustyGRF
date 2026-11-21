//! `Thumbs.db` (the thumbnail cache of Windows Explorer, often left inside client folders and
//! archives): the names of the pictures and their JPEG thumbnails. Ported from GRF Editor's
//! `ThumbDB` (with the permission of its author, see `docs/05-licensing.md`), reading the
//! compound file itself instead of Windows' own routines.

use crate::cfb::{Cfb, CfbError};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ThumbsError {
    #[error(transparent)]
    Container(#[from] CfbError),
    #[error("the file has no catalog of thumbnails")]
    NoCatalog,
    #[error("the catalog is damaged")]
    Catalog,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumb {
    pub name: String,
    pub id: u32,
    /// Windows FILETIME of the picture's last change.
    pub modified: u64,
    /// The JPEG, when the thumbnail is stored in the usual way.
    pub jpeg: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbsDb {
    pub width: u32,
    pub height: u32,
    pub thumbs: Vec<Thumb>,
}

fn i32_at(b: &[u8], at: usize) -> Option<i32> {
    b.get(at..at + 4)
        .map(|s| i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// The stream of a thumbnail is named by the digits of its number in reverse order.
fn stream_name(id: u32) -> String {
    id.to_string().chars().rev().collect()
}

pub fn parse(bytes: &[u8]) -> Result<ThumbsDb, ThumbsError> {
    let cfb = Cfb::open(bytes)?;
    let cat = cfb.stream("Catalog")?.ok_or(ThumbsError::NoCatalog)?;
    let head =
        usize::try_from(i32_at(&cat, 0).unwrap_or(0) & 0xffff).map_err(|_| ThumbsError::Catalog)?;
    let count = usize::try_from(i32_at(&cat, 4).ok_or(ThumbsError::Catalog)?)
        .map_err(|_| ThumbsError::Catalog)?;
    let width = if head > 8 {
        i32_at(&cat, 8).unwrap_or(0)
    } else {
        0
    };
    let height = if head > 12 {
        i32_at(&cat, 12).unwrap_or(0)
    } else {
        0
    };
    let mut at = head.max(8);
    let mut thumbs = Vec::new();
    for _ in 0..count.min(cat.len() / 18) {
        let size = usize::try_from(i32_at(&cat, at).ok_or(ThumbsError::Catalog)?).unwrap_or(0);
        let id = i32_at(&cat, at + 4).ok_or(ThumbsError::Catalog)? as u32;
        let low = u64::from(i32_at(&cat, at + 8).ok_or(ThumbsError::Catalog)? as u32);
        let high = u64::from(i32_at(&cat, at + 12).ok_or(ThumbsError::Catalog)? as u32);
        let mut units = Vec::new();
        let mut p = at + 16;
        loop {
            let s = cat.get(p..p + 2).ok_or(ThumbsError::Catalog)?;
            p += 2;
            let u = u16::from_le_bytes([s[0], s[1]]);
            if u == 0 {
                break;
            }
            units.push(u);
        }
        // the entry says how long it is; the 2 bytes behind the name are reserved
        at = if size >= p + 2 - at { at + size } else { p + 2 };
        let jpeg = cfb.stream(&stream_name(id))?.and_then(|raw| {
            let head = usize::try_from(i32_at(&raw, 0)?).ok()?;
            let len = usize::try_from(i32_at(&raw, 8)?).ok()?;
            let data = raw.get(head..head.checked_add(len)?)?;
            (data.first() == Some(&0xff)).then(|| data.to_vec())
        });
        thumbs.push(Thumb {
            name: String::from_utf16_lossy(&units),
            id,
            modified: high << 32 | low,
            jpeg,
        });
    }
    Ok(ThumbsDb {
        width: width as u32,
        height: height as u32,
        thumbs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_names_are_the_reversed_digits() {
        assert_eq!(stream_name(1), "1");
        assert_eq!(stream_name(123), "321");
        assert!(matches!(
            parse(&[7u8; 600]),
            Err(ThumbsError::Container(CfbError::Header))
        ));
    }
}
