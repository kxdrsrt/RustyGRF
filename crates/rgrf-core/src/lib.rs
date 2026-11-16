//! RustyGRF core library. Milestone M0: skeleton only, no format code yet.

/// Crate version, used by the CLI and the MCP server for `--version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
