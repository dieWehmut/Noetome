# 桌面资料流程浏览器 QA

这是开发环境中的 Tauri mock IPC 交互测试，不代表真实 PDF、模型调用、Windows 窗口或
安装后的 UI 已验收。生产 App 不导入此目录，构建后的安装前端不包含这些模拟入口。

从仓库根目录启动 `npm run web:dev`，创建 `.artifacts/B15-T02/frontend`，在项目的
Playwright MCP 中导航到 `http://127.0.0.1:1420`，然后用
`browser_run_code_unsafe` 的 `filename` 参数执行 `apps/desktop/qa/workbench.playwright.js`。
返回结果必须 `passed: true`，每条 `checks[].passed` 都必须为 true；失败结果不会当成功。
该脚本只使用官方 Tauri `mockIPC` / `mockWindows`，假数据明确标记为 QA synthetic。
模型、凭据和预算恢复均被模拟，不发送 HTTP、不读取或保存真实 key。

覆盖自己的资料流程与失败边界、中文空格项目路径、完整原文及 `confirmedUnits`、
来源绑定不符、显式批量选择的 64 项/64 KiB 上限、取消与同一时刻的重复提交、
切页保留与切项目的迟到响应丢弃、模型与累计账本状态、导出与保存结果恢复、
独立离线样例，以及 640/1200 像素的明暗主题。四张截图保存在上述新证据目录。
测试返回值需另外保存为 JSON，截图和返回值均须注明浏览器 mock 的验证边界。
