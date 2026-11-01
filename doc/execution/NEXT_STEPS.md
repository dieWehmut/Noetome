# 近期开发步骤

真实状态以[任务账本](../../dev-log/tasks.json)和[状态](../../dev-log/STATUS.md)为准。B01三项已实际验收并推送；历史步骤归档到[B01步骤](B01_STEPS.md)。用户随后明确要求最小化到托盘，因此插入只有一项的B01A桌面交付批次，完成后回到B02文本来源和确定性闭环的滚动规划。

## B01A-T01 桌面最小化托盘与安装实测

**单一结果**：安装后的Windows应用最小化到系统托盘，可恢复原窗口，并能明确退出。

**依赖**：B01-T03。关联依据：用户2026年11月1日明确要求“要有最小化托盘”。这是桌面交付行为，不代表项目业务或完整桌面产品验收。

### 执行步骤

1. 核对当前Tauri版本的托盘与窗口事件API，复用指定图标。由Rust管理托盘，不开放新的任意文件、进程或shell前端权限。
2. 最小化时隐藏主窗口，保留进程与页面状态。托盘左键恢复主窗口，右键菜单提供“显示窗口”和“退出”；关闭按钮沿用原有退出行为。
3. 恢复时取消最小化、显示并聚焦已有主窗口，不重建页面。创建托盘失败须明确报告，不能把隐藏窗口留成不可恢复状态。
4. 运行相关格式、工作区与前端检查，重新构建NSIS。已有旧安装器已关闭，正常用户目录准备安装包并安装到已验收的当前用户目录。
5. 在实际安装后的窗口验证最小化后隐藏/进程存活、左键恢复、菜单恢复与退出、状态保留、关闭按钮退出、快捷方式重启及Rust通信；保存包与安装文件指纹。
6. 同步[设计](../standards/DESIGN.md)、README及独立验证记录，用日志工具完成，分步提交，检查通过后推送并核对远端。

### 验收

- 使用既有图标，托盘图标与Noetome提示出现。
- 最小化后窗口隐藏，进程存活；托盘点击和菜单均恢复窗口/焦点与已有页面状态。
- “退出”结束进程并移除托盘图标；窗口关闭仍退出；快捷方式能重启。
- 当前构建与安装文件严格匹配，既有帮助/版本及相关回归通过。

```powershell
cargo fmt --all --check
./scripts/cargo_local.ps1 check --workspace --locked --offline
./scripts/cargo_local.ps1 test --workspace --locked --offline
npm run web:build
npm run desktop:build -- --ci -- --locked
./scripts/install_desktop.ps1
python -m unittest discover -s tests -p 'test_*.py'
python scripts/devlog.py check
git diff --check
```

构建和代码检查不能代替原生托盘实测。仅验收当前Windows环境；本项不接项目GUI、不增加开机启动、驻留任务或模型功能。

## 下一批

B01A验收后按[路线](ROADMAP.md)、[数据契约](../technical/DATA_CONTRACTS.md)和[测试标准](../standards/TESTING.md)展开B02最多三项：单份UTF8来源登记、明确fixture的IR/NotePlan验证、临时Vault与双报告。当前未启动这些产品能力；切批次本身不启动任务。
