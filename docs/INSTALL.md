# 安装 DeepSeek TUI

本文档涵盖所有支持的安装路径以及最常见的"安装失败"问题，包括 **Linux ARM64** 和其他不太常见的平台。

如果您只想要简短版本，请参阅[主 README](../README.md#quickstart) 或[简体中文 README](../README.zh-CN.md#快速开始)。

---

## 1. 支持的平台

`deepseek-tui` 从 v0.8.8 开始为以下平台/架构组合提供预构建二进制文件：

| 平台 | 架构 | npm 安装 | `cargo install` | GitHub 发布资产 |
| --- | --- | :---: | :---: | --- |
| Linux | x64 (x86_64) | ✅ | ✅ | `deepseek-linux-x64`、`deepseek-tui-linux-x64` |
| Linux | arm64 | ✅ | ✅ | `deepseek-linux-arm64`、`deepseek-tui-linux-arm64` |
| macOS | x64 | ✅ | ✅ | `deepseek-macos-x64`、`deepseek-tui-macos-x64` |
| macOS | arm64 (M 系列) | ✅ | ✅ | `deepseek-macos-arm64`、`deepseek-tui-macos-arm64` |
| Windows | x64 | ✅ | ✅ | `deepseek-windows-x64.exe`、`deepseek-tui-windows-x64.exe` |
| 其他 Linux（musl、riscv64 等） | — | ❌¹ | ✅² | 从源码构建 |
| FreeBSD / OpenBSD | — | ❌ | ✅² | 从源码构建 |

¹ npm 包会退出并显示清晰的错误信息，引导您来这里。
² 前提是您的工具链可以编译最新的 Rust 工作区；请参阅下面的[从源码构建](#5-从源码构建)。

> **Linux ARM64 说明（v0.8.7 及更早版本）。** v0.8.7 及更早版本**不**发布 Linux ARM64 预构建；HarmonyOS 轻薄本、Asahi Linux、树莓派、AWS Graviton 等用户在使用 `npm i -g deepseek-tui` 时会看到 `Unsupported architecture: arm64`。v0.8.8 发布了 `deepseek-linux-arm64` 和 `deepseek-tui-linux-arm64`，因此在任何基于 glibc 的 ARM64 Linux 上，纯 `npm i -g deepseek-tui` 即可使用。如果您卡在 v0.8.7，请跳到[从源码构建](#5-从源码构建)——`cargo install` 完全可用。

---

## 2. 通过 npm 安装（推荐）

```bash
npm install -g deepseek-tui
deepseek
```

`postinstall` 脚本从匹配的 GitHub 发布版本下载正确的二进制文件对，验证 SHA-256 清单，并将 `deepseek` 和 `deepseek-tui` 都暴露在您的 `PATH` 上。

有用的环境变量：

| 变量 | 用途 |
| --- | --- |
| `DEEPSEEK_TUI_VERSION` | 固定包装器下载的发布版本（默认为 `deepseekBinaryVersion`） |
| `DEEPSEEK_TUI_GITHUB_REPO` | 将下载器指向一个 fork（`owner/repo`） |
| `DEEPSEEK_TUI_RELEASE_BASE_URL` | 覆盖下载根路径（例如内部镜像或发布资产代理） |
| `DEEPSEEK_TUI_FORCE_DOWNLOAD=1` | 即使缓存二进制标记匹配也重新下载 |
| `DEEPSEEK_TUI_DISABLE_INSTALL=1` | 完全跳过 `postinstall` 下载（CI 冒烟、打包二进制） |
| `DEEPSEEK_TUI_OPTIONAL_INSTALL=1` | 在下载/解压错误时不使 `npm install` 失败——在 CI 矩阵中有用 |

> **中国大陆 npm 下载慢？** 如果 `npm install` 本身慢（不仅仅是 postinstall 二进制下载慢），请使用 npm 注册表镜像：
> ```bash
> npm config set registry https://registry.npmmirror.com
> npm install -g deepseek-tui
> ```
> 如果您更偏好 Cargo 而非 npm，也可参阅[第 3 节](#3-通过-cargo-安装任何-tier-1-rust-目标)。

---

## 3. 通过 Cargo 安装（任何 Tier-1 Rust 目标）

如果 GitHub 发布版本慢、被屏蔽，或者您在使用不受支持的架构，请直接从 crates.io 安装。两个 crate 都是必需的——调度器在运行时会委托给 TUI 运行时。

```bash
# 需要 Rust 1.88+（https://rustup.rs）
cargo install deepseek-tui-cli --locked   # 提供 `deepseek`
cargo install deepseek-tui     --locked   # 提供 `deepseek-tui`
deepseek --version
```

### 中国大陆 / 镜像友好安装

在中国大陆安装时，同时配置 **rustup**（Rust 工具链安装程序）和 **Cargo**（包注册表）的镜像，以避免 TLS 超时和下载失败。

**步骤 1：通过 rustup 镜像安装 Rust**

```bash
# PowerShell
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
(New-Object Net.WebClient).DownloadFile('https://win.rustup.rs/x86_64', 'rustup-init.exe')

# git-bash / msys2
export RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup
export RUSTUP_UPDATE_ROOT=https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup
./rustup-init.exe -y --default-toolchain stable

# Linux / macOS
export RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup
export RUSTUP_UPDATE_ROOT=https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
```

如果从您的网络访问 TUNA 镜像较慢，`rsproxy.cn` 是另一个 Linux/macOS 的 rustup 镜像选项：

```bash
export RUSTUP_DIST_SERVER=https://rsproxy.cn
export RUSTUP_UPDATE_ROOT=https://rsproxy.cn/rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
```

`RUSTUP_DIST_SERVER` 和 `RUSTUP_UPDATE_ROOT` 环境变量必须在运行 rustup-init **之前**设置；否则工具链下载会遇到与安装程序相同的 TLS 握手问题。

**步骤 2：配置 Cargo 注册表镜像**

```toml
# ~/.cargo/config.toml
[source.crates-io]
replace-with = "tuna"

[source.tuna]
registry = "sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/"
```

`rsproxy`、腾讯 COS 和阿里云 OSS 镜像的工作方式相同；选择从您的网络最快的那个。

---

## 4. 从 GitHub Releases 手动下载

从[发布页面](https://github.com/Hmbown/DeepSeek-TUI/releases)获取匹配平台的二进制文件对，并将它们放在 `PATH` 中的目录下（例如 `~/.local/bin`）：

```bash
# Linux ARM64 示例
mkdir -p ~/.local/bin
curl -L -o ~/.local/bin/deepseek      \
    https://github.com/Hmbown/DeepSeek-TUI/releases/latest/download/deepseek-linux-arm64
curl -L -o ~/.local/bin/deepseek-tui  \
    https://github.com/Hmbown/DeepSeek-TUI/releases/latest/download/deepseek-tui-linux-arm64
chmod +x ~/.local/bin/deepseek ~/.local/bin/deepseek-tui
deepseek --version
```

根据每个发布的 SHA-256 清单验证完整性：

```bash
curl -L -o /tmp/deepseek-artifacts-sha256.txt \
    https://github.com/Hmbown/DeepSeek-TUI/releases/latest/download/deepseek-artifacts-sha256.txt
( cd ~/.local/bin && sha256sum -c /tmp/deepseek-artifacts-sha256.txt --ignore-missing )
```

（在 macOS 上使用 `shasum -a 256 -c` 代替 `sha256sum`。）

### Windows Scoop

DeepSeek TUI 已进入 Scoop 的主 bucket：

```powershell
scoop update
scoop install deepseek-tui
deepseek --version
```

Scoop manifest 在本仓库发布流程之外维护，可能滞后于 GitHub/npm/Cargo 发布版本。当您需要立即使用最新版本时，请使用 npm 或手动 GitHub 发布版本下载。

---

## 5. 从源码构建

这是我们不提供发布的任何平台的通用方法——包括 musl、riscv64、LoongArch、FreeBSD 和 2024 年之前的 ARM64 发行版。

### 前置条件

- **Rust** 1.88 或更高版本——使用 [rustup](https://rustup.rs) 安装。
- **Linux 构建时依赖**（Debian/Ubuntu/openEuler/Kylin）：
  ```bash
  sudo apt-get install -y build-essential pkg-config libdbus-1-dev
  # openEuler / RHEL 系列：
  # sudo dnf install -y gcc make pkgconf-pkg-config dbus-devel
  ```
- 不需要可用的 `cmake`。

### 构建和安装

```bash
git clone https://github.com/Hmbown/DeepSeek-TUI.git
cd DeepSeek-TUI

cargo install --path crates/cli --locked   # 提供 `deepseek`
cargo install --path crates/tui --locked   # 提供 `deepseek-tui`

deepseek --version
```

两个二进制文件默认安装在 `~/.cargo/bin/` 中；请确保该目录在您的 `PATH` 上。

### 从 x64 交叉编译到 ARM64 Linux

如果您想在一台 x64 Linux 主机上构建 ARM64 Linux 二进制文件（例如为 HarmonyOS / openEuler ARM64 轻薄本构建），请使用 [`cross`](https://github.com/cross-rs/cross)，它将官方的 Rust 交叉目标封装在 Docker 容器中：

```bash
# 一次性操作
rustup target add aarch64-unknown-linux-gnu
cargo install cross --locked

# 每次构建
cross build --release --target aarch64-unknown-linux-gnu -p deepseek-tui-cli
cross build --release --target aarch64-unknown-linux-gnu -p deepseek-tui
```

生成的二进制文件位于 `target/aarch64-unknown-linux-gnu/release/deepseek` 和 `target/aarch64-unknown-linux-gnu/release/deepseek-tui`。将匹配的对复制到 ARM64 主机（例如通过 `scp`）并执行 `chmod +x`。

如果您没有 Docker，直接安装交叉链接器并让 Cargo 完成工作：

```bash
sudo apt-get install -y gcc-aarch64-linux-gnu
rustup target add aarch64-unknown-linux-gnu

cat >> ~/.cargo/config.toml <<'EOF'
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
EOF

cargo build --release --target aarch64-unknown-linux-gnu -p deepseek-tui-cli
cargo build --release --target aarch64-unknown-linux-gnu -p deepseek-tui
```

如果您的发行版基于 musl，同样的配方也适用于 `aarch64-unknown-linux-musl`。

### 从源码在 Windows 上构建

在 Windows 上构建需要来自 [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022) 的 **MSVC C 工具链**（免费的可选择工作负载的安装程序，不是完整 IDE）。

**前置条件（Windows）**

1. 安装 Visual Studio 2022 Build Tools——选择 **"Desktop development with C++"** 工作负载。
2. 安装 [Rust](https://rustup.rs) 1.88+（如果从中国大陆下载，请参阅上面的[中国镜像说明](#中国大陆--镜像友好安装)）。
3. 安装 [Git for Windows](https://git-scm.com/download/win)（提供 `git` 和 `git-bash` 终端）。

**推荐终端**：Windows Terminal、`git-bash` 或 PowerShell。`cmd.exe` 可用但缓冲区小且 PATH 行为有限。

**设置 MSVC 环境**

Visual Studio Build Tools 将 `cl.exe` 安装到版本化目录，但**不**将其全局添加到 `PATH`。您必须手动设置环境或使用开发人员命令提示符。所需的变量是：

```powershell
# 调整版本号以匹配您的安装
$msvc = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207"
$sdk   = "C:\Program Files (x86)\Windows Kits\10"
$sdkv  = "10.0.26100.0"

$env:INCLUDE  = "$msvc\include;$msvc\atlmfc\include;$sdk\Include\$sdkv\ucrt;$sdk\Include\$sdkv\um;$sdk\Include\$sdkv\shared"
$env:LIB      = "$msvc\lib\x64;$msvc\atlmfc\lib\x64;$sdk\Lib\$sdkv\ucrt\x64;$sdk\Lib\$sdkv\um\x64"
$env:LIBPATH  = "$msvc\lib\x64;$msvc\atlmfc\lib\x64"
$env:CC       = "$msvc\bin\Hostx64\x64\cl.exe"
$env:CXX      = "$msvc\bin\Hostx64\x64\cl.exe"
$env:PATH     = "$msvc\bin\Hostx64\x64;$env:PATH"
```

或者，打开 **"Developer Command Prompt for VS 2022"**（安装 Build Tools 后可从开始菜单找到），它会运行 `vcvars64.bat` 自动配置上述所有内容。然后在该会话中将 `cargo` 添加到 `PATH`，并从项目根目录运行 `cargo build`。

**Cargo 注册表镜像**——在 Windows 上，镜像配置放在 `%USERPROFILE%\.cargo\config.toml`。请参阅上面的[步骤 2](#中国大陆--镜像友好安装)。

**构建**

```bash
git clone https://github.com/Hmbown/DeepSeek-TUI.git
cd DeepSeek-TUI
set CARGO_HTTP_CHECK_REVOKE=false   # 在某些中国 ISP 后面可能需要
cargo build --release
```

两个二进制文件出现在 `target\release\deepseek.exe` 和 `target\release\deepseek-tui.exe`。

> **除非您需要修改源码，否则在 Windows 上优先使用 `npm install -g`。** npm 包拉取预构建二进制文件，完全避免了 C 工具链依赖——请参阅[第 2 节](#2-通过-npm-安装推荐)。

---

## 6. 故障排除

### `Unsupported architecture: arm64 on platform linux`

您在使用早于 v0.8.8 的版本，该版本不发布 Linux ARM64 二进制文件。要么升级（`npm i -g deepseek-tui@latest`），要么使用 `cargo install`（参见[第 3 节](#3-通过-cargo-安装任何-tier-1-rust-目标)）。

### 运行时 `MISSING_COMPANION_BINARY`

调度器（`deepseek`）需要 TUI 运行时（`deepseek-tui`）在同一 `PATH` 上。如果您只通过 `cargo install` 安装了一个 crate，请安装两个：

```bash
cargo install deepseek-tui-cli --locked
cargo install deepseek-tui     --locked
```

### `deepseek update` 报告 `no asset found for platform deepseek-linux-aarch64`

这是 v0.8.7 中的 [#503](https://github.com/Hmbown/DeepSeek-TUI/issues/503)——自更新器使用了 Rust 的 `aarch64`/`x86_64` 架构名称，而不是发布资产的 `arm64`/`x64`。在 v0.8.8 之前的解决方法：

```bash
npm i -g deepseek-tui@latest
# 或
cargo install deepseek-tui-cli --locked
```

### 从中国大陆下载 npm 慢或超时

将 `DEEPSEEK_TUI_RELEASE_BASE_URL` 设置为镜像的发布资产目录（rsproxy、TUNA、腾讯 COS、阿里云 OSS），或完全跳过 npm，使用[第 3 节](#3-通过-cargo-安装任何-tier-1-rust-目标)中的 Cargo 镜像设置。

### Debian/Ubuntu：`cargo install` 报错 `feature edition2024 is required`

某些 Debian/Ubuntu 发行版包提供的 Cargo 版本过老，无法解析 Rust 2024 crate。例如，Cargo 1.75.0 在构建前失败，报错：

```text
feature `edition2024` is required
```

通过 rustup 安装当前稳定的 Rust，然后重新运行[第 3 节](#3-通过-cargo-安装任何-tier-1-rust-目标)中的两个 Cargo install 命令。rustup 完成后，`which cargo` 应指向 `~/.cargo/bin/cargo`，而不是 `/usr/bin/cargo`。

### Debian/Ubuntu：构建时 `error: linker 'cc' not found`

安装 C 工具链：

```bash
sudo apt-get install -y build-essential pkg-config libdbus-1-dev
```

### 包装器安装但找不到 `deepseek`

`npm i -g` 安装到 `$(npm prefix -g)/bin`；请确保该目录在您的 shell 的 `PATH` 上。使用 nvm：`nvm use --lts && hash -r`。

### Windows：`rustup-init` 出现 `TLS handshake eof` 或 `CRYPT_E_REVOCATION_OFFLINE`

与 `static.rust-lang.org` 的 TLS 握手在 GFW 或某些中国 ISP 后面失败。在运行安装程序**之前**设置 rustup 镜像环境变量：

```bash
# git-bash / msys2
export RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup
export RUSTUP_UPDATE_ROOT=https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup
./rustup-init.exe -y --default-toolchain stable
```

如果在 Rust 安装后 Cargo 出现 `CRYPT_E_REVOCATION_OFFLINE`，还在 `cargo build` 期间设置 `CARGO_HTTP_CHECK_REVOKE=false`。

### Windows：`cargo build` 期间找不到 MSVC 编译器（`cl.exe`）

Visual Studio Build Tools 不会将 `cl.exe` 添加到全局 `PATH`。要么：

1. 从开始菜单打开 **"Developer Command Prompt for VS 2022"**，在该窗口中添加 `%USERPROFILE%\.cargo\bin` 到 `PATH`，并从那里运行 `cargo build`；要么
2. 手动设置 MSVC 环境变量——请参阅[从源码在 Windows 上构建](#从源码在-windows-上构建)部分的 PowerShell 代码片段。

验证编译器是否可达：`cl.exe /?` 应打印帮助文本。

### Windows：Cargo 执行构建脚本时 `拒绝访问 (os error 5)`

第三方防病毒软件（火绒、360、卡巴斯基等）可能会阻止 Cargo 执行新编译的构建脚本二进制文件（例如 `libsqlite3-sys`、`aws-lc-sys`、`instability`）。该错误与路径无关——移动 `target-dir` 没有帮助。

**症状**：`could not execute process ... build-script-build (never executed)`

**解决方法**（选一个）：

1. **将项目的 `target/` 目录添加到您的防病毒排除列表。**
2. **在 `cargo build` 期间临时关闭防病毒软件。**
3. **改用 `npm install -g deepseek-tui`**——npm 包提供预构建二进制文件，完全跳过 Cargo 构建（[第 2 节](#2-通过-npm-安装推荐)）。
4. **从 crates.io 使用 `cargo install deepseek-tui-cli --locked`**——这会更改二进制路径，某些防病毒软件会区别对待。

要验证构建脚本二进制文件本身是否有效（未损坏），在 `target/debug/build/<crate>/build-script-build` 下找到它并手动运行：

```bash
target/debug/build/libsqlite3-sys-*/build-script-build
# 如果运行但出现 "NotPresent" 恐慌（没有 C 编译器），则二进制文件没问题——
# 是防病毒软件专门阻止了 Cargo 的进程生成路径。
```

---

## 7. 验证您的安装

```bash
deepseek --version
deepseek doctor       # 检查 API 密钥、提供方、运行时和 PATH 完整性
deepseek doctor --json
```

`doctor` 在发现问题时以非零退出码退出，并打印结构化的修复提示。如果您需要帮助，请将 JSON 输出粘贴到 GitHub issue 中。