# DeepSeek TUI 发布运行手册

本运行手册是发布 Rust crates、GitHub 发布资产和 `deepseek-tui` npm 包装器的权威指南。

当前打包说明：
- `deepseek-tui` 是当前向用户交付的活跃运行时和 TUI 包。
- `deepseek-tui-core` 是支持提取/对等工作的工作区 crate，并非交付运行时的替代品。

## 标准发布目标

- 最终用户 crate：
  - `deepseek-tui`
  - `deepseek-tui-cli`
- 从本工作区发布的支持 crate：
  - `deepseek-secrets`
  - `deepseek-config`
  - `deepseek-protocol`
  - `deepseek-state`
  - `deepseek-agent`
  - `deepseek-execpolicy`
  - `deepseek-hooks`
  - `deepseek-mcp`
  - `deepseek-tools`
  - `deepseek-core`
  - `deepseek-app-server`
  - `deepseek-tui-core`
- crates.io 上的 `deepseek-cli` 是一个无关的 crate，不属于本发布流程。

## 版本协调

- Rust crate 继承自 [Cargo.toml](../Cargo.toml) 中的共享工作区版本。
- 内部路径依赖版本应与共享工作区版本匹配；一旦工作区版本变更，过时的旧锁定版本将成为发布阻塞项。
- npm 包装器版本位于 [npm/deepseek-tui/package.json](../npm/deepseek-tui/package.json) 中。
- `deepseekBinaryVersion` 控制 npm 包装器下载哪个 GitHub 发布二进制文件。
- 仅更新包装器的 npm 发布是允许的：
  - 提升 npm 包版本
  - 将 `deepseekBinaryVersion` 锁定到先前发布的 Rust 二进制文件
  - 在 `npm publish` 之前重新运行 `npm pack` 烟雾测试

## 预检

在打标签之前，从仓库根目录运行以下命令：

```bash
./scripts/release/check-versions.sh   # 工作区、npm、lockfile 之间的版本漂移
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo publish --dry-run --locked --allow-dirty -p deepseek-tui
./scripts/release/publish-crates.sh dry-run
```

`check-versions.sh` 也在每次推送/PR 时在 CI 中运行（`.github/workflows/ci.yml` 中的 `versions` 任务），因此 `Cargo.toml`、各个 crate 清单、`npm/deepseek-tui/package.json` 和 `Cargo.lock` 之间的漂移会在发布时之前而非发布时被发现。

`publish-crates.sh dry-run` 对没有未发布工作区依赖的 crate 执行完整的 `cargo publish --dry-run`，并对依赖的工作区 crate 执行打包预检。这避免了因 crates.io 尚未包含新工作区版本而导致的假阴性，同时仍在发布前验证包内容。

对于 npm 包装器验证，构建两个发布的二进制文件并运行跨平台烟雾测试工具。它会打包 npm 包装器，将其安装到一个干净的临时项目中，通过 HTTP 提供本地发布资产，并检查调度器到 TUI 路径（`deepseek doctor --help`）和直接 TUI 入口点（`deepseek-tui --help`）。

```bash
cargo build --release --locked -p deepseek-tui-cli -p deepseek-tui
node scripts/release/npm-wrapper-smoke.js
```

设置 `DEEPSEEK_TUI_KEEP_SMOKE_DIR=1` 可保留临时打包/安装目录以供检查。

要在本地也运行 `npm run release:check`，在启动服务器前使用完整的资产矩阵夹具重新生成本地资产目录：

```bash
DEEPSEEK_TUI_PREPARE_ALL_ASSETS=1 node scripts/release/prepare-local-release-assets.js
cd npm/deepseek-tui
DEEPSEEK_TUI_VERSION=X.Y.Z DEEPSEEK_TUI_RELEASE_BASE_URL=http://127.0.0.1:8123/ npm run release:check
```

将 `DEEPSEEK_TUI_VERSION` 设置为该次本地运行正在验证的 npm 包版本。

CI 工作流在 Linux、macOS 和 Windows 上运行相同的 tarball 安装 + 委托入口点烟雾测试。

发布后，验证发布在两个注册表中均可见：

```bash
./scripts/release/check-published.sh X.Y.Z
```

在该命令在 npm 上看到 `deepseek-tui@X.Y.Z` 且在 crates.io 上看到每个 `deepseek-*` crate 均为 `X.Y.Z` 之前，不要将 Rust 发布标记为完成。对于罕见的仅 npm 包装器发布，使用 `--allow-npm-binary-mismatch` 运行，并在发布说明中明确说明没有发布新的 Rust 二进制版本。

## Rust Crates 发布

向 crates.io 发布 crate 是**手动的**——没有自动化的 `crates-publish` GitHub 工作流。操作员在配置了 `cargo login` 的开发工作站上运行 `scripts/release/` 中的辅助脚本。

1. 更新 [Cargo.toml](../Cargo.toml) 中的工作区版本。
2. 在本地运行 `./scripts/release/check-versions.sh` 和 `./scripts/release/publish-crates.sh dry-run`；两者必须干净。
3. 将发布标记为 `vX.Y.Z`（通常通过将版本提升推送到 `main` 并让 `auto-tag.yml` 创建标签——有关 `RELEASE_TAG_PAT` 要求，请参见下面的 npm 包装器发布章节）。
4. 使用 `./scripts/release/publish-crates.sh publish` 按此顺序发布 crate：
   - `deepseek-secrets`
   - `deepseek-config`
   - `deepseek-protocol`
   - `deepseek-state`
   - `deepseek-agent`
   - `deepseek-execpolicy`
   - `deepseek-hooks`
   - `deepseek-mcp`
   - `deepseek-tools`
   - `deepseek-core`
   - `deepseek-app-server`
   - `deepseek-tui-core`
   - `deepseek-tui-cli`
   - `deepseek-tui`
5. 在发布依赖 crate 之前，等待每个已发布的 crate 版本出现在 crates.io 上。

发布辅助脚本对重新运行是幂等的：已发布的 crate 版本会被跳过。

## GitHub 发布资产

`.github/workflows/release.yml` 构建以下二进制文件：

- `deepseek-linux-x64`
- `deepseek-macos-x64`
- `deepseek-macos-arm64`
- `deepseek-windows-x64.exe`
- `deepseek-tui-linux-x64`
- `deepseek-tui-macos-x64`
- `deepseek-tui-macos-arm64`
- `deepseek-tui-windows-x64.exe`

发布任务还会上传 `deepseek-artifacts-sha256.txt`。npm 安装程序和发布验证脚本都依赖此校验和清单。

## npm 包装器发布

**npm 发布步骤是手动的。** `release.yml` 不再运行 `npm publish`，因为 npm 帐户要求在每次发布时提供 2FA OTP，并且尚未配置绕过 2FA 的自动化令牌。GitHub 发布流程保持完全自动化；只有 npm 包装器发布需要开发者在配置了 `npm login` 和身份验证器应用的工作站上操作。

### 步骤

1. 在 [npm/deepseek-tui/package.json](../npm/deepseek-tui/package.json) 中设置 npm 包版本，使其与工作区 `Cargo.toml` 匹配。CI 的版本漂移检查会在打标签前捕获不匹配。
2. 将 `deepseekBinaryVersion` 设置为应提供二进制文件的 GitHub 发布标签。
3. 将版本提升推送到 `main`。`auto-tag.yml` 创建匹配的 `vX.Y.Z` 标签，`release.yml` 构建二进制矩阵并草拟 GitHub Release。
4. **等待 GitHub Release 完成**，包含所有八个已签名二进制文件以及 `deepseek-artifacts-sha256.txt`。npm 的 `prepublishOnly` 钩子（`scripts/verify-release-assets.js`）要求每个资产都存在。
5. 从开发机器手动发布 npm 包装器：

```bash
cd npm/deepseek-tui
npm publish --access public
# （系统会提示你从身份验证器输入 npm OTP）
```

### 为什么不自动化？

- `release.yml` 旧的 `publish-npm` 任务使用了 `secrets.NPM_TOKEN`，但 npm 的默认启用 2FA 策略意味着发布令牌必须是启用了"绕过 2FA 进行令牌认证"的自动化令牌，或者是帐户级别的 2FA 禁用状态。我们两者都未配置。
- 独立的 `publish-npm.yml` 和 `crates-publish.yml` 工作流已被移除；不再保留无用的自动化管道。未来转向 npm Trusted Publishing（OIDC）将在那时重新引入专用工作流。

### 如果你稍后修复了令牌

要重新启用自动化发布：配置一个启用了"绕过 2FA 进行令牌认证"的 npm 自动化令牌（或通过 OIDC 设置 npm Trusted Publishing），在仓库中存储对应的密钥，然后将 `publish-npm` 任务重新添加到 `release.yml`（或专用工作流），同时恢复此章节的"手动"描述。

## 恢复与回滚

- Crates 部分发布：
  - 重新运行 `./scripts/release/publish-crates.sh publish`
  - 已发布的 crate 版本将被跳过
- GitHub 资产缺失或校验和清单不完整：
  - 修复 `.github/workflows/release.yml`
  - 在 `npm publish` 之前重新打标签或上传修正后的资产
- npm 仅包装器问题：
  - 仅提升 npm 包版本
  - 将 `deepseekBinaryVersion` 保持在最后一个已知良好的 Rust 发布版本
  - 重新打包并重新发布包装器
- 错误的 npm 发布无法被覆盖：
  - 发布一个包含修正元数据或安装逻辑的新 npm 版本
