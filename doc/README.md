# 开发标准文件入口

本目录是 Noetome 的需求、技术、设计和执行标准入口。用户已经采纳原需求文档 D01 至 D15 的建议；按已定基线小步执行，不重复询问相同默认策略。

## 开始一轮工作

1. 阅读根目录 [AGENTS.md](../AGENTS.md)。
2. 阅读 [当前状态](../dev-log/STATUS.md) 和 [待办事项](../dev-log/TODO.md)，以账本状态为准。
3. 在 [近期步骤](execution/NEXT_STEPS.md) 中找到本轮任务，只读与它相关的需求和标准。
4. 按 [工作流程](standards/WORKFLOW.md) 记录开始、实现、验证、完成及提交。

## 标准文件地图

| 文件 | 负责什么 | 何时更新 |
| --- | --- | --- |
| [产品需求](requirements/PRD.md) | 40组功能及完整产品验收边界 | 产品行为或验收范围变化 |
| [决策基线](requirements/DECISIONS.md) | D01至D15已采纳建议及执行解释 | 有依据的新决定或实验结果 |
| [已授权模型服务](requirements/MODEL_SERVICE.md) | 用户选定网关、模型、累计预算、本机凭据及复用入口 | 真实模型调用前复用；用户改变配置时更新 |
| [架构规范](technical/ARCHITECTURE.md) | 模块边界、技术方向和阶段约束 | 跨模块接口或责任变化 |
| [数据契约](technical/DATA_CONTRACTS.md) | 当前最小实体、身份、来源和版本语义 | 增加持久字段、Schema或接口 |
| [Parser sidecar 协议](technical/SIDECAR_PROTOCOL.md) | 单请求 wire、资源能力、限额与进程宿主错误 | parser 协议或生命周期边界变化 |
| [PDF 解析沙盒契约](technical/PDF_PARSE_CONTRACT.md) | 原始PDF、完整物理页、CropBox坐标、逐页UTF8及只读重查 | 文本PDF解析、定位或报告变化 |
| [PDF 来源登记契约](technical/PDF_SOURCE_CONTRACT.md) | Schema3、原始版本、独立解析尝试、复用键及缓存保护 | PDF项目登记、迁移、重解析或持久报告变化 |
| [知识抽取网关契约](technical/EXTRACTION_CONTRACT.md) | 严格候选、真实来源绑定、单请求预算、失败审计及离线fake边界 | 抽取请求、候选、provider或输出保护变化 |
| [单请求文本API契约](technical/EXTRACTION_API_CONTRACT.md) | Responses配置、外部凭据引用、预算预留、原响应及API候选报告 | 文本API、用量、HTTP故障或凭据保护变化 |
| [有界来源知识核对](technical/EXTRACTION_REVIEW_CONTRACT.md) | 已保存候选、独立作者gold、真实来源证据及离线测量核对 | extract verify输入、判据、分母或输出保护变化 |
| [有界多来源知识协调](technical/KNOWLEDGE_COORDINATION_CONTRACT.md) | 显式身份映射、来源依据重验、重复/互补保留与待复核分母 | knowledge coordinate输入、关系或保护变化 |
| [条件与来源时间协调](technical/KNOWLEDGE_VARIANTS_CONTRACT.md) | 严格v2条件状态、来源背景、并列变体与历史先后 | 条件、时间证据、协调版本或分母变化 |
| [有据冲突与证据不足](technical/KNOWLEDGE_CONFLICTS_CONTRACT.md) | 严格v3冲突、可比较十进制、反证保留与逐原出现处理记录 | 冲突分类、证据不足、处理记录或相关分母变化 |
| [有据主动表达](technical/ACTIVE_PRESENTATION_CONTRACT.md) | 来源与协调重验、比较表/正文选择、逐格映射及新测试Vault | knowledge present、表达计划、渲染与预览报告变化 |
| [有界过程与流程表达](technical/PROCESS_FLOW_CONTRACT.md) | 独立过程来源核对、明确步骤/关系、流程图及有据文字回退 | process-review、flow、节点/边映射与目标能力变化 |
| [明确选定公式保真](technical/FORMULA_FIDELITY_CONTRACT.md) | 原始 TeX、作者语义声明、公式 PNG 区域及 MathJax/原图回退 | formula、公式源码/附件、语义及目标能力变化 |
| [明确选定图像保真](technical/IMAGE_FIDELITY_CONTRACT.md) | 原 PNG、作者手工结构声明、有限 SVG 与原图/阻断决策 | image、标签/箭头/单位/图例/空间及图像能力变化 |
| [明确扫描公式重建](technical/SCAN_FORMULA_CONTRACT.md) | 原扫描页/区域、单请求视觉识别、原响应及等价表达核对 | scan-formula、识别策略、原件映射与关键未知阻断 |
| [视觉目标兼容](technical/VISUAL_COMPATIBILITY.md) | 精确能力组合、独立运行时证据和严格流程引擎版本 | 跨版本矩阵、目标能力或渲染引擎身份变化 |
| [有限运行检查点](technical/RUN_CHECKPOINT_CONTRACT.md) | 固定六单元、独立运行日志、严格 snapshot/intent、控制与发布恢复边界 | Run字段、恢复门禁、版本失效或调度控制变化 |
| [持久请求预算](technical/REQUEST_BUDGET_CONTRACT.md) | 人工环回请求、原子预留、未知承诺、attempt结算及有限重试 | run api、预算策略/事件、传输原件或恢复门槛变化 |
| [研究候选队列](technical/RESEARCH_QUEUE_CONTRACT.md) | 三类内容划分、候选八字段、拒绝记忆与重开、归并与只追加事件 | research 动词、候选字段、状态机或控制目录布局变化 |
| [有界检索](technical/RETRIEVAL_BUDGET_CONTRACT.md) | 检索计划、预留即计入的持久预算、结果分类、七类终止理由的证据判据 | retrieve 动词、预算口径、终止理由或控制目录布局变化 |
| [章节粗对齐](technical/OUTLINE_ALIGNMENT_CONTRACT.md) | 作者包与冻结根、章节身份与概念身份、七类对齐警告判据、输出根的字节确定性 | outline 动词、警告规则、冻结根或输出根布局变化 |
| [水平与相关性筛选](technical/RELEVANCE_FILTER_CONTRACT.md) | 用户水平必要性判据、排除理由登记与正文落点、必要前提拒绝、双密度原子不减少、输出根字节确定性 | relevance 动词、理由白名单、必要性判据或输出根布局变化 |
| [同类结构一致性与导航复习](technical/STRUCTURE_NAVIGATION_CONTRACT.md) | 章节骨架与四类偏离判据、概念唯一解释位置、链接解析与转义、导航确定性生成、输出根字节确定性 | structure 动词、骨架/链接规则或输出根布局变化 |
| [小节提升与链接迁移](technical/LINK_MIGRATION_CONTRACT.md) | 引用按知识对象而不是文件名核对、重定向位置保留、四种变更判据、身份保持与断链为零、输出根字节确定性 | migrate 动词、对象/计划字段、身份判据或输出根布局变化 |
| [可移植来源索引](technical/SOURCE_INDEX_CONTRACT.md) | 四区互斥、正文来源负担拒绝、来源按字节摘要识别、重定位三态、输出根字节确定性与所有权身份 | index 动词、区域判定、重定位判据或输出根布局变化 |
| [写作反馈修订](technical/REVISION_FEEDBACK_CONTRACT.md) | 缺口问题与候选分离、新 IR 版本与依赖重编译、无新证据不推翻、超限升级 | revision 动词、原子/表示字段、修订闸门或控制目录布局变化 |
| [设计标准](standards/DESIGN.md) | CLI、报告、笔记和后续桌面交互 | 用户可见行为和呈现变化 |
| [测试标准](standards/TESTING.md) | 阶段门槛、fixture和独立验证规则 | 引入新能力或新故障类型 |
| [工作流程](standards/WORKFLOW.md) | 每轮范围、日志、Git和验证顺序 | 开发协作方式变化 |
| [桌面安装验收](standards/DESKTOP_ACCEPTANCE.md) | 安装后人工验收清单与判定规则 | 打包目标、窗口与命令面或桌面入口变化 |
| [阶段路线](execution/ROADMAP.md) | 后续阶段目标及进入条件 | 阶段验收后滚动修订 |
| [近期步骤](execution/NEXT_STEPS.md) | 下一批最多3个可执行任务卡 | 当前批次完成后展开下一批 |
| [任务模板](templates/TASK.md) | 新任务卡的必填内容 | 工作卡结构变化 |

状态不在这些文件中重复手工维护。[任务账本](../dev-log/tasks.json) 是事实来源，[已完成事项](../dev-log/DONE.md) 和其他日志视图由工具生成。计划里的“预期结果”不等于已实现能力。

当前有界来源核对的真实CLI、独立校验、完整回归与保存输入保护见[B04-T03验证](validation/B04-T03.md)；用户追加的移除系统标题行、单行菜单与实际安装窗口操作见[窗口验证](validation/B04-T03-ui-single-row.md)。验证记录保留实际结果和收束状态，不替代任务账本。

B04R收束提交已普通推送并核对远端，执行卡归档于[B04R步骤](execution/B04R_STEPS.md)。[B05归档步骤](execution/B05_STEPS.md)规定重复/互补、条件/历史和冲突/证据不足三项；用户本轮优先追加侧栏、分类设置及默认main，执行入口见[近期步骤](execution/NEXT_STEPS.md)。当前协调实施依据见[B05-T01计划](execution/B05-T01_COORDINATION_PLAN.md)，状态仍由账本生成，计划中的命令和预期不算验收证据。

[B05-T01验证记录](validation/B05-T01.md)登记实测两事实/三处来源、多支持保留、独立作者原件、完整回归、审查修复及原始归档；最终限定复查包含重复证据链接同步伪造的拒绝验证。记录保留每次失败与当时状态，完成状态只由任务账本生成；明确身份的自造样例不证明真实文献独立性或科学正确率。

[B05-T02计划](execution/B05-T02_VARIANTS_PLAN.md)与[新作者样例](../fixtures/knowledge-coordination-v2/README.md)定义严格v2的条件与来源时间范围，真实检查按[B05-T02记录](validation/B05-T02.md)追加。调用前原文、旧Gold2及独立expected先冻结；计划和标准中的新接口须有本项实际验收才能算完成。

`requirements/PRD.html` 是由 Markdown 生成的阅读版；只修改 Markdown 后重新构建。其他标准文件以 Markdown 为准。旧 `docs` 路径仅保留迁移入口。

用户追加的窄图标栏、分组侧栏、独立分类设置与太阳/月亮主题切换见[B05U-T01验收](validation/B05U-T01.md)。记录分别保存浏览器、完整视觉对照、安装版真实IPC/保存/重启及回归证据；GitHub默认main和远端HEAD已实际核对。当前执行入口仍由账本和近期步骤维护。

遇到标准冲突时：当前用户指令优先，其次已采纳产品需求与决策；技术和设计负责具体实现；近期步骤缩小本轮范围，不得取消产品硬约束。将实质冲突写入日志并修正文档，不暗中选择方便的一边。

B05 三项已完成实际验收，[B05-T03记录](validation/B05-T03.md)保留完整 Windows 回归、真实路径保护、独立审查及失败历史。[B06执行卡](execution/B06_STEPS.md)中的主动比较表与有据流程图已完成，来源重建、完整回归、独立语义篡改、NTFS和真实Obsidian证据分别见[B06-T01记录](validation/B06-T01.md)与[B06-T02记录](validation/B06-T02.md)。原始TeX/公式原图回退按[公式契约](technical/FORMULA_FIDELITY_CONTRACT.md)推进，真实状态仍见[近期步骤](execution/NEXT_STEPS.md)及账本；当前工程预览不代表科学核实或生产发布。

[B06V-T01记录](validation/B06V-T01.md)保存明确PNG/作者手工声明的有限重绘、原图回退、关键未知阻断，以及真实原生不可读/布局反例与修复、独立同步篡改、完整回归和干净Obsidian证据。边界由[图像契约](technical/IMAGE_FIDELITY_CONTRACT.md)约束；一般自动识别、扫描重建和跨版本支持分别验收，不从有界工程样例推定科学质量。

[B06V-T02实施方案](execution/B06V-T02_SCAN_PLAN.md)限定一个公开自造扫描页公式、冻结的清晰/关键缺损状态与一个固定识别方案。原件、真实识别和语义核对分开；失败不修改独立 expected，不用正确阻断代替清晰公式重建验收。规则见[扫描契约](technical/SCAN_FORMULA_CONTRACT.md)，真实状态仍由账本维护。

[B06V-T03验收](validation/B06V-T03.md)记录最低1.6.7与当时稳定1.15.0的Windows明暗64组合、实际MathJax/Pele/安装指纹、原生数学/箭头/图片/链接、手写格式探针、完整回归及全部失败历史。[精确能力契约](technical/VISUAL_COMPATIBILITY.md)约束未知与混配版本的回退，兼容声明不能放宽源语义或推成科学/通用OCR/生产发布通过。

[B07-T01计划](execution/B07-T01_RUN_PLAN.md)与[运行检查点契约](technical/RUN_CHECKPOINT_CONTRACT.md)限定一个先验冻结的 fixture 六单元流程，运行日志与原六表编译数据库分别保存调度和发布依据。真实中断、控制、版本失效、原件保护与完整回归各自验收；计划和接口不表示已通过24小时、生产Vault或其他平台恢复，状态仍由账本维护。

[B07验证](validation/B07-T03.md)记录托管测试集合六动词（adopt/plan/approve/publish/status/rollback）、sqlite Schema1 托管根、approve 四样绑定与 publish 的 stage-verify-commit 恢复语义；端到端矩阵41项、独立审计126项检查通过，Windows 原生门禁在 T03 上未运行。[B08执行卡](execution/B08_STEPS.md)依据路线图B08行与 PRD FR-11/FR-12/FR-13/FR-14 只展开三张todo：候选队列与授权边界、批准范围内的本地合成检索与七类终止理由、写作反馈缺口回到修订与依赖重编译。研究能力目前在编译报告中如实显示为未请求；真实互联网检索、付费模型研究、科学质量与规模门槛仍分别实测，执行卡本身不构成实现或验收。

[B09执行卡](execution/B09_STEPS.md)依据路线图B09行与 PRD 第14章只展开三张 todo：桌面项目入口与真实工程状态、桌面任务中心只读投影、个人版打包配置与安装验收清单。
[B09-T01验收](validation/B09-T01.md)记录桌面新增 `pick_directory`/`project_create`/`project_open` 三条命令、共六项字段全部来自 `noetome_cli::project` 的 `initialize`/`inspect`、`PROJECT_*` 错误码原样透传与零写入快照；Windows 原生安装内交互与系统目录选择器未运行。[B09-T02验收](validation/B09-T02.md)记录桌面任务中心对六单元运行、研究队列、有界检索、写作反馈修订四类根的只读投影，四维状态按确定性规则分开计数、缺失显示为「尚未建立」而损坏保留 CLI 原错误码、读取前后快照证明零写入；从桌面发起运行/检索/修订、审批与发布仍未接入。桌面交互仍须在安装后的应用内验收，静态预览与浏览器构建不能替代。

[B09-T03验收](validation/B09-T03.md)记录 `crates/noetome-desktop/packaging-frozen.json` 冻结的十个打包面成员（`tauri.conf.json`、能力清单、图标清单、五个图标、`package.json`、安装后验收清单）、只读审计器 `scripts/verify_desktop_package.py` 的七十二项检查与三十四项反例测试、以及安装后人工清单九个章节。本机只有 `x86_64-unknown-linux-gnu` 目标且没有 NSIS 工具链，Windows 原生安装包未产出、未安装、未卸载，清单全部条目尚未执行，均单列为未运行；不以此宣布个人版发布或生产可用。

[B10执行卡](execution/B10_STEPS.md)依据路线图B10行与 PRD 剩余结构类需求只展开三张 todo：章节粗对齐与全局概念清单、用户水平与相关性筛选的可审计记录、同类结构一致性与导航复习组织。三项均为确定性规划层，只读已登记 Span 与已核对 IR，零模型调用、零网络请求、只写新测试根，不新增模型提供商，不同时做完整阅读器与编辑器。

[B11执行卡](execution/B11_STEPS.md)依据路线图B11行与 PRD 第9章笔记结构与知识关联、第10章来源追溯与结果交付只展开三张 todo：小节提升为独立笔记与链接批量更新、可移植来源索引与原始资料重定位、全库链接可用性与唯一解释位置审计。三项仍为确定性解析与审计层，只读已冻结作者包与已核对 IR，零模型调用、零网络请求、只写新测试根，不做完整阅读器与编辑器；链接迁移、重定位与审计本身的实测证据必须分别落盘，执行卡不构成实现或验收。
