# 依赖关系图

## Crate 依赖关系（来自 Cargo.toml）

```
deepseek-tui (二进制: `deepseek-tui`)
  (无工作空间依赖——单体源码位于 crates/tui/src/ 下)

deepseek-tui-cli (二进制: `deepseek`)
  <- deepseek-agent
  <- deepseek-app-server
  <- deepseek-config
  <- deepseek-execpolicy
  <- deepseek-mcp
  <- deepseek-state

deepseek-app-server
  <- deepseek-agent
  <- deepseek-config
  <- deepseek-core
  <- deepseek-execpolicy
  <- deepseek-hooks
  <- deepseek-mcp
  <- deepseek-protocol
  <- deepseek-state
  <- deepseek-tools

deepseek-core (代理循环)
  <- deepseek-agent
  <- deepseek-config
  <- deepseek-execpolicy
  <- deepseek-hooks
  <- deepseek-mcp
  <- deepseek-protocol
  <- deepseek-state
  <- deepseek-tools

deepseek-tools      <- deepseek-protocol
deepseek-mcp        <- deepseek-protocol
deepseek-hooks      <- deepseek-protocol
deepseek-execpolicy <- deepseek-protocol
deepseek-agent      <- deepseek-config

deepseek-config     (叶子——无内部依赖)
deepseek-protocol   (叶子——无内部依赖)
deepseek-state      (叶子——无内部依赖)
deepseek-tui-core   (叶子——无内部依赖)
```

注意：`deepseek-tui` 没有工作空间依赖，因为它仍然编译单体源码树（`crates/tui/src/main.rs`）。crate 拆分是结构性的——源码逐步迁移到各个工作空间 crate 中。

## 构建顺序（自底向上）

```
第 0 层（叶子层）： deepseek-protocol, deepseek-config, deepseek-state, deepseek-tui-core
第 1 层：          deepseek-tools, deepseek-mcp, deepseek-hooks, deepseek-execpolicy
第 2 层：          deepseek-agent
第 3 层：          deepseek-core
第 4 层：          deepseek-app-server, deepseek-tui
第 5 层：          deepseek-tui-cli
```
