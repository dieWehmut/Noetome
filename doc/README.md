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
| [架构规范](technical/ARCHITECTURE.md) | 模块边界、技术方向和阶段约束 | 跨模块接口或责任变化 |
| [数据契约](technical/DATA_CONTRACTS.md) | 当前最小实体、身份、来源和版本语义 | 增加持久字段、Schema或接口 |
| [候选知识 v1](technical/MODEL_CANDIDATE_PROTOCOL.md) | 来源Span、离线请求、用量、未核验候选及重读 | 修改候选边界 |
| [API 前的本地边界](technical/API_PREFLIGHT_BOUNDARY.md) | 完整短文本预检、瞬时凭据交接及尚缺的真实调用条件 | 修改 API 准备边界 |
| [单次 Responses API](technical/RESPONSES_API_PROTOCOL.md) | 唯一网关配置/授权/预算、密钥本地输入、审计schema2与原生只读重读 | 修改真实API边界 |
| [Sidecar v1](technical/SIDECAR_PROTOCOL.md) | 单请求、进程生命周期和原始字节资源契约 | 修改sidecar协议或平台边界 |
| [PDF v2](technical/PDF_PARSE_PROTOCOL.md) | 单份真实文本PDF的页、字形、区域、正文与报告发布 | 修改PDF解析契约、坐标或有限支持 |
| [设计标准](standards/DESIGN.md) | CLI、报告、笔记和后续桌面交互 | 用户可见行为和呈现变化 |
| [测试标准](standards/TESTING.md) | 阶段门槛、fixture和独立验证规则 | 引入新能力或新故障类型 |
| [工作流程](standards/WORKFLOW.md) | 每轮范围、日志、Git和验证顺序 | 开发协作方式变化 |
| [阶段路线](execution/ROADMAP.md) | 后续阶段目标及进入条件 | 阶段验收后滚动修订 |
| [近期步骤](execution/NEXT_STEPS.md) | 下一批最多3个可执行任务卡 | 当前批次完成后展开下一批 |
| [任务模板](templates/TASK.md) | 新任务卡的必填内容 | 工作卡结构变化 |

状态不在这些文件中重复手工维护。[任务账本](../dev-log/tasks.json) 是事实来源，[已完成事项](../dev-log/DONE.md) 和其他日志视图由工具生成。计划里的“预期结果”不等于已实现能力。

`requirements/PRD.html` 是由 Markdown 生成的阅读版；只修改 Markdown 后重新构建。其他标准文件以 Markdown 为准。旧 `docs` 路径仅保留迁移入口。

遇到标准冲突时：当前用户指令优先，其次已采纳产品需求与决策；技术和设计负责具体实现；近期步骤缩小本轮范围，不得取消产品硬约束。将实质冲突写入日志并修正文档，不暗中选择方便的一边。

PDF项目登记的精确Schema3、缓存与报告契约另见[PDF来源Schema](technical/PDF_SOURCE_SCHEMA.md)；真实验收记录见[B03-T03](validation/B03-T03.md)。
