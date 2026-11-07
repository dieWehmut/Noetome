# 近期开发步骤

真实状态以[任务账本](../../dev-log/tasks.json)和[当前状态](../../dev-log/STATUS.md)为准。

B06 的主动比较表、有据过程流程、明确公式保真分别按[B06-T01](../validation/B06-T01.md)、[B06-T02](../validation/B06-T02.md)、[B06-T03](../validation/B06-T03.md)验收；设置侧栏和本地重装见[B06U-T02](../validation/B06U-T02.md)。执行范围归档于[B06 步骤](B06_STEPS.md)。

B06V 三项现已实际验收：[一般图像](../validation/B06V-T01.md)、[单页扫描公式](../validation/B06V-T02.md)、[最低与当前稳定兼容矩阵](../validation/B06V-T03.md)。T03 覆盖 Windows Obsidian 1.6.7/1.15.0 明暗64唯一组合，完整 Windows Rust726、Windows Python441（434通过/7跳过）、Linux Python380（361通过/19跳过）、五视觉317次CLI；科学质量仍 not_run、知识仍 pending、生产发布未进行。归档卡见[B06V 步骤](B06V_STEPS.md)。

B07 三项现已实际验收：[有限流程检查点与恢复](../validation/B07-T01.md)、[持久预算预留下的有界重试](../validation/B07-T02.md)、[托管测试集合的审批与可恢复发布](../validation/B07-T03.md)。T03 实现 vault 六动词与 sqlite Schema1 托管根，approve 绑定计划正文/现场指纹/影响清单/owner 指纹，publish 分 stage-verify-commit 并可 needs_recovery 续跑，rollback 对后续人工改动报 rollback-conflict；端到端矩阵41项、独立审计126项检查通过，T02 的 Windows 原生门禁（813项Rust、503项Python）已补测并如实保留2项时间敏感失败与Linux完整Rust 121项Windows-only失败。收束提交 69a306b 已普通推送并核对远端一致。归档卡见[B07 步骤](B07_STEPS.md)。Windows 原生门禁在 T03 上未运行。

B08 三项现已实际验收：[研究候选队列与拒绝记忆](../validation/B08-T01.md)、[批准范围内的有界检索](../validation/B08-T02.md)、[写作反馈缺口回到修订重编译](../validation/B08-T03.md)。T01 实现 research 五动词与三表 sqlite 账本（application_id 0x4E525148），必做集合只由 adopt 冻结、越界主题只能以 out_of_scope 进队列、拒绝记忆用只追加事件区分 blocked_repeat 与 reopened，独立审计132项。T02 实现 retrieve 八动词与五表账本（0x4E525452），预留即计入的持久预算、五种结果、七类终止理由的证据判据、reserved 尝试补结算不重复预留，独立审计107项与99项（紧凑预算）。T03 实现 revision 五动词与五表账本（0x4E525652），缺口与候选分离、新 IR 版本只重编译声明依赖的表示、无来源补充标待核验、超上限升级、无新证据拒绝再推翻，独立审计49项。三者均记录真实 RED、独立审计器单测与 CLI 集成测试，收束提交分别为 e5c7c81、ad48698、98f1955，均已普通推送并核对远端一致。归档卡见[B08 步骤](B08_STEPS.md)。

依据[路线图 B09 行](ROADMAP.md)与 PRD 第14章页面清单，B09 只展开[B09 三张桌面个人版执行卡](B09_STEPS.md)：先把项目新建与只读检查接到桌面、再把四类既有根的状态做成只读任务中心、最后冻结打包配置并写安装后验收清单。三张卡现已全部实际验收；状态由日志工具维护，切批次不自动启动下一项。

## 1 B09-T01 桌面项目入口与真实工程状态（已验收，791d19b）

依赖B08-T03。新增受限 Tauri 命令 `pick_directory`/`project_create`/`project_open` 投影 `project::initialize` 与 `project::inspect` 的真实结果；前端新增「项目」页替换"尚未接入"空状态。新建只接受尚不存在的新目录并显示实际 `project_id`/`schema_version`/`created_at`/`state`/`database`；已存在、未初始化完成、损坏与外来布局按既有错误码（`PROJECT_PATH_EXISTS`/`PROJECT_INCOMPLETE`/`PROJECT_INVALID_CONFIG`/`PROJECT_INVALID_DATABASE`/`PROJECT_UNSAFE_PATH`/`PROJECT_IO_ERROR`）拒绝并保留现场。桌面层不新增写入路径、不联网、不调用模型、不创建 Vault；前端不重算或伪造状态。`cargo test -p noetome-desktop` 18 项通过（含新增 6 项四类真实返回与零写入快照），`npm run web:check` 与 `npm run web:build` 通过，Windows 原生安装内交互与 rfd 目录选择器如实列为未运行。详见[B09-T01 验收](../validation/B09-T01.md)。

## 2 B09-T02 桌面任务中心只读投影（已验收）

依赖B09-T01。新增只读命令 `task_read`/`task_kinds` 复用 `fixture_run::status`、`research_store::list`、`retrieval_store::list`、`revision_store::list` 四类既有只读入口；前端「任务中心」页分别显示四维状态、计数、候选五态、预算与终止理由、缺口与修订结果，不重算阈值、不转百分比。未建立显示为「尚未建立」（不当成失败），损坏与外来根保留 CLI 原错误码（如 `RESEARCH_UNKNOWN_ROOT`、`REVISION_UNKNOWN_ROOT`、`PROJECT_INVALID_DATABASE`）并原样显示`代码：说明`，不显示成空任务或成功。只读投影零写入，四类根读取前后快照比对；不调度新请求、不调用模型、不写 Vault。`cargo test -p noetome-desktop` 28 项通过（含新增 10 项），`npm run web:check` 与 `npm run web:build` 通过，Windows 原生安装内的任务类型切换与读取操作如实列为未运行。详见[B09-T02 验收](../validation/B09-T02.md)。

## 3 B09-T03 个人版打包配置与安装验收清单（已验收）

依赖B09-T02。冻结 `tauri.conf.json`、`capabilities/main.json`、`icons/manifest.json`、5 个图标、`package.json` 与验收清单共 10 个打包面成员（`crates/noetome-desktop/packaging-frozen.json`），只用 `longDescription` 一处实质改动反映项目入口与任务中心已接入，其余打包取值（`targets:["nsis"]`、`installMode:"currentUser"`、安装器语言 SimpChinese+English、不显示语言选择器、图标四项、最小窗口 640×600、CSP 只连 IPC、identifier）保持并由审计器逐项比对。新增 `doc/standards/DESKTOP_ACCEPTANCE.md`（9 节 40 余条：安装与首次启动、窗口与导航、Rust 通信、项目入口、任务中心、关闭/托盘/重启、卸载、明确不属于本清单的检查、判定规则），并声明静态检查与浏览器预览不能替代安装后测试。新增只读审计器 `scripts/verify_desktop_package.py` 与 34 项反例测试：本机实测打包审计 72 项检查失败 0、提示 0，`npm run web:check`/`web:build`、`cargo fmt --all --check`、`cargo check --workspace`、`cargo test -p noetome-desktop`（28 项）、`cargo build --release -p noetome-desktop` 全部通过并留原件。本机无 Windows 目标三元组与 NSIS 工具链，Windows 原生安装包未产出、未安装、未卸载，清单全部条目尚未由真人执行——如实单列，不声称通过。详见[B09-T03 验收](../validation/B09-T03.md)。

仍未完成的正式门槛包括科学200原子/98%基准、通用OCR、完整多教材知识编译、生产Vault人工保护、千页/万原子、3000笔记/200人工改动及24小时规模。当前小型工程任务不代替这些实测，也不宣布个人版发布或生产可用。
