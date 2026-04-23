# Encrust

Encrust 是一个用于学习 Rust 的简单跨平台桌面加密应用。

## 功能

- 拖拽文件到窗口并加密。
- 使用系统文件选择器选择文件并加密。
- 输入文本并加密为 `.encrust` 文件。
- 拖拽或选择 `.encrust` 文件并解密。
- 解密文本后直接显示并支持快捷复制。
- 解密文件后支持选择保存路径。
- 使用 Argon2id 派生密钥，使用 AES-256-GCM 对称加密。
- 密钥短语至少 8 个字符。
- 提供 macOS、Linux、Windows 构建脚本。

## 运行

```bash
cargo run
```

## 测试

```bash
cargo test
```

## 构建

macOS:

```bash
./scripts/build-macos.sh
```

Linux:

```bash
./scripts/build-linux.sh
```

Windows PowerShell:

```powershell
.\scripts\build-windows.ps1
```

当前脚本默认在对应操作系统本机执行 `cargo build --release`。跨系统交叉编译涉及目标链、系统库和打包格式，建议后续通过 GitHub Actions 分平台构建。

## 加密格式

`.encrust` 文件由以下部分组成：

- magic：`ENCRUST`
- version：`1`
- KDF 标识：Argon2id
- cipher 标识：AES-256-GCM
- 内容类型：文件或文本
- 原文件名长度和原文件名
- 16 字节 salt
- 12 字节 nonce
- ciphertext

文件头作为 AES-GCM AAD 参与认证，因此篡改文件头会导致解密失败。
