pub mod bash_arity;

use std::collections::HashSet;

use anyhow::Result;
use bash_arity::BashArityDict;
use deepseek_protocol::{NetworkPolicyAmendment, NetworkPolicyRuleAction};
use serde::{Deserialize, Serialize};

/// 权限规则集的优先级层级。序号越高优先级越高。
/// 冲突时，最高优先级层级的最长匹配前缀获胜。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RulesetLayer {
    BuiltinDefault = 0,
    Agent = 1,
    User = 2,
}

/// 在给定优先级层级上的一组命名允许/拒绝前缀规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ruleset {
    pub layer: RulesetLayer,
    pub trusted_prefixes: Vec<String>,
    pub denied_prefixes: Vec<String>,
}

impl Ruleset {
    pub fn builtin_default() -> Self {
        Self {
            layer: RulesetLayer::BuiltinDefault,
            trusted_prefixes: vec![],
            denied_prefixes: vec![],
        }
    }

    pub fn agent(trusted: Vec<String>, denied: Vec<String>) -> Self {
        Self {
            layer: RulesetLayer::Agent,
            trusted_prefixes: trusted,
            denied_prefixes: denied,
        }
    }

    pub fn user(trusted: Vec<String>, denied: Vec<String>) -> Self {
        Self {
            layer: RulesetLayer::User,
            trusted_prefixes: trusted,
            denied_prefixes: denied,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AskForApproval {
    UnlessTrusted,
    OnFailure,
    OnRequest,
    Reject {
        sandbox_approval: bool,
        rules: bool,
        mcp_elicitations: bool,
    },
    Never,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecPolicyAmendment {
    pub prefixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecApprovalRequirement {
    Skip {
        bypass_sandbox: bool,
        proposed_execpolicy_amendment: Option<ExecPolicyAmendment>,
    },
    NeedsApproval {
        reason: String,
        proposed_execpolicy_amendment: Option<ExecPolicyAmendment>,
        proposed_network_policy_amendments: Vec<NetworkPolicyAmendment>,
    },
    Forbidden {
        reason: String,
    },
}

impl ExecApprovalRequirement {
    pub fn reason(&self) -> &str {
        match self {
            ExecApprovalRequirement::Skip { .. } => "策略允许执行。",
            ExecApprovalRequirement::NeedsApproval { reason, .. } => reason,
            ExecApprovalRequirement::Forbidden { reason } => reason,
        }
    }

    pub fn phase(&self) -> &'static str {
        match self {
            ExecApprovalRequirement::Skip { .. } => "allowed",
            ExecApprovalRequirement::NeedsApproval { .. } => "needs_approval",
            ExecApprovalRequirement::Forbidden { .. } => "forbidden",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecPolicyDecision {
    pub allow: bool,
    pub requires_approval: bool,
    pub requirement: ExecApprovalRequirement,
    pub matched_rule: Option<String>,
}

impl ExecPolicyDecision {
    pub fn reason(&self) -> &str {
        self.requirement.reason()
    }
}

#[derive(Debug, Clone)]
pub struct ExecPolicyContext<'a> {
    pub command: &'a str,
    pub cwd: &'a str,
    pub ask_for_approval: AskForApproval,
    pub sandbox_mode: Option<&'a str>,
}

#[derive(Debug, Clone, Default)]
pub struct ExecPolicyEngine {
    /// 分层规则集（内置 → 代理 → 用户）。非空时优先于下面的旧式平面列表。
    rulesets: Vec<Ruleset>,
    /// 为向后兼容 `new()` 保留的旧式平面列表。
    trusted_prefixes: Vec<String>,
    denied_prefixes: Vec<String>,
    approved_for_session: HashSet<String>,
    /// 用于命令前缀允许规则匹配的参数词典。
    arity_dict: BashArityDict,
}

impl ExecPolicyEngine {
    /// 旧式构造函数：将两个 vec 包装为 User 层级规则集。
    pub fn new(trusted_prefixes: Vec<String>, denied_prefixes: Vec<String>) -> Self {
        Self {
            rulesets: vec![],
            trusted_prefixes,
            denied_prefixes,
            approved_for_session: HashSet::new(),
            arity_dict: BashArityDict::new(),
        }
    }

    /// 从显式的分层规则集构建引擎。
    /// 构造时按层级优先级排序。
    pub fn with_rulesets(mut rulesets: Vec<Ruleset>) -> Self {
        rulesets.sort_by_key(|r| r.layer);
        Self {
            rulesets,
            trusted_prefixes: vec![],
            denied_prefixes: vec![],
            approved_for_session: HashSet::new(),
            arity_dict: BashArityDict::new(),
        }
    }

    /// 添加一个规则集层级（内部会重新排序）。
    pub fn add_ruleset(&mut self, ruleset: Ruleset) {
        self.rulesets.push(ruleset);
        self.rulesets.sort_by_key(|r| r.layer);
    }

    /// 通过合并所有规则集来解析有效的受信任/拒绝前缀集合。
    ///
    /// 将每个层级（内置 → 代理 → 用户）的所有前缀收集到平面
    /// 受信任/拒绝列表中。然后 `check()` 方法应用拒绝始终优先的语义：
    /// 任何匹配的拒绝前缀都会阻止该命令，无论层级如何。
    /// 只有在拒绝检查通过后才查询受信任规则。
    fn resolve_prefixes(&self) -> (Vec<String>, Vec<String>) {
        if self.rulesets.is_empty() {
            return (self.trusted_prefixes.clone(), self.denied_prefixes.clone());
        }
        // 收集所有层级的 trusted/denied，最高优先级最后，使它们遮盖
        // 具有相同前缀的低优先级条目。
        let mut trusted: Vec<String> = vec![];
        let mut denied: Vec<String> = vec![];
        for rs in &self.rulesets {
            trusted.extend(rs.trusted_prefixes.iter().cloned());
            denied.extend(rs.denied_prefixes.iter().cloned());
        }
        // 同时合并旧式平面列表作为用户层级。
        trusted.extend(self.trusted_prefixes.iter().cloned());
        denied.extend(self.denied_prefixes.iter().cloned());
        (trusted, denied)
    }

    pub fn remember_session_approval(&mut self, approval_key: String) {
        self.approved_for_session.insert(approval_key);
    }

    pub fn is_session_approved(&self, approval_key: &str) -> bool {
        self.approved_for_session.contains(approval_key)
    }

    pub fn check(&self, ctx: ExecPolicyContext<'_>) -> Result<ExecPolicyDecision> {
        let normalized = normalize_command(ctx.command);
        let (trusted_prefixes, denied_prefixes) = self.resolve_prefixes();
        // 拒绝规则使用简单前缀匹配（无需 arity 语义）。
        if let Some(rule) = denied_prefixes
            .iter()
            .find(|rule| normalized.starts_with(&normalize_command(rule)))
        {
            return Ok(ExecPolicyDecision {
                allow: false,
                requires_approval: false,
                matched_rule: Some(rule.clone()),
                requirement: ExecApprovalRequirement::Forbidden {
                    reason: format!("命令被拒绝前缀规则 '{rule}' 阻止"),
                },
            });
        }

        // 允许（受信任）规则使用 arity 感知的前缀匹配，以便
        // `auto_allow = ["git status"]` 匹配 `git status -s` 但
        // 不匹配 `git push origin main`。
        let trusted_rule = trusted_prefixes
            .iter()
            .find(|rule| self.arity_dict.allow_rule_matches(rule, ctx.command))
            .cloned();
        let is_trusted = trusted_rule.is_some();

        let requirement = match ctx.ask_for_approval {
            AskForApproval::Never => ExecApprovalRequirement::Skip {
                bypass_sandbox: false,
                proposed_execpolicy_amendment: None,
            },
            AskForApproval::UnlessTrusted if is_trusted => ExecApprovalRequirement::Skip {
                bypass_sandbox: false,
                proposed_execpolicy_amendment: None,
            },
            AskForApproval::OnFailure => ExecApprovalRequirement::Skip {
                bypass_sandbox: false,
                proposed_execpolicy_amendment: None,
            },
            AskForApproval::Reject { rules, .. } if rules => ExecApprovalRequirement::Forbidden {
                reason: "策略配置为拒绝规则例外。".to_string(),
            },
            _ => ExecApprovalRequirement::NeedsApproval {
                reason: if is_trusted {
                    "策略模式请求审批。".to_string()
                } else {
                    "未匹配的命令前缀需要审批。".to_string()
                },
                proposed_execpolicy_amendment: if is_trusted {
                    None
                } else {
                    Some(ExecPolicyAmendment {
                        prefixes: vec![first_token(ctx.command)],
                    })
                },
                proposed_network_policy_amendments: vec![NetworkPolicyAmendment {
                    host: ctx.cwd.to_string(),
                    action: NetworkPolicyRuleAction::Allow,
                }],
            },
        };

        let (allow, requires_approval) = match requirement {
            ExecApprovalRequirement::Skip { .. } => (true, false),
            ExecApprovalRequirement::NeedsApproval { .. } => (true, true),
            ExecApprovalRequirement::Forbidden { .. } => (false, false),
        };

        Ok(ExecPolicyDecision {
            allow,
            requires_approval,
            matched_rule: trusted_rule,
            requirement,
        })
    }
}

fn normalize_command(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn first_token(command: &str) -> String {
    command
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string()
}
