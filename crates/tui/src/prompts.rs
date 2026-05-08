#![allow(dead_code)]
//! 不同模式的系统提示词。
//!
//! 提示词由编译时加载的可组合层组装而成：
//!   base.md → 性格叠加 → 模式增量 → 审批策略
//!
//! 这样将每个关注点放在各自文件中，使提示词调优成为单文件操作。

use crate::models::SystemPrompt;
use crate::project_context::{ProjectContext, load_project_context_with_parents};
use crate::tui::app::AppMode;
use crate::tui::approval::ApprovalMode;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default)]
pub struct PromptSessionContext<'a> {
    /// 用户记忆块（来自持久化记忆系统）。
    pub user_memory_block: Option<&'a str>,
    /// 当前会话目标 / 目标对象。
    pub goal_objective: Option<&'a str>,
    /// 已解析的 BCP-47 语言区域标签，用于系统提示词中的
    /// `## 环境` 块（例如 `"en"`、`"zh-Hans"`、`"ja"`）。
    /// 调用方负责从 `Settings` 解析此值；提示词构建器内部
    /// 不进行磁盘 I/O，因此系统提示词的工作区静态部分
    /// 保持缓存友好。
    pub locale_tag: &'a str,
}

/// 结构化会话交接工件的约定路径（#32）。
/// 前一个会话在退出或 `/compact` 时写入；下一个会话在启动时读取
/// 并将其前置到系统提示词中，使新的代理不必从头重新发现未解决的阻塞项。
pub const HANDOFF_RELATIVE_PATH: &str = ".deepseek/handoff.md";

/// `instructions = [...]` 条目的每个文件大小上限（#454）。镜像
/// `project_context::load_context_file` 中已有的项目上下文上限，
/// 以防止恶意/过大的包含文件自行撑爆提示词预算。大于此限制的
/// 文件会被截断并附加 `[…已省略]` 标记，而不是完全跳过，
/// 以便模型仍能看到开头部分。
const INSTRUCTIONS_FILE_MAX_BYTES: usize = 100 * 1024;

/// 渲染 `## 环境` 块，列出已解析的语言区域标签、
/// 运行时版本、主机平台、登录 shell 和当前工作目录。
///
/// 此块附加到系统提示词的工作区静态部分
///（模式提示词 + 项目上下文之后，配置指令/技能之前），
/// 以便 `prompts/base.md` 中的 `## 语言` 指令可以引用它，
/// 而模型无需从用户的第一条消息猜测。`locale_tag` 由
/// 调用方从 `Settings` 解析，因此此函数保持无 I/O。
fn render_environment_block(workspace: &Path, locale_tag: &str) -> String {
    let deepseek_version = env!("CARGO_PKG_VERSION");
    let platform = std::env::consts::OS;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "未知".to_string());
    let pwd = workspace.display();

    format!(
        "## 环境\n\
         \n\
         - 语言：{locale_tag}\n\
         - deepseek_版本：{deepseek_version}\n\
         - 平台：{platform}\n\
         - shell：{shell}\n\
         - 当前目录：{pwd}"
    )
}

/// 将 `instructions = [...]` 配置数组渲染为单个
/// 系统提示词块（#454）。按声明顺序加载每个路径；
/// 缺失的文件会跳过并发出追踪警告，这样 `~/.deepseek/config.toml` 中的
/// 过期条目不会导致启动失败。空输入（或所有路径缺失）返回 `None`，
/// 以便调用者不追加任何内容。
fn render_instructions_block(paths: &[PathBuf]) -> Option<String> {
    let mut sections: Vec<String> = Vec::new();
    for path in paths {
        match std::fs::read_to_string(path) {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let body = if trimmed.len() > INSTRUCTIONS_FILE_MAX_BYTES {
                    let head_end = (0..=INSTRUCTIONS_FILE_MAX_BYTES)
                        .rev()
                        .find(|&i| trimmed.is_char_boundary(i))
                        .unwrap_or(0);
                    format!("{}\n[…已省略]", &trimmed[..head_end])
                } else {
                    trimmed.to_string()
                };
                sections.push(format!(
                    "<instructions source=\"{}\">\n{}\n</instructions>",
                    path.display(),
                    body
                ));
            }
            Err(err) => {
                tracing::warn!(
                    target: "instructions",
                    ?err,
                    ?path,
                    "跳过不可读的指令文件"
                );
            }
        }
    }
    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n\n"))
    }
}

/// 读取工作区本地的交接工件（如果存在）并将其格式化为
/// 系统提示词块。当文件不存在或为空时返回 `None`，
/// 以便调用者为新工作区保持默认不杂乱的状态。
fn load_handoff_block(workspace: &Path) -> Option<String> {
    let path = workspace.join(HANDOFF_RELATIVE_PATH);
    let raw = std::fs::read_to_string(&path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(format!(
        "## 前一个会话的交接信息\n\n前一个会话在此工作区于 `{}` 留下了交接信息。请将其视为本轮要读取的第一个工件——未解决的阻塞项、进行中的变更和最近的决策都在其中。如果状态发生实质性变化，请在退出前更新或重写它。\n\n{}",
        HANDOFF_RELATIVE_PATH, trimmed
    ))
}

// ── 编译时加载的提示词层 ──────────────────────────────────────────────

/// 核心：任务执行、工具使用规则、输出格式、工具箱参考、
/// "何时不使用" 指南、子代理标记协议。
pub const BASE_PROMPT: &str = include_str!("prompts/base.md");

/// 性格叠加层——语气和风格。
pub const CALM_PERSONALITY: &str = include_str!("prompts/personalities/calm.md");
pub const PLAYFUL_PERSONALITY: &str = include_str!("prompts/personalities/playful.md");

/// 模式增量——权限、工作流期望、模式特定规则。
pub const AGENT_MODE: &str = include_str!("prompts/modes/agent.md");
pub const PLAN_MODE: &str = include_str!("prompts/modes/plan.md");
pub const YOLO_MODE: &str = include_str!("prompts/modes/yolo.md");

/// 审批策略叠加层——工具调用是自动批准、
/// 需要确认还是被阻止。
pub const AUTO_APPROVAL: &str = include_str!("prompts/approvals/auto.md");
pub const SUGGEST_APPROVAL: &str = include_str!("prompts/approvals/suggest.md");
pub const NEVER_APPROVAL: &str = include_str!("prompts/approvals/never.md");

/// 压缩交接模板——写入系统提示词，使
/// 模型知道写入 `.deepseek/handoff.md` 时应使用的格式。
pub const COMPACT_TEMPLATE: &str = include_str!("prompts/compact.md");

// ── 遗留提示词常量（为向后兼容保留）────────────────────────

/// 遗留基础提示词（agent.txt — 现已拆分为 base.md + 叠加层）。
/// 仍然可用于尚未迁移到分层 API 的调用者。
pub const AGENT_PROMPT: &str = include_str!("prompts/agent.txt");
pub const YOLO_PROMPT: &str = include_str!("prompts/yolo.txt");
pub const PLAN_PROMPT: &str = include_str!("prompts/plan.txt");

// ── 性格选择 ─────────────────────────────────────────────

/// 应用哪种性格叠加层。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Personality {
    /// 冷静、克制、沉稳——默认值。
    Calm,
    /// 温暖、有活力、俏皮——趣味模式的替代选择。
    Playful,
}

impl Personality {
    /// 从设置的 `calm_mode` 标志解析。
    /// 当 `calm_mode` 为 true → Calm；为 false → Playful（将来）。
    /// 目前，始终返回 Calm——Playful 已连接但需要选择加入。
    #[must_use]
    pub fn from_settings(calm_mode: bool) -> Self {
        if calm_mode {
            Self::Calm
        } else {
            // 未来：当在设置中暴露 playful 模式时，在此返回 Playful。
            // 目前，calm 是唯一的默认值。
            Self::Calm
        }
    }

    fn prompt(self) -> &'static str {
        match self {
            Self::Calm => CALM_PERSONALITY,
            Self::Playful => PLAYFUL_PERSONALITY,
        }
    }
}

// ── 组合 ───────────────────────────────────────────────────────

fn mode_prompt(mode: AppMode) -> &'static str {
    match mode {
        AppMode::Agent => AGENT_MODE,
        AppMode::Yolo => YOLO_MODE,
        AppMode::Plan => PLAN_MODE,
    }
}

fn default_approval_mode_for_mode(mode: AppMode) -> ApprovalMode {
    match mode {
        AppMode::Agent => ApprovalMode::Suggest,
        AppMode::Yolo => ApprovalMode::Auto,
        AppMode::Plan => ApprovalMode::Never,
    }
}

fn approval_prompt_for_mode(mode: AppMode, approval_mode: ApprovalMode) -> &'static str {
    match mode {
        AppMode::Yolo => AUTO_APPROVAL,
        AppMode::Plan => NEVER_APPROVAL,
        AppMode::Agent => match approval_mode {
            ApprovalMode::Auto => AUTO_APPROVAL,
            ApprovalMode::Suggest => SUGGEST_APPROVAL,
            ApprovalMode::Never => NEVER_APPROVAL,
        },
    }
}

/// 按确定顺序组合完整系统提示词：
///   1. base.md        —— 核心身份、工具箱、执行契约
///   2. personality    —— 语气和风格叠加层
///   3. mode delta     —— 模式特定权限和工作流
///   4. approval policy —— 工具审批行为
///
/// 每层之间用空行分隔，以便在渲染的提示词中具有可读性
///（模型将它们视为连续的部分）。
pub fn compose_prompt(mode: AppMode, personality: Personality) -> String {
    compose_prompt_with_approval(mode, personality, default_approval_mode_for_mode(mode))
}

pub fn compose_prompt_with_approval(
    mode: AppMode,
    personality: Personality,
    approval_mode: ApprovalMode,
) -> String {
    let parts: [&str; 4] = [
        BASE_PROMPT.trim(),
        personality.prompt().trim(),
        mode_prompt(mode).trim(),
        approval_prompt_for_mode(mode, approval_mode).trim(),
    ];

    let mut out =
        String::with_capacity(parts.iter().map(|p| p.len()).sum::<usize>() + (parts.len() - 1) * 2);
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            out.push('\n');
            out.push('\n');
        }
        out.push_str(part);
    }
    out
}

/// 使用默认性格（Calm）组合。
fn compose_mode_prompt(mode: AppMode) -> String {
    compose_prompt(mode, Personality::Calm)
}

fn compose_mode_prompt_with_approval(mode: AppMode, approval_mode: ApprovalMode) -> String {
    compose_prompt_with_approval(mode, Personality::Calm, approval_mode)
}

// ── Public API ────────────────────────────────────────────────────────

/// 获取特定模式的系统提示词（默认 Calm 性格）。
pub fn system_prompt_for_mode(mode: AppMode) -> SystemPrompt {
    SystemPrompt::Text(compose_mode_prompt(mode))
}

/// 获取特定模式及显式性格的系统提示词。
pub fn system_prompt_for_mode_with_personality(
    mode: AppMode,
    personality: Personality,
) -> SystemPrompt {
    SystemPrompt::Text(compose_prompt(mode, personality))
}

/// 获取特定模式及项目上下文的系统提示词。
pub fn system_prompt_for_mode_with_context(
    mode: AppMode,
    workspace: &Path,
    working_set_summary: Option<&str>,
) -> SystemPrompt {
    system_prompt_for_mode_with_context_and_skills(
        mode,
        workspace,
        working_set_summary,
        None,
        None,
        None,
    )
}

/// 获取特定模式及项目和技能上下文的系统提示词。
///
/// **易变内容最后的不变性。** 块按从最静态到最易变的顺序追加，
/// 以便 DeepSeek 的 KV 前缀缓存在回合之间命中尽可能长的字节前缀：
///
///   1. 模式提示词（编译时常量）
///   2. 项目上下文 / 回退（工作区静态）
///   3. 技能块（技能目录静态）
///   4. `## 上下文管理`（编译时常量，仅 Agent/Yolo）
///   5. 压缩交接模板（编译时常量）
///   6. 交接块——基于文件；由 `/compact` 和退出时重写
///
/// 在易变块之后追加的任何内容都会放弃请求其余部分的缓存。
/// 新块应属于交接边界之上，除非它们本身就是回合易变的。
/// 工作集元数据现在作为每回合元数据注入到最新的用户消息中，
/// 而不是在此系统提示词中。
pub fn system_prompt_for_mode_with_context_and_skills(
    mode: AppMode,
    workspace: &Path,
    working_set_summary: Option<&str>,
    skills_dir: Option<&Path>,
    instructions: Option<&[PathBuf]>,
    user_memory_block: Option<&str>,
) -> SystemPrompt {
    system_prompt_for_mode_with_context_skills_and_session(
        mode,
        workspace,
        working_set_summary,
        skills_dir,
        instructions,
        PromptSessionContext {
            user_memory_block,
            goal_objective: None,
            locale_tag: "en",
        },
    )
}

pub fn system_prompt_for_mode_with_context_skills_and_session(
    mode: AppMode,
    workspace: &Path,
    _working_set_summary: Option<&str>,
    skills_dir: Option<&Path>,
    instructions: Option<&[PathBuf]>,
    session_context: PromptSessionContext<'_>,
) -> SystemPrompt {
    system_prompt_for_mode_with_context_skills_session_and_approval(
        mode,
        workspace,
        _working_set_summary,
        skills_dir,
        instructions,
        session_context,
        default_approval_mode_for_mode(mode),
    )
}

pub fn system_prompt_for_mode_with_context_skills_session_and_approval(
    mode: AppMode,
    workspace: &Path,
    _working_set_summary: Option<&str>,
    skills_dir: Option<&Path>,
    instructions: Option<&[PathBuf]>,
    session_context: PromptSessionContext<'_>,
    approval_mode: ApprovalMode,
) -> SystemPrompt {
    let mode_prompt = compose_mode_prompt_with_approval(mode, approval_mode);

    // 从工作区加载项目上下文
    let project_context = load_project_context_with_parents(workspace);

    // 1–2. 模式提示词 + 项目上下文。
    // `load_project_context_with_parents` 会在无上下文文件时自动生成
    // .deepseek/instructions.md，因此回退应始终可用。
    let mut full_prompt = if let Some(project_block) = project_context.as_system_block() {
        format!("{}\n\n{}", mode_prompt, project_block)
    } else {
        // 极不可能：上下文生成失败（如文件系统错误）。
        // 仅使用模式提示词而非 panic。
        tracing::warn!("No project context available and auto-generation failed");
        mode_prompt
    };

    // 2.25. 环境块 — 语言区域、平台、shell、当前目录。所有
    // 四个输入在会话期间稳定（工作区路径在运行期间固定；
    // 语言区域由调用方加载一次；平台/shell 来自进程环境）。
    // 插入到指令/技能之上，使其与模式提示词和项目上下文
    // 一起保留在工作区静态缓存层中。
    full_prompt = format!(
        "{full_prompt}\n\n{}",
        render_environment_block(workspace, session_context.locale_tag),
    );

    // 2.5a. 配置的 `instructions = [...]` 文件（#454）。按声明顺序
    // 加载和拼接。位于技能块之上，使其成为 KV
    // 前缀缓存可以命中的工作区静态层的一部分，从而使每个项目的覆盖
    // 在回合之间一致地应用。
    if let Some(paths) = instructions
        && let Some(block) = render_instructions_block(paths)
    {
        full_prompt = format!("{full_prompt}\n\n{block}");
    }

    // 2.5b. 用户记忆块（#489）。位于技能/上下文管理之上，
    // 因为它是会话稳定的：当用户通过 `/memory` 或 `# foo` 快速添加
    // 编辑记忆文件时才会改变，但不会在回合之间改变。
    if let Some(memory_block) = session_context.user_memory_block
        && !memory_block.trim().is_empty()
    {
        full_prompt = format!("{full_prompt}\n\n{memory_block}");
    }

    if let Some(goal_objective) = session_context.goal_objective
        && !goal_objective.trim().is_empty()
    {
        full_prompt = format!(
            "{full_prompt}\n\n## 当前会话目标\n\n<session_goal>\n{}\n</session_goal>",
            goal_objective.trim()
        );
    }

    // 3. 技能块。 #432：遍历每个候选工作区
    // 技能目录（`.agents/skills`、`skills`、
    // `.opencode/skills`、`.claude/skills`、`.cursor/skills`）以及全局
    // `~/.agents/skills` / `~/.deepseek/skills`，以便为任何
    // AI 工具约定安装的技能显示在目录中。遗留的
    // 单一 `skills_dir` 路径作为未提供工作区感知视图的
    // 调用者的回退被保留；它在可用时会
    // 回退到相同的合并注册表。
    let skills_block = crate::skills::render_available_skills_context_for_workspace(workspace)
        .or_else(|| skills_dir.and_then(crate::skills::render_available_skills_context));
    if let Some(block) = skills_block {
        full_prompt = format!("{full_prompt}\n\n{block}");
    }

    // 4. 上下文管理（仅 Agent / Yolo 模式）。
    if matches!(mode, AppMode::Agent | AppMode::Yolo) {
        full_prompt.push_str(
            "\n\n## 上下文管理\n\n\
             当对话变长时（你会看到一个上下文使用指示器），你可以：\n\
             1. 使用 `/compact` 总结之前的上下文并释放空间\n\
             2. 系统会保留重要信息（你正在处理的文件、最近的消息、工具结果）\n\
             3. 压缩后，你将看到讨论内容的摘要，并可以无缝继续\n\n\
             如果你注意到上下文变长（>80%），主动建议用户使用 `/compact`。\n\n\
             ### 提示词缓存意识\n\n\
             DeepSeek 缓存每个请求的最长*字节稳定前缀*，缓存命中 token 的收费大约是未命中 token 的 1/100。上面的系统提示词特意按最静态优先的方式分层，以使前缀在回合间保持稳定。为保持高缓存命中率：\n\
             - **工作集位置：** 当前仓库工作集存储在新用户消息的 `<turn_meta>` 块中。将其视为高优先级的回合元数据，而不是稳定的系统提示词章节。\n\
             - **追加，不要重新排序。** 新上下文放在末尾（最新的用户/工具消息）。重新排列之前的消息或重写其内容会使更改后的所有内容的缓存失效。\n\
             - **不要改写引用的内容。** 如果你已经读取了一个文件，通过路径或行号范围引用它，而不是用不同的格式重新引用。\n\
             - **将 `/compact` 作为硬重置，而非微调。** 压缩适用于缓存已经失效的情况——它有意将前缀重写为更短的摘要。不要为小的收益触发它。\n\
             - **读取一次，回头引用。** 重新读取同一文件会产生与之前读取不同的工具结果外壳；回滚查找比重新获取更便宜。\n\
             - **底部标识：** `cache hit %` 标识在低于 40% 时变为红色，低于 80% 时变为黄色。如果已经连续数回合为红色，这表明需要整合了。"
        );
    }

    // 5. 压缩交接模板——使模型知道在退出或 `/compact` 时
    //    写入 `.deepseek/handoff.md` 应使用的格式。
    full_prompt.push_str("\n\n");
    full_prompt.push_str(COMPACT_TEMPLATE);

    // ── 易变内容边界 ─────────────────────────────────────────
    // 以下所有内容在会话中途会发生变化，并破坏后续字节的前缀缓存。
    // 保持新的静态块在此注释之上。

    // 6. 前一个会话的交接信息（基于文件，由 `/compact` 重写）。
    if let Some(handoff_block) = load_handoff_block(workspace) {
        full_prompt = format!("{full_prompt}\n\n{handoff_block}");
    }

    SystemPrompt::Text(full_prompt)
}

/// 用显式项目上下文构建系统提示词
pub fn build_system_prompt(base: &str, project_context: Option<&ProjectContext>) -> SystemPrompt {
    let full_prompt =
        match project_context.and_then(super::project_context::ProjectContext::as_system_block) {
            Some(project_block) => format!("{}\n\n{}", base.trim(), project_block),
            None => base.trim().to_string(),
        };
    SystemPrompt::Text(full_prompt)
}

// ── 为向后兼容保留的遗留函数 ────────────────────────────

pub fn base_system_prompt() -> SystemPrompt {
    SystemPrompt::Text(BASE_PROMPT.trim().to_string())
}

pub fn normal_system_prompt() -> SystemPrompt {
    system_prompt_for_mode(AppMode::Agent)
}

pub fn agent_system_prompt() -> SystemPrompt {
    system_prompt_for_mode(AppMode::Agent)
}

pub fn yolo_system_prompt() -> SystemPrompt {
    system_prompt_for_mode(AppMode::Yolo)
}

pub fn plan_system_prompt() -> SystemPrompt {
    system_prompt_for_mode(AppMode::Plan)
}

#[cfg(test)]
mod tests {
    // 不要对散文内容进行断言。如果改变措辞不会导致代码审查失败，
    // 就不应该导致测试失败。
    use super::*;
    use tempfile::tempdir;

    /// 注入的交接块的唯一标记（不会出现在
    /// agent 提示词本身对该约定的讨论中）。
    const HANDOFF_BLOCK_MARKER: &str = "于 `.deepseek/handoff.md` 留下了交接信息";

    #[test]
    fn render_environment_block_lists_supplied_locale_and_workspace() {
        let tmp = tempdir().expect("tempdir");
        let block = render_environment_block(tmp.path(), "zh-Hans");
        assert!(block.starts_with("## 环境"));
        assert!(block.contains("- 语言：zh-Hans"));
        assert!(block.contains(&format!(
            "- deepseek_版本：{}",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(block.contains(&format!("- 当前目录：{}", tmp.path().display())));
        assert!(block.contains("- 平台："));
        assert!(block.contains("- shell："));
    }

    #[test]
    fn environment_block_is_inserted_into_system_prompt() {
        let tmp = tempdir().expect("tempdir");
        let prompt = match system_prompt_for_mode_with_context_skills_and_session(
            AppMode::Agent,
            tmp.path(),
            None,
            None,
            None,
            PromptSessionContext {
                user_memory_block: None,
                goal_objective: None,
                locale_tag: "ja",
            },
        ) {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
        };
        assert!(prompt.contains("## 环境"));
        assert!(prompt.contains("- 语言：ja"));
        assert!(prompt.contains("- deepseek_版本："));
    }

    #[test]
    fn handoff_artifact_is_prepended_to_system_prompt_when_present() {
        let tmp = tempdir().expect("tempdir");
        let workspace = tmp.path();
        let handoff_dir = workspace.join(".deepseek");
        std::fs::create_dir_all(&handoff_dir).unwrap();
        std::fs::write(
            handoff_dir.join("handoff.md"),
            "# Session handoff — prior\n\n## Active task\nFinish #32.\n\n## Open blockers\n- [ ] write the basic version\n",
        )
        .unwrap();

        let prompt = match system_prompt_for_mode_with_context(AppMode::Agent, workspace, None) {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
        };

        assert!(prompt.contains(HANDOFF_BLOCK_MARKER));
        assert!(prompt.contains("Finish #32."));
        assert!(prompt.contains("write the basic version"));
    }

    #[test]
    fn missing_handoff_does_not_inject_block() {
        let tmp = tempdir().expect("tempdir");
        let prompt = match system_prompt_for_mode_with_context(AppMode::Agent, tmp.path(), None) {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
        };
        assert!(!prompt.contains(HANDOFF_BLOCK_MARKER));
    }

    #[test]
    fn empty_handoff_file_does_not_inject_block() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path().join(".deepseek");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("handoff.md"), "   \n\n  ").unwrap();
        let prompt = match system_prompt_for_mode_with_context(AppMode::Agent, tmp.path(), None) {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
        };
        assert!(!prompt.contains(HANDOFF_BLOCK_MARKER));
    }

    #[test]
    fn compose_prompt_includes_all_layers() {
        let prompt = compose_prompt(AppMode::Agent, Personality::Calm);
        // 基础层
        assert!(prompt.contains("你是 DeepSeek TUI"));
        // 性格层
        assert!(prompt.contains("性格：冷静"));
        // 模式层
        assert!(prompt.contains("模式：Agent"));
        // 审批层
        assert!(prompt.contains("审批策略：Suggest"));
    }

    #[test]
    fn package_version_is_current_hotfix_release() {
        assert_eq!(
            env!("CARGO_PKG_VERSION"),
            "0.8.20",
            "0.8.20 release branch must report the release version before publishing"
        );
    }

    #[test]
    fn compose_prompt_deterministic_order() {
        let prompt = compose_prompt(AppMode::Yolo, Personality::Calm);
        let base_pos = prompt.find("你是 DeepSeek TUI").unwrap();
        let personality_pos = prompt.find("性格：冷静").unwrap();
        let mode_pos = prompt.find("模式：YOLO").unwrap();
        let approval_pos = prompt.find("审批策略：Auto").unwrap();

        assert!(base_pos < personality_pos);
        assert!(personality_pos < mode_pos);
        assert!(mode_pos < approval_pos);
    }

    #[test]
    fn each_mode_gets_correct_approval() {
        assert!(
            compose_prompt(AppMode::Agent, Personality::Calm).contains("审批策略：Suggest")
        );
        assert!(compose_prompt(AppMode::Yolo, Personality::Calm).contains("审批策略：Auto"));
        assert!(
            compose_prompt(AppMode::Plan, Personality::Calm).contains("审批策略：Never")
        );
    }

    #[test]
    fn agent_prompt_can_reflect_never_approval_policy() {
        let prompt =
            compose_prompt_with_approval(AppMode::Agent, Personality::Calm, ApprovalMode::Never);
        assert!(prompt.contains("模式：Agent"));
        assert!(prompt.contains("审批策略：Never"));
        assert!(prompt.contains("/config approval_mode suggest"));
    }

    #[test]
    fn personality_switches_correctly() {
        let calm = compose_prompt(AppMode::Agent, Personality::Calm);
        let playful = compose_prompt(AppMode::Agent, Personality::Playful);
        assert!(calm.contains("性格：冷静"));
        assert!(playful.contains("性格：活泼"));
        assert!(!calm.contains("性格：活泼"));
    }

    #[test]
    fn compact_template_is_included_in_full_prompt() {
        let tmp = tempdir().expect("tempdir");
        let prompt = match system_prompt_for_mode_with_context(AppMode::Agent, tmp.path(), None) {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
        };
        assert!(prompt.contains("## 压缩交接"));
        // #429: 结构化 Markdown 模板。目标/约束条件/进展
        //（已完成/进行中/阻塞）/关键决策/下一步。
        assert!(prompt.contains("### 目标"));
        assert!(prompt.contains("### 约束条件"));
        assert!(prompt.contains("### 进展"));
        assert!(prompt.contains("#### 已完成"));
        assert!(prompt.contains("#### 进行中"));
        assert!(prompt.contains("#### 阻塞"));
        assert!(prompt.contains("### 关键决策"));
        assert!(prompt.contains("### 下一步"));
    }

    #[test]
    fn session_goal_is_injected_above_handoff_tail() {
        let tmp = tempdir().expect("tempdir");
        let prompt = match system_prompt_for_mode_with_context_skills_and_session(
            AppMode::Agent,
            tmp.path(),
            Some("## Repo Working Set\nsrc/lib.rs"),
            None,
            None,
            PromptSessionContext {
                user_memory_block: None,
                goal_objective: Some("Fix transcript corruption"),
                locale_tag: "en",
            },
        ) {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
        };

        let goal_pos = prompt.find("<session_goal>").expect("goal block");
        let compact_pos = prompt.find("## 压缩交接").expect("compact block");

        assert!(prompt.contains("Fix transcript corruption"));
        assert!(goal_pos < compact_pos);
        assert!(!prompt.contains("src/lib.rs"));
    }

    #[test]
    fn empty_session_goal_is_not_injected() {
        let tmp = tempdir().expect("tempdir");
        let prompt = match system_prompt_for_mode_with_context_skills_and_session(
            AppMode::Agent,
            tmp.path(),
            None,
            None,
            None,
            PromptSessionContext {
                user_memory_block: None,
                goal_objective: Some("   "),
                locale_tag: "en",
            },
        ) {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
        };

        assert!(!prompt.contains("<session_goal>"));
        assert!(!prompt.contains("## 当前会话目标"));
    }

    #[test]
    fn tool_selection_guide_avoids_defensive_tool_suppression() {
        let prompt = compose_prompt(AppMode::Agent, Personality::Calm);
        assert!(prompt.contains("工具选择指南"));
        assert!(prompt.contains("使用 `agent_result`"));
        assert!(
            !prompt.contains("何时不使用特定工具"),
            "系统提示词应引导工具选择，而不是训练模型避免可用工具"
        );
        assert!(
            !prompt.contains("不要使用"),
            "避免基础提示词中的防御性反工具措辞"
        );
    }

    /// #588: 语言镜像指令必须在所有模式中包含，以便
    /// DeepSeek 的 `reasoning_content` 和最终回复遵循用户的
    /// 语言。结构性测试——措辞不是测试关注点，但
    /// #588 的跨层面承诺特别要求
    /// `reasoning_content` 字段跟踪用户的语言（不仅仅是
    /// 可见的回复）；固定这个锚点标记以防止未来的编辑
    /// 在保持标题的情况下静默地将该部分弱化为通用的"用用户的语言回复"指令。
    #[test]
    fn language_mirroring_section_present_in_all_modes() {
        for mode in [AppMode::Agent, AppMode::Yolo, AppMode::Plan] {
            let prompt = compose_prompt(mode, Personality::Calm);
            assert!(
                prompt.contains("## 语言"),
                "所有模式都需要包含 ## 语言 章节，但 {mode:?} 缺失"
            );
            assert!(
                prompt.contains("reasoning_content"),
                "{mode:?} 中的 ## 语言 章节必须提及 `reasoning_content` — \
                 该字段名是 #588 承诺的结构性锚点，即 \
                 内部推理（不仅仅是可见回复）需遵循用户语言"
            );
        }
    }

    #[test]
    fn language_mirroring_prioritizes_latest_user_message_over_locale_default() {
        let prompt = compose_prompt(AppMode::Agent, Personality::Calm);
        assert!(
            prompt.contains("优先从最新用户消息"),
            "语言指令必须优先使用最新用户消息而非区域默认值"
        );
        assert!(
            prompt.contains("即使 `## 环境` 中的 `lang` 字段为 `en`"),
            "中文用户消息必须在 reasoning_content 中覆盖英文解析区域"
        );
        assert!(
            prompt.contains("仅在最新用户消息缺失"),
            "环境区域应作为歧义时的回退，而非主要语言来源"
        );
    }

    /// #358: rlm 指导从"一等工具"重构为"专用
    /// 工具"——验证结构性标记存在，以防止未来的
    /// 更改静默地完全移除 RLM 章节。
    ///
    /// 不要对散文内容进行断言。如果改变措辞不会导致代码审查失败，
    /// 就不应该导致测试失败。
    #[test]
    fn rlm_specialty_tool_guidance_present() {
        let prompt = compose_prompt(AppMode::Agent, Personality::Calm);
        // 结构性：RLM 标题必须作为章节锚点存在。
        assert!(prompt.contains("RLM — 使用方式"));
        // 结构性："rlm" 单词必须多次出现（工具
        // 名称、章节标题、工具箱参考）。只需验证
        // 小写形式——确切措辞不是测试关注点。
        let rlm_count = prompt.to_lowercase().matches("rlm").count();
        assert!(
            rlm_count >= 5,
            "RLM 指导存在：期望至少 5 处提及 'rlm'，实际 {rlm_count}"
        );
        assert!(
            !prompt.contains("何时不使用 RLM"),
            "RLM 指导应解释适用场景和验证方式，而不是告诉模型避免使用该工具"
        );
    }

    #[test]
    fn subagent_done_sentinel_section_present() {
        let prompt = compose_prompt(AppMode::Agent, Personality::Calm);
        assert!(prompt.contains("内部子代理完成事件"));
        assert!(prompt.contains("<deepseek:subagent.done>"));
        assert!(prompt.contains("不是用户输入"));
        assert!(prompt.contains("整合协议"));
        assert!(prompt.contains("不要告诉用户他们粘贴了哨兵标记"));
    }

    #[test]
    fn preamble_rhythm_section_present() {
        let prompt = compose_prompt(AppMode::Agent, Personality::Calm);
        assert!(prompt.contains("开场节奏"));
        assert!(prompt.contains("我先读取模块结构"));
    }

    #[test]
    fn legacy_constants_still_available() {
        // 验证旧的 .txt 常量仍然可编译且包含预期内容
        assert!(!AGENT_PROMPT.is_empty());
        assert!(!YOLO_PROMPT.is_empty());
        assert!(!PLAN_PROMPT.is_empty());
    }

    // ── 缓存前缀稳定性测试套件（#263 第 2 步）───────────────────────
    //
    // 这些测试固定了 DeepSeek 的 KV 前缀缓存命中所需的字节稳定性不变量：
    // 任何最终出现在缓存前缀中的提示词构造接口，在给定相同输入的情况下，
    // 必须在调用之间产生相同的字节。

    use crate::test_support::assert_byte_identical;

    #[test]
    fn compose_prompt_is_byte_stable_across_calls() {
        // #263 中的怀疑 #4：单模式内的模式提示词变动。
        // 两次具有相同（模式、性格）输入的调用必须产生
        // 相同的字节——否则就是缓存破坏者。
        for mode in [AppMode::Agent, AppMode::Yolo, AppMode::Plan] {
            for personality in [Personality::Calm, Personality::Playful] {
                let a = compose_prompt(mode, personality);
                let b = compose_prompt(mode, personality);
                assert_byte_identical(
                    &format!("compose_prompt(模式={mode:?}, 性格={personality:?})"),
                    &a,
                    &b,
                );
            }
        }
    }

    #[test]
    fn system_prompt_for_mode_with_context_is_byte_stable_for_unchanged_workspace() {
        // 相同工作区，调用之间没有 working_set / skills 变动 →
        // 相同的字节。这固定了最具代表性的生产
        // 表面（engine.rs 每回合通过此函数或其 _and_skills 变体构建系统提示词）。
        let tmp = tempdir().expect("tempdir");
        let workspace = tmp.path();

        for mode in [AppMode::Agent, AppMode::Yolo, AppMode::Plan] {
            let a = match system_prompt_for_mode_with_context(mode, workspace, None) {
                SystemPrompt::Text(text) => text,
                SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
            };
            let b = match system_prompt_for_mode_with_context(mode, workspace, None) {
                SystemPrompt::Text(text) => text,
                SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
            };
            assert_byte_identical(
                &format!("system_prompt_for_mode_with_context(模式={mode:?}) 空工作区"),
                &a,
                &b,
            );
        }
    }

    #[test]
    fn system_prompt_ignores_working_set_summary_argument() {
        // 工作集元数据现在每回合注入到最新的用户消息中。
        // 遗留参数保留用于调用点兼容性，
        // 但不得将易变字节重新引入系统提示词。
        let tmp = tempdir().expect("tempdir");
        let workspace = tmp.path();
        let summary = "## Repo Working Set\nWorkspace: /tmp/x\n";

        let a = match system_prompt_for_mode_with_context(AppMode::Agent, workspace, Some(summary))
        {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
        };
        let b = match system_prompt_for_mode_with_context(AppMode::Agent, workspace, Some(summary))
        {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
        };
        assert_byte_identical(
            "system_prompt_for_mode_with_context 常量工作集摘要",
            &a,
            &b,
        );
        assert!(
            !a.contains(summary),
            "摘要不得嵌入系统提示词中"
        );
    }

    #[test]
    fn system_prompt_with_handoff_file_is_byte_stable_when_file_is_unchanged() {
        // 如果 `.deepseek/handoff.md` 在两次构建之间未移动，
        // 渲染的提示词必须产生相同的字节。交接块
        // 位于 `system_prompt_for_mode_with_context_and_skills` 的静态边界之下。
        let tmp = tempdir().expect("tempdir");
        let workspace = tmp.path();
        let handoff_dir = workspace.join(".deepseek");
        std::fs::create_dir_all(&handoff_dir).unwrap();
        std::fs::write(
            handoff_dir.join("handoff.md"),
            "# Session handoff\n\n## Active task\nFinish #280.\n\n## Open blockers\n- [ ] none\n",
        )
        .unwrap();

        let a = match system_prompt_for_mode_with_context(AppMode::Agent, workspace, None) {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
        };
        let b = match system_prompt_for_mode_with_context(AppMode::Agent, workspace, None) {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
        };
        assert_byte_identical(
            "system_prompt_for_mode_with_context 常量交接文件",
            &a,
            &b,
        );
        assert!(a.contains(HANDOFF_BLOCK_MARKER), "交接块必须被嵌入");
        assert!(a.contains("Finish #280."), "交接内容必须存在");
    }

    #[test]
    fn handoff_appears_after_static_blocks_without_working_set() {
        // 缓存前缀不变量：交接块必须在静态的 `## 上下文管理` 和
        // 压缩交接模板（`## 压缩交接`）之后。
        // 工作集元数据现在是每回合的用户元数据，而不是系统提示词的尾部块。
        let tmp = tempdir().expect("tempdir");
        let workspace = tmp.path();
        let handoff_dir = workspace.join(".deepseek");
        std::fs::create_dir_all(&handoff_dir).unwrap();
        std::fs::write(handoff_dir.join("handoff.md"), "# handoff body\n").unwrap();

        let summary = "## Repo Working Set\nWorkspace: /tmp/x\n";
        let prompt =
            match system_prompt_for_mode_with_context(AppMode::Agent, workspace, Some(summary)) {
                SystemPrompt::Text(text) => text,
                SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
            };

        let context_pos = prompt
            .find("## 上下文管理")
            .expect("Agent 模式中应包含上下文管理章节");
        let compact_pos = prompt
            .find("## 压缩交接")
            .expect("应包含压缩交接模板");
        let handoff_pos = prompt
            .find(HANDOFF_BLOCK_MARKER)
            .expect("handoff block present when fixture file exists");
        assert!(
            !prompt.contains("## Repo Working Set"),
            "工作集摘要不得出现在系统提示词中"
        );

        assert!(
            context_pos < handoff_pos,
            "上下文管理章节必须在交接块之前"
        );
        assert!(
            compact_pos < handoff_pos,
            "压缩交接章节必须在交接块之前"
        );
    }

    #[test]
    fn render_instructions_block_returns_none_for_empty_input() {
        assert!(super::render_instructions_block(&[]).is_none());
    }

    #[test]
    fn render_instructions_block_skips_missing_files_with_warning() {
        let tmp = tempdir().expect("tempdir");
        let real = tmp.path().join("real.md");
        std::fs::write(&real, "real content here").unwrap();
        let bogus = tmp.path().join("does-not-exist.md");

        let block = super::render_instructions_block(&[bogus.clone(), real.clone()])
            .expect("存在的文件应产生一个块");
        assert!(block.contains("real content here"));
        assert!(block.contains(&real.display().to_string()));
        // 不存在的路径被跳过，不渲染。
        assert!(!block.contains(&bogus.display().to_string()));
    }

    #[test]
    fn render_instructions_block_concatenates_in_declared_order() {
        let tmp = tempdir().expect("tempdir");
        let a = tmp.path().join("a.md");
        let b = tmp.path().join("b.md");
        std::fs::write(&a, "ALPHA_MARKER").unwrap();
        std::fs::write(&b, "BRAVO_MARKER").unwrap();

        let block = super::render_instructions_block(&[a, b]).expect("non-empty");
        let alpha_pos = block.find("ALPHA_MARKER").expect("alpha 已渲染");
        let bravo_pos = block.find("BRAVO_MARKER").expect("bravo 已渲染");
        assert!(
            alpha_pos < bravo_pos,
            "instructions 必须按声明顺序拼接"
        );
    }

    #[test]
    fn render_instructions_block_skips_empty_files() {
        let tmp = tempdir().expect("tempdir");
        let empty = tmp.path().join("empty.md");
        let real = tmp.path().join("real.md");
        std::fs::write(&empty, "   \n   \n").unwrap();
        std::fs::write(&real, "real content").unwrap();

        let block = super::render_instructions_block(&[empty, real]).expect("non-empty");
        // 空文件不会产生 `<instructions>` 章节，只有非空文件会产生。
        let count = block.matches("<instructions").count();
        assert_eq!(count, 1, "只有非空文件应产生章节");
    }

    #[test]
    fn render_instructions_block_truncates_oversize_files() {
        let tmp = tempdir().expect("tempdir");
        let big = tmp.path().join("big.md");
        // 200 KiB 的内容——远超 100 KiB 的上限。
        std::fs::write(&big, "X".repeat(200 * 1024)).unwrap();

        let block = super::render_instructions_block(&[big]).expect("non-empty");
        assert!(block.contains("[…已省略]"), "缺少截断标记");
        // 块应比原始文件小得多。
        assert!(
            block.len() < 110 * 1024,
            "块大小应被限制在接近 100 KiB"
        );
    }

    #[test]
    fn instructions_block_appears_in_system_prompt_when_configured() {
        let tmp = tempdir().expect("tempdir");
        let workspace = tmp.path();
        let extra = workspace.join("extra-instructions.md");
        std::fs::write(&extra, "EXTRA_INSTRUCTIONS_MARKER_BODY").unwrap();

        let prompt = match super::system_prompt_for_mode_with_context_and_skills(
            AppMode::Agent,
            workspace,
            None,
            None,
            Some(std::slice::from_ref(&extra)),
            None,
        ) {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("期望文本类型系统提示词"),
        };

        assert!(
            prompt.contains("EXTRA_INSTRUCTIONS_MARKER_BODY"),
            "配置的 instructions 文件内容必须出现在提示词中"
        );
        assert!(
            prompt.contains(&extra.display().to_string()),
            "instructions 块必须标注其源路径"
        );
    }
}
