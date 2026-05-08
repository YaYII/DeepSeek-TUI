# 社区助理代理

社区助理是一组 Cloudflare Cron Triggers，调用 `deepseek-v4-flash` 来起草分类评论、PR 审查、过期 issue 提醒、重复建议和每周摘要。**它从不直接发布到 GitHub。** 每个输出都是在 Workers KV 中暂存的草稿，供维护者审查。

## 架构

```
Cloudflare Cron Triggers
  └─ worker.ts scheduled() 处理程序
       ├─ 每 30 分钟 → 分类（新 issue）+ PR 审查（新 PR）
       ├─ 每天      → 过期（30 天无活动）+ 重复（嵌入相似度扫描）
       ├─ 每周      → 摘要（周一 09:00 UTC）
       └─ 每 6 小时 → 整理（今日快讯——预先存在的）

存储在 Workers KV 中的草稿：
  draft:triage:<issue-number>
  draft:pr-review:<pr-number>
  draft:stale:<issue-number>
  draft:dupes:<issue-number>
  draft:digest:<year>-W<week>

使用情况记录到：
  usage:<YYYY-MM-DD>
```

## Cron 计划

| 表达式 | 频率 | 任务 |
|---|---|---|
| `0 */6 * * *` | 每 6 小时 | 今日快讯（整理） |
| `*/30 * * * *` | 每 30 分钟 | Issue 分类 + PR 审查 |
| `0 0 * * *` | 每天 00:00 UTC | 过期 issue 提醒 + 重复检测 |
| `0 9 * * 1` | 周一 09:00 UTC | 每周摘要 |

## 语气约束

所有草稿遵循以下规则：

- 冷静、实事求是，从不急促。
- 从不使用第一人称复数（"我们"）——维护者是一个人。
- 从不承诺时间安排、优先级或合并意图。
- 从不代表维护者道歉。
- 在讨论代码时引用具体文件/行号/相关 issue。
- 以"——由社区助理草拟，待维护者审阅"结尾
- 中文草稿以"—— 由社区助理草拟，待维护者审阅"结尾
- 中文输出用简体中文重写，而非机器翻译。

## 成本护栏

- 每次 cron 调用上限约为 30k 输入 token 和 2k 输出 token。
- Issue/PR 正文在发送给模型前截断为 1000-4000 字符。
- 去重：`hasFreshDraft` 检查是否已存在比项目 `updated_at` 更新的草稿。如果是则跳过。
- Token 使用情况记录到 `usage:<YYYY-MM-DD>` KV 键（保留 90 天）。
- 如果 `DEEPSEEK_API_KEY` 缺失或 API 出错，cron 返回 200 并包含 `{ skipped: true, reason }`——从不崩溃，从不重试循环。

## 维护者审查界面

通过 `/admin?token=<MAINTAINER_TOKEN>` 访问。

- 列出所有待处理的草稿，包含来源链接、草稿正文和三个操作：
  - **以评论形式发布** — 使用 `MAINTAINER_GITHUB_PAT` 调用 GitHub REST API
  - **编辑并发布** — 在发布前打开文本区域进行编辑
  - **丢弃** — 从 KV 中移除草稿
- 认证令牌通过 `MAINTAINER_TOKEN` 环境变量设置。访问会为会话设置 `mt` cookie。
- **没有维护者的显式点击，不会有任何内容发布到 GitHub。**

## 环境变量

| 变量 | 必需 | 用途 |
|---|---|---|
| `DEEPSEEK_API_KEY` | 是 | 社区代理的 DeepSeek API 密钥 |
| `GITHUB_TOKEN` | 可选 | 用于 GitHub API 的细粒度 PAT（提高速率限制） |
| `CRON_SECRET` | 可选 | 用于手动 cron 调用的共享密钥 |
| `MAINTAINER_TOKEN` | 可选 | /admin 面板的认证令牌 |
| `MAINTAINER_GITHUB_PAT` | 可选 | 具有 `issues:write` 范围的 GitHub PAT，用于发布评论 |

## 初始部署

首次 `npm run deploy` 前的一次性设置：

1. **创建 KV 命名空间：**
   ```bash
   npx wrangler kv namespace create CURATED_KV
   npx wrangler kv namespace create NEXT_INC_CACHE_KV
   ```
   复制返回的 `id` 值，粘贴到匹配的 `wrangler.jsonc` 绑定中，替换每个 `"REPLACE_WITH_KV_ID"`。

2. **设置密钥：**
   ```bash
   npx wrangler secret put DEEPSEEK_API_KEY
   npx wrangler secret put MAINTAINER_TOKEN
   npx wrangler secret put MAINTAINER_GITHUB_PAT
   npx wrangler secret put CRON_SECRET
   ```

3. **（可选）提高 GitHub 速率限制：**
   ```bash
   npx wrangler secret put GITHUB_TOKEN
   ```

4. **验证：**
   ```bash
   npm run predeploy   # 检查 KV ID 是否已设置
   npm run deploy      # 构建 + 部署
   ```

## 终止开关

要完全禁用社区代理：

1. 从 `wrangler.jsonc` 中移除除原始 `0 */6 * * *`（整理）外的所有 cron 触发器。
2. 重新部署：`npm run deploy`。

整理 cron（今日快讯）继续独立工作。单个任务仍然可以通过 `/api/cron?task=triage`、`/api/cron?task=pr-review` 等手动调用进行测试。

要禁用特定的 cron 任务，从 `wrangler.jsonc` 中移除其 cron 表达式并重新部署。

## 双语输出

每个草稿包含 `bodyEn`（英文）和 `bodyZh`（简体中文）。管理员面板显示匹配当前语言环境的版本。中文版本由模型原生重写，而非从英文翻译而来。