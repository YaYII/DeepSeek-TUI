#![allow(dead_code)]
//! System prompts for different modes.
//!
//! Prompts are assembled from composable layers loaded at compile time:
//!   base.md → personality overlay → mode delta → approval policy
//!
//! This keeps each concern in its own file and makes prompt tuning
//! a single-file operation.

use crate::models::SystemPrompt;
use crate::project_context::{ProjectContext, load_project_context_with_parents};
use crate::tui::app::AppMode;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default)]
pub struct PromptSessionContext<'a> {
    pub user_memory_block: Option<&'a str>,
    pub goal_objective: Option<&'a str>,
}

/// Conventional location for the structured session-handoff artifact (#32).
/// A previous session writes it on exit / `/compact`; the next session reads
/// it back on startup and prepends it to the system prompt so a fresh agent
/// doesn't have to re-discover open blockers from scratch.
pub const HANDOFF_RELATIVE_PATH: &str = ".deepseek/handoff.md";

/// Per-file size cap for `instructions = [...]` entries (#454). Mirrors
/// the existing project-context cap in `project_context::load_context_file`
/// so a malicious / oversized include can't blow the prompt budget on
/// its own. Files larger than this are truncated with an `[…elided]`
/// marker rather than skipped entirely so the model still sees the head.
const INSTRUCTIONS_FILE_MAX_BYTES: usize = 100 * 1024;

/// Render the `instructions = [...]` config array as a single
/// system-prompt block (#454). Each path is loaded in declared order;
/// missing files are skipped with a tracing warning so a stale entry
/// in `~/.deepseek/config.toml` doesn't fail the launch. Empty input
/// (or all paths missing) returns `None` so callers append nothing.
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
                    format!("{}\n[…elided]", &trimmed[..head_end])
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
                    "skipping unreadable instructions file"
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

/// Read the workspace-local handoff artifact, if present, and format it as a
/// system-prompt block. Returns `None` when the file is absent or empty so
/// callers can keep the default-uncluttered prompt for fresh workspaces.
fn load_handoff_block(workspace: &Path) -> Option<String> {
    let path = workspace.join(HANDOFF_RELATIVE_PATH);
    let raw = std::fs::read_to_string(&path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(format!(
        "## Previous Session Handoff\n\nThe previous session in this workspace left a handoff at `{}`. Consider it the first artifact to read on this turn — open blockers, in-flight changes, and recent decisions live there. Update or rewrite it before exiting if state changes materially.\n\n{}",
        HANDOFF_RELATIVE_PATH, trimmed
    ))
}

// ── Prompt layers loaded at compile time ──────────────────────────────

/// Core: task execution, tool-use rules, output format, toolbox reference,
/// "When NOT to use" guidance, sub-agent sentinel protocol.
pub const BASE_PROMPT: &str = include_str!("prompts/base.md");

/// Personality overlays — voice and tone.
pub const CALM_PERSONALITY: &str = include_str!("prompts/personalities/calm.md");
pub const PLAYFUL_PERSONALITY: &str = include_str!("prompts/personalities/playful.md");

/// Mode deltas — permissions, workflow expectations, mode-specific rules.
pub const AGENT_MODE: &str = include_str!("prompts/modes/agent.md");
pub const PLAN_MODE: &str = include_str!("prompts/modes/plan.md");
pub const YOLO_MODE: &str = include_str!("prompts/modes/yolo.md");

/// Approval-policy overlays — whether tool calls are auto-approved,
/// require confirmation, or are blocked.
pub const AUTO_APPROVAL: &str = include_str!("prompts/approvals/auto.md");
pub const SUGGEST_APPROVAL: &str = include_str!("prompts/approvals/suggest.md");
pub const NEVER_APPROVAL: &str = include_str!("prompts/approvals/never.md");

/// Compaction handoff template — written into the system prompt so the
/// model knows the format to use when writing `.deepseek/handoff.md`.
pub const COMPACT_TEMPLATE: &str = include_str!("prompts/compact.md");

// ── Legacy prompt constants (kept for backwards compatibility) ────────

/// Legacy base prompt (agent.txt — now decomposed into base.md + overlays).
/// Still available for callers that haven't migrated to the layered API.
pub const AGENT_PROMPT: &str = include_str!("prompts/agent.txt");
pub const YOLO_PROMPT: &str = include_str!("prompts/yolo.txt");
pub const PLAN_PROMPT: &str = include_str!("prompts/plan.txt");

#[derive(Debug, Clone, Copy)]
pub(crate) struct PromptLayer {
    pub relative_path: &'static str,
    pub builtin: &'static str,
}

/// Prompt layers that can be translated into `~/.deepseek/i18n`.
///
/// Localized files use the flat `name.i18n.ext` convention, e.g.
/// `base.md` -> `base.i18n.md` and `modes/agent.md` -> `agent.i18n.md`.
pub(crate) const LOCALIZABLE_PROMPT_LAYERS: &[PromptLayer] = &[
    PromptLayer {
        relative_path: "base.md",
        builtin: BASE_PROMPT,
    },
    PromptLayer {
        relative_path: "personalities/calm.md",
        builtin: CALM_PERSONALITY,
    },
    PromptLayer {
        relative_path: "personalities/playful.md",
        builtin: PLAYFUL_PERSONALITY,
    },
    PromptLayer {
        relative_path: "modes/agent.md",
        builtin: AGENT_MODE,
    },
    PromptLayer {
        relative_path: "modes/plan.md",
        builtin: PLAN_MODE,
    },
    PromptLayer {
        relative_path: "modes/yolo.md",
        builtin: YOLO_MODE,
    },
    PromptLayer {
        relative_path: "approvals/auto.md",
        builtin: AUTO_APPROVAL,
    },
    PromptLayer {
        relative_path: "approvals/suggest.md",
        builtin: SUGGEST_APPROVAL,
    },
    PromptLayer {
        relative_path: "approvals/never.md",
        builtin: NEVER_APPROVAL,
    },
    PromptLayer {
        relative_path: "compact.md",
        builtin: COMPACT_TEMPLATE,
    },
    PromptLayer {
        relative_path: "agent.txt",
        builtin: AGENT_PROMPT,
    },
    PromptLayer {
        relative_path: "yolo.txt",
        builtin: YOLO_PROMPT,
    },
    PromptLayer {
        relative_path: "plan.txt",
        builtin: PLAN_PROMPT,
    },
];

pub(crate) fn bundled_prompt_path(relative_path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("prompts")
        .join(relative_path)
}

pub(crate) fn i18n_prompt_file_name(relative_path: &str) -> String {
    let path = Path::new(relative_path);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(relative_path);

    if let Some((stem, extension)) = file_name.rsplit_once('.') {
        format!("{stem}.i18n.{extension}")
    } else {
        format!("{file_name}.i18n")
    }
}

fn prompt_i18n_dir() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("DEEPSEEK_I18N_DIR") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(crate::config::expand_path(trimmed));
        }
    }

    #[cfg(test)]
    {
        None
    }

    #[cfg(not(test))]
    {
        crate::config::default_i18n_dir()
    }
}

fn localized_prompt_from_dir(dir: &Path, relative_path: &str) -> Option<String> {
    let path = dir.join(i18n_prompt_file_name(relative_path));
    match std::fs::read_to_string(&path) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                tracing::warn!(
                    target: "i18n",
                    ?path,
                    "skipping empty localized prompt file"
                );
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            tracing::warn!(
                target: "i18n",
                ?err,
                ?path,
                "skipping unreadable localized prompt file"
            );
            None
        }
    }
}

fn prompt_layer(relative_path: &str, builtin: &str) -> String {
    prompt_i18n_dir()
        .and_then(|dir| localized_prompt_from_dir(&dir, relative_path))
        .unwrap_or_else(|| builtin.trim().to_string())
}

fn prompt_layer_from(layer: PromptLayer) -> String {
    prompt_layer(layer.relative_path, layer.builtin)
}

fn find_prompt_layer(relative_path: &str) -> PromptLayer {
    LOCALIZABLE_PROMPT_LAYERS
        .iter()
        .copied()
        .find(|layer| layer.relative_path == relative_path)
        .expect("prompt layer must be registered")
}

// ── Personality selection ─────────────────────────────────────────────

/// Which personality overlay to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Personality {
    /// Cool, spatial, reserved — the default.
    Calm,
    /// Warm, energetic, playful — alternative for fun mode.
    Playful,
}

impl Personality {
    /// Resolve from the `calm_mode` settings flag.
    /// When `calm_mode` is true → Calm; when false → Playful (future).
    /// For now, always returns Calm — Playful is wired but opt-in.
    #[must_use]
    pub fn from_settings(calm_mode: bool) -> Self {
        if calm_mode {
            Self::Calm
        } else {
            // Future: when playful mode is exposed in settings, return Playful here.
            // For now, calm is the only default.
            Self::Calm
        }
    }

    fn prompt(self) -> String {
        match self {
            Self::Calm => prompt_layer_from(find_prompt_layer("personalities/calm.md")),
            Self::Playful => prompt_layer_from(find_prompt_layer("personalities/playful.md")),
        }
    }
}

// ── Composition ───────────────────────────────────────────────────────

fn mode_prompt(mode: AppMode) -> String {
    match mode {
        AppMode::Agent => prompt_layer_from(find_prompt_layer("modes/agent.md")),
        AppMode::Yolo => prompt_layer_from(find_prompt_layer("modes/yolo.md")),
        AppMode::Plan => prompt_layer_from(find_prompt_layer("modes/plan.md")),
    }
}

fn approval_prompt(mode: AppMode) -> String {
    match mode {
        AppMode::Agent => prompt_layer_from(find_prompt_layer("approvals/suggest.md")),
        AppMode::Yolo => prompt_layer_from(find_prompt_layer("approvals/auto.md")),
        AppMode::Plan => prompt_layer_from(find_prompt_layer("approvals/never.md")),
    }
}

/// Compose the full system prompt in deterministic order:
///   1. base.md        — core identity, toolbox, execution contract
///   2. personality    — voice and tone overlay
///   3. mode delta     — mode-specific permissions and workflow
///   4. approval policy — tool-approval behavior
///
/// Each layer is separated by a blank line for readability in the
/// rendered prompt (the model sees them as contiguous sections).
pub fn compose_prompt(mode: AppMode, personality: Personality) -> String {
    let parts = [
        prompt_layer_from(find_prompt_layer("base.md")),
        personality.prompt(),
        mode_prompt(mode),
        approval_prompt(mode),
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

/// Compose for the default personality (Calm).
fn compose_mode_prompt(mode: AppMode) -> String {
    compose_prompt(mode, Personality::Calm)
}

// ── Public API ────────────────────────────────────────────────────────

/// Get the system prompt for a specific mode (default Calm personality).
pub fn system_prompt_for_mode(mode: AppMode) -> SystemPrompt {
    SystemPrompt::Text(compose_mode_prompt(mode))
}

/// Get the system prompt for a specific mode with explicit personality.
pub fn system_prompt_for_mode_with_personality(
    mode: AppMode,
    personality: Personality,
) -> SystemPrompt {
    SystemPrompt::Text(compose_prompt(mode, personality))
}

/// Get the system prompt for a specific mode with project context.
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

/// Get the system prompt for a specific mode with project and skills context.
///
/// **Volatile-content-last invariant.** Blocks are appended in order from
/// most-static to most-volatile so DeepSeek's KV prefix cache hits the
/// longest possible byte prefix turn-over-turn:
///
///   1. mode prompt (compile-time constant)
///   2. project context / fallback (workspace-static)
///   3. skills block (skills-dir-static)
///   4. `## Context Management` (compile-time constant, Agent/Yolo only)
///   5. compaction handoff template (compile-time constant)
///   6. handoff block — file-backed; rewritten by `/compact` and on exit
///
/// Anything appended after a volatile block forfeits the cache for the rest
/// of the request. New blocks belong above the handoff boundary unless they
/// themselves are turn-volatile. Working-set metadata is now injected into the
/// latest user message as per-turn metadata instead of this system prompt.
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
    let mode_prompt = compose_mode_prompt(mode);

    // Load project context from workspace
    let project_context = load_project_context_with_parents(workspace);

    // 1–2. Mode prompt + project context (or fallback automap).
    let mut full_prompt = if let Some(project_block) = project_context.as_system_block() {
        format!("{}\n\n{}", mode_prompt, project_block)
    } else {
        // Fallback: Generate an automatic project map summary
        let summary = crate::utils::summarize_project(workspace);
        let tree = crate::utils::project_tree(workspace, 2); // Shallow tree for prompt
        format!(
            "{}\n\n### Project Structure (Automatic Map)\n**Summary:** {}\n\n**Tree:**\n```\n{}\n```",
            mode_prompt, summary, tree
        )
    };

    // 2.5a. Configured `instructions = [...]` files (#454). Loaded
    // and concatenated in declared order. Lives above the skills
    // block so it's part of the workspace-static layer that the KV
    // prefix cache can hit, and so per-project overrides apply
    // consistently turn-over-turn.
    if let Some(paths) = instructions
        && let Some(block) = render_instructions_block(paths)
    {
        full_prompt = format!("{full_prompt}\n\n{block}");
    }

    // 2.5b. User memory block (#489). Goes above skills/context-management
    // because it's session-stable: the memory file changes when the user
    // edits it via `/memory` or `# foo` quick-add, but not turn-over-turn.
    if let Some(memory_block) = session_context.user_memory_block
        && !memory_block.trim().is_empty()
    {
        full_prompt = format!("{full_prompt}\n\n{memory_block}");
    }

    if let Some(goal_objective) = session_context.goal_objective
        && !goal_objective.trim().is_empty()
    {
        full_prompt = format!(
            "{full_prompt}\n\n## Current Session Goal\n\n<session_goal>\n{}\n</session_goal>",
            goal_objective.trim()
        );
    }

    // 3. Skills block. #432: walks every candidate workspace
    // skills directory (`.agents/skills`, `skills`,
    // `.opencode/skills`, `.claude/skills`) plus the global
    // default so skills installed for any AI-tool convention show
    // up in the catalogue. The legacy single-`skills_dir` path is
    // honoured as a fallback for callers that don't supply a
    // workspace-aware view; it falls through to the same merged
    // registry when available.
    let skills_block = crate::skills::render_available_skills_context_for_workspace(workspace)
        .or_else(|| skills_dir.and_then(crate::skills::render_available_skills_context));
    if let Some(block) = skills_block {
        full_prompt = format!("{full_prompt}\n\n{block}");
    }

    // 4. Context Management (Agent / Yolo only).
    if matches!(mode, AppMode::Agent | AppMode::Yolo) {
        full_prompt.push_str(
            "\n\n## Context Management\n\n\
             When the conversation gets long (you'll see a context usage indicator), you can:\n\
             1. Use `/compact` to summarize earlier context and free up space\n\
             2. The system will preserve important information (files you're working on, recent messages, tool results)\n\
             3. After compaction, you'll see a summary of what was discussed and can continue seamlessly\n\n\
             If you notice context is getting long (>80%), proactively suggest using `/compact` to the user.\n\n\
             ### Prompt-cache awareness\n\n\
             DeepSeek caches the longest *byte-stable prefix* of every request and charges roughly 100× less for cache-hit tokens than miss tokens. The system prompt above is layered most-static-first specifically so the prefix stays stable turn-over-turn. To keep cache hits high:\n\
             - **Working set location:** the current repo working set is injected into the latest user message inside a `<turn_meta>` block. Treat it as high-priority turn metadata, not as a stable system-prompt section.\n\
             - **Append, don't reorder.** New context goes at the end (latest user / tool messages). Reshuffling earlier messages or rewriting their content invalidates the cache for everything after the change.\n\
             - **Don't paraphrase quoted content.** If you've already read a file, refer to it by path or line range instead of re-quoting it with different formatting.\n\
             - **Use `/compact` as a hard reset, not a tweak.** Compaction is meant for when the cache is already losing — it intentionally rewrites the prefix to a shorter summary. Don't trigger it for small wins.\n\
             - **Read once, refer back.** Re-reading the same file produces a different tool-result envelope than the prior read; it's cheaper to scroll back than to re-fetch.\n\
             - **Footer chip:** the `cache hit %` chip turns red below 40% and yellow below 80%. If it's been red for several turns, that's a signal to consolidate."
        );
    }

    // 5. Compaction handoff template — so the model knows the format to use
    //    when writing `.deepseek/handoff.md` on exit / `/compact`.
    full_prompt.push_str("\n\n");
    full_prompt.push_str(&prompt_layer_from(find_prompt_layer("compact.md")));

    // ── Volatile-content boundary ─────────────────────────────────────────
    // Everything below drifts mid-session and busts the prefix cache for
    // bytes that follow. Keep new static blocks above this comment.

    // 6. Previous-session handoff (file-backed, rewritten by `/compact`).
    if let Some(handoff_block) = load_handoff_block(workspace) {
        full_prompt = format!("{full_prompt}\n\n{handoff_block}");
    }

    SystemPrompt::Text(full_prompt)
}

/// Build a system prompt with explicit project context
pub fn build_system_prompt(base: &str, project_context: Option<&ProjectContext>) -> SystemPrompt {
    let full_prompt =
        match project_context.and_then(super::project_context::ProjectContext::as_system_block) {
            Some(project_block) => format!("{}\n\n{}", base.trim(), project_block),
            None => base.trim().to_string(),
        };
    SystemPrompt::Text(full_prompt)
}

// ── Legacy functions for backwards compatibility ──────────────────────

pub fn base_system_prompt() -> SystemPrompt {
    SystemPrompt::Text(prompt_layer_from(find_prompt_layer("base.md")))
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

/// Legacy monolithic Agent prompt with the same local-i18n-first behavior as
/// the layered prompt path. Prefer [`agent_system_prompt`] for new call sites.
pub fn legacy_agent_system_prompt() -> SystemPrompt {
    SystemPrompt::Text(prompt_layer_from(find_prompt_layer("agent.txt")))
}

/// Legacy monolithic YOLO prompt with local-i18n-first fallback.
pub fn legacy_yolo_system_prompt() -> SystemPrompt {
    SystemPrompt::Text(prompt_layer_from(find_prompt_layer("yolo.txt")))
}

/// Legacy monolithic Plan prompt with local-i18n-first fallback.
pub fn legacy_plan_system_prompt() -> SystemPrompt {
    SystemPrompt::Text(prompt_layer_from(find_prompt_layer("plan.txt")))
}

#[cfg(test)]
mod tests {
    // Don't assert on prose. If you wouldn't fail a code review for
    // changing the wording, don't fail a test for it.
    use super::*;
    use tempfile::tempdir;

    /// Discriminator unique to the injected handoff block (not present in the
    /// agent prompt's own discussion of the convention).
    const HANDOFF_BLOCK_MARKER: &str = "left a handoff at `.deepseek/handoff.md`";

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
            SystemPrompt::Blocks(_) => panic!("expected text system prompt"),
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
            SystemPrompt::Blocks(_) => panic!("expected text system prompt"),
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
            SystemPrompt::Blocks(_) => panic!("expected text system prompt"),
        };
        assert!(!prompt.contains(HANDOFF_BLOCK_MARKER));
    }

    #[test]
    fn compose_prompt_includes_all_layers() {
        let prompt = compose_prompt(AppMode::Agent, Personality::Calm);
        // Base layer
        assert!(prompt.contains("You are DeepSeek TUI"));
        // Personality layer
        assert!(prompt.contains("Personality: Calm"));
        // Mode layer
        assert!(prompt.contains("Mode: Agent"));
        // Approval layer
        assert!(prompt.contains("Approval Policy: Suggest"));
    }

    #[test]
    fn compose_prompt_deterministic_order() {
        let prompt = compose_prompt(AppMode::Yolo, Personality::Calm);
        let base_pos = prompt.find("You are DeepSeek TUI").unwrap();
        let personality_pos = prompt.find("Personality: Calm").unwrap();
        let mode_pos = prompt.find("Mode: YOLO").unwrap();
        let approval_pos = prompt.find("Approval Policy: Auto").unwrap();

        assert!(base_pos < personality_pos);
        assert!(personality_pos < mode_pos);
        assert!(mode_pos < approval_pos);
    }

    #[test]
    fn each_mode_gets_correct_approval() {
        assert!(
            compose_prompt(AppMode::Agent, Personality::Calm).contains("Approval Policy: Suggest")
        );
        assert!(compose_prompt(AppMode::Yolo, Personality::Calm).contains("Approval Policy: Auto"));
        assert!(
            compose_prompt(AppMode::Plan, Personality::Calm).contains("Approval Policy: Never")
        );
    }

    #[test]
    fn personality_switches_correctly() {
        let calm = compose_prompt(AppMode::Agent, Personality::Calm);
        let playful = compose_prompt(AppMode::Agent, Personality::Playful);
        assert!(calm.contains("Personality: Calm"));
        assert!(playful.contains("Personality: Playful"));
        assert!(!calm.contains("Personality: Playful"));
    }

    #[test]
    fn compact_template_is_included_in_full_prompt() {
        let tmp = tempdir().expect("tempdir");
        let prompt = match system_prompt_for_mode_with_context(AppMode::Agent, tmp.path(), None) {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("expected text system prompt"),
        };
        assert!(prompt.contains("## Compaction Handoff"));
        // #429: structured Markdown template. Goal/Constraints/Progress
        // (Done/InProgress/Blocked)/Key Decisions/Next step.
        assert!(prompt.contains("### Goal"));
        assert!(prompt.contains("### Constraints"));
        assert!(prompt.contains("### Progress"));
        assert!(prompt.contains("#### Done"));
        assert!(prompt.contains("#### In Progress"));
        assert!(prompt.contains("#### Blocked"));
        assert!(prompt.contains("### Key Decisions"));
        assert!(prompt.contains("### Next step"));
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
            },
        ) {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("expected text system prompt"),
        };

        let goal_pos = prompt.find("<session_goal>").expect("goal block");
        let compact_pos = prompt.find("## Compaction Handoff").expect("compact block");

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
            },
        ) {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("expected text system prompt"),
        };

        assert!(!prompt.contains("<session_goal>"));
        assert!(!prompt.contains("## Current Session Goal"));
    }

    #[test]
    fn when_not_to_use_sections_present() {
        let prompt = compose_prompt(AppMode::Agent, Personality::Calm);
        assert!(prompt.contains("When NOT to use certain tools"));
        assert!(prompt.contains("### `apply_patch`"));
        assert!(prompt.contains("### `edit_file`"));
        assert!(prompt.contains("### `exec_shell`"));
        assert!(prompt.contains("### `agent_spawn`"));
        assert!(prompt.contains("### `rlm`"));
    }

    /// #588: language-mirroring directive must ship in every mode so
    /// DeepSeek's `reasoning_content` and final reply follow the user's
    /// language. Structural test — wording is not a test concern, but
    /// the cross-cutting commitment of #588 is specifically that the
    /// `reasoning_content` field tracks the user's language (not just
    /// the visible reply); pin that anchor token so a future edit
    /// can't silently weaken the section to a generic "respond in the
    /// user's language" directive while keeping the heading.
    #[test]
    fn language_mirroring_section_present_in_all_modes() {
        for mode in [AppMode::Agent, AppMode::Yolo, AppMode::Plan] {
            let prompt = compose_prompt(mode, Personality::Calm);
            assert!(
                prompt.contains("## Language"),
                "## Language section missing from mode {mode:?}"
            );
            assert!(
                prompt.contains("reasoning_content"),
                "## Language section in {mode:?} must mention `reasoning_content` — \
                 that field name is the structural anchor for the #588 commitment that \
                 internal reasoning, not just the visible reply, follows the user's language"
            );
        }
    }

    /// #358: rlm guidance was reframed from "first-class" to "specialty
    /// tool" — verify the structural markers are present so a future
    /// change doesn't silently remove the RLM section entirely.
    ///
    /// Don't assert on prose. If you wouldn't fail a code review for
    /// changing the wording, don't fail a test for it.
    #[test]
    fn rlm_specialty_tool_guidance_present() {
        let prompt = compose_prompt(AppMode::Agent, Personality::Calm);
        // Structural: the RLM heading must exist as a section anchor.
        assert!(prompt.contains("RLM — When to Use It"));
        // Structural: the word "rlm" must appear multiple times (tool
        // name, section heading, toolbox reference). Just verify the
        // lowercase form — exact wording is NOT a test concern.
        let rlm_count = prompt.to_lowercase().matches("rlm").count();
        assert!(
            rlm_count >= 5,
            "RLM guidance present: expected >= 5 mentions of 'rlm', got {rlm_count}"
        );
    }

    #[test]
    fn subagent_done_sentinel_section_present() {
        let prompt = compose_prompt(AppMode::Agent, Personality::Calm);
        assert!(prompt.contains("Sub-agent completion sentinel"));
        assert!(prompt.contains("<deepseek:subagent.done>"));
        assert!(prompt.contains("Integration protocol"));
    }

    #[test]
    fn preamble_rhythm_section_present() {
        let prompt = compose_prompt(AppMode::Agent, Personality::Calm);
        assert!(prompt.contains("Preamble Rhythm"));
        assert!(prompt.contains("I'll start by reading the module structure"));
    }

    #[test]
    fn legacy_constants_still_available() {
        // Verify the old .txt constants still compile and contain expected content
        assert!(!AGENT_PROMPT.is_empty());
        assert!(!YOLO_PROMPT.is_empty());
        assert!(!PLAN_PROMPT.is_empty());
    }

    #[test]
    fn localizable_prompt_layers_cover_all_runtime_prompt_files() {
        let expected = [
            "base.md",
            "personalities/calm.md",
            "personalities/playful.md",
            "modes/agent.md",
            "modes/plan.md",
            "modes/yolo.md",
            "approvals/auto.md",
            "approvals/suggest.md",
            "approvals/never.md",
            "compact.md",
            "agent.txt",
            "yolo.txt",
            "plan.txt",
        ];
        let actual = LOCALIZABLE_PROMPT_LAYERS
            .iter()
            .map(|layer| layer.relative_path)
            .collect::<Vec<_>>();

        assert_eq!(actual, expected);

        for layer in LOCALIZABLE_PROMPT_LAYERS {
            let raw = std::fs::read_to_string(bundled_prompt_path(layer.relative_path))
                .expect("bundled prompt file");
            assert_eq!(layer.builtin, raw);
        }
    }

    #[test]
    fn localized_prompt_file_name_uses_i18n_before_extension() {
        assert_eq!(i18n_prompt_file_name("base.md"), "base.i18n.md");
        assert_eq!(
            i18n_prompt_file_name("personalities/calm.md"),
            "calm.i18n.md"
        );
        assert_eq!(i18n_prompt_file_name("agent.txt"), "agent.i18n.txt");
        assert_eq!(i18n_prompt_file_name("README"), "README.i18n");
    }

    #[test]
    fn localized_prompt_from_dir_prefers_local_file_when_present() {
        let tmp = tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("base.i18n.md"), "LOCALIZED BASE\n").unwrap();

        let prompt = localized_prompt_from_dir(tmp.path(), "base.md").expect("localized prompt");

        assert_eq!(prompt, "LOCALIZED BASE");
    }

    #[test]
    fn localized_prompt_from_dir_skips_missing_and_empty_files() {
        let tmp = tempdir().expect("tempdir");
        assert!(localized_prompt_from_dir(tmp.path(), "base.md").is_none());

        std::fs::write(tmp.path().join("base.i18n.md"), "   \n").unwrap();
        assert!(localized_prompt_from_dir(tmp.path(), "base.md").is_none());
    }

    // ── Cache-prefix stability harness (#263 step 2) ───────────────────────
    //
    // These tests pin the byte-stability invariant required for DeepSeek's
    // KV prefix cache to hit: any prompt-construction surface that ends up
    // in the cached prefix must produce identical bytes given identical
    // inputs across calls.

    use crate::test_support::assert_byte_identical;

    #[test]
    fn compose_prompt_is_byte_stable_across_calls() {
        // Suspect #4 from #263: mode prompt churn within a single mode.
        // Two calls with identical (mode, personality) inputs must produce
        // identical bytes — anything else is a cache buster.
        for mode in [AppMode::Agent, AppMode::Yolo, AppMode::Plan] {
            for personality in [Personality::Calm, Personality::Playful] {
                let a = compose_prompt(mode, personality);
                let b = compose_prompt(mode, personality);
                assert_byte_identical(
                    &format!("compose_prompt(mode={mode:?}, personality={personality:?})"),
                    &a,
                    &b,
                );
            }
        }
    }

    #[test]
    fn system_prompt_for_mode_with_context_is_byte_stable_for_unchanged_workspace() {
        // Same workspace, no working_set / skills churn between calls →
        // identical bytes. This pins the most representative production
        // surface (engine.rs builds the system prompt via this fn or
        // its sibling _and_skills variant on every turn).
        let tmp = tempdir().expect("tempdir");
        let workspace = tmp.path();

        for mode in [AppMode::Agent, AppMode::Yolo, AppMode::Plan] {
            let a = match system_prompt_for_mode_with_context(mode, workspace, None) {
                SystemPrompt::Text(text) => text,
                SystemPrompt::Blocks(_) => panic!("expected text system prompt"),
            };
            let b = match system_prompt_for_mode_with_context(mode, workspace, None) {
                SystemPrompt::Text(text) => text,
                SystemPrompt::Blocks(_) => panic!("expected text system prompt"),
            };
            assert_byte_identical(
                &format!("system_prompt_for_mode_with_context(mode={mode:?}) on empty workspace"),
                &a,
                &b,
            );
        }
    }

    #[test]
    fn system_prompt_ignores_working_set_summary_argument() {
        // Working-set metadata is now injected into the latest user message
        // per turn. The legacy argument remains for call-site compatibility
        // but must not reintroduce volatile bytes into the system prompt.
        let tmp = tempdir().expect("tempdir");
        let workspace = tmp.path();
        let summary = "## Repo Working Set\nWorkspace: /tmp/x\n";

        let a = match system_prompt_for_mode_with_context(AppMode::Agent, workspace, Some(summary))
        {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("expected text system prompt"),
        };
        let b = match system_prompt_for_mode_with_context(AppMode::Agent, workspace, Some(summary))
        {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("expected text system prompt"),
        };
        assert_byte_identical(
            "system_prompt_for_mode_with_context with constant working_set summary",
            &a,
            &b,
        );
        assert!(
            !a.contains(summary),
            "summary must not be embedded in system prompt"
        );
    }

    #[test]
    fn system_prompt_with_handoff_file_is_byte_stable_when_file_is_unchanged() {
        // If `.deepseek/handoff.md` hasn't moved between two builds, the
        // rendered prompt must produce identical bytes. The handoff block
        // lands below the static boundary in
        // `system_prompt_for_mode_with_context_and_skills`.
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
            SystemPrompt::Blocks(_) => panic!("expected text system prompt"),
        };
        let b = match system_prompt_for_mode_with_context(AppMode::Agent, workspace, None) {
            SystemPrompt::Text(text) => text,
            SystemPrompt::Blocks(_) => panic!("expected text system prompt"),
        };
        assert_byte_identical(
            "system_prompt_for_mode_with_context with constant handoff file",
            &a,
            &b,
        );
        assert!(a.contains(HANDOFF_BLOCK_MARKER), "handoff must be embedded");
        assert!(a.contains("Finish #280."), "handoff body must be present");
    }

    #[test]
    fn handoff_appears_after_static_blocks_without_working_set() {
        // Cache-prefix invariant: the handoff block must come after static
        // `## Context Management` and the compaction handoff template
        // (`## Compaction Handoff`). Working-set metadata is per-turn user
        // metadata now, not a system-prompt tail block.
        let tmp = tempdir().expect("tempdir");
        let workspace = tmp.path();
        let handoff_dir = workspace.join(".deepseek");
        std::fs::create_dir_all(&handoff_dir).unwrap();
        std::fs::write(handoff_dir.join("handoff.md"), "# handoff body\n").unwrap();

        let summary = "## Repo Working Set\nWorkspace: /tmp/x\n";
        let prompt =
            match system_prompt_for_mode_with_context(AppMode::Agent, workspace, Some(summary)) {
                SystemPrompt::Text(text) => text,
                SystemPrompt::Blocks(_) => panic!("expected text system prompt"),
            };

        let context_pos = prompt
            .find("## Context Management")
            .expect("Context Management section present in Agent mode");
        let compact_pos = prompt
            .find("## Compaction Handoff")
            .expect("compaction handoff template present");
        let handoff_pos = prompt
            .find(HANDOFF_BLOCK_MARKER)
            .expect("handoff block present when fixture file exists");
        assert!(
            !prompt.contains("## Repo Working Set"),
            "working-set summary must stay out of the system prompt"
        );

        assert!(
            context_pos < handoff_pos,
            "## Context Management must precede the handoff block"
        );
        assert!(
            compact_pos < handoff_pos,
            "## Compaction Handoff must precede the handoff block"
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
            .expect("present file should produce a block");
        assert!(block.contains("real content here"));
        assert!(block.contains(&real.display().to_string()));
        // Bogus path is skipped, not rendered.
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
        let alpha_pos = block.find("ALPHA_MARKER").expect("alpha rendered");
        let bravo_pos = block.find("BRAVO_MARKER").expect("bravo rendered");
        assert!(
            alpha_pos < bravo_pos,
            "instructions must concatenate in declared order"
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
        // Empty file produces no `<instructions>` section, only the real one.
        let count = block.matches("<instructions").count();
        assert_eq!(count, 1, "only the non-empty file should produce a section");
    }

    #[test]
    fn render_instructions_block_truncates_oversize_files() {
        let tmp = tempdir().expect("tempdir");
        let big = tmp.path().join("big.md");
        // 200 KiB of content — well above the 100 KiB cap.
        std::fs::write(&big, "X".repeat(200 * 1024)).unwrap();

        let block = super::render_instructions_block(&[big]).expect("non-empty");
        assert!(block.contains("[…elided]"), "truncation marker missing");
        // Block should be much smaller than the original file.
        assert!(
            block.len() < 110 * 1024,
            "block should be capped near 100 KiB"
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
            SystemPrompt::Blocks(_) => panic!("expected text system prompt"),
        };

        assert!(
            prompt.contains("EXTRA_INSTRUCTIONS_MARKER_BODY"),
            "configured instructions file body must appear in the prompt"
        );
        assert!(
            prompt.contains(&extra.display().to_string()),
            "instructions block must annotate its source path"
        );
    }
}
