use md5::{Digest as _, Md5};

/// CRC32 and MD5 of decoded content, as GRF Editor / GrfCL report them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Digest {
    pub crc32: u32,
    pub md5: [u8; 16],
}

impl Digest {
    pub fn of(data: &[u8]) -> Self {
        let mut h = Md5::new();
        h.update(data);
        Self {
            crc32: crc32fast::hash(data),
            md5: h.finalize().into(),
        }
    }

    pub fn crc32_hex(&self) -> String {
        format!("{:08x}", self.crc32)
    }

    pub fn md5_hex(&self) -> String {
        crate::name::hex(&self.md5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        let d = Digest::of(b"");
        assert_eq!(d.crc32_hex(), "00000000");
        assert_eq!(d.md5_hex(), "d41d8cd98f00b204e9800998ecf8427e");
        let d = Digest::of(b"123456789");
        assert_eq!(d.crc32_hex(), "cbf43926");
        assert_eq!(d.md5_hex(), "25f9e794323b453885f5181f1b624d0b");
    }
}
