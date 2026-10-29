//! 策略签名密钥的生成、加密存储和加载。

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use anchor_core::crypto;
use anyhow::{anyhow, bail, Context};
use ed25519_dalek::SigningKey;
use hkdf::Hkdf;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const KEY_FILE_VERSION: u32 = 1;
const KEY_FILE_ALGORITHM: &str = "AES-256-GCM";
const KEY_DERIVATION_INFO: &[u8] = b"anchor-guard/signing-key/v1";
const AAD_PREFIX: &str = "anchor-guard/signing-key/v1";

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MasterKey([u8; 32]);

impl MasterKey {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let encoded = fs::read_to_string(path).with_context(|| {
            format!(
                "read ANCHOR_MASTER_KEY_FILE {}; create it with 64 hex characters",
                path.display()
            )
        })?;
        let encoded = encoded.trim();
        if encoded.len() != 64 {
            bail!("ANCHOR_MASTER_KEY_FILE must contain exactly 64 hex characters");
        }
        let decoded = hex::decode(encoded)
            .context("ANCHOR_MASTER_KEY_FILE must contain exactly 64 hex characters")?;
        let key = decoded
            .try_into()
            .map_err(|_| anyhow!("ANCHOR_MASTER_KEY_FILE must decode to 32 bytes"))?;
        Ok(Self(key))
    }

    fn derive_encryption_key(&self) -> anyhow::Result<Zeroizing<[u8; 32]>> {
        let hkdf = Hkdf::<Sha256>::new(None, &self.0);
        let mut key = Zeroizing::new([0_u8; 32]);
        hkdf.expand(KEY_DERIVATION_INFO, key.as_mut())
            .map_err(|_| anyhow!("derive signing key encryption key"))?;
        Ok(key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSigningKey {
    pub key_id: String,
    pub public_key_hex: String,
    pub created_file: bool,
}

#[derive(Clone)]
pub struct SigningKeyStore {
    directory: Arc<PathBuf>,
    master_key: MasterKey,
}

impl SigningKeyStore {
    pub fn new(directory: PathBuf, master_key: MasterKey) -> Self {
        Self {
            directory: Arc::new(directory),
            master_key,
        }
    }

    pub fn store(&self, signing_key: &SigningKey) -> anyhow::Result<StoredSigningKey> {
        let verifying_key = signing_key.verifying_key();
        let public_key_hex = crypto::verifying_key_to_hex(&verifying_key);
        let key_id = crypto::sha256_hex(&verifying_key.to_bytes());
        let path = self.key_file_path(&key_id)?;

        if path.exists() {
            let existing = self.load(&key_id)?;
            if existing.to_bytes() == signing_key.to_bytes() {
                return Ok(StoredSigningKey {
                    key_id,
                    public_key_hex,
                    created_file: false,
                });
            }
            bail!(
                "signing key file {} belongs to different key material",
                path.display()
            );
        }

        self.prepare_directory()?;
        let encrypted = self.encrypt(&key_id, &public_key_hex, signing_key)?;
        let encoded = serde_json::to_vec_pretty(&encrypted)
            .context("serialize encrypted signing key file")?;
        write_atomic(&path, &encoded)?;
        let verified = self.load(&key_id).and_then(|loaded| {
            if loaded.to_bytes() == signing_key.to_bytes() {
                Ok(())
            } else {
                Err(anyhow!("encrypted signing key read-back mismatch"))
            }
        });
        if let Err(error) = verified {
            let _ = self.remove_file(&key_id);
            return Err(error);
        }

        Ok(StoredSigningKey {
            key_id,
            public_key_hex,
            created_file: true,
        })
    }

    pub fn load(&self, key_id: &str) -> anyhow::Result<SigningKey> {
        let path = self.key_file_path(key_id)?;
        let encoded =
            fs::read(&path).with_context(|| format!("read signing key {}", path.display()))?;
        let encrypted: EncryptedKeyFile = serde_json::from_slice(&encoded)
            .with_context(|| format!("parse signing key {}", path.display()))?;
        if encrypted.version != KEY_FILE_VERSION || encrypted.algorithm != KEY_FILE_ALGORITHM {
            bail!("unsupported signing key file format in {}", path.display());
        }
        if encrypted.key_id != key_id || !valid_key_id(&encrypted.public_key_hex) {
            bail!("signing key metadata mismatch in {}", path.display());
        }

        let verifying_key = crypto::verifying_key_from_hex(&encrypted.public_key_hex)
            .context("invalid public key in signing key file")?;
        let computed_key_id = crypto::sha256_hex(&verifying_key.to_bytes());
        if computed_key_id != key_id {
            bail!("signing key id does not match its public key");
        }

        let nonce = decode_fixed::<12>(&encrypted.nonce_hex, "nonce")?;
        let ciphertext = hex::decode(&encrypted.ciphertext_hex).context("decode ciphertext")?;
        let encryption_key = self.master_key.derive_encryption_key()?;
        let cipher = Aes256Gcm::new_from_slice(encryption_key.as_ref())
            .map_err(|_| anyhow!("initialize AES-256-GCM"))?;
        let aad = associated_data(key_id, &encrypted.public_key_hex);
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| anyhow!("decrypt signing key; check ANCHOR_MASTER_KEY_FILE"))?;
        let secret = Zeroizing::new(
            plaintext
                .try_into()
                .map_err(|_| anyhow!("decrypted signing key has invalid length"))?,
        );
        let signing_key = SigningKey::from_bytes(&secret);
        if signing_key.verifying_key().to_bytes() != verifying_key.to_bytes() {
            bail!("decrypted signing key does not match its public key");
        }
        Ok(signing_key)
    }

    pub fn remove_file(&self, key_id: &str) -> anyhow::Result<()> {
        let path = self.key_file_path(key_id)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("remove signing key file"),
        }
    }

    fn encrypt(
        &self,
        key_id: &str,
        public_key_hex: &str,
        signing_key: &SigningKey,
    ) -> anyhow::Result<EncryptedKeyFile> {
        let encryption_key = self.master_key.derive_encryption_key()?;
        let cipher = Aes256Gcm::new_from_slice(encryption_key.as_ref())
            .map_err(|_| anyhow!("initialize AES-256-GCM"))?;
        let mut nonce = [0_u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let secret = Zeroizing::new(signing_key.to_bytes());
        let aad = associated_data(key_id, public_key_hex);
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: secret.as_slice(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| anyhow!("encrypt signing key"))?;

        Ok(EncryptedKeyFile {
            version: KEY_FILE_VERSION,
            algorithm: KEY_FILE_ALGORITHM.into(),
            key_id: key_id.into(),
            public_key_hex: public_key_hex.into(),
            nonce_hex: hex::encode(nonce),
            ciphertext_hex: hex::encode(ciphertext),
        })
    }

    fn key_file_path(&self, key_id: &str) -> anyhow::Result<PathBuf> {
        if !valid_key_id(key_id) {
            bail!("invalid signing key id");
        }
        Ok(self.directory.join(format!("{key_id}.key")))
    }

    fn prepare_directory(&self) -> anyhow::Result<()> {
        fs::create_dir_all(self.directory.as_path()).with_context(|| {
            format!("create signing key directory {}", self.directory.display())
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(self.directory.as_path(), fs::Permissions::from_mode(0o700))
                .context("set signing key directory permissions")?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedKeyFile {
    version: u32,
    algorithm: String,
    key_id: String,
    public_key_hex: String,
    nonce_hex: String,
    ciphertext_hex: String,
}

fn associated_data(key_id: &str, public_key_hex: &str) -> String {
    format!("{AAD_PREFIX}:{key_id}:{public_key_hex}")
}

fn valid_key_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn decode_fixed<const N: usize>(encoded: &str, what: &str) -> anyhow::Result<[u8; N]> {
    let bytes = hex::decode(encoded).with_context(|| format!("decode {what}"))?;
    bytes
        .try_into()
        .map_err(|_| anyhow!("{what} has invalid length"))
}

fn write_atomic(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("invalid signing key path"))?;
    let temporary = path.with_file_name(format!(".{file_name}.{}.tmp", Uuid::new_v4()));

    let result = (|| -> anyhow::Result<()> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .with_context(|| format!("create temporary key file {}", temporary.display()))?;
        std::io::Write::write_all(&mut file, contents).context("write encrypted signing key")?;
        file.sync_all().context("sync encrypted signing key")?;
        fs::rename(&temporary, path).context("publish encrypted signing key")?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn master_key(directory: &Path, seed: u8) -> MasterKey {
        let path = directory.join(format!("master-{seed}.key"));
        fs::write(&path, format!("{}\n", hex::encode([seed; 32]))).unwrap();
        MasterKey::load(&path).unwrap()
    }

    #[test]
    fn master_key_requires_32_hex_encoded_bytes() {
        let temp = TempDir::new().unwrap();
        let short = temp.path().join("short.key");
        fs::write(&short, "abcd").unwrap();
        assert!(MasterKey::load(&short).is_err());

        let invalid = temp.path().join("invalid.key");
        fs::write(&invalid, "g".repeat(64)).unwrap();
        assert!(MasterKey::load(&invalid).is_err());
    }

    #[test]
    fn signing_key_round_trips_through_encrypted_file() {
        let temp = TempDir::new().unwrap();
        let store = SigningKeyStore::new(temp.path().join("keys"), master_key(temp.path(), 1));
        let (signing_key, _) = crypto::generate_keypair();

        let stored = store.store(&signing_key).unwrap();
        assert!(stored.created_file);
        assert_eq!(stored.key_id.len(), 64);
        assert_eq!(stored.public_key_hex.len(), 64);
        assert_eq!(
            store.load(&stored.key_id).unwrap().to_bytes(),
            signing_key.to_bytes()
        );

        let encoded = fs::read(
            temp.path()
                .join("keys")
                .join(format!("{}.key", stored.key_id)),
        )
        .unwrap();
        assert!(
            !String::from_utf8_lossy(&encoded).contains(&crypto::signing_key_to_hex(&signing_key))
        );
    }

    #[test]
    fn storing_the_same_key_is_idempotent() {
        let temp = TempDir::new().unwrap();
        let store = SigningKeyStore::new(temp.path().join("keys"), master_key(temp.path(), 1));
        let (signing_key, _) = crypto::generate_keypair();

        let first = store.store(&signing_key).unwrap();
        let second = store.store(&signing_key).unwrap();

        assert!(first.created_file);
        assert!(!second.created_file);
        assert_eq!(first.key_id, second.key_id);
    }

    #[test]
    fn wrong_master_key_cannot_decrypt_signing_key() {
        let temp = TempDir::new().unwrap();
        let (signing_key, _) = crypto::generate_keypair();
        let first = SigningKeyStore::new(temp.path().join("keys"), master_key(temp.path(), 1));
        let stored = first.store(&signing_key).unwrap();

        let second = SigningKeyStore::new(temp.path().join("keys"), master_key(temp.path(), 2));
        assert!(second.load(&stored.key_id).is_err());
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let temp = TempDir::new().unwrap();
        let store = SigningKeyStore::new(temp.path().join("keys"), master_key(temp.path(), 1));
        let (signing_key, _) = crypto::generate_keypair();
        let stored = store.store(&signing_key).unwrap();
        let path = temp
            .path()
            .join("keys")
            .join(format!("{}.key", stored.key_id));
        let mut file: EncryptedKeyFile = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let replacement = if file.ciphertext_hex.starts_with('0') {
            "1"
        } else {
            "0"
        };
        file.ciphertext_hex.replace_range(..1, replacement);
        fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();

        assert!(store.load(&stored.key_id).is_err());
    }
}
