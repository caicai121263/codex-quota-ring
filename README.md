# Codex Quota Ring

[English](#english) | [中文](#中文)

> A local Windows desktop quota meter for Codex — keep your 5-hour limit, weekly limit, and next reset time in view.
>
> Windows 本地 Codex 额度悬浮看板：把 5 小时额度、周额度和下一次重置时间放在视线里。

[Download the latest Windows installer / 下载最新 Windows 安装包](https://github.com/caicai121263/codex-quota-ring/releases/latest)

---

<a id="中文"></a>

## 中文

[English](#english)

Codex Quota Ring 是一个为 Windows 10/11 设计的轻量桌面工具。它常驻在桌面边缘，以环形进度直观显示你的 Codex 使用额度；无需打开网页、无需配置 API Key，也不保存对话或用量历史。

### 适合谁

- 经常使用 Codex，希望在编程时随时掌握 5 小时和周额度的用户。
- 想在额度接近用尽或即将重置时快速判断下一步安排的用户。
- 在意账户与本地数据边界，希望工具仅进行本机只读查询的用户。

### 主要功能

- **一眼看清额度**：主环可切换 5 小时额度与周额度，并显示实时重置倒计时。
- **按需显示 Credits**：账户提供相关信息时，可在设置中显示只读的 Credits 余额、上限和重置券数量。
- **贴边但不碍事**：可停靠到左、右或上边缘，收起为迷你额度环；悬停即展开，移开自动收回。
- **为桌面常驻而设**：无边框、可拖动和缩放；支持锁定位置、始终置顶、系统托盘和多显示器位置恢复。
- **稳定刷新**：手动刷新、定时刷新、睡眠恢复刷新与实时倒计时；读取失败时保留最近一次有效数据。
- **按你的习惯启动**：支持开机启动、开机后隐藏，以及 Codex / ChatGPT 启停联动（均默认关闭）。
- **高 DPI 友好**：支持 80%、100%、125%、150% 界面缩放，并适配多显示器和不同缩放比例。

### 快速开始

1. 在 [Releases](https://github.com/caicai121263/codex-quota-ring/releases/latest) 下载并安装最新 Windows 安装包。
2. 确保已安装并登录 Codex 桌面版或 Codex CLI。
3. 启动 Codex Quota Ring；首次读取完成后即可查看额度。
4. 右键系统托盘图标可刷新、切换主环、设置贴边收起或调整常驻选项。

### 数据与隐私

这是一款**本地优先、只读**的工具。它仅通过本机的 `codex.exe app-server --stdio` 请求 `account/read` 与 `account/rateLimits/read`。

它不会读取浏览器 Cookie、OpenAI API Key、`auth.json`、对话内容、项目文件或额度历史；不会发送遥测数据、调用远程额度接口、修改账户设置、自动续跑任务，或安装全局鼠标和键盘钩子。

Credits 是否显示取决于 Codex App Server 对当前账户返回的数据；没有可用数据时，应用会自动隐藏该区域。

### 使用说明与限制

- 仅支持 Windows 10/11 x64。
- 需要本机可用且已登录的 Codex Desktop 或 Codex CLI；找不到或未登录时，应用会提示原因。
- 额度数值以 Codex App Server 返回的数据为准。该工具用于展示，不会改变额度、订阅或账户状态。
- 这是独立的社区项目，与 OpenAI 或 Codex 官方产品没有隶属关系。

### 本地开发

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

### 反馈

欢迎通过 [Issues](https://github.com/caicai121263/codex-quota-ring/issues) 提交问题、功能建议或兼容性反馈。提交问题时，请优先附上应用中的脱敏诊断信息，便于定位。

---

<a id="english"></a>

## English

[中文](#中文)

Codex Quota Ring is a lightweight desktop utility for Windows 10/11. It sits at the edge of your desktop and presents your Codex usage as a ring, so you can check limits without opening a web page, configuring an API key, or storing conversations and usage history.

### Who it is for

- Codex users who want their 5-hour and weekly limits visible while they work.
- Anyone who needs to decide quickly whether to continue a task or wait for a reset.
- Privacy-conscious users who want a local, read-only view of account usage.

### Highlights

- **Limits at a glance**: Switch the main ring between 5-hour and weekly limits, with a live reset countdown.
- **Optional Credits**: When available from the account, show read-only Credits balance, limit, and reset-credit count.
- **Dock when you need space**: Dock to the left, right, or top edge; collapse into a compact meter, expand on hover, and retract when the pointer leaves.
- **Built for the desktop**: A borderless, draggable, resizable window with position lock, always-on-top, a system tray, and multi-monitor position recovery.
- **Reliable refreshes**: Manual and scheduled refreshes, refresh after sleep, a live countdown, and the last valid data retained if a refresh fails.
- **Startup on your terms**: Optional autostart, start hidden, and Codex / ChatGPT lifecycle integration — all disabled by default.
- **High-DPI friendly**: 80%, 100%, 125%, and 150% UI scale options, with multi-monitor and mixed-DPI support.

### Quick start

1. Download and install the latest Windows installer from [Releases](https://github.com/caicai121263/codex-quota-ring/releases/latest).
2. Install and sign in to Codex Desktop or the Codex CLI.
3. Launch Codex Quota Ring; your limits appear after the first successful read.
4. Right-click the system-tray icon to refresh, switch the main ring, configure edge docking, or adjust resident options.

### Data and privacy

This is a **local-first, read-only** tool. It uses the local `codex.exe app-server --stdio` interface and requests only `account/read` and `account/rateLimits/read`.

It does not read browser cookies, OpenAI API keys, `auth.json`, conversations, project files, or usage history. It does not send telemetry, call remote usage endpoints, change account settings, restart tasks automatically, or install global mouse or keyboard hooks.

Credits appear only when the Codex App Server returns relevant account data; otherwise, the section remains hidden.

### Requirements and limitations

- Windows 10/11 x64 only.
- A locally available, signed-in Codex Desktop or Codex CLI installation is required. The app explains when Codex is missing or not signed in.
- Values are exactly what the Codex App Server returns. This app displays usage only; it cannot change limits, subscriptions, or account status.
- This is an independent community project and is not affiliated with OpenAI or the official Codex product.

### Local development

Node.js, Rust stable, the Windows WebView2 Runtime, and Microsoft C++ Build Tools are required.

```powershell
npm.cmd install
npm.cmd test
npm.cmd run tauri:dev
```

Build a Windows x64 NSIS installer:

```powershell
npm.cmd run tauri:build
```

### Feedback

Please use [Issues](https://github.com/caicai121263/codex-quota-ring/issues) for bug reports, feature ideas, and compatibility feedback. When possible, include the app's redacted diagnostics to help with investigation.
