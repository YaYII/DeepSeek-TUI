//! 社区技能安装器（#140）。
//!
//! 从 GitHub 或直接 tarball URL 拉取用户创作的技能，通过路径遍历和大小限制
//! 的提取器进行验证，并写入 `<skills_dir>/<name>/`。无需后端服务，无需自动执行：
//! 每次安装都由每个域名的 [`crate::network_policy::NetworkPolicy`] 控制，
//! 验证会拒绝任何逃逸目标目录的 tarball 条目。
//!
//! 公开接口：
//!
//! * [`InstallSource`] — `github:owner/repo`、原始 URL 或精选注册表名称。
//!   通过 [`InstallSource::parse`] 从单个字符串解析。
//! * [`install`] / [`update`] / [`uninstall`] — 异步安装、原子更新和干净卸载。
//!   三者都保留 `.installed-from` 标记，以便捆绑的 `skill-creator`（没有该标记）
//!   永远不会被触及。
//! * [`InstallOutcome`] — `Installed` / `NeedsApproval(host)` /
//!   `NetworkDenied(host)`。`NeedsApproval` 变体在无副作用的情况下返回，
//!   以便调用者（斜杠命令、运行时 API 等）可以通过自己的审批流程路由。
//!
//! # 硬性规则
//!
//! * 验证首先提取到临时目录。只有在 tarball 通过所有检查后，
//!   目标路径才会被创建（通过原子重命名）。半安装的技能永远不会出现在磁盘上。
//! * 路径遍历拒绝同时涵盖 `..` 段和绝对路径。所选技能子树内的符号链接
//!   被拒绝——在 SKILL.md 捆绑中没有它们的用例，而且它们是臭名昭著的逃逸据点。
//!   多技能仓库存档可能包含所选子树外的不相关符号链接；这些条目被忽略且从不提取。
//! * 提取的文件不会被授予 `+x` 权限。可选的 `/skill trust <name>` 命令
//!   写入 `.trusted` 标记；工具执行门控是一个单独的关注点，位于工具注册表旁边。

use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::network_policy::{Decision, NetworkPolicy, host_from_url};

/// 注册表同步技能的缓存目录。
///
/// 位于 `~/.deepseek/cache/skills/`，与用户安装的技能分开存放，
/// 可以随时清除而不会丢失任何不可替代的内容。
pub fn default_cache_skills_dir() -> PathBuf {
    dirs::home_dir().map_or_else(
        || PathBuf::from("/tmp/deepseek/cache/skills"),
        |p| p.join(".deepseek").join("cache").join("skills"),
    )
}

/// 默认注册表。回退到托管在 GitHub raw 上的社区维护的 `index.json`；
/// 用户可以通过 config.toml 中的 `[skills] registry_url` 覆盖。
pub const DEFAULT_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/Hmbown/deepseek-skills/main/index.json";

/// 默认每个技能的大小上限（5 MiB）。在解包时执行，以防止恶意 gzip 炸弹耗尽内存。
pub const DEFAULT_MAX_SIZE_BYTES: u64 = 5 * 1024 * 1024;

/// 在每个已安装技能下写入的文件，使 [`update`] / [`uninstall`] 可以
/// 恢复原始 [`InstallSource`]，而无需重新解析用户输入。
pub const INSTALLED_FROM_MARKER: &str = ".installed-from";

/// 在每个受信任技能下写入的文件。目前是咨询性质的（安装路径从不自动运行任何东西）——
/// 运行时工具调用门在执行技能附带的脚本之前会查阅此标记。
pub const TRUSTED_MARKER: &str = ".trusted";

// ─────────────────────────────────────────────────────────────────────────────
// 来源解析
// ─────────────────────────────────────────────────────────────────────────────

/// 技能安装的来源。接受的规范语法请参见 [`InstallSource::parse`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallSource {
    /// `github:owner/repo`。解析为
    /// `https://github.com/<owner>/<repo>/archive/refs/heads/main.tar.gz`，
    /// 并在 404 时回退到 `master.tar.gz`。
    GitHubRepo(String),
    /// 原始 `http(s)://…` tarball URL。按原样使用。
    DirectUrl(String),
    /// 精选注册表查找键。通过配置的 `registry_url` 查找。
    Registry(String),
}

impl InstallSource {
    /// 解析用户提供的规范。空或仅空白字符的输入被拒绝。
    ///
    /// * `github:owner/repo` → [`InstallSource::GitHubRepo`]
    /// * `https://github.com/owner/repo[.git]`（仓库路径后无其他路径）→
    ///   [`InstallSource::GitHubRepo`]
    /// * 任何其他 `http://` 或 `https://` 前缀 → [`InstallSource::DirectUrl`]
    /// * 其他任何内容 → [`InstallSource::Registry`]
    pub fn parse(spec: &str) -> Result<Self> {
        let trimmed = spec.trim();
        if trimmed.is_empty() {
            bail!("install source must not be empty");
        }
        if let Some(rest) = trimmed.strip_prefix("github:") {
            let rest = rest.trim();
            // Reject obviously bogus values up front. We intentionally accept
            // case-insensitive owner/repo so `github:Hmbown/Foo` works.
            let (owner, repo) = rest.split_once('/').with_context(|| {
                format!("github source must be 'github:owner/repo' (got {spec})")
            })?;
            let owner = owner.trim();
            let repo = repo.trim().trim_end_matches('/');
            if owner.is_empty() || repo.is_empty() {
                bail!("github source must be 'github:owner/repo' (got {spec})");
            }
            if owner.contains('/') || repo.contains('/') {
                bail!("github source must be 'github:owner/repo' (got {spec})");
            }
            return Ok(Self::GitHubRepo(format!("{owner}/{repo}")));
        }
        if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
            if let Some(repo) = parse_github_browser_url(trimmed) {
                return Ok(Self::GitHubRepo(repo));
            }
            return Ok(Self::DirectUrl(trimmed.to_string()));
        }
        Ok(Self::Registry(trimmed.to_string()))
    }
}

/// 检测裸 `https://github.com/<owner>/<repo>` URL（带或不带尾部 `.git`）
/// 并返回 `owner/repo`。对于已指向特定存档/blob/树路径的任何 URL 返回 `None`——
/// 这些是真正的直接 URL，调用者按原样获取它们。
fn parse_github_browser_url(url: &str) -> Option<String> {
    let after_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, rest) = after_scheme.split_once('/')?;
    if !host.eq_ignore_ascii_case("github.com") && !host.eq_ignore_ascii_case("www.github.com") {
        return None;
    }
    let trimmed = rest.trim_end_matches('/');
    let mut parts = trimmed.splitn(3, '/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim().trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    // 如果有第三个段，则 URL 指向子资源（`/archive/...`、`/blob/...`、`/tree/...`）。
    // 将其视为真正的直接 URL——用户显式想要该路径下的内容。
    if parts.next().is_some() {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

// ─────────────────────────────────────────────────────────────────────────────
// 结果类型
// ─────────────────────────────────────────────────────────────────────────────

/// 安装尝试的结果。
#[derive(Debug)]
pub enum InstallOutcome {
    /// 技能已安装（或已存在且幂等）。
    Installed(InstalledSkill),
    /// 主机需要用户批准后才能继续安装。调用者应通过其拥有的任何批准途径
    /// 展示此信息，并在批准后重试（通常通过将主机添加到策略的允许列表）。
    NeedsApproval(String),
    /// 主机被网络策略拒绝。安装已中止。
    NetworkDenied(String),
}

/// 成功安装的技能元数据。
#[derive(Debug, Clone)]
pub struct InstalledSkill {
    /// 技能名称（从 SKILL.md 前置元数据中获取）。
    pub name: String,
    /// 最终磁盘路径：`<skills_dir>/<name>/`。
    pub path: PathBuf,
    /// 下载的 tarball 字节的 SHA-256 哈希。由 [`update`] 用于检测上游变更而无需重新提取；
    /// 也用于遥测/未来签名验证工作。
    #[allow(dead_code)]
    pub source_checksum: String,
}

/// [`update`] 调用的结果。
#[derive(Debug)]
pub enum UpdateResult {
    /// 上游 tarball 与磁盘校验和字节相同；无需操作。
    NoChange,
    /// 上游已变更，磁盘上的安装已原子替换。
    Updated(InstalledSkill),
    /// 网络策略短路了更新。与 [`InstallOutcome::NeedsApproval`] 语义相同。
    NeedsApproval(String),
    /// 网络策略拒绝了更新。
    NetworkDenied(String),
}

/// 安装过程中可能发生的错误。大多数变体在公共边界被展平为 `anyhow::Error`；
/// 此枚举在内部使用，以便测试无需解析字符串即可进行模式匹配。
#[derive(Debug, Error)]
pub enum InstallError {
    #[error("条目逃逸出目标目录：{0}")]
    PathTraversal(String),
    #[error("条目过大；解压后总量将超过 {limit} 字节")]
    OversizedTarball { limit: u64 },
    #[error("归档中缺少 SKILL.md")]
    MissingSkillMd,
    #[error("SKILL.md 前置元数据缺少必填字段：{0}")]
    MissingFrontmatterField(&'static str),
    #[error("技能 tarball 中不允许使用符号链接")]
    SymlinkRejected,
    #[error("技能 '{0}' 已安装；请先更新或移除它")]
    AlreadyInstalled(String),
    #[error("技能 '{0}' 未通过 /skill install 安装（没有 .installed-from 标记）")]
    NotInstalledHere(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// 公开 API
// ─────────────────────────────────────────────────────────────────────────────

/// 将社区技能安装到 `skills_dir`。
///
/// 步骤：
///
/// 1. 将 `source` 解析为一个或多个候选 URL（GitHub 在 `main` 之后添加 `master` 回退）。
/// 2. 咨询 `network` 关于主机的信息。`Allow` 继续；`Deny` 返回
///    [`InstallOutcome::NetworkDenied`]；`Prompt` 返回
///    [`InstallOutcome::NeedsApproval`] 而不触碰磁盘。
/// 3. 将 tarball 流式传输到临时文件（上限为 `max_size`）。
/// 4. 验证存档（路径遍历、大小、所选技能子树中无符号链接、
///    SKILL.md 存在并包含所需的前置元数据字段）到同级 `<name>.tmp/` 目录。
/// 5. 原子重命名 `<name>.tmp/` → `<name>/`。
/// 6. 写入 `.installed-from` 并返回 [`InstalledSkill`]。
///
/// `update = false` 拒绝已存在的目标。从 [`update`] 传递 `update = true`
/// 以允许替换。
///
/// 使用捆绑的 [`DEFAULT_REGISTRY_URL`] 的 [`install_with_registry`] 的便利包装。
/// 对下游消费者（测试、运行时 API）公开，尽管斜杠命令路径始终经过
/// [`install_with_registry`]，以便用户的配置注册表胜出。
#[allow(dead_code)]
pub async fn install(
    source: InstallSource,
    skills_dir: &Path,
    max_size: u64,
    network: &NetworkPolicy,
    update: bool,
) -> Result<InstallOutcome> {
    install_with_registry(
        source,
        skills_dir,
        max_size,
        network,
        update,
        DEFAULT_REGISTRY_URL,
    )
    .await
}

/// 与 [`install`] 相同，但允许调用者覆盖注册表 URL。对测试有用；
/// 斜杠命令路径始终使用配置的注册表。
pub async fn install_with_registry(
    source: InstallSource,
    skills_dir: &Path,
    max_size: u64,
    network: &NetworkPolicy,
    update: bool,
    registry_url: &str,
) -> Result<InstallOutcome> {
    let urls = candidate_urls(&source, network, registry_url).await?;
    let urls = match urls {
        UrlResolution::Resolved(urls) => urls,
        UrlResolution::NeedsApproval(host) => return Ok(InstallOutcome::NeedsApproval(host)),
        UrlResolution::Denied(host) => return Ok(InstallOutcome::NetworkDenied(host)),
    };

    // Try each URL in order — GitHub returns 404 for `main` on master-only
    // repos, and we don't want to fail the install on that.
    let (bytes, source_url) = match download_first_success(&urls, network, max_size).await? {
        DownloadOutcome::Bytes { bytes, url } => (bytes, url),
        DownloadOutcome::NeedsApproval(host) => return Ok(InstallOutcome::NeedsApproval(host)),
        DownloadOutcome::Denied(host) => return Ok(InstallOutcome::NetworkDenied(host)),
    };

    // Compute a checksum before unpacking so [`update`] can detect upstream
    // no-op changes without redoing the extract.
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let checksum = format!("{:x}", hasher.finalize());

    let staged = stage_tarball(&bytes, skills_dir, max_size)?;

    // Move the staged dir into its final location. If `update` is set and the
    // destination exists, replace it; otherwise reject.
    let final_path = skills_dir.join(&staged.skill_name);
    if final_path.exists() {
        if !update {
            // Clean up the staging dir before returning the error.
            let _ = fs::remove_dir_all(&staged.staged_path);
            return Err(InstallError::AlreadyInstalled(staged.skill_name).into());
        }
        // Best-effort backup-then-replace; on failure we restore the original.
        let backup = skills_dir.join(format!("{}.bak", staged.skill_name));
        // If a previous failed update left a stale `.bak/`, drop it.
        if backup.exists() {
            fs::remove_dir_all(&backup).ok();
        }
        fs::rename(&final_path, &backup).with_context(|| {
            format!(
                "failed to backup existing skill at {}",
                final_path.display()
            )
        })?;
        if let Err(err) = fs::rename(&staged.staged_path, &final_path) {
            // Roll back: restore the backup so the user isn't left with an
            // empty skill directory.
            fs::rename(&backup, &final_path).ok();
            return Err(err).context("failed to install staged skill");
        }
        fs::remove_dir_all(&backup).ok();
    } else {
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create skills directory {}", parent.display())
            })?;
        }
        fs::rename(&staged.staged_path, &final_path).context("failed to install staged skill")?;
    }

    // Write the marker last so a partial install never leaves a stale
    // .installed-from on disk.
    let marker_body = serde_json::json!({
        "spec": source_spec_string(&source),
        "url": source_url,
        "checksum": checksum,
    })
    .to_string();
    fs::write(final_path.join(INSTALLED_FROM_MARKER), marker_body).with_context(|| {
        format!(
            "failed to write {} marker for skill {}",
            INSTALLED_FROM_MARKER, staged.skill_name
        )
    })?;

    Ok(InstallOutcome::Installed(InstalledSkill {
        name: staged.skill_name,
        path: final_path,
        source_checksum: checksum,
    }))
}

/// 重新获取先前安装的技能，如果上游 tarball 已变更则替换磁盘上的内容。
///
/// 读取 `.installed-from` 以恢复原始 [`InstallSource`]，因此通过
/// `/skill install github:foo/bar` 安装的技能可以通过 `/skill update bar`
/// 更新，用户无需重新输入规范。
///
/// 基于 [`update_with_registry`] 的便利包装。
#[allow(dead_code)]
pub async fn update(
    name: &str,
    skills_dir: &Path,
    max_size: u64,
    network: &NetworkPolicy,
) -> Result<UpdateResult> {
    update_with_registry(name, skills_dir, max_size, network, DEFAULT_REGISTRY_URL).await
}

/// 与 [`update`] 相同，但允许调用者覆盖注册表 URL。
pub async fn update_with_registry(
    name: &str,
    skills_dir: &Path,
    max_size: u64,
    network: &NetworkPolicy,
    registry_url: &str,
) -> Result<UpdateResult> {
    let target = skills_dir.join(name);
    let marker_path = target.join(INSTALLED_FROM_MARKER);
    if !marker_path.exists() {
        return Err(InstallError::NotInstalledHere(name.to_string()).into());
    }
    let marker_body = fs::read_to_string(&marker_path)
        .with_context(|| format!("failed to read {}", marker_path.display()))?;
    let marker: InstalledFromMarker = serde_json::from_str(&marker_body)
        .with_context(|| format!("malformed {} for {name}", INSTALLED_FROM_MARKER))?;

    // Re-resolve the URL, taking the existing checksum as a short-circuit hint:
    // we still hit the network so the user gets a useful "no upstream change"
    // signal, but we skip the unpack step if the bytes match.
    let source = InstallSource::parse(&marker.spec)?;
    let urls = match candidate_urls(&source, network, registry_url).await? {
        UrlResolution::Resolved(urls) => urls,
        UrlResolution::NeedsApproval(host) => return Ok(UpdateResult::NeedsApproval(host)),
        UrlResolution::Denied(host) => return Ok(UpdateResult::NetworkDenied(host)),
    };
    let (bytes, _url) = match download_first_success(&urls, network, max_size).await? {
        DownloadOutcome::Bytes { bytes, url } => (bytes, url),
        DownloadOutcome::NeedsApproval(host) => return Ok(UpdateResult::NeedsApproval(host)),
        DownloadOutcome::Denied(host) => return Ok(UpdateResult::NetworkDenied(host)),
    };

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let checksum = format!("{:x}", hasher.finalize());
    if checksum == marker.checksum {
        return Ok(UpdateResult::NoChange);
    }

    // Bytes changed — fall back to the regular install path with `update = true`
    // so we get the same atomic-replace semantics.
    let outcome =
        install_with_registry(source, skills_dir, max_size, network, true, registry_url).await?;
    match outcome {
        InstallOutcome::Installed(installed) => Ok(UpdateResult::Updated(installed)),
        InstallOutcome::NeedsApproval(host) => Ok(UpdateResult::NeedsApproval(host)),
        InstallOutcome::NetworkDenied(host) => Ok(UpdateResult::NetworkDenied(host)),
    }
}

/// 移除社区安装的技能。
///
/// 拒绝触碰任何没有 `.installed-from` 标记的目录——这是我们判断它是用户拥有
/// 而非系统技能的依据。
pub fn uninstall(name: &str, skills_dir: &Path) -> Result<()> {
    let target = skills_dir.join(name);
    if !target.exists() {
        bail!("skill '{name}' is not installed at {}", target.display());
    }
    if !target.join(INSTALLED_FROM_MARKER).exists() {
        return Err(InstallError::NotInstalledHere(name.to_string()).into());
    }
    fs::remove_dir_all(&target)
        .with_context(|| format!("failed to remove {}", target.display()))?;
    Ok(())
}

/// 将社区安装的技能标记为受信任。目前仅是一个标记文件；
/// 针对 `<name>/scripts/` 进行工具执行的调用者在调用任何内容之前会查阅该文件。
/// 如果已受信任则为空操作。
///
/// 拒绝标记系统技能（没有 `.installed-from`），以便捆绑的 `skill-creator`
/// 不会意外继承提升的工具权限。
pub fn trust(name: &str, skills_dir: &Path) -> Result<()> {
    let target = skills_dir.join(name);
    if !target.exists() {
        bail!("skill '{name}' is not installed at {}", target.display());
    }
    if !target.join(INSTALLED_FROM_MARKER).exists() {
        return Err(InstallError::NotInstalledHere(name.to_string()).into());
    }
    let marker = target.join(TRUSTED_MARKER);
    if !marker.exists() {
        fs::write(
            &marker,
            "Skill scripts/ are user-trusted. Delete this file to revoke.\n",
        )
        .with_context(|| format!("failed to write {}", marker.display()))?;
    }
    Ok(())
}

/// 获取精选注册表并返回解析后的条目。
///
/// 遵循 `network`（在 Deny / Prompt 时完全跳过调用）。
pub async fn fetch_registry(
    network: &NetworkPolicy,
    registry_url: &str,
) -> Result<RegistryFetchResult> {
    let host = match host_from_url(registry_url) {
        Some(host) => host,
        None => bail!("invalid registry url: {registry_url}"),
    };
    match network.decide(&host) {
        Decision::Allow => {}
        Decision::Deny => return Ok(RegistryFetchResult::Denied(host)),
        Decision::Prompt => return Ok(RegistryFetchResult::NeedsApproval(host)),
    }
    let body = reqwest::get(registry_url)
        .await
        .with_context(|| format!("failed to fetch registry {registry_url}"))?
        .error_for_status()
        .with_context(|| format!("registry {registry_url} returned an error status"))?
        .text()
        .await
        .with_context(|| format!("failed to read registry body from {registry_url}"))?;
    let parsed: RegistryDocument = serde_json::from_str(&body)
        .with_context(|| format!("failed to parse registry json from {registry_url}"))?;
    Ok(RegistryFetchResult::Loaded(parsed))
}

// ─────────────────────────────────────────────────────────────────────────────
// 注册表同步（issue #433）
// ─────────────────────────────────────────────────────────────────────────────

/// [`sync_registry`] 期间单个技能条目的结果。
#[derive(Debug, Clone)]
pub enum SkillSyncOutcome {
    /// 技能已下载并写入缓存目录。
    Downloaded { name: String, path: PathBuf },
    /// 缓存的字节与上游 ETag / SHA-256 匹配；未写入任何内容。
    Fresh { name: String },
    /// 技能下载失败；错误非致命，因此同步继续。
    Failed { name: String, reason: String },
    /// 网络策略阻止了下载主机。
    Denied { name: String, host: String },
    /// 网络策略要求用户批准下载主机。
    NeedsApproval { name: String, host: String },
}

/// [`sync_registry`] 的总体结果。
#[derive(Debug)]
pub enum SyncResult {
    /// 同步完成。`outcomes` 包含索引中每个技能的一个条目。
    Done { outcomes: Vec<SkillSyncOutcome> },
    /// 注册表获取被网络策略阻止。
    RegistryDenied(String),
    /// 注册表获取需要用户批准。
    RegistryNeedsApproval(String),
}

/// 与每个缓存技能一起写入的新鲜度元数据，以便后续同步可以跳过未变更的内容。
#[derive(Debug, Serialize, Deserialize)]
struct CacheMeta {
    /// 服务器为主资源返回的 ETag（如果有）。
    #[serde(default)]
    etag: Option<String>,
    /// 下载字节的 SHA-256 十六进制摘要。
    sha256: String,
    /// 获取资源的源 URL。
    url: String,
}

/// 将远程注册表同步到本地缓存。
///
/// 对于 `index.json` 中列出的每个技能，此函数：
///
/// 1. 解析下载 URL（与 `install` 相同的逻辑）。
/// 2. 检查缓存的 [`CacheMeta`]（etag + sha256）以确认新鲜度；如果未变更则跳过下载。
/// 3. 将 SKILL.md（如果源是 tarball 则包括任何配套文件）下载到 `<cache_dir>/<name>/`。
/// 4. 写入更新的 [`CacheMeta`] 以便下次同步更快。
///
/// 每个技能的失败是非致命的：记录 [`SkillSyncOutcome::Failed`] 并继续同步。
/// 调用者决定如何呈现每个技能的错误。
pub async fn sync_registry(
    network: &NetworkPolicy,
    registry_url: &str,
    cache_dir: &Path,
    max_size: u64,
) -> Result<SyncResult> {
    let doc = match fetch_registry(network, registry_url).await? {
        RegistryFetchResult::Loaded(doc) => doc,
        RegistryFetchResult::Denied(host) => return Ok(SyncResult::RegistryDenied(host)),
        RegistryFetchResult::NeedsApproval(host) => {
            return Ok(SyncResult::RegistryNeedsApproval(host));
        }
    };

    let mut outcomes = Vec::new();

    for (name, entry) in &doc.skills {
        let outcome = sync_one_skill(name, entry, network, cache_dir, max_size).await;
        outcomes.push(outcome);
    }

    Ok(SyncResult::Done { outcomes })
}

/// 将注册表中的单个技能条目同步到缓存目录。
async fn sync_one_skill(
    name: &str,
    entry: &RegistryEntry,
    network: &NetworkPolicy,
    cache_dir: &Path,
    max_size: u64,
) -> SkillSyncOutcome {
    // Resolve the source to a concrete URL list.
    let source = match InstallSource::parse(&entry.source) {
        Ok(s) => s,
        Err(err) => {
            return SkillSyncOutcome::Failed {
                name: name.to_string(),
                reason: format!("invalid source spec '{}': {err:#}", entry.source),
            };
        }
    };

    // Registry sources in index.json must not point back at another registry.
    if matches!(source, InstallSource::Registry(_)) {
        return SkillSyncOutcome::Failed {
            name: name.to_string(),
            reason: format!("registry entry for '{name}' must not point to another registry entry"),
        };
    }

    let urls = match &source {
        InstallSource::GitHubRepo(repo) => vec![
            format!("https://github.com/{repo}/archive/refs/heads/main.tar.gz"),
            format!("https://github.com/{repo}/archive/refs/heads/master.tar.gz"),
        ],
        InstallSource::DirectUrl(url) => vec![url.clone()],
        InstallSource::Registry(_) => unreachable!("guarded above"),
    };

    // Check the first downloadable URL against any cached meta.
    let skill_cache_dir = cache_dir.join(name);
    let meta_path = skill_cache_dir.join(".cache-meta.json");

    // Try each candidate URL in order.
    for url in &urls {
        let host = match host_from_url(url) {
            Some(h) => h,
            None => continue,
        };
        match network.decide(&host) {
            Decision::Allow => {}
            Decision::Deny => {
                return SkillSyncOutcome::Denied {
                    name: name.to_string(),
                    host,
                };
            }
            Decision::Prompt => {
                return SkillSyncOutcome::NeedsApproval {
                    name: name.to_string(),
                    host,
                };
            }
        }

        // Perform a HEAD request (or conditional GET) for freshness. We use a
        // simple GET with If-None-Match when we have an ETag, falling back to
        // an unconditional GET for servers that don't support ETags.
        let existing_meta: Option<CacheMeta> = meta_path
            .exists()
            .then(|| {
                fs::read_to_string(&meta_path)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
            })
            .flatten();

        // Build the request — add If-None-Match if we have a cached ETag.
        let client = reqwest::Client::new();
        let mut req = client.get(url);
        if let Some(ref meta) = existing_meta
            && let Some(ref etag) = meta.etag
        {
            req = req.header("If-None-Match", etag);
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(err) => {
                // Network error — try the next candidate URL.
                let _ = err;
                continue;
            }
        };

        let status = resp.status();

        // 304 Not Modified: cached copy is still fresh.
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return SkillSyncOutcome::Fresh {
                name: name.to_string(),
            };
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            // Try next URL (main → master fallback).
            continue;
        }

        if !status.is_success() {
            return SkillSyncOutcome::Failed {
                name: name.to_string(),
                reason: format!("GET {url} returned HTTP {status}"),
            };
        }

        // Capture ETag before consuming the response body.
        let etag = resp
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let compressed_cap = max_size.saturating_mul(4);
        let bytes = match resp.bytes().await {
            Ok(b) => b,
            Err(err) => {
                return SkillSyncOutcome::Failed {
                    name: name.to_string(),
                    reason: format!("failed to read body from {url}: {err:#}"),
                };
            }
        };
        if bytes.len() as u64 > compressed_cap {
            return SkillSyncOutcome::Failed {
                name: name.to_string(),
                reason: format!(
                    "download from {url} exceeds compressed size cap ({} bytes)",
                    compressed_cap
                ),
            };
        }

        // Compute SHA-256 of the downloaded bytes.
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let sha256 = format!("{:x}", hasher.finalize());

        // Short-circuit: if the hash matches the cached one, we're fresh even
        // without a 304 (some CDNs strip ETags on redirects).
        if let Some(ref meta) = existing_meta
            && meta.sha256 == sha256
            && meta.url == *url
        {
            return SkillSyncOutcome::Fresh {
                name: name.to_string(),
            };
        }

        // Determine whether this is a tarball or a plain SKILL.md.
        // Heuristic: the URL ends with `.tar.gz` or `.tgz`, or the content
        // starts with the gzip magic bytes (0x1f 0x8b).
        let is_tarball =
            url.ends_with(".tar.gz") || url.ends_with(".tgz") || bytes.starts_with(&[0x1f, 0x8b]);

        let final_path: PathBuf = if is_tarball {
            // Extract into a temp staging dir, then rename atomically.
            let staged = match stage_tarball(&bytes, cache_dir, max_size) {
                Ok(s) => s,
                Err(err) => {
                    return SkillSyncOutcome::Failed {
                        name: name.to_string(),
                        reason: format!("tarball extraction failed: {err:#}"),
                    };
                }
            };
            // Move staged dir into its final location, replacing any prior cache.
            let dest = cache_dir.join(name);
            if dest.exists() {
                let _ = fs::remove_dir_all(&dest);
            }
            if let Err(err) = fs::rename(&staged.staged_path, &dest) {
                let _ = fs::remove_dir_all(&staged.staged_path);
                return SkillSyncOutcome::Failed {
                    name: name.to_string(),
                    reason: format!("failed to move staged skill into cache: {err:#}"),
                };
            }
            dest
        } else {
            // Plain SKILL.md (or other companion text file). Write directly.
            if let Err(err) = fs::create_dir_all(&skill_cache_dir) {
                return SkillSyncOutcome::Failed {
                    name: name.to_string(),
                    reason: format!("failed to create cache dir: {err:#}"),
                };
            }
            let skill_md_path = skill_cache_dir.join("SKILL.md");
            if let Err(err) = fs::write(&skill_md_path, &bytes) {
                return SkillSyncOutcome::Failed {
                    name: name.to_string(),
                    reason: format!("failed to write SKILL.md to cache: {err:#}"),
                };
            }
            skill_cache_dir.clone()
        };

        // Write the updated freshness metadata.
        let meta = CacheMeta {
            etag,
            sha256,
            url: url.clone(),
        };
        let meta_json = serde_json::to_string(&meta).unwrap_or_default();
        let _ = fs::write(final_path.join(".cache-meta.json"), meta_json);

        return SkillSyncOutcome::Downloaded {
            name: name.to_string(),
            path: final_path,
        };
    }

    // All candidate URLs exhausted without a successful response.
    SkillSyncOutcome::Failed {
        name: name.to_string(),
        reason: format!(
            "all candidate URLs for '{}' failed or were not found",
            entry.source
        ),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 内部辅助函数
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct InstalledFromMarker {
    spec: String,
    #[serde(default)]
    checksum: String,
}

/// 精选注册表文档。其形状故意保持最小，以便日后添加可选元数据
///（主页、版本、签名）时保持向前兼容。
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryDocument {
    /// Map of skill name → entry.
    #[serde(default)]
    pub skills: std::collections::BTreeMap<String, RegistryEntry>,
}

/// 精选注册表中的一行。`description` 是可选的，以便旧索引仍能解析。
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryEntry {
    /// Source spec (e.g. `github:owner/repo`).
    pub source: String,
    /// Optional human-readable description.
    #[serde(default)]
    pub description: Option<String>,
}

/// 成功的注册表获取结果。与 [`InstallOutcome`] 的网络策略结果形状相同，
/// 以便调用者可以直接进入审批流程。
#[derive(Debug)]
pub enum RegistryFetchResult {
    Loaded(RegistryDocument),
    NeedsApproval(String),
    Denied(String),
}

enum UrlResolution {
    Resolved(Vec<String>),
    NeedsApproval(String),
    Denied(String),
}

enum DownloadOutcome {
    Bytes { bytes: Vec<u8>, url: String },
    NeedsApproval(String),
    Denied(String),
}

/// 将源规范解析为一个或多个按顺序尝试的候选 URL。
async fn candidate_urls(
    source: &InstallSource,
    network: &NetworkPolicy,
    registry_url: &str,
) -> Result<UrlResolution> {
    match source {
        InstallSource::GitHubRepo(repo) => {
            // GitHub 的存档端点在重定向后位于 `codeload.github.com`，
            // 但我们访问的公共 URL 是 `github.com`。两者通常都出现在
            // 用户允许列表中；我们检查规范主机。
            Ok(UrlResolution::Resolved(vec![
                format!("https://github.com/{repo}/archive/refs/heads/main.tar.gz"),
                format!("https://github.com/{repo}/archive/refs/heads/master.tar.gz"),
            ]))
        }
        InstallSource::DirectUrl(url) => Ok(UrlResolution::Resolved(vec![url.clone()])),
        InstallSource::Registry(name) => {
            match fetch_registry(network, registry_url).await? {
                RegistryFetchResult::Loaded(doc) => {
                    let entry = doc
                        .skills
                        .get(name)
                        .with_context(|| format!("skill '{name}' not found in registry"))?
                        .clone();
                    let inner = InstallSource::parse(&entry.source).with_context(|| {
                        format!(
                            "registry entry for '{name}' has invalid source: {}",
                            entry.source
                        )
                    })?;
                    // 仅递归一层——禁止注册表指向注册表以避免循环。
                    if matches!(inner, InstallSource::Registry(_)) {
                        bail!("registry entry for '{name}' must not point to another registry");
                    }
                    // 对内层来源重用此函数，以便 GitHub 回退仍然适用。
                    Box::pin(candidate_urls(&inner, network, registry_url)).await
                }
                RegistryFetchResult::NeedsApproval(host) => Ok(UrlResolution::NeedsApproval(host)),
                RegistryFetchResult::Denied(host) => Ok(UrlResolution::Denied(host)),
            }
        }
    }
}

/// 下载策略允许主机且返回 2xx 的第一个 URL。
/// 如果每个候选都命中了 `Prompt` 则返回 `NeedsApproval`，或者如果每个候选都被拒绝则返回 `Denied`。
async fn download_first_success(
    urls: &[String],
    network: &NetworkPolicy,
    max_size: u64,
) -> Result<DownloadOutcome> {
    let mut last_status: Option<reqwest::StatusCode> = None;
    let mut prompt_host: Option<String> = None;
    let mut denied_host: Option<String> = None;
    for url in urls {
        let host = match host_from_url(url) {
            Some(h) => h,
            None => bail!("invalid download url: {url}"),
        };
        match network.decide(&host) {
            Decision::Allow => {}
            Decision::Deny => {
                denied_host.get_or_insert(host);
                continue;
            }
            Decision::Prompt => {
                prompt_host.get_or_insert(host);
                continue;
            }
        }
        match download_with_cap(url, max_size).await? {
            DownloadAttempt::Bytes(bytes) => {
                return Ok(DownloadOutcome::Bytes {
                    bytes,
                    url: url.clone(),
                });
            }
            DownloadAttempt::NotFound(status) => {
                last_status = Some(status);
                continue;
            }
        }
    }
    if let Some(host) = denied_host {
        return Ok(DownloadOutcome::Denied(host));
    }
    if let Some(host) = prompt_host {
        return Ok(DownloadOutcome::NeedsApproval(host));
    }
    bail!(
        "failed to download skill (last status: {})",
        last_status
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    );
}

enum DownloadAttempt {
    Bytes(Vec<u8>),
    NotFound(reqwest::StatusCode),
}

/// 将 URL 流式传输到内存中并带大小上限。在第一次读取会将缓冲区推到
/// `max_size * 4` 以上时中止（*4 考虑了压缩；解包步骤仍在*未压缩*字节上强制执行 `max_size`）。
async fn download_with_cap(url: &str, max_size: u64) -> Result<DownloadAttempt> {
    let resp = reqwest::get(url)
        .await
        .with_context(|| format!("failed to GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(DownloadAttempt::NotFound(status));
        }
        bail!("download {url} returned {status}");
    }
    // *压缩后*下载的软限制——远高于 max_size，以允许高度可压缩的有效载荷但仍有界限。
    let compressed_cap = max_size.saturating_mul(4);
    let bytes = resp
        .bytes()
        .await
        .with_context(|| format!("failed to read body of {url}"))?;
    if (bytes.len() as u64) > compressed_cap {
        bail!("download {url} exceeds compressed size cap of {compressed_cap} bytes");
    }
    Ok(DownloadAttempt::Bytes(bytes.to_vec()))
}

struct StagedSkill {
    skill_name: String,
    staged_path: PathBuf,
}

/// 验证 tarball 并将其提取到 `<skills_dir>/<name>.tmp/`。
fn stage_tarball(bytes: &[u8], skills_dir: &Path, max_size: u64) -> Result<StagedSkill> {
    fs::create_dir_all(skills_dir)
        .with_context(|| format!("failed to create skills directory {}", skills_dir.display()))?;

    // Two passes: first determine the skill name (and therefore the staged
    // dir) by finding the SKILL.md, then extract under that staged dir.
    // Both passes share the same archive bytes; we reset by wrapping fresh
    // decoders.

    let scan = scan_tarball(bytes, max_size)?;

    // Prepare staged directory. Use a `.tmp` suffix so a crashed install
    // never collides with a real name; remove any leftover from a prior
    // failed attempt.
    let staged_path = skills_dir.join(format!("{}.tmp", scan.skill_name));
    if staged_path.exists() {
        fs::remove_dir_all(&staged_path).with_context(|| {
            format!(
                "failed to clean stale staging dir {}",
                staged_path.display()
            )
        })?;
    }
    fs::create_dir_all(&staged_path)
        .with_context(|| format!("failed to create staging dir {}", staged_path.display()))?;

    // Second pass — extract.
    let result = extract_into(&scan, bytes, &staged_path, max_size);
    if let Err(err) = result {
        // Cleanup on failure so a half-staged directory doesn't survive.
        let _ = fs::remove_dir_all(&staged_path);
        return Err(err);
    }

    Ok(StagedSkill {
        skill_name: scan.skill_name,
        staged_path,
    })
}

struct TarballScan {
    /// 来自 SKILL.md 前置元数据的技能名称。
    skill_name: String,
    /// 要从每个条目中去除的存档前缀（例如 `repo-main/`）。可能为空。
    prefix: String,
    /// `prefix` 内 SKILL.md 所在的子目录（如果在根目录则为 `""`，
    /// 或对于捆绑多个技能的仓库为 `skills/<name>`）。
    skill_root: String,
}

/// 第一遍：定位 SKILL.md，验证前置元数据，计算总大小，
/// 拒绝所选安装子树内的路径遍历条目和符号链接。
/// 在此遍中我们不做任何写入；那是第二遍的任务。
fn scan_tarball(bytes: &[u8], max_size: u64) -> Result<TarballScan> {
    let cursor = std::io::Cursor::new(bytes);
    let gz = GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(gz);

    let mut total_size: u64 = 0;
    let mut prefix: Option<String> = None;
    let mut skill_md_relative: Option<(SkillMdCandidate, Vec<u8>)> = None;
    let mut link_paths: Vec<String> = Vec::new();

    for entry in archive
        .entries()
        .context("failed to read tar entries (corrupt archive?)")?
    {
        let mut entry = entry.context("failed to read tar entry")?;
        let header = entry.header().clone();
        let entry_type = header.entry_type();
        let path = entry
            .path()
            .context("tar entry has invalid path")?
            .to_path_buf();
        let path_str = path.to_string_lossy().into_owned();
        if !is_safe_path(&path) {
            return Err(InstallError::PathTraversal(path_str).into());
        }

        // Track total size against `max_size` (uncompressed). We honor `header
        // .size` rather than streaming-read every file; tar archives are
        // self-describing so this is reliable for non-malicious inputs and
        // catches the gzip-bomb case.
        if let Ok(size) = header.size() {
            total_size = total_size.saturating_add(size);
            if total_size > max_size {
                return Err(InstallError::OversizedTarball { limit: max_size }.into());
            }
        }

        // Detect prefix from the first entry. GitHub archives wrap everything
        // in `<repo>-<branch>/`; direct tarballs may have no prefix. We treat
        // the first path component as the prefix iff the archive has more than
        // one entry under it, but for SKILL.md detection we just strip the
        // first component if every entry shares it.
        if prefix.is_none() {
            if let Some(Component::Normal(first)) = path.components().next() {
                let candidate = first.to_string_lossy().into_owned();
                // Only treat the first component as a prefix if it's a
                // directory-like (no extension and the path has more
                // components). Otherwise leave prefix empty.
                if path.components().count() > 1 {
                    prefix = Some(candidate);
                } else {
                    prefix = Some(String::new());
                }
            } else {
                prefix = Some(String::new());
            }
        }

        if entry_type.is_symlink() || entry_type.is_hard_link() {
            link_paths.push(path_str);
            continue;
        }

        // SKILL.md detection. Match the same workflow layouts that runtime
        // discovery understands:
        //   * `<prefix>/SKILL.md`
        //   * `<prefix>/*/skills/<name>/SKILL.md`
        //   * `<prefix>/<name>/SKILL.md`
        if entry_type.is_file() {
            let stripped = strip_prefix(&path_str, prefix.as_deref().unwrap_or(""));
            if let Some(candidate) = skill_md_candidate(&stripped) {
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .context("failed to read SKILL.md from archive")?;
                // Prefer the most explicit match: repo-root SKILL.md first,
                // then known skill-directory layouts, then a single nested
                // `<name>/SKILL.md` repository.
                let replace = skill_md_relative
                    .as_ref()
                    .is_none_or(|(current, _)| candidate.rank < current.rank);
                if replace {
                    skill_md_relative = Some((candidate, buf));
                }
            }
        }
    }

    let prefix = prefix.unwrap_or_default();
    let (skill_md, skill_md_bytes) = skill_md_relative
        .ok_or(InstallError::MissingSkillMd)
        .map_err(anyhow::Error::from)?;

    for link_path in link_paths {
        if is_within_selected_root(&link_path, &prefix, &skill_md.skill_root) {
            return Err(InstallError::SymlinkRejected.into());
        }
    }

    // Parse frontmatter to extract the skill name. We reuse the same parser
    // shape as `SkillRegistry::parse_skill` but inline it here so we don't
    // depend on the discovery module's private function.
    let name = parse_frontmatter_name(&skill_md_bytes)?;

    Ok(TarballScan {
        skill_name: name,
        prefix,
        skill_root: skill_md.skill_root,
    })
}

struct SkillMdCandidate {
    rank: u8,
    skill_root: String,
}

fn skill_md_candidate(stripped_path: &str) -> Option<SkillMdCandidate> {
    if stripped_path.eq_ignore_ascii_case("SKILL.md") {
        return Some(SkillMdCandidate {
            rank: 0,
            skill_root: String::new(),
        });
    }

    let parts: Vec<&str> = stripped_path.split('/').collect();
    if parts
        .last()
        .is_none_or(|last| !last.eq_ignore_ascii_case("SKILL.md"))
    {
        return None;
    }

    // Common workflow-pack layouts:
    // `skills/<name>/SKILL.md`, `.agents/skills/<name>/SKILL.md`,
    // `.claude/skills/<name>/SKILL.md`, and nested package layouts such as
    // `packages/foo/skills/<name>/SKILL.md`.
    if parts.len() >= 3 {
        let container = parts[parts.len() - 3];
        let name = parts[parts.len() - 2];
        if container.eq_ignore_ascii_case("skills") && !name.is_empty() {
            return Some(SkillMdCandidate {
                rank: 1,
                skill_root: parts[..parts.len() - 1].join("/"),
            });
        }
    }

    // Single-skill repos sometimes keep their root tidy with
    // `<skill-name>/SKILL.md` plus sibling docs at repo root.
    if parts.len() == 2 && !parts[0].is_empty() {
        return Some(SkillMdCandidate {
            rank: 2,
            skill_root: parts[0].to_string(),
        });
    }

    None
}

fn extract_into(scan: &TarballScan, bytes: &[u8], dest: &Path, max_size: u64) -> Result<()> {
    let cursor = std::io::Cursor::new(bytes);
    let gz = GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(gz);

    let mut total_size: u64 = 0;
    let prefix_with_root = if scan.skill_root.is_empty() {
        scan.prefix.clone()
    } else if scan.prefix.is_empty() {
        scan.skill_root.clone()
    } else {
        format!("{}/{}", scan.prefix, scan.skill_root)
    };

    for entry in archive
        .entries()
        .context("failed to read tar entries (corrupt archive?)")?
    {
        let mut entry = entry.context("failed to read tar entry")?;
        let header = entry.header().clone();
        let entry_type = header.entry_type();
        let path = entry
            .path()
            .context("tar entry has invalid path")?
            .to_path_buf();
        let path_str = path.to_string_lossy().into_owned();
        if !is_safe_path(&path) {
            return Err(InstallError::PathTraversal(path_str).into());
        }

        // Only extract entries that live under our skill root. For simple
        // tarballs (`SKILL.md` at root) that's everything; for multi-skill
        // repos it's the `skills/<name>/` slice.
        let stripped = strip_prefix(&path_str, &prefix_with_root).into_owned();
        if stripped.is_empty() && entry_type.is_dir() {
            // The root directory itself — already created.
            continue;
        }
        if stripped == path_str && !prefix_with_root.is_empty() {
            // Nothing to strip => entry is outside our subtree, skip.
            continue;
        }
        // Defense-in-depth: re-validate the stripped path.
        let stripped_path = Path::new(&stripped);
        if !is_safe_path(stripped_path) {
            return Err(InstallError::PathTraversal(stripped).into());
        }
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(InstallError::SymlinkRejected.into());
        }

        let target = dest.join(stripped_path);
        // Final paranoia check: ensure the resolved target stays under dest.
        // We can't canonicalize (target doesn't exist yet), so we walk
        // components one more time after composing.
        let target_components: Vec<_> = target.components().collect();
        let dest_components: Vec<_> = dest.components().collect();
        if !target_components.starts_with(dest_components.as_slice()) {
            return Err(InstallError::PathTraversal(stripped).into());
        }

        if entry_type.is_dir() {
            fs::create_dir_all(&target)
                .with_context(|| format!("failed to create dir {}", target.display()))?;
            continue;
        }
        if entry_type.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create dir {}", parent.display()))?;
            }
            // Read into a buffer so we can enforce `max_size`. Files inside
            // a SKILL bundle are small; copying through a buffer is fine.
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .with_context(|| format!("failed to read {}", path.display()))?;
            total_size = total_size.saturating_add(buf.len() as u64);
            if total_size > max_size {
                return Err(InstallError::OversizedTarball { limit: max_size }.into());
            }
            let mut out = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&target)
                .with_context(|| format!("failed to create {}", target.display()))?;
            out.write_all(&buf)
                .with_context(|| format!("failed to write {}", target.display()))?;
        }
    }
    Ok(())
}

fn selected_root(prefix: &str, skill_root: &str) -> String {
    if skill_root.is_empty() {
        prefix.to_string()
    } else if prefix.is_empty() {
        skill_root.to_string()
    } else {
        format!("{prefix}/{skill_root}")
    }
}

fn is_within_selected_root(path: &str, prefix: &str, skill_root: &str) -> bool {
    let root = selected_root(prefix, skill_root);
    if root.is_empty() {
        return true;
    }
    path == root || path.starts_with(&format!("{root}/"))
}

/// 确保 tar 路径没有 `..` 段且不是绝对路径。
fn is_safe_path(path: &Path) -> bool {
    if path.is_absolute() {
        return false;
    }
    for component in path.components() {
        match component {
            Component::ParentDir => return false,
            Component::Prefix(_) | Component::RootDir => return false,
            _ => {}
        }
    }
    true
}

/// 从 tarball 路径中去除前导目录前缀（例如 `repo-main/`）。
fn strip_prefix<'a>(path: &'a str, prefix: &str) -> std::borrow::Cow<'a, str> {
    if prefix.is_empty() {
        return std::borrow::Cow::Borrowed(path);
    }
    let with_slash = format!("{prefix}/");
    if let Some(rest) = path.strip_prefix(&with_slash) {
        std::borrow::Cow::Owned(rest.to_string())
    } else if path == prefix {
        std::borrow::Cow::Borrowed("")
    } else {
        std::borrow::Cow::Borrowed(path)
    }
}

/// 提取 `name:` 并确保 `description:` 存在于 SKILL.md 前置元数据中。
/// 还验证前导的 `---` 围栏，以便我们尽早拒绝格式错误的文件。
fn parse_frontmatter_name(bytes: &[u8]) -> Result<String> {
    let content = std::str::from_utf8(bytes).context("SKILL.md is not valid UTF-8")?;
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        bail!("SKILL.md is missing the leading '---' frontmatter fence");
    }
    let after_open = &trimmed[3..];
    let close = after_open.find("---").ok_or_else(|| {
        anyhow::anyhow!("SKILL.md is missing the closing '---' frontmatter fence")
    })?;
    let frontmatter = &after_open[..close];

    let mut name: Option<String> = None;
    let mut has_description = false;
    for raw in frontmatter.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            match key.as_str() {
                "name" if !value.is_empty() => name = Some(value),
                "description" if !value.is_empty() => has_description = true,
                _ => {}
            }
        }
    }

    let name = name.ok_or(InstallError::MissingFrontmatterField("name"))?;
    if !has_description {
        return Err(InstallError::MissingFrontmatterField("description").into());
    }
    // Sanity check: name must be a single path-safe segment.
    if name.contains('/')
        || name.contains('\\')
        || name == "."
        || name == ".."
        || name.contains(' ')
    {
        bail!("SKILL.md `name` must be a single path-safe segment (got '{name}')");
    }
    Ok(name)
}

fn source_spec_string(source: &InstallSource) -> String {
    match source {
        InstallSource::GitHubRepo(repo) => format!("github:{repo}"),
        InstallSource::DirectUrl(url) => url.clone(),
        InstallSource::Registry(name) => name.clone(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 测试
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_source() {
        let s = InstallSource::parse("github:Hmbown/test-skill").unwrap();
        assert_eq!(
            s,
            InstallSource::GitHubRepo("Hmbown/test-skill".to_string())
        );
    }

    #[test]
    fn parse_github_source_rejects_missing_repo() {
        let err = InstallSource::parse("github:Hmbown").unwrap_err();
        assert!(err.to_string().contains("github source must"), "got: {err}");
    }

    #[test]
    fn parse_github_source_rejects_extra_slashes() {
        let err = InstallSource::parse("github:Hmbown/repo/extra").unwrap_err();
        assert!(err.to_string().contains("github source must"), "got: {err}");
    }

    #[test]
    fn parse_direct_url_source() {
        let s = InstallSource::parse("https://example.com/skill.tar.gz").unwrap();
        assert_eq!(
            s,
            InstallSource::DirectUrl("https://example.com/skill.tar.gz".to_string())
        );
        let s = InstallSource::parse("http://example.com/skill.tar.gz").unwrap();
        assert_eq!(
            s,
            InstallSource::DirectUrl("http://example.com/skill.tar.gz".to_string())
        );
    }

    #[test]
    fn parse_github_browser_url_routes_to_github_repo() {
        // Regression for #269: `https://github.com/<owner>/<repo>` was being
        // parsed as a DirectUrl, so the installer downloaded the HTML repo
        // page and tried to gzip-decode HTML ("invalid gzip header").
        for spec in [
            "https://github.com/obra/superpowers",
            "https://github.com/obra/superpowers/",
            "https://github.com/obra/superpowers.git",
            "https://github.com/obra/superpowers.git/",
            "https://www.github.com/obra/superpowers",
            "http://github.com/obra/superpowers",
            "  https://github.com/obra/superpowers  ",
        ] {
            let parsed = InstallSource::parse(spec)
                .unwrap_or_else(|err| panic!("parse({spec}) failed: {err}"));
            assert_eq!(
                parsed,
                InstallSource::GitHubRepo("obra/superpowers".to_string()),
                "spec {spec} must route to GitHubRepo",
            );
        }
    }

    #[test]
    fn parse_github_archive_url_stays_direct() {
        // URLs that point at a specific subresource (archive tarball, blob,
        // tree) are real direct URLs — the user picked that exact path.
        for spec in [
            "https://github.com/obra/superpowers/archive/refs/heads/main.tar.gz",
            "https://github.com/obra/superpowers/blob/main/README.md",
            "https://github.com/obra/superpowers/tree/main",
        ] {
            let parsed = InstallSource::parse(spec).unwrap();
            assert!(
                matches!(parsed, InstallSource::DirectUrl(_)),
                "spec {spec} must stay DirectUrl, got {parsed:?}",
            );
        }
    }

    #[test]
    fn parse_registry_source() {
        let s = InstallSource::parse("my-skill").unwrap();
        assert_eq!(s, InstallSource::Registry("my-skill".to_string()));
    }

    #[test]
    fn parse_rejects_empty() {
        assert!(InstallSource::parse("").is_err());
        assert!(InstallSource::parse("   ").is_err());
    }

    #[test]
    fn is_safe_path_rejects_traversal() {
        assert!(!is_safe_path(Path::new("../etc/passwd")));
        assert!(!is_safe_path(Path::new("foo/../bar")));
        assert!(!is_safe_path(Path::new("/etc/passwd")));
        assert!(is_safe_path(Path::new("foo/bar/baz")));
        assert!(is_safe_path(Path::new("SKILL.md")));
    }

    #[test]
    fn parse_frontmatter_extracts_name() {
        let body = b"---\nname: hello\ndescription: greeter\n---\nbody\n";
        assert_eq!(parse_frontmatter_name(body).unwrap(), "hello");
    }

    #[test]
    fn parse_frontmatter_missing_name_fails() {
        let body = b"---\ndescription: x\n---\n";
        let err = parse_frontmatter_name(body).unwrap_err();
        assert!(format!("{err}").contains("name"));
    }

    #[test]
    fn parse_frontmatter_missing_description_fails() {
        let body = b"---\nname: x\n---\n";
        let err = parse_frontmatter_name(body).unwrap_err();
        assert!(format!("{err}").contains("description"));
    }

    #[test]
    fn parse_frontmatter_rejects_unsafe_name() {
        let body = b"---\nname: ../evil\ndescription: x\n---\n";
        assert!(parse_frontmatter_name(body).is_err());

        let body = b"---\nname: a name with spaces\ndescription: x\n---\n";
        assert!(parse_frontmatter_name(body).is_err());
    }

    #[test]
    fn parse_frontmatter_requires_opening_fence() {
        let body = b"name: hello\ndescription: x\n";
        assert!(parse_frontmatter_name(body).is_err());
    }

    #[test]
    fn strip_prefix_handles_all_cases() {
        assert_eq!(strip_prefix("foo/bar", "foo"), "bar");
        assert_eq!(strip_prefix("foo", "foo"), "");
        assert_eq!(strip_prefix("baz/bar", "foo"), "baz/bar");
        assert_eq!(strip_prefix("foo/bar", ""), "foo/bar");
    }

    #[test]
    fn source_spec_string_roundtrips() {
        assert_eq!(
            source_spec_string(&InstallSource::GitHubRepo("a/b".into())),
            "github:a/b"
        );
        assert_eq!(
            source_spec_string(&InstallSource::DirectUrl("https://x".into())),
            "https://x"
        );
        assert_eq!(
            source_spec_string(&InstallSource::Registry("x".into())),
            "x"
        );
    }
}
