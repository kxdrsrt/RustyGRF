//! PAL palette files: 256 colours of R, G, B and an unused fourth byte (1024 bytes).
//!
//! Derived from GRF Editor (`GRF/FileFormats/PalFormat/Pal.cs`), used with permission,
//! see `docs/05-licensing.md`.

use crate::spr::PALETTE_LEN;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pal {
    /// R, G, B, unused for each of the 256 colours, exactly as stored.
    pub colors: Box<[u8; PALETTE_LEN]>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PalError {
    #[error("a PAL file is {PALETTE_LEN} bytes, this one has {0}")]
    BadLength(usize),
}

impl Pal {
    pub fn parse(bytes: &[u8]) -> Result<Self, PalError> {
        let colors: Box<[u8; PALETTE_LEN]> = bytes
            .to_vec()
            .into_boxed_slice()
            .try_into()
            .map_err(|_| PalError::BadLength(bytes.len()))?;
        Ok(Self { colors })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.colors.to_vec()
    }

    /// Colour `index` as RGBA with the client's rule: index 0 is transparent, the rest opaque.
    pub fn rgba(&self, index: u8) -> [u8; 4] {
        let i = usize::from(index);
        [
            self.colors[4 * i],
            self.colors[4 * i + 1],
            self.colors[4 * i + 2],
            if i == 0 { 0 } else { 255 },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_length_check() {
        let bytes: Vec<u8> = (0..PALETTE_LEN).map(|i| (i % 251) as u8).collect();
        let p = Pal::parse(&bytes).unwrap();
        assert_eq!(p.to_bytes(), bytes);
        assert_eq!(p.rgba(0)[3], 0);
        assert_eq!(p.rgba(1), [bytes[4], bytes[5], bytes[6], 255]);
        assert_eq!(Pal::parse(&bytes[..100]), Err(PalError::BadLength(100)));
        assert_eq!(Pal::parse(&[]), Err(PalError::BadLength(0)));
    }
}
