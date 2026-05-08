//! 每轮工具注册表设置。
//!
//! 这将模式/功能特定的注册表构建从发送路径中分离出来。

use std::path::Path;

use super::*;
use crate::sandbox::SandboxPolicy;

/// 选择针对给定 UI 模式限制 shell 命令的沙箱策略。
///
/// - **Plan** (#1077)：`ReadOnly` — 无写入，无网络。之前的
///   `WorkspaceWrite` 策略允许 `python -c "open('f','w').write('x')"` 修改
///   工作区内的文件，因为它将工作区列入可写白名单。Plan 模式仅为调查；
///   如果用户想要更改文件，应切换到 Agent。
/// - **Agent**：`WorkspaceWrite`，工作区为可写根目录，网络开启。
///   审批流程限制有风险的单个命令；沙箱处理其余部分。
///   网络允许，因为 cargo / npm / curl 类命令在代理工作中是正常的，
///   而 DNS 拒绝会静默破坏它们。
/// - **YOLO**：`DangerFullAccess` — 明确的无护栏契约。
pub(crate) fn sandbox_policy_for_mode(mode: AppMode, workspace: &Path) -> SandboxPolicy {
    match mode {
        AppMode::Plan => SandboxPolicy::ReadOnly,
        AppMode::Agent => SandboxPolicy::WorkspaceWrite {
            writable_roots: vec![workspace.to_path_buf()],
            network_access: true,
            exclude_tmpdir: false,
            exclude_slash_tmp: false,
        },
        AppMode::Yolo => SandboxPolicy::DangerFullAccess,
    }
}

impl Engine {
    pub(super) fn build_turn_tool_registry_builder(
        &self,
        mode: AppMode,
        todo_list: SharedTodoList,
        plan_state: SharedPlanState,
    ) -> ToolRegistryBuilder {
        let mut builder = if mode == AppMode::Plan {
            ToolRegistryBuilder::new()
                .with_read_only_file_tools()
                .with_search_tools()
                .with_git_tools()
                .with_git_history_tools()
                .with_diagnostics_tool()
                .with_skill_tools()
                .with_validation_tools()
                .with_runtime_task_tools()
                .with_todo_tool(todo_list)
                .with_plan_tool(plan_state)
        } else {
            ToolRegistryBuilder::new()
                .with_agent_tools(self.session.allow_shell)
                .with_todo_tool(todo_list)
                .with_plan_tool(plan_state)
        };

        builder = builder
            .with_review_tool(self.deepseek_client.clone(), self.session.model.clone())
            .with_rlm_tool(self.deepseek_client.clone(), self.session.model.clone())
            .with_fim_tool(self.deepseek_client.clone(), self.session.model.clone())
            .with_user_input_tool()
            .with_parallel_tool();

        if self.config.features.enabled(Feature::ApplyPatch) && mode != AppMode::Plan {
            builder = builder.with_patch_tools();
        }
        if self.config.features.enabled(Feature::WebSearch) {
            builder = builder.with_web_tools();
        }
        // Plan mode is strictly read-only: do not expose shell execution at
        // all, even if the session would otherwise allow it.
        if mode != AppMode::Plan
            && self.config.features.enabled(Feature::ShellTool)
            && self.session.allow_shell
        {
            builder = builder.with_shell_tools();
        }

        // Register the `remember` tool only when the user has opted in to
        // user-memory (#489). Without that opt-in the tool would always
        // fail; surfacing it would just waste catalog slots.
        if self.config.memory_enabled {
            builder = builder.with_remember_tool();
        }

        builder
    }
}
