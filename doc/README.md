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
| [设计标准](standards/DESIGN.md) | CLI、报告、笔记和后续桌面交互 | 用户可见行为和呈现变化 |
| [测试标准](standards/TESTING.md) | 阶段门槛、fixture和独立验证规则 | 引入新能力或新故障类型 |
| [工作流程](standards/WORKFLOW.md) | 每轮范围、日志、Git和验证顺序 | 开发协作方式变化 |
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

B05 三项已完成实际验收，[B05-T03 记录](validation/B05-T03.md)保留完整 Windows 回归、真实路径保护、独立审查及失败历史。下一批仅规划[B06 三张执行卡](execution/B06_STEPS.md)：主动比较表、流程图、TeX 及公式原图回退。任务状态以账本为准；当前计划不是新增表达能力已实现或科学核实的证明。
