//! The client side of GRF Editor's encryption: a patched client executable and a `cps.dll` that
//! holds the key and decrypts the entries the client reads.
//!
//! Follows GRF Editor's encryptor (Tokeiburu, used with permission, see docs/05-licensing.md; the
//! dialog is in `GRFEditor/WPF/EncryptorDialog.xaml.cs`, the block layout was read from the IL of
//! its `Encryption.dll`). It needs the template `cps.dll` that GRF Editor carries as a resource
//! (`GRFEditor/Files/cps.dll`), which is built in ([`CPS_TEMPLATE`], with Tokeiburu's permission,
//! see docs/05-licensing.md); another one can be given.
//!
//! What it does: the template holds the placeholder `TKTK` followed by room for 524 bytes, which
//! become `[i32 length of the client][i32 seed][256 key bytes scrambled with the seed][260 bytes:
//! the client's file name, NUL padded, every byte inverted]`. The names the client uses for the
//! DLL (`cps.dll`) and for its function (`uncompress`) are replaced, in the client and in the
//! DLL, by names made from the key, so that the pair does not look like the usual one.

use crate::error::{Error, Result};
use crate::grfcrypt::{self, KEY_LEN, Key};

/// GRF Editor's `cps.dll` (MD5 d35eb41c552d8c2572b6938f7cd222b3), the template for [`patch_client`].
pub const CPS_TEMPLATE: &[u8] = include_bytes!("../assets/cps-template.dll");

/// The placeholder in the template DLL.
const PLACEHOLDER: &[u8] = b"TKTK";
const BLOCK_LEN: usize = 524;
const EXE_NAME_LEN: usize = 260;
/// GRF Editor's alphabet (the order of the last letters is as in its source).
const LETTERS: &[u8; 26] = b"abcdefghijklmnopqrstuvwyxz";

#[derive(Debug, Clone, Copy)]
pub struct ClientOptions {
    /// Give the DLL a name made from the key (GRF Editor's default) instead of `cps.dll`.
    pub rename: bool,
    /// Do not fail when the client lacks the names `uncompress` or the new DLL name.
    pub skip_resource_check: bool,
    /// Seed of the scrambled key in the DLL (any value; random by default).
    pub seed: i32,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            rename: true,
            skip_resource_check: false,
            seed: grfcrypt::random_seed(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientPatch {
    /// The client executable with the new names.
    pub client: Vec<u8>,
    /// The DLL with the key, to be saved under `dll_name` next to the client.
    pub dll: Vec<u8>,
    pub dll_name: String,
    pub uncompress_name: String,
}

fn find(data: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || data.len() < needle.len() {
        return None;
    }
    (from..=data.len() - needle.len()).find(|&i| &data[i..i + needle.len()] == needle)
}

/// Replaces every occurrence of `needle` (not preceded by `#`) by `insert`, which has the same
/// length; returns how many.
fn replace_all(data: &mut [u8], needle: &[u8], insert: &[u8]) -> usize {
    debug_assert_eq!(needle.len(), insert.len());
    let (mut count, mut from) = (0, 1);
    while let Some(i) = find(data, needle, from) {
        if data[i - 1] != b'#' {
            data[i..i + insert.len()].copy_from_slice(insert);
            count += 1;
        }
        from = i + 1;
    }
    count
}

/// GRF Editor mixes the key before it derives names: 250 000 times, the key is encrypted with a
/// copy of itself.
fn shuffled(key: &Key) -> Key {
    let mut k2 = *key;
    for _ in 0..250_000 {
        let k3 = k2;
        grfcrypt::crypt(&k3, &mut k2, KEY_LEN as u64);
    }
    k2
}

fn name_from_key(name: &str, key: &Key) -> String {
    name.bytes()
        .enumerate()
        .map(|(i, c)| LETTERS[usize::from(key[i]) * usize::from(c) % LETTERS.len()] as char)
        .collect()
}

/// The names GRF Editor would give the DLL and its function for `key`.
pub fn names_for_key(key: &Key, rename: bool) -> (String, String) {
    let k2 = shuffled(key);
    let dll = if rename {
        format!("{}.dll", name_from_key("cps", &k2))
    } else {
        "cps.dll".to_owned()
    };
    let un = name_from_key("uncompress", &k2);
    (dll, format!("g{}", &un[1..]))
}

/// Patches a client and the template DLL for `key`. `exe_name` is the file name the client will
/// have (the DLL checks it).
pub fn patch_client(
    client: &[u8],
    template: &[u8],
    key: &Key,
    exe_name: &[u8],
    opts: &ClientOptions,
) -> Result<ClientPatch> {
    if exe_name.len() > EXE_NAME_LEN {
        return Err(Error::InvalidInput(
            "the executable name is too long".into(),
        ));
    }
    let mut client = client.to_vec();
    // a client without "cps.dll" gets the name appended for the work and loses it again at the end
    let band_aid = find(&client, b"cps.dll\0", 0).is_none();
    if band_aid {
        client.extend_from_slice(b"cps.dll\0");
    }

    // the block in the DLL
    let at = find(template, PLACEHOLDER, 0)
        .filter(|&at| at + BLOCK_LEN <= template.len())
        .ok_or_else(|| {
            Error::InvalidInput(
                "this is not GRF Editor's cps.dll (the placeholder TKTK with room for the key is missing)"
                    .into(),
            )
        })?;
    let mut block = vec![0u8; BLOCK_LEN];
    block[..4].copy_from_slice(&(client.len() as i32).to_le_bytes());
    block[4..8].copy_from_slice(&opts.seed.to_le_bytes());
    let mut scrambled = *key;
    let mut seed = opts.seed;
    for b in &mut scrambled {
        *b ^= seed as u8;
        seed = seed.wrapping_mul(47);
    }
    block[8..8 + KEY_LEN].copy_from_slice(&scrambled);
    let mut name = [0xffu8; EXE_NAME_LEN];
    for (i, b) in exe_name.iter().enumerate() {
        name[i] = b ^ 0xff;
    }
    block[8 + KEY_LEN..].copy_from_slice(&name);
    let mut dll = template.to_vec();
    dll[at..at + BLOCK_LEN].copy_from_slice(&block);

    let (dll_name, uncompress) = names_for_key(key, opts.rename);
    replace_all(&mut client, b"cps.dll", dll_name.as_bytes());
    replace_all(&mut client, b"uncompress", uncompress.as_bytes());
    replace_all(&mut dll, b"uncompress", uncompress.as_bytes());
    let missing = |what: &str| {
        Error::InvalidInput(format!(
            "{what} was not found in the client. A 2024 client needs the cps.dll restore patch, and \
             a client that was already changed with another key cannot be changed again (or skip the check)"
        ))
    };
    if !opts.skip_resource_check {
        if find(&client, uncompress.as_bytes(), 0).is_none() {
            return Err(missing("the function name \"uncompress\""));
        }
        if opts.rename && find(&client, dll_name.as_bytes(), 0).is_none() {
            return Err(missing("the name \"cps.dll\""));
        }
    }
    if band_aid {
        client.truncate(client.len() - 8);
    }
    Ok(ClientPatch {
        client,
        dll,
        dll_name,
        uncompress_name: uncompress,
    })
}

/// What a patched DLL holds (GrfCL's `-extractDllInfo`): the client's length and file name and the
/// key.
pub fn read_dll_info(dll: &[u8]) -> Option<(i32, Vec<u8>, Key)> {
    let tail = dll.get(dll.len().checked_sub(BLOCK_LEN)?..)?;
    let len = i32::from_le_bytes(tail[..4].try_into().ok()?);
    let seed = i32::from_le_bytes(tail[4..8].try_into().ok()?);
    let mut key: Key = tail[8..8 + KEY_LEN].try_into().ok()?;
    let mut s = seed;
    for b in &mut key {
        *b ^= s as u8;
        s = s.wrapping_mul(47);
    }
    let name: Vec<u8> = tail[8 + KEY_LEN..]
        .iter()
        .map(|b| b ^ 0xff)
        .take_while(|&b| b != 0)
        .collect();
    Some((len, name, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template() -> Vec<u8> {
        let mut t = vec![7u8; 1000];
        t.extend_from_slice(b"..uncompress..");
        t.extend_from_slice(PLACEHOLDER);
        t.extend(vec![0u8; BLOCK_LEN - PLACEHOLDER.len()]);
        t
    }

    fn client() -> Vec<u8> {
        let mut c = vec![1u8; 500];
        c.extend_from_slice(b"cps.dll\0");
        c.extend_from_slice(b"uncompress\0");
        c
    }

    #[test]
    fn the_names_come_from_the_key_and_the_dll_holds_the_key() {
        let key = grfcrypt::key_from_password("secret").unwrap();
        let opts = ClientOptions {
            seed: 12345,
            ..ClientOptions::default()
        };
        let p = patch_client(&client(), &template(), &key, b"MyRO.exe", &opts).unwrap();
        let (dll, un) = names_for_key(&key, true);
        assert_eq!(
            (p.dll_name.as_str(), p.uncompress_name.as_str()),
            (dll.as_str(), un.as_str())
        );
        assert!(dll.ends_with(".dll") && dll.len() == 7 && un.len() == 10 && un.starts_with('g'));
        // the client and the DLL use the new names
        assert!(find(&p.client, dll.as_bytes(), 0).is_some());
        assert!(find(&p.client, b"cps.dll", 0).is_none());
        assert!(find(&p.client, un.as_bytes(), 0).is_some());
        assert!(find(&p.dll, un.as_bytes(), 0).is_some());
        assert!(find(&p.dll, b"uncompress", 0).is_none());
        assert_eq!(p.client.len(), client().len());
        let (len, name, k) = read_dll_info(&p.dll).unwrap();
        assert_eq!(
            (len as usize, name.as_slice(), k),
            (client().len(), &b"MyRO.exe"[..], key)
        );
    }

    #[test]
    fn missing_resources_and_a_foreign_dll_are_errors() {
        let key = grfcrypt::key_from_password("secret").unwrap();
        let opts = ClientOptions::default();
        // a client without the function name
        let bare = vec![1u8; 100];
        assert!(patch_client(&bare, &template(), &key, b"a.exe", &opts).is_err());
        let skip = ClientOptions {
            skip_resource_check: true,
            ..opts
        };
        let p = patch_client(&bare, &template(), &key, b"a.exe", &skip).unwrap();
        assert_eq!(p.client.len(), 100, "the appended name is taken off again");
        // not GRF Editor's DLL
        assert!(patch_client(&client(), &[0u8; 2000], &key, b"a.exe", &opts).is_err());
        assert!(patch_client(&client(), &template(), &key, &[b'a'; 300], &opts).is_err());
    }
}
