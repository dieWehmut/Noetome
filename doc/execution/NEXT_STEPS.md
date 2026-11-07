# 近期开发步骤

真实状态以[任务账本](../../dev-log/tasks.json)和[当前状态](../../dev-log/STATUS.md)为准。

B06 的主动比较表、有据过程流程、明确公式保真分别按[B06-T01](../validation/B06-T01.md)、[B06-T02](../validation/B06-T02.md)、[B06-T03](../validation/B06-T03.md)验收；设置侧栏和本地重装见[B06U-T02](../validation/B06U-T02.md)。执行范围归档于[B06 步骤](B06_STEPS.md)。

B06V 三项现已实际验收：[一般图像](../validation/B06V-T01.md)、[单页扫描公式](../validation/B06V-T02.md)、[最低与当前稳定兼容矩阵](../validation/B06V-T03.md)。T03 覆盖 Windows Obsidian 1.6.7/1.15.0 明暗64唯一组合，完整 Windows Rust726、Windows Python441（434通过/7跳过）、Linux Python380（361通过/19跳过）、五视觉317次CLI；科学质量仍 not_run、知识仍 pending、生产发布未进行。归档卡见[B06V 步骤](B06V_STEPS.md)。

B07 三项现已实际验收：[有限流程检查点与恢复](../validation/B07-T01.md)、[持久预算预留下的有界重试](../validation/B07-T02.md)、[托管测试集合的审批与可恢复发布](../validation/B07-T03.md)。T03 实现 vault 六动词与 sqlite Schema1 托管根，approve 绑定计划正文/现场指纹/影响清单/owner 指纹，publish 分 stage-verify-commit 并可 needs_recovery 续跑，rollback 对后续人工改动报 rollback-conflict；端到端矩阵41项、独立审计126项检查通过，T02 的 Windows 原生门禁（813项Rust、503项Python）已补测并如实保留2项时间敏感失败与Linux完整Rust 121项Windows-only失败。收束提交 69a306b 已普通推送并核对远端一致。归档卡见[B07 步骤](B07_STEPS.md)。Windows 原生门禁在 T03 上未运行。

B08 三项现已实际验收：[研究候选队列与拒绝记忆](../validation/B08-T01.md)、[批准范围内的有界检索](../validation/B08-T02.md)、[写作反馈缺口回到修订重编译](../validation/B08-T03.md)。T01 实现 research 五动词与三表 sqlite 账本（application_id 0x4E525148），必做集合只由 adopt 冻结、越界主题只能以 out_of_scope 进队列、拒绝记忆用只追加事件区分 blocked_repeat 与 reopened，独立审计132项。T02 实现 retrieve 八动词与五表账本（0x4E525452），预留即计入的持久预算、五种结果、七类终止理由的证据判据、reserved 尝试补结算不重复预留，独立审计107项与99项（紧凑预算）。T03 实现 revision 五动词与五表账本（0x4E525652），缺口与候选分离、新 IR 版本只重编译声明依赖的表示、无来源补充标待核验、超上限升级、无新证据拒绝再推翻，独立审计49项。三者均记录真实 RED、独立审计器单测与 CLI 集成测试，收束提交分别为 e5c7c81、ad48698、98f1955，均已普通推送并核对远端一致。归档卡见[B08 步骤](B08_STEPS.md)。

依据[路线图 B09 行](ROADMAP.md)与 PRD 第14章页面清单，当前批次只展开[B09 三张桌面个人版执行卡](B09_STEPS.md)。三张卡依次验收：先把项目新建与只读检查接到桌面，再把四类既有根的状态做成只读任务中心，最后冻结打包配置并写安装后验收清单；状态由日志工具维护，切批次不自动启动下一项。

## 1 B09-T01 桌面项目入口与真实工程状态

依赖B08-T03。新增受限 Tauri 命令投影 `project::initialize` 与 `project::inspect` 的真实结果；前端新增项目页替换"尚未接入"空状态。新建只接受尚不存在的新目录并显示实际 `project_id`/`schema_version`/`created_at`/`state`/`database`；已存在、未初始化完成、损坏与外来布局按既有错误码拒绝并保留现场。桌面层不新增写入路径、不联网、不调用模型、不创建 Vault；前端不重算或伪造状态。Rust 单测覆盖四类结果，前端 `tsc --noEmit` 与 `vite build` 实际通过，Windows 原生安装内交互测试如实列为未运行。

## 2 B09-T02 桌面任务中心只读投影

依赖B09-T01。新增只读命令复用 run/research/retrieve/revision 四类既有状态查询，前端分列显示四维状态、计数、候选五态、预算与七类终止理由、缺口与修订结果，不重算阈值。未建立、损坏、外来布局分别显示真实错误码，不显示成空任务或成功。四维状态口径与 CLI 报告一致：分母为零显示不适用，未启用研究显示未请求或不适切，不用百分比代替计数。只读投影零写入，读取前后对每个根做快照比对；不调度新请求、不调用模型、不写 Vault。

## 3 B09-T03 个人版打包配置与安装验收清单

依赖B09-T02。核对并冻结 `tauri.conf.json` 的打包目标、图标、NSIS installMode 与语言，任何改动逐条说明效果；新增 `doc/standards/DESKTOP_ACCEPTANCE.md` 列出安装后必须人工执行的检查项（窗口、导航、Rust 通信、帮助与版本、项目入口、任务中心、关闭与重启、托盘），每项含操作/期望/记录位置，并声明静态与浏览器预览不能替代安装后测试。新增 `scripts/verify_desktop_package.py` 只读审计配置取值、图标指纹与清单完整性，并对篡改分别报错；本环境实际执行的前端与 Rust 构建、打包前置检查留下原件，Windows 原生 NSIS 安装与卸载未运行时单列。

仍未完成的正式门槛包括科学200原子/98%基准、通用OCR、完整多教材知识编译、生产Vault人工保护、千页/万原子、3000笔记/200人工改动及24小时规模。当前小型工程任务不代替这些实测，也不宣布个人版发布或生产可用。
