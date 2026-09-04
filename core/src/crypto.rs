//! WhatsApp encrypted backup decryption (`msgstore.db.crypt12/14/15`).
//!
//! Layout details differ slightly between crypt versions and WhatsApp
//! releases, so instead of hardcoding a single layout the decryptor tries a
//! small set of documented candidates (key offsets inside the key file, IV
//! offsets inside the crypt file) and accepts the first combination whose
//! plaintext starts with the SQLite magic header. crypt12+ backups use
//! AES-256-GCM with a 16-byte IV and a 16-byte authentication tag appended
//! after the ciphertext.

use crate::error::{CoreError, Result};
use aes_gcm::{aead::Aead, Aes256Gcm, AesGcm, KeyInit};
use generic_array::typenum::U16;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// AES-256-GCM with a 16-byte nonce (the documented crypt12+ layout).
type Aes256Gcm16 = AesGcm<aes_gcm::aes::Aes256, U16>;

const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";
const TAG_LEN: usize = 16;

/// Candidate offsets of the AES key inside the WhatsApp key file.
/// 126 is the documented offset for classic 158-byte key files; later
/// versions store longer key files where the key sits further back.
const KEY_OFFSETS: &[usize] = &[126, 158, 190];

/// Candidate IV offsets inside the crypt file. The ciphertext always starts
/// immediately after the IV.
const IV_OFFSETS: &[usize] = &[51, 67, 83, 99];

/// Try one key/IV combination; returns the plaintext when it starts with the
/// SQLite magic header. Primary mode is AES-256-GCM with the full 16-byte IV
/// (crypt12+); plain 12-byte-nonce GCM is tried as a fallback.
fn try_decrypt(data: &[u8], key: &[u8], iv_off: usize, payload: &[u8]) -> Option<Vec<u8>> {
    if data.len() >= iv_off + 16 {
        if let Ok(cipher) = Aes256Gcm16::new_from_slice(key) {
            let nonce = generic_array::GenericArray::from_slice(&data[iv_off..iv_off + 16]);
            if let Ok(plain) = cipher.decrypt(nonce, payload) {
                if plain.starts_with(SQLITE_MAGIC) {
                    return Some(plain);
                }
            }
        }
    }
    if data.len() >= iv_off + 12 {
        if let Ok(cipher) = Aes256Gcm::new_from_slice(key) {
            let nonce = aes_gcm::Nonce::from_slice(&data[iv_off..iv_off + 12]);
            if let Ok(plain) = cipher.decrypt(nonce, payload) {
                if plain.starts_with(SQLITE_MAGIC) {
                    return Some(plain);
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptOutcome {
    pub output: String,
    /// `plain` (input was already an SQLite db), or the guessed crypt format.
    pub format: String,
    pub key_offset: usize,
    pub iv_offset: usize,
    pub plaintext_size: u64,
}

/// Decrypt an encrypted WhatsApp backup using the device key file.
///
/// If `input` is already a plain SQLite database it is simply copied to
/// `output` (reported as format `plain`).
pub fn decrypt_backup(input: &Path, key_file: &Path, output: &Path) -> Result<DecryptOutcome> {
    let data = std::fs::read(input)?;
    if data.len() >= 16 && data.starts_with(SQLITE_MAGIC) {
        std::fs::write(output, &data)?;
        return Ok(DecryptOutcome {
            output: output.display().to_string(),
            format: "plain".into(),
            key_offset: 0,
            iv_offset: 0,
            plaintext_size: data.len() as u64,
        });
    }

    let key_buf = std::fs::read(key_file).map_err(|e| {
        CoreError::Decrypt(format!("cannot read key file {}: {e}", key_file.display()))
    })?;
    if key_buf.len() < 32 {
        return Err(CoreError::Decrypt(
            "key file is too small to contain a WhatsApp key".into(),
        ));
    }

    // Candidate key offsets, including "last 32 bytes" for unknown versions.
    let mut key_offsets: Vec<usize> = KEY_OFFSETS
        .iter()
        .copied()
        .filter(|&o| key_buf.len() >= o + 32)
        .collect();
    if key_buf.len() >= 32 {
        key_offsets.push(key_buf.len() - 32);
    }
    key_offsets.sort_unstable();
    key_offsets.dedup();

    for &ko in &key_offsets {
        for &ivo in IV_OFFSETS {
            if data.len() < ivo + TAG_LEN + TAG_LEN {
                continue;
            }
            let ct = &data[ivo + TAG_LEN..data.len() - TAG_LEN];
            let tag = &data[data.len() - TAG_LEN..];
            let mut payload = Vec::with_capacity(ct.len() + TAG_LEN);
            payload.extend_from_slice(ct);
            payload.extend_from_slice(tag);
            if let Some(plain) = try_decrypt(&data, &key_buf[ko..ko + 32], ivo, &payload) {
                std::fs::write(output, &plain)?;
                return Ok(DecryptOutcome {
                    output: output.display().to_string(),
                    format: guess_format(input, ivo),
                    key_offset: ko,
                    iv_offset: ivo,
                    plaintext_size: plain.len() as u64,
                });
            }
        }
    }

    Err(CoreError::Decrypt(
        "no key/IV combination produced a valid SQLite database; \
         verify the key file belongs to this backup (same device/install)"
            .into(),
    ))
}

fn guess_format(input: &Path, iv_offset: usize) -> String {
    let name = input
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    for v in ["crypt15", "crypt14", "crypt13", "crypt12"] {
        if name.contains(v) {
            return v.to_string();
        }
    }
    format!("crypt? (iv@{iv_offset})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes_gcm::aead::AeadInPlace;

    /// Build a synthetic crypt12-style file (IV at 51, tag at end) and verify
    /// the decryptor finds it.
    #[test]
    fn decrypt_roundtrip_synthetic_crypt12() {
        // 32-byte key at offset 126 inside a 158-byte key file.
        let key: [u8; 32] = std::array::from_fn(|i| i as u8);
        let mut key_file = vec![0u8; 158];
        key_file[126..158].copy_from_slice(&key);

        // Plaintext: SQLite magic + filler.
        let mut plain = SQLITE_MAGIC.to_vec();
        plain.extend_from_slice(&[7u8; 4096]);

        // Header: 51 bytes + 16-byte IV at offset 51 => ciphertext starts at 67.
        let iv: [u8; 16] = std::array::from_fn(|i| 200 - i as u8);
        let mut blob = vec![0u8; 51];
        blob.extend_from_slice(&iv);

        let cipher = Aes256Gcm16::new_from_slice(&key).unwrap();
        let nonce = generic_array::GenericArray::from_slice(&iv);
        let mut buf = plain.clone();
        cipher
            .encrypt_in_place(nonce, b"", &mut buf)
            .expect("encrypt");
        blob.extend_from_slice(&buf); // ciphertext + 16-byte tag

        let dir = std::env::temp_dir().join(format!("rc-crypto-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let enc = dir.join("msgstore.db.crypt12");
        let kf = dir.join("key");
        let out = dir.join("msgstore.db");
        std::fs::write(&enc, &blob).unwrap();
        std::fs::write(&kf, &key_file).unwrap();

        let outcome = decrypt_backup(&enc, &kf, &out).expect("decrypt should succeed");
        assert_eq!(outcome.format, "crypt12");
        assert_eq!(outcome.iv_offset, 51);
        assert_eq!(outcome.key_offset, 126);
        let decrypted = std::fs::read(&out).unwrap();
        assert_eq!(decrypted, plain);
    }

    #[test]
    fn plain_sqlite_passes_through() {
        let dir = std::env::temp_dir().join(format!("rc-crypto2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("plain.db");
        let kf = dir.join("key");
        let out = dir.join("out.db");
        let mut plain = SQLITE_MAGIC.to_vec();
        plain.extend_from_slice(&[0u8; 64]);
        std::fs::write(&input, &plain).unwrap();
        std::fs::write(&kf, vec![0u8; 158]).unwrap();

        let outcome = decrypt_backup(&input, &kf, &out).unwrap();
        assert_eq!(outcome.format, "plain");
        assert_eq!(std::fs::read(&out).unwrap(), plain);
    }

    #[test]
    fn wrong_key_fails() {
        let key: [u8; 32] = [9; 32];
        let mut key_file = vec![0u8; 158];
        key_file[126..158].copy_from_slice(&key);

        let mut plain = SQLITE_MAGIC.to_vec();
        plain.extend_from_slice(&[3u8; 128]);
        let iv = [5u8; 16];
        let mut blob = vec![0u8; 51];
        blob.extend_from_slice(&iv);
        let cipher = Aes256Gcm16::new_from_slice(&key).unwrap();
        let nonce = generic_array::GenericArray::from_slice(&iv);
        let mut buf = plain.clone();
        cipher.encrypt_in_place(nonce, b"", &mut buf).unwrap();
        blob.extend_from_slice(&buf);

        let dir = std::env::temp_dir().join(format!("rc-crypto3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let enc = dir.join("x.crypt12");
        let kf = dir.join("wrongkey");
        let out = dir.join("no.db");
        std::fs::write(&enc, &blob).unwrap();
        // Different key -> every candidate must fail.
        key_file[126] ^= 0xff;
        std::fs::write(&kf, &key_file).unwrap();

        assert!(decrypt_backup(&enc, &kf, &out).is_err());
    }
}
