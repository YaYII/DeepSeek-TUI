//! Dynamic localization registry for all user-facing TUI strings.
//!
//! Strings are loaded from JSON files at compile time (embedded `en.json`)
//! and at runtime from `~/.deepseek/i18n/{locale}.json`. The fallback chain
//! is: active locale JSON → embedded `en.json` → key name (emergency).
//!
//! This intentionally covers UI chrome only. It does not change model prompts,
//! model output language, provider behavior, or media payload semantics.

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDirection {
    Ltr,
    Rtl,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocaleCoverage {
    English,
    V076Core,
    PlannedQa,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocaleSpec {
    pub tag: &'static str,
    pub display_name: &'static str,
    pub script: &'static str,
    pub direction: TextDirection,
    pub fallback: &'static str,
    pub coverage: LocaleCoverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Locale {
    En,
    Ja,
    ZhHans,
    PtBr,
}

impl Locale {
    pub fn tag(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ja => "ja",
            Self::ZhHans => "zh-Hans",
            Self::PtBr => "pt-BR",
        }
    }

    #[allow(dead_code)]
    pub fn spec(self) -> LocaleSpec {
        match self {
            Self::En => LocaleSpec {
                tag: "en",
                display_name: "English",
                script: "Latin",
                direction: TextDirection::Ltr,
                fallback: "en",
                coverage: LocaleCoverage::English,
            },
            Self::Ja => LocaleSpec {
                tag: "ja",
                display_name: "Japanese",
                script: "Jpan",
                direction: TextDirection::Ltr,
                fallback: "en",
                coverage: LocaleCoverage::V076Core,
            },
            Self::ZhHans => LocaleSpec {
                tag: "zh-Hans",
                display_name: "Chinese Simplified",
                script: "Hans",
                direction: TextDirection::Ltr,
                fallback: "en",
                coverage: LocaleCoverage::V076Core,
            },
            Self::PtBr => LocaleSpec {
                tag: "pt-BR",
                display_name: "Portuguese (Brazil)",
                script: "Latin",
                direction: TextDirection::Ltr,
                fallback: "en",
                coverage: LocaleCoverage::V076Core,
            },
        }
    }

    #[allow(dead_code)]
    pub fn shipped() -> &'static [Self] {
        &[Self::En, Self::Ja, Self::ZhHans, Self::PtBr]
    }
}

#[allow(dead_code)]
pub const PLANNED_QA_LOCALES: &[LocaleSpec] = &[
    LocaleSpec {
        tag: "ar",
        display_name: "Arabic",
        script: "Arab",
        direction: TextDirection::Rtl,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "hi",
        display_name: "Hindi",
        script: "Deva",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "bn",
        display_name: "Bengali",
        script: "Beng",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "id",
        display_name: "Indonesian",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "vi",
        display_name: "Vietnamese",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "sw",
        display_name: "Swahili",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "ha",
        display_name: "Hausa",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "yo",
        display_name: "Yoruba",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "es-419",
        display_name: "Spanish (Latin America)",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "fr",
        display_name: "French",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "fil",
        display_name: "Filipino/Tagalog",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    // ── Onboarding ──
    OnboardingWelcomeTitle,
    OnboardingWelcomeDesc,
    OnboardingWelcomeStep1,
    OnboardingWelcomeStep2,
    OnboardingWelcomePromptEnter,
    OnboardingWelcomePromptExit,
    OnboardingLanguageTitle,
    OnboardingLanguageDesc,
    OnboardingLanguageFooter,
    OnboardingApiKeyTitle,
    OnboardingApiKeyStep1,
    OnboardingApiKeyStep2,
    OnboardingApiKeyPathNote,
    OnboardingApiKeyPasteNote,
    OnboardingApiKeyPlaceholder,
    OnboardingApiKeyLabel,
    OnboardingApiKeyFooter,
    OnboardingTrustTitle,
    OnboardingTrustPrompt,
    OnboardingTrustWorkspaceLabel,
    OnboardingTrustYExplain,
    OnboardingTrustNExplain,
    OnboardingTrustFooter,
    OnboardingTipsTitle,
    OnboardingTipsTip1,
    OnboardingTipsTip2,
    OnboardingTipsTip3,
    OnboardingTipsTip4,
    OnboardingTipsFooter,
    OnboardingPanelTitle,
    OnboardingStepIndicator,
    // ── Footer states ──
    FooterStateReady,
    FooterStateDraft,
    FooterStateOverlay,
    FooterStateCompacting,
    // ── Approval modal ──
    ApprovalTitleBenign,
    ApprovalTitleDestructive,
    ApprovalImpactReadonly,
    ApprovalImpactWrite,
    ApprovalImpactShell,
    ApprovalImpactNetwork,
    ApprovalImpactMcpRead,
    ApprovalImpactMcpAction,
    ApprovalImpactUnknown,
    ApprovalOptionOnce,
    ApprovalOptionAlways,
    ApprovalOptionDeny,
    ApprovalOptionAbort,
    ApprovalPressApprove,
    ApprovalPressDeny,
    ApprovalStagedHint,
    // ── Routing/tool status ──
    ToolReading,
    ToolListing,
    ToolSearching,
    ToolInteracting,
    ShellJobStatusRunning,
    ShellJobStatusComplete,
    ShellJobStatusFailed,
    // ── Session picker ──
    SessionPickerTitle,
    SessionPickerPreviewTitle,
    SessionSortRecent,
    SessionSortName,
    SessionSortSize,
    // ── Model picker ──
    ModelPickerFlagship,
    ModelPickerFast,
    ProviderConfigured,
    ProviderNeedsKey,
    // ── Pager/views ──
    PagerHelpSearch,
    PagerHelpNavigate,
    LiveTranscriptTailing,
    LiveTranscriptPaused,
    // ── Context inspector ──
    ContextInspectorSessionContext,
    ContextInspectorModel,
    ContextInspectorWorkspace,
    // ── Plan prompt ──
    PlanPromptTitle,
    PlanPromptAcceptAgent,
    PlanPromptAcceptYolo,
    PlanPromptRevise,
    PlanPromptExit,
    // ── Status picker ──
    StatusPickerTitle,
    StatusPickerFooterToggle,
    StatusPickerFooterAll,
    StatusPickerFooterNone,
    // ── Misc UI ──
    SlashMenuStatus,
    FileTreeBuilding,
    FileTreeEmpty,
    SidebarEmptyHint,
    SidebarTitlePlan,
    SidebarTitleTodos,
    SidebarTitleTasks,
    SidebarTitleAgents,
    // ── Task status ──
    TaskStatusQueued,
    TaskStatusRunning,
    TaskStatusCompleted,
    TaskStatusFailed,
    // ── Turn status ──
    TurnStatusCompleted,
    TurnStatusInterrupted,
    TurnStatusFailed,
    // ── Sidebar panel status strings ──
    SidebarTitleSession,
    SidebarNoTodos,
    SidebarNoTasks,
    SidebarNoAgents,
    SidebarPlanPanelHint,
    SidebarPlanUpdating,
    SidebarTodoUpdating,
    SidebarTaskRunning,
    SidebarTasksActive,
    SidebarNMoreSteps,
    SidebarNMoreTodos,
    SidebarAgentRunning,
    SidebarAgentDone,
    SidebarAgentDetailHint,
    SidebarLspOn,
    SidebarLspOff,
    // ── Plan prompt modal ──
    PlanPromptActionRequired,
    PlanPromptChooseAction,
    PlanPromptAcceptAgentDesc,
    PlanPromptAcceptYoloDesc,
    PlanPromptReviseDesc,
    PlanPromptExitDesc,
    PlanPromptQuickPick,
    PlanPromptMove,
    PlanPromptConfirm,
    PlanPromptClose,
    SubagentStarting,
    SubagentCompleted,
    // ── Detail pager titles ──
    DetailTitleYou,
    DetailTitleAssistant,
    DetailTitleNote,
    DetailTitleError,
    DetailTitleReasoning,
    DetailTitleMessage,
    DetailTitleSubAgent,
    DetailTitleArchivedContext,
    // ── Context menu ──
    ContextMenuCopySelection,
    ContextMenuCopySelectionDesc,
    ContextMenuOpenSelection,
    ContextMenuOpenSelectionDesc,
    ContextMenuClearSelection,
    ContextMenuOpenDetails,
    ContextMenuCopyMessage,
    ContextMenuCopyMessageDesc,
    ContextMenuOpenInEditor,
    ContextMenuOpenInEditorDesc,
    ContextMenuShowCell,
    ContextMenuShowCellDesc,
    ContextMenuHideCell,
    ContextMenuHideCellDesc,
    ContextMenuShowHidden,
    ContextMenuShowHiddenDesc,
    ContextMenuPaste,
    ContextMenuPasteDesc,
    ContextMenuCommandPalette,
    ContextMenuCommandPaletteDesc,
    ContextMenuContextInspector,
    ContextMenuContextInspectorDesc,
    ContextMenuHelp,
    ContextMenuHelpDesc,
    // ── Config editor ──
    ConfigEditorEditTitle,
    ConfigEditorScope,
    ConfigEditorCurrent,
    ConfigEditorHint,
    ConfigEditorNew,
    ConfigEditorFooter,
    // ── Live transcript ──
    LiveTranscriptFooter,
    // ── Status toast / context menu action messages ──
    StatusSelectionCopied,
    StatusSelectionCleared,
    StatusCellHidden,
    StatusCellShown,
    StatusNoSelection,
    StatusNoDetails,
    StatusMessageCopied,
    StatusMessageEmpty,
    StatusCopyFailed,
    StatusOpenedFileInEditor,
    StatusNoFileLinePattern,
    StatusShowHidden,
    StatusNoMessageAtLine,
    StatusNoSelectionToCopy,
    // ── Shell control messages ──
    ShellControlNoForeground,
    ShellControlOpened,
    ShellControlNoBackground,
    ShellControlNotAttached,
    ShellControlBackgrounding,
    ShellControlLockPoisoned,
    // ── Footer chip / state labels ──
    FooterChipWorking,
    FooterChipAgents,
    FooterChipTools,
    FooterChipActive,
    FooterChipDone,
    FooterChipShell,
    // ── Sidebar focus messages ──
    SidebarFocusPlan,
    SidebarFocusTodos,
    SidebarFocusTasks,
    SidebarFocusAgents,
    SidebarFocusContext,
    SidebarFocusAuto,
    // ── Status messages ──
    StatusRequestCancelled,
    StatusBacktrackCancelled,
    StatusBacktrackPressEsc,
    StatusComposerFocused,
    StatusAttachmentSelected,
    StatusRemovedAttachment,
    StatusAttachedImage,
    StatusHistorySearchStart,
    StatusHistorySearchActive,
    StatusHistoryMatchInserted,
    StatusHistoryNoMatches,
    StatusHistoryCancelled,
    StatusModeSwitched,
    StatusThinking,
    StatusApiKeySaved,
    StatusDraftStashed,
    StatusCopiedToClipboard,
    StatusSessionSaved,
    StatusSessionLoaded,
    StatusSessionDeleted,
    StatusNoSessions,
    StatusRefreshSubAgents,
    StatusSteeringTurn,
    StatusQueuedAccepted,
    StatusRevisePlan,
    StatusPlanPromptClosed,
    StatusEditedInEditor,
    StatusEditorClosed,
    StatusEditorCancelled,
    StatusEditorError,
    StatusNoPreviousTool,
    StatusNoNextTool,
    StatusNoCellToCopy,
    StatusFileTreeClosed,
    StatusFileTreeHint,
    StatusAttachedPath,
    StatusLargePasteConsolidated,
    StatusPasteFailed,
    // ── Approval widget labels ──
    RiskBadgeReview,
    RiskBadgeDestructive,
    CategoryLabelSafe,
    CategoryLabelFileWrite,
    CategoryLabelShellCommand,
    CategoryLabelNetwork,
    CategoryLabelMcpRead,
    CategoryLabelMcpAction,
    CategoryLabelUnknown,
    ApprovalTypeLabel,
    ApprovalAboutLabel,
    ApprovalImpactLabel,
    ApprovalParamsLabel,
    ApprovalStagedBadge,
    ApprovalSingleKeyHint,
    ApprovalConfirmDestructive,
    ApprovalConfirmDestructiveSuffix,
    ApprovalTwoKeyHint,
    ApprovalKeyHintEnterY,
    ApprovalKeyHintEnterA,
    ApprovalKeyHintEnter,
    ApprovalKeyHintTwoKeys,
    ApprovalFooterHint,
    ApprovalCardTitle,
    // ── Elevation UI ──
    ElevationTitle,
    ElevationToolLabel,
    ElevationCmdLabel,
    ElevationReasonLabel,
    ElevationImpactSection,
    ElevationImpactNetwork,
    ElevationImpactWrite,
    ElevationImpactFullAccess,
    ElevationProceedSection,
    ElevationShortcutKeyN,
    ElevationShortcutKeyW,
    ElevationShortcutKeyF,
    ElevationShortcutKeyA,
    ElevationModalTitle,
    // ── Elevation options ──
    ElevationOptionNetwork,
    ElevationOptionWrite,
    ElevationOptionFullAccess,
    ElevationOptionAbort,
    ElevationOptionNetworkDesc,
    ElevationOptionWriteDesc,
    ElevationOptionFullAccessDesc,
    ElevationOptionAbortDesc,
    // ── Sub-agent status ──
    SubAgentStatusRunning,
    SubAgentStatusCompleted,
    SubAgentStatusFailed,
    SubAgentStatusCancelled,
    SubAgentStatusInterrupted,
    SubAgentsTitle,
    SubAgentEscToClose,
    SubAgentRToRefresh,
    // ── Status picker ──
    StatusPickerInstruction,
    // ── Pending input preview ──
    PendingInputSectionTitle,
    PendingInputContextSection,
    // ── Shell job routing ──
    ShellJobStatusStale,
    ShellJobStatusKilled,
    ShellJobStatusTimeout,
    ShellJobNoLiveJobs,
    ShellJobNoNewOutput,
    ShellJobStdout,
    ShellJobStderr,
    ShellJobEmpty,
    // ── Sub-agent routing ──
    TaskStatusCanceled,
    TaskNoTasksFound,
    // ── Command usage strings ──
    UsageMcp,
    UsageMcpList,
    UsageMcpShow,
    UsageMcpEnable,
    UsageMcpDisable,
    UsageMcpRefresh,
    UsageMcpRemove,
    UsageMcpAdd,
    UsageJobs,
    UsageJobsList,
    UsageJobsShow,
    UsageJobsPoll,
    UsageJobsWait,
    UsageJobsStdin,
    UsageJobsCancel,
    UsageJobsCloseStdin,
    UsageTask,
    UsageTaskList,
    UsageTaskShow,
    UsageTaskCancel,
    UsageQueue,
    UsageCycle,
    UsageRestore,
    UsageNote,
    UsageRlm,
    UsageProfile,
    UsageRecall,
    UsageAttach,
    UsageSkill,
    UsageStatusline,
    // ── Command error/success messages ──
    CmdFailedGeneric,
    CmdSessionSaved,
    CmdSessionLoaded,
    CmdQueueCleared,
    CmdNoQueuedMessages,
    CmdNoSnapshots,
    CmdStashEmpty,
    CmdLspEnabled,
    CmdLspDisabled,
    CmdLoggedOut,
    CmdMcpNoServers,
    CmdJobsNone,
    CmdTaskNone,
    CmdRestoreDone,
    CmdNoteAppended,
    CmdProfileSwitched,
    CmdAttachedFile,
    CmdAttachedDir,
    CmdDetached,
    CmdInvalidArgs,
    CmdUnknownCommand,
    CmdExecutionError,
}

impl MessageId {
    /// Convert a MessageId variant to its snake_case JSON key.
    ///
    /// This is the single source of truth for key generation. Every variant
    /// must have an explicit arm so that missing mappings are caught by the
    /// compiler rather than silently producing wrong keys at runtime.
    #[must_use]
    pub fn to_key(self) -> &'static str {
        match self {
            MessageId::ComposerPlaceholder => "composer_placeholder",
            MessageId::HistorySearchPlaceholder => "history_search_placeholder",
            MessageId::HistorySearchTitle => "history_search_title",
            MessageId::HistoryHintMove => "history_hint_move",
            MessageId::HistoryHintAccept => "history_hint_accept",
            MessageId::HistoryHintRestore => "history_hint_restore",
            MessageId::HistoryNoMatches => "history_no_matches",
            MessageId::ConfigTitle => "config_title",
            MessageId::ConfigModalTitle => "config_modal_title",
            MessageId::ConfigSearchPlaceholder => "config_search_placeholder",
            MessageId::ConfigNoSettings => "config_no_settings",
            MessageId::ConfigNoMatchesPrefix => "config_no_matches_prefix",
            MessageId::ConfigFilteredSettings => "config_filtered_settings",
            MessageId::ConfigShowing => "config_showing",
            MessageId::ConfigFooterDefault => "config_footer_default",
            MessageId::ConfigFooterScrollable => "config_footer_scrollable",
            MessageId::ConfigFooterFiltered => "config_footer_filtered",
            MessageId::HelpTitle => "help_title",
            MessageId::HelpFilterPlaceholder => "help_filter_placeholder",
            MessageId::HelpFilterPrefix => "help_filter_prefix",
            MessageId::HelpNoMatches => "help_no_matches",
            MessageId::HelpSlashCommands => "help_slash_commands",
            MessageId::HelpKeybindings => "help_keybindings",
            MessageId::HelpFooterTypeFilter => "help_footer_type_filter",
            MessageId::HelpFooterMove => "help_footer_move",
            MessageId::HelpFooterJump => "help_footer_jump",
            MessageId::HelpFooterClose => "help_footer_close",
            MessageId::CmdAgentDescription => "cmd_agent_description",
            MessageId::CmdAttachDescription => "cmd_attach_description",
            MessageId::CmdCacheDescription => "cmd_cache_description",
            MessageId::CmdClearDescription => "cmd_clear_description",
            MessageId::CmdCompactDescription => "cmd_compact_description",
            MessageId::CmdConfigDescription => "cmd_config_description",
            MessageId::CmdContextDescription => "cmd_context_description",
            MessageId::CmdCostDescription => "cmd_cost_description",
            MessageId::CmdCycleDescription => "cmd_cycle_description",
            MessageId::CmdCyclesDescription => "cmd_cycles_description",
            MessageId::CmdDiffDescription => "cmd_diff_description",
            MessageId::CmdEditDescription => "cmd_edit_description",
            MessageId::CmdExitDescription => "cmd_exit_description",
            MessageId::CmdExportDescription => "cmd_export_description",
            MessageId::CmdHelpDescription => "cmd_help_description",
            MessageId::CmdHomeDescription => "cmd_home_description",
            MessageId::CmdHooksDescription => "cmd_hooks_description",
            MessageId::CmdGoalDescription => "cmd_goal_description",
            MessageId::CmdInitDescription => "cmd_init_description",
            MessageId::CmdJobsDescription => "cmd_jobs_description",
            MessageId::CmdLinksDescription => "cmd_links_description",
            MessageId::CmdLoadDescription => "cmd_load_description",
            MessageId::CmdLogoutDescription => "cmd_logout_description",
            MessageId::CmdMcpDescription => "cmd_mcp_description",
            MessageId::CmdMemoryDescription => "cmd_memory_description",
            MessageId::CmdModelDescription => "cmd_model_description",
            MessageId::CmdModelsDescription => "cmd_models_description",
            MessageId::CmdNoteDescription => "cmd_note_description",
            MessageId::CmdPlanDescription => "cmd_plan_description",
            MessageId::CmdProviderDescription => "cmd_provider_description",
            MessageId::CmdQueueDescription => "cmd_queue_description",
            MessageId::CmdRecallDescription => "cmd_recall_description",
            MessageId::CmdRestoreDescription => "cmd_restore_description",
            MessageId::CmdRetryDescription => "cmd_retry_description",
            MessageId::CmdReviewDescription => "cmd_review_description",
            MessageId::CmdRlmDescription => "cmd_rlm_description",
            MessageId::CmdSaveDescription => "cmd_save_description",
            MessageId::CmdSessionsDescription => "cmd_sessions_description",
            MessageId::CmdSettingsDescription => "cmd_settings_description",
            MessageId::CmdSkillDescription => "cmd_skill_description",
            MessageId::CmdSkillsDescription => "cmd_skills_description",
            MessageId::CmdStashDescription => "cmd_stash_description",
            MessageId::CmdStatuslineDescription => "cmd_statusline_description",
            MessageId::CmdSubagentsDescription => "cmd_subagents_description",
            MessageId::CmdSwarmDescription => "cmd_swarm_description",
            MessageId::CmdSystemDescription => "cmd_system_description",
            MessageId::CmdTaskDescription => "cmd_task_description",
            MessageId::CmdTokensDescription => "cmd_tokens_description",
            MessageId::CmdTrustDescription => "cmd_trust_description",
            MessageId::CmdLspDescription => "cmd_lsp_description",
            MessageId::CmdShareDescription => "cmd_share_description",
            MessageId::CmdUndoDescription => "cmd_undo_description",
            MessageId::CmdYoloDescription => "cmd_yolo_description",
            MessageId::CmdCacheAdvice => "cmd_cache_advice",
            MessageId::CmdCacheFootnote => "cmd_cache_footnote",
            MessageId::CmdCacheHeader => "cmd_cache_header",
            MessageId::CmdCacheNoData => "cmd_cache_no_data",
            MessageId::CmdCacheTotals => "cmd_cache_totals",
            MessageId::CmdCostReport => "cmd_cost_report",
            MessageId::CmdTokensCacheBoth => "cmd_tokens_cache_both",
            MessageId::CmdTokensCacheHitOnly => "cmd_tokens_cache_hit_only",
            MessageId::CmdTokensCacheMissOnly => "cmd_tokens_cache_miss_only",
            MessageId::CmdTokensContextUnknownWindow => "cmd_tokens_context_unknown_window",
            MessageId::CmdTokensContextWithWindow => "cmd_tokens_context_with_window",
            MessageId::CmdTokensNotReported => "cmd_tokens_not_reported",
            MessageId::CmdTokensReport => "cmd_tokens_report",
            MessageId::FooterAgentSingular => "footer_agent_singular",
            MessageId::FooterAgentsPlural => "footer_agents_plural",
            MessageId::FooterPressCtrlCAgain => "footer_press_ctrl_c_again",
            MessageId::FooterWorking => "footer_working",
            MessageId::HelpSectionActions => "help_section_actions",
            MessageId::HelpSectionClipboard => "help_section_clipboard",
            MessageId::HelpSectionEditing => "help_section_editing",
            MessageId::HelpSectionHelp => "help_section_help",
            MessageId::HelpSectionModes => "help_section_modes",
            MessageId::HelpSectionNavigation => "help_section_navigation",
            MessageId::HelpSectionSessions => "help_section_sessions",
            MessageId::KbScrollTranscript => "kb_scroll_transcript",
            MessageId::KbNavigateHistory => "kb_navigate_history",
            MessageId::KbScrollTranscriptAlt => "kb_scroll_transcript_alt",
            MessageId::KbScrollPage => "kb_scroll_page",
            MessageId::KbJumpTopBottom => "kb_jump_top_bottom",
            MessageId::KbJumpTopBottomEmpty => "kb_jump_top_bottom_empty",
            MessageId::KbJumpToolBlocks => "kb_jump_tool_blocks",
            MessageId::KbMoveCursor => "kb_move_cursor",
            MessageId::KbJumpLineStartEnd => "kb_jump_line_start_end",
            MessageId::KbDeleteChar => "kb_delete_char",
            MessageId::KbClearDraft => "kb_clear_draft",
            MessageId::KbStashDraft => "kb_stash_draft",
            MessageId::KbSearchHistory => "kb_search_history",
            MessageId::KbInsertNewline => "kb_insert_newline",
            MessageId::KbSendDraft => "kb_send_draft",
            MessageId::KbCloseMenu => "kb_close_menu",
            MessageId::KbCancelOrExit => "kb_cancel_or_exit",
            MessageId::KbShellControls => "kb_shell_controls",
            MessageId::KbExitEmpty => "kb_exit_empty",
            MessageId::KbCommandPalette => "kb_command_palette",
            MessageId::KbFuzzyFilePicker => "kb_fuzzy_file_picker",
            MessageId::KbCompactInspector => "kb_compact_inspector",
            MessageId::KbLastMessagePager => "kb_last_message_pager",
            MessageId::KbSelectedDetails => "kb_selected_details",
            MessageId::KbToolDetailsPager => "kb_tool_details_pager",
            MessageId::KbThinkingPager => "kb_thinking_pager",
            MessageId::KbLiveTranscript => "kb_live_transcript",
            MessageId::KbBacktrackMessage => "kb_backtrack_message",
            MessageId::KbCompleteCycleModes => "kb_complete_cycle_modes",
            MessageId::KbJumpPlanAgentYolo => "kb_jump_plan_agent_yolo",
            MessageId::KbAltJumpPlanAgentYolo => "kb_alt_jump_plan_agent_yolo",
            MessageId::KbFocusSidebar => "kb_focus_sidebar",
            MessageId::KbTogglePlanAgent => "kb_toggle_plan_agent",
            MessageId::KbSessionPicker => "kb_session_picker",
            MessageId::KbPasteAttach => "kb_paste_attach",
            MessageId::KbCopySelection => "kb_copy_selection",
            MessageId::KbContextMenu => "kb_context_menu",
            MessageId::KbAttachPath => "kb_attach_path",
            MessageId::KbHelpOverlay => "kb_help_overlay",
            MessageId::KbToggleHelp => "kb_toggle_help",
            MessageId::KbToggleHelpSlash => "kb_toggle_help_slash",
            MessageId::HelpUsageLabel => "help_usage_label",
            MessageId::HelpAliasesLabel => "help_aliases_label",
            MessageId::SettingsTitle => "settings_title",
            MessageId::SettingsConfigFile => "settings_config_file",
            MessageId::ClearConversation => "clear_conversation",
            MessageId::ClearConversationBusy => "clear_conversation_busy",
            MessageId::ModelChanged => "model_changed",
            MessageId::LinksTitle => "links_title",
            MessageId::LinksDashboard => "links_dashboard",
            MessageId::LinksDocs => "links_docs",
            MessageId::LinksTip => "links_tip",
            MessageId::SubagentsFetching => "subagents_fetching",
            MessageId::HelpUnknownCommand => "help_unknown_command",
            MessageId::HomeDashboardTitle => "home_dashboard_title",
            MessageId::HomeModel => "home_model",
            MessageId::HomeMode => "home_mode",
            MessageId::HomeWorkspace => "home_workspace",
            MessageId::HomeHistory => "home_history",
            MessageId::HomeTokens => "home_tokens",
            MessageId::HomeQueued => "home_queued",
            MessageId::HomeSubagents => "home_subagents",
            MessageId::HomeSkill => "home_skill",
            MessageId::HomeQuickActions => "home_quick_actions",
            MessageId::HomeQuickLinks => "home_quick_links",
            MessageId::HomeQuickSkills => "home_quick_skills",
            MessageId::HomeQuickConfig => "home_quick_config",
            MessageId::HomeQuickSettings => "home_quick_settings",
            MessageId::HomeQuickModel => "home_quick_model",
            MessageId::HomeQuickSubagents => "home_quick_subagents",
            MessageId::HomeQuickTaskList => "home_quick_task_list",
            MessageId::HomeQuickHelp => "home_quick_help",
            MessageId::HomeModeTips => "home_mode_tips",
            MessageId::HomeAgentModeTip => "home_agent_mode_tip",
            MessageId::HomeAgentModeReviewTip => "home_agent_mode_review_tip",
            MessageId::HomeAgentModeYoloTip => "home_agent_mode_yolo_tip",
            MessageId::HomeYoloModeTip => "home_yolo_mode_tip",
            MessageId::HomeYoloModeCaution => "home_yolo_mode_caution",
            MessageId::HomePlanModeTip => "home_plan_mode_tip",
            MessageId::HomePlanModeChecklistTip => "home_plan_mode_checklist_tip",
            MessageId::OnboardingWelcomeTitle => "onboarding_welcome_title",
            MessageId::OnboardingWelcomeDesc => "onboarding_welcome_desc",
            MessageId::OnboardingWelcomeStep1 => "onboarding_welcome_step1",
            MessageId::OnboardingWelcomeStep2 => "onboarding_welcome_step2",
            MessageId::OnboardingWelcomePromptEnter => "onboarding_welcome_prompt_enter",
            MessageId::OnboardingWelcomePromptExit => "onboarding_welcome_prompt_exit",
            MessageId::OnboardingLanguageTitle => "onboarding_language_title",
            MessageId::OnboardingLanguageDesc => "onboarding_language_desc",
            MessageId::OnboardingLanguageFooter => "onboarding_language_footer",
            MessageId::OnboardingApiKeyTitle => "onboarding_api_key_title",
            MessageId::OnboardingApiKeyStep1 => "onboarding_api_key_step1",
            MessageId::OnboardingApiKeyStep2 => "onboarding_api_key_step2",
            MessageId::OnboardingApiKeyPathNote => "onboarding_api_key_path_note",
            MessageId::OnboardingApiKeyPasteNote => "onboarding_api_key_paste_note",
            MessageId::OnboardingApiKeyPlaceholder => "onboarding_api_key_placeholder",
            MessageId::OnboardingApiKeyLabel => "onboarding_api_key_label",
            MessageId::OnboardingApiKeyFooter => "onboarding_api_key_footer",
            MessageId::OnboardingTrustTitle => "onboarding_trust_title",
            MessageId::OnboardingTrustPrompt => "onboarding_trust_prompt",
            MessageId::OnboardingTrustWorkspaceLabel => "onboarding_trust_workspace_label",
            MessageId::OnboardingTrustYExplain => "onboarding_trust_y_explain",
            MessageId::OnboardingTrustNExplain => "onboarding_trust_n_explain",
            MessageId::OnboardingTrustFooter => "onboarding_trust_footer",
            MessageId::OnboardingTipsTitle => "onboarding_tips_title",
            MessageId::OnboardingTipsTip1 => "onboarding_tips_tip1",
            MessageId::OnboardingTipsTip2 => "onboarding_tips_tip2",
            MessageId::OnboardingTipsTip3 => "onboarding_tips_tip3",
            MessageId::OnboardingTipsTip4 => "onboarding_tips_tip4",
            MessageId::OnboardingTipsFooter => "onboarding_tips_footer",
            MessageId::OnboardingPanelTitle => "onboarding_panel_title",
            MessageId::OnboardingStepIndicator => "onboarding_step_indicator",
            MessageId::FooterStateReady => "footer_state_ready",
            MessageId::FooterStateDraft => "footer_state_draft",
            MessageId::FooterStateOverlay => "footer_state_overlay",
            MessageId::FooterStateCompacting => "footer_state_compacting",
            MessageId::ApprovalTitleBenign => "approval_title_benign",
            MessageId::ApprovalTitleDestructive => "approval_title_destructive",
            MessageId::ApprovalImpactReadonly => "approval_impact_readonly",
            MessageId::ApprovalImpactWrite => "approval_impact_write",
            MessageId::ApprovalImpactShell => "approval_impact_shell",
            MessageId::ApprovalImpactNetwork => "approval_impact_network",
            MessageId::ApprovalImpactMcpRead => "approval_impact_mcp_read",
            MessageId::ApprovalImpactMcpAction => "approval_impact_mcp_action",
            MessageId::ApprovalImpactUnknown => "approval_impact_unknown",
            MessageId::ApprovalOptionOnce => "approval_option_once",
            MessageId::ApprovalOptionAlways => "approval_option_always",
            MessageId::ApprovalOptionDeny => "approval_option_deny",
            MessageId::ApprovalOptionAbort => "approval_option_abort",
            MessageId::ApprovalPressApprove => "approval_press_approve",
            MessageId::ApprovalPressDeny => "approval_press_deny",
            MessageId::ApprovalStagedHint => "approval_staged_hint",
            MessageId::ToolReading => "tool_reading",
            MessageId::ToolListing => "tool_listing",
            MessageId::ToolSearching => "tool_searching",
            MessageId::ToolInteracting => "tool_interacting",
            MessageId::ShellJobStatusRunning => "shell_job_status_running",
            MessageId::ShellJobStatusComplete => "shell_job_status_complete",
            MessageId::ShellJobStatusFailed => "shell_job_status_failed",
            MessageId::SessionPickerTitle => "session_picker_title",
            MessageId::SessionPickerPreviewTitle => "session_picker_preview_title",
            MessageId::SessionSortRecent => "session_sort_recent",
            MessageId::SessionSortName => "session_sort_name",
            MessageId::SessionSortSize => "session_sort_size",
            MessageId::ModelPickerFlagship => "model_picker_flagship",
            MessageId::ModelPickerFast => "model_picker_fast",
            MessageId::ProviderConfigured => "provider_configured",
            MessageId::ProviderNeedsKey => "provider_needs_key",
            MessageId::PagerHelpSearch => "pager_help_search",
            MessageId::PagerHelpNavigate => "pager_help_navigate",
            MessageId::LiveTranscriptTailing => "live_transcript_tailing",
            MessageId::LiveTranscriptPaused => "live_transcript_paused",
            MessageId::ContextInspectorSessionContext => "context_inspector_session_context",
            MessageId::ContextInspectorModel => "context_inspector_model",
            MessageId::ContextInspectorWorkspace => "context_inspector_workspace",
            MessageId::PlanPromptTitle => "plan_prompt_title",
            MessageId::PlanPromptAcceptAgent => "plan_prompt_accept_agent",
            MessageId::PlanPromptAcceptYolo => "plan_prompt_accept_yolo",
            MessageId::PlanPromptRevise => "plan_prompt_revise",
            MessageId::PlanPromptExit => "plan_prompt_exit",
            MessageId::StatusPickerTitle => "status_picker_title",
            MessageId::StatusPickerFooterToggle => "status_picker_footer_toggle",
            MessageId::StatusPickerFooterAll => "status_picker_footer_all",
            MessageId::StatusPickerFooterNone => "status_picker_footer_none",
            MessageId::SlashMenuStatus => "slash_menu_status",
            MessageId::FileTreeBuilding => "file_tree_building",
            MessageId::FileTreeEmpty => "file_tree_empty",
            MessageId::SidebarEmptyHint => "sidebar_empty_hint",
            MessageId::SidebarTitlePlan => "sidebar_title_plan",
            MessageId::SidebarTitleTodos => "sidebar_title_todos",
            MessageId::SidebarTitleTasks => "sidebar_title_tasks",
            MessageId::SidebarTitleAgents => "sidebar_title_agents",
            MessageId::TaskStatusQueued => "task_status_queued",
            MessageId::TaskStatusRunning => "task_status_running",
            MessageId::TaskStatusCompleted => "task_status_completed",
            MessageId::TaskStatusFailed => "task_status_failed",
            MessageId::TurnStatusCompleted => "turn_status_completed",
            MessageId::TurnStatusInterrupted => "turn_status_interrupted",
            MessageId::TurnStatusFailed => "turn_status_failed",
            MessageId::SidebarTitleSession => "sidebar_title_session",
            MessageId::SidebarNoTodos => "sidebar_no_todos",
            MessageId::SidebarNoTasks => "sidebar_no_tasks",
            MessageId::SidebarNoAgents => "sidebar_no_agents",
            MessageId::SidebarPlanPanelHint => "sidebar_plan_panel_hint",
            MessageId::SidebarPlanUpdating => "sidebar_plan_updating",
            MessageId::SidebarTodoUpdating => "sidebar_todo_updating",
            MessageId::SidebarTaskRunning => "sidebar_task_running",
            MessageId::SidebarTasksActive => "sidebar_tasks_active",
            MessageId::SidebarNMoreSteps => "sidebar_n_more_steps",
            MessageId::SidebarNMoreTodos => "sidebar_n_more_todos",
            MessageId::SidebarAgentRunning => "sidebar_agent_running",
            MessageId::SidebarAgentDone => "sidebar_agent_done",
            MessageId::SidebarAgentDetailHint => "sidebar_agent_detail_hint",
            MessageId::SidebarLspOn => "sidebar_lsp_on",
            MessageId::SidebarLspOff => "sidebar_lsp_off",
            MessageId::PlanPromptActionRequired => "plan_prompt_action_required",
            MessageId::PlanPromptChooseAction => "plan_prompt_choose_action",
            MessageId::PlanPromptAcceptAgentDesc => "plan_prompt_accept_agent_desc",
            MessageId::PlanPromptAcceptYoloDesc => "plan_prompt_accept_yolo_desc",
            MessageId::PlanPromptReviseDesc => "plan_prompt_revise_desc",
            MessageId::PlanPromptExitDesc => "plan_prompt_exit_desc",
            MessageId::PlanPromptQuickPick => "plan_prompt_quick_pick",
            MessageId::PlanPromptMove => "plan_prompt_move",
            MessageId::PlanPromptConfirm => "plan_prompt_confirm",
            MessageId::PlanPromptClose => "plan_prompt_close",
            MessageId::SubagentStarting => "subagent_starting",
            MessageId::SubagentCompleted => "subagent_completed",
            MessageId::DetailTitleYou => "detail_title_you",
            MessageId::DetailTitleAssistant => "detail_title_assistant",
            MessageId::DetailTitleNote => "detail_title_note",
            MessageId::DetailTitleError => "detail_title_error",
            MessageId::DetailTitleReasoning => "detail_title_reasoning",
            MessageId::DetailTitleMessage => "detail_title_message",
            MessageId::DetailTitleSubAgent => "detail_title_sub_agent",
            MessageId::DetailTitleArchivedContext => "detail_title_archived_context",
            MessageId::ContextMenuCopySelection => "context_menu_copy_selection",
            MessageId::ContextMenuCopySelectionDesc => "context_menu_copy_selection_desc",
            MessageId::ContextMenuOpenSelection => "context_menu_open_selection",
            MessageId::ContextMenuOpenSelectionDesc => "context_menu_open_selection_desc",
            MessageId::ContextMenuClearSelection => "context_menu_clear_selection",
            MessageId::ContextMenuOpenDetails => "context_menu_open_details",
            MessageId::ContextMenuCopyMessage => "context_menu_copy_message",
            MessageId::ContextMenuCopyMessageDesc => "context_menu_copy_message_desc",
            MessageId::ContextMenuOpenInEditor => "context_menu_open_in_editor",
            MessageId::ContextMenuOpenInEditorDesc => "context_menu_open_in_editor_desc",
            MessageId::ContextMenuShowCell => "context_menu_show_cell",
            MessageId::ContextMenuShowCellDesc => "context_menu_show_cell_desc",
            MessageId::ContextMenuHideCell => "context_menu_hide_cell",
            MessageId::ContextMenuHideCellDesc => "context_menu_hide_cell_desc",
            MessageId::ContextMenuShowHidden => "context_menu_show_hidden",
            MessageId::ContextMenuShowHiddenDesc => "context_menu_show_hidden_desc",
            MessageId::ContextMenuPaste => "context_menu_paste",
            MessageId::ContextMenuPasteDesc => "context_menu_paste_desc",
            MessageId::ContextMenuCommandPalette => "context_menu_command_palette",
            MessageId::ContextMenuCommandPaletteDesc => "context_menu_command_palette_desc",
            MessageId::ContextMenuContextInspector => "context_menu_context_inspector",
            MessageId::ContextMenuContextInspectorDesc => "context_menu_context_inspector_desc",
            MessageId::ContextMenuHelp => "context_menu_help",
            MessageId::ContextMenuHelpDesc => "context_menu_help_desc",
            MessageId::ConfigEditorEditTitle => "config_editor_edit_title",
            MessageId::ConfigEditorScope => "config_editor_scope",
            MessageId::ConfigEditorCurrent => "config_editor_current",
            MessageId::ConfigEditorHint => "config_editor_hint",
            MessageId::ConfigEditorNew => "config_editor_new",
            MessageId::ConfigEditorFooter => "config_editor_footer",
            MessageId::LiveTranscriptFooter => "live_transcript_footer",
            MessageId::StatusSelectionCopied => "status_selection_copied",
            MessageId::StatusSelectionCleared => "status_selection_cleared",
            MessageId::StatusCellHidden => "status_cell_hidden",
            MessageId::StatusCellShown => "status_cell_shown",
            MessageId::StatusNoSelection => "status_no_selection",
            MessageId::StatusNoDetails => "status_no_details",
            MessageId::StatusMessageCopied => "status_message_copied",
            MessageId::StatusMessageEmpty => "status_message_empty",
            MessageId::StatusCopyFailed => "status_copy_failed",
            MessageId::StatusOpenedFileInEditor => "status_opened_file_in_editor",
            MessageId::StatusNoFileLinePattern => "status_no_file_line_pattern",
            MessageId::StatusShowHidden => "status_show_hidden",
            MessageId::StatusNoMessageAtLine => "status_no_message_at_line",
            MessageId::StatusNoSelectionToCopy => "status_no_selection_to_copy",
            MessageId::ShellControlNoForeground => "shell_control_no_foreground",
            MessageId::ShellControlOpened => "shell_control_opened",
            MessageId::ShellControlNoBackground => "shell_control_no_background",
            MessageId::ShellControlNotAttached => "shell_control_not_attached",
            MessageId::ShellControlBackgrounding => "shell_control_backgrounding",
            MessageId::ShellControlLockPoisoned => "shell_control_lock_poisoned",
            MessageId::FooterChipWorking => "footer_chip_working",
            MessageId::FooterChipAgents => "footer_chip_agents",
            MessageId::FooterChipTools => "footer_chip_tools",
            MessageId::FooterChipActive => "footer_chip_active",
            MessageId::FooterChipDone => "footer_chip_done",
            MessageId::FooterChipShell => "footer_chip_shell",
            MessageId::SidebarFocusPlan => "sidebar_focus_plan",
            MessageId::SidebarFocusTodos => "sidebar_focus_todos",
            MessageId::SidebarFocusTasks => "sidebar_focus_tasks",
            MessageId::SidebarFocusAgents => "sidebar_focus_agents",
            MessageId::SidebarFocusContext => "sidebar_focus_context",
            MessageId::SidebarFocusAuto => "sidebar_focus_auto",
            MessageId::StatusRequestCancelled => "status_request_cancelled",
            MessageId::StatusBacktrackCancelled => "status_backtrack_cancelled",
            MessageId::StatusBacktrackPressEsc => "status_backtrack_press_esc",
            MessageId::StatusComposerFocused => "status_composer_focused",
            MessageId::StatusAttachmentSelected => "status_attachment_selected",
            MessageId::StatusRemovedAttachment => "status_removed_attachment",
            MessageId::StatusAttachedImage => "status_attached_image",
            MessageId::StatusHistorySearchStart => "status_history_search_start",
            MessageId::StatusHistorySearchActive => "status_history_search_active",
            MessageId::StatusHistoryMatchInserted => "status_history_match_inserted",
            MessageId::StatusHistoryNoMatches => "status_history_no_matches",
            MessageId::StatusHistoryCancelled => "status_history_cancelled",
            MessageId::StatusModeSwitched => "status_mode_switched",
            MessageId::StatusThinking => "status_thinking",
            MessageId::StatusApiKeySaved => "status_api_key_saved",
            MessageId::StatusDraftStashed => "status_draft_stashed",
            MessageId::StatusCopiedToClipboard => "status_copied_to_clipboard",
            MessageId::StatusSessionSaved => "status_session_saved",
            MessageId::StatusSessionLoaded => "status_session_loaded",
            MessageId::StatusSessionDeleted => "status_session_deleted",
            MessageId::StatusNoSessions => "status_no_sessions",
            MessageId::StatusRefreshSubAgents => "status_refresh_sub_agents",
            MessageId::StatusSteeringTurn => "status_steering_turn",
            MessageId::StatusQueuedAccepted => "status_queued_accepted",
            MessageId::StatusRevisePlan => "status_revise_plan",
            MessageId::StatusPlanPromptClosed => "status_plan_prompt_closed",
            MessageId::StatusEditedInEditor => "status_edited_in_editor",
            MessageId::StatusEditorClosed => "status_editor_closed",
            MessageId::StatusEditorCancelled => "status_editor_cancelled",
            MessageId::StatusEditorError => "status_editor_error",
            MessageId::StatusNoPreviousTool => "status_no_previous_tool",
            MessageId::StatusNoNextTool => "status_no_next_tool",
            MessageId::StatusNoCellToCopy => "status_no_cell_to_copy",
            MessageId::StatusFileTreeClosed => "status_file_tree_closed",
            MessageId::StatusFileTreeHint => "status_file_tree_hint",
            MessageId::StatusAttachedPath => "status_attached_path",
            MessageId::StatusLargePasteConsolidated => "status_large_paste_consolidated",
            MessageId::StatusPasteFailed => "status_paste_failed",
            MessageId::RiskBadgeReview => "risk_badge_review",
            MessageId::RiskBadgeDestructive => "risk_badge_destructive",
            MessageId::CategoryLabelSafe => "category_label_safe",
            MessageId::CategoryLabelFileWrite => "category_label_file_write",
            MessageId::CategoryLabelShellCommand => "category_label_shell_command",
            MessageId::CategoryLabelNetwork => "category_label_network",
            MessageId::CategoryLabelMcpRead => "category_label_mcp_read",
            MessageId::CategoryLabelMcpAction => "category_label_mcp_action",
            MessageId::CategoryLabelUnknown => "category_label_unknown",
            MessageId::ApprovalTypeLabel => "approval_type_label",
            MessageId::ApprovalAboutLabel => "approval_about_label",
            MessageId::ApprovalImpactLabel => "approval_impact_label",
            MessageId::ApprovalParamsLabel => "approval_params_label",
            MessageId::ApprovalStagedBadge => "approval_staged_badge",
            MessageId::ApprovalSingleKeyHint => "approval_single_key_hint",
            MessageId::ApprovalConfirmDestructive => "approval_confirm_destructive",
            MessageId::ApprovalConfirmDestructiveSuffix => "approval_confirm_destructive_suffix",
            MessageId::ApprovalTwoKeyHint => "approval_two_key_hint",
            MessageId::ApprovalKeyHintEnterY => "approval_key_hint_enter_y",
            MessageId::ApprovalKeyHintEnterA => "approval_key_hint_enter_a",
            MessageId::ApprovalKeyHintEnter => "approval_key_hint_enter",
            MessageId::ApprovalKeyHintTwoKeys => "approval_key_hint_two_keys",
            MessageId::ApprovalFooterHint => "approval_footer_hint",
            MessageId::ApprovalCardTitle => "approval_card_title",
            MessageId::ElevationTitle => "elevation_title",
            MessageId::ElevationToolLabel => "elevation_tool_label",
            MessageId::ElevationCmdLabel => "elevation_cmd_label",
            MessageId::ElevationReasonLabel => "elevation_reason_label",
            MessageId::ElevationImpactSection => "elevation_impact_section",
            MessageId::ElevationImpactNetwork => "elevation_impact_network",
            MessageId::ElevationImpactWrite => "elevation_impact_write",
            MessageId::ElevationImpactFullAccess => "elevation_impact_full_access",
            MessageId::ElevationProceedSection => "elevation_proceed_section",
            MessageId::ElevationShortcutKeyN => "elevation_shortcut_key_n",
            MessageId::ElevationShortcutKeyW => "elevation_shortcut_key_w",
            MessageId::ElevationShortcutKeyF => "elevation_shortcut_key_f",
            MessageId::ElevationShortcutKeyA => "elevation_shortcut_key_a",
            MessageId::ElevationModalTitle => "elevation_modal_title",
            MessageId::ElevationOptionNetwork => "elevation_option_network",
            MessageId::ElevationOptionWrite => "elevation_option_write",
            MessageId::ElevationOptionFullAccess => "elevation_option_full_access",
            MessageId::ElevationOptionAbort => "elevation_option_abort",
            MessageId::ElevationOptionNetworkDesc => "elevation_option_network_desc",
            MessageId::ElevationOptionWriteDesc => "elevation_option_write_desc",
            MessageId::ElevationOptionFullAccessDesc => "elevation_option_full_access_desc",
            MessageId::ElevationOptionAbortDesc => "elevation_option_abort_desc",
            MessageId::SubAgentStatusRunning => "sub_agent_status_running",
            MessageId::SubAgentStatusCompleted => "sub_agent_status_completed",
            MessageId::SubAgentStatusFailed => "sub_agent_status_failed",
            MessageId::SubAgentStatusCancelled => "sub_agent_status_cancelled",
            MessageId::SubAgentStatusInterrupted => "sub_agent_status_interrupted",
            MessageId::SubAgentsTitle => "sub_agents_title",
            MessageId::SubAgentEscToClose => "sub_agent_esc_to_close",
            MessageId::SubAgentRToRefresh => "sub_agent_r_to_refresh",
            MessageId::StatusPickerInstruction => "status_picker_instruction",
            MessageId::PendingInputSectionTitle => "pending_input_section_title",
            MessageId::PendingInputContextSection => "pending_input_context_section",
            MessageId::ShellJobStatusStale => "shell_job_status_stale",
            MessageId::ShellJobStatusKilled => "shell_job_status_killed",
            MessageId::ShellJobStatusTimeout => "shell_job_status_timeout",
            MessageId::ShellJobNoLiveJobs => "shell_job_no_live_jobs",
            MessageId::ShellJobNoNewOutput => "shell_job_no_new_output",
            MessageId::ShellJobStdout => "shell_job_stdout",
            MessageId::ShellJobStderr => "shell_job_stderr",
            MessageId::ShellJobEmpty => "shell_job_empty",
            MessageId::TaskStatusCanceled => "task_status_canceled",
            MessageId::TaskNoTasksFound => "task_no_tasks_found",
            MessageId::UsageMcp => "usage_mcp",
            MessageId::UsageMcpList => "usage_mcp_list",
            MessageId::UsageMcpShow => "usage_mcp_show",
            MessageId::UsageMcpEnable => "usage_mcp_enable",
            MessageId::UsageMcpDisable => "usage_mcp_disable",
            MessageId::UsageMcpRefresh => "usage_mcp_refresh",
            MessageId::UsageMcpRemove => "usage_mcp_remove",
            MessageId::UsageMcpAdd => "usage_mcp_add",
            MessageId::UsageJobs => "usage_jobs",
            MessageId::UsageJobsList => "usage_jobs_list",
            MessageId::UsageJobsShow => "usage_jobs_show",
            MessageId::UsageJobsPoll => "usage_jobs_poll",
            MessageId::UsageJobsWait => "usage_jobs_wait",
            MessageId::UsageJobsStdin => "usage_jobs_stdin",
            MessageId::UsageJobsCancel => "usage_jobs_cancel",
            MessageId::UsageJobsCloseStdin => "usage_jobs_close_stdin",
            MessageId::UsageTask => "usage_task",
            MessageId::UsageTaskList => "usage_task_list",
            MessageId::UsageTaskShow => "usage_task_show",
            MessageId::UsageTaskCancel => "usage_task_cancel",
            MessageId::UsageQueue => "usage_queue",
            MessageId::UsageCycle => "usage_cycle",
            MessageId::UsageRestore => "usage_restore",
            MessageId::UsageNote => "usage_note",
            MessageId::UsageRlm => "usage_rlm",
            MessageId::UsageProfile => "usage_profile",
            MessageId::UsageRecall => "usage_recall",
            MessageId::UsageAttach => "usage_attach",
            MessageId::UsageSkill => "usage_skill",
            MessageId::UsageStatusline => "usage_statusline",
            MessageId::CmdFailedGeneric => "cmd_failed_generic",
            MessageId::CmdSessionSaved => "cmd_session_saved",
            MessageId::CmdSessionLoaded => "cmd_session_loaded",
            MessageId::CmdQueueCleared => "cmd_queue_cleared",
            MessageId::CmdNoQueuedMessages => "cmd_no_queued_messages",
            MessageId::CmdNoSnapshots => "cmd_no_snapshots",
            MessageId::CmdStashEmpty => "cmd_stash_empty",
            MessageId::CmdLspEnabled => "cmd_lsp_enabled",
            MessageId::CmdLspDisabled => "cmd_lsp_disabled",
            MessageId::CmdLoggedOut => "cmd_logged_out",
            MessageId::CmdMcpNoServers => "cmd_mcp_no_servers",
            MessageId::CmdJobsNone => "cmd_jobs_none",
            MessageId::CmdTaskNone => "cmd_task_none",
            MessageId::CmdRestoreDone => "cmd_restore_done",
            MessageId::CmdNoteAppended => "cmd_note_appended",
            MessageId::CmdProfileSwitched => "cmd_profile_switched",
            MessageId::CmdAttachedFile => "cmd_attached_file",
            MessageId::CmdAttachedDir => "cmd_attached_dir",
            MessageId::CmdDetached => "cmd_detached",
            MessageId::CmdInvalidArgs => "cmd_invalid_args",
            MessageId::CmdUnknownCommand => "cmd_unknown_command",
            MessageId::CmdExecutionError => "cmd_execution_error",
        }
    }
}

#[allow(dead_code)]
pub const ALL_MESSAGE_IDS: &[MessageId] = &[
    MessageId::ComposerPlaceholder,
    MessageId::HistorySearchPlaceholder,
    MessageId::HistorySearchTitle,
    MessageId::HistoryHintMove,
    MessageId::HistoryHintAccept,
    MessageId::HistoryHintRestore,
    MessageId::HistoryNoMatches,
    MessageId::ConfigTitle,
    MessageId::ConfigModalTitle,
    MessageId::ConfigSearchPlaceholder,
    MessageId::ConfigNoSettings,
    MessageId::ConfigNoMatchesPrefix,
    MessageId::ConfigFilteredSettings,
    MessageId::ConfigShowing,
    MessageId::ConfigFooterDefault,
    MessageId::ConfigFooterScrollable,
    MessageId::ConfigFooterFiltered,
    MessageId::HelpTitle,
    MessageId::HelpFilterPlaceholder,
    MessageId::HelpFilterPrefix,
    MessageId::HelpNoMatches,
    MessageId::HelpSlashCommands,
    MessageId::HelpKeybindings,
    MessageId::HelpFooterTypeFilter,
    MessageId::HelpFooterMove,
    MessageId::HelpFooterJump,
    MessageId::HelpFooterClose,
    MessageId::CmdAgentDescription,
    MessageId::CmdAttachDescription,
    MessageId::CmdCacheDescription,
    MessageId::CmdClearDescription,
    MessageId::CmdCompactDescription,
    MessageId::CmdConfigDescription,
    MessageId::CmdContextDescription,
    MessageId::CmdCostDescription,
    MessageId::CmdCycleDescription,
    MessageId::CmdCyclesDescription,
    MessageId::CmdDiffDescription,
    MessageId::CmdEditDescription,
    MessageId::CmdExitDescription,
    MessageId::CmdExportDescription,
    MessageId::CmdHelpDescription,
    MessageId::CmdHomeDescription,
    MessageId::CmdHooksDescription,
    MessageId::CmdInitDescription,
    MessageId::CmdJobsDescription,
    MessageId::CmdLinksDescription,
    MessageId::CmdLoadDescription,
    MessageId::CmdLogoutDescription,
    MessageId::CmdMcpDescription,
    MessageId::CmdMemoryDescription,
    MessageId::CmdModelDescription,
    MessageId::CmdModelsDescription,
    MessageId::CmdNoteDescription,
    MessageId::CmdPlanDescription,
    MessageId::CmdProviderDescription,
    MessageId::CmdQueueDescription,
    MessageId::CmdRecallDescription,
    MessageId::CmdRestoreDescription,
    MessageId::CmdRetryDescription,
    MessageId::CmdReviewDescription,
    MessageId::CmdRlmDescription,
    MessageId::CmdSaveDescription,
    MessageId::CmdSessionsDescription,
    MessageId::CmdSettingsDescription,
    MessageId::CmdSkillDescription,
    MessageId::CmdSkillsDescription,
    MessageId::CmdStashDescription,
    MessageId::CmdStatuslineDescription,
    MessageId::CmdSubagentsDescription,
    MessageId::CmdSwarmDescription,
    MessageId::CmdSystemDescription,
    MessageId::CmdTaskDescription,
    MessageId::CmdTokensDescription,
    MessageId::CmdTrustDescription,
    MessageId::CmdLspDescription,
    MessageId::CmdShareDescription,
    MessageId::CmdUndoDescription,
    MessageId::CmdYoloDescription,
    MessageId::CmdCacheAdvice,
    MessageId::CmdCacheFootnote,
    MessageId::CmdCacheHeader,
    MessageId::CmdCacheNoData,
    MessageId::CmdCacheTotals,
    MessageId::CmdCostReport,
    MessageId::CmdTokensCacheBoth,
    MessageId::CmdTokensCacheHitOnly,
    MessageId::CmdTokensCacheMissOnly,
    MessageId::CmdTokensContextUnknownWindow,
    MessageId::CmdTokensContextWithWindow,
    MessageId::CmdTokensNotReported,
    MessageId::CmdTokensReport,
    MessageId::FooterAgentSingular,
    MessageId::FooterAgentsPlural,
    MessageId::FooterPressCtrlCAgain,
    MessageId::FooterWorking,
    MessageId::HelpSectionActions,
    MessageId::HelpSectionClipboard,
    MessageId::HelpSectionEditing,
    MessageId::HelpSectionHelp,
    MessageId::HelpSectionModes,
    MessageId::HelpSectionNavigation,
    MessageId::HelpSectionSessions,
    MessageId::KbScrollTranscript,
    MessageId::KbNavigateHistory,
    MessageId::KbScrollTranscriptAlt,
    MessageId::KbScrollPage,
    MessageId::KbJumpTopBottom,
    MessageId::KbJumpTopBottomEmpty,
    MessageId::KbJumpToolBlocks,
    MessageId::KbMoveCursor,
    MessageId::KbJumpLineStartEnd,
    MessageId::KbDeleteChar,
    MessageId::KbClearDraft,
    MessageId::KbStashDraft,
    MessageId::KbSearchHistory,
    MessageId::KbInsertNewline,
    MessageId::KbSendDraft,
    MessageId::KbCloseMenu,
    MessageId::KbCancelOrExit,
    MessageId::KbShellControls,
    MessageId::KbExitEmpty,
    MessageId::KbCommandPalette,
    MessageId::KbFuzzyFilePicker,
    MessageId::KbCompactInspector,
    MessageId::KbLastMessagePager,
    MessageId::KbSelectedDetails,
    MessageId::KbToolDetailsPager,
    MessageId::KbThinkingPager,
    MessageId::KbLiveTranscript,
    MessageId::KbBacktrackMessage,
    MessageId::KbCompleteCycleModes,
    MessageId::KbJumpPlanAgentYolo,
    MessageId::KbAltJumpPlanAgentYolo,
    MessageId::KbFocusSidebar,
    MessageId::KbTogglePlanAgent,
    MessageId::KbSessionPicker,
    MessageId::KbPasteAttach,
    MessageId::KbCopySelection,
    MessageId::KbContextMenu,
    MessageId::KbAttachPath,
    MessageId::KbHelpOverlay,
    MessageId::KbToggleHelp,
    MessageId::KbToggleHelpSlash,
    MessageId::HelpUsageLabel,
    MessageId::HelpAliasesLabel,
    MessageId::SettingsTitle,
    MessageId::SettingsConfigFile,
    MessageId::ClearConversation,
    MessageId::ClearConversationBusy,
    MessageId::ModelChanged,
    MessageId::LinksTitle,
    MessageId::LinksDashboard,
    MessageId::LinksDocs,
    MessageId::LinksTip,
    MessageId::SubagentsFetching,
    MessageId::HelpUnknownCommand,
    MessageId::HomeDashboardTitle,
    MessageId::HomeModel,
    MessageId::HomeMode,
    MessageId::HomeWorkspace,
    MessageId::HomeHistory,
    MessageId::HomeTokens,
    MessageId::HomeQueued,
    MessageId::HomeSubagents,
    MessageId::HomeSkill,
    MessageId::HomeQuickActions,
    MessageId::HomeQuickLinks,
    MessageId::HomeQuickSkills,
    MessageId::HomeQuickConfig,
    MessageId::HomeQuickSettings,
    MessageId::HomeQuickModel,
    MessageId::HomeQuickSubagents,
    MessageId::HomeQuickTaskList,
    MessageId::HomeQuickHelp,
    MessageId::HomeModeTips,
    MessageId::HomeAgentModeTip,
    MessageId::HomeAgentModeReviewTip,
    MessageId::HomeAgentModeYoloTip,
    MessageId::HomeYoloModeTip,
    MessageId::HomeYoloModeCaution,
    MessageId::HomePlanModeTip,
    MessageId::HomePlanModeChecklistTip,
    MessageId::OnboardingWelcomeTitle,
    MessageId::OnboardingWelcomeDesc,
    MessageId::OnboardingWelcomeStep1,
    MessageId::OnboardingWelcomeStep2,
    MessageId::OnboardingWelcomePromptEnter,
    MessageId::OnboardingWelcomePromptExit,
    MessageId::OnboardingLanguageTitle,
    MessageId::OnboardingLanguageDesc,
    MessageId::OnboardingLanguageFooter,
    MessageId::OnboardingApiKeyTitle,
    MessageId::OnboardingApiKeyStep1,
    MessageId::OnboardingApiKeyStep2,
    MessageId::OnboardingApiKeyPathNote,
    MessageId::OnboardingApiKeyPasteNote,
    MessageId::OnboardingApiKeyPlaceholder,
    MessageId::OnboardingApiKeyLabel,
    MessageId::OnboardingApiKeyFooter,
    MessageId::OnboardingTrustTitle,
    MessageId::OnboardingTrustPrompt,
    MessageId::OnboardingTrustWorkspaceLabel,
    MessageId::OnboardingTrustYExplain,
    MessageId::OnboardingTrustNExplain,
    MessageId::OnboardingTrustFooter,
    MessageId::OnboardingTipsTitle,
    MessageId::OnboardingTipsTip1,
    MessageId::OnboardingTipsTip2,
    MessageId::OnboardingTipsTip3,
    MessageId::OnboardingTipsTip4,
    MessageId::OnboardingTipsFooter,
    MessageId::OnboardingPanelTitle,
    MessageId::OnboardingStepIndicator,
    MessageId::FooterStateReady,
    MessageId::FooterStateDraft,
    MessageId::FooterStateOverlay,
    MessageId::FooterStateCompacting,
    MessageId::ApprovalTitleBenign,
    MessageId::ApprovalTitleDestructive,
    MessageId::ApprovalImpactReadonly,
    MessageId::ApprovalImpactWrite,
    MessageId::ApprovalImpactShell,
    MessageId::ApprovalImpactNetwork,
    MessageId::ApprovalImpactMcpRead,
    MessageId::ApprovalImpactMcpAction,
    MessageId::ApprovalImpactUnknown,
    MessageId::ApprovalOptionOnce,
    MessageId::ApprovalOptionAlways,
    MessageId::ApprovalOptionDeny,
    MessageId::ApprovalOptionAbort,
    MessageId::ApprovalPressApprove,
    MessageId::ApprovalPressDeny,
    MessageId::ApprovalStagedHint,
    MessageId::ToolReading,
    MessageId::ToolListing,
    MessageId::ToolSearching,
    MessageId::ToolInteracting,
    MessageId::ShellJobStatusRunning,
    MessageId::ShellJobStatusComplete,
    MessageId::ShellJobStatusFailed,
    MessageId::SessionPickerTitle,
    MessageId::SessionPickerPreviewTitle,
    MessageId::SessionSortRecent,
    MessageId::SessionSortName,
    MessageId::SessionSortSize,
    MessageId::ModelPickerFlagship,
    MessageId::ModelPickerFast,
    MessageId::ProviderConfigured,
    MessageId::ProviderNeedsKey,
    MessageId::PagerHelpSearch,
    MessageId::PagerHelpNavigate,
    MessageId::LiveTranscriptTailing,
    MessageId::LiveTranscriptPaused,
    MessageId::ContextInspectorSessionContext,
    MessageId::ContextInspectorModel,
    MessageId::ContextInspectorWorkspace,
    MessageId::PlanPromptTitle,
    MessageId::PlanPromptAcceptAgent,
    MessageId::PlanPromptAcceptYolo,
    MessageId::PlanPromptRevise,
    MessageId::PlanPromptExit,
    MessageId::StatusPickerTitle,
    MessageId::StatusPickerFooterToggle,
    MessageId::StatusPickerFooterAll,
    MessageId::StatusPickerFooterNone,
    MessageId::SlashMenuStatus,
    MessageId::FileTreeBuilding,
    MessageId::FileTreeEmpty,
    MessageId::SidebarEmptyHint,
    MessageId::SidebarTitlePlan,
    MessageId::SidebarTitleTodos,
    MessageId::SidebarTitleTasks,
    MessageId::SidebarTitleAgents,
    MessageId::TaskStatusQueued,
    MessageId::TaskStatusRunning,
    MessageId::TaskStatusCompleted,
    MessageId::TaskStatusFailed,
    MessageId::TurnStatusCompleted,
    MessageId::TurnStatusInterrupted,
    MessageId::TurnStatusFailed,
    MessageId::SidebarTitleSession,
    MessageId::SidebarNoTodos,
    MessageId::SidebarNoTasks,
    MessageId::SidebarNoAgents,
    MessageId::SidebarPlanPanelHint,
    MessageId::SidebarPlanUpdating,
    MessageId::SidebarTodoUpdating,
    MessageId::SidebarTaskRunning,
    MessageId::SidebarTasksActive,
    MessageId::SidebarNMoreSteps,
    MessageId::SidebarNMoreTodos,
    MessageId::SidebarAgentRunning,
    MessageId::SidebarAgentDone,
    MessageId::SidebarAgentDetailHint,
    MessageId::SidebarLspOn,
    MessageId::SidebarLspOff,
    MessageId::PlanPromptActionRequired,
    MessageId::PlanPromptChooseAction,
    MessageId::PlanPromptAcceptAgentDesc,
    MessageId::PlanPromptAcceptYoloDesc,
    MessageId::PlanPromptReviseDesc,
    MessageId::PlanPromptExitDesc,
    MessageId::PlanPromptQuickPick,
    MessageId::PlanPromptMove,
    MessageId::PlanPromptConfirm,
    MessageId::PlanPromptClose,
    MessageId::SubagentStarting,
    MessageId::SubagentCompleted,
    MessageId::DetailTitleYou,
    MessageId::DetailTitleAssistant,
    MessageId::DetailTitleNote,
    MessageId::DetailTitleError,
    MessageId::DetailTitleReasoning,
    MessageId::DetailTitleMessage,
    MessageId::DetailTitleSubAgent,
    MessageId::DetailTitleArchivedContext,
    MessageId::ContextMenuCopySelection,
    MessageId::ContextMenuCopySelectionDesc,
    MessageId::ContextMenuOpenSelection,
    MessageId::ContextMenuOpenSelectionDesc,
    MessageId::ContextMenuClearSelection,
    MessageId::ContextMenuOpenDetails,
    MessageId::ContextMenuCopyMessage,
    MessageId::ContextMenuCopyMessageDesc,
    MessageId::ContextMenuOpenInEditor,
    MessageId::ContextMenuOpenInEditorDesc,
    MessageId::ContextMenuShowCell,
    MessageId::ContextMenuShowCellDesc,
    MessageId::ContextMenuHideCell,
    MessageId::ContextMenuHideCellDesc,
    MessageId::ContextMenuShowHidden,
    MessageId::ContextMenuShowHiddenDesc,
    MessageId::ContextMenuPaste,
    MessageId::ContextMenuPasteDesc,
    MessageId::ContextMenuCommandPalette,
    MessageId::ContextMenuCommandPaletteDesc,
    MessageId::ContextMenuContextInspector,
    MessageId::ContextMenuContextInspectorDesc,
    MessageId::ContextMenuHelp,
    MessageId::ContextMenuHelpDesc,
    MessageId::ConfigEditorEditTitle,
    MessageId::ConfigEditorScope,
    MessageId::ConfigEditorCurrent,
    MessageId::ConfigEditorHint,
    MessageId::ConfigEditorNew,
    MessageId::ConfigEditorFooter,
    MessageId::LiveTranscriptFooter,
    MessageId::StatusSelectionCopied,
    MessageId::StatusSelectionCleared,
    MessageId::StatusCellHidden,
    MessageId::StatusCellShown,
    MessageId::StatusNoSelection,
    MessageId::StatusNoDetails,
    MessageId::StatusMessageCopied,
    MessageId::StatusMessageEmpty,
    MessageId::StatusCopyFailed,
    MessageId::StatusOpenedFileInEditor,
    MessageId::StatusNoFileLinePattern,
    MessageId::StatusShowHidden,
    MessageId::StatusNoMessageAtLine,
    MessageId::StatusNoSelectionToCopy,
    MessageId::ShellControlNoForeground,
    MessageId::ShellControlOpened,
    MessageId::ShellControlNoBackground,
    MessageId::ShellControlNotAttached,
    MessageId::ShellControlBackgrounding,
    MessageId::ShellControlLockPoisoned,
    MessageId::FooterChipWorking,
    MessageId::FooterChipAgents,
    MessageId::FooterChipTools,
    MessageId::FooterChipActive,
    MessageId::FooterChipDone,
    MessageId::FooterChipShell,
    MessageId::SidebarFocusPlan,
    MessageId::SidebarFocusTodos,
    MessageId::SidebarFocusTasks,
    MessageId::SidebarFocusAgents,
    MessageId::SidebarFocusContext,
    MessageId::SidebarFocusAuto,
    MessageId::StatusRequestCancelled,
    MessageId::StatusBacktrackCancelled,
    MessageId::StatusBacktrackPressEsc,
    MessageId::StatusComposerFocused,
    MessageId::StatusAttachmentSelected,
    MessageId::StatusRemovedAttachment,
    MessageId::StatusAttachedImage,
    MessageId::StatusHistorySearchStart,
    MessageId::StatusHistorySearchActive,
    MessageId::StatusHistoryMatchInserted,
    MessageId::StatusHistoryNoMatches,
    MessageId::StatusHistoryCancelled,
    MessageId::StatusModeSwitched,
    MessageId::StatusThinking,
    MessageId::StatusApiKeySaved,
    MessageId::StatusDraftStashed,
    MessageId::StatusCopiedToClipboard,
    MessageId::StatusSessionSaved,
    MessageId::StatusSessionLoaded,
    MessageId::StatusSessionDeleted,
    MessageId::StatusNoSessions,
    MessageId::StatusRefreshSubAgents,
    MessageId::StatusSteeringTurn,
    MessageId::StatusQueuedAccepted,
    MessageId::StatusRevisePlan,
    MessageId::StatusPlanPromptClosed,
    MessageId::StatusEditedInEditor,
    MessageId::StatusEditorClosed,
    MessageId::StatusEditorCancelled,
    MessageId::StatusEditorError,
    MessageId::StatusNoPreviousTool,
    MessageId::StatusNoNextTool,
    MessageId::StatusNoCellToCopy,
    MessageId::StatusFileTreeClosed,
    MessageId::StatusFileTreeHint,
    MessageId::StatusAttachedPath,
    MessageId::StatusLargePasteConsolidated,
    MessageId::StatusPasteFailed,
    MessageId::RiskBadgeReview,
    MessageId::RiskBadgeDestructive,
    MessageId::CategoryLabelSafe,
    MessageId::CategoryLabelFileWrite,
    MessageId::CategoryLabelShellCommand,
    MessageId::CategoryLabelNetwork,
    MessageId::CategoryLabelMcpRead,
    MessageId::CategoryLabelMcpAction,
    MessageId::CategoryLabelUnknown,
    MessageId::ApprovalTypeLabel,
    MessageId::ApprovalAboutLabel,
    MessageId::ApprovalImpactLabel,
    MessageId::ApprovalParamsLabel,
    MessageId::ApprovalStagedBadge,
    MessageId::ApprovalSingleKeyHint,
    MessageId::ApprovalConfirmDestructive,
    MessageId::ApprovalConfirmDestructiveSuffix,
    MessageId::ApprovalTwoKeyHint,
    MessageId::ApprovalKeyHintEnterY,
    MessageId::ApprovalKeyHintEnterA,
    MessageId::ApprovalKeyHintEnter,
    MessageId::ApprovalKeyHintTwoKeys,
    MessageId::ApprovalFooterHint,
    MessageId::ApprovalCardTitle,
    MessageId::ElevationTitle,
    MessageId::ElevationToolLabel,
    MessageId::ElevationCmdLabel,
    MessageId::ElevationReasonLabel,
    MessageId::ElevationImpactSection,
    MessageId::ElevationImpactNetwork,
    MessageId::ElevationImpactWrite,
    MessageId::ElevationImpactFullAccess,
    MessageId::ElevationProceedSection,
    MessageId::ElevationShortcutKeyN,
    MessageId::ElevationShortcutKeyW,
    MessageId::ElevationShortcutKeyF,
    MessageId::ElevationShortcutKeyA,
    MessageId::ElevationModalTitle,
    MessageId::ElevationOptionNetwork,
    MessageId::ElevationOptionWrite,
    MessageId::ElevationOptionFullAccess,
    MessageId::ElevationOptionAbort,
    MessageId::ElevationOptionNetworkDesc,
    MessageId::ElevationOptionWriteDesc,
    MessageId::ElevationOptionFullAccessDesc,
    MessageId::ElevationOptionAbortDesc,
    MessageId::SubAgentStatusRunning,
    MessageId::SubAgentStatusCompleted,
    MessageId::SubAgentStatusFailed,
    MessageId::SubAgentStatusCancelled,
    MessageId::SubAgentStatusInterrupted,
    MessageId::StatusPickerInstruction,
    MessageId::PendingInputSectionTitle,
    MessageId::PendingInputContextSection,
    MessageId::ShellJobStatusStale,
    MessageId::ShellJobStatusKilled,
    MessageId::ShellJobStatusTimeout,
    MessageId::ShellJobNoLiveJobs,
    MessageId::ShellJobNoNewOutput,
    MessageId::ShellJobStdout,
    MessageId::ShellJobStderr,
    MessageId::ShellJobEmpty,
    MessageId::TaskStatusCanceled,
    MessageId::TaskNoTasksFound,
    MessageId::UsageMcp,
    MessageId::UsageMcpList,
    MessageId::UsageMcpShow,
    MessageId::UsageMcpEnable,
    MessageId::UsageMcpDisable,
    MessageId::UsageMcpRefresh,
    MessageId::UsageMcpRemove,
    MessageId::UsageMcpAdd,
    MessageId::UsageJobs,
    MessageId::UsageJobsList,
    MessageId::UsageJobsShow,
    MessageId::UsageJobsPoll,
    MessageId::UsageJobsWait,
    MessageId::UsageJobsStdin,
    MessageId::UsageJobsCancel,
    MessageId::UsageJobsCloseStdin,
    MessageId::UsageTask,
    MessageId::UsageTaskList,
    MessageId::UsageTaskShow,
    MessageId::UsageTaskCancel,
    MessageId::UsageQueue,
    MessageId::UsageCycle,
    MessageId::UsageRestore,
    MessageId::UsageNote,
    MessageId::UsageRlm,
    MessageId::UsageProfile,
    MessageId::UsageRecall,
    MessageId::UsageAttach,
    MessageId::UsageSkill,
    MessageId::UsageStatusline,
    MessageId::CmdFailedGeneric,
    MessageId::CmdSessionSaved,
    MessageId::CmdSessionLoaded,
    MessageId::CmdQueueCleared,
    MessageId::CmdNoQueuedMessages,
    MessageId::CmdNoSnapshots,
    MessageId::CmdStashEmpty,
    MessageId::CmdLspEnabled,
    MessageId::CmdLspDisabled,
    MessageId::CmdLoggedOut,
    MessageId::CmdMcpNoServers,
    MessageId::CmdJobsNone,
    MessageId::CmdTaskNone,
    MessageId::CmdRestoreDone,
    MessageId::CmdNoteAppended,
    MessageId::CmdProfileSwitched,
    MessageId::CmdAttachedFile,
    MessageId::CmdAttachedDir,
    MessageId::CmdDetached,
    MessageId::CmdInvalidArgs,
    MessageId::CmdUnknownCommand,
    MessageId::CmdExecutionError,
];

/// Central i18n string store with compile-time embedded English and
/// runtime-loaded locale overrides.
///
/// # Lifetime strategy
///
/// `tr()` must continue returning `&'static str` to preserve every existing
/// call site. Runtime-loaded strings (from `~/.deepseek/i18n/`) are leaked
/// via `Box::leak()`. The leak is bounded (~50 KB per locale, typically
/// one switch per session).
struct I18nStore {
    /// Embedded English strings loaded from `assets/i18n/en.json` at compile time.
    embedded_en: HashMap<&'static str, &'static str>,
    /// Currently active locale.
    active_locale: Locale,
    /// Active locale strings — either embedded or loaded from disk.
    active_strings: HashMap<&'static str, &'static str>,
}

/// Global i18n store, initialised on first access.
static I18N: LazyLock<RwLock<I18nStore>> = LazyLock::new(|| {
    // Load embedded English from the compile-time-included en.json.
    let embedded_en = load_embedded_json(include_str!("../assets/i18n/en.json"));
    RwLock::new(I18nStore {
        embedded_en: embedded_en.clone(),
        active_locale: Locale::En,
        active_strings: embedded_en,
    })
});

/// Parse a JSON string of i18n entries into a `HashMap<&'static str, &'static str>`.
///
/// Every value string is leaked so that `tr()` can return `&'static str`.
fn parse_json_to_static_map(raw: &str) -> Option<HashMap<&'static str, &'static str>> {
    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    let mut map = HashMap::new();
    if let serde_json::Value::Object(obj) = parsed {
        for (key, value) in obj {
            if let serde_json::Value::String(text) = value {
                let leaked_key: &'static str = Box::leak(key.into_boxed_str());
                let leaked_val: &'static str = Box::leak(text.into_boxed_str());
                map.insert(leaked_key, leaked_val);
            }
        }
    }
    Some(map)
}

fn load_embedded_json(json: &'static str) -> HashMap<&'static str, &'static str> {
    parse_json_to_static_map(json).unwrap_or_default()
}

fn load_disk_json(path: &std::path::Path) -> Option<HashMap<&'static str, &'static str>> {
    let content = std::fs::read_to_string(path).ok()?;
    parse_json_to_static_map(&content)
}

/// Initialise the i18n store for the given locale.
///
/// For English this is a no-op (the store already holds embedded English).
/// For other locales the embedded JSON for that locale is loaded if available,
/// otherwise English is used as fallback.
pub fn init_locale(locale: Locale) {
    let mut store = I18N.write().unwrap();
    store.active_locale = locale;
    if locale == Locale::En {
        store.active_strings = store.embedded_en.clone();
    } else {
        // Try to load runtime override from disk first.
        if let Some(disk) = load_disk_from_i18n_dir(locale) {
            store.active_strings = disk;
        } else {
            // No runtime file — fall back to embedded English.
            store.active_strings = store.embedded_en.clone();
        }
    }
}

/// Reload the active locale from disk.
///
/// This is called after the AI translator writes a new i18n.json, or when the
/// user switches locale at runtime. Delegates to [`init_locale`].
pub fn reload_locale(locale: Locale) {
    init_locale(locale);
}

/// Try to load a locale-specific JSON file from `~/.deepseek/i18n/{tag}.json`.
fn load_disk_from_i18n_dir(locale: Locale) -> Option<HashMap<&'static str, &'static str>> {
    let home = dirs::home_dir()?;
    let path = home
        .join(".deepseek")
        .join("i18n")
        .join(format!("{}.json", locale.tag()));
    load_disk_json(&path)
}

/// Save a translated `HashMap<String, String>` to `~/.deepseek/i18n/{tag}.json`.
pub fn save_locale_json(tag: &str, data: &HashMap<String, String>) -> std::io::Result<()> {
    let home = dirs::home_dir().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "home directory not found")
    })?;
    let i18n_dir = home.join(".deepseek").join("i18n");
    std::fs::create_dir_all(&i18n_dir)?;
    let path = i18n_dir.join(format!("{}.json", tag));
    let json = serde_json::to_string_pretty(data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, json)?;
    // Copy to i18n.json as an alias for the current locale
    let i18n_json_path = i18n_dir.join("i18n.json");
    let _ = std::fs::copy(&path, &i18n_json_path);
    Ok(())
}

/// Get the embedded English data as owned `HashMap<String, String>`.
pub fn get_embedded_en_data() -> HashMap<String, String> {
    let store = I18N.read().unwrap();
    store
        .embedded_en
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

fn cache_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|home| home.join(".deepseek").join("i18n").join("cache"))
}

fn compute_cache_key(keys: &[String], target_lang: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    let mut sorted_keys = keys.to_vec();
    sorted_keys.sort();
    for key in &sorted_keys {
        hasher.update(key.as_bytes());
    }
    hasher.update(target_lang.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn load_cached_translation(cache_key: &str) -> Option<HashMap<String, String>> {
    let cache_path = cache_dir()?.join(format!("{}.json", cache_key));
    if !cache_path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&cache_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let mut map = HashMap::new();
    if let serde_json::Value::Object(obj) = json {
        for (key, value) in obj {
            if let serde_json::Value::String(text) = value {
                map.insert(key, text);
            }
        }
    }
    Some(map)
}

fn save_cached_translation(cache_key: &str, data: &HashMap<String, String>) {
    let Some(dir) = cache_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    let cache_path = dir.join(format!("{}.json", cache_key));
    let mut map = serde_json::Map::new();
    for (key, value) in data {
        map.insert(key.clone(), serde_json::Value::String(value.clone()));
    }
    let Ok(content) = serde_json::to_string_pretty(&serde_json::Value::Object(map)) else {
        return;
    };
    let _ = std::fs::write(&cache_path, content);
}

/// Translate English UI strings to the target language via the DeepSeek API.
///
/// Delegates the HTTP call to [`DeepSeekClient::translate_json`] so the
/// project's unified retry / rate-limit / TLS stack is used. Results are
/// cached under `~/.deepseek/i18n/cache/` keyed by a SHA-256 hash of the
/// keys + language.
///
/// If `missing_keys` is `Some`, only those keys are sent for translation;
/// otherwise all keys in `en_data` are translated.
pub async fn translate_via_api(
    client: &crate::client::DeepSeekClient,
    en_data: &HashMap<String, String>,
    target_lang: &str,
    missing_keys: Option<&[String]>,
) -> Result<HashMap<String, String>, String> {
    // Determine which keys to translate
    let keys_to_translate: Vec<String> = match missing_keys {
        Some(keys) => keys.to_vec(),
        None => en_data.keys().cloned().collect(),
    };

    if keys_to_translate.is_empty() {
        return Ok(HashMap::new());
    }

    // Check cache first
    let cache_key = compute_cache_key(&keys_to_translate, target_lang);
    if let Some(cached) = load_cached_translation(&cache_key) {
        return Ok(cached);
    }

    // Build the subset of data to send to the API
    let subset_data: HashMap<String, String> = keys_to_translate
        .iter()
        .filter_map(|key| en_data.get(key).map(|v| (key.clone(), v.clone())))
        .collect();

    // Delegate to the project's unified DeepSeekClient
    let result = client
        .translate_json(&subset_data, target_lang)
        .await
        .map_err(|e| format!("{e:#}"))?;

    // Save to cache for future incremental translations
    save_cached_translation(&cache_key, &result);

    Ok(result)
}

pub fn tr(locale: Locale, id: MessageId) -> &'static str {
    let _ = locale; // locale parameter is advisory; actual language depends on init_locale()
    let key = id.to_key();
    let store = I18N.read().unwrap();
    // 1. Try active locale strings
    if let Some(text) = store.active_strings.get(key) {
        return text;
    }
    // 2. Fall back to embedded English
    if let Some(text) = store.embedded_en.get(key) {
        return text;
    }
    // 3. Emergency: return the key itself (should never happen for valid IDs)
    key
}

/// Convenience wrapper around [`tr`] that replaces `{key}` placeholders with
/// provided values.
///
/// Example:
/// ```ignore
/// tr_fmt(locale, MessageId::HelpUnknownCommand, &[("topic", "/foo")])
/// ```
/// is equivalent to:
/// ```ignore
/// tr(locale, MessageId::HelpUnknownCommand)
///     .replace("{topic}", "/foo")
/// ```
pub fn tr_fmt(locale: Locale, id: MessageId, args: &[(&str, &str)]) -> String {
    let mut s = tr(locale, id).to_string();
    for (key, value) in args {
        s = s.replace(&format!("{{{key}}}"), value);
    }
    s
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
        if value.contains("hant")
            || value.contains("-tw")
            || value.contains("-hk")
            || value.contains("-mo")
        {
            return None;
        }
        return Some(Locale::ZhHans);
    }
    if value.starts_with("pt") || value == "br" {
        return Some(Locale::PtBr);
    }
    None
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
        assert_eq!(normalize_configured_locale("zh-TW"), None);
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

    /// Check which MessageId keys are missing from a locale's string map.
    /// Returns a list of keys that are defined in `ALL_MESSAGE_IDS` but absent
    /// from the active locale strings.
    fn missing_message_ids(locale: Locale) -> Vec<&'static str> {
        let store = I18N.read().unwrap();
        let active_locale = store.active_locale;
        // Temporarily init the locale to check its coverage.
        drop(store);
        let prev_locale = I18N.read().unwrap().active_locale;
        init_locale(locale);
        let store = I18N.read().unwrap();
        let mut missing = Vec::new();
        for id in ALL_MESSAGE_IDS {
            let key = id.to_key();
            if !store.active_strings.contains_key(key) && !store.embedded_en.contains_key(key) {
                missing.push(key);
            }
        }
        drop(store);
        init_locale(prev_locale);
        missing
    }

    #[test]
    fn shipped_first_pack_has_no_missing_core_messages() {
        for locale in Locale::shipped() {
            let missing = missing_message_ids(*locale);
            assert!(
                missing.is_empty(),
                "{} is missing messages: {:?}",
                locale.tag(),
                missing
            );
        }
    }

    #[test]
    fn unsupported_locale_falls_back_to_english() {
        assert_eq!(
            resolve_locale_with_env("ar", |_| None),
            Locale::En,
            "Arabic is planned for QA but not shipped in the v0.7.6 core pack"
        );
    }


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
                "{tag} sample overflowed: {truncated:?}"
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
                "{label} sample produced an empty render"
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
