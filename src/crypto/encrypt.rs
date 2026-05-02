use rand_core::{OsRng, RngCore};

use super::error::CryptoError;
use super::format::{SALT_LEN, build_v2_header};
use super::kdf::{Argon2idParams, derive_key};
use super::suite::{EncryptionSuite, encrypt_with_suite};
use super::types::ContentKind;
use super::validate_passphrase;

/// 使用默认加密套件加密任意 bytes，并返回完整的 `.encrust` 文件内容。
///
/// 文件加密和文本加密都走这条路径，避免格式分叉。
pub fn encrypt_bytes(
    plaintext: &[u8],
    passphrase: &str,
    kind: ContentKind,
    file_name: Option<&str>,
) -> Result<Vec<u8>, CryptoError> {
    encrypt_bytes_with_suite(
        plaintext,
        passphrase,
        kind,
        file_name,
        EncryptionSuite::Aes256Gcm,
    )
}

/// 使用指定加密套件加密任意 bytes。新文件默认写入 v2 自描述格式。
pub fn encrypt_bytes_with_suite(
    plaintext: &[u8],
    passphrase: &str,
    kind: ContentKind,
    file_name: Option<&str>,
    suite: EncryptionSuite,
) -> Result<Vec<u8>, CryptoError> {
    validate_passphrase(passphrase)?;

    let mut salt = vec![0_u8; SALT_LEN];
    let mut nonce = vec![0_u8; suite.nonce_len()];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);

    let kdf_params = Argon2idParams::default();
    let key = derive_key(passphrase, &salt, kdf_params)?;
    let header = build_v2_header(kind, file_name, suite, kdf_params, &salt, &nonce)?;
    let ciphertext = encrypt_with_suite(suite, key.as_slice(), &nonce, plaintext, &header)?;

    let mut output = Vec::with_capacity(header.len() + ciphertext.len());
    output.extend_from_slice(&header);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}
