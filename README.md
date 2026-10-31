# Noetome

面向个人学科学习的知识编译器，将多份电子资料整合为可追溯、可维护的 Obsidian 知识库。

当前仓库处于需求评审阶段，尚未实现产品。

## 需求文档

- [详细需求文档 Markdown](docs/Noetome-PRD-v1.0.md)：内容源文件，包含20章、40组功能需求、17项非功能要求和40个验收场景。
- [离线阅读版 HTML](docs/Noetome-PRD-v1.0.html)：使用浏览器打开，支持目录导航和打印。

文档明确区分用户已确认要求、建议基线和待决策项。优先评审第18章，再按第17章拆分实施任务。

## 更新阅读版

修改 Markdown 后运行：

```powershell
python scripts/build_prd.py --format html
```

构建脚本只使用 Python 标准库。生成后需检查目录导航、表格、代码示例和全文完整性，并将 Markdown 与 HTML 一同提交。
