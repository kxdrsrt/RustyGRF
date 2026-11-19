//! GRF Editor's encryption on the command line: the key from `--password` / `--key-file`, used by
//! every command that opens an archive, and `rgrf key create`.

use std::path::Path;
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use rgrf_core::Archive;
use rgrf_core::grfcrypt::{self, Key};

static KEY: OnceLock<Option<Key>> = OnceLock::new();

/// Reads the key the user gave (once, at start); `None` when neither option is present.
pub fn init(password: Option<&str>, key_file: Option<&Path>) -> Result<()> {
    let from_env = std::env::var("RGRF_PASSWORD")
        .ok()
        .filter(|p| !p.is_empty());
    let password = password.or(from_env.as_deref());
    let key = match (password, key_file) {
        (Some(_), Some(_)) => bail!("give either --password or --key-file, not both"),
        (Some(p), None) => Some(grfcrypt::key_from_password(p)?),
        (None, Some(f)) => {
            let bytes = std::fs::read(f)
                .with_context(|| format!("cannot read key file {}", f.display()))?;
            Some(grfcrypt::key_from_file(&bytes).with_context(|| f.display().to_string())?)
        }
        (None, None) => None,
    };
    let _ = KEY.set(key);
    Ok(())
}

pub fn key() -> Option<&'static Key> {
    KEY.get().and_then(Option::as_ref)
}

/// `Archive::open`, with the key if the user gave one.
pub fn open_archive(path: impl AsRef<Path>) -> rgrf_core::Result<Archive> {
    match key() {
        Some(k) => Archive::open_with_key(path, k),
        None => Archive::open(path),
    }
}

/// `rgrf key create`: writes a `.grfkey` file for a password (GRF Editor loads it).
pub fn create(password: &str, output: &Path, overwrite: bool, json: bool) -> Result<i32> {
    let key = grfcrypt::key_from_password(password)?;
    if output.exists() && !overwrite {
        bail!("{} exists already (use --overwrite)", output.display());
    }
    std::fs::write(output, grfcrypt::key_file(&key, grfcrypt::random_seed()))
        .with_context(|| format!("cannot write {}", output.display()))?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "schema_version": crate::SCHEMA_VERSION,
                "output": output.display().to_string(),
                "key_crc32": format!("{:08x}", grfcrypt::key_crc(&key)),
            })
        );
    } else {
        println!(
            "key file {} written (CRC32 of the key {:08x})",
            output.display(),
            grfcrypt::key_crc(&key)
        );
    }
    Ok(0)
}
