//! Cerebrate 脑虫记忆中枢客户端。
//!
//! Cerebrate 是 AI 智能体群体记忆服务端。本模块提供异步 HTTP 客户端，
//! 在 DeepSeek TUI 会话生命周期中自动查询群体记忆和提交经验。
//!
//! # 集成点
//! - 会话开始 → `sense()` 健康检查 + `query()` 搜索相关经验
//! - 会话结束 → `propose()` 提交本次经验
//! - 可选触发 → `evolve()` 脑虫进化

use crate::config::CerebrateConfig;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Cerebrate API 响应结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CerebrateResponse {
    pub status: String,
    pub data: Option<serde_json::Value>,
    pub error: Option<CerebrateError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CerebrateError {
    pub code: i32,
    pub message: String,
}

/// 查询结果中的单条记忆
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SwarmMemory {
    #[serde(default)]
    pub memory_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub solution: String,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub reuse_count: u32,
    #[serde(default)]
    pub life_stage: String,
}

/// 查询响应
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueryResult {
    #[serde(default)]
    pub found: bool,
    #[serde(default)]
    pub recommendation: String,
    pub swarm_result: Option<SwarmMemory>,
    pub task: Option<serde_json::Value>,
}

/// 提议响应
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProposeResult {
    #[serde(default)]
    pub memory_id: String,
    #[serde(default)]
    pub life_stage: String,
}

/// Cerebrate HTTP 客户端。
///
/// 所有方法均为异步，失败时静默返回 `None`（不阻塞 TUI 运行）。
#[derive(Clone)]
pub struct CerebrateClient {
    client: Client,
    base_url: String,
    agent_id: String,
}

impl CerebrateClient {
    /// 从配置创建客户端。若 Cerebrate 未启用，返回 `None`。
    pub fn from_config(cfg: &CerebrateConfig) -> Option<Self> {
        if !cfg.enabled {
            return None;
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .ok()?;
        // 去掉尾部斜杠
        let base_url = cfg.url.trim_end_matches('/').to_string();
        Some(Self {
            client,
            base_url,
            agent_id: cfg.agent_id.clone(),
        })
    }

    /// 健康检查 — 验证服务端是否可达。
    pub async fn sense(&self) -> Option<serde_json::Value> {
        let url = format!("{}/v1/sense", self.base_url);
        let resp = self.client.get(&url).send().await.ok()?;
        let body: CerebrateResponse = resp.json().await.ok()?;
        if body.status == "ok" {
            body.data
        } else {
            tracing::warn!("Cerebrate sense failed: {:?}", body.error);
            None
        }
    }

    /// 查询群体记忆 — 搜索相关历史经验。
    pub async fn query(&self, query_text: &str) -> Option<QueryResult> {
        let url = format!("{}/v1/query", self.base_url);
        let payload = serde_json::json!({
            "query": query_text,
            "agent_id": self.agent_id,
        });
        let resp = self.client.post(&url).json(&payload).send().await.ok()?;
        let body: CerebrateResponse = resp.json().await.ok()?;
        if body.status == "ok" {
            let data = body.data?;
            serde_json::from_value(data).ok()
        } else {
            None
        }
    }

    /// 提交候选记忆 — 分享本次会话经验。
    pub async fn propose(
        &self,
        title: &str,
        content: &str,
        category: &str,
        tags: &[String],
        problem: &str,
        solution: &str,
    ) -> Option<ProposeResult> {
        let url = format!("{}/v1/memories/propose", self.base_url);
        let payload = serde_json::json!({
            "title": title,
            "content": content,
            "category": category,
            "tags": tags,
            "agent_id": self.agent_id,
            "problem": problem,
            "solution": solution,
        });
        let resp = self.client.post(&url).json(&payload).send().await.ok()?;
        let body: CerebrateResponse = resp.json().await.ok()?;
        if body.status == "ok" {
            let data = body.data?;
            serde_json::from_value(data).ok()
        } else {
            None
        }
    }

    /// 触发脑虫进化 — 去重 + 技能提炼 + 教条固化。
    pub async fn evolve(&self) -> Option<serde_json::Value> {
        let url = format!("{}/v1/evolve", self.base_url);
        let resp = self.client.post(&url).json(&serde_json::json!({})).send().await.ok()?;
        let body: CerebrateResponse = resp.json().await.ok()?;
        if body.status == "ok" {
            body.data
        } else {
            None
        }
    }

    /// 构建查询文本：从当前会话上下文提取关键词。
    pub fn build_context_query(workspace: &str, mode: &str) -> String {
        // 从工作区路径提取项目名
        let project = std::path::Path::new(workspace)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        format!("{project} 项目 {mode} 模式常用经验")
    }

    /// 检查服务端是否可达（带超时）。
    pub async fn is_reachable(&self) -> bool {
        self.sense().await.is_some()
    }
}
