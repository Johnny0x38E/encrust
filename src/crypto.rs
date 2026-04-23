use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use rand_core::{OsRng, RngCore};
use thiserror::Error;
use zeroize::Zeroizing;

/// 加密文件的魔数。
///
/// 魔数的作用类似文件签名：解密时先读取文件开头，判断它是不是 Encrust
/// 生成的文件，而不是盲目尝试解密任意文件。
const MAGIC: &[u8; 7] = b"ENCRUST";
const VERSION: u8 = 1;
const KDF_ARGON2ID: u8 = 1;
const CIPHER_AES_256_GCM: u8 = 1;
const CONTENT_FILE: u8 = 1;
const CONTENT_TEXT: u8 = 2;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;
const MIN_PASSPHRASE_CHARS: usize = 8;

/// 当前版本固定头部的最小长度。
///
/// 文件格式：
/// - 7 字节 magic: ENCRUST
/// - 1 字节 version
/// - 1 字节 KDF 标识
/// - 1 字节 cipher 标识
/// - 1 字节内容类型：文件或文本
/// - 2 字节原文件名长度，使用 big-endian u16
/// - N 字节原文件名，UTF-8；文本加密时 N 为 0
/// - 16 字节 salt
/// - 12 字节 nonce
/// - 剩余部分 ciphertext
///
/// 注意：header 会作为 AES-GCM 的 AAD 参与认证。AAD 不会被加密，
/// 但会被认证；任何人篡改 header 都会导致解密失败。
pub const MIN_HEADER_LEN: usize = MAGIC.len() + 1 + 1 + 1 + 1 + 2 + SALT_LEN + NONCE_LEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    File,
    Text,
}

#[derive(Debug, Clone)]
pub struct DecryptedPayload {
    pub kind: ContentKind,
    pub file_name: Option<String>,
    pub plaintext: Vec<u8>,
}

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
    #[error("原文件名太长，无法写入当前文件格式")]
    FileNameTooLong,
}

/// 用于 UI 层做轻量校验。
///
/// 这里按 Unicode 字符数量检查，而不是按字节数检查。这样中文、emoji 等输入
/// 对用户来说更符合“字符长度”的直觉。
pub fn validate_passphrase(passphrase: &str) -> Result<(), CryptoError> {
    if passphrase.chars().count() < MIN_PASSPHRASE_CHARS {
        return Err(CryptoError::PassphraseTooShort);
    }

    Ok(())
}

/// 加密任意 bytes，并返回完整的 `.encrust` 文件内容。
///
/// 文件加密和文本加密都会调用这个函数。这样 UI 不需要关心密码学细节，
/// 测试也可以直接覆盖核心逻辑。
pub fn encrypt_bytes(plaintext: &[u8], passphrase: &str, kind: ContentKind, file_name: Option<&str>) -> Result<Vec<u8>, CryptoError> {
    validate_passphrase(passphrase)?;

    let mut salt = [0_u8; SALT_LEN];
    let mut nonce_bytes = [0_u8; NONCE_LEN];

    // OsRng 使用操作系统提供的安全随机数。salt 和 nonce 必须每次加密都重新生成。
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce_bytes);

    let key = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(key.as_slice()).map_err(|_| CryptoError::Encryption)?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let header = build_header(kind, file_name, &salt, &nonce_bytes)?;
    let ciphertext = cipher.encrypt(nonce, Payload { msg: plaintext, aad: &header }).map_err(|_| CryptoError::Encryption)?;

    // 提前分配容量，避免 Vec 在 push/extend 时多次扩容。这里不是性能关键，
    // 但这是 Rust 中处理二进制格式时常见的写法。
    let mut output = Vec::with_capacity(header.len() + ciphertext.len());
    output.extend_from_slice(&header);
    output.extend_from_slice(&ciphertext);

    Ok(output)
}

/// 解密完整的 `.encrust` 文件内容。
///
/// 返回值包含内容类型和明文 bytes。UI 层再根据 `ContentKind` 决定：
/// - Text：按 UTF-8 展示并提供复制。
/// - File：提供保存到文件路径的功能。
pub fn decrypt_bytes(encrypted_file: &[u8], passphrase: &str) -> Result<DecryptedPayload, CryptoError> {
    validate_passphrase(passphrase)?;

    let parsed = parse_header(encrypted_file)?;
    let key = derive_key(passphrase, &parsed.salt)?;
    let cipher = Aes256Gcm::new_from_slice(key.as_slice()).map_err(|_| CryptoError::Decryption)?;
    let nonce = Nonce::from_slice(&parsed.nonce);
    let ciphertext = &encrypted_file[parsed.header_len..];
    let plaintext =
        cipher.decrypt(nonce, Payload { msg: ciphertext, aad: &encrypted_file[..parsed.header_len] }).map_err(|_| CryptoError::Decryption)?;

    Ok(DecryptedPayload { kind: parsed.kind, file_name: parsed.file_name, plaintext })
}

fn build_header(kind: ContentKind, file_name: Option<&str>, salt: &[u8; SALT_LEN], nonce_bytes: &[u8; NONCE_LEN]) -> Result<Vec<u8>, CryptoError> {
    let kind_byte = match kind {
        ContentKind::File => CONTENT_FILE,
        ContentKind::Text => CONTENT_TEXT,
    };
    let file_name_bytes = file_name.unwrap_or_default().as_bytes();
    let file_name_len = u16::try_from(file_name_bytes.len()).map_err(|_| CryptoError::FileNameTooLong)?;

    let mut output = Vec::with_capacity(MIN_HEADER_LEN + file_name_bytes.len());
    output.extend_from_slice(MAGIC);
    output.push(VERSION);
    output.push(KDF_ARGON2ID);
    output.push(CIPHER_AES_256_GCM);
    output.push(kind_byte);
    output.extend_from_slice(&file_name_len.to_be_bytes());
    output.extend_from_slice(file_name_bytes);
    output.extend_from_slice(salt);
    output.extend_from_slice(nonce_bytes);

    Ok(output)
}

struct ParsedHeader {
    kind: ContentKind,
    file_name: Option<String>,
    salt: [u8; SALT_LEN],
    nonce: [u8; NONCE_LEN],
    header_len: usize,
}

fn parse_header(input: &[u8]) -> Result<ParsedHeader, CryptoError> {
    if input.len() < MIN_HEADER_LEN || &input[..MAGIC.len()] != MAGIC {
        return Err(CryptoError::InvalidFormat);
    }

    let mut cursor = MAGIC.len();
    let version = read_u8(input, &mut cursor)?;
    if version != VERSION {
        return Err(CryptoError::UnsupportedVersion);
    }

    let kdf = read_u8(input, &mut cursor)?;
    let cipher = read_u8(input, &mut cursor)?;
    if kdf != KDF_ARGON2ID || cipher != CIPHER_AES_256_GCM {
        return Err(CryptoError::InvalidFormat);
    }

    let kind = match read_u8(input, &mut cursor)? {
        CONTENT_FILE => ContentKind::File,
        CONTENT_TEXT => ContentKind::Text,
        _ => return Err(CryptoError::InvalidFormat),
    };

    let file_name_len = read_u16(input, &mut cursor)? as usize;
    let file_name_bytes = read_slice(input, &mut cursor, file_name_len)?;
    let file_name =
        if file_name_bytes.is_empty() { None } else { Some(String::from_utf8(file_name_bytes.to_vec()).map_err(|_| CryptoError::InvalidFormat)?) };

    let salt = read_array::<SALT_LEN>(input, &mut cursor)?;
    let nonce = read_array::<NONCE_LEN>(input, &mut cursor)?;

    if input.len() <= cursor {
        return Err(CryptoError::InvalidFormat);
    }

    Ok(ParsedHeader { kind, file_name, salt, nonce, header_len: cursor })
}

fn read_u8(input: &[u8], cursor: &mut usize) -> Result<u8, CryptoError> {
    let value = *input.get(*cursor).ok_or(CryptoError::InvalidFormat)?;
    *cursor += 1;
    Ok(value)
}

fn read_u16(input: &[u8], cursor: &mut usize) -> Result<u16, CryptoError> {
    let bytes = read_array::<2>(input, cursor)?;
    Ok(u16::from_be_bytes(bytes))
}

fn read_array<const N: usize>(input: &[u8], cursor: &mut usize) -> Result<[u8; N], CryptoError> {
    let slice = read_slice(input, cursor, N)?;
    let mut array = [0_u8; N];
    array.copy_from_slice(slice);
    Ok(array)
}

fn read_slice<'a>(input: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], CryptoError> {
    let end = cursor.checked_add(len).ok_or(CryptoError::InvalidFormat)?;
    let slice = input.get(*cursor..end).ok_or(CryptoError::InvalidFormat)?;
    *cursor = end;
    Ok(slice)
}

/// 使用 Argon2id 从用户输入的密钥短语派生 AES-256 需要的 32 字节密钥。
///
/// 用户输入通常不适合直接作为加密 key：长度不固定，熵也不可控。
/// KDF 的职责是把用户输入和随机 salt 转换成固定长度、抗暴力破解成本更高的 key。
fn derive_key(passphrase: &str, salt: &[u8; SALT_LEN]) -> Result<Zeroizing<[u8; KEY_LEN]>, CryptoError> {
    // 这些参数面向桌面学习项目，兼顾安全性和本机响应速度。
    // 当前通过 VERSION 固定参数；未来如果要调参，应升级文件格式版本。
    let params = Params::new(19 * 1024, 2, 1, Some(KEY_LEN)).map_err(|_| CryptoError::KeyDerivation)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = Zeroizing::new([0_u8; KEY_LEN]);
    argon2.hash_password_into(passphrase.as_bytes(), salt, key.as_mut()).map_err(|_| CryptoError::KeyDerivation)?;

    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_passphrase() {
        let err = encrypt_bytes(b"hello", "short", ContentKind::Text, None).expect_err("short passphrase must fail");
        assert!(matches!(err, CryptoError::PassphraseTooShort));
    }

    #[test]
    fn output_starts_with_expected_header_fields() {
        let encrypted = encrypt_bytes(b"hello", "correct horse battery staple", ContentKind::Text, None).unwrap();

        assert_eq!(&encrypted[..MAGIC.len()], MAGIC);
        assert_eq!(encrypted[MAGIC.len()], VERSION);
        assert_eq!(encrypted[MAGIC.len() + 1], KDF_ARGON2ID);
        assert_eq!(encrypted[MAGIC.len() + 2], CIPHER_AES_256_GCM);
        assert_eq!(encrypted[MAGIC.len() + 3], CONTENT_TEXT);
        assert!(encrypted.len() > MIN_HEADER_LEN);
    }

    #[test]
    fn same_plaintext_encrypts_to_different_outputs() {
        let first = encrypt_bytes(b"repeatable input", "correct horse battery staple", ContentKind::Text, None).unwrap();
        let second = encrypt_bytes(b"repeatable input", "correct horse battery staple", ContentKind::Text, None).unwrap();

        // salt 和 nonce 每次都随机，所以完整输出不应相同。
        assert_ne!(first, second);
    }

    #[test]
    fn encrypts_binary_and_utf8_data() {
        let binary = [0_u8, 159, 146, 150, 255, 10];
        let text = "你好，Rust encryption!";

        assert!(encrypt_bytes(&binary, "correct horse battery staple", ContentKind::File, Some("sample.bin"),).is_ok());
        assert!(encrypt_bytes(text.as_bytes(), "correct horse battery staple", ContentKind::Text, None,).is_ok());
    }

    #[test]
    fn decrypts_text_payload() {
        let encrypted = encrypt_bytes("hello rust".as_bytes(), "correct horse battery staple", ContentKind::Text, None).unwrap();

        let decrypted = decrypt_bytes(&encrypted, "correct horse battery staple").unwrap();

        assert_eq!(decrypted.kind, ContentKind::Text);
        assert_eq!(decrypted.plaintext, b"hello rust");
    }

    #[test]
    fn decrypts_file_payload_with_original_name() {
        let encrypted = encrypt_bytes(b"file bytes", "correct horse battery staple", ContentKind::File, Some("report.pdf")).unwrap();

        let decrypted = decrypt_bytes(&encrypted, "correct horse battery staple").unwrap();

        assert_eq!(decrypted.kind, ContentKind::File);
        assert_eq!(decrypted.file_name.as_deref(), Some("report.pdf"));
        assert_eq!(decrypted.plaintext, b"file bytes");
    }

    #[test]
    fn decrypt_rejects_wrong_passphrase() {
        let encrypted = encrypt_bytes(b"secret", "correct horse battery staple", ContentKind::Text, None).unwrap();

        let err = decrypt_bytes(&encrypted, "wrong horse battery staple").unwrap_err();

        assert!(matches!(err, CryptoError::Decryption));
    }
}
