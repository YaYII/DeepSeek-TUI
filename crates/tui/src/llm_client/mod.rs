//! LLM 客户端 Trait 和重试逻辑
//!
//! 本模块为 LLM 提供者提供统一接口，具有健壮的重试逻辑、
//! 指数退避和正确的错误分类。
//!
//! # 架构
//!
//! - `LlmClient` trait：LLM 提供者（DeepSeek、`OpenAI` 等）的异步接口
//! - `RetryConfig`：具有指数退避和抖动的可配置重试行为
//! - `LlmError`：带有可重试性信息的分类错误

//! - `with_retry`：任何异步操作的通用重试包装器
//!
//! # 示例
//!
//! ```ignore
//! use crate::llm_client::{LlmClient, RetryConfig, with_retry};
//!
//! let config = RetryConfig::default();
//! let result = with_retry(&config, || async {
//!     client.create_message(request).await
//! }, None).await;
//! ```

use crate::config::RetryPolicy;
use crate::models::{MessageRequest, MessageResponse, StreamEvent};
use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[cfg(test)]
pub mod mock;

// === LlmClient Trait ===

/// SSE 事件的盒装流类型别名
pub type StreamEventBox =
    Pin<Box<dyn futures_util::Stream<Item = Result<StreamEvent>> + Send + 'static>>;

/// LLM 提供者的统一接口。
///
/// 此 trait 抽象了不同的 LLM API（DeepSeek、`OpenAI` 等），
/// 允许代理与实现此接口的任何提供者一起工作。
///
/// # 实现说明
///
/// - 所有方法都是异步的，需要 `Send + Sync` 以保证线程安全
/// - `create_message_stream` 方法返回一个用于 SSE 的固定盒装流
/// - 实现应处理自己的身份验证和基本 URL 配置
#[allow(async_fn_in_trait, dead_code)] // Trait methods are part of the LLM provider interface
pub trait LlmClient: Send + Sync {
    /// 返回提供者名称（例如 "openai"、"deepseek"）
    fn provider_name(&self) -> &'static str;

    /// 返回正在使用的模型标识符
    fn model(&self) -> &str;

    /// 创建非流式消息补全
    fn create_message(
        &self,
        request: MessageRequest,
    ) -> impl Future<Output = Result<MessageResponse>> + Send;

    /// 创建流式消息补全
    ///
    /// 返回应消费直到完成的 SSE 事件流。
    async fn create_message_stream(&self, request: MessageRequest) -> Result<StreamEventBox>;

    /// 可选健康检查，用于验证 API 连接性
    async fn health_check(&self) -> Result<bool> {
        Ok(true)
    }
}

/// 支持可配置重试行为的客户端的 Trait
#[allow(dead_code)] // Part of LLM provider interface, will be used by additional providers
pub trait RetryConfigurable {
    fn retry_config(&self) -> &RetryConfig;
    fn set_retry_config(&mut self, config: RetryConfig);
}

// === LlmError - Classified Error Types ===

/// 带有可重试性信息的分类 LLM 错误。
///
/// 此枚举对 API 错误进行分类以支持智能重试决策。
/// 某些错误（速率限制、瞬时服务器错误）是可重试的，
/// 而其他错误（认证失败、无效请求）应立即失败。
#[derive(Debug)]
pub enum LlmError {
    /// 超过速率限制（HTTP 429）
    /// 包含可选的服务器 Retry-After 持续时间
    RateLimited {
        message: String,
        retry_after: Option<Duration>,
    },

    /// 服务器错误（HTTP 5xx）
    ServerError { status: u16, message: String },

    /// 网络连接错误
    NetworkError(String),

    /// 请求超时
    Timeout(Duration),

    /// 认证失败（HTTP 401、403）
    AuthenticationError(String),

    /// 无效的请求参数（HTTP 400）
    InvalidRequest { status: u16, message: String },

    /// 模型特定错误（模型未找到等）
    ModelError(String),

    /// 内容策略违规（安全过滤器）
    ContentPolicyError(String),

    /// 解析 API 响应失败
    ParseError(String),

    /// 上下文长度超出
    ContextLengthError(String),

    /// 其他错误的捕获-all
    Other(String),
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::RateLimited { message, .. } => write!(f, "Rate limit exceeded: {message}"),
            LlmError::ServerError { status, message } => {
                write!(f, "Server error ({status}): {message}")
            }
            LlmError::NetworkError(msg) => write!(f, "Network error: {msg}"),
            LlmError::Timeout(d) => write!(f, "Request timed out after {d:?}"),
            LlmError::AuthenticationError(msg) => write!(f, "Authentication failed: {msg}"),
            LlmError::InvalidRequest { status, message } => {
                write!(f, "Invalid request ({status}): {message}")
            }
            LlmError::ModelError(msg) => write!(f, "Model error: {msg}"),
            LlmError::ContentPolicyError(msg) => write!(f, "Content policy violation: {msg}"),
            LlmError::ParseError(msg) => write!(f, "Response parsing error: {msg}"),
            LlmError::ContextLengthError(msg) => write!(f, "Context length exceeded: {msg}"),
            LlmError::Other(msg) => write!(f, "LLM error: {msg}"),
        }
    }
}

impl std::error::Error for LlmError {}

impl LlmError {
    /// 判断此错误是否可能是瞬时的并且值得重试。
    ///
    /// 可重试的错误：
    /// - 速率限制（带退避）
    /// - 服务器错误（5xx）
    /// - 网络错误（连接问题）
    /// - 超时
    ///
    /// 不可重试的错误：
    /// - 认证失败
    /// - 无效请求
    /// - 内容策略违规
    /// - 上下文长度错误
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            LlmError::RateLimited { .. }
                | LlmError::ServerError { .. }
                | LlmError::NetworkError(_)
                | LlmError::Timeout(_)
        )
    }

    /// 返回服务器建议的重试延迟（如果可用）。
    ///
    /// 对于速率限制错误，当服务器提供 Retry-After 标头时通常存在。
    pub fn suggested_retry_delay(&self) -> Option<Duration> {
        match self {
            LlmError::RateLimited { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    /// 从 HTTP 状态码和响应体构造 `LlmError`。
    ///
    /// 基于以下条件执行启发式分类：
    /// - 状态码（429 = 速率限制、401/403 = 认证、5xx = 服务器错误）
    /// - 响应体关键词（`context_length`、`content_policy`、safety 等）
    pub fn from_http_response(status: u16, body: &str) -> Self {
        match status {
            429 => LlmError::RateLimited {
                message: body.to_string(),
                retry_after: None,
            },
            401 | 403 => LlmError::AuthenticationError(body.to_string()),
            400 => {
                // Classify 400 errors by examining the response body
                let body_lower = body.to_lowercase();
                if body_lower.contains("context_length")
                    || body_lower.contains("token")
                    || body_lower.contains("too long")
                    || body_lower.contains("maximum")
                {
                    LlmError::ContextLengthError(body.to_string())
                } else if body_lower.contains("content_policy")
                    || body_lower.contains("safety")
                    || body_lower.contains("harmful")
                    || body_lower.contains("inappropriate")
                {
                    LlmError::ContentPolicyError(body.to_string())
                } else if body_lower.contains("model") && body_lower.contains("not found") {
                    LlmError::ModelError(body.to_string())
                } else {
                    LlmError::InvalidRequest {
                        status,
                        message: body.to_string(),
                    }
                }
            }
            404 => {
                if body.to_lowercase().contains("model") {
                    LlmError::ModelError(body.to_string())
                } else {
                    LlmError::InvalidRequest {
                        status,
                        message: body.to_string(),
                    }
                }
            }
            500..=599 => LlmError::ServerError {
                status,
                message: body.to_string(),
            },
            _ => LlmError::Other(format!("HTTP {status}: {body}")),
        }
    }

    /// Constructs an `LlmError` from HTTP status code, body, and optional Retry-After header.
    pub fn from_http_response_with_retry_after(
        status: u16,
        body: &str,
        retry_after: Option<Duration>,
    ) -> Self {
        let mut error = Self::from_http_response(status, body);
        if let LlmError::RateLimited {
            retry_after: ref mut ra,
            ..
        } = error
        {
            *ra = retry_after;
        }
        error
    }

    /// Constructs an `LlmError` from a reqwest error.
    pub fn from_reqwest(err: &reqwest::Error) -> Self {
        if err.is_timeout() {
            LlmError::Timeout(Duration::from_secs(0))
        } else if err.is_connect() {
            LlmError::NetworkError(format!("Connection failed: {err}"))
        } else if err.is_request() {
            LlmError::NetworkError(format!("Request failed: {err}"))
        } else {
            LlmError::Other(err.to_string())
        }
    }
}

impl From<reqwest::Error> for LlmError {
    fn from(err: reqwest::Error) -> Self {
        LlmError::from_reqwest(&err)
    }
}

impl From<serde_json::Error> for LlmError {
    fn from(err: serde_json::Error) -> Self {
        LlmError::ParseError(err.to_string())
    }
}

// === RetryConfig - Exponential Backoff Configuration ===

/// 具有指数退避的重试行为配置。
///
/// 此结构控制重试的执行方式：
/// - 重试尝试次数
/// - 延迟计算（带可选抖动的指数退避）
/// - 哪些 HTTP 状态码可重试
/// - 超时处理
///
/// # 默认值
///
/// - `enabled`：true
/// - `max_retries`：3
/// - `initial_delay`：1.0 秒
/// - `max_delay`：60.0 秒
/// - `exponential_base`：2.0
/// - `jitter`：true（添加随机性以防止惊群效应）
/// - `jitter_factor`：0.1（10% 的变化）
/// - `retryable_status_codes`：[429, 500, 502, 503, 504]
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// 重试逻辑是否启用
    pub enabled: bool,

    /// 最大重试尝试次数（0 = 不重试，3 = 最多 4 次总尝试）
    pub max_retries: u32,

    /// 首次重试前的初始延迟（秒）
    pub initial_delay: f64,

    /// 重试之间的最大延迟（秒）
    pub max_delay: f64,

    /// 指数退避的基数（延迟 = 初始 * 基数^尝试次数）
    pub exponential_base: f64,

    /// 是否向延迟添加随机抖动
    pub jitter: bool,

    /// 抖动因子（0.1 = +/- 10% 变化）
    pub jitter_factor: f64,

    /// 是否尊重服务器的 Retry-After 标头
    pub respect_retry_after: bool,

    /// 应触发重试的 HTTP 状态码
    #[allow(dead_code)] // Used in tests via is_retryable_status()
    pub retryable_status_codes: Vec<u16>,

    /// 单个请求的超时时间（秒，0 = 无超时）
    #[allow(dead_code)] // Configuration field for retry consumers
    pub request_timeout: f64,

    /// 所有重试尝试的总超时时间（秒，0 = 无总超时）
    pub total_timeout: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_retries: 3,
            initial_delay: 1.0,
            max_delay: 60.0,
            exponential_base: 2.0,
            jitter: true,
            jitter_factor: 0.1,
            respect_retry_after: true,
            retryable_status_codes: vec![429, 500, 502, 503, 504],
            request_timeout: 120.0,
            total_timeout: 0.0, // No total timeout by default
        }
    }
}

#[allow(dead_code)] // Public builder API, used in tests
impl RetryConfig {
    /// 使用默认值创建新的 `RetryConfig`
    pub fn new() -> Self {
        Self::default()
    }

    /// 创建一个禁用重试的配置
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Builder 方法，用于设置最大重试次数
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Builder method to set initial delay
    pub fn with_initial_delay(mut self, delay: f64) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Builder method to set max delay
    pub fn with_max_delay(mut self, delay: f64) -> Self {
        self.max_delay = delay;
        self
    }

    /// Builder method to enable/disable jitter
    pub fn with_jitter(mut self, enabled: bool) -> Self {
        self.jitter = enabled;
        self
    }

    /// Builder method to set request timeout
    pub fn with_request_timeout(mut self, timeout: f64) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Builder method to set total timeout
    pub fn with_total_timeout(mut self, timeout: f64) -> Self {
        self.total_timeout = timeout;
        self
    }

    /// Calculates the delay for a given retry attempt.
    ///
    /// Uses exponential backoff: delay = `initial_delay` * `exponential_base^attempt`
    /// The result is capped at `max_delay` and optionally has jitter applied.
    ///
    /// # Arguments
    ///
    /// * `attempt` - Zero-based attempt number (0 = first retry)
    ///
    /// # Returns
    ///
    /// Duration to wait before the next retry attempt
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let exponent = i32::try_from(attempt).unwrap_or(i32::MAX);
        let base_delay = self.initial_delay * self.exponential_base.powi(exponent);
        let capped_delay = base_delay.min(self.max_delay);

        let final_delay = if self.jitter {
            // Add random jitter to prevent thundering herd problem
            let jitter_range = capped_delay * self.jitter_factor;
            // Use UUID v4 entropy for jitter randomness.
            let bytes = *Uuid::new_v4().as_bytes();
            let sample = u16::from_le_bytes([bytes[0], bytes[1]]);
            let random_factor = f64::from(sample) / f64::from(u16::MAX); // 0.0 to 1.0
            let jitter = jitter_range * (2.0 * random_factor - 1.0); // -range to +range

            (capped_delay + jitter).max(0.0)
        } else {
            capped_delay
        };

        Duration::from_secs_f64(final_delay)
    }

    /// Checks if a given HTTP status code should trigger a retry
    pub fn is_retryable_status(&self, status: u16) -> bool {
        self.retryable_status_codes.contains(&status)
    }
}

/// Converts from the existing `RetryPolicy` in config
impl From<RetryPolicy> for RetryConfig {
    fn from(policy: RetryPolicy) -> Self {
        Self {
            enabled: policy.enabled,
            max_retries: policy.max_retries,
            initial_delay: policy.initial_delay,
            max_delay: policy.max_delay,
            exponential_base: policy.exponential_base,
            ..Default::default()
        }
    }
}

/// Converts back to `RetryPolicy` for compatibility
impl From<RetryConfig> for RetryPolicy {
    fn from(config: RetryConfig) -> Self {
        Self {
            enabled: config.enabled,
            max_retries: config.max_retries,
            initial_delay: config.initial_delay,
            max_delay: config.max_delay,
            exponential_base: config.exponential_base,
        }
    }
}

// === Retry Error and Result Types ===

/// 当所有重试尝试已耗尽时返回的错误。
#[derive(Debug)]
pub struct RetryError {
    /// 遇到的最后一个错误
    pub last_error: LlmError,

    /// 进行的总尝试次数
    pub attempts: u32,

    /// 所有尝试花费的总时间
    pub total_time: Duration,
}

impl std::fmt::Display for RetryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Retry exhausted after {} attempts ({:?}): {}",
            self.attempts, self.total_time, self.last_error
        )
    }
}

impl std::error::Error for RetryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.last_error)
    }
}

/// 重试操作的结果类型
pub type RetryResult<T> = Result<T, RetryError>;

/// 重试通知的回调类型
///
/// 在每次重试前调用，参数为：
/// - 触发重试的错误
/// - 尝试次数（从 0 开始）
/// - 下次尝试前的延迟
pub type RetryCallback = Box<dyn Fn(&LlmError, u32, Duration) + Send + Sync>;

// === with_retry - Generic Retry Wrapper ===

/// 使用可配置的重试逻辑执行异步操作。
///
/// 此函数包装任何返回 `Result<T, LlmError>` 的异步操作，
/// 并在瞬时故障时使用指数退避自动重试。
///
/// # 参数
///
/// * `config` — 重试配置（延迟、最大尝试次数等）
/// * `operation` — 要执行的异步闭包（重试时将多次调用）
/// * `callback` — 可选的重试通知回调（日志记录、指标等）
///
/// # 返回值
///
/// * `Ok(T)` — 操作的成功结果
/// * `Err(RetryError)` — 所有重试已耗尽或遇到不可重试的错误
///
/// # Example
///
/// ```ignore
/// let result = with_retry(
///     &config,
///     || async { client.send_request(&req).await },
///     Some(Box::new(|err, attempt, delay| {
///         eprintln!("Retry {} after {:?}: {}", attempt, delay, err);
///     })),
/// ).await;
/// ```
pub async fn with_retry<F, Fut, T>(
    config: &RetryConfig,
    mut operation: F,
    callback: Option<RetryCallback>,
) -> RetryResult<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, LlmError>>,
{
    // If retries are disabled, just run once
    if !config.enabled {
        return operation().await.map_err(|e| RetryError {
            last_error: e,
            attempts: 1,
            total_time: Duration::ZERO,
        });
    }

    let start_time = Instant::now();
    let total_timeout = if config.total_timeout > 0.0 {
        Some(Duration::from_secs_f64(config.total_timeout))
    } else {
        None
    };

    let mut last_error: Option<LlmError> = None;

    // Attempt 0 is the first try, then up to max_retries additional attempts
    for attempt in 0..=config.max_retries {
        // Check total timeout
        if let Some(timeout) = total_timeout
            && start_time.elapsed() >= timeout
        {
            return Err(RetryError {
                last_error: last_error.unwrap_or(LlmError::Timeout(timeout)),
                attempts: attempt,
                total_time: start_time.elapsed(),
            });
        }

        match operation().await {
            Ok(result) => return Ok(result),
            Err(err) => {
                // Non-retryable errors fail immediately
                if !err.is_retryable() {
                    return Err(RetryError {
                        last_error: err,
                        attempts: attempt + 1,
                        total_time: start_time.elapsed(),
                    });
                }

                // Last attempt - no more retries
                if attempt >= config.max_retries {
                    return Err(RetryError {
                        last_error: err,
                        attempts: attempt + 1,
                        total_time: start_time.elapsed(),
                    });
                }

                // Calculate delay
                // Use server's Retry-After if available and configured
                let base_delay = config.delay_for_attempt(attempt);
                let delay = if config.respect_retry_after {
                    err.suggested_retry_delay().unwrap_or(base_delay)
                } else {
                    base_delay
                };

                // Notify callback if provided
                if let Some(ref cb) = callback {
                    cb(&err, attempt, delay);
                }

                last_error = Some(err);

                // Wait before retrying
                tokio::time::sleep(delay).await;
            }
        }
    }

    // Should not reach here, but handle gracefully
    Err(RetryError {
        last_error: last_error.unwrap_or(LlmError::Other("Unknown retry error".to_string())),
        attempts: config.max_retries + 1,
        total_time: start_time.elapsed(),
    })
}

/// `with_retry` 的简化版本，无回调
#[allow(dead_code)] // Convenience wrapper for with_retry
pub async fn with_retry_simple<F, Fut, T>(config: &RetryConfig, operation: F) -> RetryResult<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, LlmError>>,
{
    with_retry(config, operation, None).await
}

// === Utility Functions ===

/// 将 Retry-After 标头值解析为 Duration。
///
/// 支持两种格式：
/// - 整型秒数："120" -> 120 秒
/// - HTTP 日期格式："Wed, 21 Oct 2015 07:28:00 GMT"（未实现，返回 None）
pub fn parse_retry_after(value: &str) -> Option<Duration> {
    // Try parsing as seconds
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    // Try parsing as float seconds
    if let Ok(seconds) = value.parse::<f64>() {
        return Some(Duration::from_secs_f64(seconds));
    }

    // HTTP-date format not supported yet
    // Could use chrono or httpdate crate if needed
    None
}

/// 从响应标头中提取 Retry-After 持续时间
pub fn extract_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_retry_after)
}

// === Tests ===

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_f64_eq(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < f64::EPSILON,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_retries, 3);
        assert_f64_eq(config.initial_delay, 1.0);
        assert_f64_eq(config.max_delay, 60.0);
        assert_f64_eq(config.exponential_base, 2.0);
        assert!(config.jitter);
    }

    #[test]
    fn test_retry_config_disabled() {
        let config = RetryConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_retry_config_builder() {
        let config = RetryConfig::new()
            .with_max_retries(5)
            .with_initial_delay(2.0)
            .with_max_delay(120.0)
            .with_jitter(false);

        assert_eq!(config.max_retries, 5);
        assert_f64_eq(config.initial_delay, 2.0);
        assert_f64_eq(config.max_delay, 120.0);
        assert!(!config.jitter);
    }

    #[test]
    fn test_delay_for_attempt_exponential() {
        let config = RetryConfig::new().with_jitter(false);

        // delay = initial * base^attempt
        // 1.0 * 2^0 = 1.0
        let d0 = config.delay_for_attempt(0);
        assert_eq!(d0, Duration::from_secs_f64(1.0));

        // 1.0 * 2^1 = 2.0
        let d1 = config.delay_for_attempt(1);
        assert_eq!(d1, Duration::from_secs_f64(2.0));

        // 1.0 * 2^2 = 4.0
        let d2 = config.delay_for_attempt(2);
        assert_eq!(d2, Duration::from_secs_f64(4.0));

        // 1.0 * 2^3 = 8.0
        let d3 = config.delay_for_attempt(3);
        assert_eq!(d3, Duration::from_secs_f64(8.0));
    }

    #[test]
    fn test_delay_for_attempt_capped() {
        let config = RetryConfig::new().with_jitter(false).with_max_delay(5.0);

        // 1.0 * 2^3 = 8.0, but capped at 5.0
        let d3 = config.delay_for_attempt(3);
        assert_eq!(d3, Duration::from_secs_f64(5.0));
    }

    #[test]
    fn test_delay_for_attempt_with_jitter() {
        let config = RetryConfig::new().with_jitter(true);

        // With jitter, delays should vary slightly
        let d1 = config.delay_for_attempt(1);
        let d2 = config.delay_for_attempt(1);

        // Both should be close to 2.0 seconds (within 10% jitter)
        let base = 2.0;
        let range = base * 0.1;
        assert!(d1.as_secs_f64() >= base - range);
        assert!(d1.as_secs_f64() <= base + range);
        assert!(d2.as_secs_f64() >= base - range);
        assert!(d2.as_secs_f64() <= base + range);
    }

    #[test]
    fn test_is_retryable_status() {
        let config = RetryConfig::default();

        assert!(config.is_retryable_status(429)); // Rate limit
        assert!(config.is_retryable_status(500)); // Internal server error
        assert!(config.is_retryable_status(502)); // Bad gateway
        assert!(config.is_retryable_status(503)); // Service unavailable
        assert!(config.is_retryable_status(504)); // Gateway timeout

        assert!(!config.is_retryable_status(400)); // Bad request
        assert!(!config.is_retryable_status(401)); // Unauthorized
        assert!(!config.is_retryable_status(403)); // Forbidden
        assert!(!config.is_retryable_status(404)); // Not found
    }

    #[test]
    fn test_llm_error_retryable() {
        // Retryable errors
        assert!(
            LlmError::RateLimited {
                message: "too many requests".to_string(),
                retry_after: None
            }
            .is_retryable()
        );
        assert!(
            LlmError::ServerError {
                status: 500,
                message: "internal error".to_string()
            }
            .is_retryable()
        );
        assert!(LlmError::NetworkError("connection refused".to_string()).is_retryable());
        assert!(LlmError::Timeout(Duration::from_secs(30)).is_retryable());

        // Non-retryable errors
        assert!(!LlmError::AuthenticationError("invalid key".to_string()).is_retryable());
        assert!(
            !LlmError::InvalidRequest {
                status: 400,
                message: "bad json".to_string()
            }
            .is_retryable()
        );
        assert!(!LlmError::ContentPolicyError("unsafe content".to_string()).is_retryable());
        assert!(!LlmError::ContextLengthError("too long".to_string()).is_retryable());
    }

    #[test]
    fn test_llm_error_from_http_response() {
        // Rate limit
        let err = LlmError::from_http_response(429, "rate limit exceeded");
        assert!(matches!(err, LlmError::RateLimited { .. }));

        // Auth errors
        let err = LlmError::from_http_response(401, "invalid api key");
        assert!(matches!(err, LlmError::AuthenticationError(_)));

        let err = LlmError::from_http_response(403, "forbidden");
        assert!(matches!(err, LlmError::AuthenticationError(_)));

        // Server errors
        let err = LlmError::from_http_response(500, "internal server error");
        assert!(matches!(err, LlmError::ServerError { status: 500, .. }));

        let err = LlmError::from_http_response(503, "service unavailable");
        assert!(matches!(err, LlmError::ServerError { status: 503, .. }));

        // Context length
        let err = LlmError::from_http_response(400, "context_length_exceeded");
        assert!(matches!(err, LlmError::ContextLengthError(_)));

        // Content policy
        let err = LlmError::from_http_response(400, "content_policy_violation");
        assert!(matches!(err, LlmError::ContentPolicyError(_)));

        // Generic 400
        let err = LlmError::from_http_response(400, "invalid json");
        assert!(matches!(err, LlmError::InvalidRequest { status: 400, .. }));
    }

    #[test]
    fn test_llm_error_suggested_retry_delay() {
        let err = LlmError::RateLimited {
            message: "slow down".to_string(),
            retry_after: Some(Duration::from_secs(60)),
        };
        assert_eq!(err.suggested_retry_delay(), Some(Duration::from_secs(60)));

        let err = LlmError::ServerError {
            status: 500,
            message: "error".to_string(),
        };
        assert_eq!(err.suggested_retry_delay(), None);
    }

    #[test]
    fn test_parse_retry_after() {
        // Integer seconds
        assert_eq!(parse_retry_after("120"), Some(Duration::from_secs(120)));
        assert_eq!(parse_retry_after("0"), Some(Duration::from_secs(0)));

        // Float seconds
        assert_eq!(parse_retry_after("1.5"), Some(Duration::from_secs_f64(1.5)));

        // Invalid
        assert_eq!(parse_retry_after("invalid"), None);
        assert_eq!(parse_retry_after(""), None);
    }

    #[test]
    fn test_retry_policy_conversion() {
        let policy = RetryPolicy {
            enabled: true,
            max_retries: 5,
            initial_delay: 2.0,
            max_delay: 30.0,
            exponential_base: 3.0,
        };

        let config: RetryConfig = policy.clone().into();
        assert_eq!(config.enabled, policy.enabled);
        assert_eq!(config.max_retries, policy.max_retries);
        assert_f64_eq(config.initial_delay, policy.initial_delay);
        assert_f64_eq(config.max_delay, policy.max_delay);
        assert_f64_eq(config.exponential_base, policy.exponential_base);

        // Convert back
        let policy2: RetryPolicy = config.into();
        assert_eq!(policy2.enabled, policy.enabled);
        assert_eq!(policy2.max_retries, policy.max_retries);
    }

    #[tokio::test]
    async fn test_with_retry_success_first_attempt() {
        let config = RetryConfig::default();
        let mut call_count = 0;

        let result = with_retry(
            &config,
            || {
                call_count += 1;
                async { Ok::<_, LlmError>(42) }
            },
            None,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count, 1);
    }

    #[tokio::test]
    async fn test_with_retry_disabled() {
        let config = RetryConfig::disabled();
        let mut call_count = 0;

        let result: RetryResult<i32> = with_retry(
            &config,
            || {
                call_count += 1;
                async {
                    Err(LlmError::ServerError {
                        status: 500,
                        message: "error".to_string(),
                    })
                }
            },
            None,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(call_count, 1); // No retries when disabled
    }

    #[tokio::test]
    async fn test_with_retry_non_retryable_error() {
        let config = RetryConfig::default();
        let mut call_count = 0;

        let result: RetryResult<i32> = with_retry(
            &config,
            || {
                call_count += 1;
                async { Err(LlmError::AuthenticationError("bad key".to_string())) }
            },
            None,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(call_count, 1); // Auth errors are not retried
    }

    #[tokio::test]
    async fn test_with_retry_eventual_success() {
        let config = RetryConfig::new()
            .with_max_retries(3)
            .with_initial_delay(0.01); // Fast for testing

        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();

        let result = with_retry(
            &config,
            || {
                let count = cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async move {
                    if count < 2 {
                        Err(LlmError::ServerError {
                            status: 500,
                            message: "temporary error".to_string(),
                        })
                    } else {
                        Ok::<_, LlmError>(42)
                    }
                }
            },
            None,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 3); // 2 failures + 1 success
    }

    #[tokio::test]
    async fn test_with_retry_exhausted() {
        let config = RetryConfig::new()
            .with_max_retries(2)
            .with_initial_delay(0.01);

        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();

        let result: RetryResult<i32> = with_retry(
            &config,
            || {
                cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async {
                    Err(LlmError::ServerError {
                        status: 500,
                        message: "persistent error".to_string(),
                    })
                }
            },
            None,
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.attempts, 3); // 1 initial + 2 retries
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_retry_callback() {
        let config = RetryConfig::new()
            .with_max_retries(2)
            .with_initial_delay(0.01);

        let callback_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = callback_count.clone();

        let _: RetryResult<i32> = with_retry(
            &config,
            || async {
                Err(LlmError::ServerError {
                    status: 500,
                    message: "error".to_string(),
                })
            },
            Some(Box::new(move |_err, _attempt, _delay| {
                cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })),
        )
        .await;

        // Callback called once per retry (not for the final failure)
        assert_eq!(callback_count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn test_retry_error_display() {
        let err = RetryError {
            last_error: LlmError::ServerError {
                status: 500,
                message: "internal error".to_string(),
            },
            attempts: 4,
            total_time: Duration::from_secs(10),
        };

        let display = format!("{err}");
        assert!(display.contains("4 attempts"));
        assert!(display.contains("10"));
        assert!(display.contains("Server error"));
    }
}
