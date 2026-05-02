use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

use super::error::CryptoError;
use super::format::{KEY_LEN, V2_KDF_PARAMS_LEN, read_fixed};

const DEFAULT_ARGON2_MEMORY_KIB: u32 = 19 * 1024;
const DEFAULT_ARGON2_ITERATIONS: u32 = 2;
const DEFAULT_ARGON2_PARALLELISM: u32 = 1;

/// Argon2id 参数快照。
///
/// 参数会写入 v2 文件头，所以未来即使提高默认成本，也能继续用旧参数解密旧文件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Argon2idParams {
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    output_len: u16,
}

impl Default for Argon2idParams {
    fn default() -> Self {
        Self {
            memory_kib: DEFAULT_ARGON2_MEMORY_KIB,
            iterations: DEFAULT_ARGON2_ITERATIONS,
            parallelism: DEFAULT_ARGON2_PARALLELISM,
            output_len: KEY_LEN as u16,
        }
    }
}

impl Argon2idParams {
    /// 把 KDF 参数写入 v2 文件头，避免未来默认参数变化后无法还原旧文件。
    pub(super) fn encode(self) -> [u8; V2_KDF_PARAMS_LEN as usize] {
        let mut output = [0_u8; V2_KDF_PARAMS_LEN as usize];
        output[0..4].copy_from_slice(&self.memory_kib.to_be_bytes());
        output[4..8].copy_from_slice(&self.iterations.to_be_bytes());
        output[8..12].copy_from_slice(&self.parallelism.to_be_bytes());
        output[12..14].copy_from_slice(&self.output_len.to_be_bytes());
        output
    }

    pub(super) fn decode(input: &[u8]) -> Result<Self, CryptoError> {
        // v2 当前只接受固定长度参数块。格式扩展需要新版本号，而不是在同一版本里
        // 悄悄改变结构。
        if input.len() != V2_KDF_PARAMS_LEN as usize {
            return Err(CryptoError::InvalidFormat);
        }

        Ok(Self {
            memory_kib: u32::from_be_bytes(read_fixed::<4>(&input[0..4])?),
            iterations: u32::from_be_bytes(read_fixed::<4>(&input[4..8])?),
            parallelism: u32::from_be_bytes(read_fixed::<4>(&input[8..12])?),
            output_len: u16::from_be_bytes(read_fixed::<2>(&input[12..14])?),
        })
    }

    pub(super) fn output_len(self) -> u16 {
        self.output_len
    }
}

/// 使用 Argon2id 从用户输入的密钥短语派生 32 字节密钥。
///
/// 密钥短语不能直接作为 AES key：长度不固定，熵也不可控。KDF 会把短语和
/// 每个文件独立的随机 salt 转换成固定长度密钥，并提高暴力破解成本。
pub(super) fn derive_key(
    passphrase: &str,
    salt: &[u8],
    params: Argon2idParams,
) -> Result<Zeroizing<[u8; KEY_LEN]>, CryptoError> {
    let params = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(KEY_LEN),
    )
    .map_err(|_| CryptoError::KeyDerivation)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = Zeroizing::new([0_u8; KEY_LEN]);
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, key.as_mut())
        .map_err(|_| CryptoError::KeyDerivation)?;

    Ok(key)
}
