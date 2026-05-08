# 容量控制器

`deepseek-tui` 包含一个可选的容量感知上下文控制器。在默认的 V4 路径中它被禁用，因为其主动干预可能会重写实时提示并破坏前缀缓存亲和性。除非显式设置了 `capacity.enabled = true`，否则将其视为遥测或实验性护栏。

## 策略概览

每个检查点计算：

- `H_hat`（运行时压力代理）
- `C_hat`（模型容量先验）
- `slack = C_hat - H_hat`
- 最近 `N=8` 次观测的动态松弛度曲线

### 运行时压力代理（`H_hat`）

- `action_complexity_bits = log2(1 + action_count_this_turn)`
- `tool_complexity_bits = log2(1 + tool_calls_recent_window)`
- `ref_complexity_bits = log2(1 + unique_reference_ids_recent_window)`
- `context_pressure_bits = 6.0 * context_used_ratio`

公式：

`H_hat = 0.35*action_complexity_bits + 0.30*tool_complexity_bits + 0.20*ref_complexity_bits + 0.15*context_pressure_bits`

### 容量先验（`C_hat`）

各模型先验值：

- `deepseek_v3_2_chat = 3.9`
- `deepseek_v3_2_reasoner = 4.1`
- `deepseek_v4_pro = 3.5`
- `deepseek_v4_flash = 4.2`
- 回退值 `3.8`（用于其他 DeepSeek ID，包括未来版本）

### 失败概率

使用滚动曲线字段：

- `final_slack`
- `min_slack`
- `violation_ratio`
- `slack_volatility`
- `slack_drop`

公式：

`z = -1.65*final_slack -0.85*min_slack +1.35*violation_ratio +0.70*slack_volatility +0.28*slack_drop -0.12`

`p_fail = sigmoid(z)` 限制在 `[0,1]` 范围内。

风险等级：

- 低：`p_fail <= low_risk_max`
- 中：`p_fail <= medium_risk_max`
- 高：其他

当控制器显式启用时的动作映射：

- 低 -> `NoIntervention`（无干预）
- 中 -> `TargetedContextRefresh`（定向上下文刷新）
- 高 + 严重动态（`min_slack <= severe_min_slack` 或 `violation_ratio >= severe_violation_ratio`）-> `VerifyAndReplan`（验证并重新规划）
- 其他高 -> `VerifyWithToolReplay`（工具重放验证）

## 检查点

启用时，引擎在以下位置评估控制器策略：

1. 预请求检查点（`MessageRequest` 组装之前）
2. 工具后检查点（工具结果追加之后）
3. 错误升级检查点（工具错误连续路径）

## 干预措施

干预措施不是默认 v0.7.5 V4 路径的一部分。默认路径是：追加消息、保留前缀缓存重用、在接近实际模型压力时建议手动 `/compact`，仅当请求会超过模型输入预算时使用溢出恢复。

### `TargetedContextRefresh`（定向上下文刷新）

- 在可能时运行压缩（`compact_messages_safe`）
- 压缩路径失败时回退到本地裁剪
- 持久化规范状态
- 用紧凑的规范提示 + 记忆指针替换长尾活动上下文

### `VerifyWithToolReplay`（工具重放验证）

- 从最近轮次上下文中重放一个只读关键工具调用
- 追加包含通过/失败 + diff 摘要的验证说明
- 在重放冲突/错误时，标记升级候选并禁用当前轮次的重放

### `VerifyAndReplan`（验证并重新规划）

- 持久化规范快照
- 清除易变提示尾部，同时保留最新的用户请求和最新的验证说明
- 将规范重新规划指令注入系统提示
- 从紧凑的规范状态继续轮次循环

## 安全控制

- 每轮最多一次干预
- 刷新和重新规划的冷却期
- 每轮重放预算（`max_replay_per_turn`）
- 控制器输入不可用时的故障开放行为
- 压缩/重放失败会记入日志；轮次继续

## 内存存储

路径：

- `DEEPSEEK_CAPACITY_MEMORY_DIR`（如果设置）
- 否则为 `~/.deepseek/memory/<session_id>.jsonl`
- 回退：当主目录不可用/不可写时，使用 `<workspace>/.deepseek/memory/<session_id>.jsonl`

记录字段：

- `id`、`ts`、`turn_index`、`action_trigger`
- `h_hat`、`c_hat`、`slack`、`risk_band`
- `canonical_state`
- `source_message_ids`
- 可选的 `replay_info`

加载工具支持获取最后 `K` 个快照用于重新水合。

## 配置

`[capacity]` 配置项：

- `enabled`（默认 `false`）
- `low_risk_max`（默认 `0.50`）
- `medium_risk_max`（默认 `0.62`）
- `severe_min_slack`（默认 `-0.25`）
- `severe_violation_ratio`（默认 `0.40`）
- `refresh_cooldown_turns`（默认 `6`）
- `replan_cooldown_turns`（默认 `5`）
- `max_replay_per_turn`（默认 `1`）
- `min_turns_before_guardrail`（默认 `4`）
- `profile_window`（默认 `8`）
- `deepseek_v3_2_chat_prior`（默认 `3.9`）
- `deepseek_v3_2_reasoner_prior`（默认 `4.1`）
- `deepseek_v4_pro_prior`（默认 `3.5`）
- `deepseek_v4_flash_prior`（默认 `4.2`）
- `fallback_default_prior`（默认 `3.8`）

可通过 `DEEPSEEK_CAPACITY_*` 形式的环境变量覆盖相应配置。