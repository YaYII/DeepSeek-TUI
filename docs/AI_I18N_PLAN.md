# AI 动态国际化方案 - 执行计划

## 📋 项目概述

### 核心理念
颠覆传统国际化模式，利用 AI 实时翻译能力，将英文源文件直接翻译为用户选择的语言，实现**单文件、零配置、无限语言支持**的动态国际化方案。

### 与传统方案的对比

| 维度 | 传统 i18n | AI 动态翻译 |
|------|-----------|-------------|
| 文件数量 | N 种语言 = N 份文件 | 永远只有 1 份文件 |
| 维护成本 | 需同步更新所有语言版本 | 零维护，AI 自动处理 |
| 新增语言 | 需创建新翻译文件 | 只需更改配置，AI 即时翻译 |
| 存储空间 | O(N) | O(1) |
| 一致性 | 容易出现不同步 | 单一数据源，天然一致 |
| 灵活性 | 受限于预定义语言 | 支持任何 AI 能翻译的语言 |

---

## 🎯 核心目标

### Phase 1: 基础架构（v0.8.11）
- [ ] 用户语言偏好配置系统
- [ ] AI 翻译引擎集成
- [ ] 原文备份与版本追踪机制
- [ ] 核心文件翻译流程

### Phase 2: 智能优化（v0.8.12）
- [ ] 增量翻译（只翻译变更部分）
- [ ] 术语一致性词典
- [ ] 翻译缓存策略
- [ ] 性能优化

### Phase 3: 全面覆盖（v0.9.0）
- [ ] 所有文档文件支持
- [ ] UI 文本动态翻译
- [ ] 错误消息翻译
- [ ] 代码注释可选翻译

---

## 🏗️ 技术架构

### 系统组件图

```
┌─────────────────────────────────────────────┐
│           用户交互层                         │
│  ┌──────────┐    ┌──────────────────┐      │
│  │ 初始化向导 │───▶│ 语言选择界面     │      │
│  └──────────┘    └──────────────────┘      │
└──────────────────┬──────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────┐
│           配置管理层                         │
│  ┌──────────────────────────────────┐      │
│  │ ~/.deepseek/config.toml          │      │
│  │   language = "zh-CN"             │      │
│  │   last_translation_lang = "en"   │      │
│  └──────────────────────────────────┘      │
└──────────────────┬──────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────┐
│         AI 翻译引擎层                        │
│  ┌────────────┐  ┌──────────────────┐      │
│  │ 语言检测    │  │ 翻译请求构建      │      │
│  └────────────┘  └──────────────────┘      │
│  ┌────────────┐  ┌──────────────────┐      │
│  │ DeepSeek API│  │ 翻译结果验证      │      │
│  └────────────┘  └──────────────────┘      │
└──────────────────┬──────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────┐
│         文件管理层                           │
│  ┌────────────┐  ┌──────────────────┐      │
│  │ 原文备份    │  │ 目标文件写入      │      │
│  │ (.backup/) │  │ (原地覆盖)        │      │
│  └────────────┘  └──────────────────┘      │
│  ┌────────────┐                             │
│  │ 翻译元数据  │ ← 记录语言状态、时间戳      │
│  └────────────┘                             │
└─────────────────────────────────────────────┘
```

### 核心模块设计

#### 1. 配置管理模块 (`crates/tui/src/i18n/config.rs`)

```rust
pub struct I18nConfig {
    /// 用户首选语言 (RFC 5646 格式: zh-CN, en-US, ja-JP)
    pub target_language: String,
    
    /// 当前文件的语言状态
    pub current_file_language: String,
    
    /// 是否启用 AI 翻译
    pub enable_ai_translation: bool,
    
    /// 翻译缓存目录
    pub cache_dir: PathBuf,
}
```

**职责：**
- 读取/写入 `~/.deepseek/config.toml`
- 管理语言偏好
- 追踪当前文件语言状态

#### 2. 翻译引擎模块 (`crates/tui/src/i18n/translator.rs`)

```rust
pub struct AITranslator {
    /// DeepSeek API 客户端
    client: DeepSeekClient,
    
    /// 术语词典（保证一致性）
    glossary: TermGlossary,
    
    /// 翻译缓存
    cache: TranslationCache,
}

impl AITranslator {
    pub async fn translate_file(
        &self,
        file_path: &Path,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<String>;
    
    pub async fn translate_incremental(
        &self,
        original: &str,
        modified: &str,
        previous_translation: &str,
    ) -> Result<String>;
}
```

**职责：**
- 调用 DeepSeek API 进行翻译
- 管理翻译缓存
- 处理术语一致性
- 支持增量翻译

#### 3. 文件管理模块 (`crates/tui/src/i18n/file_manager.rs`)

```rust
pub struct FileManager {
    /// 需要翻译的文件列表
    translatable_files: Vec<PathBuf>,
    
    /// 原文备份目录
    backup_dir: PathBuf,
    
    /// 翻译元数据存储
    metadata_store: MetadataStore,
}

impl FileManager {
    pub fn backup_original(&self, file: &Path) -> Result<()>;
    pub fn restore_original(&self, file: &Path) -> Result<()>;
    pub fn mark_translated(&self, file: &Path, lang: &str) -> Result<()>;
    pub fn needs_translation(&self, file: &Path) -> bool;
}
```

**职责：**
- 管理可翻译文件清单
- 原文备份与恢复
- 翻译状态追踪
- 文件权限处理

#### 4. 语言检测模块 (`crates/tui/src/i18n/detector.rs`)

```rust
pub struct LanguageDetector;

impl LanguageDetector {
    /// 检测文件当前语言
    pub fn detect_language(content: &str) -> Option<String>;
    
    /// 判断是否需要翻译
    pub fn needs_translation(current_lang: &str, target_lang: &str) -> bool;
}
```

---

## 📂 目录结构

```
crates/tui/src/i18n/
├── mod.rs                  # 模块入口
├── config.rs               # 配置管理
├── translator.rs           # AI 翻译引擎
├── file_manager.rs         # 文件管理
├── detector.rs             # 语言检测
├── cache.rs                # 翻译缓存
├── glossary.rs             # 术语词典
└── metadata.rs             # 元数据存储

~/.deepseek/
├── config.toml             # 主配置文件（新增 language 字段）
├── i18n/
│   ├── backup/             # 原文备份
│   │   ├── README.md.en
│   │   └── docs/guide.md.en
│   ├── cache/              # 翻译缓存
│   │   └── <hash>.json
│   └── metadata.json       # 翻译元数据
└── glossary.toml           # 自定义术语词典（可选）
```

---

## 🔧 实现步骤

### Step 1: 配置系统扩展（预计 2 小时）

**文件：** `crates/tui/src/config.rs`

**任务：**
1. 在配置结构中新增 `language` 字段
2. 更新配置解析逻辑
3. 在初始化向导中添加语言选择步骤
4. 编写单元测试

**示例配置：**
```toml
# ~/.deepseek/config.toml

[i18n]
enabled = true
target_language = "zh-CN"
auto_translate_on_startup = true
cache_enabled = true
```

**验收标准：**
- ✅ 用户可以设置语言偏好
- ✅ 配置正确持久化
- ✅ 默认值为 "en-US"

---

### Step 2: 原文备份机制（预计 3 小时）

**文件：** `crates/tui/src/i18n/file_manager.rs`

**任务：**
1. 实现文件备份功能
2. 创建 `.deepseek/i18n/backup/` 目录结构
3. 实现备份恢复功能
4. 处理文件路径映射（保持目录结构）

**关键逻辑：**
```rust
// 备份示例
README.md → ~/.deepseek/i18n/backup/README.md.en
docs/guide.md → ~/.deepseek/i18n/backup/docs/guide.md.en
```

**验收标准：**
- ✅ 首次翻译前自动备份原文
- ✅ 可以从备份恢复原始英文
- ✅ 备份文件带语言后缀标识

---

### Step 3: AI 翻译引擎核心（预计 6 小时）

**文件：** `crates/tui/src/i18n/translator.rs`

**任务：**
1. 集成 DeepSeek API 客户端
2. 构建翻译提示词模板
3. 实现批量翻译逻辑
4. 添加错误处理和重试机制
5. 实现翻译结果验证

**提示词模板示例：**
```
你是一个专业的技术文档翻译专家。请将以下 Markdown 文档从 {source_lang} 
翻译成 {target_lang}。

要求：
1. 保持 Markdown 格式不变（标题、列表、代码块等）
2. 技术术语保持一致性（参考术语表）
3. 代码示例中的注释也需要翻译
4. 不要翻译代码本身（变量名、函数名等）
5. 保持原有的语气和风格

术语表：
{glossary_entries}

待翻译内容：
{content}
```

**验收标准：**
- ✅ 成功调用 DeepSeek API
- ✅ 翻译结果保持格式完整
- ✅ 错误时有合理的降级策略
- ✅ 支持断点续传

---

### Step 4: 翻译缓存系统（预计 3 小时）

**文件：** `crates/tui/src/i18n/cache.rs`

**任务：**
1. 实现基于内容哈希的缓存
2. 缓存键设计：`hash(content + source_lang + target_lang)`
3. 缓存过期策略（TTL 或手动清除）
4. 缓存存储格式（JSON）

**缓存结构：**
```json
{
  "cache_key": "a1b2c3d4...",
  "source_lang": "en",
  "target_lang": "zh-CN",
  "content_hash": "e5f6g7h8...",
  "translated_content": "...",
  "timestamp": 1234567890,
  "ttl": 86400
}
```

**验收标准：**
- ✅ 相同内容不重复翻译
- ✅ 缓存命中率 > 80%
- ✅ 缓存可手动清除

---

### Step 5: 术语一致性管理（预计 4 小时）

**文件：** `crates/tui/src/i18n/glossary.rs`

**任务：**
1. 内置技术术语词典（DeepSeek、API、TUI 等）
2. 用户自定义术语支持
3. 术语在翻译提示词中注入
4. 术语学习机制（从历史翻译中提取）

**术语表示例：**
```toml
# ~/.deepseek/glossary.toml

[terms]
"DeepSeek" = "DeepSeek"  # 不翻译
"TUI" = "终端界面"
"API" = "API"            # 不翻译
"context window" = "上下文窗口"
"token" = "token"        # 不翻译
```

**验收标准：**
- ✅ 术语翻译一致性强
- ✅ 用户可以自定义术语
- ✅ 常见技术术语有默认值

---

### Step 6: 语言检测模块（预计 2 小时）

**文件：** `crates/tui/src/i18n/detector.rs`

**任务：**
1. 集成语言检测库（如 `lingua-rs`）
2. 实现文件语言状态判断
3. 避免重复翻译检测

**验收标准：**
- ✅ 准确检测文件当前语言
- ✅ 准确率 > 90%

---

### Step 7: 启动时自动翻译（预计 4 小时）

**文件：** `crates/tui/src/main.rs` 或启动流程

**任务：**
1. 在应用启动时检查语言配置
2. 如果目标语言 ≠ 当前语言，触发翻译
3. 显示翻译进度
4. 翻译失败时的降级处理（使用英文）

**流程：**
```
启动 → 读取配置 → 检测文件语言 → 需要翻译？
  ↓ 是
显示进度条 → 调用翻译引擎 → 写入文件 → 完成
  ↓ 否
正常启动
```

**验收标准：**
- ✅ 启动时自动检测并翻译
- ✅ 显示清晰的进度反馈
- ✅ 翻译失败不影响程序运行

---

### Step 8: 用户界面集成（预计 3 小时）

**文件：** `crates/tui/src/tui/onboarding/language_selector.rs`（新建）

**任务：**
1. 创建语言选择界面
2. 支持常用语言快速选择
3. 支持自定义语言代码
4. 预览翻译效果

**支持的语言列表：**
- English (en-US) - 默认
- 简体中文 (zh-CN)
- 繁體中文 (zh-TW)
- 日本語 (ja-JP)
- 한국어 (ko-KR)
- Español (es-ES)
- Français (fr-FR)
- Deutsch (de-DE)
- Русский (ru-RU)
- العربية (ar-SA)

**验收标准：**
- ✅ 直观的语言选择界面
- ✅ 支持搜索和过滤
- ✅ 显示语言名称（本地化）

---

### Step 9: 命令行工具支持（预计 2 小时）

**文件：** `crates/cli/src/commands/i18n.rs`（新建）

**任务：**
实现 CLI 命令：

```bash
# 设置语言
deepseek i18n set zh-CN

# 查看当前语言
deepseek i18n status

# 手动触发翻译
deepseek i18n translate

# 恢复英文原版
deepseek i18n restore

# 清除翻译缓存
deepseek i18n cache clear

# 编辑术语表
deepseek i18n glossary edit
```

**验收标准：**
- ✅ 所有命令正常工作
- ✅ 有帮助信息
- ✅ 错误提示清晰

---

### Step 10: 文档与测试（预计 4 小时）

**任务：**
1. 编写用户使用文档 `docs/I18N.md`
2. 编写开发者文档（架构说明）
3. 编写单元测试（覆盖率 > 80%）
4. 编写集成测试
5. 性能基准测试

**验收标准：**
- ✅ 文档完整清晰
- ✅ 测试覆盖核心功能
- ✅ 性能符合预期（翻译 < 5秒/文件）

---

## 📊 可翻译文件清单

### 优先级 P0（必须翻译）
- [ ] `README.md`
- [ ] `README.zh-CN.md`（合并到主 README）
- [ ] `docs/*.md`（所有文档）
- [ ] `config.example.toml`（配置示例注释）

### 优先级 P1（建议翻译）
- [ ] TUI 界面帮助文本
- [ ] 错误消息
- [ ] 命令提示信息
- [ ] 初始化向导文本

### 优先级 P2（可选翻译）
- [ ] 代码注释（关键部分）
- [ ] CHANGELOG.md
- [ ] CONTRIBUTING.md

---

## ⚠️ 风险与挑战

### 1. 翻译质量风险
**问题：** AI 翻译可能不准确或不一致  
**缓解措施：**
- 建立术语词典确保一致性
- 提供用户反馈机制
- 允许手动修正翻译

### 2. 性能问题
**问题：** 大量文件翻译耗时较长  
**缓解措施：**
- 实现增量翻译
- 使用缓存避免重复翻译
- 后台异步翻译

### 3. API 成本
**问题：** 频繁调用 DeepSeek API 产生费用  
**缓解措施：**
- 激进的缓存策略
- 批量翻译减少请求次数
- 提供离线模式选项

### 4. 格式破坏
**问题：** 翻译可能破坏 Markdown 格式  
**缓解措施：**
- 翻译后格式验证
- 特殊标记保护（代码块、链接等）
- 回退机制

### 5. 并发安全
**问题：** 多实例同时翻译可能导致冲突  
**缓解措施：**
- 文件锁机制
- 原子写入操作
- 事务性更新

---

## 🧪 测试策略

### 单元测试
- 配置读写测试
- 翻译引擎 mock 测试
- 缓存命中/未命中测试
- 文件格式保护测试

### 集成测试
- 完整翻译流程测试
- 语言切换测试
- 断点续传测试
- 错误恢复测试

### 端到端测试
- 新用户初始化流程
- 多语言切换场景
- 大文件翻译性能
- 网络异常处理

### 性能测试
- 翻译速度基准（目标：< 5秒/1000字）
- 缓存命中率（目标：> 80%）
- 内存占用（目标：< 100MB）

---

## 📈 成功指标

### 功能性指标
- ✅ 支持至少 10 种语言
- ✅ 翻译准确率 > 90%（人工抽检）
- ✅ 格式保持率 100%
- ✅ 术语一致性 > 95%

### 性能指标
- ✅ 单文件翻译时间 < 5秒
- ✅ 缓存命中率 > 80%
- ✅ 启动延迟增加 < 2秒
- ✅ 内存增长 < 50MB

### 用户体验指标
- ✅ 语言切换成功率 100%
- ✅ 用户满意度 > 4.5/5
- ✅ 零配置即可使用

---

## 🔄 工作流程示例

### 场景 1：新用户首次使用

```
1. 用户运行 deepseek-tui
2. 初始化向导启动
3. 用户选择语言：简体中文
4. 系统检测所有文档为英文
5. 调用 AI 翻译所有文档为中文
6. 保存翻译结果，覆盖原文件
7. 备份英文原版到 ~/.deepseek/i18n/backup/
8. 用户看到中文界面和文档
```

### 场景 2：用户切换语言

```
1. 用户运行 deepseek i18n set ja-JP
2. 系统从备份恢复英文原版
3. 调用 AI 翻译为日文
4. 覆盖文件内容
5. 更新配置中的语言设置
6. 用户下次启动看到日文界面
```

### 场景 3：文档更新后

```
1. 开发者更新 README.md（英文）
2. 用户启动应用
3. 系统检测到文件变更（通过 hash）
4. 仅翻译变更部分（增量翻译）
5. 更新缓存
6. 用户看到最新翻译
```

---

## 📝 交接说明

### 关键决策点

1. **是否保留英文备份？**
   - 决策：✅ 是，必须保留
   - 原因：作为翻译基准，支持语言切换和恢复

2. **翻译触发时机？**
   - 决策：启动时自动检测 + 手动触发
   - 原因：平衡自动化和用户控制

3. **缓存策略？**
   - 决策：基于内容哈希 + TTL 7天
   - 原因：平衡性能和时效性

4. **错误处理？**
   - 决策：降级到英文 + 错误日志
   - 原因：保证可用性优先

### 待解决问题

- [ ] 如何处理 RTL 语言（阿拉伯语、希伯来语）？
- [ ] 是否需要支持混合语言（部分翻译，部分不翻译）？
- [ ] 翻译质量如何量化评估？
- [ ] 是否需要社区贡献术语表？

### 依赖项

- DeepSeek API 访问权限
- Rust crate: `lingua-rs`（语言检测）
- Rust crate: `tokio`（异步运行时）
- Rust crate: `serde`（序列化）

---

## 🚀 下一步行动

1. **立即开始：** Step 1 - 配置系统扩展
2. **并行开发：** Step 2 和 Step 3 可以并行
3. **里程碑检查：** 完成 Step 1-5 后进行第一次演示
4. **用户测试：** 完成 Step 1-8 后邀请 beta 测试者

---

## 📞 联系方式

- 项目负责人：[待填写]
- 技术顾问：[待填写]
- GitHub Issue: [待创建]
- 讨论群组：[待创建]

---

**最后更新：** 2026-05-05  
**文档版本：** v1.0  
**分支：** `feat/ai-dynamic-i18n`
