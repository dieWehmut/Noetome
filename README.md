# Noetome

面向个人学科学习的知识编译器，将多份电子资料整合为可追溯、可维护的 Obsidian 知识库。

当前仓库已建立开发基线和自动日志工具，产品代码尚未实现。

## 开始开发

- [开发指引](AGENTS.md)：每轮工作规则及标准路径。
- [标准文件入口](doc/README.md)：需求、技术、设计、测试和执行规范。
- [近期开发步骤](doc/execution/NEXT_STEPS.md)：仅展开下一批3项，每轮推进一项。
- [当前状态](dev-log/STATUS.md)、[待办事项](dev-log/TODO.md)、[已完成事项](dev-log/DONE.md)：由任务账本自动生成。
- [详细需求](doc/requirements/PRD.md)及[HTML阅读版](doc/requirements/PRD.html)。

原待决策事项已按建议采纳，详见[决策基线](doc/requirements/DECISIONS.md)。目前先做CLI与本地小闭环，后续逐步接入PDF、模型和桌面界面。

## 更新阅读版

修改 Markdown 后运行：

```powershell
python scripts/build_prd.py --format html
```

构建和日志脚本只使用Python标准库。使用可用Python解释器，改PRD后检查生成内容，再将源文档与HTML一同提交。

## 日志和检查

```text
python scripts/devlog.py render
python scripts/devlog.py check
python -m unittest discover -s tests -p "test_*.py"
```

开发时通过start、complete、block、reopen维护状态和证据，工具自动刷新三份清单。具体命令见[日志规则](dev-log/README.md)。不要手动把计划写成已完成。
