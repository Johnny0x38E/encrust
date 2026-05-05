Encrust 的加密模块是连接用户界面与底层密码学实现的唯一桥梁。它的核心设计目标不是提供“尽可能多的算法选项”，而是在**向后兼容**、**算法可扩展**与**API 易用性**之间建立稳固的边界：UI 层只需关心“加密什么”和“用哪个算法”，而解密时则完全由文件自身的历史元数据驱动，杜绝因默认算法升级而导致旧文件无法打开的风险。本文从模块分层、公开 API 契约以及内部协作流程三个维度，阐述这一架构的构成逻辑。

## 模块分层与职责边界

加密模块位于 `src/crypto/` 目录下，采用**门面（Facade）+ 垂直分层**的结构。`src/crypto.rs` 作为唯一公开入口，通过 `pub use` 向外暴露经过裁剪的 API 集合；具体实现按职责拆分为六个私有子模块，彼此不跨层直接耦合。

```
src/crypto/
├── crypto.rs      # 门面：模块聚合、公开 API 导出、轻量校验
├── encrypt.rs     # 加密流程编排：随机数生成 → KDF → 文件头构建 → AEAD 加密
├── decrypt.rs     # 解密流程编排：文件头解析 → KDF → AEAD 解密 → 载荷组装
├── suite.rs       # AEAD 套件抽象：AES-256-GCM / XChaCha20-Poly1305 / SM4-GCM
├── kdf.rs         # 密钥派生：Argon2id 参数快照与密钥生成
├── format.rs      # 文件格式：v1/v2 文件头的编码与解析
├── types.rs       # 核心数据模型：ContentKind / EncryptedFileMetadata / DecryptedPayload
├── error.rs       # 类型化错误：CryptoError
└── tests.rs       # 模块级单元测试
```

这种分层的关键在于**职责收敛**：`suite.rs` 只处理原始字节与 AEAD 接口的对接，`format.rs` 只处理文件头的二进制布局，而 `encrypt.rs` 与 `decrypt.rs` 负责把它们按正确顺序串联。若未来新增第四套 AEAD 算法，只需修改 `suite.rs` 和 `format.rs` 里的 id 映射，加密/解密流程本身无需改动。

Sources: [crypto.rs](src/crypto.rs#L1-L37), [suite.rs](src/crypto/suite.rs#L15-L24), [format.rs](src/crypto/format.rs#L6-L12)

## 公开 API 速查

从 `src/crypto.rs` 导出的符号构成了 UI 层与加密模块之间的全部契约。中间层开发者只需掌握以下 6 个核心元素，即可完成文件或文本的加解密集成。

| 符号 | 类型 | 用途 |
|---|---|---|
| `encrypt_bytes` | 函数 | 使用**默认套件**（AES-256-GCM）加密，返回完整的 `.encrust` 文件字节 |
| `encrypt_bytes_with_suite` | 函数 | 使用**指定套件**加密，允许 UI 让用户选择算法 |
| `decrypt_bytes` | 函数 | 解密完整的 `.encrust` 文件，**自动识别** v1 / v2 格式 |
| `inspect_encrypted_file` | 函数 | **无需密钥短语**，仅从文件头读取格式版本、套件类型与内容种类 |
| `validate_passphrase` | 函数 | 按 Unicode 字符数检查密钥短语长度，供 UI 实时校验输入 |
| `CryptoError` | 枚举 | 统一错误类型，可直接转换为 UI 提示文案 |

此外，`EncryptionSuite`、`ContentKind`、`DecryptedPayload` 与 `EncryptedFileMetadata` 四个类型作为参数与返回值载体，也在公开接口中暴露。它们都是拥有 `'static` 展示名称或纯数据字段的结构，便于在 egui 界面中直接绑定到下拉框与状态标签。

Sources: [crypto.rs](src/crypto.rs#L17-L22), [types.rs](src/crypto/types.rs#L1-L31)

## API 设计原则与调用约定

公开 API 的设计遵循三条显式约定，它们直接影响了 UI 层的实现方式与用户体验。

**第一，加密时由调用方指定算法，解密时由文件头决定算法。** `encrypt_bytes_with_suite` 接受 `EncryptionSuite` 参数，因此 UI 可以在加密面板提供下拉选择；而 `decrypt_bytes` 完全不接收算法参数，它在内部先调用 `parse_header` 提取文件头里的套件 id，再分发给对应的 AEAD 实现。这意味着即使未来默认加密算法从 AES-256-GCM 迁移到更新套件，三年前生成的旧文件仍能按当时的元数据正确解密，UI 层无需维护任何算法版本映射表。

**第二，文件元数据与密文不可被分开传递。** `inspect_encrypted_file` 虽然能独立读取元数据，但 `decrypt_bytes` 会**重新解析**一次文件头，而不是接受外部传入的元数据结构。这消除了“元数据被篡改后仍用旧密文解密”的潜在不一致风险。UI 层可以用 `inspect_encrypted_file` 给用户展示“该文件使用 XChaCha20-Poly1305 加密”，但真正执行解密时仍以完整文件为准。

**第三，所有错误都收敛到 `CryptoError`，且不区分“密码错误”与“密文被篡改”。** `Decryption` 错误的统一文案为“解密失败：密钥错误或文件被篡改”。这种模糊化设计是有意为之——避免攻击者通过错误反馈判断输入是否触及了有效密文边界。

Sources: [decrypt.rs](src/crypto/decrypt.rs#L8-L34), [error.rs](src/crypto/error.rs#L10-L27), [app.rs](src/app.rs#L973-L995)

## 内部协作流程

公开 API 的简洁性建立在内部子模块的精确协作之上。以下两张序列图展示了加密与解密时的数据流与模块交互边界。

### 加密流程

```mermaid
sequenceDiagram
    participant UI as app.rs (UI)
    participant Facade as crypto.rs
    participant Enc as encrypt.rs
    participant KDF as kdf.rs
    participant Fmt as format.rs
    participant Suite as suite.rs

    UI->>Facade: encrypt_bytes_with_suite(plaintext, passphrase, kind, file_name, suite)
    Facade->>Enc: 转发调用
    Enc->>Enc: OsRng 生成 salt / nonce
    Enc->>KDF: derive_key(passphrase, salt, default_params)
    KDF-->>Enc: Zeroizing<[u8; 32]>
    Enc->>Fmt: build_v2_header(kind, file_name, suite, params, salt, nonce)
    Fmt-->>Enc: header bytes
    Enc->>Suite: encrypt_with_suite(suite, key, nonce, plaintext, aad=header)
    Suite-->>Enc: ciphertext
    Enc->>Enc: [header || ciphertext]
    Enc-->>Facade: Vec<u8>
    Facade-->>UI: 完整 .encrust 文件
```

加密流程的核心特征是**一次性顺序执行**：随机数生成、密钥派生、文件头构建、AEAD 加密四个步骤在单线程内串行完成，没有状态机或中间缓存。`Zeroizing` 包装确保密钥数组在离开 `kdf.rs` 后仍能在内存中被安全清零。

### 解密流程

```mermaid
sequenceDiagram
    participant UI as app.rs (UI)
    participant Facade as crypto.rs
    participant Dec as decrypt.rs
    participant Fmt as format.rs
    participant KDF as kdf.rs
    participant Suite as suite.rs

    UI->>Facade: decrypt_bytes(file_bytes, passphrase)
    Facade->>Dec: 转发调用
    Dec->>Fmt: parse_header(file_bytes)
    Fmt->>Fmt: 检查 MAGIC，按版本分发 v1 / v2
    Fmt-->>Dec: ParsedHeader { suite, salt, nonce, kdf_params, header_len }
    Dec->>KDF: derive_key(passphrase, salt, params)
    KDF-->>Dec: Zeroizing<[u8; 32]>
    Dec->>Dec: ciphertext = file_bytes[header_len..]
    Dec->>Suite: decrypt_with_suite(suite, key, nonce, ciphertext, aad=header)
    Suite-->>Dec: plaintext
    Dec-->>Facade: DecryptedPayload { kind, file_name, plaintext }
    Facade-->>UI: 明文载荷与元数据
```

解密流程的关键在于 `parse_header` 的**版本自识别**。文件头前 7 字节固定为 `ENCRUST` magic，第 8 字节是版本号。v1 格式没有 `header_len` 字段，只能按固定偏移量读取；v2 格式在 magic 与版本号之后立即写入 2 字节的 `header_len`，使解析器可以明确知道“文件头在哪里结束、密文从哪里开始”。这种设计让 `decrypt.rs` 无需关心版本差异，只消费 `ParsedHeader` 里的结构化数据。

Sources: [encrypt.rs](src/crypto/encrypt.rs#L28-L52), [decrypt.rs](src/crypto/decrypt.rs#L12-L34), [format.rs](src/crypto/format.rs#L97-L108)

## 文件格式版本兼容的架构意义

v1 与 v2 并存不是简单的“新旧替换”，而是架构层面的**时间胶囊**机制。v2 文件头把 `suite.id()`、`kdf_params.encode()`、`salt_len`、`nonce_len` 全部写入自描述结构，使得每一个 `.encrust` 文件都携带了生成它时所需的全部密码学参数。这带来了两个架构级收益：

1. **默认参数演进不破坏旧文件**：当 Argon2id 的默认迭代次数或内存成本未来需要提升时，新文件会使用更高参数，而旧文件仍按文件头里记录的历史参数解密。
2. **算法扩展不破坏旧文件**：新增第四、第五套 AEAD 套件时，只要分配新的 `suite.id()`，旧文件的 id 映射就不会被覆盖或复用。

因此，加密模块的公开 API 能够长期保持稳定——`decrypt_bytes` 的签名自项目发布以来无需改变，因为所有兼容性信息都内嵌在文件格式中，而非硬编码在 API 参数里。

Sources: [format.rs](src/crypto/format.rs#L44-L95), [suite.rs](src/crypto/suite.rs#L43-L60)

## 边界与防错设计

在 API 与 UI 的交界面上，加密模块设置了两道显式防线。**第一道是密钥短语长度校验**：`validate_passphrase` 按 Unicode 字符计数而非字节长度检查，确保中文、emoji 等多字节字符不会被误判为短密码。UI 在启用“加密/解密”按钮前会调用此函数进行预校验。**第二道是文件头边界校验**：`format.rs` 中所有读取操作都通过 `cursor + checked_add` 进行，遇到截断或恶意构造的文件时统一返回 `InvalidFormat`，不会触发 panic。

这两道防线共同保证了一个重要性质：**加密模块的公开函数在任意用户输入下都不会 panic**，所有异常都以 `CryptoError` 形式返回，由 UI 层决定如何展示。

Sources: [crypto.rs](src/crypto.rs#L27-L33), [format.rs](src/crypto/format.rs#L256-L267)

## 阅读下一页

理解公开 API 的契约之后，下一步可以深入探究这些 API 背后的数据载体。在 [核心数据模型与类型定义](11-he-xin-shu-ju-mo-xing-yu-lei-xing-ding-yi) 中，我们将详细剖析 `ContentKind`、`DecryptedPayload` 与 `EncryptedFileMetadata` 的设计意图，以及它们如何在 UI 状态管理与文件 IO 之间承担语义桥梁的角色。若你对文件头的二进制布局更感兴趣，可直接跳转至 [自描述文件格式与版本兼容策略](12-zi-miao-shu-wen-jian-ge-shi-yu-ban-ben-jian-rong-ce-lue)。