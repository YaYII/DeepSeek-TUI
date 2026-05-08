//! `deepseek metrics` — 读取审计日志和会话/任务存储，并输出
//! 可读的使用汇总。
//!
//! 数据来源：
//! - `~/.deepseek/audit.log`   — 每事件一行 JSON（审批、凭证）
//! - `~/.deepseek/sessions/`   — 已保存的会话 JSON 文件（工具调用历史）
//! - `~/.deepseek/tasks/runtime/events/` — 运行时线程 JSONL 事件流

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

// ──────────────────────────────────────────────────────────────────────────────
// 公共入口
// ──────────────────────────────────────────────────────────────────────────────

/// `deepseek metrics` 接受的参数。
#[derive(Debug, Default)]
pub struct MetricsArgs {
    /// 输出机器可读的 JSON 而非人类文本。
    pub json: bool,
    /// 仅包含在此截止时间之后的事件（含）。
    pub since: Option<DateTime<Utc>>,
}

pub fn run(args: MetricsArgs) -> Result<()> {
    let base = deepseek_home();

    // 从每个来源收集数据；将缺失文件视为空。
    let mut rollup = Rollup::default();
    read_audit_log(&base.join("audit.log"), args.since, &mut rollup);
    read_session_files(&base.join("sessions"), args.since, &mut rollup);
    read_runtime_events(
        &base.join("tasks").join("runtime").join("events"),
        args.since,
        &mut rollup,
    );

    if args.json {
        print_json(&rollup)?;
    } else {
        print_human(&rollup);
    }

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// 持续时间字符串解析器 ("7d", "24h", "30m", "2h", "now-2h", "2h30m")
// ──────────────────────────────────────────────────────────────────────────────

/// 将松散的人类可读持续时间字符串解析为绝对 `DateTime<Utc>`
/// 截止时间（即 `Utc::now() - duration`）。
///
/// 接受的格式：
/// - `7d` / `24h` / `30m` / `90s`
/// - `2h30m`, `1d12h`
/// - `now-2h`（解析前会去除前导 `now-`）
pub fn parse_since(s: &str) -> Result<DateTime<Utc>> {
    let s = s.trim().to_ascii_lowercase();
    let s = s.strip_prefix("now-").unwrap_or(&s);
    let secs = parse_duration_secs(s)?;
    Ok(Utc::now() - Duration::seconds(secs))
}

fn parse_duration_secs(s: &str) -> Result<i64> {
    // 遍历字符串，累加数字并消费单位后缀。
    let mut total: i64 = 0;
    let mut num_buf = String::new();

    for ch in s.chars() {
        match ch {
            '0'..='9' => num_buf.push(ch),
            'd' | 'h' | 'm' | 's' => {
                let n: i64 = num_buf
                    .parse()
                    .map_err(|_| anyhow::anyhow!("无效的持续时间组件: {:?}", num_buf))?;
                num_buf.clear();
                let factor = match ch {
                    'd' => 86_400,
                    'h' => 3_600,
                    'm' => 60,
                    's' => 1,
                    _ => unreachable!(),
                };
                total += n * factor;
            }
            _ => anyhow::bail!("持续时间 {:?} 中存在无法识别的字符 {:?}", s, ch),
        }
    }

    if !num_buf.is_empty() {
        // 尾部裸露数字 — 视为秒。
        let n: i64 = num_buf.parse()?;
        total += n;
    }

    if total == 0 {
        anyhow::bail!("持续时间 {:?} 解析结果为零秒", s);
    }

    Ok(total)
}

// ──────────────────────────────────────────────────────────────────────────────
// 汇总数据模型
// ──────────────────────────────────────────────────────────────────────────────

/// 按工具聚合的计数器。
#[derive(Debug, Default, serde::Serialize)]
pub struct ToolStats {
    pub calls: u64,
    /// 自动批准（无需提示）的调用次数。
    pub auto_approved: u64,
    /// 需要手动提示的调用次数。
    pub prompted: u64,
    /// 总耗时毫秒（来自携带此字段的事件）。
    pub total_elapsed_ms: u64,
    /// `total_elapsed_ms` 中包含的耗时样本数量。
    pub elapsed_samples: u64,
    /// 成功的调用次数（有结果数据的）。
    pub successes: u64,
    /// 失败的调用次数。
    pub failures: u64,
}

impl ToolStats {
    fn success_rate_pct(&self) -> Option<f64> {
        let judged = self.successes + self.failures;
        if judged == 0 {
            None
        } else {
            Some(self.successes as f64 / judged as f64 * 100.0)
        }
    }

    fn avg_elapsed_ms(&self) -> Option<u64> {
        self.total_elapsed_ms.checked_div(self.elapsed_samples)
    }
}

/// 压缩事件统计。
#[derive(Debug, Default, serde::Serialize)]
pub struct CompactionStats {
    pub events: u64,
    /// 携带此字段的事件的 `reduction_ratio` 之和（每个 0.0–1.0）。
    pub ratio_sum: f64,
    pub ratio_samples: u64,
}

impl CompactionStats {
    fn avg_reduction_pct(&self) -> Option<f64> {
        if self.ratio_samples == 0 {
            None
        } else {
            Some(self.ratio_sum / self.ratio_samples as f64 * 100.0)
        }
    }
}

/// 子代理生成统计。
#[derive(Debug, Default, serde::Serialize)]
pub struct AgentStats {
    pub spawns: u64,
    pub successes: u64,
    pub failures: u64,
}

impl AgentStats {
    fn success_rate_pct(&self) -> Option<f64> {
        let judged = self.successes + self.failures;
        if judged == 0 {
            None
        } else {
            Some(self.successes as f64 / judged as f64 * 100.0)
        }
    }
}

/// 容量控制器/速率限制干预统计。
#[derive(Debug, Default, serde::Serialize)]
pub struct CapacityStats {
    pub total: u64,
    pub by_category: HashMap<String, u64>,
}

/// 凭证/会话事件统计（来自审计日志）。
#[derive(Debug, Default, serde::Serialize)]
pub struct CredentialStats {
    pub saves: u64,
    pub clears: u64,
}

/// 顶层汇总。
#[derive(Debug, Default, serde::Serialize)]
pub struct Rollup {
    /// 所见最早事件的 UTC 时间戳。
    pub earliest_ts: Option<DateTime<Utc>>,
    /// 所见最新事件的 UTC 时间戳。
    pub latest_ts: Option<DateTime<Utc>>,
    /// 按工具名称分组的各工具统计。
    pub tools: HashMap<String, ToolStats>,
    pub compaction: CompactionStats,
    pub agents: AgentStats,
    pub capacity: CapacityStats,
    pub credentials: CredentialStats,
    /// 所有来源读取的总行数。
    pub total_lines: u64,
    /// 成功解析的行数。
    pub parsed_lines: u64,
}

impl Rollup {
    fn touch_ts(&mut self, ts: &DateTime<Utc>) {
        match self.earliest_ts {
            None => self.earliest_ts = Some(*ts),
            Some(ref cur) if ts < cur => self.earliest_ts = Some(*ts),
            _ => {}
        }
        match self.latest_ts {
            None => self.latest_ts = Some(*ts),
            Some(ref cur) if ts > cur => self.latest_ts = Some(*ts),
            _ => {}
        }
    }

    fn tool_mut(&mut self, name: &str) -> &mut ToolStats {
        self.tools.entry(name.to_string()).or_default()
    }

    fn total_tool_calls(&self) -> u64 {
        self.tools.values().map(|t| t.calls).sum()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 来源读取器
// ──────────────────────────────────────────────────────────────────────────────

/// 读取一行一 JSON 事件的审计日志。
fn read_audit_log(path: &Path, since: Option<DateTime<Utc>>, rollup: &mut Rollup) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            tracing::trace!(
                "metrics: 无法读取审计日志 {}: {}",
                path.display(),
                e
            );
            return;
        }
    };

    for raw_line in content.lines() {
        rollup.total_lines += 1;
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                tracing::trace!("metrics: 跳过格式错误的审计行: {e}");
                continue;
            }
        };

        // 解析时间戳 — 审计日志中的字段为 "ts"。
        let ts = parse_ts_field(&v, "ts");

        if let Some(cutoff) = since {
            match ts {
                Some(t) if t < cutoff => continue,
                _ => {}
            }
        }

        rollup.parsed_lines += 1;
        if let Some(t) = &ts {
            rollup.touch_ts(t);
        }

        let event = v.get("event").and_then(|e| e.as_str()).unwrap_or("");

        match event {
            "tool.approval.auto_approve" => {
                let tool_name = v
                    .pointer("/details/tool_name")
                    .and_then(|t| t.as_str())
                    .unwrap_or("unknown");
                let stats = rollup.tool_mut(tool_name);
                stats.calls += 1;
                stats.auto_approved += 1;
            }
            "tool.approval.prompted" => {
                let tool_name = v
                    .pointer("/details/tool_name")
                    .and_then(|t| t.as_str())
                    .unwrap_or("unknown");
                let stats = rollup.tool_mut(tool_name);
                stats.calls += 1;
                stats.prompted += 1;
            }
            "tool.completed" | "tool.result" => {
                let tool_name = v
                    .pointer("/details/tool_name")
                    .or_else(|| v.pointer("/payload/tool_name"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("unknown");
                let stats = rollup.tool_mut(tool_name);
                stats.calls += 1;

                // 可选的 elapsed_ms
                if let Some(ms) = v
                    .pointer("/details/elapsed_ms")
                    .or_else(|| v.pointer("/payload/elapsed_ms"))
                    .and_then(|v| v.as_u64())
                {
                    stats.total_elapsed_ms += ms;
                    stats.elapsed_samples += 1;
                }

                // 成功/失败
                let success = v
                    .pointer("/details/success")
                    .or_else(|| v.pointer("/payload/success"))
                    .and_then(|b| b.as_bool())
                    .unwrap_or(true);
                if success {
                    stats.successes += 1;
                } else {
                    stats.failures += 1;
                }
            }
            "compaction.completed" | "context.compaction" => {
                rollup.compaction.events += 1;
                if let Some(ratio) = v
                    .pointer("/details/reduction_ratio")
                    .or_else(|| v.pointer("/payload/reduction_ratio"))
                    .and_then(|r| r.as_f64())
                {
                    rollup.compaction.ratio_sum += ratio;
                    rollup.compaction.ratio_samples += 1;
                }
            }
            "agent.spawn" | "subagent.spawned" => {
                rollup.agents.spawns += 1;
            }
            "agent.completed" | "subagent.completed" => {
                let success = v
                    .pointer("/details/success")
                    .or_else(|| v.pointer("/payload/success"))
                    .and_then(|b| b.as_bool())
                    .unwrap_or(true);
                if success {
                    rollup.agents.successes += 1;
                } else {
                    rollup.agents.failures += 1;
                }
            }
            e if e.starts_with("capacity.") => {
                rollup.capacity.total += 1;
                let category = v
                    .pointer("/details/category")
                    .or_else(|| v.pointer("/payload/category"))
                    .and_then(|c| c.as_str())
                    .unwrap_or(e.trim_start_matches("capacity."));
                *rollup
                    .capacity
                    .by_category
                    .entry(category.to_string())
                    .or_insert(0) += 1;
            }
            "credential.save" => {
                rollup.credentials.saves += 1;
            }
            "credential.clear" => {
                rollup.credentials.clears += 1;
            }
            _ => {
                // 未知事件 — 计入 parsed_lines 但忽略。
            }
        }
    }
}

/// 读取 `sessions/` 下的会话 JSON 文件（每个会话一个文件）。
/// 这些文件携带工具调用历史，包含可选的 elapsed_ms 和结果数据。
fn read_session_files(sessions_dir: &Path, since: Option<DateTime<Utc>>, rollup: &mut Rollup) {
    let rd = match std::fs::read_dir(sessions_dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            tracing::trace!(
                "metrics: 无法列出会话目录 {}: {}",
                sessions_dir.display(),
                e
            );
            return;
        }
    };

    for entry in rd.flatten() {
        let path = entry.path();
        // 仅查看 sessions/ 下直接存在的 .json 文件；跳过子目录。
        if path.is_dir() || path.extension().map(|e| e != "json").unwrap_or(true) {
            continue;
        }
        read_session_file(&path, since, rollup);
    }
}

fn read_session_file(path: &Path, since: Option<DateTime<Utc>>, rollup: &mut Rollup) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::trace!(
                "metrics: 无法读取会话文件 {}: {}",
                path.display(),
                e
            );
            return;
        }
    };

    rollup.total_lines += 1;

    let v: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::trace!(
                "metrics: 跳过格式错误的会话文件 {}: {}",
                path.display(),
                e
            );
            return;
        }
    };

    rollup.parsed_lines += 1;

    // 会话级时间戳过滤（检查 metadata.created_at 或 updated_at）。
    let session_ts = v
        .pointer("/metadata/updated_at")
        .or_else(|| v.pointer("/metadata/created_at"))
        .and_then(|t| t.as_str())
        .and_then(|s| s.parse::<DateTime<Utc>>().ok());

    if let Some(cutoff) = since
        && let Some(ts) = &session_ts
        && *ts < cutoff
    {
        return;
    }

    if let Some(ts) = session_ts {
        rollup.touch_ts(&ts);
    }

    // 遍历消息，查找带有关联结果的 tool_use 调用。
    let messages = match v.get("messages").and_then(|m| m.as_array()) {
        Some(m) => m,
        None => return,
    };

    // 构建 tool_use_id → (tool_name, elapsed_ms_option, started_at_option) 映射。
    let mut pending: HashMap<String, (String, Option<u64>)> = HashMap::new();

    for msg in messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        let content_arr = match msg.get("content").and_then(|c| c.as_array()) {
            Some(c) => c,
            None => continue,
        };

        for block in content_arr {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match (role, block_type) {
                ("assistant", "tool_use") => {
                    let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    let name = block
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");
                    let elapsed_ms = block.get("elapsed_ms").and_then(|e| e.as_u64());
                    if !id.is_empty() {
                        pending.insert(id.to_string(), (name.to_string(), elapsed_ms));
                    }
                }
                ("user", "tool_result") => {
                    let id = block
                        .get("tool_use_id")
                        .and_then(|i| i.as_str())
                        .unwrap_or("");
                    if let Some((name, elapsed_ms)) = pending.remove(id) {
                        let stats = rollup.tool_mut(&name);
                        // 仅在尚未通过审计日志计数时计数（我们不进行去重，所以
                        // 会话文件可能会重复计算审批；这是可接受的 — 需要精确计数的用户
                        // 应使用 --json 并交叉引用）。
                        stats.calls += 1;
                        if let Some(ms) = elapsed_ms {
                            stats.total_elapsed_ms += ms;
                            stats.elapsed_samples += 1;
                        }
                        // 工具结果成功：没有 "is_error": true 即为成功
                        let is_error = block
                            .get("is_error")
                            .and_then(|e| e.as_bool())
                            .unwrap_or(false);
                        if is_error {
                            stats.failures += 1;
                        } else {
                            stats.successes += 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // 遍历消息，查找嵌入为特殊用户消息的压缩事件。
    for msg in messages {
        if let Some(compaction) = msg
            .get("compaction")
            .or_else(|| msg.pointer("/metadata/compaction"))
        {
            rollup.compaction.events += 1;
            if let Some(ratio) = compaction.get("reduction_ratio").and_then(|r| r.as_f64()) {
                rollup.compaction.ratio_sum += ratio;
                rollup.compaction.ratio_samples += 1;
            }
        }
    }
}

/// 从任务运行时事件目录读取 JSONL 事件流。
fn read_runtime_events(events_dir: &Path, since: Option<DateTime<Utc>>, rollup: &mut Rollup) {
    let rd = match std::fs::read_dir(events_dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            tracing::trace!(
                "metrics: 无法列出事件目录 {}: {}",
                events_dir.display(),
                e
            );
            return;
        }
    };

    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e != "jsonl").unwrap_or(true) {
            continue;
        }
        read_events_jsonl(&path, since, rollup);
    }
}

fn read_events_jsonl(path: &Path, since: Option<DateTime<Utc>>, rollup: &mut Rollup) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::trace!(
                "metrics: 无法读取事件文件 {}: {}",
                path.display(),
                e
            );
            return;
        }
    };

    for raw_line in content.lines() {
        rollup.total_lines += 1;
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                tracing::trace!("metrics: 跳过格式错误的事件行: {e}");
                continue;
            }
        };

        let ts = parse_ts_field(&v, "timestamp");

        if let Some(cutoff) = since {
            match ts {
                Some(t) if t < cutoff => continue,
                _ => {}
            }
        }

        rollup.parsed_lines += 1;
        if let Some(t) = &ts {
            rollup.touch_ts(t);
        }

        let event = v.get("event").and_then(|e| e.as_str()).unwrap_or("");

        match event {
            "tool.started" | "tool.completed" | "tool.failed" => {
                let tool_name = v
                    .pointer("/payload/tool_name")
                    .or_else(|| v.pointer("/payload/name"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("unknown");
                let stats = rollup.tool_mut(tool_name);

                if event == "tool.started" {
                    stats.calls += 1;
                } else if event == "tool.completed" {
                    stats.successes += 1;
                    if let Some(ms) = v.pointer("/payload/elapsed_ms").and_then(|v| v.as_u64()) {
                        stats.total_elapsed_ms += ms;
                        stats.elapsed_samples += 1;
                    }
                } else {
                    // tool.failed
                    stats.failures += 1;
                }
            }
            "compaction.completed" => {
                rollup.compaction.events += 1;
                if let Some(ratio) = v
                    .pointer("/payload/reduction_ratio")
                    .and_then(|r| r.as_f64())
                {
                    rollup.compaction.ratio_sum += ratio;
                    rollup.compaction.ratio_samples += 1;
                }
            }
            "agent.spawned" | "subagent.spawned" => {
                rollup.agents.spawns += 1;
            }
            "agent.completed" | "subagent.completed" => {
                let success = v
                    .pointer("/payload/success")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(true);
                if success {
                    rollup.agents.successes += 1;
                } else {
                    rollup.agents.failures += 1;
                }
            }
            e if e.starts_with("capacity.") => {
                rollup.capacity.total += 1;
                let category = v
                    .pointer("/payload/category")
                    .and_then(|c| c.as_str())
                    .unwrap_or(e.trim_start_matches("capacity."));
                *rollup
                    .capacity
                    .by_category
                    .entry(category.to_string())
                    .or_insert(0) += 1;
            }
            _ => {}
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 输出格式化器
// ──────────────────────────────────────────────────────────────────────────────

fn print_json(rollup: &Rollup) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(rollup)?);
    Ok(())
}

fn print_human(rollup: &Rollup) {
    // 时段标题
    match (rollup.earliest_ts, rollup.latest_ts) {
        (Some(start), Some(end)) => {
            let days = (end - start).num_days();
            println!(
                "时段: {} → {} ({} 天)",
                start.format("%Y-%m-%d"),
                end.format("%Y-%m-%d"),
                days
            );
        }
        (Some(start), None) | (None, Some(start)) => {
            println!("时段: {} → (未知)", start.format("%Y-%m-%d"));
        }
        (None, None) => {
            println!("时段: (无数据)");
        }
    }

    // ── 工具 ──────────────────────────────────────────────────────────────
    let total_calls = rollup.total_tool_calls();
    if total_calls > 0 {
        // 来自会话文件数据的总体成功率（有结果信息）。
        let total_ok: u64 = rollup.tools.values().map(|t| t.successes).sum();
        let total_judged: u64 = rollup
            .tools
            .values()
            .map(|t| t.successes + t.failures)
            .sum();
        let overall_rate = if total_judged > 0 {
            format!(
                "{:.1}% 成功",
                total_ok as f64 / total_judged as f64 * 100.0
            )
        } else {
            // 仅有审批事件 — 显示自动/提示的细分。
            let auto: u64 = rollup.tools.values().map(|t| t.auto_approved).sum();
            let prompted: u64 = rollup.tools.values().map(|t| t.prompted).sum();
            format!("{auto} 自动批准, {prompted} 提示")
        };

        println!(
            "工具: {:>6} 次调用 ({})",
            fmt_num(total_calls),
            overall_rate
        );

        // 按调用次数降序排列工具，取前 15 个。
        let mut tools: Vec<(&String, &ToolStats)> = rollup.tools.iter().collect();
        tools.sort_by_key(|b| std::cmp::Reverse(b.1.calls));
        for (name, stats) in tools.iter().take(15) {
            let rate_str = match stats.success_rate_pct() {
                Some(pct) => format!("{pct:5.1}%"),
                None => {
                    // 仅有审批数据可用 — 显示自动/提示的细分。
                    let a = stats.auto_approved;
                    let p = stats.prompted;
                    if p == 0 {
                        format!("自动×{a}  ")
                    } else {
                        format!("自动×{a}/提示×{p}")
                    }
                }
            };
            let avg_str = match stats.avg_elapsed_ms() {
                Some(ms) => format!("  平均 {ms}ms"),
                None => String::new(),
            };
            println!(
                "  {name:<22} {:>6}  {rate_str}{avg_str}",
                fmt_num(stats.calls)
            );
        }
        if tools.len() > 15 {
            println!("  ……以及 {} 个其他工具", tools.len() - 15);
        }
    } else {
        println!("工具: (无数据)");
    }

    // ── 压缩 ─────────────────────────────────────────────────────────
    if rollup.compaction.events > 0 {
        let avg_str = match rollup.compaction.avg_reduction_pct() {
            Some(pct) => format!("，平均缩减 {pct:.0}%"),
            None => String::new(),
        };
        println!(
            "压缩: {} 次事件{}",
            fmt_num(rollup.compaction.events),
            avg_str
        );
    } else {
        println!("压缩: (无数据)");
    }

    // ── 子代理 ─────────────────────────────────────────────────────────
    if rollup.agents.spawns > 0 {
        let rate_str = match rollup.agents.success_rate_pct() {
            Some(pct) => format!("，{pct:.1}% 成功"),
            None => String::new(),
        };
        println!(
            "子代理: {} 次生成{}",
            fmt_num(rollup.agents.spawns),
            rate_str
        );
    } else {
        println!("子代理: (无数据)");
    }

    // ── 容量干预 ─────────────────────────────────────────────────────────
    if rollup.capacity.total > 0 {
        let cat_str: String = {
            let mut cats: Vec<(&String, &u64)> = rollup.capacity.by_category.iter().collect();
            cats.sort_by(|a, b| b.1.cmp(a.1));
            cats.iter()
                .map(|(k, v)| format!("{} {}", v, k))
                .collect::<Vec<_>>()
                .join(", ")
        };
        println!(
            "容量干预: {} ({})",
            fmt_num(rollup.capacity.total),
            cat_str
        );
    } else {
        println!("容量干预: (无数据)");
    }

    // ── 凭证 ────────────────────────────────────────────────────────
    if rollup.credentials.saves > 0 || rollup.credentials.clears > 0 {
        println!(
            "凭证: {} 次保存, {} 次清除",
            rollup.credentials.saves, rollup.credentials.clears
        );
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 辅助函数
// ──────────────────────────────────────────────────────────────────────────────

fn deepseek_home() -> PathBuf {
    // 优先使用 DEEPSEEK_HOME 环境变量覆盖；否则使用 ~/.deepseek。
    if let Ok(v) = std::env::var("DEEPSEEK_HOME")
        && !v.is_empty()
    {
        return PathBuf::from(v);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".deepseek")
}

/// 从 JSON 值字段解析时间戳（尝试 RFC3339）。
fn parse_ts_field(v: &Value, field: &str) -> Option<DateTime<Utc>> {
    v.get(field)?.as_str()?.parse::<DateTime<Utc>>().ok()
}

/// 使用千位分隔符格式化数字。
fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

// ──────────────────────────────────────────────────────────────────────────────
// 测试
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── 持续时间解析器 ──

    #[test]
    fn parse_since_7d() {
        let cutoff = parse_since("7d").unwrap();
        let expected = Utc::now() - Duration::days(7);
        // 允许 ±2s 的测试执行时间。
        assert!((cutoff - expected).num_seconds().abs() < 2);
    }

    #[test]
    fn parse_since_24h() {
        let cutoff = parse_since("24h").unwrap();
        let expected = Utc::now() - Duration::hours(24);
        assert!((cutoff - expected).num_seconds().abs() < 2);
    }

    #[test]
    fn parse_since_30m() {
        let cutoff = parse_since("30m").unwrap();
        let expected = Utc::now() - Duration::minutes(30);
        assert!((cutoff - expected).num_seconds().abs() < 2);
    }

    #[test]
    fn parse_since_now_prefix() {
        // "now-2h" 应去除 "now-" 并解析 "2h"。
        let cutoff = parse_since("now-2h").unwrap();
        let expected = Utc::now() - Duration::hours(2);
        assert!((cutoff - expected).num_seconds().abs() < 2);
    }

    #[test]
    fn parse_since_compound() {
        let cutoff = parse_since("2h30m").unwrap();
        let expected = Utc::now() - Duration::seconds(2 * 3600 + 30 * 60);
        assert!((cutoff - expected).num_seconds().abs() < 2);
    }

    #[test]
    fn parse_since_compound_days_hours() {
        let cutoff = parse_since("1d12h").unwrap();
        let expected = Utc::now() - Duration::seconds(36 * 3600);
        assert!((cutoff - expected).num_seconds().abs() < 2);
    }

    #[test]
    fn parse_since_error_on_invalid() {
        assert!(parse_since("xyz").is_err());
        assert!(parse_since("").is_err());
    }

    // ── fmt_num ──

    #[test]
    fn fmt_num_zero() {
        assert_eq!(fmt_num(0), "0");
    }

    #[test]
    fn fmt_num_thousands() {
        assert_eq!(fmt_num(1_000), "1,000");
        assert_eq!(fmt_num(12_453), "12,453");
        assert_eq!(fmt_num(1_000_000), "1,000,000");
    }

    // ── 从审计日志汇总 ──

    fn make_audit_line(event: &str, tool: &str, ts: &str) -> String {
        format!(
            r#"{{"details":{{"mode":"YOLO","session_id":null,"tool_name":"{tool}"}},"event":"{event}","ts":"{ts}"}}"#
        )
    }

    #[test]
    fn audit_log_empty_file() {
        let mut rollup = Rollup::default();
        // 不存在的路径 — 不应 panic，rollup 保持为空。
        read_audit_log(Path::new("/nonexistent/audit.log"), None, &mut rollup);
        assert_eq!(rollup.total_lines, 0);
    }

    #[test]
    fn audit_log_parses_auto_approve() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        let line1 = make_audit_line(
            "tool.approval.auto_approve",
            "exec_shell",
            "2026-04-01T10:00:00+00:00",
        );
        let line2 = make_audit_line(
            "tool.approval.auto_approve",
            "read_file",
            "2026-04-02T10:00:00+00:00",
        );
        writeln!(tmp, "{line1}").unwrap();
        writeln!(tmp, "{line2}").unwrap();

        let mut rollup = Rollup::default();
        read_audit_log(tmp.path(), None, &mut rollup);

        assert_eq!(rollup.parsed_lines, 2);
        assert_eq!(rollup.tools["exec_shell"].calls, 1);
        assert_eq!(rollup.tools["exec_shell"].auto_approved, 1);
        assert_eq!(rollup.tools["read_file"].calls, 1);
    }

    #[test]
    fn audit_log_skips_malformed_lines() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "not json at all").unwrap();
        writeln!(
            tmp,
            r#"{{"event":"credential.save","ts":"2026-04-01T10:00:00+00:00"}}"#
        )
        .unwrap();

        let mut rollup = Rollup::default();
        read_audit_log(tmp.path(), None, &mut rollup);

        // 共 2 行，1 行格式错误被跳过，1 行被解析。
        assert_eq!(rollup.total_lines, 2);
        assert_eq!(rollup.parsed_lines, 1);
        assert_eq!(rollup.credentials.saves, 1);
    }

    #[test]
    fn audit_log_since_filter() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        let line_old = make_audit_line(
            "tool.approval.auto_approve",
            "exec_shell",
            "2025-01-01T00:00:00+00:00",
        );
        let line_new = make_audit_line(
            "tool.approval.auto_approve",
            "read_file",
            "2026-04-01T00:00:00+00:00",
        );
        writeln!(tmp, "{line_old}").unwrap();
        writeln!(tmp, "{line_new}").unwrap();

        let cutoff: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let mut rollup = Rollup::default();
        read_audit_log(tmp.path(), Some(cutoff), &mut rollup);

        // 只有较新的行应被计数。
        assert_eq!(rollup.parsed_lines, 1);
        assert!(!rollup.tools.contains_key("exec_shell"));
        assert_eq!(rollup.tools["read_file"].calls, 1);
    }

    #[test]
    fn total_tool_calls_sums_across_tools() {
        let mut rollup = Rollup::default();
        rollup.tool_mut("read_file").calls = 4_012;
        rollup.tool_mut("exec_shell").calls = 1_118;
        assert_eq!(rollup.total_tool_calls(), 5_130);
    }
}
