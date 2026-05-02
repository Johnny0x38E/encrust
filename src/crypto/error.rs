use thiserror::Error;

use super::format::MIN_PASSPHRASE_CHARS;

/// 加密模块对外暴露的错误类型。
///
/// 错误文案可以直接给 UI 展示，但不会区分“密码错”和“密文被篡改”这类细节，
/// 避免给攻击者提供额外判断信息。
#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("密钥长度不能少于 {MIN_PASSPHRASE_CHARS} 个字符")]
    PassphraseTooShort,
    #[error("密钥派生失败")]
    KeyDerivation,
    #[error("加密失败")]
    Encryption,
    #[error("解密失败：密钥错误或文件被篡改")]
    Decryption,
    #[error("不是有效的 Encrust 加密文件")]
    InvalidFormat,
    #[error("不支持的 Encrust 文件版本")]
    UnsupportedVersion,
    #[error("不支持的加密方式")]
    UnsupportedSuite,
    #[error("原文件名太长，无法写入当前文件格式")]
    FileNameTooLong,
}
