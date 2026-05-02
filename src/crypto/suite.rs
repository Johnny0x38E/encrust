use aes_gcm::aead::consts::U12;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use sm4::Sm4;

use super::error::CryptoError;
use super::format::AES_GCM_NONCE_LEN;

const XCHACHA20_POLY1305_NONCE_LEN: usize = 24;
const SM4_GCM_KEY_LEN: usize = 16;

type Sm4Gcm = aes_gcm::AesGcm<Sm4, U12>;

/// Encrust 支持的 AEAD 加密套件。
///
/// 枚举顺序不用于文件格式，真正写入文件的是 `id()`。因此以后调整 UI 展示顺序
/// 不会影响旧文件解密，但不能随意复用已经发布过的 id。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionSuite {
    Aes256Gcm,
    XChaCha20Poly1305,
    Sm4Gcm,
}

impl EncryptionSuite {
    /// UI 下拉框可选择的加密方式。
    ///
    /// AES 保持第一位作为默认推荐项；解密不走这个列表，而是读取文件头里的 suite id。
    pub fn available_for_encryption() -> &'static [Self] {
        &[Self::Aes256Gcm, Self::XChaCha20Poly1305, Self::Sm4Gcm]
    }

    /// 面向用户展示的名称。这里不暴露内部 suite id，避免把文件格式细节混进 UI。
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Aes256Gcm => "AES-256-GCM（推荐）",
            Self::XChaCha20Poly1305 => "XChaCha20-Poly1305",
            Self::Sm4Gcm => "SM4-GCM（国密）",
        }
    }

    /// 写入 v2 文件头的稳定算法编号。
    pub(super) fn id(self) -> u8 {
        match self {
            Self::Aes256Gcm => 1,
            Self::XChaCha20Poly1305 => 2,
            Self::Sm4Gcm => 3,
        }
    }

    /// 从文件头里的稳定编号还原算法。
    pub(super) fn from_id(id: u8) -> Result<Self, CryptoError> {
        match id {
            1 => Ok(Self::Aes256Gcm),
            2 => Ok(Self::XChaCha20Poly1305),
            3 => Ok(Self::Sm4Gcm),
            _ => Err(CryptoError::UnsupportedSuite),
        }
    }

    /// 各套件要求的 nonce 长度不同，v2 文件头会记录并在解析时校验。
    pub(super) fn nonce_len(self) -> usize {
        match self {
            Self::Aes256Gcm => AES_GCM_NONCE_LEN,
            Self::XChaCha20Poly1305 => XCHACHA20_POLY1305_NONCE_LEN,
            Self::Sm4Gcm => AES_GCM_NONCE_LEN,
        }
    }
}

/// 使用指定 AEAD 套件加密明文。
///
/// `aad` 是未加密但必须认证的文件头。只要攻击者改动文件头里的算法、
/// 文件名、salt 或 nonce，AEAD 校验都会失败。
pub(super) fn encrypt_with_suite(
    suite: EncryptionSuite,
    key: &[u8],
    nonce: &[u8],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    match suite {
        EncryptionSuite::Aes256Gcm => {
            let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::Encryption)?;
            let nonce = Nonce::from_slice(nonce);
            cipher
                .encrypt(
                    nonce,
                    Payload {
                        msg: plaintext,
                        aad,
                    },
                )
                .map_err(|_| CryptoError::Encryption)
        }
        EncryptionSuite::XChaCha20Poly1305 => {
            let cipher =
                XChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoError::Encryption)?;
            let nonce = XNonce::from_slice(nonce);
            cipher
                .encrypt(
                    nonce,
                    Payload {
                        msg: plaintext,
                        aad,
                    },
                )
                .map_err(|_| CryptoError::Encryption)
        }
        EncryptionSuite::Sm4Gcm => {
            // Argon2id 统一派生 32 字节主密钥材料；SM4 是 128-bit 分组密码，
            // 因此固定取前 16 字节作为 SM4-GCM key。这个规则由 suite id 和
            // v2 文件头共同固定，未来不能悄悄改变。
            let sm4_key = key.get(..SM4_GCM_KEY_LEN).ok_or(CryptoError::Encryption)?;
            let cipher = Sm4Gcm::new_from_slice(sm4_key).map_err(|_| CryptoError::Encryption)?;
            let nonce = Nonce::from_slice(nonce);
            cipher
                .encrypt(
                    nonce,
                    Payload {
                        msg: plaintext,
                        aad,
                    },
                )
                .map_err(|_| CryptoError::Encryption)
        }
    }
}

/// 使用文件头里记录的套件解密密文。
///
/// 解密不接受 UI 传入的算法选择，避免用户选错算法，也保证旧文件能按自己的
/// 历史元数据自动走正确路径。
pub(super) fn decrypt_with_suite(
    suite: EncryptionSuite,
    key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    match suite {
        EncryptionSuite::Aes256Gcm => {
            let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::Decryption)?;
            let nonce = Nonce::from_slice(nonce);
            cipher
                .decrypt(
                    nonce,
                    Payload {
                        msg: ciphertext,
                        aad,
                    },
                )
                .map_err(|_| CryptoError::Decryption)
        }
        EncryptionSuite::XChaCha20Poly1305 => {
            let cipher =
                XChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoError::Decryption)?;
            let nonce = XNonce::from_slice(nonce);
            cipher
                .decrypt(
                    nonce,
                    Payload {
                        msg: ciphertext,
                        aad,
                    },
                )
                .map_err(|_| CryptoError::Decryption)
        }
        EncryptionSuite::Sm4Gcm => {
            let sm4_key = key.get(..SM4_GCM_KEY_LEN).ok_or(CryptoError::Decryption)?;
            let cipher = Sm4Gcm::new_from_slice(sm4_key).map_err(|_| CryptoError::Decryption)?;
            let nonce = Nonce::from_slice(nonce);
            cipher
                .decrypt(
                    nonce,
                    Payload {
                        msg: ciphertext,
                        aad,
                    },
                )
                .map_err(|_| CryptoError::Decryption)
        }
    }
}
