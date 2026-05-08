//! `deepseek` 二进制文件的自我更新。
//!
//! `update` 子命令从 `github.com/Hmbown/DeepSeek-TUI/releases/latest` 获取最新发布，
//! 下载与平台匹配的二进制文件，验证其 SHA256 校验和，并以原子方式替换当前运行的二进制文件。

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use std::io::Write;

/// 运行自我更新流程。
pub fn run_update() -> Result<()> {
    let current_exe =
        std::env::current_exe().context("无法确定当前可执行文件路径")?;

    println!("正在检查更新...");
    println!("当前二进制文件: {}", current_exe.display());

    let binary_name =
        release_asset_stem_for(&current_exe, std::env::consts::OS, std::env::consts::ARCH);

    // 步骤 1：获取最新发布元数据
    let release = fetch_latest_release()?;
    let latest_tag = &release.tag_name;
    println!("最新发布: {latest_tag}");

    // 步骤 2：查找匹配的资产
    let asset = select_platform_asset(&release, &binary_name).with_context(|| {
        format!(
            "在发布 {latest_tag} 中未找到平台 {binary_name} 的资产。\
                 可用资产: {}",
            release
                .assets
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    println!("正在下载 {}...", asset.name);

    // 步骤 3：下载资产
    let bytes = download_url(&asset.browser_download_url)
        .with_context(|| format!("下载 {} 失败", asset.name))?;

    // 步骤 4：如有可用则下载 SHA256 校验和文件
    let sha_url = format!("{}.sha256", asset.browser_download_url);
    let expected_hash = match download_url(&sha_url) {
        Ok(sha_bytes) => {
            let sha_text = String::from_utf8_lossy(&sha_bytes);
            // 解析 "hash  filename" 格式
            sha_text.split_whitespace().next().map(|s| s.to_string())
        }
        Err(_) => {
            println!("  （未找到 SHA256 校验和文件；跳过验证）");
            None
        }
    };

    // 步骤 5：如有校验和则进行验证
    if let Some(expected) = &expected_hash {
        let actual = sha256_hex(&bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            bail!("SHA256 不匹配！\n  期望值: {expected}\n  实际值:   {actual}");
        }
        println!("SHA256 校验和已验证。");
    }

    // 步骤 6：以原子方式替换当前二进制文件
    replace_binary(&current_exe, &bytes)?;

    println!(
        "\n✅ 已成功更新至 {latest_tag}！\n\
         新二进制文件: {}\n\
         \n\
         请重新启动应用程序以使用新版本。",
        current_exe.display()
    );

    Ok(())
}

pub(crate) fn release_arch_for_rust_arch(arch: &str) -> &str {
    match arch {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    }
}

pub(crate) fn binary_prefix_for_exe(current_exe: &Path) -> &'static str {
    let exe_name = current_exe
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("deepseek");
    if exe_name.contains("deepseek-tui") {
        "deepseek-tui"
    } else {
        "deepseek"
    }
}

pub(crate) fn release_asset_stem_for(current_exe: &Path, os: &str, rust_arch: &str) -> String {
    let prefix = binary_prefix_for_exe(current_exe);
    let arch = release_arch_for_rust_arch(rust_arch);
    format!("{prefix}-{os}-{arch}")
}

pub(crate) fn asset_matches_platform(asset_name: &str, binary_name: &str) -> bool {
    if asset_name.ends_with(".sha256") {
        return false;
    }
    asset_name == binary_name
        || asset_name == format!("{binary_name}.exe")
        || asset_name.starts_with(&format!("{binary_name}."))
}

fn select_platform_asset<'a>(release: &'a Release, binary_name: &str) -> Option<&'a Asset> {
    release
        .assets
        .iter()
        .find(|asset| asset_matches_platform(&asset.name, binary_name))
}

/// GitHub 发布元数据。
#[derive(serde::Deserialize, Debug)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

/// 单个发布资产。
#[derive(serde::Deserialize, Debug)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// 从 GitHub 获取最新发布元数据。
fn fetch_latest_release() -> Result<Release> {
    let url = "https://api.github.com/repos/Hmbown/DeepSeek-TUI/releases/latest";
    let output = Command::new("curl")
        .args([
            "-sSfL",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: deepseek-tui-updater",
            url,
        ])
        .output()
        .context("运行 curl 获取发布信息失败")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("curl 失败: {stderr}");
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let release: Release = serde_json::from_str(&body).with_context(|| {
        format!("无法从 GitHub API 解析发布 JSON。响应: {body}")
    })?;

    Ok(release)
}

/// 使用 curl 下载 URL 至字节数组。
fn download_url(url: &str) -> Result<Vec<u8>> {
    let output = Command::new("curl")
        .args(["-sSfL", url])
        .output()
        .with_context(|| format!("下载 {url} 失败"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("curl 下载失败: {stderr}");
    }

    Ok(output.stdout)
}

/// 计算数据的 SHA256 十六进制摘要。
fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(data);
    format!("{hash:x}")
}

/// 替换正在运行的二进制文件。
///
/// 将新二进制文件写入目标目录中的安全临时文件，然后原位安装。
/// Unix 可以原子方式替换可执行文件路径。在 Windows 上，替换正在运行的
/// 可执行文件可能会失败，因此先将当前文件移走，再将新二进制文件移入原始路径。
fn replace_binary(target: &Path, new_bytes: &[u8]) -> Result<()> {
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let mut tmp = tempfile::Builder::new()
        .prefix(".deepseek-update-")
        .tempfile_in(parent)
        .with_context(|| format!("无法在 {} 中创建临时文件", parent.display()))?;
    tmp.write_all(new_bytes)
        .with_context(|| format!("无法写入临时文件 {}", tmp.path().display()))?;

    // 保留原始二进制文件的权限（如果存在）
    if target.exists() {
        if let Ok(meta) = std::fs::metadata(target) {
            let _ = std::fs::set_permissions(tmp.path(), meta.permissions());
        }
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o755));
        }
    }

    #[cfg(windows)]
    {
        let backup = backup_path_for(target);
        if target.exists() {
            std::fs::rename(target, &backup).with_context(|| {
                format!(
                    "无法将当前可执行文件 {} 移至 {}",
                    target.display(),
                    backup.display()
                )
            })?;
        }

        if let Err(err) = tmp.persist(target) {
            if backup.exists() {
                let _ = std::fs::rename(&backup, target);
            }
            bail!(
                "无法在新位置 {} 安装二进制文件: {}",
                target.display(),
                err.error
            );
        }

        let _ = std::fs::remove_file(&backup);
    }

    #[cfg(not(windows))]
    {
        tmp.persist(target)
            .map_err(|err| err.error)
            .with_context(|| format!("无法将临时文件重命名为 {}", target.display()))?;
    }

    Ok(())
}

#[cfg(windows)]
fn backup_path_for(target: &Path) -> std::path::PathBuf {
    let pid = std::process::id();
    for index in 0..100 {
        let mut candidate = target.to_path_buf();
        let suffix = if index == 0 {
            format!("old-{pid}")
        } else {
            format!("old-{pid}-{index}")
        };
        candidate.set_extension(suffix);
        if !candidate.exists() {
            return candidate;
        }
    }
    target.with_extension(format!("old-{pid}-fallback"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证用于构造资产名称的架构映射。
    /// 该映射必须使用发布资产命名（arm64/x64），而不是 Rust
    /// 标准库常量（aarch64/x86_64）。
    #[test]
    fn test_arch_mapping() {
        assert_eq!(release_arch_for_rust_arch("aarch64"), "arm64");
        assert_eq!(release_arch_for_rust_arch("x86_64"), "x64");
        // 对于未知架构直通
        assert_eq!(release_arch_for_rust_arch("riscv64"), "riscv64");
        // 当前编译的架构映射到发布资产名称
        let compiled_arch = std::env::consts::ARCH;
        let asset_arch = release_arch_for_rust_arch(compiled_arch);
        // 不得包含原始的 Rust 常量名称
        assert!(
            !asset_arch.contains("aarch64") && !asset_arch.contains("x86_64"),
            "资产架构 '{asset_arch}' 仍使用原始的 Rust 常量名称"
        );
    }

    /// 验证调度器与 TUI 二进制文件的前缀检测。
    #[test]
    fn test_binary_prefix_detection() {
        // TUI 二进制文件应使用 deepseek-tui 前缀
        assert_eq!(
            binary_prefix_for_exe(Path::new("deepseek-tui")),
            "deepseek-tui"
        );
        assert_eq!(
            binary_prefix_for_exe(Path::new("deepseek-tui.exe")),
            "deepseek-tui"
        );
        assert_eq!(
            binary_prefix_for_exe(Path::new("/usr/local/bin/deepseek-tui")),
            "deepseek-tui"
        );

        // 调度器二进制文件应使用 deepseek 前缀
        assert_eq!(binary_prefix_for_exe(Path::new("deepseek")), "deepseek");
        assert_eq!(binary_prefix_for_exe(Path::new("deepseek.exe")), "deepseek");
        assert_eq!(
            binary_prefix_for_exe(Path::new("/usr/local/bin/deepseek")),
            "deepseek"
        );

        // 未知名称的备用方案
        assert_eq!(binary_prefix_for_exe(Path::new("other-binary")), "deepseek");
    }

    #[test]
    fn test_release_asset_stem_for_supported_platforms() {
        let cases = [
            ("deepseek", "macos", "aarch64", "deepseek-macos-arm64"),
            ("deepseek", "macos", "x86_64", "deepseek-macos-x64"),
            ("deepseek", "linux", "x86_64", "deepseek-linux-x64"),
            ("deepseek", "windows", "x86_64", "deepseek-windows-x64"),
            (
                "deepseek-tui",
                "macos",
                "aarch64",
                "deepseek-tui-macos-arm64",
            ),
            ("deepseek-tui", "linux", "x86_64", "deepseek-tui-linux-x64"),
        ];

        for (exe, os, arch, expected) in cases {
            assert_eq!(release_asset_stem_for(Path::new(exe), os, arch), expected);
        }
    }

    #[test]
    fn test_asset_matching_accepts_binary_assets_and_rejects_checksums() {
        assert!(asset_matches_platform(
            "deepseek-macos-arm64",
            "deepseek-macos-arm64"
        ));
        assert!(asset_matches_platform(
            "deepseek-macos-arm64.tar.gz",
            "deepseek-macos-arm64"
        ));
        assert!(asset_matches_platform(
            "deepseek-tui-windows-x64.exe",
            "deepseek-tui-windows-x64"
        ));
        assert!(!asset_matches_platform(
            "deepseek-tui-windows-x64.exe.sha256",
            "deepseek-tui-windows-x64"
        ));
        assert!(!asset_matches_platform(
            "deepseek-macos-aarch64.tar.gz",
            "deepseek-macos-arm64"
        ));
    }

    #[test]
    fn test_sha256_hex_known_value() {
        let data = b"hello";
        let hash = sha256_hex(data);
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_sha256_hex_empty() {
        let hash = sha256_hex(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_replace_binary_creates_and_replaces() {
        let dir = tempfile::TempDir::new().unwrap();
        let target = dir.path().join("deepseek-test");
        // 写入初始内容
        std::fs::write(&target, b"old binary").unwrap();

        replace_binary(&target, b"new binary content").unwrap();
        let content = std::fs::read_to_string(&target).unwrap();
        assert_eq!(content, "new binary content");
    }

    #[test]
    fn test_replace_binary_creates_new_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let target = dir.path().join("deepseek-new-test");

        replace_binary(&target, b"fresh binary").unwrap();
        let content = std::fs::read_to_string(&target).unwrap();
        assert_eq!(content, "fresh binary");
    }

    /// 模拟的 GitHub 发布载荷，涵盖调度器（`deepseek`）和旧版 TUI（`deepseek-tui`）
    /// 二进制文件在我们发布的平台/架构矩阵上，以及一个永远不应被选为主二进制文件的
    /// 校验和兄弟文件。
    fn mocked_release() -> Release {
        let json = r#"{
          "tag_name": "v0.8.8",
          "assets": [
            { "name": "deepseek-linux-x64",          "browser_download_url": "https://example.invalid/deepseek-linux-x64" },
            { "name": "deepseek-macos-x64",          "browser_download_url": "https://example.invalid/deepseek-macos-x64" },
            { "name": "deepseek-macos-arm64",        "browser_download_url": "https://example.invalid/deepseek-macos-arm64" },
            { "name": "deepseek-windows-x64.exe",    "browser_download_url": "https://example.invalid/deepseek-windows-x64.exe" },
            { "name": "deepseek-windows-x64.exe.sha256", "browser_download_url": "https://example.invalid/deepseek-windows-x64.exe.sha256" },
            { "name": "deepseek-tui-linux-x64",      "browser_download_url": "https://example.invalid/deepseek-tui-linux-x64" },
            { "name": "deepseek-tui-macos-x64",      "browser_download_url": "https://example.invalid/deepseek-tui-macos-x64" },
            { "name": "deepseek-tui-macos-arm64",    "browser_download_url": "https://example.invalid/deepseek-tui-macos-arm64" },
            { "name": "deepseek-tui-windows-x64.exe","browser_download_url": "https://example.invalid/deepseek-tui-windows-x64.exe" }
          ]
        }"#;
        serde_json::from_str(json).expect("模拟发布 JSON")
    }

    #[test]
    fn mocked_release_selects_dispatcher_asset_for_supported_platforms() {
        let release = mocked_release();
        let cases = [
            ("macos", "aarch64", "deepseek-macos-arm64"),
            ("macos", "x86_64", "deepseek-macos-x64"),
            ("linux", "x86_64", "deepseek-linux-x64"),
            ("windows", "x86_64", "deepseek-windows-x64.exe"),
        ];

        for (os, arch, expected) in cases {
            let stem = release_asset_stem_for(Path::new("/usr/local/bin/deepseek"), os, arch);
            let asset = select_platform_asset(&release, &stem)
                .unwrap_or_else(|| panic!("{os}/{arch} 无资产（stem {stem}）"));
            assert_eq!(asset.name, expected, "{os}/{arch}");
        }
    }

    #[test]
    fn mocked_release_selects_tui_asset_when_tui_binary_invokes_update() {
        let release = mocked_release();
        let stem =
            release_asset_stem_for(Path::new("/usr/local/bin/deepseek-tui"), "macos", "aarch64");
        let asset = select_platform_asset(&release, &stem).expect("TUI 平台资产");
        assert_eq!(asset.name, "deepseek-tui-macos-arm64");
    }
}
