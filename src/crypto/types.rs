use super::suite::EncryptionSuite;

/// 明文内容类型。
///
/// 加密格式会把它写入文件头，解密后 UI 据此决定是显示文本，还是提示用户保存文件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    File,
    Text,
}

/// 不需要密钥短语即可从 `.encrust` 文件头读取的元数据。
///
/// 这里只包含 UI 可安全展示的信息，不包含 salt、nonce 或任何密钥材料。
#[derive(Debug, Clone)]
pub struct EncryptedFileMetadata {
    pub format_version: u8,
    pub suite: EncryptionSuite,
    pub kind: ContentKind,
}

/// 解密后的完整载荷。
///
/// `plaintext` 对文件和文本共用：文本由 UI 按 UTF-8 转换，文件则原样写回磁盘。
#[derive(Debug, Clone)]
pub struct DecryptedPayload {
    pub kind: ContentKind,
    pub file_name: Option<String>,
    pub plaintext: Vec<u8>,
}
