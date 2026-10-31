# 任务定义模板

任务定义写入 `dev-log/tasks.json`，不要手工创建派生卡。新增后运行plan render与devlog render，检查全量依赖和追溯。状态和历史由devlog工具维护。

```json
{
  "id": "Bxx-Txx",
  "title": "一个可观察结果",
  "batch": "Bxx",
  "status": "todo",
  "depends_on": ["前置任务ID"],
  "requirements": ["FR-01"],
  "acceptance": ["给定输入时可直接检查的结果"],
  "history": [],
  "plan_version": 1,
  "specs": ["doc/technical/DATA_CONTRACTS.md"],
  "steps": ["读现有实现", "实现单一行为", "验证成功与失败路径"],
  "outputs": ["仓库内具体文件路径"],
  "non_goals": ["本任务不扩张的范围"],
  "failure_policy": ["保留最小复现，最多3轮有效修复，仍失败则block"],
  "scenarios": ["AC-03"],
  "quality": ["NFR-01"],
  "checks": [
    {
      "name": "behavior",
      "argv": ["cargo", "test", "--locked", "-p", "noetome-core", "--test", "source"],
      "timeout_s": 1200,
      "min_tests": 1
    }
  ]
}
```

checks是argv数组，不拼接shell字符串；当前Python用 `{python}` 占位。针对性Rust测试使用确切integration test target，避免过滤器拼错后“0 tests”通过。Python/Vitest也必须设min_tests。发行gate不是单元测试计数，以RELEASE规定证据校验为准。

outputs必须是本任务结束时存在的真实文件，不用目录/通配符/日志本身。specs是已存在的标准路径。修改定义后旧verify报告失效。
