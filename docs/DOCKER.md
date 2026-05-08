# Docker

DeepSeek-TUI 在每个版本发布时会将多架构 Linux 镜像发布到 GitHub Container Registry。

```bash
docker pull ghcr.io/hmbown/deepseek-tui:latest
```

## 快速开始

使用你现有的配置目录挂载来运行已发布的镜像：

```bash
docker run --rm -it \
  -e DEEPSEEK_API_KEY="$DEEPSEEK_API_KEY" \
  -v ~/.deepseek:/home/deepseek/.deepseek \
  ghcr.io/hmbown/deepseek-tui:latest
```

使用固定版本标签以获得可重现的安装：

```bash
docker run --rm -it \
  -e DEEPSEEK_API_KEY="$DEEPSEEK_API_KEY" \
  -v ~/.deepseek:/home/deepseek/.deepseek \
  ghcr.io/hmbown/deepseek-tui:v0.8.20
```

## 本地构建

从仓库检出本地构建镜像：

```bash
docker build -t deepseek-tui .
```

然后使用你现有的配置目录挂载来运行：

```bash
docker run --rm -it \
  -e DEEPSEEK_API_KEY="$DEEPSEEK_API_KEY" \
  -v ~/.deepseek:/home/deepseek/.deepseek \
  deepseek-tui
```

未配置 Docker Hub 发布；GHCR 是受支持的预构建镜像仓库。

## 环境变量

| 变量                  | 必需    | 描述                                           |
|-----------------------|---------|------------------------------------------------|
| `DEEPSEEK_API_KEY`    | 是      | DeepSeek API 密钥                              |
| `DEEPSEEK_BASE_URL`   | 否      | 自定义 API 基础 URL（例如 `https://api.deepseek.com`） |
| `DEEPSEEK_NO_COLOR`   | 否      | 设为 `1` 以禁用终端彩色输出                     |

## 卷挂载

挂载 `~/.deepseek` 以在容器重启之间持久保存会话、配置、技能、记忆和离线队列：

```bash
-v ~/.deepseek:/home/deepseek/.deepseek
```

如果没有此挂载，容器每次都会从全新状态启动。

## 非交互 / 管道使用

当 stdin 不是 TTY 时，`deepseek` 会降级到调度器的一次性模式（`deepseek -c "…"`）。通过 stdin 管道输入提示：

```bash
echo "用结构化英语解释 Cargo.toml。" | \
  docker run --rm -i -e DEEPSEEK_API_KEY ghcr.io/hmbown/deepseek-tui:latest
```

## 本地构建

```bash
# 单平台（你的宿主机架构）
docker build -t deepseek-tui .

# 多平台（需要支持模拟的构建器）
docker buildx create --use
docker buildx build --platform linux/amd64,linux/arm64 -t deepseek-tui .
```

## Devcontainer

代码仓库包含一个 [`.devcontainer/devcontainer.json`](../.devcontainer/devcontainer.json) 配置文件，用于 VS Code / GitHub Codespaces。它会预装 Rust 工具链、rust-analyzer 和 `deepseek` 二进制文件。在 devcontainer 中打开仓库即可获得一个立即可用的开发环境。

## 发布状态

Docker 镜像发布是发布门控流程的一部分。镜像会以 semver 标签以及 `latest` 标签发布到 GHCR，支持 `linux/amd64` 和 `linux/arm64` 架构。
