use super::error::CryptoError;
use super::format::parse_header;
use super::kdf::derive_key;
use super::suite::decrypt_with_suite;
use super::types::{DecryptedPayload, EncryptedFileMetadata};
use super::validate_passphrase;

/// 解密完整的 `.encrust` 文件内容。v1 和 v2 都会自动识别。
///
/// 这里的关键点是：解密只信任文件头里的版本和套件信息，不使用 UI 当前选择。
/// 这保证了未来默认算法改变后，旧文件仍能按旧格式解密。
pub fn decrypt_bytes(
    encrypted_file: &[u8],
    passphrase: &str,
) -> Result<DecryptedPayload, CryptoError> {
    validate_passphrase(passphrase)?;

    let parsed = parse_header(encrypted_file)?;
    let key = derive_key(passphrase, &parsed.salt, parsed.kdf_params)?;
    let ciphertext = &encrypted_file[parsed.header_len..];
    let plaintext = decrypt_with_suite(
        parsed.suite,
        key.as_slice(),
        &parsed.nonce,
        ciphertext,
        &encrypted_file[..parsed.header_len],
    )?;

    Ok(DecryptedPayload {
        kind: parsed.kind,
        file_name: parsed.file_name,
        plaintext,
    })
}

/// 读取文件头元数据，不需要密钥短语。
///
/// UI 可用它显示“文件格式 / 加密方式”，但真正解密仍由 `decrypt_bytes`
/// 再次解析文件头，避免元数据和密文被分开传递时产生不一致。
pub fn inspect_encrypted_file(input: &[u8]) -> Result<EncryptedFileMetadata, CryptoError> {
    let parsed = parse_header(input)?;
    Ok(EncryptedFileMetadata {
        format_version: parsed.version,
        suite: parsed.suite,
        kind: parsed.kind,
    })
}
