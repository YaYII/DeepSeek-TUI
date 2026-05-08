//! 本地 SKILL.md 文件的技能发现和注册表。

pub mod install;
mod system;
// 为文档一致性和下游消费者保留重导出；二进制文件本身直接从
// `skills::install` 导入。`#[allow(...)]` 用于消除死代码警告，
// 因为没有任何 `bin` 源代码路径通过 `skills::*` 引用这些名称。
#[allow(unused_imports)]
pub use install::{
    DEFAULT_MAX_SIZE_BYTES, DEFAULT_REGISTRY_URL, INSTALLED_FROM_MARKER, InstallOutcome,
    InstallSource, InstalledSkill, RegistryDocument, RegistryEntry, RegistryFetchResult,
    SkillSyncOutcome, SyncResult, UpdateResult, default_cache_skills_dir,
};
pub use system::install_system_skills;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};

use crate::logging;

const MAX_SKILL_DESCRIPTION_CHARS: usize = 512;
const MAX_AVAILABLE_SKILLS_CHARS: usize = 12_000;

// === Defaults ===

#[allow(dead_code)]
#[must_use]
pub fn default_skills_dir() -> PathBuf {
    dirs::home_dir().map_or_else(
        || PathBuf::from("/tmp/deepseek/skills"),
        |p| p.join(".deepseek").join("skills"),
    )
}

/// 全局 agentskills.io 兼容技能目录（`~/.agents/skills`）。
#[must_use]
pub fn agents_global_skills_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|p| p.join(".agents").join("skills"))
}

/// 全局 Claude 兼容技能目录（`~/.claude/skills`）。SKILL.md
/// 前置元数据约定在更广泛的 Claude 生态系统中共享，因此引入全局路径
/// 使用户可以继承他们已为其他 Claude 兼容工具安装的技能，
/// 而无需在 DeepSeek 的原生布局中重新编写它们（#902）。
#[must_use]
pub fn claude_global_skills_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|p| p.join(".claude").join("skills"))
}

// === Types ===

/// 已解析的 SKILL.md 定义表示。
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub body: String,
    /// 此技能加载来源的 `SKILL.md` 的磁盘路径。对于社区安装或手动放置的技能，
    /// 目录名称可能与前置元数据中的 `name` 不同，因此调用者必须使用此路径
    /// 而非重新构造 `<dir>/<name>/SKILL.md`。
    pub path: PathBuf,
}

/// 已发现技能的集合。
#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    skills: Vec<Skill>,
    warnings: Vec<String>,
}

impl SkillRegistry {
    /// 发现技能时的最大目录遍历深度。
    ///
    /// 防御病态配置（例如用户将 `skills_dir` 指向 `~`），
    /// 同时不人为限制实际的供应商布局，如 `<root>/<org>/<repo>/<skill>/SKILL.md`。
    const MAX_DISCOVERY_DEPTH: usize = 8;

    /// 从给定目录发现技能。
    ///
    /// 搜索递归遍历 `dir`：任何包含 `SKILL.md` 的目录都被加载为一个技能，
    /// 并且遍历**不会**进一步进入该目录（配套文件位于 `SKILL.md` 旁边，
    /// 而 `tools::skill::collect_companion_files` 已将嵌套子目录视为范围外）。
    /// 这使用户可以按供应商/类别组织技能——例如
    /// `<root>/<vendor>/<skill>/SKILL.md`——而不是被迫使用扁平化的
    /// `<root>/<skill>/SKILL.md` 布局。
    ///
    /// 根目录以下的隐藏子目录（名称以 `.` 开头）会被跳过，以避免进入
    /// VCS/缓存树（如 `.git/`）。提供的 `dir` 本身始终被保留，即使它是
    /// 隐藏目录——因为这是用户显式配置的。
    /// 当符号链接解析到目录时会被跟随，通过规范路径跟踪和
    /// [`Self::MAX_DISCOVERY_DEPTH`] 确保在技能布局包含循环时遍历仍保持有限。
    #[must_use]
    pub fn discover(dir: &Path) -> Self {
        let mut registry = Self::default();
        if !dir.exists() {
            return registry;
        }

        let mut visited = HashSet::new();
        Self::discover_recursive(dir, 0, &mut registry, &mut visited);
        registry
    }

    fn discover_recursive(
        dir: &Path,
        depth: usize,
        registry: &mut Self,
        visited: &mut HashSet<PathBuf>,
    ) {
        if depth > Self::MAX_DISCOVERY_DEPTH {
            return;
        }
        if !Self::mark_discovered_dir(dir, visited) {
            return;
        }

        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(err) => {
                // 仅对用户提供的根目录（depth == 0）显示警告。
                // 嵌套的权限错误通常是噪音（例如某人
                // `~/.agents/skills` 中的零散 `.Trash` 目录）。
                if depth == 0 {
                    registry.push_warning(format!(
                        "Failed to read skills directory {}: {err}",
                        dir.display()
                    ));
                }
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            // 跳过隐藏子目录。常见的罪魁祸首是 `.git`、`.cache`、`.Trash`。
            // 提供的根目录本身不受此限制：用户显式地将 `skills_dir` 指向了它，
            // 我们不会过滤它（它直接传递给此函数，而非通过迭代）。
            // 此检查适用于当前目录的*子级*（包括深度0），
            // 因为紧挨着我们想要的技能的 `.git/` 正是我们不得进入的噪音。
            if path
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|name| name.starts_with('.'))
            {
                continue;
            }

            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            if !metadata.is_dir() {
                continue;
            }

            let skill_path = path.join("SKILL.md");
            match fs::read_to_string(&skill_path) {
                Ok(content) => match Self::parse_skill(&skill_path, &content) {
                    Ok(mut skill) => {
                        if !Self::mark_discovered_dir(&path, visited) {
                            continue;
                        }
                        skill.path = skill_path.clone();
                        registry.skills.push(skill);
                        // 此目录就是一个技能。不再继续深入：
                        // 任何嵌套的 `SKILL.md` 都将是父技能附带的
                        // 固定配置或示例，而非可单独安装的技能。
                        continue;
                    }
                    Err(reason) => {
                        if !Self::mark_discovered_dir(&path, visited) {
                            continue;
                        }
                        registry.push_warning(format!(
                            "Failed to parse {}: {reason}",
                            skill_path.display()
                        ));
                        // 仍将此目录视为"已占用"——格式错误的 SKILL.md
                        // 不应导致我们双重加载嵌套的固定配置作为技能。
                        continue;
                    }
                },
                Err(err) if skill_path.exists() => {
                    if !Self::mark_discovered_dir(&path, visited) {
                        continue;
                    }
                    registry
                        .push_warning(format!("Failed to read {}: {err}", skill_path.display()));
                    continue;
                }
                Err(_) => {
                    // 此处没有 SKILL.md——递归查找嵌套的技能目录
                    //（例如 `<vendor>/<skill>/SKILL.md`）。
                }
            }

            Self::discover_recursive(&path, depth + 1, registry, visited);
        }
    }

    fn mark_discovered_dir(dir: &Path, visited: &mut HashSet<PathBuf>) -> bool {
        let key = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        visited.insert(key)
    }

    fn push_warning(&mut self, warning: String) {
        logging::warn(&warning);
        self.warnings.push(warning);
    }

    fn parse_skill(_path: &Path, content: &str) -> std::result::Result<Skill, String> {
        let trimmed = content.trim_start();

        // 先尝试解析前置元数据块。如果不存在，则回退到提取第一个
        // `# Heading` 作为技能名称，以便纯 Markdown 文件（无 `---` 分隔线）
        // 被接受而非拒绝。
        if trimmed.starts_with("---") {
            let start = content
                .find("---")
                .ok_or_else(|| "missing frontmatter opening delimiter".to_string())?;
            let rest = &content[start + 3..];
            let end = rest
                .find("---")
                .ok_or_else(|| "missing frontmatter closing delimiter".to_string())?;
            let frontmatter = &rest[..end];
            let body = &rest[end + 3..];

            let mut metadata = HashMap::new();
            for raw in frontmatter.lines() {
                let line = raw.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once(':') {
                    let value = value.trim();
                    let unquoted = if (value.starts_with('"')
                        && value.ends_with('"')
                        && value.len() >= 2)
                        || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2)
                    {
                        &value[1..value.len() - 1]
                    } else {
                        value
                    };
                    metadata.insert(key.trim().to_ascii_lowercase(), unquoted.to_string());
                }
            }

            let name = metadata
                .get("name")
                .filter(|name| !name.is_empty())
                .cloned()
                .ok_or_else(|| "missing required frontmatter field: name".to_string())?;

            let description = metadata.get("description").cloned().unwrap_or_default();

            return Ok(Skill {
                name,
                description,
                body: body.trim().to_string(),
                // 由 `discover` 在解析成功后填充；默认为空路径以便直接构造者（如测试）编译。
                path: PathBuf::new(),
            });
        }

        // 优雅降级：未找到前置元数据分隔线。
        // 提取第一个 `# Heading` 作为技能名称。
        let heading_re = regex::Regex::new(r"(?m)^#\s+(.+)$").expect("static regex is valid");
        let name = heading_re
            .captures(content)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                "no frontmatter and no `# Heading` found to use as skill name".to_string()
            })?;

        Ok(Skill {
            name,
            description: String::new(),
            body: content.trim().to_string(),
            path: PathBuf::new(),
        })
    }

    /// 按名称查找技能。
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.name == name)
    }

    /// 返回所有已加载的技能。
    pub fn list(&self) -> &[Skill] {
        &self.skills
    }

    /// 发现技能时遇到的解析或 I/O 警告。
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// 检查是否加载了任何技能。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// 返回已加载技能的数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.skills.len()
    }
}

/// 解析给定工作区的活动技能目录，镜像 `App::new` 的遍历层次：
/// `<workspace>/.agents/skills` → `<workspace>/skills` →
/// [`agents_global_skills_dir`]（`~/.agents/skills`，如果存在）→
/// [`default_skills_dir`]（`~/.deepseek/skills`）。
/// 返回第一个存在的目录，或全局默认值（如果用户没有家目录则回退到
/// `/tmp/deepseek/skills`）。
///
/// 为需要单一规范目录的调用者保留（例如"我在哪里安装新技能？"）。
/// 对于需要同时获取跨工具技能文件夹的会话时发现，请使用
/// [`skills_directories`] / [`discover_in_workspace`]（#432）。
#[must_use]
#[allow(dead_code)] // 特意保留作为"单一规范安装目录"接口；实际调用者使用 discover_in_workspace。
pub fn resolve_skills_dir(workspace: &Path) -> PathBuf {
    let agents = workspace.join(".agents").join("skills");
    if agents.exists() {
        return agents;
    }
    let local = workspace.join("skills");
    if local.exists() {
        return local;
    }
    if let Some(global_agents) = agents_global_skills_dir()
        && global_agents.exists()
    {
        return global_agents;
    }
    default_skills_dir()
}

/// 解析工作区的每个候选技能目录，按优先级顺序——最具体的优先。
/// 用于会话时的技能发现，以便模型看到同一工作区中安装的其他
/// AI 工具约定的技能（#432）。
///
/// 优先级（名称冲突时第一个匹配胜出）：
///
/// 1. `<workspace>/.agents/skills` — deepseek 原生约定。
/// 2. `<workspace>/skills` — 扁平的项目本地目录。
/// 3. `<workspace>/.opencode/skills` — OpenCode 互操作。
/// 4. `<workspace>/.claude/skills` — Claude Code 互操作。
/// 5. `<workspace>/.cursor/skills` — Cursor 互操作。
/// 6. [`agents_global_skills_dir`] — agentskills.io 全局目录。
/// 7. [`claude_global_skills_dir`] — Claude 生态系统全局目录（#902）。
/// 8. [`default_skills_dir`] — DeepSeek 全局用户安装目录。
///
/// 仅返回磁盘上存在的目录——调用者无需进一步过滤。
/// 当没有任何安装时返回空向量（系统提示词技能块随后被抑制）。
#[must_use]
pub fn skills_directories(workspace: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![
        workspace.join(".agents").join("skills"),
        workspace.join("skills"),
        workspace.join(".opencode").join("skills"),
        workspace.join(".claude").join("skills"),
        workspace.join(".cursor").join("skills"),
    ];
    if let Some(global_agents) = agents_global_skills_dir() {
        candidates.push(global_agents);
    }
    if let Some(global_claude) = claude_global_skills_dir() {
        candidates.push(global_claude);
    }
    candidates.push(default_skills_dir());
    existing_skill_dirs(candidates)
}

fn existing_skill_dirs(candidates: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in candidates {
        if path.is_dir() && !out.iter().any(|p: &PathBuf| p == &path) {
            out.push(path);
        }
    }
    out
}

/// 遍历工作区的每个候选技能目录，并将发现的技能合并到单个注册表中。
/// 名称冲突按照 [`skills_directories`] 的优先级以先匹配胜出方式解决。
///
/// 来自每个扫描目录的警告会累积，以便模型（和通过 `/skill list` 的用户）
/// 可以看到技能未能加载的原因。
#[must_use]
pub fn discover_in_workspace(workspace: &Path) -> SkillRegistry {
    let mut merged = SkillRegistry::default();
    for dir in skills_directories(workspace) {
        let registry = SkillRegistry::discover(&dir);
        for skill in registry.skills {
            if !merged.skills.iter().any(|s| s.name == skill.name) {
                merged.skills.push(skill);
            }
        }
        for warning in registry.warnings {
            merged.warnings.push(warning);
        }
    }
    merged
}

/// 从工作区搜索集以及配置的安装目录中发现技能。
/// 工作区/全局目录保持其正常优先级；自定义配置的目录在它不在该集合中时被追加。
#[must_use]
pub fn discover_for_workspace_and_dir(workspace: &Path, skills_dir: &Path) -> SkillRegistry {
    let mut dirs = skills_directories(workspace);
    if skills_dir.is_dir() && !dirs.iter().any(|p| p == skills_dir) {
        dirs.push(skills_dir.to_path_buf());
    }

    let mut merged = SkillRegistry::default();
    for dir in dirs {
        let registry = SkillRegistry::discover(&dir);
        for skill in registry.skills {
            if !merged.skills.iter().any(|s| s.name == skill.name) {
                merged.skills.push(skill);
            }
        }
        for warning in registry.warnings {
            merged.warnings.push(warning);
        }
    }
    merged
}

/// 从每个工作区候选目录加上全局默认目录（#432）渲染系统提示词技能块。
/// 包装 [`discover_in_workspace`] 供仅持有工作区路径的调用者（例如 `prompts.rs`）使用。
#[must_use]
pub fn render_available_skills_context_for_workspace(workspace: &Path) -> Option<String> {
    let registry = discover_in_workspace(workspace);
    render_skills_block(&registry)
}

/// Codex 的渐进式披露约定：模型先看到技能名称、描述和路径，
/// 然后仅在技能相关时才打开特定的 `SKILL.md`。
///
/// 单目录变体——在扫描工作区以查找跨工具技能文件夹时
/// 请使用 [`render_available_skills_context_for_workspace`]（#432）。
#[must_use]
pub fn render_available_skills_context(skills_dir: &Path) -> Option<String> {
    let registry = SkillRegistry::discover(skills_dir);
    render_skills_block(&registry)
}

fn render_skills_block(registry: &SkillRegistry) -> Option<String> {
    if registry.is_empty() {
        return None;
    }

    let mut skills = registry.list().to_vec();
    skills.sort_by(|a, b| a.name.cmp(&b.name));

    let mut out = String::new();
    out.push_str("## 技能\n");
    out.push_str(
        "技能是一组存储在 `SKILL.md` 文件中的本地指令。\
以下是本次会话中可用的技能列表。每个条目包含\
名称、描述和文件路径，以便你在使用特定技能时可以打开源文件查看完整\
指令。\n\n",
    );
    out.push_str("### 可用技能\n");

    let mut omitted = 0usize;
    for skill in skills {
        // 使用发现时捕获的真实磁盘路径——对于社区安装，目录名称可能与
        // 前置元数据中的 `name` 不同，此时 `<dir>/<name>/SKILL.md` 将不存在，
        // 模型将无法打开它。
        let description = truncate_for_prompt(&skill.description, MAX_SKILL_DESCRIPTION_CHARS);
        let line = if description.is_empty() {
            format!("- {}: (file: {})\n", skill.name, skill.path.display())
        } else {
            format!(
                "- {}: {} (file: {})\n",
                skill.name,
                description,
                skill.path.display()
            )
        };

        if out.chars().count() + line.chars().count() > MAX_AVAILABLE_SKILLS_CHARS {
            omitted += 1;
        } else {
            out.push_str(&line);
        }
    }

    if omitted > 0 {
        out.push_str(&format!(
            "- ... 还有 {omitted} 个技能因提示词预算限制被省略。\n"
        ));
    }

    if !registry.warnings().is_empty() {
        out.push_str("\n### 技能加载警告\n");
        for warning in registry.warnings().iter().take(8) {
            out.push_str("- ");
            out.push_str(&truncate_for_prompt(warning, MAX_SKILL_DESCRIPTION_CHARS));
            out.push('\n');
        }
    }

    out.push_str(
        "\n### 如何使用技能\n\
- 发现：以上列表是本次会话中可用的技能。技能内容存储在所列路径的磁盘上。\n\
- 触发规则：如果用户指定了技能名称（通过 `$SkillName`、`/skill <name>` 或纯文本），或者任务明确匹配上述某个技能描述，则在该轮次使用该技能。多个提及意味着全部使用。除非再次提及，否则不要跨轮次延续技能。\n\
- 缺失/受阻：如果指定的技能缺失或其 `SKILL.md` 无法读取，简要说明并继续使用最佳备选方案。\n\
- 渐进式披露：决定使用某个技能后，仅读取该技能的 `SKILL.md`。当它引用相对路径（如 `scripts/foo.py`）时，相对于技能目录解析它们。\n\
- 上下文卫生：仅加载任务所需的特定引用文件。避免批量加载不相关的技能资源。\n\
- 安全：除非用户明确要求或该技能已被信任可用于脚本执行，否则不要执行来自社区技能的脚本。\n",
    );

    Some(out)
}

fn truncate_for_prompt(value: &str, max_chars: usize) -> String {
    let single_line = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() <= max_chars {
        return single_line;
    }

    let mut truncated = single_line
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

// === CLI 辅助函数 ===

#[allow(dead_code)] // CLI 工具，供将来使用
pub fn list(skills_dir: &Path) -> Result<()> {
    if !skills_dir.exists() {
        println!("No skills directory found at {}", skills_dir.display());
        return Ok(());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(skills_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            entries.push(entry.file_name().to_string_lossy().to_string());
        }
    }

    if entries.is_empty() {
        println!("No skills found in {}", skills_dir.display());
        return Ok(());
    }

    entries.sort();
    for entry in entries {
        println!("{entry}");
    }
    Ok(())
}

#[allow(dead_code)] // CLI 工具，供将来使用
pub fn show(skills_dir: &Path, name: &str) -> Result<()> {
    let path = skills_dir.join(name).join("SKILL.md");
    let contents =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    println!("{contents}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    fn create_skill_dir(tmpdir: &TempDir, skill_name: &str, skill_content: &str) {
        let skill_dir = tmpdir.path().join("skills").join(skill_name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), skill_content).unwrap();
    }

    #[test]
    fn render_available_skills_context_lists_paths_and_usage() {
        // 验证渲染输出包含技能名称、磁盘路径和使用说明。
        let tmpdir = TempDir::new().unwrap();
        create_skill_dir(
            &tmpdir,
            "test-skill",
            "---\nname: test-skill\ndescription: A test skill\n---\nDo something special",
        );

        let rendered =
            crate::skills::render_available_skills_context(&tmpdir.path().join("skills"))
                .expect("skill context");

        let expected_path = tmpdir
            .path()
            .join("skills")
            .join("test-skill")
            .join("SKILL.md")
            .display()
            .to_string();

        assert!(rendered.contains("## 技能"));
        assert!(rendered.contains("- test-skill: A test skill"));
        assert!(
            rendered.contains(&expected_path),
            "expected path {expected_path:?} not in rendered output"
        );
        assert!(rendered.contains("### 如何使用技能"));
    }

    #[test]
    fn render_available_skills_context_uses_real_dir_name_not_frontmatter_name() {
        // 回归测试：当社区安装或手动放置的技能所在的目录名称与其前置元数据
        // `name` 不同时，渲染的提示词必须指向真实的磁盘文件路径，
        // 而不是 <skills_dir>/<frontmatter-name>/SKILL.md（该路径不存在）。
        let tmpdir = TempDir::new().unwrap();
        create_skill_dir(
            &tmpdir,
            "weird-dir-name",
            "---\nname: friendly-name\ndescription: drift case\n---\nbody",
        );

        let rendered =
            crate::skills::render_available_skills_context(&tmpdir.path().join("skills"))
                .expect("skill context");

        let real_path = tmpdir
            .path()
            .join("skills")
            .join("weird-dir-name")
            .join("SKILL.md")
            .display()
            .to_string();
        let stale_path = tmpdir
            .path()
            .join("skills")
            .join("friendly-name")
            .join("SKILL.md")
            .display()
            .to_string();

        assert!(
            rendered.contains(&real_path),
            "expected real on-disk path {real_path:?} in rendered output, got:\n{rendered}"
        );
        assert!(
            !rendered.contains(&stale_path),
            "rendered output must not invent a path under the frontmatter name:\n{rendered}"
        );
    }

    #[test]
    fn render_available_skills_context_returns_none_when_empty() {
        let tmpdir = TempDir::new().unwrap();
        let empty = tmpdir.path().join("skills");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(crate::skills::render_available_skills_context(&empty).is_none());

        let missing = tmpdir.path().join("does-not-exist");
        assert!(crate::skills::render_available_skills_context(&missing).is_none());
    }

    #[test]
    fn render_available_skills_context_truncates_long_descriptions() {
        let tmpdir = TempDir::new().unwrap();
        let long_desc = "x".repeat(2_000);
        let body = format!("---\nname: bigdesc\ndescription: {long_desc}\n---\nbody");
        create_skill_dir(&tmpdir, "bigdesc", &body);

        let rendered =
            crate::skills::render_available_skills_context(&tmpdir.path().join("skills"))
                .expect("skill context");

        let max = super::MAX_SKILL_DESCRIPTION_CHARS;
        assert!(rendered.contains('…'), "expected truncation marker");
        assert!(
            !rendered.contains(&"x".repeat(max + 1)),
            "untruncated long run should not appear"
        );
    }

    #[test]
    fn render_available_skills_context_collapses_internal_whitespace() {
        let tmpdir = TempDir::new().unwrap();
        create_skill_dir(
            &tmpdir,
            "spaced-skill",
            "---\nname: spaced-skill\ndescription: alpha  \t  beta   gamma\n---\nbody",
        );

        let rendered =
            crate::skills::render_available_skills_context(&tmpdir.path().join("skills"))
                .expect("skill context");

        let line = rendered
            .lines()
            .find(|l| l.starts_with("- spaced-skill:"))
            .expect("skill line");
        assert!(line.contains("alpha beta gamma"), "got: {line:?}");
    }

    #[test]
    fn render_available_skills_context_omits_overflowing_skills() {
        let tmpdir = TempDir::new().unwrap();
        let big_desc = "y".repeat(super::MAX_SKILL_DESCRIPTION_CHARS - 20);
        for i in 0..200 {
            let body = format!("---\nname: skill-{i:03}\ndescription: {big_desc}\n---\nbody");
            create_skill_dir(&tmpdir, &format!("skill-{i:03}"), &body);
        }

        let rendered =
            crate::skills::render_available_skills_context(&tmpdir.path().join("skills"))
                .expect("skill context");

        assert!(
            rendered.contains("因提示词预算限制被省略"),
            "expected overflow notice"
        );
        assert!(
            rendered.chars().count() < super::MAX_AVAILABLE_SKILLS_CHARS + 4_000,
            "rendered length should stay near the budget"
        );
    }

    fn write_skill(dir: &std::path::Path, name: &str, description: &str, body: &str) {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n{body}\n"),
        )
        .unwrap();
    }

    #[cfg(unix)]
    fn create_dir_symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    fn create_dir_symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_dir(target, link)
    }

    #[test]
    fn skills_directories_returns_existing_dirs_in_precedence_order() {
        let tmpdir = TempDir::new().unwrap();
        let workspace = tmpdir.path();

        // 创建五个工作区候选目录中的四个（跳过 `.opencode`）。
        std::fs::create_dir_all(workspace.join(".agents").join("skills")).unwrap();
        std::fs::create_dir_all(workspace.join("skills")).unwrap();
        std::fs::create_dir_all(workspace.join(".claude").join("skills")).unwrap();
        std::fs::create_dir_all(workspace.join(".cursor").join("skills")).unwrap();

        let dirs = super::skills_directories(workspace);
        // 我们不断言全局默认位置，因为它依赖于主机（测试机器上可能不存在）。
        let mut idx = 0;
        let agents = workspace.join(".agents").join("skills");
        let local = workspace.join("skills");
        let claude = workspace.join(".claude").join("skills");
        let cursor = workspace.join(".cursor").join("skills");

        assert_eq!(dirs.get(idx), Some(&agents), "agents must come first");
        idx += 1;
        assert_eq!(dirs.get(idx), Some(&local), "local must come second");
        idx += 1;
        // .opencode/skills was not created — it must NOT appear.
        assert!(
            !dirs
                .iter()
                .any(|p| p == &workspace.join(".opencode").join("skills")),
            "missing dir must be omitted, got: {dirs:?}"
        );
        assert_eq!(dirs.get(idx), Some(&claude), "claude must come after local");
        idx += 1;
        assert_eq!(
            dirs.get(idx),
            Some(&cursor),
            "cursor must come after claude"
        );
    }

    #[test]
    fn claude_global_skills_dir_returns_home_relative_path() {
        // #902 辅助函数的冒烟测试。我们不断言确切的路径，
        // 因为 dirs::home_dir() 依赖于主机；我们只固定后缀形状，
        // 以便将来的重构不会静默地重命名它。
        let path = super::claude_global_skills_dir().expect("home dir resolves on test host");
        assert!(path.ends_with(".claude/skills") || path.ends_with(r".claude\skills"));
    }

    #[test]
    fn existing_skill_dirs_orders_globals_agents_then_claude_then_deepseek() {
        // 固定三个全局技能根目录之间的优先级（#902）。
        // 工作区候选目录在上面单独测试；这里我们仅在 existing_skill_dirs
        // 级别测试全局排序，以便断言与主机无关。
        let tmpdir = TempDir::new().unwrap();
        let agents_global = tmpdir.path().join(".agents").join("skills");
        let claude_global = tmpdir.path().join(".claude").join("skills");
        let deepseek_global = tmpdir.path().join(".deepseek").join("skills");
        std::fs::create_dir_all(&agents_global).unwrap();
        std::fs::create_dir_all(&claude_global).unwrap();
        std::fs::create_dir_all(&deepseek_global).unwrap();

        let dirs = super::existing_skill_dirs(vec![
            agents_global.clone(),
            claude_global.clone(),
            deepseek_global.clone(),
        ]);

        assert_eq!(dirs, vec![agents_global, claude_global, deepseek_global]);
    }

    #[test]
    fn existing_skill_dirs_keeps_agents_global_before_deepseek_global() {
        let tmpdir = TempDir::new().unwrap();
        let agents_global = tmpdir.path().join(".agents").join("skills");
        let deepseek_global = tmpdir.path().join(".deepseek").join("skills");
        let missing = tmpdir.path().join("missing").join("skills");
        std::fs::create_dir_all(&agents_global).unwrap();
        std::fs::create_dir_all(&deepseek_global).unwrap();

        let dirs = super::existing_skill_dirs(vec![
            missing,
            agents_global.clone(),
            deepseek_global.clone(),
            agents_global.clone(),
        ]);

        assert_eq!(dirs, vec![agents_global, deepseek_global]);
    }

    #[test]
    fn discover_in_workspace_merges_with_first_wins_precedence() {
        let tmpdir = TempDir::new().unwrap();
        let workspace = tmpdir.path();

        // 两个位置中存在相同的技能名称 `shared`——优先级更高的目录版本应胜出。
        write_skill(
            &workspace.join(".agents").join("skills"),
            "shared",
            "agents wins",
            "from agents",
        );
        write_skill(
            &workspace.join(".claude").join("skills"),
            "shared",
            "claude loses",
            "from claude",
        );
        // claude 中的唯一技能——仍应被发现。
        write_skill(
            &workspace.join(".claude").join("skills"),
            "unique-claude",
            "only here",
            "claude-only",
        );

        let registry = super::discover_in_workspace(workspace);
        let names: Vec<&str> = registry.list().iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"shared"),
            "shared must be present: {names:?}"
        );
        assert!(names.contains(&"unique-claude"));

        let shared = registry.get("shared").expect("shared present");
        assert_eq!(
            shared.description, "agents wins",
            "first-wins precedence should keep .agents/skills version"
        );
        assert!(
            shared.path.starts_with(workspace.join(".agents")),
            "shared.path should be from .agents/skills, got {:?}",
            shared.path
        );
    }

    #[test]
    fn discover_in_workspace_pulls_skills_from_opencode_dir() {
        let tmpdir = TempDir::new().unwrap();
        let workspace = tmpdir.path();
        write_skill(
            &workspace.join(".opencode").join("skills"),
            "opencode-only",
            "for interop",
            "body",
        );

        let registry = super::discover_in_workspace(workspace);
        assert!(
            registry.get("opencode-only").is_some(),
            ".opencode/skills must be scanned (#432)"
        );
    }

    #[test]
    fn discover_in_workspace_pulls_skills_from_cursor_dir() {
        let tmpdir = TempDir::new().unwrap();
        let workspace = tmpdir.path();
        write_skill(
            &workspace.join(".cursor").join("skills"),
            "cursor-only",
            "for cursor interop",
            "body",
        );

        let registry = super::discover_in_workspace(workspace);
        assert!(
            registry.get("cursor-only").is_some(),
            ".cursor/skills must be scanned"
        );
    }

    #[test]
    fn discover_accepts_plain_markdown_heading_without_frontmatter() {
        let tmpdir = TempDir::new().unwrap();
        let skill_dir = tmpdir.path().join("plain-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "# Plain Skill\n\nUse this skill without YAML frontmatter.\n",
        )
        .unwrap();

        let registry = super::SkillRegistry::discover(tmpdir.path());
        let skill = registry.get("Plain Skill").expect("plain skill parsed");
        assert_eq!(skill.description, "");
        assert!(skill.body.contains("Use this skill"));
    }

    #[test]
    fn discover_warns_for_plain_markdown_without_heading() {
        let tmpdir = TempDir::new().unwrap();
        let skill_dir = tmpdir.path().join("plain-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "Use this skill without a heading or YAML frontmatter.\n",
        )
        .unwrap();

        let registry = super::SkillRegistry::discover(tmpdir.path());
        assert!(registry.is_empty());
        assert!(
            registry
                .warnings()
                .iter()
                .any(|warning| warning.contains("no `# Heading` found")),
            "expected missing-heading warning, got {:?}",
            registry.warnings()
        );
    }

    #[test]
    fn render_available_skills_context_for_workspace_picks_up_cross_tool_dirs() {
        let tmpdir = TempDir::new().unwrap();
        let workspace = tmpdir.path();
        write_skill(
            &workspace.join(".claude").join("skills"),
            "from-claude",
            "claude-style skill",
            "body",
        );
        let rendered =
            super::render_available_skills_context_for_workspace(workspace).expect("non-empty");
        assert!(rendered.contains("from-claude"));
    }

    /// 针对 GitHub issue 的回归测试，用户按供应商/类别子目录组织技能
    ///（例如克隆的技能仓库打包了多个技能）。旧的单层 `read_dir` 只能找到
    /// `<root>/<skill>/SKILL.md` 并静默忽略了 `<root>/<vendor>/<skill>/SKILL.md`。
    #[test]
    fn discover_finds_skills_nested_under_vendor_subdirectory() {
        let tmpdir = TempDir::new().unwrap();
        let root = tmpdir.path().join("skills");

        // 两层嵌套：`<root>/<vendor>/<skill>/SKILL.md`。这与
        // 错误报告中的 `clawhub-skills/clawhub/SKILL.md` 布局匹配。
        write_skill(
            &root.join("clawhub-skills"),
            "clawhub",
            "claw search",
            "body",
        );
        write_skill(
            &root.join("clawhub-skills"),
            "github",
            "github helpers",
            "body",
        );
        // 三层嵌套：`<root>/<org>/<repo>/<skill>/SKILL.md`。
        write_skill(
            &root.join("pasky").join("chrome-cdp-skill"),
            "chrome-cdp",
            "browser automation",
            "body",
        );
        // 混合深度：扁平技能与嵌套布局并存仍然有效
        //（这就是捆绑的 `skill-creator` 的样子）。
        write_skill(&root, "skill-creator", "make skills", "body");

        let registry = super::SkillRegistry::discover(&root);
        let names: Vec<&str> = registry.list().iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"clawhub"), "vendor/skill missed: {names:?}");
        assert!(names.contains(&"github"), "vendor/skill missed: {names:?}");
        assert!(
            names.contains(&"chrome-cdp"),
            "deeply-nested skill missed: {names:?}"
        );
        assert!(
            names.contains(&"skill-creator"),
            "flat top-level skill must still load: {names:?}"
        );
        assert!(
            registry.warnings().is_empty(),
            "well-formed nested layout should not warn: {:?}",
            registry.warnings()
        );
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn discover_follows_symlinked_skill_directories() {
        let tmpdir = TempDir::new().unwrap();
        let source_root = tmpdir.path().join("claude-skills");
        let skills_root = tmpdir.path().join(".deepseek").join("skills");
        write_skill(&source_root, "agent-browser", "browser automation", "body");
        std::fs::create_dir_all(&skills_root).unwrap();
        let link_path = skills_root.join("agent-browser");

        if let Err(err) = create_dir_symlink(&source_root.join("agent-browser"), &link_path) {
            eprintln!("skipping symlink discovery assertion: {err}");
            return;
        }

        let registry = super::SkillRegistry::discover(&skills_root);
        let skill = registry
            .get("agent-browser")
            .expect("symlinked skill directory should be discovered");
        assert_eq!(skill.description, "browser automation");
        assert_eq!(skill.path, link_path.join("SKILL.md"));
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn discover_dedupes_symlink_cycles_by_canonical_directory() {
        let tmpdir = TempDir::new().unwrap();
        let root = tmpdir.path().join("skills");
        write_skill(&root, "real-skill", "ok", "body");
        let loop_parent = root.join("vendor");
        std::fs::create_dir_all(&loop_parent).unwrap();

        if let Err(err) = create_dir_symlink(&root, &loop_parent.join("loop")) {
            eprintln!("skipping symlink cycle assertion: {err}");
            return;
        }

        let registry = super::SkillRegistry::discover(&root);
        let matches = registry
            .list()
            .iter()
            .filter(|skill| skill.name == "real-skill")
            .count();
        assert_eq!(
            matches, 1,
            "symlink cycle should not rediscover the same canonical skill directory"
        );
    }

    /// 一旦目录被识别为技能（包含 `SKILL.md`），遍历器不得进入其中：
    /// 任何嵌套的 `SKILL.md` 都将是父技能附带的固定配置/示例，
    /// 而非可单独安装的技能。这与 `tools::skill::collect_companion_files`
    /// 已记录的约定一致（"嵌套目录——已跳过"）。
    #[test]
    fn discover_does_not_descend_into_a_skill_directory() {
        let tmpdir = TempDir::new().unwrap();
        let root = tmpdir.path().join("skills");

        // 父技能：<root>/parent/SKILL.md。
        write_skill(&root, "parent", "outer skill", "outer body");
        // 捆绑在父技能目录内的固定配置：
        // <root>/parent/examples/inner-fixture/SKILL.md。遍历器在找到
        // 其 SKILL.md 后不得进入 <root>/parent/，因此 `inner-fixture` 不得被加载。
        write_skill(
            &root.join("parent").join("examples"),
            "inner-fixture",
            "should not load",
            "fixture body",
        );

        let registry = super::SkillRegistry::discover(&root);
        let names: Vec<&str> = registry.list().iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"parent"));
        assert!(
            !names.contains(&"inner-fixture"),
            "nested SKILL.md inside an existing skill must be ignored: {names:?}"
        );
    }

    /// 根目录下的隐藏子目录（例如 `.git`、`.cache`）必须被跳过，
    /// 以便位于检出仓库内的 `skills_dir` 不会意外地从 VCS 元数据中
    /// 加载随机名为 `SKILL.md` 的固定配置。根目录本身不受限——用户显式将
    /// `skills_dir` 指向了它。
    #[test]
    fn discover_skips_hidden_subdirectories_below_root() {
        let tmpdir = TempDir::new().unwrap();
        let root = tmpdir.path().join("skills");

        write_skill(&root, "real-skill", "ok", "body");
        // 一个 `<root>/.git/<junk>/SKILL.md` 的模拟，不得加载。
        // `.git` 是用户提供根目录的直接子级（遍历的深度 0），
        // 这正好是旧的 `depth > 0` 守卫遗漏的情况。
        write_skill(&root.join(".git"), "vcs-noise", "should not load", "body");

        let registry = super::SkillRegistry::discover(&root);
        let names: Vec<&str> = registry.list().iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"real-skill"));
        assert!(
            !names.contains(&"vcs-noise"),
            "skills under hidden subdirs must be skipped: {names:?}"
        );
    }

    /// 用户显式选择根目录，因此即使是像 `~/.agents/skills`（错误报告中的布局）
    /// 这样的隐藏路径也必须正常工作。
    #[test]
    fn discover_honors_a_hidden_root_directory() {
        let tmpdir = TempDir::new().unwrap();
        let root = tmpdir.path().join(".agents").join("skills");

        // 匹配错误报告：skills_dir = "~/.agents/skills"
        // 技能嵌套在 <root>/custom-skills/git-conventions/SKILL.md。
        write_skill(
            &root.join("custom-skills"),
            "git-conventions",
            "conventions",
            "body",
        );

        let registry = super::SkillRegistry::discover(&root);
        let names: Vec<&str> = registry.list().iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"git-conventions"),
            "hidden root must still be walked: {names:?}"
        );
    }
}
