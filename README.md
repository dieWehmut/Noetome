# Noetome

面向个人学科学习的知识编译器，将多份资料整合为可追溯、可维护的Obsidian知识库。

当前已有完整V1工程蓝图和开发日志/验证工具；产品代码尚未实现。

- [开发指引](AGENTS.md)：执行规则与标准路径。
- [完整技术与标准入口](doc/README.md)：架构、数据、算法、协议、设计和验收。
- [64项开发路线](doc/execution/ROADMAP.md)及[需求追溯](doc/execution/TRACEABILITY.md)。
- [交给后续Agent的指令](doc/execution/HANDOFF.md)。
- [自动测试与最少人验安排](doc/execution/AUTOMATED_TESTING.md)：你只需说“继续这个项目”。
- [当前状态](dev-log/STATUS.md)、[待办](dev-log/TODO.md)、[完成历史](dev-log/DONE.md)。
- [产品需求](doc/requirements/PRD.md)及[HTML阅读版](doc/requirements/PRD.html)。

```text
python scripts/plan.py next
python scripts/autotest.py
python scripts/plan.py show B01-T01
python scripts/devlog.py check
python scripts/plan.py check
python scripts/check_docs.py
python -m unittest discover -s tests -p "test_*.py"
```

任务开始和完成自动记录；完整步骤见[日志规则](dev-log/README.md)。任务卡中的产品命令是后续开发的验收要求。修改PRD后运行 `python scripts/build_prd.py` 同步阅读版。
