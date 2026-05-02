use rand_core::{OsRng, RngCore};

use super::error::CryptoError;
use super::format::{
    AES_GCM_NONCE_LEN, CURRENT_VERSION, KDF_ARGON2ID, LEGACY_CIPHER_AES_256_GCM, LEGACY_VERSION_V1,
    MAGIC, MIN_HEADER_LEN, SALT_LEN, kind_to_byte,
};
use super::kdf::{Argon2idParams, derive_key};
use super::suite::{EncryptionSuite, encrypt_with_suite};
use super::types::ContentKind;
use super::{
    decrypt_bytes, encrypt_bytes, encrypt_bytes_with_suite, inspect_encrypted_file,
    validate_passphrase,
};

#[test]
fn rejects_short_passphrase() {
    let err = encrypt_bytes(b"hello", "short", ContentKind::Text, None)
        .expect_err("short passphrase must fail");
    assert!(matches!(err, CryptoError::PassphraseTooShort));
}

#[test]
fn output_starts_with_expected_header_fields() {
    let encrypted = encrypt_bytes(
        b"hello",
        "correct horse battery staple",
        ContentKind::Text,
        None,
    )
    .unwrap();

    let metadata = inspect_encrypted_file(&encrypted).unwrap();
    assert_eq!(&encrypted[..MAGIC.len()], MAGIC);
    assert_eq!(metadata.format_version, CURRENT_VERSION);
    assert_eq!(metadata.suite, EncryptionSuite::Aes256Gcm);
    assert_eq!(metadata.kind, ContentKind::Text);
    assert!(encrypted.len() > MIN_HEADER_LEN);
}

#[test]
fn same_plaintext_encrypts_to_different_outputs() {
    let first = encrypt_bytes(
        b"repeatable input",
        "correct horse battery staple",
        ContentKind::Text,
        None,
    )
    .unwrap();
    let second = encrypt_bytes(
        b"repeatable input",
        "correct horse battery staple",
        ContentKind::Text,
        None,
    )
    .unwrap();

    // salt 和 nonce 每次都随机，所以完整输出不应相同。
    assert_ne!(first, second);
}

#[test]
fn encrypts_binary_and_utf8_data() {
    let binary = [0_u8, 159, 146, 150, 255, 10];
    let text = "你好，Rust encryption!";

    assert!(
        encrypt_bytes(
            &binary,
            "correct horse battery staple",
            ContentKind::File,
            Some("sample.bin"),
        )
        .is_ok()
    );
    assert!(
        encrypt_bytes(
            text.as_bytes(),
            "correct horse battery staple",
            ContentKind::Text,
            None,
        )
        .is_ok()
    );
}

#[test]
fn decrypts_text_payload() {
    let encrypted = encrypt_bytes(
        "hello rust".as_bytes(),
        "correct horse battery staple",
        ContentKind::Text,
        None,
    )
    .unwrap();

    let decrypted = decrypt_bytes(&encrypted, "correct horse battery staple").unwrap();

    assert_eq!(decrypted.kind, ContentKind::Text);
    assert_eq!(decrypted.plaintext, b"hello rust");
}

#[test]
fn decrypts_file_payload_with_original_name() {
    let encrypted = encrypt_bytes(
        b"file bytes",
        "correct horse battery staple",
        ContentKind::File,
        Some("report.pdf"),
    )
    .unwrap();

    let decrypted = decrypt_bytes(&encrypted, "correct horse battery staple").unwrap();

    assert_eq!(decrypted.kind, ContentKind::File);
    assert_eq!(decrypted.file_name.as_deref(), Some("report.pdf"));
    assert_eq!(decrypted.plaintext, b"file bytes");
}

#[test]
fn decrypt_rejects_wrong_passphrase() {
    let encrypted = encrypt_bytes(
        b"secret",
        "correct horse battery staple",
        ContentKind::Text,
        None,
    )
    .unwrap();

    let err = decrypt_bytes(&encrypted, "wrong horse battery staple").unwrap_err();

    assert!(matches!(err, CryptoError::Decryption));
}

#[test]
fn default_encryption_writes_v2_header() {
    let encrypted = encrypt_bytes(
        b"hello v2",
        "correct horse battery staple",
        ContentKind::Text,
        None,
    )
    .unwrap();

    assert_eq!(&encrypted[..MAGIC.len()], MAGIC);
    assert_eq!(encrypted[MAGIC.len()], CURRENT_VERSION);
}

#[test]
fn encrypts_with_selected_suite() {
    for suite in [
        EncryptionSuite::Aes256Gcm,
        EncryptionSuite::XChaCha20Poly1305,
        EncryptionSuite::Sm4Gcm,
    ] {
        let encrypted = encrypt_bytes_with_suite(
            b"selected suite",
            "correct horse battery staple",
            ContentKind::Text,
            None,
            suite,
        )
        .unwrap();

        let metadata = inspect_encrypted_file(&encrypted).unwrap();
        assert_eq!(metadata.format_version, CURRENT_VERSION);
        assert_eq!(metadata.suite, suite);

        let decrypted = decrypt_bytes(&encrypted, "correct horse battery staple").unwrap();
        assert_eq!(decrypted.plaintext, b"selected suite");
    }
}

#[test]
fn decrypts_legacy_v1_payloads() {
    let encrypted = encrypt_bytes_v1_for_test(
        b"legacy text",
        "correct horse battery staple",
        ContentKind::Text,
        None,
    )
    .unwrap();

    assert_eq!(encrypted[MAGIC.len()], LEGACY_VERSION_V1);

    let decrypted = decrypt_bytes(&encrypted, "correct horse battery staple").unwrap();
    assert_eq!(decrypted.kind, ContentKind::Text);
    assert_eq!(decrypted.plaintext, b"legacy text");
}

#[test]
fn available_suites_include_aes_default() {
    let suites = EncryptionSuite::available_for_encryption();

    assert_eq!(suites[0], EncryptionSuite::Aes256Gcm);
    assert_eq!(
        suites,
        &[
            EncryptionSuite::Aes256Gcm,
            EncryptionSuite::XChaCha20Poly1305,
            EncryptionSuite::Sm4Gcm
        ]
    );
}

fn encrypt_bytes_v1_for_test(
    plaintext: &[u8],
    passphrase: &str,
    kind: ContentKind,
    file_name: Option<&str>,
) -> Result<Vec<u8>, CryptoError> {
    // 生产代码已经只写 v2。这里保留一个测试专用 v1 写入器，用来持续验证
    // “未来版本仍能解开旧文件”的兼容承诺。
    validate_passphrase(passphrase)?;

    let mut salt = [0_u8; SALT_LEN];
    let mut nonce = [0_u8; AES_GCM_NONCE_LEN];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);

    let key = derive_key(passphrase, &salt, Argon2idParams::default())?;
    let header = build_v1_header_for_test(kind, file_name, &salt, &nonce)?;
    let ciphertext = encrypt_with_suite(
        EncryptionSuite::Aes256Gcm,
        key.as_slice(),
        &nonce,
        plaintext,
        &header,
    )?;

    let mut output = Vec::with_capacity(header.len() + ciphertext.len());
    output.extend_from_slice(&header);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

fn build_v1_header_for_test(
    kind: ContentKind,
    file_name: Option<&str>,
    salt: &[u8; SALT_LEN],
    nonce: &[u8; AES_GCM_NONCE_LEN],
) -> Result<Vec<u8>, CryptoError> {
    let file_name_bytes = file_name.unwrap_or_default().as_bytes();
    let file_name_len =
        u16::try_from(file_name_bytes.len()).map_err(|_| CryptoError::FileNameTooLong)?;

    let mut output = Vec::with_capacity(MIN_HEADER_LEN + file_name_bytes.len());
    output.extend_from_slice(MAGIC);
    output.push(LEGACY_VERSION_V1);
    output.push(KDF_ARGON2ID);
    output.push(LEGACY_CIPHER_AES_256_GCM);
    output.push(kind_to_byte(kind));
    output.extend_from_slice(&file_name_len.to_be_bytes());
    output.extend_from_slice(file_name_bytes);
    output.extend_from_slice(salt);
    output.extend_from_slice(nonce);
    Ok(output)
}
