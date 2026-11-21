//! Lossless PNG optimisation: the same picture (every pixel, the colour type, the bit depth, the
//! palette and the transparency stay exactly as they are) with a better choice of scanline
//! filters and a Zopfli-compressed data stream, and without text, time stamp and Exif chunks.
//!
//! The result is checked before it is returned: the unfiltered picture of the new file must be
//! byte for byte the one of the old file. Interlaced files and animated PNGs are left alone.

use std::io::Read;

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::Write;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Chunks that do not change the picture and that no client reads.
const DROPPED: [&[u8; 4]; 5] = [b"tEXt", b"zTXt", b"iTXt", b"tIME", b"eXIf"];

/// What became of a file.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A smaller file with the same picture.
    Smaller(Vec<u8>),
    /// Already as small as this tool gets it.
    NotSmaller,
    /// Not touched, with the reason (not a PNG, interlaced, animated, damaged).
    Skipped(&'static str),
}

struct Chunk<'a> {
    kind: [u8; 4],
    data: &'a [u8],
}

fn chunks(data: &[u8]) -> Option<Vec<Chunk<'_>>> {
    if data.len() < 8 || data[..8] != SIGNATURE {
        return None;
    }
    let mut out = Vec::new();
    let mut pos = 8;
    while pos + 12 <= data.len() {
        let len = u32::from_be_bytes(data[pos..pos + 4].try_into().ok()?) as usize;
        let kind: [u8; 4] = data[pos + 4..pos + 8].try_into().ok()?;
        let end = pos.checked_add(8)?.checked_add(len)?;
        if end + 4 > data.len() {
            return None;
        }
        out.push(Chunk {
            kind,
            data: &data[pos + 8..end],
        });
        pos = end + 4;
        if &kind == b"IEND" {
            return Some(out);
        }
    }
    None
}

fn crc32(kind: &[u8; 4], data: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(kind);
    h.update(data);
    h.finalize()
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(kind, data).to_be_bytes());
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (i16::from(a), i16::from(b), i16::from(c));
    let p = ia + ib - ic;
    let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// The byte left of position `i` (0 before the first pixel), above, and above left.
fn neighbours(row: &[u8], prev: &[u8], i: usize, bpp: usize) -> (u8, u8, u8) {
    if i >= bpp {
        (row[i - bpp], prev[i], prev[i - bpp])
    } else {
        (0, prev[i], 0)
    }
}

fn unfilter(filtered: &[u8], rowlen: usize, height: usize, bpp: usize) -> Option<Vec<u8>> {
    let mut out = vec![0u8; rowlen * height];
    let zero = vec![0u8; rowlen];
    for y in 0..height {
        let line = &filtered[y * (rowlen + 1)..(y + 1) * (rowlen + 1)];
        let (filter, src) = (line[0], &line[1..]);
        let (done, rest) = out.split_at_mut(y * rowlen);
        let prev = if y == 0 {
            &zero[..]
        } else {
            &done[(y - 1) * rowlen..]
        };
        let row = &mut rest[..rowlen];
        for i in 0..rowlen {
            let (a, b, c) = neighbours(row, prev, i, bpp);
            let predicted = match filter {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                4 => paeth(a, b, c),
                _ => return None,
            };
            row[i] = src[i].wrapping_add(predicted);
        }
    }
    Some(out)
}

fn filter_row(filter: u8, row: &[u8], prev: &[u8], bpp: usize, out: &mut Vec<u8>) {
    out.push(filter);
    for i in 0..row.len() {
        let (a, b, c) = neighbours(row, prev, i, bpp);
        let predicted = match filter {
            0 => 0,
            1 => a,
            2 => b,
            3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
            _ => paeth(a, b, c),
        };
        out.push(row[i].wrapping_sub(predicted));
    }
}

/// One filter for every line, or (`None`) the line by line choice with the smallest sum of
/// absolute values.
fn refilter(raw: &[u8], rowlen: usize, height: usize, bpp: usize, fixed: Option<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity((rowlen + 1) * height);
    let zero = vec![0u8; rowlen];
    let mut trial = Vec::with_capacity(rowlen + 1);
    for y in 0..height {
        let row = &raw[y * rowlen..(y + 1) * rowlen];
        let prev = if y == 0 {
            &zero[..]
        } else {
            &raw[(y - 1) * rowlen..y * rowlen]
        };
        match fixed {
            Some(f) => filter_row(f, row, prev, bpp, &mut out),
            None => {
                let mut best: Option<(u64, Vec<u8>)> = None;
                for f in 0..5u8 {
                    trial.clear();
                    filter_row(f, row, prev, bpp, &mut trial);
                    let sum: u64 = trial[1..]
                        .iter()
                        .map(|&b| u64::from((b as i8).unsigned_abs()))
                        .sum();
                    if best.as_ref().is_none_or(|(s, _)| sum < *s) {
                        best = Some((sum, trial.clone()));
                    }
                }
                out.extend_from_slice(&best.expect("five filters tried").1);
            }
        }
    }
    out
}

fn quick(data: &[u8]) -> usize {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::new(6));
    e.write_all(data).expect("a Vec cannot fail");
    e.finish().expect("a Vec cannot fail").len()
}

/// Optimises one PNG file. `deflate` makes the final zlib stream of the chosen filtered lines
/// (the caller brings Zopfli; any zlib stream is valid).
pub fn optimize(data: &[u8], deflate: &dyn Fn(&[u8]) -> Vec<u8>) -> Outcome {
    let Some(all) = chunks(data) else {
        return Outcome::Skipped("not a readable PNG");
    };
    let Some(ihdr) = all
        .first()
        .filter(|c| &c.kind == b"IHDR" && c.data.len() == 13)
    else {
        return Outcome::Skipped("not a readable PNG");
    };
    if all
        .iter()
        .any(|c| matches!(&c.kind, b"acTL" | b"fcTL" | b"fdAT"))
    {
        return Outcome::Skipped("animated PNG");
    }
    let width = u32::from_be_bytes(ihdr.data[0..4].try_into().unwrap()) as usize;
    let height = u32::from_be_bytes(ihdr.data[4..8].try_into().unwrap()) as usize;
    let (depth, colour, interlace) = (ihdr.data[8], ihdr.data[9], ihdr.data[12]);
    if interlace != 0 {
        return Outcome::Skipped("interlaced");
    }
    let channels = match colour {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return Outcome::Skipped("not a readable PNG"),
    };
    if width == 0 || height == 0 || width > 1 << 16 || height > 1 << 16 {
        return Outcome::Skipped("not a readable PNG");
    }
    let bits = channels * usize::from(depth);
    let rowlen = (width * bits).div_ceil(8);
    let bpp = bits.div_ceil(8).max(1);
    let wanted = (rowlen + 1) * height;

    let idat: Vec<u8> = all
        .iter()
        .filter(|c| &c.kind == b"IDAT")
        .flat_map(|c| c.data.iter().copied())
        .collect();
    let mut filtered = Vec::with_capacity(wanted);
    if ZlibDecoder::new(&idat[..])
        .take(wanted as u64 + 1)
        .read_to_end(&mut filtered)
        .is_err()
        || filtered.len() != wanted
    {
        return Outcome::Skipped("damaged picture data");
    }
    let Some(raw) = unfilter(&filtered, rowlen, height, bpp) else {
        return Outcome::Skipped("damaged picture data");
    };

    // the old line filters count as one candidate, then every single filter and the line by line choice
    let mut candidates = vec![filtered];
    candidates.extend((0..5u8).map(|f| refilter(&raw, rowlen, height, bpp, Some(f))));
    candidates.push(refilter(&raw, rowlen, height, bpp, None));
    let best = candidates
        .iter()
        .min_by_key(|c| quick(c))
        .expect("candidates exist");
    let packed = deflate(best);

    let dropped = all.iter().filter(|c| DROPPED.contains(&&c.kind)).count();
    if packed.len() >= idat.len() && dropped == 0 {
        return Outcome::NotSmaller;
    }
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&SIGNATURE);
    let mut wrote = false;
    for c in &all {
        if DROPPED.contains(&&c.kind) {
            continue;
        }
        if &c.kind == b"IDAT" {
            if !wrote {
                // the smaller of the old and the new data
                if packed.len() < idat.len() {
                    write_chunk(&mut out, b"IDAT", &packed);
                } else {
                    write_chunk(&mut out, b"IDAT", &idat);
                }
                wrote = true;
            }
            continue;
        }
        write_chunk(&mut out, &c.kind, c.data);
    }
    if out.len() >= data.len() {
        return Outcome::NotSmaller;
    }
    // the picture must be the same
    let Some(again) = chunks(&out) else {
        return Outcome::Skipped("result not readable");
    };
    let new_idat: Vec<u8> = again
        .iter()
        .filter(|c| &c.kind == b"IDAT")
        .flat_map(|c| c.data.iter().copied())
        .collect();
    let mut back = Vec::new();
    let same = ZlibDecoder::new(&new_idat[..])
        .read_to_end(&mut back)
        .is_ok()
        && unfilter(&back, rowlen, height, bpp).is_some_and(|r| r == raw);
    if !same {
        return Outcome::Skipped("result differs");
    }
    Outcome::Smaller(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(
        width: u32,
        height: u32,
        colour: png::ColorType,
        depth: png::BitDepth,
        raw: &[u8],
        filter: png::Filter,
        text: bool,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, width, height);
            e.set_color(colour);
            e.set_depth(depth);
            e.set_filter(filter);
            e.set_compression(png::Compression::Fast);
            if colour == png::ColorType::Indexed {
                e.set_palette(vec![0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255]);
                e.set_trns(vec![0, 255]);
            }
            if text {
                e.add_text_chunk("Comment".into(), "made by a test".into())
                    .unwrap();
            }
            let mut w = e.write_header().unwrap();
            w.write_image_data(raw).unwrap();
        }
        out
    }

    fn decode(data: &[u8]) -> (png::OutputInfo, Vec<u8>) {
        let mut r = png::Decoder::new(std::io::Cursor::new(data))
            .read_info()
            .unwrap();
        let mut buf = vec![0; r.output_buffer_size().unwrap()];
        let info = r.next_frame(&mut buf).unwrap();
        buf.truncate(info.buffer_size());
        (info, buf)
    }

    fn picture(width: usize, height: usize, channels: usize) -> Vec<u8> {
        let mut v = Vec::new();
        for y in 0..height {
            for x in 0..width {
                for c in 0..channels {
                    v.push(((x * 3 + y * 2 + c * 40) % 251) as u8 ^ ((x * y) as u8 & 3));
                }
            }
        }
        v
    }

    fn better(data: &[u8]) -> Vec<u8> {
        let mut e = ZlibEncoder::new(Vec::new(), Compression::new(9));
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    #[test]
    fn the_picture_stays_the_same_in_every_colour_type_and_the_file_gets_smaller() {
        let cases: [(png::ColorType, png::BitDepth, usize); 4] = [
            (png::ColorType::Rgba, png::BitDepth::Eight, 4),
            (png::ColorType::Rgb, png::BitDepth::Eight, 3),
            (png::ColorType::GrayscaleAlpha, png::BitDepth::Eight, 2),
            (png::ColorType::Grayscale, png::BitDepth::Eight, 1),
        ];
        for (colour, depth, channels) in cases {
            let raw = picture(96, 64, channels);
            let file = make(96, 64, colour, depth, &raw, png::Filter::NoFilter, true);
            let Outcome::Smaller(small) = optimize(&file, &better) else {
                panic!("{colour:?} did not get smaller");
            };
            assert!(small.len() < file.len());
            let (a, b) = (decode(&file), decode(&small));
            assert_eq!(
                (a.0.color_type, a.0.bit_depth),
                (b.0.color_type, b.0.bit_depth)
            );
            assert_eq!(a.1, b.1, "{colour:?}");
            // the text chunk is gone, nothing else was added
            assert!(!small.windows(7).any(|w| w == b"Comment"));
        }
    }

    #[test]
    fn palette_with_transparency_and_packed_bits_keep_their_chunks() {
        // 32 pixels of 2 bits are 8 bytes a line
        let raw: Vec<u8> = (0..8 * 8)
            .map(|i| [0x1b, 0xe4, 0x00, 0xff][i % 4])
            .collect();
        let file = make(
            32,
            8,
            png::ColorType::Indexed,
            png::BitDepth::Two,
            &raw,
            png::Filter::NoFilter,
            false,
        );
        let (info, pixels) = decode(&file);
        let out = match optimize(&file, &better) {
            Outcome::Smaller(s) => s,
            Outcome::NotSmaller => file.clone(),
            other => panic!("{other:?}"),
        };
        let (info2, pixels2) = decode(&out);
        assert_eq!(
            (info.color_type, info.bit_depth),
            (info2.color_type, info2.bit_depth)
        );
        assert_eq!(pixels, pixels2);
        let r = png::Decoder::new(std::io::Cursor::new(&out))
            .read_info()
            .unwrap();
        assert!(r.info().palette.is_some() && r.info().trns.is_some());
    }

    #[test]
    fn what_cannot_be_improved_or_read_is_left_alone() {
        let raw = picture(40, 40, 3);
        let file = make(
            40,
            40,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &raw,
            png::Filter::NoFilter,
            false,
        );
        let Outcome::Smaller(small) = optimize(&file, &better) else {
            panic!("expected smaller")
        };
        // a second pass has nothing left to gain
        assert_eq!(optimize(&small, &better), Outcome::NotSmaller);
        assert_eq!(
            optimize(b"not a png", &better),
            Outcome::Skipped("not a readable PNG")
        );
        let mut cut = file.clone();
        cut.truncate(file.len() - 20);
        assert!(matches!(optimize(&cut, &better), Outcome::Skipped(_)));
        // interlaced
        let mut inter = file.clone();
        inter[8 + 8 + 12] = 1;
        assert_eq!(optimize(&inter, &better), Outcome::Skipped("interlaced"));
    }
}
