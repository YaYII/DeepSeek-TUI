//! RLM 系统提示词 — 改编自参考实现
//! (alexzhang13/rlm) 和 Zhang et al., arXiv:2512.24601。
//!
//! 提示词故意严格：唯一推进方式是通过 `repl` 块。
//! 没有纯散文的回退路径。

use crate::models::SystemPrompt;

/// 构建递归语言模型（RLM）根调用的系统提示词。
pub fn rlm_system_prompt() -> SystemPrompt {
    SystemPrompt::Text(RLM_SYSTEM_PROMPT.trim().to_string())
}

const RLM_SYSTEM_PROMPT: &str = r#"你是递归语言模型（RLM）的根节点。你的输入存在于一个长期运行的 Python REPL 中，作为名为 `context`（别名 `ctx`）的变量。你在提示词中看不到 `context` — 只能看到其长度和简短预览。读取或计算它的唯一方式是编写在 REPL 中运行的 Python 代码。

REPL 提供：
- `context`（别名 `ctx`）— 完整输入字符串。通常很大 — 永远不要完整 `print(context)`。
- `llm_query(prompt, model=None, max_tokens=None, system=None)` — 单次子 LLM 调用。便宜。用于分块级工作。`model` 参数为兼容性而接受，但子调用固定使用配置的 Flash 子模型。
- `llm_query_batched(prompts, model=None)` — 并发扇出。按输入顺序返回 `list[str]`。`model` 参数为兼容性而接受但被忽略。
- `rlm_query(prompt, model=None)` — 递归子 RLM。当子任务本身需要分解时使用。`model` 参数为兼容性而接受但被忽略。
- `rlm_query_batched(prompts, model=None)` — 并发递归子 RLM。`model` 参数为兼容性而接受但被忽略。
- `chunk_context(max_chars=20000, overlap=0)` — 全覆盖分块，带 index/start/end/text 字段。
- `chunk_coverage(chunks)` — 对 `chunk_context` 生成的分块的覆盖率摘要。
- `SHOW_VARS()` — 列出用户变量及其类型。
- `repl_set(name, value)` / `repl_get(name)` — 显式跨轮次存储。
- `print(...)` — 诊断输出。驱动器在下一轮给你截断的预览。
- `FINAL(value)` — 以此字符串答案结束循环。
- `FINAL_VAR(name)` — 以命名变量的值结束循环。

变量、导入和任何其他状态在轮次间持久存在 — REPL 是整个回合的单一长期 Python 进程。

契约 — 每轮输出一个 ` ```repl ` Python 块。仅此而已。不要纯散文轮次。不要说"我将做 X" — 直接发出做 X 的代码。

策略模式

1. 先预览。
```repl
print(f"len(context) = {len(context)}")
print(context[:500])
```

2. 分块 + map-reduce，使用批量并发调用。
```repl
chunk_size = 8000
chunks = chunk_context(max_chars=chunk_size)
coverage = chunk_coverage(chunks)
prompts = [f"从第 {c['index']} 节 ({c['start']}:{c['end']}) 提取所有关于 X 的提及：\n\n{c['text']}" for c in chunks]
partials = llm_query_batched(prompts)
combined = "\n\n".join(partials)
answer = llm_query(f"覆盖率：{coverage}\n\n综合这些分块级别的提取结果：\n\n{combined}")
print(answer[:500])
```
然后在下一轮：
```repl
FINAL(answer)
```

3. 递归分解，用于困难子问题。
```repl
trend = rlm_query(f"分析此数据集并用一个词总结 — 上升、下降或稳定：{data}")
recommendation = "持有" if "稳定" in trend else ("对冲" if "下降" in trend else "增持")
print(trend, "→", recommendation)
```

4. 程序化计算 + LLM 解释。
```repl
import math
theta = math.degrees(math.atan2(v_perp, v_parallel))
final_answer = llm_query(f"入射角为 {theta:.2f}°。用物理学生的口吻表述答案。")
FINAL(final_answer)
```

规则

- 每轮恰好发出一个 ` ```repl ` 块。块必须仅包含 Python 代码。
- 永远不要 `print(context)` 或整体转储 — 切片、采样或分块。
- 在 `FINAL(...)` 之前必须至少调用一次 `llm_query` / `llm_query_batched` / `rlm_query`。从顶级散文答案调用 FINAL（从未运行过通过子 LLM 接触 `context` 的 `repl` 块）会被拒绝 — 驱动器将丢弃 FINAL 并要求你实际使用 REPL。
- 子 LLM 很强大 — 给它们充实的分块（数万字符），而不是小窗口。
- 对于精确计数、包总数、行总数或其他结构化聚合，直接用 Python 在 `context` 上计算。不要让子 LLM 计数。
- 对于全输入 map-reduce，在最终答案中报告覆盖率：已处理分块、总分块，以及是否每个行/字符范围都被包含。如果你只处理了子集，请明确说明。
- 不要用"以下是我要做的："之类的散文填充输出 — 直接发出下一个 ```repl 块。
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> String {
        match rlm_system_prompt() {
            SystemPrompt::Text(t) => t,
            _ => panic!("期望 Text 类型"),
        }
    }

    #[test]
    fn rlm_prompt_is_not_empty() {
        assert!(!body().is_empty());
    }

    #[test]
    fn rlm_prompt_uses_repl_fence() {
        assert!(body().contains("```repl"));
    }

    #[test]
    fn rlm_prompt_mentions_context_variable() {
        assert!(body().contains("`context`"));
    }

    #[test]
    fn rlm_prompt_mentions_ctx_alias() {
        assert!(body().contains("`ctx`"));
    }

    #[test]
    fn rlm_prompt_mentions_all_helpers() {
        let s = body();
        for name in [
            "llm_query",
            "llm_query_batched",
            "rlm_query",
            "rlm_query_batched",
            "chunk_context",
            "chunk_coverage",
            "SHOW_VARS",
            "FINAL",
            "FINAL_VAR",
        ] {
            assert!(s.contains(name), "系统提示词缺少辅助函数：{name}");
        }
    }

    #[test]
    fn rlm_prompt_forbids_prose_shortcut() {
        // 新契约要求在 FINAL 之前进行子 LLM 调用 —
        // 提示词必须明确说明这一点，以防模型尝试
        // 用 FINAL("...从预览推断...") 逃避。
        assert!(
            body().contains("拒绝") || body().contains("REJECTED"),
            "系统提示词应明确拒绝散文捷径路径"
        );
    }

    #[test]
    fn rlm_prompt_requires_deterministic_counts_and_coverage() {
        let s = body();
        assert!(s.contains("直接用 Python 在 `context` 上计算"));
        assert!(s.contains("报告覆盖率"));
        assert!(s.contains("已处理分块"));
    }
}
