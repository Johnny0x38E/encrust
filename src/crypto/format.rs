use super::error::CryptoError;
use super::kdf::Argon2idParams;
use super::suite::EncryptionSuite;
use super::types::ContentKind;

/// 文件签名。解密时先检查 magic，避免把任意文件当作密文处理。
pub(super) const MAGIC: &[u8; 7] = b"ENCRUST";
/// v1 是早期固定格式：Argon2id + AES-256-GCM + 固定 salt/nonce 长度。
pub(super) const LEGACY_VERSION_V1: u8 = 1;
/// v2 是当前自描述格式：把算法、KDF 参数、salt/nonce 长度写进文件头。
pub(super) const CURRENT_VERSION: u8 = 2;
pub(super) const KDF_ARGON2ID: u8 = 1;
pub(super) const LEGACY_CIPHER_AES_256_GCM: u8 = 1;
pub(super) const CONTENT_FILE: u8 = 1;
pub(super) const CONTENT_TEXT: u8 = 2;
pub(super) const SALT_LEN: usize = 16;
pub(super) const AES_GCM_NONCE_LEN: usize = 12;
pub(super) const KEY_LEN: usize = 32;
pub(super) const MIN_PASSPHRASE_CHARS: usize = 8;

/// 当前 v1 固定头部的最小长度。保留它是为了长期读取已经发布的旧文件。
pub const MIN_HEADER_LEN: usize = MAGIC.len() + 1 + 1 + 1 + 1 + 2 + SALT_LEN + AES_GCM_NONCE_LEN;

/// v2 目前编码 memory/iterations/parallelism/output_len，一共 14 字节。
pub(super) const V2_KDF_PARAMS_LEN: u16 = 14;

const V2_FIXED_HEADER_LEN: usize = MAGIC.len() + 1 + 2;

/// 解析后的文件头。
///
/// salt/nonce 和 KDF 参数来自文件本身，解密必须使用这些历史参数，而不是当前
/// 代码里的默认值；这是长期兼容的核心。
pub(super) struct ParsedHeader {
    pub version: u8,
    pub suite: EncryptionSuite,
    pub kind: ContentKind,
    pub file_name: Option<String>,
    pub salt: Vec<u8>,
    pub nonce: Vec<u8>,
    pub kdf_params: Argon2idParams,
    pub header_len: usize,
}

/// 构建 v2 文件头。
///
/// v2 的目标是“自描述”：算法、KDF 参数、salt/nonce 长度都写进文件头。
/// 这样未来默认算法或参数变化时，旧文件仍然能按当年的元数据解密。
pub(super) fn build_v2_header(
    kind: ContentKind,
    file_name: Option<&str>,
    suite: EncryptionSuite,
    kdf_params: Argon2idParams,
    salt: &[u8],
    nonce: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let file_name_bytes = file_name.unwrap_or_default().as_bytes();
    let file_name_len =
        u16::try_from(file_name_bytes.len()).map_err(|_| CryptoError::FileNameTooLong)?;
    let salt_len = u8::try_from(salt.len()).map_err(|_| CryptoError::InvalidFormat)?;
    let nonce_len = u8::try_from(nonce.len()).map_err(|_| CryptoError::InvalidFormat)?;
    let kdf_params_bytes = kdf_params.encode();
    let metadata_len = 1
        + 1
        + 1
        + 2
        + file_name_bytes.len()
        + 1
        + 2
        + kdf_params_bytes.len()
        + 1
        + salt.len()
        + 1
        + nonce.len();
    let header_len = u16::try_from(V2_FIXED_HEADER_LEN + metadata_len)
        .map_err(|_| CryptoError::InvalidFormat)?;

    let mut output = Vec::with_capacity(header_len as usize);
    output.extend_from_slice(MAGIC);
    output.push(CURRENT_VERSION);
    output.extend_from_slice(&header_len.to_be_bytes());
    output.push(suite.id());
    output.push(kind_to_byte(kind));
    output.push(KDF_ARGON2ID);
    output.extend_from_slice(&file_name_len.to_be_bytes());
    output.extend_from_slice(file_name_bytes);
    output.push(KDF_ARGON2ID);
    output.extend_from_slice(&V2_KDF_PARAMS_LEN.to_be_bytes());
    output.extend_from_slice(&kdf_params_bytes);
    output.push(salt_len);
    output.extend_from_slice(salt);
    output.push(nonce_len);
    output.extend_from_slice(nonce);

    Ok(output)
}

/// 根据版本号分发到对应解析器。这里是旧文件兼容的入口。
pub(super) fn parse_header(input: &[u8]) -> Result<ParsedHeader, CryptoError> {
    if input.len() < MAGIC.len() + 1 || &input[..MAGIC.len()] != MAGIC {
        return Err(CryptoError::InvalidFormat);
    }

    match input[MAGIC.len()] {
        LEGACY_VERSION_V1 => parse_v1_header(input),
        CURRENT_VERSION => parse_v2_header(input),
        _ => Err(CryptoError::UnsupportedVersion),
    }
}

fn parse_v1_header(input: &[u8]) -> Result<ParsedHeader, CryptoError> {
    if input.len() < MIN_HEADER_LEN {
        return Err(CryptoError::InvalidFormat);
    }

    // v1 没有 header_len 字段，只能按固定顺序读取字段。
    let mut cursor = MAGIC.len();
    let version = read_u8(input, &mut cursor)?;
    let kdf = read_u8(input, &mut cursor)?;
    let cipher = read_u8(input, &mut cursor)?;
    if version != LEGACY_VERSION_V1 || kdf != KDF_ARGON2ID || cipher != LEGACY_CIPHER_AES_256_GCM {
        return Err(CryptoError::InvalidFormat);
    }

    let kind = byte_to_kind(read_u8(input, &mut cursor)?)?;
    let file_name_len = read_u16(input, &mut cursor)? as usize;
    let file_name = read_file_name(input, &mut cursor, file_name_len)?;
    let salt = read_slice(input, &mut cursor, SALT_LEN)?.to_vec();
    let nonce = read_slice(input, &mut cursor, AES_GCM_NONCE_LEN)?.to_vec();

    if input.len() <= cursor {
        return Err(CryptoError::InvalidFormat);
    }

    Ok(ParsedHeader {
        version,
        suite: EncryptionSuite::Aes256Gcm,
        kind,
        file_name,
        salt,
        nonce,
        kdf_params: Argon2idParams::default(),
        header_len: cursor,
    })
}

fn parse_v2_header(input: &[u8]) -> Result<ParsedHeader, CryptoError> {
    if input.len() < V2_FIXED_HEADER_LEN {
        return Err(CryptoError::InvalidFormat);
    }

    // v2 先读取 header_len，后续所有元数据必须刚好消费到该位置。
    // 这样可以明确区分“文件头”和“密文”，也能提前拒绝截断或多读的格式。
    let mut cursor = MAGIC.len();
    let version = read_u8(input, &mut cursor)?;
    let header_len = read_u16(input, &mut cursor)? as usize;
    if version != CURRENT_VERSION || header_len < V2_FIXED_HEADER_LEN || input.len() <= header_len {
        return Err(CryptoError::InvalidFormat);
    }

    let suite = EncryptionSuite::from_id(read_u8(input, &mut cursor)?)?;
    let kind = byte_to_kind(read_u8(input, &mut cursor)?)?;
    let kdf = read_u8(input, &mut cursor)?;
    if kdf != KDF_ARGON2ID {
        return Err(CryptoError::InvalidFormat);
    }

    let file_name_len = read_u16(input, &mut cursor)? as usize;
    let file_name = read_file_name(input, &mut cursor, file_name_len)?;

    let kdf_params_id = read_u8(input, &mut cursor)?;
    if kdf_params_id != KDF_ARGON2ID {
        return Err(CryptoError::InvalidFormat);
    }
    let kdf_params_len = read_u16(input, &mut cursor)? as usize;
    let kdf_params = Argon2idParams::decode(read_slice(input, &mut cursor, kdf_params_len)?)?;
    if usize::from(kdf_params.output_len()) != KEY_LEN {
        return Err(CryptoError::InvalidFormat);
    }

    let salt_len = read_u8(input, &mut cursor)? as usize;
    let salt = read_slice(input, &mut cursor, salt_len)?.to_vec();
    let nonce_len = read_u8(input, &mut cursor)? as usize;
    if nonce_len != suite.nonce_len() {
        return Err(CryptoError::InvalidFormat);
    }
    let nonce = read_slice(input, &mut cursor, nonce_len)?.to_vec();

    if cursor != header_len {
        return Err(CryptoError::InvalidFormat);
    }

    Ok(ParsedHeader {
        version,
        suite,
        kind,
        file_name,
        salt,
        nonce,
        kdf_params,
        header_len,
    })
}

pub(super) fn kind_to_byte(kind: ContentKind) -> u8 {
    match kind {
        ContentKind::File => CONTENT_FILE,
        ContentKind::Text => CONTENT_TEXT,
    }
}

fn byte_to_kind(value: u8) -> Result<ContentKind, CryptoError> {
    match value {
        CONTENT_FILE => Ok(ContentKind::File),
        CONTENT_TEXT => Ok(ContentKind::Text),
        _ => Err(CryptoError::InvalidFormat),
    }
}

fn read_file_name(
    input: &[u8],
    cursor: &mut usize,
    len: usize,
) -> Result<Option<String>, CryptoError> {
    let bytes = read_slice(input, cursor, len)?;
    if bytes.is_empty() {
        Ok(None)
    } else {
        String::from_utf8(bytes.to_vec())
            .map(Some)
            .map_err(|_| CryptoError::InvalidFormat)
    }
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
    read_fixed(slice)
}

pub(super) fn read_fixed<const N: usize>(input: &[u8]) -> Result<[u8; N], CryptoError> {
    let mut array = [0_u8; N];
    array.copy_from_slice(input.get(..N).ok_or(CryptoError::InvalidFormat)?);
    Ok(array)
}

fn read_slice<'a>(
    input: &'a [u8],
    cursor: &mut usize,
    len: usize,
) -> Result<&'a [u8], CryptoError> {
    // 所有读取都通过 cursor + checked_add 进行边界检查，格式错误只返回
    // InvalidFormat，不能因为恶意输入触发 panic。
    let end = cursor.checked_add(len).ok_or(CryptoError::InvalidFormat)?;
    let slice = input.get(*cursor..end).ok_or(CryptoError::InvalidFormat)?;
    *cursor = end;
    Ok(slice)
}
