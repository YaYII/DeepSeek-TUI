//! macOS Seatbelt（sandbox-exec）配置文件生成。
//!
//! Seatbelt 是 Apple 的强制访问控制框架，使用基于 Scheme 的策略语言定义进程可以访问哪些系统资源。
//! 此模块根据配置的 `SandboxPolicy` 动态生成沙箱配置文件。
//!
//! # 工作原理
//!
//! 1. 我们以 SBPL 格式生成 Seatbelt 策略字符串
//! 2. 我们调用 `/usr/bin/sandbox-exec -p <policy>` 来运行命令
//! 3. 内核强制执行策略，阻止未授权的操作
//!
//! # 参考
//!
//! - Apple 的 sandbox(7) 手册页
//! - <https://reverse.put.as/wp-content/uploads/2011/09/Apple-Sandbox-Guide-v1.0.pdf>

// 注意：cfg(target_os = "macos") 已在 mod.rs 的模块级别应用

use super::policy::SandboxPolicy;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// macOS 上 sandbox-exec 二进制文件的路径。
pub const SANDBOX_EXEC_PATH: &str = "/usr/bin/sandbox-exec";

/// 基础 seatbelt 策略，提供最小的进程功能。
///
/// 此策略：
/// - 默认拒绝所有操作
/// - 允许进程执行和分支
/// - 允许同一沙箱内的信号
/// - 允许读取用户偏好设置（许多工具需要）
/// - 允许基本进程内省
/// - 允许写入 /dev/null
/// - 允许读取 sysctl 值
/// - 允许 POSIX 信号量和伪终端操作
const SEATBELT_BASE_POLICY: &str = r#"
(version 1)
(deny default)

; Core process operations
(allow process-exec)
(allow process-fork)
(allow signal (target same-sandbox))
(allow process-info* (target same-sandbox))

; User preferences (needed by many CLI tools)
(allow user-preference-read)

; Basic I/O to /dev/null
(allow file-write-data
  (require-all
    (path "/dev/null")
    (vnode-type CHARACTER-DEVICE)))

; System information
(allow sysctl-read)

; IPC primitives
(allow ipc-posix-sem)
(allow ipc-posix-shm-read*)
(allow ipc-posix-shm-write-create)
(allow ipc-posix-shm-write-data)
(allow ipc-posix-shm-write-unlink)

; Terminal support (essential for shell commands)
(allow pseudo-tty)
(allow file-read* file-write* file-ioctl (literal "/dev/ptmx"))
(allow file-read* file-write* file-ioctl (regex #"^/dev/ttys[0-9]+$"))

; macOS-specific device access
(allow file-read* (literal "/dev/urandom"))
(allow file-read* (literal "/dev/random"))
(allow file-ioctl (literal "/dev/dtracehelper"))

; Mach IPC (needed by many system services)
(allow mach-lookup)
"#;

/// 网络访问策略附加内容。
const SEATBELT_NETWORK_POLICY: &str = r"
; Network access
(allow network-outbound)
(allow network-inbound)
(allow system-socket)
(allow network-bind)
";

/// 检查 sandbox-exec 在当前系统上是否可用且被允许。
pub fn is_available() -> bool {
    static SEATBELT_AVAILABLE: OnceLock<bool> = OnceLock::new();

    *SEATBELT_AVAILABLE.get_or_init(|| {
        if !Path::new(SANDBOX_EXEC_PATH).exists() {
            return false;
        }

        let output = Command::new(SANDBOX_EXEC_PATH)
            .args(["-p", "(version 1)(allow default)", "--", "/usr/bin/true"])
            .output();

        match output {
            Ok(result) => result.status.success(),
            Err(_) => false,
        }
    })
}

/// 为 sandbox-exec 创建命令行参数。
///
/// 返回应预置到命令前的参数 Vec。
/// 格式为：`sandbox-exec -p <policy> -D KEY=VALUE ... -- <原始命令>`
pub fn create_seatbelt_args(
    command: Vec<String>,
    policy: &SandboxPolicy,
    sandbox_cwd: &Path,
) -> Vec<String> {
    let full_policy = generate_policy(policy, sandbox_cwd);
    let params = generate_params(policy, sandbox_cwd);

    let mut args = vec!["-p".to_string(), full_policy];

    // Add parameter definitions for variable substitution
    for (key, value) in params {
        args.push(format!("-D{}={}", key, value.to_string_lossy()));
    }

    // Separator between sandbox-exec args and the actual command
    args.push("--".to_string());
    args.extend(command);

    args
}

/// 为给定策略生成完整的 Seatbelt 策略字符串。
fn generate_policy(policy: &SandboxPolicy, cwd: &Path) -> String {
    let mut full_policy = SEATBELT_BASE_POLICY.to_string();

    // Add read access policy
    if SandboxPolicy::has_full_disk_read_access() {
        full_policy.push_str("\n; 完整文件系统读取访问\n(allow file-read*)");
    }

    // Add write access policy
    let file_write_policy = generate_write_policy(policy, cwd);
    if !file_write_policy.is_empty() {
        full_policy.push_str("\n\n; 写入访问策略\n");
        full_policy.push_str(&file_write_policy);
    }

    // Add network policy if enabled
    if policy.has_network_access() {
        full_policy.push('\n');
        full_policy.push_str(SEATBELT_NETWORK_POLICY);
    }

    // Add Darwin user cache directory access (needed by many macOS tools)
    full_policy.push_str("\n\n; Darwin 用户缓存目录\n");
    full_policy
        .push_str(r#"(allow file-read* file-write* (subpath (param "DARWIN_USER_CACHE_DIR")))"#);

    // Add common macOS directories that tools often need
    full_policy.push_str("\n\n; 常用 macOS 目录\n");
    full_policy.push_str(r#"(allow file-read* (subpath "/usr/lib"))"#);
    full_policy.push('\n');
    full_policy.push_str(r#"(allow file-read* (subpath "/usr/share"))"#);
    full_policy.push('\n');
    full_policy.push_str(r#"(allow file-read* (subpath "/System/Library"))"#);
    full_policy.push('\n');
    full_policy.push_str(r#"(allow file-read* (subpath "/Library/Preferences"))"#);
    full_policy.push('\n');
    full_policy.push_str(r#"(allow file-read* (subpath "/private/var/db"))"#);

    // Cargo home（#558）：cargo build/test/publish 需要访问 ~/.cargo/registry
    // 和 ~/.cargo/git 以获取 crate 元数据、下载的 tarball 和解压后的
    // 源码。沙箱工作区写入之前拒绝了这些访问，使得 `cargo publish`
    // 无法从 TUI 的 shell 工具内部运行。
    // 读取访问始终允许；写入访问在策略允许任何写入时授予（registry 缓存
    // 需要在缓存未命中时可写，以便 `cargo build` 填充它们）。当既没有
    // 设置 `CARGO_HOME` 也没有设置 `HOME` 时完全跳过——没有这些变量，
    // 我们就无法将路径插入策略参数。
    if resolve_cargo_home().is_some() {
        full_policy.push_str("\n\n; Cargo home（~/.cargo）—— registry/index/git 缓存\n");
        full_policy.push_str(r#"(allow file-read* (subpath (param "CARGO_HOME")))"#);
        if !matches!(policy, SandboxPolicy::ReadOnly) {
            full_policy.push('\n');
            full_policy.push_str(r#"(allow file-write* (subpath (param "CARGO_HOME_REGISTRY")))"#);
            full_policy.push('\n');
            full_policy.push_str(r#"(allow file-write* (subpath (param "CARGO_HOME_GIT")))"#);
        }
    }

    full_policy
}

/// 解析用户的 cargo home 目录——如果设置了 `CARGO_HOME` 则使用它，否则使用 `$HOME/.cargo`。
/// 仅当两个环境变量都未设置时返回 `None`（在实际的 macOS 用户账户上基本不会发生；
/// 可能在未导出 `HOME` 的 CI 容器中发生）。
fn resolve_cargo_home() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("CARGO_HOME")
        && !explicit.trim().is_empty()
    {
        return Some(PathBuf::from(explicit));
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".cargo"))
}

/// 生成 Seatbelt 策略的写入访问部分。
fn generate_write_policy(policy: &SandboxPolicy, cwd: &Path) -> String {
    // 完整的磁盘写入访问
    if policy.has_full_disk_write_access() {
        return r#"(allow file-write* (regex #"^/"))"#.to_string();
    }

    // 只读——无需写入策略
    if matches!(policy, SandboxPolicy::ReadOnly) {
        return String::new();
    }

    // 工作区写入——枚举允许的路径
    let writable_roots = policy.get_writable_roots(cwd);
    if writable_roots.is_empty() {
        return String::new();
    }

    let mut policies = Vec::new();

    for (index, root) in writable_roots.iter().enumerate() {
        let root_param = format!("WRITABLE_ROOT_{index}");

        if root.read_only_subpaths.is_empty() {
            // 简单情况：整个子树可写
            policies.push(format!("(subpath (param \"{root_param}\"))"));
        } else {
            // 复杂情况：可写但有只读例外
            // 使用 require-all 组合 subpath 和每个例外的 require-not
            let mut parts = vec![format!("(subpath (param \"{}\"))", root_param)];

            for (subpath_index, _) in root.read_only_subpaths.iter().enumerate() {
                let ro_param = format!("WRITABLE_ROOT_{index}_RO_{subpath_index}");
                parts.push(format!("(require-not (subpath (param \"{ro_param}\")))"));
            }

            policies.push(format!("(require-all {})", parts.join(" ")));
        }
    }

    if policies.is_empty() {
        return String::new();
    }

    // 使用 allow 组合所有写入策略
    format!("(allow file-write*\n  {})", policies.join("\n  "))
}

/// 生成策略中变量替换的参数定义。
///
/// sandbox-exec 允许使用 -DKEY=VALUE 来替换策略中的 `(param "KEY")`。
fn generate_params(policy: &SandboxPolicy, cwd: &Path) -> Vec<(String, PathBuf)> {
    let mut params = Vec::new();

    // Add writable root parameters
    let writable_roots = policy.get_writable_roots(cwd);

    for (index, root) in writable_roots.iter().enumerate() {
        let canonical = root
            .root
            .canonicalize()
            .unwrap_or_else(|_| root.root.clone());
        params.push((format!("WRITABLE_ROOT_{index}"), canonical));

        // 为只读子路径添加参数
        for (subpath_index, subpath) in root.read_only_subpaths.iter().enumerate() {
            let canonical_subpath = subpath.canonicalize().unwrap_or_else(|_| subpath.clone());
            params.push((
                format!("WRITABLE_ROOT_{index}_RO_{subpath_index}"),
                canonical_subpath,
            ));
        }
    }

    // Add Darwin user cache directory
    if let Some(cache_dir) = get_darwin_user_cache_dir() {
        params.push(("DARWIN_USER_CACHE_DIR".to_string(), cache_dir));
    } else {
        // 回退到合理的默认值
        if let Ok(home) = std::env::var("HOME") {
            params.push((
                "DARWIN_USER_CACHE_DIR".to_string(),
                PathBuf::from(format!("{home}/Library/Caches")),
            ));
        }
    }

    // Cargo home（#558）：与 `generate_policy` 在 `resolve_cargo_home()` 成功时
    // 发出的策略行配对。两个辅助函数使用相同的回退链，以便策略文本和
    // -DKEY=VALUE 参数保持同步——只发出其中一个而不发出另一个会导致
    // sandbox-exec 拒绝加载配置文件。
    if let Some(home) = resolve_cargo_home() {
        let canonical_home = home.canonicalize().unwrap_or_else(|_| home.clone());
        params.push((
            "CARGO_HOME_REGISTRY".to_string(),
            canonical_home.join("registry"),
        ));
        params.push(("CARGO_HOME_GIT".to_string(), canonical_home.join("git")));
        params.push(("CARGO_HOME".to_string(), canonical_home));
    }

    params
}

/// 使用 confstr 获取 Darwin 用户缓存目录。
///
/// 返回 macOS 分配的每用户缓存目录，
/// 通常是类似 /var/folders/xx/xxx.../C/ 的路径
fn get_darwin_user_cache_dir() -> Option<PathBuf> {
    // Use libc to call confstr for _CS_DARWIN_USER_CACHE_DIR
    let mut buf = vec![0i8; (libc::PATH_MAX as usize) + 1];

    // Safety: `buf` 是一个为 confstr 设置了 PATH_MAX + 1 大小的可写缓冲区。
    let len =
        unsafe { libc::confstr(libc::_CS_DARWIN_USER_CACHE_DIR, buf.as_mut_ptr(), buf.len()) };

    if len == 0 {
        return None;
    }

    // 将 C 字符串转换为 Rust PathBuf
    // Safety: confstr 保证当 len > 0 时 `buf` 中包含 NUL 结尾的字符串。
    let cstr = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
    let path_str = cstr.to_str().ok()?;
    let path = PathBuf::from(path_str);

    // 尝试规范化，如果失败则返回原始路径
    path.canonicalize().ok().or(Some(path))
}

/// 从命令输出检测沙箱拒绝。
///
/// 如果输出表明沙箱阻止了操作，则返回 true。
pub fn detect_denial(exit_code: i32, stderr: &str) -> bool {
    if exit_code == 0 {
        return false;
    }

    // 常见的沙箱拒绝消息
    let denial_patterns = [
        "Operation not permitted",
        "sandbox-exec",
        "deny(",
        "Sandbox: ",
    ];

    denial_patterns.iter().any(|p| stderr.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 序列化修改进程级环境变量（HOME, CARGO_HOME）的测试，
    /// 使它们不会与此 crate 中读取这些变量的其他测试发生竞争。
    /// 镜像了 main.rs::tests 中的模式（commit d06eaed0）。
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_is_available() {
        // 此测试仅检查函数不会 panic
        // 在 macOS 上应返回 true，在其他平台上返回 false
        let _ = is_available();
    }

    #[test]
    fn test_generate_policy_default() {
        let policy = SandboxPolicy::default();
        let cwd = Path::new("/tmp/test");
        let result = generate_policy(&policy, cwd);

        assert!(result.contains("(version 1)"));
        assert!(result.contains("(deny default)"));
        assert!(result.contains("(allow file-read*)"));
        assert!(result.contains("file-write*"));
        // Default policy has no network
        assert!(!result.contains("network-outbound"));
    }

    #[test]
    fn test_generate_policy_with_network() {
        let policy = SandboxPolicy::workspace_with_network();
        let cwd = Path::new("/tmp/test");
        let result = generate_policy(&policy, cwd);

        assert!(result.contains("network-outbound"));
        assert!(result.contains("network-inbound"));
    }

    #[test]
    fn test_generate_policy_read_only() {
        let policy = SandboxPolicy::ReadOnly;
        let cwd = Path::new("/tmp/test");
        let result = generate_policy(&policy, cwd);

        assert!(result.contains("(allow file-read*)"));
        // Should not have workspace write rules
        assert!(!result.contains("WRITABLE_ROOT"));
    }

    #[test]
    fn test_generate_params() {
        let policy = SandboxPolicy::default();
        let cwd = Path::new("/tmp/test");
        let params = generate_params(&policy, cwd);

        // Should have at least the cache dir param
        assert!(params.iter().any(|(k, _)| k == "DARWIN_USER_CACHE_DIR"));
    }

    /// #558: cargo publish reaches into ~/.cargo/registry; the seatbelt has
    /// to allow read+write inside it. Both the policy text and the param
    /// table must be in sync — emitting one without the other makes
    /// sandbox-exec refuse to load the profile.
    #[test]
    fn test_cargo_home_paths_emitted_in_policy_and_params_when_home_set() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

        // SAFETY: HOME / CARGO_HOME are process-global. ENV_LOCK serializes
        // all tests in this module that mutate them, and we always restore
        // the prior value before returning.
        let saved_home = std::env::var_os("HOME");
        let saved_cargo = std::env::var_os("CARGO_HOME");
        unsafe {
            std::env::set_var("HOME", "/tmp/seatbelt-cargo-test");
            std::env::remove_var("CARGO_HOME");
        }

        let policy = SandboxPolicy::default();
        let cwd = Path::new("/tmp/test");

        let policy_text = generate_policy(&policy, cwd);
        assert!(policy_text.contains(r#"(allow file-read* (subpath (param "CARGO_HOME")))"#));
        assert!(policy_text.contains("CARGO_HOME_REGISTRY"));
        assert!(policy_text.contains("CARGO_HOME_GIT"));

        let params = generate_params(&policy, cwd);
        assert!(params.iter().any(|(k, _)| k == "CARGO_HOME"));
        assert!(params.iter().any(|(k, _)| k == "CARGO_HOME_REGISTRY"));
        assert!(params.iter().any(|(k, _)| k == "CARGO_HOME_GIT"));

        // Read-only policy should still emit CARGO_HOME read rule but skip writes.
        let read_only_text = generate_policy(&SandboxPolicy::ReadOnly, cwd);
        assert!(
            read_only_text.contains(r#"(allow file-read* (subpath (param "CARGO_HOME")))"#),
            "read-only mode should still allow reading the cargo registry: {read_only_text}"
        );
        assert!(
            !read_only_text
                .contains(r#"(allow file-write* (subpath (param "CARGO_HOME_REGISTRY")))"#),
            "read-only mode must NOT grant write access to the cargo registry"
        );

        // Restore.
        // SAFETY: restoring the prior value the test stashed at entry.
        unsafe {
            match saved_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            match saved_cargo {
                Some(v) => std::env::set_var("CARGO_HOME", v),
                None => std::env::remove_var("CARGO_HOME"),
            }
        }
    }

    /// #558: if neither `CARGO_HOME` nor `HOME` is set, the cargo lines and
    /// their params must both be omitted — emitting one without the other
    /// would crash sandbox-exec on profile load.
    #[test]
    fn test_cargo_home_skipped_when_no_env() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

        let saved_home = std::env::var_os("HOME");
        let saved_cargo = std::env::var_os("CARGO_HOME");
        // SAFETY: HOME/CARGO_HOME are process-global; ENV_LOCK serializes
        // mutations here and we restore the prior values before returning.
        unsafe {
            std::env::remove_var("HOME");
            std::env::remove_var("CARGO_HOME");
        }

        let policy = SandboxPolicy::default();
        let cwd = Path::new("/tmp/test");
        let policy_text = generate_policy(&policy, cwd);
        let params = generate_params(&policy, cwd);

        assert!(!policy_text.contains("CARGO_HOME"));
        assert!(!params.iter().any(|(k, _)| k.starts_with("CARGO_HOME")));

        // Restore.
        // SAFETY: restoring the prior values the test stashed at entry.
        unsafe {
            match saved_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            match saved_cargo {
                Some(v) => std::env::set_var("CARGO_HOME", v),
                None => std::env::remove_var("CARGO_HOME"),
            }
        }
    }

    #[test]
    fn test_create_seatbelt_args() {
        let policy = SandboxPolicy::default();
        let cwd = Path::new("/tmp/test");
        let command = vec!["echo".to_string(), "hello".to_string()];

        let args = create_seatbelt_args(command, &policy, cwd);

        // Should start with -p and the policy
        assert_eq!(args[0], "-p");
        assert!(args[1].contains("(version 1)"));

        // Should contain the separator
        assert!(args.contains(&"--".to_string()));

        // Should end with the original command
        assert!(args.contains(&"echo".to_string()));
        assert!(args.contains(&"hello".to_string()));
    }

    #[test]
    fn test_detect_denial() {
        assert!(detect_denial(1, "Operation not permitted"));
        assert!(detect_denial(1, "Sandbox: ls denied file-write*"));
        assert!(!detect_denial(0, "Operation not permitted"));
        assert!(!detect_denial(1, "File not found"));
    }
}
