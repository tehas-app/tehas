//! What a contractor holds at rest: its name, its key and the factory's
//! address, in one file of mode 0600. Nothing else stays on the box between
//! jobs.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::http::{self, As, Reply};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The factory's address, `host:port`.
    pub factory: String,
    pub name: String,
    /// The key the factory knows by its sha256. A secret of this box.
    pub key: String,
}

/// Where the identity is kept: `~/.config/tehas/contractor.json`.
pub fn path(user_home: &Path) -> PathBuf {
    user_home.join(".config/tehas/contractor.json")
}

pub fn read(file: &Path) -> Option<Identity> {
    let v: Value = serde_json::from_slice(&fs::read(file).ok()?).ok()?;
    let text = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(str::to_string);
    Some(Identity {
        factory: text("factory")?,
        name: text("name")?,
        key: text("key")?,
    })
}

/// Write the identity whole, mode 0600, in a directory of mode 0700.
pub fn write(file: &Path, id: &Identity) -> Result<(), String> {
    let dir = file.parent().ok_or("the identity has no directory")?;
    fs::create_dir_all(dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    let new = file.with_extension("json.new");
    let _ = fs::remove_file(&new);
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&new)
        .map_err(|e| format!("cannot write {}: {e}", new.display()))?;
    let text = json!({ "factory": id.factory, "name": id.name, "key": id.key }).to_string();
    out.write_all(text.as_bytes())
        .and_then(|()| out.sync_all())
        .map_err(|e| format!("cannot write {}: {e}", new.display()))?;
    fs::rename(&new, file).map_err(|e| format!("cannot keep {}: {e}", file.display()))
}

/// The sha256 of a key or a code, as the factory's store holds it.
pub fn hash(secret: &str) -> String {
    Sha256::digest(secret.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A new key: 32 bytes of the kernel's randomness, in hex.
pub fn new_key() -> Option<String> {
    let mut bytes = [0u8; 32];
    fs::File::open("/dev/urandom")
        .ok()?
        .read_exact(&mut bytes)
        .ok()?;
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Join the factory at `factory` as `name` with its one-time `code`: a new
/// key is made here, its hash is sent with the code, and the identity is kept
/// only when the factory took it.
pub fn join(file: &Path, factory: &str, name: &str, code: &str) -> Result<Identity, String> {
    let key = new_key().ok_or("no randomness to make a key of")?;
    let body = json!({ "code": code, "name": name, "key": hash(&key) });
    match http::post(factory, "join", As::Nobody, &body, Duration::from_secs(30)) {
        Reply::Ok(_) => {
            let id = Identity {
                factory: http::host_port(factory).to_string(),
                name: name.to_string(),
                key,
            };
            write(file, &id)?;
            Ok(id)
        }
        Reply::No(_, word) => Err(format!("the factory refused the join: {word}")),
        Reply::Down(why) => Err(format!("the factory did not answer: {why}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;

    #[test]
    fn an_identity_is_kept_whole_and_private() {
        let d = TestDir::new("contractor-identity");
        let file = path(d.path());
        let id = Identity {
            factory: "192.0.2.7:8090".to_string(),
            name: "box1".to_string(),
            key: "k".repeat(64),
        };
        write(&file, &id).unwrap();
        assert_eq!(read(&file), Some(id));
        let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let dir = fs::metadata(file.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir, 0o700);
        assert_eq!(read(&d.path().join("none")), None);
    }

    #[test]
    fn a_key_is_new_each_time_and_its_hash_is_hex() {
        let (a, b) = (new_key().unwrap(), new_key().unwrap());
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
        assert_eq!(hash("abc").len(), 64);
        assert_eq!(
            hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
