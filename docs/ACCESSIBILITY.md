# 无障碍

DeepSeek-TUI 运行在终端中，因此平台自身的无障碍栈（屏幕阅读器、放大镜、终端级主题）承担了大部分工作。TUI 提供了一小组开关，可减少视觉动效和密度，方便屏幕阅读器和低动态用户使用。

## 快速参考

| 开关 | 默认值 | 效果 |
| --- | --- | --- |
| `NO_ANIMATIONS=1` 环境变量 | 未设置 | 启动时强制设置 `low_motion = true` 和 `fancy_animations = false`。覆盖 `settings.toml` 中保存的任何设置。 |
| `low_motion` 设置 | `false` | 抑制加载动画的运动、对话淡入效果、底部栏漂移以及活动单元格脉冲。帧率限制器还会降低空闲重绘速度，使光标不会过于频繁闪烁。 |
| `fancy_animations` 设置 | `false` | 底部栏的水柱条和脉冲式子代理计数器。默认关闭。 |
| `calm_mode` 设置 | `false` | 默认折叠工具输出详情并精简状态消息。对于每次重绘都会播报的屏幕阅读器很有用。 |
| `show_thinking` 设置 | `true` | 设为 `false` 可完全隐藏模型的 `reasoning_content` 思维链区块。 |
| `show_tool_details` 设置 | `true` | 设为 `false` 可将工具调用显示为单行形式，不展开负载。 |

## 标准环境变量

在 shell 配置文件中设置这些变量，以便它们适用于每个会话：

```bash
# 强制低动态 + 无花哨动画
export NO_ANIMATIONS=1

# 可选：遵循终端颜色约定
export NO_COLOR=1            # 被底层 ratatui 后端所遵循
```

`NO_ANIMATIONS` 接受以下值：`1`、`true`、`yes` 或 `on`（不区分大小写）。任何其他值（包括 `0`、`false`、空值或未设置）都会保留你保存的设置。

此覆盖在启动时应用一次。在会话中更改环境变量不会产生效果——设置只会在下次启动时重新读取。

## 通过 `/settings` 配置

相同的开关也可以通过命令面板访问：

* `/settings set low_motion on`
* `/settings set fancy_animations off`
* `/settings set calm_mode on`

通过这种方式写入的设置会持久保存到 `~/.config/deepseek/settings.toml`。如果设置了 `NO_ANIMATIONS` 环境变量，它在启动时仍然优先，因此取消设置该环境变量是让你保存的选择生效的方式。

## 屏幕阅读器用户注意事项

* `low_motion` 将空闲重绘循环减慢至约 120ms 每帧，这样光标就不会被频繁重定位。结合 `calm_mode`，重绘率保持足够低，使得 VoiceOver / Orca 的播报能线性跟随模型输出，而不是在每个时钟周期重新读取整个屏幕。
* 对话是纯文本——没有图片或画布渲染——因此任何与平台无障碍服务集成的终端（例如 macOS Terminal.app、iTerm2、Ghostty、Windows Terminal）都会将渲染内容直接传递过去。
* 如果你发现某个 UI 界面在 `low_motion = true` 时仍有运动效果，请通过截图或终端录制向 [`PRIOR: Screen-reader / accessibility flag`](https://github.com/Hmbown/DeepSeek-TUI/issues/450) 提交 issue。

## 相关 Issue / 历史

* [#450](https://github.com/Hmbown/DeepSeek-TUI/issues/450) —— 记录现有标志，添加 `NO_ANIMATIONS` 启动覆盖，并编写本页面。
* [#449](https://github.com/Hmbown/DeepSeek-TUI/issues/449) —— 底部栏状态行现在使用活动主题的对比色对，而非自定义调色板。
