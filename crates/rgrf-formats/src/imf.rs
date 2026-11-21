//! IMF files (`data\imf\*.imf`): per layer of a character, per action and frame, the priority
//! and offset of the layer. Ported from GRF Editor's `Imf` (with the permission of its author,
//! see `docs/05-licensing.md`); GRF Editor notes that changing them has no effect in the game.

pub const MAGIC: [u8; 4] = [0xae, 0x47, 0x81, 0x3f];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ImfError {
    #[error("this is not an IMF file")]
    Magic,
    #[error("the file ends in the middle of a node (at byte {0})")]
    Truncated(usize),
    #[error("a count of {0} nodes cannot be right")]
    Count(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImfFrame {
    pub priority: i32,
    pub x: i32,
    pub y: i32,
}

/// `layers[layer][action][frame]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imf {
    pub checksum: u32,
    pub layers: Vec<Vec<Vec<ImfFrame>>>,
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn i32(&mut self) -> Result<i32, ImfError> {
        let s = self
            .b
            .get(self.at..self.at + 4)
            .ok_or(ImfError::Truncated(self.at))?;
        self.at += 4;
        Ok(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    /// A count of nodes that each take at least `min` bytes.
    fn count(&mut self, min: usize) -> Result<usize, ImfError> {
        let n = i64::from(self.i32()?);
        usize::try_from(n)
            .ok()
            .filter(|&n| n.saturating_mul(min) <= self.b.len() - self.at)
            .ok_or(ImfError::Count(n))
    }
}

impl Imf {
    pub fn parse(bytes: &[u8]) -> Result<Self, ImfError> {
        if bytes.len() < 12 || bytes[..4] != MAGIC {
            return Err(ImfError::Magic);
        }
        let mut r = Reader { b: bytes, at: 4 };
        let checksum = r.i32()? as u32;
        // the file stores the number of layers minus one
        let layers_n = i64::from(r.i32()?) + 1;
        if !(0..=65_536).contains(&layers_n) {
            return Err(ImfError::Count(layers_n));
        }
        let mut layers = Vec::new();
        for _ in 0..layers_n {
            let mut actions = Vec::new();
            for _ in 0..r.count(4)? {
                let mut frames = Vec::new();
                for _ in 0..r.count(12)? {
                    frames.push(ImfFrame {
                        priority: r.i32()?,
                        x: r.i32()?,
                        y: r.i32()?,
                    });
                }
                actions.push(frames);
            }
            layers.push(actions);
        }
        Ok(Self { checksum, layers })
    }

    /// GRF Editor's text of the file.
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        for (i, layer) in self.layers.iter().enumerate() {
            s += &format!("Node {i}\n\tActions... : {}\n", layer.len());
            for frames in layer {
                s += &format!("\t\tFrames... : {}\n", frames.len());
                for f in frames {
                    s += &format!("\t\t\t({}, {}) Priority {}\n", f.x, f.y, f.priority);
                }
            }
            s.push('\n');
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> Vec<u8> {
        let mut b = MAGIC.to_vec();
        for v in [7i32, 1, 2, 1, 3, -4, 5, 2, 0, 0, 0, 0, 0, 0, 0] {
            b.extend(v.to_le_bytes());
        }
        b
    }

    #[test]
    fn layers_actions_and_frames_are_read() {
        let i = Imf::parse(&file()).unwrap();
        assert_eq!(i.checksum, 7);
        assert_eq!(i.layers.len(), 2, "the stored 1 means two layers");
        assert_eq!(i.layers[0][0].len(), 1);
        assert_eq!(
            i.layers[0][0][0],
            ImfFrame {
                priority: 3,
                x: -4,
                y: 5
            }
        );
        assert!(i.to_text().contains("(-4, 5) Priority 3"));
        assert_eq!(Imf::parse(b"nope....12345"), Err(ImfError::Magic));
        assert!(matches!(
            Imf::parse(&file()[..30]),
            Err(ImfError::Truncated(_) | ImfError::Count(_))
        ));
    }
}
