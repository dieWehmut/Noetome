# Noetome开发标准入口

用户已采纳默认建议，并要求预先固定全项目方案后交给执行Agent。V1按完整蓝图实现，状态不在标准文档重复维护。

| 文件 | 权威内容 |
| --- | --- |
| [PRD](requirements/PRD.md) | 40 FR、40 AC、17 NFR与产品边界 |
| [DECISIONS](requirements/DECISIONS.md) | 已接受建议、架构与范围决定 |
| [ARCHITECTURE](technical/ARCHITECTURE.md) | 模块、工作区、数据流和实施边界 |
| [STACK](technical/STACK.md) | 固定技术选型、版本锁定和官方依据 |
| [DATA_CONTRACTS](technical/DATA_CONTRACTS.md) / [schema.sql](technical/schema.sql) | 实体、引用、版本与SQLite结构基线 |
| [RUNTIME_PROTOCOL](technical/RUNTIME_PROTOCOL.md) | CLI/IPC、状态机、预算、恢复 |
| [KNOWLEDGE_PIPELINE](technical/KNOWLEDGE_PIPELINE.md) | 筛选、抽取、融合、规划、表达、研究算法 |
| [MODEL_CONTRACTS](technical/MODEL_CONTRACTS.md) | 模型候选、prompt任务、适配与检索 |
| [PUBLICATION](technical/PUBLICATION.md) | 人工保护、审批CAS、发布journal和回滚 |
| [CONFIG](technical/CONFIG.md) | 全配置类型、默认值及校验 |
| [DESIGN](standards/DESIGN.md) | 交互、视觉、笔记格式和状态表达 |
| [TESTING](standards/TESTING.md) / [RELEASE](standards/RELEASE.md) | 工程验证与真实质量、安装、长期运行门禁 |
| [ROADMAP](execution/ROADMAP.md) / [TRACEABILITY](execution/TRACEABILITY.md) | 64项全计划及需求关联 |
| [NEXT_STEPS](execution/NEXT_STEPS.md) / [HANDOFF](execution/HANDOFF.md) | 下一项和可直接给执行Agent的指令 |
| [WORKFLOW](standards/WORKFLOW.md) / [FAILURE_PLAYBOOK](execution/FAILURE_PLAYBOOK.md) | 执行、日志、验证、提交及失败恢复 |
| [AUTOMATED_TESTING](execution/AUTOMATED_TESTING.md) / [UI_AUTOMATION](technical/UI_AUTOMATION.md) | 一键测试、真实桌面自动化和最少人验安排 |

先读根[AGENTS](../AGENTS.md)，再看[当前状态](../dev-log/STATUS.md)，用plan next/show只加载当前任务上下文。[账本](../dev-log/tasks.json)为任务定义和状态唯一来源，[完成历史](../dev-log/DONE.md)与[待办](../dev-log/TODO.md)自动生成。

Markdown为标准源；PRD.html由build_prd生成。SQL是待产品实现采用并迁移的结构基线，不代表已有Rust数据库功能。所有产品测试命令在其对应任务实现后才执行，不把本轮文档校验称为产品验收。
