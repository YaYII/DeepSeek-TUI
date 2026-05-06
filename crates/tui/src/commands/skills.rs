//! Skills commands: skills, skill

use std::fmt::Write;

use crate::localization::MessageId;
use crate::network_policy::NetworkPolicy;
use crate::skills::SkillRegistry;
use crate::skills::install::{
    self, DEFAULT_MAX_SIZE_BYTES, DEFAULT_REGISTRY_URL, InstallOutcome, InstallSource,
    RegistryFetchResult, SkillSyncOutcome, SyncResult, UpdateResult,
};
use crate::tui::app::App;
use crate::tui::history::HistoryCell;

use super::CommandResult;

fn render_skill_warnings(app: &App, registry: &SkillRegistry) -> String {
    if registry.warnings().is_empty() {
        return String::new();
    }

    let mut out = String::new();
    let _ = writeln!(
        out,
        "\n{}",
        app.tr(MessageId::CmdSkillsWarningsHeader)
            .replace("{count}", &registry.warnings().len().to_string())
    );
    for warning in registry.warnings() {
        let _ = writeln!(out, "  - {warning}");
    }
    out
}

/// List all available skills. Pass `--remote` (or `remote`) to fetch the
/// curated registry instead of scanning the local skills directory.
/// Pass `sync` to pull the registry index and download all skills to the
/// local cache (`~/.deepseek/cache/skills/`).
pub fn list_skills(app: &mut App, arg: Option<&str>) -> CommandResult {
    if let Some(arg) = arg {
        let trimmed = arg.trim();
        if trimmed == "--remote" || trimmed == "remote" {
            return list_remote_skills(app);
        }
        if trimmed == "sync" || trimmed == "--sync" {
            return sync_skills(app);
        }
        if !trimmed.is_empty() {
            return CommandResult::error(app.tr(MessageId::CmdSkillsUsage));
        }
    }
    let skills_dir = app.skills_dir.clone();
    let registry = SkillRegistry::discover(&skills_dir);
    let warnings = render_skill_warnings(app, &registry);

    if registry.is_empty() {
        let msg = app
            .tr(MessageId::CmdSkillsNoneFound)
            .replace("{dir}", &skills_dir.display().to_string())
            .replace("{warnings}", &warnings);
        return CommandResult::message(msg);
    }

    let mut output = format!(
        "{}\n",
        app.tr(MessageId::CmdSkillsAvailableHeader)
            .replace("{count}", &registry.len().to_string())
    );
    output.push_str("─────────────────────────────\n");
    for skill in registry.list() {
        let _ = writeln!(output, "  /{} - {}", skill.name, skill.description);
    }
    let _ = write!(
        output,
        "\n{}",
        app.tr(MessageId::CmdSkillsUseHint)
            .replace("{dir}", &skills_dir.display().to_string())
            .replace("{warnings}", &warnings)
    );

    CommandResult::message(output)
}

/// Run a specific skill — activates skill for next user message, or
/// dispatches a sub-command (`install`, `update`, `uninstall`, `trust`).
/// Try to run a skill by exact name (used for unified slash-command namespace, #435).
/// Returns None when no skill with that name exists, so the caller can try other sources.
pub fn run_skill_by_name(app: &mut App, name: &str, _arg: Option<&str>) -> Option<CommandResult> {
    let skills_dir = app.skills_dir.clone();
    let registry = crate::skills::SkillRegistry::discover(&skills_dir);
    if registry.get(name).is_some() {
        Some(activate_skill(app, name))
    } else {
        None
    }
}

pub fn run_skill(app: &mut App, name: Option<&str>) -> CommandResult {
    let raw = match name {
        Some(n) => n.trim(),
        None => {
            return CommandResult::error(app.tr(MessageId::CmdSkillUsage));
        }
    };

    // Sub-command dispatch happens before the activation path so users can't
    // accidentally activate a skill literally named "install".
    let mut iter = raw.splitn(2, char::is_whitespace);
    let head = iter.next().unwrap_or("").trim();
    let rest = iter.next().unwrap_or("").trim();
    match head {
        "install" => return install_skill(app, rest),
        "update" => return update_skill(app, rest),
        "uninstall" => return uninstall_skill(app, rest),
        "trust" => return trust_skill(app, rest),
        _ => {}
    }

    activate_skill(app, raw)
}

fn activate_skill(app: &mut App, name: &str) -> CommandResult {
    // `/skill new` is a friendly alias for `/skill skill-creator`.
    let name = if name == "new" { "skill-creator" } else { name };

    let skills_dir = app.skills_dir.clone();
    let registry = SkillRegistry::discover(&skills_dir);

    if let Some(skill) = registry.get(name) {
        let instruction = format!(
            "You are now using a skill. Follow these instructions:\n\n# Skill: {}\n\n{}\n\n---\n\nNow respond to the user's request following the above skill instructions.",
            skill.name, skill.body
        );

        app.add_message(HistoryCell::System {
            content: app
                .tr(MessageId::CmdSkillSystemActivated)
                .replace("{name}", &skill.name)
                .replace("{description}", &skill.description),
        });

        app.active_skill = Some(instruction);

        CommandResult::message(
            app.tr(MessageId::CmdSkillActivated)
                .replace("{name}", &skill.name)
                .replace("{description}", &skill.description),
        )
    } else {
        let available: Vec<String> = registry.list().iter().map(|s| s.name.clone()).collect();
        let warnings = render_skill_warnings(app, &registry);

        if available.is_empty() {
            CommandResult::error(
                app.tr(MessageId::CmdSkillNotFoundNone)
                    .replace("{name}", name)
                    .replace("{warnings}", &warnings),
            )
        } else {
            CommandResult::error(
                app.tr(MessageId::CmdSkillNotFoundAvailable)
                    .replace("{name}", name)
                    .replace("{available}", &available.join(", "))
                    .replace("{warnings}", &warnings),
            )
        }
    }
}

// ─── /skill install ────────────────────────────────────────────────────────

fn install_skill(app: &mut App, spec: &str) -> CommandResult {
    if spec.is_empty() {
        return CommandResult::error(app.tr(MessageId::CmdSkillInstallUsage));
    }
    let source = match InstallSource::parse(spec) {
        Ok(s) => s,
        Err(err) => {
            return CommandResult::error(
                app.tr(MessageId::CmdSkillInvalidInstallSource)
                    .replace("{err}", &err.to_string()),
            );
        }
    };
    let skills_dir = app.skills_dir.clone();
    let (network, max_size, registry_url) = installer_settings(app);

    let outcome = run_async(async move {
        install::install_with_registry(
            source,
            &skills_dir,
            max_size,
            &network,
            false,
            &registry_url,
        )
        .await
    });

    match outcome {
        Ok(InstallOutcome::Installed(installed)) => {
            let path_str = path_or_default(&installed.path);
            CommandResult::message(
                app.tr(MessageId::CmdSkillInstalled)
                    .replace("{name}", &installed.name)
                    .replace("{source}", spec)
                    .replace("{path}", &path_str),
            )
        }
        Ok(InstallOutcome::NeedsApproval(host)) => {
            CommandResult::error(needs_approval_message(&host))
        }
        Ok(InstallOutcome::NetworkDenied(host)) => {
            CommandResult::error(network_denied_message(&host))
        }
        Err(err) => CommandResult::error(
            app.tr(MessageId::CmdSkillInstallFailed)
                .replace("{err}", &format!("{err:#}")),
        ),
    }
}

// ─── /skill update ─────────────────────────────────────────────────────────

fn update_skill(app: &mut App, name: &str) -> CommandResult {
    if name.is_empty() {
        return CommandResult::error(app.tr(MessageId::CmdSkillUpdateUsage));
    }
    let skills_dir = app.skills_dir.clone();
    let (network, max_size, registry_url) = installer_settings(app);
    let owned_name = name.to_string();
    let outcome = run_async(async move {
        install::update_with_registry(&owned_name, &skills_dir, max_size, &network, &registry_url)
            .await
    });

    match outcome {
        Ok(UpdateResult::NoChange) => {
            CommandResult::message(app.tr(MessageId::CmdSkillNoChange).replace("{name}", name))
        }
        Ok(UpdateResult::Updated(installed)) => CommandResult::message(
            app.tr(MessageId::CmdSkillUpdated)
                .replace("{name}", &installed.name)
                .replace("{path}", &path_or_default(&installed.path)),
        ),
        Ok(UpdateResult::NeedsApproval(host)) => {
            CommandResult::error(needs_approval_message(&host))
        }
        Ok(UpdateResult::NetworkDenied(host)) => {
            CommandResult::error(network_denied_message(&host))
        }
        Err(err) => CommandResult::error(
            app.tr(MessageId::CmdSkillUpdateFailed)
                .replace("{err}", &format!("{err:#}")),
        ),
    }
}

// ─── /skill uninstall ──────────────────────────────────────────────────────

fn uninstall_skill(app: &mut App, name: &str) -> CommandResult {
    if name.is_empty() {
        return CommandResult::error(app.tr(MessageId::CmdSkillUninstallUsage));
    }
    match install::uninstall(name, &app.skills_dir) {
        Ok(()) => {
            CommandResult::message(app.tr(MessageId::CmdSkillRemoved).replace("{name}", name))
        }
        Err(err) => CommandResult::error(
            app.tr(MessageId::CmdSkillUninstallFailed)
                .replace("{err}", &format!("{err:#}")),
        ),
    }
}

// ─── /skill trust ──────────────────────────────────────────────────────────

fn trust_skill(app: &mut App, name: &str) -> CommandResult {
    if name.is_empty() {
        return CommandResult::error(app.tr(MessageId::CmdSkillTrustUsage));
    }
    match install::trust(name, &app.skills_dir) {
        Ok(()) => {
            CommandResult::message(app.tr(MessageId::CmdSkillTrusted).replace("{name}", name))
        }
        Err(err) => CommandResult::error(
            app.tr(MessageId::CmdSkillTrustFailed)
                .replace("{err}", &format!("{err:#}")),
        ),
    }
}

// ─── /skills --remote ──────────────────────────────────────────────────────

/// List skills available in the configured curated registry.
pub fn list_remote_skills(app: &mut App) -> CommandResult {
    let (network, _max_size, registry_url) = installer_settings(app);
    let registry = run_async(async move { install::fetch_registry(&network, &registry_url).await });
    match registry {
        Ok(RegistryFetchResult::Loaded(doc)) => {
            if doc.skills.is_empty() {
                return CommandResult::message(app.tr(MessageId::CmdSkillsRegistryEmpty));
            }
            let mut out = format!(
                "{}\n",
                app.tr(MessageId::CmdSkillsRemoteHeader)
                    .replace("{count}", &doc.skills.len().to_string())
            );
            out.push_str("─────────────────────────────\n");
            for (name, entry) in &doc.skills {
                let _ = writeln!(
                    out,
                    "  {name} — {} (source: {})",
                    entry.description.clone().unwrap_or_default(),
                    entry.source
                );
            }
            let _ = write!(out, "\n{}", app.tr(MessageId::CmdSkillsRemoteInstallHint));
            CommandResult::message(out)
        }
        Ok(RegistryFetchResult::NeedsApproval(host)) => {
            CommandResult::error(needs_approval_message(&host))
        }
        Ok(RegistryFetchResult::Denied(host)) => {
            CommandResult::error(network_denied_message(&host))
        }
        Err(err) => CommandResult::error(
            app.tr(MessageId::CmdSkillsFetchFailed)
                .replace("{err}", &format!("{err:#}")),
        ),
    }
}

// ─── /skills sync ──────────────────────────────────────────────────────────

/// Fetch the remote registry index and download every listed skill into the
/// local cache (`~/.deepseek/cache/skills/<name>/`).
///
/// For each skill the sync checks the cached ETag / SHA-256 before
/// downloading so unchanged skills are skipped in O(1) network round-trips.
fn sync_skills(app: &mut App) -> CommandResult {
    let (network, max_size, registry_url) = installer_settings(app);
    let cache_dir = install::default_cache_skills_dir();

    let result = run_async(async move {
        install::sync_registry(&network, &registry_url, &cache_dir, max_size).await
    });

    match result {
        Ok(SyncResult::RegistryDenied(host)) => CommandResult::error(network_denied_message(&host)),
        Ok(SyncResult::RegistryNeedsApproval(host)) => {
            CommandResult::error(needs_approval_message(&host))
        }
        Ok(SyncResult::Done { outcomes }) => {
            let total = outcomes.len();
            let mut downloaded = 0usize;
            let mut fresh = 0usize;
            let mut failed = 0usize;
            let mut out = app.tr(MessageId::CmdSkillsSyncComplete).to_string();
            out.push_str("\n\n");

            for outcome in &outcomes {
                match outcome {
                    SkillSyncOutcome::Downloaded { name, path } => {
                        downloaded += 1;
                        let _ = writeln!(
                            out,
                            "{}",
                            app.tr(MessageId::CmdSkillsSyncDownloaded)
                                .replace("{name}", name)
                                .replace("{path}", &path.display().to_string())
                        );
                    }
                    SkillSyncOutcome::Fresh { name } => {
                        fresh += 1;
                        let _ = writeln!(
                            out,
                            "{}",
                            app.tr(MessageId::CmdSkillsSyncFresh)
                                .replace("{name}", name)
                        );
                    }
                    SkillSyncOutcome::Failed { name, reason } => {
                        failed += 1;
                        let _ = writeln!(
                            out,
                            "{}",
                            app.tr(MessageId::CmdSkillsSyncFailedItem)
                                .replace("{name}", name)
                                .replace("{reason}", reason)
                        );
                    }
                    SkillSyncOutcome::Denied { name, host } => {
                        failed += 1;
                        let _ = writeln!(
                            out,
                            "{}",
                            app.tr(MessageId::CmdSkillsSyncDeniedItem)
                                .replace("{name}", name)
                                .replace("{host}", host)
                        );
                    }
                    SkillSyncOutcome::NeedsApproval { name, host } => {
                        failed += 1;
                        let _ = writeln!(
                            out,
                            "{}",
                            app.tr(MessageId::CmdSkillsSyncNeedsApprovalItem)
                                .replace("{name}", name)
                                .replace("{host}", host)
                        );
                    }
                }
            }

            let _ = write!(
                out,
                "\n{}",
                app.tr(MessageId::CmdSkillsSyncSummary)
                    .replace("{total}", &total.to_string())
                    .replace("{downloaded}", &downloaded.to_string())
                    .replace("{fresh}", &fresh.to_string())
                    .replace("{failed}", &failed.to_string())
            );

            CommandResult::message(out)
        }
        Err(err) => CommandResult::error(
            app.tr(MessageId::CmdSkillsSyncFailed)
                .replace("{err}", &format!("{err:#}")),
        ),
    }
}

// ─── helpers ───────────────────────────────────────────────────────────────

/// Read the active config knobs for the installer.
///
/// We load `Config::load` on demand because [`App`] does not carry a `Config`
/// field — and loading is cheap (small TOML file) compared to the network
/// round-trip the install/update operation will incur next. If the config
/// fails to parse, we fall back to defaults so the user still gets a
/// network-gated install rather than a silent crash.
fn installer_settings(_app: &App) -> (NetworkPolicy, u64, String) {
    let cfg = crate::config::Config::load(None, None).unwrap_or_default();
    let network = cfg
        .network
        .clone()
        .map(|policy| policy.into_runtime())
        .unwrap_or_default();
    let skills_cfg = cfg.skills.as_ref();
    let max_size = skills_cfg
        .and_then(|s| s.max_install_size_bytes)
        .unwrap_or(DEFAULT_MAX_SIZE_BYTES);
    let registry_url = skills_cfg
        .and_then(|s| s.registry_url.clone())
        .unwrap_or_else(|| DEFAULT_REGISTRY_URL.to_string());
    (network, max_size, registry_url)
}

fn run_async<F, T>(future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    // We're on the TUI's thread, which is part of the multi-threaded runtime.
    // `block_in_place` + `Handle::current().block_on` is the pattern used by
    // `commands/cycle.rs` to bridge sync slash-command handlers back into the
    // async ecosystem.
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
}

fn path_or_default(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| {
            // Display with parent so the user sees the full skill location.
            // We intentionally use `display()` here because it's just for
            // user-facing output, not for path comparisons.
            let parent = path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            if parent.is_empty() {
                n.to_string_lossy().to_string()
            } else {
                format!("{parent}/{}", n.to_string_lossy())
            }
        })
        .unwrap_or_else(|| path.display().to_string())
}

fn needs_approval_message(host: &str) -> String {
    tr_default(MessageId::CmdSkillsNetworkNeedsApproval).replace("{host}", host)
}

fn network_denied_message(host: &str) -> String {
    tr_default(MessageId::CmdSkillsNetworkDenied).replace("{host}", host)
}

fn tr_default(id: MessageId) -> &'static str {
    crate::localization::tr(crate::localization::Locale::default(), id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::tui::app::{App, TuiOptions};
    use tempfile::TempDir;

    fn create_test_app_with_tmpdir(tmpdir: &TempDir) -> App {
        let options = TuiOptions {
            model: "deepseek-v4-pro".to_string(),
            workspace: tmpdir.path().to_path_buf(),
            config_path: None,
            config_profile: None,
            allow_shell: false,
            use_alt_screen: true,
            use_mouse_capture: false,
            use_bracketed_paste: true,
            max_subagents: 1,
            skills_dir: tmpdir.path().join("skills"),
            memory_path: tmpdir.path().join("memory.md"),
            notes_path: tmpdir.path().join("notes.txt"),
            mcp_config_path: tmpdir.path().join("mcp.json"),
            use_memory: false,
            start_in_agent_mode: false,
            skip_onboarding: true,
            yolo: false,
            resume_session_id: None,
            initial_input: None,
        };
        App::new(options, &Config::default())
    }

    fn create_skill_dir(tmpdir: &TempDir, skill_name: &str, skill_content: &str) {
        let skill_dir = tmpdir.path().join("skills").join(skill_name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), skill_content).unwrap();
    }

    #[test]
    fn test_list_skills_empty_directory() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let result = list_skills(&mut app, None);
        assert!(result.message.is_some());
        let msg = result.message.unwrap();
        assert!(msg.contains("No skills found"));
        assert!(msg.contains("Skills location:"));
    }

    #[test]
    fn test_list_skills_with_skills() {
        let tmpdir = TempDir::new().unwrap();
        create_skill_dir(
            &tmpdir,
            "test-skill",
            "---\nname: test-skill\ndescription: A test skill\n---\nDo something",
        );
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let result = list_skills(&mut app, None);
        assert!(result.message.is_some());
        let msg = result.message.unwrap();
        assert!(msg.contains("Available skills"));
        assert!(msg.contains("/test-skill"));
    }

    #[test]
    fn test_skill_subcommand_dispatch_install_usage() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        // Empty install spec → usage hint, not invalid-source error.
        let result = run_skill(&mut app, Some("install"));
        let msg = result.message.unwrap();
        assert!(msg.contains("/skill install"), "got: {msg}");
    }

    #[test]
    fn test_skill_subcommand_dispatch_uninstall_missing() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let result = run_skill(&mut app, Some("uninstall absent-skill"));
        let msg = result.message.unwrap();
        assert!(msg.contains("not installed"), "got: {msg}");
    }

    #[test]
    fn test_run_skill_without_name() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let result = run_skill(&mut app, None);
        assert!(result.message.is_some());
        assert!(result.message.unwrap().contains("Usage: /skill"));
    }

    #[test]
    fn test_run_skill_not_found() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let result = run_skill(&mut app, Some("nonexistent"));
        assert!(result.message.is_some());
        let msg = result.message.unwrap();
        assert!(msg.contains("not found"));
    }

    #[test]
    fn test_run_skill_activates() {
        let tmpdir = TempDir::new().unwrap();
        create_skill_dir(
            &tmpdir,
            "test-skill",
            "---\nname: test-skill\ndescription: A test skill\n---\nDo something special",
        );
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let result = run_skill(&mut app, Some("test-skill"));
        assert!(result.message.is_some());
        let msg = result.message.unwrap();
        assert!(msg.contains("Skill 'test-skill' activated"));
        assert!(msg.contains("A test skill"));
        assert!(app.active_skill.is_some());
        assert!(!app.history.is_empty());
    }
}
