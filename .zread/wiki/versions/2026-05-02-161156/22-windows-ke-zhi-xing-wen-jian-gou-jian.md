Encrust 在 Windows 平台上的构建策略遵循"最小可用分发"原则：通过单条 PowerShell 命令完成 Release 编译、资源归集与 ZIP 打包，最终产物是一个解压即可运行的便携压缩包。与 macOS 的 `.app` + DMG 策略以及 Linux 的 AppImage 策略不同，Windows 构建刻意避开了安装器或自解压程序，以降低分发复杂度和用户安装门槛。本章将完整解析构建脚本的工作流程、设计取舍以及 Windows 运行时的特殊适配点。

Sources: [scripts/build-windows.ps1](scripts/build-windows.ps1)

## 构建流程概览

`build-windows.ps1` 将构建过程抽象为六个连续阶段，从版本提取到最终 ZIP 归档，全程无需手动干预。以下流程图展示了各阶段的数据流转与关键操作：

```mermaid
flowchart TD
    A[启动 PowerShell 脚本] --> B[提取版本号<br/>cargo pkgid]
    B --> C[Release 编译<br/>cargo build --release]
    C --> D[创建临时分发目录<br/>windows_dist]
    D --> E[复制 encrust.exe]
    D --> F[复制 appicon.png]
    E --> G[ZIP 打包]
    F --> G
    G --> H[清理临时目录]
    H --> I[输出 Encrust-{version}-windows.zip]
```

整个流程的核心设计是**"临时目录归集 + 一次性压缩"**：脚本先在 `target/release/windows_dist` 中聚合所有需要分发的文件，再调用 `Compress-Archive` 生成标准 ZIP，最后删除中间目录，避免污染构建输出。

Sources: [scripts/build-windows.ps1](scripts/build-windows.ps1#L1-L42)

## 环境与先决条件

在 Windows 主机上执行构建前，需确认以下环境已就绪：

| 依赖项 | 最低要求 | 说明 |
|--------|----------|------|
| 操作系统 | Windows 10/11 | 脚本依赖 PowerShell 5.1+ 的 `Compress-Archive`  cmdlet |
| Rust 工具链 | Nightly | 项目使用 Rust Edition 2024，需 nightly 编译器 |
| PowerShell | 5.1 或 7.x | 脚本使用 `$PSScriptRoot` 与 `Split-Path` 等现代语法 |
| Cargo | 跟随 Rust 安装 | 用于版本提取与编译 |

当前脚本**不支持交叉编译**，即无法在 Linux 或 macOS 主机上直接构建 Windows 可执行文件。如需实现跨平台 CI 构建，建议后续引入 GitHub Actions 并在 `windows-latest` runner 上执行本脚本。

Sources: [Cargo.toml](Cargo.toml#L1-L5), [README.md](README.md)

## 脚本工作原理详解

`build-windows.ps1` 全长 43 行，按功能可拆分为四个逻辑段落。理解每段的设计意图，有助于在需要时进行二次定制（例如追加 DLL 依赖或修改打包格式）。

**第一段：环境初始化与版本提取（第 1–7 行）**

脚本以 `$ErrorActionPreference = "Stop"` 开启严格模式，确保任何命令失败时立即终止，避免生成不完整的分发包。版本号通过 `cargo pkgid` 提取并正则截断，保证 ZIP 文件名与 `Cargo.toml` 中的 `version` 字段严格一致。`$PROJECT_ROOT` 由 `$PSScriptRoot` 向上推导，使脚本可在任意工作目录下被调用。

Sources: [scripts/build-windows.ps1](scripts/build-windows.ps1#L1-L7)

**第二段：Release 编译（第 14 行）**

直接调用 `cargo build --release`，由 Cargo 根据主机目标三元组（通常为 `x86_64-pc-windows-msvc`）自动选择链接器与目标平台。编译产物 `encrust.exe` 将位于 `target/release/`。值得注意的是，项目中并未配置 Windows 子系统属性（如 `#![windows_subsystem = "windows"]`），因此最终可执行文件在启动时会伴随一个控制台窗口，这在调试阶段是刻意保留的行为。

Sources: [scripts/build-windows.ps1](scripts/build-windows.ps1#L14), [src/main.rs](src/main.rs#L9-L31)

**第三段：资源归集（第 17–29 行）**

脚本创建临时目录 `windows_dist`，随后将可执行文件与应用图标复制其中。图标复制带有防御性检查：若 `assets/appicon.png` 存在则纳入分发包，否则打印警告并继续。这种容错设计保证了即使图标文件意外缺失，构建流程也不会中断，只是最终 ZIP 中缺少图标资源。

Sources: [scripts/build-windows.ps1](scripts/build-windows.ps1#L17-L29)

**第四段：ZIP 打包与清理（第 31–42 行）**

使用 `Compress-Archive` 将临时目录内容打包为 `${APP_NAME}-${VERSION}-windows.zip`。若同名 ZIP 已存在则先删除，避免新旧文件混合。打包完成后立即清理 `windows_dist`，使 `target/release/` 保持整洁。最终产物命名示例：`Encrust-0.1.0-windows.zip`。

Sources: [scripts/build-windows.ps1](scripts/build-windows.ps1#L31-L42)

## 三平台构建策略对比

Encrust 针对三个桌面平台采用了截然不同的打包哲学，这直接反映在构建脚本的复杂度与输出物形态上：

| 维度 | macOS | Linux | Windows |
|------|-------|-------|---------|
| **输出格式** | `.app` Bundle + DMG | AppImage | ZIP 压缩包 |
| **打包工具** | `cargo-bundle` + `hdiutil` | `appimagetool` | `Compress-Archive`（内置） |
| **图标处理** | `.icns` 嵌入 Bundle | `.png` 置于 AppDir | `.png` 随 ZIP 分发 |
| **多架构支持** | Universal Binary（x86_64 + aarch64） | x86_64 单架构 | x86_64 单架构 |
| **代码签名** | Ad-hoc `codesign` | 无 | 无 |
| **脚本复杂度** | 高（约 90 行） | 中高（约 60 行） | 低（43 行） |

Windows 策略之所以最为简洁，是因为 ZIP 格式无需处理启动器脚本、桌面集成元数据或文件系统镜像。用户解压后双击 `encrust.exe` 即可运行，没有额外的"安装"语义。当然，这也意味着程序不会自动出现在开始菜单或添加/删除程序列表中，属于典型的便携应用（portable app）模型。

Sources: [scripts/build-macos.sh](scripts/build-macos.sh), [scripts/build-linux.sh](scripts/build-linux.sh), [scripts/build-windows.ps1](scripts/build-windows.ps1)

## Windows 运行时的特殊考量

构建产物在 Windows 上的运行体验，受两处源码级条件编译的影响。

**图标加载策略**

`src/main.rs` 中通过 `#[cfg(not(target_os = "macos"))]` 将所有非 macOS 平台（包括 Windows 与 Linux）统一指向 `assets/appicon.png`。这意味着 Windows 构建不需要像 macOS 那样维护 `.icns` 专用格式，运行时窗口标题栏与任务栏图标均由同一份 PNG 解码生成。

Sources: [src/main.rs](src/main.rs#L34-L43)

**CJK 字体回退的当前局限**

`configure_fonts` 函数中列出的字体候选路径均为 Unix/macOS 风格（如 `/System/Library/Fonts/` 与 `/usr/share/fonts/`）。在 Windows 环境下，这些路径均不存在，因此构建出的可执行文件在 Windows 上运行时，CJK 字符将回退到 egui 内置的默认字体，显示效果可能不如 macOS/Linux 理想。若需改善 Windows 中文显示，需扩展字体候选列表以覆盖 `C:\Windows\Fonts\` 下的常见字体（如 `msyh.ttc`）。更详细的跨平台字体策略请参阅 [跨平台 CJK 字体回退与图标加载](7-kua-ping-tai-cjk-zi-ti-hui-tui-yu-tu-biao-jia-zai)。

Sources: [src/main.rs](src/main.rs#L54-L93)

## 已知限制与后续方向

当前 Windows 构建实现是项目早期阶段的务实选择，存在以下可改进空间：

1. **无安装器/MSI**：ZIP 虽然简单，但无法写入注册表、创建开始菜单快捷方式或管理卸载信息。若面向普通消费者分发，后续可考虑引入 `cargo-wix` 或 `cargo-msi` 生成标准 Windows 安装包。
2. **需本机构建**：脚本假设在 Windows 主机上直接运行。引入 GitHub Actions 后，可在 `windows-latest` runner 中自动执行 `build-windows.ps1`，实现无本地 Windows 环境的持续交付。
3. **缺少版本信息资源**：当前 `Cargo.toml` 中的 `[package.metadata.bundle]` 主要面向 macOS 的 `cargo-bundle`，Windows 可执行文件尚未嵌入版本资源（Version Info），在文件属性对话框中不会显示产品版本与描述。

Sources: [Cargo.toml](Cargo.toml#L7-L15), [README.md](README.md)

## 继续阅读

完成 Windows 构建的理解后，若你希望对比其他平台的打包细节，可继续阅读：

- [macOS 通用二进制与 DMG 打包](20-macos-tong-yong-er-jin-zhi-yu-dmg-da-bao)：了解 Universal Binary 的合并流程与 `.app` Bundle 结构。
- [Linux AppImage 打包](21-linux-appimage-da-bao)：了解 AppDir 组织方式与 `appimagetool` 的调用机制。
- [构建与测试命令参考](4-gou-jian-yu-ce-shi-ming-ling-can-kao)：查看跨平台通用的 `cargo` 命令速查表。