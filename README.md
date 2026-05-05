# Encrust

Encrust 是一个用于学习 Rust 的简单跨平台桌面加密应用。

## 功能

- 拖拽文件到窗口并加密。
- 使用系统文件选择器选择文件并加密。
- 输入文本并加密为 `.encrust` 文件。
- 拖拽或选择 `.encrust` 文件并解密。
- 解密文本后直接显示并支持快捷复制。
- 解密文件后支持选择保存路径。
- 加密时支持三种 AEAD 套件：AES-256-GCM（默认）、XChaCha20-Poly1305、SM4-GCM（国密）。
- 使用 Argon2id 派生密钥。
- 密钥短语至少 8 个字符。
- 提供 macOS、Linux、Windows 构建脚本。

## 运行

需要 Rust nightly（Edition 2024）。

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

## 项目结构

```
src/
├── main.rs      # 应用入口、窗口设置、CJK 字体加载
├── app.rs       # egui UI 逻辑（加密/解密界面、文件拖拽、对话框）
├── crypto.rs    # 加密模块公开入口
└── crypto/      # 加密子模块
    ├── format   # .encrust 文件头 v1/v2 编解码
    ├── kdf      # Argon2id 密钥派生
    ├── suite    # 多 AEAD 套件实现
    ├── encrypt  # 加密流程
    ├── decrypt  # 解密流程
    ├── error    # 错误类型
    ├── types    # 核心数据结构
    └── tests    # 单元测试
└── io.rs        # 文件读写与输出路径生成
```

## 加密格式

`.encrust` 文件使用自描述的 v2 格式：

- magic：`ENCRUST`
- version：`2`
- 加密套件标识（AES-256-GCM / XChaCha20-Poly1305 / SM4-GCM）
- KDF 标识：Argon2id（含参数快照）
- 内容类型：文件或文本
- 原文件名长度和原文件名
- salt 长度和 salt
- nonce 长度和 nonce
- ciphertext

v1 是早期固定格式（Argon2id + AES-256-GCM），保留读取支持以保证向后兼容。

文件头作为 AEAD AAD 参与认证，因此篡改文件头会导致解密失败。

## 安全说明

- 密钥短语、派生密钥、salt、nonce 等敏感信息不会在界面或日志中暴露。
- 解密失败时不区分"密码错误"和"文件被篡改"，避免给攻击者提供额外信息。
- 所有用户输入都通过类型化错误处理，不会因异常输入导致程序崩溃。
