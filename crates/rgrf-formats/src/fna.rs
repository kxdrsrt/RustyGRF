//! FNA files (`data\imf\*.fna`): a list of file names, each as a length (with the final 0 byte)
//! and the bytes. Ported from GRF Editor's `Fna` (with the permission of its author, see
//! `docs/05-licensing.md`).

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FnaError {
    #[error("the name at byte {0} is longer than the file")]
    Truncated(usize),
}

/// The names as stored (a client encoding, usually Korean), without the final 0 byte.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fna {
    pub files: Vec<Vec<u8>>,
}

impl Fna {
    pub fn parse(bytes: &[u8]) -> Result<Self, FnaError> {
        let mut files = Vec::new();
        let mut at = 0;
        while at + 4 <= bytes.len() {
            let n = i32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
            let start = at + 4;
            let end = usize::try_from(n)
                .ok()
                .and_then(|n| start.checked_add(n))
                .filter(|&e| e <= bytes.len())
                .ok_or(FnaError::Truncated(at))?;
            let raw = &bytes[start..end];
            let cut = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
            files.push(raw[..cut].to_vec());
            at = end;
        }
        Ok(Self { files })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for f in &self.files {
            out.extend((f.len() as i32 + 1).to_le_bytes());
            out.extend_from_slice(f);
            out.push(0);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_a_cut_name_is_refused() {
        let f = Fna {
            files: vec![b"a\\b.act".to_vec(), vec![], b"\xb0\xa1".to_vec()],
        };
        let bytes = f.to_bytes();
        assert_eq!(Fna::parse(&bytes).unwrap(), f);
        assert!(matches!(
            Fna::parse(&bytes[..bytes.len() - 1]),
            Err(FnaError::Truncated(_))
        ));
        assert!(Fna::parse(&[9, 0, 0, 0, b'x']).is_err());
        assert_eq!(Fna::parse(&[]).unwrap().files.len(), 0);
    }
}
