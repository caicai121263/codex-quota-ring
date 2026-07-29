# Codex Quota Ring

一个 Windows 本地桌面轨道，用环形图显示 Codex 的 5 小时额度、周额度、重置倒计时和可用的只读 Credits 信息。

[下载最新 Windows 安装包](https://github.com/caicai121263/codex-quota-ring/releases/latest)

## 功能

- 主环可在 5 小时额度和周额度之间切换。
- 支持 80%、100%、125%、150% 界面缩放。
- 单飞刷新、5 秒内存缓存，并在读取失败时保留最近一次有效额度。
- 无边框窗口可拖动、拉伸、锁定、始终置顶，并可靠恢复多显示器位置。
- 系统托盘支持显示/隐藏、立即刷新、设置、主环切换、位置锁定、置顶、开机启动和退出。
- 启动、手动刷新和定时刷新均不会弹出命令行窗口。
- 设置面板会根据屏幕边缘向上或向下展开，兼容高 DPI 显示缩放。
- 支持单实例，以及 Codex / ChatGPT 启停联动（默认关闭）。
- 支持开机启动后隐藏、睡眠恢复刷新、实时倒计时和脱敏诊断复制。
- 可选的左、右、上边缘停靠：自动吸附为迷你额度环，悬停展开、离开收回。

## 隐私边界

- 只通过本机 `codex.exe app-server --stdio` 调用只读的 `account/read` 和 `account/rateLimits/read`。
- 不读取浏览器 Cookie、OpenAI API Key、`auth.json`、对话、项目文件或额度历史。
- 不使用遥测、远程额度接口、账户写操作或自动续跑。

## 开发

需要 Node.js、Rust stable、Windows WebView2 Runtime 和 Microsoft C++ Build Tools。

```powershell
npm.cmd install
npm.cmd test
npm.cmd run tauri:dev
```

构建 Windows x64 NSIS 安装器：

```powershell
npm.cmd run tauri:build
```
