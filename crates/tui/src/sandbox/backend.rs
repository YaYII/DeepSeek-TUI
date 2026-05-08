//! 可插拔沙箱后端抽象。
//!
//! 外部沙箱后端将 shell 命令执行路由到远程服务（例如阿里云 OpenSandbox），
//! 而不是在本地生成进程。这是对操作系统级沙箱模块（Seatbelt / Landlock / Windows）
//! 的补充——外部后端在配置时会完全*替换*本地执行。

use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;

/// 沙箱后端执行的输出。
#[derive(Debug, Clone)]
pub struct SandboxOutput {
    /// 命令的标准输出。
    pub stdout: String,
    /// 命令的标准错误。
    pub stderr: String,
    /// 退出码（0 表示成功）。
    pub exit_code: i32,
}

/// 外部沙箱后端的类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxKind {
    /// 无外部沙箱——在本地执行命令。
    None,
    /// 阿里云 OpenSandbox 远程执行。
    OpenSandbox,
}

impl SandboxKind {
    /// 从配置中解析沙箱后端名称（不区分大小写）。
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" | "" => Some(Self::None),
            "opensandbox" | "open-sandbox" | "open_sandbox" => Some(Self::OpenSandbox),
            _ => None,
        }
    }

    /// 人类可读的标签。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::OpenSandbox => "opensandbox",
        }
    }
}

/// Abstract interface for an external sandbox backend.
///
/// Implementations send commands to a remote execution environment and return
/// structured output. The trait is `Send + Sync` so it can be stored in an
/// `Arc` and shared across async tasks.
#[async_trait]
pub trait SandboxBackend: Send + Sync {
    /// Execute a shell command and return its output.
    ///
    /// `cmd` is the full shell command string (e.g. `"ls -la"`).
    /// `env` contains additional environment variables to set.
    async fn exec(&self, cmd: &str, env: &HashMap<String, String>) -> Result<SandboxOutput>;
}

use crate::config::Config;

/// Create the configured sandbox backend from config.
///
/// Returns `None` when no external sandbox backend is configured (i.e. the
/// `sandbox_backend` key is absent, empty, or `"none"`). When `"opensandbox"`
/// is set, constructs an [`OpenSandboxBackend`](super::opensandbox::OpenSandboxBackend) using `sandbox_url` and
/// `sandbox_api_key`.
pub fn create_backend(config: &Config) -> Result<Option<Box<dyn SandboxBackend>>> {
    let kind = config
        .sandbox_backend
        .as_deref()
        .and_then(SandboxKind::parse)
        .unwrap_or(SandboxKind::None);

    match kind {
        SandboxKind::None => Ok(None),
        SandboxKind::OpenSandbox => {
            let base_url = config
                .sandbox_url
                .clone()
                .unwrap_or_else(|| "http://localhost:8080".to_string());
            let api_key = config.sandbox_api_key.clone();
            let backend = super::opensandbox::OpenSandboxBackend::new(base_url, api_key, 30)?;
            Ok(Some(Box::new(backend)))
        }
    }
}
