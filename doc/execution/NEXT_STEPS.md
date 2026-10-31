# 下一项执行入口

完整V1已有64张实现与验收任务卡，见[路线](ROADMAP.md)及[首批卡](tasks/B01.md)。每次只取一项，本文件不重复维护状态。

```text
python scripts/plan.py next
python scripts/plan.py show B01-T01
```

架构任务B01-T00完成后，第一个产品任务是B01-T01：只交付Rust CLI帮助、版本、参数错误、可重复构建和无副作用测试。B01-T02负责空项目，B01-T03负责文本来源。

后续所有阶段已展开，无需重新设计。读取卡片与关联标准，实现、verify、complete、提交推送，再下一项。见[接手指南](HANDOFF.md)与[工作流程](../standards/WORKFLOW.md)。
