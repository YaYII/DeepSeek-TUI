//! 从容量事件派生的自然语言会话一致性状态。

use serde::{Deserialize, Serialize};

use crate::core::capacity::{GuardrailAction, RiskBand};

/// 面向用户的会话健康一致性阶梯。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoherenceState {
    #[default]
    Healthy,
    GettingCrowded,
    RefreshingContext,
    VerifyingRecentWork,
    ResettingPlan,
}

impl CoherenceState {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Healthy => "健康",
            Self::GettingCrowded => "变得拥挤",
            Self::RefreshingContext => "刷新上下文",
            Self::VerifyingRecentWork => "验证最近工作",
            Self::ResettingPlan => "重置计划",
        }
    }

    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::Healthy => "会话稳定且专注。",
            Self::GettingCrowded => "会话正在接近上下文压力。",
            Self::RefreshingContext => "引擎正在继续之前刷新上下文。",
            Self::VerifyingRecentWork => {
                "引擎正在继续之前检查最近的工具结果。"
            }
            Self::ResettingPlan => {
                "引擎正在从规范上下文重建并重新规划。"
            }
        }
    }
}

/// 一致性归约器的合成输入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoherenceSignal {
    CapacityDecision {
        risk_band: RiskBand,
        action: GuardrailAction,
        cooldown_blocked: bool,
    },
    CapacityIntervention {
        action: GuardrailAction,
    },
    CompactionStarted,
    CompactionCompleted,
    CompactionFailed,
}

/// 自然语言一致性阶梯的纯转换函数。
#[must_use]
pub fn next_coherence_state(current: CoherenceState, signal: CoherenceSignal) -> CoherenceState {
    match signal {
        CoherenceSignal::CompactionStarted => CoherenceState::RefreshingContext,
        CoherenceSignal::CompactionCompleted => CoherenceState::Healthy,
        CoherenceSignal::CompactionFailed => CoherenceState::GettingCrowded,
        CoherenceSignal::CapacityIntervention { action }
        | CoherenceSignal::CapacityDecision { action, .. } => match action {
            GuardrailAction::NoIntervention => match signal {
                CoherenceSignal::CapacityDecision {
                    risk_band,
                    cooldown_blocked,
                    ..
                } => {
                    if cooldown_blocked {
                        return current;
                    }
                    match risk_band {
                        RiskBand::Low => CoherenceState::Healthy,
                        RiskBand::Medium | RiskBand::High => CoherenceState::GettingCrowded,
                    }
                }
                _ => current,
            },
            GuardrailAction::TargetedContextRefresh => CoherenceState::RefreshingContext,
            GuardrailAction::VerifyWithToolReplay => CoherenceState::VerifyingRecentWork,
            GuardrailAction::VerifyAndReplan => CoherenceState::ResettingPlan,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_capacity_event_log_drives_plain_language_ladder() {
        let log = [
            CoherenceSignal::CapacityDecision {
                risk_band: RiskBand::Low,
                action: GuardrailAction::NoIntervention,
                cooldown_blocked: false,
            },
            CoherenceSignal::CapacityDecision {
                risk_band: RiskBand::Medium,
                action: GuardrailAction::NoIntervention,
                cooldown_blocked: false,
            },
            CoherenceSignal::CapacityDecision {
                risk_band: RiskBand::Medium,
                action: GuardrailAction::TargetedContextRefresh,
                cooldown_blocked: false,
            },
            CoherenceSignal::CompactionCompleted,
            CoherenceSignal::CapacityDecision {
                risk_band: RiskBand::High,
                action: GuardrailAction::VerifyWithToolReplay,
                cooldown_blocked: false,
            },
            CoherenceSignal::CapacityDecision {
                risk_band: RiskBand::High,
                action: GuardrailAction::VerifyAndReplan,
                cooldown_blocked: false,
            },
        ];

        let mut state = CoherenceState::Healthy;
        let mut states = Vec::new();
        for signal in log {
            state = next_coherence_state(state, signal);
            states.push(state);
        }

        assert_eq!(
            states,
            vec![
                CoherenceState::Healthy,
                CoherenceState::GettingCrowded,
                CoherenceState::RefreshingContext,
                CoherenceState::Healthy,
                CoherenceState::VerifyingRecentWork,
                CoherenceState::ResettingPlan,
            ]
        );
    }
}
