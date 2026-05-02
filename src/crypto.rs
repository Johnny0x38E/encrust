//! Encrust 加密模块的公开入口。
//!
//! 这个文件只负责组织模块和导出 UI/IO 层需要的 API。具体职责分别放在：
//! - `format`：`.encrust` 文件头的 v1/v2 编码和解析。
//! - `kdf`：Argon2id 参数和密钥派生。
//! - `suite`：具体 AEAD 加密套件实现。
//! - `encrypt` / `decrypt`：加密和解密流程编排。

mod decrypt;
mod encrypt;
mod error;
mod format;
mod kdf;
mod suite;
mod types;

pub use decrypt::{decrypt_bytes, inspect_encrypted_file};
pub use encrypt::{encrypt_bytes, encrypt_bytes_with_suite};
pub use error::CryptoError;
pub use suite::EncryptionSuite;
pub use types::{ContentKind, DecryptedPayload, EncryptedFileMetadata};

/// 用于 UI 层做轻量校验。
///
/// 这里按 Unicode 字符数量检查，而不是按字节数检查。这样中文、emoji 等输入
/// 不会因为 UTF-8 字节长度被误判。
pub fn validate_passphrase(passphrase: &str) -> Result<(), CryptoError> {
    if passphrase.chars().count() < format::MIN_PASSPHRASE_CHARS {
        return Err(CryptoError::PassphraseTooShort);
    }

    Ok(())
}

#[cfg(test)]
mod tests;
