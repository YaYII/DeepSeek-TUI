//! `DeepSeek` CLI 的核心引擎模块。
//!
//! 本模块提供事件驱动架构，将 UI 与 AI 交互逻辑分离：
//!
//! - `engine`：处理操作的主引擎
//! - `events`：引擎向 UI 发出的事件
//! - `ops`：UI 向引擎提交的操作
//! - `session`：会话状态管理
//! - `turn`：轮次上下文与追踪

pub mod capacity;
pub mod capacity_memory;
pub mod coherence;
pub mod engine;
pub mod events;
pub mod ops;
pub mod session;
pub mod tool_parser;
pub mod turn;

// Re-exports
