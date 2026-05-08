# deepseek-tui

从 GitHub 发布产物安装并运行 `deepseek` 和 `deepseek-tui` 二进制文件。

## 安装

```bash
npm install -g deepseek-tui
# 或
pnpm add -g deepseek-tui
```

用于项目本地使用：

```bash
npm install deepseek-tui
npx deepseek-tui --help
```

`postinstall` 会尝试将平台二进制文件下载到 `bin/downloads/`，并暴露 `deepseek` 和 `deepseek-tui` 命令。如果 GitHub 发布资源暂时不可访问，安装会继续，包装器会在首次运行时重试下载。

## 首次运行

```bash
deepseek login --api-key "YOUR_DEEPSEEK_API_KEY"
deepseek doctor
deepseek
```

`deepseek` 外观命令和 `deepseek-tui` 二进制文件共享 `~/.deepseek/config.toml` 以配置 DeepSeek 认证和默认模型设置。常见的 TUI 命令可直接通过外观命令使用，包括 `deepseek doctor`、`deepseek models`、`deepseek sessions` 和 `deepseek resume --last`。

该应用程序与 DeepSeek 记录的 OpenAI 兼容 Chat Completions API 通信。仅当您需要中国端点或 DeepSeek 测试版功能（如严格工具模式、对话前缀补全或 FIM 补全）时，才设置 `DEEPSEEK_BASE_URL`。

NVIDIA NIM 托管的 DeepSeek V4 Pro 也受支持：

```bash
deepseek auth set --provider nvidia-nim --api-key "YOUR_NVIDIA_API_KEY"
deepseek --provider nvidia-nim
```

对于单个进程，设置 `DEEPSEEK_PROVIDER=nvidia-nim` 和 `NVIDIA_API_KEY` 或 `NVIDIA_NIM_API_KEY`（使用 `DEEPSEEK_API_KEY` 作为兼容性回退）。NIM 默认模型是 `deepseek-ai/deepseek-v4-pro`，默认基础 URL 是 `https://integrate.api.nvidia.com/v1`。使用 `--provider nvidia-nim` 时，`--model deepseek-v4-flash` 映射到 `deepseek-ai/deepseek-v4-flash`。

## 支持的平台

GitHub 发布的预构建二进制文件会自动下载：

- Linux x64
- Linux arm64（v0.8.8+）
- macOS x64 / arm64
- Windows x64

其他平台/架构组合（musl、riscv64、FreeBSD 等）不提供预构建分发。不支持的平台、校验和失败以及 glibc 兼容性问题仍会以清晰的错误信息提示，引导您使用 `cargo install deepseek-tui-cli deepseek-tui --locked` 并参考完整的 [docs/INSTALL.md](https://github.com/Hmbown/DeepSeek-TUI/blob/main/docs/INSTALL.md) 从源码构建指南。

## 配置

- 默认二进制版本来自 `package.json` 中的 `deepseekBinaryVersion`。
- 设置 `DEEPSEEK_TUI_VERSION` 或 `DEEPSEEK_VERSION` 以覆盖发布版本。
- 设置 `DEEPSEEK_TUI_GITHUB_REPO` 或 `DEEPSEEK_GITHUB_REPO` 以覆盖源仓库（默认为 `Hmbown/DeepSeek-TUI`）。
- 当 GitHub Releases 不可用时，设置 `DEEPSEEK_TUI_RELEASE_BASE_URL` 以使用内部或镜像的发布资源目录。该目录必须包含 `deepseek-artifacts-sha256.txt` 和平台二进制文件。
- 设置 `DEEPSEEK_TUI_FORCE_DOWNLOAD=1` 以强制下载，即使缓存二进制文件已存在。
- 设置 `DEEPSEEK_TUI_DISABLE_INSTALL=1` 以跳过安装时下载。
- 设置 `DEEPSEEK_TUI_OPTIONAL_INSTALL=1` 使安装时的可重试下载失败仅发出警告并以退出码 `0` 退出，而不是导致 `npm install` 失败。

## 发布完整性

- `npm publish` 在发布前会执行发布资源检查，确保目标 GitHub 发布版本存在所有必需的二进制资源。
- 安装时的下载在包装器将其标记为可执行文件之前，会针对发布校验和清单进行验证。
