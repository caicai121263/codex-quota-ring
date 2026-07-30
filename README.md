# Codex Quota Ring

> Windows 上的本地悬浮额度看板：把 Codex 的 5 小时额度、周额度和下一次重置时间放在视线里。

[下载最新 Windows 安装包](https://github.com/caicai121263/codex-quota-ring/releases/latest)

Codex Quota Ring 是一个为 Windows 10/11 设计的轻量桌面工具。它常驻在桌面边缘，以环形进度直观显示你的 Codex 使用额度；无需打开网页、无需配置 API Key，也不保存对话或用量历史。

## 适合谁

- 经常使用 Codex，希望在编程时随时掌握 5 小时和周额度的用户。
- 想在额度接近用尽或即将重置时快速判断下一步安排的用户。
- 在意账户与本地数据边界，希望工具仅进行本机只读查询的用户。

## 主要功能

- **一眼看清额度**：主环可切换 5 小时额度与周额度，并显示实时重置倒计时。
- **按需显示 Credits**：账户提供相关信息时，可在设置中显示只读的 Credits 余额、上限和重置券数量。
- **贴边但不碍事**：可停靠到左、右或上边缘，收起为迷你额度环；悬停即展开，移开自动收回。
- **为桌面常驻而设**：无边框、可拖动和缩放；支持锁定位置、始终置顶、系统托盘和多显示器位置恢复。
- **稳定刷新**：手动刷新、定时刷新、睡眠恢复刷新与实时倒计时；读取失败时保留最近一次有效数据。
- **按你的习惯启动**：支持开机启动、开机后隐藏，以及 Codex / ChatGPT 启停联动（均默认关闭）。
- **高 DPI 友好**：支持 80%、100%、125%、150% 界面缩放，并适配多显示器和不同缩放比例。

## 快速开始

1. 在 [Releases](https://github.com/caicai121263/codex-quota-ring/releases/latest) 下载并安装最新 Windows 安装包。
2. 确保已安装并登录 Codex 桌面版或 Codex CLI。
3. 启动 Codex Quota Ring；首次读取完成后即可查看额度。
4. 右键系统托盘图标可刷新、切换主环、设置贴边收起或调整常驻选项。

## 数据与隐私

这是一款**本地优先、只读**的工具。它仅通过本机的 `codex.exe app-server --stdio` 请求以下只读方法：

- `account/read`
- `account/rateLimits/read`

它不会：

- 读取浏览器 Cookie、OpenAI API Key、`auth.json`、对话内容、项目文件或额度历史；
- 发送遥测数据，调用远程额度接口，修改账户设置或自动续跑任务；
- 安装全局鼠标或键盘钩子。

Credits 是否显示取决于 Codex App Server 对当前账户返回的数据；没有可用数据时，应用会自动隐藏该区域。

## 使用说明与限制

- 仅支持 Windows 10/11 x64。
- 需要本机可用且已登录的 Codex Desktop 或 Codex CLI；找不到或未登录时，应用会提示原因。
- 额度数值以 Codex App Server 返回的数据为准。该工具用于展示，不会改变额度、订阅或账户状态。
- 这是独立的社区项目，与 OpenAI 或 Codex 官方产品没有隶属关系。

## 本地开发

开发环境需要 Node.js、Rust stable、Windows WebView2 Runtime 和 Microsoft C++ Build Tools。

```powershell
npm.cmd install
npm.cmd test
npm.cmd run tauri:dev
```

构建 Windows x64 NSIS 安装包：

```powershell
npm.cmd run tauri:build
```

## 反馈

欢迎通过 [Issues](https://github.com/caicai121263/codex-quota-ring/issues) 提交问题、功能建议或兼容性反馈。提交问题时，请优先附上应用中的脱敏诊断信息，便于定位。
