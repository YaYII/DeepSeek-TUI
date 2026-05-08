//! 子代理运行时协调的邮箱抽象。
//!
//! 单调递增的序列号为每个消费者提供一致的排序，即使
//! 多个订阅者（例如 UI 卡片 + 父级代理）独立消费时也是如此；
//! "关闭即取消"让一个信号既可以停止新邮件，也可以
//! 通过嵌套的子级传播取消。

// 这里的一些接口目前仅在此 crate 内部是生产者端，将在后续
// #128 的 UI 卡片中使用；在之前抑制死代码警告
// 而不是删除设计所依赖的能力。
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::models::Usage;

use super::SubAgentType;

/// 在子代理表面共享的稳定、结构化进度信封。
///
/// 端到端跟踪单个代理（由 `agent_id` 标识）的生命周期：
/// 生成、每步进度、工具执行、完成/失败/取消，
/// 以及父→子拓扑，以便消费者可以渲染树。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MailboxMessage {
    /// 代理已启动（后台任务正在运行）。
    Started {
        agent_id: String,
        agent_type: String,
    },
    /// 自由格式的人类可读进度（镜像 `Event::AgentProgress`）。
    Progress { agent_id: String, status: String },
    /// 代理内部的工具调用已开始。
    ToolCallStarted {
        agent_id: String,
        tool_name: String,
        step: u32,
    },
    /// 代理内部的工具调用已完成。
    ToolCallCompleted {
        agent_id: String,
        tool_name: String,
        step: u32,
        ok: bool,
    },
    /// 此代理生成了一个子代理。
    ChildSpawned { parent_id: String, child_id: String },
    /// 代理成功完成（携带显示在记录中的摘要行；
    /// 完整结果仍可通过 `agent_result` 获取）。
    Completed { agent_id: String, summary: String },
    /// 代理失败，携带错误消息。
    Failed { agent_id: String, error: String },
    /// 取消已传播到此代理。
    Cancelled { agent_id: String },
    /// 来自子代理 API 调用的增量 token 使用量。
    /// 每轮后发布，以便父级的成本计数器实时更新。
    TokenUsage {
        agent_id: String,
        /// 产生此使用量的模型，用于定价。
        model: String,
        /// 提供方使用量负载，包括缓存命中/未命中字段。
        usage: Usage,
    },
}

impl MailboxMessage {
    /// 消息主体的 `agent_id`（对于 `ChildSpawned` 这是子级，
    /// 因为这是正在宣布的新生命周期）。
    #[must_use]
    pub fn agent_id(&self) -> &str {
        match self {
            Self::Started { agent_id, .. }
            | Self::Progress { agent_id, .. }
            | Self::ToolCallStarted { agent_id, .. }
            | Self::ToolCallCompleted { agent_id, .. }
            | Self::Completed { agent_id, .. }
            | Self::Failed { agent_id, .. }
            | Self::Cancelled { agent_id }
            | Self::TokenUsage { agent_id, .. } => agent_id,
            Self::ChildSpawned { child_id, .. } => child_id,
        }
    }

    pub(crate) fn started(agent_id: impl Into<String>, agent_type: SubAgentType) -> Self {
        Self::Started {
            agent_id: agent_id.into(),
            agent_type: agent_type.as_str().to_string(),
        }
    }

    pub(crate) fn progress(agent_id: impl Into<String>, status: impl Into<String>) -> Self {
        Self::Progress {
            agent_id: agent_id.into(),
            status: status.into(),
        }
    }

    pub(crate) fn token_usage(
        agent_id: impl Into<String>,
        model: impl Into<String>,
        usage: Usage,
    ) -> Self {
        Self::TokenUsage {
            agent_id: agent_id.into(),
            model: model.into(),
            usage,
        }
    }
}

/// 一次投递：一个序列号加上消息。序列在整个邮箱上是
/// 单调递增的（不是按代理），因此即使多个子代理共享
/// 一个邮箱，单个排序也是明确定义的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxEnvelope {
    pub seq: u64,
    pub message: MailboxMessage,
}

/// 邮箱的发送端。
///
/// 可廉价克隆（内部所有内容都是 `Arc`/atomic）。克隆
/// `Mailbox` 共享相同的投递通道、序列计数器、watch
/// 通知器和关闭/取消状态——因此克隆了其父级 `Mailbox`
/// 的子运行时参与相同的流。
#[derive(Clone)]
pub struct Mailbox {
    inner: Arc<MailboxInner>,
}

struct MailboxInner {
    tx: mpsc::UnboundedSender<MailboxEnvelope>,
    next_seq: AtomicU64,
    seq_tx: watch::Sender<u64>,
    closed: AtomicBool,
    cancel_token: CancellationToken,
}

/// 邮箱的接收端。不可 `Clone`——只有原始创建者
/// 可以消费。使用 `Mailbox::subscribe()` 进行扇出
///（UI 卡片 + 父级都观察同一个流）。
pub struct MailboxReceiver {
    rx: mpsc::UnboundedReceiver<MailboxEnvelope>,
    pending: VecDeque<MailboxEnvelope>,
}

impl Mailbox {
    /// 创建一个绑定到给定取消令牌的新邮箱。关闭
    /// 邮箱（或丢弃最后一个发送者）会取消此令牌，
    /// 并通过 `child_token()` 传播到子级，如
    /// `SubAgentRuntime` 所述。
    #[must_use]
    pub fn new(cancel_token: CancellationToken) -> (Self, MailboxReceiver) {
        let (tx, rx) = mpsc::unbounded_channel();
        let (seq_tx, _) = watch::channel(0);
        let inner = MailboxInner {
            tx,
            next_seq: AtomicU64::new(0),
            seq_tx,
            closed: AtomicBool::new(false),
            cancel_token,
        };
        (
            Self {
                inner: Arc::new(inner),
            },
            MailboxReceiver {
                rx,
                pending: VecDeque::new(),
            },
        )
    }

    /// 订阅 seq 增加通知。每次 `recv()` 在序列计数器
    /// 前进时返回，表示有新邮件而无需复制它——
    /// 消费者随后在其自己的接收器上调用 `drain`
    ///（或 `recv_one`）。可能存在多个订阅者；
    /// 这是扇出原语。
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.inner.seq_tx.subscribe()
    }

    /// 发送消息；成功时返回 `Some(seq)`，如果邮箱
    /// 已关闭则返回 `None`（调用方应将其视为
    /// "接收者已消失，停止发布"）。
    pub fn send(&self, message: MailboxMessage) -> Option<u64> {
        if self.inner.closed.load(Ordering::Acquire) {
            return None;
        }
        let seq = self.inner.next_seq.fetch_add(1, Ordering::Relaxed) + 1;
        let envelope = MailboxEnvelope { seq, message };
        if self.inner.tx.send(envelope).is_err() {
            return None;
        }
        let _ = self.inner.seq_tx.send_replace(seq);
        Some(seq)
    }

    /// 邮箱是否已关闭。
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }

    /// 关闭邮箱并取消绑定的取消令牌。
    ///
    /// "关闭即取消"：不存在消费者已消失但子级应该继续生产的有用状态。
    /// 关闭父级的邮箱会级联到每个嵌套的子级，因为每个子级运行时
    /// 通过父级的 `child_token()` 派生其 `cancel_token`。
    pub fn close(&self) {
        if !self.inner.closed.swap(true, Ordering::AcqRel) {
            self.inner.cancel_token.cancel();
        }
    }
}

impl MailboxReceiver {
    fn sync_pending(&mut self) {
        while let Ok(env) = self.rx.try_recv() {
            self.pending.push_back(env);
        }
    }

    /// 是否有任何信封已缓冲（或自上次检查以来到达）。
    pub fn has_pending(&mut self) -> bool {
        self.sync_pending();
        !self.pending.is_empty()
    }

    /// 按投递顺序清空所有当前可用的信封。
    pub fn drain(&mut self) -> Vec<MailboxEnvelope> {
        self.sync_pending();
        self.pending.drain(..).collect()
    }

    /// 等待下一个信封，具有背压感知阻塞。当
    /// 所有发送者都已丢弃且缓冲区已清空时返回 `None`。
    pub async fn recv(&mut self) -> Option<MailboxEnvelope> {
        if let Some(env) = self.pending.pop_front() {
            return Some(env);
        }
        self.rx.recv().await
    }

    /// 带超时等待下一个信封。在测试中有用。
    #[allow(dead_code)]
    pub async fn recv_timeout(&mut self, timeout: Duration) -> Option<MailboxEnvelope> {
        tokio::time::timeout(timeout, self.recv())
            .await
            .ok()
            .flatten()
    }
}

/// 便捷句柄：一个邮箱 + 匹配的取消令牌，准备交给运行时。
/// 接收端存在于生成方一侧。
pub type SharedMailbox = Arc<Mutex<Option<MailboxReceiver>>>;

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    fn open() -> (Mailbox, MailboxReceiver, CancellationToken) {
        let token = CancellationToken::new();
        let (mb, rx) = Mailbox::new(token.clone());
        (mb, rx, token)
    }

    #[tokio::test]
    async fn mailbox_assigns_monotonic_sequence_numbers() {
        let (mb, _rx, _tok) = open();
        let s1 = mb
            .send(MailboxMessage::progress("a", "one"))
            .expect("seq 1");
        let s2 = mb
            .send(MailboxMessage::progress("a", "two"))
            .expect("seq 2");
        let s3 = mb
            .send(MailboxMessage::progress("b", "three"))
            .expect("seq 3");
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
        assert_eq!(s3, 3);
        assert!(s2 > s1 && s3 > s2);
    }

    #[tokio::test]
    async fn mailbox_drains_in_delivery_order() {
        let (mb, mut rx, _tok) = open();
        mb.send(MailboxMessage::progress("a", "first"));
        mb.send(MailboxMessage::progress("a", "second"));
        mb.send(MailboxMessage::Completed {
            agent_id: "a".into(),
            summary: "done".into(),
        });
        let drained = rx.drain();
        assert_eq!(drained.len(), 3);
        assert_eq!(drained[0].seq, 1);
        assert_eq!(drained[1].seq, 2);
        assert_eq!(drained[2].seq, 3);
        assert!(matches!(
            drained[0].message,
            MailboxMessage::Progress { .. }
        ));
        assert!(matches!(
            drained[2].message,
            MailboxMessage::Completed { .. }
        ));
        assert!(!rx.has_pending());
    }

    #[tokio::test]
    async fn subscribers_receive_seq_bumps_for_backpressure() {
        let (mb, _rx, _tok) = open();
        let mut sub_a = mb.subscribe();
        let mut sub_b = mb.subscribe();
        // 初始状态：两者都在 0。
        assert_eq!(*sub_a.borrow(), 0);
        assert_eq!(*sub_b.borrow(), 0);

        mb.send(MailboxMessage::progress("x", "tick"));
        sub_a.changed().await.expect("订阅者 a 看到增加");
        sub_b.changed().await.expect("订阅者 b 看到增加");
        assert_eq!(*sub_a.borrow(), 1);
        assert_eq!(*sub_b.borrow(), 1);

        // 第二次发送也会更新两个订阅者的 watch 值——即使
        // 它们共享单个 watch 通道，扇出也是 N 对多的。
        mb.send(MailboxMessage::progress("x", "tick2"));
        sub_a.changed().await.expect("a 看到第二次增加");
        assert_eq!(*sub_a.borrow(), 2);
    }

    #[tokio::test]
    async fn close_cancels_bound_token_and_blocks_further_sends() {
        let (mb, _rx, token) = open();
        assert!(!token.is_cancelled());
        mb.send(MailboxMessage::progress("a", "before close"));
        mb.close();
        assert!(token.is_cancelled(), "关闭即取消：令牌必须触发");
        assert!(mb.is_closed());
        // 后续发送是空操作，返回 None 而不是破坏 seq。
        assert!(
            mb.send(MailboxMessage::progress("a", "after close"))
                .is_none()
        );
    }

    #[tokio::test]
    async fn close_propagates_to_child_tokens_across_max_spawn_depth() {
        // 镜像运行时：根 → 子 → 孙（默认深度 3）。
        let root = CancellationToken::new();
        let child = root.child_token();
        let grandchild = child.child_token();
        let (mb, _rx) = Mailbox::new(root.clone());

        assert!(!child.is_cancelled());
        assert!(!grandchild.is_cancelled());
        mb.close();
        assert!(child.is_cancelled(), "子级继承根关闭");
        assert!(
            grandchild.is_cancelled(),
            "孙级也继承——覆盖默认 max_spawn_depth = 3"
        );
    }

    #[tokio::test]
    async fn recv_returns_envelope_then_none_after_close_and_drop() {
        let (mb, mut rx, _tok) = open();
        mb.send(MailboxMessage::progress("a", "queued"));
        let env = rx.recv().await.expect("缓冲的信封");
        assert_eq!(env.seq, 1);

        // 关闭并丢弃发送者后，recv 必须返回 None。
        mb.close();
        drop(mb);
        let next = rx.recv_timeout(Duration::from_millis(100)).await;
        assert!(next.is_none(), "已清空 + 已丢弃 → recv 返回 None");
    }

    #[tokio::test]
    async fn cloned_mailbox_shares_sequence_and_close_state() {
        let (mb, mut rx, token) = open();
        let mb_clone = mb.clone();
        let s1 = mb
            .send(MailboxMessage::progress("a", "from original"))
            .unwrap();
        let s2 = mb_clone
            .send(MailboxMessage::progress("a", "from clone"))
            .unwrap();
        assert_eq!(s1, 1);
        assert_eq!(s2, 2, "克隆共享序列计数器");

        let drained = rx.drain();
        assert_eq!(drained.len(), 2);

        // 通过一个克隆关闭会关闭所有克隆（AtomicBool 是共享的）。
        mb_clone.close();
        assert!(mb.is_closed());
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn agent_id_is_extractable_from_every_variant() {
        let cases: Vec<(MailboxMessage, &str)> = vec![
            (MailboxMessage::started("a1", SubAgentType::General), "a1"),
            (MailboxMessage::progress("a2", "x"), "a2"),
            (
                MailboxMessage::ToolCallStarted {
                    agent_id: "a3".into(),
                    tool_name: "read_file".into(),
                    step: 1,
                },
                "a3",
            ),
            (
                MailboxMessage::ToolCallCompleted {
                    agent_id: "a4".into(),
                    tool_name: "read_file".into(),
                    step: 1,
                    ok: true,
                },
                "a4",
            ),
            (
                MailboxMessage::ChildSpawned {
                    parent_id: "parent".into(),
                    child_id: "a5".into(),
                },
                "a5",
            ),
            (
                MailboxMessage::Completed {
                    agent_id: "a6".into(),
                    summary: "done".into(),
                },
                "a6",
            ),
            (
                MailboxMessage::Failed {
                    agent_id: "a7".into(),
                    error: "boom".into(),
                },
                "a7",
            ),
            (
                MailboxMessage::Cancelled {
                    agent_id: "a8".into(),
                },
                "a8",
            ),
            (
                MailboxMessage::TokenUsage {
                    agent_id: "a9".into(),
                    model: "deepseek-v4-flash".into(),
                    usage: Usage {
                        input_tokens: 100,
                        output_tokens: 50,
                        ..Default::default()
                    },
                },
                "a9",
            ),
        ];
        for (msg, expected) in cases {
            assert_eq!(msg.agent_id(), expected, "从 {msg:?} 提取失败");
        }
    }
}
