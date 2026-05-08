//! DeepSeek API 密钥的秘密存储。
//!
//! 提供一个小的抽象（`KeyringStore`），加上由 OS 密钥环支持的默认实现
//! （`DefaultKeyringStore`）、用于无头或不支持平台的文件后备方案
//! （`FileKeyringStore`），以及用于测试的内存存储（`InMemoryKeyringStore`）。
//!
//! 通过 [`Secrets::resolve`] 的更高级查找先检查密钥环，
//! 然后回退到环境变量。配置文件优先级存在于 config crate 中，
//! 因此面向用户的命令可以在调用点保持 `config -> keyring -> env` 的显式顺序。
#![deny(missing_docs)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 默认 OS 密钥链服务名称。macOS 用户可以使用
/// `security find-generic-password -s deepseek -a <provider>` 验证条目。
pub const DEFAULT_SERVICE: &str = "deepseek";

/// 可能从 [`KeyringStore`] 后端产生的错误。
#[derive(Debug, Error)]
pub enum SecretsError {
    /// 底层 OS 密钥环后端报告错误。
    #[error("密钥环后端错误: {0}")]
    Keyring(String),
    /// 文件后端回退 I/O 错误。
    #[error("文件后端秘密存储 I/O 错误: {0}")]
    Io(#[from] std::io::Error),
    /// 文件后端回退 JSON（反）序列化错误。
    #[error("文件后端秘密存储 JSON 错误: {0}")]
    Json(#[from] serde_json::Error),
    /// 当磁盘上存储的秘密具有不安全的权限时捕获。
    #[error("文件后端秘密存储 {path} 的权限 {mode:o} 不安全（应为 0600）")]
    InsecurePermissions {
        /// 秘密文件的绝对路径。
        path: PathBuf,
        /// 观察到的 unix 权限模式。
        mode: u32,
    },
}

/// 抽象秘密存储；具体实现可以使用 OS 密钥环、`~/.deepseek/secrets/` 下的 JSON 文件，
/// 或内存映射（测试）。
pub trait KeyringStore: Send + Sync {
    /// 读取秘密。如果没有条目则返回 `Ok(None)`。
    fn get(&self, key: &str) -> Result<Option<String>, SecretsError>;
    /// 写入秘密，替换任何现有值。
    fn set(&self, key: &str, value: &str) -> Result<(), SecretsError>;
    /// 移除秘密。如果条目不存在则不应报错。
    fn delete(&self, key: &str) -> Result<(), SecretsError>;
    /// 后端的简短、人类可读名称（由 `doctor` 使用）。
    fn backend_name(&self) -> &'static str;
}

/// OS 密钥环后端（macOS Keychain、Windows Credential Manager、
/// Linux Secret Service / kwallet）。在没有配置本机密钥环依赖的平台上，
/// 探测此后端会返回不支持的错误，以便 [`Secrets::auto_detect`] 可以回退到 [`FileKeyringStore`]。
#[derive(Debug, Clone)]
pub struct DefaultKeyringStore {
    /// 密钥环服务名称（默认为 [`DEFAULT_SERVICE`]）。
    service: String,
}

impl Default for DefaultKeyringStore {
    fn default() -> Self {
        Self::new(DEFAULT_SERVICE)
    }
}

impl DefaultKeyringStore {
    /// 使用给定的服务名称构建新的存储。
    #[must_use]
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    /// 探测 OS 密钥环而不写入任何内容。如果后端可达则返回 `Ok(())`，
    /// 否则返回描述原因的错误。
    pub fn probe(&self) -> Result<(), SecretsError> {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            // `Entry::new` is enough to validate the native macOS/Windows
            // backend path. Avoid a dummy read there because it can trigger
            // a second user-visible Keychain/Credential Manager access before
            // the real provider key lookup.
            let entry = keyring::Entry::new(&self.service, "__probe__")
                .map_err(|err| SecretsError::Keyring(err.to_string()))?;
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            {
                let _ = entry;
                Ok(())
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            match entry.get_password() {
                Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(keyring::Error::PlatformFailure(err)) => {
                    Err(SecretsError::Keyring(format!("platform failure: {err}")))
                }
                Err(keyring::Error::NoStorageAccess(err)) => {
                    Err(SecretsError::Keyring(format!("no storage access: {err}")))
                }
                Err(other) => Err(SecretsError::Keyring(other.to_string())),
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = &self.service;
            Err(SecretsError::Keyring(unsupported_keyring_message()))
        }
    }
}

impl KeyringStore for DefaultKeyringStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretsError> {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let entry = keyring::Entry::new(&self.service, key)
                .map_err(|err| SecretsError::Keyring(err.to_string()))?;
            match entry.get_password() {
                Ok(value) => Ok(Some(value)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(err) => Err(SecretsError::Keyring(err.to_string())),
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = key;
            Err(SecretsError::Keyring(unsupported_keyring_message()))
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretsError> {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let entry = keyring::Entry::new(&self.service, key)
                .map_err(|err| SecretsError::Keyring(err.to_string()))?;
            entry
                .set_password(value)
                .map_err(|err| SecretsError::Keyring(err.to_string()))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = (key, value);
            Err(SecretsError::Keyring(unsupported_keyring_message()))
        }
    }

    fn delete(&self, key: &str) -> Result<(), SecretsError> {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let entry = keyring::Entry::new(&self.service, key)
                .map_err(|err| SecretsError::Keyring(err.to_string()))?;
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(err) => Err(SecretsError::Keyring(err.to_string())),
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = key;
            Err(SecretsError::Keyring(unsupported_keyring_message()))
        }
    }

    fn backend_name(&self) -> &'static str {
        "系统密钥环"
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn unsupported_keyring_message() -> String {
    "此平台不支持系统密钥环后端".to_string()
}

/// 内存密钥环（仅测试）。
#[derive(Debug, Default)]
pub struct InMemoryKeyringStore {
    entries: Mutex<HashMap<String, String>>,
}

impl InMemoryKeyringStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl KeyringStore for InMemoryKeyringStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretsError> {
        Ok(self.entries.lock().unwrap().get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretsError> {
        self.entries
            .lock()
            .unwrap()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), SecretsError> {
        self.entries.lock().unwrap().remove(key);
        Ok(())
    }

    fn backend_name(&self) -> &'static str {
        "内存（测试）"
    }
}

/// 用于无头环境的 JSON-on-disk 回退方案，适用于没有 Secret Service / dbus 的环境。
/// 存储在 `<home>/.deepseek/secrets/secrets.json`，权限为 `0600`。
#[derive(Debug, Clone)]
pub struct FileKeyringStore {
    /// Absolute path to the JSON file.
    path: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct FileSecretsBlob {
    #[serde(default)]
    entries: HashMap<String, String>,
}

impl FileKeyringStore {
    /// 构建由给定 JSON 文件路径支持的存储。
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// 默认路径：`<home>/.deepseek/secrets/secrets.json`。通过 `dirs` crate
    /// 使用 `HOME`（Unix）和 `USERPROFILE`（Windows）。
    pub fn default_path() -> Result<PathBuf, SecretsError> {
        let home = dirs::home_dir().ok_or_else(|| {
            SecretsError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "could not resolve home directory for FileKeyringStore",
            ))
        })?;
        Ok(home.join(".deepseek").join("secrets").join("secrets.json"))
    }

    /// 用于存储的路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn load_unlocked(&self) -> Result<FileSecretsBlob, SecretsError> {
        if !self.path.exists() {
            return Ok(FileSecretsBlob::default());
        }
        // Reject files with unsafe permissions on unix. On Windows the
        // ACL model is too different to enforce here; the caller is
        // responsible for placing the file in a per-user directory.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = fs::metadata(&self.path)?;
            let mode = meta.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                return Err(SecretsError::InsecurePermissions {
                    path: self.path.clone(),
                    mode,
                });
            }
        }
        let raw = fs::read_to_string(&self.path)?;
        if raw.trim().is_empty() {
            return Ok(FileSecretsBlob::default());
        }
        let blob: FileSecretsBlob = serde_json::from_str(&raw)?;
        Ok(blob)
    }

    fn store_unlocked(&self, blob: &FileSecretsBlob) -> Result<(), SecretsError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(parent)?.permissions();
                perms.set_mode(0o700);
                let _ = fs::set_permissions(parent, perms);
            }
        }
        let body = serde_json::to_string_pretty(blob)?;
        fs::write(&self.path, body)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Best-effort 0o600 — matches the parent-dir chmod above which
            // is also `let _ = ...`. Filesystems that don't support Unix
            // chmod (Docker bind-mounts of NTFS, network shares — #897)
            // would otherwise fail the whole save here even though the
            // blob already wrote successfully. The host's native ACLs
            // are doing access control in those environments.
            if let Ok(meta) = fs::metadata(&self.path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o600);
                let _ = fs::set_permissions(&self.path, perms);
            }
        }
        Ok(())
    }
}

impl KeyringStore for FileKeyringStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretsError> {
        let blob = self.load_unlocked()?;
        Ok(blob.entries.get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretsError> {
        // load_unlocked already returns Ok(default) for a missing file, so the
        // first-write-creates-the-file path is preserved. Any other Err
        // (insecure permissions, corrupt JSON, transient I/O) MUST surface to
        // the caller — propagating it via `unwrap_or_default()` silently
        // wipes every previously stored secret on the next `store_unlocked`.
        let mut blob = self.load_unlocked()?;
        blob.entries.insert(key.to_string(), value.to_string());
        self.store_unlocked(&blob)
    }

    fn delete(&self, key: &str) -> Result<(), SecretsError> {
        // Same invariant as `set`: never fall back to an empty blob on read
        // error, or `delete <one-key>` becomes `delete <every-key>`.
        let mut blob = self.load_unlocked()?;
        blob.entries.remove(key);
        self.store_unlocked(&blob)
    }

    fn backend_name(&self) -> &'static str {
        "文件后端 (~/.deepseek/secrets/)"
    }
}

/// 高层外观，结合 [`KeyringStore`] 与环境变量回退。
///
/// 查找优先级：**keyring → env → none**。同时具有 TOML 配置层的调用者
/// 必须自己在链的最后连接该层。
#[derive(Clone)]
pub struct Secrets {
    /// 底层秘密存储。
    pub store: Arc<dyn KeyringStore>,
    /// 密钥环中的所有者标识符（通常为 "deepseek"）；传递给 `resolve` 的
    /// `key` 参数按原样映射到存储槽位，而环境变量则通过规范名称查找。
    service: String,
}

/// 提供已解析秘密的源层。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretSource {
    /// 配置的密钥环后端返回了秘密。
    Keyring,
    /// 进程环境变量返回了秘密。
    Env,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secrets")
            .field("backend", &self.store.backend_name())
            .field("service", &self.service)
            .finish()
    }
}

impl Secrets {
    /// 在存储周围构建新的外观。
    #[must_use]
    pub fn new(store: Arc<dyn KeyringStore>) -> Self {
        Self {
            store,
            service: DEFAULT_SERVICE.to_string(),
        }
    }

    /// 构造平台适配合适的默认后端。在有可用的 OS 密钥环后端的平台上
    /// 返回 [`DefaultKeyringStore`]；否则回退到
    /// `~/.deepseek/secrets/` 下的 [`FileKeyringStore`]。
    pub fn auto_detect() -> Self {
        let default_store = DefaultKeyringStore::default();
        match default_store.probe() {
            Ok(()) => Self::new(Arc::new(default_store)),
            Err(err) => {
                tracing::warn!(
                    "OS keyring unavailable ({err}); falling back to file-backed secret store"
                );
                let path = FileKeyringStore::default_path()
                    .unwrap_or_else(|_| PathBuf::from(".deepseek-secrets.json"));
                Self::new(Arc::new(FileKeyringStore::new(path)))
            }
        }
    }

    /// 后端标签，适用于 `doctor` 输出。
    #[must_use]
    pub fn backend_name(&self) -> &'static str {
        self.store.backend_name()
    }

    /// 使用 `keyring → env → none` 优先级解析秘密。
    ///
    /// `name` is the canonical provider name (`"deepseek"`,
    /// `"openrouter"`, `"novita"`, `"nvidia"`/`"nvidia-nim"`, `"openai"`).
    /// Empty strings on either layer are treated as "not set".
    #[must_use]
    pub fn resolve(&self, name: &str) -> Option<String> {
        self.resolve_with_source(name).map(|(value, _)| value)
    }

    /// 解析秘密并报告哪个层提供了它。
    #[must_use]
    pub fn resolve_with_source(&self, name: &str) -> Option<(String, SecretSource)> {
        if let Ok(Some(v)) = self.store.get(name)
            && !v.trim().is_empty()
        {
            return Some((v, SecretSource::Keyring));
        }
        env_for(name).map(|value| (value, SecretSource::Env))
    }

    /// 便捷方法：通过底层存储写入秘密。
    pub fn set(&self, name: &str, value: &str) -> Result<(), SecretsError> {
        self.store.set(name, value)
    }

    /// 便捷方法：通过底层存储删除秘密。
    pub fn delete(&self, name: &str) -> Result<(), SecretsError> {
        self.store.delete(name)
    }

    /// 便捷方法：直接读取秘密（无环境变量回退）。
    pub fn get(&self, name: &str) -> Result<Option<String>, SecretsError> {
        self.store.get(name)
    }
}

/// 将规范提供商名称映射到其环境变量，如果非空则返回值。
#[must_use]
pub fn env_for(name: &str) -> Option<String> {
    let candidates: &[&str] = match name.to_ascii_lowercase().as_str() {
        "deepseek" => &["DEEPSEEK_API_KEY"],
        "openrouter" => &["OPENROUTER_API_KEY"],
        "novita" => &["NOVITA_API_KEY"],
        // NVIDIA NIM 最后回退到 `DEEPSEEK_API_KEY`，因为当没有设置专用
        // NVIDIA 令牌时，目录端点接受相同 DeepSeek 颁发的密钥。
        // 这反映了 v0.7 之前的行为。
        "nvidia" | "nvidia-nim" | "nvidia_nim" | "nim" => {
            &["NVIDIA_API_KEY", "NVIDIA_NIM_API_KEY", "DEEPSEEK_API_KEY"]
        }
        "fireworks" | "fireworks-ai" => &["FIREWORKS_API_KEY"],
        "sglang" | "sg-lang" => &["SGLANG_API_KEY"],
        "vllm" | "v-llm" => &["VLLM_API_KEY"],
        "ollama" | "ollama-local" => &["OLLAMA_API_KEY"],
        "openai" => &["OPENAI_API_KEY"],
        _ => return None,
    };
    for var in candidates {
        if let Ok(value) = std::env::var(var)
            && !value.trim().is_empty()
        {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    /// 序列化环境变异测试：此模块中的测试操作 `DEEPSEEK_API_KEY` 等，
    /// 它们是进程全局的。
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }

    fn clear_known_envs() {
        for var in [
            "DEEPSEEK_API_KEY",
            "OPENROUTER_API_KEY",
            "NOVITA_API_KEY",
            "NVIDIA_API_KEY",
            "NVIDIA_NIM_API_KEY",
            "FIREWORKS_API_KEY",
            "SGLANG_API_KEY",
            "VLLM_API_KEY",
            "OLLAMA_API_KEY",
            "OPENAI_API_KEY",
        ] {
            // Safety: tests serialise on env_lock(); the broader
            // workspace has the same pattern in `crates/config`.
            unsafe { std::env::remove_var(var) };
        }
    }

    #[test]
    fn in_memory_store_round_trips() {
        let store = InMemoryKeyringStore::new();
        assert_eq!(store.get("deepseek").unwrap(), None);
        store.set("deepseek", "sk-test").unwrap();
        assert_eq!(store.get("deepseek").unwrap(), Some("sk-test".to_string()));
        store.set("deepseek", "sk-replaced").unwrap();
        assert_eq!(
            store.get("deepseek").unwrap(),
            Some("sk-replaced".to_string())
        );
        store.delete("deepseek").unwrap();
        assert_eq!(store.get("deepseek").unwrap(), None);
        // Deleting an absent key is a no-op.
        store.delete("missing").unwrap();
    }

    #[test]
    fn resolve_prefers_keyring_over_env() {
        let _lock = env_lock();
        clear_known_envs();
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::set_var("DEEPSEEK_API_KEY", "env-key") };

        let store = Arc::new(InMemoryKeyringStore::new());
        store.set("deepseek", "ring-key").unwrap();
        let secrets = Secrets::new(store);

        assert_eq!(secrets.resolve("deepseek").as_deref(), Some("ring-key"));
        assert_eq!(
            secrets.resolve_with_source("deepseek"),
            Some(("ring-key".to_string(), SecretSource::Keyring))
        );
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::remove_var("DEEPSEEK_API_KEY") };
    }

    #[test]
    fn resolve_falls_back_to_env_when_keyring_empty() {
        let _lock = env_lock();
        clear_known_envs();
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::set_var("DEEPSEEK_API_KEY", "env-fallback") };

        let secrets = Secrets::new(Arc::new(InMemoryKeyringStore::new()));
        assert_eq!(secrets.resolve("deepseek").as_deref(), Some("env-fallback"));
        assert_eq!(
            secrets.resolve_with_source("deepseek"),
            Some(("env-fallback".to_string(), SecretSource::Env))
        );
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::remove_var("DEEPSEEK_API_KEY") };
    }

    #[test]
    fn resolve_returns_none_when_both_layers_empty() {
        let _lock = env_lock();
        clear_known_envs();
        let secrets = Secrets::new(Arc::new(InMemoryKeyringStore::new()));
        assert_eq!(secrets.resolve("deepseek"), None);
    }

    #[test]
    fn resolve_treats_blank_keyring_value_as_unset() {
        let _lock = env_lock();
        clear_known_envs();
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::set_var("DEEPSEEK_API_KEY", "env-real") };

        let store = Arc::new(InMemoryKeyringStore::new());
        store.set("deepseek", "   ").unwrap();
        let secrets = Secrets::new(store);
        assert_eq!(secrets.resolve("deepseek").as_deref(), Some("env-real"));
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::remove_var("DEEPSEEK_API_KEY") };
    }

    #[test]
    fn nvidia_env_aliases_resolve() {
        let _lock = env_lock();
        clear_known_envs();
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::set_var("NVIDIA_NIM_API_KEY", "nim-key") };
        let secrets = Secrets::new(Arc::new(InMemoryKeyringStore::new()));
        assert_eq!(secrets.resolve("nvidia-nim").as_deref(), Some("nim-key"));
        assert_eq!(secrets.resolve("nvidia").as_deref(), Some("nim-key"));
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::remove_var("NVIDIA_NIM_API_KEY") };
    }

    #[test]
    fn fireworks_env_aliases_resolve() {
        let _lock = env_lock();
        clear_known_envs();
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::set_var("FIREWORKS_API_KEY", "fw-key") };

        assert_eq!(env_for("fireworks").as_deref(), Some("fw-key"));
        assert_eq!(env_for("fireworks-ai").as_deref(), Some("fw-key"));
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::remove_var("FIREWORKS_API_KEY") };
    }

    #[test]
    fn sglang_env_aliases_resolve() {
        let _lock = env_lock();
        clear_known_envs();
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::set_var("SGLANG_API_KEY", "sglang-key") };

        assert_eq!(env_for("sglang").as_deref(), Some("sglang-key"));
        assert_eq!(env_for("sg-lang").as_deref(), Some("sglang-key"));
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::remove_var("SGLANG_API_KEY") };
    }

    #[test]
    fn vllm_env_aliases_resolve() {
        let _lock = env_lock();
        clear_known_envs();
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::set_var("VLLM_API_KEY", "vllm-key") };

        assert_eq!(env_for("vllm").as_deref(), Some("vllm-key"));
        assert_eq!(env_for("v-llm").as_deref(), Some("vllm-key"));
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::remove_var("VLLM_API_KEY") };
    }

    #[test]
    fn ollama_env_aliases_resolve() {
        let _lock = env_lock();
        clear_known_envs();
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::set_var("OLLAMA_API_KEY", "ollama-key") };

        assert_eq!(env_for("ollama").as_deref(), Some("ollama-key"));
        assert_eq!(env_for("ollama-local").as_deref(), Some("ollama-key"));
        // Safety: env mutation guarded by env_lock().
        unsafe { std::env::remove_var("OLLAMA_API_KEY") };
    }

    #[cfg(unix)]
    #[test]
    fn file_store_round_trips_with_secure_perms() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("secrets.json");
        let store = FileKeyringStore::new(path.clone());
        assert_eq!(store.get("deepseek").unwrap(), None);
        store.set("deepseek", "sk-disk").unwrap();
        assert_eq!(store.get("deepseek").unwrap(), Some("sk-disk".to_string()));

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:o}");

        store.set("openrouter", "or-disk").unwrap();
        assert_eq!(
            store.get("openrouter").unwrap(),
            Some("or-disk".to_string())
        );
        // First entry must still be intact.
        assert_eq!(store.get("deepseek").unwrap(), Some("sk-disk".to_string()));

        store.delete("deepseek").unwrap();
        assert_eq!(store.get("deepseek").unwrap(), None);
    }

    #[cfg(unix)]
    #[test]
    fn file_store_rejects_world_readable_file() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("secrets.json");
        fs::write(&path, "{\"entries\":{\"deepseek\":\"leak\"}}").unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&path, perms).unwrap();

        let store = FileKeyringStore::new(path);
        let err = store.get("deepseek").unwrap_err();
        assert!(
            matches!(err, SecretsError::InsecurePermissions { .. }),
            "unexpected error: {err}"
        );
    }

    // Regression for #281: `set` and `delete` used to call
    // `load_unlocked().unwrap_or_default()`, which silently wiped every
    // existing secret whenever the read failed (insecure permissions,
    // corrupt JSON, or any other I/O error).

    #[cfg(unix)]
    #[test]
    fn file_store_set_does_not_clobber_secrets_when_perms_are_bad() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("secrets.json");
        let original = "{\"entries\":{\"deepseek\":\"sk-keep\",\"nvidia\":\"nv-keep\"}}";
        fs::write(&path, original).unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&path, perms).unwrap();

        let store = FileKeyringStore::new(path.clone());
        let err = store.set("openrouter", "or-new").unwrap_err();
        assert!(
            matches!(err, SecretsError::InsecurePermissions { .. }),
            "set must surface the read error rather than overwriting; got: {err}"
        );

        let on_disk = fs::read_to_string(&path).unwrap();
        assert_eq!(
            on_disk, original,
            "set must not modify the file when load_unlocked errored"
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_store_delete_does_not_clobber_secrets_when_perms_are_bad() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("secrets.json");
        let original = "{\"entries\":{\"deepseek\":\"sk-keep\",\"nvidia\":\"nv-keep\"}}";
        fs::write(&path, original).unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&path, perms).unwrap();

        let store = FileKeyringStore::new(path.clone());
        let err = store.delete("nvidia").unwrap_err();
        assert!(
            matches!(err, SecretsError::InsecurePermissions { .. }),
            "delete must surface the read error rather than wiping the file; got: {err}"
        );
        let on_disk = fs::read_to_string(&path).unwrap();
        assert_eq!(on_disk, original);
    }

    #[test]
    fn file_store_set_does_not_clobber_secrets_when_json_is_corrupt() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("secrets.json");
        // Corrupt JSON. Permissions ok where unix; on Windows the perm-check
        // doesn't run so we exercise the json-error path directly.
        fs::write(&path, "{ this is not valid json").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&path, perms).unwrap();
        }

        let store = FileKeyringStore::new(path.clone());
        let err = store.set("deepseek", "sk-new").unwrap_err();
        assert!(
            matches!(err, SecretsError::Json(_)),
            "set must surface the parse error rather than wiping the file; got: {err}"
        );
        let on_disk = fs::read_to_string(&path).unwrap();
        assert_eq!(on_disk, "{ this is not valid json");
    }

    #[test]
    fn file_store_set_still_creates_file_when_missing() {
        // Regression guard: the #281 fix removed `unwrap_or_default()` from
        // the load call. Make sure the original first-write-creates-the-file
        // ergonomic still works — `load_unlocked` returns `Ok(default)` for
        // a missing file, so the `?` should pass through cleanly.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("secrets.json");
        let store = FileKeyringStore::new(path.clone());

        store.set("deepseek", "sk-fresh").unwrap();
        assert_eq!(store.get("deepseek").unwrap(), Some("sk-fresh".to_string()));
    }

    #[test]
    fn file_store_default_path_uses_home() {
        // We don't override HOME here (other tests do); we just check the
        // shape of the path is `<home>/.deepseek/secrets/secrets.json`.
        let path = FileKeyringStore::default_path().unwrap();
        assert!(
            path.ends_with("secrets/secrets.json") || path.ends_with("secrets\\secrets.json"),
            "unexpected default path: {}",
            path.display()
        );
    }
}
