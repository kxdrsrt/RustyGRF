//! EBM files (guild emblems, `data\texture\...\*.ebm`): a BMP compressed with zlib. Ported from
//! GRF Editor's `Ebm` (with the permission of its author, see `docs/05-licensing.md`).

use std::io::Read;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EbmError {
    #[error("the emblem cannot be decompressed")]
    Zlib,
    #[error("the emblem is not a BMP picture")]
    NotBmp,
}

/// The BMP of an emblem (most emblems are 24 x 24 pixels, magenta is transparent in the game).
pub fn to_bmp(bytes: &[u8]) -> Result<Vec<u8>, EbmError> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(bytes)
        .read_to_end(&mut out)
        .map_err(|_| EbmError::Zlib)?;
    if !out.starts_with(b"BM") {
        return Err(EbmError::NotBmp);
    }
    Ok(out)
}

/// The compressed emblem of a BMP.
pub fn from_bmp(bmp: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(bmp).expect("writing to memory");
    z.finish().expect("writing to memory")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_emblem_is_a_zlib_compressed_bmp() {
        let bmp = crate::bmp::write_rgb(&crate::spr::RgbaImage {
            width: 2,
            height: 2,
            pixels: [255u8, 0, 255, 255].repeat(4),
        });
        let ebm = from_bmp(&bmp);
        assert_eq!(to_bmp(&ebm).unwrap(), bmp);
        assert!(crate::bmp::parse(&to_bmp(&ebm).unwrap()).is_ok());
        assert_eq!(to_bmp(b"plain"), Err(EbmError::Zlib));
        assert_eq!(to_bmp(&from_bmp(b"PNG")), Err(EbmError::NotBmp));
    }
}
