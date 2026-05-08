# deepseek-tui-web

[deepseek-tui](https://github.com/Hmbown/deepseek-tui) 的社区网站——位于 **deepseek-tui.com**。

Next.js 15（App Router）+ Tailwind，通过 [`@opennextjs/cloudflare`](https://opennext.js.org/cloudflare) 部署到 Cloudflare Workers。精心策划的「今日速报」内容每 6 小时由 Cloudflare Cron 触发器重新生成，该触发器调用 `deepseek-v4-flash` 总结最近的仓库活动，并存储在 Workers KV 中。

## 本地开发

```bash
cd web
npm install
cp .env.example .env.local   # 填入您拥有的密钥
npm run dev                  # http://localhost:3000
```

所需环境变量（仅用于策展人 + 私有仓库速率限制）：

| 变量               | 说明                                          | 是否必需？            |
| ------------------ | --------------------------------------------- | --------------------- |
| `DEEPSEEK_API_KEY` | DeepSeek 平台密钥（`sk-...`）                  | 仅用于 `/api/cron?task=curate` |
| `GITHUB_TOKEN`     | 细粒度个人访问令牌，公开仓库读取权限            | 可选（提高速率限制）  |
| `GITHUB_REPO`      | 默认为 `Hmbown/deepseek-tui`                   | 可选                  |
| `CRON_SECRET`      | 用于手动调用 cron 的共享密钥                    | 可选                  |

即使没有任何这些变量，网站也能正常渲染——「今日速报」会回退到静态编辑内容；GitHub 动态显示「动态尚未加载」。

## 部署到 Cloudflare

您已经在 Cloudflare 上拥有 `deepseek-tui.com` 域名并拥有 Workers Paid 套餐。部署分为两步：

1. **一次性配置 KV 命名空间：**

   ```bash
   npx wrangler kv namespace create CURATED_KV
   npx wrangler kv namespace create NEXT_INC_CACHE_KV
   ```

   将打印的 `id` 值复制到对应的 `wrangler.jsonc` 绑定中（将每个 `REPLACE_WITH_KV_ID` 替换掉）。

2. **设置密钥并部署：**

   ```bash
   npx wrangler secret put DEEPSEEK_API_KEY
   npx wrangler secret put GITHUB_TOKEN     # 可选
   npx wrangler secret put CRON_SECRET      # 可选，用于手动调用 /api/cron?task=curate

   npm run deploy                           # 使用 OpenNext 构建并上传
   ```

3. **绑定域名：** 在 Cloudflare 控制面板中，为 `deepseek-tui.com/*` 添加 Worker 路由 → `deepseek-tui-web`（如果区域已在您的账户中，deploy 命令会提供此选项）。

首次 cron 运行将在 6 小时内执行；您也可以手动触发：

```bash
curl -H "x-cron-secret: $CRON_SECRET" "https://deepseek-tui.com/api/cron?task=curate"
```

## 目录结构

```
web/
├── app/
│   ├── layout.tsx              根布局，字体加载
│   ├── page.tsx                首页 — 英雄区、速报、统计、工作原理、加入我们
│   ├── globals.css             设计系统：纸张纹理、细线、排版、印章
│   ├── install/page.tsx        按操作系统安装，带自动检测
│   ├── docs/page.tsx           模式 / 工具 / 审批 / 配置 / MCP / 提供商
│   ├── feed/page.tsx           Issue 和 PR 的实时镜像
│   ├── roadmap/page.tsx        已发布 / 进行中 / 考虑中 / 已排除
│   ├── contribute/page.tsx     如何提交 PR + 内部规则 + 开发流程
│   └── api/
│       ├── cron/route.ts          手动 cron 触发器：GitHub → DeepSeek → KV
│       └── github/feed/route.ts   缓存的 JSON 端点
├── components/
│   ├── nav.tsx                 粘性页眉，带日期条 + 中文字形点缀
│   ├── footer.tsx              密集的 5 列页脚
│   ├── seal.tsx                用作章节锚点的红色中文印章标记
│   ├── ticker.tsx              动画实时活动条
│   ├── stat-grid.tsx           表格式仓库统计数据行
│   ├── feed-card.tsx           单个 Issue/PR 卡片
│   └── install-tabs.tsx        客户端组件，操作系统自动检测 + 复制
├── lib/
│   ├── types.ts                共享类型
│   ├── github.ts               REST 客户端 + 相对时间格式化器
│   ├── deepseek.ts             v4-flash 聊天客户端 + curate() 提示词
│   └── kv.ts                   通过 OpenNext 绑定的 Cloudflare KV 访问
├── wrangler.jsonc              CF Worker 配置 + cron + KV 绑定
├── open-next.config.ts         OpenNext 适配器配置
└── tailwind.config.ts          设计令牌
```

## 美学风格

「衙门科技」：清代奏折 × 微信新闻流 × 彭博终端。

- **调色板**：宣纸色 `#FAF6EE`、墨色 `#0A2540`、朱砂红 `#C8102E`、古金色、翡翠绿、钴蓝色。
- **字体**：Fraunces（展示）、IBM Plex Sans（正文）、JetBrains Mono（UI/代码）、Noto Serif SC（装饰性中文锚点）。
- **结构**：1px 细线分隔线、多列网格、大号表格数字、红色用于「热门」标记的精准使用、装饰性中文印章方块作为章节锚点。

如果您想调整调色板，请编辑 `app/globals.css` 中的 `:root` 和 `tailwind.config.ts` 中的 `colors` 块。
