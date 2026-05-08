//! 代理循环的审批 + 用户输入握手。
//!
//! 从 `core/engine.rs` 提取（P1.3）。代理循环在这两个
//! future 上阻塞：当工具需要显式审批时（`await_tool_approval`）
//! 或当工具请求实时用户输入时（`await_user_input`）。通道
//! 和引擎状态对父模块保持私有。

use crate::core::events::Event;
use crate::tools::spec::ToolError;
use crate::tools::user_input::{UserInputRequest, UserInputResponse};

use super::Engine;

#[derive(Debug, Clone)]
pub(super) enum ApprovalDecision {
    Approved {
        id: String,
    },
    Denied {
        id: String,
    },
    /// 以提升的沙箱策略重试工具。
    RetryWithPolicy {
        id: String,
        policy: crate::sandbox::SandboxPolicy,
    },
}

#[derive(Debug, Clone)]
pub(super) enum UserInputDecision {
    Submitted {
        id: String,
        response: UserInputResponse,
    },
    Cancelled {
        id: String,
    },
}

/// 等待用户工具审批的结果。
#[derive(Debug)]
pub(super) enum ApprovalResult {
    /// 用户批准了工具执行。
    Approved,
    /// 用户拒绝了工具执行。
    Denied,
    /// 用户请求以提升的沙箱策略重试。
    RetryWithPolicy(crate::sandbox::SandboxPolicy),
}

impl Engine {
    pub(super) async fn await_tool_approval(
        &mut self,
        tool_id: &str,
    ) -> Result<ApprovalResult, ToolError> {
        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    return Err(ToolError::execution_failed(
                        "等待审批时请求已取消".to_string(),
                    ));
                }
                decision = self.rx_approval.recv() => {
                    let Some(decision) = decision else {
                        return Err(ToolError::execution_failed(
                            "审批通道已关闭".to_string(),
                        ));
                    };
                    match decision {
                        ApprovalDecision::Approved { id } if id == tool_id => {
                            return Ok(ApprovalResult::Approved);
                        }
                        ApprovalDecision::Denied { id } if id == tool_id => {
                            return Ok(ApprovalResult::Denied);
                        }
                        ApprovalDecision::RetryWithPolicy { id, policy } if id == tool_id => {
                            return Ok(ApprovalResult::RetryWithPolicy(policy));
                        }
                        _ => continue,
                    }
                }
            }
        }
    }

    pub(super) async fn await_user_input(
        &mut self,
        tool_id: &str,
        request: UserInputRequest,
    ) -> Result<UserInputResponse, ToolError> {
        let _ = self
            .tx_event
            .send(Event::UserInputRequired {
                id: tool_id.to_string(),
                request,
            })
            .await;

        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    return Err(ToolError::execution_failed(
                        "等待用户输入时请求已取消".to_string(),
                    ));
                }
                decision = self.rx_user_input.recv() => {
                    let Some(decision) = decision else {
                        return Err(ToolError::execution_failed(
                            "用户输入通道已关闭".to_string(),
                        ));
                    };
                    match decision {
                        UserInputDecision::Submitted { id, response } if id == tool_id => {
                            return Ok(response);
                        }
                        UserInputDecision::Cancelled { id } if id == tool_id => {
                            return Err(ToolError::execution_failed(
                                "用户输入已取消".to_string(),
                            ));
                        }
                        _ => continue,
                    }
                }
            }
        }
    }
}
