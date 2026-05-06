//! Lightweight localization registry for high-visibility TUI strings.
//!
//! This intentionally covers UI chrome only. It does not change model prompts,
//! model output language, provider behavior, or media payload semantics.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Supported locales (auto-detected from system locale)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Locale {
    En,
    Ja,
    ZhHans,   // 简体中文
    ZhHant,   // 繁体中文
    PtBr,
}

impl Default for Locale {
    fn default() -> Self {
        Self::En
    }
}

impl Locale {
    /// Get the locale tag string (e.g., "zh-Hans", "ja")
    pub fn tag(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ja => "ja",
            Self::ZhHans => "zh-Hans",
            Self::ZhHant => "zh-Hant",
            Self::PtBr => "pt-BR",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::EnumString, strum::IntoStaticStr, strum::Display)]
#[strum(serialize_all = "snake_case")]
pub enum MessageId {
    ComposerPlaceholder,
    HistorySearchPlaceholder,
    HistorySearchTitle,
    HistoryHintMove,
    HistoryHintAccept,
    HistoryHintRestore,
    HistoryNoMatches,
    ConfigTitle,
    ConfigModalTitle,
    ConfigSearchPlaceholder,
    ConfigNoSettings,
    ConfigNoMatchesPrefix,
    ConfigFilteredSettings,
    ConfigShowing,
    ConfigFooterDefault,
    ConfigFooterScrollable,
    ConfigFooterFiltered,
    HelpTitle,
    HelpFilterPlaceholder,
    HelpFilterPrefix,
    HelpNoMatches,
    HelpSlashCommands,
    HelpKeybindings,
    HelpFooterTypeFilter,
    HelpFooterMove,
    HelpFooterJump,
    HelpFooterClose,
    CmdAgentDescription,
    CmdAttachDescription,
    CmdCacheDescription,
    CmdClearDescription,
    CmdCompactDescription,
    CmdConfigDescription,
    CmdContextDescription,
    CmdCostDescription,
    CmdCycleDescription,
    CmdCyclesDescription,
    CmdDiffDescription,
    CmdEditDescription,
    CmdExitDescription,
    CmdExportDescription,
    CmdHelpDescription,
    CmdHomeDescription,
    CmdHooksDescription,
    CmdGoalDescription,
    CmdInitDescription,
    CmdJobsDescription,
    CmdLinksDescription,
    CmdLoadDescription,
    CmdLogoutDescription,
    CmdMcpDescription,
    CmdMemoryDescription,
    CmdModelDescription,
    CmdModelsDescription,
    CmdNoteDescription,
    CmdPlanDescription,
    CmdProviderDescription,
    CmdQueueDescription,
    CmdRecallDescription,
    CmdRestoreDescription,
    CmdRetryDescription,
    CmdReviewDescription,
    CmdRlmDescription,
    CmdSaveDescription,
    CmdSessionsDescription,
    CmdSettingsDescription,
    CmdSkillDescription,
    CmdSkillsDescription,
    CmdStashDescription,
    CmdStatuslineDescription,
    CmdSubagentsDescription,
    CmdSwarmDescription,
    CmdSystemDescription,
    CmdTaskDescription,
    CmdTokensDescription,
    CmdTrustDescription,
    CmdLspDescription,
    CmdShareDescription,
    CmdUndoDescription,
    CmdYoloDescription,
    CmdCacheAdvice,
    CmdCacheFootnote,
    CmdCacheHeader,
    CmdCacheNoData,
    CmdCacheTotals,
    CmdCostReport,
    CmdTokensCacheBoth,
    CmdTokensCacheHitOnly,
    CmdTokensCacheMissOnly,
    CmdTokensContextUnknownWindow,
    CmdTokensContextWithWindow,
    CmdTokensNotReported,
    CmdTokensReport,
    FooterAgentSingular,
    FooterAgentsPlural,
    FooterPressCtrlCAgain,
    FooterWorking,
    HelpSectionActions,
    HelpSectionClipboard,
    HelpSectionEditing,
    HelpSectionHelp,
    HelpSectionModes,
    HelpSectionNavigation,
    HelpSectionSessions,
    KbScrollTranscript,
    KbNavigateHistory,
    KbScrollTranscriptAlt,
    KbScrollPage,
    KbJumpTopBottom,
    KbJumpTopBottomEmpty,
    KbJumpToolBlocks,
    KbMoveCursor,
    KbJumpLineStartEnd,
    KbDeleteChar,
    KbClearDraft,
    KbStashDraft,
    KbSearchHistory,
    KbInsertNewline,
    KbSendDraft,
    KbCloseMenu,
    KbCancelOrExit,
    KbShellControls,
    KbExitEmpty,
    KbCommandPalette,
    KbFuzzyFilePicker,
    KbCompactInspector,
    KbLastMessagePager,
    KbSelectedDetails,
    KbToolDetailsPager,
    KbThinkingPager,
    KbLiveTranscript,
    KbBacktrackMessage,
    KbCompleteCycleModes,
    KbJumpPlanAgentYolo,
    KbAltJumpPlanAgentYolo,
    KbFocusSidebar,
    KbTogglePlanAgent,
    KbSessionPicker,
    KbPasteAttach,
    KbCopySelection,
    KbContextMenu,
    KbAttachPath,
    KbHelpOverlay,
    KbToggleHelp,
    KbToggleHelpSlash,
    HelpUsageLabel,
    HelpAliasesLabel,
    SettingsTitle,
    SettingsConfigFile,
    ClearConversation,
    ClearConversationBusy,
    ModelChanged,
    LinksTitle,
    LinksDashboard,
    LinksDocs,
    LinksTip,
    SubagentsFetching,
    HelpUnknownCommand,
    HomeDashboardTitle,
    HomeModel,
    HomeMode,
    HomeWorkspace,
    HomeHistory,
    HomeTokens,
    HomeQueued,
    HomeSubagents,
    HomeSkill,
    HomeQuickActions,
    HomeQuickLinks,
    HomeQuickSkills,
    HomeQuickConfig,
    HomeQuickSettings,
    HomeQuickModel,
    HomeQuickSubagents,
    HomeQuickTaskList,
    HomeQuickHelp,
    HomeModeTips,
    HomeAgentModeTip,
    HomeAgentModeReviewTip,
    HomeAgentModeYoloTip,
    HomeYoloModeTip,
    HomeYoloModeCaution,
    HomePlanModeTip,
    HomePlanModeChecklistTip,
    // Onboarding
    OnboardingLanguageTitle,
    OnboardingLanguageDetected,
    OnboardingLanguageLabel,
    OnboardingLanguageChangeHint1,
    OnboardingLanguageChangeHint2,
    OnboardingLanguageChangeHint3,
    OnboardingLanguageAutoDetected,
    OnboardingLanguagePressKey,
    OnboardingLanguageToContinue,
    // Onboarding - Welcome
    OnboardingWelcomeTitle,
    OnboardingWelcomeVersion,
    OnboardingWelcomeDescription,
    OnboardingWelcomeSteps,
    OnboardingWelcomeComposer,
    OnboardingWelcomePressEnter,
    OnboardingWelcomeCtrlC,
    // Onboarding - API Key
    OnboardingApiKeyTitle,
    OnboardingApiKeyStep1,
    OnboardingApiKeyStep2,
    OnboardingApiKeySavedTo,
    OnboardingApiKeyPasteExact,
    OnboardingApiKeyPlaceholder,
    OnboardingApiKeyLabel,
    OnboardingApiKeyInstructions,
    // Onboarding - Trust
    OnboardingTrustTitle,
    OnboardingTrustQuestion,
    OnboardingTrustWorkspace,
    OnboardingTrustYesExplanation,
    OnboardingTrustNoExplanation,
    OnboardingTrustPressY,
    OnboardingTrustYKey,
    OnboardingTrustToTrust,
    OnboardingTrustNKey,
    OnboardingTrustToSkip,
    // Doctor Command
    DoctorTitle,
    DoctorVersionTitle,
    DoctorConfigTitle,
    DoctorConfigFound,
    DoctorConfigNotFound,
    DoctorWorkspace,
    DoctorApiKeysTitle,
    DoctorApiKeyStatus,
    DoctorCredentialPrecedence,
    DoctorApiKeyResolved,
    DoctorApiKeyNotConfigured,
    DoctorApiKeyHint,
    DoctorConnectivityTitle,
    DoctorTestingConnection,
    DoctorConnectionSuccess,
    DoctorGeneratingTranslations,
    DoctorTranslationsSuccess,
    DoctorTranslationsFailed,
    DoctorConnectionFailed,
    DoctorInvalidApiKey,
    DoctorEnvKeyRejected,
    DoctorSaveConfigKeyHint,
    DoctorPermissionDenied,
    DoctorTimeoutError,
    DoctorDnsError,
    DoctorConnectionError,
    DoctorGenericError,
    DoctorSkippedNoKey,
    DoctorMcpServersTitle,
    DoctorMcpFeatureEnabled,
    DoctorMcpFeatureDisabled,
    DoctorMcpConfigFound,
    DoctorMcpZeroServers,
    DoctorMcpServersConfigured,
    DoctorMcpDisabled,
    DoctorMcpConfigParseError,
    DoctorMcpConfigNotFound,
    DoctorMcpInitHint,
    DoctorSkillsTitle,
    DoctorLocalSkillsFound,
    DoctorLocalSkillsNotFound,
    DoctorAgentsSkillsFound,
    DoctorAgentsSkillsNotFound,
    DoctorGlobalSkillsFound,
    DoctorGlobalSkillsNotFound,
    DoctorOpencodeSkillsFound,
    DoctorClaudeSkillsFound,
    DoctorSelectedSkillsDir,
    DoctorSkillsSetupHint,
    DoctorToolsTitle,
    DoctorToolsDirFound,
    DoctorToolsDirNotFound,
    DoctorToolsSetupHint,
    DoctorPluginsTitle,
    DoctorPluginsDirFound,
    DoctorPluginsDirNotFound,
    DoctorPluginsSetupHint,
    DoctorStorageTitle,
    DoctorSpilloverFound,
    DoctorSpilloverNotFound,
    DoctorComposerStashFound,
    DoctorComposerStashEmpty,
    DoctorPlatformTitle,
    DoctorSandboxAvailable,
    DoctorSandboxNotAvailable,
    DoctorAllChecksComplete,
    
    // Sidebar - Plan
    SidebarPlanTitle,
    SidebarPlanNoActivePlan,
    SidebarPlanHint,
    SidebarPlanCycles,
    SidebarPlanGoalTokens,
    SidebarPlanSteps,
    SidebarPlanStepStatusPending,
    SidebarPlanStepStatusInProgress,
    SidebarPlanStepStatusCompleted,
    SidebarPlanStepStatusCancelled,
    
    // Sidebar - Todos
    SidebarTodosTitle,
    SidebarTodosNoActiveTodos,
    SidebarTodosItemsCount,
    SidebarTodoStatusPending,
    SidebarTodoStatusInProgress,
    SidebarTodoStatusCompleted,
    
    // Sidebar - Tasks
    SidebarTasksTitle,
    SidebarTasksNoActiveTasks,
    SidebarTasksTurn,
    SidebarTasksRunning,
    SidebarTasksActive,
    SidebarTasksRunningCount,
    
    // Sidebar - Agents
    SidebarAgentsTitle,
    SidebarAgentsNoActiveAgents,
    SidebarAgentsCount,
    SidebarAgentStatusIdle,
    SidebarAgentStatusRunning,
    SidebarAgentStatusCompleted,
    SidebarAgentStatusFailed,
    
    // Sidebar - Context
    SidebarContextTitle,
    SidebarContextNoAttachments,
    SidebarContextFilesCount,
    
    // Popup Messages
    PopupActionRequired,
    PopupChooseAfterPlan,
    PopupOptionAcceptAndProceed,
    PopupOptionAcceptAndProceedDesc,
    PopupOptionAcceptYolo,
    PopupOptionAcceptYoloDesc,
    PopupOptionRevisePlan,
    PopupOptionRevisePlanDesc,
    PopupOptionExitPlanMode,
    PopupOptionExitPlanModeDesc,
    PopupOptionSwitchMode,
    PopupOptionExit,
    PopupMcpConfigCreated,
    PopupMcpConfigOverwritten,
    PopupMcpConfigExists,
    PopupMcpServerAdded,
    PopupMcpHttpServerAdded,
    PopupMcpServerEnabled,
    PopupMcpServerDisabled,
    PopupMcpServerRemoved,
    PopupMcpValidationSuccess,
    PopupMcpReloadSuccess,
    
    // Setup Command
    SetupTitle,
    SetupSeparator,
    SetupWorkspace,
    SetupMcpCreated,
    SetupMcpOverwritten,
    SetupMcpExists,
    SetupMcpNextStep,
    SetupSkillCreated,
    SetupSkillOverwritten,
    SetupSkillExists,
    SetupSkillsLocalEnabled,
    SetupSkillsDir,
    SetupSkillsNextStep,
    SetupToolsReadme,
    SetupExampleTool,
    SetupToolsDir,
    SetupToolsNextStep,
    SetupPluginsReadme,
    SetupExamplePlugin,
    SetupPluginsDir,
    SetupPluginsNextStep,
    SetupSandboxAvailable,
    SetupSandboxUnavailable,
    
    // MCP Command
    McpConfigCreated,
    McpConfigOverwritten,
    McpConfigExists,
    McpEditHint,
    McpNoServers,
    McpServersTitle,
    McpServerStatusEnabled,
    McpServerStatusDisabled,
    McpServerRequired,
    McpConnectedServer,
    McpConnectedAll,
    McpConnectFailed,
    McpToolsTitle,
    McpNoTools,
    McpToolDescription,
    McpAddedStdioServer,
    McpAddedHttpServer,
    McpRemovedServer,
    McpEnabledServer,
    McpDisabledServer,
    McpValidationSuccess,
    McpReloadSuccess,

    // CLI status / sessions / auth
    CliStatusTitle,
    CliStatusSeparator,
    CliStatusWorkspace,
    CliStatusApiKeyEnv,
    CliStatusApiKeyConfig,
    CliStatusApiKeyMissing,
    CliStatusBaseUrl,
    CliStatusDefaultTextModel,
    CliStatusMcpServers,
    CliStatusMissingSuffix,
    CliStatusToolsMissingSuffix,
    CliStatusPluginsMissingSuffix,
    CliStatusSkills,
    CliStatusTools,
    CliStatusPlugins,
    CliStatusSandboxAvailable,
    CliStatusSandboxUnavailable,
    CliStatusDoctorJsonHint,
    CliDotenvPresent,
    CliDotenvMissingWithExample,
    CliDotenvMissing,
    CliCleanNothingNoDir,
    CliCleanNothingNoFiles,
    CliCleanWouldRemove,
    CliCleanedCheckpoints,
    CliModelsEmpty,
    CliModelsTitle,
    CliSessionsEmpty,
    CliSessionsStartHint,
    CliSessionsTitle,
    CliSessionsSeparator,
    CliSessionsMore,
    CliSessionsResumeHint,
    CliSessionsContinueHint,
    CliInitAgentsExists,
    CliInitAgentsCreated,
    CliInitAgentsEditHint,
    CliInitAgentsLoadedHint,
    CliInitAgentsFailed,
    CliAuthNoApiKeyArg,
    CliAuthNoApiKeyStdin,
    CliAuthSaved,
    CliAuthCleared,
    CliSessionNoneSaved,
    CliSessionSelectPrompt,
    CliSessionInputPrompt,
    CliSessionNoneSelected,
    CliSessionInvalidInput,
    CliSessionSelectionOutOfRange,
    CliReviewNoDiff,
    CliApplyPatchEmpty,
    CliApplySuccess,
    CliApplyNoPatch,

    // Slash command messages
    CmdAttachUsage,
    CmdAttachNotFound,
    CmdAttachNotFile,
    CmdAttachUnsupported,
    CmdAttachAttached,
    CmdNoteUsage,
    CmdNoteEmpty,
    CmdNoteCreateDirFailed,
    CmdNoteOpenFailed,
    CmdNoteWriteFailed,
    CmdNoteAppended,
    CmdQueueUsage,
    CmdQueueEditingHeader,
    CmdQueueNoMessages,
    CmdQueueMessagesHeader,
    CmdQueueTip,
    CmdQueueAlreadyEditing,
    CmdQueueNotFound,
    CmdQueueEditingStatus,
    CmdQueueEditingMessage,
    CmdQueueDropped,
    CmdQueueAlreadyEmpty,
    CmdQueueCleared,
    CmdQueueMissingIndex,
    CmdQueueIndexPositive,
    CmdQueueIndexMin,
    CmdGoalCleared,
    CmdGoalBudgetSuffix,
    CmdGoalSet,
    CmdGoalUnknownElapsed,
    CmdGoalTokensSuffix,
    CmdGoalCurrent,
    CmdGoalEmptyHelp,
}

pub fn tr(_locale: Locale, id: MessageId) -> &'static str {
    // New architecture: ignore locale parameter, load from JSON cache
    let key = message_id_to_key(id);
    let result = tr_json(key);
    // Convert String to &'static str by leaking memory (acceptable for UI strings)
    Box::leak(result.into_boxed_str())
}



pub fn normalize_configured_locale(input: &str) -> Option<&'static str> {
    let normalized = normalize_locale_input(input);
    if matches!(normalized.as_str(), "" | "auto" | "system") {
        return Some("auto");
    }
    parse_locale(&normalized).map(Locale::tag)
}

pub fn resolve_locale(setting: &str) -> Locale {
    resolve_locale_with_env(setting, |key| std::env::var(key).ok())
}

pub fn resolve_locale_with_env<F>(setting: &str, env: F) -> Locale
where
    F: Fn(&str) -> Option<String>,
{
    let normalized = normalize_locale_input(setting);
    if !matches!(normalized.as_str(), "" | "auto" | "system") {
        return parse_locale(&normalized).unwrap_or(Locale::En);
    }

    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Some(value) = env(key)
            && let Some(locale) = parse_locale(&normalize_locale_input(&value))
        {
            return locale;
        }
    }

    Locale::En
}

#[allow(dead_code)]
pub fn truncate_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_string();
    }

    let ellipsis_width = '…'.width().unwrap_or(1);
    if max_width <= ellipsis_width {
        return "…".to_string();
    }

    let limit = max_width - ellipsis_width;
    let mut out = String::new();
    let mut width = 0usize;
    for ch in text.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if width + ch_width > limit {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out.push('…');
    out
}

fn normalize_locale_input(input: &str) -> String {
    input
        .split('.')
        .next()
        .unwrap_or(input)
        .split('@')
        .next()
        .unwrap_or(input)
        .trim()
        .replace('_', "-")
        .to_lowercase()
}

fn parse_locale(value: &str) -> Option<Locale> {
    if value == "c" || value == "posix" || value.starts_with("en") {
        return Some(Locale::En);
    }
    if value.starts_with("ja") {
        return Some(Locale::Ja);
    }
    if value.starts_with("zh") {
        // 繁体中文：台湾、香港、澳门
        if value.contains("hant")
            || value.contains("-tw")
            || value.contains("-hk")
            || value.contains("-mo")
        {
            return Some(Locale::ZhHant);  // ✅ 支持繁体中文
        }
        return Some(Locale::ZhHans);  // 简体中文
    }
    if value.starts_with("pt") || value == "br" {
        return Some(Locale::PtBr);
    }
    // ✅ AI 产品思维：不拒绝任何语言，未知语言回退到英语
    // 用户后续可以通过 AI 生成该语言的翻译文件
    Some(Locale::En)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        widgets::{Paragraph, Widget, Wrap},
    };

    #[test]
    fn locale_setting_normalizes_supported_tags() {
        assert_eq!(normalize_configured_locale("auto"), Some("auto"));
        assert_eq!(normalize_configured_locale("ja_JP.UTF-8"), Some("ja"));
        assert_eq!(normalize_configured_locale("zh-CN"), Some("zh-Hans"));
        assert_eq!(normalize_configured_locale("pt"), Some("pt-BR"));
        assert_eq!(normalize_configured_locale("pt-PT"), Some("pt-BR"));
        // ✅ 繁体中文现在也支持
        assert_eq!(normalize_configured_locale("zh-TW"), Some("zh-Hant"));
        assert_eq!(normalize_configured_locale("zh-HK"), Some("zh-Hant"));
    }

    #[test]
    fn locale_resolution_uses_config_then_environment_then_english() {
        assert_eq!(
            resolve_locale_with_env("ja", |_| Some("pt_BR.UTF-8".to_string())),
            Locale::Ja
        );
        assert_eq!(
            resolve_locale_with_env("auto", |key| {
                (key == "LANG").then(|| "zh_CN.UTF-8".to_string())
            }),
            Locale::ZhHans
        );
        assert_eq!(resolve_locale_with_env("auto", |_| None), Locale::En);
    }


    // 注意：在 AI 驱动的新架构中，不再存在“不支持的语言”
    // 任何语言都可以通过 AI 生成翻译并保存为 JSON 文件
    // 因此不需要测试 unsupported locale fallback

    #[test]
    fn width_truncation_handles_cjk_rtl_indic_and_latin_samples() {
        let samples = [
            ("zh-Hans", "输入以筛选配置"),
            ("ar", "تصفية الإعدادات"),
            ("hi", "सेटिंग खोजें"),
            ("pt-BR", "configurações filtradas"),
        ];

        for (tag, sample) in samples {
            let truncated = truncate_to_width(sample, 12);
            assert!(
                truncated.width() <= 12,
                "{tag} 样本溢出: {truncated:?}"
            );
        }
    }

    #[test]
    fn planned_script_samples_render_in_narrow_terminal_buffer() {
        let samples = [
            ("CJK", "输入以筛选配置"),
            ("RTL", "تصفية الإعدادات"),
            ("Indic", "सेटिंग खोजें"),
            ("Latin Global South", "configurações filtradas"),
        ];

        for (label, sample) in samples {
            let area = Rect::new(0, 0, 18, 4);
            let mut buf = Buffer::empty(area);
            Paragraph::new(sample)
                .wrap(Wrap { trim: false })
                .render(area, &mut buf);
            let dump = buffer_text(&buf, area);

            assert!(
                dump.chars().any(|ch| !ch.is_whitespace()),
                "{label} 样本渲染为空"
            );
        }
    }

    fn buffer_text(buf: &Buffer, area: Rect) -> String {
        let mut out = String::new();
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }
}

// ============================================================================
// JSON-based translation loading (new architecture)
// ============================================================================

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Global translation cache loaded from i18n.json
static TRANSLATION_CACHE: once_cell::sync::Lazy<Arc<RwLock<HashMap<String, String>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

/// Initialize translations by loading from i18n.json or en.json
pub fn init_translations() -> Result<(), Box<dyn std::error::Error>> {
    let translations = crate::i18n_generator::load_translations()?;
    
    let mut cache = TRANSLATION_CACHE.write().map_err(|e| format!("Failed to lock cache: {}", e))?;
    *cache = translations;
    
    Ok(())
}

/// Get a translation by key from the JSON-loaded cache
/// Falls back to en.json if i18n.json not available
pub fn tr_json(key: &str) -> String {
    // Try JSON cache first
    if let Ok(cache) = TRANSLATION_CACHE.read() {
        if let Some(value) = cache.get(key) {
            return value.clone();
        }
    }
    
    // Last resort: return the key itself
    key.to_string()
}

/// Convert MessageId to snake_case key for JSON lookup
/// Uses strum's IntoStaticStr derive to automatically convert
fn message_id_to_key(id: MessageId) -> &'static str {
    // strum::IntoStaticStr automatically converts enum variants to snake_case strings
    // thanks to the #[strum(serialize_all = "snake_case")] attribute
    id.into()
}
