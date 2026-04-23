use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 读取整个文件到内存。
///
/// 对学习项目来说，这是最直观的实现。大文件加密时更理想的做法是流式读取和加密，
/// 但 AES-GCM 的认证标签和错误处理会让示例复杂很多，所以 v1 保持简单。
pub fn read_file(path: &Path) -> io::Result<Vec<u8>> {
    fs::read(path)
}

/// 把加密后的 bytes 写入目标路径。
pub fn write_file(path: &Path, data: &[u8]) -> io::Result<()> {
    fs::write(path, data)
}

/// 文件加密的默认输出路径：在原文件名后追加 `.encrust`。
///
/// 例如 `/tmp/report.pdf` 会变成 `/tmp/report.pdf.encrust`。
pub fn default_file_output_path(input_path: &Path) -> PathBuf {
    let mut output = input_path.as_os_str().to_os_string();
    output.push(".encrust");
    PathBuf::from(output)
}

/// 文本加密没有源文件路径，所以默认放在当前工作目录。
pub fn default_text_output_path() -> PathBuf {
    PathBuf::from("encrypted-text.encrust")
}

/// 解密文件的默认保存路径。
///
/// 如果加密文件里记录了原文件名，则默认使用 `decrypted-原文件名`，避免直接覆盖
/// 用户电脑上可能仍然存在的原始文件。
pub fn default_decrypted_output_path(encrypted_path: &Path, original_file_name: Option<&str>) -> PathBuf {
    let parent = encrypted_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = original_file_name
        .filter(|name| !name.trim().is_empty())
        .map(|name| format!("decrypted-{name}"))
        .unwrap_or_else(|| "decrypted-output".to_owned());

    parent.join(file_name)
}
