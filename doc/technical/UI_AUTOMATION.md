# Windows桌面自动验收方案

固定方案：Tauri官方支持的外部tauri-driver路线，Microsoft Edge WebDriver驱动WebView2，WebdriverIO9+Mocha执行断言。版本锁定见[STACK](STACK.md)。这是B15/B16必须实现的工程，不表示当前已经有桌面产品或通过UI测试。[官方说明](https://v2.tauri.app/develop/tests/webdriver/)、[官方WebdriverIO接线示例](https://v2.tauri.app/develop/tests/webdriver/example/webdriverio/)

## 1 执行入口与文件

- 唯一桌面端到端入口：`pnpm --dir apps/desktop test:e2e`，对应`wdio run wdio.conf.ts`。
- 配置：`apps/desktop/wdio.conf.ts`；首例：`apps/desktop/test/e2e/smoke.spec.ts`；后续同目录按场景拆文件。
- 宿主入口`python scripts/autotest.py --task <ID>`按指定任务执行适用层级；无参数只检查当前仓库治理工具。Agent负责安装测试依赖、启动、运行、修复及采证，不让小白用户执行命令或读取代码。
- B15先测试构建出的真实应用二进制及同一Rust worker；B16必须对NSIS实际安装后的二进制重复核心冒烟，不能只测Vite开发网页。
- 单套件超时1800秒，maxInstances=1；单场景默认60秒，长恢复场景显式上限。零测试、跳过所有用例、driver未连上均失败。
- Mocha标准汇总输出`N passing`供验证器读取，断言失败非零退出。不能用输出“1 passing”的自制脚本冒充真实框架结果。

## 2 外部驱动接线

wdio配置使用host=127.0.0.1、独占测试端口、`tauri:options.application`指向准备阶段验证过的绝对EXE路径，`tauri:options.args`传`["--data-dir", "<本次隔离目录>"]`。准备阶段记录app/worker/parser/WebView2/driver版本和hash；拒绝路径指向开发时另一个旧安装。应用及worker必须使用本次构建产物。该接线字段已核对[官方driver源码](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-driver/src/server.rs)，首次引入仍须以锁定版本执行冒烟。

测试脚本显式启动`tauri-driver --port <代理端口> --native-port <原生端口> --native-driver <msedgedriver.exe绝对路径>`；driver版本与应用使用的Fixed Version WebView2运行时匹配。下载只来自Microsoft官方渠道，锁定版本/hash后可缓存。不得通过安装浏览器最新版猜测应用WebView2版本。原生驱动路径参数见[官方手动安装说明](https://v2.tauri.app/develop/tests/webdriver/manual-setup/)。

beforeSession等待可用驱动后建立session；afterSession及异常清理关闭session、tauri-driver、msedgedriver和测试应用。进程归属于测试专用Windows Job Object，超时清理仅杀自己创建的PID树，不能按进程名杀用户Obsidian/Edge/Noetome。测试端口已占用则换受控空闲端口并记录，不终止占用者。

不添加embedded WebDriver服务或Tauri测试插件到生产包，不开放用户可用的任意IPC/mock端口。页面级单元测试可mock协议；端到端必须经过真实UI→Rust壳→命名管道→worker→SQLite/临时Vault链路。

## 3 隔离和确定性

每次运行生成唯一case根，包含项目、Vault、fixture副本、日志、截图、报告和app-data目录。测试不得打开用户现有项目、Vault、凭据或扫描个人目录。应用提供正常可见的项目路径输入，供自动化选取临时路径；原生文件对话框不是唯一入口。

应用的`--data-dir <dir>`是正常运行参数，用于隔离settings和WebView2 profile；CLI启动的worker从project路径导出私有state，并核对project_id。UI测试不覆盖真实APPDATA/LOCALAPPDATA，不复用用户WebView2目录。安装测试在专用Windows测试账户或VM中运行，记录OS、权限、DPI及中文字体。

离线E2E使用fixture provider及受控本地HTTP stub：fixture/fake能力在明确测试配置显示producer=fixture，不偷换真实质量状态；stub只监听loopback，网络适配器测试模式允许该明确地址，正常研究模式仍禁止私网抓取。所有测试场景禁止外部收费请求，使用固定时钟/ID种子只限fixture执行层。

mock用于故障响应、延迟、429、畸形JSON、响应丢失和预算耗尽；不要mock掉待验证的文件句柄、DB事务、IPC或发布状态机。跨进程文件修改由独立测试进程真实执行，Windows共享冲突必须来自实际句柄。

## 4 稳定选择器和等待

交互控件及状态行使用稳定`data-testid`：`project-path-input`、`project-create`、`source-add-path`、`run-start`、`run-status`、`run-pause`、`run-resume`、`run-cancel`、`changes-list`、`change-diff`、`change-edit`、`change-approve`、`publish-start`、`recovery-panel`、`completion-report`。列表子项附entity ID，测试不依赖翻译文本、CSS类或行序。

data-testid不替代可访问性：控件保留label、role、键盘顺序、焦点和禁用原因。等待使用明确状态/事件序号及合理timeout，不用固定sleep猜加载完成，不用循环点击掩盖响应问题。截图比较只覆盖稳定区域，动态时间/ID屏蔽必须有清单。

## 5 必须自动覆盖的场景

| 场景 | 可观察断言 |
| --- | --- |
| 新建→导入→fixture生成→报告 | 真项目ID、来源hash、Span总数、producer、临时Vault与报告一致 |
| 错误输入和路径 | 缺文件/错误编码/越界路径有可读错误；没有生成假的成功记录 |
| 两种模式和配置 | source/research参数进入Run快照；缺密钥/未批范围明确阻塞 |
| 暂停恢复 | 不再领取新Job，达到paused后恢复，事件序号连续、不重复账单或产物 |
| 关闭与重开 | 托盘继续时worker状态持续；安全暂停退出再开显示paused，不自动偷偷执行 |
| 取消 | cancelled与ended区分，已发布内容不删除，外部未知费用仍可见 |
| 多源冲突及证据 | 支持/冲突/条件定位可打开，未核实不展示为已确认 |
| 编辑候选 | 创建新revision、重新验证映射、旧审批失效 |
| 人工编辑与过期审批 | 外部进程在预览后改文件，发布拒绝覆盖，B/C/N和stale可见 |
| 锁冲突及文件竞态 | 持写句柄、移动间隙新建文件、原子保存均保留外部字节 |
| 发布崩溃恢复 | 在journal各关键点终止worker，重启按实际磁盘恢复；UI不提前published |
| 未知请求 | 超时后uncertain/预算unknown；无自动重发，显式处理有记录 |
| 部分发布 | 拒绝一项后依赖闭包及最终索引重新校验，报告明确剩余项 |
| 安装和离线启动 | 无系统Python/Node可启动，PDFium/OCR资源可加载，预览不请求CDN |

每项用例断言外部结果或独立账本查询，不只断言某函数被调用。fixture正确性经人工可审阅，不解释为客观科学gold。DPI100/150/200%、浅/深色、窄窗口、中文长标题、键盘操作的布局检查用自动截图加关键元素尺寸/可见性断言，必要的最终可读性由人验。

## 6 失败采证与人验边界

失败保存：当前窗口截图、脱敏DOM/可访问性摘要、浏览器与Tauri/worker/driver日志、最后事件sequence、Run/Job状态、SQLite一致性摘要、publish journal、实际文件hash清单、应用/驱动/OS版本。目录固定`.artifacts/ui/<run-id>/<case-id>/`；私有正文不进入公共提交。

测试失败保持临时项目和Vault，不在finally自动删除调查证据；成功可按测试保留策略清理。失败报告说明期望、实际和复现命令，不能只报timeout。driver/环境故障与产品断言失败分开分类，二者均不得标测试通过。

自动测试承担可重复的UI、状态和恢复验收。真实Obsidian插件外兼容、关键科学图/公式逐元素意义、最终中文阅读体验及用户目标是否合意仍需必要人验，见[RELEASE](../standards/RELEASE.md)。Agent先生成可审阅结果、截图和简短问题清单，只在这些节点邀请用户确认；不因未来人验需要而停止可以继续的离线工程。
