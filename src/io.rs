use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 读取整个文件到内存。
pub fn read_file(path: &Path) -> io::Result<Vec<u8>> {
    fs::read(path)
}

/// 把加密后的 bytes 写入目标路径。
pub fn write_file(path: &Path, data: &[u8]) -> io::Result<()> {
    fs::write(path, data)
}

/// 解密文件的默认保存路径。
///
/// 如果加密文件里记录了原文件名，则默认使用 `decrypted-原文件名`，避免直接覆盖
/// 用户电脑上可能仍然存在的原始文件。
///
/// 注意：加密输出不在这里提供默认路径。当前产品规则要求用户在加密前手动选择
/// 保存位置，UI 只把默认文件名交给系统保存对话框作为建议。
pub fn default_decrypted_output_path(
    encrypted_path: &Path,
    original_file_name: Option<&str>,
) -> PathBuf {
    let parent = encrypted_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = original_file_name
        .filter(|name| !name.trim().is_empty())
        .map(|name| format!("decrypted-{name}"))
        .unwrap_or_else(|| "decrypted-output".to_owned());

    parent.join(file_name)
}
