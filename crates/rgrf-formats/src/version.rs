//! Version number as the client files store it: a major and a minor byte (`2.1`).

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
}

impl Version {
    pub const fn new(major: u8, minor: u8) -> Self {
        Self { major, minor }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl std::str::FromStr for Version {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || format!("version must look like 2.1, got {s:?}");
        let (a, b) = s.split_once('.').ok_or_else(bad)?;
        Ok(Self::new(
            a.parse().map_err(|_| bad())?,
            b.parse().map_err(|_| bad())?,
        ))
    }
}
