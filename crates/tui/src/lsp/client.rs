//! 针对 LSP 服务器的基于 stdio 的轻量 JSON-RPC 客户端。
//!
//! 我们特意**不**依赖 `tower-lsp`——它是一个服务端
//! 框架，引入它会增加数百个不必要的
//! 传递依赖，并拖慢每个贡献者的 `cargo build`。
//! LSP 线协议足够小，自行处理只需约 400 行代码，
//! 并让我们完全控制 spawn 生命周期、超时和异步表面。
//!
//! 架构：
//!
//! - [`LspTransport`] 是 [`super::LspManager`] 与之交互的 trait。
//!   真正的实现是 [`StdioLspTransport`]（用 `tokio::process::Command` 创建 LSP 服务器进程）；
//!   测试使用 `super::tests::FakeTransport`。
//! - [`StdioLspTransport`] 运行三个 tokio 任务：一个读取器、一个写入器和
//!   公共 API。通信使用 tokio mpsc 通道。
//! - 我们解析 `Content-Length` 帧格式的 JSON-RPC，并将入站消息路由到
//!   每个请求的响应槽（用于回复）或
//!   诊断队列（用于 `textDocument/publishDiagnostics` 通知）。
//!
//! 在 MVP 形式中，传输是每个文件一次性使用：管理器按需为某种语言
//! 生成一个传输并重复使用它。我们不实现
//! 超出 didOpen/didChange 的工作区同步，因为目标是"编辑后诊断"，
//! 而非完整的 IDE 智能。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use super::diagnostics::{Diagnostic, Severity};
use super::registry::Language;
use crate::utils::spawn_supervised;

/// LSP 管理器与之交互的 trait。真正的 LSP 服务器通过 stdio 使用此协议；
/// 测试使用进程内的模拟实现。
#[async_trait]
pub trait LspTransport: Send + Sync {
    /// 通知服务器某个文件已打开或其内容已更新，然后
    /// 最多等待 `wait` 时长，等待该文件的 `publishDiagnostics` 通知。
    /// 返回诊断列表（可能为空）。实现
    /// 不得阻塞超过 `wait`。
    async fn diagnostics_for(
        &self,
        path: &Path,
        text: &str,
        wait: Duration,
    ) -> Result<Vec<Diagnostic>>;

    /// 尽力关闭。通过 `LspManager::shutdown_all` 调用。
    #[allow(dead_code)]
    async fn shutdown(&self);
}

/// 基于 stdio 的传输。将 LSP 服务器作为子进程生成，并通过
/// stdin/stdout 传输 JSON-RPC。Stderr 被捕获到缓冲区，以便
/// 调用方可以在错误消息中包含它，而不会污染我们自己的 stderr。
pub struct StdioLspTransport {
    /// 正在运行的服务器的 JoinHandle。持有它以确保子进程在
    /// 传输的生命周期内保持存活；在 `shutdown` 期间消耗。
    #[allow(dead_code)]
    child: AsyncMutex<Option<Child>>,
    /// 发送到写入器任务的出站消息发送器。
    tx_outbound: mpsc::Sender<Vec<u8>>,
    /// 入站诊断队列。我们将每个 `publishDiagnostics`
    /// 通知推送到这里，公共 API 从中取出相关条目。
    diagnostics_rx: AsyncMutex<mpsc::Receiver<(PathBuf, Vec<Diagnostic>)>>,
    /// 正在进行的请求 id -> 回复槽的映射。我们目前不调用
    /// `initialize` 后需要回复的方法，但这是为它准备的挂钩。
    #[allow(dead_code)]
    pending: Arc<AsyncMutex<HashMap<i64, oneshot::Sender<Value>>>>,
    /// 单调递增的请求 id 计数器。为将来的 LSP 请求/回复方法
    ///（工作区符号查询等）保留。
    #[allow(dead_code)]
    next_id: AsyncMutex<i64>,
    /// 在 `textDocument/didOpen` 中传递的语言 ID（例如 "rust"）。
    language_id: &'static str,
    /// 跟踪我们已打开的文件，以便第二次操作发送
    /// `didChange` 而不是 `didOpen`。
    opened: AsyncMutex<HashMap<PathBuf, i64>>,
}

impl StdioLspTransport {
    /// 生成 `command args…` 并运行 LSP `initialize` 握手。如果
    /// 二进制文件不在 PATH 上或 `initialize` 失败，立即返回 `Err`。
    pub async fn spawn(
        command: &str,
        args: &[String],
        language: Language,
        workspace: PathBuf,
    ) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .with_context(|| format!("failed to spawn LSP server `{command}`"))?;

        let stdin = child
            .stdin
            .take()
            .context("LSP child has no stdin handle")?;
        let stdout = child
            .stdout
            .take()
            .context("LSP child has no stdout handle")?;

        let (tx_outbound, rx_outbound) = mpsc::channel::<Vec<u8>>(64);
        let (tx_inbound, rx_inbound) = mpsc::channel::<Value>(64);
        let (tx_diag, rx_diag) = mpsc::channel::<(PathBuf, Vec<Diagnostic>)>(64);

        // 写入器任务：清空出站通道，用 Content-Length 组帧，写入 stdin。
        spawn_supervised(
            "lsp-writer",
            std::panic::Location::caller(),
            writer_task(stdin, rx_outbound),
        );
        // 读取器任务：从 stdout 解析 Content-Length 帧，推送到入站队列。
        spawn_supervised(
            "lsp-reader",
            std::panic::Location::caller(),
            reader_task(stdout, tx_inbound),
        );
        // 入站分发器：将通知路由到 `tx_diag`，将回复路由到
        // pending 映射。为完整性保留 pending 映射，尽管
        // 诊断轮询本身不重用它。
        let pending: Arc<AsyncMutex<HashMap<i64, oneshot::Sender<Value>>>> =
            Arc::new(AsyncMutex::new(HashMap::new()));
        spawn_supervised(
            "lsp-dispatcher",
            std::panic::Location::caller(),
            dispatcher_task(rx_inbound, tx_diag, pending.clone()),
        );

        // 发送 `initialize` 并等待 `initialized`。我们使用 id=1。
        let init_payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": uri_from_path(&workspace),
                "capabilities": {
                    "textDocument": {
                        "publishDiagnostics": { "relatedInformation": false }
                    }
                },
                "workspaceFolders": [{
                    "uri": uri_from_path(&workspace),
                    "name": "workspace"
                }]
            }
        });
        send_message(&tx_outbound, &init_payload).await?;

        // 在 MVP 中我们实际上不等待 initialize 响应——
        // 大多数服务器会缓冲通知直到它们准备就绪，等待
        // `initialize` 回复会使第一次编辑的延迟加倍。立即发送
        // `initialized`，让 publishDiagnostics 按自己的节奏到达。
        let initialized = json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });
        send_message(&tx_outbound, &initialized).await?;

        Ok(Self {
            child: AsyncMutex::new(Some(child)),
            tx_outbound,
            diagnostics_rx: AsyncMutex::new(rx_diag),
            pending,
            next_id: AsyncMutex::new(2),
            language_id: language.language_id(),
            opened: AsyncMutex::new(HashMap::new()),
        })
    }
}

#[async_trait]
impl LspTransport for StdioLspTransport {
    async fn diagnostics_for(
        &self,
        path: &Path,
        text: &str,
        wait: Duration,
    ) -> Result<Vec<Diagnostic>> {
        let path_buf = path.to_path_buf();
        let uri = uri_from_path(&path_buf);

        // 要么发送 didOpen（第一次），要么发送 didChange（后续编辑）。
        let mut opened = self.opened.lock().await;
        let is_new = !opened.contains_key(&path_buf);
        let new_version = opened.get(&path_buf).copied().unwrap_or(0) + 1;
        opened.insert(path_buf.clone(), new_version);
        drop(opened);

        let payload = if is_new {
            json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": uri.clone(),
                        "languageId": self.language_id,
                        "version": new_version,
                        "text": text
                    }
                }
            })
        } else {
            json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didChange",
                "params": {
                    "textDocument": {
                        "uri": uri.clone(),
                        "version": new_version
                    },
                    "contentChanges": [{ "text": text }]
                }
            })
        };
        send_message(&self.tx_outbound, &payload).await?;

        // 清空匹配的 `publishDiagnostics` 通知，直到 `wait`
        // 过期。服务器通常在几百毫秒内发布；对于
        // 初始冷启动（rust-analyzer）可能需要数秒——但
        // 管理器用单独的超时保护我们。
        let deadline = tokio::time::Instant::now() + wait;
        let mut latest: Option<Vec<Diagnostic>> = None;

        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline - now;
            let mut rx = self.diagnostics_rx.lock().await;
            let next = match timeout(remaining, rx.recv()).await {
                Ok(Some(item)) => item,
                Ok(None) => break, // channel closed
                Err(_) => break,   // timed out
            };
            drop(rx);
            let (file, items) = next;
            if file == path_buf {
                latest = Some(items);
                // 我们有了一个负载——立即返回。如果服务器
                // 在快速编辑后重新发布，下一次调用将同步。
                break;
            }
            // 否则：通知是针对我们之前打开的不同文件。
            // 丢弃并继续等待。
        }
        Ok(latest.unwrap_or_default())
    }

    async fn shutdown(&self) {
        let mut child = self.child.lock().await;
        if let Some(mut c) = child.take() {
            let _ = c.start_kill();
            let _ = c.wait().await;
        }
    }
}

/// 发送一个 JSON 值作为一条 Content-Length 帧格式的 JSON-RPC 消息。
async fn send_message(tx: &mpsc::Sender<Vec<u8>>, value: &Value) -> Result<()> {
    let body = serde_json::to_vec(value).context("serialize LSP message")?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut frame = Vec::with_capacity(header.len() + body.len());
    frame.extend_from_slice(header.as_bytes());
    frame.extend_from_slice(&body);
    tx.send(frame)
        .await
        .map_err(|_| anyhow!("LSP outbound channel closed"))?;
    Ok(())
}

/// 后台任务：清空出站队列并将每个帧写入 LSP 服务器的 stdin。
/// 通道关闭时干净退出。
async fn writer_task(mut stdin: tokio::process::ChildStdin, mut rx: mpsc::Receiver<Vec<u8>>) {
    while let Some(frame) = rx.recv().await {
        if stdin.write_all(&frame).await.is_err() {
            break;
        }
        if stdin.flush().await.is_err() {
            break;
        }
    }
}

/// 后台任务：解析来自 LSP 服务器 stdout 的 `Content-Length` 帧格式 JSON-RPC 消息。
/// 将每个解析后的 JSON 值推送到 `tx`。当 stdout 关闭或
/// 帧格式错误时退出（我们选择失败关闭而不是冒险挂起）。
async fn reader_task(mut stdout: tokio::process::ChildStdout, tx: mpsc::Sender<Value>) {
    let mut buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut tmp = [0u8; 4096];
    loop {
        let n = match stdout.read(&mut tmp).await {
            Ok(0) => return,
            Ok(n) => n,
            Err(_) => return,
        };
        buf.extend_from_slice(&tmp[..n]);
        // Try to parse as many frames as we can from the accumulated buffer.
        while let Some((header_end, content_length)) = parse_header(&buf) {
            if buf.len() < header_end + content_length {
                break; // need more bytes
            }
            let body = &buf[header_end..header_end + content_length];
            let parsed = serde_json::from_slice::<Value>(body).ok();
            // Drop the consumed bytes regardless of parse result so a bad frame
            // does not stall the loop.
            buf.drain(..header_end + content_length);
            if let Some(value) = parsed
                && tx.send(value).await.is_err()
            {
                return;
            }
        }
    }
}

/// 解析 JSON-RPC 头部块。返回 `Some((header_end, content_length))`，
/// 其中 `header_end` 是第一个消息体字节的偏移量。头部
/// 终止符是 `\r\n\r\n`。我们需要一个 `Content-Length` 头部。
fn parse_header(buf: &[u8]) -> Option<(usize, usize)> {
    let term = b"\r\n\r\n";
    let pos = buf.windows(term.len()).position(|window| window == term)?;
    let header = std::str::from_utf8(&buf[..pos]).ok()?;
    let mut content_length: Option<usize> = None;
    for line in header.split("\r\n") {
        if let Some(rest) = line.strip_prefix("Content-Length:") {
            content_length = rest.trim().parse::<usize>().ok();
        }
    }
    content_length.map(|cl| (pos + term.len(), cl))
}

/// 后台任务：消费入站的 JSON 值，将其分类为
/// 通知/响应，并进行相应路由。
async fn dispatcher_task(
    mut rx: mpsc::Receiver<Value>,
    tx_diag: mpsc::Sender<(PathBuf, Vec<Diagnostic>)>,
    pending: Arc<AsyncMutex<HashMap<i64, oneshot::Sender<Value>>>>,
) {
    while let Some(value) = rx.recv().await {
        // 通知有 `method` 但没有 `id`。
        let method = value.get("method").and_then(|v| v.as_str());
        if method == Some("textDocument/publishDiagnostics") {
            if let Some((path, diags)) = parse_publish_diagnostics(&value) {
                let _ = tx_diag.send((path, diags)).await;
            }
            continue;
        }
        // 回复有 `id` 以及 `result` 或 `error`。
        if let Some(id) = value.get("id").and_then(|v| v.as_i64()) {
            let mut map = pending.lock().await;
            if let Some(slot) = map.remove(&id) {
                let _ = slot.send(value);
            }
        }
    }
}

/// 解码 `textDocument/publishDiagnostics` 通知。
fn parse_publish_diagnostics(value: &Value) -> Option<(PathBuf, Vec<Diagnostic>)> {
    let params = value.get("params")?;
    let uri = params.get("uri")?.as_str()?;
    let path = path_from_uri(uri)?;
    let raw = params.get("diagnostics")?.as_array()?;
    let mut out = Vec::with_capacity(raw.len());
    for d in raw {
        let range = d.get("range")?;
        let start = range.get("start")?;
        let line = start.get("line")?.as_u64()? as u32 + 1;
        let column = start.get("character")?.as_u64()? as u32 + 1;
        let severity = Severity::from_lsp(d.get("severity").and_then(|v| v.as_i64()))
            .unwrap_or(Severity::Error);
        let message = d
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        out.push(Diagnostic {
            line,
            column,
            severity,
            message,
        });
    }
    Some((path, out))
}

/// 将文件系统路径转换为 `file://` URI。尽力而为——我们不完美支持
/// Windows 驱动器号，但我们注册表中的 LSP 服务器
/// 能够很好地接受百分比编码的路径，满足编辑后诊断用例。
fn uri_from_path(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let s = canonical.to_string_lossy();
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{}", s.trim_start_matches('/'))
    }
}

/// [`uri_from_path`] 的反函数。当 URI 不是 `file://` 时返回 `None`。
fn path_from_uri(uri: &str) -> Option<PathBuf> {
    let stripped = uri.strip_prefix("file://")?;
    Some(PathBuf::from(stripped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lsp_header() {
        let frame = b"Content-Length: 5\r\n\r\nhello";
        let (end, len) = parse_header(frame).expect("头部解析成功");
        assert_eq!(end, 21);
        assert_eq!(len, 5);
    }

    #[test]
    fn parse_header_returns_none_when_truncated() {
        let frame = b"Content-Length: 5\r\nMissingTerm";
        assert!(parse_header(frame).is_none());
    }

    #[test]
    fn parses_publish_diagnostics_payload() {
        let payload = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": "file:///tmp/foo.rs",
                "diagnostics": [
                    {
                        "range": {
                            "start": { "line": 11, "character": 7 },
                            "end":   { "line": 11, "character": 8 }
                        },
                        "severity": 1,
                        "message": "missing semicolon"
                    }
                ]
            }
        });
        let (path, diags) = parse_publish_diagnostics(&payload).expect("解析成功");
        assert_eq!(path, PathBuf::from("/tmp/foo.rs"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 12);
        assert_eq!(diags[0].column, 8);
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].message, "missing semicolon");
    }

    #[test]
    fn round_trips_uri_path() {
        let path = PathBuf::from("/tmp/example/foo.rs");
        let uri = format!("file://{}", path.display());
        assert_eq!(path_from_uri(&uri), Some(path));
    }
}
