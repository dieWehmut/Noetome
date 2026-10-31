# Noetome 自动开发指引

自动使用任务需要的工具和适用插件。Git按可验收任务分步commit，适当按批次建立 `codex/` 分支；检查通过后推送已有origin。普通提交与推送已获授权，不重复确认，不force push，不更改既有全局身份。

**全量规划，小步实现，逐项验收。** V1架构与全项目任务已预先确定，不回到后续由执行者临场设计的方式。本次规划任务不授权顺手实现产品；收到后续开发指令后使用本文件的循环。

## 每轮入口

1. `git status --short --branch`，保留用户和其他代理已有改动。
2. 读 `dev-log/STATUS.md`，运行 `python scripts/plan.py next`。
3. `python scripts/plan.py show <ID>`，读取当前完整任务卡及其中列出的标准。
4. 首次接手读 `doc/execution/HANDOFF.md`、ARCHITECTURE、STACK。

不凭聊天摘要、文件存在或提交成功推断功能完成。plan只返回建议，不执行工作；所有验收必须真实运行。

## 标准文件地图

| 权威内容 | 路径 |
| --- | --- |
| 产品需求与已采纳决策 | `doc/requirements/PRD.md`、`doc/requirements/DECISIONS.md` |
| 完整架构与选型 | `doc/technical/ARCHITECTURE.md`、`doc/technical/STACK.md` |
| 领域契约与SQLite结构基线 | `doc/technical/DATA_CONTRACTS.md`、`doc/technical/schema.sql` |
| Worker/CLI/IPC/Job/预算 | `doc/technical/RUNTIME_PROTOCOL.md` |
| 知识算法及模型候选 | `doc/technical/KNOWLEDGE_PIPELINE.md`、`doc/technical/MODEL_CONTRACTS.md` |
| 人工保护、发布恢复 | `doc/technical/PUBLICATION.md` |
| 配置字段和默认值 | `doc/technical/CONFIG.md` |
| 交互与Obsidian输出 | `doc/standards/DESIGN.md` |
| 测试与发行门禁 | `doc/standards/TESTING.md`、`doc/standards/RELEASE.md` |
| 自动测试与必要人验 | `doc/execution/AUTOMATED_TESTING.md`、`doc/technical/UI_AUTOMATION.md`、`scripts/autotest.py` |
| 全部任务与需求追溯 | `doc/execution/ROADMAP.md`、`doc/execution/tasks/`、`doc/execution/TRACEABILITY.md` |
| 执行与失败处理 | `doc/standards/WORKFLOW.md`、`doc/execution/FAILURE_PLAYBOOK.md` |
| 日志与验证 | `dev-log/README.md`、`scripts/devlog.py`、`scripts/plan.py`、`scripts/verify.py` |

当前用户指令优先；PRD/DECISIONS定产品行为；CONFIG定默认值；DATA_CONTRACTS定交换语义，schema.sql定存储结构；RUNTIME_PROTOCOL/PUBLICATION定状态与副作用。任务卡不得推翻硬约束。矛盾要记录具体字段和场景并同步修正，不能暗选方便的一边。

## 自动执行循环

用户要求连续或自动完成项目时，按 `next → show → start → 实现 → verify → complete → commit/push → next` 推进，不把一项完成当作整个项目完成。全库最多一个in_progress，子代理只在当前任务内分工，不能并行改账本或启动后续产品阶段。

用户说“继续这个项目”“继续开发”即按此完整循环推进至V1结束或真实阻塞，不只做一项。测试、修复和技术验收由Agent执行，不要求用户读代码或运行命令；只在缺真实配置、知识样本确认与最终体验时请求必要介入，细则见 `doc/execution/AUTOMATED_TESTING.md`。

- next=resume：读历史与失败证据，恢复当前任务。
- next=start：核对依赖，start后只实现本步，只引入本步使用的依赖。
- next=batch：当前批次全部完成后切换；后续卡已存在，无需重新规划。
- next=blocked：按失败手册记录根因和解除条件。同批独立任务可继续；没有可执行项就准确报告，不伪记done。
- next=complete：核对最终V1门禁，账本全done不代替发行证据。

同一根因连续修复最多3轮，每轮需新假设和验证；仍失败则block并保留现场，避免无限重写。可细分过大的未开始任务，但必须保留父验收和下游依赖，同步计划和追溯。架构替换、删需求与扩大V1范围不属于例行选择。

## 日志和证据

`dev-log/tasks.json` 是任务定义与状态唯一来源。TODO/DONE/STATUS、`doc/execution/tasks/*.md`、TRACEABILITY均生成，不手改；历史永远保留。

```text
python scripts/devlog.py start <ID> --note "本轮范围"
python scripts/verify.py <ID>
python scripts/devlog.py complete <ID> --note "实际结果" --validation "行为验收及真实结果" --artifact <源码文件> --report .artifacts/verification/<ID>.json
python scripts/devlog.py check
python scripts/plan.py check
```

先完成源码、测试和规范修改再verify。报告绑定实际命令退出码、针对性测试数量、必交付文件、任务定义及Git可见源码指纹；源码变更导致报告失效。日志与派生视图排除出源码指纹以避免自引用。报告不能证明科学判断正确；真实质量、视觉和24h证据按RELEASE另验。

改任务定义后运行plan render和devlog render；状态/历史仅用start/complete/block/reopen/batch维护。工具不会后台猜测开发完成，执行者必须每项主动调用。

## 不可绕过

- 原始来源只读；B11人工保护门禁通过前只写新的临时Vault。
- 模型只产结构化候选，不写文件/DB、不执行shell、不决定自身验收。
- 自动模式不能解除人工编辑、锁和过期审批保护；文件锁不可靠就拒绝发布。
- fixture/mock只证明工程行为，不证明真实召回、准确率或Obsidian兼容。
- 缺API凭据/预算可继续离线开发，对应真实门禁保持阻塞，不猜密钥或自行购买。
- 不改成Markdown唯一存储，不顺手添加云同步、向量库、Agent框架或运行时服务。
- 不扫描未选取私人目录，不提交密钥、私有教材或完整请求日志。

## 验证和Git

通用检查：`python -m unittest discover -s tests -p "test_*.py"`、`python scripts/check_docs.py`、devlog check、plan check、`git diff --check`。改PRD后build_prd，不手改HTML。产品命令是未来任务要求，规划完成不能声称尚不存在的产品已测试。

统一入口为 `python scripts/autotest.py --task <ID>`，会先检查仓库再执行任务检查。无参数只测当前仓库治理工具，不冒称V1通过。

每项可拆多个有意义commit，最后同步状态。批次内沿用分支，下一批从已验证HEAD建立codex分支，避免孤立分支积压。只stage相关工作；检查通过后推送并核对远端hash，不顺带提交用户无关改动。

python指当前选定解释器，使用可用运行时，不将本机用户名绝对路径写进仓库。最终说明结果、验证、剩余障碍和下一步。
